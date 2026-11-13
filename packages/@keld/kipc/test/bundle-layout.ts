/**
 * App-layout runner for the GH-527 §4.2 and #653 staging tests: builds an app
 * layout in a fresh directory, runs it with Bun against a Unix-domain socket on
 * Linux/macOS or a Windows named-pipe listener,
 * and reports stdout, the listener's connection count and an evaluation
 * counter. Each run is its own process, because a realm opens one link.
 */
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ReadableStreamDefaultReader as NodeReadableStreamDefaultReader } from "node:stream/web";
import { expect } from "bun:test";
import { withIoDeadline } from "../src/transport.ts";

import { freshWin32AppLinkPipe } from "./app-link-host.ts";

/** The layout listener's fixed 32-byte session token. */
export const TOKEN_HEX = "ab".repeat(32);

export interface BundleRun {
  stdout: string;
  connections: number;
  /** Lines appended to `KELD_TEST_EVALS`: one per evaluation of a counting entry. */
  evals: number;
  /** Set only when the layout requests natural child exit before cleanup. */
  exitCode?: number;
  stderr?: string;
  /** Owned temporary root, removed before this result reaches the caller. */
  root: string;
}

export interface BundleSocketObserver {
  data?: (socket: Bun.Socket<undefined>, bytes: Uint8Array) => void;
  drain?: () => void;
  close?: () => void;
  error?: (error: Error) => void;
}

export interface BundleLayout {
  /** Builds the app files into `dir`; returns the file name `bun` runs. */
  write: (dir: string) => Promise<string>;
  /** Stop reading stdout once this holds (or the process exits). */
  ready: (stdout: string) => boolean;
  /** End each accepted socket at once, so a Worker that connects fails fast instead of waiting for HELLO. */
  closeOnOpen?: boolean;
  /** Extra environment for the run, such as `KELD_KIPC_TEST_HOOKS`. */
  env?: Record<string, string>;
  /** Optional wire observer/responder; default layouts only count connections. */
  socketObserver?: BundleSocketObserver;
  /** Observe natural exit and complete output before emergency cleanup. */
  waitForExit?: boolean;
  /** Consumer-owned terminal oracles, observed before listener/path cleanup. */
  beforeCleanup?: (result: BundleRun) => Promise<void>;
  /** Bound each wait; timing out does not relinquish a pending resource owner. */
  deadlineMs?: number;
}

export type LayoutOutputReader = Pick<
  NodeReadableStreamDefaultReader<Uint8Array>,
  "read" | "cancel" | "releaseLock"
>;

/** The source owner may supply an idempotent real-resource finalizer, not another stream cancel. */
export interface LayoutOutputOwner {
  reader: LayoutOutputReader;
  /** Fulfill only after the underlying resource is finalized; a rejection permits an explicit retry. */
  finalizeSource?: () => Promise<void>;
}

interface CleanupTask<T> {
  promise: Promise<T>;
  state: "pending" | "fulfilled" | "rejected";
}

function cleanupTask<T>(promise: Promise<T>): CleanupTask<T> {
  const task: CleanupTask<T> = { promise, state: "pending" };
  void promise.then(
    () => {
      task.state = "fulfilled";
    },
    () => {
      task.state = "rejected";
    },
  );
  return task;
}

export interface RetainedLayout {
  root: string;
  listener: { stop(closeActiveConnections?: boolean): void } | undefined;
  child: ReturnType<typeof Bun.spawn> | undefined;
  childExit: CleanupTask<number> | undefined;
  stderr: CleanupTask<string> | undefined;
  write: CleanupTask<string> | undefined;
  callback: CleanupTask<void> | undefined;
  output?: LayoutCleanupError["cleanup"];
  disposed: boolean;
  /** Retry disposal after pending owners settle; each attempt remains bounded. */
  dispose(): Promise<void>;
}

/** Preserves the primary failure and the owner of unfinished outer teardown. */
export class LayoutDisposalError extends AggregateError {
  constructor(
    errors: unknown[],
    original: unknown,
    readonly retainedLayout: RetainedLayout,
  ) {
    super(
      errors,
      `layout disposal incomplete; retained resources at ${retainedLayout.root}`,
      { cause: original },
    );
  }
}

export class LayoutCleanupError extends AggregateError {
  retainedLayout?: RetainedLayout;

  constructor(
    errors: unknown[],
    original: unknown,
    readonly cleanup: {
      readonly incomplete: boolean;
      readonly sourceFinalized: boolean;
      reader: LayoutOutputReader;
      readerReleased: boolean;
      stopChild: CleanupTask<void>;
      cancellation: CleanupTask<void | undefined>;
      output: CleanupTask<string>;
      /** Retry the separately owned resource finalizer; never replay rejected stream cancellation. */
      finalizeSource(): Promise<void>;
    },
  ) {
    super(
      errors,
      cleanup.incomplete
        ? "layout cleanup incomplete"
        : "layout cleanup failed",
      { cause: original },
    );
  }
}

/** Owns stdout through terminal task settlement, including an aggregate failure. */
export async function withLayoutOutput<T>(
  reader: LayoutOutputReader,
  layout: Pick<BundleLayout, "ready" | "waitForExit" | "deadlineMs">,
  use: (output: Promise<string>) => Promise<T>,
  stopChild: () => Promise<void>,
  finalizeSource?: () => Promise<void>,
): Promise<T> {
  let settled = false;
  let eof = false;
  let cancellationStarted = false;
  let readerReleased = false;
  const release = () => {
    if (!readerReleased) {
      reader.releaseLock();
      readerReleased = true;
    }
  };
  const output = (async () => {
    let stdout = "";
    const decoder = new TextDecoder();
    try {
      for (;;) {
        const chunk = await reader.read();
        if (chunk.done) {
          // Cancellation closes a WHATWG stream before its source cancel hook
          // settles. That synthetic EOF is not source-finalization evidence.
          eof = !cancellationStarted;
          break;
        }
        stdout += decoder.decode(chunk.value, { stream: true });
        if (!layout.waitForExit && layout.ready(stdout)) {
          break;
        }
      }
      return stdout + decoder.decode();
    } finally {
      settled = true;
      // Early readiness still owns the source. Keep its lock until cancellation.
      if (eof || cancellationStarted) release();
    }
  })();
  // The terminal outcome is inspected below, even if use rejects first.
  void output.catch(() => undefined);
  let failed = false;
  let failure: unknown;
  try {
    return await use(output);
  } catch (error) {
    failed = true;
    failure = error;
    throw error;
  } finally {
    const stop = cleanupTask(Promise.resolve().then(stopChild));
    const [stopped] = await Promise.allSettled([
      withIoDeadline(stop.promise, layout.deadlineMs),
    ]);
    const cancellation = cleanupTask(
      Promise.resolve().then(() => {
        if (eof || readerReleased) return;
        cancellationStarted = true;
        return reader.cancel();
      }),
    );
    const [cancelled] = await Promise.allSettled([
      withIoDeadline(cancellation.promise, layout.deadlineMs),
    ]);
    const terminalTask = cleanupTask(output);
    const [terminal] = await Promise.allSettled([
      withIoDeadline(terminalTask.promise, layout.deadlineMs),
    ]);
    const cleanupErrors: unknown[] = [];
    if (settled) {
      try {
        release();
      } catch (error) {
        cleanupErrors.push(error);
      }
    }
    if (stopped.status === "rejected") cleanupErrors.push(stopped.reason);
    if (cancelled.status === "rejected") cleanupErrors.push(cancelled.reason);
    if (
      terminal.status === "rejected" &&
      (!failed || terminal.reason !== failure)
    )
      cleanupErrors.push(terminal.reason);
    let finalization: CleanupTask<void> | undefined;
    const cleanup: LayoutCleanupError["cleanup"] = {
      get sourceFinalized() {
        return (
          eof ||
          cancellation.state === "fulfilled" ||
          finalization?.state === "fulfilled"
        );
      },
      get incomplete() {
        return (
          !readerReleased ||
          !this.sourceFinalized ||
          [stop, cancellation, terminalTask, finalization].some(
            (task) => task?.state === "pending",
          )
        );
      },
      reader,
      get readerReleased() {
        return readerReleased;
      },
      stopChild: stop,
      cancellation,
      output: terminalTask,
      async finalizeSource() {
        if (!this.sourceFinalized) {
          if (finalizeSource === undefined) {
            throw new Error(
              "layout source finalization remains unproved; no resource-owner finalizer was supplied",
            );
          }
          // Share an in-flight operation. A timeout does not license another
          // invocation; only a settled rejection allows this idempotent retry.
          if (finalization === undefined || finalization.state === "rejected") {
            finalization = cleanupTask(Promise.resolve().then(finalizeSource));
          }
          await withIoDeadline(finalization.promise, layout.deadlineMs);
        }
        await withIoDeadline(terminalTask.promise, layout.deadlineMs).catch(
          () => undefined,
        );
        if (settled) release();
        if (this.incomplete)
          throw new Error("layout output cleanup remains incomplete");
      },
    };
    if (cleanupErrors.length !== 0 || cleanup.incomplete) {
      throw new LayoutCleanupError(
        failed ? [failure, ...cleanupErrors] : cleanupErrors,
        failure,
        cleanup,
      );
    }
  }
}

/** Narrow acquisition/disposal fault seams for this test runner's own regressions. */
export interface LayoutOperations {
  listen?: (
    options: Parameters<typeof Bun.listen<undefined>>[0],
    root: string,
  ) => ReturnType<typeof Bun.listen<undefined>>;
  removeRoot?: (root: string) => void;
  /** Optional fault seam; the returned owner must retain this exact stdout source. */
  outputOwner?: (stream: ReadableStream<Uint8Array>) => LayoutOutputOwner;
}

/**
 * Owns an app layout from root creation through bounded use and disposal.
 * A timed-out writer/callback still owns the layout; the error transfers an
 * explicit dispose operation to the caller instead of deleting beneath it.
 */
export async function runLayout(
  layout: BundleLayout,
  operations: LayoutOperations = {},
): Promise<BundleRun> {
  const dir = mkdtempSync(join(tmpdir(), "keld-wl-"));
  let listener: ReturnType<typeof Bun.listen<undefined>> | undefined;
  let proc: ReturnType<typeof Bun.spawn> | undefined;
  let childExit: CleanupTask<number> | undefined;
  let stderrTask: CleanupTask<string> | undefined;
  let writeTask: CleanupTask<string> | undefined;
  let callbackTask: CleanupTask<void> | undefined;
  let outputCleanup: LayoutCleanupError["cleanup"] | undefined;
  let listenerStopped = false;
  let rootRemoved = false;
  let disposal: Promise<void> | undefined;
  const bound = <T>(promise: Promise<T>) =>
    withIoDeadline(promise, layout.deadlineMs);
  const stopChild = async () => {
    if (proc !== undefined && childExit?.state === "pending") proc.kill();
    if (proc !== undefined)
      await bound(Promise.all([proc.exited, stderrTask?.promise]));
  };
  const owner: RetainedLayout = {
    root: dir,
    listener,
    child: proc,
    childExit,
    stderr: stderrTask,
    write: writeTask,
    callback: callbackTask,
    disposed: false,
    dispose() {
      if (disposal !== undefined) return disposal;
      disposal = dispose().finally(() => {
        disposal = undefined;
      });
      return disposal;
    },
  };
  const dispose = async () => {
    if (owner.disposed) return;
    const errors: unknown[] = [];
    const stop = await Promise.allSettled([
      bound(Promise.resolve().then(stopChild)),
    ]);
    if (stop[0]!.status === "rejected") errors.push(stop[0]!.reason);
    if (outputCleanup?.incomplete) {
      try {
        await outputCleanup.finalizeSource();
      } catch (error) {
        errors.push(error);
      }
    }
    // Pending producers must finish before we remove their directory/listener.
    // Rejections were already observed by the use path; only pending work is joined.
    const pending = [
      writeTask,
      callbackTask,
      childExit,
      stderrTask,
      outputCleanup?.stopChild,
      outputCleanup?.cancellation,
      outputCleanup?.output,
    ].filter((task) => task?.state === "pending");
    const joined = await Promise.allSettled(
      pending.map((task) => bound<unknown>(task!.promise)),
    );
    for (const result of joined)
      if (result.status === "rejected") errors.push(result.reason);
    const stillPending =
      pending.some((task) => task!.state === "pending") ||
      outputCleanup?.incomplete === true;
    if (!stillPending) {
      // Independent steps: a listener-stop exception must not skip root removal.
      const terminal = await Promise.allSettled([
        Promise.resolve().then(() => {
          if (!listenerStopped) {
            listener?.stop(true);
            listenerStopped = true;
          }
        }),
        Promise.resolve().then(() => {
          if (!rootRemoved) {
            if (operations.removeRoot) operations.removeRoot(dir);
            else rmSync(dir, { recursive: true, force: true });
            rootRemoved = true;
          }
        }),
      ]);
      for (const result of terminal)
        if (result.status === "rejected") errors.push(result.reason);
    }
    owner.disposed =
      !stillPending &&
      listenerStopped &&
      rootRemoved &&
      (proc === undefined || childExit?.state === "fulfilled");
    if (errors.length)
      throw new AggregateError(errors, "layout disposal failed");
    if (!owner.disposed) throw new Error("layout disposal remains incomplete");
  };
  let failed = false;
  let failure: unknown;
  let result: BundleRun | undefined;
  try {
    const path =
      process.platform === "win32"
        ? freshWin32AppLinkPipe()
        : join(dir, "s.sock");
    const evalsPath = join(dir, "evals.txt");
    let connections = 0;
    listener = (operations.listen ?? Bun.listen<undefined>)(
      {
        unix: path,
        socket: {
          open(socket) {
            connections += 1;
            if (layout.closeOnOpen) socket.end();
          },
          data(socket, bytes) {
            layout.socketObserver?.data?.(socket, bytes);
          },
          ...(layout.socketObserver === undefined
            ? {}
            : {
                drain() {
                  layout.socketObserver?.drain?.();
                },
                close() {
                  layout.socketObserver?.close?.();
                },
                error(_socket: Bun.Socket<undefined>, error: Error) {
                  layout.socketObserver?.error?.(error);
                },
              }),
        },
      },
      dir,
    );
    owner.listener = listener;
    writeTask = cleanupTask(Promise.resolve().then(() => layout.write(dir)));
    owner.write = writeTask;
    const runner = await bound(writeTask.promise);
    proc = Bun.spawn(["bun", join(dir, runner)], {
      env: {
        ...process.env,
        ...layout.env,
        KELD_APP_LINK: `${path}#${TOKEN_HEX}`,
        KELD_TEST_EVALS: evalsPath,
      },
      stdout: "pipe",
      stderr: "pipe",
    });
    owner.child = proc;
    childExit = cleanupTask(proc.exited);
    stderrTask = cleanupTask(
      new Response(proc.stderr as ReadableStream<Uint8Array>).text(),
    );
    owner.childExit = childExit;
    owner.stderr = stderrTask;
    const child = proc;
    const errorsText = stderrTask.promise;
    const stdout = proc.stdout as ReadableStream<Uint8Array>;
    const outputOwner = operations.outputOwner?.(stdout) ?? {
      reader: stdout.getReader(),
    };
    result = await withLayoutOutput(
      outputOwner.reader,
      layout,
      async (output) => {
        let stdout: string;
        let exitCode: number | undefined;
        let errors: string | undefined;
        if (layout.waitForExit)
          [stdout, exitCode, errors] = await bound(
            Promise.all([output, child.exited, errorsText]),
          );
        else stdout = await bound(output);
        const counted = existsSync(evalsPath)
          ? readFileSync(evalsPath, "utf8")
          : "";
        const observed = {
          stdout,
          connections,
          evals: counted.split("\n").filter((line) => line.length > 0).length,
          exitCode,
          stderr: errors,
          root: dir,
        };
        if (layout.beforeCleanup) {
          callbackTask = cleanupTask(
            Promise.resolve().then(() => layout.beforeCleanup!(observed)),
          );
          owner.callback = callbackTask;
          await bound(callbackTask.promise);
        }
        return observed;
      },
      stopChild,
      outputOwner.finalizeSource?.bind(outputOwner),
    );
  } catch (error) {
    failed = true;
    failure = error;
    if (error instanceof LayoutCleanupError) {
      outputCleanup = error.cleanup;
      owner.output = outputCleanup;
    }
  }
  try {
    await owner.dispose();
  } catch (error) {
    const faults = error instanceof AggregateError ? error.errors : [error];
    if (failure instanceof LayoutCleanupError) {
      failure.errors.push(...faults);
      failure.retainedLayout = owner;
      throw failure;
    }
    throw new LayoutDisposalError(
      failed ? [failure, ...faults] : faults,
      failure,
      owner,
    );
  }
  if (failed) throw failure;
  return result!;
}

// Builds `entrySource` (TypeScript) into one bundle named `name`, beside a
// `main.js` that imports it, and runs `runner`.
export function bundleLayout(
  name: string,
  entrySource: string,
  runner: string,
  ready: (stdout: string) => boolean,
) {
  return {
    ready,
    async write(dir: string): Promise<string> {
      const entry = join(dir, "entry.ts");
      await Bun.write(entry, entrySource);
      const built = await Bun.build({
        entrypoints: [entry],
        target: "bun",
        format: "esm",
      });
      expect(built.success).toBe(true);
      await Bun.write(join(dir, name), built.outputs[0]!);
      await Bun.write(join(dir, "main.js"), `import "./${name}";\n`);
      return runner;
    },
  } satisfies BundleLayout;
}

export const transportPath = JSON.stringify(
  join(import.meta.dir, "..", "src", "transport.ts"),
);
export const openAndReport =
  `import { isMainThread } from "node:worker_threads";\n` +
  `import { WorkerLink, isCallError } from ${transportPath};\n` +
  "WorkerLink.open({ link: process.env.KELD_APP_LINK!, receive: { eventChannels: [3], callReceivers: [] } }).then(\n" +
  '  () => console.log(`${isMainThread ? "main" : "worker"}-open=opened`),\n' +
  '  (err) => console.log(`${isMainThread ? "main" : "worker"}-open=${isCallError(err) ? err.code : "untyped"}`),\n' +
  ");\n";
// The same app code, counting each evaluation of its module in any thread.
export const countedOpenAndReport =
  `import { appendFileSync } from "node:fs";\n` +
  `appendFileSync(process.env.KELD_TEST_EVALS!, "eval\\n");\n` +
  openAndReport;

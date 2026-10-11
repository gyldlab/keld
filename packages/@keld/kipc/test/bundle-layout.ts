/**
 * App-layout runner for the GH-527 §4.2 and #653 staging tests: builds an app
 * layout in a fresh directory, runs it with Bun against a live Unix listener,
 * and reports stdout, the listener's connection count and an evaluation
 * counter. Each run is its own process, because a realm opens one link.
 */
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ReadableStreamDefaultReader as NodeReadableStreamDefaultReader } from "node:stream/web";
import { expect } from "bun:test";
import { withIoDeadline } from "../src/transport.ts";

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
}

export type LayoutOutputReader = Pick<NodeReadableStreamDefaultReader<Uint8Array>, "read" | "cancel" | "releaseLock">;

interface CleanupTask<T> {
  promise: Promise<T>;
  state: "pending" | "fulfilled" | "rejected";
}

function cleanupTask<T>(promise: Promise<T>): CleanupTask<T> {
  const task: CleanupTask<T> = { promise, state: "pending" };
  void promise.then(() => { task.state = "fulfilled"; }, () => { task.state = "rejected"; });
  return task;
}

export class LayoutCleanupError extends AggregateError {
  retainedLayout?: {
    root: string;
    listener: { stop(closeActiveConnections?: boolean): void };
    child: ReturnType<typeof Bun.spawn> | undefined;
    childExit: CleanupTask<number> | undefined;
    stderr: CleanupTask<string> | undefined;
  };

  constructor(errors: unknown[], original: unknown, readonly cleanup: {
    incomplete: boolean;
    reader: LayoutOutputReader;
    readerReleased: boolean;
    stopChild: CleanupTask<void>;
    cancellation: CleanupTask<void | undefined>;
    output: CleanupTask<string>;
  }) {
    super(errors, cleanup.incomplete ? "layout cleanup incomplete" : "layout cleanup failed", { cause: original });
  }
}

/** Owns stdout through terminal task settlement, including an aggregate failure. */
export async function withLayoutOutput<T>(
  reader: LayoutOutputReader,
  layout: Pick<BundleLayout, "ready" | "waitForExit">,
  use: (output: Promise<string>) => Promise<T>,
  stopChild: () => Promise<void>,
): Promise<T> {
  let settled = false;
  const output = (async () => {
    let stdout = "";
    const decoder = new TextDecoder();
    try {
      for (;;) {
        const chunk = await reader.read();
        if (chunk.done) break;
        stdout += decoder.decode(chunk.value, { stream: true });
        if (!layout.waitForExit && layout.ready(stdout)) break;
      }
      return stdout + decoder.decode();
    } finally {
      reader.releaseLock();
      settled = true;
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
    const [stopped] = await Promise.allSettled([withIoDeadline(stop.promise)]);
    const cancellation = cleanupTask(Promise.resolve().then(() => settled ? undefined : reader.cancel()));
    const [cancelled] = await Promise.allSettled([withIoDeadline(cancellation.promise)]);
    const terminalTask = cleanupTask(output);
    const [terminal] = await Promise.allSettled([withIoDeadline(terminalTask.promise)]);
    const cleanupErrors: unknown[] = [];
    if (stopped.status === "rejected") cleanupErrors.push(stopped.reason);
    if (cancelled.status === "rejected") cleanupErrors.push(cancelled.reason);
    if (terminal.status === "rejected" && (!failed || terminal.reason !== failure)) cleanupErrors.push(terminal.reason);
    const incomplete = !settled || [stop, cancellation, terminalTask].some((task) => task.state === "pending");
    if (cleanupErrors.length !== 0 || incomplete) {
      throw new LayoutCleanupError(failed ? [failure, ...cleanupErrors] : cleanupErrors, failure, {
        incomplete,
        reader, readerReleased: settled, stopChild: stop, cancellation, output: terminalTask,
      });
    }
  }
}

// Runs one app layout in a fresh directory with a live listener and an
// evaluation counter, and returns its stdout, the listener's connection count
// and the counter. Each run is its own process: a realm opens one link.
export async function runLayout(layout: BundleLayout): Promise<BundleRun> {
  const dir = mkdtempSync(join(tmpdir(), "keld-wl-"));
  const path = join(dir, "s.sock");
  const evalsPath = join(dir, "evals.txt");
  let connections = 0;
  const listener = Bun.listen<undefined>({
    unix: path,
    socket: {
      open(socket) {
        connections += 1;
        if (layout.closeOnOpen) socket.end();
      },
      data(socket, bytes) {
        layout.socketObserver?.data?.(socket, bytes);
      },
      ...(layout.socketObserver === undefined ? {} : {
        drain() { layout.socketObserver?.drain?.(); },
        close() { layout.socketObserver?.close?.(); },
        error(_socket: Bun.Socket<undefined>, error: Error) { layout.socketObserver?.error?.(error); },
      }),
    },
  });
  let proc: ReturnType<typeof Bun.spawn> | undefined;
  let stderr: Promise<string> | undefined;
  let childExit: CleanupTask<number> | undefined;
  let stderrTask: CleanupTask<string> | undefined;
  let childStopped = false;
  let retainResources = false;
  const stopChild = async () => {
    if (childStopped) return;
    childStopped = true;
    if (proc?.exitCode === null) proc.kill();
    if (proc !== undefined) await withIoDeadline(Promise.all([proc.exited, stderr]));
  };
  try {
    const runner = await layout.write(dir);
    proc = Bun.spawn(["bun", join(dir, runner)], {
      env: { ...process.env, ...layout.env, KELD_APP_LINK: `${path}#${TOKEN_HEX}`, KELD_TEST_EVALS: evalsPath },
      stdout: "pipe",
      stderr: "pipe",
    });
    childExit = cleanupTask(proc.exited);
    stderr = new Response(proc.stderr as ReadableStream<Uint8Array>).text();
    stderrTask = cleanupTask(stderr);
    const child = proc;
    const errorsText = stderr;
    return await withLayoutOutput((proc.stdout as ReadableStream<Uint8Array>).getReader(), layout, async (output) => {
      let stdout: string;
      let exitCode: number | undefined;
      let errors: string | undefined;
      if (layout.waitForExit) {
        [stdout, exitCode, errors] = await withIoDeadline(Promise.all([output, child.exited, errorsText]));
      } else {
        stdout = await output;
      }
      const counted = existsSync(evalsPath) ? readFileSync(evalsPath, "utf8") : "";
      const result = {
        stdout, connections, evals: counted.split("\n").filter((line) => line.length > 0).length,
        exitCode, stderr: errors, root: dir,
      };
      await layout.beforeCleanup?.(result);
      return result;
    }, stopChild);
  } catch (error) {
    if (error instanceof LayoutCleanupError && (
      error.cleanup.incomplete || childExit?.state === "pending" || stderrTask?.state === "pending"
    )) {
      retainResources = true;
      error.cleanup.incomplete = true;
      error.retainedLayout = { root: dir, listener, child: proc, childExit, stderr: stderrTask };
      error.message = `layout cleanup incomplete; retained resources at ${dir}`;
    }
    throw error;
  } finally {
    if (!retainResources) {
      await stopChild();
      listener.stop(true);
      rmSync(dir, { recursive: true, force: true });
    }
  }
}

// Builds `entrySource` (TypeScript) into one bundle named `name`, beside a
// `main.js` that imports it, and runs `runner`.
export function bundleLayout(name: string, entrySource: string, runner: string, ready: (stdout: string) => boolean) {
  return {
    ready,
    async write(dir: string): Promise<string> {
      const entry = join(dir, "entry.ts");
      await Bun.write(entry, entrySource);
      const built = await Bun.build({ entrypoints: [entry], target: "bun", format: "esm" });
      expect(built.success).toBe(true);
      await Bun.write(join(dir, name), built.outputs[0]!);
      await Bun.write(join(dir, "main.js"), `import "./${name}";\n`);
      return runner;
    },
  } satisfies BundleLayout;
}

export const transportPath = JSON.stringify(join(import.meta.dir, "..", "src", "transport.ts"));
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

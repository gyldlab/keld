import { describe, expect, test } from "bun:test";
import {
  closeSync,
  existsSync,
  fstatSync,
  mkdtempSync,
  openSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  LayoutCleanupError,
  LayoutDisposalError,
  type RetainedLayout,
  runLayout,
  withLayoutOutput,
} from "../test/bundle-layout.ts";
import { withIoDeadline } from "./transport.ts";

/** Captures a thrown value without treating a successful operation as failure. */
async function rejected(action: Promise<unknown>): Promise<unknown> {
  return action.then(
    () => undefined,
    (error: unknown) => error,
  );
}

interface RetainedFailure extends AggregateError {
  retainedLayout: {
    root: string;
    child?: ReturnType<typeof Bun.spawn>;
    listener?: { stop(closeActiveConnections?: boolean): void };
    disposed: boolean;
    dispose(): Promise<void>;
  };
}

/** Checks the caller-visible ownership contract rather than the error's class name. */
function retained(error: unknown): RetainedFailure {
  expect(error).toBeInstanceOf(AggregateError);
  expect(error).toHaveProperty("retainedLayout");
  return error as RetainedFailure;
}

/** A known test child must no longer be an OS-visible live process. */
function expectExited(pid: number): void {
  let alive = true;
  try {
    process.kill(pid, 0);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error;
    alive = false;
  }
  expect(alive).toBe(false);
}

describe("layout resource ownership on failure", () => {
  test("early readiness cancels its still-open real stream before releasing ownership", async () => {
    let cancellations = 0;
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new TextEncoder().encode("ready"));
      },
      cancel() {
        cancellations += 1;
      },
    });
    const reader = stream.getReader();
    try {
      const output = await withLayoutOutput(
        reader,
        { ready: (text) => text === "ready", deadlineMs: 100 },
        (text) => text,
        async () => {},
      );
      expect(output).toBe("ready");
      expect(cancellations).toBe(1);
      expect(stream.locked).toBe(false);
    } finally {
      if (!stream.locked) await stream.cancel();
    }
  });

  test("a throwing readiness predicate preserves its error and cancels the source", async () => {
    const original = new Error("intentional readiness predicate failure");
    let cancellations = 0;
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new TextEncoder().encode("ready"));
      },
      cancel() {
        cancellations += 1;
      },
    });
    try {
      const error = await rejected(
        withLayoutOutput(
          stream.getReader(),
          {
            ready: () => {
              throw original;
            },
            deadlineMs: 100,
          },
          (text) => text,
          async () => {},
        ),
      );
      expect(error).toBe(original);
      expect(cancellations).toBe(1);
      expect(stream.locked).toBe(false);
    } finally {
      if (!stream.locked) await stream.cancel();
    }
  });

  test("natural EOF releases the reader without cancelling an already-closed source", async () => {
    let cancellations = 0;
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new TextEncoder().encode("complete"));
        controller.close();
      },
      cancel() {
        cancellations += 1;
      },
    });
    expect(
      await withLayoutOutput(
        stream.getReader(),
        { ready: () => false, deadlineMs: 100 },
        (text) => text,
        async () => {},
      ),
    ).toBe("complete");
    expect(cancellations).toBe(0);
    expect(stream.locked).toBe(false);
  });

  test("bind failure removes the exact acquired root without masking the bind error", async () => {
    const bindError = new Error("intentional listener acquisition failure");
    let root = "";
    const error = await rejected(
      runLayout(
        {
          ready: () => true,
          async write() {
            throw new Error("write must not run after failed bind");
          },
        },
        {
          listen(_options, dir) {
            root = dir;
            expect(existsSync(root)).toBe(true);
            throw bindError;
          },
        },
      ),
    );
    try {
      expect(error).toBe(bindError);
      expect(root).not.toBe("");
      expect(existsSync(root)).toBe(false);
    } finally {
      if (root !== "") rmSync(root, { recursive: true, force: true });
    }
  });

  for (const fault of ["listener", "root"] as const) {
    test(`${fault} teardown failure preserves the primary error and permits explicit recovery`, async () => {
      const primary = new Error("intentional layout construction failure");
      const secondary = new Error(`intentional ${fault} disposal failure`);
      let root = "";
      let failOnce = true;
      let stops = 0;
      let removals = 0;
      let listener: ReturnType<typeof Bun.listen<undefined>> | undefined;
      let owned: RetainedFailure | undefined;
      try {
        const error = await rejected(
          runLayout(
            {
              ready: () => true,
              async write(dir) {
                root = dir;
                throw primary;
              },
            },
            {
              listen(options) {
                listener = Bun.listen<undefined>(options);
                const actual = listener;
                return new Proxy(actual, {
                  get(target, property) {
                    if (property !== "stop")
                      return Reflect.get(target, property, target);
                    return (close?: boolean) => {
                      stops += 1;
                      if (fault === "listener" && failOnce) {
                        failOnce = false;
                        throw secondary;
                      }
                      actual.stop(close);
                    };
                  },
                });
              },
              removeRoot(dir) {
                removals += 1;
                if (fault === "root" && failOnce) {
                  failOnce = false;
                  throw secondary;
                }
                rmSync(dir, { recursive: true, force: true });
              },
            },
          ),
        );
        owned = retained(error);
        expect(owned.cause).toBe(primary);
        expect(owned.errors).toContain(primary);
        expect(owned.errors).toContain(secondary);
        expect(stops).toBe(1);
        expect(removals).toBe(1);
        expect(owned.retainedLayout.root).toBe(root);
        expect(owned.retainedLayout.disposed).toBe(false);
        await owned.retainedLayout.dispose();
        expect(owned.retainedLayout.disposed).toBe(true);
        expect(existsSync(root)).toBe(false);
        const after = { stops, removals };
        await owned.retainedLayout.dispose();
        expect({ stops, removals }).toEqual(after);
      } finally {
        listener?.stop(true);
        if (root !== "") rmSync(root, { recursive: true, force: true });
      }
    });
  }

  for (const failRemoval of [false, true]) {
    test(`a silent live child retains safe cleanup ownership (removal fault: ${failRemoval})`, async () => {
      const audit = mkdtempSync(join(tmpdir(), "keld-layout-audit-"));
      const pidPath = join(audit, "pid");
      const removalError = new Error("intentional first root removal failure");
      let root = "";
      let removals = 0;
      const operation = rejected(
        runLayout(
          {
            deadlineMs: 1000,
            ready: () => false,
            async write(dir) {
              root = dir;
              await Bun.write(
                join(dir, "silent.ts"),
                `import { writeFileSync } from "node:fs";\n` +
                  `writeFileSync(${JSON.stringify(pidPath)}, String(process.pid));\n` +
                  "setInterval(() => {}, 60_000);\n",
              );
              return "silent.ts";
            },
          },
          {
            removeRoot(dir) {
              removals += 1;
              if (failRemoval && removals === 1) throw removalError;
              rmSync(dir, { recursive: true, force: true });
            },
          },
        ),
      );
      let primary: unknown;
      let operationError: unknown;
      try {
        const error = await withIoDeadline(operation, 10_000);
        if (failRemoval) {
          const failure = retained(error);
          expect((failure.cause as Error).message).toContain("KELD-IPC-006");
          expect(failure.errors).toContain(removalError);
          expect(failure.retainedLayout.disposed).toBe(false);
          expect(existsSync(root)).toBe(true);
        } else {
          expect(error).toBeInstanceOf(Error);
          expect((error as Error).message).toContain("KELD-IPC-006");
          expect(existsSync(root)).toBe(false);
        }
        expect(existsSync(pidPath)).toBe(true);
        // Audit PIDs are observations only, never cleanup authority.
        expectExited(Number(readFileSync(pidPath, "utf8")));
      } catch (error) {
        primary = error;
        throw error;
      } finally {
        try {
          // Join the same runner even if the outer assertion deadline expired.
          // It retains custody until it returns an actionable disposal owner.
          const error = await withIoDeadline(operation, 10_000);
          operationError = error;
          if (
            error instanceof LayoutCleanupError ||
            error instanceof LayoutDisposalError
          ) {
            const owner = error.retainedLayout;
            if (owner !== undefined && !owner.disposed) {
              await withIoDeadline(owner.dispose());
              expect<boolean>(owner.disposed).toBe(true);
              if (owner.child !== undefined)
                expect(await withIoDeadline(owner.child.exited)).toBeNumber();
            }
          }
          expect(existsSync(root)).toBe(false);
          expect(removals).toBe(failRemoval ? 2 : 1);
          rmSync(audit, { recursive: true, force: true });
        } catch (cleanup) {
          // Preserve the returned owner, or the still-pending operation that
          // will return it, if this recovery attempt cannot finish.
          throw Object.assign(
            new AggregateError(
              [primary, operationError, cleanup].filter(
                (error) => error !== undefined,
              ),
              "layout test recovery remains incomplete",
              { cause: primary ?? operationError ?? cleanup },
            ),
            { layoutOperation: operation },
          );
        }
      }
      console.log(`SILENT_LAYOUT_CLEANUP_OK removal_fault=${failRemoval}`);
    }, 25_000);
  }

  test("silent-child cleanup never signals an audit PID", async () => {
    const child = Bun.spawn(
      [
        process.execPath,
        "test",
        "--preload",
        join(import.meta.dir, "../test/layout-cleanup-signal-guard.ts"),
        import.meta.path,
        "--test-name-pattern",
        "a silent live child retains safe cleanup ownership",
      ],
      { stdout: "pipe", stderr: "pipe" },
    );
    try {
      const [status, stdout, stderr] = await withIoDeadline(
        Promise.all([
          child.exited,
          new Response(child.stdout).text(),
          new Response(child.stderr).text(),
        ]),
        30_000,
      );
      expect(stdout).toContain("SILENT_LAYOUT_CLEANUP_OK removal_fault=false");
      expect(stdout).toContain("SILENT_LAYOUT_CLEANUP_OK removal_fault=true");
      expect(stderr).toContain("0 fail");
      expect(stdout).toContain("AUDIT_PID_SIGNALS=0");
      expect(status).toBe(0);
    } finally {
      if (child.exitCode === null && child.signalCode === null) child.kill();
      await withIoDeadline(child.exited);
    }
  }, 35_000);

  test("a direct child exiting with inherited descendant stderr permits bounded disposal", async () => {
    const audit = mkdtempSync(join(tmpdir(), "keld-layout-stderr-audit-"));
    const readyPath = join(audit, "descendant-ready");
    let root = "";
    let failure: unknown;
    let success: Awaited<ReturnType<typeof runLayout>> | undefined;
    try {
      const operation = runLayout({
        deadlineMs: 1_500,
        ready: () => false,
        waitForExit: true,
        async write(dir) {
          root = dir;
          const grandchild = `import { writeFileSync } from "node:fs"; writeFileSync(${JSON.stringify(readyPath)}, "alive"); setTimeout(() => {}, 9_000);`;
          const parent = `import { existsSync } from "node:fs"; Bun.spawn([process.execPath, "-e", ${JSON.stringify(grandchild)}], { stdin: "ignore", stdout: "ignore", stderr: "inherit" }); for (let i = 0; i < 400 && !existsSync(${JSON.stringify(readyPath)}); i++) await Bun.sleep(10); if (!existsSync(${JSON.stringify(readyPath)})) throw new Error("descendant did not start"); console.log("direct child exited"); process.exit(0);`;
          await Bun.write(join(dir, "inherit-stderr.ts"), parent);
          return "inherit-stderr.ts";
        },
      });
      const observed = await withIoDeadline(
        operation.then(
          (result) => ({ result, error: undefined }),
          (error: unknown) => ({ result: undefined, error }),
        ),
        12_000,
      );
      success = observed.result;
      failure = observed.error;
      expect(existsSync(readyPath)).toBe(true);
      if (success !== undefined) {
        // Bun on Windows can close the pipe with the direct child even when a
        // descendant stays alive. Unix must exercise the inherited-pipe path.
        expect(process.platform).toBe("win32");
        expect(success.stdout.trim()).toBe("direct child exited");
        expect(success.exitCode).toBe(0);
        expect(success.stderr).toBe("");
      } else {
        expect(failure).toBeInstanceOf(Error);
        expect((failure as Error).message).toContain("KELD-IPC-006");
        expect(failure).not.toBeInstanceOf(LayoutCleanupError);
        expect(failure).not.toBeInstanceOf(LayoutDisposalError);
      }
      expect(existsSync(root)).toBe(false);
    } finally {
      // The descendant exits on its own timer. No numeric PID is cleanup authority.
      if (
        (failure instanceof LayoutCleanupError ||
          failure instanceof LayoutDisposalError) &&
        failure.retainedLayout !== undefined &&
        !failure.retainedLayout.disposed
      ) {
        if (failure.retainedLayout.stderr !== undefined)
          await withIoDeadline(failure.retainedLayout.stderr.promise, 13_000);
        await withIoDeadline(failure.retainedLayout.dispose(), 5_000);
      }
      rmSync(audit, { recursive: true, force: true });
    }
  }, 25_000);

  test("stderr cancellation completion is required even after its reader reaches EOF", async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    let cancelEntered = false;
    let root = "";
    let owner: RetainedLayout | undefined;
    try {
      const failure = await rejected(
        runLayout(
          {
            deadlineMs: 1_000,
            ready: (stdout) => stdout.includes("ready"),
            async write(dir) {
              root = dir;
              await Bun.write(
                join(dir, "ready-stderr.ts"),
                'console.log("ready"); setInterval(() => {}, 60_000);',
              );
              return "ready-stderr.ts";
            },
          },
          {
            stderrOwner(stream) {
              const nativeReader = stream.getReader();
              type ReadResult = Awaited<ReturnType<typeof nativeReader.read>>;
              let finishRead!: (chunk: ReadResult) => void;
              const pending = new Promise<ReadResult>((resolve) => {
                finishRead = resolve;
              });
              return {
                reader: {
                  read: () => pending,
                  async cancel() {
                    // The reader sees cancellation EOF before the source closes.
                    finishRead({ done: true, value: undefined });
                    cancelEntered = true;
                    await gate;
                    await stream.cancel();
                  },
                  releaseLock: () => nativeReader.releaseLock(),
                },
              };
            },
          },
        ),
      );
      expect(failure).toBeInstanceOf(LayoutCleanupError);
      if (!(failure instanceof LayoutCleanupError))
        throw new Error("missing retained stderr cancellation owner");
      owner = failure.retainedLayout;
      expect(owner).toBeDefined();
      if (owner === undefined) throw new Error("missing retained layout");
      expect(cancelEntered).toBe(true);
      expect(owner.stderr?.state).toBe("fulfilled");
      expect(owner.disposed).toBe(false);
      expect(existsSync(root)).toBe(true);
      // A completed read is not proof that its cancellation has finalized.
      expect(await rejected(owner.dispose())).toBeInstanceOf(Error);
      expect(owner.disposed).toBe(false);
      expect(existsSync(root)).toBe(true);
      release();
      await withIoDeadline(owner.dispose(), 5_000);
      expect(owner.disposed).toBe(true);
      expect(existsSync(root)).toBe(false);
    } finally {
      release();
      if (owner !== undefined && !owner.disposed)
        await withIoDeadline(owner.dispose(), 5_000);
    }
  }, 20_000);

  for (const eofBeforeReject of [true, false] as const) {
    test(`rejected stderr cancellation recovers after owned source finalization (synthetic EOF: ${eofBeforeReject})`, async () => {
      const audit = mkdtempSync(
        join(tmpdir(), "keld-layout-stderr-finalizer-"),
      );
      const fd = openSync(join(audit, "retained-source"), "w+");
      const cancelError = new Error(
        "intentional stderr cancellation rejection",
      );
      const finalizeError = new Error(
        "intentional initial stderr source-finalization failure",
      );
      let root = "";
      let cancelCalls = 0;
      let finalizationCalls = 0;
      let closed = false;
      let owner: RetainedLayout | undefined;
      try {
        const error = await rejected(
          runLayout(
            {
              deadlineMs: 1_000,
              ready: (stdout) => stdout.includes("ready"),
              async write(dir) {
                root = dir;
                await Bun.write(
                  join(dir, "stderr-rejection.ts"),
                  'console.log("ready"); setInterval(() => {}, 60_000);',
                );
                return "stderr-rejection.ts";
              },
            },
            {
              stderrOwner(stream) {
                const nativeReader = eofBeforeReject
                  ? stream.getReader()
                  : undefined;
                const heldReader = eofBeforeReject
                  ? undefined
                  : new ReadableStream<Uint8Array>().getReader();
                type ReadResult = Awaited<
                  ReturnType<NonNullable<typeof nativeReader>["read"]>
                >;
                let finishRead!: (chunk: ReadResult) => void;
                const pending = new Promise<ReadResult>((resolve) => {
                  finishRead = resolve;
                });
                return {
                  reader: {
                    read: () =>
                      eofBeforeReject ? pending : heldReader!.read(),
                    async cancel() {
                      cancelCalls++;
                      // Either synthetic EOF or a still-pending native read must
                      // leave cancellation finalization to the separate owner.
                      if (eofBeforeReject)
                        finishRead({ done: true, value: undefined });
                      throw cancelError;
                    },
                    releaseLock: () => {
                      if (eofBeforeReject) nativeReader!.releaseLock();
                      else heldReader!.releaseLock();
                    },
                  },
                  async finalizeSource() {
                    finalizationCalls++;
                    if (finalizationCalls === 1) throw finalizeError;
                    await stream.cancel();
                    closeSync(fd);
                    closed = true;
                  },
                };
              },
            },
          ),
        );
        expect(error).toBeInstanceOf(LayoutCleanupError);
        if (!(error instanceof LayoutCleanupError))
          throw new Error("stderr cancellation failure was not preserved");
        expect(error.errors).toContain(cancelError);
        expect(error.errors).toContain(finalizeError);
        owner = error.retainedLayout;
        expect(owner).toBeDefined();
        if (owner === undefined) throw new Error("lost retained layout owner");
        expect(cancelCalls).toBe(1);
        expect(finalizationCalls).toBe(1);
        expect(owner.stderr?.state).toBe(
          eofBeforeReject ? "fulfilled" : "rejected",
        );
        if (!eofBeforeReject)
          expect(await rejected(owner.stderr!.promise)).toBeInstanceOf(
            TypeError,
          );
        expect(fstatSync(fd).isFile()).toBe(true);
        expect(owner.disposed).toBe(false);
        expect(existsSync(root)).toBe(true);
        await withIoDeadline(owner.dispose(), 5_000);
        expect(cancelCalls).toBe(1);
        expect(finalizationCalls).toBe(2);
        expect(closed).toBe(true);
        expect(() => fstatSync(fd)).toThrow();
        expect(owner.disposed).toBe(true);
        expect(existsSync(root)).toBe(false);
        await owner.dispose();
        expect(finalizationCalls).toBe(2);
      } finally {
        if (owner !== undefined && !owner.disposed)
          await withIoDeadline(owner.dispose(), 5_000);
        if (!closed) closeSync(fd);
        rmSync(audit, { recursive: true, force: true });
      }
    }, 20_000);
  }

  test("stderr read failure keeps source custody until explicit finalization succeeds", async () => {
    const audit = mkdtempSync(join(tmpdir(), "keld-stderr-read-failure-"));
    const fd = openSync(join(audit, "owned-stderr-source"), "w+");
    const readError = new Error("intentional stderr read failure");
    const finalizationError = new Error(
      "intentional first source finalization failure",
    );
    let root = "";
    let finalized = false;
    let attempts = 0;
    let cancels = 0;
    let owner: RetainedLayout | undefined;
    try {
      const error = await rejected(
        runLayout(
          {
            deadlineMs: 1_000,
            waitForExit: true,
            ready: () => false,
            async write(dir) {
              root = dir;
              await Bun.write(
                join(dir, "failed-stderr.ts"),
                'console.log("ready"); setInterval(() => {}, 60_000);',
              );
              return "failed-stderr.ts";
            },
          },
          {
            stderrOwner(stream) {
              return {
                reader: {
                  read: () => Promise.reject(readError),
                  async cancel() {
                    cancels++;
                  },
                  releaseLock() {},
                },
                async finalizeSource() {
                  attempts++;
                  if (attempts <= 2) throw finalizationError;
                  await stream.cancel();
                  closeSync(fd);
                  finalized = true;
                },
              };
            },
          },
        ),
      );
      expect(error).toBeInstanceOf(LayoutCleanupError);
      if (!(error instanceof LayoutCleanupError))
        throw new Error("missing stderr read failure");
      expect(error.cause).toBe(readError);
      expect(error.errors).toContain(readError);
      expect(error.errors).toContain(finalizationError);
      owner = error.retainedLayout;
      expect(owner).toBeDefined();
      if (owner === undefined) throw new Error("missing retained stderr owner");
      expect(attempts).toBe(2);
      expect(cancels).toBe(0);
      expect(fstatSync(fd).isFile()).toBe(true);
      expect(owner.stderr?.state).toBe("rejected");
      expect(owner.disposed).toBe(false);
      expect(existsSync(root)).toBe(true);
      const recovery = await rejected(withIoDeadline(owner.dispose(), 5_000));
      expect(recovery).toBeInstanceOf(AggregateError);
      expect((recovery as AggregateError).errors).toContain(readError);
      expect(attempts).toBe(3);
      expect(finalized).toBe(true);
      expect(() => fstatSync(fd)).toThrow();
      expect(owner.disposed).toBe(true);
      expect(existsSync(root)).toBe(false);
      await owner.dispose();
      expect(attempts).toBe(3);
    } finally {
      if (owner !== undefined && !owner.disposed)
        await rejected(withIoDeadline(owner.dispose(), 5_000));
      if (!finalized) closeSync(fd);
      rmSync(audit, { recursive: true, force: true });
    }
  }, 20_000);

  test("a stderr read error without an external finalizer closes the owned native pipe", async () => {
    const readError = new Error(
      "intentional native stderr read-wrapper failure",
    );
    let root = "";
    const failure = await rejected(
      runLayout(
        {
          deadlineMs: 1_000,
          waitForExit: true,
          ready: () => false,
          async write(dir) {
            root = dir;
            await Bun.write(
              join(dir, "failed-native-stderr.ts"),
              'console.log("ready"); setInterval(() => {}, 60_000);',
            );
            return "failed-native-stderr.ts";
          },
        },
        {
          stderrOwner() {
            return {
              reader: {
                read: () => Promise.reject(readError),
                async cancel() {
                  throw new Error(
                    "reader cancellation must not replace a read failure",
                  );
                },
                releaseLock() {},
              },
            };
          },
        },
      ),
    );
    expect(failure).toBeInstanceOf(LayoutCleanupError);
    if (!(failure instanceof LayoutCleanupError))
      throw new Error("lost stderr read failure");
    expect(failure.cause).toBe(readError);
    expect(failure.errors).toContain(readError);
    expect(root).not.toBe("");
    expect(existsSync(root)).toBe(false);
    if (failure.retainedLayout !== undefined)
      expect(failure.retainedLayout.disposed).toBe(true);
  }, 10_000);

  test("natural exit drains complete stderr before removing its layout root", async () => {
    const result = await runLayout({
      deadlineMs: 3_000,
      ready: () => false,
      waitForExit: true,
      async write(dir) {
        await Bun.write(
          join(dir, "natural-stderr.ts"),
          'console.error("direct-stderr"); console.log("done");',
        );
        return "natural-stderr.ts";
      },
    });
    expect(result.stdout.trim()).toBe("done");
    expect(result.stderr?.trim()).toBe("direct-stderr");
    expect(result.exitCode).toBe(0);
    expect(existsSync(result.root)).toBe(false);
  }, 10_000);

  test("a stalled callback retains its real layout until owner disposal after settlement", async () => {
    let release!: () => void;
    const callback = new Promise<void>((resolve) => {
      release = resolve;
    });
    let callbackEntered = false;
    let root = "";
    let owned: RetainedFailure | undefined;
    try {
      const error = await rejected(
        withIoDeadline(
          runLayout({
            deadlineMs: 1000,
            ready: (text) => text.includes("ready"),
            async write(dir) {
              root = dir;
              await Bun.write(
                join(dir, "ready.ts"),
                'console.log("ready"); setInterval(() => {}, 60_000);\n',
              );
              return "ready.ts";
            },
            async beforeCleanup() {
              callbackEntered = true;
              await callback;
            },
          }),
          10_000,
        ),
      );
      expect(callbackEntered).toBe(true);
      owned = retained(error);
      expect((owned.cause as Error).message).toContain("KELD-IPC-006");
      expect(owned.retainedLayout.root).toBe(root);
      expect(owned.retainedLayout.child).toBeDefined();
      expect(owned.retainedLayout.listener).toBeDefined();
      expect(owned.retainedLayout.disposed).toBe(false);
      expect(existsSync(root)).toBe(true);
      release();
      await withIoDeadline(owned.retainedLayout.dispose());
      expect(owned.retainedLayout.disposed).toBe(true);
      expect(
        await withIoDeadline(owned.retainedLayout.child!.exited),
      ).toBeNumber();
      expectExited(owned.retainedLayout.child!.pid);
      expect(existsSync(root)).toBe(false);
      await owned.retainedLayout.dispose();
      // A fresh native layout proves the failed run did not poison the runner.
      const next = await runLayout({
        ready: (text) => text.includes("healthy"),
        waitForExit: true,
        async write(dir) {
          await Bun.write(join(dir, "healthy.ts"), 'console.log("healthy");\n');
          return "healthy.ts";
        },
      });
      expect(next.stdout.trim()).toBe("healthy");
      expect(next.stderr).toBe("");
      expect(next.exitCode).toBe(0);
      expect(existsSync(next.root)).toBe(false);
    } finally {
      release();
      if (owned !== undefined && !owned.retainedLayout.disposed) {
        await withIoDeadline(owned.retainedLayout.dispose());
      }
      if (root !== "") rmSync(root, { recursive: true, force: true });
    }
  }, 15_000);
});

import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  LayoutCleanupError,
  LayoutDisposalError,
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
              expect(owner.disposed).toBe(true);
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

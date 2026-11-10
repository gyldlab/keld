import assert from "node:assert/strict";
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
  runLayout,
  withLayoutOutput,
} from "./bundle-layout.ts";
import { withIoDeadline } from "../src/transport.ts";

const mode = process.argv[2];
const rejected = (action: Promise<unknown>) =>
  action.then(
    () => undefined,
    (error: unknown) => error,
  );

async function fileSource(): Promise<void> {
  const root = mkdtempSync(join(tmpdir(), "keld-source-finalization-"));
  const fd = openSync(join(root, "source"), "w+");
  let closed = false;
  const close = () => {
    if (!closed) {
      closeSync(fd);
      closed = true;
    }
  };
  const cancellationFailure = new Error("underlying source close failed");
  const primary = new Error("initiating use failed");
  const finalizationFailure = new Error("resource close not yet available");
  let cancellations = 0;
  let finalizations = 0;
  let release!: () => void;
  const pending = new Promise<void>((resolve) => {
    release = resolve;
  });
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      if (mode !== "cancel-eof")
        controller.enqueue(new TextEncoder().encode("ready"));
    },
    cancel() {
      cancellations += 1;
      return Promise.reject(cancellationFailure);
    },
  });
  const owned = mode === "owned" || mode === "retry" || mode === "pending";
  try {
    const error = await rejected(
      withLayoutOutput(
        stream.getReader(),
        { ready: (text) => text === "ready", deadlineMs: 40 },
        mode === "cancel-eof"
          ? async () => {
              throw primary;
            }
          : (text) => text,
        async () => {},
        owned
          ? async () => {
              finalizations += 1;
              if (mode === "retry" && finalizations === 1)
                throw finalizationFailure;
              if (mode === "pending") await pending;
              close();
            }
          : undefined,
      ),
    );
    assert(error instanceof LayoutCleanupError);
    assert(error.errors.includes(cancellationFailure));
    if (mode === "cancel-eof") assert.equal(error.cause, primary);
    assert.equal(error.cleanup.cancellation.state, "rejected");
    assert.equal(stream.locked, false);
    assert(fstatSync(fd).isFile());
    assert.equal(error.cleanup.incomplete, true);
    assert.equal(error.cleanup.sourceFinalized, false);
    // WHATWG closes the stream before its cancel hook settles. Calling cancel
    // again does not re-enter the hook or close this separately owned file.
    await stream.cancel();
    assert.equal(cancellations, 1);
    assert(fstatSync(fd).isFile());
    assert.equal(error.cleanup.sourceFinalized, false);
    if (!owned) {
      const failure = await rejected(error.cleanup.finalizeSource());
      assert(
        failure instanceof Error &&
          failure.message.includes("no resource-owner finalizer"),
      );
      assert.equal(error.cleanup.incomplete, true);
      close();
      assert.throws(() => fstatSync(fd), { code: "EBADF" });
      // Emergency fixture cleanup is not evidence recorded by the helper.
      assert.equal(error.cleanup.sourceFinalized, false);
    } else {
      if (mode === "retry") {
        assert.equal(
          await rejected(error.cleanup.finalizeSource()),
          finalizationFailure,
        );
        assert.equal(error.cleanup.incomplete, true);
        assert(fstatSync(fd).isFile());
      }
      if (mode === "pending") {
        const attempts = await Promise.all([
          rejected(error.cleanup.finalizeSource()),
          rejected(error.cleanup.finalizeSource()),
        ]);
        assert(attempts.every((failure) => failure instanceof Error));
        assert.equal(finalizations, 1);
        assert.equal(error.cleanup.incomplete, true);
        assert(fstatSync(fd).isFile());
        release();
      }
      await Promise.all([
        error.cleanup.finalizeSource(),
        error.cleanup.finalizeSource(),
      ]);
      assert.throws(() => fstatSync(fd), { code: "EBADF" });
      assert.equal(error.cleanup.sourceFinalized, true);
      assert.equal(error.cleanup.incomplete, false);
      assert.equal(finalizations, mode === "retry" ? 2 : 1);
      await error.cleanup.finalizeSource();
      assert.equal(finalizations, mode === "retry" ? 2 : 1);
      assert(error.errors.includes(cancellationFailure));
      assert.equal(cancellations, 1);
    }
  } finally {
    release();
    close();
    rmSync(root, { recursive: true, force: true });
  }
}

async function outerLayout(): Promise<void> {
  const cancellationFailure = new Error("stdout cancel wrapper failed");
  const finalizationFailure = new Error("stdout owner finalizer unavailable");
  let attempts = 0;
  let stdout: ReadableStream<Uint8Array> | undefined;
  let failure: LayoutCleanupError | undefined;
  let root = "";
  try {
    const error = await rejected(
      runLayout(
        {
          deadlineMs: 1000,
          ready: (text) => text.includes("ready"),
          async write(dir) {
            root = dir;
            await Bun.write(
              join(dir, "ready.ts"),
              'console.log("ready"); setInterval(() => {}, 60000);\n',
            );
            return "ready.ts";
          },
        },
        {
          outputOwner(stream) {
            stdout = stream;
            const reader = stream.getReader();
            const outputOwner = {
              reader: {
                read: () => reader.read(),
                // Reject before native cancellation; the finalizer owns the first
                // real source cancellation, not a retry of a rejected source hook.
                cancel: async () => {
                  throw cancellationFailure;
                },
                releaseLock: () => reader.releaseLock(),
              },
              finalizeSource:
                mode === "outer-unowned"
                  ? undefined
                  : async () => {
                      attempts += 1;
                      if (attempts === 1) throw finalizationFailure;
                      await stream.cancel();
                    },
            };
            if (mode === "method-owner") {
              const methodOwner = {
                ...outputOwner,
                stream,
                async finalizeSource() {
                  attempts += 1;
                  if (attempts === 1) throw finalizationFailure;
                  await this.stream.cancel();
                },
              };
              return methodOwner;
            }
            return outputOwner;
          },
        },
      ),
    );
    assert(error instanceof LayoutCleanupError);
    failure = error;
    assert(error.errors.includes(cancellationFailure));
    assert.equal(error.cleanup.sourceFinalized, false);
    assert.equal(error.cleanup.incomplete, true);
    const owner = error.retainedLayout;
    assert(owner);
    assert.equal(owner.output, error.cleanup);
    assert.equal(owner.disposed, false);
    assert(existsSync(root));
    if (mode === "outer-unowned") {
      assert((await rejected(owner.dispose())) instanceof Error);
      assert.equal(owner.disposed, false);
      assert.equal(error.cleanup.incomplete, true);
    } else {
      assert(error.errors.includes(finalizationFailure));
      await owner.dispose();
      assert.equal(attempts, 2);
      assert.equal(error.cleanup.sourceFinalized, true);
      assert.equal(error.cleanup.incomplete, false);
      assert.equal(owner.disposed, true);
      assert.equal(existsSync(root), false);
      assert(error.errors.includes(cancellationFailure));
      await owner.dispose();
      assert.equal(attempts, 2);
    }
  } finally {
    // Rescue is separate from the assertions above and cannot certify disposal.
    if (stdout && !stdout.locked) await stdout.cancel();
    failure?.retainedLayout?.listener?.stop(true);
    const child = failure?.retainedLayout?.child;
    if (child) {
      child.kill();
      await withIoDeadline(child.exited);
    }
    if (root) rmSync(root, { recursive: true, force: true });
  }
}

async function nativePipe(): Promise<void> {
  const script = `
    import { readFileSync, writeSync } from "node:fs";
    writeSync(1, "ready\\n");
    readFileSync(0);
    try { writeSync(1, "probe"); process.exit(42); }
    catch (error) {
      if (error.code !== "EPIPE") throw error;
      writeSync(2, "NATIVE_EPIPE\\n"); process.exit(23);
    }
  `;
  const child = Bun.spawn([process.execPath, "-e", script], {
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });
  const stream = child.stdout;
  const reader = stream.getReader();
  const cancellationFailure = new Error("cancel wrapper refused native close");
  const stderr = new Response(child.stderr).text();
  try {
    const error = await rejected(
      withLayoutOutput(
        {
          read: () => reader.read(),
          cancel: async () => {
            throw cancellationFailure;
          },
          releaseLock: () => reader.releaseLock(),
        },
        { ready: (text) => text.includes("ready"), deadlineMs: 1000 },
        (text) => text,
        async () => {},
        async () => {
          await stream.cancel();
        },
      ),
    );
    assert(error instanceof LayoutCleanupError);
    assert.equal(error.cleanup.incomplete, true);
    assert.equal(child.exitCode, null);
    await error.cleanup.finalizeSource();
    // The independent native writer observes closing the parent's read end.
    // A helper boolean or child exit before this command cannot satisfy it.
    child.stdin.end();
    assert.equal(await withIoDeadline(child.exited), 23);
    assert.equal(await withIoDeadline(stderr), "NATIVE_EPIPE\n");
    assert.equal(error.cleanup.sourceFinalized, true);
    assert.equal(error.cleanup.incomplete, false);
    assert(error.errors.includes(cancellationFailure));
  } finally {
    child.stdin.end();
    child.kill();
    await withIoDeadline(child.exited);
    if (!stream.locked) await stream.cancel();
  }
}

if (
  ["unowned", "cancel-eof", "owned", "retry", "pending"].includes(mode ?? "")
) {
  await fileSource();
} else if (
  mode === "outer" ||
  mode === "outer-unowned" ||
  mode === "method-owner"
) {
  await outerLayout();
} else if (mode === "native-pipe") {
  await nativePipe();
} else {
  throw new Error("unknown finalization fixture mode");
}
console.log(`FINALIZATION_OK ${mode}`);

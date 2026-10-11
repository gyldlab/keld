/**
 * #528 T3 (owner decision recorded on #528): a release build of the transport
 * has no WorkerLink test-hook code path. The release build is this file built
 * on its own (never inlined into an app entry, §4.2) with the build-time
 * constant `KELD_KIPC_RELEASE` defined as `true` and syntax minification, so
 * every `TEST_HOOKS` branch is dead code and removed.
 */
import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { DrainSignal, FrameKind, HANDSHAKE_CHANNEL, WriteQueue, stampTransport, withIoDeadline } from "./transport.ts";
import { LayoutCleanupError, TOKEN_HEX, runLayout, withLayoutOutput, type LayoutOutputReader } from "../test/bundle-layout.ts";

const TRANSPORT = join(import.meta.dir, "transport.ts");
// Immutable header from receiver-semantics-v0.tsv's hello-valid row.
const HELLO_HEADER_HEX = "4b490200000000000000000020000000";

/** Every hook name and the hook entry's env gate and seam key. */
const HOOK_NAMES = [
  "openWorkerLinkForTest",
  "deferDispatch",
  "claimFault",
  "onBlockingCall",
  "holdMessagesUntilBlocking",
  "beforeStateCheck",
  "beforeWakeDrain",
  "beforeDeadlineCas",
  "onDispatchIdle",
  "counterStart",
  "wedgeHoldingKick",
  "onWorkerExit",
  "beforeWorkerSpawn",
  "KELD_KIPC_TEST_HOOKS",
  "worker-link-test-seam",
];

async function build(release: boolean): Promise<string> {
  const built = await Bun.build({
    entrypoints: [TRANSPORT],
    target: "bun",
    format: "esm",
    minify: { syntax: true },
    ...(release ? { define: { KELD_KIPC_RELEASE: "true" } } : {}),
  });
  expect(built.success).toBe(true);
  return built.outputs[0]!.text();
}

async function runRelease(stamped: boolean, validReceive = true) {
  const expected = Buffer.from(HELLO_HEADER_HEX + TOKEN_HEX, "hex");
  const observed = new Uint8Array(expected.length);
  const drain = new DrainSignal();
  let bytes = 0;
  let hellos = 0;
  let hostError: Error | undefined;
  let helloReply: Promise<void> | undefined;
  let peerCloses = 0;
  let peerClosed!: () => void;
  const closed = new Promise<void>((resolve) => { peerClosed = resolve; });
  const outcome = await runLayout({
    ready: (stdout) => stdout.includes('"done":true'),
    waitForExit: true,
    socketObserver: {
      data(socket, chunk) {
        if (bytes + chunk.length > observed.length || hellos !== 0) {
          hostError = new Error("release transport sent more than one HELLO envelope");
          socket.end();
          return;
        }
        observed.set(chunk, bytes);
        bytes += chunk.length;
        if (bytes !== observed.length) return;
        if (!Buffer.from(observed).equals(expected)) {
          hostError = new Error(`release HELLO bytes differ: ${Buffer.from(observed).toString("hex")}`);
          socket.end();
          return;
        }
        hellos += 1;
        helloReply = new WriteQueue(socket, drain).writeFrame(
          FrameKind.Hello, 0, HANDSHAKE_CHANNEL, 0, new Uint8Array(Buffer.from(TOKEN_HEX, "hex")),
        );
        void helloReply.catch((error: unknown) => {
          hostError = error instanceof Error ? error : new Error(String(error));
          socket.end();
        });
      },
      drain() { drain.fire(); },
      close() { peerCloses += 1; peerClosed(); },
      error(error) { hostError = error; },
    },
    async beforeCleanup(result) {
      // A refused open has no peer. Connected cases must close naturally
      // before listener.stop(true) could supply the close being observed.
      if (result.connections !== 0) {
        if (helloReply !== undefined) await withIoDeadline(helloReply);
        await withIoDeadline(closed);
      }
    },
    async write(dir) {
      const text = await build(true);
      await Bun.write(join(dir, "kipc-transport.js"), stamped ? stampTransport(text) : text);
      await Bun.write(join(dir, "probe.ts"),
        'import { appendFileSync } from "node:fs";\n' +
        'import { WorkerLink, isCallError } from "./kipc-transport.js";\n' +
        'appendFileSync(process.env.KELD_TEST_EVALS!, "app-entry\\n");\n' +
        'const seam = Symbol.for("keld.kipc.worker-link-test-seam/v1") in globalThis;\n' +
        "try {\n" +
        `  const link = await WorkerLink.open({ link: process.env.KELD_APP_LINK!, receive: ${validReceive ? "{ eventChannels: [3], callReceivers: [] }" : "undefined as never"} });\n` +
        '  const ended = new Promise((resolve) => link.onEnd((error) => resolve(error.code)));\n' +
        '  console.log(JSON.stringify({ seam, opened: true }));\n' +
        '  link.close();\n' +
        '  console.log(JSON.stringify({ done: true, endCode: await ended }));\n' +
        "} catch (error) {\n" +
        '  console.log(JSON.stringify({ done: true, seam, code: isCallError(error) ? error.code : "untyped", message: error instanceof Error ? error.message : String(error) }));\n' +
        "}\n",
      );
      return "probe.ts";
    },
  });
  return { ...outcome, bytes: Buffer.from(observed.subarray(0, bytes)), hellos, hostError, peerCloses };
}

describe("a release build of the transport strips the WorkerLink test hooks", () => {
  test("it names no hook, and keeps the link, its Worker entry and the Quit", async () => {
    const text = await build(true);
    expect(HOOK_NAMES.filter((name) => text.includes(name))).toEqual([]);
    expect(text).toContain("class WorkerLink");
    expect(text).toContain("keld-kipc-worker-link/v1");
    expect(text).toContain("quitAndCloseLink");
  });

  test("negative control: the same build without the constant keeps every hook", async () => {
    const text = await build(false);
    expect(HOOK_NAMES.filter((name) => !text.includes(name))).toEqual([]);
  });

  test("a stamped release transport completes HELLO without replaying the app", async () => {
    const outcome = await runRelease(true);
    expect(outcome.connections).toBe(1);
    expect(outcome.hellos).toBe(1);
    expect(outcome.bytes.toString("hex")).toBe(HELLO_HEADER_HEX + TOKEN_HEX);
    expect(outcome.hostError).toBeUndefined();
    expect(outcome.stdout.trim().split("\n").map((line) => JSON.parse(line))).toEqual([
      { seam: false, opened: true }, { done: true, endCode: "KELD-IPC-022" },
    ]);
    expect(outcome.evals).toBe(1);
    expect(outcome.peerCloses).toBe(1);
    expect(outcome.exitCode).toBe(0);
    expect(outcome.stderr).toBe("");
    expect(existsSync(outcome.root)).toBe(false);
  }, 30_000);

  test("an unstamped release with valid receive config is refused before HELLO", async () => {
    const outcome = await runRelease(false);
    const report = JSON.parse(outcome.stdout.trim());
    expect({ connections: outcome.connections, hellos: outcome.hellos, bytes: outcome.bytes.length }).toEqual({
      connections: 0, hellos: 0, bytes: 0,
    });
    expect(report).toMatchObject({ done: true, seam: false, code: "KELD-IPC-005" });
    expect(report.message).toContain("not the stamped canonical transport");
    expect(outcome.exitCode).toBe(0);
    expect(existsSync(outcome.root)).toBe(false);
  }, 30_000);

  test("invalid receive config is distinct from a release stamp refusal", async () => {
    const outcome = await runRelease(true, false);
    expect({ connections: outcome.connections, hellos: outcome.hellos, bytes: outcome.bytes.length }).toEqual({
      connections: 0, hellos: 0, bytes: 0,
    });
    expect(JSON.parse(outcome.stdout.trim())).toEqual({
      done: true, seam: false, code: "KELD-IPC-005",
      message: "KELD-IPC-005: receive must declare eventChannels and callReceivers arrays",
    });
    expect(outcome.exitCode).toBe(0);
    expect(existsSync(outcome.root)).toBe(false);
  }, 30_000);

  test("aggregate failure joins stdout terminal work and preserves the original error", async () => {
    type ReadResult = Awaited<ReturnType<LayoutOutputReader["read"]>>;
    const events: string[] = [];
    let readEntered!: () => void;
    const reading = new Promise<void>((resolve) => { readEntered = resolve; });
    let finishRead!: (result: ReadResult) => void;
    const terminalRead = new Promise<ReadResult>((resolve) => { finishRead = resolve; });
    let cancelEntered!: () => void;
    const cancelling = new Promise<void>((resolve) => { cancelEntered = resolve; });
    let finishCancel!: () => void;
    const cancelBarrier = new Promise<void>((resolve) => { finishCancel = resolve; });
    let cancelCompleted!: () => void;
    const cancelled = new Promise<void>((resolve) => { cancelCompleted = resolve; });
    let released = false;
    // Owned fake reader: cancellation completion and terminal read completion
    // are separate barriers. This proves task ownership, not an OS leak.
    const reader: LayoutOutputReader = {
      read() { events.push("read"); readEntered(); return terminalRead; },
      async cancel() {
        events.push("cancel"); cancelEntered();
        await cancelBarrier;
        events.push("cancelled"); cancelCompleted();
      },
      releaseLock() { released = true; events.push("released"); },
    };
    const original = new Error("intentional aggregate rejection while stdout is pending");
    let returned = false;
    const task = withLayoutOutput(reader, { ready: () => false, waitForExit: true }, async () => {
      await reading;
      throw original;
    }, async () => { events.push("stop"); });
    const result = task.then(
      () => { returned = true; events.push("returned"); return undefined; },
      (error: unknown) => { returned = true; events.push("returned"); return error; },
    );
    try {
      await withIoDeadline(cancelling);
      finishCancel();
      await withIoDeadline(cancelled);
      // One scheduled task after cancellation, with the read barrier held:
      // a missing join has returned by now. No elapsed-time predicate.
      await new Promise<void>((resolve) => setImmediate(resolve));
      expect(returned).toBe(false);
      expect(released).toBe(false);
      finishRead({ done: true, value: undefined });
      expect(await withIoDeadline(result)).toBe(original);
      expect(released).toBe(true);
      expect(events).toEqual(["read", "stop", "cancel", "cancelled", "released", "returned"]);
    } finally {
      finishCancel();
      finishRead({ done: true, value: undefined });
      await withIoDeadline(result);
    }
  }, 30_000);

  test("a real stream reports incomplete source cancellation with the original failure", async () => {
    let readEntered!: () => void;
    const reading = new Promise<void>((resolve) => { readEntered = resolve; });
    let finishPull!: () => void;
    const pullBarrier = new Promise<void>((resolve) => { finishPull = resolve; });
    let cancelEntered!: () => void;
    const cancelling = new Promise<void>((resolve) => { cancelEntered = resolve; });
    let finishCancel!: () => void;
    const cancelBarrier = new Promise<void>((resolve) => { finishCancel = resolve; });
    const stream = new ReadableStream<Uint8Array>({
      pull() { readEntered(); return pullBarrier; },
      cancel() { cancelEntered(); return cancelBarrier; },
    });
    const reader = stream.getReader();
    const original = new Error("intentional aggregate failure with pending source cancellation");
    let cleanup: LayoutCleanupError | undefined;
    const task = withLayoutOutput(reader, { ready: () => false, waitForExit: true }, async () => {
      await reading;
      throw original;
    }, async () => {});
    const result = task.then(() => undefined, (error: unknown) => error);
    try {
      await withIoDeadline(cancelling);
      const error = await result;
      expect(error).toBeInstanceOf(LayoutCleanupError);
      if (!(error instanceof LayoutCleanupError)) throw new Error("missing incomplete-cleanup report");
      cleanup = error;
      expect(error.cause).toBe(original);
      expect(error.errors[0]).toBe(original);
      expect(error.errors.some((fault: unknown) => fault instanceof Error && fault.message.startsWith("KELD-IPC-006"))).toBe(true);
      expect(error.cleanup.incomplete).toBe(true);
      expect(error.cleanup.reader).toBe(reader);
      expect(error.cleanup.readerReleased).toBe(true);
      expect(error.cleanup.stopChild.state).toBe("fulfilled");
      expect(error.cleanup.output.state).toBe("fulfilled");
      expect(error.cleanup.cancellation.state).toBe("pending");
      expect(stream.locked).toBe(false);
    } finally {
      finishPull();
      finishCancel();
      if (cleanup !== undefined) await withIoDeadline(cleanup.cleanup.cancellation.promise);
      await withIoDeadline(result);
    }
  }, 30_000);
});

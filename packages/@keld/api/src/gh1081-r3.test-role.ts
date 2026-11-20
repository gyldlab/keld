/**
 * GH-1081 round-3 research role (never merged; draft PR #1082). One public-API
 * app per process (a realm opens at most one WorkerLink). It records what its
 * own application handler saw, so `gh1081-r3.test.ts`, the fake host, can
 * compare the wire answer with an independent handler census.
 *
 * Every wait is on an observable: a host release byte on stdin, the host Ready
 * EVENT, or a settled call. Nothing here is a timer.
 */
import { writeSync } from "node:fs";
import { app, channels, echoChannel, fs, isCallError } from "./index.ts";

function report(key: string, value: unknown): void {
  writeSync(1, `${key}=${JSON.stringify(value)}\n`);
}

function codeOf(error: unknown): string {
  if (isCallError(error)) return error.code;
  return error instanceof Error ? `untyped:${error.message}` : `untyped:${String(error)}`;
}

/** Async stdin so the main thread keeps dispatching host CALLs while it waits. */
const stdin = Bun.stdin.stream().getReader();
async function hostRelease(): Promise<void> {
  const chunk = await stdin.read();
  if (chunk.done || chunk.value.byteLength !== 1) {
    throw new Error("gh1081 r3 role expected exactly one host release byte");
  }
}

/** Handler census: every label the application handler entered, in order. */
const handled: string[] = [];
/** Effects committed by the handler before it answered or threw. */
const effects: string[] = [];

function registerHandler(shape: "ok" | "throw-after-effect" | "throw-sync"): () => void {
  if (shape === "throw-sync") {
    return channels.handle(echoChannel, (request) => {
      handled.push(request.message);
      throw new Error("gh1081 r3 handler threw before any effect");
    });
  }
  return channels.handle(echoChannel, async (request) => {
    handled.push(request.message);
    if (shape === "throw-after-effect" && request.message.startsWith("throw")) {
      effects.push(request.message);
      throw new Error("gh1081 r3 handler threw after its effect");
    }
    return { message: `${request.message}-reply`, count: request.count + 1 };
  });
}

async function finish(): Promise<void> {
  report("handled", handled);
  report("effects", effects);
  await app.quit();
  report("done", true);
  // The stdin reader keeps the event loop alive; the Quit has settled.
  process.exit(0);
}

const scenario = process.argv[2];
if (scenario === "missing" || scenario === "ok-control") {
  // `missing`: the app never registers a handler. `ok-control`: the same
  // flow with the handler registered at module top (the negative control).
  if (scenario === "ok-control") registerHandler("ok");
  await app.whenReady();
  report("ready", true);
  await hostRelease();
  await finish();
} else if (scenario === "late") {
  // The Electron-style shape: the handler is registered only after a host
  // release that the fake host sends after its first CALL was answered.
  await app.whenReady();
  report("unregistered", true);
  await hostRelease();
  registerHandler("ok");
  report("registered", true);
  await hostRelease();
  await finish();
} else if (scenario === "throw-after-effect" || scenario === "throw-sync") {
  registerHandler(scenario);
  await app.whenReady();
  report("ready", true);
  await hostRelease();
  await finish();
} else if (scenario === "partial") {
  // A pending FS call is this role's only public witness of link death: it
  // rejects when the link ends. The handler census then says whether any
  // Echo CALL bytes the host wrote reached the application handler.
  registerHandler("ok");
  await app.whenReady();
  report("ready", true);
  // A call's own deadline (KELD-IPC-006) leaves the link alive; ask again
  // until a terminal code ends the link. The fake host never answers.
  let deadlines = 0;
  let end = await fs.read("/p").then(() => "resolved", codeOf);
  while (end === "KELD-IPC-006") {
    deadlines += 1;
    end = await fs.read("/p").then(() => "resolved", codeOf);
  }
  report("call-deadlines-before-end", deadlines);
  report("link-end", end);
  report("handled", handled);
  process.exit(0);
} else if (scenario === "fs-dead") {
  // Bun-side outbound calls. First: written, then the host closes before any
  // reply. Second: issued after the link already ended.
  await app.whenReady();
  report("ready", true);
  const first = await fs.read("/p").then(() => undefined, (error: unknown) => error);
  const second = await fs.read("/p").then(() => undefined, (error: unknown) => error);
  report("after-send", codeOf(first));
  report("before-send", codeOf(second));
  report("same-error-object", first !== undefined && first === second);
  process.exit(0);
} else if (scenario === "fs-deadline") {
  // The existing public FS call deadline (APP_LINK_IO_DEADLINE_MS) with a
  // host that read the CALL and never answers; then a late host REPLY for that
  // correlation, then one fresh call on the same link.
  await app.whenReady();
  report("ready", true);
  const started = performance.now();
  const first = await fs.read("/p").then(() => "resolved", codeOf);
  report("first", first);
  report("first-waited-ms", Math.round(performance.now() - started));
  await hostRelease();
  const second = await fs.read("/p").then((bytes) => `resolved:${bytes.length}`, codeOf);
  report("second", second);
  await finish();
} else {
  throw new Error(`unknown gh1081 r3 scenario ${scenario}`);
}

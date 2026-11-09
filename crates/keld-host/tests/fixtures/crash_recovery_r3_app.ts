// GH-1081 round-3 disposable public app (research only; never merged). Imports
// come from the public API prelude the test prepends. Every hold is an ordinary
// fixture HTTP wait or a synchronous block in this app's own code; nothing here
// is a host or runtime hook. The page labels each call; the label picks the
// stage at which the test kills this Bun process.
import { writeSync } from "node:fs";
const base = "http://127.0.0.1:__PORT__";
const handlerMode: "before-ready" | "inside-ready" | "never" = __HANDLER_MODE__;
const mainBlock: boolean = __MAIN_BLOCK__;
let handlerCalls = 0;
async function post(value: unknown): Promise<void> {
  const response = await fetch(base + "/observe", { method: "POST",
    headers: { "Content-Type": "text/plain" }, body: JSON.stringify(value) });
  if (!response.ok) throw new Error("gh1081 r3 app observation refused");
}
// A synchronous write to this app's stderr (relayed to the host's stderr), so
// the record exists before the next statement runs.
function marker(name: string, value: unknown): void {
  writeSync(2, name + " " + JSON.stringify(value) + "\n");
}
// Parks this app's main thread for good; only the test's SIGKILL ends it.
function blockForever(): never {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0);
  throw new Error("gh1081 r3 block returned");
}
function holdId(): string {
  return Array.from(crypto.getRandomValues(new Uint32Array(4)),
    value => value.toString(16).padStart(8, "0")).join("");
}
function registerEcho(): void {
  channels.handle(echoChannel, async request => {
    handlerCalls += 1;
    const label: string = JSON.parse(request.message).label;
    const record = { phase: "handler", pid: process.pid, call: handlerCalls,
      message: request.message, count: request.count };
    console.error("KELD_GH1081_HANDLER " + JSON.stringify(record));
    await post(record);
    if (label === "s3-during-handler") {
      // Held inside the handler; the test kills Bun while this wait is open.
      await fetch(base + "/recovery-pending/" + process.pid + "/" + holdId()).then(
        () => undefined, () => undefined);
      throw new Error("gh1081 r3 during-handler hold ended");
    }
    if (label === "throw-after-effect") {
      await post({ phase: "effect", pid: process.pid, message: request.message });
      throw new Error("gh1081 r3 handler threw after its effect");
    }
    if (label === "s4-after-handler") {
      await post({ phase: "handler-returning", pid: process.pid, message: request.message });
      // The handler returns this object. Keld's response encoder reads
      // `message` after the handler returned and before any reply bytes exist.
      return {
        count: request.count,
        get message(): string {
          marker("KELD_GH1081_R3_AFTER_HANDLER", { pid: process.pid, message: request.message });
          return blockForever();
        },
      };
    }
    return { message: JSON.stringify({ pid: process.pid, call: handlerCalls }), count: request.count };
  });
}
if (handlerMode === "before-ready") registerEcho();
const bootstrap = process.env.KELD_APP_LINK;
if (!bootstrap) throw new Error("gh1081 r3 app has no host-minted link");
await post({ phase: "recovery-app-boot", pid: process.pid, appLink: bootstrap });
const admission = await fetch(base + "/recovery-admit/" + process.pid);
if (!admission.ok || await admission.text() !== "admit") {
  throw new Error("invalid gh1081 r3 admission command");
}
await app.whenReady();
await post({ phase: "app-ready", pid: process.pid, handlerMode,
  endpoint: bootstrap.slice(0, bootstrap.lastIndexOf("#")),
  executable: process.execPath, bunVersion: Bun.version });
if (handlerMode === "inside-ready") {
  // Registration waits for the test to drop this held fetch.
  const released = await fetch(base + "/recovery-pending/" + process.pid + "/" + holdId()).then(
    () => false,
    () => true,
  );
  if (!released) throw new Error("gh1081 r3 registration hold was answered instead of released");
  registerEcho();
  await post({ phase: "handler-registered", pid: process.pid });
}
if (mainBlock) {
  // Not awaited, so a successor that is never released still reaches its Quit
  // wait. Dropping the hold parks the main thread; the transport Worker runs on.
  void fetch(base + "/recovery-pending/" + process.pid + "/" + holdId()).then(
    () => { throw new Error("gh1081 r3 main-block hold was answered"); },
    () => {
      marker("KELD_GH1081_R3_MAIN_BLOCKED", { pid: process.pid });
      blockForever();
    },
  );
}
const quit = await fetch(base + "/recovery-quit/" + process.pid);
if (!quit.ok || await quit.text() !== "quit") throw new Error("invalid gh1081 r3 quit command");
console.error("KELD_GH1081_QUIT_REQUEST calls=" + handlerCalls);
await app.quit();
process.exit(0);

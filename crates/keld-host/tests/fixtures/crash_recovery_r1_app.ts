// GH-1081 round-1 disposable public app (research only). Imports come from the
// public API prelude the test prepends. Every hold is an ordinary HTTP wait the
// test owns; nothing here is a host or runtime hook.
const base = "http://127.0.0.1:__PORT__";
const handlerMode: "before-ready" | "inside-ready" = __HANDLER_MODE__;
let handlerCalls = 0;
async function post(value: unknown): Promise<void> {
  const response = await fetch(base + "/observe", { method: "POST",
    headers: { "Content-Type": "text/plain" }, body: JSON.stringify(value) });
  if (!response.ok) throw new Error("gh1081 app observation refused");
}
function registerEcho(): void {
  channels.handle(echoChannel, async request => {
    handlerCalls += 1;
    const record = { phase: "handler", pid: process.pid, call: handlerCalls,
      message: request.message, count: request.count };
    console.error("KELD_GH1081_HANDLER " + JSON.stringify(record));
    await post(record);
    return { message: JSON.stringify({ pid: process.pid, call: handlerCalls }), count: request.count };
  });
}
// The common shape: handler registered at module top, before the first lifecycle call.
if (handlerMode === "before-ready") registerEcho();
const bootstrap = process.env.KELD_APP_LINK;
if (!bootstrap) throw new Error("gh1081 app has no host-minted link");
// Hold before the first lifecycle call (which opens the app link), so the
// test can act while the host has no admitted generation.
await post({ phase: "recovery-app-boot", pid: process.pid, appLink: bootstrap });
const admission = await fetch(base + "/recovery-admit/" + process.pid);
if (!admission.ok || await admission.text() !== "admit") {
  throw new Error("invalid gh1081 admission command");
}
await app.whenReady();
await post({ phase: "app-ready", pid: process.pid, handlerMode,
  endpoint: bootstrap.slice(0, bootstrap.lastIndexOf("#")),
  executable: process.execPath, bunVersion: Bun.version });
if (handlerMode === "inside-ready") {
  // The Electron-style shape: handler registered inside whenReady. The test
  // holds this fetch, acts, then drops the connection to release registration.
  const hold = Array.from(crypto.getRandomValues(new Uint32Array(4)),
    value => value.toString(16).padStart(8, "0")).join("");
  const released = await fetch(base + "/recovery-pending/" + process.pid + "/" + hold).then(
    () => false,
    () => true,
  );
  if (!released) throw new Error("gh1081 registration hold was answered instead of released");
  registerEcho();
  await post({ phase: "handler-registered", pid: process.pid });
}
const quit = await fetch(base + "/recovery-quit/" + process.pid);
if (!quit.ok || await quit.text() !== "quit") throw new Error("invalid gh1081 quit command");
console.error("KELD_GH1081_QUIT_REQUEST calls=" + handlerCalls);
await app.quit();
process.exit(0);

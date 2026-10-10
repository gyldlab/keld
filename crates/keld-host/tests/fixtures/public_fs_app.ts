// Prepared application imports only the public API index in its supplied prelude.
type FsSampleOutcome =
  | { ok: true; bytes: number[] }
  | { ok: false; code: string; message: string };
type Control = { action: string; nonce: string };
const base = "http://127.0.0.1:__PORT__";
const target = __TARGET__;
const outside = __OUTSIDE__;
const content = new Uint8Array(__CONTENT__);
const automaticComponent = __AUTOMATIC_COMPONENT__;
let handlerCalls = 0;
function isControl(value: unknown): value is Control {
  if (typeof value !== "object" || value === null) return false;
  const record = value as Record<string, unknown>;
  return Object.keys(record).sort().join(",") === "action,nonce" &&
    typeof record.action === "string" && typeof record.nonce === "string" &&
    /^[0-9a-f]{32}$/.test(record.nonce);
}
async function performFsOperation(action: "roundtrip" | "deny"): Promise<FsSampleOutcome> {
  try {
    if (action === "roundtrip") {
      await fs.write(target, content);
      return { ok: true, bytes: [...await fs.read(target)] };
    }
    await fs.write(outside, new Uint8Array([7, 8, 9]));
    return { ok: true, bytes: [] };
  } catch (error: unknown) {
    if (!isCallError(error)) throw error;
    // Preserve the native code and full original text, including its fix.
    return { ok: false, code: error.code, message: error.message };
  }
}
channels.handle(echoChannel, async request => {
  handlerCalls += 1;
  const control: unknown = JSON.parse(request.message);
  if (!isControl(control) || request.count !== handlerCalls) {
    throw new Error("public FS sample invalid action/nonce/count");
  }
  console.error("KELD_KEL140_PUBLIC_HANDLER " + JSON.stringify({
    call: handlerCalls, action: control.action, nonce: control.nonce, pid: process.pid,
  }));
  let outcome: FsSampleOutcome;
  if (control.action === "roundtrip" && request.count === 1) {
    outcome = await performFsOperation("roundtrip");
  } else if (control.action === "deny" && request.count === 2) {
    outcome = await performFsOperation("deny");
  } else {
    // Unrelated handler failures retain resolveEchoCall's existing API001 rule.
    throw new Error("public FS sample unknown action");
  }
  const response = { message: JSON.stringify(outcome), count: request.count };
  if (echoChannel.encodeResponse(response).byteLength > 4096) {
    throw new Error("sample outcome exceeds renderer control budget");
  }
  console.error("KELD_KEL140_PUBLIC_OUTCOME " + JSON.stringify({
    call: handlerCalls, nonce: control.nonce, outcome,
  }));
  return response;
});
await app.whenReady();
if (process.env.KELD_DEV_LEASE !== undefined ||
    process.env.KELD_INTERNAL_MACOS_GUARDIAN_REGISTRATION !== undefined) {
  throw new Error("public FS application inherited host-only authority");
}
const descendant = Bun.spawn(["/usr/bin/tail", "-f", "/dev/null"], {
  stdin: "ignore", stdout: "ignore", stderr: "ignore",
});
const appLink = process.env.KELD_APP_LINK;
if (!appLink) throw new Error("public FS app has no host-minted link");
const ready = await fetch(base + "/observe", { method: "POST",
  headers: { "Content-Type": "text/plain" }, body: JSON.stringify({
    phase: "app-ready", pid: process.pid, descendantPid: descendant.pid,
    endpoint: appLink.slice(0, appLink.lastIndexOf("#")),
    executable: process.execPath, bunVersion: Bun.version,
  }),
});
if (!ready.ok) throw new Error("public FS ready observation refused");
if (automaticComponent) {
  // A public-main component: no renderer invoke, OS input or Echo forwarding.
  const outcomes: { action: "roundtrip" | "deny"; outcome: FsSampleOutcome }[] = [];
  for (const action of ["roundtrip", "deny"] as const) {
    outcomes.push({ action, outcome: await performFsOperation(action) });
  }
  const result = { phase: "app-result", pid: process.pid, componentOnly: true, outcomes };
  console.error("KELD_KEL140_PUBLIC_APP_COMPONENT " + JSON.stringify(result));
  const observed = await fetch(base + "/observe", { method: "POST",
    headers: { "Content-Type": "text/plain" }, body: JSON.stringify(result) });
  if (!observed.ok) throw new Error("public app component observation refused");
}
// The observer retains this stream without blocking other page observations.
const quit = await fetch(base + "/quit");
if (!quit.ok || await quit.text() !== "quit") throw new Error("invalid sample quit command");
console.error("KELD_KEL140_PUBLIC_QUIT_REQUEST calls=" + handlerCalls);
await app.quit();
process.exit(0);

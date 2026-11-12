/** Real LifecycleLink and WindowSource; only the WorkerLink socket seam is controlled. */
import assert from "node:assert/strict";
import { mock } from "bun:test";
import * as transport from "@keld/kipc";
import { encodeWindowEvent } from "./window-codec.ts";

const mode = process.argv[2];
const appliers = new Map<number, (payload: Uint8Array) => void>();
const listeners = new Map<number, (payload: Uint8Array) => void>();
let ended: ((error: transport.KeldCallError) => void) | undefined;
const terminal = transport.linkError(
  "KELD-IPC-022",
  "controlled terminal after drain",
);
const refusal = transport.linkError("KELD-IPC-003", "controlled Quit refusal");
let deadNotifications = 0;
let closeCalls = 0;
const deferred: Uint8Array[] = [];
const facts: string[] = [];
const ref = { id: 1, incarnation: 1n };
const payload = encodeWindowEvent({
  variant: "Created",
  window: ref,
  state: {
    bounds: { x: 0, y: 0, width: 800, height: 600 },
    focused: false,
    maximized: false,
    full_screen: false,
  },
  pending_close: null,
});
const worker = {
  stateFactCapacity: 16,
  setStateApplier(channel: number, callback: (payload: Uint8Array) => void) {
    appliers.set(channel, callback);
  },
  onEvent(channel: number, callback: (payload: Uint8Array) => void) {
    listeners.set(channel, callback);
  },
  setCallHandler() {},
  onEnd(callback: (error: transport.KeldCallError) => void) {
    ended = callback;
  },
  async call(_channel: number, request: Uint8Array) {
    if (request[0] === 1) {
      if (mode === "subscription-failure") {
        const created = payload;
        appliers.get(transport.WINDOW_CHANNEL)?.(created);
        deferred.push(created);
        return Uint8Array.of(2, 3);
      }
      return Uint8Array.of(1);
    }
    if (mode === "refusal" || mode === "timeout") throw refusal;
    return Uint8Array.of(0);
  },
  close() {
    closeCalls += 1;
    for (const retained of deferred.splice(0))
      listeners.get(transport.WINDOW_CHANNEL)?.(retained);
    // Actual WorkerLink emits this only after its retained records drain.
    ended?.(terminal);
  },
};
mock.module("@keld/kipc", () => ({
  ...transport,
  WorkerLink: { open: async () => worker },
}));
assert.equal(
  transport.nativeFacadeBootSelected,
  true,
  "launch-selected native mode",
);
// Exercise the actual LifecycleLink consumer independently of Electron's guard.
if (mode === "selection-delete") delete process.env.KELD_NATIVE_WINDOW_FACADE;
if (mode === "selection-zero") process.env.KELD_NATIVE_WINDOW_FACADE = "0";
const { LifecycleLink } = await import("./link.ts");
let connected: Awaited<ReturnType<typeof LifecycleLink.connect>> | undefined;
const connecting = LifecycleLink.connect("controlled-link", {
  onReady() {},
  onLastWindowClosed() {},
  onLinkDead() {
    deadNotifications += 1;
  },
  onConnected(session) {
    connected = session;
  },
  onWindowFact(fact) {
    assert.ok(
      connected?.windowSource?.get(fact.window),
      "earlier callback must precede Source disposal",
    );
    facts.push(fact.variant);
  },
});
if (mode === "subscription-failure")
  await assert.rejects(connecting, /KELD-CORE-045/);
else await connecting;
const session = connected!;
const source = session.windowSource;
assert.ok(source);
if (mode !== "subscription-failure") {
  appliers.get(transport.WINDOW_CHANNEL)?.(payload);
  listeners.get(transport.WINDOW_CHANNEL)?.(payload);
  assert.equal(source.all().length, 1);
  if (mode === "explicit-close") {
    const destroyed = encodeWindowEvent({ variant: "Destroyed", window: ref });
    appliers.get(transport.WINDOW_CHANNEL)?.(destroyed);
    deferred.push(destroyed);
    session.close();
    assert.deepEqual(facts, ["Created", "Destroyed"]);
    await assert.rejects(
      session.callWindow({ variant: "Destroy", window: ref }),
      /KELD-CORE-045/,
    );
  } else if (mode === "success" || mode.startsWith("selection-"))
    await session.quit();
  else await assert.rejects(session.quit(), (error) => error === refusal);
} else assert.deepEqual(facts, ["Created"]);
assert.equal(closeCalls, 1);
assert.equal(
  deadNotifications,
  0,
  "local Quit does not notify unexpected death",
);
for (const read of [() => source.all(), () => source.get(ref)]) {
  assert.throws(
    read,
    (error: unknown) =>
      error instanceof Error && error.message === terminal.message,
  );
}
ended?.(terminal);
assert.throws(() => source.all());
console.log(`PASS ${mode}`);

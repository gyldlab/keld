/** Real LifecycleLink with a controlled socket Worker: authenticated applier vs dispatch. */
import assert from "node:assert/strict";
import { mock } from "bun:test";
import * as transport from "@keld/kipc";

const appliers = new Map<number, (payload: Uint8Array) => void>();
const listeners = new Map<number, (payload: Uint8Array) => void>();
let admission = true;
const worker = {
  stateFactCapacity: 16,
  setStateApplier(channel: number, handler: (payload: Uint8Array) => void) {
    appliers.set(channel, handler);
  },
  onEvent(channel: number, handler: (payload: Uint8Array) => void) {
    listeners.set(channel, handler);
  },
  onEventAfterRingRelease(
    channel: number,
    dispatch: (payload: Uint8Array) => () => void,
  ) {
    // This lifecycle fixture never emits compat messages. The real ring-release
    // ordering is verified separately against the actual WorkerLink.
    assert.equal(channel, transport.COMPAT_CONTROL_CHANNEL);
    listeners.set(channel, (payload) => dispatch(payload)());
    return () => {
      listeners.delete(channel);
    };
  },
  setCallHandler() {},
  onEnd() {},
  async call(_channel: number, payload: Uint8Array) {
    if (payload[0] === 1) return Uint8Array.of(1);
    throw new Error("no outbound window calls in this lifecycle contract");
  },
  close() {
    admission = false;
  },
  get callAdmissionOpen() {
    return admission;
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
const { LifecycleLink } = await import("./link.ts");

const seen: string[] = [];
let dispatchedSession: unknown;
const session = await LifecycleLink.connect("controlled-link", {
  onReady() {
    seen.push("ready");
  },
  onLastWindowClosed() {
    seen.push("last-window");
  },
  onQuitRequested(current) {
    dispatchedSession = current;
    seen.push("quit-requested");
  },
  onLinkDead() {
    throw new Error("unexpected link death");
  },
});
const channel = transport.LIFECYCLE_CHANNEL;
const apply = (bytes: Uint8Array) => appliers.get(channel)?.(bytes);
const dispatch = (bytes: Uint8Array) => listeners.get(channel)?.(bytes);
apply(Uint8Array.of(2));
dispatch(Uint8Array.of(2));
assert.deepEqual(seen, [], "pre-Ready intent cannot invoke an endpoint");
apply(Uint8Array.of(0));
dispatch(Uint8Array.of(0));
apply(Uint8Array.of(2));
dispatch(Uint8Array.of(2));
assert.deepEqual(seen, ["ready", "quit-requested"]);
assert.equal(
  dispatchedSession,
  session,
  "dispatch retains the exact admitted link object",
);
apply(Uint8Array.of(1));
dispatch(Uint8Array.of(1));
assert.deepEqual(seen, ["ready", "quit-requested", "last-window"]);
for (const malformed of [
  Uint8Array.of(3),
  Uint8Array.of(2, 0),
  Uint8Array.of(0x82, 0),
])
  assert.throws(() => apply(malformed), /KELD-IPC-003/);
admission = false;
apply(Uint8Array.of(2));
dispatch(Uint8Array.of(2));
assert.equal(
  seen.filter((v) => v === "quit-requested").length,
  1,
  "terminal-admission buffered intent is not dispatched",
);
session.close();
apply(Uint8Array.of(2));
dispatch(Uint8Array.of(2));
assert.equal(
  seen.filter((v) => v === "quit-requested").length,
  1,
  "closed predecessor cannot gain a later callback",
);
console.log("PASS scoped-lifecycle-quit-requested-current-link");

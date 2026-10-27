/** Real LifecycleLink with a controlled socket Worker: authenticated applier vs dispatch. */
import assert from "node:assert/strict";
import { mock } from "bun:test";
import * as transport from "../../kipc/src/transport.ts";

const appliers = new Map<number, (payload: Uint8Array) => void>();
const listeners = new Map<number, (payload: Uint8Array) => void>();
const worker = {
  stateFactCapacity: 16,
  setStateApplier(channel: number, handler: (payload: Uint8Array) => void) {
    appliers.set(channel, handler);
  },
  onEvent(channel: number, handler: (payload: Uint8Array) => void) {
    listeners.set(channel, handler);
  },
  setCallHandler() {},
  onEnd() {},
  async call(_channel: number, payload: Uint8Array) {
    if (payload[0] === 1) return Uint8Array.of(1);
    throw new Error("no outbound window calls in this lifecycle contract");
  },
  close() {},
};
mock.module("../../kipc/src/transport.ts", () => ({
  ...transport,
  WorkerLink: { open: async () => worker },
}));
process.env.KELD_NATIVE_WINDOW_FACADE = "1";
const { LifecycleLink } = await import("./link.ts");
const emitted: string[] = [];
const session = await LifecycleLink.connect("controlled-link", {
  onReady() {},
  onLastWindowClosed() {
    emitted.push("all-closed");
  },
  onLinkDead() {
    throw new Error("no unexpected link death");
  },
});
const apply = (channel: number) => appliers.get(channel)?.(Uint8Array.of(1));
const dispatch = (channel: number) =>
  listeners.get(channel)?.(Uint8Array.of(1));
const ch = transport.LIFECYCLE_CHANNEL;

assert.equal(
  session.consumeAppliedLastWindowClosed(),
  false,
  "no fabricated milestone",
);
apply(ch); // first LastWindowClosed reached ring state applier before Destroy REPLY
assert.equal(session.consumeAppliedLastWindowClosed(), true);
emitted.push("all-closed"); // api/app.ts invokes the caller only after native `closed`
assert.equal(session.consumeAppliedLastWindowClosed(), false);
dispatch(ch); // delayed event dispatch must not repeat it
assert.deepEqual(emitted, ["all-closed"]);

// A previous ordinary event was dispatched; an additional applied event is
// consumed only once, not as the old ordinal for a second time.
apply(ch);
dispatch(ch); // normal dispatch owns second event
assert.deepEqual(emitted, ["all-closed", "all-closed"]);
apply(ch); // new third event, parked while app performs sync Destroy
assert.equal(session.consumeAppliedLastWindowClosed(), true);
emitted.push("all-closed");
assert.equal(session.consumeAppliedLastWindowClosed(), false);
dispatch(ch); // third dispatch is skipped
assert.deepEqual(emitted, ["all-closed", "all-closed", "all-closed"]);

apply(ch); // next last-window event with no parked Destroy
dispatch(ch);
assert.deepEqual(emitted, [
  "all-closed",
  "all-closed",
  "all-closed",
  "all-closed",
]);
assert.equal(session.consumeAppliedLastWindowClosed(), false);

// Multiple already-applied Host milestones are not multiple notifications
// from the single synchronous Destroy. The cumulative handled watermark
// suppresses each old EVENT on its eventual separate ring dispatch.
apply(ch);
apply(ch);
assert.equal(session.consumeAppliedLastWindowClosed(), true);
emitted.push("all-closed");
assert.equal(session.consumeAppliedLastWindowClosed(), false);
dispatch(ch);
dispatch(ch);
assert.equal(
  emitted.length,
  5,
  "one app callback despite two pending milestones",
);

// A newly applied real Host event still has its own ordinary dispatch.
apply(ch);
dispatch(ch);
assert.equal(emitted.length, 6);
assert.equal(session.consumeAppliedLastWindowClosed(), false);
console.log("PASS correlated-last-window-apply-before-destroy-reply");

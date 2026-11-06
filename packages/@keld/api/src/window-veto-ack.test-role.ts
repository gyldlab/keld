/** Real api/app.ts + authenticated Host-response seam, isolated from Bun mocks. */
import assert from "node:assert/strict";
import { mock } from "bun:test";
import {
  encodeWindowEvent,
  type WindowEvent,
  type WindowRef,
  type WindowRequest,
  type WindowResponse,
} from "./window-codec.ts";
import { WindowSource } from "./window-source.ts";

let dispatch: ((fact: WindowEvent) => void) | undefined;
let onReady: (() => void) | undefined;
let postpone = false;
let delayedReply: ((result: WindowResponse) => void) | undefined;
let nextReply: WindowResponse = { variant: "ReplyApplied" };
const calls: WindowRequest[] = [];
const source = new WindowSource((fact) => dispatch?.(fact));

mock.module("./link.ts", () => ({
  LifecycleLink: {
    connect: async (
      _address: string,
      handlers: {
        onReady(): void;
        onConnected(session: unknown): void;
        onWindowFact(fact: WindowEvent): void;
      },
    ) => {
      dispatch = handlers.onWindowFact;
      onReady = handlers.onReady;
      const session = {
        windowSource: source,
        close() {},
        quit: async () => {},
        callWindow(request: WindowRequest): Promise<WindowResponse> {
          calls.push(request);
          if (!postpone) return Promise.resolve(nextReply);
          return new Promise((resolve) => {
            delayedReply = resolve;
          });
        },
        callWindowBlocking(request: WindowRequest): WindowResponse {
          calls.push(request);
          return { variant: "ReplyApplied" };
        },
      };
      handlers.onConnected(session);
      handlers.onReady();
      return session;
    },
  },
}));

process.env.KELD_APP_LINK = "test-only-host-endpoint#not-a-real-auth-token";
const {
  app,
  getWindowSnapshots,
  getDispatchedPendingWindowCloses,
  invokeWindow,
  invokeWindowBlocking,
} = await import("./app.ts");
await app.whenReady();
assert.equal(app.isReady(), true);
assert.ok(onReady && dispatch);

const ref = Object.freeze({ id: 7, incarnation: 300n });
const state = {
  bounds: { x: 12, y: 34, width: 640, height: 480 },
  focused: false,
  maximized: false,
  full_screen: false,
};
function hostFact(fact: WindowEvent): void {
  const decoded = source.decode(encodeWindowEvent(fact));
  source.apply(decoded);
  source.dispatch(decoded);
}
function pending(): [bigint | null, bigint | null] {
  return [
    getWindowSnapshots()[0]?.pending_close ?? null,
    getDispatchedPendingWindowCloses()[0]?.closeSeq ?? null,
  ];
}
function veto(seq: bigint): WindowRequest {
  return {
    variant: "CloseReply",
    window: ref,
    close_seq: seq,
    verdict: "Veto",
  };
}
hostFact({ variant: "Created", window: ref, state, pending_close: 7n });
assert.deepEqual(pending(), [7n, 7n]);
assert.deepEqual(await invokeWindow(veto(7n)), { variant: "ReplyApplied" });
assert.deepEqual(
  pending(),
  [null, null],
  "Host-acknowledged Veto must clear both mirrors",
);

// The already-accepted old Veto response can arrive after the Host published
// a newer CloseRequested; never clear the new seq from either mirror.
hostFact({ variant: "CloseRequested", window: ref, close_seq: 8n });
postpone = true;
const oldAck = invokeWindow(veto(8n));
assert.ok(delayedReply);
hostFact({ variant: "CloseRequested", window: ref, close_seq: 9n });
delayedReply?.({ variant: "ReplyApplied" });
await oldAck;
assert.deepEqual(pending(), [9n, 9n], "old Veto ack cannot clear newer seq");

postpone = false;
nextReply = { variant: "ReplyStale" };
assert.deepEqual(await invokeWindow(veto(9n)), { variant: "ReplyStale" });
assert.deepEqual(
  pending(),
  [9n, 9n],
  "non-applied Host reply cannot clear a pending close",
);
nextReply = { variant: "ReplyApplied" };
assert.deepEqual(await invokeWindow(veto(9n)), { variant: "ReplyApplied" });
assert.deepEqual(pending(), [null, null]);
assert.deepEqual(
  calls
    .filter((call) => call.variant === "CloseReply")
    .map((call) => call.close_seq),
  [7n, 8n, 9n, 9n],
);

// The transport may apply a CloseRequested before its user-dispatch cursor
// reaches the fact. If its correlated Host Veto reply arrives first, the
// later delayed fact must NOT resurrect a settled close in the receipt map.
const beforeDispatch = source.decode(
  encodeWindowEvent({
    variant: "CloseRequested",
    window: ref,
    close_seq: 11n,
  }),
);
source.apply(beforeDispatch);
assert.deepEqual(await invokeWindow(veto(11n)), { variant: "ReplyApplied" });
source.dispatch(beforeDispatch);
assert.deepEqual(
  pending(),
  [null, null],
  "ack-before-dispatch must not revive old Veto",
);

// The request is serialized synchronously before awaiting its Host reply.
// Mutating the caller-owned object while that reply is parked must not redirect
// the Host ack's clearing effect to a different id/sequence (or clear seq+1).
const delayed12 = source.decode(
  encodeWindowEvent({
    variant: "CloseRequested",
    window: ref,
    close_seq: 12n,
  }),
);
source.apply(delayed12);
const mutableRef: WindowRef = { id: ref.id, incarnation: ref.incarnation };
const mutableRequest: WindowRequest = {
  variant: "CloseReply",
  window: mutableRef,
  close_seq: 12n,
  verdict: "Veto",
};
postpone = true;
const ack12 = invokeWindow(mutableRequest);
assert.ok(delayedReply);
mutableRef.id = 999;
if (mutableRequest.variant === "CloseReply") {
  mutableRequest.close_seq = 700n;
  mutableRequest.verdict = "Allow";
}
const delayed13 = source.decode(
  encodeWindowEvent({
    variant: "CloseRequested",
    window: ref,
    close_seq: 13n,
  }),
);
source.apply(delayed13);
delayedReply?.({ variant: "ReplyApplied" });
await ack12;
const copiedCall = calls.at(-1);
assert.deepEqual(
  copiedCall,
  veto(12n),
  "the sent and acknowledged pair must be immutable",
);
source.dispatch(delayed12);
assert.deepEqual(
  getDispatchedPendingWindowCloses(),
  [],
  "old ack advances the receipt watermark even with a newer pre-applied pending seq",
);
source.dispatch(delayed13);
assert.deepEqual(
  pending(),
  [13n, 13n],
  "old Veto must not clear or resurrect next seq",
);

postpone = false;
assert.deepEqual(await invokeWindow(veto(13n)), { variant: "ReplyApplied" });
assert.deepEqual(pending(), [null, null]);

// Attacker must not redirect a parked response to another *existing* window
// with a matching pending sequence (an absent id=999 alone is not adequate).
const another = { id: 8, incarnation: 301n };
hostFact({ variant: "Created", window: another, state, pending_close: 14n });
hostFact({ variant: "CloseRequested", window: ref, close_seq: 14n });
postpone = true;
const mutablePair: WindowRef = { id: ref.id, incarnation: ref.incarnation };
const sent: WindowRequest = {
  variant: "CloseReply",
  window: mutablePair,
  close_seq: 14n,
  verdict: "Veto",
};
const ack14 = invokeWindow(sent);
mutablePair.id = another.id;
mutablePair.incarnation = another.incarnation;
delayedReply?.({ variant: "ReplyApplied" });
await ack14;
assert.equal(source.get(ref)?.pending_close, null);
assert.equal(
  source.get(another)?.pending_close,
  14n,
  "A's ack must not clear window B",
);
assert.deepEqual(
  getDispatchedPendingWindowCloses().map(({ window, closeSeq }) => [
    window.id,
    closeSeq,
  ]),
  [[another.id, 14n]],
);
postpone = false;
assert.deepEqual(
  invokeWindowBlocking({
    variant: "CloseReply",
    window: another,
    close_seq: 14n,
    verdict: "Veto",
  }),
  { variant: "ReplyApplied" },
);
assert.equal(
  source.get(another)?.pending_close,
  null,
  "blocking Host ack also clears exact B",
);
assert.deepEqual(getDispatchedPendingWindowCloses(), []);

hostFact({ variant: "CloseRequested", window: ref, close_seq: 15n });
assert.deepEqual(invokeWindowBlocking(veto(15n)), { variant: "ReplyApplied" });
assert.equal(source.get(ref)?.pending_close, null);

// Import the *real* Electron facade only after the Host has already accepted
// Veto. No obsolete receipt can manufacture a second CloseReply merely
// because the module's static event subscription did not exist earlier.
const originalCallCount = calls.length;
const { BrowserWindow } = await import("../../electron/src/browser-window.ts");
const window = BrowserWindow.getAllWindows()[0];
assert.equal(window.id, ref.id);
await new Promise<void>((resolve) => setImmediate(resolve));
await new Promise<void>((resolve) => setImmediate(resolve));
assert.equal(
  calls.length,
  originalCallCount,
  "late import must not send phantom Veto",
);

let closeEvents = 0;
window.on("close", (event: { preventDefault(): void }) => {
  closeEvents += 1;
  event.preventDefault();
});
hostFact({ variant: "CloseRequested", window: ref, close_seq: 16n });
await new Promise<void>((resolve) => setImmediate(resolve));
assert.equal(
  closeEvents,
  1,
  "a new native close must still reach the restored listener",
);
assert.deepEqual(pending(), [null, null]);
assert.equal(calls.length, originalCallCount + 1);
assert.deepEqual(calls.at(-1), veto(16n));

// The same authenticated ReplyApplied on an Allow moves Core to Closing, not
// back to Open. Its sequence is no longer pending, although the window remains
// live until a separate physical Destroyed fact. Do not misfire a phantom
// close from a late-dispatched CloseRequested during this release interval.
const delayedAllow = source.decode(
  encodeWindowEvent({
    variant: "CloseRequested",
    window: ref,
    close_seq: 17n,
  }),
);
source.apply(delayedAllow);
const beforeAllowCalls = calls.length;
assert.deepEqual(
  await invokeWindow({
    variant: "CloseReply",
    window: ref,
    close_seq: 17n,
    verdict: "Allow",
  }),
  { variant: "ReplyApplied" },
);
assert.deepEqual(pending(), [null, null]);
assert.equal(window.isDestroyed(), false, "Allow ack is not physical release");
source.dispatch(delayedAllow);
await new Promise<void>((resolve) => setImmediate(resolve));
assert.equal(closeEvents, 1, "settled Allow cannot replay a phantom close");
assert.equal(calls.length, beforeAllowCalls + 1);
hostFact({ variant: "Destroyed", window: ref });
assert.equal(
  window.isDestroyed(),
  true,
  "only Host Destroyed tombstones window",
);
assert.deepEqual(
  BrowserWindow.getAllWindows().map((item) => item.id),
  [another.id],
  "Destroying A cannot tombstone the independently live window B",
);
console.log("PASS host-veto-ack-clears-only-exact-sequence");

// Controlled response seam: ReplyApplied is settled first, then Source fails
// before the API continuation. The real socket counterpart is worker_link.rs.
postpone = true;
const earlierAnswer = invokeWindow({
  variant: "CloseReply",
  window: another,
  close_seq: 14n,
  verdict: "Veto",
});
delayedReply?.({ variant: "ReplyApplied" });
assert.throws(
  () =>
    source.apply({
      variant: "Focus",
      window: { ...another, incarnation: 999n },
      focused: true,
    }),
  /KELD-IPC-005/,
);
assert.deepEqual(await earlierAnswer, { variant: "ReplyApplied" });
assert.throws(getWindowSnapshots, /KELD-IPC-005/);
console.log("PASS earlier-answer-independent-of-later-source-failure");

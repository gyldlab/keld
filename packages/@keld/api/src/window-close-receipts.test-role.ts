import assert from "node:assert/strict";
import {
  WindowCloseReceipts,
  type DeliveredWindowClose,
} from "./window-close-receipts.ts";

const mode = process.argv[2];
const first = { id: 7, incarnation: 300n };
const second = { id: 9, incarnation: 400n };
const stale = { ...first, incarnation: 301n };
const state = {
  bounds: { x: 0, y: 0, width: 640, height: 480 },
  focused: false,
  maximized: false,
  full_screen: false,
};
const define = Object.defineProperty;
const descriptor = Object.getOwnPropertyDescriptor;
const mapIterator = Object.getPrototypeOf(new Map().values());
const snapshots: Record<string, readonly DeliveredWindowClose[]> = {};
let hits = 0;
let failure: unknown;
let target: object = Map.prototype;
let key: PropertyKey = mode ?? "";
if (mode === "constructor") {
  target = globalThis;
  key = "Map";
} else if (mode === "freeze") {
  target = Object;
  key = "freeze";
} else if (mode === "next") {
  target = mapIterator;
  key = "next";
} else if (mode === "push") {
  target = Array.prototype;
  key = "push";
} else if (mode === "array-iterator") {
  target = Array.prototype;
  key = Symbol.iterator;
} else if (mode === "numeric-setter") {
  target = Array.prototype;
  key = "0";
} else
  assert.ok(
    ["control", "get", "set", "delete", "values"].includes(mode ?? ""),
    "unknown scenario",
  );
const original = descriptor(target, key);
function poison(): never {
  hits += 1;
  throw new Error(`application hook ${mode}`);
}
try {
  if (mode !== "control")
    define(
      target,
      key,
      mode === "numeric-setter"
        ? { configurable: true, set: poison }
        : { configurable: true, writable: true, value: poison },
    );
  const receipts = new WindowCloseReceipts();
  // A Host ack can precede dispatch: neither Created nor old CloseRequested may resurrect it.
  receipts.acceptHostCloseReply(first, 7n);
  receipts.observe({
    variant: "Created",
    window: first,
    state,
    pending_close: 7n,
  });
  receipts.observe({ variant: "CloseRequested", window: first, close_seq: 7n });
  snapshots.acked = receipts.pending();
  receipts.observe({ variant: "CloseRequested", window: first, close_seq: 8n });
  snapshots.newer = receipts.pending();
  receipts.acceptHostCloseReply(first, 7n);
  receipts.acceptHostCloseReply(stale, 99n);
  receipts.observe({
    variant: "CloseRequested",
    window: stale,
    close_seq: 99n,
  });
  receipts.observe({ variant: "Destroyed", window: stale });
  snapshots.oldAckAndWrongPair = receipts.pending();
  receipts.observe({
    variant: "Created",
    window: second,
    state,
    pending_close: 21n,
  });
  snapshots.lateImport = receipts.pending();
  receipts.acceptHostCloseReply(first, 8n);
  snapshots.settled = receipts.pending();
  receipts.observe({ variant: "CloseRequested", window: first, close_seq: 8n });
  snapshots.delayed = receipts.pending();
  receipts.observe({ variant: "CloseRequested", window: first, close_seq: 9n });
  snapshots.reopened = receipts.pending();
  receipts.observe({ variant: "Destroyed", window: first });
  snapshots.destroyed = receipts.pending();
  receipts.observe({ variant: "Destroyed", window: second });
  snapshots.empty = receipts.pending();
} catch (error) {
  failure = error;
} finally {
  if (mode !== "control") {
    if (original) define(target, key, original);
    else Reflect.deleteProperty(target, key);
  }
}
assert.equal(
  failure,
  undefined,
  `receipt operation reached ${mode}: ${String(failure)}`,
);
assert.equal(hits, 0, "private ledger/copy must never reach application hooks");
const one = { window: first, closeSeq: 8n };
const two = { window: second, closeSeq: 21n };
assert.deepEqual(snapshots, {
  acked: [],
  newer: [one],
  oldAckAndWrongPair: [one],
  lateImport: [one, two],
  settled: [two],
  delayed: [two],
  reopened: [{ window: first, closeSeq: 9n }, two],
  destroyed: [two],
  empty: [],
});
for (const snapshot of Object.values(snapshots)) {
  assert.ok(Object.isFrozen(snapshot));
  for (const receipt of snapshot) {
    assert.ok(Object.isFrozen(receipt));
    assert.ok(Object.isFrozen(receipt.window));
  }
}
// Caller-held earlier snapshots still contain their exact sequence after later ack/destruction.
assert.equal(snapshots.newer?.[0]?.closeSeq, 8n);
console.log(`PASS receipt-integrity ${mode}`);

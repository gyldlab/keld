/** Child role for the actual named-pipe/WorkerLink pending-slot fixture. */
import assert from "node:assert/strict";
import {
  ECHO_CHANNEL,
  LIFECYCLE_CHANNEL,
  WorkerLink,
  withIoDeadline,
} from "@keld/kipc";
import { attachMirror } from "./mirror.ts";

const link = await WorkerLink.open({
  link: process.env.KELD_APP_LINK!,
  receive: { eventChannels: [LIFECYCLE_CHANNEL], callReceivers: [] },
  ringRecords: 3,
});
type Fact = { id: number; sender?: number };
const app: number[] = [];
const nested: number[][] = [];
let released = 0;
let applyScanRefusals = 0;
let terminal = false;
let firstDone!: () => void;
const firstDispatched = new Promise<void>((resolve) => {
  firstDone = resolve;
});
let ended!: () => void;
const onEnd = new Promise<void>((resolve) => {
  ended = resolve;
});
link.onEnd(() => ended());
const pending = attachMirror<Fact>(
  link,
  {
    channel: LIFECYCLE_CHANNEL,
    decode(bytes) {
      return { id: bytes[0]! };
    },
    apply(fact) {
      if (fact.id === 1) {
        let refusal: unknown;
        try {
          pending.visitPending(() => {});
        } catch (error) {
          refusal = error;
        }
        if (refusal === undefined) {
          console.error("SCAN_DURING_APPLY_ACCEPTED");
          throw new Error("scan admitted an in-flight apply");
        }
        assert.match(String(refusal), /KELD-IPC-005/);
        applyScanRefusals += 1;
      }
    },
    dispatch(fact) {
      if (terminal) return;
      assert.equal(fact.sender, fact.id);
      app.push(fact.id);
      if (fact.id === 1) {
        const rest: number[] = [];
        pending.visitPending((item) => rest.push(item.id));
        nested.push(rest);
      }
      if (fact.id === 2) firstDone();
    },
  },
  () => {
    released += 1;
  },
);

link.callBlocking(ECHO_CHANNEL, new TextEncoder().encode("scan-first"), 30_000);
assert.equal(applyScanRefusals, 1);
const initial: number[] = [];
pending.visitPending((fact) => {
  initial.push(fact.id);
  fact.sender = fact.id;
});
assert.deepEqual(initial, [1, 2]);
assert.throws(
  () => pending.visitPending(() => pending.visitPending(() => {})),
  /KELD-IPC-005/,
);
const retry: number[] = [];
pending.visitPending((fact) => retry.push(fact.id));
assert.deepEqual(retry, [1, 2]);
const originalApply = Reflect.apply;
const originalIterator = Array.prototype[Symbol.iterator];
const originalForEach = Map.prototype.forEach;
let poisonedVisits = 0;
try {
  Reflect.apply = (() => {
    throw new Error("late Reflect.apply");
  }) as typeof Reflect.apply;
  Array.prototype[Symbol.iterator] = (() => {
    throw new Error("late Array iterator");
  }) as (typeof Array.prototype)[typeof Symbol.iterator];
  Map.prototype.forEach = (() => {
    throw new Error("late Map iterator");
  }) as typeof Map.prototype.forEach;
  pending.visitPending(() => {
    poisonedVisits += 1;
  });
} finally {
  Reflect.apply = originalApply;
  Array.prototype[Symbol.iterator] = originalIterator;
  Map.prototype.forEach = originalForEach;
}
assert.equal(poisonedVisits, 2);
await withIoDeadline(firstDispatched, 10_000);
assert.deepEqual(app, [1, 2]);
assert.deepEqual(nested, [[2]]);
const empty: number[] = [];
pending.visitPending((fact) => empty.push(fact.id));
assert.deepEqual(empty, []);
assert.equal(released, 2);

link.callBlocking(ECHO_CHANNEL, new TextEncoder().encode("scan-wrap"), 30_000);
const wrap: number[] = [];
let firstPartial: Fact | undefined;
let secondPartial: Fact | undefined;
const closeDuringScan = process.argv[2] === "close-during-scan";
try {
  pending.visitPending((fact) => {
    wrap.push(fact.id);
    if (fact.id === 4) firstPartial = fact;
    if (fact.id === 5) {
      secondPartial = fact;
      if (closeDuringScan) {
        terminal = true;
        link.close();
        return;
      }
      throw new Error("forced sender capture failure");
    }
    fact.sender = fact.id;
  });
  throw new Error("expected scan failure");
} catch (error) {
  if (closeDuringScan) {
    assert.match(
      (error as Error).message,
      /mirror link ended during pending scan/,
    );
  } else {
    assert.equal((error as Error).message, "forced sender capture failure");
  }
  terminal = true; // CompatSource owns this before it closes the link.
  link.close();
}
assert.deepEqual(wrap, [4, 5]);
assert.equal(firstPartial?.sender, 4);
assert.equal(secondPartial?.sender, undefined);
assert.throws(() => pending.visitPending(() => {}), /KELD-IPC-005/);
await withIoDeadline(onEnd, 10_000);
assert.deepEqual(app, [1, 2]);
assert.equal(released, 4);
console.log(
  "PASS pending-facts: FIFO, taken exclusion, recursion, failure, terminal, physical releases",
);

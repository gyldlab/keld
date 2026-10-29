/** Independent actual app.ts Ready-publication sequence (only socket is mocked). */
import assert from "node:assert/strict";
import { mock } from "bun:test";
import type { WindowEvent } from "./window-codec.ts";

type Hooks = { onReady(): void; onWindowFact?(fact: WindowEvent): void };
let hooks: Hooks | undefined;
mock.module("./link.ts", () => ({
  LifecycleLink: {
    connect: async (_address: string, handlers: Hooks) => {
      hooks = handlers;
      return { close() {}, quit: async () => {} };
    },
  },
}));
process.env.KELD_APP_LINK = "test-only-valid-address#not-a-trust-claim";
const { app, onWindowFactApplied, getDispatchedPendingWindowCloses } =
  await import("./app.ts");
const observed: string[] = [];
onWindowFactApplied((fact) =>
  observed.push(`fact:${fact.variant}:${app.isReady()}`),
);
const ready = app.whenReady().then(() => observed.push("application:ready"));
assert.ok(
  hooks,
  "real app.whenReady installed callbacks on the single mocked link",
);
const ref = { id: 1, incarnation: 1n };
const state = {
  bounds: { x: 37, y: 91, width: 800, height: 600 },
  focused: false,
  maximized: false,
  full_screen: false,
};
hooks.onWindowFact?.({
  variant: "Created",
  window: ref,
  state,
  pending_close: 7n,
});
hooks.onReady();
assert.equal(
  app.isReady(),
  false,
  "raw Host Ready cannot publish midway through ring drain",
);
hooks.onWindowFact?.({
  variant: "Bounds",
  window: ref,
  bounds: { ...state.bounds, width: 777 },
});
assert.equal(
  app.isReady(),
  false,
  "following same-drain facts remain pre-publicReady",
);
assert.deepEqual(
  getDispatchedPendingWindowCloses(),
  [],
  "unready mirror never grants public access",
);
await ready;
assert.equal(app.isReady(), true);
assert.deepEqual(observed, [
  "fact:Created:false",
  "fact:Bounds:false",
  "application:ready",
]);
assert.deepEqual(getDispatchedPendingWindowCloses(), [
  { window: ref, closeSeq: 7n },
]);

// P1 real-API negative: a user listener and the user-replaceable logger can
// both throw. The canonical link's next valid fact must still reach another
// registered callback instead of escaping into WorkerLink's fatal async path.
let laterFacts = 0;
const unsubscribeThrower = onWindowFactApplied(() => {
  throw new Error("untrusted window callback");
});
const unsubscribeWitness = onWindowFactApplied(() => {
  laterFacts += 1;
});
const previousLogger = console.error;
console.error = () => {
  throw new Error("untrusted diagnostic logger");
};
try {
  hooks.onWindowFact?.({ variant: "Focus", window: ref, focused: true });
  hooks.onWindowFact?.({ variant: "Focus", window: ref, focused: false });
} finally {
  console.error = previousLogger;
  unsubscribeThrower();
  unsubscribeWitness();
}
assert.equal(
  laterFacts,
  2,
  "both valid native facts must survive the throwing pair",
);
console.log("PASS actual-api-ready-microtask");

/** Observe the real app dispatch while controlling its single link boundary. */
import assert from "node:assert/strict";
import { mock } from "bun:test";
import type { WindowCarrierEvent } from "./window-codec.ts";

type Finish = Extract<WindowCarrierEvent, { variant: "NavigationFinished" }>;
type MockSession = {
  windowAdmissionOpen: boolean;
  close(): void;
  quit(): Promise<void>;
};
type Hooks = {
  onConnected(session: MockSession): void;
  onReady(): void;
  onOriginalFinish(fact: Finish): void;
};
let hooks: Hooks | undefined;
const session = { windowAdmissionOpen: true, close() {}, quit: async () => {} };
mock.module("./link.ts", () => ({
  LifecycleLink: {
    connect: async (_address: string, handlers: Hooks) => {
      hooks = handlers;
      handlers.onConnected(session);
      return session;
    },
  },
}));
process.env.KELD_APP_LINK = "test-only#not-a-trust-claim";
const { app, onWindowFactApplied } = await import("./app.ts");
const ready = app.whenReady();
assert.ok(hooks);
const fact: Finish = {
  variant: "NavigationFinished",
  window: { id: 1, incarnation: 1n },
  document_generation: 1,
};
const calls: string[] = [];
onWindowFactApplied(() => {
  calls.push("first");
  session.windowAdmissionOpen = false;
});
onWindowFactApplied(() => {
  calls.push("second");
});
hooks.onOriginalFinish(fact);
assert.deepEqual(calls, [], "no user dispatch before Host Ready");
hooks.onReady();
await ready;
hooks.onOriginalFinish(fact);
assert.deepEqual(
  calls,
  ["first"],
  "retired admission must stop later listeners",
);
hooks.onOriginalFinish(fact);
assert.deepEqual(
  calls,
  ["first"],
  "dead admission must not publish another finish",
);
console.log("PASS original-finish-app-admission");

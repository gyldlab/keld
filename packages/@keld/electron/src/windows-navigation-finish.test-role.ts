/** Isolated facade fixture. Host facts are controlled; native effects are not claimed. */
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { mock } from "bun:test";
import type {
  WindowCarrierEvent,
  WindowRef,
  WindowRequest,
} from "../../api/src/window-codec.ts";

type Finish = Extract<
  WindowCarrierEvent,
  { variant: "NavigationFinished" | "NavigationFinishedUnitPinchV1" }
>;
type Snapshot = {
  window: WindowRef;
  state: {
    bounds: { x: number; y: number; width: number; height: number };
    focused: boolean;
    maximized: boolean;
    full_screen: boolean;
  };
  pending_close: null;
  host_destroyed: boolean;
  document: {
    generation: number;
    current: boolean;
    finished: boolean;
    native_url: string;
  };
};
type UnitContents = EventEmitter & {
  setVisualZoomLevelLimits(min: number, max: number): Promise<void>;
};
const snapshots = new Map<number, Snapshot>();
let listener: (fact: WindowCarrierEvent) => void = () => {};
let currentFinish: Finish | undefined;
let nextId = 1;
let selected = true;
const current = (fact: Finish): boolean => {
  const snapshot = snapshots.get(fact.window.id);
  return (
    currentFinish === fact &&
    snapshot?.window.incarnation === fact.window.incarnation &&
    snapshot.host_destroyed === false &&
    snapshot.document.current &&
    snapshot.document.finished &&
    snapshot.document.generation === fact.document_generation
  );
};
mock.module("../../api/src/app.ts", () => ({
  app: { isReady: () => true, whenReady: async () => {} },
  flushAppliedLastWindowClosedAfterDestroy: () => {},
  getDispatchedPendingWindowCloses: () => [],
  getWindowSnapshots: () => [...snapshots.values()],
  getWindowSnapshot: (ref: WindowRef) => {
    const s = snapshots.get(ref.id);
    return s?.window.incarnation === ref.incarnation ? s : undefined;
  },
  isWindowsOriginalUnitPinchSelected: () => selected,
  isCurrentUnitPinchFinish: (fact: Finish) =>
    fact.variant === "NavigationFinishedUnitPinchV1" && current(fact),
  onWindowFactApplied: (callback: (fact: WindowCarrierEvent) => void) => {
    listener = callback;
    return () => {};
  },
  invokeWindow: async () => ({ variant: "ReplyApplied" }),
  invokeWindowBlocking: (request: WindowRequest) => {
    if (request.variant !== "Create") throw new Error("unexpected host call");
    const window = { id: nextId++, incarnation: 1n };
    snapshots.set(window.id, {
      window,
      state: {
        bounds: { x: 0, y: 0, width: 800, height: 600 },
        focused: false,
        maximized: false,
        full_screen: false,
      },
      pending_close: null,
      host_destroyed: false,
      document: {
        generation: 1,
        current: true,
        finished: true,
        native_url: "file:///test",
      },
    });
    return { variant: "Created", window };
  },
}));
mock.module("./app.ts", () => ({ emitNativeAppEvent: () => {} }));
const { BrowserWindow } = await import("./browser-window.ts");
const first = new BrowserWindow();
const second = new BrowserWindow();
selected = false;
const ordinary = new BrowserWindow();
assert.equal("setVisualZoomLevelLimits" in ordinary.webContents, false);
selected = true;
const one = first.webContents as unknown as UnitContents;
const two = second.webContents as unknown as UnitContents;
const ref = snapshots.get(first.id)!.window;
const make = (variant: Finish["variant"], generation = 1): Finish => ({
  variant,
  window: ref,
  document_generation: generation,
});
const dispatch = (fact: Finish): void => {
  const previous = currentFinish;
  currentFinish = fact;
  try {
    listener(fact);
  } finally {
    currentFinish = previous;
  }
};

assert.equal(typeof one.setVisualZoomLevelLimits, "function");
assert.throws(() => one.setVisualZoomLevelLimits(1, 1), /KELD-COMPAT-013/);
let normal = 0;
one.once("did-finish-load", () => {
  normal++;
  assert.throws(() => one.setVisualZoomLevelLimits(1, 1), /KELD-COMPAT-013/);
});
dispatch(make("NavigationFinished"));
assert.equal(normal, 1);

let qualified = 0;
let nested = false;
one.once("did-finish-load", () => {
  qualified++;
  assert.throws(() => two.setVisualZoomLevelLimits(1, 1), /KELD-COMPAT-013/);
  one.once("did-finish-load", () => {
    nested = true;
    assert.throws(() => one.setVisualZoomLevelLimits(1, 1), /KELD-COMPAT-013/);
  });
  one.emit("did-finish-load");
  assert.ok(one.setVisualZoomLevelLimits(1, 1) instanceof Promise);
  assert.throws(() => one.setVisualZoomLevelLimits(1, 2), /KELD-COMPAT-013/);
  let coerced = false;
  const coercible = {
    valueOf() {
      coerced = true;
      return 1;
    },
  };
  assert.throws(
    () => one.setVisualZoomLevelLimits(coercible as unknown as number, 1),
    /KELD-COMPAT-013/,
  );
  assert.equal(coerced, false);
  assert.throws(
    () => Reflect.apply(one.setVisualZoomLevelLimits, one, [1]),
    /KELD-COMPAT-013/,
  );
  assert.throws(
    () => Reflect.apply(one.setVisualZoomLevelLimits, one, [1, 1, 1]),
    /KELD-COMPAT-013/,
  );
  snapshots.get(ref.id)!.document.current = false;
  assert.throws(() => one.setVisualZoomLevelLimits(1, 1), /KELD-COMPAT-013/);
});
dispatch(make("NavigationFinishedUnitPinchV1"));
assert.equal(qualified, 1);
assert.equal(nested, true);
assert.throws(() => one.setVisualZoomLevelLimits(1, 1), /KELD-COMPAT-013/);
const currentDocument = snapshots.get(ref.id)!.document;
currentDocument.current = true;
currentDocument.generation = 2;
let historical = 0;
one.once("did-finish-load", () => {
  currentDocument.current = false;
});
one.once("did-finish-load", () => {
  historical++;
  assert.throws(() => one.setVisualZoomLevelLimits(1, 1), /KELD-COMPAT-013/);
});
dispatch(make("NavigationFinishedUnitPinchV1", 2));
assert.equal(
  historical,
  1,
  "the in-progress EventEmitter call keeps its listeners",
);
let stale = 0;
one.once("did-finish-load", () => stale++);
dispatch(make("NavigationFinished", 2));
assert.equal(stale, 0);
snapshots.get(ref.id)!.document.current = true;
dispatch(make("NavigationFinished", 1));
assert.equal(stale, 0, "a stale generation must not dispatch");
snapshots.get(ref.id)!.host_destroyed = true;
dispatch(make("NavigationFinished", 2));
assert.equal(stale, 0, "a Host-destroyed snapshot must not emit a finish");
assert.throws(() => one.setVisualZoomLevelLimits(1, 1), /KELD-COMPAT-011/);
console.log("PASS windows-navigation-finish");

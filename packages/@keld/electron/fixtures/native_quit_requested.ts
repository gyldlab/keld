// Actual API/Electron owners with controlled Source/transport inputs only.
// No authenticated Host/native/menu/original-editor milestone is fabricated.
import assert from "node:assert/strict";
import { LifecycleLink, type LifecycleHandler } from "../../api/src/link.ts";
import type { WindowRef, WindowRequest } from "../../api/src/window-codec.ts";
const mode = process.argv[2];
const requestFailure = new Error(
  "KELD-IPC-006: controlled original RequestClose failure",
);
let handlers: LifecycleHandler | undefined;
let admitted = true,
  requests = 0,
  terminals = 0;
let replacementAppeared = false;
const outbound: WindowRequest[] = [];
let resolveClose: ((value: unknown) => void) | undefined;
let reportRequest: (() => void) | undefined;
const requestObserved = new Promise<void>((resolve) => {
  reportRequest = resolve;
});
const window = { id: 7, incarnation: 258n };
const snapshot = {
  window,
  host_destroyed: false,
  document: { current: true, finished: true, generation: 1 },
  pending_close: null as bigint | null,
};
const source = {
  desktop: () => ({}),
  get: (ref: WindowRef) =>
    ref.id === snapshot.window.id &&
    ref.incarnation === snapshot.window.incarnation
      ? snapshot
      : undefined,
  all: () =>
    mode?.startsWith("zero") && !replacementAppeared
      ? []
      : mode === "multi"
        ? [snapshot, { ...snapshot, window: { id: 8, incarnation: 259n } }]
        : [snapshot],
};
const session = {
  get windowAdmissionOpen() {
    return admitted;
  },
  windowSource: source,
  callWindow(request: WindowRequest) {
    outbound.push(request);
    requests++;
    reportRequest?.();
    if (mode === "request-error") return Promise.reject(requestFailure);
    return new Promise((resolve) => {
      resolveClose = resolve;
    });
  },
  quit() {
    terminals++;
    admitted = false;
    return Promise.resolve();
  },
  close() {
    admitted = false;
  },
};
LifecycleLink.connect = (async (
  _target: string,
  selected: LifecycleHandler,
) => {
  handlers = selected;
  selected.onConnected?.(session as unknown as LifecycleLink);
  if (mode !== "pre-ready") queueMicrotask(selected.onReady);
  return session as unknown as LifecycleLink;
}) as typeof LifecycleLink.connect;
process.env.KELD_APP_LINK = "controlled-model-only";
if (mode === "observer-retires") {
  const { onNativeQuitRequested } = await import("../../api/src/app.ts");
  onNativeQuitRequested(() => {
    admitted = false;
  });
}
const { app } = await import("../src/app.ts");
const { captureOriginalWindowQuitScope } = await import("../../api/src/app.ts");
const beforeAttack = mode?.startsWith("zero-before-") === true;
const willAttack =
  mode === "zero-will-overwrite" ||
  mode?.startsWith("zero-will-global-") === true ||
  mode?.startsWith("zero-will-reflect-") === true ||
  mode?.startsWith("zero-will-slice-") === true ||
  mode === "zero-will-getter-spoof";
let before = 0;
let willQuit = 0;
let quitEvents = 0;
const sequence: string[] = [];
const beforeVetoEvents: Array<{ readonly defaultPrevented: boolean }> = [];
const willVetoEvents: Array<{ readonly defaultPrevented: boolean }> = [];
const beforeVetoPreFlags: boolean[] = [];
const willVetoPreFlags: boolean[] = [];
let nested: Promise<void> | undefined;
app.on("before-quit", (event: unknown) => {
  before++;
  sequence.push("before-quit");
  nested = app.quit();
  if (mode === "veto") (event as { preventDefault(): void }).preventDefault();
});
if (beforeAttack) {
  app.on("before-quit", (event: unknown) => {
    sequence.push("before-sabotage");
    if (mode === "zero-before-getter-spoof")
      Object.defineProperty(event, "defaultPrevented", {
        get: () => true,
      });
    else if (!mode?.includes("-reflect-") && !mode?.includes("-slice-"))
      (event as { preventDefault(): void }).preventDefault = () => {};
  });
  app.on("before-quit", (event: unknown) => {
    sequence.push("before-veto");
    const cancel = event as {
      readonly defaultPrevented: boolean;
      preventDefault(): void;
    };
    const preVeto = cancel.defaultPrevented;
    beforeVetoPreFlags.push(preVeto);
    if (!preVeto) cancel.preventDefault();
    beforeVetoEvents.push(cancel);
  });
}
if (willAttack) {
  app.on("will-quit", (event: unknown) => {
    sequence.push("will-sabotage");
    if (mode === "zero-will-getter-spoof")
      Object.defineProperty(event, "defaultPrevented", {
        get: () => true,
      });
    else if (!mode?.includes("-reflect-") && !mode?.includes("-slice-"))
      (event as { preventDefault(): void }).preventDefault = () => {};
  });
}
if (mode?.startsWith("zero-will")) {
  app.on("will-quit", (event: unknown) => {
    willQuit++;
    sequence.push("will-quit");
    if (mode === "zero-will-create") {
      replacementAppeared = true;
      return;
    }
    if (mode === "zero-will-throw" && willQuit === 1)
      throw new Error("controlled will-quit listener failure");
    if (willAttack) {
      const cancel = event as {
        readonly defaultPrevented: boolean;
        preventDefault(): void;
      };
      const preVeto = cancel.defaultPrevented;
      willVetoPreFlags.push(preVeto);
      if (!preVeto) cancel.preventDefault();
      willVetoEvents.push(cancel);
      return;
    }
    if (mode === "zero-will-veto" || willQuit === 1)
      (event as { preventDefault(): void }).preventDefault();
  });
  app.on("quit", () => {
    quitEvents++;
    sequence.push("quit");
  });
}
const ready = app.whenReady();
if (mode !== "pre-ready") await ready;
assert.ok(handlers);
if (mode?.startsWith("live-census-")) snapshot.document.current = false;
// Poison one ordinary global at a time, only during the actual native intent.
// Source imports already captured their framework intrinsics before this point.
const dispatchWithPoison = (): unknown => {
  const originalDefine = Object.defineProperty;
  const originalApply = Reflect.apply;
  const originalSlice = Array.prototype.slice;
  const originalFilter = Array.prototype.filter;
  const originalSome = Array.prototype.some;
  try {
    if (mode?.endsWith("-global-noop"))
      Object.defineProperty = ((target: object) =>
        target) as typeof Object.defineProperty;
    if (mode?.endsWith("-global-throw"))
      Object.defineProperty = (() => {
        throw new Error("controlled global defineProperty replacement");
      }) as typeof Object.defineProperty;
    if (mode?.endsWith("-reflect-noop"))
      Reflect.apply = (() => undefined) as typeof Reflect.apply;
    if (mode?.endsWith("-reflect-throw"))
      Reflect.apply = (() => {
        throw new Error("controlled global Reflect.apply replacement");
      }) as typeof Reflect.apply;
    if (mode?.endsWith("-slice-noop"))
      Array.prototype.slice = (() => []) as typeof Array.prototype.slice;
    if (mode?.endsWith("-slice-throw"))
      Array.prototype.slice = (() => {
        throw new Error("controlled global Array.slice replacement");
      }) as typeof Array.prototype.slice;
    if (mode === "live-census-filter-only" || mode === "live-census-both")
      Array.prototype.filter = (() => []) as typeof Array.prototype.filter;
    if (mode === "live-census-some-only" || mode === "live-census-both")
      Array.prototype.some = (() => false) as typeof Array.prototype.some;
    handlers?.onQuitRequested?.(session as unknown as LifecycleLink);
    return undefined;
  } catch (error) {
    return error;
  } finally {
    Object.defineProperty = originalDefine;
    Reflect.apply = originalApply;
    Array.prototype.slice = originalSlice;
    Array.prototype.filter = originalFilter;
    Array.prototype.some = originalSome;
  }
};
if (mode?.startsWith("scope-")) {
  snapshot.pending_close = 1n;
  const scope = captureOriginalWindowQuitScope(window);
  assert.ok(
    scope,
    "approved original graph facts must capture the current scope",
  );
  assert.deepEqual(scope.window, window);
  assert.equal(scope.isCurrentPending(window, 1n), true);
  if (mode === "scope-stale-window")
    snapshot.window = { ...window, incarnation: window.incarnation + 1n };
  else if (mode === "scope-noncurrent-document")
    snapshot.document.current = false;
  else if (mode === "scope-unfinished-document")
    snapshot.document.finished = false;
  else if (mode === "scope-destroyed-window") snapshot.host_destroyed = true;
  else if (mode === "scope-retired") admitted = false;
  else if (mode === "scope-ended")
    handlers.onLinkDead(
      new Error("KELD-IPC-004: controlled captured scope ended"),
    );
  else if (mode === "scope-replaced-session")
    handlers.onConnected?.({ ...session } as unknown as LifecycleLink);
  else throw new Error("unknown scope mode: " + mode);
  assert.equal(scope.isCurrentPending(window, 1n), false);
  if (mode !== "scope-replaced-session")
    assert.equal(captureOriginalWindowQuitScope(window), undefined);
  await assert.rejects(scope.requestClose(), /KELD-CORE-043/);
  assert.equal(requests, 0, "retired scope cannot send a RequestClose");
  assert.equal(terminals, 0, "retired scope cannot fall back to forceful Quit");
} else {
  if (mode === "retired") admitted = false;
  if (mode === "ended")
    handlers.onLinkDead(
      new Error("KELD-IPC-004: controlled predecessor ended"),
    );
  const current = mode === "wrong-session" ? { ...session } : session;
  if (beforeAttack || willAttack || mode?.startsWith("live-census-"))
    assert.equal(dispatchWithPoison(), undefined, "native Quit must not throw");
  else handlers.onQuitRequested?.(current as unknown as LifecycleLink);
  if (
    [
      "missing-selection",
      "schema-denied",
      "facade-disabled",
      "runtime-disabled",
      "pre-ready",
      "retired",
      "ended",
      "wrong-session",
      "observer-retires",
    ].includes(mode!)
  ) {
    if (mode !== "wrong-session" && mode !== "observer-retires")
      assert.equal(captureOriginalWindowQuitScope(window), undefined);
    assert.equal(before, 0);
    assert.equal(requests, 0);
    assert.equal(terminals, 0);
  } else if (mode === "veto") {
    assert.equal(before, 1);
    assert.equal(requests, 0);
    assert.equal(terminals, 0);
    await nested;
    handlers.onQuitRequested?.(session as unknown as LifecycleLink);
    assert.equal(
      before,
      2,
      "a new native intent after veto can enter a fresh D2 attempt",
    );
    await nested;
  } else if (mode === "zero") {
    await nested;
    assert.equal(before, 1);
    assert.equal(requests, 0);
    assert.equal(terminals, 1);
    handlers.onQuitRequested?.(session as unknown as LifecycleLink);
    assert.equal(
      terminals,
      1,
      "queued predecessor intent cannot run after terminal admission",
    );
  } else if (mode?.startsWith("live-census-")) {
    await assert.rejects(nested, /KELD-COMPAT-012/);
    assert.equal(before, 1);
    assert.equal(requests, 0);
    assert.equal(
      terminals,
      0,
      "a live unqualified native window must block Quit",
    );
  } else if (beforeAttack) {
    await nested;
    assert.equal(before, 1);
    assert.equal(beforeVetoEvents.length, 1);
    assert.deepEqual(beforeVetoPreFlags, [false]);
    assert.equal(beforeVetoEvents[0]!.defaultPrevented, true);
    assert.equal(terminals, 0, "before-quit veto must retain the role");
    assert.equal(dispatchWithPoison(), undefined, "retry must not throw");
    await nested;
    assert.equal(before, 2);
    assert.equal(beforeVetoEvents.length, 2);
    assert.deepEqual(beforeVetoPreFlags, [false, false]);
    assert.equal(beforeVetoEvents[1]!.defaultPrevented, true);
    assert.equal(terminals, 0, "repeated veto must retain the role");
    assert.deepEqual(sequence, [
      "before-quit",
      "before-sabotage",
      "before-veto",
      "before-quit",
      "before-sabotage",
      "before-veto",
    ]);
  } else if (willAttack) {
    await nested;
    assert.equal(before, 1);
    assert.equal(willQuit, 1);
    assert.equal(willVetoEvents.length, 1);
    assert.deepEqual(willVetoPreFlags, [false]);
    assert.equal(willVetoEvents[0]!.defaultPrevented, true);
    assert.equal(terminals, 0, "will-quit veto must retain the Host");
    assert.equal(quitEvents, 0);
    assert.equal(dispatchWithPoison(), undefined, "retry must not throw");
    await nested;
    assert.equal(before, 2);
    assert.equal(willQuit, 2);
    assert.equal(willVetoEvents.length, 2);
    assert.deepEqual(willVetoPreFlags, [false, false]);
    assert.equal(willVetoEvents[1]!.defaultPrevented, true);
    assert.equal(terminals, 0, "repeated will-quit veto must retain the Host");
    assert.equal(quitEvents, 0);
    assert.deepEqual(sequence, [
      "before-quit",
      "will-sabotage",
      "will-quit",
      "before-quit",
      "will-sabotage",
      "will-quit",
    ]);
  } else if (
    mode === "zero-will-veto" ||
    mode === "zero-will-retry" ||
    mode === "zero-will-throw"
  ) {
    await nested;
    assert.equal(before, 1);
    assert.equal(willQuit, 1, "will-quit must fire before terminal Quit");
    assert.equal(terminals, 0, "preventDefault retains the Host and role");
    assert.equal(quitEvents, 0, "a veto cannot emit quit");
    handlers.onQuitRequested?.(session as unknown as LifecycleLink);
    await nested;
    assert.equal(before, 2, "retry must emit a fresh before-quit");
    assert.equal(willQuit, 2, "retry must emit a fresh will-quit");
    assert.equal(terminals, mode === "zero-will-veto" ? 0 : 1);
    assert.equal(quitEvents, mode === "zero-will-veto" ? 0 : 1);
    assert.deepEqual(
      sequence,
      mode === "zero-will-veto"
        ? ["before-quit", "will-quit", "before-quit", "will-quit"]
        : ["before-quit", "will-quit", "before-quit", "will-quit", "quit"],
    );
  } else if (mode === "zero-will-create") {
    await assert.rejects(nested, /KELD-COMPAT-012/);
    assert.deepEqual(sequence, ["before-quit", "will-quit"]);
    assert.equal(willQuit, 1);
    assert.equal(requests, 0);
    assert.equal(terminals, 0, "a created window blocks terminal teardown");
    assert.equal(quitEvents, 0);
  } else if (mode === "multi") {
    assert.ok(nested);
    await assert.rejects(nested, /KELD-COMPAT-012/);
    assert.equal(before, 1);
    assert.equal(requests, 0);
    assert.equal(terminals, 0);
  } else if (mode === "request-error") {
    await requestObserved;
    assert.ok(nested);
    await assert.rejects(nested, (error) => error === requestFailure);
    assert.equal(before, 1);
    assert.equal(requests, 1);
    assert.equal(terminals, 0);
  } else {
    assert.equal(mode, "request");
    handlers.onQuitRequested?.(session as unknown as LifecycleLink);
    assert.equal(
      before,
      1,
      "pending native intents reuse the existing D2 attempt",
    );
    // Observe the actual outbound call edge, never a guessed number of turns.
    await requestObserved;
    assert.equal(requests, 1);
    assert.deepEqual(outbound, [{ variant: "RequestClose", window }]);
    assert.equal(terminals, 0);
    assert.ok(resolveClose);
    resolveClose({ variant: "CloseRequested", close_seq: 1n });
    await nested;
  }
}
console.log("PASS native-quit-requested-source " + mode);

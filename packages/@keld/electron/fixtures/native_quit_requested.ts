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
    mode === "zero"
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
let before = 0;
let nested: Promise<void> | undefined;
app.on("before-quit", (event: unknown) => {
  before++;
  nested = app.quit();
  if (mode === "veto") (event as { preventDefault(): void }).preventDefault();
});
const ready = app.whenReady();
if (mode !== "pre-ready") await ready;
assert.ok(handlers);
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
  handlers.onQuitRequested?.(current as unknown as LifecycleLink);
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

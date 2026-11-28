// Test-only actual-module adapter. Only the existing LifecycleLink connect/CALL
// Owned by the Electron package's review-only test suite.
// boundary is injected. No real link, native window, new schema or authority.
import assert from "node:assert/strict";
import { LifecycleLink, type LifecycleHandler } from "../../api/src/link.ts";
import { WindowSource } from "../../api/src/window-source.ts";
import {
  encodeWindowEvent,
  type WindowEvent,
  type WindowRequest,
  type WindowResponse,
} from "../../api/src/window-codec.ts";

let hooks: LifecycleHandler | undefined;
let alive = true;
let forced = 0;
const trace: string[] = [];
const calls: {
  request: WindowRequest;
  resolve: (response: WindowResponse) => void;
}[] = [];
const source = new WindowSource((fact) => hooks?.onWindowFact?.(fact));
function apply(event: WindowEvent) {
  const decoded = source.decode(encodeWindowEvent(event));
  source.apply(decoded);
  return decoded;
}
function fact(event: WindowEvent) {
  source.dispatch(apply(event));
}
const ref = { id: 7, incarnation: 258n };
const state = {
  bounds: { x: 0, y: 0, width: 800, height: 600 },
  focused: false,
  maximized: false,
  full_screen: false,
};
const session = {
  windowSource: source,
  get windowAdmissionOpen() {
    return alive;
  },
  callWindow(request: WindowRequest) {
    return new Promise<WindowResponse>((resolve) =>
      calls.push({ request, resolve }),
    );
  },
  quit() {
    forced++;
    alive = false;
    return Promise.resolve();
  },
  close() {
    alive = false;
  },
};
const captured = session as unknown as LifecycleLink;
LifecycleLink.connect = (async (_target: string, handler: LifecycleHandler) => {
  hooks = handler;
  hooks.onConnected?.(captured);
  fact({
    variant: "DesktopSnapshot",
    epoch: 1n,
    snapshot: {
      primary_id: 1,
      displays: [
        {
          id: 1,
          bounds: { x: 0, y: 0, width: 1440, height: 900 },
          work_area: { x: 0, y: 24, width: 1440, height: 876 },
          scale_factor: 2,
        },
      ],
      should_use_dark_colors: false,
    },
  });
  fact({ variant: "Created", window: ref, state, pending_close: null });
  fact({
    variant: "DocumentRecovered",
    window: ref,
    document_generation: 1,
    native_url: "file:///selected/editor.html",
  });
  queueMicrotask(handler.onReady);
  return captured;
}) as typeof LifecycleLink.connect;
process.env.KELD_APP_LINK = "scripted-only";
const { app } = await import("../src/app.ts");
const { BrowserWindow, awaitOriginalIpcWindowCloseVeto } =
  await import("../src/browser-window.ts");
const { captureOriginalWindowQuitScope, getDispatchedPendingWindowCloses } =
  await import("../../api/src/app.ts");
await app.whenReady();
const scope = captureOriginalWindowQuitScope(ref);
assert.ok(scope);
const [window] = BrowserWindow.getAllWindows();
assert.ok(window);
const contents = window.webContents;
let firstVeto = true;
let nested: Promise<void> | undefined;
let nestedSettled = false;
let firstScopePending = false;
app.on("before-quit", () => {
  trace.push("before");
});
app.on("will-quit", () => {
  trace.push("will");
});
app.on("quit", () => {
  trace.push("quit");
});
app.on("window-all-closed", () => {
  trace.push("last");
});
window.on("close", (event) => {
  trace.push("close");
  firstScopePending = scope.isCurrentPending(ref, firstVeto ? 3n : 4n);
  nested = app.quit();
  void nested.then(() => {
    nestedSettled = true;
  });
  if (firstVeto) event.preventDefault();
});
contents.on("destroyed", () => trace.push("contents-destroyed"));
window.on("closed", () => {
  assert.equal(window.isDestroyed(), true);
  assert.equal(contents.isDestroyed(), true);
  trace.push("closed");
});

const first = app.quit();
let firstSettled = false;
void first.then(() => {
  firstSettled = true;
});
assert.equal(calls[0]?.request.variant, "RequestClose");
assert.deepEqual(trace, ["before"]);
const pending = apply({
  variant: "CloseRequested",
  window: ref,
  close_seq: 3n,
});
await Promise.resolve();
assert.equal(firstSettled, false);
assert.deepEqual(getDispatchedPendingWindowCloses(), []);
assert.deepEqual(trace, ["before"]);
source.dispatch(pending);
assert.equal(firstScopePending, true);
assert.equal(nested, first);
assert.deepEqual(calls[1]?.request, {
  variant: "CloseReply",
  window: ref,
  close_seq: 3n,
  verdict: "Veto",
});
// Real facade handoff must settle while the first RequestClose reply is held.
await first;
assert.equal(nestedSettled, true);
assert.equal(app.quit(), first);
assert.deepEqual(getDispatchedPendingWindowCloses(), [
  { window: ref, closeSeq: 3n },
]);
calls[0]!.resolve({ variant: "CloseRequested", close_seq: 3n });
const vetoApplied = awaitOriginalIpcWindowCloseVeto(ref, () =>
  scope.ownsCurrentGeneration(),
);
calls[1]!.resolve({ variant: "ReplyApplied" });
await vetoApplied;
assert.equal(source.get(ref)?.pending_close, null);
assert.deepEqual(getDispatchedPendingWindowCloses(), []);
assert.equal(window.isDestroyed(), false);
assert.equal(forced, 0);

firstVeto = false;
nestedSettled = false;
const second = app.quit();
assert.notEqual(second, first);
assert.deepEqual(trace, ["before", "close", "before"]);
fact({ variant: "CloseRequested", window: ref, close_seq: 4n });
assert.equal(firstScopePending, true);
assert.equal(nested, second);
await second;
assert.equal(nestedSettled, true);
assert.deepEqual(calls[3]?.request, {
  variant: "CloseReply",
  window: ref,
  close_seq: 4n,
  verdict: "Allow",
});
calls[2]!.resolve({ variant: "CloseRequested", close_seq: 4n });
calls[3]!.resolve({ variant: "ReplyApplied" });
// Resolve the real API verdict bookkeeping first. Allow is not release.
await Promise.resolve();
await Promise.resolve();
assert.equal(source.get(ref)?.host_destroyed, false);
assert.equal(window.isDestroyed(), false);
assert.equal(forced, 0);
assert.deepEqual(trace, ["before", "close", "before", "close"]);
const released = apply({ variant: "Destroyed", window: ref });
assert.equal(source.get(ref)?.host_destroyed, true);
assert.equal(window.isDestroyed(), false);
assert.equal(forced, 0);
source.dispatch(released);
assert.equal(source.get(ref), undefined);
assert.deepEqual(trace, [
  "before",
  "close",
  "before",
  "close",
  "contents-destroyed",
  "closed",
]);
assert.ok(hooks);
hooks.onLastWindowClosed(captured);
assert.equal(forced, 0);
assert.deepEqual(trace, [
  "before",
  "close",
  "before",
  "close",
  "contents-destroyed",
  "closed",
]);
assert.equal(calls.length, 4);
// This is the current scoped handoff, not the proposed terminal continuation.
console.log("PASS actual facade/Source close receipt integration");

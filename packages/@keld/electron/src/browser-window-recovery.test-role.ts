/** Isolated Bun role: real WindowSource codec/mirror with a controlled Host-call seam. */
import assert from "node:assert/strict";
import { mock } from "bun:test";
import { linkError } from "../../kipc/src/transport.ts";
import {
  encodeWindowEvent,
  type WindowEvent,
  type WindowRef,
  type WindowRequest,
  type WindowResponse,
} from "../../api/src/window-codec.ts";
import { WindowSource } from "../../api/src/window-source.ts";
import { WindowCloseReceipts } from "../../api/src/window-close-receipts.ts";

const mode = process.argv[2];
let ready = false;
let signalReady: () => void = () => {};
const readyPromise = new Promise<void>((resolve) => {
  signalReady = resolve;
});
let factListener: (fact: WindowEvent) => void = () => {
  if (mode === "late-import-replay") return;
  throw new Error("window event listener was not installed");
};
const readyCatchUps = new Set<() => void>();
const deliveredCloses = new WindowCloseReceipts();
const source = new WindowSource((fact) => {
  deliveredCloses.observe(fact);
  factListener(fact);
});
const calls: WindowRequest[] = [];
let lastId = 0;
let adoptNext: WindowRef | undefined;
const state = {
  bounds: { x: 15, y: 30, width: 640, height: 480 },
  focused: false,
  maximized: false,
  full_screen: false,
};

function fact(event: WindowEvent, dispatch = true): WindowEvent {
  const decoded = source.decode(encodeWindowEvent(event));
  source.apply(decoded);
  if (dispatch) source.dispatch(decoded);
  return decoded;
}
function createHostWindow(pending_close: bigint | null = null): WindowRef {
  const window = { id: ++lastId, incarnation: BigInt(100 + lastId) };
  fact({ variant: "Created", window, state, pending_close });
  return window;
}
function becomeReady(): void {
  if (mode === "ready-same-turn-continuation") {
    // Mirror the real API: raw Ready is applied now, while public readiness
    // is published in the next microtask, not mid-WorkerLink drain.
    queueMicrotask(() => {
      ready = true;
      signalReady();
      for (const callback of readyCatchUps) setImmediate(callback);
    });
    return;
  }
  ready = true;
  signalReady();
  for (const callback of readyCatchUps) setImmediate(callback);
}

// The mock is only the host transport; WindowSource's wire decode, apply and
// dispatch remain real. The subprocess prevents mocks leaking into other tests.
mock.module("../../api/src/app.ts", () => ({
  // This role controls Host window facts, not lifecycle LastWindowClosed.
  // Native E1 `closed -> all-closed -> Destroy return` is tested separately
  // through the real shipping macOS Host and actual lifecycle applier.
  flushAppliedLastWindowClosedAfterDestroy: () => {},
  app: { whenReady: () => readyPromise, isReady: () => ready },
  getWindowSnapshot: (window: WindowRef) =>
    ready ? source.get(window) : undefined,
  getWindowSnapshots: () => (ready ? source.all() : []),
  getDispatchedPendingWindowCloses: () =>
    ready ? deliveredCloses.pending() : [],
  onWindowFactApplied: (
    listener: (event: WindowEvent) => void,
    catchUp?: () => void,
  ) => {
    factListener = listener;
    if (catchUp) {
      readyCatchUps.add(catchUp);
      if (ready) setImmediate(catchUp);
    }
    return () => {};
  },
  invokeWindow: async (request: WindowRequest): Promise<WindowResponse> => {
    calls.push(request);
    return { variant: "ReplyApplied" };
  },
  invokeWindowBlocking: (request: WindowRequest): WindowResponse => {
    calls.push(request);
    if (request.variant === "Create") {
      if (adoptNext) {
        const window = adoptNext;
        adoptNext = undefined;
        return { variant: "Adopted", window };
      }
      return { variant: "Created", window: createHostWindow() };
    }
    if (request.variant === "Destroy") {
      // A blocking response sees the applied Host tombstone; notification
      // dispatch happens after the caller resumes, as on the real WorkerLink.
      fact({ variant: "Destroyed", window: request.window }, false);
      return { variant: "Destroyed" };
    }
    throw new Error(`unexpected blocking call ${request.variant}`);
  },
}));
const creationEvents: { name: string; value: unknown }[] = [];
mock.module("./app.ts", () => ({
  emitNativeAppEvent: (name: string, _event: unknown, value: unknown) => {
    creationEvents.push({ name, value });
  },
}));

let lateReplay: WindowRef | undefined;
if (mode === "late-import-replay") {
  // An authenticated WindowSource can apply its Created replay before the
  // Electron module installs any onWindowFactApplied listener.
  lateReplay = { id: ++lastId, incarnation: BigInt(100 + lastId) };
  fact({ variant: "Created", window: lateReplay, state, pending_close: 7n });
  becomeReady();
}
const { BrowserWindow } = await import("./browser-window.ts");
if (mode === "late-import-replay") {
  assert.ok(lateReplay);
  const win = BrowserWindow.getAllWindows()[0];
  assert.equal(win.id, lateReplay.id);
  let closeEvents = 0;
  win.on("close", (event: { preventDefault(): void }) => {
    closeEvents += 1;
    event.preventDefault();
  });
  await new Promise<void>((resolve) => setImmediate(resolve));
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(closeEvents, 1, "late import must reconcile pending Host close");
  assert.deepEqual(
    calls.filter((call) => call.variant === "CloseReply"),
    [
      {
        variant: "CloseReply",
        window: lateReplay,
        close_seq: 7n,
        verdict: "Veto",
      },
    ],
  );
} else if (mode === "ready-same-turn-continuation") {
  const ref = createHostWindow(7n);
  const observed: string[] = [];
  void readyPromise.then(() => {
    const win = BrowserWindow.getAllWindows()[0];
    assert.equal(win.id, ref.id);
    win.on("close", (event: { preventDefault(): void }) => {
      observed.push("close");
      event.preventDefault();
    });
    win.on("resize", () => observed.push("resize"));
  });
  becomeReady();
  // Same transport dispatch task, without yielding to user Promise handlers.
  fact({
    variant: "Bounds",
    window: ref,
    bounds: { ...state.bounds, width: 777 },
  });
  await new Promise<void>((resolve) => setImmediate(resolve));
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.deepEqual(
    observed,
    ["close"],
    "the pre-Ready Bounds updates the mirror, not a pre-Ready user listener",
  );
  assert.deepEqual(BrowserWindow.getAllWindows()[0]?.getSize(), [777, 480]);
  assert.deepEqual(
    calls.filter((call) => call.variant === "CloseReply"),
    [{ variant: "CloseReply", window: ref, close_seq: 7n, verdict: "Veto" }],
  );
  fact({
    variant: "Bounds",
    window: ref,
    bounds: { ...state.bounds, width: 888 },
  });
  assert.deepEqual(
    observed,
    ["close", "resize"],
    "later facts keep normal listener order",
  );
} else if (mode === "applied-ahead-close-order") {
  const ref = createHostWindow();
  becomeReady();
  const win = BrowserWindow.getAllWindows()[0];
  const observed: string[] = [];
  win.on("resize", () => observed.push("resize"));
  win.on("close", (event: { preventDefault(): void }) => {
    observed.push("close");
    event.preventDefault();
  });
  // GH527 parked synchronous CALL can apply both frames before either reaches
  // its user-dispatch cursor. Mirror.pending_close=7 is not yet permission
  // to send CloseReply ahead of the earlier Bounds callback.
  const bounds = source.decode(
    encodeWindowEvent({
      variant: "Bounds",
      window: ref,
      bounds: { ...state.bounds, width: 777 },
    }),
  );
  const close = source.decode(
    encodeWindowEvent({
      variant: "CloseRequested",
      window: ref,
      close_seq: 7n,
    }),
  );
  source.apply(bounds);
  source.apply(close);
  assert.equal(source.get(ref)?.pending_close, 7n);
  BrowserWindow.getAllWindows();
  assert.deepEqual(
    calls.filter((call) => call.variant === "CloseReply"),
    [],
  );
  source.dispatch(bounds);
  source.dispatch(close);
  assert.deepEqual(observed, ["resize", "close"]);
  assert.deepEqual(
    calls.filter((call) => call.variant === "CloseReply"),
    [{ variant: "CloseReply", window: ref, close_seq: 7n, verdict: "Veto" }],
  );
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.deepEqual(
    observed,
    ["resize", "close"],
    "catch-up must not reorder or duplicate",
  );
} else if (mode === "recovery-gap") {
  // Host replays a gap-native close before the successor's Ready barrier.
  const ref = createHostWindow(7n);
  becomeReady();
  await Promise.resolve();
  const win = BrowserWindow.getAllWindows()[0];
  assert.equal(win.id, ref.id);
  let closeEvents = 0;
  win.on("close", (event: { preventDefault(): void }) => {
    closeEvents += 1;
    event.preventDefault();
  });
  // The successor gets its Ready turn to install existing application policy
  // before the held replay close is dispatched.
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(
    closeEvents,
    1,
    "Created.pending_close must reach the successor",
  );
  assert.deepEqual(
    calls.filter((call) => call.variant === "CloseReply"),
    [{ variant: "CloseReply", window: ref, close_seq: 7n, verdict: "Veto" }],
  );
  assert.equal(BrowserWindow.getAllWindows()[0], win);
  assert.equal(win.isDestroyed(), false);
} else if (mode === "replay-order") {
  const ref = createHostWindow(7n);
  becomeReady();
  const win = BrowserWindow.getAllWindows()[0];
  const observed: string[] = [];
  win.on("close", (event: { preventDefault(): void }) => {
    observed.push("close");
    event.preventDefault();
  });
  win.on("resize", () => observed.push("resize"));
  // The Host wrote Created(pending) before this later Bounds. Both source
  // facts are real decoded/applied bytes; the listener order is the oracle.
  fact({
    variant: "Bounds",
    window: ref,
    bounds: { x: 15, y: 30, width: 777, height: 480 },
  });
  assert.deepEqual(observed, ["close", "resize"]);
  assert.deepEqual(win.getSize(), [777, 480]);
  assert.deepEqual(
    calls.filter((call) => call.variant === "CloseReply"),
    [{ variant: "CloseReply", window: ref, close_seq: 7n, verdict: "Veto" }],
  );
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.deepEqual(observed, ["close", "resize"], "no duplicate replay close");
} else if (mode === "pre-ready-close-order") {
  const ref = createHostWindow();
  // A real CloseRequested can arrive after Subscribe but before the facade
  // emits Ready. It must be ordered ahead of the next Bounds fact, not only
  // when the first close was attached to a replayed Created frame.
  fact({ variant: "CloseRequested", window: ref, close_seq: 11n });
  becomeReady();
  const win = BrowserWindow.getAllWindows()[0];
  const observed: string[] = [];
  win.on("close", (event: { preventDefault(): void }) => {
    observed.push("close");
    event.preventDefault();
  });
  win.on("resize", () => observed.push("resize"));
  fact({
    variant: "Bounds",
    window: ref,
    bounds: { x: 15, y: 30, width: 888, height: 480 },
  });
  assert.deepEqual(observed, ["close", "resize"]);
  assert.deepEqual(
    calls.filter((call) => call.variant === "CloseReply"),
    [{ variant: "CloseReply", window: ref, close_seq: 11n, verdict: "Veto" }],
  );
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.deepEqual(observed, ["close", "resize"]);
} else if (mode === "move-only-does-not-resize") {
  becomeReady();
  const win = new BrowserWindow();
  const ref = source.all()[0]?.window;
  assert.ok(ref);
  const resized: string[] = [];
  win.on("resize", () => {
    resized.push(win.getSize().join("x"));
  });
  fact({
    variant: "Bounds",
    window: ref,
    bounds: { x: 25, y: 48, width: 640, height: 480 },
  });
  assert.deepEqual(resized, [], "a pure native move cannot emit resize");
  assert.deepEqual(win.getPosition(), [25, 48]);
  fact({
    variant: "Bounds",
    window: ref,
    bounds: { x: 25, y: 48, width: 777, height: 480 },
  });
  assert.deepEqual(
    resized,
    ["777x480"],
    "resize listener reads applied Host fact",
  );
  fact({
    variant: "Bounds",
    window: ref,
    bounds: { x: 25, y: 48, width: 777, height: 480 },
  });
  assert.deepEqual(resized, ["777x480"], "no duplicate resize for same bounds");
} else if (mode === "pre-ready-bounds-baseline") {
  const ref = createHostWindow();
  // Native Bounds is dispatched while there is no public Ready and no JS
  // wrapper; its Host-confirmed dimensions become the wrapper's baseline.
  fact({
    variant: "Bounds",
    window: ref,
    bounds: { ...state.bounds, width: 777 },
  });
  becomeReady();
  const win = BrowserWindow.getAllWindows()[0];
  assert.deepEqual(win.getSize(), [777, 480]);
  const seen: string[] = [];
  win.on("move", () => seen.push("move"));
  win.on("resize", () => seen.push("resize"));
  fact({
    variant: "Bounds",
    window: ref,
    bounds: { ...state.bounds, width: 777, x: 25 },
  });
  assert.deepEqual(
    seen,
    ["move"],
    "a later move must not replay pre-Ready resize",
  );
} else if (mode === "native-fact-event-parity") {
  becomeReady();
  const win = new BrowserWindow();
  const ref = source.all()[0]?.window;
  assert.ok(ref);
  const seen: string[] = [];
  win.on("move", () => {
    seen.push(`move:${win.getPosition().join(",")}`);
  });
  win.on("resize", () => {
    seen.push(`resize:${win.getSize().join("x")}`);
  });
  win.on("focus", () => {
    assert.equal(BrowserWindow.getFocusedWindow(), win);
    seen.push("focus");
  });
  win.on("blur", () => {
    assert.equal(BrowserWindow.getFocusedWindow(), null);
    seen.push("blur");
  });
  win.on("enter-full-screen", () => {
    assert.equal(win.isFullScreen(), true);
    seen.push("enter-full-screen");
  });
  win.on("leave-full-screen", () => {
    assert.equal(win.isFullScreen(), false);
    seen.push("leave-full-screen");
  });
  win.on("maximize", () => {
    assert.equal(win.isMaximized(), true);
    seen.push("maximize");
  });
  win.on("unmaximize", () => {
    assert.equal(win.isMaximized(), false);
    seen.push("unmaximize");
  });
  fact({ variant: "Bounds", window: ref, bounds: { ...state.bounds, x: 55 } });
  fact({
    variant: "Bounds",
    window: ref,
    bounds: { ...state.bounds, x: 55, width: 777 },
  });
  fact({ variant: "Focus", window: ref, focused: true });
  fact({ variant: "Focus", window: ref, focused: false });
  fact({ variant: "FullScreen", window: ref, full_screen: true });
  fact({ variant: "FullScreen", window: ref, full_screen: false });
  fact({ variant: "Maximized", window: ref, maximized: true });
  fact({ variant: "Maximized", window: ref, maximized: false });
  assert.deepEqual(seen, [
    "move:55,30",
    "resize:777x480",
    "focus",
    "blur",
    "enter-full-screen",
    "leave-full-screen",
    "maximize",
    "unmaximize",
  ]);
} else if (mode === "bounds-throwing-move-isolated") {
  becomeReady();
  const win = new BrowserWindow();
  const ref = source.all()[0]?.window;
  assert.ok(ref);
  const seen: string[] = [];
  const reports: unknown[][] = [];
  const originalError = console.error;
  console.error = (...args: unknown[]) => {
    reports.push(args);
  };
  try {
    win.on("move", () => {
      seen.push("move");
      throw new Error("untrusted move event listener");
    });
    win.on("resize", () => {
      assert.deepEqual(win.getSize(), [777, 480]);
      seen.push("resize");
    });
    fact({
      variant: "Bounds",
      window: ref,
      bounds: { ...state.bounds, x: 25, width: 777 },
    });
    assert.deepEqual(
      seen,
      ["move", "resize"],
      "one throwing move cannot drop this same native fact's resize",
    );
    assert.equal(
      reports.length,
      1,
      "exactly one isolated application exception diagnostic",
    );
  } finally {
    console.error = originalError;
  }
} else if (mode === "bounds-move-destroys-no-resize") {
  becomeReady();
  const win = new BrowserWindow();
  const ref = source.all()[0]?.window;
  assert.ok(ref);
  const seen: string[] = [];
  win.on("move", () => {
    seen.push("move");
    win.destroy();
  });
  win.on("closed", () => seen.push("closed"));
  win.on("resize", () => seen.push("resize"));
  fact({
    variant: "Bounds",
    window: ref,
    bounds: { ...state.bounds, x: 25, width: 777 },
  });
  assert.deepEqual(
    seen,
    ["move", "closed"],
    "no event on wrapper after synchronous Destroy",
  );
  assert.equal(win.isDestroyed(), true);
  assert.deepEqual(BrowserWindow.getAllWindows(), []);
} else if (mode === "throwing-contents-destroyed") {
  becomeReady();
  const window = new BrowserWindow();
  const ref = source.all()[0]?.window;
  assert.ok(ref);
  const observed: string[] = [];
  const errors: unknown[][] = [];
  const originalError = console.error;
  console.error = (...args: unknown[]) => {
    errors.push(args);
  };
  try {
    window.webContents.on("destroyed", () => {
      observed.push("webContents destroyed");
      throw new Error("untrusted user listener");
    });
    window.on("closed", () => {
      assert.equal(window.isDestroyed(), true);
      observed.push("window closed");
    });
    // The Host has physically released the real window: its event cannot be
    // lost because a JS callback throws from the contents tombstone.
    fact({ variant: "Destroyed", window: ref });
    assert.deepEqual(observed, ["webContents destroyed", "window closed"]);
    assert.equal(window.isDestroyed(), true);
    assert.equal(BrowserWindow.getAllWindows().length, 0);
    assert.equal(errors.length, 1, "one bounded callback diagnostic");

    const second = new BrowserWindow();
    const secondContents = second.webContents;
    secondContents.on("destroyed", () => {
      observed.push("second contents destroyed");
      throw new Error("deferred user listener");
    });
    second.on("closed", () => observed.push("second closed"));
    second.destroy();
    await Promise.resolve();
    assert.equal(secondContents.isDestroyed(), true);
    assert.deepEqual(observed.slice(-2), [
      "second closed",
      "second contents destroyed",
    ]);
    assert.equal(
      errors.length,
      2,
      "deferred Destroy reports one diagnostic too",
    );
  } finally {
    console.error = originalError;
  }
} else if (mode === "no-public-contents-dispose") {
  becomeReady();
  const win = new BrowserWindow();
  const contents = win.webContents;
  let destroyed = 0;
  contents.on("destroyed", () => {
    destroyed += 1;
  });
  // A hostile app can reflect on a returned WebContents and invoke its
  // prototype methods. A public dispose() must never forge the Host tombstone.
  const forged = Reflect.get(contents, "dispose") as unknown;
  if (typeof forged === "function") Reflect.apply(forged, contents, []);
  assert.equal(contents.isDestroyed(), false, "Host never sent Destroyed");
  assert.equal(win.isDestroyed(), false);
  assert.equal(destroyed, 0, "caller must not fabricate native completion");
  assert.equal(BrowserWindow.getAllWindows()[0], win);
} else if (mode === "throwing-close-and-logger") {
  becomeReady();
  const win = new BrowserWindow();
  const ref = source.all()[0]?.window;
  assert.ok(ref);
  let closeEvents = 0;
  win.on("close", () => {
    closeEvents += 1;
    throw new Error("untrusted application close listener");
  });
  const originalError = console.error;
  console.error = () => {
    throw new Error("untrusted logger");
  };
  try {
    fact({ variant: "CloseRequested", window: ref, close_seq: 21n });
  } finally {
    console.error = originalError;
  }
  assert.equal(closeEvents, 1);
  assert.deepEqual(
    calls.filter((call) => call.variant === "CloseReply"),
    [{ variant: "CloseReply", window: ref, close_seq: 21n, verdict: "Veto" }],
    "a logging failure must never suppress the one terminal close verdict",
  );
  assert.equal(win.isDestroyed(), false);
} else if (mode === "close-listener-sync-destroy") {
  becomeReady();
  const win = new BrowserWindow();
  const ref = source.all()[0]?.window;
  assert.ok(ref);
  let closed = 0;
  win.on("close", () => win.destroy());
  win.on("closed", () => {
    closed += 1;
  });
  fact({ variant: "CloseRequested", window: ref, close_seq: 31n });
  assert.equal(closed, 1);
  assert.equal(win.isDestroyed(), true);
  assert.equal(source.get(ref)?.host_destroyed, true);
  assert.equal(calls.filter((call) => call.variant === "Destroy").length, 1);
  assert.deepEqual(
    calls.filter((call) => call.variant === "CloseReply"),
    [],
    "a synchronous Host Destroy from close listener retires pending seq; send no stale verdict",
  );
} else if (mode === "replay-link-death") {
  createHostWindow(7n);
  becomeReady();
  const errors: unknown[][] = [];
  const originalError = console.error;
  console.error = (...args: unknown[]) => {
    errors.push(args);
  };
  try {
    source.end(linkError("KELD-IPC-022", "peer closed"));
    // A deferred callback must not throw out of setImmediate after the
    // authenticated link's mirror was invalidated.
    await Promise.resolve();
    await new Promise<void>((resolve) => setImmediate(resolve));
    assert.equal(
      calls.filter((call) => call.variant === "CloseReply").length,
      0,
    );
    assert.equal(errors.length, 1, "record one terminal diagnostic");
    assert.match(
      String(errors[0]?.[0]),
      /Host mirror failure|late window replay could not be read|dispatched window close could not be read/,
      "only the first terminal mirror observation may report a diagnostic",
    );
  } finally {
    console.error = originalError;
  }
} else if (mode === "adopted-after-replay-wrapper") {
  const ref = createHostWindow();
  becomeReady();
  const previous = BrowserWindow.getAllWindows()[0];
  assert.equal(creationEvents.length, 0, "enumeration must not emit creation");
  adoptNext = ref;
  const adopted = new BrowserWindow();
  assert.equal(adopted, previous, "adoption preserves the replay wrapper");
  assert.deepEqual(
    creationEvents.map(({ name }) => name),
    ["web-contents-created", "browser-window-created"],
    "adoption publishes the ordered pair before returning",
  );
  assert.equal(creationEvents[0]?.value, previous.webContents);
  assert.equal(creationEvents[1]?.value, previous);
  adoptNext = ref;
  assert.equal(new BrowserWindow(), previous);
  assert.equal(
    creationEvents.length,
    2,
    "creation is published once per wrapper",
  );
  const fresh = new BrowserWindow();
  assert.notEqual(fresh, previous);
  assert.equal(creationEvents.length, 4, "new windows still publish creation");
} else if (mode === "adopted-without-replay-wrapper") {
  const ref = createHostWindow();
  adoptNext = ref;
  becomeReady();
  // The app constructs before calling getAllWindows(), so Host adopts a
  // retained physical window directly. This is not a brand-new safe document.
  const win = new BrowserWindow();
  assert.equal(win.id, ref.id);
  assert.equal(BrowserWindow.getAllWindows()[0], win);
  fact({ variant: "CloseRequested", window: ref, close_seq: 14n });
  assert.deepEqual(
    calls.filter((call) => call.variant === "CloseReply"),
    [{ variant: "CloseReply", window: ref, close_seq: 14n, verdict: "Veto" }],
    "host-adopted window must not automatically close before user veto setup",
  );
} else if (mode === "unmaterialized-close") {
  becomeReady();
  const ref = createHostWindow();
  // No caller has requested getAllWindows(), but Host already owns the window.
  const closeEvents: number[] = [];
  const emit = BrowserWindow.prototype.emit;
  BrowserWindow.prototype.emit = function (
    event: string | symbol,
    ...args: unknown[]
  ) {
    if (event === "close") closeEvents.push(this.id);
    return Reflect.apply(emit, this, [event, ...args]) as boolean;
  };
  const request: WindowEvent = {
    variant: "CloseRequested",
    window: ref,
    close_seq: 8n,
  };
  const applied = fact(request);
  source.dispatch(applied); // duplicate dispatch must not send a second verdict
  const win = BrowserWindow.getAllWindows()[0];
  assert.equal(win.id, ref.id);
  assert.deepEqual(
    closeEvents,
    [ref.id],
    "one close event for the pending sequence",
  );
  assert.deepEqual(
    calls.filter((call) => call.variant === "CloseReply"),
    [{ variant: "CloseReply", window: ref, close_seq: 8n, verdict: "Veto" }],
    "an unclaimed recovered wrapper must not silently Allow unsaved-close",
  );
  let laterCalls = 0;
  win.on("close", (event: { preventDefault(): void }) => {
    laterCalls += 1;
    event.preventDefault();
  });
  fact({ variant: "CloseRequested", window: ref, close_seq: 9n });
  assert.equal(laterCalls, 1, "reuse the wrapper and its installed listeners");
  assert.equal(calls.filter((call) => call.variant === "CloseReply").length, 2);
} else if (mode === "mutable-id") {
  becomeReady();
  const a = new BrowserWindow();
  const b = new BrowserWindow();
  const aId = a.id;
  const bId = b.id;
  assert.notEqual(aId, bId);
  assert.equal(Reflect.set(a, "id", bId), false, "id must not be writable");
  assert.throws(() => Object.defineProperty(a, "id", { value: bId }));
  assert.equal(a.id, aId);
  const descriptors = Object.getOwnPropertyDescriptor(a, "id");
  assert.equal(descriptors?.writable, false);
  assert.equal(descriptors?.configurable, false);
  a.destroy();
  assert.equal(BrowserWindow.getAllWindows()[0], b);
  let closeEvents = 0;
  b.on("close", (event: { preventDefault(): void }) => {
    closeEvents += 1;
    event.preventDefault();
  });
  fact({
    variant: "CloseRequested",
    window: { id: bId, incarnation: BigInt(100 + bId) },
    close_seq: 1n,
  });
  assert.equal(
    closeEvents,
    1,
    "A.destroy must not steal B's dispatch identity",
  );
  assert.equal(
    calls.filter((call) => call.variant === "CloseReply").at(-1)?.variant,
    "CloseReply",
  );
} else if (mode === "warning-throws-after-create") {
  becomeReady();
  const originalWarn = console.warn;
  console.warn = () => {
    throw new Error("hostile advisory logger");
  };
  let result: InstanceType<typeof BrowserWindow> | undefined;
  try {
    result = new BrowserWindow({ backgroundColor: "#fff" });
  } finally {
    console.warn = originalWarn;
  }
  assert.ok(result, "advisory logger cannot abort the post-Create constructor");
  assert.equal(result.id, 1);
  assert.equal(result.isDestroyed(), false);
  assert.equal(calls.filter((call) => call.variant === "Create").length, 1);
  assert.equal(BrowserWindow.getAllWindows()[0], result);

  const callerOptions: Record<string, unknown> = {};
  Object.defineProperty(callerOptions, "backgroundColor", {
    get() {
      throw new Error("hostile options getter");
    },
  });
  assert.throws(
    () => new BrowserWindow(callerOptions),
    /hostile options getter/,
  );
  assert.equal(
    calls.filter((call) => call.variant === "Create").length,
    1,
    "a throwing caller-owned getter must be evaluated before native Create",
  );
  assert.equal(BrowserWindow.getAllWindows()[0], result);
} else {
  throw new Error(`unknown recovery role ${mode}`);
}
console.log(`PASS ${mode}`);

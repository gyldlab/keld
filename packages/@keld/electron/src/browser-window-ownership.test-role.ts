/** Isolated hostile application: real Host-fact mirror, controlled CALL boundary. */
import assert from "node:assert/strict";
import { mock } from "bun:test";
import { linkError } from "../../kipc/src/transport.ts";
import { WindowSource } from "../../api/src/window-source.ts";
import {
  encodeWindowEvent,
  type WindowEvent,
  type WindowRef,
  type WindowRequest,
  type WindowResponse,
} from "../../api/src/window-codec.ts";

const mode = process.argv[2];
const defineProperty = Object.defineProperty;
const descriptor = Object.getOwnPropertyDescriptor;
let restore = (): void => {};
function poison(target: object, name: string): void {
  const original = descriptor(target, name);
  assert.ok(original);
  defineProperty(target, name, {
    ...original,
    value: () => {
      throw new Error(`hostile ${name}`);
    },
  });
  restore = () => defineProperty(target, name, original);
}
function poisonAfterCreate(): void {
  if (mode.startsWith("post-create-map-")) {
    poison(Map.prototype, mode.slice("post-create-map-".length));
  } else if (mode === "post-create-define-property") {
    poison(Object, "defineProperty");
  } else if (mode === "post-create-freeze") {
    poison(Object, "freeze");
  }
}
let listener: (fact: WindowEvent) => void = () => {};
const source = new WindowSource((fact) => listener(fact));
const calls: WindowRequest[] = [];
const creationEvents: { name: string; value: unknown }[] = [];
const ref = { id: 41, incarnation: 9001n };
let adopted = false;
let failConfirmation = false;
const state = {
  bounds: { x: 15, y: 30, width: 640, height: 480 },
  focused: false,
  maximized: false,
  full_screen: false,
};
function fact(event: WindowEvent, dispatch = true): void {
  const decoded = source.decode(encodeWindowEvent(event));
  source.apply(decoded);
  if (dispatch) source.dispatch(decoded);
}
mock.module("../../api/src/app.ts", () => ({
  app: { isReady: () => true, whenReady: async () => {} },
  flushAppliedLastWindowClosedAfterDestroy: () => {},
  getDispatchedPendingWindowCloses: () => [],
  getWindowSnapshots: () => source.all(),
  getWindowSnapshot: (window: WindowRef) => {
    if (failConfirmation) {
      failConfirmation = false;
      return undefined;
    }
    return source.get(window);
  },
  onWindowFactApplied: (callback: (event: WindowEvent) => void) => {
    listener = callback;
    return () => {};
  },
  invokeWindow: async (request: WindowRequest): Promise<WindowResponse> => {
    calls.push(request);
    return { variant: "ReplyApplied" };
  },
  invokeWindowBlocking: (request: WindowRequest): WindowResponse => {
    calls.push(request);
    if (request.variant === "Create") {
      if (mode === "unknown-create-outcome")
        throw linkError("KELD-IPC-022", "Create outcome is unknown");
      if (adopted) return { variant: "Adopted", window: ref };
      fact({ variant: "Created", window: ref, state, pending_close: null });
      failConfirmation = mode.startsWith("post-create-cleanup");
      poisonAfterCreate();
      return { variant: "Created", window: ref };
    }
    if (request.variant === "Destroy") {
      if (mode === "post-create-cleanup-fault")
        throw linkError("KELD-IPC-022", "Destroy confirmation was lost");
      if (mode === "post-create-cleanup-unexpected-reply")
        return { variant: "ReplyApplied" };
      if (mode !== "post-create-cleanup-no-tombstone")
        fact({ variant: "Destroyed", window: request.window }, false);
      return { variant: "Destroyed" };
    }
    throw new Error(`unexpected CALL ${request.variant}`);
  },
}));
mock.module("./app.ts", () => ({
  emitNativeAppEvent: (name: string, _event: unknown, value: unknown) => {
    creationEvents.push({ name, value });
    if (mode === "published-listener-throws")
      throw new Error("application creation callback threw");
  },
}));
const { BrowserWindow, readDialogParentRef } =
  await import("./browser-window.ts");
if (mode === "unknown-create-outcome") {
  assert.throws(() => new BrowserWindow(), /KELD-IPC-022/);
  assert.deepEqual(calls, [
    { variant: "Create", width: 800, height: 600, x: null, y: null },
  ]);
  assert.deepEqual(source.all(), []);
} else if (mode.startsWith("post-create-cleanup")) {
  let error: unknown;
  try {
    new BrowserWindow();
  } catch (caught) {
    error = caught;
  }
  assert.ok(
    error,
    "the original post-Create setup failure must remain observable",
  );
  assert.deepEqual(
    calls.map((call) => call.variant),
    ["Create", "Destroy"],
    "known newly Created window without a published owner must be cleaned up",
  );
  assert.deepEqual(calls[1], { variant: "Destroy", window: ref });
  if (mode !== "post-create-cleanup") {
    assert.match(
      String(error),
      /cleanup|release/i,
      "a cleanup fault must not be swallowed behind the original setup failure",
    );
    assert.equal(source.get(ref)?.host_destroyed, false);
    assert.ok(error instanceof Error);
    const cause = error.cause as { setup?: unknown; cleanup?: unknown };
    assert.match(String(cause.setup), /matching host-owned window fact/);
    assert.match(
      String(cause.cleanup),
      /Destroy confirmation|confirm native release/,
    );
  } else {
    assert.equal(
      source.get(ref)?.host_destroyed,
      true,
      "cleanup needs the authenticated Host tombstone",
    );
    assert.deepEqual(BrowserWindow.getAllWindows(), []);
  }
  assert.deepEqual(creationEvents, []);
} else if (mode === "published-listener-throws") {
  assert.throws(
    () => new BrowserWindow(),
    /application creation callback threw/,
  );
  const win = BrowserWindow.getAllWindows()[0];
  assert.equal(win.id, ref.id, "the published registry still owns the wrapper");
  assert.equal(
    calls.filter((call) => call.variant === "Destroy").length,
    0,
    "an application callback failure cannot revoke a successfully published wrapper",
  );
  assert.equal(source.get(ref)?.host_destroyed, false);
  win.destroy();
  assert.equal(source.get(ref)?.host_destroyed, true);
} else if (mode === "adopted-setup-failure") {
  fact({ variant: "Created", window: ref, state, pending_close: null });
  adopted = true;
  failConfirmation = true;
  assert.throws(() => new BrowserWindow(), /matching host-owned/);
  assert.deepEqual(
    calls.map((call) => call.variant),
    ["Create"],
  );
  assert.equal(
    source.get(ref)?.host_destroyed,
    false,
    "local adoption failure must preserve the existing dirty native document",
  );
  const win = BrowserWindow.getAllWindows()[0];
  assert.equal(win.id, ref.id);
  win.destroy();
} else {
  let win: InstanceType<typeof BrowserWindow> | undefined;
  if (mode === "weakmap-set") poison(WeakMap.prototype, "set");
  try {
    win = new BrowserWindow();
  } finally {
    // Collection poison remains active through identity/adoption/close/release.
    // Restore Object operations before the assertion library reads descriptors.
    if (mode === "post-create-define-property" || mode === "post-create-freeze")
      restore();
  }
  assert.ok(win, "successful Host Create must return an owned wrapper");
  assert.equal(win.id, ref.id);
  if (mode === "dialog-parent") {
    assert.deepEqual(readDialogParentRef(win), ref);
    assert.throws(() => readDialogParentRef(new Proxy(win, {})), TypeError);
  }
  assert.equal(BrowserWindow.getAllWindows()[0], win);
  assert.deepEqual(
    creationEvents.map(({ name }) => name),
    ["web-contents-created", "browser-window-created"],
  );
  assert.equal(creationEvents[0]?.value, win.webContents);
  assert.equal(creationEvents[1]?.value, win);
  assert.equal(source.get(ref)?.host_destroyed, false);
  adopted = true;
  assert.equal(new BrowserWindow(), win, "adoption preserves wrapper identity");
  assert.equal(
    creationEvents.length,
    2,
    "adoption does not duplicate publication",
  );
  let closes = 0;
  win.on("close", (event) => {
    closes += 1;
    event.preventDefault();
  });
  fact({ variant: "CloseRequested", window: ref, close_seq: 7n });
  assert.equal(closes, 1);
  assert.deepEqual(
    calls.filter((call) => call.variant === "CloseReply"),
    [{ variant: "CloseReply", window: ref, close_seq: 7n, verdict: "Veto" }],
  );
  const contents = win.webContents;
  const terminal: string[] = [];
  contents.on("destroyed", () => terminal.push("contents"));
  win.on("closed", () => terminal.push("closed"));
  if (mode === "terminal-map-delete") poison(Map.prototype, "delete");
  if (mode === "terminal-weakmap-get") poison(WeakMap.prototype, "get");
  try {
    fact({ variant: "Destroyed", window: ref });
  } finally {
    if (mode.startsWith("terminal-")) restore();
  }
  assert.equal(win.isDestroyed(), true);
  if (mode === "dialog-parent") {
    assert.throws(() => readDialogParentRef(win), /KELD-COMPAT-011/);
  }
  assert.equal(contents.isDestroyed(), true);
  assert.deepEqual(terminal, ["contents", "closed"]);
  assert.deepEqual(BrowserWindow.getAllWindows(), []);
  assert.equal(
    calls.filter((call) => call.variant === "Destroy").length,
    0,
    "successful ownership never requires compensating native Destroy",
  );
}
restore();
console.log(`PASS ${mode}`);

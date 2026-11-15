/** Real API/facade/receipt path; only the authenticated Host CALL seam is controlled. */
import assert from "node:assert/strict";
import { LifecycleLink, type LifecycleHandler } from "../../api/src/link.ts";
import { WindowSource } from "../../api/src/window-source.ts";
import {
  encodeWindowEvent,
  type WindowEvent,
  type WindowRequest,
  type WindowResponse,
} from "../../api/src/window-codec.ts";

const mode = process.argv[2];
const boot = process.env.KELD_NATIVE_WINDOW_FACADE === "1";
const defineProperty = Object.defineProperty;
let restore = (): void => {};
let handlers: LifecycleHandler | undefined;
let quitCalls = 0;
const calls: WindowRequest[] = [];
const ref = { id: 71, incarnation: 8001n };
const state = {
  bounds: { x: 5, y: 10, width: 640, height: 480 },
  focused: false,
  maximized: false,
  full_screen: false,
};
const source = new WindowSource((event) => handlers?.onWindowFact?.(event));
function fact(event: WindowEvent, dispatch = true): void {
  const decoded = source.decode(encodeWindowEvent(event));
  source.apply(decoded);
  if (dispatch) source.dispatch(decoded);
}
const session = {
  windowSource: boot ? source : undefined,
  quit: async () => {
    quitCalls += 1;
  },
  close: () => {},
  consumeAppliedLastWindowClosed: () => false,
  callWindow: async (request: WindowRequest): Promise<WindowResponse> => {
    calls.push(request);
    return { variant: "ReplyApplied" };
  },
  callWindowBlocking: (request: WindowRequest): WindowResponse => {
    calls.push(request);
    if (request.variant === "Create")
      return { variant: "Adopted", window: ref };
    if (request.variant === "Destroy") {
      fact({ variant: "Destroyed", window: request.window }, false);
      return { variant: "Destroyed" };
    }
    throw new Error(`unexpected CALL ${request.variant}`);
  },
};
LifecycleLink.connect = (async (_link: string, observer: LifecycleHandler) => {
  handlers = observer;
  observer.onConnected?.(session as LifecycleLink);
  return session;
}) as typeof LifecycleLink.connect;
process.env.KELD_APP_LINK = `/unused-ownership-fixture#${"a".repeat(64)}`;
const { app: apiApp, getWindowSnapshots } =
  await import("../../api/src/app.ts");
const apiReady = apiApp.whenReady();
if (boot) fact({ variant: "Created", window: ref, state, pending_close: 7n });
assert.ok(handlers);
if (mode.startsWith("poison-")) {
  // The real API has already dispatched the Host replay; the facade has not
  // imported yet. Keep poisoning active through replay catch-up and teardown.
  handlers.onReady();
  await apiReady;
  const weak = mode.startsWith("poison-weakmap-");
  const name = mode.slice(
    weak ? "poison-weakmap-".length : "poison-map-".length,
  );
  const target =
    name === "constructor"
      ? globalThis
      : weak
        ? WeakMap.prototype
        : Map.prototype;
  const property = name === "constructor" ? (weak ? "WeakMap" : "Map") : name;
  const descriptor = Object.getOwnPropertyDescriptor(target, property);
  assert.ok(descriptor);
  defineProperty(target, property, {
    ...descriptor,
    value: () => {
      throw new Error(`hostile Map.${name}`);
    },
  });
  restore = () => defineProperty(target, property, descriptor);
} else if (mode === "native-delete-before-import") {
  delete process.env.KELD_NATIVE_WINDOW_FACADE;
} else if (mode === "native-zero-before-import") {
  process.env.KELD_NATIVE_WINDOW_FACADE = "0";
} else if (mode === "legacy-set-before-import") {
  process.env.KELD_NATIVE_WINDOW_FACADE = "1";
} else {
  throw new Error(`unknown mode ${mode}`);
}
const { app, BrowserWindow } = await import("./index.ts");
if (!mode.startsWith("poison-")) {
  assert.equal(app.isReady(), false);
  assert.deepEqual(
    getWindowSnapshots(),
    [],
    "pre-Ready census is deliberately hidden",
  );
  if (boot) {
    assert.equal(
      source.all()[0]?.window.id,
      ref.id,
      "the recovery survivor is still alive",
    );
    let refusal: unknown;
    await app.quit().catch((error: unknown) => {
      refusal = error;
    });
    assert.equal(
      quitCalls,
      0,
      "pre-import tampering cannot force Host teardown",
    );
    assert.match(String(refusal), /KELD-COMPAT-012/);
  } else {
    // The native launch selector is immutable in both directions. Legacy
    // contract remains the existing lazy Quit, despite a later forged hint.
    await app.quit();
    assert.equal(quitCalls, 1);
    restore();
    console.log(`PASS ${mode}`);
    process.exit(0);
  }
  handlers.onReady();
  await apiReady;
  await app.whenReady();
}
let allClosed = 0;
app.on("window-all-closed", () => {
  allClosed += 1;
});
const win = BrowserWindow.getAllWindows()[0];
assert.equal(win.id, ref.id);
let closeEvents = 0;
win.on("close", (event) => {
  closeEvents += 1;
  event.preventDefault();
});
await new Promise<void>((resolve) => setImmediate(resolve));
await new Promise<void>((resolve) => setImmediate(resolve));
assert.equal(
  closeEvents,
  1,
  "late facade import must deliver the held recovery close",
);
assert.deepEqual(
  calls.filter((call) => call.variant === "CloseReply"),
  [{ variant: "CloseReply", window: ref, close_seq: 7n, verdict: "Veto" }],
);
assert.equal(
  source.get(ref)?.pending_close,
  null,
  "real API settles the exact Host acknowledgement",
);
assert.equal(win.isDestroyed(), false);
const creation: unknown[] = [];
app.on("browser-window-created", () => {
  throw new Error("application creation listener");
});
app.on("browser-window-created", (...args) => creation.push(args[1]));
assert.equal(
  new BrowserWindow(),
  win,
  "same adopted Host pair preserves the veto wrapper",
);
assert.equal(creation.length, 1);
assert.equal(creation[0], win);
await assert.rejects(app.quit(), /KELD-COMPAT-012/);
assert.equal(quitCalls, 0, "live vetoed survivor cannot be force-quit");
const contents = win.webContents;
let closed = 0;
let released = 0;
win.on("closed", () => {
  closed += 1;
});
contents.on("destroyed", () => {
  released += 1;
});
win.destroy();
await Promise.resolve();
assert.equal(win.isDestroyed(), true);
assert.equal(contents.isDestroyed(), true);
assert.equal(closed, 1);
assert.equal(released, 1);
assert.deepEqual(BrowserWindow.getAllWindows(), []);
handlers.onLastWindowClosed();
assert.equal(
  allClosed,
  1,
  "real API and facade listener registries deliver final closure",
);
assert.equal(
  quitCalls,
  0,
  "an enrolled all-closed listener keeps the role alive",
);
restore();
console.log(`PASS ${mode}`);

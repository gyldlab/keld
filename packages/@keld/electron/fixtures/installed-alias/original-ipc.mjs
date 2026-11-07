/**
 * Installed-package composition with a controlled Host-response seam. The real
 * API, window mirror, BrowserWindow and IPC event capability execute here.
 * These supplied facts do not establish native destruction or authentication.
 */
import assert from "node:assert/strict";

const [mode, ...extra] = process.argv.slice(2);
assert.equal(extra.length, 0);
assert.ok(mode === undefined || mode === "missing-destroyed");
process.env.KELD_NATIVE_WINDOW_FACADE = "1";
process.env.KELD_APP_LINK =
  "/controlled-installed-original-ipc#" + "a".repeat(64);

const apiEntry = import.meta.resolve("@keld/api/internal/app");
assert.match(apiEntry, /\/node_modules\/@keld\/api\/app\.js$/);
assert.match(
  import.meta.resolve("electron/internal/original-ipc"),
  /\/node_modules\/electron\/dist\/original-ipc\.js$/,
);
const { LifecycleLink } = await import("@keld/api/internal/link");
// Use the actual packed mirror beside its owning API entry, not source files.
const { WindowSource } = await import(
  new URL("./window-source.js", apiEntry).href
);
const { encodeWindowEvent } = await import("@keld/api/internal/window-codec");
const { encodeEndpointArguments, decodeEndpointArguments } =
  await import("@keld/kipc/endpoint-value-codec");
const { nativeFacadeBootSelected, hostLaunchFacts } =
  await import("@keld/kipc");
assert.equal(nativeFacadeBootSelected, true);
assert.equal(hostLaunchFacts, undefined);

const ref = Object.freeze({ id: 7, incarnation: 258n });
const url = "keld-app://actual/index.html";
const origin = "keld-app://actual";
let handlers;
let connects = 0;
let open = true;
let finishVeto;
let finishDestroy;
let enterDestroy;
const destroyEntered = new Promise((resolve) => {
  enterDestroy = resolve;
});
const windowRequests = [];
const replies = [];
const source = new WindowSource((fact) => handlers.onWindowFact(fact));
function observe(fact) {
  const decoded = source.decode(encodeWindowEvent(fact));
  source.apply(decoded);
  source.dispatch(decoded);
}
function current() {
  const snapshot = source.get(ref);
  return (
    open &&
    snapshot !== undefined &&
    !snapshot.host_destroyed &&
    snapshot.document?.generation === 2 &&
    snapshot.document.current &&
    snapshot.document.finished
  );
}

const session = {
  windowSource: source,
  get windowAdmissionOpen() {
    return open;
  },
  close() {
    open = false;
  },
  quit() {
    throw new Error("unexpected Quit in original IPC fixture");
  },
  consumeAppliedLastWindowClosed: () => false,
  callWindowBlocking() {
    throw new Error("replayed fixture window must not issue Create or Destroy");
  },
  callWindow(request) {
    windowRequests.push(request);
    if (request.variant === "CloseReply") {
      assert.equal(finishVeto, undefined);
      assert.deepEqual(request, {
        variant: "CloseReply",
        window: ref,
        close_seq: 3n,
        verdict: "Veto",
      });
      return new Promise((resolve) => {
        finishVeto = resolve;
      });
    }
    assert.equal(request.variant, "DestroyOriginalDocument");
    assert.equal(finishDestroy, undefined);
    return new Promise((resolve) => {
      finishDestroy = resolve;
      enterDestroy();
    });
  },
  callCompat(request, eligible) {
    assert.equal(eligible(), true);
    replies.push(request);
    return Promise.resolve();
  },
};
const originalConnect = LifecycleLink.connect;
LifecycleLink.connect = async (_address, supplied) => {
  connects += 1;
  handlers = supplied;
  handlers.onConnected(session);
  return session;
};
const timeout = setTimeout(() => {
  throw new Error("installed original IPC fixture timed out");
}, 5000);

try {
  const api = await import("@keld/api/internal/app");
  const ready = api.app.whenReady();
  assert.equal(connects, 1);
  assert.ok(handlers);
  observe({
    variant: "Created",
    window: ref,
    pending_close: null,
    state: {
      bounds: { x: 12, y: 34, width: 640, height: 480 },
      focused: false,
      maximized: false,
      full_screen: false,
    },
  });
  observe({
    variant: "NavigationCommitted",
    window: ref,
    document_generation: 2,
    native_url: url,
  });
  observe({
    variant: "NavigationFinished",
    window: ref,
    document_generation: 2,
  });
  handlers.onReady();
  await ready;
  assert.equal(api.app.isReady(), true);

  // Loading the private entry first must still share the public facade's map.
  const internal = await import("electron/internal/original-ipc");
  const electron = await import("electron");
  assert.deepEqual(Object.keys(internal).sort(), [
    "awaitOriginalIpcCloseVeto",
    "consumeOriginalRecoveryStartup",
    "destroyOriginalIpcDocument",
    "isCurrentOriginalIpcDocument",
  ]);
  assert.equal(typeof internal.consumeOriginalRecoveryStartup, "function");
  assert.strictEqual(electron.ipcMain, electron.default.ipcMain);
  const windows = electron.BrowserWindow.getAllWindows();
  assert.equal(windows.length, 1);
  const window = windows[0];
  assert.equal(window.id, ref.id);
  assert.equal(window.isDestroyed(), false);
  for (const candidate of [undefined, null, 7, {}, Object.create(window)]) {
    assert.equal(internal.consumeOriginalRecoveryStartup(candidate), false);
  }
  assert.equal(
    internal.consumeOriginalRecoveryStartup(window),
    false,
    "replayed fixture has no captured original recovery startup decision",
  );
  const contents = window.webContents;
  let closed = 0;
  window.once("closed", () => {
    closed += 1;
  });
  window.once("close", (event) => event.preventDefault());

  let event;
  let argument;
  electron.ipcMain.once("rendererReq", (received, value) => {
    event = received;
    argument = value;
  });
  const message = {
    variant: "RendererMessage",
    webview: 1,
    navigation: 2,
    message: 3,
    window_id: 7,
    window_incarnation: 258n,
    url,
    origin,
    topic: "rendererReq",
    payload: encodeEndpointArguments([{ action: "isFullscreen", reqId: 73 }]),
  };
  assert.deepEqual(handlers.onCompatEvent(message, current), {
    variant: "Consumed",
  });
  assert.ok(event);
  assert.equal(electron.ipcMain.listenerCount("rendererReq"), 0);
  assert.strictEqual(event.sender, contents);
  assert.deepEqual(event.senderFrame, { url, origin });
  assert.deepEqual(argument, { action: "isFullscreen", reqId: 73 });
  assert.equal(internal.isCurrentOriginalIpcDocument(event), true);
  for (const forged of [{}, Object.create(event)]) {
    assert.equal(internal.isCurrentOriginalIpcDocument(forged), false);
    await assert.rejects(internal.awaitOriginalIpcCloseVeto(forged), {
      code: "KELD-CORE-054",
    });
    await assert.rejects(internal.destroyOriginalIpcDocument(forged), {
      code: "KELD-CORE-054",
    });
  }
  assert.deepEqual(windowRequests, []);

  event.reply("mainResp", { success: true, data: false, reqId: 73 });
  assert.equal(replies.length, 1);
  assert.deepEqual(replies[0], {
    variant: "DeliverToOriginalDocument",
    webview: 1,
    navigation: 2,
    topic: "mainResp",
    payload: encodeEndpointArguments([
      { success: true, data: false, reqId: 73 },
    ]),
  });
  assert.deepEqual(decodeEndpointArguments(replies[0].payload), [
    { success: true, data: false, reqId: 73 },
  ]);
  message.navigation = 999;
  message.window_id = 99;
  message.window_incarnation = 999n;

  observe({ variant: "CloseRequested", window: ref, close_seq: 3n });
  assert.equal(typeof finishVeto, "function");
  const veto = internal.awaitOriginalIpcCloseVeto(event);
  assert.strictEqual(internal.awaitOriginalIpcCloseVeto(event), veto);
  let vetoSettled = false;
  void veto.then(() => {
    vetoSettled = true;
  });
  const teardown = internal.destroyOriginalIpcDocument(event);
  assert.strictEqual(internal.destroyOriginalIpcDocument(event), teardown);
  await Promise.resolve();
  assert.equal(
    vetoSettled,
    false,
    "local close request is not its Host Veto receipt",
  );
  assert.equal(
    windowRequests.length,
    1,
    "destruction must wait for actual Veto settlement",
  );
  assert.equal(source.get(ref).pending_close, 3n);
  finishVeto({ variant: "ReplyApplied" });
  await veto;
  await destroyEntered;
  assert.equal(source.get(ref).pending_close, null);
  assert.equal(windowRequests.length, 2);
  assert.deepEqual(windowRequests[1], {
    variant: "DestroyOriginalDocument",
    window: { id: 7, incarnation: 258n },
    document_generation: 2,
  });
  assert.equal(Object.isFrozen(windowRequests[1]), true);
  assert.equal(Object.isFrozen(windowRequests[1].window), true);
  assert.equal(window.isDestroyed(), false);
  assert.equal(internal.isCurrentOriginalIpcDocument(event), true);

  // The negative changes only this supplied physical observation. The actual
  // BrowserWindow owner must reject the reply-only completion itself.
  if (mode !== "missing-destroyed")
    observe({ variant: "Destroyed", window: ref });
  finishDestroy({ variant: "Destroyed" });
  await teardown;
  assert.equal(window.isDestroyed(), true);
  assert.equal(contents.isDestroyed(), true);
  assert.throws(() => window.webContents, /KELD-COMPAT-011/);
  assert.equal(closed, 1);
  assert.equal(source.get(ref), undefined);
  assert.equal(internal.isCurrentOriginalIpcDocument(event), false);
  assert.equal(event.senderFrame, null);
  event.reply("mainResp", { reqId: 73 });
  assert.equal(replies.length, 1, "retired original event must not send again");
  assert.strictEqual(internal.destroyOriginalIpcDocument(event), teardown);
  assert.equal(windowRequests.length, 2, "same-event teardown never resends");
  assert.equal(connects, 1);
  console.log("INSTALLED_ORIGINAL_IPC_SHARED_CAPABILITY");
} finally {
  clearTimeout(timeout);
  LifecycleLink.connect = originalConnect;
}

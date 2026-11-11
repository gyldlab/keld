/** Local dispatch semantics only; no native/Host provenance or grants are proved. */
import { expect, test } from "bun:test";
import {
  encodeEndpointValue,
  decodeEndpointValue,
} from "@keld/kipc/endpoint-value-codec";
import type {
  CompatControlEvent,
  CompatControlRequest,
} from "@keld/kipc/compat-control";
import {
  ipcMain,
  installIpcMainDelivery,
  destroyOriginalIpcDocument,
  awaitOriginalIpcCloseVeto,
  isCurrentOriginalIpcDocument,
  type IpcMainEvent,
} from "./ipc-main.ts";
import type { WebContents } from "./browser-window.ts";

let consume:
  ((event: CompatControlEvent, current: () => boolean) => unknown) | undefined;
let live = true;
let pair = { id: 7, incarnation: 258n };
const sent: CompatControlRequest[] = [];
const destroyed: {
  request: {
    variant: "DestroyOriginalDocument";
    window: { id: number; incarnation: bigint };
    document_generation: number;
  };
  current: () => boolean;
}[] = [];
const vetoWaits: {
  window: { id: number; incarnation: bigint };
  current: () => boolean;
}[] = [];
let vetoResult: Promise<void> = Promise.resolve();
let destroyResult: Promise<void> = Promise.resolve();
const sender = { isDestroyed: () => false } as WebContents;
installIpcMainDelivery({
  register(consumer) {
    consume = consumer;
  },
  findSender(ref) {
    return ref.id === pair.id && ref.incarnation === pair.incarnation
      ? sender
      : undefined;
  },
  send(request) {
    sent.push(request);
    return Promise.resolve();
  },
  destroyOriginalDocument(request, current) {
    destroyed.push({ request, current });
    return destroyResult;
  },
  awaitCloseVeto(window, current) {
    vetoWaits.push({ window, current });
    return vetoResult;
  },
  report() {},
});
function message(): CompatControlEvent {
  return {
    variant: "RendererMessage",
    webview: 1,
    navigation: 2,
    message: 3,
    window_id: 7,
    window_incarnation: 258n,
    url: "keld-app://actual/index.html",
    origin: "keld-app://actual",
    topic: "rendererReq",
    payload: encodeEndpointValue([{ action: "isFullscreen", reqId: 73 }]),
  };
}

test("actual shared registry receives decoded values and replies to captured original document", () => {
  live = true;
  sent.length = 0;
  let seen: IpcMainEvent | undefined;
  ipcMain.once("rendererReq", (event, arg) => {
    seen = event;
    expect(event.sender).toBe(sender);
    expect(event.senderFrame).toEqual({
      url: "keld-app://actual/index.html",
      origin: "keld-app://actual",
    });
    expect(arg).toEqual({ action: "isFullscreen", reqId: 73 });
    event.reply("mainResp", { success: true, data: false, reqId: 73 });
  });
  expect(consume!(message(), () => live)).toEqual({ variant: "Consumed" });
  expect(ipcMain.listenerCount("rendererReq")).toBe(0);
  expect(sent).toHaveLength(1);
  const response = sent[0]!;
  expect(response).toMatchObject({
    variant: "DeliverToOriginalDocument",
    webview: 1,
    navigation: 2,
    topic: "mainResp",
  });
  if (response.variant !== "ReleaseConsumed")
    expect(decodeEndpointValue(response.payload)).toEqual([
      { success: true, data: false, reqId: 73 },
    ]);
  live = false;
  expect(seen!.senderFrame).toBeNull();
  seen!.reply("mainResp", { reqId: 73 });
  expect(sent).toHaveLength(1);
  expect(() => seen!.processId).toThrow("KELD-COMPAT-013");
});

test("wrong pair and stale original perform zero callback/send; async callback return is no reply", async () => {
  sent.length = 0;
  live = true;
  let calls = 0;
  ipcMain.on("rendererReq", async () => {
    calls += 1;
    return true;
  });
  pair = { id: 7, incarnation: 259n };
  expect(consume!(message(), () => live)).toEqual({ variant: "NoListener" });
  pair = { id: 7, incarnation: 258n };
  expect(consume!(message(), () => false)).toEqual({ variant: "NoListener" });
  expect(calls).toBe(0);
  expect(sent).toEqual([]);
  consume!(message(), () => true);
  await Promise.resolve();
  expect(calls).toBe(1);
  expect(sent).toEqual([]);
  ipcMain.removeAllListeners("rendererReq");
});

test("reply after an actual async listener continuation cannot target a retired document", async () => {
  live = true;
  sent.length = 0;
  let release: () => void = () => undefined;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  ipcMain.once("rendererReq", async (event) => {
    await held;
    event.reply("mainResp", { reqId: 73 });
  });
  consume!(message(), () => live);
  live = false;
  release();
  await held;
  await Promise.resolve();
  expect(sent).toEqual([]);
});

test("private original teardown retains immutable A selector through app mutation and coalesces while physical ACK is held", async () => {
  live = true;
  destroyed.length = 0;
  let event: IpcMainEvent | undefined;
  const original = message();
  let finish!: () => void;
  destroyResult = new Promise<void>((resolve) => {
    finish = resolve;
  });
  ipcMain.once("rendererReq", (value) => {
    event = value;
  });
  consume!(original, () => live);
  if (original.variant === "RendererMessage") {
    original.navigation = 999;
    original.window_id = 99;
    original.window_incarnation = 999n;
  }
  const pending = destroyOriginalIpcDocument(event!);
  expect(destroyOriginalIpcDocument(event!)).toBe(pending);
  expect(destroyed).toHaveLength(1);
  expect(destroyed[0]!.request).toEqual({
    variant: "DestroyOriginalDocument",
    window: { id: 7, incarnation: 258n },
    document_generation: 2,
  });
  expect(Object.isFrozen(destroyed[0]!.request)).toBe(true);
  expect(Object.isFrozen(destroyed[0]!.request.window)).toBe(true);
  let complete = false;
  void pending.then(() => {
    complete = true;
  });
  await Promise.resolve();
  expect(complete).toBe(false);
  // Successful destruction itself retires A; its authentic physical ACK may
  // settle after current() becomes false and must not become a fake stale error.
  live = false;
  finish();
  await pending;
  expect(complete).toBe(true);
});
test("retained A event cannot acquire B's same-window generation and forged events delegate nothing", async () => {
  destroyed.length = 0;
  live = true;
  let a: IpcMainEvent | undefined, b: IpcMainEvent | undefined;
  let originalCurrent = true;
  ipcMain.once("rendererReq", (value) => {
    a = value;
  });
  consume!(message(), () => originalCurrent);
  originalCurrent = false;
  const successor = message();
  if (successor.variant === "RendererMessage") successor.navigation = 3;
  ipcMain.once("rendererReq", (value) => {
    b = value;
  });
  consume!(successor, () => true);
  await expect(destroyOriginalIpcDocument(a!)).rejects.toMatchObject({
    code: "KELD-CORE-054",
  });
  await expect(
    destroyOriginalIpcDocument(Object.create(a!) as IpcMainEvent),
  ).rejects.toMatchObject({ code: "KELD-CORE-054" });
  expect(destroyed).toEqual([]);
  destroyResult = Promise.resolve();
  await destroyOriginalIpcDocument(b!);
  expect(destroyed).toHaveLength(1);
  expect(destroyed[0]!.request.document_generation).toBe(3);
});
test("changed WindowRef incarnation and stale original callback admit zero scoped destruction", async () => {
  destroyed.length = 0;
  live = true;
  let event: IpcMainEvent | undefined;
  ipcMain.once("rendererReq", (value) => {
    event = value;
  });
  consume!(message(), () => live);
  pair = { id: 7, incarnation: 259n };
  await expect(destroyOriginalIpcDocument(event!)).rejects.toMatchObject({
    code: "KELD-CORE-054",
  });
  expect(destroyed).toEqual([]);
  pair = { id: 7, incarnation: 258n };
});
test("post-admission failure retains exact unknown outcome and never retries original event", async () => {
  destroyed.length = 0;
  live = true;
  let event: IpcMainEvent | undefined;
  const unknown = new Error("KELD-IPC-006: outcome unknown after admission");
  ipcMain.once("rendererReq", (value) => {
    event = value;
  });
  consume!(message(), () => live);
  destroyResult = Promise.reject(unknown);
  const failed = destroyOriginalIpcDocument(event!);
  await expect(failed).rejects.toBe(unknown);
  expect(destroyOriginalIpcDocument(event!)).toBe(failed);
  expect(destroyed).toHaveLength(1);
});

test("original event capability and frozen selector use existing captured intrinsics after app mutation", async () => {
  destroyed.length = 0;
  live = true;
  destroyResult = Promise.resolve();
  let event: IpcMainEvent | undefined;
  const freeze = Object.getOwnPropertyDescriptor(Object, "freeze")!;
  const get = Object.getOwnPropertyDescriptor(WeakMap.prototype, "get")!,
    set = Object.getOwnPropertyDescriptor(WeakMap.prototype, "set")!;
  try {
    Object.defineProperty(Object, "freeze", {
      ...freeze,
      value() {
        throw new Error("poison freeze");
      },
    });
    Object.defineProperty(WeakMap.prototype, "get", {
      ...get,
      value() {
        throw new Error("poison weak get");
      },
    });
    Object.defineProperty(WeakMap.prototype, "set", {
      ...set,
      value() {
        throw new Error("poison weak set");
      },
    });
    ipcMain.once("rendererReq", (value) => {
      event = value;
    });
    consume!(message(), () => live);
    await destroyOriginalIpcDocument(event!);
  } finally {
    Object.defineProperty(Object, "freeze", freeze);
    Object.defineProperty(WeakMap.prototype, "get", get);
    Object.defineProperty(WeakMap.prototype, "set", set);
  }
  expect(destroyed).toHaveLength(1);
  expect(Object.isFrozen(destroyed[0]!.request)).toBe(true);
  expect(Object.isFrozen(destroyed[0]!.request.window)).toBe(true);
});

test("current predicate uses the same genuine retained capability and no exposed event getters", async () => {
  live = true;
  let event: IpcMainEvent | undefined;
  ipcMain.once("rendererReq", (value) => {
    event = value;
  });
  consume!(message(), () => live);
  expect(isCurrentOriginalIpcDocument(event!)).toBe(true);
  const forged = Object.create(event!);
  expect(isCurrentOriginalIpcDocument(forged)).toBe(false);
  expect(isCurrentOriginalIpcDocument({} as IpcMainEvent)).toBe(false);
  live = false;
  expect(isCurrentOriginalIpcDocument(event!)).toBe(false);
  await expect(awaitOriginalIpcCloseVeto(forged)).rejects.toMatchObject({
    code: "KELD-CORE-054",
  });
});
test("original veto wait shares the held owner verdict, never local completion or destruction", async () => {
  live = true;
  vetoWaits.length = 0;
  destroyed.length = 0;
  let event: IpcMainEvent | undefined, finish!: () => void;
  vetoResult = new Promise<void>((resolve) => {
    finish = resolve;
  });
  ipcMain.once("rendererReq", (value) => {
    event = value;
  });
  consume!(message(), () => live);
  const waiting = awaitOriginalIpcCloseVeto(event!);
  expect(awaitOriginalIpcCloseVeto(event!)).toBe(waiting);
  let settled = false;
  void waiting.then(() => {
    settled = true;
  });
  await Promise.resolve();
  expect(settled).toBe(false);
  expect(vetoWaits).toHaveLength(1);
  expect(destroyed).toEqual([]);
  finish();
  await waiting;
  expect(settled).toBe(true);
  expect(vetoWaits[0]!.window).toEqual({ id: 7, incarnation: 258n });
  // A later same-document attempt must consult the owning current close attempt,
  // never reuse this event capability's already-resolved old verdict.
  vetoResult = Promise.resolve();
  await awaitOriginalIpcCloseVeto(event!);
  expect(vetoWaits).toHaveLength(2);
});
test("original veto continuation rechecks retirement after the actual async owner settlement", async () => {
  live = true;
  vetoWaits.length = 0;
  destroyed.length = 0;
  let event: IpcMainEvent | undefined, finish!: () => void;
  vetoResult = new Promise<void>((resolve) => {
    finish = resolve;
  });
  ipcMain.once("rendererReq", (value) => {
    event = value;
  });
  consume!(message(), () => live);
  const waiting = awaitOriginalIpcCloseVeto(event!);
  live = false;
  finish();
  await expect(waiting).rejects.toMatchObject({ code: "KELD-CORE-054" });
  expect(destroyed).toEqual([]);
  expect(isCurrentOriginalIpcDocument(event!)).toBe(false);
});
test("private export shares the three existing IPC capabilities and the original recovery owner", async () => {
  const internal = await import("./original-ipc.ts");
  expect(Object.keys(internal).sort()).toEqual([
    "awaitOriginalIpcCloseVeto",
    "consumeOriginalRecoveryStartup",
    "destroyOriginalIpcDocument",
    "isCurrentOriginalIpcDocument",
  ]);
  expect(internal.destroyOriginalIpcDocument).toBe(destroyOriginalIpcDocument);
  expect(internal.awaitOriginalIpcCloseVeto).toBe(awaitOriginalIpcCloseVeto);
  expect(internal.isCurrentOriginalIpcDocument).toBe(
    isCurrentOriginalIpcDocument,
  );
  const { BrowserWindow, consumeOriginalRecoveryStartup } =
    await import("./browser-window.ts");
  expect(internal.consumeOriginalRecoveryStartup).toBe(
    consumeOriginalRecoveryStartup,
  );
  for (const forged of [
    undefined,
    null,
    7,
    {},
    Object.create(BrowserWindow.prototype),
  ]) {
    expect(internal.consumeOriginalRecoveryStartup(forged)).toBe(false);
  }
  // This local delivery fixture has no authenticated original recovery
  // document; an actual IPC event cannot impersonate a BrowserWindow.
  live = true;
  let observed: IpcMainEvent | undefined;
  ipcMain.once("rendererReq", (event) => {
    observed = event;
  });
  expect(consume!(message(), () => live)).toEqual({ variant: "Consumed" });
  expect(observed).toBeDefined();
  expect(internal.consumeOriginalRecoveryStartup(observed)).toBe(false);
});

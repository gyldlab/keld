/** One ordinary release refusal on a controlled transport, with no socket or native effect. */
import assert from "node:assert/strict";
import { mock } from "bun:test";
import * as transport from "@keld/kipc";
import { encodeCompatControlEvent } from "@keld/kipc/compat-control";
import { encodeWindowEvent } from "./window-codec.ts";

const appliers = new Map<number, (payload: Uint8Array) => void>();
const listeners = new Map<number, (payload: Uint8Array) => void>();
const refusal = transport.linkError(
  "KELD-IPC-003",
  "fixture release could not be confirmed",
);
const closeCause = transport.linkError("KELD-IPC-001", "transport closed");
let closes = 0;
let requests = 0;
let notifyEnd: ((cause: typeof closeCause) => void) | undefined;
const worker = {
  stateFactCapacity: 16,
  setStateApplier(channel: number, handler: (payload: Uint8Array) => void) {
    appliers.set(channel, handler);
  },
  onEvent(channel: number, handler: (payload: Uint8Array) => void) {
    listeners.set(channel, handler);
  },
  onEventAfterRingRelease(
    channel: number,
    dispatch: (payload: Uint8Array) => () => void,
  ) {
    listeners.set(channel, (payload) => dispatch(payload)());
  },
  setCallHandler() {},
  onEnd(handler: typeof notifyEnd) {
    notifyEnd = handler;
  },
  async call(channel: number, payload: Uint8Array) {
    if (channel === transport.LIFECYCLE_CHANNEL) return Uint8Array.of(1);
    assert.equal(channel, transport.COMPAT_CONTROL_CHANNEL);
    assert.deepEqual(payload, Uint8Array.of(2, 1, 2, 1, 0));
    requests++;
    throw refusal;
  },
  close() {
    closes++;
    notifyEnd?.(closeCause);
  },
  get callAdmissionOpen() {
    return closes === 0;
  },
};
mock.module("@keld/kipc", () => ({
  ...transport,
  WorkerLink: { open: async () => worker },
}));
assert.equal(transport.nativeFacadeBootSelected, true);
const { LifecycleLink } = await import("./link.ts");

const terminal: Error[] = [];
const reports: unknown[] = [];
let observed = 0;
const session = await LifecycleLink.connect("controlled-link", {
  onReady() {},
  onLastWindowClosed() {},
  onCompatEvent() {
    observed++;
    return { variant: "Consumed" };
  },
  onLinkDead(error) {
    terminal.push(error);
  },
  onCompatError(error) {
    reports.push(error);
  },
});
const window = { id: 7, incarnation: 258n };
const route = "keld-app://actual/index.html";
function deliver(channel: number, bytes: Uint8Array): void {
  assert(appliers.has(channel));
  assert(listeners.has(channel));
  appliers.get(channel)!(bytes);
  listeners.get(channel)!(bytes);
}
deliver(
  transport.WINDOW_CHANNEL,
  encodeWindowEvent({
    variant: "Created",
    window,
    state: {
      bounds: { x: 0, y: 0, width: 640, height: 480 },
      focused: true,
      maximized: false,
      full_screen: false,
    },
    pending_close: null,
  }),
);
deliver(
  transport.WINDOW_CHANNEL,
  encodeWindowEvent({
    variant: "NavigationCommitted",
    window,
    document_generation: 2,
    native_url: route,
  }),
);
deliver(
  transport.COMPAT_CONTROL_CHANNEL,
  encodeCompatControlEvent({
    variant: "RendererMessage",
    webview: 1,
    navigation: 2,
    message: 1,
    window_id: window.id,
    window_incarnation: window.incarnation,
    url: route,
    origin: "keld-app://actual",
    topic: "ordinary-status",
    payload: Uint8Array.of(0),
  }),
);
await new Promise<void>((resolve) => setImmediate(resolve));
assert.equal(observed, 1);
assert.equal(requests, 1);
assert.equal(closes, 1);
assert.equal(terminal.length, 1, "release failure must notify the link owner");
assert.equal(terminal[0], refusal);
assert.equal(reports[0], refusal);
await assert.rejects(
  session.callFs(Uint8Array.of(0)),
  (err) => err === refusal,
);
await assert.rejects(session.quit(), (err) => err === refusal);
console.log("PASS compat release terminal cause");

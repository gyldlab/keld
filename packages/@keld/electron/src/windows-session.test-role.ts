/** GH553 source conformance over the real API Ready path and a controlled Host link. */
import assert from "node:assert/strict";
import * as host from "@keld/kipc";

const mode = process.argv[2];
let applyDesktop: ((payload: Uint8Array) => void) | undefined;
let rawReady: ((payload: Uint8Array) => void) | undefined;
let calls = 0;
const worker = {
  stateFactCapacity: 16,
  setStateApplier(channel: number, callback: (payload: Uint8Array) => void) {
    if (channel === host.WINDOW_CHANNEL) applyDesktop = callback;
  },
  onEvent(channel: number, callback: (payload: Uint8Array) => void) {
    if (channel === host.LIFECYCLE_CHANNEL) rawReady = callback;
  },
  setCallHandler() {},
  onEnd() {},
  async call(
    channel: number,
    payload: Uint8Array,
    deadline: number,
    beforeReply?: () => void,
  ) {
    calls += 1;
    assert.equal(channel, host.LIFECYCLE_CHANNEL);
    assert.deepEqual([...payload], [1, host.WINDOW_CHANNEL]);
    assert.equal(deadline, 30_000);
    applyDesktop!(
      Uint8Array.from(
        Buffer.from(
          "07ac02011101110000a00b84070030a00bec06000000000000004000",
          "hex",
        ),
      ),
    );
    beforeReply?.();
    rawReady!(Uint8Array.of(0));
    return Uint8Array.of(1);
  },
};
host.WorkerLink.open = async () => worker as unknown as host.WorkerLink;
const diagnostics: string[] = [];
console.warn = (message: string) => {
  diagnostics.push(message);
};

const { app, session, default: alias } = await import("./index.ts");
const refused = (error: unknown) =>
  host.isCallError(error) && error.code === "KELD-COMPAT-013";
assert.equal(session, alias.session);
assert.throws(() => session.defaultSession, refused);
await app.whenReady();
assert.equal(app.isReady(), true);

let invoked = 0;
const listener = () => {
  invoked += 1;
};
if (mode === "wrong-profile") {
  assert.throws(() => session.defaultSession, refused);
} else {
  const selected = session.defaultSession;
  assert.equal(session.defaultSession, selected);
  assert.ok(Object.isFrozen(selected.webRequest));
  const requests = selected.webRequest;
  if (mode === "missing-policy") {
    assert.throws(() => requests.onHeadersReceived(listener), refused);
    assert.throws(
      () => requests.onBeforeRequest({ urls: ["file://*"] }, listener),
      refused,
    );
  } else if (mode === "invalidated") {
    applyDesktop!(Uint8Array.of(7, 0xad, 2, 0));
    assert.throws(
      () => requests.onHeadersReceived(listener),
      (error) => host.isCallError(error) && error.code === "KELD-CORE-049",
    );
  } else {
    assert.equal(requests.onHeadersReceived(listener), undefined);
    assert.equal(
      requests.onBeforeRequest({ urls: ["file://*"] }, listener),
      undefined,
    );
    assert.equal(requests.onBeforeRequest(null), undefined);
    let reads = 0;
    const getter = {
      get urls() {
        reads += 1;
        return [];
      },
    };
    const proxy = new Proxy(
      {},
      {
        ownKeys() {
          reads += 1;
          return [];
        },
      },
    );
    const sparse = new Array<string>(1);
    for (const input of [
      getter,
      proxy,
      { urls: sparse },
      { urls: ["file://*"], types: [] },
    ])
      assert.throws(
        () =>
          Reflect.apply(requests.onBeforeRequest, requests, [input, listener]),
        refused,
      );
    assert.deepEqual(
      diagnostics.map(
        (line) => line.match(/on(?:HeadersReceived|BeforeRequest)/)?.[0],
      ),
      ["onHeadersReceived", "onBeforeRequest", "onBeforeRequest"],
    );
    for (const method of [
      "onBeforeSendHeaders",
      "onSendHeaders",
      "onResponseStarted",
      "onBeforeRedirect",
      "onCompleted",
      "onErrorOccurred",
    ] as const)
      assert.throws(() => requests[method](), refused);
    assert.equal(diagnostics.length, 3);
    assert.equal(
      reads,
      0,
      "malformed registration must not evaluate getters or Proxy traps",
    );
  }
}
assert.equal(invoked, 0);
assert.equal(
  calls,
  1,
  "registrations issue zero IPC after the initial subscription",
);
if (mode !== "selected") assert.deepEqual(diagnostics, []);

/** Real LifecycleLink and state applier driven by a controlled WorkerLink. */
import assert from "node:assert/strict";
import { mock } from "bun:test";
import * as transport from "@keld/kipc";

const mode = process.argv[2];
const selected = mode !== "unselected" && mode !== "foreign-host";
const appliers = new Map<number, (payload: Uint8Array) => void>();
const listeners = new Map<number, (payload: Uint8Array) => void>();
let ready = 0;
let close = 0;
let calls = 0;
let dead: string | undefined;
const worker = {
  stateFactCapacity: 16,
  setStateApplier(channel: number, handler: (payload: Uint8Array) => void) {
    appliers.set(channel, handler);
  },
  onEvent(channel: number, handler: (payload: Uint8Array) => void) {
    listeners.set(channel, handler);
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
    assert.equal(channel, transport.LIFECYCLE_CHANNEL);
    assert.deepEqual([...payload], [1, transport.WINDOW_CHANNEL]);
    assert.equal(
      deadline,
      selected ? 30_000 : transport.APP_LINK_IO_DEADLINE_MS,
    );
    if (mode !== "late-ready" && mode !== "public-ready")
      listeners.get(transport.LIFECYCLE_CHANNEL)!(Uint8Array.of(0));
    assert.equal(ready, 0, "raw Ready cannot outrun Subscribe acknowledgement");
    if (
      mode === "valid" ||
      mode === "invalidated" ||
      mode === "late-ready" ||
      mode === "public-ready"
    )
      appliers.get(transport.WINDOW_CHANNEL)!(
        Uint8Array.from(
          Buffer.from(
            "07ac02011101110000a00b84070030a00bec06000000000000004000",
            "hex",
          ),
        ),
      );
    if (mode === "unavailable")
      throw transport.linkError("KELD-CORE-049", "native snapshot unavailable");
    beforeReply?.();
    if (mode === "invalidated") invalidate();
    return Uint8Array.of(1);
  },
  close() {
    close += 1;
  },
};
function invalidate() {
  appliers.get(transport.WINDOW_CHANNEL)!(Uint8Array.of(7, 0xad, 2, 0));
}
mock.module("@keld/kipc", () => ({
  ...transport,
  WorkerLink: { open: async () => worker },
}));
if (mode === "public-ready") {
  process.env.KELD_APP_LINK = "controlled-link";
  const { app, hasObservedHostReady } = await import("./app.ts");
  const pending = app.whenReady();
  void pending.catch(() => {});
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(calls, 1);
  assert.equal(app.isReady(), false);
  listeners.get(transport.LIFECYCLE_CHANNEL)!(Uint8Array.of(0));
  assert.equal(hasObservedHostReady(), true);
  assert.equal(app.isReady(), false);
  invalidate();
  await assert.rejects(
    pending,
    (error) => transport.isCallError(error) && error.code === "KELD-IPC-005",
  );
  assert.equal(app.isReady(), false);
  assert.equal(close, 1);
  process.exit(0);
}
const { LifecycleLink } = await import("./link.ts");
const connecting = LifecycleLink.connect("controlled-link", {
  onReady() {
    ready += 1;
  },
  onLastWindowClosed() {},
  onLinkDead(error) {
    assert.ok(transport.isCallError(error));
    dead = error.code;
  },
});
if (mode === "missing" || mode === "invalidated" || mode === "unavailable") {
  const code = mode === "unavailable" ? "KELD-CORE-049" : "KELD-IPC-005";
  await assert.rejects(
    connecting,
    (error) => transport.isCallError(error) && error.code === code,
  );
  assert.equal(close, 1);
  assert.equal(ready, 0);
} else if (mode === "late-ready") {
  await connecting;
  assert.equal(ready, 0);
  invalidate();
  const readyFrame = Uint8Array.of(0);
  listeners.get(transport.LIFECYCLE_CHANNEL)!(readyFrame);
  listeners.get(transport.LIFECYCLE_CHANNEL)!(readyFrame);
  assert.equal(ready, 0);
  assert.equal(close, 1);
  assert.equal(dead, "KELD-IPC-005");
} else {
  const session = await connecting;
  assert.equal(ready, 1);
  assert.equal(
    session.windowSource?.desktop()?.primary_id,
    selected ? 17 : undefined,
  );
}
assert.equal(calls, 1);

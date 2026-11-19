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
  async call(channel: number, payload: Uint8Array, deadline: number) {
    calls += 1;
    assert.equal(channel, transport.LIFECYCLE_CHANNEL);
    assert.deepEqual([...payload], [1, transport.WINDOW_CHANNEL]);
    assert.equal(
      deadline,
      selected ? 30_000 : transport.APP_LINK_IO_DEADLINE_MS,
    );
    listeners.get(transport.LIFECYCLE_CHANNEL)!(Uint8Array.of(0));
    assert.equal(ready, 0, "raw Ready cannot outrun Subscribe acknowledgement");
    if (mode === "valid" || mode === "invalidated")
      appliers.get(transport.WINDOW_CHANNEL)!(
        Uint8Array.from(
          Buffer.from(
            "07ac02011101110000a00b84070030a00bec06000000000000004000",
            "hex",
          ),
        ),
      );
    if (mode === "invalidated")
      appliers.get(transport.WINDOW_CHANNEL)!(Uint8Array.of(7, 0xad, 2, 0));
    if (mode === "unavailable")
      throw transport.linkError("KELD-CORE-049", "native snapshot unavailable");
    return Uint8Array.of(1);
  },
  close() {
    close += 1;
  },
};
mock.module("@keld/kipc", () => ({
  ...transport,
  WorkerLink: { open: async () => worker },
}));
const { LifecycleLink } = await import("./link.ts");
const connecting = LifecycleLink.connect("controlled-link", {
  onReady() {
    ready += 1;
  },
  onLastWindowClosed() {},
  onLinkDead() {
    throw new Error("unexpected link death");
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
} else {
  const session = await connecting;
  assert.equal(ready, 1);
  assert.equal(
    session.windowSource?.desktop()?.primary_id,
    selected ? 17 : undefined,
  );
}
assert.equal(calls, 1);

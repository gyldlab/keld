/** S1 source conformance over the real API mirror and a controlled WorkerLink. */
import assert from "node:assert/strict";
import * as transport from "@keld/kipc";

const mode = process.argv[2];
const windows = new Map<number, (payload: Uint8Array) => void>();
const listeners = new Map<number, (payload: Uint8Array) => void>();
let calls = 0;
const worker = {
  stateFactCapacity: 16,
  setStateApplier(channel: number, callback: (payload: Uint8Array) => void) {
    windows.set(channel, callback);
  },
  onEvent(channel: number, callback: (payload: Uint8Array) => void) {
    listeners.set(channel, callback);
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
    assert.equal(deadline, 30_000);
    windows.get(transport.WINDOW_CHANNEL)!(
      Uint8Array.from(
        Buffer.from(
          "07ac02011101110000a00b84070030a00bec06000000000000004000",
          "hex",
        ),
      ),
    );
    beforeReply?.();
    listeners.get(transport.LIFECYCLE_CHANNEL)!(Uint8Array.of(0));
    return Uint8Array.of(1);
  },
};
transport.WorkerLink.open = async () =>
  worker as unknown as transport.WorkerLink;
const electron = await import("./index.ts");
if (mode === "unselected") {
  assert.equal(electron.screen, undefined);
  assert.equal(electron.nativeTheme, undefined);
  assert.equal(electron.default.screen, undefined);
  assert.equal(electron.default.nativeTheme, undefined);
  assert.equal(Object.hasOwn(electron.default, "screen"), false);
  assert.equal(Object.hasOwn(electron.default, "nativeTheme"), false);
  assert.equal(calls, 0);
} else {
  const { screen, nativeTheme, app } = electron;
  assert.ok(screen);
  assert.ok(nativeTheme);
  assert.equal(electron.default.screen, screen);
  assert.equal(electron.default.nativeTheme, nativeTheme);
  assert.throws(
    () => screen.getPrimaryDisplay(),
    (error) => transport.isCallError(error) && error.code === "KELD-CORE-040",
  );
  await app.whenReady();
  const primary = screen.getPrimaryDisplay();
  assert.deepEqual(primary, {
    id: 17,
    bounds: { x: 0, y: 0, width: 1440, height: 900 },
    workArea: { x: 0, y: 24, width: 1440, height: 876 },
    scaleFactor: 2,
  });
  assert.deepEqual(screen.getAllDisplays(), [primary]);
  primary.workArea.y = 999;
  assert.equal(screen.getPrimaryDisplay().workArea.y, 24);
  assert.equal(nativeTheme.shouldUseDarkColors, false);
  assert.equal(Reflect.set(nativeTheme, "shouldUseDarkColors", true), false);
  assert.ok(Object.isFrozen(nativeTheme));
  const appearance = Object.getOwnPropertyDescriptor(
    nativeTheme,
    "shouldUseDarkColors",
  );
  assert.equal(appearance?.set, undefined);
  assert.equal(appearance?.configurable, false);

  if (mode === "poisoned-array-methods") {
    const map = Object.getOwnPropertyDescriptor(Array.prototype, "map")!;
    const find = Object.getOwnPropertyDescriptor(Array.prototype, "find")!;
    const first = Object.getOwnPropertyDescriptor(Array.prototype, "0");
    let hooks = 0;
    let primaryCopy: ReturnType<typeof screen.getPrimaryDisplay>;
    let allCopies: ReturnType<typeof screen.getAllDisplays>;
    try {
      Object.defineProperties(Array.prototype, {
        map: {
          value() {
            hooks += 1;
            return [];
          },
        },
        find: {
          value() {
            hooks += 1;
            return primary;
          },
        },
        0: {
          configurable: true,
          set() {
            hooks += 1;
          },
        },
      });
      primaryCopy = screen.getPrimaryDisplay();
      allCopies = screen.getAllDisplays();
    } finally {
      Object.defineProperty(Array.prototype, "map", map);
      Object.defineProperty(Array.prototype, "find", find);
      if (first) Object.defineProperty(Array.prototype, "0", first);
      else Reflect.deleteProperty(Array.prototype, "0");
    }
    assert.equal(hooks, 0, "array hooks must not see the retained snapshot");
    assert.equal(primaryCopy.workArea.y, 24);
    assert.deepEqual(allCopies, [primaryCopy]);
    assert.notEqual(allCopies[0]!.bounds, primaryCopy.bounds);
    allCopies[0]!.workArea.y = 222;
    primaryCopy.workArea.y = 333;
    assert.equal(screen.getAllDisplays()[0]!.workArea.y, 24);
  }
  if (mode === "invalidated") {
    windows.get(transport.WINDOW_CHANNEL)!(Uint8Array.of(7, 0xad, 2, 0));
    for (const read of [
      screen.getPrimaryDisplay,
      screen.getAllDisplays,
      () => nativeTheme.shouldUseDarkColors,
    ]) {
      assert.throws(
        read,
        (error) =>
          transport.isCallError(error) && error.code === "KELD-CORE-049",
      );
    }
  }
  assert.equal(calls, 1, "display and theme reads must not issue IPC");
}

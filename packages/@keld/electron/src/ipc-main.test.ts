/**
 * GH-466 registration slice: pinned Electron v44.4.5 registration cells and
 * source-parity tests for `ipcMain` (docs/specs/gh466-ipcmain-registration.md).
 *
 * Corpus `electron-ipc-v1` (`crates/keld-compat/fixtures/ipc-corpus/`) maps
 * each pass cell to one exact test name below; its delivery cells live in
 * `ipc.test.ts` and stay red. Every oracle here is independent of the facade:
 * the exact listener arrays and counts EventEmitter exposes, the typed error
 * code and Electron's error sentence, or an observer child's printed markers.
 *
 * No message is delivered by this slice. The one `emit` call below is the
 * application-side EventEmitter method Electron apps can call themselves; it
 * is the differential check GH540 §4.10 asks for, not Keld delivery, and the
 * value it passes is test data, not a minted IpcMainEvent.
 *
 * `bun test` shares one module cache across files, so this file uses unique
 * channel names, cleans up after each test, and never calls the no-channel
 * `removeAllListeners()` in-process (that is the observer child's job).
 */
import { EventEmitter } from "node:events";
import { join } from "node:path";
import { afterEach, describe, expect, test } from "bun:test";

import * as electron from "./index";
import { ipcMain } from "./index";

const fixtures = join(import.meta.dir, "..", "fixtures");

/** Kill switch only, below bun's 5 s per-test timeout. Every observer exits on its own. */
const OBSERVER_KILL_MS = 4_000;

/** Runs a fixture with no app link at all: any transport touch is a loud failure. */
function observe(script: string): {
  code: number | null;
  stderr: string;
  lines: string[];
} {
  const env = { ...process.env };
  delete env.KELD_APP_LINK;
  const child = Bun.spawnSync({
    cmd: [process.execPath, `./${script}`],
    cwd: fixtures,
    env,
    stdout: "pipe",
    stderr: "pipe",
    timeout: OBSERVER_KILL_MS,
  });
  return {
    code: child.exitCode,
    stderr: child.stderr.toString(),
    lines: child.stdout
      .toString()
      .split("\n")
      .filter((line) => line.startsWith("GH466_")),
  };
}

const used = new Set<string>();
function channel(name: string): string {
  const full = `gh466-registration-${name}`;
  used.add(full);
  return full;
}

afterEach(() => {
  for (const name of used) {
    ipcMain.removeAllListeners(name);
    ipcMain.removeHandler(name);
  }
  used.clear();
});

describe("GH-466 registration cells (pass)", () => {
  test("ipc-main.event-emitter: ipcMain is one node:events EventEmitter instance shared by the named and default electron exports", () => {
    expect(ipcMain).toBeInstanceOf(EventEmitter);
    expect(electron.ipcMain).toBe(ipcMain);
    expect(electron.default.ipcMain).toBe(ipcMain);
  });

  test("ipc-main.off.removes-listener: off removes exactly the given listener from the channel's listener array and leaves the other entries in order", () => {
    const ch = channel("off");
    const a = (): void => {};
    const b = (): void => {};
    const c = (): void => {};
    ipcMain.on(ch, a).on(ch, b).on(ch, c);
    expect(ipcMain.listeners(ch)).toEqual([a, b, c]);
    expect(ipcMain.off(ch, b)).toBe(ipcMain);
    expect(ipcMain.listeners(ch)).toEqual([a, c]);
    // Removing a listener that is not registered changes nothing.
    ipcMain.off(ch, b);
    expect(ipcMain.listeners(ch)).toEqual([a, c]);
  });

  test("ipc-main.remove-listener: removeListener removes exactly one occurrence like off, including a once registration by its original function", () => {
    const ch = channel("remove-listener");
    const a = (): void => {};
    const once = (): void => {};
    ipcMain.on(ch, a).on(ch, a).once(ch, once);
    expect(ipcMain.listeners(ch)).toEqual([a, a, once]);
    expect(ipcMain.removeListener(ch, a)).toBe(ipcMain);
    expect(ipcMain.listeners(ch)).toEqual([a, once]);
    ipcMain.removeListener(ch, once);
    expect(ipcMain.listeners(ch)).toEqual([a]);
    expect(ipcMain.listenerCount(ch)).toBe(1);
  });

  test("ipc-main.add-listener.alias: addListener appends to the same listener array as on, in registration order with duplicates kept", () => {
    const ch = channel("add-listener");
    const a = (): void => {};
    const b = (): void => {};
    expect(ipcMain.addListener(ch, a)).toBe(ipcMain);
    ipcMain.on(ch, b);
    ipcMain.addListener(ch, a);
    expect(ipcMain.listeners(ch)).toEqual([a, b, a]);
    expect(ipcMain.listenerCount(ch)).toBe(3);
  });

  test("ipc-main.remove-all-listeners: removeAllListeners(channel) empties only that channel; with no channel an observer child sees every channel emptied", () => {
    const one = channel("remove-all-one");
    const other = channel("remove-all-other");
    const a = (): void => {};
    ipcMain.on(one, a).on(one, a).once(other, a);
    expect(ipcMain.removeAllListeners(one)).toBe(ipcMain);
    expect(ipcMain.listenerCount(one)).toBe(0);
    expect(ipcMain.listenerCount(other)).toBe(1);
    expect(observe("ipc_main_remove_all.ts")).toEqual({
      code: 0,
      stderr: "",
      lines: [
        "GH466_REMOVE_ALL before error=1 a=2 b=1 c=1",
        "GH466_REMOVE_ALL channel error=1 a=2 b=0 c=1",
        "GH466_REMOVE_ALL none error=0 a=0 b=0 c=0 names=0",
      ],
    });
  });

  test("ipc-main.handle.registers: handle records one invoke handler per channel and refuses a second with KELD-COMPAT-098 before checking the handler type", () => {
    const ch = channel("handle");
    const handler = (): number => 1;
    expect(ipcMain.handle(ch, handler)).toBeUndefined();
    expect(() => ipcMain.handle(ch, handler)).toThrow(
      `KELD-COMPAT-098: Attempted to register a second handler for '${ch}'`,
    );
    // Electron's order: the occupied channel is reported before the bad type.
    expect(() => ipcMain.handle(ch, 42 as unknown as () => unknown)).toThrow(
      "KELD-COMPAT-098",
    );
    // A different channel is free.
    expect(ipcMain.handle(channel("handle-other"), handler)).toBeUndefined();
  });

  test("ipc-main.remove-handler: removeHandler clears the channel's handler if present so handle registers again, and is a no-op for an absent channel", () => {
    const ch = channel("remove-handler");
    const handler = (): number => 1;
    expect(ipcMain.removeHandler(ch)).toBeUndefined();
    ipcMain.handle(ch, handler);
    ipcMain.removeHandler(ch);
    expect(ipcMain.handle(ch, handler)).toBeUndefined();
    expect(() => ipcMain.handle(ch, handler)).toThrow("KELD-COMPAT-098");
    expect(() =>
      ipcMain.removeHandler(channel("never-registered")),
    ).not.toThrow();
  });
});

describe("Electron v44.4.5 ipc-main-impl.ts parity (source-cited, not corpus cells)", () => {
  test("the constructor installs Electron's no-op error listener so a message on channel 'error' cannot raise the unhandled-error exception", () => {
    expect(ipcMain.listenerCount("error")).toBe(1);
    expect(() =>
      ipcMain.emit("error", new Error("renderer named a channel 'error'")),
    ).not.toThrow();
  });

  test("handle rejects a non-function handler with a KELD-COMPAT-099 TypeError carrying Electron's sentence and the found type", () => {
    const ch = channel("handle-type");
    expect(() => ipcMain.handle(ch, 42 as unknown as () => unknown)).toThrow(
      TypeError,
    );
    expect(() =>
      ipcMain.handle(ch, "nope" as unknown as () => unknown),
    ).toThrow(
      "KELD-COMPAT-099: Expected handler to be a function, but found type 'string'",
    );
    // The refused registration recorded nothing.
    expect(ipcMain.handle(ch, () => 1)).toBeUndefined();
  });

  test("handleOnce records a self-removing wrapper without checking its argument, as the pinned source does; nothing invokes it", () => {
    const ch = channel("handle-once");
    let calls = 0;
    ipcMain.handleOnce(ch, () => {
      calls += 1;
      return calls;
    });
    expect(() => ipcMain.handle(ch, () => 2)).toThrow("KELD-COMPAT-098");
    ipcMain.removeHandler(ch);
    const unchecked = channel("handle-once-unchecked");
    expect(() =>
      ipcMain.handleOnce(unchecked, 42 as unknown as () => unknown),
    ).not.toThrow();
    expect(() => ipcMain.handle(unchecked, () => 3)).toThrow("KELD-COMPAT-098");
    expect(calls).toBe(0);
  });

  test("the listener registry and the invoke-handler table are separate: neither removal touches the other (GH540 registry-separation control)", () => {
    const ch = channel("separate");
    const listener = (): void => {};
    ipcMain.on(ch, listener);
    ipcMain.handle(ch, () => 1);
    ipcMain.removeHandler(ch);
    expect(ipcMain.listeners(ch)).toEqual([listener]);
    ipcMain.handle(ch, () => 1);
    ipcMain.removeAllListeners(ch);
    expect(ipcMain.listenerCount(ch)).toBe(0);
    expect(() => ipcMain.handle(ch, () => 2)).toThrow("KELD-COMPAT-098");
  });

  test("once registrations follow node:events: listeners() unwraps, the wrapper is removed before the callback runs, and a reentrant application emit cannot re-enter it", () => {
    const ch = channel("once-reentrant");
    const order: string[] = [];
    const once = (): void => {
      order.push(`once:${ipcMain.listenerCount(ch)}`);
      ipcMain.emit(ch);
    };
    const plain = (): void => {
      order.push("plain");
    };
    ipcMain.once(ch, once).on(ch, plain);
    expect(ipcMain.listeners(ch)).toEqual([once, plain]);
    expect(ipcMain.rawListeners(ch)[0]).not.toBe(once);
    ipcMain.emit(ch);
    expect(order).toEqual(["once:1", "plain", "plain"]);
    expect(ipcMain.listeners(ch)).toEqual([plain]);
  });

  test("registration never invokes anything: after on, once and handle on fresh channels every counter stays at zero", () => {
    let calls = 0;
    const count = (): number => {
      calls += 1;
      return calls;
    };
    ipcMain.on(channel("silent-on"), count);
    ipcMain.once(channel("silent-once"), count);
    ipcMain.handle(channel("silent-handle"), count);
    ipcMain.handleOnce(channel("silent-handle-once"), count);
    expect(calls).toBe(0);
  });
});

describe("import-time registration in draw.io's shape (no app link)", () => {
  // Negative controls: removing the `ipcMain` export makes the child fail with
  // Bun's export-not-found SyntaxError; adding `app.whenReady()` to the fixture
  // surfaces KELD-IPC-007 on stderr because KELD_APP_LINK is unset.
  test("the 26 on/once sites, 5 removeListener sites and both library registrations register with exact counts, open no link and deliver nothing", () => {
    expect(observe("ipc_main_registration.ts")).toEqual({
      code: 0,
      stderr: "",
      lines: [
        "GH466_BINDINGS default-same=true emitter=true error-listeners=1",
        "GH466_REGISTERED sites=26 channels=24 app-load-finished=3 checkForUpdates=1 draftRemoved=1 export=1 export-error=1 export-finalize=1 import-error=1 import-success=1 isModified-result=1 newfile=1 openDevTools=1 render-finished=1 rendererReq=1 resetZoom=1 saveAndClose-result=1 svg-data=1 toggleFullscreen=1 toggleGoogleFonts=1 toggleSpellCheck=1 toggleStoreBkp=1 xml-data=1 xml-data-error=1 zoomIn=1 zoomOut=1",
        "GH466_REMOVED saveAndClose-result=0 import-success=0 import-error=0 render-finished=0 export-error=0 remaining=21",
        "GH466_LIBRARIES electron-store-get-data=1 __ELECTRON_LOG__=1 second-handle=KELD-COMPAT-098",
        "GH466_DELIVERED calls=0",
      ],
    });
  });
});

/**
 * GH-464 slice 1 delivery cells at Electron v44.4.5, re-recorded by the GH-466
 * registration slice (docs/specs/gh466-ipcmain-registration.md).
 *
 * Each mapped test asserts today's behaviour and PASSES while its corpus cell
 * keeps expected_verdict: fail (gh532 rule 3): the method now REGISTERS, but
 * no message is delivered by this slice. There is no sender, no IpcMainEvent
 * or IpcMainInvokeEvent value to mint one from, and no dispatch path, so the
 * registered function is never invoked by Keld: not dropped, simply never
 * sent to. The GH-466 delivery slice (listeners) and GH-547 (invoke) must
 * replace each absence observation with the cited delivery contract, flip
 * the cell, remove implementing_ticket and regenerate the digest.
 * No renderer, transport, native WebKit or Electron runtime proof is claimed.
 */
import { afterEach, describe, expect, test } from "bun:test";

import * as electron from "./index";

/** Read the same public member an application can read, including inherited methods. */
function member(target: unknown, name: string): unknown {
  if (
    (typeof target !== "object" && typeof target !== "function") ||
    target === null
  ) {
    return undefined;
  }
  return Reflect.get(target, name);
}

const { ipcMain } = electron;

/** Both public shapes must agree; a default-only or named-only binding is a defect. */
function expectBothBindings(): void {
  expect(member(electron, "ipcMain")).toBe(ipcMain);
  expect(member(electron.default, "ipcMain")).toBe(ipcMain);
}

/** No event value exists on either binding, so nothing can mint a delivery. */
function expectNoEventValue(name: "IpcMainEvent" | "IpcMainInvokeEvent"): void {
  expect(member(electron, name)).toBeUndefined();
  expect(member(electron.default, name)).toBeUndefined();
}

const channels = {
  on: "gh466-delivery-on",
  once: "gh466-delivery-once",
  handle: "gh466-delivery-handle",
  handleOnce: "gh466-delivery-handle-once",
} as const;

afterEach(() => {
  for (const channel of Object.values(channels)) {
    ipcMain.removeAllListeners(channel);
    ipcMain.removeHandler(channel);
  }
});

describe("GH-466 delivery cells, red until a sender exists", () => {
  test("ipc-main.on today: ipcMain.on registers the listener, but no message can arrive: no IpcMainEvent value exists and the listener is never invoked", () => {
    expectBothBindings();
    let calls = 0;
    const listener = (): void => {
      calls += 1;
    };
    expect(ipcMain.on(channels.on, listener)).toBe(ipcMain);
    expect(ipcMain.listenerCount(channels.on)).toBe(1);
    expectNoEventValue("IpcMainEvent");
    expect(calls).toBe(0);
  });

  test("ipc-main.once today: ipcMain.once registers a one-time listener, but no message can arrive: no IpcMainEvent value exists and the listener is never invoked", () => {
    expectBothBindings();
    let calls = 0;
    const listener = (): void => {
      calls += 1;
    };
    expect(ipcMain.once(channels.once, listener)).toBe(ipcMain);
    expect(ipcMain.listenerCount(channels.once)).toBe(1);
    // The registered entry is EventEmitter's once wrapper over the original.
    expect(ipcMain.listeners(channels.once)).toEqual([listener]);
    expect(ipcMain.rawListeners(channels.once)[0]).not.toBe(listener);
    expectNoEventValue("IpcMainEvent");
    expect(calls).toBe(0);
  });

  test("ipc-main.handle.invoked today: ipcMain.handle records the handler, but no renderer invoke can reach it: no IpcMainInvokeEvent value exists and the handler is never called", () => {
    expectBothBindings();
    let calls = 0;
    const handler = (): number => {
      calls += 1;
      return calls;
    };
    expect(ipcMain.handle(channels.handle, handler)).toBeUndefined();
    // The only public witness that a handler is recorded: a second one is refused.
    expect(() => ipcMain.handle(channels.handle, handler)).toThrow(
      "KELD-COMPAT-098",
    );
    expectNoEventValue("IpcMainInvokeEvent");
    expect(calls).toBe(0);
  });

  test("ipc-main.handle-once today: ipcMain.handleOnce records a single-use handler, but no renderer invoke can reach it: nothing is handled or removed because nothing is sent", () => {
    expectBothBindings();
    let calls = 0;
    const handler = (): number => {
      calls += 1;
      return calls;
    };
    expect(ipcMain.handleOnce(channels.handleOnce, handler)).toBeUndefined();
    // Still recorded: the self-removing wrapper has not run, because no invoke exists.
    expect(() => ipcMain.handle(channels.handleOnce, handler)).toThrow(
      "KELD-COMPAT-098",
    );
    expectNoEventValue("IpcMainInvokeEvent");
    expect(calls).toBe(0);
  });
});

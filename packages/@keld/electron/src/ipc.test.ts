/**
 * GH-464 slice 1: main-process listener baselines at Electron v44.4.5.
 * Target path: packages/@keld/electron/src/ipc.test.ts.
 *
 * The mapped tests assert today's missing methods and PASS. Their corpus cells
 * remain expected_verdict: fail with implementing_ticket: GH-466 (gh532 rule 3).
 * GH-466 must replace each missing-method observation with the cited delivery
 * contract, flip its cell, remove implementing_ticket and regenerate the digest.
 * No renderer, transport, native WebKit or Electron runtime proof is claimed.
 */
import { describe, expect, test } from "bun:test";

import * as electron from "./index";

/** Read the same public member an application can read, including inherited methods. */
function member(target: unknown, name: string): unknown {
  if ((typeof target !== "object" && typeof target !== "function") || target === null) {
    return undefined;
  }
  return Reflect.get(target, name);
}

/** Observe the real module's missing method and the resulting ordinary application call. */
function expectMissingListenerMethod(name: "on" | "once" | "removeListener"): void {
  // Check both public shapes; an empty named export must not mask a callable default.
  for (const ipcMain of [member(electron, "ipcMain"), member(electron.default, "ipcMain")]) {
    const method = member(ipcMain, name);
    expect(typeof method).toBe("undefined");
    const listener = (): void => {};
    expect(() =>
      Reflect.apply(method as (...args: unknown[]) => unknown, ipcMain, ["rendererReq", listener]),
    ).toThrow(TypeError);
  }
}

describe("GH-466 IPC listener cells, red until implemented", () => {
  test("ipc-main.on today: ipcMain.on is absent and a listener cannot be registered", () => {
    expectMissingListenerMethod("on");
  });

  test("ipc-main.once today: ipcMain.once is absent and a one-time listener cannot be registered", () => {
    expectMissingListenerMethod("once");
  });

  test("ipc-main.remove-listener today: ipcMain.removeListener is absent and a listener cannot be removed", () => {
    expectMissingListenerMethod("removeListener");
  });
});

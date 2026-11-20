/**
 * GH-477 first-proof native/dialog conformance, pinned to Electron v44.4.5
 * (694f45852a0f1726cd23bfd379854de489cccb65).
 *
 * GH-566's original four red-until-implemented cells stay independently mapped.
 * The guarded parented async Message case became real on macOS under GH-478:
 * its separate no-flag Host test exercises three OS clicks, actual checkbox
 * state, physical quiescence and 257+1 same-link operations. The parentless
 * overload remains unsupported, so the broad Message cell stays expected-fail
 * alongside sync Message, Open and Save until parentless and native OS gates pass.
 *
 * The Electron docs guarantee a blocking numeric clicked index for the
 * sync variant and distinct Promise result shapes for three async variants.
 * This Bun test checks exports, Promise shape and early refusal only. Actual
 * NSAlert/WorkerLink/Guard/native evidence belongs to the macOS Host fixtures.
 */
import { expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import * as electron from "./index";

// Exercise the platform gate in an isolated process so platform/readiness
// overrides cannot affect another test. This proves facade routing, not native
// Windows/Linux or AppKit acceptance. Removing the gate must fail this case.
test("Message refuses unsupported platforms before touching the app link", () => {
  const result = spawnSync(
    process.execPath,
    [
      "--eval",
      `
    import assert from "node:assert/strict";
    import { app } from "../../api/src/app.ts";
    import { showMessageBox, showMessageBoxSync } from "./dialog.ts";
    let readinessCalls = 0;
    const sentinel = new Error("readiness sentinel");
    app.whenReady = () => {
      readinessCalls++;
      return Promise.reject(sentinel);
    };
    for (const platform of ["linux", "win32"]) {
      Object.defineProperty(process, "platform", { value: platform, configurable: true });
      for (const args of [[{ message: "Confirm" }], [null, { message: "Confirm" }]]) {
        const result = showMessageBox(...args);
        assert.ok(result instanceof Promise);
        await assert.rejects(result, error => error.code === "KELD-NATIVE-010");
      }
      assert.equal(readinessCalls, 0);
      if (platform === "linux") {
        assert.throws(() => showMessageBoxSync({ message: "Confirm" }),
          error => error.code === "KELD-NATIVE-010");
      }
      assert.throws(() => showMessageBox({ message: "Confirm", unsupported: true }), /KELD-COMPAT-010/);
    }
    Object.defineProperty(process, "platform", { value: "darwin", configurable: true });
    assert.throws(() => showMessageBoxSync({ message: "Confirm" }),
      error => error.code === "KELD-NATIVE-010");
    await assert.rejects(showMessageBox({ message: "Confirm" }), error => error === sentinel);
    assert.equal(readinessCalls, 1);
    console.log("platform refusal and macOS delegation checks pass");
  `,
    ],
    {
      cwd: fileURLToPath(new URL(".", import.meta.url)),
      encoding: "utf8",
      timeout: 10_000,
    },
  );
  expect(result.error).toBeUndefined();
  expect(result.signal).toBeNull();
  expect(result.status).toBe(0);
  expect(result.stdout).toContain(
    "platform refusal and macOS delegation checks pass",
  );
});

/** Read the exported surface (not a copied implementation of dialog behavior). */
function member(target: unknown, key: string): unknown {
  if (
    (typeof target !== "object" && typeof target !== "function") ||
    target === null
  ) {
    return undefined;
  }
  return Reflect.get(target, key);
}

/** The API is normally imported as `dialog` either named or on Electron's default export. */
function dialogMethod(exports: unknown, method: string): unknown {
  return member(member(exports, "dialog"), method);
}

// This checks only the export and deterministic pre-Ready refusal. Actual
// native index, permission and physical cleanup require a matched Host run.
test("dialog.show-message-box-sync exports the same method and refuses before Ready", () => {
  const method = dialogMethod(electron, "showMessageBoxSync");
  expect(typeof method).toBe("function");
  expect(dialogMethod(electron.default, "showMessageBoxSync")).toBe(method);
  // Other test files may already have driven the singleton through Ready.
  // Verify the pre-Ready behavior in a fresh realm instead of sharing it.
  const child = spawnSync(
    process.execPath,
    [
      "--eval",
      `
      import assert from "node:assert/strict";
      import { dialog } from "./dialog.ts";
      const code = process.platform === "win32" ? "KELD-CORE-040" : "KELD-NATIVE-010";
      assert.throws(() => dialog.showMessageBoxSync({ message: "Confirm" }), error => error.code === code);
    `,
    ],
    {
      cwd: fileURLToPath(new URL(".", import.meta.url)),
      encoding: "utf8",
      timeout: 10_000,
    },
  );
  expect(child.error).toBeUndefined();
  expect(child.signal).toBeNull();
  expect(child.status, child.stderr).toBe(0);
});

// Real parented Open returns a native selected path or canceled: true with an
// empty array; the macOS Host fixture proves both with OS UI and file census.
// The broad corpus cell stays expected-fail because parentless is incomplete.
test("dialog.show-open-dialog.canceled-paths: parented native works; parentless remains incomplete", () => {
  const method = dialogMethod(electron, "showOpenDialog");
  expect(typeof method).toBe("function");
  expect(dialogMethod(electron.default, "showOpenDialog")).toBe(method);
  expect(() =>
    Reflect.apply(method as Function, member(electron, "dialog"), [
      { filters: [{ name: "Files", extensions: ["*.drawio"] }] },
    ]),
  ).toThrow("KELD-COMPAT-010");
  const pending: unknown = Reflect.apply(
    method as Function,
    member(electron, "dialog"),
    [{ properties: ["openFile", "openDirectory"] }],
  );
  expect(pending).toBeInstanceOf(Promise);
  void (pending as Promise<unknown>).catch(() => {});
});

// Save is path-only; the OS fixture asserts positive native path without a
// file write, plus empty canceled path. Parentless remains unimplemented.
test("dialog.show-save-dialog.canceled-path: parented native works; parentless remains incomplete", () => {
  const method = dialogMethod(electron, "showSaveDialog");
  expect(typeof method).toBe("function");
  expect(dialogMethod(electron.default, "showSaveDialog")).toBe(method);
  expect(() =>
    Reflect.apply(method as Function, member(electron, "dialog"), [
      { securityScopedBookmarks: true },
    ]),
  ).toThrow("KELD-COMPAT-010");
  const pending: unknown = Reflect.apply(
    method as Function,
    member(electron, "dialog"),
    [{ defaultPath: "diagram.drawio", securityScopedBookmarks: false }],
  );
  expect(pending).toBeInstanceOf(Promise);
  void (pending as Promise<unknown>).catch(() => {});
});

// Electron v44.4.5: real async facade exists on both export shapes; native
// clicked index 0/1/2 and actual checkbox state are proved separately by
// no_flag_macos::dialog_message on the shipping Host, not a JS-only stub.
test("dialog.show-message-box.response-checkbox: parented async works; parentless native remains incomplete", () => {
  const method = dialogMethod(electron, "showMessageBox");
  expect(typeof method).toBe("function");
  expect(dialogMethod(electron.default, "showMessageBox")).toBe(method);
  expect(() =>
    Reflect.apply(method as Function, member(electron, "dialog"), [
      { message: "invalid", unsupported: undefined },
    ]),
  ).toThrow("KELD-COMPAT-010");
  const result: unknown = Reflect.apply(
    method as Function,
    member(electron, "dialog"),
    [
      {
        message: "Confirm",
        buttons: ["Save", "Cancel", "Discard Changes"],
        defaultId: 0,
        cancelId: 1,
      },
    ],
  );
  expect(result).toBeInstanceOf(Promise);
  void (result as Promise<unknown>).catch(() => {});
});

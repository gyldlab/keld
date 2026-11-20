/**
 * GH-477 first-proof native/dialog conformance, pinned to Electron v44.4.5
 * (694f45852a0f1726cd23bfd379854de489cccb65).
 *
 * GH-566's original four red-until-implemented cells stay independently mapped.
 * The guarded parented async Message case became real on macOS under GH-478:
 * its separate no-flag Host test exercises three OS clicks, actual checkbox
 * state, physical quiescence and 257+1 same-link operations. The parentless
 * overload remains unsupported, so the broad Message cell stays expected-fail
 * alongside sync Message, Open and Save until parentless and remaining OS gates pass.
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
test("dialogs refuse unsupported platforms before touching the app link", () => {
  const result = spawnSync(
    process.execPath,
    [
      "--eval",
      `
    import assert from "node:assert/strict";
    import { app } from "../../api/src/app.ts";
    import { showMessageBox, showOpenDialog, showSaveDialog } from "./dialog.ts";
    let readinessCalls = 0;
    const sentinel = new Error("readiness sentinel");
    app.whenReady = () => {
      readinessCalls++;
      return Promise.reject(sentinel);
    };
    const operations = [
      [showMessageBox, { message: "Confirm" }],
      [showOpenDialog, { properties: ["openFile"] }],
      [showSaveDialog, { defaultPath: "example.txt" }],
    ];
    for (const platform of ["linux", "win32"]) {
      Object.defineProperty(process, "platform", { value: platform, configurable: true });
      for (const [showDialog, options] of operations) {
        for (const args of [[options], [null, options]]) {
          const result = showDialog(...args);
          assert.ok(result instanceof Promise);
          await assert.rejects(result, error => error.code === "KELD-NATIVE-010");
        }
        assert.throws(() => showDialog({ ...options, unsupported: true }), /KELD-COMPAT-010/);
      }
      assert.equal(readinessCalls, 0);
    }
    Object.defineProperty(process, "platform", { value: "darwin", configurable: true });
    for (const [showDialog, options] of operations) {
      await assert.rejects(showDialog(options), error => error === sentinel);
    }
    assert.equal(readinessCalls, operations.length);
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

// Electron dialog.md: `Returns Integer - the index of the clicked button.`
// Independent negative control: add any showMessageBoxSync export, even a
// Promise-returning shim. The present absence assertions must fail. After #564,
// the inverted test must invoke the real native method and check clicked index,
// non-Promise behavior, denied authority, and native teardown separately.
test("dialog.show-message-box-sync.clicked-index today: no synchronous native dialog method is exported", () => {
  expect(dialogMethod(electron, "showMessageBoxSync")).toBeUndefined();
  expect(dialogMethod(electron.default, "showMessageBoxSync")).toBeUndefined();
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

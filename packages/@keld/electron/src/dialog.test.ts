/**
 * GH-477 first-proof native/dialog conformance, pinned to Electron v44.4.5
 * (694f45852a0f1726cd23bfd379854de489cccb65).
 *
 * GH-566's original four red-until-implemented cells stay independently mapped.
 * The guarded parented async Message case became real on macOS under GH-478:
 * its separate no-flag Host test exercises three OS clicks, actual checkbox
 * state, physical quiescence and 257+1 same-link operations. The parentless
 * overload remains unsupported, so the broad Message cell stays expected-fail
 * alongside sync Message, Open and Save until their OS gates pass.
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
    import { showMessageBox } from "./dialog.ts";
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
      assert.throws(() => showMessageBox({ message: "Confirm", unsupported: true }), /KELD-COMPAT-010/);
    }
    Object.defineProperty(process, "platform", { value: "darwin", configurable: true });
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

// Electron dialog.md: `Returns Integer - the index of the clicked button.`
// Independent negative control: add any showMessageBoxSync export, even a
// Promise-returning shim. The present absence assertions must fail. After #564,
// the inverted test must invoke the real native method and check clicked index,
// non-Promise behavior, denied authority, and native teardown separately.
test("dialog.show-message-box-sync.clicked-index today: no synchronous native dialog method is exported", () => {
  expect(dialogMethod(electron, "showMessageBoxSync")).toBeUndefined();
  expect(dialogMethod(electron.default, "showMessageBoxSync")).toBeUndefined();
});

// Electron v44.4.5 dialog.md: open returns { canceled, filePaths }, where
// cancellation requires an empty filePaths array. This red-until-implemented
// case detects an actually added API rather than accepting an import-only shim.
test("dialog.show-open-dialog.canceled-paths today: no asynchronous native file-open method is exported", () => {
  expect(dialogMethod(electron, "showOpenDialog")).toBeUndefined();
  expect(dialogMethod(electron.default, "showOpenDialog")).toBeUndefined();
});

// Electron v44.4.5 dialog.md: save returns { canceled, filePath }, where
// cancellation requires the empty string. Path selection is NOT a file write.
test("dialog.show-save-dialog.canceled-path today: no asynchronous native file-save method is exported", () => {
  expect(dialogMethod(electron, "showSaveDialog")).toBeUndefined();
  expect(dialogMethod(electron.default, "showSaveDialog")).toBeUndefined();
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

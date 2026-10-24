/**
 * GH-477 first-proof native/dialog conformance, pinned to Electron v44.4.5
 * (694f45852a0f1726cd23bfd379854de489cccb65).
 *
 * Four red-until-implemented cells are registered by the shared GH-566 corpus owner.
 * Its mapped Bun test asserts the *current missing API* and therefore passes
 * today while the corpus records expected_verdict: fail: GH-564 for the sync
 * call, GH-478 for the three async calls. Each implementation owner must invert
 * only its mapped cases and cells in the same change as real native acceptance.
 *
 * The Electron docs guarantee a blocking numeric clicked index for the
 * sync variant and distinct Promise result shapes for three async variants.
 * These tests do NOT simulate NSAlert, exercise WorkerLink, authorize native
 * dialog access, or claim a macOS native implementation works.
 */
import { expect, test } from "bun:test";

import * as electron from "./index";

/** Read the exported surface (not a copied implementation of dialog behavior). */
function member(target: unknown, key: string): unknown {
  if ((typeof target !== "object" && typeof target !== "function") || target === null) {
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

// Electron v44.4.5 dialog.md: async message boxes return a Promise resolving
// with a clicked response index and checkboxChecked=false without a label.
test("dialog.show-message-box.response-checkbox today: no asynchronous native message-dialog method is exported", () => {
  expect(dialogMethod(electron, "showMessageBox")).toBeUndefined();
  expect(dialogMethod(electron.default, "showMessageBox")).toBeUndefined();
});

/**
 * GH-477 first-proof native/dialog conformance, pinned to Electron v44.4.5
 * (694f45852a0f1726cd23bfd379854de489cccb65).
 *
 * Four red-until-implemented cells are registered by the shared GH-566 corpus owner.
 * Since GH-565 the `dialog` binding links and each member refuses with the typed
 * `KELD-COMPAT-013` error naming its ticket, so these tests assert that *current
 * refusal* and pass while the corpus records expected_verdict: fail: GH-564 for the
 * sync call, GH-478 for the three async calls. Each implementation owner must invert
 * only its mapped cases and cells in the same change as real native acceptance.
 *
 * The Electron docs guarantee a blocking numeric clicked index for the
 * sync variant and distinct Promise result shapes for three async variants.
 * These tests do NOT simulate NSAlert, exercise WorkerLink, authorize native
 * dialog access, or claim a macOS native implementation works.
 */
import { expect, test } from "bun:test";

import * as electron from "./index";
import { isUnsupportedMemberError, type TrackingIssue } from "./index";

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

/** The typed refusal contract for today's linked, unimplemented member. */
function expectRefusal(
  caught: unknown,
  method: string,
  ticket: TrackingIssue,
): void {
  expect(isUnsupportedMemberError(caught)).toBe(true);
  if (!isUnsupportedMemberError(caught)) return;
  expect(caught.code).toBe("KELD-COMPAT-013");
  expect(caught.member).toBe(`dialog.${method}`);
  expect(caught.ticket).toBe(ticket);
  expect(caught.message).toContain(`gyldlab/keld#${ticket.slice(3)}`);
}

/** A synchronous dialog member throws the refusal on both export shapes. */
function expectSyncRefusal(method: string, ticket: TrackingIssue): void {
  for (const root of [electron, electron.default]) {
    const fn = dialogMethod(root, method);
    expect(typeof fn).toBe("function");
    let caught: unknown;
    try {
      Reflect.apply(
        fn as (...args: unknown[]) => unknown,
        member(root, "dialog"),
        [
          {
            message: "unsaved",
            buttons: ["Save", "Cancel", "Discard Changes"],
          },
        ],
      );
    } catch (error) {
      caught = error;
    }
    expectRefusal(caught, method, ticket);
  }
}

/** An async dialog member returns a Promise (no synchronous throw) that rejects with the refusal. */
async function expectAsyncRefusal(
  method: string,
  ticket: TrackingIssue,
): Promise<void> {
  for (const root of [electron, electron.default]) {
    const fn = dialogMethod(root, method);
    expect(typeof fn).toBe("function");
    const result = Reflect.apply(
      fn as (...args: unknown[]) => unknown,
      member(root, "dialog"),
      [{ title: "Open" }],
    );
    expect(result).toBeInstanceOf(Promise);
    let caught: unknown;
    try {
      await (result as Promise<unknown>);
    } catch (error) {
      caught = error;
    }
    expectRefusal(caught, method, ticket);
  }
}

// Electron dialog.md: `Returns Integer - the index of the clicked button.`
// Independent negative control: make showMessageBoxSync return any value,
// even a Promise substitute, instead of the typed refusal. After #564, the
// inverted test must invoke the real native method and check clicked index,
// non-Promise behavior, denied authority, and native teardown separately.
test("dialog.show-message-box-sync.clicked-index today: dialog.showMessageBoxSync refuses synchronously with KELD-COMPAT-013 naming GH-564", () => {
  expectSyncRefusal("showMessageBoxSync", "GH-564");
});

// Electron v44.4.5 dialog.md: open returns { canceled, filePaths }, where
// cancellation requires an empty filePaths array. This red-until-implemented
// case detects an actually added API rather than accepting an import-only shim.
test("dialog.show-open-dialog.canceled-paths today: dialog.showOpenDialog returns a Promise that rejects with KELD-COMPAT-013 naming GH-478", async () => {
  await expectAsyncRefusal("showOpenDialog", "GH-478");
});

// Electron v44.4.5 dialog.md: save returns { canceled, filePath }, where
// cancellation requires the empty string. Path selection is NOT a file write.
test("dialog.show-save-dialog.canceled-path today: dialog.showSaveDialog returns a Promise that rejects with KELD-COMPAT-013 naming GH-478", async () => {
  await expectAsyncRefusal("showSaveDialog", "GH-478");
});

// Electron v44.4.5 dialog.md: async message boxes return a Promise resolving
// with a clicked response index and checkboxChecked=false without a label.
test("dialog.show-message-box.response-checkbox today: dialog.showMessageBox returns a Promise that rejects with KELD-COMPAT-013 naming GH-478", async () => {
  await expectAsyncRefusal("showMessageBox", "GH-478");
});

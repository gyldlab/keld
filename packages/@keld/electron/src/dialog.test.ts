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

// Electron dialog.md: `Returns Integer - the index of the clicked button.`
// Independent negative control: make showMessageBoxSync return any value,
// even a Promise substitute, instead of the typed refusal. After #564, the
// inverted test must invoke the real native method and check clicked index,
// non-Promise behavior, denied authority, and native teardown separately.
test("dialog.show-message-box-sync.clicked-index today: dialog.showMessageBoxSync refuses synchronously with KELD-COMPAT-013 naming GH-564", () => {
  expectSyncRefusal("showMessageBoxSync", "GH-564");
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

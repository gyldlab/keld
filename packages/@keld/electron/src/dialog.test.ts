/**
 * GH-477 first-proof native/dialog conformance, pinned to Electron v44.4.5
 * (694f45852a0f1726cd23bfd379854de489cccb65).
 *
 * One red-until-implemented cell is registered by the shared GH-566 corpus owner.
 * Its mapped Bun test asserts the *current missing API* and therefore passes
 * today while the corpus records expected_verdict: fail, implementing_ticket:
 * GH-564. GH-564 must invert this mapped case and its cell in the same change.
 *
 * The Electron docs guarantee a blocking numeric clicked index. This test
 * does NOT simulate an NSAlert, exercise WorkerLink, authorize native dialog
 * access, or claim the eventual macOS implementation works.
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
function syncMethod(exports: unknown): unknown {
  return member(member(exports, "dialog"), "showMessageBoxSync");
}

// Electron dialog.md: `Returns Integer - the index of the clicked button.`
// Independent negative control: add any showMessageBoxSync export, even a
// Promise-returning shim. The present absence assertions must fail. After #564,
// the inverted test must invoke the real native method and check clicked index,
// non-Promise behavior, denied authority, and native teardown separately.
test("dialog.show-message-box-sync.clicked-index today: no synchronous native dialog method is exported", () => {
  expect(syncMethod(electron)).toBeUndefined();
  expect(syncMethod(electron.default)).toBeUndefined();
});

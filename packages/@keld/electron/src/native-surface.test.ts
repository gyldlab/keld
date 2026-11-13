/**
 * GH-477 Electron v44.4.5 native first-proof conformance baselines.
 *
 * The canonical GH-566 owner registers these exact Bun test names in the
 * electron-native-v1 corpus. Today every listed native member is missing,
 * so these tests pass while the manifest keeps expected_verdict: fail and
 * an implementing-ticket key. They are NOT live menu, monitor or OS tests.
 *
 * Once a consumer is implemented, its owner must invert its exact mapped
 * case in the same PR as the native conformance acceptance and update the
 * corpus verdict. No stub, Promise or inferred OS theme proves parity.
 */
import { expect, test } from "bun:test";

import * as electron from "./index";

/** No new implementation-specific facade test hook: read the public module. */
function member(target: unknown, key: string): unknown {
  if (
    (typeof target !== "object" && typeof target !== "function") ||
    target === null
  ) {
    return undefined;
  }
  return Reflect.get(target, key);
}

/** Check both Electron's ESM named binding and default binding independently. */
function expectMissingExport(key: string): void {
  expect(member(electron, key)).toBeUndefined();
  expect(member(electron.default, key)).toBeUndefined();
}

/** Each named static/facade member must be independently absent today. */
function expectMissingMethod(moduleName: string, methodName: string): void {
  expect(member(member(electron, moduleName), methodName)).toBeUndefined();
  expect(
    member(member(electron.default, moduleName), methodName),
  ).toBeUndefined();
}

// Pinned menu.md: buildFromTemplate synchronously returns a Menu. The future
// test must construct a real menu; returning a Promise/undefined is not parity.
test("menu.build-from-template today: no static template construction is exported", () => {
  expectMissingMethod("Menu", "buildFromTemplate");
});

// Pinned menu.md: setApplicationMenu sets the actual macOS application menu.
test("menu.set-application-menu today: no static application-menu setter is exported", () => {
  expectMissingMethod("Menu", "setApplicationMenu");
});

// Pinned menu-item.md: a recognized role suppresses the item click callback.
// A fallback without a focused window is source-only (not scored as parity).
test("menu-item.role-overrides-click today: no MenuItem constructor is exported", () => {
  expectMissingExport("MenuItem");
});

// Pinned tutorial/menus.md: role close closes the current window. Later
// GH-480 must prove it uses GH-450's vetoable close path, not raw destroy.
// This baseline shares absent MenuItem with the role/click and role/quit
// cases; it does NOT independently prove veto or destruction ordering.
test("menu.role-close.vetoable today: role close cannot be constructed without MenuItem", () => {
  expectMissingExport("MenuItem");
});

// Pinned tutorial/menus.md: Electron role quit enters the app quit path.
// The pending GH-563 cell has THREE mandatory phases, not one shared menu
// import test (review #5638444771):
//   0. Before GH-480, MenuItem is absent and this expected-fail baseline passes.
//   1. GH-480 adds MenuItem but HOLDS role quit: GH-480 MUST replace this test's
//      body in the same implementation PR with an actual KELD-COMPAT diagnostic
//      naming GH-563, a real no-OS-termination/no-window-close witness, and
//      no fabricated before-quit. Keep verdict=fail and ticket=GH-563.
//   2. Only GH-563's own implementation flips verdict=pass after real native
//      Cmd+Q/role quit reaches before-quit and honors the dirty-window veto.
// This explicit intermediate tripwire prevents GH-480 from shipping MenuItem
// while silently leaving an obsolete "missing MenuItem" GH-563 oracle behind.
test("menu.role-quit.before-quit pending GH-563: three-stage no-native-quit contract", () => {
  const named = member(electron, "MenuItem");
  const defaultExport = member(electron.default, "MenuItem");
  if (named !== undefined || defaultExport !== undefined) {
    throw new Error(
      "GH-480 transition gate: MenuItem now exists. In the SAME GH-480 PR, re-record this " +
        "GH-563 pending-fail test as an actual typed KELD-COMPAT GH-563 role-quit diagnostic " +
        "with zero OS termination/windows closed, preserving expected_verdict=fail and " +
        "implementing_ticket=GH-563. Only GH-563 later proves before-quit/dirty veto and flips pass.",
    );
  }
  expect(named).toBeUndefined();
  expect(defaultExport).toBeUndefined();
});

// Pinned screen.md: getPrimaryDisplay returns one synchronous Display.
test("screen.get-primary-display today: no synchronous primary Display getter exists", () => {
  expectMissingMethod("screen", "getPrimaryDisplay");
});

// Pinned screen.md: getAllDisplays returns a synchronous Display[] array.
test("screen.get-all-displays today: no synchronous Display list getter exists", () => {
  expectMissingMethod("screen", "getAllDisplays");
});

// Pinned structures/display.md: workArea is a DIP Rectangle. Later GH-481
// must assert finite x/y/width/height on the actual primary Display.
// This baseline intentionally shares the missing getPrimaryDisplay method.
test("display.work-area.rectangle today: no Display workArea is available without primary getter", () => {
  expectMissingMethod("screen", "getPrimaryDisplay");
});

// Pinned native-theme.md: shouldUseDarkColors is a read-only boolean backed
// by the actual OS theme, not an inferred constant or async request.
test("native-theme.should-use-dark-colors today: no boolean native theme fact exists", () => {
  expect(
    member(member(electron, "nativeTheme"), "shouldUseDarkColors"),
  ).toBeUndefined();
  expect(
    member(member(electron.default, "nativeTheme"), "shouldUseDarkColors"),
  ).toBeUndefined();
});

/**
 * GH-477 Electron v44.4.5 native first-proof conformance baselines.
 *
 * The canonical GH-566 owner registers these exact Bun test names in the
 * electron-native-v1 corpus. Since GH-565 the `Menu`, `screen` and `nativeTheme`
 * bindings link, and every listed member refuses with the typed `KELD-COMPAT-013`
 * error naming its implementing ticket, so these tests pass while the manifest
 * keeps expected_verdict: fail and that ticket. `MenuItem` is still absent. They
 * are NOT live menu, monitor or OS tests.
 *
 * Once a consumer is implemented, its owner must invert its exact mapped
 * case in the same PR as the native conformance acceptance and update the
 * corpus verdict. No stub, Promise or inferred OS theme proves parity.
 */
import { expect, test } from "bun:test";

import * as electron from "./index";
import { isUnsupportedMemberError, type TrackingIssue } from "./index";

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

/** The typed refusal contract for today's linked, unimplemented member. */
function expectRefusal(
  caught: unknown,
  memberPath: string,
  ticket: TrackingIssue,
): void {
  expect(isUnsupportedMemberError(caught)).toBe(true);
  if (!isUnsupportedMemberError(caught)) return;
  expect(caught.code).toBe("KELD-COMPAT-013");
  expect(caught.member).toBe(memberPath);
  expect(caught.ticket).toBe(ticket);
  expect(caught.message).toContain(`gyldlab/keld#${ticket.slice(3)}`);
}

/** A linked method must throw the typed refusal on both export shapes, never return. */
function expectRefusingMethod(
  moduleName: string,
  methodName: string,
  ticket: TrackingIssue,
): void {
  for (const root of [electron, electron.default]) {
    const target = member(root, moduleName);
    const method = member(target, methodName);
    expect(typeof method).toBe("function");
    let caught: unknown;
    try {
      Reflect.apply(method as (...args: unknown[]) => unknown, target, []);
    } catch (error) {
      caught = error;
    }
    expectRefusal(caught, `${moduleName}.${methodName}`, ticket);
  }
}

// Pinned menu.md: buildFromTemplate synchronously returns a Menu. Today the
// binding links (GH-565) and the member refuses naming GH-480; the future test
// must construct a real menu; returning a Promise/undefined is not parity.
test("menu.build-from-template today: Menu.buildFromTemplate refuses with KELD-COMPAT-013 naming GH-480", () => {
  expectRefusingMethod("Menu", "buildFromTemplate", "GH-480");
});

// Pinned menu.md: setApplicationMenu sets the actual macOS application menu.
test("menu.set-application-menu today: Menu.setApplicationMenu refuses with KELD-COMPAT-013 naming GH-480", () => {
  expectRefusingMethod("Menu", "setApplicationMenu", "GH-480");
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
test("screen.get-primary-display today: screen.getPrimaryDisplay refuses with KELD-COMPAT-013 naming GH-481", () => {
  expectRefusingMethod("screen", "getPrimaryDisplay", "GH-481");
});

// Pinned screen.md: getAllDisplays returns a synchronous Display[] array.
test("screen.get-all-displays today: screen.getAllDisplays refuses with KELD-COMPAT-013 naming GH-481", () => {
  expectRefusingMethod("screen", "getAllDisplays", "GH-481");
});

// Pinned structures/display.md: workArea is a DIP Rectangle. Later GH-481
// must assert finite x/y/width/height on the actual primary Display.
// This baseline intentionally shares the refusing getPrimaryDisplay method.
test("display.work-area.rectangle today: no Display workArea is reachable because screen.getPrimaryDisplay refuses with KELD-COMPAT-013 naming GH-481", () => {
  expectRefusingMethod("screen", "getPrimaryDisplay", "GH-481");
});

// Pinned native-theme.md: shouldUseDarkColors is a read-only boolean backed
// by the actual OS theme, not an inferred constant or async request. Today
// the property read itself refuses, so no value of any type is observable.
test("native-theme.should-use-dark-colors today: reading nativeTheme.shouldUseDarkColors refuses with KELD-COMPAT-013 naming GH-481", () => {
  for (const root of [electron, electron.default]) {
    const nativeTheme = member(root, "nativeTheme");
    expect(typeof nativeTheme).toBe("object");
    let caught: unknown;
    try {
      Reflect.get(nativeTheme as object, "shouldUseDarkColors");
    } catch (error) {
      caught = error;
    }
    expectRefusal(caught, "nativeTheme.shouldUseDarkColors", "GH-481");
  }
});

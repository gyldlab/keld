/**
 * GH477 source-side partial contracts for Electron v44.4.5 native cells.
 *
 * GH480 now constructs real Menu/MenuItem source objects and queues guarded
 * installation. These Bun checks cover that source behavior and the generic
 * Quit refusal; they do not activate a native menu, key equivalent or window.
 * The mapped native-corpus cells keep expected_verdict:fail and their owning
 * tickets until independent real-OS acceptance. GH563's actual held-menu Quit
 * diagnostic/no-termination proof remains required; no mock supplies it here.
 */
import { expect, test } from "bun:test";

import * as electron from "./index";
import { isUnsupportedMemberError, type TrackingIssue } from "./unsupported.ts";

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

// Construction and projection are source contracts. Native menu acceptance is separate.
test("menu.build-from-template source: synchronous branded tree; native acceptance remains pending GH-480", () => {
  for (const root of [electron, electron.default]) {
    const item = new root.MenuItem({ role: "copy" });
    const menu = root.Menu.buildFromTemplate([
      { label: "Edit", submenu: [item, { type: "separator" }] },
    ]);
    expect(menu).toBeInstanceOf(root.Menu);
    expect(menu).not.toBeInstanceOf(Promise);
    expect(menu.items[0]!.submenu!.items[0]).toBe(item);
    expect(item.label).toBe("Copy");
    expect(item.accelerator).toBe("Command+C");
    expect(Object.isFrozen(menu.items)).toBe(true);
  }
});

test("menu.set-application-menu source: void call reports actual missing-link failure; native acceptance remains pending GH-480", () => {
  const env = { ...process.env };
  delete env.KELD_APP_LINK;
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      "-e",
      `import {Menu} from ${JSON.stringify(new URL("./index.ts", import.meta.url).pathname)};
       process.on('unhandledRejection',()=>{console.log('UNHANDLED');process.exitCode=4;});
       console.log('RETURN='+String(Menu.setApplicationMenu(Menu.buildFromTemplate([{label:'Explicit'}]))));
       await new Promise(resolve=>setImmediate(resolve));`,
    ],
    env,
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
  expect(child.exitCode).toBe(0);
  expect(child.stdout.toString()).toBe("RETURN=undefined\n");
  const diagnostics = child.stderr
    .toString()
    .trim()
    .split("\n")
    .map((line) => JSON.parse(line));
  expect(diagnostics).toHaveLength(1);
  expect(diagnostics[0]).toMatchObject({
    code: "KELD-IPC-007",
    phase: "menu-install",
    result: "not-sent",
  });
});

test("menu-item.role-overrides-click source: recognized role constructs without invoking click; native activation remains pending GH-480", () => {
  let clicked = 0;
  for (const root of [electron, electron.default]) {
    const item = new root.MenuItem({
      role: "COPY",
      click: () => {
        clicked++;
      },
    });
    const menu = root.Menu.buildFromTemplate([item]);
    expect(item).toBeInstanceOf(root.MenuItem);
    expect(menu.items[0]).toBe(item);
    expect(item.role).toBe("copy");
    expect(item.label).toBe("Copy");
  }
  expect(clicked).toBe(0);
});

test("menu.role-close.vetoable source: close role projects its default command; native veto acceptance remains pending GH-480", () => {
  for (const root of [electron, electron.default]) {
    const item = new root.MenuItem({ role: "close" });
    const menu = root.Menu.buildFromTemplate([item]);
    expect(menu.items[0]).toBe(item);
    expect(item.role).toBe("close");
    expect(item.label).toBe("Close Window");
    expect(item.accelerator).toBe("Command+W");
  }
});

// Reuse the real API/WindowSource/close-receipt role, whose only controlled seam
// is Host CALL. It independently asserts COMPAT012 and zero forceful Quit calls
// both before authenticated Ready and with a live vetoed window. This source
// observation is not GH563's required actual menu/Cmd+Q diagnostic or OS proof.
test("menu.role-quit.before-quit pending GH-563: generic live-window Quit stays refused; native menu qualification remains pending", () => {
  const item = new electron.MenuItem({
    role: "quit",
    label: "Quit source fixture",
  });
  expect(item.role).toBe("quit");
  expect(item.accelerator).toBe("Command+Q");
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      "--preload",
      new URL("../../kipc/src/transport.ts", import.meta.url).pathname,
      new URL("./browser-window-boot.test-role.ts", import.meta.url).pathname,
      "native-delete-before-import",
    ],
    env: { ...process.env, KELD_NATIVE_WINDOW_FACADE: "1" },
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
  expect(child.exitCode).toBe(0);
  expect(child.stdout.toString()).toBe("PASS native-delete-before-import\n");
  expect(child.stderr.toString()).toBe("");
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

import { expect, test } from "bun:test";
import { Menu, MenuItem, MenuTemplateError } from "./menu.ts";
import { isUnsupportedMemberError } from "./unsupported.ts";

test("GH480 real Menu graph constructs synchronously and retains branded item identity", () => {
  const item = new MenuItem({ role: "uNdO" });
  const menu = Menu.buildFromTemplate([
    {
      label: "Edit",
      submenu: [
        item,
        { type: "separator" },
        { type: "checkbox", checked: true, label: "Preference", click() {} },
      ],
    },
  ]);
  expect(menu).toBeInstanceOf(Menu);
  expect(menu).not.toBeInstanceOf(Promise);
  expect(menu.items[0]!.submenu!.items[0]).toBe(item);
  expect(item).toBeInstanceOf(MenuItem);
  expect(item.role).toBe("undo");
  expect(item.label).toBe("Undo");
  expect(item.accelerator).toBe("Command+Z");
  expect(menu.items[0]!.submenu!.items[2]!.checked).toBe(true);
  item.label = "Local undo";
  item.enabled = false;
  expect(menu.items[0]!.submenu!.items[0]!.label).toBe("Local undo");
  expect(menu.items[0]!.submenu!.items[0]!.enabled).toBe(false);
  expect(Object.isFrozen(menu.items)).toBe(true);
});

test("GH480 source constructor retains checked false and never invokes click or option accessors", () => {
  let calls = 0;
  const menu = Menu.buildFromTemplate([
    {
      type: "checkbox",
      checked: false,
      label: "Preference",
      click() {
        calls++;
      },
    },
    {
      role: "copy",
      click() {
        calls++;
      },
    },
  ]);
  expect(menu.items[0]!.checked).toBe(false);
  menu.items[0]!.checked = true;
  expect(menu.items[0]!.checked).toBe(true);
  expect(menu.items[1]!.role).toBe("copy");
  expect(calls).toBe(0);
  expect(() =>
    Menu.buildFromTemplate([
      {
        get label() {
          calls++;
          return "getter";
        },
      },
    ]),
  ).toThrow(MenuTemplateError);
  expect(calls).toBe(0);
});

test("GH480 native role labels use only actual pre-entry Host launch metadata", () => {
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      '--define=__KELD_HOST_LAUNCH_FACTS__:{"schema":1,"kind":"development","name":"draw.io","root":"/qualified-stage"}',
      `--preload=${new URL("../../kipc/src/transport.ts", import.meta.url).pathname}`,
      "-e",
      `import { Menu } from ${JSON.stringify(new URL("./menu.ts", import.meta.url).pathname)}; const m=Menu.buildFromTemplate([{role:'hide'},{role:'hideothers'},{role:'unhide'},{role:'quit'}]); console.log(JSON.stringify(m.items.map(i=>[i.role,i.label,i.accelerator])));`,
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
  expect(child.exitCode).toBe(0);
  expect(child.stderr.toString()).toBe("");
  expect(JSON.parse(child.stdout.toString())).toEqual([
    ["hide", "Hide draw.io", "Command+H"],
    ["hideothers", "Hide Others", "Command+Option+H"],
    ["unhide", "Show All", null],
    ["quit", "Quit draw.io", "Command+Q"],
  ]);
});

test("GH480 both original macOS template variants construct all source menus, roles and initial checkbox states", () => {
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      '--define=__KELD_HOST_LAUNCH_FACTS__:{"schema":1,"kind":"development","name":"draw.io","root":"/qualified-stage"}',
      `--preload=${new URL("../../kipc/src/transport.ts", import.meta.url).pathname}`,
      "-e",
      `import { Menu } from ${JSON.stringify(new URL("./menu.ts", import.meta.url).pathname)}; import {drawioMenuTemplate} from ${JSON.stringify(new URL("./menu.test-fixture.ts", import.meta.url).pathname)}; const results=[];for(const [disabled,dontCheck] of [[false,undefined],[false,true],[true,undefined]]){const menu=Menu.buildFromTemplate(drawioMenuTemplate(disabled,dontCheck));const items=menu.items.flatMap(i=>i.submenu.items);results.push({roots:menu.items.map(i=>i.label),counts:menu.items.map(i=>i.submenu.items.length),roles:items.filter(i=>i.role).map(i=>i.role),checked:items.filter(i=>i.type==='checkbox').map(i=>i.checked)});}console.log(JSON.stringify(results));`,
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
  expect(child.exitCode).toBe(0);
  expect(child.stderr.toString()).toBe("");
  const results = JSON.parse(child.stdout.toString());
  for (const result of results) {
    expect(result.roots).toEqual(["draw.io", "File", "Edit"]);
    expect(result.roles).toEqual([
      "hide",
      "hideothers",
      "unhide",
      "quit",
      "close",
      "undo",
      "redo",
      "cut",
      "copy",
      "paste",
      "pasteandmatchstyle",
      "selectall",
    ]);
  }
  expect(results.map((r: { counts: number[] }) => r.counts)).toEqual([
    [15, 1, 8],
    [15, 1, 8],
    [12, 1, 8],
  ]);
  expect(results.map((r: { checked: boolean[] }) => r.checked)).toEqual([
    [true],
    [false],
    [],
  ]);
});

test("GH480 malformed options, prototype forgery, sharing, depth and count fail before installation", () => {
  const invalid: unknown[] = [
    [{ label: null }],
    [{ type: null }],
    [{ role: "terminate:" }],
    [{ role: "hıde" }],
    [{ role: "close", accelerator: "Command+Q" }],
    [{ type: "normal", checked: false }],
    [{ type: "radio" }],
    [{ type: "checkbox", role: "copy" }],
    [{ label: "\ud800" }],
    [{ label: "é".repeat(2049) }],
    [{ label: "x\0y" }],
    [,],
    [Object.create(MenuItem.prototype)],
    [Object.create({ label: "inherited" })],
    Array.from({ length: 257 }, () => ({ label: "n" })),
  ];
  const shared = new MenuItem({ label: "same" });
  invalid.push([shared, shared]);
  const cycle: unknown[] = [];
  cycle.push({ submenu: cycle });
  invalid.push(cycle);
  const chain = (n: number): unknown =>
    n ? [{ submenu: chain(n - 1) }] : [{ label: "leaf" }];
  expect(Menu.buildFromTemplate(chain(8) as never).items).toHaveLength(1);
  invalid.push(chain(9));
  for (const input of invalid)
    expect(() => Menu.buildFromTemplate(input as never)).toThrow(
      MenuTemplateError,
    );
  expect(
    Menu.buildFromTemplate(Array.from({ length: 256 }, () => ({ label: "n" })))
      .items,
  ).toHaveLength(256);
});

test("GH480 setter is void and consumes a real missing-link failure instead of claiming installation", () => {
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      "-e",
      `delete process.env.KELD_APP_LINK; Object.defineProperty(Error.prototype,"origin",{value:"peer",configurable:true}); import { Menu } from ${JSON.stringify(new URL("./menu.ts", import.meta.url).pathname)}; process.on('unhandledRejection',()=>{console.log('UNHANDLED');process.exitCode=4;}); console.log('RETURN='+String(Menu.setApplicationMenu(Menu.buildFromTemplate([{label:'Explicit'}])))); await new Promise(resolve=>setImmediate(resolve));`,
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
  expect(child.exitCode).toBe(0);
  expect(child.stdout.toString()).toBe("RETURN=undefined\n");
  expect(child.stderr.toString()).toContain('"code":"KELD-IPC-007"');
  expect(child.stderr.toString()).toContain('"result":"not-sent"');
});

test("GH480 keeps unrelated construction and null-removal refusals typed", () => {
  for (const call of [
    () => new Menu(),
    () => Menu.setApplicationMenu(null),
    () => Menu.getApplicationMenu(),
  ]) {
    let caught: unknown;
    try {
      call();
    } catch (error) {
      caught = error;
    }
    expect(isUnsupportedMemberError(caught)).toBe(true);
  }
});

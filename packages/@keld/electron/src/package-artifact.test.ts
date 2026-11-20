import { expect, test } from "bun:test";
import {
  cpSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

function namedDesktopBindings(entry: string): string {
  const path = JSON.stringify(entry);
  return `
import bindingAssert from 'node:assert/strict';
import { WorkerLink } from '@keld/kipc';
let bindingCalls = 0;
WorkerLink.open = async () => { bindingCalls++; throw new Error('unexpected Host link'); };
const bindingNamespace = await import(${path});
const { default: alias, session, Menu, shell, ClipboardItem, clipboard, nativeImage, ipcMain } = bindingNamespace;
for (const [name, member] of Object.entries({ session, Menu, shell, ClipboardItem, clipboard, nativeImage })) {
  bindingAssert.equal(alias[name], member);
  bindingAssert.ok(member, name);
}
bindingAssert.equal(bindingNamespace.ipcMain, ipcMain);
bindingAssert.equal(alias.ipcMain, ipcMain);
const ipcProbe = () => {};
ipcMain.on('keld-gh466-packed-probe', ipcProbe);
bindingAssert.deepEqual(ipcMain.listeners('keld-gh466-packed-probe'), [ipcProbe]);
ipcMain.removeListener('keld-gh466-packed-probe', ipcProbe);
bindingAssert.equal(ipcMain.listenerCount('keld-gh466-packed-probe'), 0);
const bindingRefusal = (kind, member, ticket) => (error) => {
  bindingAssert.deepEqual(
    [error.name, error.code, error.kind, error.member, error.ticket],
    ['UnsupportedMemberError', 'KELD-COMPAT-013', kind, member, ticket],
  );
  return true;
};
bindingAssert.throws(() => session.defaultSession, bindingRefusal('property', 'session.defaultSession', 'GH-553'));
bindingAssert.throws(() => Menu.setApplicationMenu(null), bindingRefusal('method', 'Menu.setApplicationMenu(null)', 'GH-556'));
await bindingAssert.rejects(shell.openExternal('https://example.invalid'), bindingRefusal('promise', 'shell.openExternal', 'GH-479'));
await bindingAssert.rejects(clipboard.readText(), bindingRefusal('promise', 'clipboard.readText', 'GH-559'));
bindingAssert.throws(() => nativeImage.createFromDataURL('data:,'), bindingRefusal('method', 'nativeImage.createFromDataURL', 'GH-559'));
const bindingItem = new ClipboardItem({ 'text/plain': 'probe' });
bindingAssert.throws(() => bindingItem.types, bindingRefusal('property', 'ClipboardItem.types', 'GH-559'));
bindingAssert.equal(bindingCalls, 0);
`;
}

test("ipcMain bundling keeps the canonical KIPC provider external", async () => {
  const result = await Bun.build({
    entrypoints: [join(import.meta.dir, "ipc-main.ts")],
    target: "node",
    format: "esm",
    packages: "external",
  });
  expect(result.success).toBe(true);
  expect(result.outputs).toHaveLength(1);
  const source = await result.outputs[0]!.text();
  expect(source).not.toMatch(/\bclass\s+WorkerLink\s*\{/);
  expect(source).toContain('from "@keld/kipc"');
});

// Package graph evidence only, not native Host or original-application acceptance.
test("packed Electron uses one declared API/KIPC graph and all five aliases", () => {
  const root = mkdtempSync(join(tmpdir(), "keld-electron-artifacts-"));
  const packages = resolve(import.meta.dir, "../..");
  const run = (args: string[], cwd: string) =>
    Bun.spawnSync(args, { cwd, stdout: "pipe", stderr: "pipe" });
  const success = (p: ReturnType<typeof run>) =>
    expect(
      p.exitCode,
      new TextDecoder().decode(p.stderr) + new TextDecoder().decode(p.stdout),
    ).toBe(0);
  try {
    success(
      run(
        [
          process.execPath,
          join(packages, "kipc/scripts/package-artifacts.ts"),
          root,
          "--electron",
        ],
        packages,
      ),
    );
    const consumer = join(root, "consumer");
    mkdirSync(consumer);
    writeFileSync(
      join(consumer, "package.json"),
      JSON.stringify({
        type: "module",
        dependencies: {
          electron: "file:../keld-electron-0.0.1.tgz",
        },
        overrides: {
          "@keld/api": "file:../keld-api-0.0.1.tgz",
          "@keld/kipc": "file:../keld-kipc-0.0.1.tgz",
          "@types/node": "26.4.1",
          "undici-types": "8.3.0",
        },
      }),
    );
    success(
      run(
        [
          process.execPath,
          "install",
          "--offline",
          "--ignore-scripts",
          "--linker",
          "hoisted",
        ],
        consumer,
      ),
    );
    // The current Bun Electron packer preserves class names in its ESM output.
    const electronEntry = join(consumer, "node_modules/electron/dist/index.js");
    expect(readFileSync(electronEntry, "utf8")).not.toMatch(
      /\bclass\s+WorkerLink\s*\{/,
    );
    writeFileSync(
      join(consumer, "probe.mjs"),
      `
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { isCallError } from '@keld/kipc';
const require = createRequire(import.meta.url);
` +
        namedDesktopBindings("electron") +
        `
const main = await import('electron');
assert.equal(main.isCallError, isCallError);
for (const name of ['electron','electron/main','electron/renderer','electron/common','electron/utility']) {
 const esm = await import(name); const cjs = require(name);
 assert.equal(esm.app, main.app); assert.equal(cjs.app, main.app);
 assert.equal(esm.ipcMain, main.ipcMain); assert.equal(cjs.ipcMain, main.ipcMain);
 assert.equal(esm.default, main.default); assert.equal(cjs.default, main.default);
 assert.equal(typeof esm.dialog.showMessageBoxSync, 'function');
 assert.equal(esm.dialog.showMessageBoxSync, main.dialog.showMessageBoxSync);
 assert.equal(cjs.dialog.showMessageBoxSync, main.dialog.showMessageBoxSync);
 assert.throws(() => esm.dialog.showMessageBoxSync({ message: 'Confirm' }),
  error => error.code === (process.platform === 'win32' ? 'KELD-CORE-040' : 'KELD-NATIVE-010'));
 for (const name of ['session','Menu','shell','ClipboardItem','clipboard','nativeImage']) {
  assert.equal(esm[name], main[name]); assert.equal(cjs[name], main[name]);
 }
}
`,
    );
    for (const runtime of [process.execPath, "node"])
      success(run([runtime, "probe.mjs"], consumer));
    writeFileSync(
      join(consumer, "consumer.ts"),
      `import electron, {type OpenDialogOptions, BrowserWindow, session, Menu, shell, ClipboardItem, clipboard, nativeImage} from 'electron';
const x: OpenDialogOptions = {};
declare const parent: BrowserWindow;
const result: number = electron.dialog.showMessageBoxSync(parent, {message: 'Confirm'});
// @ts-expect-error arbitrary objects do not carry a BrowserWindow identity
electron.dialog.showMessageBoxSync({}, {message: 'Confirm'});
void [electron, BrowserWindow, x, session, Menu, shell, ClipboardItem, clipboard, nativeImage, result];`,
    );
    writeFileSync(
      join(consumer, "tsconfig.json"),
      JSON.stringify({
        compilerOptions: {
          strict: true,
          noEmit: true,
          target: "ES2024",
          module: "Preserve",
          moduleResolution: "Bundler",
          lib: ["ES2024", "DOM"],
        },
        files: ["consumer.ts"],
      }),
    );
    success(
      run(
        [
          join(packages, "electron/node_modules/.bin/tsc"),
          "-p",
          "tsconfig.json",
        ],
        consumer,
      ),
    );
    const manifest = join(consumer, "node_modules/electron/package.json");
    const original = readFileSync(manifest, "utf8");
    const changed = JSON.parse(original);
    delete changed.exports["./main"];
    writeFileSync(manifest, JSON.stringify(changed));
    for (const runtime of [process.execPath, "node"])
      expect(run([runtime, "probe.mjs"], consumer).exitCode).not.toBe(0);
    writeFileSync(manifest, original);
    for (const dependency of ["api", "kipc"]) {
      const path = join(consumer, "node_modules/@keld", dependency);
      renameSync(path, `${path}-absent`);
      for (const runtime of [process.execPath, "node"])
        expect(run([runtime, "probe.mjs"], consumer).exitCode).not.toBe(0);
      renameSync(`${path}-absent`, path);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}, 30_000);

test("source-copy staging preserves one resolvable declared graph", () => {
  const root = mkdtempSync(join(tmpdir(), "keld-source-packages-"));
  const packages = resolve(import.meta.dir, "../..");
  try {
    for (const name of ["api", "electron", "kipc"]) {
      const target = join(root, "node_modules/@keld", name);
      mkdirSync(target, { recursive: true });
      cpSync(join(packages, name, "src"), join(target, "src"), {
        recursive: true,
      });
      cpSync(
        join(packages, name, "package.json"),
        join(target, "package.json"),
      );
    }
    writeFileSync(
      join(root, "probe.mjs"),
      namedDesktopBindings("@keld/electron") +
        `const {isCallError: e} = await import('@keld/electron'); const {isCallError: k} = await import('@keld/kipc'); if(e !== k) throw Error('split transport');`,
    );
    const p = Bun.spawnSync([process.execPath, join(root, "probe.mjs")], {
      cwd: root,
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(p.exitCode, new TextDecoder().decode(p.stderr)).toBe(0);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

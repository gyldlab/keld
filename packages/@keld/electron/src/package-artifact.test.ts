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
    // Pack the exact declaration dependencies from the frozen producer install.
    // A fresh offline consumer must not depend on a populated registry cache.
    const declarations = Object.fromEntries(
      ["@types/node", "undici-types"].map((name) => {
        const installed = join(packages, "electron/node_modules", name);
        const packed = run(
          [
            "npm",
            "pack",
            "--offline",
            "--ignore-scripts",
            "--json",
            "--cache",
            join(root, ".pack-cache"),
            installed,
          ],
          root,
        );
        success(packed);
        const [{ filename }] = JSON.parse(
          new TextDecoder().decode(packed.stdout),
        );
        return [name, `file:../${filename}`];
      }),
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
          ...declarations,
        },
      }),
    );
    success(
      run(
        [
          "npm",
          "install",
          "--offline",
          "--ignore-scripts",
          "--no-audit",
          "--no-fund",
          "--cache",
          join(consumer, ".npm-cache"),
        ],
        consumer,
      ),
    );
    writeFileSync(
      join(consumer, "probe.mjs"),
      `
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import * as main from 'electron';
import { isCallError } from '@keld/kipc';
const require = createRequire(import.meta.url);
assert.equal(main.isCallError, isCallError);
for (const name of ['electron','electron/main','electron/renderer','electron/common','electron/utility']) {
 const esm = await import(name); const cjs = require(name);
 assert.equal(esm.app, main.app); assert.equal(cjs.app, main.app);
 assert.equal(esm.default, main.default); assert.equal(cjs.default, main.default);
}
`,
    );
    for (const runtime of [process.execPath, "node"])
      success(run([runtime, "probe.mjs"], consumer));
    writeFileSync(
      join(consumer, "consumer.ts"),
      `import electron, {type OpenDialogOptions, BrowserWindow} from 'electron'; const x: OpenDialogOptions = {}; void [electron, BrowserWindow, x];`,
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
      join(root, "probe.ts"),
      `import {isCallError as e} from './node_modules/@keld/electron/src/index.ts'; import {isCallError as k} from '@keld/kipc'; if(e !== k) throw Error('split transport');`,
    );
    const p = Bun.spawnSync([process.execPath, join(root, "probe.ts")], {
      cwd: root,
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(p.exitCode, new TextDecoder().decode(p.stderr)).toBe(0);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

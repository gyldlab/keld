import { expect, test } from "bun:test";
import {
  cpSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

interface Observation {
  namespace: string[];
  defaultKeys: string[];
  electronVersion: string;
}

// Exercise the actual probe and real package sources, never an export stub.
function withPackages(run: (root: string) => void): void {
  const root = mkdtempSync(join(tmpdir(), "keld-gh569-regression-"));
  try {
    for (const name of ["electron", "api", "kipc"]) {
      const target = join(root, name);
      mkdirSync(target);
      for (const path of ["src", "package.json"]) {
        cpSync(
          resolve(import.meta.dir, `../../${name}/${path}`),
          join(target, path),
          {
            recursive: true,
          },
        );
      }
    }
    const fixtures = join(root, "electron/fixtures");
    mkdirSync(fixtures);
    for (const name of [
      "module_process_probe.ts",
      "module_export_mutation_probe.ts",
    ]) {
      cpSync(join(import.meta.dir, name), join(fixtures, name));
    }
    run(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function probe(root: string, name: string) {
  const env = Object.fromEntries(
    Object.entries(process.env).filter(([key]) => !key.startsWith("KELD_")),
  );
  env.NODE_PATH = "";
  env.NODE_OPTIONS = "";
  return Bun.spawnSync(
    [process.execPath, "--no-install", join(root, "electron/fixtures", name)],
    { cwd: root, env },
  );
}

function observe(root: string, name: string): Observation {
  const result = probe(root, name);
  expect(result.exitCode, result.stderr.toString()).toBe(0);
  return JSON.parse(result.stdout.toString()) as Observation;
}

// Reproduce the SDK package edge without importing an unmerged branch or
// replacing the transport implementation. The package export map must resolve.
function usePackageTransport(root: string): void {
  const api = join(root, "api/src");
  let replacements = 0;
  for (const name of readdirSync(api)) {
    if (!name.endsWith(".ts")) continue;
    const path = join(api, name);
    const source = readFileSync(path, "utf8");
    const changed = source.replaceAll(
      "../../kipc/src/transport.ts",
      "@keld/kipc",
    );
    if (source !== changed) {
      replacements++;
      writeFileSync(path, changed);
    }
  }
  expect(replacements).toBeGreaterThan(0);
  const scope = join(root, "api/node_modules/@keld");
  mkdirSync(scope, { recursive: true });
  cpSync(join(root, "kipc"), join(scope, "kipc"), { recursive: true });
}

for (const block of [
  "export { app, BrowserWindow };",
  "export {\n  app,\n  BrowserWindow,\n};",
  "export { BrowserWindow, app };",
]) {
  test(`mutation removes only BrowserWindow from ${JSON.stringify(block)}`, () => {
    withPackages((root) => {
      const entry = join(root, "electron/src/index.ts");
      const source = readFileSync(entry, "utf8");
      expect(source).toContain("export { app, BrowserWindow };");
      writeFileSync(
        entry,
        source.replace("export { app, BrowserWindow };", block),
      );
      const before = observe(root, "module_process_probe.ts");
      expect(before.namespace).toEqual([
        "BrowserWindow",
        "ELECTRON_VERSION_SHIM",
        "app",
        "isCallError",
      ]);
      expect(before.defaultKeys).toEqual(["BrowserWindow", "app"]);
      const after = observe(root, "module_export_mutation_probe.ts");
      expect(after).toEqual({
        ...before,
        namespace: before.namespace.filter((name) => name !== "BrowserWindow"),
      });
      expect(readFileSync(entry, "utf8")).toBe(
        source.replace("export { app, BrowserWindow };", block),
      );
    });
  });
}

test("mutation retains the real package manifest and local API-to-KIPC graph", () => {
  withPackages((root) => {
    usePackageTransport(root);
    const before = observe(root, "module_process_probe.ts");
    const after = observe(root, "module_export_mutation_probe.ts");
    expect(after).toEqual({
      ...before,
      namespace: before.namespace.filter((name) => name !== "BrowserWindow"),
    });
  });
});

test("missing local KIPC export target is refused for the dependency failure", () => {
  withPackages((root) => {
    usePackageTransport(root);
    observe(root, "module_export_mutation_probe.ts");
    const manifest = join(root, "kipc/package.json");
    writeFileSync(
      manifest,
      readFileSync(manifest, "utf8").replace(
        "./src/transport.ts",
        "./src/missing-transport.ts",
      ),
    );
    const result = probe(root, "module_export_mutation_probe.ts");
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr.toString()).toContain("unmutated facade probe failed");
    expect(result.stderr.toString()).toContain("@keld/kipc");
    expect(result.stderr.toString()).toMatch(/Cannot find|ModuleNotFound/);
    expect(result.stdout.toString()).toBe("");
  });
});

test("an absent named BrowserWindow export is not accepted as a mutation", () => {
  withPackages((root) => {
    const entry = join(root, "electron/src/index.ts");
    writeFileSync(
      entry,
      readFileSync(entry, "utf8").replace(
        "export { app, BrowserWindow };",
        "export { app };",
      ),
    );
    const result = probe(root, "module_export_mutation_probe.ts");
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr.toString()).toContain(
      "expected one explicit BrowserWindow value-export block",
    );
    expect(result.stdout.toString()).toBe("");
  });
});

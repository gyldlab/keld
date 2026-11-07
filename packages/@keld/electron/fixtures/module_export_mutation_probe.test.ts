import { expect, test } from "bun:test";
import {
  cpSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
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
    const scope = join(root, "api/node_modules/@keld");
    mkdirSync(scope, { recursive: true });
    cpSync(join(root, "kipc"), join(scope, "kipc"), { recursive: true });
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
  const link = join(root, "api/src/link.ts");
  const source = readFileSync(link, "utf8");
  const changed = source.replaceAll(
    "../../kipc/src/transport.ts",
    "@keld/kipc",
  );
  expect(changed).toMatch(/from\s+["']@keld\/kipc["']/);
  writeFileSync(link, changed);
}

// This parser only prepares formatting variants. The oracle below is the
// independently imported namespace/default object, not these source tokens.
function valueExportBlock(source: string) {
  const blocks = [...source.matchAll(/\bexport\s*\{([^{}]*)\}/g)].map(
    (match) => ({
      text: match[0],
      names: match[1]
        .split(",")
        .map((name) => name.trim())
        .filter(Boolean),
    }),
  );
  const matches = blocks.filter((block) =>
    block.names.includes("BrowserWindow"),
  );
  expect(matches).toHaveLength(1);
  return matches[0];
}

for (const format of ["single-line", "multiline", "BrowserWindow-first"]) {
  test(`mutation removes only BrowserWindow from ${format} exports`, () => {
    withPackages((root) => {
      const entry = join(root, "electron/src/index.ts");
      const source = readFileSync(entry, "utf8");
      const original = observe(root, "module_process_probe.ts");
      expect(original.namespace).toContain("BrowserWindow");
      expect(original.defaultKeys).toContain("BrowserWindow");
      const block = valueExportBlock(source);
      const names =
        format === "BrowserWindow-first"
          ? [
              "BrowserWindow",
              ...block.names.filter((name) => name !== "BrowserWindow"),
            ]
          : block.names;
      const formatted =
        format === "multiline"
          ? `export {\n  ${names.join(",\n  ")},\n}`
          : `export { ${names.join(", ")} }`;
      const prepared = source.replace(block.text, formatted);
      writeFileSync(entry, prepared);
      const before = observe(root, "module_process_probe.ts");
      expect(before).toEqual(original);
      const after = observe(root, "module_export_mutation_probe.ts");
      expect(after).toEqual({
        ...before,
        namespace: before.namespace.filter((name) => name !== "BrowserWindow"),
      });
      expect(readFileSync(entry, "utf8")).toBe(prepared);
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
    const source = readFileSync(entry, "utf8");
    const before = observe(root, "module_process_probe.ts");
    expect(before.namespace).toContain("BrowserWindow");
    const block = valueExportBlock(source);
    writeFileSync(
      entry,
      source.replace(
        block.text,
        `export { ${block.names.filter((name) => name !== "BrowserWindow").join(", ")} }`,
      ),
    );
    expect(observe(root, "module_process_probe.ts")).toEqual({
      ...before,
      namespace: before.namespace.filter((name) => name !== "BrowserWindow"),
    });
    const result = probe(root, "module_export_mutation_probe.ts");
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr.toString()).toContain(
      "expected one explicit BrowserWindow value-export block",
    );
    expect(result.stdout.toString()).toBe("");
  });
});

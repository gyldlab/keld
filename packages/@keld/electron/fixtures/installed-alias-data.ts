import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  readFileSync,
  readdirSync,
  realpathSync,
  writeFileSync,
} from "node:fs";
import { createRequire, isBuiltin } from "node:module";
import { join, sep } from "node:path";

function json(path: string): any {
  return JSON.parse(readFileSync(path, "utf8"));
}
function hash(path: string): string {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}
function writeJson(path: string, value: unknown): void {
  writeFileSync(path, JSON.stringify(value, null, 2) + "\n");
}

function files(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    assert(
      !entry.isSymbolicLink(),
      "Installed artifact must not escape through a source symlink: " + path,
    );
    return entry.isDirectory() ? files(path) : [path];
  });
}

function within(child: string, parent: string): boolean {
  return child === parent || child.startsWith(parent + sep);
}

export function checkInstalledData(
  consumer: string,
  versions: Record<string, string>,
  nodeTypesVersion: string,
  receipts: string,
  extraPackages: readonly string[] = ["@keld/gh502-probe-dependency"],
): void {
  const alias = join(consumer, "node_modules/electron");
  const manifest = json(join(alias, "package.json"));
  assert.equal(manifest.name, "@keld/electron");
  assert.deepEqual(
    Object.keys(manifest.exports).sort(),
    [".", "./main", "./renderer", "./common", "./utility"].sort(),
  );
  assert.deepEqual(
    Object.values(manifest.exports),
    Array(5).fill("./dist/index.js"),
    "GH502_ONE_ENTRY",
  );
  assert.equal(manifest.dependencies["@keld/api"], versions.api);
  assert.equal(manifest.dependencies["@keld/kipc"], versions.kipc);
  assert.equal(manifest.dependencies["@types/node"], nodeTypesVersion);
  const api = json(join(consumer, "node_modules/@keld/api/package.json"));
  assert.equal(api.dependencies["@keld/kipc"], versions.kipc);
  assert(
    api.exports["./internal/dialog-codec"],
    "Save runtime codec needs a declared private export",
  );
  // Read only trusted local fixture artifacts before importing their code.
  // This bounded static fixture check does not replace the owning app admission gate.
  const scanner = new Bun.Transpiler({ loader: "js" });
  const edges: unknown[] = [];
  for (const relative of [
    "electron",
    "@keld/api",
    "@keld/kipc",
    ...extraPackages,
  ]) {
    const packageRoot = join(consumer, "node_modules", relative);
    const own = json(join(packageRoot, "package.json"));
    for (const file of files(packageRoot).filter((path) =>
      /\.(mjs|cjs|js)$/.test(path),
    )) {
      const require = createRequire(file);
      for (const edge of scanner.scanImports(readFileSync(file, "utf8"))) {
        const specifier = edge.path;
        if (isBuiltin(specifier)) continue;
        assert(
          !specifier.startsWith("file:") && !specifier.startsWith("/"),
          "Absolute source escape: " + specifier,
        );
        if (!specifier.startsWith(".")) {
          const packageName = specifier.startsWith("@")
            ? specifier.split("/").slice(0, 2).join("/")
            : specifier.split("/")[0];
          assert(
            packageName === own.name || own.dependencies?.[packageName],
            "Undeclared runtime edge: " + own.name + " -> " + specifier,
          );
        }
        const target = realpathSync(require.resolve(specifier));
        if (specifier.startsWith("."))
          assert(
            within(target, packageRoot),
            "Relative import left its package root: " + specifier,
          );
        assert(
          within(target, consumer),
          "Resolved outside installed fixture: " + target,
        );
        edges.push({ file, specifier, target, sha256: hash(target) });
      }
    }
  }
  writeJson(join(receipts, "installed-static-edges.json"), edges);
}

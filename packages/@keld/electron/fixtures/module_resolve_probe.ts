/** GH-569 package-resolution observation, without evaluating any foreign module. */
import {
  cpSync,
  mkdtempSync,
  mkdirSync,
  rmSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = mkdtempSync(join(tmpdir(), "keld-gh569-resolve-"));
try {
  const modules = join(root, "node_modules");
  mkdirSync(modules);
  const packageRoot = join(modules, "electron");
  mkdirSync(packageRoot);
  cpSync(
    resolve(import.meta.dir, "../package.json"),
    join(packageRoot, "package.json"),
  );
  cpSync(resolve(import.meta.dir, "../src"), join(packageRoot, "src"), {
    recursive: true,
  });
  if (process.argv[2] === "add-utility") {
    const manifestPath = join(packageRoot, "package.json");
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    manifest.exports["./utility"] = "./src/index.ts";
    writeFileSync(manifestPath, JSON.stringify(manifest));
  }
  const caller = join(root, "caller.cjs");
  const localRequire = createRequire(caller);
  const result: Record<string, { resolved: boolean; code?: string }> = {};
  for (const name of [
    "electron",
    "electron/main",
    "electron/renderer",
    "electron/common",
    "electron/utility",
  ]) {
    try {
      localRequire.resolve(name);
      result[name] = { resolved: true };
    } catch (error) {
      if (
        !(error instanceof Error) ||
        !("code" in error) ||
        typeof error.code !== "string"
      )
        throw error;
      result[name] = { resolved: false, code: error.code };
    }
  }
  console.log(JSON.stringify(result));
} finally {
  rmSync(root, { recursive: true, force: true });
}

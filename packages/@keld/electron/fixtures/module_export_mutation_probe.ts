/** One-fault control: remove a real named export from a disposable Keld copy. */
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

const root = mkdtempSync(join(tmpdir(), "keld-gh569-export-"));
try {
  for (const name of ["electron", "api", "kipc"]) {
    const target = join(root, name);
    mkdirSync(target);
    cpSync(
      resolve(import.meta.dir, `../../${name}/package.json`),
      join(target, "package.json"),
    );
    cpSync(resolve(import.meta.dir, `../../${name}/src`), join(target, "src"), {
      recursive: true,
    });
  }
  // Keep the sibling-relative graph and the SDK's package import wholly local.
  const scope = join(root, "api/node_modules/@keld");
  mkdirSync(scope, { recursive: true });
  cpSync(join(root, "kipc"), join(scope, "kipc"), { recursive: true });
  const observer = join(root, "module_process_probe.ts");
  cpSync(resolve(import.meta.dir, "module_process_probe.ts"), observer);
  const env = Object.fromEntries(
    Object.entries(process.env).filter(([name]) => !name.startsWith("KELD_")),
  );
  env.NODE_PATH = "";
  env.NODE_OPTIONS = "";
  const entry = join(root, "electron/src/index.ts");
  const command = [process.execPath, "--no-install", observer, entry];
  const baseline = Bun.spawnSync(command, { cwd: root, env });
  if (baseline.exitCode !== 0)
    throw new Error(
      `unmutated facade probe failed: ${baseline.stderr.toString()}`,
    );
  const source = readFileSync(entry, "utf8");
  const exportBlocks = [...source.matchAll(/\bexport\s*\{[^{}]*\}/g)]
    .map((match) => match[0])
    .filter((block) => /\bBrowserWindow\b/.test(block));
  if (exportBlocks.length !== 1)
    throw new Error("expected one explicit BrowserWindow value-export block");
  const block = exportBlocks[0];
  const mutatedBlock = block.replace(/\bBrowserWindow\b\s*,?/, "");
  const mutated = source.replace(block, mutatedBlock);
  if (mutated === source)
    throw new Error("named export mutation was not applied");
  writeFileSync(entry, mutated);
  const result = Bun.spawnSync(command, { cwd: root, env });
  if (result.exitCode !== 0)
    throw new Error(`mutated facade probe failed: ${result.stderr.toString()}`);
  process.stdout.write(result.stdout);
} finally {
  rmSync(root, { recursive: true, force: true });
}

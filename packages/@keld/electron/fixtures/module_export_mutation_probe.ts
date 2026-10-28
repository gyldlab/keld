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
    cpSync(resolve(import.meta.dir, `../../${name}/src`), join(target, "src"), {
      recursive: true,
    });
  }
  const entry = join(root, "electron/src/index.ts");
  const source = readFileSync(entry, "utf8");
  const mutated = source.replace(
    "export { app, BrowserWindow };",
    "export { app };",
  );
  if (mutated === source)
    throw new Error("named export mutation was not applied");
  writeFileSync(entry, mutated);
  const result = Bun.spawnSync([
    process.execPath,
    "--no-install",
    resolve(import.meta.dir, "module_process_probe.ts"),
    entry,
  ]);
  if (result.exitCode !== 0)
    throw new Error(`mutated facade probe failed: ${result.stderr.toString()}`);
  process.stdout.write(result.stdout);
} finally {
  rmSync(root, { recursive: true, force: true });
}

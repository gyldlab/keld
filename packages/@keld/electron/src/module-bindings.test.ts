/**
 * GH-565 (F06-T8): link-time named bindings of the Electron v44.4.5 main-process modules
 * draw.io's main imports (`src/main/electron.js:5-6` at 2edf9fb9).
 *
 * Corpus `electron-main-bindings-v1` (`crates/keld-compat/fixtures/main-bindings-corpus/`)
 * maps one cell per binding to one exact case name below. Each case spawns a Bun child
 * that evaluates a one-line module importing that binding by name from `./index.ts`
 * (an isolated temp file, `file://` specifier). The oracle is Bun's own ESM linker: a
 * missing export is `SyntaxError: Export named '<name>' not found in module '…'.`, exit 1
 * (Bun 1.4.4, measured 2026-10-31).
 *
 * Red until GH-565 lands (gh532 rule 3): every cell case asserts today's link failure and
 * passes. The implementing change inverts the cases and flips the cells to `pass` in the
 * same diff. The `app` control proves the harness distinguishes a linked binding.
 */
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

/** The facade entry Electron code imports, as a specifier any directory can resolve. */
const INDEX_URL = pathToFileURL(join(import.meta.dir, "index.ts")).href;

/** Kill switch only: a linked or refused import settles far below this. */
const PROBE_KILL_MS = 10_000;

/** The ten bindings this slice adds, with their corpus operation prefix. */
const PENDING_BINDINGS: ReadonlyArray<readonly [prefix: string, name: string]> =
  [
    ["menu", "Menu"],
    ["shell", "shell"],
    ["dialog", "dialog"],
    ["session", "session"],
    ["screen", "screen"],
    ["clipboard-item", "ClipboardItem"],
    ["clipboard", "clipboard"],
    ["native-image", "nativeImage"],
    ["native-theme", "nativeTheme"],
    ["ipc-main", "ipcMain"],
  ];

interface ProbeResult {
  code: number | null;
  stdout: string;
  stderr: string;
}

/**
 * Evaluates `import { <names> } from "<index.ts>"` in a fresh Bun process and prints the
 * `typeof` of every binding. The module graph is the child's own, so a missing export is
 * observed as Bun's link-time SyntaxError and never as a cached namespace lookup.
 */
function linkProbe(names: readonly string[]): ProbeResult {
  const dir = mkdtempSync(join(tmpdir(), "gh565-link-"));
  try {
    const probe = join(dir, "probe.ts");
    const typeofs = names.map((name) => `typeof ${name}`).join(", ");
    writeFileSync(
      probe,
      `import { ${names.join(", ")} } from ${JSON.stringify(INDEX_URL)};\n` +
        `console.log(JSON.stringify([${typeofs}]));\n`,
    );
    const child = Bun.spawnSync({
      cmd: [process.execPath, probe],
      cwd: dir,
      env: process.env,
      stdout: "pipe",
      stderr: "pipe",
      timeout: PROBE_KILL_MS,
    });
    return {
      code: child.exitCode,
      stdout: child.stdout.toString(),
      stderr: child.stderr.toString(),
    };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** Bun's exact link failure for one missing named export of the facade entry. */
function exportNotFound(name: string): RegExp {
  return new RegExp(
    `SyntaxError: Export named '${name}' not found in module '[^']*index\\.ts'\\.`,
  );
}

describe("harness control", () => {
  test("app and BrowserWindow link today, so a linked binding is distinguishable from a missing one", () => {
    const result = linkProbe(["app", "BrowserWindow"]);
    expect(result.stderr).toBe("");
    expect(result.code).toBe(0);
    expect(JSON.parse(result.stdout)).toEqual(["object", "function"]);
  });
});

describe("GH-565 link-time cells, red until implemented", () => {
  for (const [prefix, name] of PENDING_BINDINGS) {
    test(`${prefix}.binding.named-import today: importing { ${name} } by name from the facade fails to link with Bun's Export-named-not-found SyntaxError`, () => {
      const result = linkProbe([name]);
      expect(result.code).toBe(1);
      expect(result.stdout).toBe("");
      expect(result.stderr).toMatch(exportNotFound(name));
    });
  }
});

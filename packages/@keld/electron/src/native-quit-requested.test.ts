// Source contracts only; injected selection/Source data is not Host/native qualification.
import { expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

test("spawned Bun fixtures use native paths, including URL-escaped names", () => {
  const directory = mkdtempSync(join(tmpdir(), "keld-electron-url-path-"));
  try {
    const script = join(directory, "fixture space %.ts");
    writeFileSync(script, 'console.log("FIXTURE_PATH_OK");\n');
    const scriptURL = pathToFileURL(script);
    const run = (filename: string) =>
      Bun.spawnSync({
        cmd: [process.execPath, "--no-install", "--no-env-file", filename],
        stdout: "pipe",
        stderr: "pipe",
        timeout: 4000,
      });
    const valid = run(fileURLToPath(scriptURL));
    expect(valid.exitCode, valid.stderr.toString()).toBe(0);
    expect(valid.stdout.toString()).toBe("FIXTURE_PATH_OK\n");

    // The legacy URL pathname is not a native path on Windows and leaves URL
    // escapes in the filename on POSIX. A missing file must also fail closed.
    const malformed = run(scriptURL.pathname);
    expect(malformed.exitCode).not.toBe(0);
    expect(malformed.stdout.toString()).not.toContain("FIXTURE_PATH_OK");
    const missing = run(fileURLToPath(new URL("./missing.ts", scriptURL)));
    expect(missing.exitCode).not.toBe(0);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

const developmentFacts = {
  schema: 5,
  kind: "development",
  name: "model",
  root: "/selected",
  paths: {},
  originalResourcePolicy: {
    contract: "gh473-original-editor-offline/v1",
    graphSha256: `sha256:${"1".repeat(64)}`,
    policySha256: `sha256:${"2".repeat(64)}`,
    windowKey: "editor",
    selector: "original-editor-offline",
  },
};
const installedFacts = JSON.parse(
  readFileSync(
    new URL(
      "../../kipc/test/fixtures/installed-launch-facts-v7.json",
      import.meta.url,
    ),
    "utf8",
  ),
) as Record<string, unknown>;
const modes = [
  "request",
  "request-error",
  "veto",
  "zero",
  "zero-before-overwrite",
  "zero-before-global-noop",
  "zero-before-global-throw",
  "zero-before-getter-spoof",
  "zero-off-before-clean",
  "zero-off-before-splice-poison",
  "zero-off-will-clean",
  "zero-off-will-splice-poison",
  "zero-register-before-push-noop",
  "zero-register-will-push-noop",
  "zero-before-reflect-noop",
  "zero-before-reflect-throw",
  "zero-before-slice-noop",
  "zero-before-slice-throw",
  "zero-will-veto",
  "zero-will-overwrite",
  "zero-will-global-noop",
  "zero-will-global-throw",
  "zero-will-getter-spoof",
  "zero-will-reflect-noop",
  "zero-will-reflect-throw",
  "zero-will-slice-noop",
  "zero-will-slice-throw",
  "zero-will-retry",
  "zero-will-throw",
  "zero-will-create",
  "zero-preexisting-pending-create",
  "zero-preexisting-pending-preload",
  "zero-dynamic-from-create",
  "zero-dynamic-from-preload",
  "zero-dynamic-proxy-from-preload",
  "zero-dynamic-will-blocking-from-create",
  "zero-dynamic-will-blocking-from-preload",
  "zero-dynamic-to-create",
  "zero-dynamic-to-preload",
  "zero-before-pending-create",
  "zero-before-pending-preload",
  "zero-before-transient-create",
  "zero-will-pending-create",
  "zero-will-pending-preload",
  "zero-will-transient-create",
  "zero-will-transient-preload",
  "zero-will-transient-blocking-create",
  "zero-will-transient-blocking-preload",
  "zero-will-transient-sync-throw-create",
  "live-census-filter-only",
  "live-census-some-only",
  "live-census-both",
  "live-census-index-setter",
  "zero-will-index-setter",
  "multi",
  "missing-selection",
  "pre-ready",
  "retired",
  "ended",
  "wrong-session",
  "observer-retires",
  "facade-disabled",
  "runtime-disabled",
  "scope-stale-window",
  "scope-noncurrent-document",
  "scope-unfinished-document",
  "scope-destroyed-window",
  "scope-retired",
  "scope-ended",
  "scope-replaced-session",
];
const cases = [developmentFacts, installedFacts].flatMap((facts) =>
  modes.map((mode) => ({ facts, mode })),
);
cases.push({
  mode: "schema-denied",
  facts: {
    schema: 6,
    kind: installedFacts.kind,
    name: installedFacts.name,
    version: installedFacts.version,
    root: installedFacts.root,
    resourcesRoot: installedFacts.resourcesRoot,
  },
});

for (const { facts, mode } of cases) {
  test(`qualified native Quit intent uses the existing Source/app.quit owner: schema ${facts.schema}, ${mode}`, () => {
    const selected =
      mode === "missing-selection"
        ? {
            schema: 4,
            kind: "development",
            name: "model",
            root: "/selected",
            paths: {},
          }
        : facts;
    const child = Bun.spawnSync({
      cmd: [
        process.execPath,
        "--no-install",
        "--no-env-file",
        `--define=__KELD_HOST_LAUNCH_FACTS__:${JSON.stringify(selected)}`,
        `--define=__KELD_RUNTIME_PROCESS_FACTS__:${mode !== "runtime-disabled"}`,
        fileURLToPath(
          new URL("../fixtures/native_quit_requested.ts", import.meta.url),
        ),
        mode,
      ],
      env: {
        ...process.env,
        KELD_NATIVE_WINDOW_FACADE: mode === "facade-disabled" ? "0" : "1",
      },
      stdout: "pipe",
      stderr: "pipe",
      timeout: 4000,
    });
    expect(
      child.exitCode,
      child.stdout.toString() + child.stderr.toString(),
    ).toBe(0);
    if (mode === "request-error") {
      expect(child.stderr.toString()).toContain(
        "KELD native menu Quit failed;",
      );
      expect(child.stderr.toString()).toContain(
        "KELD-IPC-006: controlled original RequestClose failure",
      );
    } else if (
      mode === "zero-will-create" ||
      mode?.startsWith("zero-preexisting-pending-") ||
      (mode?.startsWith("zero-dynamic-") && mode.includes("-from-")) ||
      mode?.startsWith("zero-before-pending-") ||
      mode?.startsWith("zero-before-transient-") ||
      mode?.startsWith("zero-will-pending-") ||
      mode?.startsWith("zero-will-transient-") ||
      mode === "multi" ||
      mode?.startsWith("live-census-")
    ) {
      expect(child.stderr.toString()).toContain(
        "KELD native menu Quit failed;",
      );
      expect(child.stderr.toString()).toContain("KELD-COMPAT-012");
    } else expect(child.stderr.toString()).toBe("");
    expect(child.stdout.toString()).toBe(
      `PASS native-quit-requested-source ${mode}\n`,
    );
  });
}

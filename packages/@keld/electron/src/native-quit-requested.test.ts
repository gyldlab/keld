// Source contracts only; injected selection/Source data is not Host/native qualification.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

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
        new URL("../fixtures/native_quit_requested.ts", import.meta.url)
          .pathname,
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
    } else if (mode === "multi") {
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

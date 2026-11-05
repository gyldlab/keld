// Source contracts only; injected selection/Source data is not Host/native qualification.
import { expect, test } from "bun:test";
const facts = {
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
for (const mode of [
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
]) {
  test(`qualified native Quit intent uses the existing Source/app.quit owner: ${mode}`, () => {
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
        "--define=__KELD_RUNTIME_PROCESS_FACTS__:true",
        new URL("../fixtures/native_quit_requested.ts", import.meta.url)
          .pathname,
        mode,
      ],
      env: { ...process.env, KELD_NATIVE_WINDOW_FACADE: "1" },
      stdout: "pipe",
      stderr: "pipe",
      timeout: 4000,
    });
    expect(child.exitCode).toBe(0);
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

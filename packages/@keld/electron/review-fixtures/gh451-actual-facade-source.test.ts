// Independently discoverable test; all package aliases resolve to existing
// Owned by the Electron package's review-only test suite.
// checked-out sources. No dependency installation or shipping source edits.
import { expect, test } from "bun:test";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

test("GH451 actual app + BrowserWindow + WindowSource: held Veto receipt then physical Destroyed", () => {
  const base = fileURLToPath(new URL("../..", import.meta.url));
  const directory = mkdtempSync(join(tmpdir(), "gh451-actual-facade-"));
  try {
    const config = join(directory, "tsconfig.json");
    writeFileSync(
      config,
      JSON.stringify({
        compilerOptions: {
          baseUrl: base,
          paths: {
            "@keld/api/internal/app": ["api/src/app.ts"],
            "@keld/api/internal/window-codec": ["api/src/window-codec.ts"],
            "@keld/kipc": ["kipc/src/transport.ts"],
            "@keld/kipc/endpoint-value-codec": [
              "kipc/src/endpoint-value-codec.ts",
            ],
            "@keld/kipc/compat-control": [
              "kipc/src/compat-control.generated.ts",
            ],
            "@keld/kipc/runtime-process-facts": [
              "kipc/src/runtime-process.generated.ts",
            ],
          },
        },
      }),
    );
    const facts = {
      schema: 5,
      kind: "development",
      name: "fixture",
      root: "/selected",
      paths: {},
      originalResourcePolicy: {
        contract: "gh473-original-editor-offline/v1",
        graphSha256: "sha256:" + "1".repeat(64),
        policySha256: "sha256:" + "2".repeat(64),
        windowKey: "editor",
        selector: "original-editor-offline",
      },
    };
    const child = Bun.spawnSync({
      cmd: [
        process.execPath,
        "--no-install",
        "--no-env-file",
        "--tsconfig-override=" + config,
        "--define=__KELD_HOST_LAUNCH_FACTS__:" + JSON.stringify(facts),
        "--define=__KELD_RUNTIME_PROCESS_FACTS__:true",
        fileURLToPath(
          new URL("./gh451-actual-facade-source.ts", import.meta.url),
        ),
      ],
      env: { ...process.env, KELD_NATIVE_WINDOW_FACADE: "1" },
      stdout: "pipe",
      stderr: "pipe",
      timeout: 4000,
    });
    expect(
      child.exitCode,
      child.stdout.toString() + child.stderr.toString(),
    ).toBe(0);
    expect(child.stderr.toString()).toBe("");
    expect(child.stdout.toString()).toBe(
      "PASS actual facade/Source close receipt integration\n",
    );
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

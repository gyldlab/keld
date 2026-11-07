import { expect, test } from "bun:test";
import { join, resolve } from "node:path";

const role = join(import.meta.dir, "ready-desktop-platform.test-role.ts");
const transport = resolve(import.meta.dir, "../../kipc/src/transport.ts");
const qualifiedMacDevFacts = {
  schema: 3,
  kind: "development",
  name: "gh481-s1",
  root: "/qualified-native-stage",
  paths: {},
  locale: "en-US",
};

for (const mode of [
  "other-facade",
  "other-renderer",
  "mac-valid",
  "mac-missing",
  "mac-no-mirror",
  "mac-invalidated",
]) {
  test(`GH481 S1 platform/Ready/zero-effect ${mode}`, () => {
    // Existing Guardian defines these two literals before the app entry.
    // Only the macOS dev-host test carries its existing Host launch-facts
    // record; Windows/Linux Host paths never mint this record. This test
    // models the platform seam but cannot claim a real Windows/Linux run.
    const payload = mode.startsWith("mac-")
      ? JSON.stringify(qualifiedMacDevFacts)
      : "undefined";
    const child = Bun.spawnSync({
      cmd: [
        process.execPath,
        "--no-install",
        "--no-env-file",
        `--define=process.env.KELD_NATIVE_WINDOW_FACADE:"${mode === "other-renderer" ? "0" : "1"}"`,
        `--define=__KELD_HOST_LAUNCH_FACTS__:${payload}`,
        `--preload=${transport}`,
        role,
        mode,
      ],
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
      timeout: 5_000,
    });
    expect(child.exitCode).toBe(0);
    expect(child.stdout.toString()).toBe(`PASS GH481 ${mode}\n`);
    expect(child.stderr.toString()).toBe("");
  });
}

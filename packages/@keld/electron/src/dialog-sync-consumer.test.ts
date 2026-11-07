/**
 * GH564 failure-first source-boundary tests. Each child uses the real API
 * singleton, WindowSource, BrowserWindow brand and Electron dialog facade.
 * Only LifecycleLink's Host seam is controlled. These cases do not establish
 * WorkerLink parking, native clicks, visibility, Guard or physical cleanup.
 */
import { expect, test } from "bun:test";
import { resolve } from "node:path";

const role = resolve(
  import.meta.dir,
  "../../api/src/dialog-sync-consumer.test-role.ts",
);
const transport = resolve(import.meta.dir, "../../kipc/src/transport.ts");

for (const mode of [
  "unconnected",
  "connected-before-ready",
  "ready-indices",
  "source-validation",
  "failed-link",
  "quitting",
  "native-failure",
  "wrong-reply",
]) {
  test("GH564 synchronous consumer source boundary: " + mode, () => {
    const child = Bun.spawnSync({
      cmd: [
        process.execPath,
        "--no-install",
        "--no-env-file",
        '--define=process.env.KELD_NATIVE_WINDOW_FACADE:"1"',
        "--define=__KELD_HOST_LAUNCH_FACTS__:undefined",
        "--preload=" + transport,
        role,
        mode,
      ],
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
      timeout: 5_000,
    });
    expect(
      child.exitCode,
      mode + "\n" + child.stdout.toString() + child.stderr.toString(),
    ).toBe(0);
    expect(child.stdout.toString()).toBe("PASS GH564 source " + mode + "\n");
    expect(child.stderr.toString()).toBe("");
  });
}

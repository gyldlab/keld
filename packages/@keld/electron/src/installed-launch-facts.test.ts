/** GH-1404 §4.5: the installed private record is data, never a grant. */
import { expect, test } from "bun:test";
import { join } from "node:path";

const observer = join(
  import.meta.dir,
  "..",
  "fixtures",
  "installed_launch_facts_probe.ts",
);

test("installed launch facts: the private installed record reaches the facade without environment or path inference", () => {
  const record = {
    schema: 6,
    kind: "local-development",
    name: "Installed Facts",
    root: "/Applications/Installed Facts.app/Contents/Resources/app",
    version: "1.2.3",
    resourcesRoot: "/Applications/Installed Facts.app/Contents/Resources",
  };
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      `--define=__KELD_HOST_LAUNCH_FACTS__:${JSON.stringify(record)}`,
      '--define=process.env.KELD_NATIVE_WINDOW_FACADE:"0"',
      "run",
      observer,
    ],
    env: {
      ...process.env,
      BUN_OPTIONS: "",
      KELD_APP_LINK: `1#${"ab".repeat(32)}`,
      KELD_APP_PATH: "/decoy/app",
      KELD_RESOURCES_PATH: "/decoy/resources",
      KELD_IS_PACKAGED: "false",
    },
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4_000,
  });
  expect(child.exitCode).toBe(0);
  expect(child.stderr.toString()).toBe("");
  const line = child.stdout
    .toString()
    .split("\n")
    .find((value) => value.startsWith("GH1404_INSTALLED_FACTS "));
  expect(line).toBeDefined();
  expect(JSON.parse(line!.slice("GH1404_INSTALLED_FACTS ".length))).toEqual({
    launchFacts: record,
    isPackaged: true,
    appPath: record.root,
    name: record.name,
    version: record.version,
    resourcesPath: record.resourcesRoot,
    defaultApp: true,
    execPath: process.execPath,
    argv: [process.execPath, observer],
  });
});

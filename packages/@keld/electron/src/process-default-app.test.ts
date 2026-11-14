/** GH570 main-role defaultApp and GH452 Host-fact absence; no original app execution. */
import { expect, test } from "bun:test";
import { join } from "node:path";

const transport = join(import.meta.dir, "../../kipc/src/transport.ts");
const role = join(import.meta.dir, "process-default-app.test-role.ts");
const development = {
  schema: 1,
  kind: "development",
  name: "independent development main",
  root: "/qualified-stage",
};

function run(
  mode: string,
  fact: unknown,
  args: string[] = [],
  supplied = true,
) {
  return Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      ...(supplied
        ? [`--define=__KELD_HOST_LAUNCH_FACTS__:${JSON.stringify(fact)}`]
        : []),
      `--preload=${transport}`,
      role,
      ...args,
    ],
    env: {
      ...process.env,
      // Same public role selection for all cases. This hint has no Host kind authority.
      KELD_NATIVE_WINDOW_FACADE: "1",
      KELD_TEST_DEFAULT_APP_CASE: mode,
    },
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
}

test("qualified development main excludes the Bun script from no-file and explicit-file inputs", () => {
  for (const [mode, args, count] of [
    ["development-empty", [], 0],
    ["development-file", ["input.drawio"], 1],
  ] as const) {
    const child = run(mode, development, [...args]);
    expect(child.exitCode, child.stderr.toString()).toBe(0);
    expect(child.stderr.toString()).toBe("");
    expect(JSON.parse(child.stdout.toString())).toEqual({
      mode,
      defaultApp: true,
      own: true,
      hostLaunchFactsPresent: true,
      originalArgvPreserved: true,
      readsDuringFacadeImport: 0,
      oldParsedCount: count + 1,
      parsedCount: count,
    });
  }
});

test("main-role defaultApp preserves Bun argument slicing without fabricating absent Host facts", () => {
  const child = run("missing-facts", undefined, [], false);
  expect(child.exitCode, child.stderr.toString()).toBe(0);
  expect(child.stderr.toString()).toBe("");
  expect(JSON.parse(child.stdout.toString())).toEqual({
    mode: "missing-facts",
    defaultApp: true,
    own: true,
    hostLaunchFactsPresent: false,
    originalArgvPreserved: true,
    readsDuringFacadeImport: 0,
    oldParsedCount: 1,
    parsedCount: 0,
  });
});

test("malformed or non-development Host facts fail before the application entry", () => {
  for (const value of [
    { ...development, kind: "installed" },
    { ...development, root: "relative" },
    { ...development, schema: 6 },
  ]) {
    const child = run("development-empty", value);
    expect(child.exitCode).not.toBe(0);
    expect(child.stderr.toString()).toContain("KELD-IPC-029");
    expect(child.stdout.toString()).toBe("");
  }
});

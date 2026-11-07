/** Real Bun pre-entry/facade observations; Host signing/native locks are separate tests. */
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const fixture = join(
  import.meta.dir,
  "../../kipc/test/fixtures/installed-launch-facts-v7.json",
);
const dependency = join(
  import.meta.dir,
  "../fixtures/installed_original_early_dependency.ts",
);
const preload = join(import.meta.dir, "installed-launch-preload.ts");
function original(): Record<string, unknown> {
  return JSON.parse(readFileSync(fixture, "utf8"));
}
function observe(value: unknown) {
  return Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      `--define=__KELD_HOST_LAUNCH_FACTS__:${JSON.stringify(value)}`,
      '--define=process.env.KELD_NATIVE_WINDOW_FACADE:"0"',
      `--preload=${preload}`,
      "-e",
      `import { earlyFacts } from ${JSON.stringify(dependency)};
       console.log("GH1404_ORIGINAL_FACTS " + JSON.stringify(earlyFacts));`,
    ],
    env: {
      ...process.env,
      BUN_OPTIONS: "",
      KELD_APP_LINK: `1#${"ab".repeat(32)}`,
      KELD_APP_USER_DATA: "/decoy/userData",
      KELD_APP_PATH: "/decoy/app",
      KELD_RESOURCES_PATH: "/decoy/resources",
      KELD_LOCALE: "zz-ZZ",
    },
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
}
function observed(value: unknown) {
  const child = observe(value);
  expect(child.exitCode).toBe(0);
  expect(child.stderr.toString()).toBe("");
  const line = child.stdout
    .toString()
    .split("\n")
    .find((value) => value.startsWith("GH1404_ORIGINAL_FACTS "));
  expect(line).toBeDefined();
  return JSON.parse(line!.slice("GH1404_ORIGINAL_FACTS ".length));
}

test("installed original facts reach the first dependency before Ready without environment inference", () => {
  expect(observed(original())).toEqual({
    ready: false,
    name: "Paper Map",
    version: "1.2.3",
    appPath: "/Applications/Paper Map.app/Contents/Resources/app",
    isPackaged: true,
    appData: "/Users/native/Library/Application Support",
    userData: "/Users/native/Library/Application Support/Paper Map",
    documents: "/Users/native/Documents",
    locale: "en-GB",
    singleInstance: true,
    defaultApp: true,
    resourcesPath: "/Applications/Paper Map.app/Contents/Resources",
  });
  expect(
    observed({
      ...original(),
      singleInstance: { verdict: "contended" },
    }).singleInstance,
  ).toBe(false);
});

test("missing optional native observations stay typed unavailable facts", () => {
  const value = original();
  value.paths = {};
  delete value.locale;
  delete value.singleInstance;
  const result = observed(value);
  for (const name of ["appData", "userData", "documents"]) {
    expect(result[name]).toEqual({
      code: "KELD-COMPAT-040",
      fact: `path.${name}`,
    });
  }
  expect(result.locale).toEqual({
    code: "KELD-COMPAT-040",
    fact: "locale",
  });
  expect(result.singleInstance).toEqual({
    code: "KELD-COMPAT-040",
    fact: "singleInstance",
    reason: "undeclared",
  });
});

test("native preferred language reaches early imports without normalization", () => {
  for (const locale of ["en_US", "EN-gb", "zh-Hant-TW", "é".repeat(80)]) {
    expect(observed({ ...original(), locale }).locale).toBe(locale);
  }
});

test("invalid installed original facts stop before the first application dependency", () => {
  const input = original();
  for (const value of [
    { ...input, name: ".." },
    { ...input, root: "/foreign/app" },
    { ...input, singleInstance: { verdict: "guessed" } },
    { ...input, paths: { userData: "/decoy" } },
    { ...input, locale: "EN\0gb" },
    { ...input, paths: {}, singleInstance: { verdict: "acquired" } },
    { ...input, originalResourcePolicy: null },
  ]) {
    const child = observe(value);
    expect(child.exitCode).not.toBe(0);
    expect(child.stderr.toString()).toContain("KELD-IPC-029");
    expect(child.stdout.toString()).not.toContain(
      "GH1404_ORIGINAL_DEPENDENCY_REACHED",
    );
  }
});

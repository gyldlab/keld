import { expect, test } from "bun:test";
import {
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const role = join(import.meta.dir, "ready-desktop-platform.test-role.ts");
const transport = resolve(import.meta.dir, "../../kipc/src/transport.ts");
const apiRoot = realpathSync(resolve(import.meta.dir, ".."));
const electronSource = realpathSync(
  resolve(import.meta.dir, "../../electron/src"),
);
const electronFiles = [
  "screen.ts",
  "native-theme.ts",
  "unsupported.ts",
] as const;
const apiManifest = JSON.parse(
  readFileSync(join(apiRoot, "package.json"), "utf8"),
) as { exports?: Record<string, string> };
const appExport = apiManifest.exports?.["./internal/app"];
if (appExport !== "./src/app.ts")
  throw new Error(
    "GH481 test requires the canonical ./internal/app source export",
  );
const canonicalApp = realpathSync(resolve(apiRoot, appExport));
if (canonicalApp !== realpathSync(join(import.meta.dir, "app.ts")))
  throw new Error(
    "GH481 test resolved ./internal/app outside canonical app.ts",
  );

function sourceFixture(withMapping: boolean): {
  root: string;
  sourceRoot: string;
} {
  const root = realpathSync(
    mkdtempSync(join(tmpdir(), "keld-gh481-electron-source-")),
  );
  const sourceRoot = join(root, "src");
  mkdirSync(sourceRoot);
  for (const name of electronFiles) {
    const source = join(electronSource, name);
    const sourceMetadata = lstatSync(source);
    if (!sourceMetadata.isFile() || sourceMetadata.isSymbolicLink())
      throw new Error(
        "GH481 Electron source must be a regular non-symlink file",
      );
    if (realpathSync(source) !== source)
      throw new Error("GH481 Electron source realpath changed unexpectedly");
    const bytes = readFileSync(source);
    const destination = join(sourceRoot, name);
    writeFileSync(destination, bytes);
    expect(readFileSync(destination)).toEqual(bytes);
    expect(realpathSync(destination)).toBe(destination);
  }
  if (withMapping)
    writeFileSync(
      join(root, "tsconfig.json"),
      JSON.stringify(
        {
          compilerOptions: {
            module: "Preserve",
            moduleResolution: "Bundler",
            paths: { "@keld/api/internal/app": [canonicalApp] },
          },
        },
        null,
        2,
      ) + "\n",
    );
  return { root, sourceRoot };
}

const qualifiedMacDevFacts = {
  schema: 3,
  kind: "development",
  name: "gh481-s1",
  root: "/qualified-native-stage",
  paths: {},
  locale: "en-US",
};

test("GH481 real Electron source fails closed without the test-owned API export mapping", () => {
  const fixture = sourceFixture(false);
  try {
    const child = Bun.spawnSync({
      cmd: [
        process.execPath,
        "--no-install",
        "--no-env-file",
        "-e",
        `await import(${JSON.stringify(pathToFileURL(join(fixture.sourceRoot, "screen.ts")).href)})`,
      ],
      cwd: apiRoot,
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
      timeout: 5_000,
    });
    const stderr = child.stderr.toString();
    expect(child.exitCode).not.toBe(0);
    expect(stderr).toContain("Cannot find module '@keld/api/internal/app'");
    expect(stderr).toContain(join(fixture.sourceRoot, "screen.ts"));
  } finally {
    rmSync(fixture.root, { recursive: true, force: true });
  }
});

for (const mode of [
  "other-facade",
  "other-renderer",
  "mac-valid",
  "mac-missing",
  "mac-no-mirror",
  "mac-invalidated",
]) {
  test(`GH481 S1 platform/Ready/zero-effect ${mode}`, () => {
    const fixture = sourceFixture(true);
    try {
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
          fixture.sourceRoot,
        ],
        stdin: "ignore",
        stdout: "pipe",
        stderr: "pipe",
        timeout: 5_000,
      });
      expect(
        child.exitCode,
        child.stdout.toString() + child.stderr.toString(),
      ).toBe(0);
      expect(child.stdout.toString()).toBe(`PASS GH481 ${mode}\n`);
      expect(child.stderr.toString()).toBe("");
    } finally {
      rmSync(fixture.root, { recursive: true, force: true });
    }
  });
}

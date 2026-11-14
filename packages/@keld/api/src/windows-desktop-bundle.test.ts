import { expect, test } from "bun:test";
import {
  copyFileSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const repo = resolve(import.meta.dir, "../../../..");
const builder = join(repo, "packages/@keld/api/fixtures/staged-api-bundle.ts");
const appSource = join(import.meta.dir, "app.ts").replaceAll("\\", "/");
const transportSource = resolve(import.meta.dir, "../../kipc/src/transport.ts");

const windowsFacts = {
  schema: 3,
  kind: "development",
  name: "desktop-bundle",
  root: "\\\\?\\D:\\qualified-native-stage",
  version: "31.7.0",
  paths: {},
  locale: "en-US",
};

// Same canonical build, external transport and pre-run constants as public Host.
// Keep the // @bun marker: direct source-only tests cannot expose a selector
// captured in the wrong already-preprocessed module.
for (const [mode, windows, desktop, facts, selected] of [
  ["windows-unselected", true, false, windowsFacts, false],
  ["windows-selected", true, true, windowsFacts, true],
  ["windows-selected-no-launch-facts", true, true, undefined, true],
  ["foreign-target-capability", false, true, undefined, false],
] as const) {
  test(
    "canonical staged bundle reads preload-owned desktop selection: " + mode,
    () => {
      const root = mkdtempSync(join(tmpdir(), "keld-windows-desktop-bundle-"));
      try {
        const entry = join(root, "entry.ts");
        const bundle = join(root, "main.ts");
        const transport = join(root, "kipc-transport.ts");
        copyFileSync(transportSource, transport);
        expect(readFileSync(transport)).toEqual(readFileSync(transportSource));
        writeFileSync(
          entry,
          [
            "import { isDesktopS1Selected } from " +
              JSON.stringify(appSource) +
              ";",
            'import { windowsHostSelected, windowsDesktopSnapshotSelected, nativeFacadeBootSelected } from "@keld/kipc";',
            "console.log(JSON.stringify({ windows: windowsHostSelected, desktop: windowsDesktopSnapshotSelected, facade: nativeFacadeBootSelected, selected: isDesktopS1Selected() }));",
          ].join("\n"),
        );
        const environment = { ...process.env };
        delete environment.KELD_APP_LINK;
        const built = Bun.spawnSync({
          cmd: [
            process.execPath,
            "--no-install",
            "--no-env-file",
            builder,
            entry,
            bundle,
          ],
          cwd: repo,
          env: environment,
          stdin: "ignore",
          stdout: "pipe",
          stderr: "pipe",
          timeout: 10_000,
        });
        expect(
          built.exitCode,
          built.stdout.toString() + built.stderr.toString(),
        ).toBe(0);
        const emitted = readFileSync(bundle, "utf8");
        expect(emitted.startsWith("// @bun\n")).toBe(true);
        expect(emitted).toContain("./kipc-transport.ts");
        const child = Bun.spawnSync({
          cmd: [
            process.execPath,
            "--no-install",
            "--no-env-file",
            '--define=process.env.KELD_NATIVE_WINDOW_FACADE:"1"',
            "--define=__KELD_WINDOWS_HOST__:" + windows,
            "--define=__KELD_WINDOWS_DESKTOP_SNAPSHOT__:" + desktop,
            "--define=__KELD_HOST_LAUNCH_FACTS__:" +
              (facts === undefined ? "undefined" : JSON.stringify(facts)),
            "--preload=" + transport,
            "run",
            bundle,
          ],
          cwd: root,
          env: environment,
          stdin: "ignore",
          stdout: "pipe",
          stderr: "pipe",
          timeout: 5_000,
        });
        expect(
          child.exitCode,
          child.stdout.toString() + child.stderr.toString(),
        ).toBe(0);
        expect(JSON.parse(child.stdout.toString())).toEqual({
          windows,
          desktop: windows && desktop,
          facade: true,
          selected,
        });
        expect(child.stderr.toString()).toBe("");
      } finally {
        rmSync(root, { recursive: true, force: true });
      }
    },
  );
}

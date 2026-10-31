/**
 * Main-role `process.*` facts (GH-570, `docs/specs/gh570-main-role-process-facts.md`).
 *
 * Each case binds to one spec acceptance criterion. Oracles are independent of the
 * installer: the launcher's own argument list echoed back by a child process, exact
 * property descriptors against Electron v44.4.5's `default_app/main.ts:92-96`, the
 * package manifest bytes, and the pinned oracle release string.
 *
 * Negative controls (one mutation each, named per case): defining `defaultApp` as
 * `undefined` fails the argv cases; defining `sandboxed = false` fails the
 * undefined-not-false case; `ELECTRON_VERSION_SHIM = "44.4.5"` fails the honesty
 * case; dropping the preload's import fails the pre-entry case.
 *
 * `bun test` shares one module cache across files, so the in-process cases read
 * the descriptors the single facade import produced; argv-shape cases spawn a
 * child with the Host guardian's exact flag shape instead of reading this
 * process's `bun test` argv.
 */
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import "./index";
import {
  ELECTRON_VERSION_SHIM,
  MAIN_PROCESS_TYPE,
  MAIN_ROLE_UNDEFINED_FLAGS,
  type ProcessFactsTarget,
  installMainProcessFacts,
} from "./process-facts";

const fixtures = join(import.meta.dir, "..", "fixtures");

/**
 * The flag the macOS guardian always passes before `run <entry>`
 * (`crates/keld-host/src/main.rs::guardian_bun_command`). `--preload` is added
 * per case because the guardian adds it only when a sidecar is staged.
 */
const GUARDIAN_DEFINE = '--define=process.env.KELD_NATIVE_WINDOW_FACADE:"0"';

/** Kill switch only, below bun's per-test timeout; every fixture exits by itself. */
const KILL_MS = 4_000;

/** Electron's `defaultApp` descriptor (`default_app/main.ts:92-96`). */
function frozen(value: string | boolean): PropertyDescriptor {
  return { value, writable: false, enumerable: true, configurable: false };
}

function spawnFixture(
  script: string,
  userArgs: readonly string[],
  preload?: string,
): { code: number | null; stderr: string; lines: string[] } {
  const env: Record<string, string | undefined> = { ...process.env };
  // Import-only probes: no link is dialled, so the host variable must be absent.
  delete env.KELD_APP_LINK;
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      GUARDIAN_DEFINE,
      ...(preload === undefined ? [] : [`--preload=./${preload}`]),
      "run",
      `./${script}`,
      ...userArgs,
    ],
    cwd: fixtures,
    env,
    stdout: "pipe",
    stderr: "pipe",
    timeout: KILL_MS,
  });
  return {
    code: child.exitCode,
    stderr: child.stderr.toString(),
    lines: child.stdout
      .toString()
      .split("\n")
      .filter((line) => line.startsWith("GH570_")),
  };
}

function descriptors(target: object & { versions: object }) {
  return {
    type: Object.getOwnPropertyDescriptor(target, "type"),
    defaultApp: Object.getOwnPropertyDescriptor(target, "defaultApp"),
    electron: Object.getOwnPropertyDescriptor(target.versions, "electron"),
  };
}

describe("(argv, defaultApp) atom — AC1, AC2", () => {
  test("process.argv.default-app: a draw.io-shaped main slices exactly the user arguments under the guardian flag shape", () => {
    const userArgs = ["gh570-alpha", "--disable-update"];
    expect(spawnFixture("process_facts_drawio_shape.ts", userArgs)).toEqual({
      code: 0,
      stderr: "",
      lines: [
        "GH570_DEFAULT_APP=true",
        `GH570_EXEC_ARGV=${JSON.stringify([GUARDIAN_DEFINE])}`,
        "GH570_ARGV1_IS_ENTRY=true",
        `GH570_USER_ARGS=${JSON.stringify(userArgs)}`,
      ],
    });
  });

  test("process.argv.default-app.empty-user-args: with no user arguments (the keld dev case) the tokens are empty, not the entry path", () => {
    expect(spawnFixture("process_facts_drawio_shape.ts", [])).toEqual({
      code: 0,
      stderr: "",
      lines: [
        "GH570_DEFAULT_APP=true",
        `GH570_EXEC_ARGV=${JSON.stringify([GUARDIAN_DEFINE])}`,
        "GH570_ARGV1_IS_ENTRY=true",
        "GH570_USER_ARGS=[]",
      ],
    });
  });
});

describe("descriptors — AC3, AC4", () => {
  test("process.default-app.domain: defaultApp is Electron's frozen enumerable true", () => {
    expect(Object.getOwnPropertyDescriptor(process, "defaultApp")).toEqual(
      frozen(true),
    );
  });

  test("process.type and process.versions.electron are frozen read-only facts", () => {
    expect(Object.getOwnPropertyDescriptor(process, "type")).toEqual(
      frozen(MAIN_PROCESS_TYPE),
    );
    expect(MAIN_PROCESS_TYPE).toBe("browser");
    expect(
      Object.getOwnPropertyDescriptor(process.versions, "electron"),
    ).toEqual(frozen(ELECTRON_VERSION_SHIM));
    const versions = process.versions as { electron?: string };
    expect(() => {
      versions.electron = "44.4.5";
    }).toThrow(TypeError);
    expect(versions.electron).toBe(ELECTRON_VERSION_SHIM);
  });

  test("process.versions.electron.honesty: the shim constant is the package version, never the pinned oracle release", () => {
    const manifest = JSON.parse(
      readFileSync(join(import.meta.dir, "..", "package.json"), "utf8"),
    ) as { version: string };
    expect(ELECTRON_VERSION_SHIM).toBe(manifest.version);
    expect(ELECTRON_VERSION_SHIM).toMatch(/^\d+\.\d+\.\d+$/);
    expect(ELECTRON_VERSION_SHIM).not.toBe("44.4.5");
    // npm `electron` starts at 0.1.0; the bounded full-list predicate is #569's cell.
    expect(ELECTRON_VERSION_SHIM.startsWith("0.0.")).toBe(true);
  });
});

describe("undefined, not false — AC5", () => {
  test("process.mas/windows-store/sandboxed/context-isolated/is-main-frame are not own properties and read undefined", () => {
    expect([...MAIN_ROLE_UNDEFINED_FLAGS]).toEqual([
      "mas",
      "windowsStore",
      "sandboxed",
      "contextIsolated",
      "isMainFrame",
    ]);
    const facts = process as unknown as Record<string, unknown>;
    for (const name of MAIN_ROLE_UNDEFINED_FLAGS) {
      expect(Object.hasOwn(process, name)).toBe(false);
      expect(typeof facts[name]).toBe("undefined");
    }
    expect(Object.hasOwn(process.versions, "chrome")).toBe(false);
  });

  test("process.resources-path.string today: resourcesPath is absent until the #452 payload delivers it (spec T3 inverts this case)", () => {
    expect(Object.hasOwn(process, "resourcesPath")).toBe(false);
    expect(typeof (process as { resourcesPath?: unknown }).resourcesPath).toBe(
      "undefined",
    );
  });
});

describe("one owner — AC6, AC7", () => {
  test("a second installation is a no-op with byte-identical descriptors", () => {
    const before = descriptors(process);
    expect(() => installMainProcessFacts()).not.toThrow();
    expect(descriptors(process)).toEqual(before);
  });

  test("a plain target receives the three facts and nothing else", () => {
    const target: ProcessFactsTarget & Record<string, unknown> = {
      versions: {},
    };
    installMainProcessFacts(target);
    expect(descriptors(target as { versions: object })).toEqual({
      type: frozen("browser"),
      defaultApp: frozen(true),
      electron: frozen(ELECTRON_VERSION_SHIM),
    });
    expect(Object.keys(target).sort()).toEqual([
      "defaultApp",
      "type",
      "versions",
    ]);
  });

  test("a foreign non-configurable type refuses with KELD-COMPAT-013 and defines no fact", () => {
    const target: ProcessFactsTarget = { versions: {} };
    Object.defineProperty(target, "type", frozen("renderer"));
    expect(() => installMainProcessFacts(target)).toThrow(
      /^KELD-COMPAT-013: process\.type /,
    );
    expect(Object.hasOwn(target, "defaultApp")).toBe(false);
    expect(Object.hasOwn(target.versions, "electron")).toBe(false);
  });

  test("a foreign configurable type with another value refuses too; it is not silently overwritten", () => {
    const target = { versions: {}, type: "renderer" };
    expect(() => installMainProcessFacts(target)).toThrow(
      /^KELD-COMPAT-013: process\.type is already "renderer"/,
    );
    expect(target.type).toBe("renderer");
    expect(Object.hasOwn(target, "defaultApp")).toBe(false);
  });

  test("a foreign versions.electron refuses before any fact is written", () => {
    const target: ProcessFactsTarget = { versions: { electron: "44.4.5" } };
    expect(() => installMainProcessFacts(target)).toThrow(
      /^KELD-COMPAT-013: process\.versions\.electron is already "44\.4\.5"/,
    );
    expect(Object.hasOwn(target, "type")).toBe(false);
    expect(Object.hasOwn(target, "defaultApp")).toBe(false);
  });

  test("a same-value configurable definition is adopted and frozen", () => {
    const target = { versions: {}, type: "browser" };
    installMainProcessFacts(target);
    expect(Object.getOwnPropertyDescriptor(target, "type")).toEqual(
      frozen("browser"),
    );
    expect(Object.getOwnPropertyDescriptor(target, "defaultApp")).toEqual(
      frozen(true),
    );
  });
});

describe("pre-entry readiness — AC8", () => {
  test("process.early.type: with a preload that imports only process-facts, an electron-log-shaped dependency selects its main mode before electron is imported", () => {
    expect(
      spawnFixture(
        "process_facts_early_entry.ts",
        [],
        "process_facts_preload.ts",
      ),
    ).toEqual({
      code: 0,
      stderr: "",
      lines: ["GH570_EARLY mode=main defaultApp=true facadeType=browser"],
    });
  });

  test("negative control: without the preload the dependency sees undefined while the facade import still installs the facts afterwards", () => {
    expect(spawnFixture("process_facts_early_entry.ts", [])).toEqual({
      code: 0,
      stderr: "",
      lines: [
        "GH570_EARLY mode=unknown defaultApp=undefined facadeType=browser",
      ],
    });
  });
});

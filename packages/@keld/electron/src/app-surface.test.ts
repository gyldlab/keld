/**
 * Pinned Electron v44.4.5 cells for the first-proof app surface (GH-445).
 *
 * Corpus `electron-app-v1` (`crates/keld-compat/fixtures/app-corpus/`) maps each
 * cell to one exact test name below. The corpus owner admits a cell on a macOS
 * host only when that case passes once (gh566 D8, D13).
 *
 * - Pass cells assert the cited sentence.
 * - Red cells (`expected_verdict: fail`, gh532 rule 3) assert today's behaviour
 *   and pass. Their implementing ticket inverts the case and flips the cell to
 *   `pass` in the same diff, so adding a member without that edit fails here.
 *
 * Every case reads `app` from the `electron` entry (`./index`), the module
 * Electron code imports. The ready, whenReady and getLocale cells run an
 * observer child process. Stubbing `LifecycleLink.connect` in this process would
 * leak into `link.test.ts`, because bun test shares one module cache across files.
 */
import { describe, expect, test } from "bun:test";
import { join } from "node:path";

import { app } from "./index";

const fixtures = join(import.meta.dir, "..", "fixtures");

/**
 * Kill switch only, below bun's 5 s per-test timeout so a stuck observer is
 * reported with its partial output. Every observer exits once its sequence ends.
 */
const OBSERVER_KILL_MS = 4_000;

function observe(script: string): { code: number | null; stderr: string; lines: string[] } {
  const child = Bun.spawnSync({
    cmd: [process.execPath, `./${script}`],
    cwd: fixtures,
    env: { ...process.env, KELD_APP_LINK: `1#${"ab".repeat(32)}` },
    stdout: "pipe",
    stderr: "pipe",
    timeout: OBSERVER_KILL_MS,
  });
  return {
    code: child.exitCode,
    stderr: child.stderr.toString(),
    lines: child.stdout
      .toString()
      .split("\n")
      .filter((line) => line.startsWith("GH445_")),
  };
}

/** Today's observable for an absent facade method: Electron code that calls it gets a TypeError. */
function expectNoMethod(name: string, ...args: unknown[]): void {
  expect(Reflect.has(app, name)).toBe(false);
  const facade = app as unknown as Record<string, (...rest: unknown[]) => unknown>;
  expect(() => facade[name](...args)).toThrow(TypeError);
}

/** Today's observable for an absent facade property: reading it yields undefined. */
function expectNoProperty(name: string): void {
  expect(Reflect.has(app, name)).toBe(false);
  expect(Reflect.get(app, name)).toBeUndefined();
}

describe("ready and whenReady (pass cells)", () => {
  test("app.ready.emitted-once: a ready listener does not run before host Ready, then runs exactly once, also when host Ready is delivered again", () => {
    expect(observe("app_surface_ready_once.ts")).toEqual({
      code: 0,
      stderr: "",
      lines: ["GH445_READY_ONCE before=0 after=1 repeat=1"],
    });
  });

  test("app.when-ready.is-ready-agreement: whenReady stays pending while isReady is false and is fulfilled once isReady is true; after the event it is still fulfilled while a ready listener added then is not called", () => {
    expect(observe("app_surface_when_ready.ts")).toEqual({
      code: 0,
      stderr: "",
      lines: [
        "GH445_BEFORE_READY isReady=false fulfilled=false",
        "GH445_AT_FULFIL isReady=true",
        "GH445_AFTER_READY isReady=true fulfilled=true lateListener=0",
      ],
    });
  });
});

describe("session facts (red until GH-452)", () => {
  test("app.get-path.user-data-default today: the facade has no getPath, so app.getPath('userData') throws a TypeError", () => {
    expectNoMethod("getPath", "userData");
  });

  test("app.get-version.loaded-app today: the facade has no getVersion, so app.getVersion() throws a TypeError", () => {
    expectNoMethod("getVersion");
  });

  test("app.get-name.package-name today: the facade has no getName, so app.getName() throws a TypeError", () => {
    expectNoMethod("getName");
  });

  test("app.name.property today: the facade has no name property, so app.name is undefined", () => {
    expectNoProperty("name");
  });

  test("app.get-app-path.app-directory today: the facade has no getAppPath, so app.getAppPath() throws a TypeError", () => {
    expectNoMethod("getAppPath");
  });

  test("app.get-locale.after-ready today: the facade has no getLocale, so app.getLocale() throws a TypeError after ready", () => {
    expect(observe("app_surface_locale_after_ready.ts")).toEqual({
      code: 0,
      stderr: "",
      lines: ["GH445_LOCALE_AFTER_READY has=false threw=TypeError"],
    });
  });

  test("app.is-packaged.dev-false today: the facade has no isPackaged, so app.isPackaged is undefined, not false", () => {
    expectNoProperty("isPackaged");
  });
});

describe("single-instance verdict (red until GH-453)", () => {
  test("app.request-single-instance-lock.primary-verdict today: the facade has no requestSingleInstanceLock, so the call throws a TypeError", () => {
    expectNoMethod("requestSingleInstanceLock");
  });

  test("app.has-single-instance-lock.holder today: the facade has no hasSingleInstanceLock, so the call throws a TypeError", () => {
    expectNoMethod("hasSingleInstanceLock");
  });
});

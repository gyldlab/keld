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
 *   The GH-452 session-fact cells record the fail-closed half of that ticket
 *   (`docs/specs/gh452-app-session-facts.md` T1): the member exists and throws
 *   KELD-COMPAT-040 until the Host delivers the payload (T2 flips them).
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

function observe(script: string): {
  code: number | null;
  stderr: string;
  lines: string[];
} {
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
  const facade = app as unknown as Record<
    string,
    (...rest: unknown[]) => unknown
  >;
  expect(() => facade[name](...args)).toThrow(TypeError);
}

describe("ready and whenReady (pass cells)", () => {
  test("app.ready.emitted-once: a ready listener does not run before host Ready, then runs exactly once, also when host Ready is delivered again", () => {
    expect(observe("app_surface_ready_once.ts")).toEqual({
      code: 0,
      stderr: "",
      lines: ["GH445_READY_ONCE before=0 after=1 repeat=1 late=0"],
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

describe("late Ready replay (pass cells)", () => {
  test("app.ready.api-first-late-listener: no ready replay to a listener registered after the API observed Host Ready", () => {
    expect(observe("app_surface_api_ready_first.ts")).toEqual({
      code: 0,
      stderr: "",
      lines: ["GH445_API_FIRST_READY hostReady=true lateListener=0"],
    });
  });
});

/**
 * Today's observable for a session fact the Host has not delivered (GH-452 T1):
 * the member exists and fails closed with KELD-COMPAT-040 naming the fact, the
 * Electron call and GH-452. Asserted structurally so this oracle does not depend
 * on the facade's error class identity.
 */
function expectSessionFactRefusal(
  read: () => unknown,
  call: string,
  fact: string,
): void {
  let caught: unknown;
  try {
    read();
  } catch (error) {
    caught = error;
  }
  expect(caught).toBeInstanceOf(Error);
  const typed = caught as Error & { code?: unknown; fact?: unknown };
  expect(typed.code).toBe("KELD-COMPAT-040");
  expect(typed.fact).toBe(fact);
  expect(typed.message.startsWith("KELD-COMPAT-040: ")).toBe(true);
  expect(typed.message).toContain(call);
  expect(typed.message).toContain(fact);
  expect(typed.message).toContain("GH-452");
}

describe("session facts (red until GH-452 T2 delivers the Host payload)", () => {
  test("app.get-path.user-data-default today: no session facts are delivered, so app.getPath('userData') throws KELD-COMPAT-040 naming path.userData and GH-452", () => {
    expectSessionFactRefusal(
      () => app.getPath("userData"),
      "app.getPath('userData')",
      "path.userData",
    );
  });

  test("app.get-version.loaded-app today: no session facts are delivered, so app.getVersion() throws KELD-COMPAT-040 naming version and GH-452", () => {
    expectSessionFactRefusal(
      () => app.getVersion(),
      "app.getVersion()",
      "version",
    );
  });

  test("app.get-name.package-name today: no session facts are delivered, so app.getName() throws KELD-COMPAT-040 naming name and GH-452", () => {
    expectSessionFactRefusal(() => app.getName(), "app.getName()", "name");
  });

  test("app.name.property today: no session facts are delivered, so reading app.name throws KELD-COMPAT-040 naming name and GH-452", () => {
    // Electron defines `name` as an accessor; so does the facade.
    expect(Object.getOwnPropertyDescriptor(app, "name")?.get).toBeDefined();
    expectSessionFactRefusal(() => app.name, "app.name", "name");
  });

  test("app.get-app-path.app-directory today: no session facts are delivered, so app.getAppPath() throws KELD-COMPAT-040 naming appPath and GH-452", () => {
    expectSessionFactRefusal(
      () => app.getAppPath(),
      "app.getAppPath()",
      "appPath",
    );
  });

  test("app.get-locale.after-ready today: no session facts are delivered, so app.getLocale() throws KELD-COMPAT-040 naming locale and GH-452 after ready", () => {
    expect(observe("app_surface_locale_after_ready.ts")).toEqual({
      code: 0,
      stderr: "",
      lines: [
        "GH445_LOCALE_AFTER_READY has=true threw=KELD-COMPAT-040 fact=locale cites=GH-452",
      ],
    });
  });

  test("app.is-packaged.dev-false today: no session facts are delivered, so reading app.isPackaged throws KELD-COMPAT-040 naming isPackaged and GH-452; it is neither false nor undefined", () => {
    expect(
      Object.getOwnPropertyDescriptor(app, "isPackaged")?.get,
    ).toBeDefined();
    expectSessionFactRefusal(
      () => app.isPackaged,
      "app.isPackaged",
      "isPackaged",
    );
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

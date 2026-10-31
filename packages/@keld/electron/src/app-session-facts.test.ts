/**
 * GH-452 T1: the fail-closed half of the `app` session-facts contract
 * (`docs/specs/gh452-app-session-facts.md` §3 criteria 1–9).
 *
 * The corpus-mapped today-tests live in `app-surface.test.ts`; this file holds
 * the module contract: the pinned `getPath` name list, the three typed codes,
 * the `fact` each member names, Electron accessor parity (non-enumerable, with a
 * refusing `name` setter), the non-string `getPath` guard, and the
 * decoy-environment observer, whose recording `process.env` Proxy and
 * before/after-`ready` reads prove that no member derives a value from HOME,
 * XDG, APPDATA, LANG, LC_*, cwd or the ready state.
 */
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import {
  type AppSessionFact,
  AppSessionFactError,
  ELECTRON_PATH_NAMES,
  NAME_ASSIGNMENT_REFUSED,
  SESSION_FACT_NOT_DELIVERED,
  UNKNOWN_PATH_NAME,
} from "./app-session-facts.ts";
import { app } from "./index";
import { isCallError } from "./link";

const fixtures = join(import.meta.dir, "..", "fixtures");

/** The pinned Electron v44.4.5 `docs/api/app.md` the app corpus cites. */
const PINNED_APP_MD = join(
  import.meta.dir,
  "../../../../crates/keld-compat/fixtures/doc-snapshots",
  "694f45852a0f1726cd23bfd379854de489cccb65/docs/api/app.md",
);

const OBSERVER_KILL_MS = 4_000;

/** Top-level `* \`name\`` bullets under `### app.getPath(name)` in the pinned page. */
function pinnedPathNames(): string[] {
  const page = readFileSync(PINNED_APP_MD, "utf8");
  const heading = "### `app.getPath(name)`";
  const start = page.indexOf(heading);
  expect(start).toBeGreaterThan(-1);
  const rest = page.slice(start + heading.length);
  const section = rest.slice(0, rest.indexOf("\n### "));
  const names: string[] = [];
  for (const line of section.split("\n")) {
    const match = /^ {2}\* `([A-Za-z]+)` /.exec(line);
    if (match) names.push(match[1]);
  }
  return names;
}

function caught(read: () => unknown): unknown {
  try {
    read();
  } catch (error) {
    return error;
  }
  return undefined;
}

describe("getPath name validation (criterion 7)", () => {
  test("ELECTRON_PATH_NAMES equals the names documented under app.getPath(name) in the pinned v44.4.5 app.md, in order", () => {
    const pinned = pinnedPathNames();
    expect(pinned.length).toBe(17);
    expect(pinned).toEqual([...ELECTRON_PATH_NAMES]);
  });

  test("app.getPath('bogus') throws KELD-COMPAT-041 naming the bad name and listing the documented names, not the not-delivered code", () => {
    const error = caught(() => app.getPath("bogus"));
    expect(error).toBeInstanceOf(AppSessionFactError);
    const typed = error as AppSessionFactError;
    expect(typed.code).toBe(UNKNOWN_PATH_NAME);
    expect(typed.code).toBe("KELD-COMPAT-041");
    expect(typed.fact).toBeUndefined();
    expect(typed.message.startsWith("KELD-COMPAT-041: ")).toBe(true);
    expect(typed.message).toContain("'bogus'");
    expect(typed.message).toContain("documented");
    expect(typed.message).toContain("home");
    expect(typed.message).toContain("crashDumps");
  });

  test("a non-string name is KELD-COMPAT-041 naming the received type, never a second untyped TypeError (criterion 9)", () => {
    const loose = app.getPath as unknown as (name: unknown) => unknown;
    for (const [value, described] of [
      [undefined, "undefined"],
      [null, "null"],
      [7, "number"],
      [Symbol("userData"), "symbol"],
      [{ name: "userData" }, "object"],
    ] as const) {
      const error = caught(() => loose(value));
      expect(error).toBeInstanceOf(AppSessionFactError);
      const typed = error as AppSessionFactError;
      expect(typed.code).toBe("KELD-COMPAT-041");
      expect(typed.message).toContain(`received ${described}`);
    }
  });

  test("every documented name is accepted as a name and then fails closed on the undelivered path fact", () => {
    for (const name of ELECTRON_PATH_NAMES) {
      const error = caught(() => app.getPath(name)) as AppSessionFactError;
      expect(error.code).toBe(SESSION_FACT_NOT_DELIVERED);
      expect(error.fact).toBe(`path.${name}`);
    }
  });
});

describe("fail-closed typed refusal (criteria 1–6)", () => {
  const members: ReadonlyArray<
    readonly [call: string, fact: AppSessionFact, read: () => unknown]
  > = [
    ["app.getPath('userData')", "path.userData", () => app.getPath("userData")],
    ["app.getVersion()", "version", () => app.getVersion()],
    ["app.getName()", "name", () => app.getName()],
    ["app.name", "name", () => app.name],
    ["app.getAppPath()", "appPath", () => app.getAppPath()],
    ["app.getLocale()", "locale", () => app.getLocale()],
    ["app.isPackaged", "isPackaged", () => app.isPackaged],
  ];

  for (const [call, fact, read] of members) {
    test(`${call} throws AppSessionFactError KELD-COMPAT-040 with fact ${fact}, naming the call, GH-452 and the forbidden derivations`, () => {
      const error = caught(read);
      expect(error).toBeInstanceOf(AppSessionFactError);
      expect(error).toBeInstanceOf(Error);
      const typed = error as AppSessionFactError;
      expect(typed.name).toBe("AppSessionFactError");
      expect(typed.code).toBe(SESSION_FACT_NOT_DELIVERED);
      expect(typed.code).toBe("KELD-COMPAT-040");
      expect(typed.fact).toBe(fact);
      expect(typed.message.startsWith("KELD-COMPAT-040: ")).toBe(true);
      expect(typed.message).toContain(call);
      expect(typed.message).toContain(`"${fact}"`);
      expect(typed.message).toContain("GH-452");
      expect(typed.message).toContain("HOME");
      expect(typed.message).toContain("LC_*");
      expect(typed.message).toContain("package.json");
      // The public typed shape is `Error & { code }`, so the kipc predicate agrees.
      expect(isCallError(error)).toBe(true);
    });
  }
});

describe("Electron accessor parity (criteria 3, 6 and 9)", () => {
  test("name and isPackaged are non-enumerable, non-configurable accessors: a spread, Object.entries or JSON.stringify of app does not invoke them", () => {
    for (const property of ["name", "isPackaged"]) {
      const descriptor = Object.getOwnPropertyDescriptor(app, property);
      expect(descriptor).toBeDefined();
      expect(typeof descriptor?.get).toBe("function");
      expect(descriptor?.enumerable).toBe(false);
      expect(descriptor?.configurable).toBe(false);
      expect("value" in (descriptor ?? {})).toBe(false);
    }
    expect(Object.keys(app)).not.toContain("name");
    expect(Object.keys(app)).not.toContain("isPackaged");
    expect(() => ({ ...app })).not.toThrow();
    expect(() => Object.entries(app)).not.toThrow();
    expect(() => JSON.stringify(app)).not.toThrow();
    // Electron gives `name` a setter (setName) and leaves `isPackaged` read-only.
    expect(typeof Object.getOwnPropertyDescriptor(app, "name")?.set).toBe(
      "function",
    );
    expect(
      Object.getOwnPropertyDescriptor(app, "isPackaged")?.set,
    ).toBeUndefined();
  });

  test("assigning app.name (Electron's setName) throws KELD-COMPAT-042 naming the value and GH-452, for a string and for a Symbol", () => {
    for (const [value, described] of [
      ["Custom", '"Custom"'],
      [Symbol("Custom"), "symbol"],
    ] as const) {
      const error = caught(() => {
        (app as { name: unknown }).name = value;
      });
      expect(error).toBeInstanceOf(AppSessionFactError);
      const typed = error as AppSessionFactError;
      expect(typed.code).toBe(NAME_ASSIGNMENT_REFUSED);
      expect(typed.code).toBe("KELD-COMPAT-042");
      expect(typed.fact).toBeUndefined();
      expect(typed.message.startsWith("KELD-COMPAT-042: ")).toBe(true);
      expect(typed.message).toContain(`app.name = ${described}`);
      expect(typed.message).toContain("setName");
      expect(typed.message).toContain("GH-452");
    }
    // The refused assignment changed nothing: the next read still fails closed.
    const after = caught(() => app.name) as AppSessionFactError;
    expect(after.code).toBe("KELD-COMPAT-040");
  });
});

describe("decoy environment and environment-read oracle (criterion 8)", () => {
  test("with HOME, XDG_CONFIG_HOME, APPDATA, LANG and LC_ALL pointing at decoys, every member fails closed at import and after ready, and the facade read none of the watched variables nor the cwd", () => {
    const child = Bun.spawnSync({
      cmd: [process.execPath, "./app_session_facts_decoy_env.ts"],
      cwd: fixtures,
      env: {
        ...process.env,
        KELD_APP_LINK: `1#${"ab".repeat(32)}`,
        HOME: "/nonexistent/gh452-decoy-home",
        XDG_CONFIG_HOME: "/nonexistent/gh452-decoy-xdg",
        APPDATA: "C:\\nonexistent\\gh452-decoy-appdata",
        LANG: "de_DE.UTF-8",
        LC_ALL: "de_DE.UTF-8",
      },
      stdout: "pipe",
      stderr: "pipe",
      timeout: OBSERVER_KILL_MS,
    });
    expect(child.stderr.toString()).toBe("");
    expect(child.exitCode).toBe(0);
    const lines = child.stdout
      .toString()
      .split("\n")
      .filter((line) => line.startsWith("GH452_"));
    const expectedReads = (phase: string): string[] => [
      `GH452_DECOY phase=${phase} member=getPath(userData) code=KELD-COMPAT-040 fact=path.userData value=none`,
      `GH452_DECOY phase=${phase} member=getPath(documents) code=KELD-COMPAT-040 fact=path.documents value=none`,
      `GH452_DECOY phase=${phase} member=getVersion code=KELD-COMPAT-040 fact=version value=none`,
      `GH452_DECOY phase=${phase} member=getName code=KELD-COMPAT-040 fact=name value=none`,
      `GH452_DECOY phase=${phase} member=name code=KELD-COMPAT-040 fact=name value=none`,
      `GH452_DECOY phase=${phase} member=getAppPath code=KELD-COMPAT-040 fact=appPath value=none`,
      `GH452_DECOY phase=${phase} member=getLocale code=KELD-COMPAT-040 fact=locale value=none`,
      `GH452_DECOY phase=${phase} member=isPackaged code=KELD-COMPAT-040 fact=isPackaged value=none`,
    ];
    expect(lines).toEqual([
      ...expectedReads("import"),
      ...expectedReads("after-ready"),
      "GH452_ENV_READS watched=[] cwdCalls=0",
    ]);
  });
});

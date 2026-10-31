/**
 * GH-452 T1: the fail-closed half of the `app` session-facts contract
 * (`docs/specs/gh452-app-session-facts.md` §3 criteria 1–8).
 *
 * The corpus-mapped today-tests live in `app-surface.test.ts`; this file holds
 * the module contract: the pinned `getPath` name list, the two typed codes, the
 * `fact` each member names, Electron accessor parity, and the decoy-environment
 * negative control that proves no member derives a value from HOME, XDG, APPDATA,
 * LANG or LC_ALL.
 */
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import {
  AppSessionFactError,
  ELECTRON_PATH_NAMES,
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
  test("ELECTRON_PATH_NAMES equals the names listed under app.getPath(name) in the pinned v44.4.5 app.md, in order", () => {
    const pinned = pinnedPathNames();
    expect(pinned.length).toBe(17);
    expect([...ELECTRON_PATH_NAMES]).toEqual(pinned);
  });

  test("app.getPath('bogus') throws KELD-COMPAT-041 naming the bad name and listing the accepted names, not the not-delivered code", () => {
    const error = caught(() => app.getPath("bogus"));
    expect(error).toBeInstanceOf(AppSessionFactError);
    const typed = error as AppSessionFactError;
    expect(typed.code).toBe(UNKNOWN_PATH_NAME);
    expect(typed.code).toBe("KELD-COMPAT-041");
    expect(typed.fact).toBeUndefined();
    expect(typed.message.startsWith("KELD-COMPAT-041: ")).toBe(true);
    expect(typed.message).toContain("'bogus'");
    expect(typed.message).toContain("home");
    expect(typed.message).toContain("crashDumps");
  });

  test("every pinned name is accepted as a name and then fails closed on the undelivered path fact", () => {
    for (const name of ELECTRON_PATH_NAMES) {
      const error = caught(() => app.getPath(name)) as AppSessionFactError;
      expect(error.code).toBe(SESSION_FACT_NOT_DELIVERED);
      expect(error.fact).toBe(`path.${name}`);
    }
  });
});

describe("fail-closed typed refusal (criteria 1–6)", () => {
  const members: ReadonlyArray<
    readonly [call: string, fact: string, read: () => unknown]
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
      expect(typed.message).toContain("package.json");
      // The public typed shape is `Error & { code }`, so the kipc predicate agrees.
      expect(isCallError(error)).toBe(true);
    });
  }

  test("name and isPackaged are accessors on the facade, as in Electron, not data properties", () => {
    for (const property of ["name", "isPackaged"]) {
      const descriptor = Object.getOwnPropertyDescriptor(app, property);
      expect(descriptor).toBeDefined();
      expect(typeof descriptor?.get).toBe("function");
      expect("value" in (descriptor ?? {})).toBe(false);
    }
  });
});

describe("decoy environment (criterion 8)", () => {
  test("with HOME, XDG_CONFIG_HOME, APPDATA, LANG and LC_ALL pointing at decoys, every session-fact member still throws KELD-COMPAT-040 and yields no value", () => {
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
      .filter((line) => line.startsWith("GH452_DECOY "));
    expect(lines).toEqual([
      "GH452_DECOY getPath(userData) code=KELD-COMPAT-040 fact=path.userData value=none",
      "GH452_DECOY getPath(documents) code=KELD-COMPAT-040 fact=path.documents value=none",
      "GH452_DECOY getVersion code=KELD-COMPAT-040 fact=version value=none",
      "GH452_DECOY getName code=KELD-COMPAT-040 fact=name value=none",
      "GH452_DECOY name code=KELD-COMPAT-040 fact=name value=none",
      "GH452_DECOY getAppPath code=KELD-COMPAT-040 fact=appPath value=none",
      "GH452_DECOY getLocale code=KELD-COMPAT-040 fact=locale value=none",
      "GH452_DECOY isPackaged code=KELD-COMPAT-040 fact=isPackaged value=none",
    ]);
  });
});

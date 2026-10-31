/**
 * `app` session facts: the fail-closed half of GH-452
 * (`docs/specs/gh452-app-session-facts.md`, T1).
 *
 * Electron reads `getPath`, `getVersion`, `getName`/`name`, `getAppPath`,
 * `getLocale` and `isPackaged` from the loaded application's package.json, from
 * Chromium's locale and from the packaging state (pinned v44.4.5
 * `docs/api/app.md`). Under Keld every one of them is a session fact the Host
 * mints at spawn: the Host owns the launch root, the compiled descriptor `name`,
 * every filesystem path grant, the webview engine whose locale the renderer sees,
 * and the dev-lease versus installed-provenance distinction.
 *
 * No Host delivers a session-facts payload at this facade version (the role
 * receives only `KELD_APP_LINK` and the `KELD_NATIVE_WINDOW_FACADE` define), so
 * every read here is the contract's fail-closed branch: a typed
 * `KELD-COMPAT-040` naming the missing fact, the Electron call and the issue.
 * This module never reads HOME, APPDATA, XDG_*, LANG/LC_*, argv, cwd, `Intl` or
 * a package.json to invent a value; the spec records why each of those is
 * rejected, and `fixtures/app_session_facts_decoy_env.ts` watches for such reads.
 * T2 (Host delivery) replaces the throwing bodies with payload reads.
 */

/**
 * Names documented for `app.getPath(name)` in the pinned Electron v44.4.5
 * `docs/api/app.md` (under `### app.getPath(name)`), in page order. The page is
 * the facade's oracle; `app-session-facts.test.ts` checks this list against the
 * committed snapshot. Electron's source also accepts a few undocumented names
 * (`cache`, `userCache`, `userDesktop`); whether the Host payload carries them is
 * a GH-452 T2 decision (spec §10).
 */
export const ELECTRON_PATH_NAMES = [
  "home",
  "appData",
  "assets",
  "userData",
  "sessionData",
  "temp",
  "exe",
  "module",
  "desktop",
  "documents",
  "downloads",
  "music",
  "pictures",
  "videos",
  "recent",
  "logs",
  "crashDumps",
] as const;

export type ElectronPathName = (typeof ELECTRON_PATH_NAMES)[number];

/** One Host-minted fact; each session-fact member of `app` reads exactly one. */
export type AppSessionFact =
  | "name"
  | "version"
  | "appPath"
  | "locale"
  | "isPackaged"
  | `path.${ElectronPathName}`;

/** The Host delivered no session facts to this role, so the named fact is missing. */
export const SESSION_FACT_NOT_DELIVERED = "KELD-COMPAT-040";
/** `app.getPath(name)` was called with a non-string or an undocumented name. */
export const UNKNOWN_PATH_NAME = "KELD-COMPAT-041";
/** `app.name` was assigned (Electron's `setName`), which this slice does not support. */
export const NAME_ASSIGNMENT_REFUSED = "KELD-COMPAT-042";

export type AppSessionFactCode =
  | typeof SESSION_FACT_NOT_DELIVERED
  | typeof UNKNOWN_PATH_NAME
  | typeof NAME_ASSIGNMENT_REFUSED;

const ISSUE_URL = "https://github.com/gyldlab/keld/issues/452";
const FORBIDDEN_SOURCES =
  "HOME, APPDATA, XDG_*, LANG/LC_*, argv, cwd, Intl or package.json";

/**
 * A typed session-fact refusal. `code` is the registered `KELD-COMPAT-*` code and
 * the message starts with it, like the facade's other typed errors; the `code`
 * field additionally gives it the public `Error & { code }` shape `isCallError`
 * recognises. `fact` is the missing fact for KELD-COMPAT-040 and `undefined` for
 * KELD-COMPAT-041/042, where the call itself is the defect.
 */
export class AppSessionFactError extends Error {
  override readonly name: string = "AppSessionFactError";
  readonly code: AppSessionFactCode;
  readonly fact: AppSessionFact | undefined;

  constructor(
    code: AppSessionFactCode,
    fact: AppSessionFact | undefined,
    detail: string,
  ) {
    super(`${code}: ${detail}`);
    this.code = code;
    this.fact = fact;
  }
}

/**
 * Renders a caller-supplied value for an error message without invoking the
 * value's own string conversion: a Symbol in a template literal would throw a
 * second, untyped TypeError and hide the typed refusal.
 */
function describeValue(value: unknown): string {
  if (typeof value === "string") return JSON.stringify(value);
  if (value === null) return "null";
  return typeof value;
}

function notDelivered(fact: AppSessionFact, call: string): AppSessionFactError {
  return new AppSessionFactError(
    SESSION_FACT_NOT_DELIVERED,
    fact,
    `${call} needs the app session fact "${fact}", and the Host delivered no ` +
      `session facts to this role. GH-452 T2 will mint these facts in the Host ` +
      `at spawn (${ISSUE_URL}); the facade does not derive them from ` +
      `${FORBIDDEN_SOURCES}. Fix: no Keld host delivers session facts yet, so ` +
      `code that can continue without "${fact}" should catch this error; ` +
      `otherwise wait for GH-452 T2.`,
  );
}

function isElectronPathName(name: string): name is ElectronPathName {
  return (ELECTRON_PATH_NAMES as readonly string[]).includes(name);
}

/**
 * Reads one session fact for the Electron member `call`. No payload exists at
 * this facade version, so the read always fails closed; the return type says so.
 */
export function readSessionFact(fact: AppSessionFact, call: string): never {
  throw notDelivered(fact, call);
}

/**
 * Validates `name` (a string documented for `app.getPath` in the pinned page,
 * else KELD-COMPAT-041), then reads the `path.<name>` session fact
 * (KELD-COMPAT-040 until the Host delivers it).
 */
export function readSessionPath(name: string): never {
  const documented = `Documented names: ${ELECTRON_PATH_NAMES.join(", ")}.`;
  if (typeof name !== "string") {
    throw new AppSessionFactError(
      UNKNOWN_PATH_NAME,
      undefined,
      `app.getPath() requires a string name and received ` +
        `${describeValue(name)}. ${documented} Fix: pass one of the documented names.`,
    );
  }
  if (!isElectronPathName(name)) {
    throw new AppSessionFactError(
      UNKNOWN_PATH_NAME,
      undefined,
      `app.getPath('${name}') is not a name documented for app.getPath in the ` +
        `pinned Electron v44.4.5 app.md, the facade's oracle. ${documented} ` +
        `Fix: pass a documented name; the undocumented names Electron's source ` +
        `also accepts (cache, userCache, userDesktop) are a GH-452 T2 decision.`,
    );
  }
  throw notDelivered(`path.${name}`, `app.getPath('${name}')`);
}

function nameAssignmentRefused(value: unknown): AppSessionFactError {
  return new AppSessionFactError(
    NAME_ASSIGNMENT_REFUSED,
    undefined,
    `app.name = ${describeValue(value)} (Electron's setName) is not supported: ` +
      `the app name is the Host's compiled descriptor name (keld.boot.json ` +
      `schema v1 "name"), and setName is outside the GH-452 first-proof slice ` +
      `(${ISSUE_URL}). Fix: remove the assignment and declare the name in ` +
      `keld.config.ts.`,
  );
}

/** The two Electron `app` properties this slice defines as accessors. */
export interface AppSessionFactAccessors {
  /**
   * Electron defines `app.name` as an accessor with a `setName` setter. Reading
   * fails closed (KELD-COMPAT-040); assigning is refused (KELD-COMPAT-042).
   */
  name: string;
  /** Electron's read-only boolean property. Reading fails closed (KELD-COMPAT-040). */
  readonly isPackaged: boolean;
}

/**
 * Defines `name` and `isPackaged` on `target` as non-enumerable, non-configurable
 * accessors, the way Electron defines `app.name`
 * (`lib/browser/api/app.ts` at 694f4585: `Object.defineProperty(app, 'name',
 * { get, set })`). Object-literal getters would be enumerable, so `{ ...app }`,
 * `Object.entries(app)` or `JSON.stringify(app)` would invoke them and throw a
 * refusal that blames a read the caller never made.
 */
export function defineSessionFactAccessors<T extends object>(
  target: T,
): T & AppSessionFactAccessors {
  Object.defineProperties(target, {
    name: {
      get: (): string => readSessionFact("name", "app.name"),
      set: (value: unknown): void => {
        throw nameAssignmentRefused(value);
      },
      enumerable: false,
      configurable: false,
    },
    isPackaged: {
      get: (): boolean => readSessionFact("isPackaged", "app.isPackaged"),
      enumerable: false,
      configurable: false,
    },
  });
  return target as T & AppSessionFactAccessors;
}

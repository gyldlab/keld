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
 * rejected. T2 (Host delivery) replaces the throwing bodies with payload reads.
 */

/**
 * Names `app.getPath(name)` accepts in Electron v44.4.5, in the order the pinned
 * `docs/api/app.md` lists them under `### app.getPath(name)`.
 * `app-session-facts.test.ts` checks this list against the committed snapshot.
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
/** `app.getPath(name)` was called with a name Electron v44.4.5 does not define. */
export const UNKNOWN_PATH_NAME = "KELD-COMPAT-041";

export type AppSessionFactCode =
  typeof SESSION_FACT_NOT_DELIVERED | typeof UNKNOWN_PATH_NAME;

const ISSUE_URL = "https://github.com/gyldlab/keld/issues/452";

/**
 * A typed session-fact refusal. `code` is the registered `KELD-COMPAT-*` code and
 * the message starts with it, matching the facade's other typed errors and the
 * `Error & { code }` shape `isCallError` recognises. `fact` is the missing fact
 * for KELD-COMPAT-040 and `undefined` for KELD-COMPAT-041, where the name itself
 * is the defect.
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

function notDelivered(fact: AppSessionFact, call: string): AppSessionFactError {
  return new AppSessionFactError(
    SESSION_FACT_NOT_DELIVERED,
    fact,
    `${call} needs the app session fact "${fact}", and the Host delivered no ` +
      `session facts to this role. Keld mints these facts at spawn (GH-452, ` +
      `${ISSUE_URL}); the facade does not derive them from HOME, APPDATA, XDG_*, ` +
      `LANG, argv, cwd, Intl or package.json. Fix: run under a Keld host that ` +
      `delivers session facts (GH-452 T2); code that can continue without ` +
      `"${fact}" should catch this error.`,
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
 * Validates `name` against the pinned Electron list (KELD-COMPAT-041), then reads
 * the `path.<name>` session fact (KELD-COMPAT-040 until the Host delivers it).
 */
export function readSessionPath(name: string): never {
  if (!isElectronPathName(name)) {
    throw new AppSessionFactError(
      UNKNOWN_PATH_NAME,
      undefined,
      `app.getPath('${name}') names no Electron v44.4.5 path. Accepted names: ` +
        `${ELECTRON_PATH_NAMES.join(", ")}. Fix: pass one of the documented names.`,
    );
  }
  throw notDelivered(`path.${name}`, `app.getPath('${name}')`);
}

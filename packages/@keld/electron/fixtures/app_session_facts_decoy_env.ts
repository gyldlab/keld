/**
 * Decoy-environment observer for GH-452 T1 (spec gh452 §3 criterion 8).
 *
 * `src/app-session-facts.test.ts` spawns this file with HOME, XDG_CONFIG_HOME,
 * APPDATA, LANG and LC_ALL pointing at decoys. The facade must not read any of
 * them: every session-fact member fails closed with KELD-COMPAT-040 and no
 * string or boolean is produced. A facade that derived `userData` from HOME,
 * `name`/`version` from a package.json, `locale` from Intl or `isPackaged` from
 * a constant prints a `value=` other than `none` here and fails the oracle.
 *
 * `LifecycleLink.connect` is replaced as in the other observers so the import
 * never opens a real link. One `GH452_DECOY` line per member, then exit 0.
 */
import { writeSync } from "node:fs";
import { LifecycleLink } from "../src/link.ts";

type Handlers = Parameters<typeof LifecycleLink.connect>[1];

LifecycleLink.connect = (async (_link: string, _received: Handlers) => {
  return {
    async quit() {},
    close() {},
  };
}) as typeof LifecycleLink.connect;

const { app } = await import("../src/index.ts");

const facade = app as unknown as Record<string, unknown>;
const reads: ReadonlyArray<readonly [string, () => unknown]> = [
  [
    "getPath(userData)",
    () => (facade.getPath as (name: string) => unknown)("userData"),
  ],
  [
    "getPath(documents)",
    () => (facade.getPath as (name: string) => unknown)("documents"),
  ],
  ["getVersion", () => (facade.getVersion as () => unknown)()],
  ["getName", () => (facade.getName as () => unknown)()],
  ["name", () => facade.name],
  ["getAppPath", () => (facade.getAppPath as () => unknown)()],
  ["getLocale", () => (facade.getLocale as () => unknown)()],
  ["isPackaged", () => facade.isPackaged],
];

for (const [member, read] of reads) {
  let line: string;
  try {
    const value = read();
    line = `GH452_DECOY ${member} code=none fact=none value=${typeof value}`;
  } catch (error) {
    const typed = error as Error & { code?: unknown; fact?: unknown };
    const code = typeof typed.code === "string" ? typed.code : String(error);
    line = `GH452_DECOY ${member} code=${code} fact=${String(typed.fact)} value=none`;
  }
  writeSync(1, `${line}\n`);
}
process.exit(0);

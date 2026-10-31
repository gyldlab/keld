/**
 * Decoy-environment and environment-read observer for GH-452 T1 (spec gh452
 * §3 criterion 8).
 *
 * `src/app-session-facts.test.ts` spawns this file with HOME, XDG_CONFIG_HOME,
 * APPDATA, LANG and LC_ALL pointing at decoys. Two independent oracles:
 *
 * 1. `process.env` is replaced by a recording Proxy and `process.cwd` by a
 *    counting wrapper BEFORE any facade module loads (every import below is
 *    dynamic for that reason), so any read of a watched variable or of the cwd
 *    during facade evaluation or during the member reads shows up in the final
 *    `GH452_ENV_READS` line. The facade must produce `watched=[] cwdCalls=0`.
 * 2. Every session-fact member is read at import (before any `ready`) and again
 *    after the facade `ready` event, which this observer reaches through a
 *    `ready` listener after delivering host Ready itself (the link is stubbed as
 *    in the other observers). Each read must fail closed with KELD-COMPAT-040
 *    and yield no value in both phases: a facade that minted a value only once
 *    `isReady()` is true prints a `value=` other than `none` here.
 *
 * One `GH452_DECOY` line per member and phase, one `GH452_ENV_READS` line, exit
 * 0; exit 2 when the facade never opened the link.
 */
import { writeSync } from "node:fs";

const WATCHED_ENV = [
  "HOME",
  "USERPROFILE",
  "APPDATA",
  "LOCALAPPDATA",
  "XDG_CONFIG_HOME",
  "XDG_DATA_HOME",
  "LANG",
  "LC_ALL",
  "LC_MESSAGES",
  "TMPDIR",
];
const watchedReads: string[] = [];
const realEnv = process.env;
process.env = new Proxy(realEnv, {
  get(target, key, receiver) {
    if (typeof key === "string" && WATCHED_ENV.includes(key)) {
      watchedReads.push(key);
    }
    return Reflect.get(target, key, receiver);
  },
  has(target, key) {
    if (typeof key === "string" && WATCHED_ENV.includes(key)) {
      watchedReads.push(key);
    }
    return Reflect.has(target, key);
  },
  ownKeys(target) {
    watchedReads.push("*enumerate*");
    return Reflect.ownKeys(target);
  },
});
let cwdCalls = 0;
const realCwd = process.cwd.bind(process);
process.cwd = (): string => {
  cwdCalls += 1;
  return realCwd();
};

const { LifecycleLink } = await import("../src/link.ts");
type Handlers = Parameters<typeof LifecycleLink.connect>[1];

let handlers: Handlers | undefined;
LifecycleLink.connect = (async (_link: string, received: Handlers) => {
  handlers = received;
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

function readAll(phase: string): void {
  for (const [member, read] of reads) {
    let line: string;
    try {
      const value = read();
      line = `GH452_DECOY phase=${phase} member=${member} code=none fact=none value=${typeof value}`;
    } catch (error) {
      const typed = error as Error & { code?: unknown; fact?: unknown };
      const code = typeof typed.code === "string" ? typed.code : String(error);
      line = `GH452_DECOY phase=${phase} member=${member} code=${code} fact=${String(typed.fact)} value=none`;
    }
    writeSync(1, `${line}\n`);
  }
}

readAll("import");

const ready = new Promise<void>((resolve) => {
  app.on("ready", resolve);
});
await new Promise<void>((resolve) => {
  setImmediate(resolve);
});
if (!handlers) {
  writeSync(2, "GH452_LINK_NOT_OPENED\n");
  process.exit(2);
}
handlers.onReady();
await ready;

readAll("after-ready");
writeSync(
  1,
  `GH452_ENV_READS watched=${JSON.stringify(watchedReads)} cwdCalls=${cwdCalls}\n`,
);
process.exit(0);

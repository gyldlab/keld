/**
 * draw.io-shaped argv consumer for the GH-570 `(argv, defaultApp)` atom.
 *
 * Mirrors pinned drawio-desktop `src/main/electron.js:1056-1062` and
 * `src/main/args.js:191-199`: `argv.unshift(null)` unless
 * `process.defaultApp == true`, then `argv.slice(2).filter(t => t != null)` are
 * the tokens the parser sees. The oracle is the launcher's own argument list;
 * `src/process-facts.test.ts` compares these exact stdout lines against it.
 *
 * Markers use `writeSync(1, …)` so a piped stdout cannot hide them behind
 * block buffering.
 */
import { writeSync } from "node:fs";
import "electron";

const facts = process as typeof process & { defaultApp?: unknown };
const argv: (string | null)[] = [...process.argv];
if (facts.defaultApp != true) {
  argv.unshift(null);
}
const tokens = argv.slice(2).filter((token) => token != null);

writeSync(1, `GH570_DEFAULT_APP=${JSON.stringify(facts.defaultApp)}\n`);
writeSync(1, `GH570_EXEC_ARGV=${JSON.stringify(process.execArgv)}\n`);
writeSync(1, `GH570_ARGV1_IS_ENTRY=${process.argv[1] === import.meta.path}\n`);
writeSync(1, `GH570_USER_ARGS=${JSON.stringify(tokens)}\n`);

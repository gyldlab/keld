/**
 * Proves `probe_preload.ts` observes real entry points through the import forms
 * production code uses: a global, an ESM named import of a Node builtin in three
 * families (net, fs, child_process), a `Bun.*` call and a constructor. Each
 * attempt is expected to be refused by the probe; `caught` records what happened.
 */
import { spawnSync } from "node:child_process";
import {
  constants,
  closeSync,
  readFileSync,
  open,
  openSync,
  write,
  writeSync,
  writev,
  writevSync,
  writeFileSync,
} from "node:fs";
import * as fsPromises from "node:fs/promises";
import { connect } from "node:net";

const caught: Record<string, string> = {};

function attempt(label: string, run: () => unknown): void {
  try {
    run();
    caught[label] = "no-throw";
  } catch (error) {
    caught[label] = error instanceof Error ? error.message : String(error);
  }
}

attempt("fetch", () => fetch("https://example.invalid/gh491"));
attempt("net.connect", () => connect({ host: "127.0.0.1", port: 1 }));
attempt("fs.writeFileSync", () =>
  writeFileSync("/nonexistent-gh491/never-written", "x"),
);
attempt("child_process.spawnSync", () => spawnSync("true"));
attempt("Bun.connect", () =>
  Bun.connect({ hostname: "127.0.0.1", port: 1, socket: {} }),
);
attempt("Worker", () => new Worker("data:text/javascript,"));

for (const [label, run] of [
  ["fs.open", () => open("/nonexistent-gh491/never-written", "w", () => {})],
  ["fs.openSync", () => openSync("/nonexistent-gh491/never-written", "w")],
  ["fs.write", () => write(-1, "x", () => {})],
  ["fs.writeSync", () => writeSync(-1, "x")],
  ["fs.writev", () => writev(-1, [Buffer.from("x")], () => {})],
  ["fs.writevSync", () => writevSync(-1, [Buffer.from("x")])],
] as const)
  attempt(label, run);
attempt("fs.openSync.numeric", () =>
  openSync(
    "/nonexistent-gh491/never-written",
    constants.O_CREAT | constants.O_WRONLY,
  ),
);
try {
  const handle = await fsPromises.open("/nonexistent-gh491/never-written", "w");
  await handle.close();
  caught["fs/promises.open"] = "no-throw";
} catch (error) {
  caught["fs/promises.open"] =
    error instanceof Error ? error.message : String(error);
}

const readFd = openSync(import.meta.path, "r");
const syncRead = readFileSync(readFd, "utf8").includes("GH491_RESULT");
closeSync(readFd);
const readHandle = await fsPromises.open(import.meta.path, "r");
const asyncRead = (await readHandle.readFile("utf8")).includes("GH491_RESULT");
await readHandle.close();
process.stdout.write(
  `GH491_RESULT ${JSON.stringify({ caught, syncRead, asyncRead })}\n`,
);

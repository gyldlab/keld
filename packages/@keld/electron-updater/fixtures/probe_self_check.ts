/**
 * Proves `probe_preload.ts` observes real entry points through the import forms
 * production code uses: a global, an ESM named import of a Node builtin in three
 * families (net, fs, child_process), a `Bun.*` call and a constructor. Each
 * attempt is expected to be refused by the probe; `caught` records what happened.
 */
import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
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

process.stdout.write(`GH491_RESULT ${JSON.stringify({ caught })}\n`);

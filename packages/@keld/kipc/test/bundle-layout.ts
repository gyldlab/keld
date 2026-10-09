/**
 * App-layout runner for the GH-527 §4.2 and #653 staging tests: builds an app
 * layout in a fresh directory, runs it with Bun against a live Unix listener,
 * and reports stdout, the listener's connection count and an evaluation
 * counter. Each run is its own process, because a realm opens one link.
 */
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect } from "bun:test";

/** Any 32-byte token: the listener never answers HELLO. */
export const TOKEN_HEX = "ab".repeat(32);

export interface BundleRun {
  stdout: string;
  connections: number;
  /** Lines appended to `KELD_TEST_EVALS`: one per evaluation of a counting entry. */
  evals: number;
}

export interface BundleLayout {
  /** Builds the app files into `dir`; returns the file name `bun` runs. */
  write: (dir: string) => Promise<string>;
  /** Stop reading stdout once this holds (or the process exits). */
  ready: (stdout: string) => boolean;
  /** End each accepted socket at once, so a Worker that connects fails fast instead of waiting for HELLO. */
  closeOnOpen?: boolean;
  /** Extra environment for the run, such as `KELD_KIPC_TEST_HOOKS`. */
  env?: Record<string, string>;
}

// Runs one app layout in a fresh directory with a live listener and an
// evaluation counter, and returns its stdout, the listener's connection count
// and the counter. Each run is its own process: a realm opens one link.
export async function runLayout(layout: BundleLayout): Promise<BundleRun> {
  const dir = mkdtempSync(join(tmpdir(), "keld-wl-"));
  const path = join(dir, "s.sock");
  const evalsPath = join(dir, "evals.txt");
  let connections = 0;
  const listener = Bun.listen({
    unix: path,
    socket: {
      open(socket) {
        connections += 1;
        if (layout.closeOnOpen) socket.end();
      },
      data() {},
    },
  });
  let proc: ReturnType<typeof Bun.spawn> | undefined;
  try {
    const runner = await layout.write(dir);
    proc = Bun.spawn(["bun", join(dir, runner)], {
      env: { ...process.env, ...layout.env, KELD_APP_LINK: `${path}#${TOKEN_HEX}`, KELD_TEST_EVALS: evalsPath },
      stdout: "pipe",
      stderr: "pipe",
    });
    let stdout = "";
    const reader = (proc.stdout as ReadableStream<Uint8Array>).getReader();
    const decoder = new TextDecoder();
    for (;;) {
      const chunk = await reader.read();
      if (chunk.done) break;
      stdout += decoder.decode(chunk.value, { stream: true });
      if (layout.ready(stdout)) break;
    }
    const counted = existsSync(evalsPath) ? readFileSync(evalsPath, "utf8") : "";
    return { stdout, connections, evals: counted.split("\n").filter((line) => line.length > 0).length };
  } finally {
    proc?.kill();
    listener.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}

// Builds `entrySource` (TypeScript) into one bundle named `name`, beside a
// `main.js` that imports it, and runs `runner`.
export function bundleLayout(name: string, entrySource: string, runner: string, ready: (stdout: string) => boolean) {
  return {
    ready,
    async write(dir: string): Promise<string> {
      const entry = join(dir, "entry.ts");
      await Bun.write(entry, entrySource);
      const built = await Bun.build({ entrypoints: [entry], target: "bun", format: "esm" });
      expect(built.success).toBe(true);
      await Bun.write(join(dir, name), built.outputs[0]!);
      await Bun.write(join(dir, "main.js"), `import "./${name}";\n`);
      return runner;
    },
  } satisfies BundleLayout;
}

export const transportPath = JSON.stringify(join(import.meta.dir, "..", "src", "transport.ts"));
export const openAndReport =
  `import { isMainThread } from "node:worker_threads";\n` +
  `import { WorkerLink, isCallError } from ${transportPath};\n` +
  "WorkerLink.open({ link: process.env.KELD_APP_LINK!, receive: { eventChannels: [3], callReceivers: [] } }).then(\n" +
  '  () => console.log(`${isMainThread ? "main" : "worker"}-open=opened`),\n' +
  '  (err) => console.log(`${isMainThread ? "main" : "worker"}-open=${isCallError(err) ? err.code : "untyped"}`),\n' +
  ");\n";
// The same app code, counting each evaluation of its module in any thread.
export const countedOpenAndReport =
  `import { appendFileSync } from "node:fs";\n` +
  `appendFileSync(process.env.KELD_TEST_EVALS!, "eval\\n");\n` +
  openAndReport;

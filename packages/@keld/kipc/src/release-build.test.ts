/**
 * #528 T3 (owner decision recorded on #528): a release build of the transport
 * has no WorkerLink test-hook code path. The release build is this file built
 * on its own (never inlined into an app entry, §4.2) with the build-time
 * constant `KELD_KIPC_RELEASE` defined as `true` and syntax minification, so
 * every `TEST_HOOKS` branch is dead code and removed.
 */
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const TRANSPORT = join(import.meta.dir, "transport.ts");

/** Every hook name and the hook entry's env gate and seam key. */
const HOOK_NAMES = [
  "openWorkerLinkForTest",
  "deferDispatch",
  "claimFault",
  "onBlockingCall",
  "holdMessagesUntilBlocking",
  "beforeStateCheck",
  "beforeWakeDrain",
  "beforeDeadlineCas",
  "onDispatchIdle",
  "counterStart",
  "wedgeHoldingKick",
  "onWorkerExit",
  "beforeWorkerSpawn",
  "KELD_KIPC_TEST_HOOKS",
  "worker-link-test-seam",
];

async function build(release: boolean): Promise<string> {
  const built = await Bun.build({
    entrypoints: [TRANSPORT],
    target: "bun",
    format: "esm",
    minify: { syntax: true },
    ...(release ? { define: { KELD_KIPC_RELEASE: "true" } } : {}),
  });
  expect(built.success).toBe(true);
  return built.outputs[0]!.text();
}

describe("a release build of the transport strips the WorkerLink test hooks", () => {
  test("it names no hook, and keeps the link, its Worker entry and the Quit", async () => {
    const text = await build(true);
    expect(HOOK_NAMES.filter((name) => text.includes(name))).toEqual([]);
    expect(text).toContain("class WorkerLink");
    expect(text).toContain("keld-kipc-worker-link/v1");
    expect(text).toContain("quitAndCloseLink");
  });

  test("negative control: the same build without the constant keeps every hook", async () => {
    const text = await build(false);
    expect(HOOK_NAMES.filter((name) => !text.includes(name))).toEqual([]);
  });

  test("the release build runs as a staged kipc-transport file", async () => {
    const dir = mkdtempSync(join(tmpdir(), "keld-rel-"));
    try {
      const staged = join(dir, "kipc-transport.js");
      await Bun.write(staged, await build(true));
      const probe = join(dir, "probe.ts");
      await Bun.write(
        probe,
        `import { WorkerLink, isCallError } from ${JSON.stringify(staged)};\n` +
          "const seam = Symbol.for('keld.kipc.worker-link-test-seam/v1') in globalThis;\n" +
          "const code = await WorkerLink.open({ link: '/nonexistent#' + 'ab'.repeat(32), receive: undefined as never })\n" +
          "  .then(() => 'opened', (err) => (isCallError(err) ? err.code : 'untyped'));\n" +
          "console.log(JSON.stringify({ seam, code }));\n",
      );
      const proc = Bun.spawn(["bun", probe], {
        stdout: "pipe",
        stderr: "pipe",
      });
      const [stdout, exit] = await Promise.all([
        new Response(proc.stdout).text(),
        proc.exited,
      ]);
      expect(exit).toBe(0);
      expect(JSON.parse(stdout)).toEqual({ seam: false, code: "KELD-IPC-005" });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }, 30_000);
});

describe("Node cold import and accessor admission (#1289)", () => {
  test("the emitted release module imports in Node and refuses setup before reading options", async () => {
    const dir = mkdtempSync(join(tmpdir(), "keld-node-cold-"));
    try {
      const staged = join(dir, "transport.mjs");
      await Bun.write(staged, await build(true));
      const clean = Bun.spawn(
        [
          "node",
          "-e",
          `import(${JSON.stringify(pathToFileURL(staged).href)}).then(m => { const c = require(${JSON.stringify(staged)}); if (m.WorkerLink !== c.WorkerLink) process.exitCode = 1; }).catch(e => { console.error(e); process.exitCode = 1; })`,
        ],
        { stdout: "pipe", stderr: "pipe" },
      );
      const [cleanStderr, cleanExit] = await Promise.all([
        new Response(clean.stderr).text(),
        clean.exited,
      ]);
      expect({ cleanExit, cleanStderr }).toEqual({
        cleanExit: 0,
        cleanStderr: "",
      });
      const probe = join(dir, "probe.cjs");
      await Bun.write(
        probe,
        `
        (async () => {
          let setups = 0;
          const threads = require('node:worker_threads');
          for (const name of ['MessageChannel', 'Worker']) {
            threads[name] = new Proxy(threads[name], { construct(target, args, newTarget) { setups++; return Reflect.construct(target, args, newTarget); } });
          }
          require('node:module').syncBuiltinESMExports();
          globalThis.SharedArrayBuffer = new Proxy(SharedArrayBuffer, { construct(target, args, newTarget) { setups++; return Reflect.construct(target, args, newTarget); } });
          const esm = await import(${JSON.stringify(pathToFileURL(staged).href)});
          const cjs = require(${JSON.stringify(staged)});
          let hits = 0;
          const options = { get link() { hits++; throw Error('options read'); } };
          Object.defineProperty(require('node:worker_threads').MessageChannel.prototype, 'port1', { configurable: true, get() { hits++; } });
          Object.defineProperty(require('node:worker_threads').MessageChannel.prototype, 'port2', { configurable: true, get() { hits++; } });
          const error = await esm.WorkerLink.open(options).then(() => null, e => e);
          console.log(JSON.stringify({ same: esm.WorkerLink === cjs.WorkerLink, code: error?.code, message: error?.message, hits, setups }));
        })().catch(e => { console.error(e); process.exitCode = 1; });
      `,
      );
      const proc = Bun.spawn(["node", probe], {
        stdout: "pipe",
        stderr: "pipe",
      });
      const [stdout, stderr, exit] = await Promise.all([
        new Response(proc.stdout).text(),
        new Response(proc.stderr).text(),
        proc.exited,
      ]);
      expect({ exit, stderr }).toEqual({ exit: 0, stderr: "" });
      const result = JSON.parse(stdout);
      expect(result.same).toBe(true);
      expect(result.code).toBe("KELD-IPC-005");
      expect(result.message).toContain("Bun");
      expect(result.hits).toBe(0);
      expect(result.setups).toBe(0);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  for (const port of ["port1", "port2"]) {
    for (const shape of ["missing", "noncallable"]) {
      test(`${port} ${shape} at capture refuses before options or construction`, async () => {
        const dir = mkdtempSync(join(tmpdir(), "keld-capture-"));
        try {
          const staged = join(dir, "transport.mjs");
          await Bun.write(staged, await build(true));
          const probe = join(dir, "probe.ts");
          await Bun.write(
            probe,
            `
            import { MessageChannel } from 'node:worker_threads';
            const descriptor = Object.getOwnPropertyDescriptor;
            Object.getOwnPropertyDescriptor = (object, key) => object === MessageChannel.prototype && key === ${JSON.stringify(port)}
              ? (${JSON.stringify(shape)} === 'missing' ? undefined : { get: 1 }) : descriptor(object, key);
            const module = await import(${JSON.stringify(pathToFileURL(staged).href)});
            let hits = 0;
            const options = { get link() { hits++; throw Error('options read'); } };
            const error = await module.WorkerLink.open(options).then(() => null, e => e);
            console.log(JSON.stringify({ code: error?.code, message: error?.message, hits }));
          `,
          );
          const proc = Bun.spawn(["bun", probe], {
            stdout: "pipe",
            stderr: "pipe",
          });
          const [stdout, stderr, exit] = await Promise.all([
            new Response(proc.stdout).text(),
            new Response(proc.stderr).text(),
            proc.exited,
          ]);
          expect({ exit, stderr }).toEqual({ exit: 0, stderr: "" });
          const result = JSON.parse(stdout);
          expect(result.code).toBe("KELD-IPC-005");
          expect(result.message).toContain("Bun");
          expect(result.hits).toBe(0);
        } finally {
          rmSync(dir, { recursive: true, force: true });
        }
      });
    }
  }
});

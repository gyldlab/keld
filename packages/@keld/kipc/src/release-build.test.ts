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
      const proc = Bun.spawn(["bun", probe], { stdout: "pipe", stderr: "pipe" });
      const [stdout, exit] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
      expect(exit).toBe(0);
      expect(JSON.parse(stdout)).toEqual({ seam: false, code: "KELD-IPC-005" });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }, 30_000);
});

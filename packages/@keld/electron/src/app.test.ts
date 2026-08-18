/**
 * Spawns `fixtures/app_link_death.ts` so the connect stub cannot leak into
 * `link.test.ts` (bun test keeps a process-wide module cache across files).
 */
import { afterEach, describe, expect, test } from "bun:test";
import { join } from "node:path";

const fixtures = join(import.meta.dir, "..", "fixtures");

let child: ReturnType<typeof Bun.spawn> | undefined;

afterEach(() => {
  if (child && !child.killed) {
    try {
      child.kill();
    } catch {
      // already reaped
    }
  }
  child = undefined;
});

async function waitChildOrKill(
  proc: ReturnType<typeof Bun.spawn>,
  ms: number,
): Promise<{ stdout: string; stderr: string; code: number }> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const finished = Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ]).then(([stdout, stderr, code]) => ({ stdout, stderr, code }));
    const timeout = new Promise<never>((_, reject) => {
      timer = setTimeout(() => {
        reject(new Error(`app_link_death fixture did not exit within ${ms}ms`));
      }, ms);
    });
    return await Promise.race([finished, timeout]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
    if (!proc.killed) {
      try {
        proc.kill();
      } catch {
        // already reaped
      }
    }
  }
}

describe("app.quit vs Electron void", () => {
  test("returns Promise<void> so unset KELD_APP_LINK is KELD-IPC-007, not silent void", async () => {
    const prev = process.env.KELD_APP_LINK;
    delete process.env.KELD_APP_LINK;
    try {
      const { app } = await import("./app");
      const result = app.quit();
      expect(result).toBeInstanceOf(Promise);
      await expect(result).rejects.toThrow("KELD-IPC-007");
    } finally {
      if (prev === undefined) {
        delete process.env.KELD_APP_LINK;
      } else {
        process.env.KELD_APP_LINK = prev;
      }
    }
  });
});

describe("app.whenReady on link death", () => {
  test(
    "rejects pending whenReady and retries connect after HELLO-then-death",
    async () => {
      child = Bun.spawn({
        cmd: ["bun", "./app_link_death.ts"],
        cwd: fixtures,
        stdout: "pipe",
        stderr: "pipe",
        env: {
          ...process.env,
          KELD_APP_LINK: `1#${"ab".repeat(32)}`,
        },
      });
      const { stdout, stderr, code } = await waitChildOrKill(child, 8_000);
      expect(stderr).toBe("");
      expect(code).toBe(0);
      expect(stdout).toContain("KEL72_WHEN_READY_DEAD");
      expect(stdout).toContain("KEL72_RETRY_READY");
      expect(stdout).toContain("KEL72_CONNECT_CALLS=2");
    },
    10_000,
  );
});

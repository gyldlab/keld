/**
 * Spawns fixtures that stub `LifecycleLink.connect` so the stub cannot leak
 * into `link.test.ts` (bun test keeps a process-wide module cache across files).
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
        reject(new Error(`app fixture did not exit within ${ms}ms`));
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
    "rejects whenReady and retries when onLinkDead runs before connect() returns",
    async () => {
      child = Bun.spawn({
        cmd: ["bun", "./app_sync_link_dead.ts"],
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
      expect(stdout).toContain("KEL72_SYNC_DEAD");
      expect(stdout).toContain("KEL72_SYNC_DEAD_RETRY_READY");
      expect(stdout).toContain("KEL72_CONNECT_CALLS=2");
    },
    10_000,
  );

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

describe("window-all-closed Electron default quit", () => {
  test(
    "LastWindowClosed default-quits with zero listeners, after last removeListener, and not while a listener remains",
    async () => {
      child = Bun.spawn({
        cmd: ["bun", "./window_all_closed_default.ts"],
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
      expect(stdout).toContain("KEL72_DEFAULT_QUIT");
      expect(stdout).toContain("KEL72_WINDOW_ALL_CLOSED_SECOND");
      expect(stdout).toContain("KEL72_DEFAULT_QUIT_AFTER_REMOVE");
      expect(stdout).toContain("KEL72_REMAINING");
      expect(stdout).not.toContain("KEL72_DROPPED_FIRED");
      expect(stdout).toContain("KEL72_QUIT_CALLS=2");
    },
    10_000,
  );
});

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
      new Response(proc.stdout as ReadableStream<Uint8Array>).text(),
      new Response(proc.stderr as ReadableStream<Uint8Array>).text(),
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
  // The title below is pinned by the frozen `electron-lifecycle-v0` corpus cell
  // `app.quit.return-contract` (crates/keld-compat/fixtures/lifecycle-corpus/corpus.json;
  // gh532 AC11 keeps that manifest and its records byte-identical), so it still names
  // KELD-IPC-007 although GH-1118 moved the unset-link code to KELD-CLI-010. The cell
  // proves the return contract: a Promise carrying a typed KELD rejection, never silent
  // void. The exact code is the next test's contract. Retitle this case together with
  // the corpus when a v1 lifecycle manifest supersedes v0.
  test("returns Promise<void> so unset KELD_APP_LINK is KELD-IPC-007, not silent void", async () => {
    const prev = process.env.KELD_APP_LINK;
    delete process.env.KELD_APP_LINK;
    try {
      const { app } = await import("./app");
      const result = app.quit();
      expect(result).toBeInstanceOf(Promise);
      const rejection: unknown = await result.then(
        () => undefined,
        (err: unknown) => err,
      );
      expect(rejection).toBeInstanceOf(Error);
      expect((rejection as Error).message).toMatch(/^KELD-[A-Z]+-?\d{3}: /);
    } finally {
      if (prev === undefined) {
        delete process.env.KELD_APP_LINK;
      } else {
        process.env.KELD_APP_LINK = prev;
      }
    }
  });

  test("an unset KELD_APP_LINK rejects with KELD-CLI-010, the registry code for that condition", async () => {
    const prev = process.env.KELD_APP_LINK;
    delete process.env.KELD_APP_LINK;
    try {
      const { app } = await import("./app");
      const rejection: unknown = await app.quit().then(
        () => undefined,
        (err: unknown) => err,
      );
      expect(rejection).toBeInstanceOf(Error);
      const message = (rejection as Error).message;
      // GH-1118: the shared @keld/api owner reports the unset link under KELD-CLI-010.
      expect(message).toContain("KELD-CLI-010");
      // Negative control: KELD-IPC-007 is reserved for a rejected HELLO session token.
      expect(message).not.toContain("KELD-IPC-007");
    } finally {
      if (prev === undefined) {
        delete process.env.KELD_APP_LINK;
      } else {
        process.env.KELD_APP_LINK = prev;
      }
    }
  });
});

describe("Electron ready event over the shared lifecycle owner", () => {
  test("a ready listener alone starts shared-link readiness observation, isolated from a throwing one", async () => {
    child = Bun.spawn({
      cmd: ["bun", "./ready_listener_only.ts"],
      cwd: fixtures,
      stdout: "pipe",
      stderr: "pipe",
      env: {
        ...process.env,
        KELD_APP_LINK: `1#${"ab".repeat(32)}`,
      },
    });
    const { stdout, stderr, code } = await waitChildOrKill(child, 4_000);
    expect(stderr).toBe("");
    expect(code).toBe(0);
    expect(stdout).toContain("KEL142_READY_LISTENER_ONLY");
    expect(stdout).toContain("KEL72_READY_SECOND");
    expect(stdout).toContain("KEL142_CONNECT_CALLS=1");
  });

  test("quit opening the shared link still delivers the host Ready event", async () => {
    child = Bun.spawn({
      cmd: ["bun", "./ready_via_quit.ts"],
      cwd: fixtures,
      stdout: "pipe",
      stderr: "pipe",
      env: {
        ...process.env,
        KELD_APP_LINK: `1#${"ab".repeat(32)}`,
      },
    });
    const { stdout, stderr, code } = await waitChildOrKill(child, 4_000);
    expect(stderr).toBe("");
    expect(code).toBe(0);
    expect(stdout).toContain("KEL142_READY_VIA_QUIT");
    expect(stdout).toContain("KEL142_CONNECT_CALLS=1");
  });
});

describe("app.whenReady on link death", () => {
  test(
    "rejects whenReady, and keeps rejecting without a reconnect, when onLinkDead runs before connect() returns",
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
      expect(stdout).toContain("KEL72_SYNC_DEAD_STICKY");
      expect(stdout).toContain("KEL72_CONNECT_CALLS=1");
    },
    10_000,
  );

  test(
    "rejects pending whenReady after HELLO-then-death, and keeps rejecting without a reconnect",
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
      expect(stdout).toContain("KEL72_DEATH_STICKY");
      expect(stdout).toContain("KEL72_CONNECT_CALLS=1");
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

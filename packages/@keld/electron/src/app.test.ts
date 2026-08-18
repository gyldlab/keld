/**
 * Spawns `fixtures/app_link_death.ts` so the connect stub cannot leak into
 * `link.test.ts` (bun test keeps a process-wide module cache across files).
 */
import { describe, expect, test } from "bun:test";
import { join } from "node:path";

const fixtures = join(import.meta.dir, "..", "fixtures");

describe("app.whenReady on link death", () => {
  test("rejects pending whenReady and retries connect after HELLO-then-death", async () => {
    const proc = Bun.spawn({
      cmd: ["bun", "./app_link_death.ts"],
      cwd: fixtures,
      stdout: "pipe",
      stderr: "pipe",
      env: {
        ...process.env,
        KELD_APP_LINK: `1#${"ab".repeat(32)}`,
      },
    });
    const [stdout, stderr, code] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ]);
    expect(stderr).toBe("");
    expect(code).toBe(0);
    expect(stdout).toContain("KEL72_WHEN_READY_DEAD");
    expect(stdout).toContain("KEL72_RETRY_READY");
    expect(stdout).toContain("KEL72_CONNECT_CALLS=2");
  });
});

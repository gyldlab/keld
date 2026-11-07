import assert from "node:assert/strict";
import { join } from "node:path";
import { test } from "bun:test";
import { APP_LINK_IO_DEADLINE_MS, withIoDeadline } from "@keld/kipc";

for (const scenario of [
  "success",
  "refusal",
  "explicit-close",
  "subscription-failure",
  "selection-delete",
  "selection-zero",
]) {
  test(`local termination releases window source after earlier fact drain: ${scenario}`, async () => {
    const child = Bun.spawn(
      [
        process.execPath,
        join(import.meta.dir, "link-quit-release.test-role.ts"),
        scenario,
      ],
      {
        env: { ...process.env, KELD_NATIVE_WINDOW_FACADE: "1" },
        stdin: "ignore",
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    try {
      const [code, stdout, stderr] = await withIoDeadline(
        Promise.all([
          child.exited,
          new Response(child.stdout).text(),
          new Response(child.stderr).text(),
        ]),
        APP_LINK_IO_DEADLINE_MS,
      );
      assert.equal(code, 0, `${stdout}\n${stderr}`);
      assert.equal(stdout.trim(), `PASS ${scenario}`);
      assert.equal(stderr, "");
    } finally {
      if (child.exitCode === null) child.kill();
      await child.exited;
    }
  });
}

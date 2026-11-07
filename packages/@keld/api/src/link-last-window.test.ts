import assert from "node:assert/strict";
import { join } from "node:path";
import { test } from "bun:test";
import { APP_LINK_IO_DEADLINE_MS, withIoDeadline } from "@keld/kipc";

test("sync Destroy may consume authenticated LastWindowClosed once before delayed dispatch", async () => {
  const child = Bun.spawn(
    [process.execPath, join(import.meta.dir, "link-last-window.test-role.ts")],
    {
      env: { ...process.env, KELD_NATIVE_WINDOW_FACADE: "1" },
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    },
  );
  try {
    const [exit, stdout, stderr] = await withIoDeadline(
      Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
      ]),
      APP_LINK_IO_DEADLINE_MS,
    );
    assert.equal(exit, 0, `${stdout}\n${stderr}`);
    assert.equal(
      stdout.trim(),
      "PASS correlated-last-window-apply-before-destroy-reply",
    );
    assert.equal(stderr, "");
  } finally {
    if (child.exitCode === null) child.kill();
    await child.exited;
  }
});

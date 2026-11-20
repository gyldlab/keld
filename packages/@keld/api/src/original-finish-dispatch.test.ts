/** Keep the process-wide link mock isolated from unrelated Bun tests. */
import assert from "node:assert/strict";
import { join } from "node:path";
import { test } from "bun:test";
import { APP_LINK_IO_DEADLINE_MS, withIoDeadline } from "@keld/kipc";

test("original finish dispatch follows the actual app Ready and link admission", async () => {
  const child = Bun.spawn(
    [
      process.execPath,
      join(import.meta.dir, "original-finish-dispatch.test-role.ts"),
    ],
    { stdin: "ignore", stdout: "pipe", stderr: "pipe" },
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
    assert.equal(stdout.trim(), "PASS original-finish-app-admission");
    assert.equal(stderr, "");
  } finally {
    if (child.exitCode === null) child.kill();
    await child.exited;
  }
});

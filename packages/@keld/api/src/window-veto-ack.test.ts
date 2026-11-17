import assert from "node:assert/strict";
import { join } from "node:path";
import { test } from "bun:test";

test("actual app link Veto ack clears only Host-confirmed exact close sequence", async () => {
  const child = Bun.spawn(
    [process.execPath, join(import.meta.dir, "window-veto-ack.test-role.ts")],
    { stdin: "ignore", stdout: "pipe", stderr: "pipe" },
  );
  try {
    const [code, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    assert.equal(code, 0, `${stdout}\n${stderr}`);
    assert.equal(
      stdout.trim(),
      "PASS host-veto-ack-clears-only-exact-sequence\nPASS earlier-answer-independent-of-later-source-failure",
    );
    assert.equal(stderr, "");
  } finally {
    if (child.exitCode === null) child.kill();
    await child.exited;
  }
});

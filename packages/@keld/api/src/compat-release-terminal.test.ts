import assert from "node:assert/strict";
import { join } from "node:path";
import { test } from "bun:test";
import { withIoDeadline } from "@keld/kipc";

test("ordinary compatibility release refusal keeps its exact terminal reason", async () => {
  const role = Bun.spawn(
    [
      process.execPath,
      join(import.meta.dir, "compat-release-terminal.test-role.ts"),
    ],
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
        role.exited,
        new Response(role.stdout).text(),
        new Response(role.stderr).text(),
      ]),
    );
    assert.equal(exit, 0, `${stdout}\n${stderr}`);
    assert.equal(stdout.trim(), "PASS compat release terminal cause");
    assert.equal(stderr, "");
  } finally {
    if (role.exitCode === null) role.kill();
    await role.exited;
  }
});

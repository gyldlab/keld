import assert from "node:assert/strict";
import { join } from "node:path";
import { test } from "bun:test";

test("real @keld/api Host Ready publication is after synchronous ring drain", async () => {
  const child = Bun.spawn(
    [process.execPath, join(import.meta.dir, "ready-publication.test-role.ts")],
    { stdin: "ignore", stdout: "pipe", stderr: "pipe" },
  );
  try {
    const [code, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    assert.equal(code, 0, `${stdout}\n${stderr}`);
    assert.equal(stdout.trim(), "PASS actual-api-ready-microtask");
    assert.equal(stderr, "");
  } finally {
    if (child.exitCode === null) child.kill();
    await child.exited;
  }
});

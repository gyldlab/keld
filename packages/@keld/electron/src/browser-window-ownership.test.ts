/** Constructor and lifecycle ownership faults stay isolated from the Bun runner. */
import assert from "node:assert/strict";
import { join } from "node:path";
import { test } from "bun:test";
import {
  APP_LINK_IO_DEADLINE_MS,
  withIoDeadline,
} from "../../kipc/src/transport.ts";

for (const scenario of [
  "post-create-map-has",
  "post-create-map-get",
  "post-create-map-set",
  "post-create-define-property",
  "post-create-freeze",
  "weakmap-set",
  "terminal-map-delete",
  "terminal-weakmap-get",
  "post-create-cleanup",
  "post-create-cleanup-fault",
  "post-create-cleanup-no-tombstone",
  "post-create-cleanup-unexpected-reply",
  "unknown-create-outcome",
  "published-listener-throws",
  "adopted-setup-failure",
]) {
  test(`GH531 constructor ownership ${scenario}`, async () => {
    const child = Bun.spawn(
      [
        process.execPath,
        join(import.meta.dir, "browser-window-ownership.test-role.ts"),
        scenario,
      ],
      { stdin: "ignore", stdout: "pipe", stderr: "pipe" },
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
      assert.equal(exit, 0, `${scenario}: ${stdout}\n${stderr}`);
      assert.equal(stdout.trim(), `PASS ${scenario}`);
      assert.equal(stderr, "");
    } finally {
      if (child.exitCode === null) child.kill();
      await child.exited;
    }
  });
}

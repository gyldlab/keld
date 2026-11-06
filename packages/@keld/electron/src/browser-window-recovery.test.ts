/** Boundary regressions: independent Bun process keeps transport mocks isolated. */
import assert from "node:assert/strict";
import { join } from "node:path";
import { test } from "bun:test";
import {
  APP_LINK_IO_DEADLINE_MS,
  withIoDeadline,
} from "../../kipc/src/transport.ts";

for (const scenario of [
  "late-import-replay",
  "ready-same-turn-continuation",
  "applied-ahead-close-order",
  "recovery-gap",
  "replay-order",
  "pre-ready-close-order",
  "move-only-does-not-resize",
  "pre-ready-bounds-baseline",
  "native-fact-event-parity",
  "bounds-throwing-move-isolated",
  "bounds-move-destroys-no-resize",
  "throwing-contents-destroyed",
  "no-public-contents-dispose",
  "throwing-close-and-logger",
  "close-listener-sync-destroy",
  "replay-link-death",
  "adopted-without-replay-wrapper",
  "adopted-after-replay-wrapper",
  "unmaterialized-close",
  "mutable-id",
  "warning-throws-after-create",
]) {
  const severity = [
    "replay-order",
    "replay-link-death",
    "move-only-does-not-resize",
    "throwing-contents-destroyed",
    "warning-throws-after-create",
    "bounds-throwing-move-isolated",
    "bounds-move-destroys-no-resize",
  ].includes(scenario)
    ? "P2"
    : "P1";
  test(`GH531 ${severity} ${scenario} dispatch and ownership`, async () => {
    const child = Bun.spawn(
      [
        process.execPath,
        join(import.meta.dir, "browser-window-recovery.test-role.ts"),
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

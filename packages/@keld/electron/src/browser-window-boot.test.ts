/** Immutable launch selection and persistent poison use separate preloaded Bun roles. */
import assert from "node:assert/strict";
import { join } from "node:path";
import { test } from "bun:test";
import {
  APP_LINK_IO_DEADLINE_MS,
  withIoDeadline,
} from "../../kipc/src/transport.ts";

for (const [scenario, boot] of [
  ["native-delete-before-import", "1"],
  ["native-zero-before-import", "1"],
  ["legacy-set-before-import", "0"],
  ["legacy-set-before-import", undefined],
  ["poison-map-get", "1"],
  ["poison-map-set", "1"],
  ["poison-map-has", "1"],
  ["poison-map-delete", "1"],
  ["poison-map-values", "1"],
  ["poison-map-constructor", "1"],
  ["poison-weakmap-constructor", "1"],
  ["poison-weakmap-get", "1"],
  ["poison-weakmap-set", "1"],
] as const) {
  test(`GH531 preloaded facade ${scenario} launch=${boot ?? "unset"}`, async () => {
    const childEnv = { ...process.env };
    if (boot === undefined) delete childEnv.KELD_NATIVE_WINDOW_FACADE;
    else childEnv.KELD_NATIVE_WINDOW_FACADE = boot;
    const child = Bun.spawn(
      [
        process.execPath,
        "--preload",
        join(import.meta.dir, "../../kipc/src/transport.ts"),
        join(import.meta.dir, "browser-window-boot.test-role.ts"),
        scenario,
      ],
      { env: childEnv, stdin: "ignore", stdout: "pipe", stderr: "pipe" },
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

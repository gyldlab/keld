import assert from "node:assert/strict";
import { join } from "node:path";
import { test } from "bun:test";
import { APP_LINK_IO_DEADLINE_MS, withIoDeadline } from "@keld/kipc";

for (const mode of [
  "valid",
  "missing",
  "invalidated",
  "unavailable",
  "unselected",
  "foreign-host",
]) {
  test(`desktop Ready over the existing link: ${mode}`, async () => {
    const child = Bun.spawn(
      [
        process.execPath,
        `--define=__KELD_WINDOWS_HOST__:${mode !== "foreign-host"}`,
        ...(mode === "unselected"
          ? []
          : ["--define=__KELD_WINDOWS_DESKTOP_SNAPSHOT__:true"]),
        join(import.meta.dir, "ready-desktop-link.test-role.ts"),
        mode,
      ],
      {
        env: {
          ...process.env,
          KELD_NATIVE_WINDOW_FACADE: "1",
          __KELD_WINDOWS_DESKTOP_SNAPSHOT__: "true",
        },
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
    } finally {
      if (child.exitCode === null) child.kill();
      await child.exited;
    }
  });
}

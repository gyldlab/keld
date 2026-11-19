import assert from "node:assert/strict";
import { join } from "node:path";
import { test } from "bun:test";
import { APP_LINK_IO_DEADLINE_MS, withIoDeadline } from "@keld/kipc";

for (const mode of ["selected", "unselected", "invalidated"]) {
  test(`Windows first-ready desktop getters: ${mode}`, async () => {
    const child = Bun.spawn(
      [
        process.execPath,
        "--no-install",
        "--define=__KELD_WINDOWS_HOST__:true",
        `--define=__KELD_WINDOWS_DESKTOP_SNAPSHOT__:${mode !== "unselected"}`,
        join(import.meta.dir, "desktop-getters.test-role.ts"),
        mode,
      ],
      {
        env: {
          ...process.env,
          KELD_APP_LINK: "controlled-link",
          KELD_NATIVE_WINDOW_FACADE: "1",
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

import { expect, test } from "bun:test";
import { join } from "node:path";
import { APP_LINK_IO_DEADLINE_MS, withIoDeadline } from "@keld/kipc";

for (const mode of [
  "control",
  "map",
  "push",
  "iterator",
  "array-brand",
  "integer",
  "finite",
  "set-has",
  "set-add",
  "float-read",
  "buffer",
  "offset",
  "early-data-view",
]) {
  test(`desktop state has no application hook under ${mode}`, async () => {
    const child = Bun.spawn(
      [
        process.execPath,
        "--no-install",
        join(import.meta.dir, "window-desktop-state.test-role.ts"),
        mode,
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
      expect({ exit, stdout, stderr }).toEqual({
        exit: 0,
        stdout:
          JSON.stringify({ mode, hits: 0, callbacks: 1, primary: 17 }) + "\n",
        stderr: "",
      });
    } finally {
      if (child.exitCode === null) child.kill();
      await child.exited;
    }
  });
}

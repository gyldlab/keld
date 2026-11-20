/** Keep module mocks inside the child, independent of the Bun test runner. */
import { expect, test } from "bun:test";
import { join } from "node:path";
import {
  APP_LINK_IO_DEADLINE_MS,
  withIoDeadline,
} from "../../kipc/src/transport.ts";

test("selected native finishes only authorize the matching current callback", async () => {
  const child = Bun.spawn(
    [
      process.execPath,
      join(import.meta.dir, "windows-navigation-finish.test-role.ts"),
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
    expect({ exit, stdout: stdout.trim(), stderr }).toEqual({
      exit: 0,
      stdout: "PASS windows-navigation-finish",
      stderr: "",
    });
  } finally {
    if (child.exitCode === null) child.kill();
    await child.exited;
  }
});

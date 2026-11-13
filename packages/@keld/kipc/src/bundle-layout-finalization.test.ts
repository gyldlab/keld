import { expect, test } from "bun:test";
import { join } from "node:path";
import { withIoDeadline } from "./transport.ts";

// Risky resource lifetimes run in isolated children, with exact success output
// and exit status. Every fixture also keeps separate emergency cleanup.
for (const mode of [
  "unowned",
  "cancel-eof",
  "owned",
  "retry",
  "pending",
  "outer",
  "outer-unowned",
  "method-owner",
  "native-pipe",
]) {
  test(`layout source finalization: ${mode}`, async () => {
    const child = Bun.spawn(
      [
        process.execPath,
        join(import.meta.dir, "../test/layout-source-finalization.fixture.ts"),
        mode,
      ],
      { stdout: "pipe", stderr: "pipe" },
    );
    try {
      const [status, stdout, stderr] = await withIoDeadline(
        Promise.all([
          child.exited,
          new Response(child.stdout).text(),
          new Response(child.stderr).text(),
        ]),
        10_000,
      );
      expect(stderr).toBe("");
      expect(status).toBe(0);
      expect(stdout).toBe(`FINALIZATION_OK ${mode}\n`);
    } finally {
      child.kill();
      await withIoDeadline(child.exited);
    }
  }, 15_000);
}

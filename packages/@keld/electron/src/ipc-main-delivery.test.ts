import { expect, test } from "bun:test";

test("isolated main registry/delivery contracts", async () => {
  const child = Bun.spawn(
    [
      process.execPath,
      "test",
      import.meta.dir + "/ipc-main-delivery.test-role.ts",
    ],
    { stdout: "pipe", stderr: "pipe" },
  );
  const [stdout, stderr, exit] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  expect({ stdout, stderr, exit }).toMatchObject({ exit: 0 });
  expect(stderr).toContain("12 pass");
});

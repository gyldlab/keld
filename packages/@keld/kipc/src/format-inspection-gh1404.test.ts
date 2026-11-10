import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

test("diagnostic: print exact Bun formatted GH1404 hostile test", () => {
  const target = `${import.meta.dir}/endpoint-value-codec-hostile.test.ts`;
  const before = readFileSync(target, "utf8");
  const child = Bun.spawnSync(["bun", "fmt", "--no-script", target], {
    cwd: import.meta.dir,
    stdout: "pipe",
    stderr: "pipe",
  });
  const after = readFileSync(target, "utf8");
  console.log("GH1404_FORMAT_BEGIN\n" + after + "\nGH1404_FORMAT_END");
  console.log("GH1404_FORMAT_STATUS=" + JSON.stringify({
    exitCode: child.exitCode,
    changed: before !== after,
    stderr: new TextDecoder().decode(child.stderr).slice(0, 400),
  }));
  expect(child.exitCode).toBe(0);
});

// Temporary formatter receipt for the KELD CI-only toolchain; remove after capture.
import { expect, test } from "bun:test";
import {
  copyFileSync,
  mkdtempSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

test("temporary pinned Bun formatter receipt", () => {
  const scratch = mkdtempSync(join(tmpdir(), "keld-fmt-receipt-"));
  try {
    const fixture = join(scratch, "fixture.ts");
    const unit = join(scratch, "unit.test.ts");
    copyFileSync(new URL("../fixtures/installed-alias-types.ts", import.meta.url), fixture);
    copyFileSync(new URL("./installed-alias-types.test.ts", import.meta.url), unit);
    const result = Bun.spawnSync(
      [process.execPath, "fmt", "--no-script", fixture, unit],
      { stdout: "pipe", stderr: "pipe", timeout: 10_000 },
    );
    expect(result.exitCode, new TextDecoder().decode(result.stderr)).toBe(0);
    console.log("KELD_FMT_FIXTURE:" + JSON.stringify(readFileSync(fixture, "utf8")));
    console.log("KELD_FMT_UNIT:" + JSON.stringify(readFileSync(unit, "utf8")));
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});

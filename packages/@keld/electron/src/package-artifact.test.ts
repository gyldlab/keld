import { expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

test("installed Electron aliases share one entry and declared types on Bun and Node26", () => {
  const source = resolve(import.meta.dir, "../../../..");
  const retained = process.env.KELD_ELECTRON_PACKAGE_RECEIPTS;
  const receipts = retained
    ? resolve(retained)
    : mkdtempSync(join(tmpdir(), "keld-alias-receipts-"));
  let passed = false;
  try {
    const node = Bun.which("node");
    const npm = Bun.which("npm");
    expect(
      node,
      "Node26 is required by the installed package contract",
    ).not.toBeNull();
    expect(
      npm,
      "npm is required for the actual isolated offline install",
    ).not.toBeNull();
    const result = Bun.spawnSync(
      [
        process.execPath,
        resolve(import.meta.dir, "../fixtures/installed-alias-regression.ts"),
        source,
        receipts,
        node!,
        npm!,
        "KELD-COMPAT-051",
      ],
      {
        cwd: source,
        env: {
          ...process.env,
          KELD_ELECTRON_KEEP_FIXTURE: retained ? "1" : "0",
        },
        stdout: "pipe",
        stderr: "pipe",
        timeout: 170_000,
      },
    );
    const output =
      new TextDecoder().decode(result.stdout) +
      new TextDecoder().decode(result.stderr);
    expect(result.exitCode, output).toBe(0);
    expect(output).toContain("INSTALLED_ALIAS_COMPONENT_PASS");
    const receipt = JSON.parse(
      readFileSync(join(receipts, "installed-receipt.json"), "utf8"),
    );
    expect(
      receipt.runtimes.map(
        (runtime: { observations: number }) => runtime.observations,
      ),
    ).toEqual([40, 40]);
    passed = true;
  } finally {
    if (passed && !retained) rmSync(receipts, { recursive: true, force: true });
    else console.log("INSTALLED_ALIAS_RECEIPTS " + receipts);
  }
}, 180_000);

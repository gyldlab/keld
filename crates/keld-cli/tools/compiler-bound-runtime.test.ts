// Qualified Windows acceptance for the Rust production compiler binder.
// Set KELD_COMPILER_TEST_REQUIRED=1 in the native acceptance run. That makes
// missing tool paths/digests a hard failure instead of a skipped test.
import { expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const repo = resolve(import.meta.dir, "../../..");
const required = process.env.KELD_COMPILER_TEST_REQUIRED === "1";
const bun = process.env.KELD_COMPILER_TEST_BUN ?? process.execPath;
const qualified = [
  process.env.KELD_COMPILER_TEST_TYPESCRIPT_ROOT,
  process.env.KELD_COMPILER_TEST_TSGO,
  process.env.KELD_COMPILER_TEST_BUN_SHA256,
  process.env.KELD_COMPILER_TEST_TSGO_SHA256,
].every(Boolean);
const acceptanceTest = required || qualified ? test : test.skip;

acceptanceTest(
  "Rust production binder executes the exact pinned Bun/TypeScript compiler closure",
  () => {
    const requiredVariables = [
      "KELD_COMPILER_TEST_TYPESCRIPT_ROOT",
      "KELD_COMPILER_TEST_TSGO",
      "KELD_COMPILER_TEST_BUN_SHA256",
      "KELD_COMPILER_TEST_TSGO_SHA256",
    ];
    const missing = requiredVariables.filter(
      (name) => !process.env[name] || process.env[name]!.length === 0,
    );
    if (missing.length > 0) {
      throw new Error(
        "qualified acceptance must provide every pinned tool path and digest: " +
          missing.join(", "),
      );
    }

    const result = spawnSync(
      process.env.CARGO ?? "cargo",
      [
        "test",
        "--frozen",
        "--offline",
        "-p",
        "keld-cli",
        "--lib",
        "rust_production_binder_runs_five_qualified_compiler_acceptance_cases",
        "--",
        "--nocapture",
      ],
      {
        cwd: repo,
        env: {
          ...process.env,
          KELD_COMPILER_TEST_REQUIRED: "1",
          KELD_COMPILER_TEST_BUN: bun,
        },
        windowsHide: true,
        timeout: 600_000,
        maxBuffer: 16 * 1024 * 1024,
      },
    );
    if (result.error) throw result.error;
    const stdout = result.stdout.toString("utf8");
    const stderr = result.stderr.toString("utf8");
    expect(result.status, stdout + "\n" + stderr).toBe(0);
    expect(stdout).toContain("KELD_COMPILER_BINDER_ACCEPTANCE: 5/5");
    for (const name of [
      "positive",
      "hostile HOME",
      "ambient package",
      "missing member",
      "filesystem escape",
    ]) {
      expect(stdout).toContain("KELD_COMPILER_BINDER_CASE: " + name);
    }
  },
  620_000,
);

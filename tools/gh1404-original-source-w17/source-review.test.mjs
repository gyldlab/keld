import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { pins, assertPinned, produceReview, sha256 } from "./source-review.mjs";
const root = process.env.GH1404_PINNED_PRISTINE_DESKTOP;
const adapter = process.env.GH1404_PINNED_1506_ADAPTER;
const output = process.env.GH1404_WORKER17_SOURCE_OUTPUT;
test("mutated pristine source is refused by independent exact hashes", () => {
  const wrong = Buffer.from("replaced");
  assert.throws(() => assertPinned("src/main/electron.js", wrong), /SOURCE PIN MISMATCH/);
  assert.equal(pins["src/main/electron.js"][0].length, 64);
  assert.equal(sha256(wrong).length, 64);
});
test("source-only producer rejects missing independent input", async () => {
  await assert.rejects(() => produceReview(), /Usage:/);
  await assert.rejects(() => produceReview(root, adapter, undefined), /Usage:/);
});
test("pinned pristine source + exact #1506 adapter yield bounded source and review-only manifest", { skip: !(root && adapter && output) }, async () => {
  const manifest = await produceReview(root, adapter, output);
  const main = readFileSync(output + "/src/main/electron.js", "utf8");
  assert.match(main, /let argv = originalParserArgv\(/);
  assert.match(main, /hostLaunchFacts, runtimeProcessFactsSelected/);
  assert.match(main, /saveAndClose/);
  assert.match(main, /Discard Changes/);
  assert.match(main, /app\.on\("open-file"/);
  assert.doesNotMatch(main, /argv\.unshift\(null\)/);
  assert.equal(sha256(Buffer.from(main)), manifest.proposed_main_sha256);
  assert.equal(manifest.independent_owner_approval_count, 0);
  assert.equal(manifest.approved_permissions_policy, null);
  assert.equal(manifest.approved_migrated_package_and_solver_lock, null);
  await assert.rejects(() => produceReview(root, adapter, output), /refuse existing output/);
});

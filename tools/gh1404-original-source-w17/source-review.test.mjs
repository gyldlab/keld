import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { test } from "node:test";
import { runInNewContext } from "node:vm";
import { pins, assertPinned, produceReview, sha256 } from "./source-review.mjs";
const root = process.env.GH1404_PINNED_PRISTINE_DESKTOP;
const adapter = process.env.GH1404_PINNED_1506_ADAPTER;
const output = process.env.GH1404_WORKER17_SOURCE_OUTPUT;
test("mutated pristine source is refused by independent exact hashes", () => {
  const wrong = Buffer.from("replaced");
  assert.throws(
    () => assertPinned("src/main/electron.js", wrong),
    /SOURCE PIN MISMATCH/,
  );
  assert.equal(pins["src/main/electron.js"][0].length, 64);
  assert.equal(sha256(wrong).length, 64);
});
test("source-only producer rejects missing independent input", async () => {
  await assert.rejects(() => produceReview(), /Usage:/);
  await assert.rejects(() => produceReview(root, adapter, undefined), /Usage:/);
});
test(
  "pinned pristine source + exact #1506 adapter yield bounded source and review-only manifest",
  { skip: !(root && adapter && output) },
  async () => {
    if (process.versions.bun) {
      // Bun transforms function.toString(): it must not mint a different source artifact.
      await assert.rejects(
        () => produceReview(root, adapter, output),
        /generated main pin mismatch/,
      );
      assert.equal(existsSync(output), false);
      return;
    }
    const manifest = await produceReview(root, adapter, output);
    const main = readFileSync(output + "/src/main/electron.js", "utf8");
    assert.match(main, /let argv = originalParserArgv\(/);
    assert.match(main, /hostLaunchFacts, runtimeProcessFactsSelected/);
    assert.match(main, /saveAndClose/);
    assert.match(main, /Discard Changes/);
    assert.match(main, /app\.on\("open-file"/);
    assert.doesNotMatch(main, /argv\.unshift\(null\)/);
    assert.equal(sha256(Buffer.from(main)), manifest.proposed_main_sha256);
    assert.equal(
      manifest.proposed_main_sha256,
      "f2969473b298e6b8ad55577b7e6494db691b8273f22d69ef8ad5cf33c477fa9b",
    );
    assert.equal(
      manifest.parser_proposal_commit,
      "40861a8c772d6bfd99849559a126d773722974a4",
    );
    assert.equal(manifest.status, "UNAPPROVED_INCOMPLETE_NOT_INSTALLABLE");
    const helper = main.match(/function originalParserArgv\([\s\S]*?\n}/)?.[0];
    assert(helper, "generated original must contain the parser adapter");
    const parse = runInNewContext("(" + helper + ")");
    const facts = {
      schema: 7,
      kind: "local-development",
      originalResourcePolicy: { contract: "gh473-original-editor-offline/v1" },
    };
    for (const mode of [true, false, undefined]) {
      const raw = Object.freeze(["bun", "bootstrap"]);
      assert.equal(
        JSON.stringify(parse(raw, mode, true, facts)),
        '["bun",null]',
      );
      assert.deepEqual(raw, ["bun", "bootstrap"]);
      for (const operand of ["original.js", "selected.drawio"]) {
        assert.throws(
          () => parse([...raw, operand], mode, true, facts),
          /KELD-IPC-005/,
        );
      }
      assert.throws(() => parse(raw, mode, true, undefined), /KELD-IPC-005/);
      assert.throws(() => parse(raw, mode, false, facts), /KELD-IPC-005/);
    }
    assert.equal(manifest.independent_owner_approval_count, 0);
    assert.equal(manifest.approved_permissions_policy, null);
    assert.equal(manifest.approved_migrated_package_and_solver_lock, null);
    await assert.rejects(
      () => produceReview(root, adapter, output),
      /refuse existing output/,
    );
  },
);

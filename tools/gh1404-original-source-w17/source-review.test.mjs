import assert from "node:assert/strict";
import {
  existsSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { runInNewContext } from "node:vm";
import {
  pins,
  assertPinned,
  produceReview,
  sha256,
  materializeReviewFiles,
  claimNewReviewDirectory,
} from "./source-review.mjs";
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
test("source-only review cannot put generated files inside its pristine input tree", async () => {
  const pristine = mkdtempSync(
    join(tmpdir(), "gh1404-pristine-output-refusal-"),
  );
  try {
    const outputInsideSource = join(pristine, "generated-review");
    await assert.rejects(
      () =>
        produceReview(
          pristine,
          join(pristine, "missing-adapter"),
          outputInsideSource,
        ),
      /review output overlaps pristine source/,
    );
    assert.equal(existsSync(outputInsideSource), false);
  } finally {
    rmSync(pristine, { recursive: true, force: true });
  }
});

test("source-only review rejects an output aliased into the pristine tree", async () => {
  const parent = mkdtempSync(join(tmpdir(), "gh1404-pristine-output-alias-"));
  try {
    const pristine = join(parent, "pristine");
    const alias = join(parent, "alias");
    mkdirSync(pristine);
    symlinkSync(pristine, alias, "dir");
    await assert.rejects(
      () =>
        produceReview(
          pristine,
          join(parent, "missing-adapter"),
          join(alias, "review"),
        ),
      /symlinked review output parent|review output overlaps pristine source/,
    );
    assert.equal(existsSync(join(pristine, "review")), false);
  } finally {
    rmSync(parent, { recursive: true, force: true });
  }
});

test("source-only materializer does not write through a symlinked immediate parent", () => {
  const parent = mkdtempSync(join(tmpdir(), "gh1404-review-parent-alias-"));
  try {
    const destination = join(parent, "elsewhere");
    const alias = join(parent, "alias");
    mkdirSync(destination);
    symlinkSync(destination, alias, "dir");
    assert.throws(
      () =>
        materializeReviewFiles(join(alias, "review"), [
          ["main.js", "unapproved"],
        ]),
      /symlinked review output parent/,
    );
    assert.equal(existsSync(join(destination, "review")), false);
  } finally {
    rmSync(parent, { recursive: true, force: true });
  }
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
    const diskManifest = JSON.parse(
      readFileSync(join(output, "REVIEW-MANIFEST.UNAPPROVED.json"), "utf8"),
    );
    assert.deepEqual(diskManifest, manifest);
    assert.equal(
      manifest.proposed_main_sha256,
      "cc240f18df8eb102c4fc3c0d75f4f6da21129a8d32835b43fe260b6a3bfbc0de",
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

test("reviewed digest pairs are immutable, including exported receipt pins", () => {
  for (const [name, pair] of Object.entries(pins)) {
    assert.equal(Object.isFrozen(pair), true, name + " digest pair is mutable");
    assert.throws(() => {
      pair[0] = "0".repeat(64);
    }, TypeError);
    assert.throws(() => {
      pair[1] = "0".repeat(40);
    }, TypeError);
  }
  assert.throws(
    () => assertPinned("src/main/electron.js", Buffer.from("replaced")),
    /SOURCE PIN MISMATCH/,
  );
});

test("review output requires an exclusive new directory, never adopts foreign files", () => {
  const root = mkdtempSync(join(tmpdir(), "gh1404-source-review-exclusive-"));
  try {
    const output = join(root, "review");
    assert.equal(existsSync(output), false);
    // A second owner creates this destination after the early absence check.
    mkdirSync(output);
    writeFileSync(join(output, "foreign.txt"), "keep this foreign record");
    assert.throws(
      () =>
        materializeReviewFiles(output, [["src/main/electron.js", "candidate"]]),
      /existing output|EEXIST/,
    );
    assert.equal(
      readFileSync(join(output, "foreign.txt"), "utf8"),
      "keep this foreign record",
    );
    assert.equal(existsSync(join(output, "src")), false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("late write failure removes only the owned partial source review", () => {
  const root = mkdtempSync(join(tmpdir(), "gh1404-source-review-rollback-"));
  try {
    const output = join(root, "review");
    assert.throws(
      () =>
        materializeReviewFiles(output, [
          ["src/main/generated.js", "first author file"],
          ["src/main/generated.js", "duplicate must refuse exclusive creation"],
        ]),
      /EEXIST/,
    );
    assert.equal(
      existsSync(output),
      false,
      "no partial review is left looking complete",
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("ordinary reviewed output writes exact bytes without granting approval", () => {
  const root = mkdtempSync(join(tmpdir(), "gh1404-source-review-valid-"));
  try {
    const output = join(root, "review");
    materializeReviewFiles(output, [
      ["src/main/electron.js", "verified source"],
      ["REVIEW-MANIFEST.UNAPPROVED.json", '{"status":"UNAPPROVED"}\n'],
    ]);
    assert.equal(
      readFileSync(join(output, "src/main/electron.js"), "utf8"),
      "verified source",
    );
    assert.equal(
      readFileSync(join(output, "REVIEW-MANIFEST.UNAPPROVED.json"), "utf8"),
      '{"status":"UNAPPROVED"}\n',
    );
    assert.throws(
      () => materializeReviewFiles(output, []),
      /existing output|EEXIST/,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test(
  "checked-in proposal patch applied to pinned pristine main yields exact reviewed bytes",
  { skip: !root },
  () => {
    const scratch = mkdtempSync(
      join(tmpdir(), "gh1404-patch-versus-candidate-"),
    );
    try {
      const main = join(scratch, "src/main/electron.js");
      mkdirSync(join(scratch, "src/main"), { recursive: true });
      const pristine = readFileSync(join(root, "src/main/electron.js"));
      assertPinned("src/main/electron.js", pristine);
      writeFileSync(main, pristine);
      const patch = readFileSync(
        new URL("./electron-argv.UNAPPROVED.patch", import.meta.url),
      );
      const applied = spawnSync(
        "git",
        ["apply", "--unidiff-zero", "--whitespace=error", "-"],
        {
          cwd: scratch,
          input: patch,
          encoding: "utf8",
        },
      );
      assert.equal(applied.status, 0, applied.stderr);
      assert.equal(
        sha256(readFileSync(main)),
        "cc240f18df8eb102c4fc3c0d75f4f6da21129a8d32835b43fe260b6a3bfbc0de",
        "patch and reviewed producer must emit the same immutable source",
      );
      // An isolated one-token patch mutation must fail the independent digest.
      writeFileSync(main, pristine);
      const corrupt = Buffer.from(
        patch
          .toString("utf8")
          .replace("runtimeProcessFactsSelected", "runtimeProcessFactsAltered"),
      );
      const mutated = spawnSync(
        "git",
        ["apply", "--unidiff-zero", "--whitespace=error", "-"],
        {
          cwd: scratch,
          input: corrupt,
          encoding: "utf8",
        },
      );
      assert.equal(mutated.status, 0, mutated.stderr);
      assert.notEqual(
        sha256(readFileSync(main)),
        "cc240f18df8eb102c4fc3c0d75f4f6da21129a8d32835b43fe260b6a3bfbc0de",
        "mutation must break source approval hash",
      );
    } finally {
      rmSync(scratch, { recursive: true, force: true });
    }
  },
);

test(
  "unreviewed adapter bytes are refused before source materialization",
  { skip: !(root && adapter) },
  async () => {
    const scratch = mkdtempSync(join(tmpdir(), "gh1404-adapter-tamper-"));
    try {
      const modified = join(scratch, "modified-adapter.mjs");
      writeFileSync(
        modified,
        Buffer.concat([
          readFileSync(adapter),
          Buffer.from("\\n// independent tamper negative\\n"),
        ]),
      );
      const destination = join(scratch, "unapproved");
      await assert.rejects(
        () => produceReview(root, modified, destination),
        /exact #1506 adapter pin mismatch/,
      );
      assert.equal(existsSync(destination), false);
    } finally {
      rmSync(scratch, { recursive: true, force: true });
    }
  },
);

test("failed owned producer preserves an independently inserted foreign file", () => {
  const scratch = mkdtempSync(join(tmpdir(), "gh1404-source-review-foreign-"));
  try {
    const destination = join(scratch, "owned-review");
    const late = ["late.js"];
    Object.defineProperty(late, 1, {
      get() {
        // An input getter can create a competing output directory before
        // our atomic claim; its foreign bytes must never be removed.
        mkdirSync(destination);
        writeFileSync(join(destination, "foreign.txt"), "not ours");
        throw new Error("controlled late source failure");
      },
    });
    assert.throws(
      () =>
        materializeReviewFiles(destination, [
          ["src/main/owned.js", "ours"],
          late,
        ]),
      /controlled late source failure/,
    );
    assert.equal(
      readFileSync(join(destination, "foreign.txt"), "utf8"),
      "not ours",
    );
    assert.equal(existsSync(join(destination, "src/main/owned.js")), false);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});

test("late competing creation is refused at the atomic output claim", () => {
  const scratch = mkdtempSync(join(tmpdir(), "gh1404-atomic-new-dir-"));
  try {
    const output = join(scratch, "claimed");
    assert.equal(existsSync(output), false, "earlier absence is not a lock");
    mkdirSync(output); // A competing producer creates it after preflight.
    writeFileSync(join(output, "foreign.txt"), "untouched");
    assert.throws(() => claimNewReviewDirectory(output), /EEXIST/);
    assert.equal(
      readFileSync(join(output, "foreign.txt"), "utf8"),
      "untouched",
    );
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});

test("one-time entry-name snapshot cannot escape the claimed output after validation", () => {
  const scratch = mkdtempSync(
    join(tmpdir(), "gh1404-source-review-name-snapshot-"),
  );
  try {
    const output = join(scratch, "review");
    const entry = ["safe.txt", "reviewed bytes"];
    let reads = 0;
    Object.defineProperty(entry, 0, {
      get() {
        reads += 1;
        return reads === 1 ? "safe.txt" : "../outside.txt";
      },
    });
    materializeReviewFiles(output, [entry]);
    assert.equal(
      reads,
      1,
      "name must be captured once before output is created",
    );
    assert.equal(
      readFileSync(join(output, "safe.txt"), "utf8"),
      "reviewed bytes",
    );
    assert.equal(existsSync(join(scratch, "outside.txt")), false);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});

test("side-effectful bytes getter cannot install nested symlink after output claim", () => {
  const scratch = mkdtempSync(join(tmpdir(), "gh1404-source-review-symlink-"));
  try {
    const outside = join(scratch, "foreign");
    const output = join(scratch, "review");
    mkdirSync(outside);
    const entry = ["nested/output.js", "source"];
    Object.defineProperty(entry, 1, {
      get() {
        // Simulate an untrusted caller attempting to install an alias at the
        // next output directory after the destination claim.
        mkdirSync(output, { recursive: true });
        symlinkSync(outside, join(output, "nested"));
        return "attacker bytes";
      },
    });
    assert.throws(
      () => materializeReviewFiles(output, [entry]),
      /EEXIST|existing output directory/,
    );
    assert.equal(existsSync(join(outside, "output.js")), false);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});

test(
  "selected second-instance source grants no internal argv file",
  {
    skip: !root,
  },
  () => {
    const scratch = mkdtempSync(join(tmpdir(), "gh1404-selected-second-"));
    try {
      const relative = "src/main/electron.js";
      mkdirSync(join(scratch, "src/main"), { recursive: true });
      writeFileSync(
        join(scratch, relative),
        readFileSync(join(root, relative)),
      );
      const patch = readFileSync(
        new URL("./electron-argv.UNAPPROVED.patch", import.meta.url),
      );
      const applied = spawnSync(
        "git",
        ["apply", "--unidiff-zero", "--whitespace=error", "-"],
        { cwd: scratch, input: patch, encoding: "utf8" },
      );
      assert.equal(applied.status, 0, applied.stderr);
      const main = readFileSync(join(scratch, relative), "utf8");
      function simulateSecondInstance(source, selected) {
        const start = source.indexOf("app.on('second-instance',");
        assert(start >= 0);
        const handler = source
          .slice(start)
          .match(
            /^app\.on\('second-instance',[\s\S]*?^[ \t]*\}\)[ \t]*$/m,
          )?.[0];
        assert(handler, "real original second-instance callback is present");
        const appEvents = new Map();
        const windowEvents = new Map();
        const ipcEvents = new Map();
        const grants = [];
        const sends = [];
        const win = {
          webContents: {
            on(name, callback) {
              windowEvents.set(name, callback);
            },
            send(name, body) {
              sends.push([name, body]);
            },
            setVisualZoomLevelLimits() {},
          },
        };
        runInNewContext(handler, {
          app: {
            on(name, callback) {
              appEvents.set(name, callback);
            },
          },
          ipcMain: {
            once(name, callback) {
              ipcEvents.set(name, callback);
            },
          },
          createWindow() {
            return win;
          },
          validateSender() {
            return true;
          },
          statSafe() {
            return {
              isFile() {
                return true;
              },
            };
          },
          blessPath(path) {
            grants.push(path);
          },
          runtimeProcessFactsSelected: selected,
          appZoom: 1,
          dialogOpen: false,
        });
        appEvents.get("second-instance")(
          {},
          ["bun", "/sealed/original.js"],
          "/",
        );
        windowEvents.get("did-finish-load")();
        ipcEvents.get("app-load-finished")({ senderFrame: {} });
        return { grants, sends };
      }
      const selected = simulateSecondInstance(main, true);
      assert.deepEqual(selected.grants, []);
      assert.equal(selected.sends.length, 0);
      const ordinary = simulateSecondInstance(main, false);
      assert.deepEqual(ordinary.grants, ["/sealed/original.js"]);
      assert.equal(ordinary.sends.length, 1);
      const unguarded = main.replace(
        "if (loadEvtCount == 2 && runtimeProcessFactsSelected === false)",
        "if (loadEvtCount == 2)",
      );
      assert.notEqual(
        unguarded,
        main,
        "negative mutation must change exact source",
      );
      assert.deepEqual(
        simulateSecondInstance(unguarded, true).grants,
        ["/sealed/original.js"],
        "removing selected guard must restore the unsafe grant",
      );
    } finally {
      rmSync(scratch, { recursive: true, force: true });
    }
  },
);

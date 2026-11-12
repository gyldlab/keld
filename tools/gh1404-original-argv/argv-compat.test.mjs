import { createHash } from "node:crypto";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { strict as assert } from "node:assert";
import { test } from "node:test";
import {
  originalParserArgv,
  patchOriginalMainText,
} from "./patch-original-main.mjs";

const bun = "/sealed/Helpers/bun";
const bootstrap = "/sealed/Resources/app/keld-installed-runtime-entry.mjs";
const original = "/sealed/Resources/app/src/main/electron.js";
const diagram = "/user/actual-selected.drawio";
// Reuse the schema-7 source fixture from the KIPC owner's own test corpus.
// Parsing this fixture is test data, NOT an authentication mechanism.
const selectedFacts = Object.freeze(
  JSON.parse(
    readFileSync(
      new URL(
        "../../packages/@keld/kipc/test/fixtures/installed-launch-facts-v7.json",
        import.meta.url,
      ),
      "utf8",
    ),
  ),
);

function parsedPositions(argv) {
  return argv.slice(2).filter((part) => part != null);
}

for (const defaultApp of [true, false, undefined]) {
  test(`runtime-selected original refuses missing or mismatched schema-7 facts before parsing, defaultApp=${String(defaultApp)}`, () => {
    const mismatches = [
      undefined,
      null,
      { schema: 6, kind: "local-development" },
      { schema: 7, kind: "development" },
      { schema: 7, kind: "local-development" },
      {
        schema: 7,
        kind: "local-development",
        originalResourcePolicy: { contract: "unqualified" },
      },
    ];
    for (const hostFacts of mismatches) {
      for (const extra of [[], [diagram]]) {
        const raw = Object.freeze([bun, bootstrap, original, ...extra]);
        assert.throws(
          () => originalParserArgv(raw, defaultApp, true, hostFacts),
          /KELD-IPC-005/,
          `selected role must refuse ${JSON.stringify(hostFacts)} with extra=${extra.length}`,
        );
        assert.deepEqual(raw, [bun, bootstrap, original, ...extra]);
      }
    }
  });

  test(`matching installed facts without runtime selection refuse downgrade, defaultApp=${String(defaultApp)}`, () => {
    assert.throws(
      () =>
        originalParserArgv(
          Object.freeze([bun, bootstrap, original]),
          defaultApp,
          false,
          selectedFacts,
        ),
      /KELD-IPC-005/,
    );
  });

  test(`unknown runtime selection never becomes a direct-entry fallback, defaultApp=${String(defaultApp)}`, () => {
    for (const selection of [undefined, null, 0, 1, "true"]) {
      assert.throws(
        () =>
          originalParserArgv(
            Object.freeze([bun, bootstrap, original]),
            defaultApp,
            selection,
            selectedFacts,
          ),
        /KELD-IPC-005/,
      );
    }
  });
}

for (const defaultApp of [true, false, undefined]) {
  for (const user of [[], [diagram]]) {
    test(`signed-installed parser refuses unadmitted CLI operands; defaultApp=${String(defaultApp)} extra=${user.length}`, () => {
      const authentic = Object.freeze([bun, bootstrap, original, ...user]);
      if (user.length) {
        assert.throws(
          () => originalParserArgv(authentic, defaultApp, true, selectedFacts),
          /KELD-IPC-005/,
        );
        assert.deepEqual(authentic, [bun, bootstrap, original, ...user]);
        return;
      }
      const effective = originalParserArgv(
        authentic,
        defaultApp,
        true,
        selectedFacts,
      );
      assert.deepEqual(parsedPositions(effective), []);
      assert.deepEqual(authentic, [bun, bootstrap, original, ...user]);
      assert.equal(effective.includes(bootstrap), false);
      assert.equal(effective.includes(original), false);
      assert.equal(effective[1], null);
      assert.notEqual(effective, authentic);
    });
  }
}

test("installed parser refuses absent original identity instead of treating argv as a grant", () => {
  for (const bad of [
    [bun, bootstrap],
    [bun, bootstrap, null],
    [bun, bootstrap, { malicious: true }],
  ])
    assert.throws(
      () => originalParserArgv(bad, false, true, selectedFacts),
      /KELD-IPC-005/,
    );
});

test("installed bootstrap refuses appended argv even if it names an existing user diagram", () => {
  // The signed Host currently authenticates only argc=3. An extra path is NOT
  // a trustworthy open-file event; never bless it by parsing mutable argv.
  for (const mode of [true, false, undefined]) {
    assert.throws(
      () =>
        originalParserArgv(
          [bun, bootstrap, original, diagram],
          mode,
          true,
          selectedFacts,
        ),
      /KELD-IPC-005/,
    );
  }
});

test("unselected direct original retains upstream parser positions without mutating argv", () => {
  for (const defaultApp of [true, false, undefined]) {
    const source = Object.freeze([bun, original, diagram]);
    const before =
      defaultApp === true
        ? [bun, original, diagram]
        : [null, bun, original, diagram];
    assert.deepEqual(
      originalParserArgv(source, defaultApp, false, undefined),
      before,
    );
    assert.deepEqual(source, [bun, original, diagram]);
  }
});

const sourceSample =
  "import { parseDrawioArgs, formatHelp, validFormatRegExp as validFormatRegExpImport } from './args.js';\n" +
  "let store;\n" +
  "    let argv = process.argv\n\n" +
  "    // https://github.com/electron/electron/issues/4690#issuecomment-217435222\n" +
  "    if (process.defaultApp != true)\n" +
  "    {\n" +
  "        argv.unshift(null)\n" +
  "    }\n\n" +
  "\tvar validFormatRegExp = validFormatRegExpImport;\n";

test("patch only exact original parser seam and preserve runtime identities", () => {
  const patched = patchOriginalMainText(sourceSample);
  assert.match(patched, /hostLaunchFacts, runtimeProcessFactsSelected/);
  assert.match(patched, /originalParserArgv\(\s*process\.argv/);
  assert.match(
    patched,
    /process\.argv, process\.defaultApp, runtimeProcessFactsSelected, hostLaunchFacts/,
  );
  assert.match(patched, /hostFacts\?\.schema !== 7/);
  assert.doesNotMatch(patched, /const sealedOriginalGraph/);
  assert.doesNotMatch(patched, /argv\.unshift\(null\)/);
  assert.doesNotMatch(patched, /process\.argv\s*=/);
  assert.doesNotMatch(patched, /Bun\.main\s*=/);
  assert.doesNotMatch(patched, /import\.meta\.main\s*=/);
  assert.throws(() => patchOriginalMainText(patched), /original parser anchor/);
  assert.throws(
    () =>
      patchOriginalMainText(
        sourceSample.replace("let argv = process.argv", "let argv = external"),
      ),
    /original parser anchor/,
  );
});

test(
  "pristine upstream source can be patched only when supplied as an explicit pinned test input",
  {
    skip: !process.env.GH1404_PINNED_ORIGINAL_MAIN,
  },
  () => {
    const path = process.env.GH1404_PINNED_ORIGINAL_MAIN;
    const source = readFileSync(path, "utf8");
    assert.equal(
      createHash("sha256").update(source).digest("hex"),
      "55505e690d37070344be172a68ad997ab48c687302dc5dee30a988d4f153770d",
    );
    const patched = patchOriginalMainText(source);
    assert(patched.includes("function originalParserArgv("));
    assert(patched.includes("let argv = originalParserArgv("));
    assert.equal(patched.includes("argv.unshift(null)"), false);
    assert(patched.length > source.length);
  },
);

test(
  "exact upstream args.js can serve as an independent parser oracle when pinned input is supplied",
  {
    skip: !process.env.GH1404_PINNED_ORIGINAL_ARGS,
  },
  async () => {
    const path = process.env.GH1404_PINNED_ORIGINAL_ARGS;
    const source = readFileSync(path);
    assert.equal(
      createHash("sha1")
        .update(Buffer.from(`blob ${source.byteLength}\0`))
        .update(source)
        .digest("hex"),
      "e5ab92e4468fc80d888f953945b60a24e20c8b06",
    );
    const { parseDrawioArgs } = await import(
      "data:text/javascript;base64," + source.toString("base64")
    );
    for (const defaultApp of [true, false, undefined]) {
      for (const user of [[], [diagram]]) {
        if (user.length) {
          assert.throws(
            () =>
              originalParserArgv(
                [bun, bootstrap, original, ...user],
                defaultApp,
                true,
                selectedFacts,
              ),
            /KELD-IPC-005/,
          );
          continue;
        }
        const mapped = originalParserArgv(
          [bun, bootstrap, original, ...user],
          defaultApp,
          true,
          selectedFacts,
        );
        assert.deepEqual(parseDrawioArgs(mapped).args, []);
        for (const badFacts of [
          undefined,
          { schema: 6, kind: "local-development" },
          { schema: 7, kind: "development" },
          { schema: 7, kind: "local-development" },
        ]) {
          let parserCalls = 0;
          assert.throws(() => {
            // Refusal must occur *before* the actual pinned draw.io parser.
            const permitted = originalParserArgv(
              [bun, bootstrap, original, ...user],
              defaultApp,
              true,
              badFacts,
            );
            parserCalls += 1;
            return parseDrawioArgs(permitted);
          }, /KELD-IPC-005/);
          assert.equal(
            parserCalls,
            0,
            "invalid selected facts cannot reach args.js",
          );
        }
      }
    }
  },
);

test(
  "real pinned parser exposes no internal paths to original args-obj or conditional grant eligibility",
  {
    skip:
      !process.env.GH1404_PINNED_ORIGINAL_ARGS ||
      !process.env.GH1404_PINNED_ORIGINAL_MAIN,
  },
  async () => {
    const parserPath = process.env.GH1404_PINNED_ORIGINAL_ARGS;
    const originalPath = process.env.GH1404_PINNED_ORIGINAL_MAIN;

    const originalSource = readFileSync(originalPath, "utf8");
    assert.match(originalSource, /if \(fs\.existsSync\(a\)\)/);
    assert.match(originalSource, /blessPath\(a\)/);
    assert.match(originalSource, /webContents\.send\('args-obj'/);
    assert.match(originalSource, /app\.on\("open-file"/);
    const bytes = readFileSync(parserPath);
    const { parseDrawioArgs } = await import(
      "data:text/javascript;base64," + bytes.toString("base64")
    );
    const scratch = mkdtempSync(join(tmpdir(), "keld-gh1404-w12-argv-"));
    try {
      const roleBootstrap = join(scratch, "keld-installed-runtime-entry.mjs");
      const roleOriginal = join(scratch, "electron.js");
      const realDiagram = join(scratch, "chosen.drawio");
      for (const file of [roleBootstrap, roleOriginal, realDiagram]) {
        writeFileSync(file, "source-only fixture\n");
      }
      for (const mode of [true, false, undefined]) {
        for (const selected of [[], [realDiagram]]) {
          const actual = [bun, roleBootstrap, roleOriginal, ...selected];
          if (selected.length) {
            // A real selected .drawio still needs its independent OS open-file
            // authorization; the installed Host cannot admit it via CLI argv.
            assert.throws(
              () => originalParserArgv(actual, mode, true, selectedFacts),
              /KELD-IPC-005/,
            );
            continue;
          }
          const fixed = parseDrawioArgs(
            originalParserArgv(actual, mode, true, selectedFacts),
          ).args;
          const conditionalBlessInputs = fixed.filter((path) =>
            existsSync(path),
          );
          assert.deepEqual(
            fixed,
            [],
            "args-obj must not receive an internal source",
          );
          assert.deepEqual(conditionalBlessInputs, []);
          assert(!fixed.includes(roleOriginal));
          assert(!fixed.includes(roleBootstrap));
          // Deleting the adaptation restores the concrete original.js false grant.
          const old = parseDrawioArgs(actual).args;
          assert(old.includes(roleOriginal));
          assert(old.filter((path) => existsSync(path)).includes(roleOriginal));
        }
      }
    } finally {
      rmSync(scratch, { recursive: true, force: true });
    }
  },
);

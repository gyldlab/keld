import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  cpSync,
  existsSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
const packageJson = JSON.parse(
  readFileSync(join(root, "package.json"), "utf8"),
);
const lock = JSON.parse(readFileSync(join(root, "package-lock.json"), "utf8"));
const build = readFileSync(join(root, "keld.build.ts"), "utf8");

// This is an immutable source candidate, not a second TypeScript authoring
// parser. Bind the inspected config and executable main bytes before using
// the descriptive checks below. Regex matches alone cannot distinguish an
// effective property/statement from text in a string. Intentional source
// changes require independent inspection and a reviewed identity update;
// never derive these expected values from the files under test.
test("the generic app source retains all inspected candidate identities", () => {
  assert.equal(
    sha256("keld.build.ts"),
    "8c344e570b46473e1b7fab2203af03a6cc88b919f20f4d24362bfc2cf7573638",
    "build config differs from inspected effective field selection",
  );
  assert.equal(
    sha256("src/main.mjs"),
    "9bc44dfa873b4d49bec979414da45edfc4e1b5912cfaf24604c01d11dbd897b2",
    "main differs from inspected executable module",
  );
  assert.equal(
    sha256("keld.config.ts"),
    "5ad4f4072112f6a7c27d9f21e234048e835e7b68ed2fc17b81d66a28ea5488ed",
    "app config differs from inspected source selection",
  );
  assert.equal(
    sha256("keld.permissions.jsonc"),
    "5b2bde7049d9258ad2ea78720f481cb986f113e8c8db56f5f5805bc49abf5223",
    "permission manifest differs from inspected window grants",
  );
  assert.equal(
    sha256("web/pages/index.html"),
    "e17fd49bf4786a70beeb5e167621fdd2b279508b241e35109ecd93096efb8a46",
    "renderer HTML differs from inspected offline document",
  );
});

test(
  "source verifier rejects active-field decoys and inert entry",
  {
    skip: process.env.KELD_B3_MUTANT === "1",
  },
  () => {
    const mutations = [
      [
        "host digest decoy",
        "keld.build.ts",
        (text) =>
          text
            .replace('"UNQUALIFIED_HOST_SHA256"', "'" + "a".repeat(64) + "'")
            .replace(
              'version: "unqualified"',
              "version: 'unqualified sha256: \"UNQUALIFIED_HOST_SHA256\"'",
            ),
      ],
      ...[
        ["packageSha256", "package.json"],
        ["lockSha256", "package-lock.json"],
      ].map(([field, file]) => [
        field + " decoy",
        "keld.build.ts",
        (text) =>
          text
            .replace(
              new RegExp(field + ':\\s*"[^"]+"'),
              field + ": '" + "0".repeat(64) + "'",
            )
            .replace(
              'version: "unqualified"',
              "version: 'unqualified " + field + ': "' + sha256(file) + "\"'",
            ),
      ]),
      [
        "preload path",
        "keld.build.ts",
        (text) =>
          text.replace(
            'path: "src/preload.mjs"',
            'path: "web/pages/index.html"',
          ),
      ],
      [
        "inert main",
        "src/main.mjs",
        (text) => "const inert = `\n" + text + "\n`;\n",
      ],
      [
        "unreviewed app ID",
        "keld.config.ts",
        (text) =>
          text.replace(
            'id: "dev.keld.gh1404.minimal"',
            'id: "dev.keld.unreviewed.app"',
          ),
      ],
      [
        "inline renderer script",
        "web/pages/index.html",
        (text) =>
          text.replace(
            "  </body>",
            "    <script>window.unreviewed = true;</script>\n  </body>",
          ),
      ],
      [
        "extra permission channel",
        "keld.permissions.jsonc",
        (text) =>
          text.replace(
            '"main": {\n      "web": {',
            '"main": {\n      "channels": ["el:unreviewed-topic"],\n      "web": {',
          ),
      ],
    ];
    const outcomes = [];
    for (const [name, file, mutate] of mutations) {
      const scratch = mkdtempSync(join(tmpdir(), "keld-b3-oracle-"));
      try {
        cpSync(root, scratch, { recursive: true });
        const target = join(scratch, file);
        const before = readFileSync(target, "utf8");
        const after = mutate(before);
        assert.notEqual(after, before, name + " mutation must apply");
        const env = { ...process.env, KELD_B3_MUTANT: "1" };
        delete env.KELD_B3_PACK_RLIB;
        delete env.KELD_B3_CLI;
        delete env.NODE_TEST_CONTEXT;
        const intact = spawnSync(
          process.env.KELD_B3_NODE_BIN || "node",
          ["--test", "verify.test.mjs"],
          { cwd: scratch, env, encoding: "utf8", timeout: 20_000 },
        );
        assert.equal(intact.error, undefined, name + ": intact child must run");
        assert.equal(
          intact.status,
          0,
          name + ": intact copied source must pass",
        );
        writeFileSync(target, after);
        const result = spawnSync(
          process.env.KELD_B3_NODE_BIN || "node",
          ["--test", "verify.test.mjs"],
          {
            cwd: scratch,
            env,
            encoding: "utf8",
            timeout: 20_000,
          },
        );
        assert.equal(result.error, undefined, name + ": child must run");
        const identityError = {
          "keld.build.ts":
            /build config differs from inspected effective field selection/,
          "src/main.mjs": /main differs from inspected executable module/,
          "keld.config.ts": /app config differs from inspected source selection/,
          "keld.permissions.jsonc":
            /permission manifest differs from inspected window grants/,
          "web/pages/index.html":
            /renderer HTML differs from inspected offline document/,
        }[file];
        assert.match(
          result.stdout + result.stderr,
          identityError,
          name +
            ": refusal must come from source identity, not an unrelated failure",
        );
        outcomes.push([name, result.status]);
      } finally {
        rmSync(scratch, { recursive: true, force: true });
      }
    }
    assert.deepEqual(
      outcomes,
      mutations.map(([name]) => [name, 1]),
    );
  },
);

// This verifier never evaluates authoring TypeScript or imports unapproved
// application modules. Ignore comments while retaining quoted literals so
// obsolete configuration examples cannot masquerade as the effective source.
function withoutComments(source) {
  let result = "";
  let quote = "";
  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    const next = source[index + 1];
    if (quote) {
      result += char;
      if (char === "\\") {
        result += next ?? "";
        index += 1;
      } else if (char === quote) {
        quote = "";
      }
    } else if (char === '"' || char === "'" || char === "`") {
      quote = char;
      result += char;
    } else if (char === "/" && next === "/") {
      index += 2;
      while (index < source.length && source[index] !== "\n") index += 1;
      result += "\n";
    } else if (char === "/" && next === "*") {
      index += 2;
      while (
        index + 1 < source.length &&
        !(source[index] === "*" && source[index + 1] === "/")
      ) {
        if (source[index] === "\n") result += "\n";
        index += 1;
      }
      index += 1;
    } else {
      result += char;
    }
  }
  assert.equal(quote, "", "unterminated source literal");
  return result;
}

const activeBuild = withoutComments(build);

function literalObject(source, name) {
  const matches = [...source.matchAll(new RegExp(`\\b${name}:\\s*\\{`, "g"))];
  assert.equal(matches.length, 1, `one active ${name} object required`);
  const start = matches[0].index + matches[0][0].length;
  let depth = 1;
  for (let index = start; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(start, index);
  }
  assert.fail(`unterminated ${name} object`);
}

function literalString(source, name) {
  const matches = [
    ...source.matchAll(new RegExp(`\\b${name}:\\s*"([^"\\n]*)"`, "g")),
  ];
  assert.equal(matches.length, 1, `one active ${name} literal required`);
  return matches[0][1];
}

function sha256(relative) {
  return createHash("sha256")
    .update(readFileSync(join(root, relative)))
    .digest("hex");
}

function declaredCaptureRoots(source = activeBuild) {
  // The production CLI accepts a closed literal resources array. Parse its
  // authored JSON string literals without importing or evaluating TypeScript.
  const matches = [...source.matchAll(/^\s*resources:\s*(\[[^\]]*\]),?$/gm)];
  assert.equal(
    matches.length,
    1,
    "one active closed Pack resources array required",
  );
  const raw = matches[0][1];
  const roots = JSON.parse(raw.replace(/,\s*]$/, "]"));
  assert.ok(Array.isArray(roots));
  assert.ok(
    roots.every((value) => typeof value === "string" && value.length > 0),
  );
  assert.equal(new Set(roots).size, roots.length, "no repeated resource roots");
  assert.deepEqual(roots, [
    "src",
    "web",
    "package.json",
    "package-lock.json",
    "node_modules",
  ]);
  return roots;
}

function coversResource(roots, path) {
  return roots.some((root) => path === root || path.startsWith(`${root}/`));
}

function expectedRootDependencies(metadata, locked) {
  const expected = {
    "@keld/api": "0.0.1",
    "@keld/kipc": "0.0.1",
    electron: "npm:@keld/electron@0.0.1",
  };
  // The protected-manifest decision request is for an inert, closed source
  // candidate. A matching digest is insufficient if an executable npm hook,
  // alternate dependency namespace, or extra installed lock row is added.
  assert.deepEqual(
    Object.keys(metadata).sort(),
    ["name", "version", "private", "type", "dependencies"].sort(),
    "root manifest must remain inert and closed",
  );
  assert.deepEqual(
    Object.keys(locked).sort(),
    ["name", "version", "lockfileVersion", "requires", "packages"].sort(),
    "candidate lock must remain closed",
  );
  assert.equal(locked.lockfileVersion, 3, "npm lock v3");
  assert.equal(locked.requires, true, "npm lock requires explicit metadata");
  assert.equal(locked.name, metadata.name);
  assert.equal(locked.version, metadata.version);
  assert.deepEqual(
    Object.keys(locked.packages).sort(),
    [
      "",
      "node_modules/electron",
      "node_modules/@keld/api",
      "node_modules/@keld/kipc",
    ].sort(),
    "candidate lock must have exactly three first-party installed rows",
  );
  assert.deepEqual(
    Object.keys(locked.packages[""]).sort(),
    ["name", "version", "dependencies"].sort(),
    "root lock entry must be closed",
  );
  assert.equal(locked.packages[""].name, metadata.name);
  assert.equal(locked.packages[""].version, metadata.version);
  assert.deepEqual(locked.packages[""].dependencies, metadata.dependencies);
  assert.deepEqual(
    metadata.dependencies,
    expected,
    "exact first-party dependency selectors",
  );
  for (const [path, name] of [
    ["node_modules/electron", "@keld/electron"],
    ["node_modules/@keld/api", "@keld/api"],
    ["node_modules/@keld/kipc", "@keld/kipc"],
  ]) {
    assert.deepEqual(
      Object.keys(locked.packages[path]).sort(),
      ["name", "version"].sort(),
      `installed lock row ${path} must contain no executable or foreign metadata`,
    );
    assert.equal(locked.packages[path].name, name);
    assert.equal(locked.packages[path].version, "0.0.1");
    assert.equal(locked.packages[path].resolved, undefined);
    assert.equal(locked.packages[path].integrity, undefined);
  }
}

test("committed manifest and npm lock v3 have one exact first-party alias", () => {
  assert.equal(packageJson.private, true);
  assert.equal(packageJson.type, "module");
  expectedRootDependencies(packageJson, lock);
});

test("a changed locked alias is detected independently of the app source", () => {
  const mutated = structuredClone(lock);
  mutated.packages[""].dependencies.electron = "electron";
  assert.throws(
    () => expectedRootDependencies(packageJson, mutated),
    /strictEqual|deep-equal|Expected values/,
  );
});

test("a first-party SDK selector redirected in BOTH manifests is refused", () => {
  for (const name of ["@keld/api", "@keld/kipc"]) {
    const changedMetadata = structuredClone(packageJson);
    const changedLock = structuredClone(lock);
    changedMetadata.dependencies[name] = "npm:@outsider/redirect@0.0.1";
    changedLock.packages[""].dependencies[name] =
      "npm:@outsider/redirect@0.0.1";
    assert.throws(
      () => expectedRootDependencies(changedMetadata, changedLock),
      /deep-equal|Expected values|first-party/,
      `redirected ${name} must be refused even if lock agrees`,
    );
  }
});

test("unreviewed npm lifecycle hooks and extra locked providers are refused", () => {
  const hooked = structuredClone(packageJson);
  hooked.scripts = { postinstall: "echo UNQUALIFIED_HOOK_DO_NOT_RUN" };
  assert.throws(
    () => expectedRootDependencies(hooked, lock),
    /root manifest must remain inert and closed/,
  );

  const hookedProvider = structuredClone(lock);
  hookedProvider.packages["node_modules/@keld/api"].hasInstallScript = true;
  assert.throws(
    () => expectedRootDependencies(packageJson, hookedProvider),
    /installed lock row .* must contain no executable or foreign metadata/,
  );

  const extraProvider = structuredClone(lock);
  extraProvider.packages["node_modules/unreviewed"] = {
    name: "unreviewed",
    version: "0.0.1",
  };
  assert.throws(
    () => expectedRootDependencies(packageJson, extraProvider),
    /exactly three first-party installed rows/,
  );
});

test("package, lock and preload content bind to exact candidate SHA-256 bytes", () => {
  assert.equal(
    literalString(
      literalObject(literalObject(activeBuild, "graphSource"), "preload"),
      "path",
    ),
    "src/preload.mjs",
    "selected preload must match the main module and digest witness",
  );
  for (const [field, relative] of [
    ["packageSha256", "package.json"],
    ["lockSha256", "package-lock.json"],
  ]) {
    assert.match(
      activeBuild,
      new RegExp(`${field}:\\s*"${sha256(relative)}"`),
      `${field} must match the committed candidate file`,
    );
  }
  assert.match(
    activeBuild,
    new RegExp(`sha256:\\s*"${sha256("src/preload.mjs")}"`),
    "the separately named preload source must be captured",
  );
  assert.ok(existsSync(join(root, "src/main.mjs")));
  assert.ok(existsSync(join(root, "web/pages/index.html")));
  assert.equal(existsSync(join(root, "node_modules")), false);
});

test("commented capture-root decoys cannot satisfy effective resource selection", () => {
  const actual = activeBuild.replace(
    /resources:\s*\[[^\]]*\]/m,
    'resources: ["src", "web"]',
  );
  const commented = `/* resources: ["src", "web", "package.json", "package-lock.json", "node_modules"] */\n${actual}`;
  assert.throws(
    () => declaredCaptureRoots(withoutComments(commented)),
    /deep-equal|Expected values/,
  );
});

test("Pack's declared capture roots retain root graph inputs and future provider install", () => {
  const roots = declaredCaptureRoots();
  // Pack Capture::plan_mode visits only input.resources plus entry/renderer,
  // then materializes that exact tree before Core's graph callback. Neither
  // entry nor renderer rescues these root-level package/provider inputs.
  for (const path of [
    "package.json",
    "package-lock.json",
    "node_modules/electron/package.json",
    "node_modules/@keld/api/package.json",
    "node_modules/@keld/kipc/package.json",
  ]) {
    assert.ok(coversResource(roots, path), `Pack would omit ${path}`);
  }
  assert.equal(existsSync(join(root, "node_modules")), false);
  assert.ok(
    !coversResource(
      roots.filter((part) => part !== "package-lock.json"),
      "package-lock.json",
    ),
    "missing-lock negative control must not be rescued by another root",
  );
});

test(
  "real Pack graph transaction copies root inputs before intentional pre-sign refusal",
  {
    skip:
      (!process.env.KELD_B3_PACK_RLIB || process.platform !== "darwin") &&
      "set KELD_B3_PACK_RLIB to the verified GH1404 keld-pack macOS .rlib",
  },
  () => {
    const scratch = mkdtempSync(join(tmpdir(), "keld-gh1404-pack-capture-"));
    try {
      const copied = join(scratch, "candidate");
      cpSync(root, copied, { recursive: true });
      // No fake provider: an EMPTY root models the future installation path
      // and tests Pack's declared directory topology before graph admission.
      mkdirSync(join(copied, "node_modules"));
      const library = resolve(process.env.KELD_B3_PACK_RLIB);
      assert.ok(existsSync(library), `Pack library unavailable: ${library}`);
      const probe = join(scratch, "capture-probe");
      const compiled = spawnSync(
        "rustc",
        [
          "--edition=2024",
          "--crate-name=gh1404_b3_capture_probe",
          join(root, "capture-contract-probe.rs"),
          "--extern",
          `keld_pack=${library}`,
          "-L",
          `dependency=${dirname(library)}`,
          "-o",
          probe,
        ],
        { encoding: "utf8", timeout: 60_000 },
      );
      assert.equal(
        compiled.status,
        0,
        compiled.stderr || compiled.error?.message,
      );
      const captured = spawnSync(probe, [copied, ...declaredCaptureRoots()], {
        encoding: "utf8",
        timeout: 30_000,
      });
      assert.equal(
        captured.status,
        0,
        `real Pack capture rejected: ${captured.stderr || captured.error?.message}`,
      );
      assert.match(
        captured.stdout,
        /GH1404_B3_PACK_CAPTURE_PASS_REAL_TRANSACTION/,
      );
      assert.equal(existsSync(join(copied, "dist")), false);
    } finally {
      rmSync(scratch, { recursive: true, force: true });
    }
  },
);

test("every pending executable/provider pin is unambiguously invalid", () => {
  const providers = literalObject(
    literalObject(activeBuild, "graphSource"),
    "providerTrees",
  );
  for (const [actual, expected] of [
    [
      literalString(literalObject(activeBuild, "host"), "sha256"),
      "UNQUALIFIED_HOST_SHA256",
    ],
    [
      literalString(literalObject(activeBuild, "bun"), "sha256"),
      "UNQUALIFIED_BUN_SHA256",
    ],
    [literalString(providers, "electron"), "UNQUALIFIED_PROVIDER_ELECTRON"],
    [literalString(providers, "api"), "UNQUALIFIED_PROVIDER_API"],
    [literalString(providers, "kipc"), "UNQUALIFIED_PROVIDER_KIPC"],
  ]) {
    assert.equal(actual, expected, `effective ${expected} pin was changed`);
    assert.equal(/^[0-9a-f]{64}$/.test(actual), false);
  }
  assert.equal(existsSync(join(root, "dist")), false);
});

test("comment-only stale sentinel text cannot hide an active valid digest", () => {
  const changed = activeBuild.replace(
    '"UNQUALIFIED_HOST_SHA256"',
    `"${"a".repeat(64)}"`,
  );
  const commented = `${changed}\n// "UNQUALIFIED_HOST_SHA256"`;
  assert.notEqual(
    literalString(literalObject(withoutComments(commented), "host"), "sha256"),
    "UNQUALIFIED_HOST_SHA256",
  );
});

test("the installed graph has exactly one trusted offline window policy", () => {
  const policy = JSON.parse(
    readFileSync(join(root, "keld.permissions.jsonc"), "utf8"),
  );
  assert.equal(policy.profile, "legacy");
  assert.deepEqual(Object.keys(policy.windows ?? {}), ["main"]);
  assert.equal(policy.windows.main.web.csp, "original-editor-offline");
  assert.equal(policy.app, undefined, "the fixture declares no ambient grants");
});

test("the selected generic renderer is nested and matches the real app source", () => {
  const config = withoutComments(
    readFileSync(join(root, "keld.config.ts"), "utf8"),
  );
  const selection = literalObject(config, "rendererSource");
  assert.equal(literalString(selection, "root"), "web");
  assert.equal(literalString(selection, "entry"), "pages/index.html");
  assert.ok(existsSync(join(root, "web/pages/index.html")));
});

test("the real application entry parses without evaluating an unqualified provider", () => {
  const checked = spawnSync(
    process.env.KELD_B3_NODE_BIN || "node",
    ["--check", join(root, "src/main.mjs")],
    {
      encoding: "utf8",
      timeout: 5_000,
    },
  );
  assert.equal(checked.status, 0, checked.stderr || checked.error?.message);
  const main = withoutComments(
    readFileSync(join(root, "src/main.mjs"), "utf8"),
  );
  assert.match(main, /^import \{ app, BrowserWindow \} from "electron";/m);
  assert.match(main, /^await app\.whenReady\(\);$/m);
  assert.match(main, /^const root = app\.getAppPath\(\);$/m);
  assert.match(main, /^const window = new BrowserWindow\(\{/m);
  assert.match(main, /await window\.loadURL\(pathToFileURL\(/);
  assert.match(main, /preload: join\(root, "src\/preload\.mjs"\)/);
  assert.match(main, /join\(root, "web\/pages\/index\.html"\)/);
  assert.doesNotMatch(main, /loadFile\(/);
  assert.doesNotThrow(
    () => new Function(readFileSync(join(root, "src/preload.mjs"), "utf8")),
  );
});

test(
  "GH-1404 shipping CLI refuses missing provider approval before publishing",
  {
    skip:
      !process.env.KELD_B3_CLI &&
      "set KELD_B3_CLI to a GH-1404 keld executable",
  },
  () => {
    const workspace = mkdtempSync(join(tmpdir(), "keld-gh1404-b3-"));
    try {
      const copied = join(workspace, "app");
      cpSync(root, copied, { recursive: true });
      const authoring = join(copied, "keld.build.ts");
      const original = readFileSync(authoring, "utf8");
      // Test-only syntactically valid Host/Bun digests move the real CLI
      // parser to its independent providerTrees gate. Neither binary exists.
      const repaired = original
        .replace('"UNQUALIFIED_HOST_SHA256"', `"${"a".repeat(64)}"`)
        .replace('"UNQUALIFIED_BUN_SHA256"', `"${"b".repeat(64)}"`);
      assert.notEqual(repaired, original);
      writeFileSync(authoring, repaired, "utf8");
      const outcome = spawnSync(
        resolve(process.env.KELD_B3_CLI),
        ["build", "--target", "macos-arm64"],
        { cwd: copied, encoding: "utf8", timeout: 20_000 },
      );
      assert.equal(outcome.status, 1, outcome.stderr || outcome.error?.message);
      assert.match(outcome.stderr, /KELD-CLI-052/);
      assert.match(outcome.stderr, /keld\.build\.ts graphSource/);
      assert.match(outcome.stderr, /electron.*64 lowercase hexadecimal/);
      assert.equal(existsSync(join(copied, "dist")), false);
      assert.equal(existsSync(join(copied, ".keld/dev")), false);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  },
);

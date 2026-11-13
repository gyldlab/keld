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

function sha256(relative) {
  return createHash("sha256")
    .update(readFileSync(join(root, relative)))
    .digest("hex");
}

function declaredCaptureRoots() {
  // The production CLI accepts a closed literal resources array. Parse its
  // authored JSON string literals without importing or evaluating TypeScript.
  const raw = build.match(/^\s*resources:\s*(\[[^\]]*\]),?$/m)?.[1];
  assert.ok(raw, "closed literal Pack resources array is missing");
  const roots = JSON.parse(raw.replace(/,\s*]$/, "]"));
  assert.ok(Array.isArray(roots));
  assert.ok(
    roots.every((value) => typeof value === "string" && value.length > 0),
  );
  assert.equal(new Set(roots).size, roots.length, "no repeated resource roots");
  return roots;
}

function coversResource(roots, path) {
  return roots.some((root) => path === root || path.startsWith(`${root}/`));
}

function expectedRootDependencies(metadata, locked) {
  assert.equal(locked.lockfileVersion, 3, "npm lock v3");
  assert.equal(locked.name, metadata.name);
  assert.equal(locked.version, metadata.version);
  assert.equal(locked.packages[""].name, metadata.name);
  assert.equal(locked.packages[""].version, metadata.version);
  assert.deepEqual(locked.packages[""].dependencies, metadata.dependencies);
  assert.equal(
    metadata.dependencies.electron,
    "npm:@keld/electron@0.0.1",
    "only the explicit first-party Electron alias is selected",
  );
  for (const [path, name] of [
    ["node_modules/electron", "@keld/electron"],
    ["node_modules/@keld/api", "@keld/api"],
    ["node_modules/@keld/kipc", "@keld/kipc"],
  ]) {
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

test("package, lock and preload content bind to exact candidate SHA-256 bytes", () => {
  for (const [field, relative] of [
    ["packageSha256", "package.json"],
    ["lockSha256", "package-lock.json"],
  ]) {
    assert.match(
      build,
      new RegExp(`${field}:\\s*"${sha256(relative)}"`),
      `${field} must match the committed candidate file`,
    );
  }
  assert.match(
    build,
    new RegExp(`sha256:\\s*"${sha256("src/preload.mjs")}"`),
    "the separately named preload source must be captured",
  );
  assert.ok(existsSync(join(root, "src/main.mjs")));
  assert.ok(existsSync(join(root, "web/index.html")));
  assert.equal(existsSync(join(root, "node_modules")), false);
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
  for (const name of [
    "UNQUALIFIED_HOST_SHA256",
    "UNQUALIFIED_BUN_SHA256",
    "UNQUALIFIED_PROVIDER_ELECTRON",
    "UNQUALIFIED_PROVIDER_API",
    "UNQUALIFIED_PROVIDER_KIPC",
  ]) {
    assert.ok(build.includes(`"${name}"`), `${name} was replaced`);
    assert.equal(/^[0-9a-f]{64}$/.test(name), false);
  }
  assert.equal(existsSync(join(root, "dist")), false);
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
  assert.match(
    readFileSync(join(root, "src/main.mjs"), "utf8"),
    /await app\.whenReady\(\)/,
  );
  const main = readFileSync(join(root, "src/main.mjs"), "utf8");
  assert.match(main, /window\.loadURL\(pathToFileURL\(/);
  assert.match(main, /preload: join\(root, "src\/preload\.mjs"\)/);
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

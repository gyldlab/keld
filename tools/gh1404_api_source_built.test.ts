/**
 * GH-1404 B2 Row6 source-only SDK artifact custody.
 *
 * The selected source, compiler and producer must emit the exact shipped
 * JavaScript, declarations and archive bytes/modes. This is NOT release
 * authority: the five-archive original app graph remains unqualified.
 */
import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import {
  chmodSync,
  cpSync,
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";

import receipt from "./fixtures/gh1404_api_source_built.receipt.json";

const repo = resolve(import.meta.dir, "..");
const api = join(repo, "packages/@keld/api");
const producer = join(repo, "packages/@keld/kipc/scripts/package-artifacts.ts");
const originalSource = join(api, "src");
const apiArchiveName = "keld-api-0.0.1.tgz";
const kipcArchiveName = "keld-kipc-0.0.1.tgz";
// Manifests read by the API/KIPC producer and the two project configs it
// selects (including their local extends targets). This is not a dependency census.
const buildMetadataPaths = ["api", "kipc"].flatMap((name) =>
  ["package.json", "tsconfig.build.json", "tsconfig.json"].map(
    (file) => `packages/@keld/${name}/${file}`,
  ),
);
const expectedFiles = new Map(
  receipt.api.files.map((file) => [file.path, file]),
);

function digest(data: Uint8Array, algorithm: "sha256" | "sha512"): string {
  return createHash(algorithm).update(data).digest("hex");
}

function sri(data: Uint8Array): string {
  return "sha512-" + createHash("sha512").update(data).digest("base64");
}

function gitSafeEnvironment() {
  const env = { ...process.env };
  for (const name of Object.keys(env)) {
    if (name.toUpperCase().startsWith("GIT_")) delete env[name];
  }
  env.GIT_CONFIG_NOSYSTEM = "1";
  env.GIT_CONFIG_GLOBAL = process.platform === "win32" ? "NUL" : "/dev/null";
  return env;
}

function gitArgv(args: string[]): string[] {
  return ["git", "-c", "core.fsmonitor=false", ...args];
}

function isExpectedGitRoot(
  expectedRoot: string,
  env: ReturnType<typeof gitSafeEnvironment>,
): boolean {
  const result = Bun.spawnSync(gitArgv(["rev-parse", "--show-toplevel"]), {
    cwd: expectedRoot,
    stdout: "pipe",
    stderr: "pipe",
    timeout: 10_000,
    env,
  });
  if (result.exitCode !== 0) return false;
  try {
    return (
      realpathSync(new TextDecoder().decode(result.stdout).trim()) ===
      realpathSync(expectedRoot)
    );
  } catch {
    return false;
  }
}

function run(argv: string[], cwd = repo, input?: Buffer): string {
  const env = gitSafeEnvironment();
  if (argv[0] === "git" && argv[1] !== "init") {
    expect(isExpectedGitRoot(cwd, env), `unexpected Git root for ${cwd}`).toBe(
      true,
    );
  }
  const command = argv[0] === "git" ? gitArgv(argv.slice(1)) : argv;
  const result = Bun.spawnSync(command, {
    cwd,
    stdout: "pipe",
    stderr: "pipe",
    timeout: 60_000,
    ...(input === undefined ? {} : { stdin: input }),
    env: {
      ...env,
      PATH: dirname(process.execPath) + delimiter + (env.PATH ?? ""),
    },
  });
  const stderr = new TextDecoder().decode(result.stderr);
  expect(result.exitCode, `${argv.join(" ")}: ${stderr}`).toBe(0);
  return new TextDecoder().decode(result.stdout).trim();
}

/**
 * A receipt is not authority merely because its digests match the current
 * checkout. The selected commit itself must be a real reachable ancestor,
 * and both source/producer trees must be those of that exact commit.
 *
 * This gate is intentionally limited to the authoritative repo checkout:
 * the independent fault fixture below uses its own unrelated temporary Git.
 */
function selectedCommitProblems(
  selectedCommit: string,
  gitRoot = repo,
): string[] {
  if (!/^[0-9a-f]{40}$/.test(selectedCommit)) {
    return ["selected-commit-format"];
  }
  const env = gitSafeEnvironment();
  if (!isExpectedGitRoot(gitRoot, env)) return ["git-root-mismatch"];
  function gitCheck(args: string[]): { code: number | null; text: string } {
    const result = Bun.spawnSync(gitArgv(args), {
      cwd: gitRoot,
      stdout: "pipe",
      stderr: "pipe",
      timeout: 10_000,
      env,
    });
    return {
      code: result.exitCode,
      text: new TextDecoder().decode(result.stdout).trim(),
    };
  }
  if (gitCheck(["cat-file", "-e", `${selectedCommit}^{commit}`]).code !== 0) {
    return ["selected-commit-missing"];
  }
  const problems: string[] = [];
  if (
    gitCheck(["merge-base", "--is-ancestor", selectedCommit, "HEAD"]).code !== 0
  ) {
    problems.push("selected-commit-not-ancestor");
  }
  for (const [treePath, expected, kind] of [
    ["packages/@keld/api/src", receipt.sourceGitTree, "selected-api-tree"],
    [
      "packages/@keld/kipc/scripts",
      receipt.producerGitTree,
      "selected-producer-tree",
    ],
  ] as const) {
    const actual = gitCheck(["rev-parse", `${selectedCommit}:${treePath}`]);
    if (actual.code !== 0 || actual.text !== expected) problems.push(kind);
  }
  return problems;
}

function emittedSourceFacts(root: string): string[] {
  const problems: string[] = [];
  const tree = run(["git", "rev-parse", "HEAD:packages/@keld/api/src"], root);
  if (tree !== receipt.sourceGitTree) problems.push("source-tree");
  const source = readFileSync(join(root, "packages/@keld/api/src/app.ts"));
  if (digest(source, "sha256") !== receipt.sourceAppSha256) {
    problems.push("source-app");
  }
  return problems;
}

function buildMetadataProblems(selectedCommit: string, root = repo): string[] {
  const problems: string[] = [];
  for (const path of buildMetadataPaths) {
    const selected = run(["git", "rev-parse", `${selectedCommit}:${path}`], root);
    const head = run(["git", "rev-parse", `HEAD:${path}`], root);
    if (head !== selected) problems.push("build-metadata-head:" + path);
    const source = join(root, path);
    if (
      !lstatSync(source).isFile() ||
      run(["git", "hash-object", "--stdin"], root, readFileSync(source)) !==
        selected
    ) {
      problems.push("build-metadata-worktree:" + path);
    }
  }
  return problems;
}

function inspectFiles(extractedRoot: string): string[] {
  const problems: string[] = [];
  const found: string[] = [];
  function visit(current: string): void {
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) {
        visit(path);
      } else {
        const relative = path
          .slice(extractedRoot.length + 1)
          .replaceAll("\\", "/");
        found.push("package/" + relative);
      }
    }
  }
  visit(extractedRoot);
  const foundSet = new Set(found);
  for (const name of expectedFiles.keys()) {
    if (!foundSet.has(name)) problems.push("missing:" + name);
  }
  for (const name of foundSet) {
    if (!expectedFiles.has(name)) problems.push("unlisted:" + name);
  }
  for (const name of foundSet) {
    const expected = expectedFiles.get(name);
    if (!expected) continue;
    const path = join(extractedRoot, name.slice("package/".length));
    const stats = lstatSync(path);
    if (!stats.isFile() || stats.isSymbolicLink()) {
      problems.push("not-regular:" + name);
      continue;
    }
    if (stats.size !== expected.bytes) problems.push("size:" + name);
    if (digest(readFileSync(path), "sha256") !== expected.sha256) {
      problems.push("sha256:" + name);
    }
    if (process.platform !== "win32") {
      const mode = (stats.mode & 0o777).toString(8).padStart(4, "0");
      if (mode !== expected.mode) problems.push("mode:" + name);
    }
  }
  return problems;
}

function validateArchive(archive: string, extractedRoot: string): string[] {
  const problems: string[] = [];
  const bytes = readFileSync(archive);
  if (digest(bytes, "sha256") !== receipt.api.sha256) {
    problems.push("archive-sha256");
  }
  if (sri(bytes) !== receipt.api.sri) problems.push("archive-sri");
  if (problems.length > 0) return problems;

  // Feed both operations the verified bytes, not a reopened archive path.
  const names = run(["tar", "-tzf", "-"], repo, bytes)
    .split("\n")
    .filter(Boolean);
  if (
    names.length !== expectedFiles.size ||
    new Set(names).size !== names.length ||
    names.some((name) => !/^package\/[A-Za-z0-9_.-]+$/.test(name))
  ) {
    problems.push("archive-member-census");
    return problems;
  }
  mkdirSync(dirname(extractedRoot), { recursive: true });
  run(["tar", "-xzf", "-", "-C", dirname(extractedRoot)], repo, bytes);
  return inspectFiles(extractedRoot);
}

test("unverified archive bytes are refused before tar and extraction", () => {
  const scratch = mkdtempSync(join(tmpdir(), "keld-gh1404-row6-archive-negative-"));
  try {
    const archive = join(scratch, apiArchiveName);
    const extraction = join(scratch, "extracted");
    // A digest rejection is returned even when these inert bytes are not a tar.
    writeFileSync(archive, "not an archive");
    expect(validateArchive(archive, join(extraction, "package"))).toEqual([
      "archive-sha256",
      "archive-sri",
    ]);
    expect(existsSync(extraction)).toBe(false);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});

test("selected source and canonical compiler/producer yield identical API bytes, modes and lock SRI", () => {
  expect(receipt.schema).toBe("keld.api.gh1404-source-built/v1");
  expect(selectedCommitProblems(receipt.selectedSourceCommit)).toEqual([]);
  expect(receipt.scope).toContain("source-only");
  expect(receipt.api.files.length).toBe(37);
  expect(Bun.version).toBe(receipt.toolchain.bun);
  expect(
    JSON.parse(
      readFileSync(join(api, "node_modules/typescript/package.json"), "utf8"),
    ).version,
  ).toBe(receipt.toolchain.typescript);
  expect(
    JSON.parse(
      readFileSync(join(api, "node_modules/@types/bun/package.json"), "utf8"),
    ).version,
  ).toBe(receipt.toolchain.typesBun);
  expect(digest(readFileSync(producer), "sha256")).toBe(receipt.producerSha256);
  expect(run(["git", "rev-parse", "HEAD:packages/@keld/kipc/scripts"])).toBe(
    receipt.producerGitTree,
  );
  expect(
    run(["git", "diff", "--name-only", "HEAD", "--", "packages/@keld/api/src"]),
  ).toBe("");
  expect(emittedSourceFacts(repo)).toEqual([]);
  expect(buildMetadataProblems(receipt.selectedSourceCommit)).toEqual([]);

  const scratch = mkdtempSync(join(tmpdir(), "keld-gh1404-row6-"));
  try {
    const first = join(scratch, "first");
    const second = join(scratch, "second");
    mkdirSync(first);
    mkdirSync(second);
    run([process.execPath, producer, first], repo);
    run([process.execPath, producer, second], api);
    for (const [name, expected] of [
      [apiArchiveName, receipt.api],
      [kipcArchiveName, receipt.kipc],
    ] as const) {
      const left = readFileSync(join(first, name));
      const right = readFileSync(join(second, name));
      expect(left).toEqual(right);
      expect(digest(left, "sha256")).toBe(expected.sha256);
      expect(sri(left)).toBe(expected.sri);
    }
    const archive = join(first, apiArchiveName);
    expect(
      validateArchive(archive, join(scratch, "extracted", "package")),
    ).toEqual([]);

    // A real lock, not a source-version label, must bind the exact archive.
    const consumer = join(scratch, "consumer");
    const vendor = join(consumer, "vendor");
    mkdirSync(vendor, { recursive: true });
    for (const name of [apiArchiveName, kipcArchiveName]) {
      cpSync(join(first, name), join(vendor, name));
    }
    writeFileSync(
      join(consumer, "package.json"),
      JSON.stringify({
        name: "gh1404-row6-source-only-consumer",
        private: true,
        version: "0.0.1",
        dependencies: {
          "@keld/api": "file:vendor/" + apiArchiveName,
          "@keld/kipc": "file:vendor/" + kipcArchiveName,
        },
      }),
    );
    const npm = process.platform === "win32" ? "npm.cmd" : "npm";
    run(
      [
        npm,
        "install",
        "--offline",
        "--package-lock-only",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
        "--cache",
        join(scratch, "npm-cache"),
      ],
      consumer,
    );
    const lock = JSON.parse(
      readFileSync(join(consumer, "package-lock.json"), "utf8"),
    );
    expect(lock.packages["node_modules/@keld/api"].integrity).toBe(
      receipt.api.sri,
    );
    expect(lock.packages["node_modules/@keld/kipc"].integrity).toBe(
      receipt.kipc.sri,
    );
    run(
      [
        npm,
        "ci",
        "--offline",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
        "--cache",
        join(scratch, "npm-cache"),
      ],
      consumer,
    );
    expect(inspectFiles(join(consumer, "node_modules/@keld/api"))).toEqual([]);

    // A mode-only change is invisible to an archive SHA if done after install.
    if (process.platform !== "win32") {
      const installed = join(consumer, "node_modules/@keld/api/app.js");
      chmodSync(installed, 0o755);
      expect(inspectFiles(join(consumer, "node_modules/@keld/api"))).toContain(
        "mode:package/app.js",
      );
    }
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});

test("receipt selected commit must exist, be ancestral and bind the API and producer trees", () => {
  expect(selectedCommitProblems(receipt.selectedSourceCommit)).toEqual([]);
  expect(selectedCommitProblems("0".repeat(40))).toEqual([
    "selected-commit-missing",
  ]);
  expect(selectedCommitProblems("not-a-git-sha")).toEqual([
    "selected-commit-format",
  ]);
});

test("a different committed Git source HEAD cannot reuse the unchanged selected TGZ", () => {
  const scratch = mkdtempSync(
    join(tmpdir(), "keld-gh1404-row6-source-negative-"),
  );
  try {
    const copiedSource = join(scratch, "packages/@keld/api/src");
    mkdirSync(dirname(copiedSource), { recursive: true });
    cpSync(originalSource, copiedSource, { recursive: true });
    run(["git", "init", "-q"], scratch);
    run(["git", "config", "--local", "maintenance.auto", "false"], scratch);
    expect(
      run(["git", "config", "--local", "--get", "maintenance.auto"], scratch),
    ).toBe("false");
    for (const path of buildMetadataPaths) {
      mkdirSync(dirname(join(scratch, path)), { recursive: true });
      cpSync(join(repo, path), join(scratch, path));
    }
    run(["git", "add", "packages/@keld/api/src", ...buildMetadataPaths], scratch);
    run(
      [
        "git",
        "-c",
        "user.name=Keld Source Test",
        "-c",
        "user.email=source-test@example.invalid",
        "-c",
        "core.hooksPath=/dev/null",
        "commit",
        "-qm",
        "committed selected API source",
      ],
      scratch,
    );
    const healthyHead = run(["git", "rev-parse", "HEAD"], scratch);
    expect(emittedSourceFacts(scratch)).toEqual([]);

    expect(buildMetadataProblems(healthyHead, scratch)).toEqual([]);
    const metadataPath = "packages/@keld/kipc/tsconfig.json";
    const metadata = join(scratch, metadataPath);
    const metadataBytes = readFileSync(metadata);
    // Whitespace alone changes source identity while preserving JSON meaning.
    writeFileSync(metadata, Buffer.concat([metadataBytes, Buffer.from("\n")]));
    expect(buildMetadataProblems(healthyHead, scratch)).toEqual([
      "build-metadata-worktree:" + metadataPath,
    ]);
    writeFileSync(metadata, metadataBytes);

    const unchangedArchive = receipt.api.sha256;
    const appPath = join(copiedSource, "app.ts");
    writeFileSync(
      appPath,
      readFileSync(appPath, "utf8") + "\n// committed source change\n",
    );
    run(["git", "add", "packages/@keld/api/src/app.ts"], scratch);
    run(
      [
        "git",
        "-c",
        "user.name=Keld Source Test",
        "-c",
        "user.email=source-test@example.invalid",
        "-c",
        "core.hooksPath=/dev/null",
        "commit",
        "-qm",
        "change selected committed API source",
      ],
      scratch,
    );
    expect(run(["git", "rev-parse", "HEAD"], scratch)).not.toBe(healthyHead);
    expect(emittedSourceFacts(scratch)).toEqual(["source-tree", "source-app"]);
    expect(receipt.api.sha256).toBe(unchangedArchive);

    // A second real root commit with identical bytes is still a foreign
    // source generation when it is not in the selected commit's ancestry.
    const foreignHead = run(
      [
        "git",
        "-c",
        "user.name=Keld Source Test",
        "-c",
        "user.email=source-test@example.invalid",
        "commit-tree",
        "-m",
        "unrelated committed source",
        "HEAD^{tree}",
      ],
      scratch,
    );
    run(["git", "update-ref", "HEAD", foreignHead], scratch);
    expect(selectedCommitProblems(healthyHead, scratch)).toContain(
      "selected-commit-not-ancestor",
    );
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});

test("foreign Git environment cannot mask a dirty source in the expected checkout", () => {
  const scratch = mkdtempSync(join(tmpdir(), "keld-gh1404-row6-git-env-"));
  const checkout = join(scratch, "checkout");
  const foreign = join(scratch, "foreign");
  const sourcePath = "packages/@keld/api/src/app.ts";
  const cleanSource = readFileSync(join(originalSource, "app.ts"));
  const oldWorkTree = process.env.GIT_WORK_TREE;
  const oldGitDir = process.env.GIT_DIR;
  try {
    mkdirSync(dirname(join(checkout, sourcePath)), { recursive: true });
    mkdirSync(dirname(join(foreign, sourcePath)), { recursive: true });
    writeFileSync(join(checkout, sourcePath), cleanSource);
    writeFileSync(join(foreign, sourcePath), cleanSource);
    run(["git", "init", "-q"], checkout);
    run(["git", "config", "maintenance.auto", "false"], checkout);
    run(["git", "init", "-q"], foreign);
    run(["git", "config", "maintenance.auto", "false"], foreign);
    run(["git", "add", sourcePath], checkout);
    run(
      [
        "git",
        "-c",
        "user.name=Keld Source Test",
        "-c",
        "user.email=source-test@example.invalid",
        "-c",
        "core.hooksPath=/dev/null",
        "commit",
        "-qm",
        "clean expected source",
      ],
      checkout,
    );

    process.env.GIT_WORK_TREE = foreign;
    expect(
      run(["git", "diff", "--name-only", "HEAD", "--", sourcePath], checkout),
    ).toBe("");
    writeFileSync(
      join(checkout, sourcePath),
      Buffer.concat([cleanSource, Buffer.from("\n// dirty expected source\n")]),
    );
    expect(
      run(["git", "diff", "--name-only", "HEAD", "--", sourcePath], checkout),
    ).toBe(sourcePath);

    process.env.GIT_DIR = join(foreign, ".git");
    expect(selectedCommitProblems(receipt.selectedSourceCommit)).toEqual([]);
    expect(
      selectedCommitProblems(receipt.selectedSourceCommit, join(repo, "tools")),
    ).toEqual(["git-root-mismatch"]);
    expect(() =>
      run(["git", "rev-parse", "--show-toplevel"], join(repo, "tools")),
    ).toThrow("unexpected Git root");
  } finally {
    if (oldWorkTree === undefined) delete process.env.GIT_WORK_TREE;
    else process.env.GIT_WORK_TREE = oldWorkTree;
    if (oldGitDir === undefined) delete process.env.GIT_DIR;
    else process.env.GIT_DIR = oldGitDir;
    rmSync(scratch, { recursive: true, force: true });
  }
});

test("source checks suppress a repository-local fsmonitor command", () => {
  if (process.platform === "win32") return;
  const scratch = mkdtempSync(join(tmpdir(), "keld-gh1404-row6-fsmonitor-"));
  const checkout = join(scratch, "checkout");
  const hook = join(scratch, "fsmonitor-hook.sh");
  const marker = join(scratch, "fsmonitor-ran");
  const sourcePath = "packages/@keld/api/src/app.ts";
  try {
    mkdirSync(dirname(join(checkout, sourcePath)), { recursive: true });
    writeFileSync(
      join(checkout, sourcePath),
      readFileSync(join(originalSource, "app.ts")),
    );
    writeFileSync(
      hook,
      "#!/bin/sh\nprintf 'called\\n' >> \"$KELD_FSMONITOR_MARKER\"\nprintf 'token\\n'\n",
    );
    chmodSync(hook, 0o755);
    run(["git", "init", "-q"], checkout);
    run(["git", "config", "maintenance.auto", "false"], checkout);
    run(["git", "add", sourcePath], checkout);
    run(
      [
        "git",
        "-c",
        "user.name=Keld Source Test",
        "-c",
        "user.email=source-test@example.invalid",
        "-c",
        "core.hooksPath=/dev/null",
        "commit",
        "-qm",
        "clean source",
      ],
      checkout,
    );
    run(["git", "config", "--local", "core.fsmonitor", hook], checkout);

    const inheritedEnv = {
      ...gitSafeEnvironment(),
      KELD_FSMONITOR_MARKER: marker,
    };
    const unguarded = Bun.spawnSync(["git", "status", "--short"], {
      cwd: checkout,
      stdout: "pipe",
      stderr: "pipe",
      timeout: 10_000,
      env: inheritedEnv,
    });
    expect(unguarded.exitCode).toBe(0);
    expect(readFileSync(marker, "utf8")).toBe("called\n");

    rmSync(marker);
    run(["git", "status", "--short"], checkout);
    expect(() => readFileSync(marker, "utf8")).toThrow();
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});

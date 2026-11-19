import { createHash } from "node:crypto";
import { expect, test } from "bun:test";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

const workflow = Bun.YAML.parse(
  readFileSync(".github/workflows/ci-provenance.yml", "utf8"),
) as Record<string, any>;
const job = workflow.jobs.provenance;
const publisherJob = workflow.jobs.publish;
const reviewedAction =
  "actions/github-script@3a2844b7e9c422d3c10d287c895573f7108da1b3";
const code = job.steps[0].with.script as string;
const AsyncFunction = Object.getPrototypeOf(async function () {})
  .constructor as new (
  ...args: string[]
) => (...args: unknown[]) => Promise<void>;
const evaluate = new AsyncFunction("github", "context", "core", code);

type ChangedFile = {
  filename: string;
  status: string;
  previous_filename?: string;
};

function fixture(
  files: ChangedFile[],
  overrides: {
    head?: string;
    base?: string;
    apiHead?: string;
    apiBase?: string;
    baseRef?: string;
    repo?: string;
    state?: string;
    changedFiles?: number;
    changedOnSecondFetch?: boolean;
    changedCountOnSecondFetch?: number;
    apiFailure?: boolean;
    badEventNumber?: boolean;
    mutablePrFileList?: ChangedFile[];
    comparisonFiles?: ChangedFile[];
    comparisonBase?: string;
    comparisonMergeBase?: string;
    baseTreeEntries?: Record<string, unknown>[];
    headTreeEntries?: Record<string, unknown>[];
    truncatedTree?: "base" | "head";
    wrongTreeSha?: "base" | "head";
    wrongCommitSha?: "base" | "head";
    gitApiFailure?: boolean;
  } = {},
) {
  const expectedHead = overrides.head ?? "head-commit-1";
  const expectedBase = overrides.base ?? "base-commit-1";
  const mergeBaseSha = overrides.comparisonMergeBase ?? "c".repeat(40);
  const baseTreeSha = "d".repeat(40);
  const headTreeSha = "e".repeat(40);
  const current = {
    number: 123,
    state: overrides.state ?? "open",
    changed_files: overrides.changedFiles ?? files.length,
    base: {
      ref: overrides.baseRef ?? "main",
      sha: overrides.apiBase ?? expectedBase,
      repo: { full_name: "gyldlab/keld" },
    },
    head: { sha: overrides.apiHead ?? expectedHead },
  };
  let reads = 0;
  let paginationCalls = 0;
  let comparisonCalls = 0;
  let treeCalls = 0;
  const github = {
    rest: {
      git: {
        getCommit: async (args: Record<string, unknown>) => {
          if (overrides.gitApiFailure) throw Error("Git tree API unavailable");
          const side = args.commit_sha === mergeBaseSha ? "base" : "head";
          expect(args).toEqual({
            owner: "gyldlab",
            repo: "keld",
            commit_sha: side === "base" ? mergeBaseSha : expectedHead,
          });
          return {
            data: {
              sha:
                overrides.wrongCommitSha === side
                  ? "0".repeat(40)
                  : args.commit_sha,
              tree: { sha: side === "base" ? baseTreeSha : headTreeSha },
            },
          };
        },
        getTree: async (args: Record<string, unknown>) => {
          if (overrides.gitApiFailure) throw Error("Git tree API unavailable");
          treeCalls++;
          const side = args.tree_sha === baseTreeSha ? "base" : "head";
          expect(args).toEqual({
            owner: "gyldlab",
            repo: "keld",
            tree_sha: side === "base" ? baseTreeSha : headTreeSha,
            recursive: "1",
          });
          const headEntries = files.map((file) => ({
            path: file.filename,
            type: "blob",
            mode: "100644",
            sha: "f".repeat(40),
          }));
          return {
            data: {
              sha:
                overrides.wrongTreeSha === side
                  ? "0".repeat(40)
                  : args.tree_sha,
              truncated: overrides.truncatedTree === side,
              tree:
                side === "base"
                  ? (overrides.baseTreeEntries ?? [])
                  : (overrides.headTreeEntries ?? headEntries),
            },
          };
        },
      },
      pulls: {
        get: async () => {
          if (overrides.apiFailure) throw Error("GitHub API unavailable");
          reads += 1;
          if (reads === 2 && overrides.changedOnSecondFetch) {
            return { data: { ...current, head: { sha: "newer-head" } } };
          }
          if (
            reads === 2 &&
            overrides.changedCountOnSecondFetch !== undefined
          ) {
            return {
              data: {
                ...current,
                changed_files: overrides.changedCountOnSecondFetch,
              },
            };
          }
          return { data: current };
        },
        listFiles: () => {
          throw Error("GitHub paginate must be used");
        },
      },
      repos: {
        compareCommits: async (args: Record<string, unknown>) => {
          comparisonCalls += 1;
          if (overrides.apiFailure) throw Error("GitHub API unavailable");
          expect(args).toEqual({
            owner: "gyldlab",
            repo: "keld",
            base: current.base.sha,
            head: current.head.sha,
            page: 1,
            per_page: 100,
          });
          return {
            data: {
              base_commit: {
                sha: overrides.comparisonBase ?? current.base.sha,
              },
              merge_base_commit: { sha: mergeBaseSha },
              files: overrides.comparisonFiles ?? files,
            },
          };
        },
      },
    },
    paginate: async (_unused: unknown, args: Record<string, unknown>) => {
      paginationCalls += 1;
      if (overrides.apiFailure) throw Error("GitHub API unavailable");
      expect(args).toEqual({
        owner: "gyldlab",
        repo: "keld",
        pull_number: 123,
        per_page: 100,
      });
      return overrides.mutablePrFileList ?? files;
    },
  };
  const context = {
    repo: { owner: "gyldlab", repo: overrides.repo ?? "keld" },
    payload: {
      pull_request: {
        number: overrides.badEventNumber ? -123 : 123,
        head: { sha: expectedHead },
        base: { sha: expectedBase },
      },
    },
  };
  const messages: string[] = [];
  const outputs: Record<string, string> = {};
  const core = {
    info: (message: string) => messages.push(message),
    setOutput: (name: string, value: string) => {
      outputs[name] = value;
    },
  };
  return {
    run: () => evaluate(github, context, core),
    getReads: () => reads,
    getPages: () => paginationCalls,
    getComparisons: () => comparisonCalls,
    getTrees: () => treeCalls,
    messages,
    outputs,
  };
}

test("base-owned workflow is immutable/read-only and never executes PR code", () => {
  expect(workflow.name).toBe("CI provenance");
  expect(workflow.on.pull_request_target.types).toEqual([
    "opened",
    "reopened",
    "synchronize",
    "edited",
    "ready_for_review",
  ]);
  expect(workflow.permissions).toEqual({
    contents: "read",
    "pull-requests": "read",
  });
  expect(Object.keys(workflow.jobs)).toEqual(["provenance", "publish"]);
  expect(job.outputs).toEqual({
    verified_head_sha: "${{ steps.verify.outputs.verified_head_sha }}",
    verified_base_sha: "${{ steps.verify.outputs.verified_base_sha }}",
  });
  expect(job.name).toBe("CI provenance");
  expect(job["runs-on"]).toBe("ubuntu-latest");
  expect(job["timeout-minutes"]).toBe(5);
  expect(job.steps).toHaveLength(1);
  expect(job.steps[0].uses).toBe(reviewedAction);
  expect(job.steps[0].id).toBe("verify");
  expect(Object.keys(job.steps[0])).toEqual(["name", "id", "uses", "with"]);
  expect(Object.keys(job.steps[0].with)).toEqual(["script"]);
  expect(job.steps[0].run).toBeUndefined();
  expect(job.if).toBeUndefined();
});

test("benign documentation and non-authority lockfile changes are admitted", async () => {
  const f = fixture([
    { filename: "docs/notes.md", status: "modified" },
    { filename: "Cargo.lock", status: "modified" },
  ]);
  await expect(f.run()).resolves.toBeUndefined();
  expect(f.getReads()).toBe(2);
  expect(f.getPages()).toBe(0);
  expect(f.getComparisons()).toBe(1);
  expect(f.messages[0]).toContain("2 PR files inspected");
  expect(f.outputs).toEqual({
    verified_head_sha: "head-commit-1",
    verified_base_sha: "base-commit-1",
  });
});

test.each([
  [".github/workflows/evil.yml", "added"],
  [".github/workflows/ci.yml", "modified"],
  [".github/workflows/keldbot.yml", "removed"],
  [".github/workflows/CI.YML", "modified"],
  [".github/CODEOWNERS", "modified"],
  ["tools/ci-inputs.json", "modified"],
  ["tools/dependency_review_metadata.sh", "modified"],
  ["tools/repo_path_contract.rs", "modified"],
  ["tools/windows_signed_installed_acceptance.ps1", "modified"],
  ["crates/keld-wv/tests/linux_gui_smoke.sh", "modified"],
  ["crates/keld-wv/tests/windows_media_guard.ps1", "modified"],
  ["crates/keld-wv/tests/fixtures/linux_media_interpose.c", "modified"],
  ["crates/keld-wv/tests/linux_media_guard.sh", "modified"],
  ["crates/keld-wv/tests/linux_media_checkout.sh", "modified"],
  ["crates/keld-wv/tests/linux_media_checkout_test.sh", "modified"],
  ["crates/keld-wv/examples/linux_media_guard.rs", "modified"],
  ["crates/keld-wv/tests/fixtures/NEW-WATCHDOG.CMD", "added"],
  ["docs/audits/verify.py", "modified"],
  ["docs/audits/test_verify.py", "modified"],
  ["docs/audits/VERIFY.PY", "added"],
  [".config/nextest.toml", "modified"],
  // All three are real authority paths, not benign source/document content.
  ["crates/keld-updater-helper/clippy.toml", "modified"],
  ["crates/keld-core/.clippy.toml", "added"],
  [".rustfmt.toml", "modified"],
  ["crates/keld-core/.rustfmt.toml", "added"],
  [".ci-reader-proofs/readers/forged.json", "added"],
  [".ci-reader-proofs/readers/old.json", "removed"],
  [".ci-reader-proofs/readers/edited.json", "modified"],
  [".cargo/config.toml", "added"],
  [".github/actions/untrusted/action.yml", "added"],
  ["crates/keld-updater-helper/deny.toml", "modified"],
  [".gitleaks.toml", "modified"],
  ["tools/ci_changes.sh", "modified"],
  ["tools/ci_workflow_security.ts", "modified"],
  ["tools/ci_hygiene.rs", "modified"],
  ["tools/test_ci_local.py", "modified"],
  ["justfile", "modified"],
  ["rust-toolchain.toml", "modified"],
] as const)(
  "protected CI authority change %s (%s) is refused even if legacy CI required is green",
  async (filename, status) => {
    const f = fixture([{ filename, status }]);
    await expect(f.run()).rejects.toThrow(
      "protected CI authority require a separately reviewed",
    );
  },
);

test.each([
  ".gitignore",
  ".GITIGNORE",
  "crates/keld-cli/templates/hello/.gitignore",
  ".prettierignore",
  "packages/demo/.prettierignore",
  ".prettierrc",
  "packages/demo/.prettierrc.yaml",
  "packages/demo/prettier.config.cjs",
  ".editorconfig",
  "crates/keld-cli/templates/hello/.editorconfig",
  ".oxfmtrc.jsonc",
  "packages/demo/oxfmt.config.ts",
  "packages/demo/package.yaml",
  "packages/demo/package.yml",
  "packages/demo/biome.jsonc",
] as const)(
  "formatter policy input %s cannot alter required checks",
  async (filename) => {
    const f = fixture([{ filename, status: "modified" }]);
    await expect(f.run()).rejects.toThrow("protected CI authority");
  },
);

test("added, deleted and renamed formatter authority cannot masquerade as documentation", async () => {
  for (const file of [
    { filename: "packages/demo/.gitignore", status: "added" },
    { filename: ".prettierignore", status: "removed" },
    {
      filename: "docs/old-format-config.md",
      previous_filename: "packages/demo/.prettierrc",
      status: "renamed",
    },
  ]) {
    await expect(fixture([file]).run()).rejects.toThrow(
      "protected CI authority",
    );
  }
  await expect(
    fixture([
      { filename: "docs/gitignore-guide.md", status: "modified" },
    ]).run(),
  ).resolves.toBeUndefined();
});

test("a native formatter false-green from tracked source plus ignore is refused", async () => {
  const root = mkdtempSync(join(tmpdir(), "keld-ci-provenance-format-"));
  const source = "packages/demo/example.ts";
  try {
    mkdirSync(dirname(join(root, source)), { recursive: true });
    cpSync(join(import.meta.dir, "../bunfig.toml"), join(root, "bunfig.toml"));
    expect(
      Bun.spawnSync(["git", "init", "--quiet"], { cwd: root }).exitCode,
    ).toBe(0);
    expect(
      Bun.spawnSync(["git", "config", "maintenance.auto", "false"], {
        cwd: root,
      }).exitCode,
    ).toBe(0);
    writeFileSync(join(root, source), "export const answer={value:42};\n");
    const native = (...flags: string[]) =>
      Bun.spawnSync(
        [
          process.execPath,
          "fmt",
          "--no-script",
          ...flags,
          "**/*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}",
        ],
        { cwd: root },
      );
    expect(native().exitCode).toBe(0);
    expect(Bun.spawnSync(["git", "add", source], { cwd: root }).exitCode).toBe(
      0,
    );
    expect(native("--check").exitCode).toBe(0);
    writeFileSync(join(root, source), "export const answer={value:42};\n");
    expect(native("--check").exitCode).toBe(1);
    writeFileSync(join(root, ".gitignore"), source + "\n");
    expect(native("--check").exitCode).toBe(0);
    await expect(
      fixture([
        { filename: source, status: "modified" },
        { filename: ".gitignore", status: "modified" },
      ]).run(),
    ).rejects.toThrow("protected CI authority");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}, 30_000);

test.each([
  "Cargo.toml",
  "crates/keld-core/Cargo.toml",
  "crates/keld-ipc/fuzz/Cargo.toml",
  "bunfig.toml",
  "packages/@keld/kipc/bunfig.toml",
  "rustfmt.toml",
  "tsconfig.json",
  "packages/@keld/api/package.json",
  "packages/@keld/kipc/package.json",
  "packages/@keld/kipc/scripts/echo-codegen.ts",
] as const)(
  "root or nested test/build authority %s is refused",
  async (filename) => {
    const f = fixture([{ filename, status: "modified" }]);
    await expect(f.run()).rejects.toThrow("protected CI authority");
  },
);

test("renaming protected CI workflow to innocuous docs path is refused", async () => {
  const f = fixture([
    {
      filename: "docs/old-ci-workflow.txt",
      previous_filename: ".github/workflows/ci.yml",
      status: "renamed",
    },
  ]);
  await expect(f.run()).rejects.toThrow("protected CI authority");
});

test.each([
  "crates/keld-updater-helper/clippy.toml",
  "crates/keld-core/.clippy.toml",
  ".rustfmt.toml",
  "crates/keld-core/.rustfmt.toml",
  ".ci-reader-proofs/readers/forged.json",
] as const)(
  "renaming protected authority %s into an unrelated docs file is refused",
  async (previous_filename) => {
    const f = fixture([
      { filename: "docs/notes.md", previous_filename, status: "renamed" },
    ]);
    await expect(f.run()).rejects.toThrow("protected CI authority");
  },
);

test("A-to-B-to-A mutable PR-file listing cannot hide protected commit A", async () => {
  const f = fixture(
    [{ filename: ".github/workflows/ci.yml", status: "modified" }],
    {
      // Before and after PR reads both report A. Between them, the mutable
      // list endpoint serves B with an equal-sized innocuous change.
      mutablePrFileList: [{ filename: "docs/ok.md", status: "modified" }],
    },
  );
  await expect(f.run()).rejects.toThrow("protected CI authority");
  expect(f.getPages()).toBe(0);
  expect(f.getComparisons()).toBe(1);
});

test("immutable comparison checks correct commit base and complete file coverage", async () => {
  const benignFile = [{ filename: "docs/ok.md", status: "modified" }];
  // A response for a different base must be refused even if its file count matches.
  await expect(
    fixture(benignFile, {
      comparisonBase: "wrong-base-sha",
    }).run(),
  ).rejects.toThrow("comparison for the wrong immutable base");
  // The immutable comparison, not a mutable PR list, is the authority: a
  // mismatched/truncated immutable response cannot produce admitted evidence.
  await expect(
    fixture(benignFile, {
      comparisonFiles: [],
    }).run(),
  ).rejects.toThrow("incomplete or truncated file list");
});

test("immutable comparison returns all 101 files on first commit-list page", async () => {
  const files = Array.from({ length: 101 }, (_, i) => ({
    filename: `docs/benign-${i}.md`,
    status: "modified",
  }));
  const f = fixture(files);
  await expect(f.run()).resolves.toBeUndefined();
  expect(f.getComparisons()).toBe(1);
  expect(f.getPages()).toBe(0);
});

test.each([300, 448, 600])(
  "%i benign files are admitted only after a complete immutable tree census",
  async (count) => {
    const files = Array.from({ length: count }, (_, i) => ({
      filename: `docs/file${i}.md`,
      status: "added",
    }));
    const f = fixture(files, {
      head: "a".repeat(40),
      base: "b".repeat(40),
      // CompareCommits provides only its first 300 entries even at 600.
      comparisonFiles: files.slice(0, 300),
      mutablePrFileList: [{ filename: "docs/decoy.md", status: "added" }],
    });
    await expect(f.run()).resolves.toBeUndefined();
    expect(f.getComparisons()).toBe(1);
    expect(f.getTrees()).toBe(2);
    expect(f.getPages()).toBe(0);
    expect(f.messages[0]).toContain(`${count} PR files inspected`);
  },
);

test("inconsistent 300-file metadata cannot disguise 448 distinct immutable additions", async () => {
  const files = Array.from({ length: 448 }, (_, i) => ({
    filename: `docs/add-${i}.md`,
    status: "added",
  }));
  const f = fixture(files, {
    head: "a".repeat(40),
    base: "b".repeat(40),
    changedFiles: 300,
    comparisonFiles: files.slice(0, 300),
  });
  await expect(f.run()).rejects.toThrow(
    "census disagrees with changed-file count",
  );
  expect(f.getTrees()).toBe(2);
});

test("immutable tree census supports exact source/destination rename pairs", async () => {
  const files = Array.from({ length: 300 }, (_, i) => ({
    filename: `docs/add-${i}.md`,
    status: "added",
  }));
  const old = {
    path: "docs/old-name.md",
    type: "blob",
    mode: "100644",
    sha: "f".repeat(40),
  };
  const f = fixture(files, {
    head: "a".repeat(40),
    base: "b".repeat(40),
    baseTreeEntries: [old],
    comparisonFiles: files.slice(0, 300),
  });
  await expect(f.run()).resolves.toBeUndefined();
  expect(f.getTrees()).toBe(2);
});

test("599 and 600 are permitted, but 601 files are rejected before enumeration", async () => {
  const f = fixture([], { changedFiles: 601 });
  await expect(f.run()).rejects.toThrow("more than 600 changed files");
  expect(f.getComparisons()).toBe(0);
  expect(f.getTrees()).toBe(0);
  await expect(
    fixture(
      Array.from({ length: 299 }, (_, i) => ({
        filename: `docs/file${i}.md`,
        status: "added",
      })),
    ).run(),
  ).resolves.toBeUndefined();
});

test("protected authority after Compare API's first 300 files cannot hide", async () => {
  const files = Array.from({ length: 448 }, (_, i) => ({
    filename: `docs/benign-${i}.md`,
    status: "added",
  }));
  files[447].filename = "deny.toml";
  const f = fixture(files, {
    head: "a".repeat(40),
    base: "b".repeat(40),
    comparisonFiles: files.slice(0, 300),
  });
  await expect(f.run()).rejects.toThrow("protected CI authority");
  expect(f.getTrees()).toBe(2);
});

test("immutable tree census catches a protected-file rename even when the new name is benign", async () => {
  const files = Array.from({ length: 300 }, (_, i) => ({
    filename: `docs/benign-${i}.md`,
    status: "added",
  }));
  const f = fixture(files, {
    head: "a".repeat(40),
    base: "b".repeat(40),
    comparisonFiles: files.slice(0, 300),
    baseTreeEntries: [
      {
        path: ".github/workflows/ci.yml",
        type: "blob",
        mode: "100644",
        sha: "f".repeat(40),
      },
    ],
  });
  await expect(f.run()).rejects.toThrow("protected CI authority");
});

test("unchanged protected paths are not mistaken for PR changes", async () => {
  const files = Array.from({ length: 300 }, (_, i) => ({
    filename: `docs/benign-${i}.md`,
    status: "added",
  }));
  const protectedEntry = {
    path: ".github/workflows/ci.yml",
    type: "blob",
    mode: "100644",
    sha: "f".repeat(40),
  };
  const f = fixture(files, {
    head: "a".repeat(40),
    base: "b".repeat(40),
    comparisonFiles: files.slice(0, 300),
    baseTreeEntries: [protectedEntry],
    headTreeEntries: [
      protectedEntry,
      ...files.map((file) => ({
        path: file.filename,
        type: "blob",
        mode: "100644",
        sha: "f".repeat(40),
      })),
    ],
  });
  await expect(f.run()).resolves.toBeUndefined();
});

test.each(["base", "head"] as const)(
  "truncated or unbound %s tree fails closed at 300 files",
  async (side) => {
    const files = Array.from({ length: 300 }, (_, i) => ({
      filename: `docs/benign-${i}.md`,
      status: "added",
    }));
    const opts = { head: "a".repeat(40), base: "b".repeat(40) };
    await expect(
      fixture(files, { ...opts, truncatedTree: side }).run(),
    ).rejects.toThrow("truncated or unbound Git tree");
    await expect(
      fixture(files, { ...opts, wrongTreeSha: side }).run(),
    ).rejects.toThrow("truncated or unbound Git tree");
    await expect(
      fixture(files, { ...opts, wrongCommitSha: side }).run(),
    ).rejects.toThrow("unbound Git commit tree");
  },
);

test("duplicate/malformed immutable paths and incomplete census are rejected", async () => {
  const files = Array.from({ length: 300 }, (_, i) => ({
    filename: `docs/benign-${i}.md`,
    status: "added",
  }));
  const goodEntries = files.map((file) => ({
    path: file.filename,
    type: "blob",
    mode: "100644",
    sha: "f".repeat(40),
  }));
  const opts = { head: "a".repeat(40), base: "b".repeat(40) };
  await expect(
    fixture(files, {
      ...opts,
      headTreeEntries: [...goodEntries, goodEntries[0]],
    }).run(),
  ).rejects.toThrow("malformed or duplicated Git tree entry");
  await expect(
    fixture(files, {
      ...opts,
      headTreeEntries: [
        ...goodEntries.slice(0, -1),
        {
          ...goodEntries[299],
          path: "../ci.yml",
        },
      ],
    }).run(),
  ).rejects.toThrow("malformed or duplicated Git tree entry");
  await expect(
    fixture(files, {
      ...opts,
      headTreeEntries: goodEntries.slice(0, 299),
    }).run(),
  ).rejects.toThrow("census disagrees");
  await expect(
    fixture(files, { ...opts, comparisonMergeBase: "not-a-sha" }).run(),
  ).rejects.toThrow("missing immutable comparison commits");
  await expect(
    fixture(files, { ...opts, gitApiFailure: true }).run(),
  ).rejects.toThrow("Git tree API unavailable");
});

test("renames missing original path and unknown statuses fail closed", async () => {
  await expect(
    fixture([{ filename: "x", status: "renamed" }]).run(),
  ).rejects.toThrow("rename with missing source path");
  await expect(
    fixture([{ filename: "x", status: "surprise" }]).run(),
  ).rejects.toThrow("unknown changed-file status");
});

test("mismatched or changing head/base, missing files, and wrong base fail closed", async () => {
  await expect(
    fixture([{ filename: "docs/ok.md", status: "modified" }], {
      apiHead: "unexpected-head",
    }).run(),
  ).rejects.toThrow("stale pull-request head/base");
  await expect(
    fixture([{ filename: "docs/ok.md", status: "modified" }], {
      apiBase: "different-base",
    }).run(),
  ).rejects.toThrow("stale pull-request head/base");
  await expect(
    fixture([{ filename: "docs/ok.md", status: "modified" }], {
      changedOnSecondFetch: true,
    }).run(),
  ).rejects.toThrow("stale pull-request head/base");
  await expect(
    fixture([{ filename: "docs/ok.md", status: "modified" }], {
      changedCountOnSecondFetch: 2,
    }).run(),
  ).rejects.toThrow("changed PR file count during verification");
  await expect(
    fixture([{ filename: "docs/ok.md", status: "modified" }], {
      changedFiles: 2,
    }).run(),
  ).rejects.toThrow("incomplete or truncated file list");
  await expect(fixture([], { baseRef: "dev" }).run()).rejects.toThrow(
    "accepts only gyldlab/keld main",
  );
  await expect(fixture([], { state: "closed" }).run()).rejects.toThrow(
    "absent, closed or changed",
  );
});

test("GitHub API problems, invalid PR/repo and comparison cap refuse admission", async () => {
  await expect(fixture([], { apiFailure: true }).run()).rejects.toThrow(
    "GitHub API unavailable",
  );
  await expect(fixture([], { repo: "evil" }).run()).rejects.toThrow(
    "expected base repository",
  );
  await expect(fixture([], { badEventNumber: true }).run()).rejects.toThrow(
    "valid pull request number",
  );
  await expect(fixture([], { changedFiles: 3000 }).run()).rejects.toThrow(
    "more than 600 changed files",
  );
});

test("backslash and NUL in GitHub paths are refused, not normalized", async () => {
  for (const filename of [".github\\workflows\\evil.yml", "x\0y"]) {
    await expect(
      fixture([{ filename, status: "added" }]).run(),
    ).rejects.toThrow("malformed Git paths");
  }
});

test("review-state changes cannot authorize a protected CI edit without a trusted re-evaluation event", async () => {
  const f = fixture([
    { filename: ".github/workflows/ci-provenance.yml", status: "modified" },
  ]);
  await expect(f.run()).rejects.toThrow("protected CI authority");
  // A previously published App PASS must never rely on an ephemeral review
  // that can be dismissed without triggering pull_request_target again.
  expect(code).not.toContain("listReviews");
  expect(f.outputs).toEqual({});
});

test("tree census rechecks a 300-file count after enumeration", async () => {
  const files = Array.from({ length: 300 }, (_, i) => ({
    filename: `docs/tree-${i}.md`,
    status: "added",
  }));
  const f = fixture(files, {
    head: "a".repeat(40),
    base: "b".repeat(40),
    comparisonFiles: files.slice(0, 300),
    changedCountOnSecondFetch: 301,
  });
  await expect(f.run()).rejects.toThrow(
    "changed PR file count during verification",
  );
  expect(f.getTrees()).toBe(2);
});

// B-82 evidence must run before candidate-owned source (including transitive
// echo-codegen imports) can rewrite its verifier. Only the pinned, reviewed Rust
// installer is allowed between credential-free checkout and this gate.
// Keep the B-82 shell program pinned independently of the workflow YAML.
// Otherwise an in-step command injection or `|| true` could pass a substring-only check.
const trustedKEL270ReceiptPath =
  "docs/engineering/evidence/kel270-catalogue-admission-v2/review-receipt.json";
const trustedKEL270ReceiptSha256 =
  "7b9e858738d51665120d4690929d13b1ab0ee7f80b0e6d82d6bb093ffcff0c42";
const trustedKEL270VerifierPath =
  "docs/engineering/evidence/kel270-catalogue-admission-v2/verify_spec_evidence.py";
const trustedKEL270VerifierSha256 =
  "9ac3f00c1c2db790f3d98ab24d32ee74396438bcb17d3ca23b8a34a24721290a";
const safePythonEnvironment = [
  'for variable in "${!PYTHON@}"; do',
  '  unset "$variable"',
  "done",
  "export PYTHONNOUSERSITE=1",
  "export PYTHONSAFEPATH=1",
].join("\n");
const approvedKEL270VerifierIntegrityCheck = String.raw`# The receipt is pinned above, so its verifier digest is an independent
# trust anchor. Authenticate the verifier before executing candidate code.
verifier_path=${trustedKEL270VerifierPath}
python3 -I -S -c '
import hashlib, json, re, sys
receipt_path, verifier_path = sys.argv[1:]
with open(receipt_path, encoding="utf-8") as source:
    receipt = json.load(source)
expected = receipt["published_file_sha256"].get(verifier_path)
if not isinstance(expected, str) or re.fullmatch(r"[0-9a-f]{64}", expected) is None:
    raise SystemExit("invalid verifier digest in trusted receipt")
with open(verifier_path, "rb") as source:
    actual = hashlib.sha256(source.read()).hexdigest()
if actual != expected:
    raise SystemExit("KEL-270 B-82 verifier SHA-256 mismatch")
' "$receipt_path" "$verifier_path"`;
const githubShaShell = "${GITHUB_SHA:-}";
const githubWorkspaceShell = "${GITHUB_WORKSPACE:?}";
const approvedKEL270CheckoutCheck = String.raw`checkout_root="$(pwd -P)"
if [[ ! "${githubShaShell}" =~ ^[0-9a-f]{40}$ ]] ||
   [[ "$checkout_root" != "$(realpath -e -- "${githubWorkspaceShell}")" ]] ||
   [[ "$checkout_root" != "$(git rev-parse --show-toplevel)" ]] ||
   [[ "$GITHUB_SHA" != "$(git rev-parse --verify HEAD)" ]]; then
  echo "KEL-270 B-82 checkout identity mismatch" >&2
  exit 1
fi
require_checkout_file() {
  local path="$1" mode kind object
  read -r mode kind object _ <<< "$(git ls-tree "$GITHUB_SHA" -- "$path")"
  if [[ ! -f "$path" || "$(realpath -e -- "$path")" != "$checkout_root/$path" ||
        "$mode" != 100644 || "$kind" != blob ||
        "$object" != "$(git hash-object --no-filters -- "$path")" ]]; then
    echo "KEL-270 B-82 evidence file does not match checkout: $path" >&2
    exit 1
  fi
}`;
const approvedKEL270SourcePathsCheck = String.raw`# Bind all receipt-pinned sources to regular blobs in the expected checkout.
python3 -I -S -c '
import json, pathlib, subprocess, sys
root = pathlib.Path.cwd()
commit = sys.argv[2]
with open(sys.argv[1], encoding="utf-8") as source:
    paths = json.load(source)["published_file_sha256"]
for name in paths:
    relative = pathlib.PurePosixPath(name)
    path = root / relative
    if (relative.is_absolute() or ".." in relative.parts
            or path.resolve(strict=True) != path or not path.is_file()):
        raise SystemExit("KEL-270 B-82 evidence source outside canonical checkout")
    row = subprocess.check_output([
        "git", "--literal-pathspecs", "ls-tree",
        "--format=%(objectmode) %(objecttype) %(objectname)", commit, "--", name,
    ]).decode("ascii").split()
    if (len(row) != 3 or row[:2] != ["100644", "blob"]
            or row[2] != subprocess.check_output([
                "git", "hash-object", "--no-filters", "--", name,
            ]).decode("ascii").strip()):
        raise SystemExit("KEL-270 B-82 evidence source does not match checkout")
' "$receipt_path" "$GITHUB_SHA"`;
const approvedKEL270EvidenceRun = String.raw`set -euo pipefail
${approvedKEL270CheckoutCheck}
# The receipt supplies all 15 source hashes. Pin its reviewed bytes in
# protected CI so a candidate cannot repin both a source and its manifest.
receipt_path=${trustedKEL270ReceiptPath}
require_checkout_file "$receipt_path"
if ! printf '%s  %s\n' \
  '${trustedKEL270ReceiptSha256}' \
  "$receipt_path" | sha256sum --check --status; then
  echo "KEL-270 B-82 trusted receipt SHA-256 mismatch" >&2
  exit 1
fi
# A pull request can add sourceless imports (for example argparse.pyc)
# beside the verifier. -B alone prevents writes, not importing them.
# Clear runner-provided Python startup controls before the verifier and
# its child processes; re-enable only safe-path and no-user-site flags.
${safePythonEnvironment}
require_checkout_file "${trustedKEL270VerifierPath}"
${approvedKEL270VerifierIntegrityCheck}
${approvedKEL270SourcePathsCheck}
python3.12 -I -m venv "$RUNNER_TEMP/kel270-evidence-python"
cat > "$RUNNER_TEMP/kel270-evidence-requirements.txt" <<'REQ'
cryptography==50.0.2 \
    --hash=sha256:9dab55f57c74c3cad24c323bacbbd04be4705ba6eb0d92e920b1fc4837ed5079
cffi==2.0.0 \
    --hash=sha256:3e17ed538242334bf70832644a32a7aae3d83b57567f9fd60a26257e992b79ba
pycparser==2.23 \
    --hash=sha256:e5c6e8d3fbad53479cab09ac03729e0a9faf2bee3db8208a550daf5af81a5934
REQ
"$RUNNER_TEMP/kel270-evidence-python/bin/python" -I -m pip install \
  --disable-pip-version-check --no-input --no-cache-dir \
  --only-binary=:all: --require-hashes \
  -r "$RUNNER_TEMP/kel270-evidence-requirements.txt"
"$RUNNER_TEMP/kel270-evidence-python/bin/python" -I -B -c \
  'import cryptography; assert cryptography.__version__ == "50.0.2"'
require_checkout_file "$verifier_path"
"$RUNNER_TEMP/kel270-evidence-python/bin/python" -I -B \
  docs/engineering/evidence/kel270-catalogue-admission-v2/verify_spec_evidence.py --check
`;

function assertEvidenceBeforeCandidateCode(steps: Record<string, any>[]) {
  expect(steps[0].uses).toMatch(/^actions\/checkout@[a-f0-9]{40}$/);
  expect(steps[0].with["persist-credentials"]).toBe(false);
  expect(steps[1].uses).toBe(
    "dtolnay/rust-toolchain@683b588c487a69d01f6c6ddf40f036ff682cc2c8",
  );
  expect(steps[1].if).toBeUndefined();
  expect(steps[1]["continue-on-error"]).toBeUndefined();
  expect(steps[2].name).toBe("KEL-270 definition evidence (read-only)");
  expect(steps[2].shell).toBe("bash");
  expect(steps[2].if).toBeUndefined();
  expect(steps[2]["continue-on-error"]).toBeUndefined();
  expect(steps[2].run).toContain(trustedKEL270ReceiptSha256);
  expect(steps[2].run).toContain("sha256sum --check --status");
  expect(steps[2].run).toContain('for variable in "${!PYTHON@}"; do');
  expect(steps[2].run).toContain("export PYTHONNOUSERSITE=1");
  expect(steps[2].run).toContain("export PYTHONSAFEPATH=1");
  expect(steps[2].run).toContain("python3 -I -S -c");
  expect(steps[2].run).toContain(trustedKEL270VerifierPath);
  expect(steps[2].run).toContain("verifier SHA-256 mismatch");
  expect(steps[2].run).toContain("python3.12 -I -m venv");
  expect(steps[2].run).toContain("verify_spec_evidence.py --check");
  expect(steps[2].run).toBe(approvedKEL270EvidenceRun);
}

// The shell body is not the first code that Bash executes: BASH_ENV, PATH,
// job selection and action inputs are independent preflight entry points.
// Keep the job/step context assertions separate from the exact shell oracle.
function assertKEL270HygieneContext(job: Record<string, any>) {
  expect(Object.keys(job).sort()).toEqual(
    ["name", "runs-on", "timeout-minutes", "needs", "if", "steps"].sort(),
  );
  expect(job.name).toBe("CODEOWNERS + templates");
  expect(job["runs-on"]).toBe("ubuntu-24.04");
  expect(job["timeout-minutes"]).toBe(10);
  expect(
    job.needs === "changes" ||
      (Array.isArray(job.needs) &&
        job.needs.length === 1 &&
        job.needs[0] === "changes"),
  ).toBe(true);
  expect(job.if).toBe(
    "needs.changes.outputs.hygiene == 'true' || needs.changes.outputs.docs == 'true'",
  );
  const steps = job.steps as Record<string, any>[];
  expect(steps.length).toBeGreaterThanOrEqual(3);
  expect(Object.keys(steps[0]).sort()).toEqual(["uses", "with"]);
  expect(steps[0].uses).toBe(
    "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1",
  );
  expect(steps[0].with).toEqual({
    "fetch-depth": 0,
    "persist-credentials": false,
  });
  expect(Object.keys(steps[1]).sort()).toEqual(["uses", "with"]);
  expect(steps[1].uses).toBe(
    "dtolnay/rust-toolchain@683b588c487a69d01f6c6ddf40f036ff682cc2c8",
  );
  expect(steps[1].with).toEqual({
    toolchain: "1.99.1",
    components: "rustfmt, clippy",
  });
  expect(Object.keys(steps[2]).sort()).toEqual(["name", "run", "shell"]);
  expect(steps[2].name).toBe("KEL-270 definition evidence (read-only)");
  expect(steps[2].shell).toBe("bash");
}

test("KEL-270 evidence executes before any candidate-controlled code", () => {
  const ci = Bun.YAML.parse(
    readFileSync(".github/workflows/ci.yml", "utf8"),
  ) as Record<string, any>;
  const steps = ci.jobs.hygiene.steps as Record<string, any>[];
  assertEvidenceBeforeCandidateCode(steps);
  assertKEL270HygieneContext(ci.jobs.hygiene);

  // An injected candidate-owned command before verification must fail the
  // ordering guard, even if that command later restores the verifier bytes.
  expect(() =>
    assertEvidenceBeforeCandidateCode([
      steps[0],
      steps[1],
      { name: "Untrusted transport import", run: "bun echo-codegen.ts" },
      ...steps.slice(2),
    ]),
  ).toThrow();
  expect(() =>
    assertEvidenceBeforeCandidateCode([
      steps[0],
      steps[1],
      { ...steps[2], if: "false" },
      ...steps.slice(3),
    ]),
  ).toThrow();
});

test("KEL-270 job and step startup overrides cannot bypass evidence", () => {
  const ci = Bun.YAML.parse(
    readFileSync(".github/workflows/ci.yml", "utf8"),
  ) as Record<string, any>;
  const pristine = ci.jobs.hygiene as Record<string, any>;
  assertKEL270HygieneContext(pristine);
  const run = (mutate: (job: Record<string, any>) => void) => {
    const job = structuredClone(pristine);
    mutate(job);
    expect(() => assertKEL270HygieneContext(job)).toThrow();
  };
  run((job) => {
    job.env = { BASH_ENV: "./tools/attacker-init.sh" };
  });
  run((job) => {
    job.env = { PATH: "attacker-bin" };
  });
  run((job) => {
    job.if = false;
  });
  run((job) => {
    job.needs = ["changes", "check"];
  });
  const equivalentDependency = structuredClone(pristine);
  equivalentDependency.needs = ["changes"];
  expect(() => assertKEL270HygieneContext(equivalentDependency)).not.toThrow();
  run((job) => {
    job["runs-on"] = "self-hosted";
  });
  run((job) => {
    job.steps[0].env = { BASH_ENV: "./tools/attacker-init.sh" };
  });
  run((job) => {
    job.steps[1].env = { BASH_ENV: "./tools/attacker-init.sh" };
  });
  run((job) => {
    job.steps[2].env = { BASH_ENV: "./tools/attacker-init.sh" };
  });
  run((job) => {
    job.steps[2]["working-directory"] = "docs";
  });
  run((job) => {
    job.steps[1].with.components = "rustfmt, clippy, rogue";
  });
  run((job) => {
    job.steps[0].uses = "actions/checkout@" + "1".repeat(40);
  });
});

const unixOnlyKEL270Test = test.skipIf(process.platform === "win32");
unixOnlyKEL270Test(
  "KEL-270 Bash startup negative control proves pre-body exit-zero",
  () => {
    const temporary = mkdtempSync(join(tmpdir(), "keld270-bash-env-"));
    try {
      const startup = join(temporary, "attacker-init.sh");
      writeFileSync(startup, 'echo "STARTUP_EXECUTED"; exit 0\n');
      const env = { ...process.env };
      delete env.BASH_ENV;
      const body = 'echo "EVIDENCE_BODY_EXECUTED"; exit 73';
      const invoke = (environment: Record<string, string | undefined>) =>
        Bun.spawnSync(
          ["bash", "--noprofile", "--norc", "-e", "-o", "pipefail", "-c", body],
          { cwd: temporary, env: environment },
        );
      const clean = invoke(env);
      expect(clean.exitCode).toBe(73);
      expect(new TextDecoder().decode(clean.stdout)).toContain(
        "EVIDENCE_BODY_EXECUTED",
      );
      const poisoned = invoke({ ...env, BASH_ENV: startup });
      expect(poisoned.exitCode).toBe(0);
      const output = new TextDecoder().decode(poisoned.stdout);
      expect(output).toContain("STARTUP_EXECUTED");
      expect(output).not.toContain("EVIDENCE_BODY_EXECUTED");
    } finally {
      rmSync(temporary, { recursive: true, force: true });
    }
  },
);

test("KEL-270 evidence rejects in-step untrusted execution and failure masking", () => {
  const ci = Bun.YAML.parse(
    readFileSync(".github/workflows/ci.yml", "utf8"),
  ) as Record<string, any>;
  const steps = ci.jobs.hygiene.steps as Record<string, any>[];
  const original = steps[2].run as string;
  for (const mutated of [
    original.replace(trustedKEL270ReceiptSha256, "0".repeat(64)),
    original.replace(safePythonEnvironment, ""),
    original.replace(approvedKEL270VerifierIntegrityCheck, ""),
    original.replace(
      "sha256sum --check --status",
      "sha256sum --check --status || true",
    ),
    original.replace(
      "export PYTHONSAFEPATH=1",
      "bash tools/untrusted.sh\nexport PYTHONSAFEPATH=1",
    ),
    original.replace(
      "verify_spec_evidence.py --check",
      "verify_spec_evidence.py --check || true",
    ),
  ]) {
    expect(mutated).not.toBe(original);
    expect(() =>
      assertEvidenceBeforeCandidateCode([
        steps[0],
        steps[1],
        { ...steps[2], run: mutated },
        ...steps.slice(3),
      ]),
    ).toThrow();
  }
});

const linuxTest = test.skipIf(process.platform !== "linux");
linuxTest("KEL-270 preflight rejects SHA, symlinks, and repins", () => {
  // This preflight targets the Ubuntu runner and uses GNU realpath.

  const temporary = mkdtempSync(join(tmpdir(), "keld270-checkout-binding-"));
  const receipt = JSON.parse(
    readFileSync(trustedKEL270ReceiptPath, "utf8"),
  ) as {
    published_file_sha256: Record<string, string>;
  };
  const git = (...args: string[]) => {
    const result = Bun.spawnSync(
      [
        "git",
        "-c",
        "core.autocrlf=false",
        "-c",
        "core.hooksPath=/dev/null",
        ...args,
      ],
      { cwd: temporary },
    );
    expect(result.exitCode).toBe(0);
    return new TextDecoder().decode(result.stdout).trim();
  };
  try {
    for (const name of [
      trustedKEL270ReceiptPath,
      ...Object.keys(receipt.published_file_sha256),
    ]) {
      const target = join(temporary, name);
      mkdirSync(dirname(target), { recursive: true });
      writeFileSync(target, readFileSync(name));
    }
    git("init", "-q", "--object-format=sha1");
    git("config", "maintenance.auto", "false");
    git("add", "-f", "--", "docs");
    git(
      "-c",
      "user.name=KEL-270 test",
      "-c",
      "user.email=kel270-test@example.invalid",
      "-c",
      "commit.gpgsign=false",
      "commit",
      "-qm",
      "Add pinned evidence fixture",
    );
    const head = git("rev-parse", "HEAD");
    expect(head).toMatch(/^[0-9a-f]{40}$/);
    expect(git("status", "--porcelain")).toBe("");

    const boundary = approvedKEL270EvidenceRun.indexOf(
      "\npython3.12 -I -m venv",
    );
    expect(boundary).toBeGreaterThan(0);
    const preflight = approvedKEL270EvidenceRun.slice(0, boundary);
    const run = (sha: string) =>
      Bun.spawnSync(["bash", "-c", preflight], {
        cwd: temporary,
        env: { ...process.env, GITHUB_WORKSPACE: temporary, GITHUB_SHA: sha },
      });
    expect(run(head).exitCode).toBe(0);
    const incorrect = run("0".repeat(40));
    expect(incorrect.exitCode).toBe(1);
    expect(new TextDecoder().decode(incorrect.stderr)).toContain(
      "KEL-270 B-82 checkout identity mismatch",
    );

    // A file or one of its ancestors can redirect Python's __file__ to a
    // different evidence/specification tree without changing reviewed bytes.
    // Exercise the two path guards independently: a full preflight refusal
    // alone could be preserved by the other guard after one was deleted.
    const bashBoundary = preflight.indexOf("\n# The receipt supplies");
    const sourceBoundary = preflight.indexOf(
      "# Bind all receipt-pinned sources to regular blobs",
    );
    expect(bashBoundary).toBeGreaterThan(0);
    expect(sourceBoundary).toBeGreaterThan(bashBoundary);
    const bashPathCheck =
      preflight.slice(0, bashBoundary) +
      `\nrequire_checkout_file "${trustedKEL270VerifierPath}"\n`;
    const sourcePathCheck =
      `set -euo pipefail\nreceipt_path=${trustedKEL270ReceiptPath}\n` +
      preflight.slice(sourceBoundary);
    const runPathCheck = (script: string) =>
      Bun.spawnSync(["bash", "-c", script], {
        cwd: temporary,
        env: {
          ...process.env,
          GITHUB_WORKSPACE: temporary,
          GITHUB_SHA: head,
        },
      });
    expect(runPathCheck(bashPathCheck).exitCode).toBe(0);
    expect(runPathCheck(sourcePathCheck).exitCode).toBe(0);

    const nested = join(temporary, "alternate-evidence-tree");
    mkdirSync(nested);
    cpSync(join(temporary, "docs"), join(nested, "docs"), {
      recursive: true,
    });
    const verifier = join(temporary, trustedKEL270VerifierPath);
    const verifierParent = dirname(verifier);
    const nestedVerifier = join(nested, trustedKEL270VerifierPath);
    const assertPinnedBytes = () => {
      expect(
        createHash("sha256")
          .update(readFileSync(join(temporary, trustedKEL270ReceiptPath)))
          .digest("hex"),
      ).toBe(trustedKEL270ReceiptSha256);
      expect(
        createHash("sha256").update(readFileSync(verifier)).digest("hex"),
      ).toBe(trustedKEL270VerifierSha256);
    };
    const assertRejected = (script: string, diagnostic: string) => {
      const result = runPathCheck(script);
      expect(result.exitCode).not.toBe(0);
      expect(new TextDecoder().decode(result.stderr)).toContain(diagnostic);
    };

    // The same verifier bytes through a symlink must not become executable
    // evidence just because Git hash-object and SHA-256 still match.
    rmSync(verifier);
    symlinkSync(nestedVerifier, verifier);
    assertPinnedBytes();
    assertRejected(bashPathCheck, "evidence file does not match checkout");
    assertRejected(preflight, "evidence file does not match checkout");
    assertRejected(
      sourcePathCheck,
      "evidence source outside canonical checkout",
    );
    rmSync(verifier);
    writeFileSync(verifier, readFileSync(nestedVerifier));

    // A symlinked ancestor of the verifier has the same failure mode.
    const originalVerifierParent = `${verifierParent}-original`;
    renameSync(verifierParent, originalVerifierParent);
    symlinkSync(dirname(nestedVerifier), verifierParent);
    assertPinnedBytes();
    assertRejected(bashPathCheck, "evidence file does not match checkout");
    assertRejected(preflight, "evidence file does not match checkout");
    assertRejected(
      sourcePathCheck,
      "evidence source outside canonical checkout",
    );
    rmSync(verifierParent);
    renameSync(originalVerifierParent, verifierParent);

    // A symlinked specification parent must also fail the source inventory,
    // even though the verifier itself is still a regular tracked blob.
    const specParent = join(
      temporary,
      "docs/specs/kel270-catalogue-admission-v2",
    );
    const originalSpecParent = `${specParent}-original`;
    renameSync(specParent, originalSpecParent);
    symlinkSync(
      join(nested, "docs/specs/kel270-catalogue-admission-v2"),
      specParent,
    );
    assertPinnedBytes();
    expect(runPathCheck(bashPathCheck).exitCode).toBe(0);
    assertRejected(preflight, "evidence source outside canonical checkout");
    assertRejected(
      sourcePathCheck,
      "evidence source outside canonical checkout",
    );
    rmSync(specParent);
    renameSync(originalSpecParent, specParent);

    // A source mutation that leaves HEAD and the reviewed receipt unchanged
    // must fail even when the candidate verifier would otherwise run.
    const identityName = "docs/specs/kel270-catalogue-admission-v2/identity.md";
    const identityTarget = join(temporary, identityName);
    const originalIdentity = readFileSync(identityTarget);
    writeFileSync(
      identityTarget,
      Buffer.concat([
        originalIdentity,
        Buffer.from("\n<!-- altered source -->\n"),
      ]),
    );
    const sourceMutant = run(head);
    expect(sourceMutant.exitCode).not.toBe(0);
    expect(new TextDecoder().decode(sourceMutant.stderr)).toContain(
      "KEL-270 B-82 evidence source does not match checkout",
    );
    writeFileSync(identityTarget, originalIdentity);
    expect(run(head).exitCode).toBe(0);

    // A malicious commit can update the Git tree, source, verifier and
    // manifest together. It still cannot change the trusted workflow pin.
    const receiptTarget = join(temporary, trustedKEL270ReceiptPath);
    const repinned = JSON.parse(readFileSync(receiptTarget, "utf8")) as {
      published_file_sha256: Record<string, string>;
    };
    const attackedIdentity = Buffer.concat([
      originalIdentity,
      Buffer.from("\n<!-- attacker-controlled policy -->\n"),
    ]);
    const noOpVerifier = Buffer.from(
      '#!/usr/bin/env python3\nprint("NOOP VERIFIER: PASS")\n',
    );
    writeFileSync(identityTarget, attackedIdentity);
    writeFileSync(verifier, noOpVerifier);
    repinned.published_file_sha256[identityName] = createHash("sha256")
      .update(attackedIdentity)
      .digest("hex");
    repinned.published_file_sha256[trustedKEL270VerifierPath] = createHash(
      "sha256",
    )
      .update(noOpVerifier)
      .digest("hex");
    writeFileSync(receiptTarget, JSON.stringify(repinned, null, 2) + "\n");
    git("add", "-f", "--", "docs");
    git(
      "-c",
      "user.name=KEL-270 test",
      "-c",
      "user.email=kel270-test@example.invalid",
      "-c",
      "commit.gpgsign=false",
      "commit",
      "-qm",
      "Repin changed source and replaced verifier",
    );
    const repinnedHead = git("rev-parse", "HEAD");
    expect(repinnedHead).not.toBe(head);
    const repinnedMutant = run(repinnedHead);
    expect(repinnedMutant.exitCode).not.toBe(0);
    expect(new TextDecoder().decode(repinnedMutant.stderr)).toContain(
      "KEL-270 B-82 trusted receipt SHA-256 mismatch",
    );
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test("KEL-270 gate authenticates the verifier before executing candidate code", () => {
  if (process.platform === "win32") return;

  const receiptBytes = readFileSync(trustedKEL270ReceiptPath);
  const receipt = JSON.parse(receiptBytes.toString("utf8")) as {
    published_file_sha256: Record<string, string>;
  };
  expect(receipt.published_file_sha256[trustedKEL270VerifierPath]).toBe(
    trustedKEL270VerifierSha256,
  );

  const temporary = mkdtempSync(join(tmpdir(), "keld270-verifier-pin-"));
  const evidenceDirectory = join(temporary, dirname(trustedKEL270VerifierPath));
  const receiptTarget = join(temporary, trustedKEL270ReceiptPath);
  const verifierTarget = join(temporary, trustedKEL270VerifierPath);
  const identityTarget = join(
    temporary,
    "docs/specs/kel270-catalogue-admission-v2/identity.md",
  );
  try {
    mkdirSync(evidenceDirectory, { recursive: true });
    mkdirSync(dirname(identityTarget), { recursive: true });
    writeFileSync(receiptTarget, receiptBytes);
    writeFileSync(verifierTarget, readFileSync(trustedKEL270VerifierPath));
    writeFileSync(identityTarget, "original reviewed specification\n");

    const check = [
      "set -euo pipefail",
      `receipt_path=${trustedKEL270ReceiptPath}`,
      approvedKEL270VerifierIntegrityCheck,
    ].join("\n");
    const valid = Bun.spawnSync(["bash", "-c", check], { cwd: temporary });
    expect(valid.exitCode).toBe(0);

    // Reproduce the CodeRabbit attack: keep the pinned receipt unchanged,
    // mutate a source and replace the verifier with a successful no-op.
    writeFileSync(identityTarget, "attacker-modified specification\n");
    writeFileSync(
      verifierTarget,
      '#!/usr/bin/env python3\nprint("NOOP VERIFIER: PASS")\n',
    );
    const mutant = Bun.spawnSync(["bash", "-c", check], { cwd: temporary });
    expect(mutant.exitCode).not.toBe(0);
    expect(new TextDecoder().decode(mutant.stderr)).toContain(
      "KEL-270 B-82 verifier SHA-256 mismatch",
    );
    expect(
      createHash("sha256").update(readFileSync(receiptTarget)).digest("hex"),
    ).toBe(trustedKEL270ReceiptSha256);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test("KEL-270 trusted receipt pin rejects candidate-rewritten source hashes", () => {
  const receiptBytes = readFileSync(trustedKEL270ReceiptPath);
  expect(createHash("sha256").update(receiptBytes).digest("hex")).toBe(
    trustedKEL270ReceiptSha256,
  );

  // Model the reproduced attack without changing the checked-out source tree:
  // mutate identity.md, then repin that inner digest in the candidate receipt.
  const receipt = JSON.parse(receiptBytes.toString("utf8")) as {
    published_file_sha256: Record<string, string>;
  };
  const sourcePath = "docs/specs/kel270-catalogue-admission-v2/identity.md";
  const mutatedSource = Buffer.concat([
    readFileSync(sourcePath),
    Buffer.from("\n<!-- adversarial source mutation -->\n"),
  ]);
  receipt.published_file_sha256[sourcePath] = createHash("sha256")
    .update(mutatedSource)
    .digest("hex");
  const candidateReceipt = Buffer.from(`${JSON.stringify(receipt, null, 2)}\n`);
  expect(createHash("sha256").update(candidateReceipt).digest("hex")).not.toBe(
    trustedKEL270ReceiptSha256,
  );
});

test("KEL-270 verifier children cannot import sitecustomize from inherited PYTHONPATH", () => {
  // GitHub Actions runs this verifier on Ubuntu. Keep the bash/Python startup
  // regression off Windows jobs, where the required shell is not installed.
  if (process.platform === "win32") return;

  const temporary = mkdtempSync(join(tmpdir(), "keld270-python-path-"));
  try {
    const marker = join(temporary, "sitecustomize-ran");
    writeFileSync(
      join(temporary, "sitecustomize.py"),
      `from pathlib import Path\nPath(${JSON.stringify(marker)}).write_text("executed")\n`,
    );
    const pythonEnvironment = {
      ...process.env,
      PYTHONPATH: temporary,
      PYTHONSAFEPATH: "1",
    };
    delete pythonEnvironment.PYTHONHOME;

    const vulnerable = Bun.spawnSync(["python3", "-B", "-c", "pass"], {
      env: pythonEnvironment,
    });
    expect(vulnerable.exitCode).toBe(0);
    expect(existsSync(marker)).toBe(true);
    rmSync(marker, { force: true });

    const isolated = Bun.spawnSync(
      ["bash", "-c", `${safePythonEnvironment}\npython3 -B -c 'pass'`],
      { env: pythonEnvironment },
    );
    expect(isolated.exitCode).toBe(0);
    expect(existsSync(marker)).toBe(false);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

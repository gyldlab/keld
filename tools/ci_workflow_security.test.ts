import "./ci_provenance.test";
import "./ci_verisentry_publisher.test";
import "./ci_verisentry_refresh.test";
import { expect, test } from "bun:test";
import {
  cpSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, dirname, join } from "node:path";
import {
  aptStepTimeoutMinutes,
  cargoDenyArchiveSha256,
  cargoDenyChecks,
  cargoDenyInstallStep,
  cargoDenyVersion,
  webkitgtkAptJobs,
  webkitgtkDebCachePath,
  checkPullRequestTargetWorkflow,
  checkWindowsMediaOracle,
  checkWorkflowSecurity,
  checkRustToolchainPins,
  codeqlLanguages,
  codeqlRoute,
} from "./ci_workflow_security";

type Step = Record<string, unknown>;

// Thirteen native Bun and six Git children share this fixture's kill switch.
// The oracle is their effects and exit status, with no latency requirement.
test("direct native formatter selects JS/TS, honors ignores and checks without writes", () => {
  const root = mkdtempSync(join(tmpdir(), "keld-native-fmt-"));
  try {
    cpSync(join(import.meta.dir, "../bunfig.toml"), join(root, "bunfig.toml"));
    expect(
      Bun.spawnSync(["git", "init", "--quiet"], { cwd: root }).exitCode,
    ).toBe(0);
    expect(
      Bun.spawnSync(["git", "config", "maintenance.auto", "false"], {
        cwd: root,
      }).exitCode,
    ).toBe(0);
    const original = "export const value={answer:42};\n";
    const sources = ["js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts"].map(
      (extension) => `source.${extension}`,
    );
    sources.push("source with spaces.ts", "--ignore-pattern=source.ts");
    const untouched = [
      "source.rs",
      "document.md",
      "page.html",
      "data.json",
      "data.yaml",
      "ignored.ts",
    ];
    for (const name of [...sources, ...untouched])
      writeFileSync(join(root, name), original);
    // Track a source and an ignored source; all the remaining sources stay untracked.
    expect(
      Bun.spawnSync(["git", "add", "source.ts", "ignored.ts"], { cwd: root })
        .exitCode,
    ).toBe(0);
    writeFileSync(join(root, ".gitignore"), "ignored.ts\nnode_modules/\n");
    mkdirSync(join(root, "node_modules"));
    writeFileSync(join(root, "node_modules/invalid.ts"), "invalid {{{");
    // A same-named script must never take over this native formatting command.
    const packageBytes = JSON.stringify({ scripts: { fmt: "exit 99" } });
    writeFileSync(join(root, "package.json"), packageBytes);
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
    const listing = native("--list-files");
    expect(listing.exitCode).toBe(0);
    expect(listing.stdout.toString().trim().split(/\r?\n/).sort()).toEqual(
      [...sources].sort(),
    );
    expect(native("--check").exitCode).toBe(1);
    for (const name of [...sources, ...untouched])
      expect(readFileSync(join(root, name), "utf8")).toBe(original);
    expect(native().exitCode).toBe(0);
    const formatted = sources.map((name) => readFileSync(join(root, name)));
    for (const bytes of formatted) expect(bytes.toString()).not.toBe(original);
    expect(native("--check").exitCode).toBe(0);
    expect(native().exitCode).toBe(0);
    sources.forEach((name, index) =>
      expect(readFileSync(join(root, name))).toEqual(formatted[index]),
    );
    for (const name of untouched)
      expect(readFileSync(join(root, name), "utf8")).toBe(original);
    expect(readFileSync(join(root, "package.json"), "utf8")).toBe(packageBytes);
    // An ignore-only edit can expose an existing tracked, unformatted source.
    writeFileSync(join(root, ".gitignore"), "node_modules/\n");
    expect(native("--check").exitCode).toBe(1);
    expect(readFileSync(join(root, "ignored.ts"), "utf8")).toBe(original);
    expect(native().exitCode).toBe(0);
    expect(native("--check").exitCode).toBe(0);
    const nestedIgnore = "crates/keld-cli/templates/hello/.gitignore";
    const nestedSource = "crates/keld-cli/templates/hello/keld.config.ts";
    mkdirSync(dirname(join(root, nestedSource)), { recursive: true });
    const templateIgnore = readFileSync(
      join(import.meta.dir, "../", nestedIgnore),
      "utf8",
    );
    writeFileSync(join(root, nestedSource), original);
    writeFileSync(
      join(root, nestedIgnore),
      templateIgnore + "\nkeld.config.ts\n",
    );
    expect(
      Bun.spawnSync(["git", "add", "-f", nestedSource], { cwd: root }).exitCode,
    ).toBe(0);
    expect(Bun.spawnSync(["git", "add", "."], { cwd: root }).exitCode).toBe(0);
    expect(native("--check").exitCode).toBe(0);
    // Only the real nested ignore path changes; the tracked TS bytes do not.
    writeFileSync(join(root, nestedIgnore), templateIgnore);
    const changed = Bun.spawnSync(["git", "diff", "--name-only"], {
      cwd: root,
    });
    expect(changed.exitCode).toBe(0);
    expect(changed.stdout.toString().trim()).toBe(nestedIgnore);
    expect(native("--check").exitCode).toBe(1);
    expect(readFileSync(join(root, nestedSource), "utf8")).toBe(original);
    expect(native().exitCode).toBe(0);
    expect(native("--check").exitCode).toBe(0);
    const invalid = "export const broken={{{\n";
    writeFileSync(join(root, "syntax-error.ts"), invalid);
    expect(native("--check").exitCode).toBe(2);
    expect(readFileSync(join(root, "syntax-error.ts"), "utf8")).toBe(invalid);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}, 30_000);

test("native configuration alone can reject unchanged TypeScript", () => {
  const cases = [
    {
      path: "packages/@keld/api/package.json",
      before: "{}\n",
      after: '{"prettier":{"semi":false}}\n',
    },
    {
      path: "crates/keld-cli/templates/hello/package.json",
      before: "{}\n",
      after: '{"prettier":{"semi":false}}\n',
    },
    {
      path: "crates/keld-cli/templates/hello/.editorconfig",
      before: "[*]\nindent_size=2\n",
      after: "[*]\nindent_size=4\n",
    },
    {
      path: "crates/keld-cli/templates/hello/.prettierrc",
      before: "{}\n",
      after: '{"semi":false}\n',
    },
    {
      path: "crates/keld-cli/templates/hello/.oxfmtrc.jsonc",
      before: "{}\n",
      after: '{"semi":false}\n',
    },
    {
      path: "crates/keld-cli/templates/hello/package.yaml",
      before: "{}\n",
      after: "prettier:\n  semi: false\n",
    },
    {
      path: ".prettierignore",
      before: "packages/probe/src/value.ts\n",
      after: "\n",
    },
  ];
  // Windows resolves config basenames without regard to ASCII case.
  if (process.platform === "win32")
    cases.push(
      {
        path: "crates/keld-cli/templates/hello/.PrettierRC",
        before: "{}\n",
        after: '{"semi":false}\n',
      },
      { path: ".GITIGNORE", before: "src/value.ts\n", after: "\n" },
      {
        path: ".PRETTIERIGNORE",
        before: "packages/probe/src/value.ts\n",
        after: "\n",
      },
    );
  for (const config of cases) {
    const root = mkdtempSync(join(tmpdir(), "keld-native-config-"));
    try {
      const prettierIgnore = config.path.toLowerCase() === ".prettierignore";
      const ignored =
        prettierIgnore || config.path.toLowerCase() === ".gitignore";
      const source = prettierIgnore
        ? "packages/probe/src/value.ts"
        : join(dirname(config.path), "src/value.ts").replaceAll("\\", "/");
      mkdirSync(dirname(join(root, source)), { recursive: true });
      const bytes = ignored
        ? "export const answer={value:42};\n"
        : "export const answer = {\n  value: 42,\n};\n";
      writeFileSync(join(root, source), bytes);
      cpSync(
        join(import.meta.dir, "../bunfig.toml"),
        join(root, "bunfig.toml"),
      );
      writeFileSync(join(root, config.path), config.before);
      for (const args of [
        ["init", "--quiet"],
        ["config", "maintenance.auto", "false"],
        ["add", "-f", "."],
      ])
        expect(Bun.spawnSync(["git", ...args], { cwd: root }).exitCode).toBe(0);
      const check = () =>
        Bun.spawnSync(
          [
            process.execPath,
            "fmt",
            "--no-script",
            "--check",
            "**/*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}",
          ],
          { cwd: root },
        );
      expect(check().exitCode, config.path).toBe(0);
      writeFileSync(join(root, config.path), config.after);
      const changed = Bun.spawnSync(["git", "diff", "--name-only"], {
        cwd: root,
      });
      expect(changed.exitCode).toBe(0);
      expect(changed.stdout.toString().trim()).toBe(config.path);
      const rejected = check();
      expect(rejected.exitCode, config.path).toBe(1);
      expect(rejected.stdout.toString(), config.path).toContain(source);
      expect(readFileSync(join(root, source), "utf8")).toBe(bytes);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }
}, 30_000);

type FixtureJob = {
  steps: Step[];
  defaults?: { run: { shell?: string; "working-directory"?: string } };
  env?: Record<string, unknown>;
  permissions?: Record<string, unknown>;
  strategy?: { matrix: Record<string, unknown> };
  needs?: unknown;
  if?: unknown;
  "runs-on"?: unknown;
  "continue-on-error"?: unknown;
  "timeout-minutes"?: unknown;
};
type Fixture = {
  jobs: Record<string, FixtureJob>;
  on?: Record<string, unknown>;
  permissions?: Record<string, unknown>;
  env?: Record<string, unknown>;
  defaults?: { run: { shell?: string; "working-directory"?: string } };
};
const source = readFileSync(
  join(import.meta.dir, "../.github/workflows/ci.yml"),
  "utf8",
);
const windowsMediaOracle = readFileSync(
  join(import.meta.dir, "../crates/keld-wv/tests/windows_media_guard.ps1"),
);
const selectedRustToolchain = readFileSync(
  join(import.meta.dir, "../rust-toolchain.toml"),
  "utf8",
);
// Both versions come from the live pin, so a toolchain bump never turns a
// mutation below into a silent no-op.
const selectedRust = String(
  (Bun.TOML.parse(selectedRustToolchain) as { toolchain: { channel: string } })
    .toolchain.channel,
);
const staleRust = selectedRust.replace(
  /\.(\d+)$/,
  (_, patch: string) => `.${Number(patch) + 1}`,
);
if (!/^\d+\.\d+\.\d+$/.test(selectedRust) || staleRust === selectedRust) {
  throw new Error(
    `rust-toolchain.toml must pin an exact X.Y.Z channel, got ${selectedRust}`,
  );
}
const staleRustToolchain = selectedRustToolchain.replace(
  `"${selectedRust}"`,
  `"${staleRust}"`,
);
if (staleRustToolchain === selectedRustToolchain)
  throw new Error("stale-pin mutation did not apply");
const checkout = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1";
const requiredRustInstallerJobs = [
  "changes",
  "router-contracts",
  "fmt",
  "check",
  "windows-media",
  "doctest",
  "linux-gui-smoke",
  "msrv",
  "deny",
  "hygiene",
  "mermaid",
  "codeql-rust",
];

function fixture(): Fixture {
  checkWorkflowSecurity(source);
  return Bun.YAML.parse(source) as Fixture;
}

function step(f: Fixture, job: string, name: string): Step {
  const found = f.jobs[job]!.steps.find((candidate) => candidate.name === name);
  if (!found) throw new Error(`missing fixture step: ${name}`);
  return found;
}

function check(f: Fixture): void {
  checkWorkflowSecurity(Bun.YAML.stringify(f, null, 2));
}

test("actual workflow passes parsed security admission", () => {
  expect(() => checkWorkflowSecurity(source)).not.toThrow();
});

// GH1301: this checks the shipping workflow, not a copied example or fixture.
// A real cold hosted API suite failed because it ran before KIPC's compiler was
// installed; a Rust-hosted Bun corpus independently failed to import @keld/kipc.
test("frozen SDK prerequisites precede both TypeScript and Rust-hosted Bun tests", () => {
  const workflow = fixture();
  const run = step(
    workflow,
    "bun-test",
    "frozen install, typecheck, and bun test",
  ).run!;
  const phases = run.split("for package in $KELD_CI_TS_PACKAGES; do");
  expect(phases).toHaveLength(3);
  expect(phases[1]).toContain("bun install --frozen-lockfile");
  expect(phases[1]).not.toContain("bun test");
  expect(phases[1]).not.toContain("bun run typecheck");
  expect(phases[2]).toContain("bun run typecheck");
  expect(phases[2]).toContain("bun test");
  expect(phases[2]).not.toContain("bun install --frozen-lockfile");

  const nativeSteps = workflow.jobs.check!.steps;
  const setupIndex = nativeSteps.findIndex((candidate) =>
    candidate.uses?.startsWith("oven-sh/setup-bun@"),
  );
  const dependencyIndex = nativeSteps.findIndex(
    (candidate) =>
      candidate.name ===
      "Prepare frozen SDK fixtures for Rust-hosted Bun corpus",
  );
  const clippyIndex = nativeSteps.findIndex(
    (candidate) => candidate.name === "clippy (warnings deny)",
  );
  expect(setupIndex).toBeGreaterThanOrEqual(0);
  expect(dependencyIndex).toBeGreaterThan(setupIndex);
  expect(clippyIndex).toBeGreaterThan(dependencyIndex);
  const dependency = nativeSteps[dependencyIndex]!;
  expect(dependency.run).toContain("packages/@keld/kipc");
  expect(dependency.run).toContain("packages/@keld/api");
  expect(dependency.run).toContain("packages/@keld/electron");
  expect(dependency.run).toContain("bun install --frozen-lockfile");
  expect(dependency["continue-on-error"]).toBeUndefined();
});

test("cold Bun dependency setup cannot be echoed, removed or reordered", () => {
  const echoOnly = fixture();
  const bun = step(
    echoOnly,
    "bun-test",
    "frozen install, typecheck, and bun test",
  );
  bun.run = bun.run!.replace(
    "bun install --frozen-lockfile",
    "echo bun install --frozen-lockfile",
  );
  expect(() => check(echoOnly)).toThrow(
    "Bun test reviewed fixture install/order",
  );

  const missingNative = fixture();
  const nativeSteps = missingNative.jobs.check!.steps;
  const installIndex = nativeSteps.findIndex(
    (candidate) =>
      candidate.name ===
      "Prepare frozen SDK fixtures for Rust-hosted Bun corpus",
  );
  expect(installIndex).toBeGreaterThanOrEqual(0);
  nativeSteps.splice(installIndex, 1);
  expect(() => check(missingNative)).toThrow(
    "native check reviewed step inventory/order",
  );

  const reordered = fixture();
  const run = step(
    reordered,
    "bun-test",
    "frozen install, typecheck, and bun test",
  );
  const phaseBoundary = "done\nfor package in $KELD_CI_TS_PACKAGES; do";
  expect(run.run).toContain(phaseBoundary);
  run.run = run.run!.replace(
    phaseBoundary,
    "bun test\ndone\nfor package in $KELD_CI_TS_PACKAGES; do",
  );
  expect(() => check(reordered)).toThrow(
    "Bun test reviewed fixture install/order",
  );
});

test("Linux and Windows native acceptance tests print cargo failure output", () => {
  const f = fixture();
  const bash =
    process.platform === "win32"
      ? "C:\\Program Files\\Git\\bin\\bash.exe"
      : "bash";
  for (const name of [
    "Real Linux foreign-owner refusal (KEL-167)",
    "Real Windows release host identity container round trip (KEL-19)",
    "Real Windows release updater helper loader and container (KEL-53 T4d S9c)",
  ]) {
    const selected = step(f, "check", name);
    if (typeof selected.run !== "string")
      throw new Error(`${name}: missing script`);
    const mock =
      "cargo() { if [ \"$1\" = test ]; then printf 'cargo failure diagnostic: fixture\\n' >&2; return 17; fi; return 0; }\n";
    const result = Bun.spawnSync([bash, "-c", mock + selected.run], {
      env: { ...process.env, RUNNER_TEMP: tmpdir() },
    });
    expect(result.exitCode, name).not.toBe(0);
    expect(new TextDecoder().decode(result.stdout), name).toContain(
      "cargo failure diagnostic: fixture",
    );
  }
});
test("KEL-167 Linux exact test rejects a zero-test result", () => {
  const linux = step(
    fixture(),
    "check",
    "Real Linux foreign-owner refusal (KEL-167)",
  );
  expect(linux.shell).toBe("bash");
  if (typeof linux.run !== "string") throw new Error("KEL-167 script missing");
  expect(linux.run).toContain(
    "real_linux_second_account_refuses_foreign_owned_project",
  );
  const bash =
    process.platform === "win32"
      ? "C:\\Program Files\\Git\\bin\\bash.exe"
      : "bash";
  const script =
    'cargo() { printf \'%s\\n\' "$KELD_MOCK_CARGO_STDOUT"; return "$KELD_MOCK_CARGO_EXIT"; }\n' +
    linux.run;
  const run = (stdout: string, exitCode = 0) =>
    Bun.spawnSync([bash, "-c", script], {
      env: {
        ...process.env,
        KELD_MOCK_CARGO_STDOUT: stdout,
        KELD_MOCK_CARGO_EXIT: String(exitCode),
      },
    });
  expect(
    run("test result: ok. 0 passed; 0 failed; 1 filtered out").exitCode,
  ).not.toBe(0);
  expect(run("test result: ok. 1 passed; 0 failed").exitCode).toBe(0);
  expect(run("test result: FAILED. 0 passed; 1 failed", 101).exitCode).not.toBe(
    0,
  );
});

test("KEL-167 Linux proof cannot be skipped or replaced while CI remains green", () => {
  const stepName = "Real Linux foreign-owner refusal (KEL-167)";
  const mutations: Array<[string, (selected: Step) => void]> = [
    [
      "skip the security proof",
      (selected) => {
        selected.if = false;
      },
    ],
    [
      "replace the exact test selector",
      (selected) => {
        selected.run = String(selected.run).replace(
          "real_linux_second_account_refuses_foreign_owned_project",
          "not_a_real_linux_security_test",
        );
      },
    ],
    [
      "remove the one-pass assertion",
      (selected) => {
        selected.run = String(selected.run).replace(
          "grep -q 'test result: ok\\. 1 passed;' <<<\"$out\"",
          "true",
        );
      },
    ],
    [
      "disable the real-OS proof environment",
      (selected) => {
        (selected.env as Step).KELD_REAL_LINUX_OWNER_PROOF = "0";
      },
    ],
    [
      "suppress the script with a custom shell",
      (selected) => {
        selected.shell = "bash -c true {0}";
      },
    ],
  ];
  for (const [label, mutate] of mutations) {
    const f = fixture();
    mutate(step(f, "check", stepName));
    expect(() => check(f), label).toThrow("KEL-167");
  }
});

test("router contract regression runs independently without releasing its required gate", () => {
  const workflow = fixture();
  const job = workflow.jobs["router-contracts"];
  expect(job).toBeDefined();
  expect(
    job!.steps.some(
      (candidate) =>
        candidate.name === "Router contract tests" &&
        candidate.run === "tools/ci_changes_test.sh",
    ),
  ).toBe(true);
  expect(
    workflow.jobs.changes!.steps.some(
      (candidate) => candidate.name === "Router contract tests",
    ),
  ).toBe(false);
  expect(workflow.jobs.required!.needs).toContain("router-contracts");
});

test("independent router regression job is unconditional, exact and required", () => {
  for (const [name, mutate] of [
    [
      "absent job",
      (f: Fixture) => {
        delete f.jobs["router-contracts"];
      },
    ],
    [
      "job skipped",
      (f: Fixture) => {
        f.jobs["router-contracts"]!.if = false;
      },
    ],
    [
      "job queued behind classifier",
      (f: Fixture) => {
        f.jobs["router-contracts"]!.needs = "changes";
      },
    ],
    [
      "missing test",
      (f: Fixture) => {
        f.jobs["router-contracts"]!.steps.pop();
      },
    ],
    [
      "duplicate test",
      (f: Fixture) => {
        const steps = f.jobs["router-contracts"]!.steps;
        steps.push(structuredClone(steps.at(-1)!));
      },
    ],
    [
      "echo-only test",
      (f: Fixture) => {
        f.jobs["router-contracts"]!.steps.at(-1)!.run =
          "echo tools/ci_changes_test.sh";
      },
    ],
    [
      "conditional test",
      (f: Fixture) => {
        f.jobs["router-contracts"]!.steps.at(-1)!.if = false;
      },
    ],
    [
      "partial history",
      (f: Fixture) => {
        (f.jobs["router-contracts"]!.steps[0]!.with as Record<string, unknown>)[
          "fetch-depth"
        ] = 1;
      },
    ],
    [
      "suite back in blocking router",
      (f: Fixture) => {
        f.jobs.changes!.steps.splice(2, 0, {
          name: "Router contract tests",
          run: "tools/ci_changes_test.sh",
        });
      },
    ],
    [
      "required needs missing",
      (f: Fixture) => {
        f.jobs.required!.needs = (f.jobs.required!.needs as string[]).filter(
          (name) => name !== "router-contracts",
        );
      },
    ],
    [
      "result handoff changed",
      (f: Fixture) => {
        (
          step(f, "required", "Verify required CI results").env as Record<
            string,
            unknown
          >
        ).KELD_RESULT_ROUTER_CONTRACTS = "success";
      },
    ],
    [
      "result argument missing",
      (f: Fixture) => {
        const evaluator = step(f, "required", "Verify required CI results");
        evaluator.run = String(evaluator.run).replace(
          '"$KELD_RESULT_ROUTER_CONTRACTS"',
          "",
        );
      },
    ],
  ] as const) {
    const altered = fixture();
    mutate(altered);
    expect(() => check(altered), name).toThrow();
  }
});

test("CI retains main push and every pull request without trigger filters", () => {
  const mutants: Array<[string, (f: Fixture) => void]> = [
    [
      "missing push",
      (f) => {
        delete f.on!.push;
      },
    ],
    [
      "wrong push branch",
      (f) => {
        f.on!.push = { branches: ["no-ci-push"] };
      },
    ],
    [
      "limited push path",
      (f) => {
        f.on!.push = { branches: ["main"], paths: ["docs/**"] };
      },
    ],
    [
      "missing pull request",
      (f) => {
        delete f.on!.pull_request;
      },
    ],
    [
      "limited pull request",
      (f) => {
        f.on!.pull_request = { paths: ["docs/**"] };
      },
    ],
    [
      "unexpected privileged trigger",
      (f) => {
        f.on!.pull_request_target = null;
      },
    ],
  ];
  for (const [name, mutate] of mutants) {
    const f = fixture();
    mutate(f);
    expect(() => check(f), name).toThrow(
      "CI workflow must run on main pushes and all pull requests",
    );
  }
});

test("CI workflow-wide token privileges stay read-only", () => {
  const mutants: Array<[string, (f: Fixture) => void]> = [
    [
      "missing permissions",
      (f) => {
        delete f.permissions;
      },
    ],
    [
      "contents write",
      (f) => {
        f.permissions!.contents = "write";
      },
    ],
    [
      "extra actions write",
      (f) => {
        f.permissions!.actions = "write";
      },
    ],
    [
      "extra security events write",
      (f) => {
        f.permissions!["security-events"] = "write";
      },
    ],
  ];
  for (const [name, mutate] of mutants) {
    const f = fixture();
    mutate(f);
    expect(() => check(f), name).toThrow(
      "CI workflow permissions must remain read-only",
    );
  }
});

test("non-CodeQL jobs cannot override workflow read-only token privileges", () => {
  for (const jobName of [
    "check",
    "router-contracts",
    "windows-media",
    "required",
    "deny",
  ]) {
    const f = fixture();
    f.jobs[jobName]!.permissions = { contents: "write", actions: "write" };
    expect(() => check(f), jobName).toThrow(
      "non-CodeQL jobs must inherit read-only workflow permissions",
    );
  }
});

test("native CI jobs reject unreviewed container and services overrides", () => {
  for (const jobName of [
    "check",
    "windows-media",
    "router-contracts",
  ] as const) {
    for (const [key, value] of [
      ["container", "rust:latest"],
      ["services", { override: { image: "alpine:latest" } }],
    ] as const) {
      const f = fixture();
      (f.jobs[jobName] as Record<string, unknown>)[key] = value;
      expect(() => check(f), `${jobName}.${key}`).toThrow(
        `jobs.${jobName} must not set container or services`,
      );
    }
  }
});
test("native Clippy and Nextest matrix must use every router-selected OS", () => {
  const baseline = fixture();
  expect(() => check(baseline)).not.toThrow();
  for (const mutate of [
    (f: Fixture) => {
      f.jobs.check!.strategy = {
        "fail-fast": false,
        matrix: { os: ["windows-latest"] },
      } as FixtureJob["strategy"];
    },
    (f: Fixture) => {
      f.jobs.check!.strategy = {
        "fail-fast": false,
        matrix: {
          os: "${{ fromJSON(needs.changes.outputs.check_os) }}",
          exclude: [{ os: "macos-latest" }],
        },
      } as FixtureJob["strategy"];
    },
    (f: Fixture) => {
      f.jobs.check!.strategy = {
        matrix: { os: "${{ fromJSON(needs.changes.outputs.check_os) }}" },
      };
    },
    (f: Fixture) => {
      f.jobs.check!["runs-on"] = "windows-latest";
    },
  ]) {
    const f = fixture();
    mutate(f);
    expect(() => check(f)).toThrow(
      "native check must use the router-owned OS matrix",
    );
  }
});

test("change router output bindings cannot forge native OS or selected test inventory", () => {
  for (const [name, spoof] of [
    ["check_os", '["windows-latest"]'],
    ["rust_documentation_only", "true"],
    ["rust", "false"],
    ["packages", "keld-wv"],
    ["codeql_rust", "false"],
    ["deny", "false"],
  ]) {
    const f = fixture();
    const output = (f.jobs.changes as Step).outputs as Record<string, unknown>;
    output[name!] = spoof;
    expect(() => check(f), name).toThrow(
      "change router outputs must match the trusted classifier bindings",
    );
  }
  const added = fixture();
  ((added.jobs.changes as Step).outputs as Record<string, unknown>).shadow =
    "false";
  expect(() => check(added)).toThrow(
    "change router outputs must match the trusted classifier bindings",
  );
});

test("change router classifier must execute its exact authoritative script", () => {
  for (const mutate of [
    (f: Fixture) => {
      step(f, "changes", "Classify changed-path ownership").run =
        'echo \'check_os=["windows-latest"]\' >> "$GITHUB_OUTPUT"';
    },
    (f: Fixture) => {
      step(f, "changes", "Classify changed-path ownership").id = "fake";
    },
    (f: Fixture) => {
      step(f, "changes", "Classify changed-path ownership").env = {
        KELD_CI_EVENT_NAME: "push",
      };
    },
    (f: Fixture) => {
      step(f, "changes", "Classify changed-path ownership").if = "false";
    },
    (f: Fixture) => {
      f.jobs.changes!.steps.unshift({
        id: "classify",
        run: "echo 'rust=false' >> \"$GITHUB_OUTPUT\"",
      });
    },
  ]) {
    const f = fixture();
    mutate(f);
    expect(() => check(f)).toThrow(
      "change router classifier must run the reviewed command and source",
    );
  }
});

test("router executable steps cannot rewrite the classifier script before it runs", () => {
  for (const mutate of [
    (f: Fixture) => {
      const prior = step(f, "changes", "Source budget tool tests");
      prior.run =
        String(prior.run) + '\nprintf "rust=false\\n" > tools/ci_changes.sh\n';
    },
    (f: Fixture) => {
      f.jobs.changes!.steps.splice(6, 0, {
        name: "Prepare router",
        run: 'echo "rust=false" > tools/ci_changes.sh',
      });
    },
    (f: Fixture) => {
      (f.jobs.changes as Step)["if"] = "false";
    },
    (f: Fixture) => {
      (f.jobs.changes as Step)["runs-on"] = "windows-latest";
    },
  ]) {
    const f = fixture();
    mutate(f);
    expect(() => check(f)).toThrow(
      "change router job must preserve its reviewed executable steps and context",
    );
  }
});

test("CI rejects unreviewed external actions, including mutable Docker image tags", () => {
  const unreviewed = [
    "docker://alpine:latest",
    `docker://ghcr.io/example/unknown@sha256:${"0".repeat(64)}`,
    `unreviewed/third-party-action@${"0".repeat(40)}`,
    `oven-sh/setup-bun@${"0".repeat(40)}`,
  ];
  for (const uses of unreviewed) {
    const f = fixture();
    // The CodeQL JS job has the power to publish code-scanning results.
    // An extra action before analysis must not inherit that privilege.
    f.jobs["codeql-javascript-typescript"]!.steps.splice(2, 0, { uses });
    expect(() => check(f), uses).toThrow(
      "CI workflow steps must use only reviewed immutable action refs",
    );
  }
});

test("selected native and MSRV checks cannot become false-green via continue-on-error", () => {
  const msrv = fixture();
  const selected = step(msrv, "msrv", "cargo check on MSRV toolchain");
  selected.run = "exit 1";
  selected["continue-on-error"] = true;
  expect(() => check(msrv)).toThrow(
    "CI workflow jobs and steps must not tolerate failures",
  );

  const job = fixture();
  job.jobs.msrv!["continue-on-error"] = true;
  expect(() => check(job)).toThrow(
    "CI workflow jobs and steps must not tolerate failures",
  );

  const harmlessLookingFalse = fixture();
  step(harmlessLookingFalse, "msrv", "cargo check on MSRV toolchain")[
    "continue-on-error"
  ] = false;
  expect(() => check(harmlessLookingFalse)).toThrow(
    "CI workflow jobs and steps must not tolerate failures",
  );

  // GitHub YAML accepts inline flow mappings, which the older line-oriented
  // Rust scanner missed. This must fail against parsed effects, not styling.
  const start = source.indexOf("      - name: cargo check on MSRV toolchain\n");
  const end = source.indexOf("\n  deny:\n", start);
  expect(start).toBeGreaterThan(0);
  expect(end).toBeGreaterThan(start);
  const flow =
    source.slice(0, start) +
    "      - {name: cargo check on MSRV toolchain, run: 'exit 1', continue-on-error: true}\n" +
    source.slice(end);
  expect(() => checkWorkflowSecurity(flow)).toThrow(
    "CI workflow jobs and steps must not tolerate failures",
  );
});

function executableFmtLines(run: unknown): Set<string> {
  // This CI step contains literal commands, not a generated shell program.
  // Match entire executable lines; echoed text and comments must not satisfy
  // the test that protects the otherwise-unreachable included Rust files.
  return new Set(
    String(run ?? "")
      .split(/\r?\n/)
      .map((line) => line.trim()),
  );
}

test("rustfmt avoids compiled Cargo cache without reducing format coverage", () => {
  const workflow = fixture();
  const steps = workflow.jobs.fmt!.steps;
  expect(
    steps.some((step) =>
      String(step.uses ?? "").startsWith("Swatinem/rust-cache@"),
    ),
  ).toBe(false);
  expect(
    steps.some(
      (step) => String(step.run ?? "").trim() === "cargo fmt --all --check",
    ),
  ).toBe(true);
  const included = steps.find(
    (step) => step.name === "rustfmt include!d and macro-body #[path] files",
  );
  const lines = executableFmtLines(included?.run);
  expect(lines.has("tools/fmt_included.sh test")).toBe(true);
  expect(lines.has("tools/fmt_included.sh --check")).toBe(true);
  for (const fake of [
    "echo tools/fmt_included.sh test\necho tools/fmt_included.sh --check",
    "# tools/fmt_included.sh test\n# tools/fmt_included.sh --check",
    "true # tools/fmt_included.sh test\ntrue # tools/fmt_included.sh --check",
  ]) {
    const muted = executableFmtLines(fake);
    expect(muted.has("tools/fmt_included.sh test")).toBe(false);
    expect(muted.has("tools/fmt_included.sh --check")).toBe(false);
  }
});

test("required included-file format commands cannot be skipped or shell-swallowed", () => {
  for (const mutation of [
    "skip",
    "ignore-error",
    "swallow-shell",
    "echo-only",
  ] as const) {
    const workflow = fixture();
    const included = step(
      workflow,
      "fmt",
      "rustfmt include!d and macro-body #[path] files",
    );
    if (mutation === "skip") included.if = false;
    if (mutation === "ignore-error") included["continue-on-error"] = true;
    if (mutation === "swallow-shell") included.shell = "bash -c true {0}";
    if (mutation === "echo-only") {
      included.run =
        "echo tools/fmt_included.sh test\necho tools/fmt_included.sh --check";
    }
    expect(() => check(workflow)).toThrow(
      "rustfmt included-file commands must execute exactly",
    );
  }
});

test("rustfmt Cargo command and changed-Rust routing preserve executable failure", () => {
  for (const mutation of [
    "skip",
    "ignore-error",
    "swallow-shell",
    "ignore-script",
  ] as const) {
    const workflow = fixture();
    const cargo = workflow.jobs.fmt!.steps.find(
      (candidate) => candidate.run === "cargo fmt --all --check",
    )!;
    if (mutation === "skip") cargo.if = false;
    if (mutation === "ignore-error") cargo["continue-on-error"] = true;
    if (mutation === "swallow-shell") cargo.shell = "bash -c true {0}";
    if (mutation === "ignore-script")
      cargo.run = "set +e\ncargo fmt --all --check\ntrue";
    expect(() => check(workflow)).toThrow(
      "rustfmt cargo fmt command must execute exactly",
    );
  }
  const skipped = fixture();
  skipped.jobs.fmt!.if = false;
  expect(() => check(skipped)).toThrow("rustfmt job must preserve");
  const hiddenCache = fixture();
  hiddenCache.jobs.fmt!.steps.splice(2, 0, {
    uses: "sWatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0",
  });
  expect(() => check(hiddenCache)).toThrow(
    "rustfmt must not restore or save a compiled Cargo cache",
  );
});

test("rustfmt rejects unreviewed job execution contexts", () => {
  const mutations: Array<[string, Record<string, unknown>]> = [
    ["BASH_ENV", { env: { BASH_ENV: "/tmp/hostile-env" } }],
    ["PATH", { env: { PATH: "/tmp/hostile-bin" } }],
    ["container", { container: "ubuntu:22.04" }],
    ["services", { services: { db: { image: "postgres:16" } } }],
    ["runner", { "runs-on": "ubuntu-22.04" }],
  ];
  for (const [name, mutation] of mutations) {
    const workflow = fixture();
    Object.assign(workflow.jobs.fmt!, mutation);
    expect(() => check(workflow), name).toThrow(
      "rustfmt job must preserve its reviewed execution context",
    );
  }
});

test("rustfmt rejects injected steps and step-level environment overrides", () => {
  const mutations: Array<[string, (steps: Step[]) => void]> = [
    [
      "GITHUB_PATH prelude",
      (steps) =>
        steps.splice(2, 0, {
          run: 'echo /tmp/hostile-bin >> "$GITHUB_PATH"',
        }),
    ],
    [
      "BASH_ENV setup",
      (steps) => {
        steps[1]!.env = { BASH_ENV: "/tmp/hostile-env" };
      },
    ],
    [
      "checkout PATH replacement",
      (steps) => {
        steps[0]!.env = { PATH: "/tmp/hostile-bin" };
      },
    ],
  ];
  for (const [name, mutate] of mutations) {
    const workflow = fixture();
    mutate(workflow.jobs.fmt!.steps);
    expect(() => check(workflow), name).toThrow(
      "rustfmt reviewed step inventory/order must not acquire unreviewed executable or environment writers",
    );
  }
});

test("inherited custom shells cannot false-green terminal required results", () => {
  for (const scope of ["workflow", "fmt", "required"] as const) {
    const workflow = fixture();
    if (scope === "workflow")
      workflow.defaults = { run: { shell: "bash -c true {0}" } };
    else
      workflow.jobs[scope]!.defaults = { run: { shell: "bash -c true {0}" } };
    expect(() => check(workflow)).toThrow("must not set defaults.run.shell");
  }
  const nonblocking = fixture();
  nonblocking.jobs.required!["continue-on-error"] = true;
  expect(() => check(nonblocking)).toThrow(
    "CI required must not tolerate failures",
  );
  for (const mutation of ["skip", "ignore-error", "swallow-shell"] as const) {
    const workflow = fixture();
    const evaluator = step(workflow, "required", "Verify required CI results");
    if (mutation === "skip") evaluator.if = false;
    if (mutation === "ignore-error") evaluator["continue-on-error"] = true;
    if (mutation === "swallow-shell") evaluator.shell = "bash -c true {0}";
    expect(() => check(workflow)).toThrow(
      "CI required evaluator must execute directly",
    );
  }
});

test("inherited working directories cannot redirect executable CI gates", () => {
  for (const scope of ["workflow", "fmt", "changes"] as const) {
    const workflow = fixture();
    if (scope === "workflow")
      workflow.defaults = { run: { "working-directory": "./spoof" } };
    else
      workflow.jobs[scope]!.defaults = {
        run: { "working-directory": "./spoof" },
      };
    expect(() => check(workflow), scope).toThrow(
      "must not set defaults.run.working-directory",
    );
  }
});

test("required job must still run when an upstream required lane fails", () => {
  const valid = fixture();
  expect(valid.jobs.required!.if).toBe("${{ always() }}");
  for (const condition of [
    undefined,
    null,
    false,
    true,
    "always()",
    "${{ false }}",
    "${{ success() }}",
    "${{ !cancelled() }}",
    "${{ always() && false }}",
  ]) {
    const workflow = fixture();
    if (condition === undefined) delete workflow.jobs.required!.if;
    else workflow.jobs.required!.if = condition;
    expect(() => check(workflow), String(condition)).toThrow(
      "CI required must use exact always() condition",
    );
  }
});

test("required evaluator runs exactly the reviewed 35-argument blocking script", () => {
  const original = String(
    step(fixture(), "required", "Verify required CI results").run,
  );
  expect(
    original.startsWith(
      "tools/ci_required.sh test\ntools/ci_required.sh check \\\n",
    ),
  ).toBe(true);
  for (const [label, changed] of [
    ["early exit bypass", "exit 0\n" + original],
    ["early no-op", "true\n" + original],
    ["prepended diagnostics", "echo start\n" + original],
    [
      "test command echoed",
      original.replace(
        "tools/ci_required.sh test",
        "echo tools/ci_required.sh test",
      ),
    ],
    [
      "gate command echoed",
      original.replace(
        "tools/ci_required.sh check",
        "echo tools/ci_required.sh check",
      ),
    ],
    [
      "first result spoofed",
      original.replace('"$KELD_RESULT_CHANGES"', '"success"'),
    ],
    ["shell failure disabled", "set +e\n" + original],
    ["final command appended", original + "exit 0\n"],
  ] as const) {
    expect(changed, label + " mutation applied").not.toBe(original);
    const workflow = fixture();
    step(workflow, "required", "Verify required CI results").run = changed;
    expect(() => check(workflow), label).toThrow(
      "CI required evaluator must run exact reviewed script",
    );
  }
});

test("required evaluator rejects injected shell startup and command search overrides", () => {
  for (const [name, value] of [
    ["BASH_ENV", "tools/ci_required.sh"],
    ["PATH", "/tmp/other-bin"],
    ["GITHUB_ENV", "/tmp/other-env"],
    ["SHELLOPTS", "errexit"],
    ["BASHOPTS", "expand_aliases"],
    ["IFS", "/"],
    ["ENV", "/tmp/other-env"],
  ] as const) {
    const workflow = fixture();
    const env = step(workflow, "required", "Verify required CI results")
      .env as Step;
    env[name] = value;
    expect(() => check(workflow), name).toThrow(
      "CI required evaluator environment",
    );
  }
  const spoofed = fixture();
  (
    step(spoofed, "required", "Verify required CI results").env as Step
  ).KELD_RESULT_CHANGES = "success";
  expect(() => check(spoofed)).toThrow("CI required evaluator environment");
});

test("required evaluator cannot inherit unreviewed workflow or job environments", () => {
  for (const [name, value] of [
    ["BASH_ENV", "tools/ci_required.sh"],
    ["PATH", "/tmp/other-bin"],
    ["GITHUB_ENV", "/tmp/other-env"],
  ] as const) {
    const workflow = fixture();
    (workflow.env as Step)[name] = value;
    expect(() => check(workflow), "workflow " + name).toThrow(
      "CI workflow environment",
    );

    const required = fixture();
    required.jobs.required!.env = { [name]: value };
    expect(() => check(required), "required job " + name).toThrow(
      "CI required job must not set env",
    );
  }
  const changedDefault = fixture();
  (changedDefault.env as Step).CARGO_INCREMENTAL = "1";
  expect(() => check(changedDefault)).toThrow("CI workflow environment");
});

test("required job keeps its hosted runner, full dependency set and exact two-step order", () => {
  const mutations: Array<[string, (workflow: Fixture) => void]> = [
    [
      "self-hosted runner",
      (f) => {
        f.jobs.required!["runs-on"] = "self-hosted";
      },
    ],
    [
      "wrong runner",
      (f) => {
        f.jobs.required!["runs-on"] = "windows-latest";
      },
    ],
    [
      "extended deadline",
      (f) => {
        f.jobs.required!["timeout-minutes"] = 45;
      },
    ],
    [
      "missing required lane",
      (f) => {
        f.jobs.required!.needs = (f.jobs.required!.needs as string[]).filter(
          (name) => name !== "fmt",
        );
      },
    ],
    [
      "missing gitleaks",
      (f) => {
        f.jobs.required!.needs = (f.jobs.required!.needs as string[]).filter(
          (name) => name !== "secrets",
        );
      },
    ],
    [
      "extra unreviewed dependency",
      (f) => {
        (f.jobs.required!.needs as string[]).push("irrelevant");
      },
    ],
    [
      "prepended environment mutation step",
      (f) => {
        f.jobs.required!.steps.splice(1, 0, {
          run: 'echo BASH_ENV=tools/other.sh >> "$GITHUB_ENV"',
        });
      },
    ],
    [
      "checkout dropped",
      (f) => {
        f.jobs.required!.steps.shift();
      },
    ],
    [
      "required steps reversed",
      (f) => {
        f.jobs.required!.steps.reverse();
      },
    ],
    [
      "extra job strategy",
      (f) => {
        (f.jobs.required as Step).strategy = {
          matrix: { os: ["ubuntu-latest"] },
        };
      },
    ],
  ];
  for (const [label, mutate] of mutations) {
    const workflow = fixture();
    mutate(workflow);
    expect(() => check(workflow), label).toThrow("CI required");
  }
});

test("every Rust installer agrees with the exact pin and immutable action commit", () => {
  expect(() =>
    checkRustToolchainPins(source, selectedRustToolchain),
  ).not.toThrow();
  const workflow = fixture();
  const installers = Object.values(workflow.jobs)
    .flatMap((job) => job.steps)
    .filter((s) => String(s.uses ?? "").startsWith("dtolnay/rust-toolchain@"));
  expect(installers.length).toBe(requiredRustInstallerJobs.length);

  // A single stale installer may not hide behind nine current copies.
  (installers[3]!.with as Record<string, unknown>).toolchain = staleRust;
  expect(() =>
    checkRustToolchainPins(Bun.YAML.stringify(workflow), selectedRustToolchain),
  ).toThrow("must select Rust");
  (installers[3]!.with as Record<string, unknown>).toolchain = selectedRust;

  // A changed action SHA is independent from the compiler version, but all
  // jobs must use the same reviewed immutable action implementation.
  installers[3]!.uses = "dtolnay/rust-toolchain@" + "0".repeat(40);
  expect(() =>
    checkRustToolchainPins(Bun.YAML.stringify(workflow), selectedRustToolchain),
  ).toThrow("same reviewed immutable");
  expect(() => checkRustToolchainPins(source, staleRustToolchain)).toThrow(
    "must select Rust",
  );
});

test("Rust toolchain admission reads the actual TOML toolchain table, never a decoy", () => {
  const decoy = `[decoy]\nchannel = "${selectedRust}"\n[toolchain]\nchannel = "${staleRust}"\ncomponents = ["rustfmt", "clippy"]\n`;
  expect(() => checkRustToolchainPins(source, decoy)).toThrow(
    "must select Rust",
  );
  const duplicate = `[toolchain]\nchannel = "${selectedRust}"\nchannel = "${staleRust}"\n`;
  expect(() => checkRustToolchainPins(source, duplicate)).toThrow();
});

test("dropping any one required Rust installer cannot silently use runner Rust", () => {
  for (const name of requiredRustInstallerJobs) {
    const workflow = fixture();
    workflow.jobs[name]!.steps = workflow.jobs[name]!.steps.filter(
      (step) => !String(step.uses ?? "").startsWith("dtolnay/rust-toolchain@"),
    );
    expect(() =>
      checkRustToolchainPins(
        Bun.YAML.stringify(workflow),
        selectedRustToolchain,
      ),
    ).toThrow("requires a pinned Rust toolchain installer");
  }
});

test("a uniformly replaced unknown action SHA is not an approved installer", () => {
  const workflow = fixture();
  for (const job of Object.values(workflow.jobs)) {
    for (const step of job.steps) {
      if (String(step.uses ?? "").startsWith("dtolnay/rust-toolchain@")) {
        step.uses = "dtolnay/rust-toolchain@" + "0".repeat(40);
      }
    }
  }
  expect(() =>
    checkRustToolchainPins(Bun.YAML.stringify(workflow), selectedRustToolchain),
  ).toThrow("approved immutable action SHA");
});

test("a pinned Rust installer cannot be conditional or failure-tolerant", () => {
  for (const name of requiredRustInstallerJobs) {
    for (const mutation of [
      (installer: Step) => {
        installer.if = "${{ false }}";
      },
      (installer: Step) => {
        installer["continue-on-error"] = true;
      },
    ]) {
      const workflow = fixture();
      mutation(workflow.jobs[name]!.steps[1]!);
      expect(() =>
        checkRustToolchainPins(
          Bun.YAML.stringify(workflow),
          selectedRustToolchain,
        ),
      ).toThrow("must run unconditionally and refuse installer failures");
    }
  }
});

test("Rust setup must run immediately after checkout, before other steps", () => {
  for (const name of requiredRustInstallerJobs) {
    for (const mutation of [
      (steps: Step[]) => {
        steps.splice(1, 0, { run: "cargo --version" });
      },
      (steps: Step[]) => {
        steps.unshift(steps.splice(1, 1)[0]!);
      },
    ]) {
      const workflow = fixture();
      mutation(workflow.jobs[name]!.steps);
      expect(() =>
        checkRustToolchainPins(
          Bun.YAML.stringify(workflow),
          selectedRustToolchain,
        ),
      ).toThrow("must install Rust immediately after checkout");
    }
  }
});

test("a Rustup environment override cannot shadow the pinned compiler", () => {
  for (const scope of ["workflow", "job", "step"]) {
    const workflow = fixture();
    if (scope === "workflow") {
      workflow.env = { RUSTUP_TOOLCHAIN: "stable" };
    } else if (scope === "job") {
      workflow.jobs.fmt!.env = { RUSTUP_TOOLCHAIN: "stable" };
    } else {
      const consumer = workflow.jobs.fmt!.steps.find((candidate) =>
        String(candidate.run ?? "").includes("cargo fmt"),
      );
      if (!consumer) throw new Error("missing actual Rust consumer");
      consumer.env = {
        ...((consumer.env as Record<string, unknown>) ?? {}),
        RUSTUP_TOOLCHAIN: "stable",
      };
    }
    expect(() =>
      checkRustToolchainPins(
        Bun.YAML.stringify(workflow),
        selectedRustToolchain,
      ),
    ).toThrow("must not override RUSTUP_TOOLCHAIN");
  }
});

test("a run step cannot shadow the pinned compiler for later steps", () => {
  for (const [script, message] of [
    [
      'echo RUSTUP_TOOLCHAIN=stable >> "$GITHUB_ENV"',
      "must not set RUSTUP_TOOLCHAIN",
    ],
    ["RUSTUP_TOOLCHAIN=stable cargo build", "must not set RUSTUP_TOOLCHAIN"],
    ["rustup default stable", "must not change the Rustup default"],
    ['rustup "override" set nightly', "must not change the Rustup default"],
    [
      "rustup --quiet override set nightly",
      "must not change the Rustup default",
    ],
    ["rustup \\\n  override set nightly", "must not change the Rustup default"],
    [
      'echo RUSTUP_\\\nTOOLCHAIN=stable >> "$GITHUB_ENV"',
      "must not set RUSTUP_TOOLCHAIN",
    ],
  ] as const) {
    const workflow = fixture();
    // Insert after the installer so the existing setup-order rule stays satisfied.
    workflow.jobs.fmt!.steps.splice(2, 0, { run: script });
    expect(() =>
      checkRustToolchainPins(
        Bun.YAML.stringify(workflow),
        selectedRustToolchain,
      ),
    ).toThrow(message);
  }
  // Call-site toolchain selection (the MSRV job's shape) changes nothing for
  // other commands and stays admitted.
  const workflow = fixture();
  workflow.jobs.fmt!.steps.splice(2, 0, {
    run: "rustup toolchain install 1.0.0 --profile minimal\ncargo +1.0.0 check",
  });
  expect(() =>
    checkRustToolchainPins(Bun.YAML.stringify(workflow), selectedRustToolchain),
  ).not.toThrow();
});

test("source-budget reporting stays advisory, visible and unprivileged", () => {
  const workflow = fixture();
  const s = step(workflow, "changes", "Source budget proposals (report only)");
  const command = String(s.run);
  const env = s.env as Record<string, unknown>;
  expect(s["continue-on-error"]).toBeUndefined();
  expect(s.if).toBeUndefined();
  expect(command).toContain("tools/source_budget_report.py");
  expect(command).toContain("--policy candidate");
  expect(command).toContain("GITHUB_STEP_SUMMARY");
  expect(command).not.toContain("--enforce");
  expect(String(env.KELD_BUDGET_BASE_SHA)).toContain(
    "github.event.pull_request.base.sha",
  );
  expect(String(env.KELD_BUDGET_HEAD_SHA)).toContain(
    "github.event.pull_request.head.sha",
  );
});

test("offline CI attempt reporter regressions execute in the always-on router", () => {
  const job = step(
    fixture(),
    "changes",
    "CI attempt timing reporter regression tests",
  );
  expect(String(job.run ?? "").trim()).toBe(
    "python3 -B tools/test_ci_attempt_metrics.py",
  );
  expect(job.if).toBeUndefined();
  expect(job["continue-on-error"]).toBeUndefined();
});

test("CI attempt reporter tests cannot be skipped or shell-swallowed", () => {
  for (const mutation of [
    "skip",
    "ignore-error",
    "swallow-shell",
    "echo-only",
  ] as const) {
    const workflow = fixture();
    const reporter = step(
      workflow,
      "changes",
      "CI attempt timing reporter regression tests",
    );
    if (mutation === "skip") reporter.if = false;
    if (mutation === "ignore-error") reporter["continue-on-error"] = true;
    if (mutation === "swallow-shell") reporter.shell = "bash -c true {0}";
    if (mutation === "echo-only")
      reporter.run = "echo python3 -B tools/test_ci_attempt_metrics.py";
    expect(() => check(workflow)).toThrow(
      "CI attempt timing reporter tests must execute exactly",
    );
  }
});

test("source-budget tool unit tests run in normal unprivileged CI", () => {
  const workflow = fixture();
  const s = step(workflow, "changes", "Source budget tool tests");
  const command = String(s.run);
  expect(s["continue-on-error"]).toBeUndefined();
  expect(s.if).toBeUndefined();
  expect(command).toContain("tools/test_source_census.py");
  expect(command).toContain("tools/test_source_budget_report.py");
  const missing = fixture();
  missing.jobs.changes!.steps = missing.jobs.changes!.steps.filter(
    (x) => x.name !== s.name,
  );
  expect(() => check(missing)).toThrow(
    "Source budget tool tests must occur exactly once",
  );
});

test("new Rust test imports require explicit names in normal unprivileged CI", () => {
  const workflow = fixture();
  const s = step(workflow, "changes", "Test module import ownership");
  const command = String(s.run);
  const env = s.env as Record<string, unknown>;
  expect(s["continue-on-error"]).toBeUndefined();
  expect(s.if).toBeUndefined();
  expect(command).toContain("tools/test_test_import_guard.py");
  expect(command).toContain("tools/test_import_guard.py");
  expect(command).toContain("--base");
  expect(command).toContain("--head");
  expect(String(env.KELD_IMPORT_BASE_SHA)).toContain(
    "github.event.pull_request.base.sha",
  );
  expect(String(env.KELD_IMPORT_HEAD_SHA)).toContain(
    "github.event.pull_request.head.sha",
  );
  const missing = fixture();
  missing.jobs.changes!.steps = missing.jobs.changes!.steps.filter(
    (x) => x.name !== s.name,
  );
  expect(() => check(missing)).toThrow(
    "Test module import ownership must occur exactly once",
  );
});

test("Windows media runner avoids unused Bun and rustfmt installation", () => {
  const steps = fixture().jobs["windows-media"]!.steps;
  const bunSetup = steps.filter(
    (candidate) =>
      typeof candidate.uses === "string" &&
      candidate.uses.toLowerCase().startsWith("oven-sh/setup-bun@"),
  );
  expect(bunSetup).toHaveLength(0);
  const installers = steps.filter(
    (candidate) =>
      typeof candidate.uses === "string" &&
      candidate.uses.startsWith("dtolnay/rust-toolchain@"),
  );
  expect(installers).toHaveLength(1);
  expect((installers[0]!.with as Record<string, unknown>).components).toBe(
    "clippy",
  );
});

test("Windows media acceptance must execute without step-level skip or failure masking", () => {
  for (const [label, edit] of [
    [
      "literal false condition",
      (media: Step) => {
        media.if = false;
      },
    ],
    [
      "expression false condition",
      (media: Step) => {
        media.if = "${{ false }}";
      },
    ],
    [
      "failure tolerance",
      (media: Step) => {
        media["continue-on-error"] = true;
      },
    ],
    [
      "different shell",
      (media: Step) => {
        media.shell = "bash";
      },
    ],
  ] as const) {
    const altered = fixture();
    edit(
      step(
        altered,
        "windows-media",
        "Windows media guard acceptance (KEL-132)",
      ),
    );
    expect(() => check(altered), label).toThrow(
      "Windows media acceptance step",
    );
  }
});

test("Windows media rejects unreviewed path and environment writers", () => {
  const inventoryError =
    "Windows media reviewed step inventory/order must not acquire unreviewed executable or environment writers";
  for (const variable of ["GITHUB_PATH", "GITHUB_ENV"]) {
    const f = fixture();
    const steps = f.jobs["windows-media"]!.steps;
    const acceptance = steps.findIndex(
      (candidate) =>
        candidate.name === "Windows media guard acceptance (KEL-132)",
    );
    expect(acceptance).toBeGreaterThan(0);
    steps.splice(acceptance, 0, {
      name: "Unreviewed Windows media prelude",
      run: 'echo /tmp/harmless >> "$' + variable + '"',
    });
    expect(() => check(f), variable).toThrow(inventoryError);
  }
  const changed = fixture();
  changed.jobs["windows-media"]!.steps[0]!.env = {
    PATH: "C:\\unreviewed",
  };
  expect(() => check(changed), "checkout environment").toThrow(inventoryError);
});

test("both Nextest installers pin 0.9.149 and reject unpinned or downgraded releases", () => {
  for (const [jobName, reviewed, unpinned, previous] of [
    [
      "check",
      "nextest@0.9.149,just@1.58.0",
      "nextest,just@1.58.0",
      "nextest@0.9.148,just@1.58.0",
    ],
    ["windows-media", "nextest@0.9.149", "nextest", "nextest@0.9.148"],
  ] as const) {
    const clean = fixture();
    const installer = clean.jobs[jobName]!.steps.find((candidate) =>
      String(candidate.uses ?? "").startsWith("taiki-e/install-action@"),
    );
    expect(installer).toBeDefined();
    expect(installer!.uses).toBe(
      "taiki-e/install-action@1e29709c0ec63babbe8f266397a465f29a825f91",
    );
    expect((installer!.with as Step).tool).toBe(reviewed);
    expect((installer!.with as Step).fallback).toBe("none");
    expect(() => check(clean)).not.toThrow();

    // v2.87.29 cannot resolve 0.9.149 from its embedded manifest.
    const oldAction = fixture();
    const staleInstaller = oldAction.jobs[jobName]!.steps.find((candidate) =>
      String(candidate.uses ?? "").startsWith("taiki-e/install-action@"),
    )!;
    staleInstaller.uses =
      "taiki-e/install-action@8cca0918f8471c33515202b73f56ab27f2bed283";
    expect(() => check(oldAction)).toThrow("reviewed immutable action refs");

    for (const rejected of [
      unpinned,
      previous,
      jobName === "check" ? "nextest@9999.0.0,just@1.58.0" : "nextest@9999.0.0",
    ]) {
      const altered = fixture();
      const downgraded = altered.jobs[jobName]!.steps.find((candidate) =>
        String(candidate.uses ?? "").startsWith("taiki-e/install-action@"),
      )!;
      (downgraded.with as Step).tool = rejected;
      expect(() => check(altered), `${jobName}: ${rejected}`).toThrow(
        "reviewed taiki installer inputs",
      );
    }
  }
});

test("Nextest version verifier is mandatory on both native runner jobs", () => {
  const expected =
    "cargo nextest --version | grep -E '^cargo-nextest 0[.]9[.]149 [(]'";
  for (const jobName of ["check", "windows-media"] as const) {
    const clean = fixture();
    const steps = clean.jobs[jobName]!.steps;
    const installer = steps.findIndex((step) =>
      String(step.uses ?? "").startsWith("taiki-e/install-action@"),
    );
    const verifier = steps[installer + 1];
    expect(verifier?.name).toBe("Verify cargo-nextest 0.9.149");
    expect(verifier?.shell).toBe("bash");
    expect(verifier?.run).toBe(expected);
    expect(() => check(clean)).not.toThrow();
    for (const mutation of [
      "removed",
      "changed",
      "skipped",
      "ignored",
      "moved",
    ] as const) {
      const altered = fixture();
      const mutated = altered.jobs[jobName]!.steps;
      const index = mutated.findIndex(
        (step) => step.name === "Verify cargo-nextest 0.9.149",
      );
      if (mutation === "removed") mutated.splice(index, 1);
      if (mutation === "changed")
        mutated[index]!.run = "echo cargo-nextest 0.9.149";
      if (mutation === "skipped") mutated[index]!.if = false;
      if (mutation === "ignored") mutated[index]!["continue-on-error"] = true;
      if (mutation === "moved")
        mutated.splice(index + 2, 0, ...mutated.splice(index, 1));
      expect(() => check(altered), `${jobName}/${mutation}`).toThrow();
    }
  }
});

test("both Nextest installers refuse unreviewed tools and integrity overrides", () => {
  for (const jobName of ["check", "windows-media"] as const) {
    const clean = fixture();
    const actual = clean.jobs[jobName]!.steps.find((candidate) =>
      String(candidate.uses ?? "").startsWith("taiki-e/install-action@"),
    );
    expect(actual).toBeDefined();
    expect(() => check(clean)).not.toThrow();
    for (const [name, edit] of [
      [
        "checksum disabled",
        (inputs: Record<string, unknown>) => {
          inputs.checksum = false;
        },
      ],
      [
        "fallback to cargo-binstall",
        (inputs: Record<string, unknown>) => {
          inputs.fallback = "cargo-binstall";
        },
      ],
      [
        "fallback to cargo-install",
        (inputs: Record<string, unknown>) => {
          inputs.fallback = "cargo-install";
        },
      ],
      [
        "unreviewed tool",
        (inputs: Record<string, unknown>) => {
          inputs.tool = "cargo-hack";
        },
      ],
    ] as const) {
      const altered = fixture();
      const installer = altered.jobs[jobName]!.steps.find((candidate) =>
        String(candidate.uses ?? "").startsWith("taiki-e/install-action@"),
      )!;
      edit(installer.with as Record<string, unknown>);
      expect(() => check(altered), `${jobName}: ${name}`).toThrow(
        "reviewed taiki installer inputs",
      );
    }
  }
});

test("required native main gates cannot skip, swallow failures or inherit unsafe environment", () => {
  const mutations: Array<[string, (workflow: Fixture) => void]> = [
    [
      "native tests skipped",
      (f) => {
        step(f, "check", "test").if = false;
      },
    ],
    [
      "Clippy skipped",
      (f) => {
        step(f, "check", "clippy (warnings deny)").if = false;
      },
    ],
    [
      "native test ignores failure",
      (f) => {
        step(f, "check", "test")["continue-on-error"] = true;
      },
    ],
    [
      "native test shell overridden",
      (f) => {
        step(f, "check", "test").shell = "bash -c true {0}";
      },
    ],
    [
      "native test script swallowed",
      (f) => {
        step(f, "check", "test").run =
          "echo cargo nextest run --profile ci --no-tests=pass";
      },
    ],
    [
      "Clippy script swallowed",
      (f) => {
        step(f, "check", "clippy (warnings deny)").run =
          "echo cargo clippy -- -D warnings";
      },
    ],
    [
      "native step BASH_ENV",
      (f) => {
        (step(f, "check", "test").env as Step).BASH_ENV = "./untrusted.sh";
      },
    ],
    [
      "Clippy step BASH_ENV",
      (f) => {
        (step(f, "check", "clippy (warnings deny)").env as Step).BASH_ENV =
          "./untrusted.sh";
      },
    ],
    [
      "native step PATH",
      (f) => {
        (step(f, "check", "test").env as Step).PATH = "/tmp/untrusted";
      },
    ],
    [
      "native job BASH_ENV",
      (f) => {
        (f.jobs.check!.env as Step).BASH_ENV = "./untrusted.sh";
      },
    ],
    [
      "native job PATH",
      (f) => {
        (f.jobs.check!.env as Step).PATH = "/tmp/untrusted";
      },
    ],
    [
      "media job PATH",
      (f) => {
        f.jobs["windows-media"]!.env = { PATH: "C:\\untrusted" };
      },
    ],
  ];
  for (const [label, mutate] of mutations) {
    const f = fixture();
    mutate(f);
    expect(() => check(f), label).toThrow();
  }
});

test("native check rejects unreviewed steps or environment writers before Clippy and Nextest", () => {
  for (const variable of ["GITHUB_PATH", "GITHUB_ENV", "BASH_ENV"]) {
    const f = fixture();
    const steps = f.jobs.check!.steps;
    const clippy = steps.findIndex(
      (item) => item.name === "clippy (warnings deny)",
    );
    expect(clippy).toBeGreaterThan(0);
    steps.splice(clippy, 0, {
      name: "Unreviewed environment writer",
      run: `printf '%s\\n' /tmp/harmless >> "$${variable}"`,
    });
    expect(() => check(f), variable).toThrow(
      "native check reviewed step inventory",
    );
  }
  const changed = fixture();
  step(changed, "check", "Worktree cache tool native tests").run +=
    '\nprintf "%s\\n" /tmp/harmless >> "$GITHUB_PATH"\n';
  expect(() => check(changed)).toThrow("native check reviewed step inventory");
});

test("native jobs reject missing, duplicate or delayed reviewed Nextest installers", () => {
  for (const jobName of ["check", "windows-media"] as const) {
    for (const kind of ["missing", "duplicate", "delayed"] as const) {
      const altered = fixture();
      const steps = altered.jobs[jobName]!.steps;
      const index = steps.findIndex((candidate) =>
        String(candidate.uses ?? "").startsWith("taiki-e/install-action@"),
      );
      expect(index).toBeGreaterThan(0);
      const installer = steps[index]!;
      if (kind === "missing") steps.splice(index, 1);
      if (kind === "duplicate")
        steps.splice(index, 0, structuredClone(installer));
      if (kind === "delayed") {
        steps.splice(index, 1);
        steps.push(installer);
      }
      const reason =
        kind === "delayed"
          ? "reviewed taiki installer must precede"
          : "must contain exactly one reviewed taiki installer";
      expect(() => check(altered), `${jobName}: ${kind}`).toThrow(reason);
    }
  }
});

test("Windows media oracle matches its reviewed full-file digest", () => {
  expect(() => checkWindowsMediaOracle(windowsMediaOracle)).not.toThrow();
  const mutated = Buffer.concat([
    windowsMediaOracle,
    Buffer.from("\n# inert override\n"),
  ]);
  expect(() => checkWindowsMediaOracle(mutated)).toThrow(
    "reviewed SHA-256 owner",
  );
});

const codeqlJobs = codeqlLanguages.map((language) => `codeql-${language}`);

test("CodeQL Rust stays on hosted macOS for platform-specific semantic coverage", () => {
  expect(fixture().jobs["codeql-rust"]!["runs-on"]).toBe("macos-latest");
  for (const runner of [
    "ubuntu-24.04",
    "ubuntu-latest",
    "self-hosted",
    "${{ matrix.os }}",
  ]) {
    const f = fixture();
    f.jobs["codeql-rust"]!["runs-on"] = runner;
    expect(() => check(f)).toThrow("CodeQL Rust must use macos-latest");
  }
});

for (const language of codeqlLanguages) {
  const job = `codeql-${language}`;
  const other = codeqlLanguages.find((candidate) => candidate !== language)!;

  test(`CodeQL ${language} rejects extra executable steps and job environment overrides`, () => {
    const injected = fixture();
    injected.jobs[job]!.steps.splice(-1, 0, {
      name: "Unreviewed command",
      run: "echo harmless",
    });
    expect(() => check(injected)).toThrow("CodeQL reviewed step inventory");
    const env = fixture();
    env.jobs[job]!.env = { BASH_ENV: "./unreviewed.sh" };
    expect(() => check(env)).toThrow("CodeQL job environment");
    const runner = fixture();
    runner.jobs[job]!["runs-on"] = "self-hosted";
    expect(() => check(runner)).toThrow(
      language === "rust"
        ? "CodeQL Rust must use macos-latest"
        : "CodeQL job runner",
    );
  });

  test(`CodeQL ${language} cannot tolerate job-level analysis failures`, () => {
    for (const value of [true, false, "${{ true }}"]) {
      const f = fixture();
      f.jobs[job]!["continue-on-error"] = value;
      expect(() => check(f)).toThrow(
        `CodeQL job ${job} must fail on analyzer errors`,
      );
    }
  });

  test(`CodeQL ${language} init and analyze must use an identical immutable SHA`, () => {
    const f = fixture();
    const init = step(f, job, "Initialize CodeQL");
    const analyze = step(f, job, "Analyze and upload CodeQL results");
    expect(String(init.uses)).toMatch(
      /^github\/codeql-action\/init@[0-9a-f]{40}$/,
    );
    expect(String(analyze.uses)).toBe(
      String(init.uses).replace("/init@", "/analyze@"),
    );
    analyze.uses = `github/codeql-action/analyze@${"0".repeat(40)}`;
    expect(() => check(f)).toThrow(
      "CodeQL init and analyze must use the same pinned commit",
    );
  });

  test(`CodeQL ${language} rejects uniformly changed unreviewed pinned actions`, () => {
    const f = fixture();
    const init = step(f, job, "Initialize CodeQL");
    const analyze = step(f, job, "Analyze and upload CodeQL results");
    const unreviewedSha = "0".repeat(40);
    init.uses = `github/codeql-action/init@${unreviewedSha}`;
    analyze.uses = `github/codeql-action/analyze@${unreviewedSha}`;
    expect(() => check(f)).toThrow(
      "CodeQL init and analyze must use the reviewed pinned commit",
    );
  });

  test(`CodeQL cannot omit the ${language} job`, () => {
    const f = fixture();
    delete f.jobs[job];
    expect(() => check(f)).toThrow(`CodeQL job ${job} must exist`);
  });

  test(`CodeQL ${language} routes only on its own router output`, () => {
    expect(fixture().jobs[job]!.if).toBe(codeqlRoute(language));
    for (const mutation of [
      "other-language",
      "always",
      "missing",
      "push-bypass",
      "no-needs",
      "extra-needs",
    ] as const) {
      const f = fixture();
      const target = f.jobs[job]!;
      if (mutation === "other-language") target.if = codeqlRoute(other);
      if (mutation === "always") target.if = true;
      if (mutation === "missing") delete target.if;
      if (mutation === "push-bypass")
        target.if = `github.event_name == 'push' || ${codeqlRoute(language)}`;
      if (mutation === "no-needs") delete target.needs;
      if (mutation === "extra-needs") target.needs = ["changes", "fmt"];
      expect(() => check(f)).toThrow("must need changes and run only on");
    }
  });

  test(`CodeQL ${language} analyses and uploads exactly its own category`, () => {
    const init = fixture();
    (step(init, job, "Initialize CodeQL").with as Step).languages = other;
    expect(() => check(init)).toThrow("Initialize CodeQL");
    const category = fixture();
    (
      step(category, job, "Analyze and upload CodeQL results").with as Step
    ).category = `/language:${other}`;
    expect(() => check(category)).toThrow("Analyze and upload CodeQL results");
    const matrix = fixture();
    matrix.jobs[job]!.strategy = { matrix: { language: [language] } };
    expect(() => check(matrix)).toThrow("must not set strategy");
  });

  test(`CodeQL ${language} permissions retain only read content and SARIF-upload authority`, () => {
    for (const mutation of [
      "missing",
      "security-events-read",
      "contents-write",
      "extra",
    ] as const) {
      const f = fixture();
      const permissions = f.jobs[job]!.permissions!;
      if (mutation === "missing") delete f.jobs[job]!.permissions;
      if (mutation === "security-events-read")
        permissions["security-events"] = "read";
      if (mutation === "contents-write") permissions.contents = "write";
      if (mutation === "extra") permissions.actions = "read";
      expect(() => check(f)).toThrow("CodeQL job permissions");
    }
  });
}

const aptSteps = [
  ["check", "Install WebKitGTK build deps (KEL-28, see keld-wv/Cargo.toml)"],
  [
    "linux-gui-smoke",
    "Install WebKitGTK build deps + X11 control tools (KEL-28)",
  ],
] as const;

for (const [job, name] of aptSteps) {
  test(`${name} keeps the evidence-based step timeout under a longer job timeout`, () => {
    expect(step(fixture(), job, name)["timeout-minutes"]).toBe(
      aptStepTimeoutMinutes,
    );
    // Negative controls: the 10-minute bound that failed a slow but healthy
    // download, and every other value or shape.
    for (const value of [
      undefined,
      10,
      0,
      aptStepTimeoutMinutes - 1,
      aptStepTimeoutMinutes + 1,
      "${{ inputs.minutes }}",
      "15m",
    ]) {
      const f = fixture();
      const selected = step(f, job, name);
      if (value === undefined) delete selected["timeout-minutes"];
      else selected["timeout-minutes"] = value;
      expect(() => check(f)).toThrow(
        `runs apt without step timeout-minutes: ${aptStepTimeoutMinutes}`,
      );
    }
    for (const value of [aptStepTimeoutMinutes, 10, "${{ inputs.minutes }}"]) {
      const f = fixture();
      f.jobs[job]!["timeout-minutes"] = value;
      expect(() => check(f)).toThrow(
        "does not exceed the 15-minute apt step bound",
      );
    }
  });
}

test("apt timeout reads the parsed run script, not names or layout", () => {
  for (const run of [
    "sudo apt update\n",
    "sudo apt install -y jq\n",
    "set -e; sudo apt-get install -y jq\n",
  ]) {
    const f = fixture();
    f.jobs.check!.steps.push({ name: "Install a tool", run });
    expect(() => check(f)).toThrow("runs apt without step timeout-minutes");
  }
  // Negative controls in unfrozen jobs: benign apt-like names, flow mapping,
  // quoted timeouts, and unrelated words must not trigger the apt guard.
  const named = fixture();
  named.jobs["linux-gui-smoke"]!.steps.push({
    name: "Diagnose apt-get mirror",
    run: "echo ok\n",
  });
  expect(() => check(named)).not.toThrow();
  const flowMarker = "      - name: Build Linux media guard probe\n";
  const flowed = source.replace(
    flowMarker,
    "      - { run: 'sudo apt-get install -y jq', timeout-minutes: 15 }\n" +
      flowMarker,
  );
  expect(flowed).not.toBe(source);
  expect(() => checkWorkflowSecurity(flowed)).not.toThrow();
  const quoted = fixture();
  quoted.jobs["linux-gui-smoke"]!.steps.push({
    run: "sudo apt-get install -y jq\n",
    "timeout-minutes": "15",
  });
  expect(() => check(quoted)).not.toThrow();
  const unrelated = fixture();
  unrelated.jobs["linux-gui-smoke"]!.steps.push({
    run: "echo adapter aptitude\n",
  });
  expect(() => check(unrelated)).not.toThrow();
});

const b3MainSaveIf =
  "${{ github.event_name == 'push' && github.ref == 'refs/heads/main' }}";
const b3BunNoCache =
  "${{ github.event_name != 'push' || github.ref != 'refs/heads/main' }}";
const b3BunJobs = ["check", "bun-test", "hygiene"] as const;

function b3BunAction(f: Fixture, jobName: string): Step {
  const actions = f.jobs[jobName]!.steps.filter((step) =>
    String(step.uses ?? "")
      .toLowerCase()
      .startsWith("oven-sh/setup-bun@"),
  );
  expect(actions.length, jobName).toBe(1);
  return actions[0]!;
}

test("B3 Bun action post-cache save is disabled for every PR and non-main push", () => {
  for (const job of b3BunJobs) {
    const action = b3BunAction(fixture(), job);
    expect((action.with as Step)["no-cache"], job).toBe(b3BunNoCache);
    for (const unsafe of [
      undefined,
      false,
      "false",
      "${{ github.event_name != 'pull_request' }}",
      "${{ github.ref != 'refs/heads/main' }}",
      "${{ github.event_name != 'push' }}",
    ]) {
      const f = fixture();
      const inputs = b3BunAction(f, job).with as Step;
      if (unsafe === undefined) delete inputs["no-cache"];
      else inputs["no-cache"] = unsafe;
      expect(() => check(f), `${job} ${String(unsafe)}`).toThrow(
        "B3 Bun cache must be disabled outside protected-main pushes",
      );
    }
  }
});

test("B3 Bun setup ownership rejects duplicated and rogue post-cache savers", () => {
  const duplicate = fixture();
  duplicate.jobs.hygiene!.steps.push({ ...b3BunAction(duplicate, "hygiene") });
  expect(() => check(duplicate)).toThrow(
    "B3 Bun cache setup must appear exactly once",
  );
  const rogue = fixture();
  rogue.jobs.fmt!.steps.push({ ...b3BunAction(rogue, "check") });
  expect(() => check(rogue)).toThrow("B3 Bun cache action not approved in job");
});

const b3RustCacheJobs = [
  "check",
  "windows-media",
  "doctest",
  "linux-gui-smoke",
  "msrv",
  "deny",
] as const;

function b3CacheStep(f: Fixture, jobName: string): Step {
  const steps = f.jobs[jobName]!.steps.filter((candidate) =>
    String(candidate.uses ?? "")
      .toLowerCase()
      .startsWith("swatinem/rust-cache@"),
  );
  expect(steps.length, jobName).toBe(1);
  return steps[0]!;
}

test("B3 Cargo cache writes are restricted to trusted protected-main pushes", () => {
  for (const job of b3RustCacheJobs) {
    const f = fixture();
    const cache = b3CacheStep(f, job);
    const inputs = cache.with as Step;
    expect(inputs["save-if"], job).toBe(b3MainSaveIf);
    for (const unsafe of [
      undefined,
      "true",
      "${{ github.event_name == 'push' }}",
      "${{ github.ref == 'refs/heads/main' }}",
      "${{ github.event_name != 'pull_request' }}",
    ]) {
      const mutated = fixture();
      const inputs = b3CacheStep(mutated, job).with as Step;
      if (unsafe === undefined) delete inputs["save-if"];
      else inputs["save-if"] = unsafe;
      expect(() => check(mutated), `${job}: ${unsafe}`).toThrow(
        "B3 Cargo cache save must require trusted protected-main push",
      );
    }
    const duplicate = fixture();
    duplicate.jobs[job]!.steps.push({ ...b3CacheStep(duplicate, job) });
    expect(() => check(duplicate), job).toThrow(
      job === "deny"
        ? "jobs.deny must run exactly"
        : "B3 Cargo cache action must appear exactly once in its reviewed job",
    );
  }
  const moved = fixture();
  moved.jobs.fmt!.steps.push({
    uses: b3CacheStep(moved, "check").uses,
    with: { "save-if": b3MainSaveIf },
  });
  expect(() => check(moved)).toThrow(
    "rustfmt must not restore or save a compiled Cargo cache",
  );
});

test("B3 signed WebKitGTK caches save only on a miss during protected-main push", () => {
  for (const job of webkitgtkAptJobs) {
    const f = fixture();
    const save = step(f, job, "Save WebKitGTK .deb cache");
    expect(String(save.if), job).toContain(
      "github.event_name == 'push' && github.ref == 'refs/heads/main'",
    );
    const weakened = fixture();
    const saveStep = step(weakened, job, "Save WebKitGTK .deb cache");
    saveStep.if = String(saveStep.if).replace(
      " && github.event_name == 'push' && github.ref == 'refs/heads/main'",
      "",
    );
    expect(() => check(weakened), job).toThrow(
      "B3 WebKitGTK cache save must require trusted protected-main push",
    );
  }
});

for (const job of webkitgtkAptJobs) {
  test(`${job} WebKitGTK .deb cache keeps its key, verified install and miss-only save`, () => {
    const mutations: [string, (f: Fixture) => void, string][] = [
      [
        "key without the image and package binding",
        (f) => {
          step(f, job, "Resolve WebKitGTK apt cache key").run =
            'echo "key=webkitgtk" >> "$GITHUB_OUTPUT"\n';
        },
        "resolve the cache key",
      ],
      [
        "restore under another key",
        (f) => {
          (step(f, job, "Restore WebKitGTK .deb cache").with as Step).key =
            "webkitgtk-${{ runner.os }}";
        },
        "Restore WebKitGTK .deb cache",
      ],
      [
        "cache the apt index lists",
        (f) => {
          (step(f, job, "Restore WebKitGTK .deb cache").with as Step).path =
            "/var/lib/apt/lists";
        },
        "Restore WebKitGTK .deb cache",
      ],
      [
        "save on a hit",
        (f) => {
          step(f, job, "Save WebKitGTK .deb cache").if = String(
            step(f, job, "Restore WebKitGTK .deb cache").if ?? "always()",
          );
        },
        "only on a miss",
      ],
      [
        "save under another key",
        (f) => {
          (step(f, job, "Save WebKitGTK .deb cache").with as Step).key =
            "other";
        },
        "Save WebKitGTK .deb cache",
      ],
      [
        "install bypassing verification",
        (f) => {
          const s = f.jobs[job]!.steps.find(
            (c) =>
              typeof c.run === "string" &&
              c.run.includes("ci_webkitgtk_apt.sh install"),
          )!;
          s.run = `sudo cp ${webkitgtkDebCachePath}/*.deb /var/cache/apt/archives/ && sudo apt-get update && sudo apt-get install -y $KELD_WEBKITGTK_PACKAGES\n`;
        },
        "install with exactly",
      ],
      [
        "missing package list",
        (f) => {
          delete (f.jobs[job]!.env as Step).KELD_WEBKITGTK_PACKAGES;
        },
        "KELD_WEBKITGTK_PACKAGES",
      ],
      [
        "save before install",
        (f) => {
          const steps = f.jobs[job]!.steps;
          const save = steps.findIndex(
            (c) => c.name === "Save WebKitGTK .deb cache",
          );
          const [s] = steps.splice(save, 1);
          steps.splice(save - 1, 0, s!);
        },
        "key, restore, install, save",
      ],
      [
        "floating cache action",
        (f) => {
          step(f, job, "Restore WebKitGTK .deb cache").uses =
            "actions/cache/restore@v6";
        },
        "immutable",
      ],
      [
        "unbounded restore",
        (f) => {
          delete step(f, job, "Restore WebKitGTK .deb cache")[
            "timeout-minutes"
          ];
        },
        "Restore WebKitGTK .deb cache",
      ],
      [
        "restore bound above 5 minutes",
        (f) => {
          step(f, job, "Restore WebKitGTK .deb cache")["timeout-minutes"] = 30;
        },
        "timeout-minutes: 5",
      ],
      [
        "unbounded save",
        (f) => {
          delete step(f, job, "Save WebKitGTK .deb cache")["timeout-minutes"];
        },
        "Save WebKitGTK .deb cache",
      ],
      [
        "stuck download held to the step bound",
        (f) => {
          (
            step(f, job, "Restore WebKitGTK .deb cache").env as Step
          ).SEGMENT_DOWNLOAD_TIMEOUT_MINS = "10";
        },
        "abort as a miss",
      ],
    ];
    for (const [label, mutate, message] of mutations) {
      const f = fixture();
      mutate(f);
      expect(() => check(f), label).toThrow(message);
    }
  });
}

const keldbot = readFileSync(
  join(import.meta.dir, "../.github/workflows/keldbot.yml"),
  "utf8",
);

test("pull_request_target workflows never check out or run pull-request code", () => {
  expect(() =>
    checkPullRequestTargetWorkflow(keldbot, "keldbot.yml"),
  ).not.toThrow();
  const parsed = Bun.YAML.parse(keldbot) as {
    jobs: Record<string, { steps: Step[] }>;
  };
  const firstJob = Object.keys(parsed.jobs)[0]!;
  for (const [label, inserted, message] of [
    [
      "run step",
      { run: "echo ${{ github.event.pull_request.title }}" },
      "run: step under pull_request_target",
    ],
    [
      "checkout",
      {
        uses: checkout,
        with: { ref: "${{ github.event.pull_request.head.sha }}" },
      },
      "checks out code",
    ],
    [
      "checkout with case and subpath",
      { uses: "Actions/Checkout/.@3d3c42e5aac5ba805825da76410c181273ba90b1" },
      "checks out code",
    ],
  ] as const) {
    const f = Bun.YAML.parse(keldbot) as typeof parsed;
    f.jobs[firstJob]!.steps.push({ ...inserted });
    expect(
      () =>
        checkPullRequestTargetWorkflow(Bun.YAML.stringify(f), "keldbot.yml"),
      label,
    ).toThrow(message);
  }
  const reusable = Bun.YAML.parse(keldbot) as Record<string, unknown> &
    typeof parsed;
  (reusable.jobs as Record<string, unknown>).extra = {
    uses: "owner/repo/.github/workflows/x.yml@3d3c42e5aac5ba805825da76410c181273ba90b1",
  };
  expect(() =>
    checkPullRequestTargetWorkflow(Bun.YAML.stringify(reusable), "keldbot.yml"),
  ).toThrow("reusable workflow");
  // Every trigger form is recognised, including flow and alias steps.
  for (const on of ["pull_request_target", "[push, pull_request_target]"]) {
    expect(() =>
      checkPullRequestTargetWorkflow(
        `on: ${on}\njobs:\n  j:\n    steps:\n      - &s { run: echo }\n      - *s\n`,
        "x.yml",
      ),
    ).toThrow("run: step");
  }
  // Negative control: the same run step under pull_request is not this rule's concern.
  expect(() =>
    checkPullRequestTargetWorkflow(
      "on: pull_request\njobs:\n  j:\n    steps:\n      - run: echo\n",
      "x.yml",
    ),
  ).not.toThrow();
});

test("B3 low-trust pull_request_target forbids workflow and job cache-mode overrides", () => {
  for (const mode of ["write", "write-only", "${{ inputs.cache_mode }}"]) {
    const workflow = Bun.YAML.parse(keldbot) as Record<string, unknown> & {
      jobs: Record<string, Record<string, unknown>>;
    };
    workflow["cache-mode"] = mode;
    expect(
      () =>
        checkPullRequestTargetWorkflow(
          Bun.YAML.stringify(workflow),
          "keldbot.yml",
        ),
      `workflow ${mode}`,
    ).toThrow("cache-mode");
    const perJob = Bun.YAML.parse(keldbot) as typeof workflow;
    perJob.jobs[Object.keys(perJob.jobs)[0]!]!["cache-mode"] = mode;
    expect(
      () =>
        checkPullRequestTargetWorkflow(
          Bun.YAML.stringify(perJob),
          "keldbot.yml",
        ),
      `job ${mode}`,
    ).toThrow("cache-mode");
  }
});

test("no other step may use actions/cache, so apt index lists are never cached", () => {
  const f = fixture();
  f.jobs.fmt!.steps.push({
    name: "Cache apt lists",
    uses: "actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9",
    with: { path: "/var/lib/apt/lists", key: "lists" },
  });
  expect(() => check(f)).toThrow("outside the WebKitGTK .deb cache steps");
});

test("no job outside the per-language owners may run CodeQL", () => {
  const f = fixture();
  f.jobs.fmt!.steps.push({ ...step(f, "codeql-rust", "Initialize CodeQL") });
  expect(() => check(f)).toThrow("outside the per-language jobs");
});

// #676: cargo-deny comes from the checksum-pinned release binary, not a Docker image.
const [cargoDenyMain, cargoDenyHelper] = cargoDenyChecks.map(
  ([name]) => name,
) as [string, string];

function editRun(f: Fixture, name: string, from: string, to: string): void {
  const selected = step(f, "deny", name);
  const run = String(selected.run);
  if (!run.includes(from)) throw new Error(`fixture ${name} lacks ${from}`);
  selected.run = run.replace(from, to);
}

test("cargo-deny installs the pinned version and verifies its recorded SHA-256", () => {
  const install = String(step(fixture(), "deny", cargoDenyInstallStep).run);
  const shaLine = `echo "${cargoDenyArchiveSha256}  \${release}.tar.gz" | sha256sum -c -\n`;
  expect(install).toContain(`version=${cargoDenyVersion}\n`);
  expect(install).toContain(shaLine);
  const mutations: [string, (f: Fixture) => void][] = [
    [
      "unpinned version",
      (f) =>
        editRun(
          f,
          cargoDenyInstallStep,
          `version=${cargoDenyVersion}`,
          "version=latest",
        ),
    ],
    [
      "version taken from the environment",
      (f) =>
        editRun(
          f,
          cargoDenyInstallStep,
          `version=${cargoDenyVersion}`,
          'version="${CARGO_DENY_VERSION:-0.20.2}"',
        ),
    ],
    [
      "latest-release URL",
      (f) =>
        editRun(
          f,
          cargoDenyInstallStep,
          "releases/download/${version}/",
          "releases/latest/download/",
        ),
    ],
    ["missing checksum", (f) => editRun(f, cargoDenyInstallStep, shaLine, "")],
    [
      "different checksum",
      (f) =>
        editRun(
          f,
          cargoDenyInstallStep,
          cargoDenyArchiveSha256,
          "0".repeat(64),
        ),
    ],
    [
      "suppressed checksum",
      (f) =>
        editRun(
          f,
          cargoDenyInstallStep,
          "sha256sum -c -",
          "sha256sum -c - || true",
        ),
    ],
    [
      "verify after extracting",
      (f) => {
        editRun(f, cargoDenyInstallStep, shaLine, "");
        editRun(
          f,
          cargoDenyInstallStep,
          "cargo-deny/cargo-deny --version\n",
          `cargo-deny/cargo-deny --version\n${shaLine}`,
        );
      },
    ],
  ];
  for (const [label, mutate] of mutations) {
    const f = fixture();
    mutate(f);
    expect(() => check(f), label).toThrow(
      `${cargoDenyInstallStep} must download release ${cargoDenyVersion}`,
    );
  }
});

test("cargo-deny steps cannot continue on error, be conditioned, removed, duplicated or reordered", () => {
  for (const name of [cargoDenyInstallStep, cargoDenyMain, cargoDenyHelper]) {
    for (const [key, value] of [
      ["continue-on-error", true],
      ["if", false],
      ["shell", "bash {0} || true"],
    ] as const) {
      const f = fixture();
      step(f, "deny", name)[key] = value;
      expect(() => check(f), `${name} ${key}`).toThrow(
        `${name} must contain only name, run`,
      );
    }
    const removed = fixture();
    removed.jobs.deny!.steps = removed.jobs.deny!.steps.filter(
      (candidate) => candidate.name !== name,
    );
    expect(() => check(removed), `${name} removed`).toThrow(
      "jobs.deny must run exactly",
    );
    const duplicated = fixture();
    duplicated.jobs.deny!.steps.push({ ...step(duplicated, "deny", name) });
    expect(() => check(duplicated), `${name} duplicated`).toThrow(
      "jobs.deny must run exactly",
    );
  }
  const reordered = fixture();
  const steps = reordered.jobs.deny!.steps;
  const install = steps.findIndex(
    (candidate) => candidate.name === cargoDenyInstallStep,
  );
  const [moved] = steps.splice(install, 1);
  steps.push(moved!);
  expect(() => check(reordered)).toThrow("jobs.deny must run exactly");
});

test("cargo-deny runs the verified binary with the reviewed policy arguments", () => {
  const mutations: [string, string, string, string][] = [
    [
      "PATH-resolved binary",
      cargoDenyMain,
      '"$RUNNER_TEMP/cargo-deny/cargo-deny"',
      "cargo deny",
    ],
    ["default features only", cargoDenyMain, " --all-features check", " check"],
    [
      "advisories skipped",
      cargoDenyMain,
      "--all-features check",
      "--all-features check bans licenses sources",
    ],
    [
      "swallowed result",
      cargoDenyMain,
      "--all-features check",
      "--all-features check || true",
    ],
    [
      "helper licenses instead of bans",
      cargoDenyHelper,
      "check bans",
      "check licenses",
    ],
    [
      "helper without its own ban list",
      cargoDenyHelper,
      " --config crates/keld-updater-helper/deny.toml",
      "",
    ],
    [
      "helper rooted at the workspace",
      cargoDenyHelper,
      "crates/keld-updater-helper/Cargo.toml",
      "./Cargo.toml",
    ],
  ];
  for (const [label, name, from, to] of mutations) {
    const f = fixture();
    editRun(f, name, from, to);
    expect(() => check(f), label).toThrow(`${name} must run exactly`);
  }
});

test("the deny job admits no Docker action, including the former cargo-deny-action", () => {
  for (const uses of [
    "EmbarkStudios/cargo-deny-action@3c6349835b2b7b196a839186cb8b78e02f7b5f25",
    "taiki-e/install-action@4cef1412cce204788f482e778a0b9187f9626a29",
  ]) {
    const f = fixture();
    const steps = f.jobs.deny!.steps;
    steps.splice(
      steps.findIndex((candidate) => candidate.name === cargoDenyMain),
      0,
      { uses, with: { command: "check" } },
    );
    const reason = uses.startsWith("taiki-e/")
      ? "taiki installer is not approved in jobs.deny"
      : "jobs.deny must run exactly";
    expect(() => check(f), uses).toThrow(reason);
  }
  const missing = fixture();
  delete missing.jobs.deny;
  expect(() => check(missing)).toThrow("jobs.deny must exist");
});

test("nothing in the deny job can replace or redirect the verified binary", () => {
  // An unnamed or named step between install and check could overwrite the binary.
  for (const inserted of [
    {
      run: `printf '#!/bin/sh\\nexit 0\\n' > "$RUNNER_TEMP/cargo-deny/cargo-deny"\n`,
    },
    { name: "Refresh cargo-deny", run: "cargo install cargo-deny --locked\n" },
  ]) {
    const f = fixture();
    const steps = f.jobs.deny!.steps;
    steps.splice(
      steps.findIndex((candidate) => candidate.name === cargoDenyMain),
      0,
      inserted,
    );
    expect(() => check(f), JSON.stringify(inserted)).toThrow(
      "jobs.deny must run exactly",
    );
  }
  for (const [key, value] of [
    ["env", { BASH_ENV: "./tools/noop.sh" }],
    ["defaults", { run: { shell: "bash --noprofile --norc {0} || true" } }],
    ["container", "rust:1.97.1"],
    ["services", { registry: { image: "registry:2" } }],
  ] as const) {
    const f = fixture();
    (f.jobs.deny as Record<string, unknown>)[key] = value;
    expect(() => check(f), key).toThrow(`jobs.deny must not set ${key}`);
  }
  // The policy must read the event's own tree, not another ref.
  const ref = fixture();
  (ref.jobs.deny!.steps[0]!.with as Step).ref = "main";
  expect(() => check(ref)).toThrow("event-owned checkout");
});

for (const style of ["block", "flow", "alias"] as const) {
  for (const secure of [true, false]) {
    test(`${style} checkout with persistence ${secure ? "disabled" : "enabled"}`, () => {
      checkWorkflowSecurity(source);
      const flag = secure ? "false" : "true";
      const inserted =
        style === "block"
          ? `      - name: Extra checkout\n        uses: ${checkout}\n        with:\n          persist-credentials: ${flag}\n`
          : style === "flow"
            ? `      - { uses: '${checkout}', with: { persist-credentials: ${flag} } }\n`
            : `      - &shared_checkout { uses: '${checkout}', with: { persist-credentials: ${flag} } }\n      - *shared_checkout\n`;
      const mutated = source.replace(
        "      - name: Initialize CodeQL\n",
        inserted + "      - name: Initialize CodeQL\n",
      );
      // Credentials-disabled alone is insufficient for a privileged CodeQL
      // job: every additional step must also be reviewed and inventoried.
      if (secure)
        expect(() => checkWorkflowSecurity(mutated)).toThrow(
          "CodeQL reviewed step inventory",
        );
      else
        expect(() => checkWorkflowSecurity(mutated)).toThrow(
          "persist-credentials: false",
        );
    });
  }
}

test("every checkout must be protected; one secure step cannot mask another", () => {
  const base = fixture();
  let checked = 0;
  for (const [jobName, job] of Object.entries(base.jobs)) {
    for (const [index, candidate] of job.steps.entries()) {
      if (
        typeof candidate.uses !== "string" ||
        !candidate.uses.startsWith("actions/checkout@")
      )
        continue;
      for (const value of [true, undefined]) {
        const f = fixture();
        const inputs = f.jobs[jobName]!.steps[index]!.with as Step;
        if (value === undefined) delete inputs["persist-credentials"];
        else inputs["persist-credentials"] = value;
        expect(() => check(f)).toThrow("persist-credentials: false");
      }
      checked++;
    }
  }
  expect(checked).toBeGreaterThan(1);
});

test("every CI checkout must use the event-owned repository and source tree", () => {
  const workflow = fixture();
  const forbiddenInputs: Array<[string, unknown]> = [
    ["ref", "refs/heads/main"],
    ["repository", "untrusted/keld"],
    ["path", "other-checkout"],
    ["sparse-checkout", "tools/**"],
    ["filter", "blob:none"],
    ["submodules", true],
    ["token", "unreviewed-token"],
    ["ssh-key", "unreviewed-key"],
    ["clean", false],
    ["fetch-depth", 1],
  ];
  let checked = 0;
  for (const [jobName, job] of Object.entries(workflow.jobs)) {
    for (const [index, candidate] of job.steps.entries()) {
      if (
        typeof candidate.uses !== "string" ||
        !candidate.uses.toLowerCase().startsWith("actions/checkout@")
      )
        continue;
      for (const [input, value] of forbiddenInputs) {
        const mutated = fixture();
        (mutated.jobs[jobName]!.steps[index]!.with as Step)[input] = value;
        expect(() => check(mutated), jobName + " checkout " + input).toThrow(
          "event-owned checkout",
        );
      }
      checked++;
    }
  }
  expect(checked).toBeGreaterThan(10);
});

test("flow-style and aliased checkouts cannot select another source", () => {
  const examples: Array<[string, string]> = [
    [
      "flow ref",
      "      - { uses: '" +
        checkout +
        "', with: { persist-credentials: false, ref: refs/heads/main } }\n",
    ],
    [
      "flow repository",
      "      - { uses: '" +
        checkout +
        "', with: { persist-credentials: false, repository: untrusted/keld } }\n",
    ],
    [
      "aliased ref",
      "      - &source_checkout { uses: '" +
        checkout +
        "', with: { persist-credentials: false, ref: refs/heads/main } }\n      - *source_checkout\n",
    ],
    [
      "case and path alias",
      "      - { uses: 'Actions/Checkout/.@3d3c42e5aac5ba805825da76410c181273ba90b1', with: { persist-credentials: false, path: other-checkout } }\n",
    ],
  ];
  for (const [label, inserted] of examples) {
    const mutated = source.replace(
      "      - name: Initialize CodeQL\n",
      inserted + "      - name: Initialize CodeQL\n",
    );
    expect(mutated, label + " mutation applied").not.toBe(source);
    expect(() => checkWorkflowSecurity(mutated), label).toThrow(
      "event-owned checkout",
    );
  }
});

test("checkout repository case and subpaths cannot bypass its policy", () => {
  for (const name of ["ACTIONS/CHECKOUT", "Actions/Checkout/."]) {
    for (const protectedCheckout of [true, false]) {
      const f = fixture();
      f.jobs["codeql-rust"]!.steps.push({
        uses: `${name}@3d3c42e5aac5ba805825da76410c181273ba90b1`,
        with: { "persist-credentials": !protectedCheckout },
      });
      if (protectedCheckout)
        expect(() => check(f)).toThrow("CodeQL reviewed step inventory");
      else expect(() => check(f)).toThrow("persist-credentials: false");
    }
  }
});

for (const [job, name] of [
  ...codeqlJobs.flatMap(
    (job) =>
      [
        [job, "Initialize CodeQL"],
        [job, "Analyze and upload CodeQL results"],
      ] as const,
  ),
  ["dependency-review", "Review dependency vulnerabilities"],
  ["dependency-review", "Reject incomplete dependency metadata"],
] as const) {
  test(`${job} ${name} cannot be skipped, removed or duplicated`, () => {
    for (const mutation of ["condition", "remove", "duplicate"] as const) {
      const f = fixture();
      const selected = step(f, job, name);
      if (mutation === "condition") selected.if = false;
      if (mutation === "remove")
        f.jobs[job]!.steps = f.jobs[job]!.steps.filter((s) => s !== selected);
      if (mutation === "duplicate") f.jobs[job]!.steps.push(selected);
      expect(() => check(f)).toThrow();
    }
  });
}

for (const [job, name, key, value] of [
  ...codeqlJobs.flatMap(
    (job) =>
      [
        [job, "Analyze and upload CodeQL results", "upload", "never"],
        [job, "Analyze and upload CodeQL results", "skip-queries", true],
        [
          job,
          "Analyze and upload CodeQL results",
          "wait-for-processing",
          false,
        ],
      ] as const,
  ),
  [
    "dependency-review",
    "Review dependency vulnerabilities",
    "vulnerability-check",
    false,
  ],
  ["dependency-review", "Review dependency vulnerabilities", "warn-only", true],
  [
    "dependency-review",
    "Review dependency vulnerabilities",
    "fail-on-severity",
    "critical",
  ],
  [
    "dependency-review",
    "Review dependency vulnerabilities",
    "fail-on-scopes",
    "runtime",
  ],
  [
    "dependency-review",
    "Review dependency vulnerabilities",
    "allow-ghsas",
    "GHSA-xxxx-yyyy-zzzz",
  ],
] as const) {
  test(`required scanner input ${job} ${key} cannot bypass its effect`, () => {
    const f = fixture();
    (step(f, job, name).with as Step)[key] = value;
    expect(() => check(f)).toThrow();
  });
}

test("wrong or floating action identity fails, including flow mappings", () => {
  for (const uses of [
    "github/codeql-action/init@cdf488f595d80d6e07e03d4674febd5ab45fa938",
    "github/codeql-action/analyze@main",
  ]) {
    const f = fixture();
    step(f, "codeql-rust", "Analyze and upload CodeQL results").uses = uses;
    expect(() => checkWorkflowSecurity(Bun.YAML.stringify(f))).toThrow();
  }
  const f = fixture();
  f.jobs["codeql-rust"]!.steps.push({
    uses: "actions/checkout@main",
    with: { "persist-credentials": false },
  });
  expect(() => check(f)).toThrow("immutable");
});

test("metadata admission cannot echo/swallow checks or change event refs", () => {
  for (const mutation of ["echo", "swallow", "ref"] as const) {
    const f = fixture();
    const selected = step(
      f,
      "dependency-review",
      "Reject incomplete dependency metadata",
    );
    if (mutation === "ref")
      (selected.env as Step).KELD_DEPENDENCY_BASE = "${{ github.sha }}";
    else
      selected.run =
        mutation === "echo"
          ? `echo ${selected.run}`
          : `${String(selected.run).trim()} || true\n`;
    expect(() => check(f)).toThrow("metadata");
  }
});

test("scanner prerequisites cannot be reordered", () => {
  const f = fixture();
  f.jobs["codeql-rust"]!.steps.reverse();
  expect(() => check(f)).toThrow("initialization must precede");
  const d = fixture();
  d.jobs["dependency-review"]!.steps.reverse();
  expect(() => check(d)).toThrow("metadata admission must precede");
});

for (const invalid of [
  "",
  "jobs: [",
  "---\na: 1\n---\nb: 2",
  "jobs: {}",
  "jobs: []",
  "jobs: {x: {steps: [false]}}",
  "jobs: {x: {steps: [[{}]]}}",
  "jobs: {x: {uses: owner/reusable@main}}",
  "jobs: &jobs { x: *jobs }",
]) {
  test(`missing/malformed/unknown workflow shape refuses: ${JSON.stringify(invalid)}`, () => {
    expect(() => checkWorkflowSecurity(invalid)).toThrow();
  });
}

test("Bun duplicate-key behavior is documented, not claimed as syntax validation", () => {
  expect(Bun.YAML.parse("x: true\nx: false")).toEqual({ x: false });
  expect(() =>
    checkWorkflowSecurity(
      source.replace(
        "warn-only: false",
        "warn-only: false\n          warn-only: true",
      ),
    ),
  ).toThrow("warn-only");
});

test("parsing budget admits the boundary and refuses one extra byte", () => {
  const exact =
    source + "\n#" + "x".repeat(1024 * 1024 - Buffer.byteLength(source) - 2);
  expect(() => checkWorkflowSecurity(exact)).not.toThrow();
  expect(() => checkWorkflowSecurity(exact + "x")).toThrow("1 MiB");
});

test("equivalent scalar quotes and scope order preserve effects", () => {
  expect(() =>
    checkWorkflowSecurity(
      source
        .replace("warn-only: false", "'warn-only': 'false'")
        .replace(
          "fail-on-scopes: runtime, development, unknown",
          "fail-on-scopes: unknown,runtime,development",
        ),
    ),
  ).not.toThrow();
});

test("existing Rust CLI invokes semantic admission and preserves its refusal", () => {
  const repository = join(import.meta.dir, "..");
  const temporary = mkdtempSync(join(tmpdir(), "keld-workflow-security-"));
  const started = performance.now();
  let phase = "list tracked snapshot sources";
  let sourceFiles = 0;
  let cliInvocations = 0;
  try {
    const checkoutRoot = join(temporary, "checkout");
    mkdirSync(checkoutRoot);
    const listing = Bun.spawnSync(["git", "-C", repository, "ls-files", "-z"]);
    expect(listing.exitCode).toBe(0);
    const files = new Set(
      new TextDecoder().decode(listing.stdout).split("\0").filter(Boolean),
    );
    files.add("tools/ci_workflow_security.ts");
    sourceFiles = files.size;
    phase = "copy tracked snapshot sources";
    for (const relative of files) {
      const destination = join(checkoutRoot, relative);
      mkdirSync(dirname(destination), { recursive: true });
      cpSync(join(repository, relative), destination, { dereference: false });
    }
    // `ci-hygiene check` compares the GH-508 channel allocation baseline with a
    // merge base, so the copied checkout is a git repository compared to its HEAD.
    const git = (...args: string[]) => {
      phase = `fixture git ${args[0]}`;
      return Bun.spawnSync([
        "git",
        "-C",
        checkoutRoot,
        "-c",
        "user.name=fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "init.defaultBranch=main",
        ...args,
      ]);
    };
    expect(git("init", "--quiet").exitCode).toBe(0);
    // No detached `git maintenance` repack may outlive the commit and race cleanup (#670).
    expect(git("config", "maintenance.auto", "false").exitCode).toBe(0);
    expect(git("add", "-A").exitCode).toBe(0);
    expect(
      git("commit", "--quiet", "--no-verify", "-m", "fixture").exitCode,
    ).toBe(0);
    const binary = join(
      temporary,
      process.platform === "win32" ? "ci-hygiene.exe" : "ci-hygiene",
    );
    phase = "compile Rust CLI";
    const compilation = Bun.spawnSync([
      "rustc",
      "--edition=2024",
      "-D",
      "warnings",
      join(repository, "tools/ci_hygiene.rs"),
      "-o",
      binary,
    ]);
    expect(compilation.exitCode).toBe(0);
    // The CLI must use the same verified Bun executable as this test process.
    const runtimeEnv = {
      ...process.env,
      KELD_CI_BASE_REF: "HEAD",
      PATH: `${dirname(process.execPath)}${delimiter}${process.env.PATH ?? ""}`,
    };
    const run = (yaml: string, env = runtimeEnv) => {
      phase = `semantic CLI invocation ${++cliInvocations}`;
      writeFileSync(join(checkoutRoot, ".github/workflows/ci.yml"), yaml);
      return Bun.spawnSync([binary, "check", checkoutRoot], {
        cwd: repository,
        env,
      });
    };
    const original = run(source);
    expect(original.exitCode).toBe(0);
    // The CLI also refuses a pull_request_target workflow that runs shell code.
    const keldbotPath = join(checkoutRoot, ".github/workflows/keldbot.yml");
    writeFileSync(
      keldbotPath,
      keldbot.replace(
        "    steps:\n",
        "    steps:\n      - run: echo injected\n",
      ),
    );
    const prTarget = run(source);
    expect(prTarget.exitCode).not.toBe(0);
    expect(new TextDecoder().decode(prTarget.stderr)).toContain(
      "pull_request_target",
    );
    writeFileSync(keldbotPath, keldbot);
    expect(new TextDecoder().decode(original.stdout)).toContain(
      "CI workflow security semantics ok",
    );
    const actionsRoute = `    if: ${codeqlRoute("actions")}\n`;
    expect(source).toContain(actionsRoute);
    const misrouted = run(
      source.replace(actionsRoute, `    if: ${codeqlRoute("rust")}\n`),
    );
    expect(misrouted.exitCode).not.toBe(0);
    expect(new TextDecoder().decode(misrouted.stderr)).toContain(
      "CodeQL job codeql-actions",
    );
    for (const insertion of [
      `      - { uses: '${checkout}', with: { persist-credentials: true } }\n`,
      `      - &insecure { uses: '${checkout}', with: { persist-credentials: true } }\n      - *insecure\n`,
    ]) {
      const result = run(
        source.replace(
          "      - name: Initialize CodeQL\n",
          insertion + "      - name: Initialize CodeQL\n",
        ),
      );
      expect(result.exitCode).not.toBe(0);
      expect(new TextDecoder().decode(result.stderr)).toContain(
        "persist-credentials: false",
      );
    }
    const skipped = run(
      source.replace(
        "      - name: Analyze and upload CodeQL results\n",
        "      - name: Analyze and upload CodeQL results\n        if: false\n",
      ),
    );
    expect(skipped.exitCode).not.toBe(0);
    expect(new TextDecoder().decode(skipped.stderr)).toContain(
      "conditions or unknown controls",
    );
    const weakenedUpload = run(
      source.replace(
        "      security-events: write",
        "      security-events: read",
      ),
    );
    expect(weakenedUpload.exitCode).not.toBe(0);
    expect(new TextDecoder().decode(weakenedUpload.stderr)).toContain(
      "CodeQL job permissions",
    );
    expect(run(source, { ...runtimeEnv, PATH: temporary }).exitCode).not.toBe(
      0,
    );
    rmSync(join(checkoutRoot, "tools/ci_workflow_security.ts"));
    expect(run(source).exitCode).not.toBe(0);
  } catch (error) {
    console.error(
      "workflow security fixture failed:",
      JSON.stringify({
        phase,
        elapsedMs: Math.round(performance.now() - started),
        sourceFiles,
        cliInvocations,
      }),
    );
    throw error;
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
  // Snapshot/Git setup and rustc precede all semantic checks. The original 30s
  // kill switch expired during Git staging in full CI (GH1091); bound the full
  // cold-tool fixture while retaining every acceptance and refusal assertion.
}, 120_000);

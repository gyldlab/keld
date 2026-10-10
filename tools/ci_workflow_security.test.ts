import { expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, dirname, join } from "node:path";
import { aptStepTimeoutMinutes, webkitgtkAptJobs, webkitgtkDebCachePath, checkPullRequestTargetWorkflow, checkWindowsMediaOracle, checkWorkflowSecurity, codeqlLanguages, codeqlRoute } from "./ci_workflow_security";

type Step = Record<string, unknown>;
type FixtureJob = {
  steps: Step[];
  env?: Record<string, unknown>;
  permissions?: Record<string, unknown>;
  strategy?: { matrix: Record<string, unknown> };
  needs?: unknown;
  if?: unknown;
  "timeout-minutes"?: unknown;
};
type Fixture = { jobs: Record<string, FixtureJob> };
const source = readFileSync(join(import.meta.dir, "../.github/workflows/ci.yml"), "utf8");
const windowsMediaOracle = readFileSync(join(import.meta.dir, "../crates/keld-wv/tests/windows_media_guard.ps1"));
const checkout = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1";

function fixture(): Fixture {
  checkWorkflowSecurity(source);
  return Bun.YAML.parse(source) as Fixture;
}

function step(f: Fixture, job: string, name: string): Step {
  const found = f.jobs[job]!.steps.find(candidate => candidate.name === name);
  if (!found) throw new Error(`missing fixture step: ${name}`);
  return found;
}

function check(f: Fixture): void { checkWorkflowSecurity(Bun.YAML.stringify(f, null, 2)); }

test("actual workflow passes parsed security admission", () => {
  expect(() => checkWorkflowSecurity(source)).not.toThrow();
});

test("Windows media oracle matches its reviewed full-file digest", () => {
  expect(() => checkWindowsMediaOracle(windowsMediaOracle)).not.toThrow();
  const mutated = Buffer.concat([windowsMediaOracle, Buffer.from("\n# inert override\n")]);
  expect(() => checkWindowsMediaOracle(mutated)).toThrow("reviewed SHA-256 owner");
});

const codeqlJobs = codeqlLanguages.map(language => `codeql-${language}`);

for (const language of codeqlLanguages) {
  const job = `codeql-${language}`;
  const other = codeqlLanguages.find(candidate => candidate !== language)!;

  test(`CodeQL cannot omit the ${language} job`, () => {
    const f = fixture();
    delete f.jobs[job];
    expect(() => check(f)).toThrow(`CodeQL job ${job} must exist`);
  });

  test(`CodeQL ${language} routes only on its own router output`, () => {
    expect(fixture().jobs[job]!.if).toBe(codeqlRoute(language));
    for (const mutation of ["other-language", "always", "missing", "push-bypass", "no-needs", "extra-needs"] as const) {
      const f = fixture();
      const target = f.jobs[job]!;
      if (mutation === "other-language") target.if = codeqlRoute(other);
      if (mutation === "always") target.if = true;
      if (mutation === "missing") delete target.if;
      if (mutation === "push-bypass") target.if = `github.event_name == 'push' || ${codeqlRoute(language)}`;
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
    (step(category, job, "Analyze and upload CodeQL results").with as Step).category = `/language:${other}`;
    expect(() => check(category)).toThrow("Analyze and upload CodeQL results");
    const matrix = fixture();
    matrix.jobs[job]!.strategy = { matrix: { language: [language] } };
    expect(() => check(matrix)).toThrow("must not set strategy");
  });

  test(`CodeQL ${language} permissions retain only read content and SARIF-upload authority`, () => {
    for (const mutation of ["missing", "security-events-read", "contents-write", "extra"] as const) {
      const f = fixture();
      const permissions = f.jobs[job]!.permissions!;
      if (mutation === "missing") delete f.jobs[job]!.permissions;
      if (mutation === "security-events-read") permissions["security-events"] = "read";
      if (mutation === "contents-write") permissions.contents = "write";
      if (mutation === "extra") permissions.actions = "read";
      expect(() => check(f)).toThrow("CodeQL job permissions");
    }
  });
}

const aptSteps = [
  ["check", "Install WebKitGTK build deps (KEL-28, see keld-wv/Cargo.toml)"],
  ["linux-gui-smoke", "Install WebKitGTK build deps + X11 control tools (KEL-28)"],
] as const;

for (const [job, name] of aptSteps) {
  test(`${name} keeps the evidence-based step timeout under a longer job timeout`, () => {
    expect(step(fixture(), job, name)["timeout-minutes"]).toBe(aptStepTimeoutMinutes);
    // Negative controls: the 10-minute bound that failed a slow but healthy
    // download, and every other value or shape.
    for (const value of [undefined, 10, 0, aptStepTimeoutMinutes - 1, aptStepTimeoutMinutes + 1, "${{ inputs.minutes }}", "15m"]) {
      const f = fixture();
      const selected = step(f, job, name);
      if (value === undefined) delete selected["timeout-minutes"];
      else selected["timeout-minutes"] = value;
      expect(() => check(f)).toThrow(`runs apt without step timeout-minutes: ${aptStepTimeoutMinutes}`);
    }
    for (const value of [aptStepTimeoutMinutes, 10, "${{ inputs.minutes }}"]) {
      const f = fixture();
      f.jobs[job]!["timeout-minutes"] = value;
      expect(() => check(f)).toThrow("does not exceed the 15-minute apt step bound");
    }
  });
}

test("apt timeout reads the parsed run script, not names or layout", () => {
  for (const run of ["sudo apt update\n", "sudo apt install -y jq\n", "set -e; sudo apt-get install -y jq\n"]) {
    const f = fixture();
    f.jobs.check!.steps.push({ name: "Install a tool", run });
    expect(() => check(f)).toThrow("runs apt without step timeout-minutes");
  }
  // Negative controls: a name that mentions apt-get, a flow-mapped step with a
  // timeout, a quoted timeout and an unrelated word do not fail.
  const named = fixture();
  named.jobs.fmt!.steps.push({ name: "Diagnose apt-get mirror", run: "echo ok\n" });
  expect(() => check(named)).not.toThrow();
  expect(() => checkWorkflowSecurity(source.replace("      - name: clippy (warnings deny)\n",
    "      - { run: 'sudo apt-get install -y jq', timeout-minutes: 15 }\n      - name: clippy (warnings deny)\n"))).not.toThrow();
  const quoted = fixture();
  quoted.jobs.check!.steps.push({ run: "sudo apt-get install -y jq\n", "timeout-minutes": "15" });
  expect(() => check(quoted)).not.toThrow();
  const unrelated = fixture();
  unrelated.jobs.fmt!.steps.push({ run: "echo adapter aptitude\n" });
  expect(() => check(unrelated)).not.toThrow();
});

for (const job of webkitgtkAptJobs) {
  test(`${job} WebKitGTK .deb cache keeps its key, verified install and miss-only save`, () => {
    const mutations: [string, (f: Fixture) => void, string][] = [
      ["key without the image and package binding", f => { step(f, job, "Resolve WebKitGTK apt cache key").run = 'echo "key=webkitgtk" >> "$GITHUB_OUTPUT"\n'; }, "resolve the cache key"],
      ["restore under another key", f => { (step(f, job, "Restore WebKitGTK .deb cache").with as Step).key = "webkitgtk-${{ runner.os }}"; }, "Restore WebKitGTK .deb cache"],
      ["cache the apt index lists", f => { (step(f, job, "Restore WebKitGTK .deb cache").with as Step).path = "/var/lib/apt/lists"; }, "Restore WebKitGTK .deb cache"],
      ["save on a hit", f => { step(f, job, "Save WebKitGTK .deb cache").if = String(step(f, job, "Restore WebKitGTK .deb cache").if ?? "always()"); }, "only on a miss"],
      ["save under another key", f => { (step(f, job, "Save WebKitGTK .deb cache").with as Step).key = "other"; }, "Save WebKitGTK .deb cache"],
      ["install bypassing verification", f => { const s = f.jobs[job]!.steps.find(c => typeof c.run === "string" && c.run.includes("ci_webkitgtk_apt.sh install"))!; s.run = `sudo cp ${webkitgtkDebCachePath}/*.deb /var/cache/apt/archives/ && sudo apt-get update && sudo apt-get install -y $KELD_WEBKITGTK_PACKAGES\n`; }, "install with exactly"],
      ["missing package list", f => { delete (f.jobs[job]!.env as Step).KELD_WEBKITGTK_PACKAGES; }, "KELD_WEBKITGTK_PACKAGES"],
      ["save before install", f => { const steps = f.jobs[job]!.steps; const save = steps.findIndex(c => c.name === "Save WebKitGTK .deb cache"); const [s] = steps.splice(save, 1); steps.splice(save - 1, 0, s!); }, "key, restore, install, save"],
      ["floating cache action", f => { step(f, job, "Restore WebKitGTK .deb cache").uses = "actions/cache/restore@v6"; }, "immutable"],
      ["unbounded restore", f => { delete step(f, job, "Restore WebKitGTK .deb cache")["timeout-minutes"]; }, "Restore WebKitGTK .deb cache"],
      ["restore bound above 5 minutes", f => { step(f, job, "Restore WebKitGTK .deb cache")["timeout-minutes"] = 30; }, "timeout-minutes: 5"],
      ["unbounded save", f => { delete step(f, job, "Save WebKitGTK .deb cache")["timeout-minutes"]; }, "Save WebKitGTK .deb cache"],
      ["stuck download held to the step bound", f => { (step(f, job, "Restore WebKitGTK .deb cache").env as Step).SEGMENT_DOWNLOAD_TIMEOUT_MINS = "10"; }, "abort as a miss"],
    ];
    for (const [label, mutate, message] of mutations) {
      const f = fixture();
      mutate(f);
      expect(() => check(f), label).toThrow(message);
    }
  });
}

const keldbot = readFileSync(join(import.meta.dir, "../.github/workflows/keldbot.yml"), "utf8");

test("pull_request_target workflows never check out or run pull-request code", () => {
  expect(() => checkPullRequestTargetWorkflow(keldbot, "keldbot.yml")).not.toThrow();
  const parsed = Bun.YAML.parse(keldbot) as { jobs: Record<string, { steps: Step[] }> };
  const firstJob = Object.keys(parsed.jobs)[0]!;
  for (const [label, inserted, message] of [
    ["run step", { run: "echo ${{ github.event.pull_request.title }}" }, "run: step under pull_request_target"],
    ["checkout", { uses: checkout, with: { ref: "${{ github.event.pull_request.head.sha }}" } }, "checks out code"],
    ["checkout with case and subpath", { uses: "Actions/Checkout/.@3d3c42e5aac5ba805825da76410c181273ba90b1" }, "checks out code"],
  ] as const) {
    const f = Bun.YAML.parse(keldbot) as typeof parsed;
    f.jobs[firstJob]!.steps.push({ ...inserted });
    expect(() => checkPullRequestTargetWorkflow(Bun.YAML.stringify(f), "keldbot.yml"), label).toThrow(message);
  }
  const reusable = Bun.YAML.parse(keldbot) as Record<string, unknown> & typeof parsed;
  (reusable.jobs as Record<string, unknown>).extra = { uses: "owner/repo/.github/workflows/x.yml@3d3c42e5aac5ba805825da76410c181273ba90b1" };
  expect(() => checkPullRequestTargetWorkflow(Bun.YAML.stringify(reusable), "keldbot.yml")).toThrow("reusable workflow");
  // Every trigger form is recognised, including flow and alias steps.
  for (const on of ["pull_request_target", "[push, pull_request_target]"]) {
    expect(() => checkPullRequestTargetWorkflow(`on: ${on}\njobs:\n  j:\n    steps:\n      - &s { run: echo }\n      - *s\n`, "x.yml")).toThrow("run: step");
  }
  // Negative control: the same run step under pull_request is not this rule's concern.
  expect(() => checkPullRequestTargetWorkflow("on: pull_request\njobs:\n  j:\n    steps:\n      - run: echo\n", "x.yml")).not.toThrow();
});

test("no other step may use actions/cache, so apt index lists are never cached", () => {
  const f = fixture();
  f.jobs.fmt!.steps.push({ name: "Cache apt lists", uses: "actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9", with: { path: "/var/lib/apt/lists", key: "lists" } });
  expect(() => check(f)).toThrow("outside the WebKitGTK .deb cache steps");
});

test("no job outside the per-language owners may run CodeQL", () => {
  const f = fixture();
  f.jobs.fmt!.steps.push({ ...step(f, "codeql-rust", "Initialize CodeQL") });
  expect(() => check(f)).toThrow("outside the per-language jobs");
});

for (const style of ["block", "flow", "alias"] as const) {
  for (const secure of [true, false]) {
    test(`${style} checkout with persistence ${secure ? "disabled" : "enabled"}`, () => {
      checkWorkflowSecurity(source);
      const flag = secure ? "false" : "true";
      const inserted = style === "block"
        ? `      - name: Extra checkout\n        uses: ${checkout}\n        with:\n          persist-credentials: ${flag}\n`
        : style === "flow"
          ? `      - { uses: '${checkout}', with: { persist-credentials: ${flag} } }\n`
          : `      - &shared_checkout { uses: '${checkout}', with: { persist-credentials: ${flag} } }\n      - *shared_checkout\n`;
      const mutated = source.replace("      - name: Initialize CodeQL\n", inserted + "      - name: Initialize CodeQL\n");
      if (secure) expect(() => checkWorkflowSecurity(mutated)).not.toThrow();
      else expect(() => checkWorkflowSecurity(mutated)).toThrow("persist-credentials: false");
    });
  }
}

test("every checkout must be protected; one secure step cannot mask another", () => {
  const base = fixture();
  let checked = 0;
  for (const [jobName, job] of Object.entries(base.jobs)) {
    for (const [index, candidate] of job.steps.entries()) {
      if (typeof candidate.uses !== "string" || !candidate.uses.startsWith("actions/checkout@")) continue;
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

test("checkout repository case and subpaths cannot bypass its policy", () => {
  for (const name of ["ACTIONS/CHECKOUT", "Actions/Checkout/."]) {
    for (const protectedCheckout of [true, false]) {
      const f = fixture();
      f.jobs["codeql-rust"]!.steps.push({ uses: `${name}@3d3c42e5aac5ba805825da76410c181273ba90b1`, with: { "persist-credentials": !protectedCheckout } });
      if (protectedCheckout) expect(() => check(f)).not.toThrow();
      else expect(() => check(f)).toThrow("persist-credentials: false");
    }
  }
});

for (const [job, name] of [
  ...codeqlJobs.flatMap(job => [[job, "Initialize CodeQL"], [job, "Analyze and upload CodeQL results"]] as const),
  ["dependency-review", "Review dependency vulnerabilities"],
  ["dependency-review", "Reject incomplete dependency metadata"],
] as const) {
  test(`${job} ${name} cannot be skipped, removed or duplicated`, () => {
    for (const mutation of ["condition", "remove", "duplicate"] as const) {
      const f = fixture();
      const selected = step(f, job, name);
      if (mutation === "condition") selected.if = false;
      if (mutation === "remove") f.jobs[job]!.steps = f.jobs[job]!.steps.filter(s => s !== selected);
      if (mutation === "duplicate") f.jobs[job]!.steps.push(selected);
      expect(() => check(f)).toThrow();
    }
  });
}

for (const [job, name, key, value] of [
  ...codeqlJobs.flatMap(job => [
    [job, "Analyze and upload CodeQL results", "upload", "never"],
    [job, "Analyze and upload CodeQL results", "skip-queries", true],
    [job, "Analyze and upload CodeQL results", "wait-for-processing", false],
  ] as const),
  ["dependency-review", "Review dependency vulnerabilities", "vulnerability-check", false],
  ["dependency-review", "Review dependency vulnerabilities", "warn-only", true],
  ["dependency-review", "Review dependency vulnerabilities", "fail-on-severity", "critical"],
  ["dependency-review", "Review dependency vulnerabilities", "fail-on-scopes", "runtime"],
  ["dependency-review", "Review dependency vulnerabilities", "allow-ghsas", "GHSA-xxxx-yyyy-zzzz"],
] as const) {
  test(`required scanner input ${job} ${key} cannot bypass its effect`, () => {
    const f = fixture();
    (step(f, job, name).with as Step)[key] = value;
    expect(() => check(f)).toThrow();
  });
}

test("wrong or floating action identity fails, including flow mappings", () => {
  for (const uses of ["github/codeql-action/init@cdf488f595d80d6e07e03d4674febd5ab45fa938", "github/codeql-action/analyze@main"]) {
    const f = fixture();
    step(f, "codeql-rust", "Analyze and upload CodeQL results").uses = uses;
    expect(() => checkWorkflowSecurity(Bun.YAML.stringify(f))).toThrow();
  }
  const f = fixture();
  f.jobs["codeql-rust"]!.steps.push({ uses: "actions/checkout@main", with: { "persist-credentials": false } });
  expect(() => check(f)).toThrow("immutable");
});

test("metadata admission cannot echo/swallow checks or change event refs", () => {
  for (const mutation of ["echo", "swallow", "ref"] as const) {
    const f = fixture();
    const selected = step(f, "dependency-review", "Reject incomplete dependency metadata");
    if (mutation === "ref") (selected.env as Step).KELD_DEPENDENCY_BASE = "${{ github.sha }}";
    else selected.run = mutation === "echo" ? `echo ${selected.run}` : `${String(selected.run).trim()} || true\n`;
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

for (const invalid of ["", "jobs: [", "---\na: 1\n---\nb: 2", "jobs: {}", "jobs: []", "jobs: {x: {steps: [false]}}", "jobs: {x: {steps: [[{}]]}}", "jobs: {x: {uses: owner/reusable@main}}", "jobs: &jobs { x: *jobs }"]) {
  test(`missing/malformed/unknown workflow shape refuses: ${JSON.stringify(invalid)}`, () => {
    expect(() => checkWorkflowSecurity(invalid)).toThrow();
  });
}

test("Bun duplicate-key behavior is documented, not claimed as syntax validation", () => {
  expect(Bun.YAML.parse("x: true\nx: false")).toEqual({ x: false });
  expect(() => checkWorkflowSecurity(source.replace("warn-only: false", "warn-only: false\n          warn-only: true"))).toThrow("warn-only");
});

test("parsing budget admits the boundary and refuses one extra byte", () => {
  const exact = source + "\n#" + "x".repeat(1024 * 1024 - Buffer.byteLength(source) - 2);
  expect(() => checkWorkflowSecurity(exact)).not.toThrow();
  expect(() => checkWorkflowSecurity(exact + "x")).toThrow("1 MiB");
});

test("equivalent scalar quotes and scope order preserve effects", () => {
  expect(() => checkWorkflowSecurity(source.replace("warn-only: false", "'warn-only': 'false'")
    .replace("fail-on-scopes: runtime, development, unknown", "fail-on-scopes: unknown,runtime,development"))).not.toThrow();
});

test("existing Rust CLI invokes semantic admission and preserves its refusal", () => {
  const repository = join(import.meta.dir, "..");
  const temporary = mkdtempSync(join(tmpdir(), "keld-workflow-security-"));
  try {
    const checkoutRoot = join(temporary, "checkout");
    mkdirSync(checkoutRoot);
    const listing = Bun.spawnSync(["git", "-C", repository, "ls-files", "-z"]);
    expect(listing.exitCode).toBe(0);
    const files = new Set(new TextDecoder().decode(listing.stdout).split("\0").filter(Boolean));
    files.add("tools/ci_workflow_security.ts");
    for (const relative of files) {
      const destination = join(checkoutRoot, relative);
      mkdirSync(dirname(destination), { recursive: true });
      cpSync(join(repository, relative), destination, { dereference: false });
    }
    // `ci-hygiene check` compares the GH-508 channel allocation baseline with a
    // merge base, so the copied checkout is a git repository compared to its HEAD.
    const git = (...args: string[]) =>
      Bun.spawnSync([
        "git", "-C", checkoutRoot,
        "-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid",
        "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", "-c", "init.defaultBranch=main",
        ...args,
      ]);
    expect(git("init", "--quiet").exitCode).toBe(0);
    // No detached `git maintenance` repack may outlive the commit and race cleanup (#670).
    expect(git("config", "maintenance.auto", "false").exitCode).toBe(0);
    expect(git("add", "-A").exitCode).toBe(0);
    expect(git("commit", "--quiet", "--no-verify", "-m", "fixture").exitCode).toBe(0);
    const binary = join(temporary, process.platform === "win32" ? "ci-hygiene.exe" : "ci-hygiene");
    const compilation = Bun.spawnSync(["rustc", "--edition=2024", "-D", "warnings", join(repository, "tools/ci_hygiene.rs"), "-o", binary]);
    expect(compilation.exitCode).toBe(0);
    // The CLI must use the same verified Bun executable as this test process.
    const runtimeEnv = {
      ...process.env,
      KELD_CI_BASE_REF: "HEAD",
      PATH: `${dirname(process.execPath)}${delimiter}${process.env.PATH ?? ""}`,
    };
    const run = (yaml: string, env = runtimeEnv) => {
      writeFileSync(join(checkoutRoot, ".github/workflows/ci.yml"), yaml);
      return Bun.spawnSync([binary, "check", checkoutRoot], { cwd: repository, env });
    };
    const original = run(source);
    expect(original.exitCode).toBe(0);
    // The CLI also refuses a pull_request_target workflow that runs shell code.
    const keldbotPath = join(checkoutRoot, ".github/workflows/keldbot.yml");
    writeFileSync(keldbotPath, keldbot.replace("    steps:\n", "    steps:\n      - run: echo injected\n"));
    const prTarget = run(source);
    expect(prTarget.exitCode).not.toBe(0);
    expect(new TextDecoder().decode(prTarget.stderr)).toContain("pull_request_target");
    writeFileSync(keldbotPath, keldbot);
    expect(new TextDecoder().decode(original.stdout)).toContain("CI workflow security semantics ok");
    const actionsRoute = `    if: ${codeqlRoute("actions")}\n`;
    expect(source).toContain(actionsRoute);
    const misrouted = run(source.replace(actionsRoute, `    if: ${codeqlRoute("rust")}\n`));
    expect(misrouted.exitCode).not.toBe(0);
    expect(new TextDecoder().decode(misrouted.stderr)).toContain("CodeQL job codeql-actions");
    for (const insertion of [
      `      - { uses: '${checkout}', with: { persist-credentials: true } }\n`,
      `      - &insecure { uses: '${checkout}', with: { persist-credentials: true } }\n      - *insecure\n`,
    ]) {
      const result = run(source.replace("      - name: Initialize CodeQL\n", insertion + "      - name: Initialize CodeQL\n"));
      expect(result.exitCode).not.toBe(0);
      expect(new TextDecoder().decode(result.stderr)).toContain("persist-credentials: false");
    }
    const skipped = run(source.replace("      - name: Analyze and upload CodeQL results\n", "      - name: Analyze and upload CodeQL results\n        if: false\n"));
    expect(skipped.exitCode).not.toBe(0);
    expect(new TextDecoder().decode(skipped.stderr)).toContain("conditions or unknown controls");
    const weakenedUpload = run(source.replace("      security-events: write", "      security-events: read"));
    expect(weakenedUpload.exitCode).not.toBe(0);
    expect(new TextDecoder().decode(weakenedUpload.stderr)).toContain("CodeQL job permissions");
    expect(run(source, { ...runtimeEnv, PATH: temporary }).exitCode).not.toBe(0);
    rmSync(join(checkoutRoot, "tools/ci_workflow_security.ts"));
    expect(run(source).exitCode).not.toBe(0);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}, 30000);

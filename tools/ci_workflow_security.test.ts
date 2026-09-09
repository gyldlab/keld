import { expect, test } from "bun:test";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, dirname, join } from "node:path";
import { checkWorkflowSecurity } from "./ci_workflow_security";

type Step = Record<string, unknown>;
type FixtureJob = {
  steps: Step[];
  permissions?: Record<string, unknown>;
  strategy?: { matrix: Record<string, unknown> };
};
type Fixture = { jobs: Record<string, FixtureJob> };
const source = readFileSync(join(import.meta.dir, "../.github/workflows/ci.yml"), "utf8");
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

for (const language of ["rust", "javascript-typescript", "actions"]) {
  test(`CodeQL matrix cannot omit ${language}`, () => {
    const f = fixture();
    const matrix = f.jobs.codeql!.strategy!.matrix;
    matrix.include = (matrix.include as Step[]).filter(row => row.language !== language);
    expect(() => check(f)).toThrow("CodeQL matrix");
  });
}

test("CodeQL matrix rejects duplicate languages and coverage-altering axes", () => {
  const duplicate = fixture();
  const include = duplicate.jobs.codeql!.strategy!.matrix.include as Step[];
  include[2] = { ...include[0] };
  expect(() => check(duplicate)).toThrow("CodeQL matrix");
  for (const extra of [{ exclude: [{ language: "rust" }] }, { language: ["actions"] }]) {
    const f = fixture();
    Object.assign(f.jobs.codeql!.strategy!.matrix, extra);
    expect(() => check(f)).toThrow("CodeQL matrix");
  }
});

test("CodeQL matrix coverage is independent of row order", () => {
  const f = fixture();
  (f.jobs.codeql!.strategy!.matrix.include as Step[]).reverse();
  expect(() => check(f)).not.toThrow();
});

test("CodeQL permissions retain only read content and SARIF-upload authority", () => {
  for (const mutation of ["missing", "security-events-read", "contents-write", "extra"] as const) {
    const f = fixture();
    const permissions = f.jobs.codeql!.permissions!;
    if (mutation === "missing") delete f.jobs.codeql!.permissions;
    if (mutation === "security-events-read") permissions["security-events"] = "read";
    if (mutation === "contents-write") permissions.contents = "write";
    if (mutation === "extra") permissions.actions = "read";
    expect(() => check(f)).toThrow("CodeQL job permissions");
  }
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
      f.jobs.codeql!.steps.push({ uses: `${name}@3d3c42e5aac5ba805825da76410c181273ba90b1`, with: { "persist-credentials": !protectedCheckout } });
      if (protectedCheckout) expect(() => check(f)).not.toThrow();
      else expect(() => check(f)).toThrow("persist-credentials: false");
    }
  }
});

for (const [job, name] of [
  ["codeql", "Initialize CodeQL"],
  ["codeql", "Analyze and upload CodeQL results"],
  ["dependency-review", "Review dependency vulnerabilities"],
  ["dependency-review", "Reject incomplete dependency metadata"],
] as const) {
  test(`${name} cannot be skipped, removed or duplicated`, () => {
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
  ["codeql", "Analyze and upload CodeQL results", "upload", "never"],
  ["codeql", "Analyze and upload CodeQL results", "skip-queries", true],
  ["codeql", "Analyze and upload CodeQL results", "wait-for-processing", false],
  ["dependency-review", "Review dependency vulnerabilities", "vulnerability-check", false],
  ["dependency-review", "Review dependency vulnerabilities", "warn-only", true],
  ["dependency-review", "Review dependency vulnerabilities", "fail-on-severity", "critical"],
  ["dependency-review", "Review dependency vulnerabilities", "fail-on-scopes", "runtime"],
  ["dependency-review", "Review dependency vulnerabilities", "allow-ghsas", "GHSA-xxxx-yyyy-zzzz"],
] as const) {
  test(`required scanner input ${key} cannot bypass its effect`, () => {
    const f = fixture();
    (step(f, job, name).with as Step)[key] = value;
    expect(() => check(f)).toThrow();
  });
}

test("wrong or floating action identity fails, including flow mappings", () => {
  for (const uses of ["github/codeql-action/init@cdf488f595d80d6e07e03d4674febd5ab45fa938", "github/codeql-action/analyze@main"]) {
    const f = fixture();
    step(f, "codeql", "Analyze and upload CodeQL results").uses = uses;
    expect(() => checkWorkflowSecurity(Bun.YAML.stringify(f))).toThrow();
  }
  const f = fixture();
  f.jobs.codeql!.steps.push({ uses: "actions/checkout@main", with: { "persist-credentials": false } });
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
  f.jobs.codeql!.steps.reverse();
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
    const binary = join(temporary, process.platform === "win32" ? "ci-hygiene.exe" : "ci-hygiene");
    const compilation = Bun.spawnSync(["rustc", "--edition=2024", "-D", "warnings", join(repository, "tools/ci_hygiene.rs"), "-o", binary]);
    expect(compilation.exitCode).toBe(0);
    // The CLI must use the same verified Bun executable as this test process.
    const runtimeEnv = { ...process.env, PATH: `${dirname(process.execPath)}${delimiter}${process.env.PATH ?? ""}` };
    const run = (yaml: string, env = runtimeEnv) => {
      writeFileSync(join(checkoutRoot, ".github/workflows/ci.yml"), yaml);
      return Bun.spawnSync([binary, "check", checkoutRoot], { cwd: repository, env });
    };
    const original = run(source);
    expect(original.exitCode).toBe(0);
    expect(new TextDecoder().decode(original.stdout)).toContain("CI workflow security semantics ok");
    const missingLanguage = run(source.replace("          - language: actions\n            os: ubuntu-latest\n", ""));
    expect(missingLanguage.exitCode).not.toBe(0);
    expect(new TextDecoder().decode(missingLanguage.stderr)).toContain("CodeQL matrix");
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

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

type Mapping = Record<string, unknown>;
const baseRef = "${{ github.event.pull_request.base.sha || github.event.before }}";
const headRef = "${{ github.event.pull_request.head.sha || github.sha }}";

function fail(message: string): never {
  throw new Error(`CI-HYGIENE: ${message}`);
}

function mapping(value: unknown, label: string): Mapping {
  if (value === null || typeof value !== "object" || Array.isArray(value)
    || ![Object.prototype, null].includes(Object.getPrototypeOf(value))) {
    fail(`${label} must be a plain mapping; this workflow shape is not admitted.`);
  }
  return value as Mapping;
}

function scalar(value: unknown): string | undefined {
  return ["string", "boolean", "number"].includes(typeof value) ? String(value) : undefined;
}

function exactKeys(value: Mapping, keys: string[], label: string): void {
  if (Object.keys(value).length !== keys.length || keys.some(key => !Object.hasOwn(value, key))) {
    fail(`${label} must contain only ${keys.join(", ")}; conditions or unknown controls are not admitted.`);
  }
}

function finiteGraph(value: unknown, active = new Set<object>(), seen = new Set<object>(), depth = 0): void {
  if (value === null || typeof value !== "object") {
    if (value !== null && !["string", "number", "boolean"].includes(typeof value)) fail("unsupported YAML value is not admitted.");
    if (typeof value === "number" && !Number.isFinite(value)) fail("non-finite YAML numbers are not admitted.");
    return;
  }
  if (active.has(value)) fail("cyclic YAML aliases are not admitted.");
  if (seen.has(value)) return;
  if (depth > 100 || seen.size >= 10000) fail("workflow object graph exceeds the bounded validation budget.");
  if (!Array.isArray(value)) mapping(value, "YAML object");
  active.add(value);
  seen.add(value);
  for (const child of Object.values(value)) finiteGraph(child, active, seen, depth + 1);
  active.delete(value);
}

function actionRef(value: unknown, label: string): string {
  if (typeof value !== "string" || (!value.startsWith("./") && !value.startsWith("docker://")
    && !/^[^\s@]+\/[\w./-]+@[0-9a-f]{40}$/.test(value))) {
    fail(`${label} must use an immutable 40-character action SHA or an existing local/container action reference.`);
  }
  return value;
}

function actionName(reference: string): string {
  const [owner = "", repository = "", ...path] = reference.split("@", 1)[0]!.split("/");
  // GitHub repository identity is case-insensitive; paths within it retain case.
  return [owner.toLowerCase(), repository.toLowerCase(), ...path].join("/");
}

function inputsMatch(actual: Mapping, expected: Record<string, string>, label: string): void {
  exactKeys(actual, Object.keys(expected), label);
  for (const [key, expectedValue] of Object.entries(expected)) {
    const value = scalar(actual[key]);
    const matches = key === "fail-on-scopes"
      ? value?.split(",").map(scope => scope.trim()).sort().join(",") === "development,runtime,unknown"
      : value === expectedValue;
    if (!matches) fail(`${label}.${key} must preserve ${expectedValue}; restore the blocking security input.`);
  }
}

function namedStep(steps: Mapping[], name: string): Mapping {
  const matches = steps.filter(step => step.name === name);
  if (matches.length !== 1) fail(`${name} must occur exactly once in its job's actual steps.`);
  return matches[0]!;
}

function requiredAction(steps: Mapping[], name: string, action: string, expected: Record<string, string>): Mapping {
  const step = namedStep(steps, name);
  exactKeys(step, ["name", "uses", "with"], name);
  if (actionName(actionRef(step.uses, name)) !== action) fail(`${name} must execute ${action}.`);
  inputsMatch(mapping(step.with, `${name}.with`), expected, name);
  return step;
}

/** Validate security effects over parsed workflow objects, never YAML line shapes. */
export function checkWorkflowSecurity(source: string): void {
  if (Buffer.byteLength(source) > 1024 * 1024) fail("workflow exceeds the 1 MiB parsing budget.");
  let parsed: unknown;
  try { parsed = Bun.YAML.parse(source); }
  catch (error) { fail(`cannot parse workflow YAML: ${error instanceof Error ? error.message : String(error)}`); }
  finiteGraph(parsed);
  const workflow = mapping(parsed, "workflow (one YAML document)");
  const jobs = mapping(workflow.jobs, "workflow.jobs");
  if (Object.keys(jobs).length === 0) fail("workflow.jobs must not be empty.");
  const stepsByJob = new Map<string, Mapping[]>();
  let checkouts = 0;
  for (const [jobName, value] of Object.entries(jobs)) {
    const job = mapping(value, `jobs.${jobName}`);
    if (!Array.isArray(job.steps) || job.steps.length === 0 || Object.hasOwn(job, "uses")) {
      fail(`jobs.${jobName} requires a nonempty concrete steps array; opaque/reusable job shapes need a reviewed contract.`);
    }
    const steps = job.steps.map((step, index) => mapping(step, `jobs.${jobName}.steps[${index}]`));
    stepsByJob.set(jobName, steps);
    for (const [index, step] of steps.entries()) {
      const label = `jobs.${jobName}.steps[${index}]`;
      const uses = Object.hasOwn(step, "uses");
      if (uses === Object.hasOwn(step, "run")) fail(`${label} must contain exactly one action or executable run string.`);
      if (!uses) {
        if (typeof step.run !== "string" || !step.run.trim()) fail(`${label}.run must be a nonempty string.`);
        continue;
      }
      const action = actionRef(step.uses, label);
      const identity = actionName(action);
      if (identity === "actions/checkout" || identity.startsWith("actions/checkout/")) {
        checkouts++;
        const inputs = mapping(step.with, `${label}.with`);
        if (scalar(inputs["persist-credentials"]) !== "false") {
          fail(`${label} actions/checkout must set persist-credentials: false; every checkout is checked.`);
        }
      }
    }
  }
  if (checkouts === 0) fail("workflow must contain its audited checkout steps.");
  const codeql = stepsByJob.get("codeql");
  const dependencies = stepsByJob.get("dependency-review");
  if (!codeql || !dependencies) fail("CodeQL and dependency-review jobs must exist.");
  const codeqlJob = mapping(jobs.codeql, "jobs.codeql");
  inputsMatch(mapping(codeqlJob.permissions, "CodeQL job permissions"), {
    contents: "read", "security-events": "write",
  }, "CodeQL job permissions");
  const strategy = mapping(codeqlJob.strategy, "CodeQL strategy");
  const matrix = mapping(strategy.matrix, "CodeQL matrix");
  exactKeys(matrix, ["include"], "CodeQL matrix");
  const expectedLanguages = ["rust", "javascript-typescript", "actions"];
  const languages = Array.isArray(matrix.include)
    ? matrix.include.map(row => mapping(row, "CodeQL matrix row").language) : [];
  if (languages.length !== expectedLanguages.length || !expectedLanguages.every(language => languages.includes(language))) {
    fail("CodeQL matrix must include rust, javascript-typescript and actions exactly once; restore all scan categories.");
  }
  const init = requiredAction(codeql, "Initialize CodeQL", "github/codeql-action/init", {
    languages: "${{ matrix.language }}", "build-mode": "none",
  });
  const analyze = requiredAction(codeql, "Analyze and upload CodeQL results", "github/codeql-action/analyze", {
    category: "/language:${{ matrix.language }}", upload: "always", "skip-queries": "false", "wait-for-processing": "true",
  });
  if (codeql.indexOf(init) >= codeql.indexOf(analyze)) fail("CodeQL initialization must precede analysis.");
  const review = requiredAction(dependencies, "Review dependency vulnerabilities", "actions/dependency-review-action", {
    "base-ref": baseRef, "head-ref": headRef, "fail-on-severity": "low",
    "fail-on-scopes": "runtime, development, unknown", "vulnerability-check": "true",
    "license-check": "false", "comment-summary-in-pr": "never", "retry-on-snapshot-warnings": "false",
    "warn-only": "false", "show-openssf-scorecard": "false",
  });
  const metadata = namedStep(dependencies, "Reject incomplete dependency metadata");
  exactKeys(metadata, ["name", "env", "run"], "dependency metadata admission");
  inputsMatch(mapping(metadata.env, "dependency metadata env"), {
    GH_TOKEN: "${{ github.token }}", KELD_DEPENDENCY_REPOSITORY: "${{ github.repository }}",
    KELD_DEPENDENCY_BASE: baseRef, KELD_DEPENDENCY_HEAD: headRef,
  }, "dependency metadata env");
  const commands = typeof metadata.run === "string" ? metadata.run.trim().split(/\r?\n/).map(line => line.trim()) : [];
  const expectedCommands = [
    "tools/dependency_review_metadata.sh test",
    'tools/dependency_review_metadata.sh check "$KELD_DEPENDENCY_REPOSITORY" "$KELD_DEPENDENCY_BASE" "$KELD_DEPENDENCY_HEAD"',
  ];
  if (JSON.stringify(commands) !== JSON.stringify(expectedCommands)) fail("dependency metadata admission must execute its exact checks without wrappers.");
  if (dependencies.indexOf(metadata) >= dependencies.indexOf(review)) fail("metadata admission must precede dependency review.");
}

if (import.meta.main) {
  try {
    const [, , command, root = ".", ...extra] = process.argv;
    if (command !== "check" || extra.length) fail("use ci_workflow_security.ts check [workspace].");
    checkWorkflowSecurity(readFileSync(resolve(root, ".github/workflows/ci.yml"), "utf8"));
    console.log("CI workflow security semantics ok");
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}

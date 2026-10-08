import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { createHash } from "node:crypto";

type Mapping = Record<string, unknown>;
const baseRef = "${{ github.event.pull_request.base.sha || github.event.before }}";
const headRef = "${{ github.event.pull_request.head.sha || github.sha }}";
const windowsMediaOracle = "crates/keld-wv/tests/windows_media_guard.ps1";
const windowsMediaOracleSha256 = "fba6622b6faf56a785f10e5dfbf4da7d0b781faecd0a66e5735e568d028c11aa";

function fail(message: string): never {
  throw new Error(`CI-HYGIENE: ${message}`);
}

export function checkWindowsMediaOracle(source: Uint8Array): void {
  const actual = createHash("sha256").update(source).digest("hex");
  if (actual !== windowsMediaOracleSha256) {
    fail(`${windowsMediaOracle} changed without updating its reviewed SHA-256 owner; expected ${windowsMediaOracleSha256}, got ${actual}.`);
  }
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

/**
 * The `timeout-minutes` every step whose `run` script invokes apt must set
 * (#624): above the slowest observed successful apt step (642 s), far below
 * the hangs that ran to the 45-minute job timeout. A shorter bound turned a
 * slow but progressing mirror download into a failure.
 */
export const aptStepTimeoutMinutes = 15;
const aptInvocation = /\bapt(-get)?\b|tools\/ci_webkitgtk_apt\.sh install\b/;

/**
 * A step whose `run` script mentions `apt` or `apt-get` must bound itself with
 * exactly `aptStepTimeoutMinutes`, and its job must allow longer, so that the
 * step bound (not the job timeout) is what ends a hung Ubuntu mirror. Only the
 * parsed `run` string counts (never the step name); local actions and scripts
 * the step calls are outside this check.
 */
function checkAptStepTimeout(step: Mapping, job: Mapping, label: string): void {
  if (typeof step.run !== "string" || !aptInvocation.test(step.run)) return;
  if (scalar(step["timeout-minutes"]) !== String(aptStepTimeoutMinutes)) {
    fail(`${label} runs apt without step timeout-minutes: ${aptStepTimeoutMinutes}; a hung Ubuntu mirror must fail the step, and a shorter bound fails slow but healthy downloads. Do not retry or continue on error.`);
  }
  const jobMinutes = scalar(job["timeout-minutes"]);
  if (jobMinutes !== undefined && !(/^[1-9][0-9]*$/.test(jobMinutes) && Number(jobMinutes) > aptStepTimeoutMinutes)) {
    fail(`${label} runs apt in a job whose timeout-minutes (${jobMinutes}) does not exceed the ${aptStepTimeoutMinutes}-minute apt step bound; raise the job timeout so the step bound is the one that applies.`);
  }
}

/** Jobs that install WebKitGTK on Ubuntu through the verified .deb cache (#645). */
export const webkitgtkAptJobs = ["check", "linux-gui-smoke"] as const;
export const webkitgtkDebCachePath = "~/.cache/keld-webkitgtk-debs";
const webkitgtkKeyOutput = "${{ steps.webkitgtk-apt-key.outputs.key }}";
const webkitgtkCacheHitGuard = "steps.webkitgtk-apt-cache.outputs.cache-hit != 'true'";
/** Hard bound on each cache step; a stuck restore download gives up sooner and proceeds as a miss. */
export const webkitgtkCacheStepTimeoutMinutes = 5;
export const webkitgtkCacheSegmentTimeoutMinutes = 2;

/**
 * The WebKitGTK .deb cache (#645): key, restore, install and save, in that
 * order, under one condition. The key comes from tools/ci_webkitgtk_apt.sh,
 * which binds the runner image and the job's exact package list; the install
 * goes through the same script, which refreshes apt's signed indexes and stages
 * a cached .deb only when its SHA256 matches them; the save runs only on a miss.
 * No other step may use actions/cache, so apt index lists are never cached.
 */
function checkWebkitgtkAptCache(jobs: Mapping, stepsByJob: Map<string, Mapping[]>): void {
  const owners = new Set<string>(webkitgtkAptJobs);
  for (const [jobName, steps] of stepsByJob) {
    for (const step of steps) {
      const uses = typeof step.uses === "string" ? actionName(step.uses) : "";
      if (uses.startsWith("actions/cache") && !(owners.has(jobName) && ["Restore WebKitGTK .deb cache", "Save WebKitGTK .deb cache"].includes(String(step.name)))) {
        fail(`jobs.${jobName} uses actions/cache outside the WebKitGTK .deb cache steps; apt index lists and other paths must not be cached.`);
      }
    }
  }
  for (const jobName of webkitgtkAptJobs) {
    const steps = stepsByJob.get(jobName);
    if (!steps) fail(`jobs.${jobName} must exist with the WebKitGTK .deb cache steps.`);
    const job = mapping(jobs[jobName], `jobs.${jobName}`);
    const env = mapping(job.env, `jobs.${jobName}.env`);
    const packages = scalar(env.KELD_WEBKITGTK_PACKAGES);
    if (!packages || !/^[a-z0-9][a-z0-9.+-]*( [a-z0-9][a-z0-9.+-]*)*$/.test(packages)) {
      fail(`jobs.${jobName}.env.KELD_WEBKITGTK_PACKAGES must list the exact WebKitGTK packages, the one source for the cache key and the install.`);
    }
    const key = namedStep(steps, "Resolve WebKitGTK apt cache key");
    const restore = namedStep(steps, "Restore WebKitGTK .deb cache");
    const install = steps.find(step => typeof step.run === "string" && aptInvocation.test(step.run));
    const save = namedStep(steps, "Save WebKitGTK .deb cache");
    if (!install) fail(`jobs.${jobName} must install WebKitGTK through tools/ci_webkitgtk_apt.sh install.`);
    const condition = key.if;
    for (const [step, keys] of [[key, ["name", "id", "run"]], [restore, ["name", "id", "timeout-minutes", "env", "uses", "with"]], [install, ["name", "timeout-minutes", "run"]]] as const) {
      exactKeys(step, condition === undefined ? [...keys] : [...keys, "if"], `jobs.${jobName} ${String(step.name)}`);
      if (step.if !== condition) fail(`jobs.${jobName} ${String(step.name)} must share the key step's condition, so the cache and the install apply to the same legs.`);
    }
    if (key.id !== "webkitgtk-apt-key" || String(key.run).trim() !== 'tools/ci_webkitgtk_apt.sh key >> "$GITHUB_OUTPUT"') {
      fail(`jobs.${jobName} must resolve the cache key with exactly \`tools/ci_webkitgtk_apt.sh key >> "$GITHUB_OUTPUT"\` (id webkitgtk-apt-key): the runner image plus the exact package list.`);
    }
    if (String(install.run).trim() !== `tools/ci_webkitgtk_apt.sh install ${webkitgtkDebCachePath}`) {
      fail(`jobs.${jobName} must install with exactly \`tools/ci_webkitgtk_apt.sh install ${webkitgtkDebCachePath}\`, which still runs apt-get update and verifies every cached .deb against the signed indexes.`);
    }
    for (const [step, action, id] of [[restore, "actions/cache/restore", "webkitgtk-apt-cache"], [save, "actions/cache/save", undefined]] as const) {
      if (actionName(actionRef(step.uses, String(step.name))) !== action || (id !== undefined && step.id !== id)) {
        fail(`jobs.${jobName} ${String(step.name)} must use ${action}${id ? ` with id ${id}` : ""}.`);
      }
      inputsMatch(mapping(step.with, `${String(step.name)}.with`), { path: webkitgtkDebCachePath, key: webkitgtkKeyOutput }, `jobs.${jobName} ${String(step.name)}`);
    }
    exactKeys(save, ["name", "if", "timeout-minutes", "uses", "with"], `jobs.${jobName} Save WebKitGTK .deb cache`);
    for (const step of [restore, save]) {
      if (scalar(step["timeout-minutes"]) !== String(webkitgtkCacheStepTimeoutMinutes)) {
        fail(`jobs.${jobName} ${String(step.name)} must set timeout-minutes: ${webkitgtkCacheStepTimeoutMinutes}; a stuck cache service must not hold the job.`);
      }
    }
    inputsMatch(mapping(restore.env, `jobs.${jobName} Restore WebKitGTK .deb cache env`), {
      SEGMENT_DOWNLOAD_TIMEOUT_MINS: String(webkitgtkCacheSegmentTimeoutMinutes),
    }, `jobs.${jobName} Restore WebKitGTK .deb cache env (a stuck download must abort as a miss before the step bound)`);
    const saveIf = condition === undefined ? webkitgtkCacheHitGuard : `${String(condition)} && ${webkitgtkCacheHitGuard}`;
    if (save.if !== saveIf) fail(`jobs.${jobName} Save WebKitGTK .deb cache must run only on a miss: \`if: ${saveIf}\`.`);
    const order = [key, restore, install, save].map(step => steps.indexOf(step));
    if (order.some((index, i) => i > 0 && index <= order[i - 1]!)) {
      fail(`jobs.${jobName} must order the WebKitGTK cache steps key, restore, install, save.`);
    }
  }
}

/** The CodeQL languages, each analysed by its own job `codeql-<language>`. */
export const codeqlLanguages = ["rust", "javascript-typescript", "actions"] as const;

/**
 * One language's job condition: its router output (tools/ci_changes.sh), or a
 * push whose router job did not succeed and so published no outputs. The push
 * clause keeps main's baseline when the router fails; it never overrides a
 * router that ran.
 */
export function codeqlRoute(language: string): string {
  const output = `needs.changes.outputs.codeql_${language.replaceAll("-", "_")}`;
  return `\${{ !cancelled() && (${output} == 'true' || (github.event_name == 'push' && needs.changes.result != 'success')) }}`;
}

/**
 * One job per language, because a job-level `if` cannot read `matrix` (#624).
 * Each job keeps its upload category, minimal permissions and blocking analysis,
 * and is gated only on its own router output. No other job may run CodeQL, so a
 * successful job cannot stand in for a missing or duplicated category.
 */
function checkCodeqlJobs(jobs: Mapping, stepsByJob: Map<string, Mapping[]>): void {
  const owners = new Set<string>(codeqlLanguages.map(language => `codeql-${language}`));
  for (const [jobName, steps] of stepsByJob) {
    if (owners.has(jobName)) continue;
    if (steps.some(step => typeof step.uses === "string" && actionName(step.uses).startsWith("github/codeql-action/"))) {
      fail(`jobs.${jobName} runs CodeQL outside the per-language jobs; restore one codeql-<language> job per scan category.`);
    }
  }
  for (const language of codeqlLanguages) {
    const jobName = `codeql-${language}`;
    const steps = stepsByJob.get(jobName);
    if (!steps) fail(`CodeQL job ${jobName} must exist; rust, javascript-typescript and actions each need their scan category.`);
    const job = mapping(jobs[jobName], `jobs.${jobName}`);
    inputsMatch(mapping(job.permissions, "CodeQL job permissions"), {
      contents: "read", "security-events": "write",
    }, "CodeQL job permissions");
    if (Object.hasOwn(job, "strategy")) fail(`CodeQL job ${jobName} must not set strategy; one job analyses exactly one language.`);
    const needs = Array.isArray(job.needs) && job.needs.length === 1 ? job.needs[0] : job.needs;
    if (needs !== "changes" || job.if !== codeqlRoute(language)) {
      fail(`CodeQL job ${jobName} must need changes and run only on \`${codeqlRoute(language)}\`; the router owns push and fallback selection.`);
    }
    const init = requiredAction(steps, "Initialize CodeQL", "github/codeql-action/init", {
      languages: language, "build-mode": "none",
    });
    const analyze = requiredAction(steps, "Analyze and upload CodeQL results", "github/codeql-action/analyze", {
      category: `/language:${language}`, upload: "always", "skip-queries": "false", "wait-for-processing": "true",
    });
    if (steps.indexOf(init) >= steps.indexOf(analyze)) fail("CodeQL initialization must precede analysis.");
  }
}

/** The trigger names of a parsed workflow, whichever `on:` form it uses. */
function workflowTriggers(workflow: Mapping): string[] {
  const on = workflow.on;
  if (typeof on === "string") return [on];
  if (Array.isArray(on)) return on.map(String);
  if (on !== null && typeof on === "object") return Object.keys(on);
  fail("workflow has no parseable `on:` trigger.");
}

/**
 * A `pull_request_target` workflow runs with the base repository's token and
 * writes the default branch's caches. It must never run pull-request code: no
 * checkout of any ref, no `run:` shell step and no reusable-workflow job (#645).
 * This keeps main-scoped caches (the WebKitGTK .deb cache, rust-cache) from
 * being written by untrusted code.
 */
export function checkPullRequestTargetWorkflow(source: string, label: string): void {
  if (Buffer.byteLength(source) > 1024 * 1024) fail(`${label} exceeds the 1 MiB parsing budget.`);
  let parsed: unknown;
  try { parsed = Bun.YAML.parse(source); }
  catch (error) { fail(`cannot parse ${label}: ${error instanceof Error ? error.message : String(error)}`); }
  finiteGraph(parsed);
  const workflow = mapping(parsed, `${label} (one YAML document)`);
  if (!workflowTriggers(workflow).includes("pull_request_target")) return;
  const jobs = mapping(workflow.jobs, `${label} jobs`);
  for (const [jobName, value] of Object.entries(jobs)) {
    const job = mapping(value, `${label} jobs.${jobName}`);
    if (Object.hasOwn(job, "uses") || !Array.isArray(job.steps)) {
      fail(`${label} jobs.${jobName} under pull_request_target must be concrete steps; a reusable workflow could run pull-request code.`);
    }
    for (const [index, value] of job.steps.entries()) {
      const step = mapping(value, `${label} jobs.${jobName}.steps[${index}]`);
      if (Object.hasOwn(step, "run")) {
        fail(`${label} jobs.${jobName}.steps[${index}] is a run: step under pull_request_target; this secret-bearing trigger must not run shell code.`);
      }
      const identity = actionName(actionRef(step.uses, `${label} jobs.${jobName}.steps[${index}]`));
      if (identity === "actions/checkout" || identity.startsWith("actions/checkout/")) {
        fail(`${label} jobs.${jobName}.steps[${index}] checks out code under pull_request_target; never run pull-request code with the base repository's token and caches.`);
      }
    }
  }
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
        checkAptStepTimeout(step, job, `${label} (${typeof step.name === "string" ? step.name : "unnamed"})`);
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
  checkCodeqlJobs(jobs, stepsByJob);
  checkWebkitgtkAptCache(jobs, stepsByJob);
  const dependencies = stepsByJob.get("dependency-review");
  if (!dependencies) fail("CodeQL and dependency-review jobs must exist.");
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
    const workflowsDir = resolve(root, ".github/workflows");
    for (const name of readdirSync(workflowsDir).filter(entry => /\.ya?ml$/.test(entry)).sort()) {
      checkPullRequestTargetWorkflow(readFileSync(resolve(workflowsDir, name), "utf8"), `.github/workflows/${name}`);
    }
    checkWindowsMediaOracle(readFileSync(resolve(root, windowsMediaOracle)));
    console.log("CI workflow security semantics ok");
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}

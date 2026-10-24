import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { createHash } from "node:crypto";

type Mapping = Record<string, unknown>;
const baseRef =
  "${{ github.event.pull_request.base.sha || github.event.before }}";
const headRef = "${{ github.event.pull_request.head.sha || github.sha }}";
const windowsMediaOracle = "crates/keld-wv/tests/windows_media_guard.ps1";
const windowsMediaOracleSha256 =
  "c78794b25f446e52c38b25448cc10a75e006f846ba97fdc7c52fbc5925a5b680";

function fail(message: string): never {
  throw new Error(`CI-HYGIENE: ${message}`);
}

export function checkWindowsMediaOracle(source: Uint8Array): void {
  const actual = createHash("sha256").update(source).digest("hex");
  if (actual !== windowsMediaOracleSha256) {
    fail(
      `${windowsMediaOracle} changed without updating its reviewed SHA-256 owner; expected ${windowsMediaOracleSha256}, got ${actual}.`,
    );
  }
}

function mapping(value: unknown, label: string): Mapping {
  if (
    value === null ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    ![Object.prototype, null].includes(Object.getPrototypeOf(value))
  ) {
    fail(
      `${label} must be a plain mapping; this workflow shape is not admitted.`,
    );
  }
  return value as Mapping;
}

function scalar(value: unknown): string | undefined {
  return ["string", "boolean", "number"].includes(typeof value)
    ? String(value)
    : undefined;
}

function exactKeys(value: Mapping, keys: string[], label: string): void {
  if (
    Object.keys(value).length !== keys.length ||
    keys.some((key) => !Object.hasOwn(value, key))
  ) {
    fail(
      `${label} must contain only ${keys.join(", ")}; conditions or unknown controls are not admitted.`,
    );
  }
}

function finiteGraph(
  value: unknown,
  active = new Set<object>(),
  seen = new Set<object>(),
  depth = 0,
): void {
  if (value === null || typeof value !== "object") {
    if (
      value !== null &&
      !["string", "number", "boolean"].includes(typeof value)
    )
      fail("unsupported YAML value is not admitted.");
    if (typeof value === "number" && !Number.isFinite(value))
      fail("non-finite YAML numbers are not admitted.");
    return;
  }
  if (active.has(value)) fail("cyclic YAML aliases are not admitted.");
  if (seen.has(value)) return;
  if (depth > 100 || seen.size >= 10000)
    fail("workflow object graph exceeds the bounded validation budget.");
  if (!Array.isArray(value)) mapping(value, "YAML object");
  active.add(value);
  seen.add(value);
  for (const child of Object.values(value))
    finiteGraph(child, active, seen, depth + 1);
  active.delete(value);
}

function actionRef(value: unknown, label: string): string {
  if (
    typeof value !== "string" ||
    (!value.startsWith("./") &&
      !value.startsWith("docker://") &&
      !/^[^\s@]+\/[\w./-]+@[0-9a-f]{40}$/.test(value))
  ) {
    fail(
      `${label} must use an immutable 40-character action SHA or an existing local/container action reference.`,
    );
  }
  return value;
}

function actionName(reference: string): string {
  const [owner = "", repository = "", ...path] = reference
    .split("@", 1)[0]!
    .split("/");
  // GitHub repository identity is case-insensitive; paths within it retain case.
  return [owner.toLowerCase(), repository.toLowerCase(), ...path].join("/");
}

function inputsMatch(
  actual: Mapping,
  expected: Record<string, string>,
  label: string,
): void {
  exactKeys(actual, Object.keys(expected), label);
  for (const [key, expectedValue] of Object.entries(expected)) {
    const value = scalar(actual[key]);
    const matches =
      key === "fail-on-scopes"
        ? value
            ?.split(",")
            .map((scope) => scope.trim())
            .sort()
            .join(",") === "development,runtime,unknown"
        : value === expectedValue;
    if (!matches)
      fail(
        `${label}.${key} must preserve ${expectedValue}; restore the blocking security input.`,
      );
  }
}

function namedStep(steps: Mapping[], name: string): Mapping {
  const matches = steps.filter((step) => step.name === name);
  if (matches.length !== 1)
    fail(`${name} must occur exactly once in its job's actual steps.`);
  return matches[0]!;
}

function requiredAction(
  steps: Mapping[],
  name: string,
  action: string,
  expected: Record<string, string>,
): Mapping {
  const step = namedStep(steps, name);
  exactKeys(step, ["name", "uses", "with"], name);
  if (actionName(actionRef(step.uses, name)) !== action)
    fail(`${name} must execute ${action}.`);
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
    fail(
      `${label} runs apt without step timeout-minutes: ${aptStepTimeoutMinutes}; a hung Ubuntu mirror must fail the step, and a shorter bound fails slow but healthy downloads. Do not retry or continue on error.`,
    );
  }
  const jobMinutes = scalar(job["timeout-minutes"]);
  if (
    jobMinutes !== undefined &&
    !(
      /^[1-9][0-9]*$/.test(jobMinutes) &&
      Number(jobMinutes) > aptStepTimeoutMinutes
    )
  ) {
    fail(
      `${label} runs apt in a job whose timeout-minutes (${jobMinutes}) does not exceed the ${aptStepTimeoutMinutes}-minute apt step bound; raise the job timeout so the step bound is the one that applies.`,
    );
  }
}

/** Jobs that install WebKitGTK on Ubuntu through the verified .deb cache (#645). */
export const webkitgtkAptJobs = ["check", "linux-gui-smoke"] as const;
export const webkitgtkDebCachePath = "~/.cache/keld-webkitgtk-debs";
const webkitgtkKeyOutput = "${{ steps.webkitgtk-apt-key.outputs.key }}";
const webkitgtkCacheHitGuard =
  "steps.webkitgtk-apt-cache.outputs.cache-hit != 'true'";
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
function checkWebkitgtkAptCache(
  jobs: Mapping,
  stepsByJob: Map<string, Mapping[]>,
): void {
  const owners = new Set<string>(webkitgtkAptJobs);
  for (const [jobName, steps] of stepsByJob) {
    for (const step of steps) {
      const uses = typeof step.uses === "string" ? actionName(step.uses) : "";
      if (
        uses.startsWith("actions/cache") &&
        !(
          owners.has(jobName) &&
          [
            "Restore WebKitGTK .deb cache",
            "Save WebKitGTK .deb cache",
          ].includes(String(step.name))
        )
      ) {
        fail(
          `jobs.${jobName} uses actions/cache outside the WebKitGTK .deb cache steps; apt index lists and other paths must not be cached.`,
        );
      }
    }
  }
  for (const jobName of webkitgtkAptJobs) {
    const steps = stepsByJob.get(jobName);
    if (!steps)
      fail(`jobs.${jobName} must exist with the WebKitGTK .deb cache steps.`);
    const job = mapping(jobs[jobName], `jobs.${jobName}`);
    const env = mapping(job.env, `jobs.${jobName}.env`);
    const packages = scalar(env.KELD_WEBKITGTK_PACKAGES);
    if (
      !packages ||
      !/^[a-z0-9][a-z0-9.+-]*( [a-z0-9][a-z0-9.+-]*)*$/.test(packages)
    ) {
      fail(
        `jobs.${jobName}.env.KELD_WEBKITGTK_PACKAGES must list the exact WebKitGTK packages, the one source for the cache key and the install.`,
      );
    }
    const key = namedStep(steps, "Resolve WebKitGTK apt cache key");
    const restore = namedStep(steps, "Restore WebKitGTK .deb cache");
    const install = steps.find(
      (step) => typeof step.run === "string" && aptInvocation.test(step.run),
    );
    const save = namedStep(steps, "Save WebKitGTK .deb cache");
    if (!install)
      fail(
        `jobs.${jobName} must install WebKitGTK through tools/ci_webkitgtk_apt.sh install.`,
      );
    const condition = key.if;
    for (const [step, keys] of [
      [key, ["name", "id", "run"]],
      [restore, ["name", "id", "timeout-minutes", "env", "uses", "with"]],
      [install, ["name", "timeout-minutes", "run"]],
    ] as const) {
      exactKeys(
        step,
        condition === undefined ? [...keys] : [...keys, "if"],
        `jobs.${jobName} ${String(step.name)}`,
      );
      if (step.if !== condition)
        fail(
          `jobs.${jobName} ${String(step.name)} must share the key step's condition, so the cache and the install apply to the same legs.`,
        );
    }
    if (
      key.id !== "webkitgtk-apt-key" ||
      String(key.run).trim() !==
        'tools/ci_webkitgtk_apt.sh key >> "$GITHUB_OUTPUT"'
    ) {
      fail(
        `jobs.${jobName} must resolve the cache key with exactly \`tools/ci_webkitgtk_apt.sh key >> "$GITHUB_OUTPUT"\` (id webkitgtk-apt-key): the runner image plus the exact package list.`,
      );
    }
    if (
      String(install.run).trim() !==
      `tools/ci_webkitgtk_apt.sh install ${webkitgtkDebCachePath}`
    ) {
      fail(
        `jobs.${jobName} must install with exactly \`tools/ci_webkitgtk_apt.sh install ${webkitgtkDebCachePath}\`, which still runs apt-get update and verifies every cached .deb against the signed indexes.`,
      );
    }
    for (const [step, action, id] of [
      [restore, "actions/cache/restore", "webkitgtk-apt-cache"],
      [save, "actions/cache/save", undefined],
    ] as const) {
      if (
        actionName(actionRef(step.uses, String(step.name))) !== action ||
        (id !== undefined && step.id !== id)
      ) {
        fail(
          `jobs.${jobName} ${String(step.name)} must use ${action}${id ? ` with id ${id}` : ""}.`,
        );
      }
      inputsMatch(
        mapping(step.with, `${String(step.name)}.with`),
        { path: webkitgtkDebCachePath, key: webkitgtkKeyOutput },
        `jobs.${jobName} ${String(step.name)}`,
      );
    }
    exactKeys(
      save,
      ["name", "if", "timeout-minutes", "uses", "with"],
      `jobs.${jobName} Save WebKitGTK .deb cache`,
    );
    for (const step of [restore, save]) {
      if (
        scalar(step["timeout-minutes"]) !==
        String(webkitgtkCacheStepTimeoutMinutes)
      ) {
        fail(
          `jobs.${jobName} ${String(step.name)} must set timeout-minutes: ${webkitgtkCacheStepTimeoutMinutes}; a stuck cache service must not hold the job.`,
        );
      }
    }
    inputsMatch(
      mapping(restore.env, `jobs.${jobName} Restore WebKitGTK .deb cache env`),
      {
        SEGMENT_DOWNLOAD_TIMEOUT_MINS: String(
          webkitgtkCacheSegmentTimeoutMinutes,
        ),
      },
      `jobs.${jobName} Restore WebKitGTK .deb cache env (a stuck download must abort as a miss before the step bound)`,
    );
    const saveIf =
      condition === undefined
        ? webkitgtkCacheHitGuard
        : `${String(condition)} && ${webkitgtkCacheHitGuard}`;
    if (save.if !== saveIf)
      fail(
        `jobs.${jobName} Save WebKitGTK .deb cache must run only on a miss: \`if: ${saveIf}\`.`,
      );
    const order = [key, restore, install, save].map((step) =>
      steps.indexOf(step),
    );
    if (order.some((index, i) => i > 0 && index <= order[i - 1]!)) {
      fail(
        `jobs.${jobName} must order the WebKitGTK cache steps key, restore, install, save.`,
      );
    }
  }
}

/** The CodeQL languages, each analysed by its own job `codeql-<language>`. */
export const codeqlLanguages = [
  "rust",
  "javascript-typescript",
  "actions",
] as const;

// Both actions must use the same *reviewed* immutable revision. Merely
// requiring equal 40-hex SHAs would allow all six init/analyze uses to drift
// together to an unreviewed action while the security admission stays green.
const approvedCodeqlActionSha = "6f2ac81f5cd81fe351a197ba0b00d91d03cd2aee"; // v4.39.0

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
function checkCodeqlJobs(
  jobs: Mapping,
  stepsByJob: Map<string, Mapping[]>,
): void {
  const owners = new Set<string>(
    codeqlLanguages.map((language) => `codeql-${language}`),
  );
  for (const [jobName, steps] of stepsByJob) {
    if (owners.has(jobName)) continue;
    if (
      steps.some(
        (step) =>
          typeof step.uses === "string" &&
          actionName(step.uses).startsWith("github/codeql-action/"),
      )
    ) {
      fail(
        `jobs.${jobName} runs CodeQL outside the per-language jobs; restore one codeql-<language> job per scan category.`,
      );
    }
  }
  for (const language of codeqlLanguages) {
    const jobName = `codeql-${language}`;
    const steps = stepsByJob.get(jobName);
    if (!steps)
      fail(
        `CodeQL job ${jobName} must exist; rust, javascript-typescript and actions each need their scan category.`,
      );
    const job = mapping(jobs[jobName], `jobs.${jobName}`);
    inputsMatch(
      mapping(job.permissions, "CodeQL job permissions"),
      {
        contents: "read",
        "security-events": "write",
      },
      "CodeQL job permissions",
    );
    if (Object.hasOwn(job, "strategy"))
      fail(
        `CodeQL job ${jobName} must not set strategy; one job analyses exactly one language.`,
      );
    if (Object.hasOwn(job, "continue-on-error"))
      fail(`CodeQL job ${jobName} must fail on analyzer errors.`);
    if (language === "rust" && scalar(job["runs-on"]) !== "macos-latest") {
      fail(
        "CodeQL Rust must use macos-latest for the reviewed macOS-specific semantic coverage.",
      );
    }
    const needs =
      Array.isArray(job.needs) && job.needs.length === 1
        ? job.needs[0]
        : job.needs;
    if (needs !== "changes" || job.if !== codeqlRoute(language)) {
      fail(
        `CodeQL job ${jobName} must need changes and run only on \`${codeqlRoute(language)}\`; the router owns push and fallback selection.`,
      );
    }
    const init = requiredAction(
      steps,
      "Initialize CodeQL",
      "github/codeql-action/init",
      {
        languages: language,
        "build-mode": "none",
      },
    );
    const analyze = requiredAction(
      steps,
      "Analyze and upload CodeQL results",
      "github/codeql-action/analyze",
      {
        category: `/language:${language}`,
        upload: "always",
        "skip-queries": "false",
        "wait-for-processing": "true",
      },
    );
    if (
      String(init.uses).split("@")[1] !== String(analyze.uses).split("@")[1]
    ) {
      fail("CodeQL init and analyze must use the same pinned commit.");
    }
    if (String(init.uses).split("@")[1] !== approvedCodeqlActionSha) {
      fail("CodeQL init and analyze must use the reviewed pinned commit.");
    }
    if (steps.indexOf(init) >= steps.indexOf(analyze))
      fail("CodeQL initialization must precede analysis.");
  }
}

/**
 * cargo-deny runs from the upstream release binary at this exact version, and only
 * after `sha256sum -c` matches this SHA-256 (#676). The Docker-based
 * EmbarkStudios/cargo-deny-action built its image from a Docker Hub base, whose
 * anonymous pull limit failed the lane; a release binary pulls no image. cargo-deny
 * still fetches the advisory database itself, from its default GitHub URL.
 */
export const cargoDenyVersion = "0.20.2";
export const cargoDenyArchiveSha256 =
  "9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f";
export const cargoDenyInstallStep = `Install cargo-deny ${cargoDenyVersion}`;
const cargoDenyInstallCommands = [
  "set -euo pipefail",
  `version=${cargoDenyVersion}`,
  'release="cargo-deny-${version}-x86_64-unknown-linux-musl"',
  'cd "$RUNNER_TEMP"',
  'curl -fsSL -o "${release}.tar.gz" \\',
  '"https://github.com/EmbarkStudios/cargo-deny/releases/download/${version}/${release}.tar.gz"',
  `echo "${cargoDenyArchiveSha256}  \${release}.tar.gz" | sha256sum -c -`,
  "mkdir cargo-deny",
  'tar -xzf "${release}.tar.gz" -C cargo-deny --strip-components=1 "${release}/cargo-deny"',
  "cargo-deny/cargo-deny --version",
];
const cargoDenyBinary = '"$RUNNER_TEMP/cargo-deny/cargo-deny"';
/**
 * The two policy runs, by absolute path to the verified binary so a `cargo-deny`
 * earlier on PATH (a restored `~/.cargo/bin`, say) cannot stand in for it: the
 * workspace against `deny.toml`, then the updater helper as the sole graph root
 * against its own ban list (KEL-53 §4). The arguments are the ones the action ran.
 */
export const cargoDenyChecks = [
  [
    "cargo-deny check",
    `${cargoDenyBinary} --log-level warn --manifest-path ./Cargo.toml --all-features check`,
  ],
  [
    "Updater helper edge set (KEL-53 T4d S9c)",
    `${cargoDenyBinary} --log-level warn --manifest-path crates/keld-updater-helper/Cargo.toml --all-features --config crates/keld-updater-helper/deny.toml check bans`,
  ],
] as const;
/**
 * The deny job's exact steps, in order: three setup actions (none of them a Docker
 * action), the verified install, then both policy runs. No other step may sit between
 * the install and the runs, where it could replace the verified binary.
 */
const cargoDenyJobSequence = [
  "actions/checkout",
  "dtolnay/rust-toolchain",
  "swatinem/rust-cache",
  cargoDenyInstallStep,
  ...cargoDenyChecks.map(([name]) => name),
];
/** Job keys that can change what the verified binary runs (`BASH_ENV`, a default shell) or pull an image. */
const cargoDenyForbiddenJobKeys = ["env", "defaults", "container", "services"];

function runLines(step: Mapping): string[] {
  return typeof step.run === "string"
    ? step.run
        .trim()
        .split(/\r?\n/)
        .map((line) => line.trim())
    : [];
}

/** The deny job: one verified install, then both policy runs, unconditional and unwrapped. */
function checkCargoDenyJob(
  jobs: Mapping,
  stepsByJob: Map<string, Mapping[]>,
): void {
  const steps = stepsByJob.get("deny");
  if (!steps)
    fail(
      "jobs.deny must exist; cargo-deny owns the Cargo advisory, ban, license and source policy.",
    );
  const job = mapping(jobs.deny, "jobs.deny");
  const forbidden = cargoDenyForbiddenJobKeys.filter((key) =>
    Object.hasOwn(job, key),
  );
  if (forbidden.length) {
    fail(
      `jobs.deny must not set ${forbidden.join(", ")}: job environment or defaults (BASH_ENV, a shell) can change what the verified binary runs, and a container or service pulls an image.`,
    );
  }
  const sequence = steps.map((step) =>
    Object.hasOwn(step, "uses")
      ? actionName(String(step.uses))
      : String(step.name),
  );
  if (JSON.stringify(sequence) !== JSON.stringify(cargoDenyJobSequence)) {
    fail(
      `jobs.deny must run exactly ${cargoDenyJobSequence.join(", ")}, in that order: no Docker action such as EmbarkStudios/cargo-deny-action (#676), and no other step that could replace the verified binary before it runs.`,
    );
  }
  inputsMatch(
    mapping(steps[0]!.with, "jobs.deny checkout.with"),
    { "persist-credentials": "false" },
    "jobs.deny checkout (the policy reads the event's own tree)",
  );
  const [install, ...checks] = steps.slice(3);
  exactKeys(install!, ["name", "run"], cargoDenyInstallStep);
  if (
    JSON.stringify(runLines(install!)) !==
    JSON.stringify(cargoDenyInstallCommands)
  ) {
    fail(
      `${cargoDenyInstallStep} must download release ${cargoDenyVersion} from its GitHub release URL and pass \`sha256sum -c\` against ${cargoDenyArchiveSha256} before extracting it; an unpinned version, a missing or different checksum, or a wrapper is not admitted.`,
    );
  }
  for (const [index, [name, command]] of cargoDenyChecks.entries()) {
    const check = checks[index]!;
    exactKeys(check, ["name", "run"], name);
    if (JSON.stringify(runLines(check)) !== JSON.stringify([command])) {
      fail(
        `${name} must run exactly \`${command}\`: the verified binary with the reviewed policy arguments, and no wrapper or suppression.`,
      );
    }
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
export function checkPullRequestTargetWorkflow(
  source: string,
  label: string,
): void {
  if (Buffer.byteLength(source) > 1024 * 1024)
    fail(`${label} exceeds the 1 MiB parsing budget.`);
  let parsed: unknown;
  try {
    parsed = Bun.YAML.parse(source);
  } catch (error) {
    fail(
      `cannot parse ${label}: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  finiteGraph(parsed);
  const workflow = mapping(parsed, `${label} (one YAML document)`);
  if (!workflowTriggers(workflow).includes("pull_request_target")) return;
  const jobs = mapping(workflow.jobs, `${label} jobs`);
  for (const [jobName, value] of Object.entries(jobs)) {
    const job = mapping(value, `${label} jobs.${jobName}`);
    if (Object.hasOwn(job, "uses") || !Array.isArray(job.steps)) {
      fail(
        `${label} jobs.${jobName} under pull_request_target must be concrete steps; a reusable workflow could run pull-request code.`,
      );
    }
    for (const [index, value] of job.steps.entries()) {
      const step = mapping(value, `${label} jobs.${jobName}.steps[${index}]`);
      if (Object.hasOwn(step, "run")) {
        fail(
          `${label} jobs.${jobName}.steps[${index}] is a run: step under pull_request_target; this secret-bearing trigger must not run shell code.`,
        );
      }
      const identity = actionName(
        actionRef(step.uses, `${label} jobs.${jobName}.steps[${index}]`),
      );
      if (
        identity === "actions/checkout" ||
        identity.startsWith("actions/checkout/")
      ) {
        fail(
          `${label} jobs.${jobName}.steps[${index}] checks out code under pull_request_target; never run pull-request code with the base repository's token and caches.`,
        );
      }
    }
  }
}

/**
 * All CI toolchain installers must select the canonical compiler pin.
 * The action commit and its `with: toolchain` input are separate identities:
 * each update to the reviewed immutable action SHA is a separate security
 * decision, and no job may fall back to runner-installed Rust.
 */
const approvedRustToolchainAction =
  "dtolnay/rust-toolchain@683b588c487a69d01f6c6ddf40f036ff682cc2c8";
const rustToolchainInstallerJobs = new Set([
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
]);

// The required-result command consumes these bindings in the documented
// 35-argument order. Keep the environment and complete shell script bound to
// one reviewed contract: injected BASH_ENV/PATH or a prepended exit 0 must fail.
const reviewedRequiredResultBindings: Record<string, string> = {
  KELD_RESULT_CHANGES: "${{ needs.changes.result }}",
  KELD_RESULT_FMT: "${{ needs.fmt.result }}",
  KELD_RESULT_CHECK: "${{ needs.check.result }}",
  KELD_RESULT_BUN: "${{ needs['bun-test'].result }}",
  KELD_RESULT_GUI: "${{ needs['linux-gui-smoke'].result }}",
  KELD_RESULT_MSRV: "${{ needs.msrv.result }}",
  KELD_RESULT_DENY: "${{ needs.deny.result }}",
  KELD_RESULT_SECRETS: "${{ needs.secrets.result }}",
  KELD_RESULT_HYGIENE: "${{ needs.hygiene.result }}",
  KELD_RESULT_MERMAID: "${{ needs.mermaid.result }}",
  KELD_ROUTE_RUST: "${{ needs.changes.outputs.rust }}",
  KELD_ROUTE_TS: "${{ needs.changes.outputs.ts }}",
  KELD_ROUTE_GUI: "${{ needs.changes.outputs.gui }}",
  KELD_ROUTE_MSRV: "${{ needs.changes.outputs.msrv }}",
  KELD_ROUTE_DENY: "${{ needs.changes.outputs.deny }}",
  KELD_ROUTE_HYGIENE: "${{ needs.changes.outputs.hygiene }}",
  KELD_ROUTE_DOCS: "${{ needs.changes.outputs.docs }}",
  KELD_ROUTE_MERMAID: "${{ needs.changes.outputs.mermaid }}",
  KELD_RESULT_CODEQL_RUST: "${{ needs['codeql-rust'].result }}",
  KELD_RESULT_CODEQL_JAVASCRIPT_TYPESCRIPT:
    "${{ needs['codeql-javascript-typescript'].result }}",
  KELD_RESULT_CODEQL_ACTIONS: "${{ needs['codeql-actions'].result }}",
  KELD_ROUTE_CODEQL_RUST: "${{ needs.changes.outputs.codeql_rust }}",
  KELD_ROUTE_CODEQL_JAVASCRIPT_TYPESCRIPT:
    "${{ needs.changes.outputs.codeql_javascript_typescript }}",
  KELD_ROUTE_CODEQL_ACTIONS: "${{ needs.changes.outputs.codeql_actions }}",
  KELD_RESULT_DEPENDENCY_REVIEW: "${{ needs['dependency-review'].result }}",
  KELD_RESULT_WORKSPACE: "${{ needs['workspace-contracts'].result }}",
  KELD_ROUTE_WORKSPACE: "${{ needs.changes.outputs.workspace }}",
  KELD_EVENT_NAME: "${{ github.event_name }}",
  KELD_ROUTE_RUST_DOCUMENTATION_ONLY:
    "${{ needs.changes.outputs.rust_documentation_only }}",
  KELD_ROUTE_CHECK_OS: "${{ needs.changes.outputs.check_os }}",
  KELD_RESULT_DOCTEST: "${{ needs.doctest.result }}",
  KELD_ROUTE_DOCTEST: "${{ needs.changes.outputs.doctest }}",
  KELD_RESULT_MEDIA: "${{ needs['windows-media'].result }}",
  KELD_ROUTE_MEDIA:
    "${{ needs.changes.outputs.rust == 'true' && contains(needs.changes.outputs.packages, 'keld-wv') }}",
  KELD_RESULT_ROUTER_CONTRACTS: "${{ needs['router-contracts'].result }}",
};

const reviewedRequiredResultNames = Object.keys(reviewedRequiredResultBindings);
const reviewedRequiredRun =
  [
    "tools/ci_required.sh test",
    "tools/ci_required.sh check \\",
    ...reviewedRequiredResultNames.map(
      (name, index) =>
        '  "$' +
        name +
        '"' +
        (index + 1 === reviewedRequiredResultNames.length ? "" : " \\"),
    ),
  ].join("\n") + "\n";

function refuseRustupToolchainOverride(env: unknown, label: string): void {
  if (env === undefined) return;
  const values = mapping(env, label);
  if (
    Object.keys(values).some(
      (name) => name.toUpperCase() === "RUSTUP_TOOLCHAIN",
    )
  ) {
    fail(
      `${label} must not override RUSTUP_TOOLCHAIN; environment overrides outrank rust-toolchain.toml.`,
    );
  }
}

/**
 * A run step can shadow the pin for itself and every later step without an
 * `env:` key: by exporting RUSTUP_TOOLCHAIN (inline or via $GITHUB_ENV), or
 * through a Rustup default or directory override. Explicit `cargo +<version>`
 * and `rustup toolchain install` stay admitted: they name their compiler at
 * the call site (the MSRV job) and change nothing for other commands.
 */
function refuseRustupRunOverride(run: unknown, label: string): void {
  if (run === undefined) return;
  const text = scalar(run);
  if (text === undefined) fail(`${label} must be a scalar shell script.`);
  // Bash removes line continuations, unquoted escapes and quote delimiters
  // before resolving command names. Inspect that conservative spelling too:
  // quoted "override" and RUSTUP_\\<newline>TOOLCHAIN must not evade the pin.
  const joined = text
    .replace(/\\\r?\n/g, "")
    .replace(/\\(?=[A-Za-z_])/g, "")
    .replace(/['"]/g, "");
  if (/RUSTUP_TOOLCHAIN/i.test(joined)) {
    fail(
      `${label} must not set RUSTUP_TOOLCHAIN; environment overrides outrank rust-toolchain.toml.`,
    );
  }
  if (/\brustup(?:[ \t]+-\S+)*[ \t]+(?:default|override)\b/.test(joined)) {
    fail(
      `${label} must not change the Rustup default or directory override; either outranks or replaces the pinned toolchain.`,
    );
  }
}

function refuseInheritedCustomShell(owner: Mapping, label: string): void {
  if (!Object.hasOwn(owner, "defaults")) return;
  const defaults = mapping(owner.defaults, `${label}.defaults`);
  if (Object.hasOwn(defaults, "run")) {
    const run = mapping(defaults.run, `${label}.defaults.run`);
    if (Object.hasOwn(run, "shell")) {
      fail(
        `${label} must not set defaults.run.shell; an inherited custom shell can hide failed required gates.`,
      );
    }
    if (Object.hasOwn(run, "working-directory")) {
      fail(
        `${label} must not set defaults.run.working-directory; a redirected relative script can bypass the reviewed CI gate.`,
      );
    }
  }
  // The current workflow has no defaults. Admit no other inherited execution
  // settings, even if a future syntax adds more than shell/working-directory.
  fail(
    `${label} must not set defaults; executable CI commands require their reviewed execution context.`,
  );
}

/** Preserve the branch-protection entry points and the default token scope. */
function checkWorkflowAdmission(workflow: Mapping): void {
  const events = workflow.on;
  if (events === null || typeof events !== "object" || Array.isArray(events)) {
    fail(
      "CI workflow must run on main pushes and all pull requests without filters.",
    );
  }
  const triggers = mapping(events, "workflow.on");
  const push = triggers.push;
  if (push === null || typeof push !== "object" || Array.isArray(push)) {
    fail(
      "CI workflow must run on main pushes and all pull requests without filters.",
    );
  }
  const pushTrigger = mapping(push, "workflow.on.push");
  if (
    Object.keys(triggers).length !== 2 ||
    !Object.hasOwn(triggers, "push") ||
    !Object.hasOwn(triggers, "pull_request") ||
    triggers.pull_request !== null ||
    Object.keys(pushTrigger).length !== 1 ||
    !Object.hasOwn(pushTrigger, "branches") ||
    !Array.isArray(pushTrigger.branches) ||
    pushTrigger.branches.length !== 1 ||
    pushTrigger.branches[0] !== "main"
  ) {
    fail(
      "CI workflow must run on main pushes and all pull requests without filters.",
    );
  }

  const rawPermissions = workflow.permissions;
  if (
    rawPermissions === null ||
    typeof rawPermissions !== "object" ||
    Array.isArray(rawPermissions)
  ) {
    fail("CI workflow permissions must remain read-only.");
  }
  const permissions = mapping(rawPermissions, "workflow.permissions");
  if (
    Object.keys(permissions).length !== 1 ||
    permissions.contents !== "read"
  ) {
    fail("CI workflow permissions must remain read-only.");
  }
  // The required-result script inherits workflow-level environment variables.
  // An extra BASH_ENV, PATH or shell option can bypass an otherwise exact run
  // step. Bind the three existing global settings and refuse all other inputs.
  inputsMatch(
    mapping(workflow.env, "CI workflow environment"),
    {
      CARGO_TERM_COLOR: "always",
      RUST_BACKTRACE: "1",
      CARGO_INCREMENTAL: "0",
    },
    "CI workflow environment",
  );
}

function directRunStep(
  steps: Mapping[],
  name: string,
  expectedRun: string,
  label: string,
): void {
  const step = namedStep(steps, name);
  // No condition, custom shell, or failure-tolerant wrapper may shadow this
  // literal, reviewed test invocation.
  if (
    Object.keys(step).length !== 2 ||
    !Object.hasOwn(step, "name") ||
    !Object.hasOwn(step, "run") ||
    step.run !== expectedRun
  ) {
    fail(
      `${label} must execute exactly, with no skip or failure-swallowing controls.`,
    );
  }
}

export function checkRustToolchainPins(
  source: string,
  toolchainToml: string,
): void {
  let parsedToolchain: unknown;
  try {
    parsedToolchain = Bun.TOML.parse(toolchainToml);
  } catch (error) {
    fail(
      `cannot parse rust-toolchain.toml: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  const toolchain = mapping(
    mapping(parsedToolchain, "rust-toolchain.toml").toolchain,
    "rust-toolchain.toml [toolchain]",
  );
  const selected = toolchain.channel;
  if (
    typeof selected !== "string" ||
    !/^[0-9]+\.[0-9]+\.[0-9]+$/.test(selected)
  ) {
    fail(
      "rust-toolchain.toml [toolchain].channel must select an exact stable X.Y.Z release.",
    );
  }
  let parsed: unknown;
  try {
    parsed = Bun.YAML.parse(source);
  } catch (error) {
    fail(
      `cannot parse workflow YAML: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  finiteGraph(parsed);
  const workflow = mapping(parsed, "workflow");
  refuseRustupToolchainOverride(workflow.env, "workflow.env");
  const jobs = mapping(workflow.jobs, "workflow.jobs");
  let pinnedAction: string | undefined;
  const installedJobs = new Set<string>();
  for (const [jobName, value] of Object.entries(jobs)) {
    const job = mapping(value, `jobs.${jobName}`);
    refuseRustupToolchainOverride(job.env, `jobs.${jobName}.env`);
    if (!Array.isArray(job.steps))
      fail(`jobs.${jobName} must contain concrete steps.`);
    for (const [index, value] of job.steps.entries()) {
      const step = mapping(value, `jobs.${jobName}.steps[${index}]`);
      refuseRustupToolchainOverride(
        step.env,
        `jobs.${jobName}.steps[${index}].env`,
      );
      refuseRustupRunOverride(step.run, `jobs.${jobName}.steps[${index}].run`);
      if (!Object.hasOwn(step, "uses")) continue;
      const label = `jobs.${jobName}.steps[${index}]`;
      const action = actionRef(step.uses, label);
      if (actionName(action) !== "dtolnay/rust-toolchain") continue;
      if (
        !rustToolchainInstallerJobs.has(jobName) ||
        installedJobs.has(jobName)
      ) {
        fail(
          `${label} is not the single approved Rust toolchain installer for its job.`,
        );
      }
      // The selected compiler must be installed before any executable work.
      // A skipped or fail-open setup step silently falls back to runner Rust.
      const firstStep = mapping(job.steps[0], `jobs.${jobName}.steps[0]`);
      if (
        index !== 1 ||
        actionName(actionRef(firstStep.uses, `jobs.${jobName}.steps[0]`)) !==
          "actions/checkout"
      ) {
        fail(
          `${label} must install Rust immediately after checkout, before any other step.`,
        );
      }
      if (
        Object.hasOwn(step, "if") ||
        Object.hasOwn(step, "continue-on-error")
      ) {
        fail(
          `${label} must run unconditionally and refuse installer failures.`,
        );
      }
      if (pinnedAction && action !== pinnedAction) {
        fail(
          `${label} must use the same reviewed immutable dtolnay/rust-toolchain action SHA as every other job.`,
        );
      }
      pinnedAction = action;
      installedJobs.add(jobName);
      const inputs = mapping(step.with, `${label}.with`);
      if (scalar(inputs.toolchain) !== selected) {
        fail(
          `${label}.with.toolchain must select Rust ${selected} from rust-toolchain.toml; a stale compiler pin changes cache and MSRV behavior.`,
        );
      }
    }
  }
  for (const jobName of rustToolchainInstallerJobs) {
    if (!installedJobs.has(jobName)) {
      fail(
        `jobs.${jobName} requires a pinned Rust toolchain installer; runner-default Rust is not admitted.`,
      );
    }
  }
  if (pinnedAction !== approvedRustToolchainAction) {
    fail(
      "all Rust toolchain installers must use the approved immutable action SHA.",
    );
  }
}

// Only these source-reviewed, immutable Actions may execute in the privileged
// CI workflow. An arbitrary 40-hex action or mutable docker://image:tag is
// still unreviewed code; the SHA syntax check alone is not an allowlist.
const reviewedWorkflowActionRefs = new Set([
  "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1",
  approvedRustToolchainAction,
  "Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0",
  "actions/cache/restore@55cc8345863c7cc4c66a329aec7e433d2d1c52a9",
  "actions/cache/save@55cc8345863c7cc4c66a329aec7e433d2d1c52a9",
  "oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6",
  "taiki-e/install-action@8cca0918f8471c33515202b73f56ab27f2bed283",
  `github/codeql-action/init@${approvedCodeqlActionSha}`,
  `github/codeql-action/analyze@${approvedCodeqlActionSha}`,
  "actions/dependency-review-action@a1d282b36b6f3519aa1f3fc636f609c47dddb294",
]);

function reviewedWorkflowActionUse(value: string): boolean {
  if (reviewedWorkflowActionRefs.has(value)) return true;
  // GitHub repository identity is case-insensitive. A harmless "/." suffix
  // at checkout's own pinned repo root must not defeat its existing checkout
  // credentials policy or be treated as a new third-party action.
  const revision = value.split("@");
  return (
    revision.length === 2 &&
    revision[1] === "3d3c42e5aac5ba805825da76410c181273ba90b1" &&
    ["actions/checkout", "actions/checkout/."].includes(actionName(value))
  );
}

/** Validate security effects over parsed workflow objects, never YAML line shapes. */
export function checkWorkflowSecurity(source: string): void {
  if (Buffer.byteLength(source) > 1024 * 1024)
    fail("workflow exceeds the 1 MiB parsing budget.");
  let parsed: unknown;
  try {
    parsed = Bun.YAML.parse(source);
  } catch (error) {
    fail(
      `cannot parse workflow YAML: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  finiteGraph(parsed);
  const workflow = mapping(parsed, "workflow (one YAML document)");
  checkWorkflowAdmission(workflow);
  refuseInheritedCustomShell(workflow, "workflow");
  const jobs = mapping(workflow.jobs, "workflow.jobs");
  if (Object.keys(jobs).length === 0) fail("workflow.jobs must not be empty.");
  const stepsByJob = new Map<string, Mapping[]>();
  let checkouts = 0;
  for (const [jobName, value] of Object.entries(jobs)) {
    const job = mapping(value, `jobs.${jobName}`);
    refuseInheritedCustomShell(job, `jobs.${jobName}`);
    if (
      !Array.isArray(job.steps) ||
      job.steps.length === 0 ||
      Object.hasOwn(job, "uses")
    ) {
      fail(
        `jobs.${jobName} requires a nonempty concrete steps array; opaque/reusable job shapes need a reviewed contract.`,
      );
    }
    const steps = job.steps.map((step, index) =>
      mapping(step, `jobs.${jobName}.steps[${index}]`),
    );
    stepsByJob.set(jobName, steps);
    for (const [index, step] of steps.entries()) {
      const label = `jobs.${jobName}.steps[${index}]`;
      const uses = Object.hasOwn(step, "uses");
      if (uses === Object.hasOwn(step, "run"))
        fail(
          `${label} must contain exactly one action or executable run string.`,
        );
      if (!uses) {
        if (typeof step.run !== "string" || !step.run.trim())
          fail(`${label}.run must be a nonempty string.`);
        checkAptStepTimeout(
          step,
          job,
          `${label} (${typeof step.name === "string" ? step.name : "unnamed"})`,
        );
        continue;
      }
      const action = actionRef(step.uses, label);
      const identity = actionName(action);
      if (
        identity === "actions/checkout" ||
        identity.startsWith("actions/checkout/")
      ) {
        checkouts++;
        const inputs = mapping(step.with, `${label}.with`);
        if (scalar(inputs["persist-credentials"]) !== "false") {
          fail(
            `${label} actions/checkout must set persist-credentials: false; every checkout is checked.`,
          );
        }
        // Every CI lane must test the event's source tree. A pinned checkout
        // action can still select a different ref, repository or partial tree
        // through its inputs. Admit only the existing reviewed settings.
        const reviewedInputs = Object.hasOwn(inputs, "fetch-depth")
          ? ["persist-credentials", "fetch-depth"]
          : ["persist-credentials"];
        exactKeys(
          inputs,
          reviewedInputs,
          label + " event-owned checkout inputs",
        );
        if (
          Object.hasOwn(inputs, "fetch-depth") &&
          scalar(inputs["fetch-depth"]) !== "0"
        ) {
          fail(
            label +
              " event-owned checkout permits fetch-depth only 0 when specified.",
          );
        }
      }
      if (identity === "taiki-e/install-action") {
        // An approved action SHA alone cannot protect its binary trust policy:
        // checksum:false or fallback:cargo-install bypasses reviewed defaults.
        // These are the only two reviewed invocations and tool selections.
        const tool =
          jobName === "check"
            ? "nextest,just@1.58.0"
            : jobName === "windows-media"
              ? "nextest"
              : undefined;
        if (!tool)
          fail(`${label} taiki installer is not approved in jobs.${jobName}.`);
        exactKeys(
          step,
          ["uses", "with"],
          `${label} reviewed taiki installer step`,
        );
        inputsMatch(
          mapping(step.with, `${label}.with`),
          { tool },
          `${label} reviewed taiki installer inputs`,
        );
      }
    }
  }
  if (checkouts === 0)
    fail("workflow must contain its audited checkout steps.");
  // Every selected native test runner must install its reviewed executables.
  // Inspecting only the inputs of an existing action is insufficient: deleting
  // that action could fall back to an ambient, unreviewed Nextest/just binary.
  for (const [jobName, consumers] of [
    ["check", ["Worktree cache tool provenance test", "test"]],
    ["windows-media", ["Windows media guard acceptance (KEL-132)"]],
  ] as const) {
    const steps = stepsByJob.get(jobName);
    if (!steps) fail(`jobs.${jobName} must exist to run its native tests.`);
    const installers = steps.flatMap((step, index) =>
      typeof step.uses === "string" &&
      actionName(step.uses) === "taiki-e/install-action"
        ? [index]
        : [],
    );
    if (installers.length !== 1) {
      fail(
        `jobs.${jobName} must contain exactly one reviewed taiki installer, never runner-default native tools.`,
      );
    }
    for (const consumer of consumers) {
      if (steps.indexOf(namedStep(steps, consumer)) <= installers[0]!) {
        fail(
          `jobs.${jobName} reviewed taiki installer must precede ${consumer}.`,
        );
      }
    }
  }
  // GitHub treats a skipped step in an otherwise successful job as SUCCESS.
  // CI required sees only the job result, so the selected media oracle must
  // not have an independent step-level skip or failure-masking control.
  // Rust hygiene owns the exact, independently reviewed PowerShell body.
  const mediaAcceptance = namedStep(
    stepsByJob.get("windows-media")!,
    "Windows media guard acceptance (KEL-132)",
  );
  exactKeys(
    mediaAcceptance,
    ["name", "shell", "run"],
    "Windows media acceptance step",
  );
  if (mediaAcceptance.shell !== "pwsh") {
    fail("Windows media acceptance step must use the reviewed pwsh shell.");
  }
  checkCodeqlJobs(jobs, stepsByJob);
  checkWebkitgtkAptCache(jobs, stepsByJob);
  checkCargoDenyJob(jobs, stepsByJob);
  const dependencies = stepsByJob.get("dependency-review");
  if (!dependencies) fail("CodeQL and dependency-review jobs must exist.");
  const changes = stepsByJob.get("changes");
  if (!changes) fail("the changes job must exist.");
  // A 56-second suite used to gate the outputs of every OS and CodeQL lane.
  // It now has its own always-created runner, but remains a mandatory result.
  const routerJob = mapping(jobs["router-contracts"], "jobs.router-contracts");
  exactKeys(
    routerJob,
    ["name", "runs-on", "timeout-minutes", "steps"],
    "router contract regression job",
  );
  if (
    routerJob.name !== "Router contract tests" ||
    routerJob["runs-on"] !== "ubuntu-latest" ||
    scalar(routerJob["timeout-minutes"]) !== "10"
  ) {
    fail(
      "router contract regression job must run unconditionally on hosted Ubuntu with the reviewed 10-minute bound.",
    );
  }
  const routerSteps = stepsByJob.get("router-contracts")!;
  if (
    routerSteps.length !== 3 ||
    actionName(actionRef(routerSteps[0]!.uses, "router contracts checkout")) !==
      "actions/checkout" ||
    scalar(
      mapping(routerSteps[0]!.with, "router contracts checkout inputs")[
        "fetch-depth"
      ],
    ) !== "0" ||
    actionName(actionRef(routerSteps[1]!.uses, "router contracts compiler")) !==
      "dtolnay/rust-toolchain"
  ) {
    fail(
      "router contract regression job must have full-history checkout, reviewed Rust toolchain and exact test in that order.",
    );
  }
  directRunStep(
    routerSteps,
    "Router contract tests",
    "tools/ci_changes_test.sh",
    "independent router contract regression tests",
  );
  if (
    routerSteps[2]!.name !== "Router contract tests" ||
    changes.some(
      (step) =>
        step.name === "Router contract tests" ||
        (typeof step.run === "string" &&
          step.run.includes("tools/ci_changes_test.sh")),
    )
  ) {
    fail(
      "router contracts must run only in the independent regression job, not block change-router outputs.",
    );
  }
  namedStep(changes, "Test module import ownership");
  namedStep(changes, "Source budget tool tests");
  directRunStep(
    changes,
    "CI attempt timing reporter regression tests",
    "python3 -B tools/test_ci_attempt_metrics.py",
    "CI attempt timing reporter tests",
  );
  const fmt = stepsByJob.get("fmt");
  if (!fmt) fail("rustfmt job must exist.");
  const fmtJob = mapping(jobs.fmt, "jobs.fmt");
  if (
    fmtJob.if !== "needs.changes.outputs.rust == 'true'" ||
    Object.hasOwn(fmtJob, "continue-on-error")
  ) {
    fail(
      "rustfmt job must preserve its changed-Rust routing and failing exit status.",
    );
  }
  const fmtCommands = fmt.filter((step) => Object.hasOwn(step, "run"));
  // Other independent run steps cannot suppress the exit status of this step.
  // Preserve one exact, unwrapped Cargo formatter invocation, but do not
  // reject unrelated diagnostic steps in the formatting job.
  const cargoFmtCommands = fmtCommands.filter(
    (step) => step.run === "cargo fmt --all --check",
  );
  if (
    cargoFmtCommands.length !== 1 ||
    Object.keys(cargoFmtCommands[0]).length !== 1
  ) {
    fail(
      "rustfmt cargo fmt command must execute exactly and fail on format drift.",
    );
  }
  directRunStep(
    fmt,
    "rustfmt include!d and macro-body #[path] files",
    "tools/fmt_included.sh test\ntools/fmt_included.sh --check\n",
    "rustfmt included-file commands",
  );
  if (
    fmt.some(
      (step) =>
        typeof step.uses === "string" &&
        actionName(step.uses).startsWith("swatinem/rust-cache"),
    )
  ) {
    fail("rustfmt must not restore or save a compiled Cargo cache.");
  }
  const required = stepsByJob.get("required");
  if (!required) fail("CI required job must exist.");
  const requiredJob = mapping(jobs.required, "jobs.required");
  if (requiredJob.if !== "${{ always() }}") {
    fail(
      "CI required must use exact always() condition so failed dependencies cannot skip merge admission.",
    );
  }
  if (Object.hasOwn(requiredJob, "env")) {
    fail(
      "CI required job must not set env; its evaluator has only reviewed result bindings.",
    );
  }
  if (Object.hasOwn(requiredJob, "continue-on-error")) {
    fail("CI required must not tolerate failures.");
  }
  exactKeys(
    requiredJob,
    ["name", "if", "needs", "runs-on", "timeout-minutes", "steps"],
    "CI required job",
  );
  if (
    requiredJob.name !== "CI required" ||
    requiredJob["runs-on"] !== "ubuntu-latest" ||
    scalar(requiredJob["timeout-minutes"]) !== "10"
  ) {
    fail(
      "CI required job must use the reviewed name, hosted Ubuntu runner and 10-minute bound.",
    );
  }
  const requiredNeeds = [
    "changes",
    "router-contracts",
    "fmt",
    "check",
    "bun-test",
    "linux-gui-smoke",
    "msrv",
    "deny",
    "secrets",
    "hygiene",
    "mermaid",
    "codeql-rust",
    "codeql-javascript-typescript",
    "codeql-actions",
    "dependency-review",
    "workspace-contracts",
    "doctest",
    "windows-media",
  ];
  const actualNeeds = requiredJob.needs;
  if (
    !Array.isArray(actualNeeds) ||
    actualNeeds.length !== requiredNeeds.length ||
    requiredNeeds.some((name, index) => actualNeeds[index] !== name)
  ) {
    fail(
      "CI required must depend on every reviewed routed and unconditional job, exactly once.",
    );
  }
  if (required.length !== 2) {
    fail(
      "CI required must retain exactly two ordered steps: event checkout and result evaluator.",
    );
  }
  const checkout = required[0]!;
  exactKeys(checkout, ["uses", "with"], "CI required checkout");
  if (
    actionName(actionRef(checkout.uses, "CI required checkout")) !==
    "actions/checkout"
  ) {
    fail(
      "CI required checkout must read the event-owned source with the reviewed action.",
    );
  }
  inputsMatch(
    mapping(checkout.with, "CI required checkout inputs"),
    { "persist-credentials": "false" },
    "CI required checkout",
  );
  const evaluator = namedStep(required, "Verify required CI results");
  if (
    required[1] !== evaluator ||
    Object.keys(evaluator).length !== 3 ||
    !Object.hasOwn(evaluator, "name") ||
    !Object.hasOwn(evaluator, "env") ||
    typeof evaluator.run !== "string"
  ) {
    fail(
      "CI required evaluator must execute directly without shell, condition or failure swallowing.",
    );
  }
  inputsMatch(
    mapping(evaluator.env, "CI required evaluator environment"),
    reviewedRequiredResultBindings,
    "CI required evaluator environment",
  );
  if (evaluator.run !== reviewedRequiredRun) {
    fail(
      "CI required evaluator must run exact reviewed script with all 35 results and no injected commands.",
    );
  }
  const review = requiredAction(
    dependencies,
    "Review dependency vulnerabilities",
    "actions/dependency-review-action",
    {
      "base-ref": baseRef,
      "head-ref": headRef,
      "fail-on-severity": "low",
      "fail-on-scopes": "runtime, development, unknown",
      "vulnerability-check": "true",
      "license-check": "false",
      "comment-summary-in-pr": "never",
      "retry-on-snapshot-warnings": "false",
      "warn-only": "false",
      "show-openssf-scorecard": "false",
    },
  );
  const metadata = namedStep(
    dependencies,
    "Reject incomplete dependency metadata",
  );
  exactKeys(metadata, ["name", "env", "run"], "dependency metadata admission");
  inputsMatch(
    mapping(metadata.env, "dependency metadata env"),
    {
      GH_TOKEN: "${{ github.token }}",
      KELD_DEPENDENCY_REPOSITORY: "${{ github.repository }}",
      KELD_DEPENDENCY_BASE: baseRef,
      KELD_DEPENDENCY_HEAD: headRef,
    },
    "dependency metadata env",
  );
  const commands =
    typeof metadata.run === "string"
      ? metadata.run
          .trim()
          .split(/\r?\n/)
          .map((line) => line.trim())
      : [];
  const expectedCommands = [
    "tools/dependency_review_metadata.sh test",
    'tools/dependency_review_metadata.sh check "$KELD_DEPENDENCY_REPOSITORY" "$KELD_DEPENDENCY_BASE" "$KELD_DEPENDENCY_HEAD"',
  ];
  if (JSON.stringify(commands) !== JSON.stringify(expectedCommands))
    fail(
      "dependency metadata admission must execute its exact checks without wrappers.",
    );
  if (dependencies.indexOf(metadata) >= dependencies.indexOf(review))
    fail("metadata admission must precede dependency review.");

  // Parsed traversal catches flow-style YAML (e.g. "- {run: 'exit 1',
  // continue-on-error: true}") that the separate line-oriented Rust hygiene
  // scanner cannot interpret. A selected failed step must never pass CI required.
  for (const [jobName, value] of Object.entries(jobs)) {
    const job = mapping(value, `jobs.${jobName}`);
    if (Object.hasOwn(job, "continue-on-error")) {
      fail("CI workflow jobs and steps must not tolerate failures.");
    }
    for (const [index, step] of (stepsByJob.get(jobName) ?? []).entries()) {
      if (Object.hasOwn(step, "continue-on-error")) {
        fail("CI workflow jobs and steps must not tolerate failures.");
      }
      if (
        Object.hasOwn(step, "uses") &&
        !reviewedWorkflowActionUse(String(step.uses))
      ) {
        fail(
          `CI workflow steps must use only reviewed immutable action refs: jobs.${jobName}.steps[${index}].`,
        );
      }
    }
  }
}

if (import.meta.main) {
  try {
    const [, , command, root = ".", ...extra] = process.argv;
    if (command !== "check" || extra.length)
      fail("use ci_workflow_security.ts check [workspace].");
    const workflowSource = readFileSync(
      resolve(root, ".github/workflows/ci.yml"),
      "utf8",
    );
    checkWorkflowSecurity(workflowSource);
    checkRustToolchainPins(
      workflowSource,
      readFileSync(resolve(root, "rust-toolchain.toml"), "utf8"),
    );
    const workflowsDir = resolve(root, ".github/workflows");
    for (const name of readdirSync(workflowsDir)
      .filter((entry) => /\.ya?ml$/.test(entry))
      .sort()) {
      checkPullRequestTargetWorkflow(
        readFileSync(resolve(workflowsDir, name), "utf8"),
        `.github/workflows/${name}`,
      );
    }
    checkWindowsMediaOracle(readFileSync(resolve(root, windowsMediaOracle)));
    console.log("CI workflow security semantics ok");
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}

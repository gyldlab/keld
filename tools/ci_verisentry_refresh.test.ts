import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const refreshSource = readFileSync(
  ".github/workflows/ci-verisentry-refresh.yml",
  "utf8",
);
const trustedSource = readFileSync(
  ".github/workflows/ci-provenance.yml",
  "utf8",
);
const refresh = Bun.YAML.parse(refreshSource) as Record<string, any>;
const automatic = Bun.YAML.parse(trustedSource) as Record<string, any>;
const job = refresh.jobs.refresh;
const body = job.steps[1].with.script as string;
const verifierBody = automatic.jobs.provenance.steps[0].with.script as string;
const AsyncFunction = Object.getPrototypeOf(async function () {})
  .constructor as new (...args: string[]) => (...args: any[]) => Promise<void>;
const execute = new AsyncFunction(
  "github",
  "context",
  "core",
  "process",
  "setTimeout",
  "Date",
  body,
);
const base = "b".repeat(40);
const head = "a".repeat(40);
const merge = "c".repeat(40);
const protectedMain = "d".repeat(40);

function protectedPathBlock(code: string): string {
  const begin = code.indexOf("const protectedPath = (path) => {");
  const end = code.includes("// __PROTECTED_PATH_POLICY_END__")
    ? code.indexOf("// __PROTECTED_PATH_POLICY_END__", begin)
    : code.indexOf("const protectedChanges = [];", begin);
  if (begin < 0 || end < 0) throw Error("Protected policy no longer found");
  return code.slice(begin, end).trim();
}

function scenario(
  options: {
    eventName?: string;
    ref?: string;
    actor?: string;
    actorId?: string;
    repo?: string;
    owner?: string;
    prs?: Record<string, any>[];
    changedFiles?: number;
    files?: Record<string, any>[];
    compareBase?: string;
    readSequence?: Record<string, any>[];
    checks?: Record<string, any>[];
    checkApp?: number;
    failedGet?: boolean;
    failedCreate?: boolean;
    forcedMerge?: string | null;
    mergeable?: boolean | null;
    mainSha?: string;
    mainSequence?: string[];
    sourceSha?: string;
    workflowRef?: string;
    runAttempt?: string;
    triggeringActor?: string;
    failedCompare?: boolean;
    batchIndex?: string;
    clockMillis?: number;
  } = {},
) {
  const basePR = {
    number: 345,
    state: "open",
    head: { sha: head },
    base: { ref: "main", sha: base, repo: { full_name: "gyldlab/keld" } },
    changed_files: options.changedFiles ?? options.files?.length ?? 1,
    mergeable: options.mergeable === undefined ? true : options.mergeable,
    merge_commit_sha:
      options.forcedMerge === undefined ? merge : options.forcedMerge,
  };
  const inputs = options.prs ?? [basePR];
  const statuses: Record<string, any>[] = [];
  const messages: string[] = [];
  const warnings: string[] = [];
  const failures: string[] = [];
  let reads = 0;
  let compareReads = 0;
  let checkReads = 0;
  let mainReads = 0;
  const context = {
    repo: { owner: options.owner ?? "gyldlab", repo: options.repo ?? "keld" },
    eventName: options.eventName ?? "schedule",
    actor: options.actor ?? "github-actions[bot]",
    ref: options.ref ?? "refs/heads/main",
    sha: options.sourceSha ?? protectedMain,
  };
  const github = {
    rest: {
      git: {
        getRef: async (args: Record<string, unknown>) => {
          mainReads++;
          expect(args).toEqual({
            owner: "gyldlab",
            repo: "keld",
            ref: "heads/main",
          });
          return {
            data: {
              object: {
                sha:
                  options.mainSequence?.[
                    Math.min(mainReads - 1, options.mainSequence.length - 1)
                  ] ??
                  options.mainSha ??
                  protectedMain,
              },
            },
          };
        },
      },
      pulls: {
        list: () => {
          throw Error("open PR pagination mandatory");
        },
        get: async (args: Record<string, unknown>) => {
          expect(args).toMatchObject({ owner: "gyldlab", repo: "keld" });
          reads++;
          if (options.failedGet) throw Error("PR GET API unavailable");
          return {
            data:
              options.readSequence?.[
                Math.min(reads - 1, options.readSequence.length - 1)
              ] ??
              inputs.find((item) => item.number === args.pull_number) ??
              basePR,
          };
        },
      },
      repos: {
        compareCommits: async (args: Record<string, unknown>) => {
          compareReads++;
          if (options.failedCompare) throw Error("GitHub compare HTTP 403");
          expect(args).toMatchObject({
            owner: "gyldlab",
            repo: "keld",
            base,
            page: 1,
            per_page: 100,
          });
          expect(
            typeof args.head === "string" && /^[a-f0-9]{40}$/.test(args.head),
          ).toBe(true);
          return {
            data: {
              base_commit: { sha: options.compareBase ?? base },
              files: options.files ?? [
                { filename: "docs/normal.md", status: "modified" },
              ],
            },
          };
        },
      },
      checks: {
        listForRef: () => {
          throw Error("App check list requires pagination");
        },
        create: async (args: Record<string, any>) => {
          if (options.failedCreate) throw Error("App Checks API unavailable");
          statuses.push(args);
          return {
            data: {
              id: 999,
              app: { id: options.checkApp ?? 5550363 },
              head_sha: args.head_sha,
            },
          };
        },
      },
    },
    paginate: async (fn: unknown, args: Record<string, unknown>) => {
      if (fn === github.rest.pulls.list) {
        expect(args).toEqual({
          owner: "gyldlab",
          repo: "keld",
          state: "open",
          base: "main",
          per_page: 100,
        });
        return inputs;
      }
      if (fn === github.rest.checks.listForRef) {
        checkReads++;
        expect(args).toEqual({
          owner: "gyldlab",
          repo: "keld",
          ref: args.ref,
          check_name: "KELD VeriSentry",
          app_id: 5550363,
          filter: "all",
          per_page: 100,
        });
        expect(
          typeof args.ref === "string" && /^[a-f0-9]{40}$/.test(args.ref),
        ).toBe(true);
        return options.checks ?? [];
      }
      throw Error("Unexpected paginated method");
    },
  };
  const core = {
    info: (m: string) => messages.push(m),
    warning: (m: string) => warnings.push(m),
    setFailed: (m: string) => failures.push(m),
  };
  const process = {
    env: {
      GITHUB_ACTOR_ID: options.actorId ?? "41898282",
      GITHUB_RUN_ATTEMPT: options.runAttempt ?? "1",
      GITHUB_TRIGGERING_ACTOR: options.triggeringActor ?? context.actor,
      GITHUB_SHA: options.sourceSha ?? protectedMain,
      GITHUB_WORKFLOW_REF:
        options.workflowRef ??
        "gyldlab/keld/.github/workflows/ci-verisentry-refresh.yml@refs/heads/main",
      VERISENTRY_BATCH_INDEX: options.batchIndex ?? "auto",
    },
  };
  return {
    run: () =>
      execute(github, context, core, process, (done: () => void) => done(), {
        now: () => options.clockMillis ?? Date.now(),
        parse: Date.parse,
      }),
    statuses,
    messages,
    warnings,
    failures,
    getReads: () => reads,
    getCompareReads: () => compareReads,
    getCheckReads: () => checkReads,
    getMainReads: () => mainReads,
  };
}

test("refresh is a protected-main-only cron and authorized manual dispatch, with no PR code checkout", () => {
  expect(refresh.name).toBe("KELD VeriSentry trusted refresh");
  expect(Object.keys(refresh.on)).toEqual([
    "push",
    "schedule",
    "workflow_dispatch",
  ]);
  expect(refresh.on.push).toEqual({ branches: ["main"] });
  expect(refresh.on.schedule).toEqual([{ cron: "17 2 * * *" }]);
  expect(refresh.on.workflow_dispatch.inputs.batch_index.default).toBe("auto");
  expect(job.if).toContain("github.ref == 'refs/heads/main'");
  expect(job.if).toContain("github.run_attempt == 1");
  expect(job.if).toContain("github.event_name == 'schedule'");
  expect(job.if).toContain("github.event_name == 'push'");
  expect(job.if).toContain("github.run_attempt == 1");
  expect(job.if).toContain("github.actor == github.triggering_actor");
  expect(job.environment).toBe("verisentry-publisher");
  expect(job["timeout-minutes"]).toBe(30);
  expect(job.permissions).toEqual({
    contents: "read",
    "pull-requests": "read",
  });
  expect(refresh.permissions).toEqual({
    contents: "read",
    "pull-requests": "read",
  });
  expect(job.steps).toHaveLength(2);
  expect(job.steps[1].env).toEqual({
    VERISENTRY_BATCH_INDEX: "${{ inputs.batch_index || 'auto' }}",
  });
  expect(job.steps[0].uses).toBe(
    "actions/create-github-app-token@64ab729a6294000f5aa4a4bc0b73d1f32bc45510",
  );
  expect(job.steps[0].with).toEqual({
    "app-id": "5550363",
    "private-key": "${{ secrets.VERISENTRY_APP_PRIVATE_KEY }}",
    owner: "gyldlab",
    repositories: "keld",
    "permission-checks": "write",
    "permission-pull-requests": "read",
    "permission-contents": "read",
  });
  expect(job.steps[1].uses).toBe(
    "actions/github-script@3a2844b7e9c422d3c10d287c895573f7108da1b3",
  );
  expect(refreshSource).not.toMatch(/actions\/checkout|\n\s+-\s+run:/);
  expect(protectedPathBlock(body)).toBe(protectedPathBlock(verifierBody));
});

test("a normal current PR gets exactly one App ID 5550363 success on test merge, never head", async () => {
  const s = scenario();
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(1);
  expect(s.statuses[0]).toMatchObject({
    owner: "gyldlab",
    repo: "keld",
    name: "KELD VeriSentry",
    head_sha: merge,
    status: "completed",
    conclusion: "success",
  });
  expect(s.statuses[0].output.summary).toContain("PR #345");
  expect(s.failures).toEqual([]);
  expect(s.getReads()).toBe(4);
  expect(s.getCompareReads()).toBe(1);
});

test("protected CI path is a real App failure, not an omitted or neutral result", async () => {
  const s = scenario({
    files: [{ filename: ".github/workflows/ci.yml", status: "modified" }],
  });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(1);
  expect(s.statuses[0].conclusion).toBe("failure");
  expect(s.failures).toHaveLength(0);
});

test("300 changed files is failure despite missing immutable comparison", async () => {
  const s = scenario({ changedFiles: 300 });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.getCompareReads()).toBe(0);
  expect(s.statuses).toHaveLength(1);
  expect(s.statuses[0].conclusion).toBe("failure");
});

test.each([
  { ref: "refs/heads/feature" },
  { eventName: "pull_request_target" },
  { eventName: "workflow_dispatch", actor: "outsider", actorId: "999" },
  { eventName: "workflow_dispatch", actor: "0monish", actorId: "999" },
  {
    eventName: "workflow_dispatch",
    actor: "0monish",
    actorId: "155816356",
    runAttempt: "2",
  },
  {
    eventName: "workflow_dispatch",
    actor: "0monish",
    actorId: "155816356",
    triggeringActor: "attacker",
  },
  { repo: "forked" },
  { owner: "attacker" },
])("untrusted refresh event %p never gets to Github PR API", async (o) => {
  const s = scenario(o);
  await expect(s.run()).rejects.toThrow("refuses non-main/non-maintainer");
  expect(s.getReads()).toBe(0);
  expect(s.statuses).toHaveLength(0);
});

test("protected-main push immediately refreshes test-merge App status on base changes", async () => {
  const s = scenario({
    eventName: "push",
    actor: "unauthenticated-push-actor",
    actorId: "9",
  });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(1);
  expect(s.statuses[0].conclusion).toBe("success");
});

test("Monish may manually refresh protected-main checks with stable actor ID", async () => {
  const s = scenario({
    eventName: "workflow_dispatch",
    actor: "0monish",
    actorId: "155816356",
  });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses[0].conclusion).toBe("success");
});

test("stale head after immutable comparison cannot publish a PASS", async () => {
  const latest = {
    number: 345,
    state: "open",
    changed_files: 1,
    head: { sha: "d".repeat(40) },
    base: { ref: "main", sha: base, repo: { full_name: "gyldlab/keld" } },
    mergeable: true,
    merge_commit_sha: merge,
  };
  const initial = { ...latest, head: { sha: head } };
  const s = scenario({ readSequence: [initial, latest, latest] });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(0);
  expect(s.getCheckReads()).toBe(0);
});

test("unmergeable or not-ready PR never gets a synthetic PASS on source head", async () => {
  for (const value of [false, null]) {
    const s = scenario({ mergeable: value });
    await expect(s.run()).resolves.toBeUndefined();
    expect(s.statuses).toHaveLength(0);
  }
});

test("GitHub asynchronous mergeability null is retried before check publication", async () => {
  const current = {
    number: 345,
    state: "open",
    head: { sha: head },
    base: { ref: "main", sha: base, repo: { full_name: "gyldlab/keld" } },
    changed_files: 1,
    mergeable: true,
    merge_commit_sha: merge,
  };
  const computing = { ...current, mergeable: null, merge_commit_sha: null };
  // 2 immutable-comparison GETs, 2 mergeability GETs, 1 last-minute PR GET.
  const s = scenario({
    readSequence: [current, current, computing, current, current],
  });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.getReads()).toBe(5);
  expect(s.statuses).toHaveLength(1);
  expect(s.statuses[0]?.conclusion).toBe("success");
  expect(s.failures).toHaveLength(0);
});

test("final mergeable:null re-read cannot erase a previously proven immutable merge", async () => {
  const current = {
    number: 345,
    state: "open",
    changed_files: 1,
    head: { sha: head },
    base: { ref: "main", sha: base, repo: { full_name: "gyldlab/keld" } },
    mergeable: true,
    merge_commit_sha: merge,
  };
  const recomputing = { ...current, mergeable: null };
  const s = scenario({
    readSequence: [current, current, current, recomputing],
  });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.getReads()).toBe(4);
  expect(s.statuses).toHaveLength(1);
  expect(s.statuses[0].conclusion).toBe("success");
  expect(s.messages.join(" ")).toContain("recomputes mergeability");
  expect(s.failures).toHaveLength(0);
});

test("final definite conflict after previously positive immutable merge never signs App PASS", async () => {
  const current = {
    number: 345,
    state: "open",
    changed_files: 1,
    head: { sha: head },
    base: { ref: "main", sha: base, repo: { full_name: "gyldlab/keld" } },
    mergeable: true,
    merge_commit_sha: merge,
  };
  const conflict = { ...current, mergeable: false };
  const s = scenario({ readSequence: [current, current, current, conflict] });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(0);
  expect(s.failures).toHaveLength(1);
  expect(s.warnings.join(" ")).toContain("definite conflict");
});

test("final untrusted changed test-merge SHA does not sign PASS even with transient null", async () => {
  const current = {
    number: 345,
    state: "open",
    changed_files: 1,
    head: { sha: head },
    base: { ref: "main", sha: base, repo: { full_name: "gyldlab/keld" } },
    mergeable: true,
    merge_commit_sha: merge,
  };
  const drift = {
    ...current,
    mergeable: null,
    merge_commit_sha: "e".repeat(40),
  };
  const s = scenario({ readSequence: [current, current, current, drift] });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(0);
});

test("GitHub mergeability never resolves: no fabricated PASS and job reports incomplete", async () => {
  const s = scenario({ mergeable: null, forcedMerge: null });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.getReads()).toBe(5); // 2 proof reads + 3 bounded readiness attempts
  expect(s.statuses).toHaveLength(0);
  expect(s.failures).toHaveLength(1);
  expect(s.warnings.join(" ")).toContain("mergeability pending");
});

test("a genuinely unmergeable PR is skipped without a misleading merge-SHA check", async () => {
  const s = scenario({ mergeable: false });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(0);
  expect(s.failures).toHaveLength(0);
});

test("old App successes expire after four days; daily refresh renews from verified main", async () => {
  const old = new Date(Date.now() - 5 * 86400 * 1000).toISOString();
  const s = scenario({
    checks: [
      {
        id: 42,
        app: { id: 5550363 },
        head_sha: merge,
        status: "completed",
        conclusion: "success",
        completed_at: old,
      },
    ],
  });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(1);
  expect(s.statuses[0].conclusion).toBe("success");
});

test("fresh matching App check is reused instead of flooding GitHub check-run quota", async () => {
  const recent = new Date(Date.now() - 30000).toISOString();
  const s = scenario({
    checks: [
      {
        id: 42,
        app: { id: 5550363 },
        head_sha: merge,
        status: "completed",
        conclusion: "success",
        completed_at: recent,
      },
    ],
  });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(0);
  expect(s.failures).toHaveLength(0);
});

test("existing App FAIL cannot silently become PASS from scheduled restarts", async () => {
  const s = scenario({
    checks: [
      {
        id: 42,
        app: { id: 5550363 },
        head_sha: merge,
        status: "completed",
        conclusion: "failure",
        completed_at: new Date().toISOString(),
      },
    ],
  });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(0);
  expect(s.failures).toHaveLength(1);
});

test("a transient 403 Compare Commits outage must NOT poison an otherwise benign PR", async () => {
  const outage = scenario({ failedCompare: true });
  await expect(outage.run()).resolves.toBeUndefined();
  expect(outage.getCompareReads()).toBe(1);
  expect(outage.statuses).toHaveLength(0);
  expect(outage.failures).toHaveLength(1);
  expect(outage.warnings.join(" ")).toContain("mergeability pending");
  const recovered = scenario();
  await expect(recovered.run()).resolves.toBeUndefined();
  expect(recovered.statuses).toHaveLength(1);
  expect(recovered.statuses[0].conclusion).toBe("success");
  expect(recovered.failures).toHaveLength(0);
});

test("incomplete comparison metadata fails the refresh without writing sticky App failure", async () => {
  const s = scenario({ compareBase: "f".repeat(40) });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(0);
  expect(s.failures).toHaveLength(1);
});

test("abandoned manual App approval or revocation watermark cannot be rewritten by scheduler", async () => {
  for (const decision of ["approve", "revoke"]) {
    const summary = `actor=0monish decision=${decision} pr=345 base=${base} head=${head} merge=${merge} maintenance-run=220`;
    const s = scenario({
      checks: [
        {
          id: 42,
          app: { id: 5550363 },
          head_sha: merge,
          status: "completed",
          conclusion: decision === "approve" ? "success" : "failure",
          completed_at: new Date().toISOString(),
          output: { summary },
        },
      ],
    });
    await expect(s.run()).resolves.toBeUndefined();
    expect(s.statuses).toHaveLength(0);
    expect(s.failures).toHaveLength(1);
  }
});

test("candidate-controlled same-name Actions status is NEVER accepted as App proof", async () => {
  const s = scenario({
    checks: [
      {
        id: 55,
        app: { id: 15368 },
        head_sha: merge,
        status: "completed",
        conclusion: "success",
        completed_at: new Date().toISOString(),
      },
    ],
  });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(0);
  expect(s.failures).toHaveLength(1);
});

test("duplicate PRs with identical merge SHA are fail-closed before check publication", async () => {
  const other = {
    number: 346,
    state: "open",
    head: { sha: head },
    base: { ref: "main", sha: base, repo: { full_name: "gyldlab/keld" } },
    changed_files: 1,
    mergeable: true,
    merge_commit_sha: merge,
  };
  const first = { ...other, number: 345 };
  const s = scenario({ prs: [first, other] });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(0);
  expect(s.failures).toHaveLength(1);
});

test("GitHub write failure is reported, never falsely celebrated as a successful refresh", async () => {
  const s = scenario({ failedCreate: true });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(0);
  expect(s.failures).toHaveLength(1);
});

test("old scheduled-run rerun cannot publish a genuine-App PASS under an outdated trust policy", async () => {
  const s = scenario({ runAttempt: "2" });
  await expect(s.run()).rejects.toThrow("rerun events");
  expect(s.statuses).toHaveLength(0);
  expect(s.getMainReads()).toBe(0);
});

test("delayed scheduled job with obsolete protected-main source SHA cannot sign any PR", async () => {
  const s = scenario({ sourceSha: "e".repeat(40) });
  await expect(s.run()).rejects.toThrow("stale protected-main workflow SHA");
  expect(s.getMainReads()).toBe(1);
  expect(s.getReads()).toBe(0);
  expect(s.statuses).toHaveLength(0);
});

test("main changes while checking PRs: no App check can be minted by stale policy", async () => {
  const s = scenario({ mainSequence: [protectedMain, "e".repeat(40)] });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.getMainReads()).toBe(2);
  expect(s.statuses).toHaveLength(0);
  expect(s.failures).toHaveLength(1);
});

test("scheduled workflow cannot claim another workflow source path", async () => {
  const s = scenario({
    workflowRef: "gyldlab/keld/.github/workflows/ci-evil.yml@refs/heads/main",
  });
  await expect(s.run()).rejects.toThrow("rerun events");
  expect(s.statuses).toHaveLength(0);
});

function largePRs(count: number, pending = 0): Record<string, any>[] {
  return Array.from({ length: count }, (_, i) => ({
    number: 1000 + i,
    state: "open",
    head: { sha: (i + 10000).toString(16).padStart(40, "0") },
    base: { ref: "main", sha: base, repo: { full_name: "gyldlab/keld" } },
    changed_files: 1,
    mergeable: i < pending ? null : true,
    merge_commit_sha:
      i < pending ? null : (i + 1).toString(16).padStart(40, "0"),
  }));
}

test("501 open main PRs cannot globally abort: authorized batches 0 and 4 both make bounded progress", async () => {
  const all = largePRs(501);
  const first = scenario({ prs: all, batchIndex: "0" });
  await expect(first.run()).resolves.toBeUndefined();
  expect(first.statuses).toHaveLength(101);
  expect(first.getReads()).toBe(101 * 4);
  expect(first.failures).toHaveLength(0);
  expect(first.messages.join(" ")).toContain("batch 1/5");
  const last = scenario({ prs: all, batchIndex: "4" });
  await expect(last.run()).resolves.toBeUndefined();
  expect(last.statuses).toHaveLength(100);
  expect(last.failures).toHaveLength(0);
  expect(last.messages.join(" ")).toContain("batch 5/5");
});

test("80 transient mergeability-null PRs do not starve next eligible PR within the same batch", async () => {
  const s = scenario({ prs: largePRs(81, 80), batchIndex: "0" });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.statuses).toHaveLength(1);
  expect(s.statuses[0]?.conclusion).toBe("success");
  expect(s.failures).toHaveLength(1); // pending computation is not green
  expect(s.warnings.length).toBeGreaterThan(0);
  expect(s.getReads()).toBe(80 * 5 + 4);
});

test.each(["-1", "999", "ABC", "4.2", "1e5", "01"])(
  "malformed or out-of-range manual batch index %s cannot mint App checks",
  async (batchIndex) => {
    const s = scenario({ prs: largePRs(121), batchIndex });
    await expect(s.run()).rejects.toThrow(/batch index/);
    expect(s.statuses).toHaveLength(0);
    expect(s.getReads()).toBe(0);
  },
);

test("20 days of attacker-controlled 480/481 open PR churn cannot starve stable PR #480", async () => {
  const start = Date.UTC(2026, 10, 7, 2);
  const refreshedOn: number[] = [];
  for (let day = 0; day < 20; day++) {
    const count = day % 2 === 0 ? 480 : 481;
    const prs = Array.from({ length: count }, (_, i) => ({
      number: i + 1,
      state: "open",
      head: { sha: (i + 10000).toString(16).padStart(40, "0") },
      base: { ref: "main", sha: base, repo: { full_name: "gyldlab/keld" } },
      changed_files: 1,
      mergeable: true,
      merge_commit_sha: (i + 1).toString(16).padStart(40, "0"),
    }));
    const s = scenario({ prs, clockMillis: start + day * 86400000 });
    await expect(s.run()).resolves.toBeUndefined();
    expect(s.failures).toHaveLength(0);
    if (
      s.statuses.some(
        (c) => c.details_url === "https://github.com/gyldlab/keld/pull/480",
      )
    )
      refreshedOn.push(day);
  }
  expect(refreshedOn.length).toBeGreaterThanOrEqual(3);
  for (let i = 1; i < refreshedOn.length; i++)
    expect(refreshedOn[i] - refreshedOn[i - 1]).toBeLessThanOrEqual(5);
});

test("cross-batch duplicate parent commits are denied even if other PR list merge SHA is null", async () => {
  const all = largePRs(121);
  // PR #1000 is bucket0; PR #1001 is bucket1 and may show a null merge hint.
  // Git identical HEAD and BASE parent pair could otherwise inherit a same
  // App check across distinct PR identities and bypass the source binding.
  all[1].head.sha = all[0].head.sha;
  all[1].merge_commit_sha = null;
  const s = scenario({ prs: all, batchIndex: "0" });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.failures).toHaveLength(1);
  expect(s.warnings.join(" ")).toContain("same synthetic merge SHA");
  expect(
    s.statuses.some(
      (check) =>
        check.details_url === "https://github.com/gyldlab/keld/pull/1000",
    ),
  ).toBe(false);
  expect(s.statuses.length).toBeGreaterThan(0);
});

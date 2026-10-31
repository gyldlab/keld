import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(
  ".github/workflows/ci-verisentry-maintenance.yml",
  "utf8",
);
const workflow = Bun.YAML.parse(source) as Record<string, any>;
const job = workflow.jobs.decision;
const verifierWorkflow = Bun.YAML.parse(
  readFileSync(".github/workflows/ci-provenance.yml", "utf8"),
) as Record<string, any>;
const verifierCode = verifierWorkflow.jobs.provenance.steps[0].with
  .script as string;
const policyBlock = (code: string) => {
  const start = code.indexOf("const protectedPath = (path) => {");
  const end = code.indexOf("const protectedChanges = [];", start);
  if (start < 0 || end < 0) throw Error("Missing protected path policy block");
  return code.slice(start, end).trim();
};
const script = job.steps[1].with.script as string;
const AsyncFunction = Object.getPrototypeOf(async function () {})
  .constructor as new (...args: string[]) => (...args: any[]) => Promise<void>;
const evaluate = new AsyncFunction(
  "github",
  "context",
  "process",
  "core",
  "setTimeout",
  script,
);
const base = "b".repeat(40);
const head = "a".repeat(40);
const merge = "c".repeat(40);
const prior = (number = 299) =>
  `actor=amishabenramani decision=approve pr=345 base=${base} head=${head} merge=${merge} maintenance-run=${number}`;

function scenario(
  options: {
    decision?: string;
    actor?: string;
    actorId?: string;
    triggeringActor?: string;
    runAttempt?: string;
    runId?: string;
    workflowRef?: string;
    eventName?: string;
    ref?: string;
    number?: string;
    expectedHead?: string;
    expectedBase?: string;
    expectedMerge?: string;
    currentHead?: string;
    currentBase?: string;
    currentMerge?: string | null;
    currentState?: string;
    currentBaseRef?: string;
    currentRepo?: string;
    currentPrAuthor?: string;
    mergeable?: boolean | null;
    reviews?: Record<string, unknown>[];
    reviewsSequence?: Array<Record<string, unknown>[]>;
    baseRefSequence?: string[];
    existing?: Record<string, any>[];
    totalCount?: number;
    restFailure?: boolean;
    writeFailure?: boolean;
  } = {},
) {
  const written: Record<string, any>[] = [];
  const created: Record<string, any>[] = [];
  let reads = 0;
  let reviewReads = 0;
  let checkReads = 0;
  let compareReads = 0;
  const prNumber = 345;
  const initial = {
    id: 42,
    status: "completed",
    app: { id: 5550363 },
    output: { title: "Trusted CI admission rejected", summary: "Fail closed" },
    conclusion: "failure",
  };
  const github = {
    rest: {
      repos: {
        compareCommits: async (args: Record<string, unknown>) => {
          compareReads++;
          expect(args).toEqual({
            owner: "gyldlab",
            repo: "keld",
            base,
            head,
            page: 1,
            per_page: 100,
          });
          if (options.restFailure) throw Error("REST comparison unavailable");
          return {
            data: {
              base_commit: { sha: options.comparisonBase ?? base },
              files: options.files ?? [
                {
                  filename: ".github/workflows/ci-provenance.yml",
                  status: "modified",
                },
              ],
            },
          };
        },
      },
      pulls: {
        get: async (args: Record<string, unknown>) => {
          expect(args).toEqual({
            owner: "gyldlab",
            repo: "keld",
            pull_number: prNumber,
          });
          reads++;
          if (options.restFailure) throw new Error("REST unavailable");
          return {
            data: {
              number: prNumber,
              changed_files: options.changedFiles ?? options.files?.length ?? 1,
              state: options.currentState ?? "open",
              user: { login: options.currentPrAuthor ?? "0monish" },
              head: { sha: options.currentHead ?? head },
              base: {
                ref:
                  options.baseRefSequence?.[reads - 1] ??
                  options.currentBaseRef ??
                  "main",
                sha: options.currentBase ?? base,
                repo: { full_name: options.currentRepo ?? "gyldlab/keld" },
              },
              mergeable:
                options.mergeable === undefined ? true : options.mergeable,
              merge_commit_sha:
                options.currentMerge === undefined
                  ? merge
                  : options.currentMerge,
            },
          };
        },
        listReviews: () => {
          throw new Error("Review pagination must use GitHub paginate");
        },
      },
      checks: {
        listForRef: () => {
          throw Error("App checks must be fully paginated");
        },
        update: async (args: Record<string, any>) => {
          if (options.writeFailure) throw new Error("Check update denied");
          written.push(args);
          return { data: { id: args.check_run_id } };
        },
        create: async (args: Record<string, any>) => {
          if (options.writeFailure) throw new Error("Check create denied");
          created.push(args);
          return { data: { id: 101 } };
        },
      },
    },
    paginate: async (fn: unknown, args: Record<string, unknown>) => {
      if (fn === github.rest.checks.listForRef) {
        checkReads++;
        expect(args).toEqual({
          owner: "gyldlab",
          repo: "keld",
          ref: options.expectedMerge ?? merge,
          check_name: "KELD VeriSentry",
          app_id: 5550363,
          filter: "all",
          per_page: 100,
        });
        if (options.restFailure) throw Error("REST unavailable");
        if (options.totalCount === 1001)
          return Array.from({ length: 1001 }, (_, i) => ({
            id: i + 1,
            app: { id: 5550363 },
          }));
        return (
          options.existing?.map((run) => ({
            status: "completed",
            conclusion: "failure",
            ...run,
          })) ?? [initial]
        );
      }
      if (fn !== github.rest.pulls.listReviews)
        throw Error("Unexpected GitHub paginate source");
      reviewReads++;
      expect(args).toEqual({
        owner: "gyldlab",
        repo: "keld",
        pull_number: prNumber,
        per_page: 100,
      });
      return (
        options.reviewsSequence?.[reviewReads - 1] ??
        options.reviews ?? [
          {
            user: { login: "amishabenramani" },
            state: "APPROVED",
            commit_id: head,
            submitted_at: "2026-10-31T04:00:00Z",
          },
        ]
      );
    },
  };
  const actor = options.actor ?? "0monish";
  const context = {
    actor,
    eventName: options.eventName ?? "workflow_dispatch",
    ref: options.ref ?? "refs/heads/main",
    repo: { owner: "gyldlab", repo: "keld" },
  };
  const process = {
    env: {
      VERISENTRY_PR: options.number ?? "345",
      VERISENTRY_BASE: options.expectedBase ?? base,
      VERISENTRY_HEAD: options.expectedHead ?? head,
      VERISENTRY_MERGE: options.expectedMerge ?? merge,
      VERISENTRY_DECISION: options.decision ?? "approve",
      GITHUB_ACTOR_ID: options.actorId ?? "155816356",
      GITHUB_RUN_ATTEMPT: options.runAttempt ?? "1",
      GITHUB_TRIGGERING_ACTOR: options.triggeringActor ?? actor,
      GITHUB_WORKFLOW_REF:
        options.workflowRef ??
        "gyldlab/keld/.github/workflows/ci-verisentry-maintenance.yml@refs/heads/main",
      GITHUB_RUN_ID: options.runId ?? "300",
    },
  };
  const messages: string[] = [];
  const core = { info: (message: string) => messages.push(message) };
  return {
    run: () =>
      evaluate(github, context, process, core, (resolve: () => void) =>
        resolve(),
      ),
    written,
    created,
    messages,
    getReads: () => reads,
    getReviewReads: () => reviewReads,
    getCheckReads: () => checkReads,
    getCompareReads: () => compareReads,
  };
}

test("trusted main-only manual workflow has one principal-gated App-only writer", () => {
  expect(policyBlock(script)).toEqual(policyBlock(verifierCode));
  expect(workflow.name).toBe("KELD VeriSentry maintenance decision");
  expect(Object.keys(workflow.on)).toEqual(["workflow_dispatch"]);
  expect(Object.keys(workflow.on.workflow_dispatch.inputs)).toEqual([
    "pull_request",
    "expected_base",
    "expected_head",
    "expected_merge",
    "decision",
  ]);
  expect(workflow.on.workflow_dispatch.inputs.decision.type).toBe("choice");
  expect(workflow.on.workflow_dispatch.inputs.decision.options).toEqual([
    "approve",
    "revoke",
  ]);
  expect(job.environment).toBe("verisentry-publisher");
  expect(job.if).toContain("github.ref == 'refs/heads/main'");
  expect(job.if).toContain("github.run_attempt == 1");
  expect(job.if).toContain("github.actor == github.triggering_actor");
  // A PR-owned workflow could cancel an authorized queued revocation via
  // a shared predictable group. No repo-wide concurrency is used here.
  expect(workflow.concurrency).toBeUndefined();
  expect(job.steps).toHaveLength(2);
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
  expect(source).not.toMatch(/actions\/checkout|\n\s+-\s+run:/);
  expect(workflow.permissions).toEqual({
    contents: "read",
    "pull-requests": "read",
  });
});

test("a current independent review and exact receipt may authorize only App-owned check", async () => {
  const s = scenario({
    existing: [
      { id: 42, app: { id: 5550363 }, output: { summary: "Fail closed" } },
    ],
  });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.getReads()).toBe(2);
  expect(s.getReviewReads()).toBe(2);
  expect(s.getCompareReads()).toBe(1);
  expect(s.getCheckReads()).toBe(1);
  expect(s.created).toHaveLength(0);
  expect(s.written).toHaveLength(1);
  expect(s.written[0]).toMatchObject({
    owner: "gyldlab",
    repo: "keld",
    check_run_id: 42,
    status: "completed",
    conclusion: "success",
    details_url: "https://github.com/gyldlab/keld/actions/runs/300",
    output: {
      title: "Trusted CI maintenance exception",
      summary: `actor=0monish decision=approve pr=345 base=${base} head=${head} merge=${merge} maintenance-run=300`,
    },
  });
});

test("manual revocation works after close/head change, without trusting mutable reviews", async () => {
  const s = scenario({
    decision: "revoke",
    currentState: "closed",
    currentHead: "d".repeat(40),
    reviews: [],
    existing: [{ id: 42, app: { id: 5550363 }, output: { summary: prior() } }],
  });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.getReads()).toBe(1);
  expect(s.getReviewReads()).toBe(0);
  expect(s.written[0]).toMatchObject({
    check_run_id: 42,
    conclusion: "failure",
    output: { title: "Trusted CI maintenance revoked" },
  });
});

test.each([
  { actor: "attacker" },
  { actorId: "130687844" },
  { triggeringActor: "attacker" },
  { runAttempt: "2" },
  { ref: "refs/heads/feature" },
  { workflowRef: "gyldlab/keld/.github/workflows/evil.yml@refs/heads/main" },
  { eventName: "pull_request_target" },
  { number: "not-a-number" },
  { number: "000345" },
  { expectedBase: "wrong" },
  { expectedHead: "wrong" },
  { expectedMerge: "wrong" },
  { decision: "unsupported" },
])(
  "unauthorized or malformed dispatch %p never touches PR or App checks",
  async (options) => {
    const s = scenario(options);
    await expect(s.run()).rejects.toThrow("unauthorized or malformed");
    expect(s.getReads()).toBe(0);
    expect(s.written).toHaveLength(0);
    expect(s.created).toHaveLength(0);
  },
);

test.each([
  { currentHead: "d".repeat(40) },
  { currentBase: "e".repeat(40) },
  { currentMerge: "f".repeat(40) },
  { currentState: "closed" },
  { currentBaseRef: "dev" },
  { currentRepo: "attacker/evil" },
  { mergeable: false },
])(
  "stale or invalid authorization target %p fails before App writes",
  async (options) => {
    const s = scenario(options);
    await expect(s.run()).rejects.toThrow();
    expect(s.written).toHaveLength(0);
    expect(s.created).toHaveLength(0);
  },
);

test("approval requires a fresh independent maintainer approval on the exact head", async () => {
  for (const reviews of [
    [],
    [
      {
        user: { login: "attacker" },
        state: "APPROVED",
        commit_id: head,
        submitted_at: "2026-10-31T04:00:00Z",
      },
    ],
    [
      {
        user: { login: "0monish" },
        state: "APPROVED",
        commit_id: head,
        submitted_at: "2026-10-31T04:00:00Z",
      },
    ],
    [
      {
        user: { login: "amishabenramani" },
        state: "DISMISSED",
        commit_id: head,
        submitted_at: "2026-10-31T04:00:00Z",
      },
    ],
    [
      {
        user: { login: "amishabenramani" },
        state: "APPROVED",
        commit_id: "f".repeat(40),
        submitted_at: "2026-10-31T04:00:00Z",
      },
    ],
    [
      {
        user: { login: "amishabenramani" },
        state: "APPROVED",
        commit_id: head,
        submitted_at: "2026-10-31T03:00:00Z",
      },
      {
        user: { login: "amishabenramani" },
        state: "CHANGES_REQUESTED",
        commit_id: head,
        submitted_at: "2026-10-31T04:00:00Z",
      },
    ],
  ]) {
    const s = scenario({ reviews });
    await expect(s.run()).rejects.toThrow(
      "Independent current-head GitHub approval is missing",
    );
    expect(s.written).toHaveLength(0);
  }
});

test("a duplicate PR marker, older decision, or missing revocation cannot mint PASS", async () => {
  const crossPr = scenario({
    existing: [
      {
        id: 42,
        app: { id: 5550363 },
        output: { summary: prior().replace("pr=345", "pr=99") },
      },
    ],
  });
  await expect(crossPr.run()).rejects.toThrow("different maintenance receipt");
  expect(crossPr.written).toHaveLength(0);
  const stale = scenario({
    runId: "298",
    existing: [
      { id: 42, app: { id: 5550363 }, output: { summary: prior(299) } },
    ],
  });
  await expect(stale.run()).rejects.toThrow("Older maintenance decision");
  expect(stale.written).toHaveLength(0);
  const missing = scenario({ decision: "revoke", existing: [] });
  await expect(missing.run()).rejects.toThrow(
    "No matching historical manual grant",
  );
  expect(missing.written).toHaveLength(0);
});

test("an absent trusted App failure can never be overridden by a manual PASS", async () => {
  const s = scenario({ existing: [] });
  await expect(s.run()).rejects.toThrow("No prior trusted App check exists");
  expect(s.written).toHaveLength(0);
  expect(s.created).toHaveLength(0);
});

test.each([
  { changedFiles: 300 },
  { changedFiles: 301 },
  { changedFiles: 0 },
  { changedFiles: 2 },
  { comparisonBase: "f".repeat(40) },
  { files: [{ filename: "docs/benign.md", status: "modified" }] },
  { files: [{ filename: ".github/workflows/x.yml", status: "renamed" }] },
  { files: [{ filename: ".github/workflows/x.yml", status: "surprise" }] },
  { files: [{ filename: ".github\\workflows\\x.yml", status: "added" }] },
])(
  "manual authorization must not bypass immutable file/authority proof %p",
  async (options) => {
    const s = scenario(options);
    await expect(s.run()).rejects.toThrow();
    expect(s.created).toHaveLength(0);
    expect(s.written).toHaveLength(0);
  },
);

test("a foreign-App check inside an App-filtered API response is refused", async () => {
  const s = scenario({ existing: [{ id: 9, app: { id: 15368 } }] });
  await expect(s.run()).rejects.toThrow("genuine VeriSentry checks");
  expect(s.created).toHaveLength(0);
  expect(s.written).toHaveLength(0);
});

test("API failures, missing merge readiness and incomplete check lists never publish a success", async () => {
  for (const options of [
    { restFailure: true },
    { writeFailure: true },
    { currentMerge: null },
    { mergeable: null },
    { totalCount: 1001 },
  ]) {
    const s = scenario(options);
    await expect(s.run()).rejects.toThrow();
    expect(s.created).toHaveLength(0);
    if (options.writeFailure !== true) expect(s.written).toHaveLength(0);
  }
});

test("the workflow operator cannot count their own APPROVED review for a third-party PR", async () => {
  const s = scenario({
    currentPrAuthor: "external-contributor",
    reviews: [
      {
        user: { login: "0monish" },
        state: "APPROVED",
        commit_id: head,
        submitted_at: "2026-10-31T04:00:00Z",
      },
    ],
  });
  await expect(s.run()).rejects.toThrow(
    "Independent current-head GitHub approval is missing",
  );
  expect(s.written).toHaveLength(0);
});

test("review dismissal between immutable comparison and final writer check blocks PASS", async () => {
  const good = [
    {
      user: { login: "amishabenramani" },
      state: "APPROVED",
      commit_id: head,
      submitted_at: "2026-10-31T04:00:00Z",
    },
  ];
  const dismissed = [
    { ...good[0], state: "DISMISSED", submitted_at: "2026-10-31T04:01:00Z" },
  ];
  const s = scenario({ reviewsSequence: [good, dismissed] });
  await expect(s.run()).rejects.toThrow(
    "trusted review was dismissed or changed",
  );
  expect(s.getReviewReads()).toBe(2);
  expect(s.written).toHaveLength(0);
});

test("target retargeted from main between review reads cannot receive App PASS", async () => {
  const s = scenario({ baseRefSequence: ["main", "dev"] });
  await expect(s.run()).rejects.toThrow("Maintenance receipt became stale");
  expect(s.written).toHaveLength(0);
});

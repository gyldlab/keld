import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(
  ".github/workflows/ci-verisentry-maintenance.yml",
  "utf8",
);
const workflow = Bun.YAML.parse(source) as Record<string, any>;
const job = workflow.jobs.decision;
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
  const prNumber = 345;
  const initial = {
    id: 42,
    app: { id: 5550363 },
    output: { title: "Trusted CI admission rejected", summary: "Fail closed" },
    conclusion: "failure",
  };
  const github = {
    rest: {
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
              state: options.currentState ?? "open",
              user: { login: options.currentPrAuthor ?? "0monish" },
              head: { sha: options.currentHead ?? head },
              base: {
                ref: options.currentBaseRef ?? "main",
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
        listForRef: async (args: Record<string, unknown>) => {
          expect(args).toEqual({
            owner: "gyldlab",
            repo: "keld",
            ref: options.expectedMerge ?? merge,
            check_name: "KELD VeriSentry",
            filter: "all",
            per_page: 100,
          });
          if (options.restFailure) throw new Error("REST unavailable");
          return {
            data: {
              total_count: options.totalCount ?? options.existing?.length ?? 1,
              check_runs: options.existing ?? [initial],
            },
          };
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
    paginate: async (_unused: unknown, args: Record<string, unknown>) => {
      reviewReads++;
      expect(args).toEqual({
        owner: "gyldlab",
        repo: "keld",
        pull_number: prNumber,
        per_page: 100,
      });
      return (
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
  };
}

test("trusted main-only manual workflow has one principal-gated App-only writer", () => {
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
  expect(workflow.concurrency["cancel-in-progress"]).toBe(false);
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
      {
        id: 20,
        app: { id: 15368 },
        output: { summary: "Forged check success" },
      },
      { id: 42, app: { id: 5550363 }, output: { summary: "Fail closed" } },
    ],
  });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.getReads()).toBe(2);
  expect(s.getReviewReads()).toBe(1);
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

test("an absent App check can be created by this trusted manual action only", async () => {
  const s = scenario({
    existing: [{ id: 5, app: { id: 15368 }, conclusion: "success" }],
  });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.written).toHaveLength(0);
  expect(s.created).toHaveLength(1);
  expect(s.created[0]).toMatchObject({
    owner: "gyldlab",
    repo: "keld",
    name: "KELD VeriSentry",
    head_sha: merge,
    conclusion: "success",
  });
});

test("API failures, missing merge readiness and incomplete check lists never publish a success", async () => {
  for (const options of [
    { restFailure: true },
    { writeFailure: true },
    { currentMerge: null },
    { mergeable: null },
    { totalCount: 101 },
  ]) {
    const s = scenario(options);
    await expect(s.run()).rejects.toThrow();
    expect(s.created).toHaveLength(0);
    if (options.writeFailure !== true) expect(s.written).toHaveLength(0);
  }
});

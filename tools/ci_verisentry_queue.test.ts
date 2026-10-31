import { expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
const load = (file: string) =>
  Bun.YAML.parse(
    readFileSync(".github/workflows/" + file + ".yml", "utf8"),
  ) as Record<string, any>;
const pipeline = load("ci-verisentry-queue-publisher");
const refresh = load("ci-verisentry-refresh");
const admit = pipeline.jobs.admit,
  publisher = pipeline.jobs.publish;
const code = [
  admit.steps[0].with.script,
  publisher.steps[0].with.script,
  publisher.steps[2].with.script,
] as string[];
const AsyncFunction = Object.getPrototypeOf(async function () {})
  .constructor as new (...args: string[]) => (...args: any[]) => Promise<void>;
const runs = code.map(
  (s) => new AsyncFunction("github", "context", "core", "process", s),
);
const main = "d".repeat(40),
  head = "a".repeat(40),
  stage = "c".repeat(40),
  previewSha = "b".repeat(40),
  treeSha = "e".repeat(40);
const prNumber = 345,
  repoId = 1258884903,
  runId = 41189900001,
  ciWorkflowId = 309460818;
const entryId = "MQE_current",
  nodeId = "PR_current";
const branch = `gh-readonly-queue/main/pr-${prNumber}-${main}`;
const ciPath = ".github/workflows/ci.yml";
const source =
  "gyldlab/keld/.github/workflows/ci-verisentry-queue-publisher.yml@refs/heads/main";
const safe = { filename: "docs/safe.md", status: "modified" };
const danger = { filename: ".github/workflows/ci.yml", status: "modified" };
function fixture() {
  const run = {
    id: runId,
    workflow_id: ciWorkflowId,
    name: "CI",
    event: "merge_group",
    status: "completed",
    conclusion: "success",
    run_attempt: 1,
    repository: { id: repoId },
    head_repository: { id: repoId },
    head_sha: stage,
    head_branch: branch,
    path: ciPath,
    created_at: new Date(Date.now() - 60_000).toISOString(),
    updated_at: new Date(Date.now() - 30_000).toISOString(),
  };
  const entry = {
    id: entryId,
    state: "AWAITING_CHECKS",
    baseCommit: { oid: main },
    headCommit: { oid: stage },
    pullRequest: {
      id: nodeId,
      number: prNumber,
      state: "OPEN",
      isDraft: false,
      headRefOid: head,
      baseRefOid: main,
      baseRefName: "main",
      baseRepository: { nameWithOwner: "gyldlab/keld" },
      mergeQueueEntry: { id: entryId },
    },
  };
  const queue = {
    repository: {
      mergeQueue: {
        configuration: { maximumEntriesToMerge: 1, minimumEntriesToMerge: 1 },
        entries: {
          totalCount: 1,
          pageInfo: { hasNextPage: false },
          nodes: [entry],
        },
      },
    },
  };
  const pr = {
    number: prNumber,
    node_id: nodeId,
    state: "open",
    draft: false,
    changed_files: 1,
    merge_commit_sha: stage,
    head: { sha: head },
    base: { ref: "main", sha: main, repo: { full_name: "gyldlab/keld" } },
  };
  const context = {
    repo: { owner: "gyldlab", repo: "keld" },
    eventName: "workflow_run",
    ref: "refs/heads/main",
    sha: main,
    payload: {
      action: "completed",
      workflow_run: {
        id: runId,
        workflow_id: ciWorkflowId,
        event: "merge_group",
        run_attempt: 1,
        conclusion: "success",
        head_sha: stage,
        head_branch: branch,
      },
    },
  };
  const env = {
    GITHUB_WORKFLOW_REF: source,
    GITHUB_RUN_ATTEMPT: "1",
    GITHUB_SHA: main,
    GITHUB_RUN_ID: "123",
    VERISENTRY_QUEUE_PR: String(prNumber),
    VERISENTRY_QUEUE_BASE: main,
    VERISENTRY_QUEUE_HEAD: head,
    VERISENTRY_QUEUE_STAGE: stage,
    VERISENTRY_QUEUE_ENTRY: entryId,
    VERISENTRY_QUEUE_REF: `heads/${branch}`,
    VERISENTRY_QUEUE_RUN: String(runId),
    VERISENTRY_QUEUE_CI_JOB: "124499000001",
    VERISENTRY_QUEUE_READY: "true",
    VERISENTRY_QUEUE_CI_FINISHED: run.updated_at,
  };
  return {
    run,
    entry,
    queue,
    pr,
    context,
    env,
    ciWorkflow: {
      id: ciWorkflowId,
      name: "CI",
      path: ciPath,
      state: "active",
    },
    job: {
      id: 124499000001,
      name: "CI required",
      workflow_name: "CI",
      status: "completed",
      conclusion: "success",
      head_sha: stage,
      steps: [
        {
          number: 1,
          name: "Set up job",
          status: "completed",
          conclusion: "success",
        },
        {
          number: 2,
          name: "Run actions/checkout",
          status: "completed",
          conclusion: "success",
        },
        {
          number: 3,
          name: "Verify required CI results",
          status: "completed",
          conclusion: "success",
        },
        {
          number: 4,
          name: "Complete job",
          status: "completed",
          conclusion: "success",
        },
      ],
    },
    parents: [{ sha: main }, { sha: head }],
    previewParents: [{ sha: main }, { sha: head }],
    stageTree: treeSha,
    previewTree: treeSha,
    ciMainSha: "f".repeat(40),
    ciStageSha: "f".repeat(40),
    prCommits: [{ sha: head, parents: [{ sha: main }] }],
    stageCommits: [
      { sha: head, parents: [{ sha: main }] },
      { sha: stage, parents: [{ sha: main }, { sha: head }] },
    ],
    // Hosted KELD #1346 head..stage was an empty tree diff (0 files),
    // despite the merge commit itself having two parents.
    commitEdges: { [`${head}:${stage}`]: [] } as Record<string, any[]>,
    prTotalOverride: null as number | null,
    stageTotalOverride: null as number | null,
    prFiles: [safe],
    stagedFiles: [safe],
    required: [
      {
        id: 124499000001,
        name: "CI required",
        head_sha: stage,
        app: { id: 15368 },
        status: "completed",
        conclusion: "success",
      },
      {
        id: 124499000002,
        name: "gatekeeper",
        head_sha: stage,
        app: { id: 15368 },
        status: "completed",
        conclusion: "success",
      },
      {
        id: 124499000003,
        name: "title-lint",
        head_sha: stage,
        app: { id: 15368 },
        status: "completed",
        conclusion: "success",
      },
    ],
    existing: [] as any[],
    checkApp: 5550363,
    mainReads: [] as string[],
    refReads: [] as string[],
    prReads: [] as any[],
    compareStatus: "ahead",
    comparePhantomHead: false,
    compareTruncateCommits: false,
    compareFails: false,
    graphqlFails: false,
  };
}
type Fixture = ReturnType<typeof fixture>;
function scenario(mutate?: (f: Fixture) => void) {
  const f = fixture();
  mutate?.(f);
  const outputs: Record<string, string> = {},
    written: Record<string, any>[] = [];
  let m = 0,
    r = 0,
    p = 0,
    compareCount = 0;
  const github: any = {
    rest: {
      git: {
        getRef: async (a: any) => {
          expect(a.owner).toBe("gyldlab");
          expect(a.repo).toBe("keld");
          if (a.ref === "heads/main")
            return {
              data: {
                ref: "refs/heads/main",
                object: { sha: f.mainReads[m++] ?? main },
              },
            };
          expect(a.ref).toBe(f.env.VERISENTRY_QUEUE_REF);
          return {
            data: {
              ref: "refs/" + a.ref,
              object: { sha: f.refReads[r++] ?? stage },
            },
          };
        },
      },
      actions: {
        getWorkflow: async () => ({ data: f.ciWorkflow }),
        getWorkflowRun: async () => ({ data: f.run }),
        listJobsForWorkflowRun: () => {
          throw Error("not paginated");
        },
      },
      repos: {
        getContent: async (a: any) => {
          expect(a).toMatchObject({
            owner: "gyldlab",
            repo: "keld",
            path: ciPath,
          });
          return {
            data: {
              type: "file",
              path: ciPath,
              sha: a.ref === main ? f.ciMainSha : f.ciStageSha,
              size: 74122,
            },
          };
        },
        getCommit: async (a: any) => ({
          data:
            a.ref === stage
              ? {
                  sha: stage,
                  parents: f.parents,
                  commit: { tree: { sha: f.stageTree } },
                }
              : a.ref === previewSha
                ? {
                    sha: previewSha,
                    parents: f.previewParents,
                    commit: { tree: { sha: f.previewTree } },
                  }
                : (() => {
                    throw Error("Unexpected commit SHA");
                  })(),
        }),
        compareCommits: async (a: any) => {
          compareCount++;
          if (f.compareFails) throw Error("compare API fail");
          expect(a).toMatchObject({
            owner: "gyldlab",
            repo: "keld",
            page: 1,
            per_page: 100,
          });
          let commits: any[], files: any[], total_commits: number;
          if (a.base === main && a.head === head) {
            commits = f.prCommits;
            files = f.prFiles;
            total_commits = f.prTotalOverride ?? commits.length;
          } else if (a.base === main && a.head === stage) {
            commits = f.stageCommits;
            files = f.stagedFiles;
            total_commits = f.stageTotalOverride ?? commits.length;
          } else {
            commits = [{ sha: a.head, parents: [{ sha: a.base }] }];
            files = f.commitEdges[`${a.base}:${a.head}`] ?? [safe];
            total_commits = commits.length;
          }
          return {
            data: {
              base_commit: { sha: a.base },
              files,
              commits: f.compareTruncateCommits ? [] : commits,
              total_commits,
              status: f.compareStatus,
              ahead_by: total_commits,
              behind_by: 0,
              ...(f.comparePhantomHead ? { head_commit: { sha: a.head } } : {}),
            },
          };
        },
      },
      pulls: { get: async () => ({ data: f.prReads[p++] ?? f.pr }) },
      checks: {
        listForRef: () => {
          throw Error("not paginated");
        },
        create: async (a: any) => {
          written.push(a);
          return {
            data: {
              name: "KELD VeriSentry queue",
              head_sha: stage,
              app: { id: f.checkApp },
              conclusion: "success",
            },
          };
        },
      },
    },
    graphql: async () => {
      if (f.graphqlFails) throw Error("queue API fail");
      return f.queue;
    },
    paginate: async (fn: any, a: any) => {
      if (fn === github.rest.actions.listJobsForWorkflowRun) {
        expect(a.run_id).toBe(runId);
        return [f.job];
      }
      if (fn === github.rest.checks.listForRef) {
        expect(a.ref).toBe(stage);
        if (a.app_id === 15368) return f.required;
        expect(a.app_id).toBe(5550363);
        expect(a.check_name).toBe("KELD VeriSentry queue");
        return f.existing.map((check) => ({
          name: "KELD VeriSentry queue",
          ...check,
        }));
      }
      throw Error("unexpected paginator");
    },
  };
  const core = {
    setOutput: (k: string, v: string) => {
      outputs[k] = v;
    },
    info: () => {},
  };
  const process = { env: f.env };
  return {
    f,
    outputs,
    written,
    counts: () => ({ m, r, p, compareCount }),
    admit: () => runs[0](github, f.context, core, process),
    fresh: () => runs[1](github, f.context, core, process),
    publish: () => runs[2](github, f.context, core, process),
  };
}
function policy(s: string) {
  let a = s.indexOf("const protectedPath = (path) => {"),
    b = s.indexOf("// __PROTECTED_PATH_POLICY_END__", a);
  if (a < 0 || b < 0) throw Error("missing policy");
  return s.slice(a, b).trim();
}
test("static workflow boundaries: protected-main workflow_run after existing CI, without marker", () => {
  expect(pipeline.on.workflow_run).toEqual({
    workflows: ["CI"],
    types: ["completed"],
  });
  expect(pipeline.permissions).toMatchObject({
    actions: "read",
    checks: "read",
    contents: "read",
    "pull-requests": "read",
  });
  expect(Object.keys(pipeline.jobs)).toEqual(["admit", "publish"]);
  expect(admit.environment).toBeUndefined();
  expect(publisher.environment).toBe("verisentry-publisher");
  expect(publisher.needs).toBe("admit");
  expect(admit.if).toContain("workflow_id == 309460818");
  expect(publisher.steps[1].uses).toBe(
    "actions/create-github-app-token@64ab729a6294000f5aa4a4bc0b73d1f32bc45510",
  );
  expect(publisher.steps[1].with["app-id"]).toBe("5550363");
  expect(publisher.steps[2].with["github-token"]).toBe(
    "${{ steps.app-token.outputs.token }}",
  );
  expect(JSON.stringify(admit)).not.toContain("VERISENTRY_APP_PRIVATE_KEY");
  expect(JSON.stringify(pipeline)).not.toMatch(
    /actions\/checkout|upload-artifact|download-artifact|ci-verisentry-queue-marker/,
  );
  expect(policy(code[0])).toBe(
    policy(refresh.jobs.refresh.steps[1].with.script),
  );
});
test("positive: full queue attestation publishes real App5550363 on staged commit only", async () => {
  const s = scenario();
  await s.admit();
  expect(s.outputs).toMatchObject({
    pr_number: "345",
    head_sha: head,
    base_sha: main,
    staged_sha: stage,
    queue_ref: `heads/${branch}`,
    entry_id: entryId,
    ci_run_id: String(runId),
    ci_job_id: "124499000001",
  });
  expect(s.counts().compareCount).toBe(3); // base..head, base..stage, head..stage
  await s.fresh();
  expect(s.outputs.ready).toBe("true");
  await s.publish();
  expect(s.written).toHaveLength(1);
  expect(s.written[0]).toMatchObject({
    head_sha: stage,
    name: "KELD VeriSentry queue",
    status: "completed",
    conclusion: "success",
  });
  expect(s.written[0].output.summary).toContain(
    `entry=${entryId} ci-run=${runId}`,
  );
});
test.each([
  [
    "foreign event",
    (f: Fixture) => {
      f.context.eventName = "pull_request";
    },
  ],
  [
    "foreign repo",
    (f: Fixture) => {
      f.context.repo.owner = "evil";
    },
  ],
  [
    "wrong action",
    (f: Fixture) => {
      f.context.payload.action = "requested";
    },
  ],
  [
    "publisher rerun",
    (f: Fixture) => {
      f.env.GITHUB_RUN_ATTEMPT = "2";
    },
  ],
  [
    "non-main source",
    (f: Fixture) => {
      f.env.GITHUB_WORKFLOW_REF = source.replace("main", "branch");
    },
  ],
  [
    "stale source main",
    (f: Fixture) => {
      f.mainReads = [head];
    },
  ],
  [
    "wrong CI workflow identity",
    (f: Fixture) => {
      f.run.workflow_id = 42;
      f.context.payload.workflow_run.workflow_id = 42;
    },
  ],
  [
    "CI run rerun",
    (f: Fixture) => {
      f.run.run_attempt = 2;
      f.context.payload.workflow_run.run_attempt = 2;
    },
  ],
  [
    "CI run failed",
    (f: Fixture) => {
      f.run.conclusion = "failure";
      f.context.payload.workflow_run.conclusion = "failure";
    },
  ],
  [
    "skipped CI required job",
    (f: Fixture) => {
      f.job.conclusion = "skipped";
    },
  ],
  [
    "foreign CI workflow path",
    (f: Fixture) => {
      f.run.path = ".github/workflows/attacker.yml";
    },
  ],
  [
    "fork workflow",
    (f: Fixture) => {
      f.run.head_repository.id = 42;
    },
  ],
  [
    "forged branch",
    (f: Fixture) => {
      f.run.head_branch = "other";
      f.context.payload.workflow_run.head_branch = "other";
    },
  ],
  [
    "stale queue ref",
    (f: Fixture) => {
      f.refReads = [head];
    },
  ],
  [
    "stale parent",
    (f: Fixture) => {
      f.parents[0] = { sha: head };
    },
  ],
  [
    "multiple staged commit parents",
    (f: Fixture) => {
      f.parents.push({ sha: "f".repeat(40) });
    },
  ],
  [
    "queue stage contains a second PR tree",
    (f: Fixture) => {
      f.stageTree = "f".repeat(40);
      f.pr.merge_commit_sha = previewSha;
    },
  ],
  [
    "missing standalone PR test merge",
    (f: Fixture) => {
      f.pr.merge_commit_sha = "";
    },
  ],
  [
    "standalone preview wrong parent",
    (f: Fixture) => {
      f.pr.merge_commit_sha = previewSha;
      f.previewParents[1] = { sha: "f".repeat(40) };
    },
  ],
  [
    "no queue",
    (f: Fixture) => {
      f.queue.repository.mergeQueue = null as any;
    },
  ],
  [
    "multi-PR group enabled",
    (f: Fixture) => {
      f.queue.repository.mergeQueue.configuration.maximumEntriesToMerge = 2;
    },
  ],
  [
    "queue census truncated",
    (f: Fixture) => {
      f.queue.repository.mergeQueue.entries.pageInfo.hasNextPage = true;
    },
  ],
  [
    "duplicate queue SHA",
    (f: Fixture) => {
      f.queue.repository.mergeQueue.entries.nodes.push({
        ...f.entry,
        id: "other",
      });
      f.queue.repository.mergeQueue.entries.totalCount = 2;
    },
  ],
  [
    "stale queue entry",
    (f: Fixture) => {
      f.entry.headCommit.oid = head;
    },
  ],
  [
    "PR detached from entry",
    (f: Fixture) => {
      f.entry.pullRequest.mergeQueueEntry.id = "wrong";
    },
  ],
  [
    "different PR number",
    (f: Fixture) => {
      f.entry.pullRequest.number = 44;
    },
  ],
  [
    "non-main PR",
    (f: Fixture) => {
      f.entry.pullRequest.baseRefName = "release";
    },
  ],
  [
    "draft PR",
    (f: Fixture) => {
      f.pr.draft = true;
    },
  ],
  [
    "moved PR head",
    (f: Fixture) => {
      f.pr.head.sha = stage;
    },
  ],
  [
    "moved PR base",
    (f: Fixture) => {
      f.pr.base.sha = head;
    },
  ],
  [
    "swapped PR identity",
    (f: Fixture) => {
      f.pr.node_id = "PR_impersonator";
    },
  ],
  [
    "300 changed files",
    (f: Fixture) => {
      f.pr.changed_files = 300;
    },
  ],
  [
    "PR count mismatch",
    (f: Fixture) => {
      f.pr.changed_files = 299;
    },
  ],
  [
    "PR altered authority",
    (f: Fixture) => {
      f.prFiles = [danger];
    },
  ],
  [
    "stage altered authority",
    (f: Fixture) => {
      f.stagedFiles = [danger];
    },
  ],
  [
    "renamed protected source",
    (f: Fixture) => {
      f.stagedFiles = [
        {
          filename: "docs/renamed.md",
          previous_filename: ".github/workflows/ci.yml",
          status: "renamed",
        },
      ];
    },
  ],
  [
    "missing rename source",
    (f: Fixture) => {
      f.stagedFiles = [{ filename: "docs/renamed.md", status: "renamed" }];
    },
  ],
  [
    "missing queue API",
    (f: Fixture) => {
      f.graphqlFails = true;
    },
  ],
  [
    "missing Compare Commit API",
    (f: Fixture) => {
      f.compareFails = true;
    },
  ],
] as Array<[string, (f: Fixture) => void]>)(
  "admission fails closed: %s",
  async (_, mutate) => {
    const s = scenario(mutate);
    await expect(s.admit()).rejects.toThrow();
    expect(s.written).toHaveLength(0);
  },
);
test("299 paths verified, 300 staged paths fail closed despite small PR summary", async () => {
  const files = (n: number) =>
    Array.from({ length: n }, (_, i) => ({
      filename: `docs/${i}.md`,
      status: "added",
    }));
  const success = scenario((f) => {
    f.pr.changed_files = 299;
    f.prFiles = files(299);
    f.stagedFiles = files(299);
  });
  await expect(success.admit()).resolves.toBeUndefined();
  const denied = scenario((f) => {
    f.stagedFiles = files(300);
  });
  await expect(denied.admit()).rejects.toThrow(/census incomplete/);
});
test("fresh source and queue ref must remain current after immutable comparisons", async () => {
  const changedMain = scenario((f) => {
    f.mainReads = [main, head];
  });
  await expect(changedMain.admit()).rejects.toThrow(
    /moved during immutable verification/,
  );
  const changedQueue = scenario((f) => {
    f.refReads = [stage, head];
  });
  await expect(changedQueue.admit()).rejects.toThrow(
    /moved during immutable verification/,
  );
  const changedPR = scenario((f) => {
    f.prReads = [f.pr, { ...f.pr, head: { sha: stage } }];
  });
  await expect(changedPR.admit()).rejects.toThrow(
    /moved during immutable verification/,
  );
});
test("second default-token prewrite gate rejects queue removal, main advance and PR change", async () => {
  const movedMain = scenario((f) => {
    f.mainReads = [head];
  });
  await expect(movedMain.fresh()).rejects.toThrow(/source advanced/);
  const queueRemoved = scenario((f) => {
    f.entry.state = "UNMERGEABLE";
  });
  await expect(queueRemoved.fresh()).rejects.toThrow(/binding changed/);
  const changedPR = scenario((f) => {
    f.pr.head.sha = stage;
  });
  await expect(changedPR.fresh()).rejects.toThrow(/PR changed/);
});
test("App check creation fails closed on stale stage, missing gate receipt and moved PR", async () => {
  const staleMain = scenario((f) => {
    f.mainReads = [head];
  });
  await expect(staleMain.publish()).rejects.toThrow(/stale protected-main/);
  const stageGone = scenario((f) => {
    f.refReads = [head];
  });
  await expect(stageGone.publish()).rejects.toThrow(/rebuilt or removed/);
  const missingGate = scenario((f) => {
    f.env.VERISENTRY_QUEUE_READY = "false";
  });
  await expect(missingGate.publish()).rejects.toThrow(
    /unverified protected-main/,
  );
  const movedPR = scenario((f) => {
    f.pr.head.sha = stage;
  });
  await expect(movedPR.publish()).rejects.toThrow(/moved queue PR/);
});
test("same-name Github Actions App15368 cannot masquerade as VeriSentry App5550363", async () => {
  const wrongCreation = scenario((f) => {
    f.checkApp = 15368;
  });
  await expect(wrongCreation.publish()).rejects.toThrow(/wrong source/);
  const forgedExisting = scenario((f) => {
    f.existing = [
      {
        app: { id: 15368 },
        head_sha: stage,
        status: "completed",
        conclusion: "success",
      },
    ];
  });
  await expect(forgedExisting.publish()).rejects.toThrow(
    /foreign or incomplete/,
  );
  expect(forgedExisting.written).toHaveLength(0);
});
test("previous App success must bind identical staged SHA, PR, queue entry and originating run", async () => {
  const proof = `PR #${prNumber} base=${main} head=${head} stage=${stage} entry=${entryId} ci-run=${runId}`;
  const matching = scenario((f) => {
    f.existing = [
      {
        app: { id: 5550363 },
        head_sha: stage,
        status: "completed",
        conclusion: "success",
        completed_at: new Date().toISOString(),
        output: { summary: proof },
      },
    ];
  });
  await expect(matching.publish()).resolves.toBeUndefined();
  expect(matching.written).toHaveLength(0);
  for (const conclusion of ["failure", "neutral", "cancelled"]) {
    const s = scenario((f) => {
      f.existing = [
        {
          app: { id: 5550363 },
          head_sha: stage,
          status: "completed",
          conclusion,
          output: { summary: proof },
        },
      ];
    });
    await expect(s.publish()).rejects.toThrow(/conflicting or revoked/);
  }
  const otherPR = scenario((f) => {
    f.existing = [
      {
        app: { id: 5550363 },
        head_sha: stage,
        status: "completed",
        conclusion: "success",
        output: { summary: proof.replace("PR #345", "PR #999") },
      },
    ];
  });
  await expect(otherPR.publish()).rejects.toThrow(/conflicting or revoked/);
});
test("final queue mutation before App write refuses success", async () => {
  const s = scenario((f) => {
    f.refReads = [stage, head];
  });
  await expect(s.publish()).rejects.toThrow(/final check creation boundary/);
  expect(s.written).toHaveLength(0);
});

test("KELD refuses one-parent fast-forward queue stages even with hidden queue config", async () => {
  const s = scenario((f) => {
    f.parents = [{ sha: main }];
    f.stageCommits = [{ sha: stage, parents: [{ sha: main }] }];
    f.queue.repository.mergeQueue.configuration = null as any;
  });
  await expect(s.admit()).rejects.toThrow(/two-parent merge/);
  await expect(s.fresh()).rejects.toThrow(
    /Queued commit or PR test-merge preview changed/,
  );
  await expect(s.publish()).rejects.toThrow(/mismatched parents/);
  expect(s.written).toHaveLength(0);
});
test("prewrite tree-drift and grouped second PR refuse both source and App gates", async () => {
  const fresh = scenario((f) => {
    f.pr.merge_commit_sha = previewSha;
    f.previewTree = "f".repeat(40);
  });
  await expect(fresh.fresh()).rejects.toThrow(
    /Queued commit or PR test-merge preview changed/,
  );
  const publish = scenario((f) => {
    f.pr.merge_commit_sha = previewSha;
    f.stageTree = "f".repeat(40);
  });
  await expect(publish.publish()).rejects.toThrow(
    /exact staged test-merge SHA/,
  );
  expect(publish.written).toHaveLength(0);
});

test("days-old staged App SUCCESS cannot be reissued from an old CI event", async () => {
  const proof = `PR #${prNumber} base=${main} head=${head} stage=${stage} entry=${entryId} ci-run=${runId}`;
  const old = new Date(Date.now() - 5 * 86400 * 1000).toISOString();
  const s = scenario((f) => {
    f.existing = [
      {
        app: { id: 5550363 },
        head_sha: stage,
        status: "completed",
        conclusion: "success",
        completed_at: old,
        output: { summary: proof },
      },
    ];
  });
  await expect(s.publish()).rejects.toThrow(/too old or malformed/);
  expect(s.written).toHaveLength(0);
  const malformed = scenario((f) => {
    f.existing = [
      {
        app: { id: 5550363 },
        head_sha: stage,
        status: "completed",
        conclusion: "success",
        output: { summary: proof },
      },
    ];
  });
  await expect(malformed.publish()).rejects.toThrow(/too old or malformed/);
  expect(malformed.written).toHaveLength(0);
});

test("queue admission refuses second queued PR even when maxEntriesToMerge remains one", async () => {
  const s = scenario((f) => {
    // Documented GitHub behavior: builds can include PRs ahead in the queue;
    // merge-limit 1 only controls how many PRs land per merge operation.
    f.queue.repository.mergeQueue.entries.nodes.push({
      ...f.entry,
      id: "MQE_preceding",
      headCommit: { oid: "9".repeat(40) },
      pullRequest: {
        ...f.entry.pullRequest,
        id: "PR_preceding",
        number: prNumber - 1,
        mergeQueueEntry: { id: "MQE_preceding" },
      },
    });
    f.queue.repository.mergeQueue.entries.totalCount = 2;
  });
  await expect(s.admit()).rejects.toThrow(
    /Queue configuration or live entry census/,
  );
  await expect(s.fresh()).rejects.toThrow(/Queue census became incomplete/);
  await expect(s.publish()).rejects.toThrow(
    /Queue census is missing or incomplete/,
  );
  expect(s.written).toHaveLength(0);
});

test("candidate-mutated CI source cannot authenticate the privileged App bridge", async () => {
  const s = scenario((f) => {
    f.ciStageSha = "0".repeat(40);
  });
  await expect(s.admit()).rejects.toThrow(/CI workflow source changed/);
  await expect(s.fresh()).rejects.toThrow(/CI source differs/);
  await expect(s.publish()).rejects.toThrow(/candidate-mutated CI source/);
  expect(s.written).toHaveLength(0);
});

test("old, future and delayed CI workflow completions never admit", async () => {
  const old = scenario((f) => {
    f.run.created_at = new Date(Date.now() - 2 * 3600_000).toISOString();
    f.run.updated_at = new Date(
      Date.now() - 2 * 3600_000 + 30_000,
    ).toISOString();
  });
  await expect(old.admit()).rejects.toThrow(/CI run is stale/);
  const late = scenario((f) => {
    f.run.updated_at = new Date(Date.now() - 21 * 60_000).toISOString();
  });
  await expect(late.admit()).rejects.toThrow(/CI run is stale/);
  const future = scenario((f) => {
    f.run.created_at = new Date(Date.now() + 5 * 60_000).toISOString();
    f.run.updated_at = new Date(Date.now() + 6 * 60_000).toISOString();
  });
  await expect(future.admit()).rejects.toThrow(/CI run is stale/);
});

test("protected CI file touched and reverted in PR history cannot hide behind benign net diff", async () => {
  const intermediate = "9".repeat(40);
  const s = scenario((f) => {
    f.prCommits = [
      { sha: intermediate, parents: [{ sha: main }] },
      { sha: head, parents: [{ sha: intermediate }] },
    ];
    f.commitEdges[`${main}:${intermediate}`] = [danger];
    f.commitEdges[`${intermediate}:${head}`] = [danger];
    // Both net diffs remain docs-only. GitHub Compare Commits can hide
    // the original protected path when the next commit restores it.
    f.prFiles = [safe];
    f.stagedFiles = [safe];
  });
  await expect(s.admit()).rejects.toThrow(
    /Protected CI authority touched in commit history/,
  );
  expect(s.written).toHaveLength(0);
});

test("protected rename source touched then reverted also fails immutable commit history proof", async () => {
  const intermediate = "9".repeat(40);
  const s = scenario((f) => {
    f.prCommits = [
      { sha: intermediate, parents: [{ sha: main }] },
      { sha: head, parents: [{ sha: intermediate }] },
    ];
    f.commitEdges[`${main}:${intermediate}`] = [
      {
        filename: "docs/neutral.md",
        previous_filename: ".github/workflows/ci.yml",
        status: "renamed",
      },
    ];
  });
  await expect(s.admit()).rejects.toThrow(
    /Protected CI authority touched in commit history/,
  );
});

test("incomplete history and excessive commit count fail without App checks", async () => {
  const incomplete = scenario((f) => {
    f.prTotalOverride = 3;
  });
  await expect(incomplete.admit()).rejects.toThrow(
    /History commit enumeration is missing/,
  );
  const tooLong = scenario((f) => {
    f.prCommits = Array.from({ length: 65 }, (_, i) => ({
      sha: String(i).padStart(40, "0"),
      parents: [{ sha: main }],
    }));
    f.prCommits[64].sha = head;
  });
  await expect(tooLong.admit()).rejects.toThrow(
    /History commit enumeration is missing/,
  );
  expect(incomplete.written).toHaveLength(0);
  expect(tooLong.written).toHaveLength(0);
});

test("net-safe PR with >=300 distinct paths across history is denied", async () => {
  const intermediate = "9".repeat(40);
  const s = scenario((f) => {
    f.prCommits = [
      { sha: intermediate, parents: [{ sha: main }] },
      { sha: head, parents: [{ sha: intermediate }] },
    ];
    f.commitEdges[`${main}:${intermediate}`] = Array.from(
      { length: 299 },
      (_, i) => ({ filename: `docs/historical-${i}.md`, status: "added" }),
    );
    f.commitEdges[`${intermediate}:${head}`] = [
      { filename: "docs/historical-299.md", status: "added" },
    ];
    f.prFiles = [safe];
  });
  await expect(s.admit()).rejects.toThrow(
    /history touches >=300 distinct files/,
  );
  expect(s.written).toHaveLength(0);
});

test("stacked staged history fails even when the final tree equals standalone preview", async () => {
  const intermediate = "9".repeat(40);
  const s = scenario((f) => {
    f.parents = [{ sha: main }, { sha: head }];
    f.stageCommits = [
      { sha: intermediate, parents: [{ sha: main }] },
      { sha: head, parents: [{ sha: intermediate }] },
      { sha: stage, parents: [{ sha: main }, { sha: head }] },
    ];
    f.commitEdges[`${main}:${intermediate}`] = [danger];
    f.commitEdges[`${intermediate}:${head}`] = [danger];
    f.prCommits = [
      { sha: intermediate, parents: [{ sha: main }] },
      { sha: head, parents: [{ sha: intermediate }] },
    ];
  });
  await expect(s.admit()).rejects.toThrow(
    /Protected CI authority touched in commit history/,
  );
  expect(s.written).toHaveLength(0);
});

test("CI workflow freshness must be revalidated after privileged environment delay", async () => {
  const oldFresh = scenario((f) => {
    f.run.updated_at = new Date(Date.now() - 21 * 60_000).toISOString();
  });
  await expect(oldFresh.fresh()).rejects.toThrow(
    /aged out before privileged token/,
  );
  const delayedPublish = scenario((f) => {
    f.env.VERISENTRY_QUEUE_CI_FINISHED = new Date(
      Date.now() - 21 * 60_000,
    ).toISOString();
  });
  await expect(delayedPublish.publish()).rejects.toThrow(
    /stale or malformed CI completion/,
  );
  expect(delayedPublish.written).toHaveLength(0);
});

test.each([
  [
    "CI required missing",
    (f: Fixture) => {
      f.required = f.required.filter((c) => c.name !== "CI required");
    },
  ],
  [
    "gatekeeper missing",
    (f: Fixture) => {
      f.required = f.required.filter((c) => c.name !== "gatekeeper");
    },
  ],
  [
    "title-lint missing",
    (f: Fixture) => {
      f.required = f.required.filter((c) => c.name !== "title-lint");
    },
  ],
  [
    "CI required failed",
    (f: Fixture) => {
      f.required[0].conclusion = "failure";
    },
  ],
  [
    "gatekeeper cancelled",
    (f: Fixture) => {
      f.required[1].conclusion = "cancelled";
    },
  ],
  [
    "title-lint skipped",
    (f: Fixture) => {
      f.required[2].conclusion = "skipped";
    },
  ],
  [
    "untrusted gatekeeper App",
    (f: Fixture) => {
      f.required[1].app.id = 5550363;
    },
  ],
  [
    "forged same-name Actions check",
    (f: Fixture) => {
      f.required[1].app.id = 42;
    },
  ],
  [
    "legacy required wrong staged commit",
    (f: Fixture) => {
      f.required[2].head_sha = head;
    },
  ],
  [
    "duplicate gatekeeper context",
    (f: Fixture) => {
      f.required.push({ ...f.required[1], id: 912034 });
    },
  ],
  [
    "CI required job ID not same run",
    (f: Fixture) => {
      f.job.id = 12345;
    },
  ],
  [
    "CI required job never succeeded",
    (f: Fixture) => {
      f.job.conclusion = "failure";
    },
  ],
  [
    "required CI source check wrong job ID",
    (f: Fixture) => {
      f.required[0].id = 3;
    },
  ],
  [
    "CI workflow REST path changed",
    (f: Fixture) => {
      f.ciWorkflow.path = ".github/workflows/attacker.yml";
    },
  ],
  [
    "CI workflow REST ID changed",
    (f: Fixture) => {
      f.ciWorkflow.id = 42;
    },
  ],
  [
    "CI workflow run repository ID changed",
    (f: Fixture) => {
      f.run.repository.id = 42;
    },
  ],
  [
    "non-merge_group workflow run",
    (f: Fixture) => {
      f.run.event = "pull_request";
      f.context.payload.workflow_run.event = "pull_request";
    },
  ],
  [
    "CI run first attempt is stale",
    (f: Fixture) => {
      f.run.run_attempt = 2;
      f.context.payload.workflow_run.run_attempt = 2;
    },
  ],
] as Array<[string, (f: Fixture) => void]>)(
  "required Actions source and workflow identity fail closed: %s",
  async (_label, mutate) => {
    const s = scenario(mutate);
    await expect(s.admit()).rejects.toThrow();
    expect(s.written).toHaveLength(0);
  },
);

test("exact required Actions statuses revalidate before App key and App check write", async () => {
  const missingGatekeeper = scenario((f) => {
    f.required[1].conclusion = "failure";
  });
  await expect(missingGatekeeper.fresh()).rejects.toThrow(
    /required status changed/,
  );
  await expect(missingGatekeeper.publish()).rejects.toThrow(
    /legacy required check changed/,
  );
  expect(missingGatekeeper.written).toHaveLength(0);
  const wrongRequiredJob = scenario((f) => {
    f.job.id = 5544;
  });
  await expect(wrongRequiredJob.fresh()).rejects.toThrow(
    /CI required job changed/,
  );
  await expect(wrongRequiredJob.publish()).resolves.toBeUndefined();
  expect(wrongRequiredJob.written).toHaveLength(1);
});

test("CI-only workflow_run bridge cannot consume untrusted queue marker; marker workflow is absent", () => {
  expect(pipeline.on.workflow_run.workflows).toEqual(["CI"]);
  expect(
    readFileSync(".github/workflows/ci-verisentry-queue-publisher.yml", "utf8"),
  ).not.toContain("ci-verisentry-queue-marker.yml");
  expect(existsSync(".github/workflows/ci-verisentry-queue-marker.yml")).toBe(
    false,
  );
});

test.each([
  [
    "missing verification step",
    (f: Fixture) => {
      f.job.steps = f.job.steps.filter(
        (s) => s.name !== "Verify required CI results",
      );
    },
  ],
  [
    "failed verification step",
    (f: Fixture) => {
      f.job.steps[2].conclusion = "failure";
    },
  ],
  [
    "cancelled verification step",
    (f: Fixture) => {
      f.job.steps[2].conclusion = "cancelled";
    },
  ],
  [
    "skipped verification step",
    (f: Fixture) => {
      f.job.steps[2].conclusion = "skipped";
    },
  ],
  [
    "incomplete verification step",
    (f: Fixture) => {
      f.job.steps[2].status = "in_progress";
    },
  ],
  [
    "duplicate verification step",
    (f: Fixture) => {
      f.job.steps.push({ ...f.job.steps[2], number: 99 });
    },
  ],
  [
    "malformed verification step index",
    (f: Fixture) => {
      f.job.steps[2].number = 0;
    },
  ],
  [
    "wrong owning workflow for CI required",
    (f: Fixture) => {
      f.job.workflow_name = "Untrusted CI";
    },
  ],
] as Array<[string, (f: Fixture) => void]>)(
  "exact CI required STEP fails closed: %s",
  async (_label, mutate) => {
    const s = scenario(mutate);
    await expect(s.admit()).rejects.toThrow();
    await expect(s.fresh()).rejects.toThrow();
    expect(s.written).toHaveLength(0);
  },
);

test("replayed App PASS from another queue instance sharing staged SHA cannot be minted again", async () => {
  const proof = `PR #${prNumber} base=${main} head=${head} stage=${stage} entry=${entryId} ci-run=${runId}`;
  const replay = scenario((f) => {
    // Native SHA is unchanged. The queue entry is fresh, while the App
    // check belongs to a previous queue run and/or entry.
    f.existing = [
      {
        app: { id: 5550363 },
        head_sha: stage,
        status: "completed",
        conclusion: "success",
        completed_at: new Date().toISOString(),
        output: {
          summary: proof.replace(`ci-run=${runId}`, `ci-run=${runId - 1}`),
        },
      },
    ];
  });
  await expect(replay.publish()).rejects.toThrow(
    /conflicting or revoked binding/,
  );
  expect(replay.written).toHaveLength(0);
  const differentEntry = scenario((f) => {
    f.existing = [
      {
        app: { id: 5550363 },
        head_sha: stage,
        status: "completed",
        conclusion: "success",
        completed_at: new Date().toISOString(),
        output: {
          summary: proof.replace(`entry=${entryId}`, "entry=MQE_previous"),
        },
      },
    ];
  });
  await expect(differentEntry.publish()).rejects.toThrow(
    /conflicting or revoked binding/,
  );
  expect(differentEntry.written).toHaveLength(0);
});

test("VeriSentry App key never belongs to CI-stage admission or candidate workflow", () => {
  expect(pipeline.jobs.admit.environment).toBeUndefined();
  expect(pipeline.jobs.admit.permissions).toEqual({
    actions: "read",
    checks: "read",
    contents: "read",
    "pull-requests": "read",
  });
  const uses = publisher.steps.map((s: Record<string, any>) => s.uses);
  expect(uses).toHaveLength(3);
  const credentials = publisher.steps.filter((s: Record<string, any>) =>
    JSON.stringify(s).includes("VERISENTRY_APP_PRIVATE_KEY"),
  );
  expect(credentials).toHaveLength(1);
  expect(credentials[0].id).toBe("app-token");
  expect(publisher.environment).toBe("verisentry-publisher");
  expect(JSON.stringify(pipeline.jobs.admit)).not.toContain("secrets.");
  expect(JSON.stringify(publisher.steps[0])).not.toContain("secrets.");
  expect(JSON.stringify(publisher.steps[2])).not.toContain("secrets.");
  expect(publisher.steps[1].with["permission-checks"]).toBe("write");
  expect(publisher.steps[1].with["permission-pull-requests"]).toBe("read");
  expect(publisher.steps[1].with["permission-contents"]).toBe("read");
  expect(publisher.steps[1].with["permission-actions"]).toBeUndefined();
  expect(publisher.steps[1].with.owner).toBe("gyldlab");
  expect(publisher.steps[1].with.repositories).toBe("keld");
  // This code emits a staged check only. There is no App check on PR HEAD.
  expect(code[2]).toContain("head_sha:stage");
  expect(code[2]).not.toMatch(/head_sha\s*:\s*head/);
});

// Captured GitHub REST receipts: KELD disposable merge-queue controls on
// 2026-10-31 used genuine two-parent stage SHA == standalone test-merge SHA.
const hostedKeldStages = [
  {
    pr: 1343,
    base: "9995265d35a88fa4bec3bef1ab0caa0d78d7dfc3",
    head: "95432c063e39b0e8ded65101d01d51312678c7f6",
    stage: "285d126aeb5830737ae15451e01c9de6d6678a6d",
  },
  {
    pr: 1346,
    base: "ca8759d7c1cf5fb08be74aaa26d6e1bc504d44f4",
    head: "7299c0751ceb68f63af42523c6f457be8eab306f",
    stage: "6f491682c48cd7b3884e0b784bd5ffc9002852a8",
  },
];
test.each(hostedKeldStages)(
  "real hosted KELD two-parent queue stage PR #$pr is a supported admission shape",
  async (receipt) => {
    for (const sha of [receipt.base, receipt.head, receipt.stage])
      expect(sha).toMatch(/^[a-f0-9]{40}$/);
    const s = scenario((f) => {
      f.parents = [{ sha: main }, { sha: head }];
      f.stageCommits = [
        { sha: head, parents: [{ sha: main }] },
        { sha: stage, parents: [{ sha: main }, { sha: head }] },
      ];
      f.pr.merge_commit_sha = stage;
    });
    await expect(s.admit()).resolves.toBeUndefined();
    await expect(s.fresh()).resolves.toBeUndefined();
    await expect(s.publish()).resolves.toBeUndefined();
    expect(s.written).toHaveLength(1);
    expect(s.written[0].head_sha).toBe(stage);
  },
);

test("staged two-parent merge must bind exact live main and PR HEAD", async () => {
  const correct = scenario((f) => {
    f.parents = [{ sha: main }, { sha: head }];
    f.stageCommits = [
      { sha: head, parents: [{ sha: main }] },
      { sha: stage, parents: [{ sha: main }, { sha: head }] },
    ];
  });
  await expect(correct.admit()).resolves.toBeUndefined();
  await expect(correct.fresh()).resolves.toBeUndefined();
  await expect(correct.publish()).resolves.toBeUndefined();
  const wrongParent = scenario((f) => {
    f.parents = [{ sha: main }, { sha: "f".repeat(40) }];
  });
  await expect(wrongParent.admit()).rejects.toThrow(/distinct stage/);
  await expect(wrongParent.fresh()).rejects.toThrow(
    /Queued commit or PR test-merge preview changed/,
  );
  await expect(wrongParent.publish()).rejects.toThrow(/mismatched parents/);
});

test("two-parent stage rejects extra stacked-PR commit ancestry, even if final tree matches", async () => {
  const foreign = "f".repeat(40);
  const stacked = scenario((f) => {
    f.parents = [{ sha: main }, { sha: head }];
    f.stageCommits = [
      { sha: head, parents: [{ sha: main }] },
      { sha: foreign, parents: [{ sha: head }] },
      { sha: stage, parents: [{ sha: main }, { sha: head }] },
    ];
  });
  await expect(stacked.admit()).rejects.toThrow(
    /Queue stage contains commits outside this one PR/,
  );
  expect(stacked.written).toHaveLength(0);
});

test("queue-only status context excludes older legacy PR synthetic App PASS on same staged SHA", async () => {
  const s = scenario();
  await s.admit();
  await s.fresh();
  await s.publish();
  expect(s.written).toHaveLength(1);
  expect(s.written[0].name).toBe("KELD VeriSentry queue");
  expect(s.written[0].head_sha).toBe(stage);
  expect(code[2]).toContain("check_name:'KELD VeriSentry queue'");
  expect(code[2]).not.toContain("check_name:'KELD VeriSentry'");
  // Real KELD #1346 preview SHA == staged SHA. Old check name must not
  // satisfy the separately required new queue context.
  expect(hostedKeldStages.find((r) => r.pr === 1346)?.stage).toBe(
    "6f491682c48cd7b3884e0b784bd5ffc9002852a8",
  );
});

test("fabricated Compare Commits head_commit cannot replace missing actual commits", async () => {
  const falseHead = scenario((f) => {
    f.comparePhantomHead = true;
    f.compareTruncateCommits = true;
  });
  await expect(falseHead.admit()).rejects.toThrow(
    /History commit enumeration is missing/,
  );
  expect(falseHead.written).toHaveLength(0);
});

test("unexpected Compare Commits ancestry status fails closed with no App publication", async () => {
  const diverged = scenario((f) => {
    f.compareStatus = "diverged";
  });
  await expect(diverged.admit()).rejects.toThrow(
    /Immutable comparison file census incomplete/,
  );
  expect(diverged.written).toHaveLength(0);
});

test("KELD stage must differ from PR HEAD and equal its current live merge preview SHA", async () => {
  const regenerated = scenario((f) => {
    f.pr.merge_commit_sha = previewSha;
  });
  await expect(regenerated.admit()).rejects.toThrow(/exact PR test-merge SHA/);
  await expect(regenerated.fresh()).rejects.toThrow(
    /Queued commit or PR test-merge preview changed/,
  );
  await expect(regenerated.publish()).rejects.toThrow(
    /exact staged test-merge SHA/,
  );
  const headStage = scenario((f) => {
    f.entry.pullRequest.headRefOid = stage;
    f.pr.head.sha = stage;
  });
  await expect(headStage.admit()).rejects.toThrow(/distinct stage/);
  expect(headStage.written).toHaveLength(0);
  const script = pipeline.jobs.admit.steps[0].with.script as string;
  expect(script).toContain("stage === head");
  expect(code[1]).toContain("expected.stage === expected.head");
  expect(code[2]).toContain("stage === head");
});

import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { checkPullRequestTargetWorkflow } from "./ci_workflow_security";

const source = readFileSync(".github/workflows/ci-provenance.yml", "utf8");
const workflow = Bun.YAML.parse(source) as Record<string, any>;
const verify = workflow.jobs.provenance;
const publish = workflow.jobs.publish;
const action =
  "actions/create-github-app-token@64ab729a6294000f5aa4a4bc0b73d1f32bc45510";
const githubScript =
  "actions/github-script@3a2844b7e9c422d3c10d287c895573f7108da1b3";
const body = publish.steps[1].with.script as string;
const AsyncFunction = Object.getPrototypeOf(async function () {})
  .constructor as new (
  ...args: string[]
) => (...args: unknown[]) => Promise<void>;
const execute = new AsyncFunction(
  "github",
  "context",
  "process",
  "setTimeout",
  "core",
  body,
);
const head = "a".repeat(40);
const base = "b".repeat(40);
const merge = "c".repeat(40);

function scenario(
  options: {
    admission?: string;
    verifiedHead?: string;
    verifiedBase?: string;
    eventHead?: string;
    eventBase?: string;
    currentHead?: string;
    currentBase?: string;
    currentMerge?: string | null;
    mergeSequence?: Array<string | null>;
    currentMergeable?: boolean | null;
    headSequence?: string[];
    currentBaseName?: string;
    currentBaseRepo?: string;
    currentState?: string;
    owner?: string;
    repository?: string;
    prNumber?: number;
    apiReadFails?: boolean;
    checkWriteFails?: boolean;
    strictNative?: boolean;
    verifiedAncestry?: string;
    duplicateHead?: boolean;
    duplicateMerge?: boolean;
    runAttempt?: string;
    workflowRef?: string;
    sourceSha?: string;
    mainSequence?: string[];
    appId?: number;
  } = {},
) {
  const prNumber = options.prNumber ?? 345;
  const eventHead = options.eventHead ?? head;
  const eventBase = options.eventBase ?? base;
  const written: Record<string, any>[] = [];
  let reads = 0;
  let mainReads = 0;
  const github = {
    rest: {
      git: {
        getRef: async () => ({
          data: {
            object: { sha: options.mainSequence?.[mainReads++] ?? eventBase },
          },
        }),
      },
      pulls: {
        list: () => {
          throw Error("PR list must be paginated");
        },
        get: async (args: Record<string, unknown>) => {
          expect(args).toEqual({
            owner: "gyldlab",
            repo: "keld",
            pull_number: prNumber,
          });
          reads += 1;
          if (options.apiReadFails) throw new Error("read unavailable");
          return {
            data: {
              number: prNumber,
              state: options.currentState ?? "open",
              base: {
                ref: options.currentBaseName ?? "main",
                sha: options.currentBase ?? eventBase,
                repo: { full_name: options.currentBaseRepo ?? "gyldlab/keld" },
              },
              head: {
                sha:
                  options.headSequence?.[reads - 1] ??
                  options.currentHead ??
                  eventHead,
              },
              mergeable:
                options.currentMergeable === undefined
                  ? options.mergeSequence?.[reads - 1] === null
                    ? null
                    : true
                  : options.currentMergeable,
              merge_commit_sha: options.mergeSequence
                ? options.mergeSequence[
                    Math.min(reads - 1, options.mergeSequence.length - 1)
                  ]
                : options.currentMerge === undefined
                  ? merge
                  : options.currentMerge,
            },
          };
        },
      },
      checks: {
        create: async (args: Record<string, any>) => {
          if (options.checkWriteFails) throw new Error("write unavailable");
          written.push(args);
          return {
            data: {
              id: 1,
              app: { id: options.appId ?? 5550363 },
              head_sha: args.head_sha,
              conclusion: args.conclusion,
            },
          };
        },
      },
      repos: {
        getRepoRuleset: async () => ({
          data: {
            enforcement: "active",
            rules: [
              {
                type: "required_status_checks",
                parameters: {
                  strict_required_status_checks_policy:
                    options.strictNative ?? false,
                  required_status_checks: [
                    { context: "KELD VeriSentry", integration_id: 5550363 },
                  ],
                },
              },
            ],
          },
        }),
      },
    },
    paginate: async (fn: unknown, args: Record<string, any>) => {
      expect(args).toMatchObject({
        owner: "gyldlab",
        repo: "keld",
        state: "open",
        base: "main",
        per_page: 100,
      });
      if (fn !== github.rest.pulls.list) throw Error("Unexpected pagination");
      const first = {
        number: prNumber,
        head: { sha: eventHead },
        base: { sha: eventBase },
        merge_commit_sha: merge,
      };
      if (options.duplicateHead)
        return [first, { ...first, number: prNumber + 1 }];
      if (options.duplicateMerge)
        return [
          first,
          {
            number: prNumber + 1,
            head: { sha: "f".repeat(40) },
            base: { sha: eventBase },
            merge_commit_sha: merge,
          },
        ];
      return [first];
    },
  };
  const context = {
    repo: {
      owner: options.owner ?? "gyldlab",
      repo: options.repository ?? "keld",
    },
    sha: options.sourceSha ?? eventBase,
    payload: {
      pull_request: {
        number: prNumber,
        head: { sha: eventHead },
        base: { sha: eventBase },
      },
    },
  };
  const processStub = {
    env: {
      VERISENTRY_ADMISSION_RESULT: options.admission ?? "success",
      VERISENTRY_VERIFIED_HEAD: options.verifiedHead ?? head,
      VERISENTRY_VERIFIED_BASE: options.verifiedBase ?? base,
      VERISENTRY_VERIFIED_ANCESTRY: options.verifiedAncestry ?? "true",
      GITHUB_RUN_ATTEMPT: options.runAttempt ?? "1",
      GITHUB_WORKFLOW_REF:
        options.workflowRef ??
        "gyldlab/keld/.github/workflows/ci-provenance.yml@refs/heads/main",
      GITHUB_SHA: options.sourceSha ?? eventBase,
      GITHUB_SERVER_URL: "https://github.com",
      GITHUB_RUN_ID: "4321",
    },
  };
  const core = { info: (_m: string) => undefined };
  return {
    run: () =>
      execute(
        github,
        context,
        processStub,
        (resolve: () => void) => resolve(),
        core,
      ),
    written,
    getReads: () => reads,
    getMainReads: () => mainReads,
  };
}

test("publisher has its own pinned identity and no PR-owned executable code", () => {
  expect(workflow.on).toEqual({
    pull_request_target: {
      types: [
        "opened",
        "reopened",
        "synchronize",
        "edited",
        "ready_for_review",
      ],
    },
  });
  expect(workflow.permissions).toEqual({
    contents: "read",
    "pull-requests": "read",
  });
  expect(Object.keys(workflow.jobs)).toEqual(["provenance", "publish"]);
  expect(verify.environment).toBeUndefined();
  expect(verify.steps).toHaveLength(1);
  expect(verify.steps[0].id).toBe("verify");
  expect(verify.steps[0].uses).toBe(githubScript);
  expect(verify.outputs).toEqual({
    verified_head_sha: "${{ steps.verify.outputs.verified_head_sha }}",
    verified_base_sha: "${{ steps.verify.outputs.verified_base_sha }}",
    verified_base_is_ancestor:
      "${{ steps.verify.outputs.verified_base_is_ancestor }}",
  });
  expect(publish.needs).toBe("provenance");
  expect(publish.if).toBe("${{ always() }}");
  expect(publish.environment).toBe("verisentry-publisher");
  expect(publish.permissions).toEqual({
    contents: "read",
    "pull-requests": "read",
  });
  expect(publish.steps).toHaveLength(2);
  expect(publish.steps[0].uses).toBe(action);
  expect(publish.steps[0].id).toBe("app-token");
  expect(publish.steps[0].with).toEqual({
    "app-id": "5550363",
    "private-key": "${{ secrets.VERISENTRY_APP_PRIVATE_KEY }}",
    owner: "gyldlab",
    repositories: "keld",
    "permission-checks": "write",
    "permission-pull-requests": "read",
    "permission-contents": "read",
  });
  expect(publish.steps[1].uses).toBe(githubScript);
  expect(publish.steps[1].with["github-token"]).toBe(
    "${{ steps.app-token.outputs.token }}",
  );
  expect(publish.steps[1].env).toEqual({
    VERISENTRY_ADMISSION_RESULT: "${{ needs.provenance.result }}",
    VERISENTRY_VERIFIED_HEAD:
      "${{ needs.provenance.outputs.verified_head_sha }}",
    VERISENTRY_VERIFIED_BASE:
      "${{ needs.provenance.outputs.verified_base_sha }}",
    VERISENTRY_VERIFIED_ANCESTRY:
      "${{ needs.provenance.outputs.verified_base_is_ancestor }}",
  });
  expect(
    (source.match(/secrets\.VERISENTRY_APP_PRIVATE_KEY/g) ?? []).length,
  ).toBe(1);
  expect(source).not.toMatch(/actions\/checkout|\n\s+-\s+run:/);
  expect(() =>
    checkPullRequestTargetWorkflow(source, "ci-provenance.yml"),
  ).not.toThrow();
});

test("only a real success with matching immutable head/base emits verified App PASS", async () => {
  const s = scenario();
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.getReads()).toBe(2);
  expect(s.written).toHaveLength(1);
  expect(s.written[0]).toEqual({
    owner: "gyldlab",
    repo: "keld",
    name: "KELD VeriSentry",
    head_sha: merge,
    status: "completed",
    conclusion: "success",
    details_url: "https://github.com/gyldlab/keld/actions/runs/4321",
    output: {
      title: "Trusted CI admission passed",
      summary:
        "PR #345 base=" +
        base +
        " head=" +
        head +
        " merge=" +
        merge +
        " target=test-merge policy=" +
        base +
        " trusted immutable history and protected-path admission success",
    },
  });
});

test("strict native App gate and complete ancestry publish both merge and HEAD", async () => {
  const s = scenario({ strictNative: true });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.written.map((x) => x.head_sha)).toEqual([merge, head]);
  expect(s.written.every((x) => x.conclusion === "success")).toBe(true);
  expect(s.written[1]?.output.summary).toContain("target=head");
});
test.each([
  { strictNative: false },
  { strictNative: true, verifiedAncestry: "false" },
])(
  "a loose native App rule or stale branch ancestry withholds HEAD PASS %p",
  async (inputs) => {
    const s = scenario(inputs);
    await expect(s.run()).resolves.toBeUndefined();
    expect(s.written.map((x) => x.head_sha)).toEqual([merge]);
  },
);
test.each([{ duplicateHead: true }, { duplicateMerge: true }])(
  "cross-PR SHA collisions refuse both signed targets %p",
  async (inputs) => {
    const s = scenario(inputs);
    await expect(s.run()).rejects.toThrow(
      "shared HEAD or synthetic-merge identity",
    );
    expect(s.written).toEqual([]);
  },
);
test("stale workflow source and old rerun cannot mint App PASS", async () => {
  for (const inputs of [
    { runAttempt: "2" },
    { sourceSha: "f".repeat(40) },
    { mainSequence: ["f".repeat(40)] },
    { workflowRef: "gyldlab/keld/.github/workflows/evil.yml@refs/heads/main" },
  ]) {
    const s = scenario(inputs);
    await expect(s.run()).rejects.toThrow(/stale|workflow|source/);
    expect(s.written).toHaveLength(0);
  }
});
test("App identity mismatch is a publication error", async () => {
  const s = scenario({ appId: 15368 });
  await expect(s.run()).rejects.toThrow("unverified App identity");
});

test.each(["failure", "cancelled", "skipped", "neutral", "unknown"])(
  "admission result %s cannot publish a successful required check",
  async (admission) => {
    const s = scenario({ admission });
    await expect(s.run()).rejects.toThrow("blocking failure");
    expect(s.written).toHaveLength(1);
    expect(s.written[0]!.conclusion).toBe("failure");
    expect(s.written[0]!.head_sha).toBe(merge);
  },
);

test.each([
  { verifiedHead: "c".repeat(40) },
  { verifiedBase: "d".repeat(40) },
  { verifiedHead: "" },
  { verifiedBase: "" },
])("forged or missing admission proof %p is never a PASS", async (inputs) => {
  const s = scenario(inputs);
  await expect(s.run()).rejects.toThrow("blocking failure");
  expect(s.written[0]?.conclusion).toBe("failure");
});

test.each([
  { currentHead: "c".repeat(40) },
  { currentBase: "d".repeat(40) },
  { currentState: "closed" },
  { currentBaseName: "dev" },
  { currentBaseRepo: "attacker/repo" },
])(
  "stale or mutated PR %p cannot receive even a stale PASS",
  async (inputs) => {
    const s = scenario(inputs);
    await expect(s.run()).rejects.toThrow("stale or changed PR state");
    expect(s.written).toHaveLength(0);
  },
);

test.each([
  { owner: "attacker" },
  { repository: "evil" },
  { eventHead: "wrong" },
  { eventBase: "wrong" },
  { prNumber: -2 },
])(
  "wrong origin or malformed event %p is refused before read or publish",
  async (inputs) => {
    const s = scenario(inputs);
    await expect(s.run()).rejects.toThrow("untrusted PR event");
    expect(s.getReads()).toBe(0);
    expect(s.written).toHaveLength(0);
  },
);

test("read and App-write failures cannot become successful checks", async () => {
  const read = scenario({ apiReadFails: true });
  await expect(read.run()).rejects.toThrow("read unavailable");
  expect(read.written).toHaveLength(0);
  const write = scenario({ checkWriteFails: true });
  await expect(write.run()).rejects.toThrow("write unavailable");
  expect(write.written).toHaveLength(0);
});

test.each([null, "", "not-a-SHA", "ABCDEF".repeat(6)])(
  "publisher refuses unavailable or malformed test merge commit %p",
  async (currentMerge) => {
    const f = scenario({ currentMerge });
    await expect(f.run()).rejects.toThrow(
      "requires a current test-merge commit",
    );
    expect(f.written).toHaveLength(0);
  },
);

test("a delayed GitHub synthetic merge becomes usable after a bounded re-read", async () => {
  const s = scenario({ mergeSequence: [null, null, merge] });
  await expect(s.run()).resolves.toBeUndefined();
  expect(s.getReads()).toBe(4);
  expect(s.written).toHaveLength(1);
  expect(s.written[0]?.head_sha).toBe(merge);
  expect(s.written[0]?.conclusion).toBe("success");
});

test("reviewed source head changing during merge readiness retry fails closed", async () => {
  const s = scenario({
    mergeSequence: [null, merge],
    headSequence: [head, "d".repeat(40)],
  });
  await expect(s.run()).rejects.toThrow("stale or changed PR state");
  expect(s.written).toHaveLength(0);
});

test("unmergeable or indefinitely pending PR never receives a PASS", async () => {
  const conflict = scenario({ currentMergeable: false });
  await expect(conflict.run()).rejects.toThrow("unmergeable PR");
  expect(conflict.getReads()).toBe(1);
  expect(conflict.written).toHaveLength(0);
  const pending = scenario({ currentMergeable: null });
  await expect(pending.run()).rejects.toThrow(
    "requires a current test-merge commit",
  );
  expect(pending.getReads()).toBe(6);
  expect(pending.written).toHaveLength(0);
});

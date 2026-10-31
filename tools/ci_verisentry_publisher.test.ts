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
const execute = new AsyncFunction("github", "context", "process", body);
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
    currentBaseName?: string;
    currentBaseRepo?: string;
    currentState?: string;
    owner?: string;
    repository?: string;
    prNumber?: number;
    apiReadFails?: boolean;
    checkWriteFails?: boolean;
  } = {},
) {
  const prNumber = options.prNumber ?? 345;
  const eventHead = options.eventHead ?? head;
  const eventBase = options.eventBase ?? base;
  const written: Record<string, any>[] = [];
  let reads = 0;
  const github = {
    rest: {
      pulls: {
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
              head: { sha: options.currentHead ?? eventHead },
              merge_commit_sha:
                options.currentMerge === undefined
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
          return { data: { id: 1 } };
        },
      },
    },
  };
  const context = {
    repo: {
      owner: options.owner ?? "gyldlab",
      repo: options.repository ?? "keld",
    },
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
      GITHUB_SERVER_URL: "https://github.com",
      GITHUB_RUN_ID: "4321",
    },
  };
  return {
    run: () => execute(github, context, processStub),
    written,
    getReads: () => reads,
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
  expect(s.getReads()).toBe(1);
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
        "Protected-base CI authority inspected for the current PR head, base and test merge.",
    },
  });
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

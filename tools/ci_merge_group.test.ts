import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const keldbot = Bun.YAML.parse(
  readFileSync(".github/workflows/keldbot.yml", "utf8"),
) as Record<string, any>;
const ci = Bun.YAML.parse(
  readFileSync(".github/workflows/ci.yml", "utf8"),
) as Record<string, any>;
const base = "b".repeat(40);
const head = "c".repeat(40);
const group = "d".repeat(40);
const previewSha = "f".repeat(40);
const treeSha = "1".repeat(40);
const number = 1340;
const ref = "refs/heads/gh-readonly-queue/main/pr-1340-" + base;
const sections = [
  "Summary",
  "Spec refs",
  "Review gates",
  "Tests",
  "Platforms",
  "Perf impact",
];
const validBody = sections.map((s) => "## " + s + "\nFilled").join("\n\n");
const expression = (key: string) => "$" + "{{ " + key + " }}";
const AsyncFunction = Object.getPrototypeOf(async function () {})
  .constructor as new (
  ...args: string[]
) => (...args: unknown[]) => Promise<void>;

const identity = keldbot.jobs["queue-identity"].steps[0];
const guard = (name: string) => keldbot.jobs[name].steps[0];
const readGroup = (name: string) => keldbot.jobs[name].steps[2];

function fixture(): any {
  const output: Record<string, string> = {};
  const calls = { graphql: 0, pr: 0, ref: 0, commit: 0 };
  const pr = {
    number,
    state: "open",
    draft: false,
    title: "ci(queue): validate group",
    body: validBody,
    base: { ref: "main", sha: base, repo: { full_name: "gyldlab/keld" } },
    head: { sha: head },
    merge_commit_sha: previewSha,
  };
  const entry = {
    state: "AWAITING_CHECKS",
    headCommit: { oid: group },
    pullRequest: {
      number,
      headRefOid: head,
      baseRefName: "main",
      state: "OPEN",
      isDraft: false,
    },
  };
  const queue = {
    configuration: { maximumEntriesToMerge: 1 },
    entries: {
      totalCount: 1,
      pageInfo: { hasNextPage: false },
      nodes: [entry],
    },
  };
  const stageCommit = {
    sha: group,
    parents: [{ sha: base }, { sha: head }],
    commit: { tree: { sha: treeSha } },
  };
  const previewCommit = {
    sha: previewSha,
    parents: [{ sha: base }, { sha: head }],
    commit: { tree: { sha: treeSha } },
  };
  const context = {
    repo: { owner: "gyldlab", repo: "keld" },
    eventName: "merge_group",
    ref,
    sha: group,
    payload: {
      action: "checks_requested",
      merge_group: {
        base_ref: "refs/heads/main",
        base_sha: base,
        head_ref: ref,
        head_sha: group,
      },
    },
  };
  const github = {
    graphql: async (_query: string, args: any) => {
      calls.graphql++;
      expect(args).toEqual({ owner: "gyldlab", repo: "keld" });
      return { repository: { mergeQueue: queue } };
    },
    rest: {
      pulls: {
        get: async (args: any) => {
          calls.pr++;
          expect(args).toEqual({
            owner: "gyldlab",
            repo: "keld",
            pull_number: number,
          });
          return { data: pr };
        },
      },
      git: {
        getRef: async (args: any) => {
          calls.ref++;
          expect(args).toEqual({
            owner: "gyldlab",
            repo: "keld",
            ref: ref.replace(/^refs\//, ""),
          });
          return { data: { object: { sha: group } } };
        },
      },
      repos: {
        getCommit: async (args: any) => {
          calls.commit++;
          expect(args.owner).toBe("gyldlab");
          expect(args.repo).toBe("keld");
          if (args.ref === group) return { data: stageCommit };
          if (args.ref === previewSha) return { data: previewCommit };
          throw Error("Unexpected GitHub commit ref " + String(args.ref));
        },
      },
    },
  };
  return {
    queue,
    entry,
    pr,
    stageCommit,
    previewCommit,
    context,
    github,
    output,
    calls,
    core: {
      setOutput: (key: string, value: string) => {
        output[key] = value;
      },
      info: (_value: string) => {},
    },
  };
}

function invoke(
  step: Record<string, any>,
  f: any,
  env: Record<string, string> = {},
) {
  const exec = new AsyncFunction(
    "github",
    "context",
    "core",
    "process",
    step.with.script as string,
  );
  return exec(f.github, f.context, f.core, { env });
}

test("merge_group produces the three existing GitHub required checks on its own commit", () => {
  for (const workflow of [ci, keldbot]) {
    expect(workflow.on.merge_group).toEqual({
      types: ["checks_requested"],
      branches: ["main"],
    });
  }
  expect(ci.jobs.required.name).toBe("CI required");
  expect(ci.jobs.required.if).toBe(expression("always()"));
  expect(ci.jobs.required.needs).toContain("changes");
  expect(ci.jobs.required.needs).toContain("router-contracts");
  expect(keldbot.permissions).toEqual({});
  expect(keldbot.jobs["queue-identity"].permissions).toEqual({
    contents: "read",
    "pull-requests": "read",
  });
  for (const name of ["gatekeeper", "title-lint"]) {
    const job = keldbot.jobs[name];
    expect(job.needs).toBe("queue-identity");
    expect(job.if).toBe(expression("always()"));
    expect(job.permissions).toEqual({
      contents: "read",
      "pull-requests": "read",
    });
    expect(job.steps).toHaveLength(3);
    expect(job.steps[0].name).toBe("Require authenticated queue identity");
    expect(job.steps[1].if).toBe("github.event_name == 'pull_request_target'");
    expect(job.steps[2].if).toBe("github.event_name == 'merge_group'");
    expect(job.steps[1].with["github-token"]).toBe(
      expression("secrets.ROBOKELD_TOKEN"),
    );
    expect(job.steps[2].with["github-token"]).toBe(expression("github.token"));
    expect(job.steps[2].with.script).not.toMatch(
      /issues\.(create|update|add|remove)/,
    );
  }
  expect(keldbot.jobs["size-label"].if).toContain(
    "github.event_name == 'pull_request_target'",
  );
  for (const job of Object.values(keldbot.jobs) as any[]) {
    for (const step of job.steps ?? []) {
      expect(step).not.toHaveProperty("run");
      expect(step.uses).not.toMatch(/actions\/checkout/);
    }
  }
});

test("live authenticated singleton queue resolves exact PR and group SHA", async () => {
  const f = fixture();
  await expect(invoke(identity, f)).resolves.toBeUndefined();
  expect(f.output).toEqual({ number: String(number), head_sha: head });
  expect(f.calls).toEqual({ graphql: 1, pr: 1, ref: 1, commit: 2 });
});

test("live queue configuration:null accepts distinct staged SHA and PR head", async () => {
  const f = fixture();
  // Dify live GraphQL can return configuration:null. The entry HEAD is
  // staged commit SHA, while pullRequest.headRefOid is the original PR head.
  f.queue.configuration = null;
  expect(f.entry.headCommit.oid).toBe(group);
  expect(f.entry.pullRequest.headRefOid).toBe(head);
  expect(f.pr.head.sha).toBe(head);
  expect(group).not.toBe(head);
  await expect(invoke(identity, f)).resolves.toBeUndefined();
  expect(f.output).toEqual({ number: String(number), head_sha: head });
  expect(f.calls).toEqual({ graphql: 1, pr: 1, ref: 1, commit: 2 });
});

test("existing pull_request_target path still yields PR metadata without queue reads", async () => {
  const f = fixture();
  f.context.eventName = "pull_request_target";
  f.context.payload = {
    action: "synchronize",
    pull_request: { number, head: { sha: head } },
  };
  await expect(invoke(identity, f)).resolves.toBeUndefined();
  expect(f.output).toEqual({ number: String(number), head_sha: head });
  expect(f.calls).toEqual({ graphql: 0, pr: 0, ref: 0, commit: 0 });
});

test.each([
  [
    "wrong repository",
    (f: any) => {
      f.context.repo.owner = "attacker";
    },
  ],
  [
    "wrong action",
    (f: any) => {
      f.context.payload.action = "other";
    },
  ],
  [
    "wrong target",
    (f: any) => {
      f.context.payload.merge_group.base_ref = "refs/heads/dev";
    },
  ],
  [
    "wrong GitHub ref",
    (f: any) => {
      f.context.ref = "refs/heads/main";
    },
  ],
  [
    "wrong PR in ref",
    (f: any) => {
      f.context.payload.merge_group.head_ref =
        "refs/heads/gh-readonly-queue/main/pr-999-" + base;
    },
  ],
  [
    "wrong event SHA",
    (f: any) => {
      f.context.sha = "e".repeat(40);
    },
  ],
  [
    "malformed group SHA",
    (f: any) => {
      f.context.payload.merge_group.head_sha = "invalid";
    },
  ],
  [
    "unrestricted group size",
    (f: any) => {
      f.queue.configuration.maximumEntriesToMerge = 2;
    },
  ],
  [
    "malformed present queue configuration",
    (f: any) => {
      f.queue.configuration = {};
    },
  ],
  [
    "invalid present queue limit",
    (f: any) => {
      f.queue.configuration.maximumEntriesToMerge = "1";
    },
  ],
  [
    "missing entry",
    (f: any) => {
      f.queue.entries.nodes = [];
    },
  ],
  [
    "incomplete queue pagination",
    (f: any) => {
      f.queue.entries.pageInfo.hasNextPage = true;
    },
  ],
  [
    "duplicate PR entries",
    (f: any) => {
      f.queue.entries.nodes.push(structuredClone(f.entry));
      f.queue.entries.totalCount = 2;
    },
  ],
  [
    "wrong queue PR",
    (f: any) => {
      f.entry.pullRequest.number = 1341;
    },
  ],
  [
    "stale staged group SHA",
    (f: any) => {
      f.entry.headCommit.oid = "e".repeat(40);
    },
  ],
  [
    "mismatched queued PR head identity",
    (f: any) => {
      f.entry.pullRequest.headRefOid = "e".repeat(40);
    },
  ],
  [
    "changed PR base SHA",
    (f: any) => {
      f.pr.base.sha = "e".repeat(40);
    },
  ],
  [
    "missing standalone preview",
    (f: any) => {
      f.pr.merge_commit_sha = null;
    },
  ],
  [
    "stale preview commit identity",
    (f: any) => {
      f.previewCommit.sha = "e".repeat(40);
    },
  ],
  [
    "wrong preview base parent",
    (f: any) => {
      f.previewCommit.parents[0].sha = "e".repeat(40);
    },
  ],
  [
    "wrong preview head parent",
    (f: any) => {
      f.previewCommit.parents[1].sha = "e".repeat(40);
    },
  ],
  [
    "second queued PR changes staged tree",
    (f: any) => {
      // A group build containing another PR has a different tree even
      // when the queue configuration says maximumEntriesToMerge=1.
      f.stageCommit.commit.tree.sha = "e".repeat(40);
    },
  ],
  [
    "preview tree missing",
    (f: any) => {
      f.previewCommit.commit.tree.sha = null;
    },
  ],
  [
    "changed PR head",
    (f: any) => {
      f.pr.head.sha = "e".repeat(40);
    },
  ],
  [
    "retargeted PR",
    (f: any) => {
      f.pr.base.ref = "dev";
    },
  ],
  [
    "closed PR",
    (f: any) => {
      f.pr.state = "closed";
    },
  ],
  [
    "draft PR",
    (f: any) => {
      f.pr.draft = true;
    },
  ],
  [
    "unmergeable entry",
    (f: any) => {
      f.entry.state = "UNMERGEABLE";
    },
  ],
  [
    "changed queue branch ref",
    (f: any) => {
      f.github.rest.git.getRef = async () => ({
        data: { object: { sha: "e".repeat(40) } },
      });
    },
  ],
  [
    "invalid queue base parent",
    (f: any) => {
      f.github.rest.repos.getCommit = async () => ({
        data: { sha: group, parents: [{ sha: "e".repeat(40) }] },
      });
    },
  ],
  [
    "GitHub read outage",
    (f: any) => {
      f.github.graphql = async () => {
        throw Error("GitHub 503");
      };
    },
  ],
] as const)(
  "merge_group %s refuses required metadata success",
  async (_label, mutate) => {
    const f = fixture();
    mutate(f);
    await expect(invoke(identity, f)).rejects.toThrow();
    expect(f.output).toEqual({});
  },
);

test("required jobs fail after a missing or failed queue identity", async () => {
  for (const name of ["gatekeeper", "title-lint"]) {
    const f = fixture();
    await expect(
      invoke(guard(name), f, {
        KELD_QUEUE_RESULT: "failure",
        KELD_QUEUE_NUMBER: String(number),
        KELD_QUEUE_HEAD: head,
      }),
    ).rejects.toThrow("missing queue identity");
    await expect(
      invoke(guard(name), f, {
        KELD_QUEUE_RESULT: "success",
        KELD_QUEUE_NUMBER: String(number),
        KELD_QUEUE_HEAD: head,
      }),
    ).resolves.toBeUndefined();
    await expect(
      invoke(guard(name), f, {
        KELD_QUEUE_RESULT: "success",
        KELD_QUEUE_NUMBER: "",
        KELD_QUEUE_HEAD: head,
      }),
    ).rejects.toThrow();
  }
});

test("read-only group gatekeeper and title lint validate fresh PR metadata", async () => {
  const f = fixture();
  await expect(invoke(identity, f)).resolves.toBeUndefined();
  const env = { KELD_QUEUE_NUMBER: String(number), KELD_QUEUE_HEAD: head };
  for (const name of ["gatekeeper", "title-lint"]) {
    await expect(invoke(readGroup(name), f, env)).resolves.toBeUndefined();
  }
  f.pr.body = "## Summary\n<!-- placeholder -->";
  await expect(invoke(readGroup("gatekeeper"), f, env)).rejects.toThrow(
    "Missing queued PR template",
  );
  f.pr.body = validBody;
  f.pr.title = "unconventional title";
  await expect(invoke(readGroup("title-lint"), f, env)).rejects.toThrow(
    "not conventional",
  );
  f.pr.draft = true;
  await expect(invoke(readGroup("title-lint"), f, env)).rejects.toThrow(
    "changed PR",
  );
});

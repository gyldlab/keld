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
  body,
);
const base = "b".repeat(40);
const head = "a".repeat(40);
const merge = "c".repeat(40);

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
  const context = {
    repo: { owner: options.owner ?? "gyldlab", repo: options.repo ?? "keld" },
    eventName: options.eventName ?? "schedule",
    actor: options.actor ?? "github-actions[bot]",
    ref: options.ref ?? "refs/heads/main",
  };
  const github = {
    rest: {
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
          expect(args).toEqual({
            owner: "gyldlab",
            repo: "keld",
            base,
            head,
            page: 1,
            per_page: 100,
          });
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
          ref: merge,
          check_name: "KELD VeriSentry",
          app_id: 5550363,
          filter: "all",
          per_page: 100,
        });
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
  const process = { env: { GITHUB_ACTOR_ID: options.actorId ?? "41898282" } };
  return {
    run: () =>
      execute(github, context, core, process, (done: () => void) => done()),
    statuses,
    messages,
    warnings,
    failures,
    getReads: () => reads,
    getCompareReads: () => compareReads,
    getCheckReads: () => checkReads,
  };
}

test("refresh is a protected-main-only cron and authorized manual dispatch, with no PR code checkout", () => {
  expect(refresh.name).toBe("KELD VeriSentry trusted refresh");
  expect(Object.keys(refresh.on)).toEqual(["schedule", "workflow_dispatch"]);
  expect(refresh.on.schedule).toEqual([{ cron: "17 2 * * *" }]);
  expect(job.if).toContain("github.ref == 'refs/heads/main'");
  expect(job.if).toContain("github.event_name == 'schedule'");
  expect(job.environment).toBe("verisentry-publisher");
  expect(job.permissions).toEqual({
    contents: "read",
    "pull-requests": "read",
  });
  expect(refresh.permissions).toEqual({
    contents: "read",
    "pull-requests": "read",
  });
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
  { repo: "forked" },
  { owner: "attacker" },
])("untrusted refresh event %p never gets to Github PR API", async (o) => {
  const s = scenario(o);
  await expect(s.run()).rejects.toThrow("refuses non-main/non-maintainer");
  expect(s.getReads()).toBe(0);
  expect(s.statuses).toHaveLength(0);
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

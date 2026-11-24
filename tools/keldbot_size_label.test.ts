import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const workflow = Bun.YAML.parse(
  readFileSync(
    new URL("../.github/workflows/keldbot.yml", import.meta.url),
    "utf8",
  ),
) as { jobs: Record<string, { steps: { with: { script: string } }[] }> };
const script = workflow.jobs["size-label"].steps[0].with.script;
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;

async function run(
  additions: number,
  deletions: number,
  labels: string[] = [],
  removeStatus?: number,
) {
  const added: string[] = [];
  const removed: string[] = [];
  await new AsyncFunction("github", "context", "core", script)(
    {
      rest: {
        pulls: {
          get: async () => ({
            data: {
              additions,
              deletions,
              labels: labels.map((name) => ({ name })),
            },
          }),
        },
        issues: {
          addLabels: async ({ labels }: { labels: string[] }) =>
            added.push(...labels),
          removeLabel: async ({ name }: { name: string }) => {
            if (removeStatus) throw { status: removeStatus };
            removed.push(name);
          },
        },
      },
    },
    {
      repo: { owner: "gyldlab", repo: "keld" },
      payload: { pull_request: { number: 1 } },
    },
    { info: () => {} },
  );
  return { added, removed };
}

// Literal contract edges, independent of the production tier array.
const bands: [string, number, number][] = [
  ["XS", 0, 19],
  ["S", 20, 99],
  ["M", 100, 399],
  ["L", 400, 999],
  ["XL", 1000, 1999],
  ["2XL", 2000, 3999],
  ["3XL", 4000, 7999],
  ["4XL", 8000, 15999],
  ["5XL", 16000, 31999],
  ["6XL", 32000, 63999],
  ["7XL", 64000, 127999],
  ["8XL", 128000, 255999],
  ["9XL", 256000, 511999],
  ["10XL", 512000, Number.MAX_SAFE_INTEGER],
];
for (const [tier, min, max] of bands) {
  test(`size/${tier} includes both exact boundaries`, async () => {
    for (const value of [min, max]) {
      expect((await run(value, 0)).added).toEqual([`size/${tier}`]);
      expect((await run(0, value)).added).toEqual([`size/${tier}`]);
    }
  });
}
test("sum additions and deletions, replace all stale tiers, retain unrelated labels", async () => {
  const labels = bands.map(([tier]) => `size/${tier}`);
  const result = await run(8000, 8000, [...labels, "gate:security"]);
  expect(result.added).toEqual([]);
  expect(result.removed).toEqual(
    labels.filter((label) => label !== "size/5XL"),
  );
});
test("downgrade from the highest tier and add only the new tier", async () => {
  expect(await run(1, 0, ["size/10XL", "gate:security"])).toEqual({
    added: ["size/XS"],
    removed: ["size/10XL"],
  });
});

test("ignore only a missing stale label, not permission failures", async () => {
  expect(await run(1, 0, ["size/10XL"], 404)).toEqual({
    added: ["size/XS"],
    removed: [],
  });
  await expect(run(1, 0, ["size/10XL"], 403)).rejects.toEqual({ status: 403 });
});

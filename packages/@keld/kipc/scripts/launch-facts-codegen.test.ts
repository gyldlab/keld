import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  assertLaunchFactsRegionFresh,
  LAUNCH_FACTS_REGION_BEGIN,
  LAUNCH_FACTS_REGION_END,
  replaceLaunchFactsRegion,
} from "./echo-codegen.ts";

const owner = readFileSync(
  join(import.meta.dir, "../src/launch-facts.ts"),
  "utf8",
);
const transport = readFileSync(
  join(import.meta.dir, "../src/transport.ts"),
  "utf8",
);

test("the standalone transport embeds the exact one launch-facts codec owner", () => {
  expect(() => assertLaunchFactsRegionFresh(owner, transport)).not.toThrow();
  expect(() =>
    assertLaunchFactsRegionFresh(owner + "\n// owner changed\n", transport),
  ).toThrow("stale");
});

test("launch-facts generation refuses missing duplicate and inverted boundaries", () => {
  for (const invalid of [
    "",
    LAUNCH_FACTS_REGION_BEGIN,
    `${LAUNCH_FACTS_REGION_BEGIN}\n${LAUNCH_FACTS_REGION_BEGIN}\n${LAUNCH_FACTS_REGION_END}`,
    `${LAUNCH_FACTS_REGION_END}\n${LAUNCH_FACTS_REGION_BEGIN}`,
  ])
    expect(() => replaceLaunchFactsRegion(invalid, owner)).toThrow(
      "exactly one ordered",
    );
});

test("refreshing the launch-facts region preserves the independent channel-table bytes", () => {
  const changed = owner + "\n// independent owner-change control\n";
  const next = replaceLaunchFactsRegion(transport, changed);
  const channelRegion = (text: string) =>
    text.slice(
      text.indexOf("// @generated-begin channel-table"),
      text.indexOf("// @generated-end channel-table") +
        "// @generated-end channel-table".length,
    );
  expect(channelRegion(next)).toBe(channelRegion(transport));
  expect(() => assertLaunchFactsRegionFresh(changed, next)).not.toThrow();
});

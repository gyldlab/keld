/** Capture/immutability tests only: injected fixture data never proves native qualification. */
import { expect, test } from "bun:test";
const transport = new URL("./transport.ts", import.meta.url).href;
const descriptor = {
  contract: "gh473-original-editor-offline/v1",
  graphSha256: `sha256:${"1".repeat(64)}`,
  policySha256: `sha256:${"2".repeat(64)}`,
  windowKey: "editor",
  selector: "original-editor-offline",
};
const base = {
  schema: 5,
  kind: "development",
  name: "capture-test",
  root: "/selected",
  paths: {},
};
function capture(value: unknown) {
  return Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      `--define=__KELD_HOST_LAUNCH_FACTS__:${JSON.stringify(value)}`,
      "-e",
      `import {hostLaunchFacts} from ${JSON.stringify(transport)};
       Object.prototype.originalResourcePolicy = {contract:'forged-after-capture'};
       console.log(JSON.stringify({facts:hostLaunchFacts,frozen:Object.isFrozen(hostLaunchFacts),nullPrototype:Object.getPrototypeOf(hostLaunchFacts)===null,policyFrozen:hostLaunchFacts.originalResourcePolicy===undefined?null:Object.isFrozen(hostLaunchFacts.originalResourcePolicy),policyNullPrototype:hostLaunchFacts.originalResourcePolicy===undefined?null:Object.getPrototypeOf(hostLaunchFacts.originalResourcePolicy)===null}));`,
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
}
test("schema5 captures a closed immutable descriptor without claiming native proof", () => {
  const value = { ...base, originalResourcePolicy: descriptor };
  const run = capture(value);
  expect(run.exitCode).toBe(0);
  expect(run.stderr.toString()).toBe("");
  expect(JSON.parse(run.stdout.toString())).toEqual({
    facts: value,
    frozen: true,
    nullPrototype: true,
    policyFrozen: true,
    policyNullPrototype: true,
  });
});
test("older metadata schemas preserve absent eligibility through prototype poisoning", () => {
  for (const schema of [1, 2, 3, 4]) {
    const value = {
      schema,
      kind: "development",
      name: "old",
      root: "/selected",
      ...(schema === 1 ? {} : { paths: {} }),
    };
    const run = capture(value);
    expect(run.exitCode).toBe(0);
    expect(run.stderr.toString()).toBe("");
    const observed = JSON.parse(run.stdout.toString());
    expect(observed.facts).toEqual(value);
    expect(observed.policyFrozen).toBeNull();
  }
});
test("schema5 missing or malformed descriptor and forged fields fail typed capture", () => {
  const values: unknown[] = [
    base,
    { ...base, originalResourcePolicy: null },
    { ...base, originalResourcePolicy: [] },
    { ...base, originalResourcePolicy: true },
  ];
  for (const [key, value] of [
    ["contract", "forged"],
    ["selector", "default"],
    ["graphSha256", "sha256:0"],
    ["policySha256", `sha256:${"A".repeat(64)}`],
    ["windowKey", ""],
    ["grant", "all"],
  ] as const)
    values.push({
      ...base,
      originalResourcePolicy: { ...descriptor, [key]: value },
    });
  for (const value of values) {
    const run = capture(value);
    expect(run.exitCode).not.toBe(0);
    expect(run.stdout.toString()).toBe("");
    expect(run.stderr.toString()).toContain("KELD-IPC-029");
  }
});
test("old schema cannot smuggle a syntactically valid source descriptor", () => {
  for (const schema of [1, 2, 3, 4]) {
    const run = capture({
      ...base,
      schema,
      originalResourcePolicy: descriptor,
    });
    expect(run.exitCode).not.toBe(0);
    expect(run.stderr.toString()).toContain("KELD-IPC-029");
  }
});

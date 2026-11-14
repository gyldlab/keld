import { expect, test } from "bun:test";
import { resolve } from "node:path";

const owner = resolve(import.meta.dir, "launch-facts.ts");
const transport = resolve(import.meta.dir, "transport.ts");
const facts = (root: string, name = 'Quoted "app" λ') => ({
  schema: 1,
  kind: "development",
  name,
  root,
});

// Each child uses the actual canonical codec and its generated standalone
// consumer. This is a codec/parse-time carrier proof, not a Host lease receipt.
function observe(
  value: unknown,
  target: "windows" | "other" | "absent" = "windows",
) {
  return Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      ...(target === "absent"
        ? []
        : ["--define=__KELD_WINDOWS_HOST__:" + (target === "windows")]),
      "--define=__KELD_HOST_LAUNCH_FACTS__:" + JSON.stringify(value),
      "--preload=" + transport,
      "-e",
      "import { hostLaunchFacts as owner } from " +
        JSON.stringify(owner) +
        ";" +
        "import { hostLaunchFacts as embedded } from " +
        JSON.stringify(transport) +
        ";" +
        "console.log(JSON.stringify({ owner, embedded," +
        " frozen: Object.isFrozen(owner) && Object.isFrozen(embedded) &&" +
        " (!owner.paths || (Object.isFrozen(owner.paths) && Object.isFrozen(embedded.paths)))," +
        " nullPrototype: Object.getPrototypeOf(owner) === null && Object.getPrototypeOf(embedded) === null }));",
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
}

test("Windows schema-one preserves exact canonical local-drive bytes in both codec consumers", () => {
  for (const root of [
    "\\\\?\\D:\\",
    "\\\\?\\d:\\a b\\λ",
    "\\\\?\\D:\\stage-λ",
  ]) {
    const input = facts(root);
    const child = observe(input);
    expect(child.exitCode).toBe(0);
    expect(child.stderr.toString()).toBe("");
    expect(JSON.parse(child.stdout.toString())).toEqual({
      owner: input,
      embedded: input,
      frozen: true,
      nullPrototype: true,
    });
  }
});

test("Windows root grammar refuses alternate spellings without normalization", () => {
  for (const root of [
    "/stage",
    "D:\\stage",
    "\\\\server\\share\\stage",
    "\\\\?\\UNC\\server\\share\\stage",
    "\\\\.\\D:\\stage",
    "\\\\?\\D:stage",
    "\\\\?\\D:\\a\\..\\b",
    "\\\\?\\D:\\a\\.\\b",
    "\\\\?\\D:\\a/b",
    "\\\\?\\D:\\a\\\\b",
    "\\\\?\\D:\\a\\",
    "\\\\?\\1:\\stage",
    "\\\\?\\D:\\bad\u001f",
    ...[":", "*", "?", '"', "<", ">", "|"].map(
      (ch) => "\\\\?\\D:\\a" + ch + "b",
    ),
    "relative",
  ]) {
    const child = observe(facts(root));
    expect(child.exitCode).not.toBe(0);
    expect(child.stdout.toString()).toBe("");
    expect(child.stderr.toString()).toContain("KELD-IPC-029");
  }
});

test("only the pinned Windows literal selects bounded development facts", () => {
  for (const target of ["other", "absent"] as const) {
    const refused = observe(facts("\\\\?\\D:\\stage"), target);
    expect(refused.exitCode).not.toBe(0);
    expect(refused.stderr.toString()).toContain("KELD-IPC-029");
    const existing = facts("/unchanged-mac-stage");
    const accepted = observe(existing, target);
    expect(accepted.exitCode).toBe(0);
    expect(JSON.parse(accepted.stdout.toString()).owner).toEqual(existing);
  }
  for (const schema of [4, 5, 6, 7]) {
    const child = observe({
      ...facts("\\\\?\\D:\\stage"),
      schema,
      version: "1",
      paths: {},
    });
    expect(child.exitCode).not.toBe(0);
    expect(child.stderr.toString()).toContain("KELD-IPC-029");
  }
});

test("Windows fields and escaped whole record keep their independent byte bounds", () => {
  for (const name of ["x".repeat(4096), "λ".repeat(2048)]) {
    const child = observe(facts("\\\\?\\D:\\stage", name));
    expect(child.exitCode).toBe(0);
    expect(JSON.parse(child.stdout.toString()).owner.name).toBe(name);
  }
  for (const input of [
    facts("\\\\?\\D:\\stage", ""),
    facts("\\\\?\\D:\\stage", "x".repeat(4097)),
    facts("\\\\?\\D:\\stage", "λ".repeat(2049)),
    facts("\\\\?\\D:\\stage", "nul\0name"),
    facts("\\\\?\\D:\\stage", "\u0001".repeat(4096)),
    facts("\\\\?\\D:\\" + "x".repeat(4090)),
    facts("\\\\?\\D:\\stage\0suffix"),
    { ...facts("\\\\?\\D:\\stage"), grant: "all" },
  ]) {
    const child = observe(input);
    expect(child.exitCode).not.toBe(0);
    expect(child.stdout.toString()).toBe("");
    expect(child.stderr.toString()).toContain("KELD-IPC-029");
  }
});

test("Windows schema-three preserves explicit version, first locale and actual native folder spellings", () => {
  for (const directory of [
    "D:\\Users\\Exact\\Roaming",
    "\\\\?\\d:\\Users\\λ\\Roaming",
    "\\\\server\\share\\Exact",
    "\\\\?\\UNC\\server\\share\\Exact",
  ]) {
    const input = {
      ...facts("\\\\?\\D:\\stage"),
      schema: 3,
      version: "31.7.0-selected-input",
      locale: "qps-ploc",
      paths: {
        appData: directory,
        userData: directory + "\\Exact",
        documents: directory + "\\Documents",
      },
    };
    const child = observe(input);
    expect(child.exitCode).toBe(0);
    expect(child.stderr.toString()).toBe("");
    expect(JSON.parse(child.stdout.toString())).toEqual({
      owner: input,
      embedded: input,
      frozen: true,
      nullPrototype: true,
    });
  }
  const absent = { ...facts("\\\\?\\D:\\stage"), schema: 3, paths: {} };
  const child = observe(absent);
  expect(child.exitCode).toBe(0);
  expect(JSON.parse(child.stdout.toString()).owner).toEqual(absent);
});

test("Windows schema-three refuses malformed optional facts and unsupported path namespaces", () => {
  const base = { ...facts("\\\\?\\D:\\stage"), schema: 3, paths: {} };
  for (const input of [
    { ...base, version: "" },
    { ...base, version: null },
    { ...base, version: "λ".repeat(2049) },
    { ...base, version: "nul\0value" },
    { ...base, locale: "" },
    { ...base, locale: null },
    { ...base, locale: "x".repeat(4097) },
    { ...base, locale: "nul\0value" },
    { ...base, paths: null },
    { ...base, paths: { other: "D:\\Users" } },
    ...[
      "/foreign",
      "D:relative",
      "\\relative",
      "\\\\server",
      "\\\\.\\device",
      "\\\\?\\GLOBALROOT\\Device\\HarddiskVolume1",
      "D:\\a\\..\\b",
      "D:\\a\\\\b",
      "D:\\a/b",
      "D:\\a\0b",
      "D:\\" + "x".repeat(4094),
    ].map((path) => ({ ...base, paths: { documents: path } })),
  ]) {
    const child = observe(input);
    expect(child.exitCode).not.toBe(0);
    expect(child.stdout.toString()).toBe("");
    expect(child.stderr.toString()).toContain("KELD-IPC-029");
  }
});

test("Windows schema-three joins retain prior closed variants and the total record bound", () => {
  for (const schema of [2, 3]) {
    const accepted = {
      ...facts("\\\\?\\D:\\stage"),
      schema,
      paths: {},
      version: "explicit",
    };
    expect(observe(accepted).exitCode).toBe(0);
  }
  for (const input of [
    { ...facts("\\\\?\\D:\\stage"), schema: 2, paths: {}, locale: "en-US" },
    { ...facts("\\\\?\\D:\\stage"), schema: 3, version: "explicit" },
    {
      ...facts("\\\\?\\D:\\stage", "x".repeat(4096)),
      schema: 3,
      version: "x".repeat(4096),
      locale: "x".repeat(4096),
      paths: { documents: "D:\\" + "x".repeat(4093) },
    },
  ]) {
    const child = observe(input);
    expect(child.exitCode).not.toBe(0);
    expect(child.stderr.toString()).toContain("KELD-IPC-029");
  }
  const foreign = {
    ...facts("/unchanged-mac-stage"),
    schema: 3,
    paths: { documents: "/Docs" },
    locale: "fr-CA",
    version: "explicit",
  };
  for (const target of ["other", "absent"] as const) {
    const child = observe(foreign, target);
    expect(child.exitCode).toBe(0);
    expect(JSON.parse(child.stdout.toString()).owner).toEqual(foreign);
    const windowsPath = observe(
      { ...foreign, paths: { documents: "D:\\Docs" } },
      target,
    );
    expect(windowsPath.exitCode).not.toBe(0);
  }
});

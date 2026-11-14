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
        " (!owner.paths || (Object.isFrozen(owner.paths) && Object.isFrozen(embedded.paths))) &&" +
        " (!owner.originalResourcePolicy || (Object.isFrozen(owner.originalResourcePolicy) && Object.isFrozen(embedded.originalResourcePolicy)))," +
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

function originalFacts(selector = "original-editor-offline-empty-style") {
  return {
    ...facts("\\\\?\\D:\\stage"),
    schema: 8,
    version: "31.7.0-selected",
    locale: "fr-CA",
    paths: {
      appData: "D:\\Observed\\Roaming",
      userData: "D:\\Observed\\Roaming\\Exact",
      documents: "\\\\server\\share\\Documents",
    },
    originalResourcePolicy: {
      contract: "gh473-original-editor-offline/v1",
      graphSha256: "sha256:" + "1".repeat(64),
      policySha256: "sha256:" + "2".repeat(64),
      windowKey: "editor",
      selector,
    },
  };
}

test("Windows original schema8 preserves the exact descriptor and captured metadata in both consumers", () => {
  for (const selector of [
    "original-editor-offline",
    "original-editor-offline-empty-style",
  ]) {
    const input = originalFacts(selector);
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
  const input = originalFacts();
  const { version: _version, locale: _locale, ...absent } = input;
  const optional = { ...absent, paths: {} };
  const child = observe(optional);
  expect(child.exitCode).toBe(0);
  expect(JSON.parse(child.stdout.toString()).owner).toEqual(optional);
});

test("original schema8 is Windows development only and cannot reinterpret old variants", () => {
  const input = originalFacts();
  for (const target of ["other", "absent"] as const) {
    for (const root of [input.root, "/posix-shaped-decoy"]) {
      // A POSIX root with empty paths is otherwise valid on the foreign target.
      // This case must depend on the explicit Windows-only schema8 guard.
      const child = observe(
        { ...input, root, paths: root.startsWith("/") ? {} : input.paths },
        target,
      );
      expect(child.exitCode).not.toBe(0);
      expect(child.stderr.toString()).toContain("KELD-IPC-029");
    }
  }
  for (const value of [
    { ...input, kind: "local-development" },
    { ...input, resourcesRoot: "\\\\?\\D:\\" },
    { ...input, singleInstance: { verdict: "acquired" } },
    { ...input, root: "D:\\stage" },
    { ...input, schema: 3 },
    ...[4, 5, 6, 7].map((schema) => ({ ...input, schema })),
  ]) {
    const child = observe(value);
    expect(child.exitCode).not.toBe(0);
    expect(child.stderr.toString()).toContain("KELD-IPC-029");
  }
});

test("original schema8 refuses missing, malformed and foreign policy descriptors", () => {
  const input = originalFacts();
  const { originalResourcePolicy: policy, ...missing } = input;
  for (const value of [
    missing,
    { ...input, paths: undefined },
    { ...input, originalResourcePolicy: null },
    { ...input, originalResourcePolicy: {} },
    { ...input, originalResourcePolicy: { ...policy, extra: "grant" } },
    ...[
      { contract: "unqualified" },
      { windowKey: "" },
      { windowKey: "nul\0key" },
      { graphSha256: "sha256:" + "A".repeat(64) },
      { policySha256: "sha256:" + "2".repeat(63) },
      { selector: "unsafe-inline" },
    ].map((change) => ({
      ...input,
      originalResourcePolicy: { ...policy, ...change },
    })),
  ]) {
    const child = observe(value);
    expect(child.exitCode).not.toBe(0);
    expect(child.stdout.toString()).toBe("");
    expect(child.stderr.toString()).toContain("KELD-IPC-029");
  }
});

test("schema8 descriptor fields and escaped record retain the existing byte bounds", () => {
  const input = originalFacts();
  for (const value of [
    {
      ...input,
      originalResourcePolicy: {
        ...input.originalResourcePolicy,
        windowKey: "x".repeat(4097),
      },
    },
    { ...input, name: "\u0001".repeat(4096) },
    {
      ...input,
      name: "x".repeat(4096),
      version: "x".repeat(4096),
      locale: "x".repeat(4096),
      paths: { documents: "D:\\" + "x".repeat(3797) },
    },
  ]) {
    const child = observe(value);
    expect(child.exitCode).not.toBe(0);
    expect(child.stderr.toString()).toContain("KELD-IPC-029");
  }
  const accepted = {
    ...input,
    originalResourcePolicy: {
      ...input.originalResourcePolicy,
      windowKey: "λ".repeat(2048),
    },
  };
  expect(observe(accepted).exitCode).toBe(0);
});

test("schema8 rejects getters, proxies, symbols and prototype-only fields without observing them", () => {
  const script = [
    'import assert from "node:assert/strict";',
    "import { captureHostLaunchFacts as a, hasOriginalResourcePolicy as pa } from " +
      JSON.stringify(owner) +
      ";",
    "import { captureHostLaunchFacts as b, hasOriginalResourcePolicy as pb } from " +
      JSON.stringify(transport) +
      ";",
    "const base = " + JSON.stringify(originalFacts()) + ";",
    "let observed = 0;",
    'const accessor = () => { observed++; throw Error("getter ran"); };',
    "const proxy = (value) => new Proxy(value, { ownKeys() { observed++; return []; } });",
    "const cases = [",
    "proxy(base), { ...base, paths: proxy(base.paths) },",
    "{ ...base, originalResourcePolicy: proxy(base.originalResourcePolicy) },",
    'Object.defineProperty({ ...base }, "originalResourcePolicy", { get: accessor }),',
    '{ ...base, originalResourcePolicy: Object.defineProperty({ ...base.originalResourcePolicy }, "selector", { get: accessor }) },',
    '{ ...base, paths: Object.defineProperty({}, "documents", { get: accessor }) },',
    "{ ...base, originalResourcePolicy: Object.create(base.originalResourcePolicy) },",
    'Object.create(base), { ...base, [Symbol("unknown")]: true },',
    "];",
    "for (const decode of [a,b]) {",
    'for (const value of cases) assert.throws(() => decode(value), { code:"KELD-IPC-029" });',
    "const result = decode(base);",
    "assert.equal(Object.getPrototypeOf(result.paths), null);",
    "assert.equal(Object.getPrototypeOf(result.originalResourcePolicy), null);",
    "assert(Object.isFrozen(result.originalResourcePolicy));",
    "}",
    "for (const [decode,predicate] of [[a,pa],[b,pb]]) {",
    "assert.equal(predicate(decode(base)), true);",
    "assert.equal(predicate(undefined), false);",
    "const { originalResourcePolicy, ...metadata } = base;",
    "assert.equal(predicate(decode({ ...metadata, schema:3 })), false);",
    "}",
    "assert.equal(observed, 0);",
    'console.log("PASS closed schema8 owner and embedded");',
  ].join("\n");
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      "--define=__KELD_WINDOWS_HOST__:true",
      "-e",
      script,
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
  expect(child.exitCode, child.stderr.toString()).toBe(0);
  expect(child.stderr.toString()).toBe("");
  expect(child.stdout.toString()).toBe(
    "PASS closed schema8 owner and embedded\n",
  );
});

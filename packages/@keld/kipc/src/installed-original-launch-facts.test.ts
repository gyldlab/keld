/** Schema 7 codec conformance only; native admission/lock ownership is tested by Core. */
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { captureHostLaunchFacts } from "./launch-facts.ts";

const fixtureText = readFileSync(
  join(import.meta.dir, "../test/fixtures/installed-launch-facts-v7.json"),
  "utf8",
);

function original(): Record<string, unknown> {
  return JSON.parse(fixtureText);
}

function nested(record: Record<string, unknown>, key: string) {
  return record[key] as Record<string, unknown>;
}

function captureOriginal(value: unknown) {
  const captured = captureHostLaunchFacts(value);
  if (captured?.schema !== 7)
    throw new Error("expected installed-original schema 7");
  return captured;
}

test("installed schema7 captures the shared native-facts fixture with frozen own records", () => {
  const source = original();
  const captured = captureOriginal(source);
  expect(captured).toEqual(JSON.parse(fixtureText));
  for (const value of [
    captured,
    Reflect.get(captured, "paths"),
    Reflect.get(captured, "singleInstance"),
    Reflect.get(captured, "originalResourcePolicy"),
  ]) {
    expect(Object.isFrozen(value)).toBe(true);
    expect(Object.getPrototypeOf(value)).toBeNull();
  }
  nested(source, "paths").userData = "/forged";
  nested(source, "singleInstance").verdict = "contended";
  nested(source, "originalResourcePolicy").windowKey = "forged";
  expect(captured).toEqual(JSON.parse(fixtureText));
});

test("schema7 absence stays absent, and acquired and contended are distinct data", () => {
  const source = original();
  delete source.locale;
  delete source.singleInstance;
  source.paths = {};
  const captured = captureOriginal(source);
  expect<unknown>(captured).toEqual(source);
  expect(Object.hasOwn(captured, "locale")).toBe(false);
  expect(Object.hasOwn(captured, "singleInstance")).toBe(false);
  expect(Object.keys(Reflect.get(captured, "paths"))).toEqual([]);
  source.singleInstance = { verdict: "acquired" };
  expect(Reflect.get(captured, "singleInstance")).toBeUndefined();
  for (const verdict of ["acquired", "contended"] as const) {
    const value = captureHostLaunchFacts({
      ...original(),
      singleInstance: { verdict },
    })!;
    expect(Reflect.get(value, "singleInstance")).toEqual({ verdict });
  }
});

test("only installed schema7 carries the explicit empty-style catalog choice", () => {
  const source = original();
  nested(source, "originalResourcePolicy").selector =
    "original-editor-offline-empty-style";
  const captured = captureOriginal(source);
  expect(captured.originalResourcePolicy.selector).toBe(
    "original-editor-offline-empty-style",
  );
  expect(Object.isFrozen(captured.originalResourcePolicy)).toBe(true);
  const development = {
    schema: 5,
    kind: "development",
    name: "Paper Map",
    root: "/stage",
    version: "1.0.0",
    paths: {},
    originalResourcePolicy: nested(source, "originalResourcePolicy"),
  };
  expect(() => captureHostLaunchFacts(development)).toThrow("KELD-IPC-029");
  for (const selector of [
    "original-editor-offline-empty-style-extra",
    "unsafe-inline",
    "",
  ]) {
    const changed = original();
    nested(changed, "originalResourcePolicy").selector = selector;
    expect(() => captureHostLaunchFacts(changed)).toThrow("KELD-IPC-029");
  }
});

test("schema7 refuses missing, null, mixed, unknown and descriptor fields", () => {
  for (const key of Object.keys(original())) {
    const source = original();
    if (key !== "locale" && key !== "singleInstance") {
      delete source[key];
      expect(() => captureHostLaunchFacts(source)).toThrow("KELD-IPC-029");
    }
    expect(() =>
      captureHostLaunchFacts({ ...original(), [key]: null }),
    ).toThrow("KELD-IPC-029");
  }
  for (const source of [
    { ...original(), schema: 6 },
    { ...original(), schema: 5 },
    { ...original(), kind: "development" },
    { ...original(), grant: "all" },
    { ...original(), singleInstance: { verdict: "acquired", id: "org.paper" } },
    { ...original(), singleInstance: { verdict: "ACQUIRED" } },
    { ...original(), paths: { downloads: "/Users/native/Downloads" } },
    { ...original(), originalResourcePolicy: {} },
    {
      ...original(),
      originalResourcePolicy: {
        ...nested(original(), "originalResourcePolicy"),
        grant: "all",
      },
    },
  ]) {
    expect(() => captureHostLaunchFacts(source)).toThrow("KELD-IPC-029");
  }
  for (const [key, value] of [
    ["contract", "unqualified"],
    ["selector", "default"],
    ["windowKey", ""],
    ["graphSha256", `sha256:${"A".repeat(64)}`],
    ["policySha256", `sha256:${"0".repeat(63)}`],
  ]) {
    const source = original();
    nested(source, "originalResourcePolicy")[key!] = value;
    expect(() => captureHostLaunchFacts(source)).toThrow("KELD-IPC-029");
  }
});

test("schema7 requires canonical absolute roots and native path/name relations", () => {
  const paths = nested(original(), "paths");
  for (const path of [
    "relative",
    "/a/../b",
    "/a/./b",
    "/a//b",
    "/a/",
    "/a/..",
    "/a/.",
  ]) {
    for (const key of ["appData", "userData", "documents"]) {
      expect(() =>
        captureHostLaunchFacts({
          ...original(),
          paths: { ...paths, [key]: path },
        }),
      ).toThrow("KELD-IPC-029");
    }
    expect(() =>
      captureHostLaunchFacts({
        ...original(),
        resourcesRoot: path,
        root: `${path}/app`,
      }),
    ).toThrow("KELD-IPC-029");
  }
  for (const name of [".", "..", "foreign/name", "/absolute"]) {
    expect(() =>
      captureHostLaunchFacts({
        ...original(),
        name,
        // Keep the declared lock's namespace valid. This control must fail for
        // the unsafe name, not the independent verdict/appData requirement.
        paths: { appData: "/Users/native" },
      }),
    ).toThrow("KELD-IPC-029");
  }
  for (const supplied of [
    { userData: "/Users/native/Paper Map" },
    { appData: "/Users/native", userData: "/foreign/Paper Map" },
  ]) {
    expect(() =>
      captureHostLaunchFacts({ ...original(), paths: supplied }),
    ).toThrow("KELD-IPC-029");
  }
  const rootPaths = { appData: "/", userData: "/Paper Map", documents: "/" };
  expect(
    Reflect.get(
      captureHostLaunchFacts({ ...original(), paths: rootPaths })!,
      "paths",
    ),
  ).toEqual(rootPaths);
});

test("schema7 preserves the native owner's raw first preferred language", () => {
  for (const locale of ["en_US", "EN-gb", "zh-Hant-TW", "é".repeat(2048)]) {
    expect(captureOriginal({ ...original(), locale }).locale).toBe(locale);
  }
});

test("schema7 lock verdict requires its captured namespace but a namespace grants no lock", () => {
  for (const verdict of ["acquired", "contended"]) {
    expect(() =>
      captureOriginal({
        ...original(),
        paths: {},
        singleInstance: { verdict },
      }),
    ).toThrow("KELD-IPC-029");
  }
  const source = original();
  delete source.singleInstance;
  expect<unknown>(captureOriginal(source).paths.appData).toBe(
    nested(source, "paths").appData,
  );
  expect(captureOriginal(source).singleInstance).toBeUndefined();
});

test("schema7 refuses empty NUL nonstring and oversized native locale data", () => {
  for (const locale of [
    "",
    "en\0GB",
    "e".repeat(4097),
    "é".repeat(2049),
    null,
    7,
  ]) {
    expect(() => captureHostLaunchFacts({ ...original(), locale })).toThrow(
      "KELD-IPC-029",
    );
  }
  for (const locale of ["en", "en-GB", "zh-Hant-TW", "e".repeat(4096)]) {
    expect(
      Reflect.get(captureHostLaunchFacts({ ...original(), locale })!, "locale"),
    ).toBe(locale);
  }
});

test("schema7 rejects nested accessors/proxies/inherited required fields without running user code", () => {
  let reads = 0;
  for (const key of ["paths", "singleInstance", "originalResourcePolicy"]) {
    const source = original();
    const initial = nested(source, key);
    const field = Object.keys(initial)[0]!;
    for (const replacement of [
      Object.defineProperty({ ...initial }, field, {
        get() {
          reads++;
          return initial[field];
        },
      }),
      new Proxy(initial, {
        ownKeys() {
          reads++;
          return [];
        },
      }),
    ]) {
      expect(() =>
        captureHostLaunchFacts({ ...source, [key]: replacement }),
      ).toThrow("KELD-IPC-029");
    }
  }
  for (const key of ["singleInstance", "originalResourcePolicy"]) {
    expect(() =>
      captureHostLaunchFacts({
        ...original(),
        [key]: Object.create(nested(original(), key)),
      }),
    ).toThrow("KELD-IPC-029");
  }
  expect(reads).toBe(0);
});

test("capture never invokes an inherited serializer while enforcing the record size limit", () => {
  const codec = join(import.meta.dir, "launch-facts.ts");
  const child = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      "-e",
      `import { captureHostLaunchFacts } from ${JSON.stringify(codec)};
       const input = ${fixtureText};
       let calls = 0;
       Object.defineProperty(Object.prototype, "toJSON", {
         configurable: true,
         get() { calls++; return () => ({}); }
       });
       let captured;
       try { captured = captureHostLaunchFacts(input); }
       finally { delete Object.prototype.toJSON; }
       console.log(JSON.stringify({calls, captured}));`,
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
  expect(child.exitCode).toBe(0);
  expect(child.stderr.toString()).toBe("");
  expect(JSON.parse(child.stdout.toString())).toEqual({
    calls: 0,
    captured: JSON.parse(fixtureText),
  });
});

test("schema7 size limits apply to UTF-8 strings and the complete record", () => {
  expect(() =>
    captureHostLaunchFacts({ ...original(), version: "é".repeat(2048) }),
  ).not.toThrow();
  expect(() =>
    captureHostLaunchFacts({ ...original(), version: "é".repeat(2049) }),
  ).toThrow("KELD-IPC-029");
  const maximum = original();
  maximum.version = "v".repeat(4096);
  maximum.resourcesRoot = "/" + "r".repeat(4080);
  maximum.root = maximum.resourcesRoot + "/app";
  maximum.paths = { appData: "/" + "a".repeat(3000) };
  nested(maximum, "originalResourcePolicy").windowKey = "";
  const remaining =
    16384 - new TextEncoder().encode(JSON.stringify(maximum)).byteLength;
  expect(remaining).toBeGreaterThan(0);
  expect(remaining).toBeLessThanOrEqual(4096);
  nested(maximum, "originalResourcePolicy").windowKey = "w".repeat(remaining);
  expect(new TextEncoder().encode(JSON.stringify(maximum)).byteLength).toBe(
    16384,
  );
  expect(() => captureHostLaunchFacts(maximum)).not.toThrow();
  nested(maximum, "originalResourcePolicy").windowKey += "w";
  expect(() => captureHostLaunchFacts(maximum)).toThrow("KELD-IPC-029");
});

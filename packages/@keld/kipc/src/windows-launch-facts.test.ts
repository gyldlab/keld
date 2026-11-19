import { expect, test } from "bun:test";
import { resolve } from "node:path";
import { captureHostLaunchFacts } from "./transport.ts";

const transport = resolve(import.meta.dir, "transport.ts");
const facts = {
  schema: 8,
  kind: "development",
  name: "Selected Windows app",
  root: "\\\\?\\D:\\retained-stage",
  version: "31.7.0",
  locale: "fr-CA",
  paths: {
    appData: "D:\\Observed\\Roaming",
    userData: "D:\\Observed\\Roaming\\Selected Windows app",
    documents: "\\\\server\\share\\Documents",
  },
  originalResourcePolicy: {
    contract: "gh473-original-editor-offline/v1",
    graphSha256: "sha256:" + "ab".repeat(32),
    policySha256: "sha256:" + "cd".repeat(32),
    windowKey: "editor",
    selector: "original-editor-offline-empty-style",
  },
  singleInstance: { verdict: "acquired" },
};

function observe(
  record: unknown,
  windows = true,
  runtime = true,
  facade = true,
) {
  return Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      "--define=__KELD_WINDOWS_HOST__:" + windows,
      "--define=__KELD_RUNTIME_PROCESS_FACTS__:" + runtime,
      "--define=__KELD_HOST_LAUNCH_FACTS__:" + JSON.stringify(record),
      "-e",
      `const { hostLaunchFacts: facts } = await import(${JSON.stringify(transport)});
      console.log(JSON.stringify({ facts, owned: [facts, facts?.paths, facts?.originalResourcePolicy, facts?.singleInstance]
        .every(value => value && Object.isFrozen(value) && Object.getPrototypeOf(value) === null) }));`,
    ],
    env: { ...process.env, KELD_NATIVE_WINDOW_FACADE: facade ? "1" : "0" },
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4_000,
  });
}

test("selected Windows schema eight captures the exact frozen Host record", () => {
  const child = observe(facts);
  expect(child.exitCode).toBe(0);
  expect(child.stderr.toString()).toBe("");
  expect(JSON.parse(child.stdout.toString())).toEqual({ facts, owned: true });
});

test("selected launch refuses foreign flags, layouts, and verdicts", () => {
  const cases = [
    observe(facts, false),
    observe(facts, true, false),
    observe(facts, true, true, false),
    observe({ ...facts, schema: 3 }),
    observe({ ...facts, singleInstance: { verdict: "retry" } }),
    observe({
      ...facts,
      singleInstance: { verdict: "acquired", owner: "other" },
    }),
    observe({ ...facts, root: "D:\\retained-stage" }),
    observe({ ...facts, paths: { appData: "D:\\..\\escape" } }),
    observe({ ...facts, locale: null }),
    observe({ ...facts, originalResourcePolicy: {} }),
    observe({ ...facts, extra: "metadata" }),
  ];
  for (const child of cases) {
    expect(child.exitCode).not.toBe(0);
    expect(child.stdout.toString()).toBe("");
    expect(child.stderr.toString()).toContain("KELD-IPC-029");
  }
});

test("capture refuses getters and proxies without invoking them", () => {
  let reads = 0;
  const getter = Object.defineProperty({ ...facts }, "singleInstance", {
    get() {
      reads++;
      return { verdict: "acquired" };
    },
  });
  const proxy = new Proxy(facts.paths, {
    ownKeys() {
      reads++;
      return [];
    },
  });
  expect(() => captureHostLaunchFacts(getter, true)).toThrow("KELD-IPC-029");
  expect(() =>
    captureHostLaunchFacts({ ...facts, paths: proxy }, true),
  ).toThrow("KELD-IPC-029");
  expect(reads).toBe(0);
});

test("a revoked launch-facts proxy receives the typed codec refusal", () => {
  const { proxy, revoke } = Proxy.revocable(facts, {});
  revoke();
  let refusal: unknown;
  try {
    captureHostLaunchFacts(proxy, true);
  } catch (error) {
    refusal = error;
  }
  expect(refusal).toHaveProperty("code", "KELD-IPC-029");
});

test("valid individual fields cannot exceed the aggregate Host record budget", () => {
  const input = {
    ...facts,
    name: "n".repeat(4096),
    version: "v".repeat(4096),
    locale: "x".repeat(4096),
    root: "\\\\?\\D:\\" + "r".repeat(4089),
  };
  expect(() => captureHostLaunchFacts(input, true)).toThrow("KELD-IPC-029");
  expect(
    captureHostLaunchFacts({ ...input, version: "1" }, true)?.version,
  ).toBe("1");
});

test("metadata capture ignores post-import intrinsic hooks and inherited setters", () => {
  const input = { ...facts, name: "selected 🖊" };
  let hooks = 0;
  const poison = () => {
    hooks++;
    throw new Error("poisoned intrinsic");
  };
  const locations = [
    [Reflect, "ownKeys"],
    [Object, "getOwnPropertyDescriptor"],
    [Object, "hasOwn"],
    [Object, "setPrototypeOf"],
    [Object, "freeze"],
    [Array, "isArray"],
    [Array.prototype, "includes"],
    [Array.prototype, "some"],
    [Array.prototype, "every"],
    [Array.prototype, Symbol.iterator],
    [String.prototype, "startsWith"],
    [String.prototype, "includes"],
    [String.prototype, "slice"],
    [String.prototype, "split"],
    [String.prototype, "charCodeAt"],
    [RegExp.prototype, "test"],
    [RegExp.prototype, "exec"],
    [JSON, "stringify"],
    [TextEncoder.prototype, "encode"],
  ] as const;
  const define = Object.defineProperty;
  const saved = locations.map(([owner, key]) =>
    Object.getOwnPropertyDescriptor(owner, key)!,
  );
  define(Object.prototype, "appData", { set: poison, configurable: true });
  let result;
  let refused;
  try {
    for (let index = 0; index < locations.length; index++) {
      const slot = locations[index]!;
      define(slot[0], slot[1], { ...saved[index], value: poison });
    }
    result = captureHostLaunchFacts(input, true);
    try {
      captureHostLaunchFacts({ ...input, unexpected: "foreign" }, true);
    } catch (error) {
      refused = error;
    }
  } finally {
    for (let index = 0; index < locations.length; index++) {
      const slot = locations[index]!;
      define(slot[0], slot[1], saved[index]!);
    }
    Reflect.deleteProperty(Object.prototype, "appData");
  }
  expect(result?.name).toBe(input.name);
  expect(result?.paths.appData).toBe(input.paths.appData);
  expect(refused).toHaveProperty("code", "KELD-IPC-029");
  expect(hooks).toBe(0);
});

test("qualified strings require Unicode scalar values", () => {
  for (const name of ["\ud800", "\udfff", "\ud800x", "\ud800\ud800"]) {
    expect(() => captureHostLaunchFacts({ ...facts, name }, true)).toThrow(
      "KELD-IPC-029",
    );
  }
  expect(captureHostLaunchFacts({ ...facts, name: "λ🖊" }, true)?.name).toBe(
    "λ🖊",
  );
});

test("truncated verbatim drive roots refuse without consulting inherited string indices", () => {
  const suffixes = ["", "D", "D:"];
  const keys = ["4", "5", "6"];
  const saved = keys.map((key) =>
    Object.getOwnPropertyDescriptor(String.prototype, key),
  );
  let reads = 0;
  const errors: unknown[] = [];
  try {
    for (const key of keys) {
      Object.defineProperty(String.prototype, key, {
        get() {
          reads++;
          throw new Error("inherited string index");
        },
        configurable: true,
      });
    }
    for (const suffix of suffixes) {
      try {
        captureHostLaunchFacts({ ...facts, root: "\\\\?\\" + suffix }, true);
      } catch (error) {
        errors.push(error);
      }
    }
  } finally {
    for (let index = 0; index < keys.length; index++) {
      const key = keys[index]!;
      const previous = saved[index];
      if (previous) Object.defineProperty(String.prototype, key, previous);
      else Reflect.deleteProperty(String.prototype, key);
    }
  }
  expect(errors).toHaveLength(suffixes.length);
  for (const error of errors)
    expect(error).toHaveProperty("code", "KELD-IPC-029");
  expect(reads).toBe(0);
  expect(captureHostLaunchFacts(facts, true)?.root).toBe(facts.root);
});

test("Windows schema eight freezes the exact own application menu absence", () => {
  const input = { ...facts, windowsApplicationMenu: "absent-v1" };
  const child = observe(input);
  expect(child.exitCode, child.stderr.toString()).toBe(0);
  expect(child.stderr.toString()).toBe("");
  expect(JSON.parse(child.stdout.toString())).toEqual({
    facts: input,
    owned: true,
  });
  const captured = captureHostLaunchFacts(input, true)!;
  input.windowsApplicationMenu = "changed";
  expect(Object.getOwnPropertyDescriptor(captured, "windowsApplicationMenu")).toEqual({
    value: "absent-v1",
    enumerable: true,
    writable: false,
    configurable: false,
  });
});

test("Windows menu absence refuses foreign, older and malformed own data", () => {
  const { originalResourcePolicy: _policy, singleInstance: _verdict, ...legacy } = facts;
  const attested = { ...facts, windowsApplicationMenu: "absent-v1" };
  for (const child of [
    observe(attested, false),
    observe(attested, true, false),
    observe(attested, true, true, false),
    observe({ ...legacy, schema: 3, windowsApplicationMenu: "absent-v1" }),
  ]) {
    expect(child.exitCode).not.toBe(0);
    expect(child.stderr.toString()).toContain("KELD-IPC-029");
    expect(child.stdout.toString()).toBe("");
  }
  for (const value of [undefined, null, true, "", "absent-v2", ["absent-v1"]]) {
    expect(() => captureHostLaunchFacts({ ...facts, windowsApplicationMenu: value }, true))
      .toThrow("KELD-IPC-029");
  }
});

test("Windows menu absence never reads an accessor or inherits a fact", () => {
  let reads = 0;
  const accessor = Object.defineProperty({ ...facts }, "windowsApplicationMenu", {
    get() {
      reads++;
      return "absent-v1";
    },
  });
  expect(() => captureHostLaunchFacts(accessor, true)).toThrow("KELD-IPC-029");
  expect(reads).toBe(0);
  const inherited = Object.assign(
    Object.create({ windowsApplicationMenu: "absent-v1" }),
    facts,
  );
  expect(captureHostLaunchFacts(inherited, true)?.windowsApplicationMenu).toBeUndefined();
  expect(captureHostLaunchFacts(facts, true)?.windowsApplicationMenu).toBeUndefined();
});

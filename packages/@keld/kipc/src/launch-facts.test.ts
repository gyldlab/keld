import { expect, test } from "bun:test";
import { captureHostLaunchFacts } from "./launch-facts.ts";

function installed() {
  return {
    schema: 6,
    kind: "local-development",
    name: "Facts",
    version: "1.2.3",
    root: "/Applications/Facts.app/Contents/Resources/app",
    resourcesRoot: "/Applications/Facts.app/Contents/Resources",
  };
}

test("installed codec captures immutable own fields and preserves omitted facts", () => {
  const input = installed();
  const facts = captureHostLaunchFacts(input)!;
  input.name = "changed";
  expect(facts.name).toBe("Facts");
  expect(Object.isFrozen(facts)).toBe(true);
  expect(Object.getPrototypeOf(facts)).toBeNull();
  expect(Reflect.has(facts, "locale")).toBe(false);
  expect(captureHostLaunchFacts(undefined)).toBeUndefined();
});

test("installed codec rejects missing unknown mixed and oversized fields", () => {
  for (const key of ["name", "root", "version", "resourcesRoot"]) {
    const missing: Record<string, unknown> = installed();
    delete missing[key];
    expect(() => captureHostLaunchFacts(missing)).toThrow("KELD-IPC-029");
    expect(() =>
      captureHostLaunchFacts({ ...installed(), [key]: null }),
    ).toThrow("KELD-IPC-029");
  }
  for (const input of [
    null,
    [],
    true,
    { ...installed(), schema: 4 },
    { ...installed(), schema: 5 },
    { ...installed(), schema: 7 },
    { ...installed(), kind: "development" },
    { ...installed(), root: "/foreign/app" },
    { ...installed(), resourcesRoot: "/foreign" },
    { ...installed(), paths: {} },
    { ...installed(), locale: "en" },
    { ...installed(), grant: "all" },
    { ...installed(), version: "" },
    { ...installed(), version: "a".repeat(4097) },
    { ...installed(), name: "\0bad" },
    Object.assign(installed(), { [Symbol("extra")]: true }),
  ])
    expect(() => captureHostLaunchFacts(input)).toThrow("KELD-IPC-029");
});

test("the closed codec does not invoke accessors or proxy traps", () => {
  let reads = 0;
  const accessor = Object.defineProperty(installed(), "schema", {
    get() {
      reads++;
      return 6;
    },
  });
  expect(() => captureHostLaunchFacts(accessor)).toThrow("KELD-IPC-029");
  const proxy = new Proxy(installed(), {
    ownKeys() {
      reads++;
      return [];
    },
  });
  expect(() => captureHostLaunchFacts(proxy)).toThrow("KELD-IPC-029");
  expect(reads).toBe(0);
});

test("missing own schema is rejected without reading an inherited schema accessor", () => {
  let reads = 0;
  const prototype = Object.defineProperty({}, "schema", {
    get() {
      reads++;
      return 6;
    },
  });
  const input: Record<string, unknown> = installed();
  delete input.schema;
  Object.setPrototypeOf(input, prototype);
  expect(() => captureHostLaunchFacts(input)).toThrow("KELD-IPC-029");
  expect(reads).toBe(0);
});

test("installed extension preserves development schemas without relabeling them", () => {
  for (const schema of [1, 2, 3]) {
    const input = {
      schema,
      kind: "development",
      name: "Dev",
      root: "/stage",
      ...(schema >= 2 ? { version: "1", paths: {} } : {}),
      ...(schema === 3 ? { locale: "fr-CA" } : {}),
    };
    const facts = captureHostLaunchFacts(input)!;
    expect(facts.kind).toBe("development");
    expect(Reflect.has(facts, "resourcesRoot")).toBe(false);
    expect(() =>
      captureHostLaunchFacts({ ...input, resourcesRoot: "/stage" }),
    ).toThrow("KELD-IPC-029");
  }
});

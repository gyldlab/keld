/** GH-570 descriptor and conflict regressions for the installed GH-1404 owner. */
import { expect, test } from "bun:test";
import { installMainProcessFacts } from "./process-facts.ts";

const installedResources = "/Applications/Independent.app/Contents/Resources";

test("native GH-570 descriptors coexist with the Host-only installed resourcesPath", () => {
  const target = { versions: {} };
  installMainProcessFacts(target, { resourcesPath: installedResources });
  expect(Object.getOwnPropertyDescriptor(target, "type")).toEqual({
    value: "browser",
    writable: false,
    enumerable: true,
    configurable: true,
  });
  expect(Object.getOwnPropertyDescriptor(target.versions, "electron")).toEqual({
    value: "0.0.1",
    writable: false,
    enumerable: true,
    configurable: true,
  });
  expect(Object.getOwnPropertyDescriptor(target, "defaultApp")).toEqual({
    value: true,
    writable: false,
    enumerable: true,
    configurable: false,
  });
  expect(Object.getOwnPropertyDescriptor(target, "resourcesPath")).toEqual({
    value: installedResources,
    writable: false,
    enumerable: true,
    configurable: false,
  });
  installMainProcessFacts(target, { resourcesPath: installedResources });
  expect(Object.hasOwn(target, "mas")).toBe(false);
  expect(Object.hasOwn(target, "sandboxed")).toBe(false);
  const noHostData = { versions: {} };
  installMainProcessFacts(noHostData);
  expect(Object.hasOwn(noHostData, "resourcesPath")).toBe(false);
});

test("a frozen versions owner rejects missing electron before any main-role fact is written", () => {
  const target = { versions: Object.preventExtensions({}) };
  expect(() =>
    installMainProcessFacts(target, { resourcesPath: installedResources }),
  ).toThrow("KELD-COMPAT-014");
  expect(Object.hasOwn(target, "type")).toBe(false);
  expect(Object.hasOwn(target, "defaultApp")).toBe(false);
  expect(Object.hasOwn(target, "resourcesPath")).toBe(false);
  expect(Object.hasOwn(target.versions, "electron")).toBe(false);
});

test("a frozen top-level process refuses a missing Host resourcesPath after native facts exist", () => {
  const target = { versions: {} };
  installMainProcessFacts(target);
  Object.preventExtensions(target);
  expect(() =>
    installMainProcessFacts(target, { resourcesPath: installedResources }),
  ).toThrow("KELD-COMPAT-014");
  expect(Object.getOwnPropertyNames(target)).toEqual([
    "versions",
    "type",
    "defaultApp",
  ]);
  expect(Object.getOwnPropertyNames(target.versions)).toEqual(["electron"]);
  expect(Object.hasOwn(target, "resourcesPath")).toBe(false);
});

test("conflicting object/function values never invoke diagnostic coercion hooks or write partial facts", () => {
  for (const alien of [
    {
      [Symbol.toPrimitive]() {
        throw Error("foreign hook executed");
      },
    },
    Object.assign(Object.create(null), { foreign: true }),
    Object.assign(function foreign() {}, {
      toString() {
        throw Error("foreign function diagnostic executed");
      },
    }),
  ]) {
    const target = { versions: { electron: alien } };
    expect(() =>
      installMainProcessFacts(target, { resourcesPath: installedResources }),
    ).toThrow("KELD-COMPAT-014");
    expect(Object.hasOwn(target, "type")).toBe(false);
    expect(Object.hasOwn(target, "defaultApp")).toBe(false);
    expect(Object.hasOwn(target, "resourcesPath")).toBe(false);
    expect(Reflect.get(target.versions, "electron")).toBe(alien);
  }
});

test("a same-value but non-configurable type/electron descriptor refuses before mutation", () => {
  for (const fact of ["type", "electron"] as const) {
    const target = { versions: {} };
    Object.defineProperty(fact === "type" ? target : target.versions, fact, {
      value: fact === "type" ? "browser" : "0.0.1",
      enumerable: true,
      writable: false,
      configurable: false,
    });
    expect(() =>
      installMainProcessFacts(target, { resourcesPath: installedResources }),
    ).toThrow("KELD-COMPAT-014");
    expect(Object.hasOwn(target, "resourcesPath")).toBe(false);
    expect(Object.hasOwn(target, "defaultApp")).toBe(false);
    if (fact === "electron") expect(Object.hasOwn(target, "type")).toBe(false);
  }
});

test("conflicting installed resourcesPath rejects without changing compatible GH-570 facts", () => {
  const target = { versions: {} };
  Object.defineProperty(target, "resourcesPath", {
    value: "/foreign",
    enumerable: true,
    configurable: true,
  });
  expect(() =>
    installMainProcessFacts(target, { resourcesPath: installedResources }),
  ).toThrow("KELD-COMPAT-014");
  expect(Object.hasOwn(target, "type")).toBe(false);
  expect(Object.hasOwn(target, "defaultApp")).toBe(false);
  expect(Object.hasOwn(target.versions, "electron")).toBe(false);
  expect(Reflect.get(target, "resourcesPath")).toBe("/foreign");
});

test("foreign defaultApp and resourcesPath toString hooks never execute on refusal", () => {
  for (const name of ["defaultApp", "resourcesPath"] as const) {
    let calls = 0;
    const alien = {
      toString(): never {
        calls += 1;
        throw new Error("application toString must not execute");
      },
    };
    const target = { versions: {} };
    Object.defineProperty(target, name, { value: alien, configurable: true });
    expect(() =>
      installMainProcessFacts(target, { resourcesPath: installedResources }),
    ).toThrow("KELD-COMPAT-014");
    expect(calls).toBe(0);
    expect(Object.hasOwn(target, "type")).toBe(false);
    expect(Object.hasOwn(target.versions, "electron")).toBe(false);
    expect(Reflect.get(target, name)).toBe(alien);
  }
});

test("nonextensible initial process target refuses before writing versions", () => {
  const target = Object.preventExtensions({ versions: {} });
  expect(() => installMainProcessFacts(target)).toThrow("KELD-COMPAT-014");
  expect(Object.hasOwn(target.versions, "electron")).toBe(false);
  expect(Object.hasOwn(target, "type")).toBe(false);
});

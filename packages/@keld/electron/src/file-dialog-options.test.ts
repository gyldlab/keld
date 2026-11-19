import { expect, test } from "bun:test";

import { encodeDialogRequest } from "../../api/src/dialog.generated.ts";
import {
  normalizeOpenDialogOptions,
  normalizeSaveDialogOptions,
} from "./file-dialog-options.ts";

const drawioFilters = [
  { name: "draw.io", extensions: ["drawio", "xml"] },
  { name: "VSDX", extensions: ["vsdx"] },
  { name: "Mermaid", extensions: ["mmd", "mermaid"] },
  { name: "All Files", extensions: ["*"] },
];

test("pinned draw.io Open keeps file+directory, multiple selection and four independent groups", () => {
  const options = normalizeOpenDialogOptions({
    defaultPath: undefined,
    filters: drawioFilters,
    properties: ["openFile", "openDirectory", "multiSelections"],
    securityScopedBookmarks: false,
  });
  expect(options).toEqual({
    default_path: null,
    filters: drawioFilters,
    properties: ["OpenFile", "OpenDirectory", "MultiSelections"],
  });
  expect(Object.isFrozen(options)).toBe(true);
  expect(Object.isFrozen(options.filters)).toBe(true);
  expect(Object.isFrozen(options.filters[0].extensions)).toBe(true);
  expect(Object.isFrozen(options.properties)).toBe(true);
  const bytes = encodeDialogRequest({
    variant: "Open",
    nonce: 1n,
    parent: null,
    options,
  });
  expect([...bytes].slice(0, 3)).toEqual([2, 1, 0]);
  expect(bytes.byteLength).toBeLessThanOrEqual(16 * 1024 + 32);
  expect(options.filters.map((group) => group.name)).toEqual([
    "draw.io",
    "VSDX",
    "Mermaid",
    "All Files",
  ]);
});

test("Save never grants security scope and preserves filename hint without writing it", () => {
  const save = normalizeSaveDialogOptions({
    defaultPath: "diagram.drawio",
    filters: drawioFilters,
    securityScopedBookmarks: false,
  });
  expect(save).toEqual({
    default_path: "diagram.drawio",
    filters: drawioFilters,
    security_scoped_bookmarks: false,
  });
  expect(
    [
      ...encodeDialogRequest({
        variant: "Save",
        nonce: 1n,
        parent: null,
        options: save,
      }),
    ].slice(0, 3),
  ).toEqual([3, 1, 0]);
  expect(normalizeSaveDialogOptions({})).toEqual({
    default_path: null,
    filters: [],
    security_scoped_bookmarks: false,
  });
  expect(normalizeOpenDialogOptions({})).toEqual({
    default_path: null,
    filters: [],
    properties: [],
  });
  for (const invalid of [
    { securityScopedBookmarks: true },
    { securityScopedBookmarks: null },
    { defaultPath: null },
    { properties: ["openFile"] },
    { unknown: undefined },
  ]) {
    expect(() => normalizeSaveDialogOptions(invalid)).toThrow(
      "KELD-COMPAT-010",
    );
  }
  expect(() =>
    normalizeOpenDialogOptions({ securityScopedBookmarks: true }),
  ).toThrow("KELD-COMPAT-010");
});

test("untrusted objects and getters never perform side effects during early admission", () => {
  let reads = 0;
  const accessor = Object.defineProperty({}, "filters", {
    enumerable: true,
    get() {
      reads++;
      return drawioFilters;
    },
  });
  const filterAccessor = Object.defineProperty(
    { name: "draw.io" },
    "extensions",
    {
      enumerable: true,
      get() {
        reads++;
        return ["drawio"];
      },
    },
  );
  const arrayGetter: unknown[] = [];
  Object.defineProperty(arrayGetter, "0", {
    enumerable: true,
    get() {
      reads++;
      return "openFile";
    },
  });
  arrayGetter.length = 1;
  for (const malformed of [
    null,
    undefined,
    false,
    "strings",
    [],
    Object.create({ defaultPath: "inherited" }),
    accessor,
    { filters: [filterAccessor] },
    { properties: arrayGetter },
    Object.defineProperty({}, "unknown", {
      enumerable: true,
      value: undefined,
    }),
  ]) {
    expect(() => normalizeOpenDialogOptions(malformed)).toThrow(
      "KELD-COMPAT-010",
    );
  }
  expect(reads).toBe(0);
  for (const malformed of [
    { filters: [null] },
    { filters: [{ name: "x", extensions: ["xml"], unrecognized: undefined }] },
    { filters: [{ name: undefined, extensions: ["xml"] }] },
    { filters: [{ name: "x", extensions: undefined }] },
    { properties: ["openFile", "openFile"] },
    { properties: ["showHiddenFiles"] },
    { properties: null },
  ]) {
    expect(() => normalizeOpenDialogOptions(malformed)).toThrow(
      "KELD-COMPAT-010",
    );
  }
});

test("revoked and throwing Proxy inspection returns typed COMPAT-010 without a panel", () => {
  const revokedArray = Proxy.revocable([], {});
  const revokedObject = Proxy.revocable({}, {});
  revokedArray.revoke();
  revokedObject.revoke();
  const throwingPrototype = new Proxy([], {
    getPrototypeOf() {
      throw new TypeError("deliberate prototype inspection trap");
    },
  });
  for (const input of [
    revokedObject.proxy,
    { filters: revokedArray.proxy },
    { properties: revokedArray.proxy },
    { filters: [revokedObject.proxy] },
    { filters: throwingPrototype },
    { properties: throwingPrototype },
  ]) {
    expect(() => normalizeOpenDialogOptions(input)).toThrow("KELD-COMPAT-010");
  }
  for (const input of [
    revokedObject.proxy,
    { filters: revokedArray.proxy },
    { filters: [revokedObject.proxy] },
    { filters: throwingPrototype },
  ]) {
    expect(() => normalizeSaveDialogOptions(input)).toThrow("KELD-COMPAT-010");
  }
});

test("byte and count bounds reject malformed extensions without widening native file access", () => {
  const filter = (extensions: unknown) => ({ name: "File", extensions });
  for (const extensions of [
    [],
    ["*.drawio"],
    [".xml"],
    ["bad."],
    ["a/b"],
    ["a\\b"],
    ["xml", "xml"],
    ["*", "xml"],
    ["a".repeat(129)],
    ["a\u007f"],
    Array(33).fill("xml"),
  ]) {
    expect(() =>
      normalizeOpenDialogOptions({ filters: [filter(extensions)] }),
    ).toThrow("KELD-COMPAT-010");
  }
  expect(() =>
    normalizeOpenDialogOptions({ filters: Array(33).fill(filter(["xml"])) }),
  ).toThrow("KELD-COMPAT-010");
  expect(() =>
    normalizeOpenDialogOptions({
      filters: [{ name: "a".repeat(257), extensions: ["xml"] }],
    }),
  ).toThrow("KELD-COMPAT-010");
  expect(() =>
    normalizeOpenDialogOptions({ defaultPath: "x".repeat(4097) }),
  ).toThrow("KELD-COMPAT-010");
  expect(() => normalizeSaveDialogOptions({ defaultPath: "x\0y" })).toThrow(
    "KELD-COMPAT-010",
  );
  const valid = normalizeOpenDialogOptions({ filters: [filter(["xml"])] });
  expect(valid.filters[0].extensions).toEqual(["xml"]);
  const final = normalizeOpenDialogOptions({
    defaultPath: "🌍".repeat(1000),
    properties: ["openFile"],
  });
  expect(final.default_path).toHaveLength(2000);
});

test("oversized chooser arrays reject before enumerating element descriptors", () => {
  for (const [field, max] of [
    ["properties", 3],
    ["filters", 32],
    ["extensions", 32],
  ] as const) {
    for (const count of [max + 1, 1_000_000]) {
      let enumerations = 0;
      let elementDescriptors = 0;
      let valueReads = 0;
      const array = new Proxy(new Array(count), {
        ownKeys(target) {
          enumerations++;
          return Reflect.ownKeys(target);
        },
        getOwnPropertyDescriptor(target, key) {
          if (key !== "length") elementDescriptors++;
          return Reflect.getOwnPropertyDescriptor(target, key);
        },
        get() {
          valueReads++;
          throw new Error("array value access is forbidden");
        },
      });
      const options =
        field === "extensions"
          ? { filters: [{ name: "File", extensions: array }] }
          : { [field]: array };
      expect(() => normalizeOpenDialogOptions(options)).toThrow(
        "KELD-COMPAT-010",
      );
      if (field !== "properties") {
        expect(() => normalizeSaveDialogOptions(options)).toThrow(
          "KELD-COMPAT-010",
        );
      }
      expect(enumerations).toBe(0);
      expect(elementDescriptors).toBe(0);
      expect(valueReads).toBe(0);
    }
  }
});

test("array descriptor preflight rejects traps and changed lengths without reading values", () => {
  const throwingLength = new Proxy(["openFile"], {
    getOwnPropertyDescriptor() {
      throw new Error("length inspection fault");
    },
  });
  let lengthReads = 0;
  const changingLength = new Proxy(["openFile"], {
    getOwnPropertyDescriptor(target, key) {
      const descriptor = Reflect.getOwnPropertyDescriptor(target, key);
      if (key === "length" && ++lengthReads > 1) {
        return { ...descriptor, value: 0 };
      }
      return descriptor;
    },
  });
  const throwingKeys = new Proxy(["openFile"], {
    ownKeys() {
      throw new Error("enumeration fault");
    },
  });
  const hole = new Array(1);
  const hidden = Object.defineProperty(["openFile"], "0", {
    enumerable: false,
  });
  const extra = Object.assign(["openFile"], { extra: true });
  const symbol = Object.assign(["openFile"], { [Symbol("extra")]: true });
  for (const properties of [
    throwingLength,
    changingLength,
    throwingKeys,
    hole,
    hidden,
    extra,
    symbol,
  ]) {
    expect(() => normalizeOpenDialogOptions({ properties })).toThrow(
      "KELD-COMPAT-010",
    );
  }
  let valueReads = 0;
  const valid = new Proxy(["openFile", "openDirectory", "multiSelections"], {
    get() {
      valueReads++;
      throw new Error("value trap");
    },
  });
  expect(normalizeOpenDialogOptions({ properties: valid }).properties).toEqual([
    "OpenFile",
    "OpenDirectory",
    "MultiSelections",
  ]);
  expect(valueReads).toBe(0);
  expect(normalizeOpenDialogOptions({ properties: [] }).properties).toEqual([]);
});

test("chooser filter and extension counts accept their exact 32-entry limits", () => {
  const groups = Array.from({ length: 32 }, (_, index) => ({
    name: `Group ${index}`,
    extensions: [`type${index}`],
  }));
  const extensions = Array.from({ length: 32 }, (_, index) => `type${index}`);
  for (const filters of [groups, [{ name: "Many extensions", extensions }]]) {
    for (const normalize of [
      normalizeOpenDialogOptions,
      normalizeSaveDialogOptions,
    ]) {
      const options = normalize({ filters });
      expect(options.filters).toEqual(filters);
      expect(Object.isFrozen(options.filters)).toBe(true);
      for (const filter of options.filters) {
        expect(Object.isFrozen(filter)).toBe(true);
        expect(Object.isFrozen(filter.extensions)).toBe(true);
      }
    }
  }
});

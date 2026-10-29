/**
 * Private first-proof file chooser option admission. No browser/window
 * authority or native side effect is created in this module. A future public
 * facade may only submit the frozen canonical snapshot through the one
 * authenticated app-link, after real AppKit Open/Save is independently proven.
 */
import {
  encodeDialogRequest,
  type FileDialogFilter,
  type OpenDialogOptions,
  type OpenProperty,
  type SaveDialogOptions,
} from "../../api/src/dialog.generated.ts";

const MAX_TEXT = 4096;
const MAX_GROUPS = 32;
const MAX_EXTENSIONS = 32;
const MAX_FILTER_NAME = 256;
const MAX_EXTENSION_BYTES = 128;

const OPEN_FIELDS = new Set([
  "defaultPath",
  "filters",
  "properties",
  "securityScopedBookmarks",
]);
const SAVE_FIELDS = new Set([
  "defaultPath",
  "filters",
  "securityScopedBookmarks",
]);
const FILTER_FIELDS = new Set(["name", "extensions"]);

function invalid(detail: string): never {
  throw new Error(
    `KELD-COMPAT-010: unsupported native file dialog options: ${detail}. ` +
      "No panel was opened and no filesystem authority was granted.",
  );
}

/** Inspect only own data descriptors, never inherited or getter values. */
function objectFields(
  input: unknown,
  supported: ReadonlySet<string>,
): Map<string, unknown> {
  if (input === null || typeof input !== "object" || Array.isArray(input)) {
    return invalid("expected a plain options object");
  }
  let descriptors: PropertyDescriptorMap;
  try {
    const prototype = Object.getPrototypeOf(input);
    if (prototype !== Object.prototype && prototype !== null) {
      return invalid("custom-prototype input is unsupported");
    }
    descriptors = Object.getOwnPropertyDescriptors(input);
  } catch {
    return invalid("options could not be inspected safely");
  }
  const result = new Map<string, unknown>();
  for (const name of Reflect.ownKeys(descriptors)) {
    const descriptor = Reflect.get(descriptors, name) as
      PropertyDescriptor | undefined;
    if (!descriptor?.enumerable) continue;
    if (typeof name !== "string" || !supported.has(name)) {
      return invalid("unknown own option key");
    }
    if (!("value" in descriptor))
      return invalid(`accessor ${name} is unsupported`);
    result.set(name, descriptor.value);
  }
  return result;
}

/** Copy a dense ordinary array using only data descriptors, refusing getters. */
function denseArray(input: unknown, max: number, name: string): unknown[] {
  if (
    !Array.isArray(input) ||
    Object.getPrototypeOf(input) !== Array.prototype
  ) {
    return invalid(`${name} must be a plain array`);
  }
  let descriptors: PropertyDescriptorMap;
  try {
    // TypeScript's array-specific overload does not advertise the symbol
    // index signature, but the runtime descriptor map still carries own
    // string and symbol keys and is inspected by Reflect.ownKeys below.
    descriptors = Object.getOwnPropertyDescriptors(
      input,
    ) as unknown as PropertyDescriptorMap;
  } catch {
    return invalid(`${name} cannot be inspected safely`);
  }
  const length = descriptors.length?.value;
  if (!Number.isInteger(length) || length < 0 || length > max) {
    return invalid(`${name} exceeds its bounded count`);
  }
  for (const key of Reflect.ownKeys(descriptors)) {
    if (key === "length") continue;
    if (
      typeof key !== "string" ||
      !/^(0|[1-9]\d*)$/.test(key) ||
      Number(key) >= length ||
      !descriptors[key]?.enumerable
    ) {
      return invalid(`${name} has an unsupported own array property`);
    }
  }
  const entries: unknown[] = [];
  for (let index = 0; index < length; index++) {
    const descriptor = descriptors[String(index)];
    if (!descriptor || !("value" in descriptor)) {
      return invalid(`${name} contains a hole or accessor`);
    }
    entries.push(descriptor.value);
  }
  return entries;
}

function optionalText(
  fields: Map<string, unknown>,
  name: string,
): string | null {
  const value = fields.get(name);
  if (value === undefined) return null;
  if (
    typeof value !== "string" ||
    Buffer.byteLength(value, "utf8") > MAX_TEXT ||
    value.includes("\0")
  ) {
    return invalid(`${name} must be a bounded non-NUL string`);
  }
  return value;
}

function bookmarkAllowed(fields: Map<string, unknown>): void {
  const value = fields.get("securityScopedBookmarks");
  if (value !== undefined && value !== false) {
    return invalid(
      "securityScopedBookmarks is false-only; no scope can be minted",
    );
  }
}

function normalizeFilters(
  fields: Map<string, unknown>,
): readonly FileDialogFilter[] {
  const input = fields.get("filters");
  if (input === undefined) return Object.freeze([]);
  const filters = denseArray(input, MAX_GROUPS, "filters");
  const normalized: FileDialogFilter[] = [];
  for (const inputFilter of filters) {
    const parts = objectFields(inputFilter, FILTER_FIELDS);
    const label = parts.get("name");
    if (
      typeof label !== "string" ||
      label.length === 0 ||
      Buffer.byteLength(label, "utf8") > MAX_FILTER_NAME ||
      label.includes("\0")
    ) {
      return invalid("filter group needs a bounded nonempty name");
    }
    const extensions = denseArray(
      parts.get("extensions"),
      MAX_EXTENSIONS,
      "extensions",
    );
    if (extensions.length === 0)
      return invalid("filter group requires an extension");
    const names: string[] = [];
    const seen = new Set<string>();
    for (const value of extensions) {
      if (
        typeof value !== "string" ||
        !value ||
        Buffer.byteLength(value, "utf8") > MAX_EXTENSION_BYTES ||
        value.startsWith(".") ||
        value.endsWith(".") ||
        (value !== "*" &&
          [...value].some(
            (ch) =>
              ch === "/" ||
              ch === "\\" ||
              ch === "*" ||
              (ch.codePointAt(0) ?? 0) < 32 ||
              ((ch.codePointAt(0) ?? 0) >= 0x7f &&
                (ch.codePointAt(0) ?? 0) <= 0x9f),
          )) ||
        seen.has(value)
      ) {
        return invalid("malformed, repeated or excessive extension");
      }
      seen.add(value);
      names.push(value);
    }
    if (seen.has("*") && seen.size !== 1) {
      return invalid("the All Files token must be alone in its group");
    }
    normalized.push(
      Object.freeze({ name: label, extensions: Object.freeze(names) }),
    );
  }
  return Object.freeze(normalized);
}

function qualifyWire(
  variant: "Open" | "Save",
  options: OpenDialogOptions | SaveDialogOptions,
): void {
  try {
    if (variant === "Open") {
      encodeDialogRequest({
        variant,
        nonce: 1n,
        parent: null,
        options: options as OpenDialogOptions,
      });
    } else {
      encodeDialogRequest({
        variant,
        nonce: 1n,
        parent: null,
        options: options as SaveDialogOptions,
      });
    }
  } catch {
    invalid("canonical native file dialog envelope exceeds its limits");
  }
}

/** Normalize exactly the supported first-proof Open grammar, without UI. */
export function normalizeOpenDialogOptions(input: unknown): OpenDialogOptions {
  const fields = objectFields(input, OPEN_FIELDS);
  bookmarkAllowed(fields);
  const flags = fields.get("properties");
  const selected: OpenProperty[] = [];
  const seen = new Set<OpenProperty>();
  if (flags !== undefined) {
    for (const value of denseArray(flags, 3, "Open properties")) {
      const flag =
        value === "openFile"
          ? "OpenFile"
          : value === "openDirectory"
            ? "OpenDirectory"
            : value === "multiSelections"
              ? "MultiSelections"
              : invalid("unknown Open selection property");
      if (seen.has(flag)) invalid("duplicate Open selection property");
      seen.add(flag);
      selected.push(flag);
    }
  }
  const options: OpenDialogOptions = {
    default_path: optionalText(fields, "defaultPath"),
    filters: normalizeFilters(fields),
    properties: Object.freeze(selected),
  };
  qualifyWire("Open", options);
  return Object.freeze(options);
}

/** Normalize exact Save grammar; no destination is created or opened here. */
export function normalizeSaveDialogOptions(input: unknown): SaveDialogOptions {
  const fields = objectFields(input, SAVE_FIELDS);
  bookmarkAllowed(fields);
  const options: SaveDialogOptions = {
    default_path: optionalText(fields, "defaultPath"),
    filters: normalizeFilters(fields),
    security_scoped_bookmarks: false,
  };
  qualifyWire("Save", options);
  return Object.freeze(options);
}

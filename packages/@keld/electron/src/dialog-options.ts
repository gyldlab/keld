/**
 * Private Electron Message option admission. The source object is not passed
 * to the app link: native effects use only this bounded, copied snapshot.
 * The Host still independently owns identity, Guard, parent and real UI.
 */
import type {
  MessageKind,
  MessageOptions,
} from "@keld/api/internal/dialog-codec";
import { encodeDialogRequest } from "@keld/api/internal/dialog-codec";

const MAX_MESSAGE_BYTES = 16 * 1024;
const MAX_BUTTONS = 16;
const ALLOWED_KEYS = new Set([
  "message",
  "type",
  "title",
  "detail",
  "buttons",
  "defaultId",
  "cancelId",
  "noLink",
  "normalizeAccessKeys",
  "checkboxLabel",
  "checkboxChecked",
]);

function invalid(detail: string): never {
  throw new Error(
    `KELD-COMPAT-010: unsupported Message dialog options: ${detail}. ` +
      "Pass only supported, bounded Electron dialog fields; no native panel was opened.",
  );
}

// Only reflection belongs inside this boundary; validation keeps its own reason.
function inspectOptions<T>(inspect: () => T, detail: string): T {
  try {
    return inspect();
  } catch {
    return invalid(detail);
  }
}

function optionValues(input: unknown): Map<string, unknown> {
  if (input === null || typeof input !== "object") {
    invalid("options must be an object");
  }
  // Do not trust inherited fields, prototype monkey-patching or accessors to
  // fabricate input at admission time. Ordinary own enumerable data fields
  // are precisely what the supported draw.io call supplies.
  const detail = "options could not be inspected safely";
  if (inspectOptions(() => Array.isArray(input), detail)) {
    invalid("options must be an object");
  }
  const prototype = inspectOptions(() => Object.getPrototypeOf(input), detail);
  if (prototype !== Object.prototype && prototype !== null) {
    invalid("custom-prototype options are unsupported");
  }
  const descriptors = inspectOptions(
    () => Object.getOwnPropertyDescriptors(input),
    detail,
  );
  const fields = new Map<string, unknown>();
  for (const name of Reflect.ownKeys(descriptors)) {
    const descriptor = Reflect.get(descriptors, name) as
      PropertyDescriptor | undefined;
    if (!descriptor?.enumerable) continue;
    if (typeof name !== "string" || !ALLOWED_KEYS.has(name)) {
      invalid("unknown own key");
    }
    if (!("value" in descriptor)) invalid(`accessor ${name} is unsupported`);
    fields.set(name, descriptor.value);
  }
  return fields;
}

function boundedText(value: unknown, name: string): string {
  if (
    typeof value !== "string" ||
    Buffer.byteLength(value, "utf8") > MAX_MESSAGE_BYTES
  ) {
    invalid(`${name} must be bounded UTF-8 text`);
  }
  return value;
}

function optionalText(
  fields: Map<string, unknown>,
  name: string,
): string | null {
  const value = fields.get(name);
  return value === undefined ? null : boundedText(value, name);
}

function optionalBoolean(
  fields: Map<string, unknown>,
  name: string,
  absent: boolean,
): boolean {
  const value = fields.get(name);
  if (value === undefined) return absent;
  if (typeof value !== "boolean") invalid(`${name} must be a boolean`);
  return value;
}

function buttonLabels(value: unknown): string[] {
  if (value === undefined) return [];
  const detail = "buttons could not be inspected safely";
  if (!inspectOptions(() => Array.isArray(value), detail)) {
    invalid("buttons must be a plain array");
  }
  const prototype = inspectOptions(() => Object.getPrototypeOf(value), detail);
  if (prototype !== Array.prototype) invalid("buttons must be a plain array");
  // Snapshot the own data length once. Never invoke Proxy get traps or
  // enumerate an unbounded array before enforcing the native button limit.
  const lengthDescriptor = inspectOptions(
    () => Object.getOwnPropertyDescriptor(value, "length"),
    detail,
  );
  const length: unknown = lengthDescriptor?.value;
  if (
    !lengthDescriptor ||
    !("value" in lengthDescriptor) ||
    typeof length !== "number" ||
    !Number.isInteger(length) ||
    length < 0 ||
    length > MAX_BUTTONS
  ) {
    invalid("buttons must have a bounded own data length");
  }
  const labels: string[] = [];
  for (let index = 0; index < length; index++) {
    const descriptor = inspectOptions(
      () => Object.getOwnPropertyDescriptor(value, String(index)),
      detail,
    );
    if (!descriptor || !("value" in descriptor)) {
      invalid("button labels must be own data values");
    }
    labels.push(boundedText(descriptor.value, "button label"));
  }
  return labels;
}

function buttonIndex(
  fields: Map<string, unknown>,
  name: string,
  count: number,
): number | null {
  const value = fields.get(name);
  if (value === undefined) return null;
  if (
    typeof value !== "number" ||
    !Number.isInteger(value) ||
    value < 0 ||
    value >= count
  ) {
    invalid(`${name} must select an existing button`);
  }
  return value;
}

function messageKind(value: unknown): MessageKind {
  if (value === undefined || value === "none") return "None";
  if (value === "info") return "Info";
  if (value === "error") return "Error";
  if (value === "question") return "Question";
  if (value === "warning") return "Warning";
  return invalid("unknown message type");
}

/** Build the native-owned canonical options, without minting a nonce or UI. */
export function normalizeMessageOptions(input: unknown): MessageOptions {
  const fields = optionValues(input);
  const buttons = buttonLabels(fields.get("buttons"));
  const checkboxLabel = optionalText(fields, "checkboxLabel");
  const initialCheckbox = fields.get("checkboxChecked");
  if (checkboxLabel === "") invalid("checkbox label must not be empty");
  if (initialCheckbox !== undefined && checkboxLabel === null) {
    invalid("checkbox state requires a checkbox label");
  }
  const options: MessageOptions = {
    message: boundedText(fields.get("message"), "message"),
    kind: messageKind(fields.get("type")),
    title: optionalText(fields, "title"),
    detail: optionalText(fields, "detail"),
    buttons,
    default_id: buttonIndex(fields, "defaultId", Math.max(1, buttons.length)),
    cancel_id: buttonIndex(fields, "cancelId", Math.max(1, buttons.length)),
    normalize_access_keys: optionalBoolean(
      fields,
      "normalizeAccessKeys",
      false,
    ),
    no_link: optionalBoolean(fields, "noLink", false),
    checkbox_label: checkboxLabel,
    checkbox_checked:
      initialCheckbox === undefined
        ? null
        : optionalBoolean(fields, "checkboxChecked", false),
  };
  // Use the single generated native codec to qualify the full composed
  // options envelope, rather than copying its Postcard layout/size formula.
  try {
    encodeDialogRequest({
      variant: "Message",
      nonce: 1n,
      parent: null,
      options,
    });
  } catch {
    invalid("native Message envelope exceeds its bounds");
  }
  return Object.freeze({ ...options, buttons: Object.freeze([...buttons]) });
}

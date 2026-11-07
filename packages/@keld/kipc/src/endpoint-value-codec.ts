/**
 * GH544 endpoint values, one browser-safe implementation for the main Bun
 * process and the isolated renderer runtime. Rust forwards these bytes opaquely.
 *
 * The enclosing GH540 message owns the version, document, principal and grants.
 * This module owns only a bounded, deterministic value snapshot and parser.
 * No Node, Bun, IPC, function proxy, JSON or ambient authority is involved.
 * Depth is16 with root1; node/property visits are independently bounded.
 * Engine allocation inside Reflect.ownKeys still needs the GH544 high-water
 * qualification; post-return codec limits do not prove an engine memory bound.
 */

export const ENDPOINT_VALUE_LIMIT = 4_096;
export const ENDPOINT_VALUE_DEPTH = 16;
export const ENDPOINT_VALUE_NODES = 256;

export type CompatCodecErrorKind =
  "UnsupportedValue" | "MalformedValue" | "CodecLimit";

const CODE: Record<CompatCodecErrorKind, string> = {
  UnsupportedValue: "KELD-COMPAT-100",
  MalformedValue: "KELD-COMPAT-101",
  CodecLimit: "KELD-COMPAT-102",
};

/** Local, synchronous refusal. It is not a Host-dispatch certainty report. */
export class CompatCodecError extends Error {
  readonly code: string;
  readonly kind: CompatCodecErrorKind;

  constructor(kind: CompatCodecErrorKind, reason: string) {
    super(
      `${CODE[kind]}: ${kind}: ${reason}. Fix the endpoint value before sending it.`,
    );
    this.name = "CompatCodecError";
    this.kind = kind;
    this.code = CODE[kind];
  }
}

function reject(kind: CompatCodecErrorKind, reason: string): never {
  throw new CompatCodecError(kind, reason);
}

// A Proxy descriptor/ownKeys trap can mutate globals during this very call.
// Capture before any value is inspected; invoke saved methods with saved apply,
// never through a subsequently mutable prototype, .call or iterator.
const apply = Reflect.apply;
const ownKeys = Reflect.ownKeys;
const ownDescriptor = Object.getOwnPropertyDescriptor;
const hasOwn = Object.hasOwn;
const defineProperty = Object.defineProperty;
const hasInstance = Function.prototype[Symbol.hasInstance];
const NativeArrayBuffer = ArrayBuffer;
const NativeDataView = DataView;
const NativeUint8Array = Uint8Array;
const NativeWeakSet = WeakSet;
const NativeDate = Date;
const NativeError = Error;
const toNumber = Number;
const toString = String;
const toBigInt = BigInt;
const isArray = Array.isArray;
const isView = ArrayBuffer.isView;
const isFiniteNumber = Number.isFinite;
const isInteger = Number.isInteger;
const isSafeInteger = Number.isSafeInteger;
const arrayPush = Array.prototype.push;
const arraySort = Array.prototype.sort;
const weakSetHas = WeakSet.prototype.has;
const weakSetAdd = WeakSet.prototype.add;
const charCodeAt = String.prototype.charCodeAt;
const regexpExec = RegExp.prototype.exec;
const dateTime = Date.prototype.getTime;
const getUint16 = DataView.prototype.getUint16;
const getFloat64 = DataView.prototype.getFloat64;
const getBigInt64 = DataView.prototype.getBigInt64;
const setFloat64 = DataView.prototype.setFloat64;
const setBigInt64 = DataView.prototype.setBigInt64;
const byteSet = Uint8Array.prototype.set;
const typedArrayPrototype = Object.getPrototypeOf(Uint8Array.prototype);
const byteLength = ownDescriptor(typedArrayPrototype, "byteLength")!.get!;
const byteOffset = ownDescriptor(typedArrayPrototype, "byteOffset")!.get!;
const byteBuffer = ownDescriptor(typedArrayPrototype, "buffer")!.get!;
const encodeText = TextEncoder.prototype.encode;
const decodeText = TextDecoder.prototype.decode;
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
const arrayIndex = /^(0|[1-9][0-9]*)$/;
const exoticConstructors: Function[] = [
  ArrayBuffer,
  Map,
  Set,
  WeakMap,
  WeakSet,
  RegExp,
  Promise,
  ...(typeof SharedArrayBuffer === "undefined" ? [] : [SharedArrayBuffer]),
  ...(typeof URL === "undefined" ? [] : [URL]),
  ...(typeof URLSearchParams === "undefined" ? [] : [URLSearchParams]),
  ...(typeof Blob === "undefined" ? [] : [Blob]),
];

function byteSize(value: Uint8Array): number {
  return apply(byteLength, value, []);
}

function isInstance(value: unknown, constructor: Function): boolean {
  return apply(hasInstance, constructor, [value]);
}

function forbiddenKey(key: string): boolean {
  return key === "__proto__" || key === "constructor" || key === "prototype";
}

/** Proxy traps may execute, but a thrown trap never escapes as an untyped error. */
function checkedOwnKeys(value: object): (string | symbol)[] {
  try {
    return ownKeys(value);
  } catch {
    return reject("MalformedValue", "ownKeys trap refused the value");
  }
}

function checkedDescriptor(
  value: object,
  key: PropertyKey,
): PropertyDescriptor | undefined {
  try {
    return ownDescriptor(value, key);
  } catch {
    return reject(
      "MalformedValue",
      "getOwnPropertyDescriptor trap refused the value",
    );
  }
}

/**
 * Avoid Object.prototype.toString, which invokes application-defined
 * Symbol.toStringTag getters on ordinary objects. Native brands known to have
 * host state or callable behavior refuse before flattening own data fields.
 */
function knownExotic(value: object): boolean {
  if (isView(value)) return true;
  for (let i = 0; i < exoticConstructors.length; i++)
    if (isInstance(value, exoticConstructors[i])) return true;
  return false;
}

/** Refuse invalid UTF-16 rather than allowing TextEncoder to repair it. */
function scalarString(value: string): void {
  if (value.length > ENDPOINT_VALUE_LIMIT)
    reject("CodecLimit", "string exceeds the bounded UTF-8 domain");
  for (let i = 0; i < value.length; i++) {
    const unit = apply(charCodeAt, value, [i]);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      if (++i >= value.length)
        reject("MalformedValue", "unpaired high surrogate");
      const low = apply(charCodeAt, value, [i]);
      if (low < 0xdc00 || low > 0xdfff)
        reject("MalformedValue", "unpaired high surrogate");
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      reject("MalformedValue", "unpaired low surrogate");
    }
  }
}

function utf8(value: string): Uint8Array {
  scalarString(value);
  const bytes = apply(encodeText, encoder, [value]);
  if (byteSize(bytes) > ENDPOINT_VALUE_LIMIT)
    reject("CodecLimit", "encoded string exceeds 4096 bytes");
  return bytes;
}

function keyOrder(left: Uint8Array, right: Uint8Array): number {
  const leftSize = byteSize(left);
  const rightSize = byteSize(right);
  const count = leftSize < rightSize ? leftSize : rightSize;
  for (let i = 0; i < count; i++)
    if (left[i] !== right[i]) return left[i] - right[i];
  return leftSize - rightSize;
}

class ValueWriter {
  readonly bytes: number[] = [];
  private readonly seen = new NativeWeakSet<object>();
  private nodes = 0;
  private visits = 0;

  private append(...bytes: number[]): void {
    if (bytes.length > ENDPOINT_VALUE_LIMIT - this.bytes.length)
      reject("CodecLimit", "encoded value exceeds 4096 bytes");
    apply(arrayPush, this.bytes, bytes);
  }

  private buffer(value: Uint8Array): void {
    if (byteSize(value) > ENDPOINT_VALUE_LIMIT - this.bytes.length)
      reject("CodecLimit", "encoded value exceeds 4096 bytes");
    for (let i = 0; i < byteSize(value); i++)
      apply(arrayPush, this.bytes, [value[i]]);
  }

  private u16(value: number): void {
    if (!isInteger(value) || value < 0 || value > 0xffff)
      reject("CodecLimit", "u16 value count or byte length exceeded");
    this.append(value & 0xff, value >>> 8);
  }

  private text(value: string): void {
    const bytes = utf8(value);
    this.u16(byteSize(bytes));
    this.buffer(bytes);
  }

  private f64(value: number): void {
    const storage = new NativeArrayBuffer(8);
    apply(setFloat64, new NativeDataView(storage), [0, value, true]);
    this.buffer(new NativeUint8Array(storage));
  }

  private i64(value: number): void {
    const storage = new NativeArrayBuffer(8);
    apply(setBigInt64, new NativeDataView(storage), [0, toBigInt(value), true]);
    this.buffer(new NativeUint8Array(storage));
  }

  private visitObject(value: object): void {
    if (apply(weakSetHas, this.seen, [value]))
      reject("MalformedValue", "cycle or shared object identity");
    apply(weakSetAdd, this.seen, [value]);
  }

  private errorField(
    value: object,
    key: "name" | "message" | "stack",
  ): string | undefined {
    if (++this.visits > ENDPOINT_VALUE_NODES)
      reject(
        "CodecLimit",
        "Error property probes exceed aggregate visit budget",
      );
    const descriptor = checkedDescriptor(value, key);
    if (descriptor === undefined) return undefined;
    if (!hasOwn(descriptor, "value") || typeof descriptor.value !== "string")
      reject(
        "MalformedValue",
        `Error ${key} must be an own string data property`,
      );
    return descriptor.value;
  }

  visit(value: unknown, depth = 1): void {
    if (depth > ENDPOINT_VALUE_DEPTH)
      reject("CodecLimit", "maximum nesting depth is 16");
    if (++this.visits > ENDPOINT_VALUE_NODES)
      reject("CodecLimit", "maximum aggregate value/property visits is 256");
    if (++this.nodes > ENDPOINT_VALUE_NODES)
      reject("CodecLimit", "maximum value node count is 256");

    if (value === undefined) return this.append(0);
    if (value === null) return this.append(1);
    if (value === false) return this.append(2);
    if (value === true) return this.append(3);
    if (typeof value === "number") {
      if (!isFiniteNumber(value)) reject("MalformedValue", "non-finite number");
      this.append(4);
      return this.f64(value);
    }
    if (typeof value === "string") {
      this.append(5);
      return this.text(value);
    }
    if (typeof value !== "object")
      reject("UnsupportedValue", `unsupported ${typeof value} value`);

    this.visitObject(value);

    if (isArray(value)) {
      this.append(6);
      const lengthDescriptor = checkedDescriptor(value, "length");
      if (
        lengthDescriptor === undefined ||
        !hasOwn(lengthDescriptor, "value") ||
        typeof lengthDescriptor.value !== "number" ||
        !isSafeInteger(lengthDescriptor.value) ||
        lengthDescriptor.value < 0
      )
        reject(
          "MalformedValue",
          "array length is not a bounded own data property",
        );
      const length: number = lengthDescriptor.value;
      if (
        length > ENDPOINT_VALUE_NODES - this.nodes ||
        length > ENDPOINT_VALUE_NODES - this.visits
      )
        reject("CodecLimit", "array count exceeds remaining node budget");
      const keys = checkedOwnKeys(value);
      if (keys.length > ENDPOINT_VALUE_NODES + 1)
        reject("CodecLimit", "array own keys exceed the bounded domain");
      for (let keyIndex = 0; keyIndex < keys.length; keyIndex++) {
        const key = keys[keyIndex];
        if (typeof key === "symbol")
          reject("MalformedValue", "symbol-keyed array property");
        if (key === "length") continue;
        const descriptor = checkedDescriptor(value, key);
        if (descriptor === undefined)
          reject("MalformedValue", "array key vanished during snapshot");
        if (
          descriptor.enumerable &&
          (apply(regexpExec, arrayIndex, [key]) === null ||
            !isSafeInteger(toNumber(key)) ||
            toNumber(key) >= length)
        )
          reject("MalformedValue", "extra enumerable array property");
      }
      this.u16(length);
      for (let i = 0; i < length; i++) {
        const item = checkedDescriptor(value, toString(i));
        if (item === undefined || !hasOwn(item, "value"))
          reject("MalformedValue", "sparse or accessor array entry");
        this.visit(item.value, depth + 1);
      }
      return;
    }

    if (isInstance(value, NativeDate)) {
      const timestamp = apply(dateTime, value, []);
      if (!isFiniteNumber(timestamp) || !isInteger(timestamp))
        reject("MalformedValue", "invalid Date");
      this.append(8);
      this.i64(timestamp);
      return;
    }
    if (isInstance(value, NativeError)) {
      const name = this.errorField(value, "name") ?? "Error";
      const message = this.errorField(value, "message") ?? "";
      const stack = this.errorField(value, "stack");
      this.append(9);
      this.text(name);
      this.text(message);
      this.append(stack === undefined ? 0 : 1);
      if (stack !== undefined) this.text(stack);
      return;
    }

    // Class instances with own enumerable string data fields flatten to
    // ordinary records. Never invoke Symbol.toStringTag or toJSON as a probe.
    if (knownExotic(value))
      reject("UnsupportedValue", "unsupported exotic object");

    const keys = checkedOwnKeys(value);
    if (keys.length > ENDPOINT_VALUE_NODES - this.visits)
      reject(
        "CodecLimit",
        "object property keys exceed aggregate visit budget",
      );
    this.visits += keys.length;
    const entries: { bytes: Uint8Array; value: unknown }[] = [];
    // Reserve the object tag/count and each key length plus its value tag.
    // A key may use 4096 bytes alone; never retain hundreds of those keys
    // before the later writer detects that the complete value cannot fit.
    let minimumBytes = this.bytes.length + 3;
    if (minimumBytes > ENDPOINT_VALUE_LIMIT)
      reject("CodecLimit", "object header exceeds the remaining byte budget");
    for (let keyIndex = 0; keyIndex < keys.length; keyIndex++) {
      const key = keys[keyIndex];
      if (typeof key === "symbol")
        reject("MalformedValue", "symbol-keyed data property");
      if (forbiddenKey(key))
        reject("MalformedValue", "reserved prototype property");
      const descriptor = checkedDescriptor(value, key);
      if (descriptor === undefined)
        reject("MalformedValue", "object key vanished during snapshot");
      if (!descriptor.enumerable) continue;
      if (!hasOwn(descriptor, "value"))
        reject("MalformedValue", "enumerable accessor is unsupported");
      if (
        entries.length >= ENDPOINT_VALUE_NODES - this.nodes ||
        entries.length >= ENDPOINT_VALUE_NODES - this.visits
      )
        reject("CodecLimit", "object count exceeds remaining node budget");
      const keyBytes = utf8(key);
      const keyAndValueMinimum = 2 + byteSize(keyBytes) + 1;
      if (keyAndValueMinimum > ENDPOINT_VALUE_LIMIT - minimumBytes)
        reject("CodecLimit", "object keys exceed the remaining byte budget");
      minimumBytes += keyAndValueMinimum;
      apply(arrayPush, entries, [{ bytes: keyBytes, value: descriptor.value }]);
    }
    apply(arraySort, entries, [
      (a: { bytes: Uint8Array }, b: { bytes: Uint8Array }) =>
        keyOrder(a.bytes, b.bytes),
    ]);
    this.append(7);
    this.u16(entries.length);
    for (let index = 0; index < entries.length; index++) {
      const entry = entries[index];
      this.u16(byteSize(entry.bytes));
      this.buffer(entry.bytes);
      this.visit(entry.value, depth + 1);
    }
  }
}

/** Snapshot one supported value before sending it over the authenticated route. */
export function encodeEndpointValue(value: unknown): Uint8Array {
  try {
    const writer = new ValueWriter();
    writer.visit(value);
    const result = new NativeUint8Array(writer.bytes.length);
    for (let i = 0; i < writer.bytes.length; i++) result[i] = writer.bytes[i];
    return result;
  } catch (error) {
    if (isInstance(error, CompatCodecError)) throw error;
    return reject(
      "MalformedValue",
      "proxy trap or exotic brand operation failed",
    );
  }
}

class ValueReader {
  private at = 0;
  private nodes = 0;
  private visits = 0;
  private readonly view: DataView;

  constructor(private readonly bytes: Uint8Array) {
    const length: number = apply(byteLength, bytes, []);
    if (length > ENDPOINT_VALUE_LIMIT)
      reject("CodecLimit", "encoded value exceeds 4096 bytes");
    this.view = new NativeDataView(
      apply(byteBuffer, bytes, []),
      apply(byteOffset, bytes, []),
      length,
    );
  }

  private require(count: number): number {
    if (
      !isSafeInteger(count) ||
      count < 0 ||
      count > byteSize(this.bytes) - this.at
    )
      reject("MalformedValue", "truncated value payload");
    const previous = this.at;
    this.at += count;
    return previous;
  }

  private u8(): number {
    return this.bytes[this.require(1)];
  }

  private u16(): number {
    return apply(getUint16, this.view, [this.require(2), true]);
  }

  private text(): { value: string; raw: Uint8Array } {
    const length = this.u16();
    const start = this.require(length);
    const raw = new NativeUint8Array(length);
    for (let i = 0; i < length; i++) raw[i] = this.bytes[start + i];
    let value: string;
    try {
      value = apply(decodeText, decoder, [raw]);
    } catch {
      return reject("MalformedValue", "invalid UTF-8 string");
    }
    // TextDecoder's fatal decoder must yield Unicode scalar values.
    scalarString(value);
    return { value, raw };
  }

  private node(depth: number): unknown {
    if (depth > ENDPOINT_VALUE_DEPTH)
      reject("CodecLimit", "maximum nesting depth is 16");
    if (++this.visits > ENDPOINT_VALUE_NODES)
      reject("CodecLimit", "maximum aggregate value/property visits is 256");
    if (++this.nodes > ENDPOINT_VALUE_NODES)
      reject("CodecLimit", "maximum value node count is 256");
    const tag = this.u8();
    switch (tag) {
      case 0:
        return undefined;
      case 1:
        return null;
      case 2:
        return false;
      case 3:
        return true;
      case 4: {
        const number = apply(getFloat64, this.view, [this.require(8), true]);
        if (!isFiniteNumber(number))
          reject("MalformedValue", "non-finite number");
        return number;
      }
      case 5:
        return this.text().value;
      case 6: {
        const count = this.u16();
        if (
          count > ENDPOINT_VALUE_NODES - this.nodes ||
          count > ENDPOINT_VALUE_NODES - this.visits
        )
          reject(
            "CodecLimit",
            "array count exceeds remaining node/visit budget",
          );
        const array: unknown[] = [];
        for (let i = 0; i < count; i++)
          apply(arrayPush, array, [this.node(depth + 1)]);
        return array;
      }
      case 7: {
        const count = this.u16();
        if (
          count > ENDPOINT_VALUE_NODES - this.nodes ||
          count * 2 > ENDPOINT_VALUE_NODES - this.visits
        )
          reject(
            "CodecLimit",
            "object count exceeds remaining node/property budget",
          );
        this.visits += count;
        const object: Record<string, unknown> = {};
        let previous: Uint8Array | undefined;
        for (let i = 0; i < count; i++) {
          const { value: key, raw } = this.text();
          if (forbiddenKey(key))
            reject("MalformedValue", "reserved prototype property");
          if (previous !== undefined && keyOrder(previous, raw) >= 0)
            reject("MalformedValue", "duplicate or noncanonical object keys");
          previous = raw;
          defineProperty(object, key, {
            value: this.node(depth + 1),
            configurable: true,
            enumerable: true,
            writable: true,
          });
        }
        return object;
      }
      case 8: {
        const epoch = apply(getBigInt64, this.view, [this.require(8), true]);
        const timestamp = toNumber(epoch);
        if (
          !isSafeInteger(timestamp) ||
          timestamp < -8_640_000_000_000_000 ||
          timestamp > 8_640_000_000_000_000
        )
          reject("MalformedValue", "invalid Date timestamp");
        return new NativeDate(timestamp);
      }
      case 9: {
        if (this.visits + 3 > ENDPOINT_VALUE_NODES)
          reject("CodecLimit", "Error fields exceed aggregate visit budget");
        this.visits += 3;
        const name = this.text().value;
        const message = this.text().value;
        const hasStack = this.u8();
        if (hasStack !== 0 && hasStack !== 1)
          reject("MalformedValue", "invalid Error stack marker");
        const stack = hasStack === 1 ? this.text().value : undefined;
        const error = new NativeError(message);
        defineProperty(error, "name", {
          value: name,
          configurable: true,
          writable: true,
        });
        if (stack === undefined) delete error.stack;
        else
          defineProperty(error, "stack", {
            value: stack,
            configurable: true,
            writable: true,
          });
        return error;
      }
      default:
        return reject("MalformedValue", "unknown value tag");
    }
  }

  decode(): unknown {
    const value = this.node(1);
    if (this.at !== byteSize(this.bytes))
      reject("MalformedValue", "trailing bytes after endpoint value");
    return value;
  }
}

/** Decode one complete owned byte value; never accepts a JSON substitute. */
export function decodeEndpointValue(bytes: Uint8Array): unknown {
  try {
    if (!isInstance(bytes, NativeUint8Array))
      reject("MalformedValue", "endpoint payload must be bytes");
    const length: number = apply(byteLength, bytes, []);
    if (length > ENDPOINT_VALUE_LIMIT)
      reject("CodecLimit", "encoded value exceeds 4096 bytes");
    const owned = new NativeUint8Array(length);
    apply(byteSet, owned, [bytes]);
    return new ValueReader(owned).decode();
  } catch (error) {
    if (isInstance(error, CompatCodecError)) throw error;
    return reject(
      "MalformedValue",
      "endpoint payload is not a readable byte snapshot",
    );
  }
}

/** Apply the IPC caller-vector count without restricting generic result arrays. */
export function encodeEndpointArguments(args: readonly unknown[]): Uint8Array {
  if (!isArray(args))
    return reject("MalformedValue", "IPC arguments must be an array");
  const descriptor = checkedDescriptor(args, "length");
  if (
    !descriptor ||
    !hasOwn(descriptor, "value") ||
    !isSafeInteger(descriptor.value) ||
    descriptor.value < 0
  )
    return reject("MalformedValue", "IPC arguments need a bounded own length");
  if (descriptor.value > 16)
    return reject(
      "CodecLimit",
      "IPC argument vector exceeds sixteen arguments",
    );
  return encodeEndpointValue(args);
}
/** Apply the same caller-vector count after the one owned endpoint decoder. */
export function decodeEndpointArguments(bytes: Uint8Array): unknown[] {
  const value = decodeEndpointValue(bytes);
  if (!isArray(value))
    return reject("MalformedValue", "IPC arguments must decode to an array");
  if (value.length > 16)
    return reject(
      "CodecLimit",
      "IPC argument vector exceeds sixteen arguments",
    );
  return value;
}

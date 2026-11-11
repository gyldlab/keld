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
 *
 * Hostile-input rules proven by `endpoint-hostile-input.test.ts`:
 * - wire bytes and accumulator entries never write through `[[Set]]` on a
 *   normal array, because an inherited indexed setter planted by a caller trap
 *   would intercept even a captured `Array.prototype.push`;
 * - the IPC argument count is enforced against the same array snapshot the
 *   writer serializes, never against an earlier separate read;
 * - a value whose native prototype was replaced still refuses, because native
 *   brands are probed by applying a built-in that requires the internal slot;
 * - codec refusals are identified by module-owned identity, so a forged
 *   `CompatCodecError.prototype` or a hostile `getPrototypeOf` trap cannot
 *   escape as an uncoded error.
 * Residual, by construction: a Proxy reports its target's own keys and hides
 * its internal slots, so a proxied native can only be reduced to its own data
 * fields; the wire stays valid and no copy of the hidden state is claimed.
 * `RegExp`, `Promise`, `Error`, `Blob` and `URL` keep only the prototype-chain
 * check, because no side-effect-free brand probe exists for them.
 */

export const ENDPOINT_VALUE_LIMIT = 4_096;
export const ENDPOINT_VALUE_DEPTH = 16;
export const ENDPOINT_VALUE_NODES = 256;
/**
 * Array own-key and descriptor inspections are the work a proxy trap can
 * amplify without growing the wire: 80 one-element arrays with 250
 * non-enumerable own keys each fit in 323 bytes yet cost 20,240 probes. They
 * are charged to this explicit internal budget, which leaves the reviewed
 * node/visit boundary and its 255/256 test untouched.
 */
const ENDPOINT_VALUE_PROBES = ENDPOINT_VALUE_NODES * 4;

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
    // A caller trap can poison Error.prototype.name before normalization.
    defineProperty(this, "name", {
      value: "CompatCodecError",
      configurable: true,
      enumerable: true,
      writable: true,
    });
    this.kind = kind;
    this.code = CODE[kind];
  }
}

// A Proxy descriptor/ownKeys trap can mutate globals during this very call.
// Capture before any value is inspected; invoke saved methods with saved apply,
// never through a subsequently mutable prototype, .call or iterator.
const apply = Reflect.apply;
const ownKeys = Reflect.ownKeys;
const ownDescriptor = Object.getOwnPropertyDescriptor;
const hasOwn = Object.hasOwn;
const defineProperty = Object.defineProperty;
const setProto = Object.setPrototypeOf;
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
const weakMapHas = WeakMap.prototype.has;
const mapHas = Map.prototype.has;
const setHas = Set.prototype.has;
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
const arrayBufferByteLength = ownDescriptor(
  ArrayBuffer.prototype,
  "byteLength",
)!.get!;
const sharedArrayBufferByteLength =
  typeof SharedArrayBuffer === "undefined"
    ? undefined
    : ownDescriptor(SharedArrayBuffer.prototype, "byteLength")!.get!;
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

/**
 * Genuine refusals are recognised by module-owned identity. A prototype probe
 * is not proof: a caller can graft `CompatCodecError.prototype` onto any
 * thrown value, and a thrown Proxy can run a hostile `getPrototypeOf` trap
 * inside the classifier itself.
 */
const ownErrors = new NativeWeakSet<object>();

function reject(kind: CompatCodecErrorKind, reason: string): never {
  const error = new CompatCodecError(kind, reason);
  apply(weakSetAdd, ownErrors, [error]);
  throw error;
}

function isOwnError(value: unknown): boolean {
  return (
    typeof value === "object" &&
    value !== null &&
    apply(weakSetHas, ownErrors, [value])
  );
}

/** Normalize every non-codec throw from a hostile trap into one typed refusal. */
function guard<T>(run: () => T, reason: string): T {
  try {
    return run();
  } catch (error) {
    if (isOwnError(error)) throw error;
    return reject("MalformedValue", reason);
  }
}

/**
 * A normal array is written through `[[Set]]`, which walks the prototype
 * chain, so an indexed setter planted on `Array.prototype` by a caller trap
 * intercepts even a captured `Array.prototype.push`. Accumulators are created
 * without that chain.
 */
function detachedArray<T>(): T[] {
  const array: T[] = [];
  // Direct call, not Reflect.apply: Bun 1.4.4 rejects the builtin when it is
  // forwarded that way ("Cannot set prototype of undefined or null").
  setProto(array, null);
  return array;
}

/**
 * Native brands are verified by applying a built-in that requires the internal
 * slot, not by walking the prototype chain, so a `Map`, `Set`, `ArrayBuffer`,
 * `SharedArrayBuffer`, `WeakMap`, `WeakSet` or `Date` whose prototype was
 * replaced still refuses instead of flattening to an empty record.
 *
 * Every probe below is spec-guaranteed to reject a receiver without the slot.
 * Deliberately absent: `RegExp.prototype.source` (spec-legal to answer "(?:)"
 * for any object, which would refuse every record on such an engine),
 * `RegExp.prototype.exec`/`Promise.prototype.then` (side effects on the probed
 * value). Those brands keep only the prototype-chain check.
 */
const sharedBufferByteLength = sharedArrayBufferByteLength;
const brandProbes: ((value: object) => unknown)[] = [
  (value) => apply(dateTime, value, []),
  (value) => apply(mapHas, value, [undefined]),
  (value) => apply(setHas, value, [undefined]),
  (value) => apply(weakMapHas, value, [{}]),
  (value) => apply(weakSetHas, value, [{}]),
  (value) => apply(arrayBufferByteLength, value, []),
  ...(sharedBufferByteLength === undefined
    ? []
    : [(value: object) => apply(sharedBufferByteLength, value, [])]),
];

function hasNativeBrand(value: object): boolean {
  for (let index = 0; index < brandProbes.length; index++) {
    try {
      brandProbes[index](value);
      return true;
    } catch {
      continue;
    }
  }
  return false;
}

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
  /**
   * Wire bytes live in a typed array: integer-indexed writes there are own
   * slot stores, so a planted `Array.prototype` setter cannot redirect them.
   * The explicit length keeps allocation independent of caller state.
   */
  private readonly storage = new NativeUint8Array(ENDPOINT_VALUE_LIMIT);
  private length = 0;
  private readonly seen = new NativeWeakSet<object>();
  private nodes = 0;
  private visits = 0;
  private probes = 0;

  constructor(private readonly rootArrayLimit = ENDPOINT_VALUE_NODES) {}

  private chargeProbes(count: number): void {
    this.probes += count;
    if (this.probes > ENDPOINT_VALUE_PROBES)
      reject(
        "CodecLimit",
        "aggregate array key probes exceed the probe budget",
      );
  }

  private reserve(count: number): number {
    if (count > ENDPOINT_VALUE_LIMIT - this.length)
      reject("CodecLimit", "encoded value exceeds 4096 bytes");
    const at = this.length;
    this.length += count;
    return at;
  }

  private append(...bytes: number[]): void {
    const at = this.reserve(bytes.length);
    for (let index = 0; index < bytes.length; index++)
      this.storage[at + index] = bytes[index];
  }

  private buffer(value: Uint8Array): void {
    const size = byteSize(value);
    const at = this.reserve(size);
    for (let index = 0; index < size; index++)
      this.storage[at + index] = value[index];
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

  finish(): Uint8Array {
    const result = new NativeUint8Array(this.length);
    // subarray consults mutable constructor/@@species even when captured.
    // Copy private indexed bytes without invoking a caller-controlled hook.
    for (let index = 0; index < this.length; index++)
      result[index] = this.storage[index];
    return result;
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
      // One read authorizes the wire: an earlier separate count check could be
      // answered with 16 while this snapshot answers with 17.
      if (depth === 1 && length > this.rootArrayLimit)
        reject("CodecLimit", "IPC argument vector exceeds sixteen arguments");
      if (length > ENDPOINT_VALUE_NODES - this.nodes)
        reject("CodecLimit", "array count exceeds remaining node budget");
      const keys = checkedOwnKeys(value);
      if (keys.length > ENDPOINT_VALUE_NODES + 1)
        reject("CodecLimit", "array own keys exceed the bounded domain");
      this.chargeProbes(1 + keys.length);
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
    // The internal-slot probes catch a native whose prototype was replaced and
    // which the chain check above therefore cannot see.
    if (knownExotic(value) || hasNativeBrand(value))
      reject("UnsupportedValue", "unsupported exotic object");

    const keys = checkedOwnKeys(value);
    if (keys.length > ENDPOINT_VALUE_NODES - this.visits)
      reject(
        "CodecLimit",
        "object property keys exceed aggregate visit budget",
      );
    this.visits += keys.length;
    const entries = detachedArray<{ bytes: Uint8Array; value: unknown }>();
    // Reserve the object tag/count and each key length plus its value tag.
    // A key may use 4096 bytes alone; never retain hundreds of those keys
    // before the later writer detects that the complete value cannot fit.
    let minimumBytes = this.length + 3;
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
  return guard(() => {
    const writer = new ValueWriter();
    writer.visit(value);
    return writer.finish();
  }, "proxy trap or exotic brand operation failed");
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
        // Define, do not push: an inherited indexed setter would otherwise
        // swallow the element while the array still reports its length.
        for (let i = 0; i < count; i++)
          defineProperty(array, i, {
            value: this.node(depth + 1),
            configurable: true,
            enumerable: true,
            writable: true,
          });
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
  return guard(() => {
    if (!isInstance(bytes, NativeUint8Array))
      reject("MalformedValue", "endpoint payload must be bytes");
    const length: number = apply(byteLength, bytes, []);
    if (length > ENDPOINT_VALUE_LIMIT)
      reject("CodecLimit", "encoded value exceeds 4096 bytes");
    const owned = new NativeUint8Array(length);
    apply(byteSet, owned, [bytes]);
    return new ValueReader(owned).decode();
  }, "endpoint payload is not a readable byte snapshot");
}

/** Apply the IPC caller-vector count without restricting generic result arrays. */
export function encodeEndpointArguments(args: readonly unknown[]): Uint8Array {
  return guard(() => {
    // Array.isArray throws for a revoked proxy; the guard keeps that a typed
    // refusal. No separate length check follows: the count is enforced on the
    // same snapshot the writer serializes, so a drifting proxy length cannot
    // authorize a seventeenth argument.
    if (!isArray(args))
      return reject("MalformedValue", "IPC arguments must be an array");
    const writer = new ValueWriter(16);
    writer.visit(args);
    return writer.finish();
  }, "IPC argument vector is not a readable array snapshot");
}
/** Apply the same caller-vector count after the one owned endpoint decoder. */
export function decodeEndpointArguments(bytes: Uint8Array): unknown[] {
  return guard(() => {
    const value = decodeEndpointValue(bytes);
    if (!isArray(value))
      return reject("MalformedValue", "IPC arguments must decode to an array");
    if (value.length > 16)
      return reject(
        "CodecLimit",
        "IPC argument vector exceeds sixteen arguments",
      );
    return value;
  }, "IPC argument vector is not a readable array snapshot");
}

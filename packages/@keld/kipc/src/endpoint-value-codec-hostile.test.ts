import { expect, test } from "bun:test";
import {
  CompatCodecError,
  decodeEndpointArguments,
  decodeEndpointValue,
  encodeEndpointArguments,
  encodeEndpointValue,
} from "./endpoint-value-codec.ts";

function caught(action: () => unknown): CompatCodecError {
  try {
    action();
  } catch (error) {
    expect(error).toBeInstanceOf(CompatCodecError);
    return error as CompatCodecError;
  }
  throw new Error("expected codec refusal");
}

test("inherited numeric setters cannot alter encoded bytes or decoded array slots", () => {
  const value = [undefined, false, { answer: 42 }, [null]];
  const expected = encodeEndpointValue(value);
  const previous = Object.getOwnPropertyDescriptor(Array.prototype, "0");
  Object.defineProperty(Array.prototype, "0", {
    configurable: true,
    set() {},
  });
  try {
    expect(encodeEndpointValue(value)).toEqual(expected);
    const decoded = decodeEndpointValue(expected) as unknown[];
    expect(Object.hasOwn(decoded, "0")).toBe(true);
    expect(Object.hasOwn(decoded, "1")).toBe(true);
    expect(decoded).toEqual(value);
  } finally {
    if (previous) Object.defineProperty(Array.prototype, "0", previous);
    else Reflect.deleteProperty(Array.prototype, "0");
  }

  const previousZero = Object.getOwnPropertyDescriptor(Array.prototype, "0");
  let trapSetterCalls = 0;
  const nestedValue = { nested: [undefined, false] };
  const expectedNested = encodeEndpointValue(nestedValue);
  const hostile = new Proxy(nestedValue, {
    ownKeys(target) {
      Object.defineProperty(Array.prototype, "0", {
        configurable: true,
        set() {
          trapSetterCalls += 1;
        },
      });
      return Reflect.ownKeys(target);
    },
  });
  try {
    const encoded = encodeEndpointValue(hostile);
    expect(encoded).toEqual(expectedNested);
    const decoded = decodeEndpointValue(encoded) as { nested: unknown[] };
    expect(Object.hasOwn(decoded.nested, "0")).toBe(true);
    expect(Object.hasOwn(decoded.nested, "1")).toBe(true);
    expect(decoded).toEqual(nestedValue);
    expect(trapSetterCalls).toBe(0);
  } finally {
    if (previousZero) Object.defineProperty(Array.prototype, "0", previousZero);
    else Reflect.deleteProperty(Array.prototype, "0");
  }
});

test("argument count is bounded from its emitted snapshot and rejects proxy growth", () => {
  const sixteen = Array.from({ length: 16 }, (_, index) => index);
  const seventeen = Array.from({ length: 17 }, (_, index) => index);
  const roundTrip = decodeEndpointArguments(encodeEndpointArguments(sixteen));
  expect(roundTrip).toEqual(sixteen);
  expect(() => encodeEndpointArguments(seventeen)).toThrow("KELD-COMPAT-102");

  // Oversize arguments must refuse before reading unsupported extra elements
  // or calling their ownKeys trap; the final wire count still protects TOCTOU.
  let ownKeysCalls = 0;
  const unreadExcess = new Proxy([...sixteen, Symbol("unread")], {
    ownKeys() {
      ownKeysCalls += 1;
      throw new Error("oversized arguments were inspected");
    },
  });
  expect(() => encodeEndpointArguments(unreadExcess)).toThrow(
    "KELD-COMPAT-102",
  );
  expect(ownKeysCalls).toBe(0);

  const growing = new Proxy(sixteen.slice(), {
    ownKeys(target) {
      target.push(16);
      return Reflect.ownKeys(target);
    },
  });
  expect(() => encodeEndpointArguments(growing)).toThrow("KELD-COMPAT-101");

  let lengthReads = 0;
  const changingLength = new Proxy(sixteen.slice(), {
    getOwnPropertyDescriptor(target, key) {
      const descriptor = Reflect.getOwnPropertyDescriptor(target, key);
      if (key === "length" && lengthReads++ === 0) target.push(16);
      return descriptor;
    },
  });
  // The newer snapshot classifier rejects the changed enumerable shape
  // as malformed before emitting a count; the extra entry never escapes.
  expect(() => encodeEndpointArguments(changingLength)).toThrow(
    "KELD-COMPAT-101",
  );
  expect(lengthReads).toBeGreaterThan(0);
});

test("native brands survive prototype erasure and supported Dates keep timestamps", () => {
  for (const value of [new Map([["k", 1]]), new Set([1]), new ArrayBuffer(4)]) {
    Object.setPrototypeOf(value, Object.prototype);
    expect(caught(() => encodeEndpointValue(value)).code).toBe(
      "KELD-COMPAT-100",
    );
  }
  for (const value of [new RegExp("secret"), new WeakMap(), new WeakSet()]) {
    Object.setPrototypeOf(value, null);
    expect(caught(() => encodeEndpointValue(value)).code).toBe(
      "KELD-COMPAT-100",
    );
  }
  // Error has no public slot probe after its prototype is replaced. A
  // data-only record must never leak its non-enumerable stack or message.
  const erasedError = new Error("failure");
  Object.setPrototypeOf(erasedError, Object.prototype);
  expect(decodeEndpointValue(encodeEndpointValue(erasedError))).toEqual({});

  const date = new Date("2024-02-03T04:05:06.789Z");
  const decoded = decodeEndpointValue(encodeEndpointValue(date));
  expect(decoded).toBeInstanceOf(Date);
  expect((decoded as Date).getTime()).toBe(1_706_933_106_789);
  // The native Date internal slot still authorizes the timestamp even
  // after the caller removes its prototype.
  Object.setPrototypeOf(date, null);
  const stripped = decodeEndpointValue(encodeEndpointValue(date));
  expect(stripped).toBeInstanceOf(Date);
  expect((stripped as Date).getTime()).toBe(1_706_933_106_789);
  const invalid = new Date(NaN);
  Object.setPrototypeOf(invalid, null);
  expect(caught(() => encodeEndpointValue(invalid)).code).toBe(
    "KELD-COMPAT-101",
  );

  const typedArrayProxy = new Proxy(new Uint8Array([1, 2]), {});
  expect(caught(() => encodeEndpointValue(typedArrayProxy)).code).toBe(
    "KELD-COMPAT-100",
  );
});

test("GH-544 class instances flatten own enumerable fields with exact wire bytes", () => {
  class Parent {
    get inheritedGetter(): never {
      throw new Error("must not invoke an inherited getter");
    }
  }
  class RecordValue extends Parent {
    label = "OK";
  }
  const instance = new RecordValue();
  Object.defineProperty(instance, "hidden", {
    value: 123,
    configurable: true,
  });
  Object.defineProperty(Parent.prototype, "extra", {
    value: "not own",
    enumerable: true,
    configurable: true,
  });
  // Record tag, one property, UTF-8 key "label", string tag and "OK".
  const golden = Uint8Array.from([
    7, 1, 0, 5, 0, 108, 97, 98, 101, 108, 5, 2, 0, 79, 75,
  ]);
  expect(encodeEndpointValue(instance)).toEqual(golden);
  expect(decodeEndpointValue(golden)).toEqual({ label: "OK" });
});

test("native Proxy wrappers cannot masquerade as supported records", () => {
  // The intrinsic %TypedArray% prototype check and the DataView constructor
  // exclude wrappers whose native slots are otherwise inaccessible.
  for (const value of [
    new Proxy(new Uint8Array([1, 2]), {}),
    new Proxy(new Int16Array([3]), {}),
    new Proxy(new DataView(new ArrayBuffer(4)), {}),
  ]) {
    expect(caught(() => encodeEndpointValue(value)).code).toBe(
      "KELD-COMPAT-100",
    );
  }
  expect(
    caught(() => encodeEndpointValue(new Proxy(new Date(1), {}))).code,
  ).toBe("KELD-COMPAT-100");
  const nativeValues: object[] = [
    new Map([["k", 1]]),
    new Set([1]),
    new ArrayBuffer(4),
    Promise.resolve(1),
    /k/,
  ];
  for (const nativeValue of nativeValues) {
    const error = caught(() => encodeEndpointValue(new Proxy(nativeValue, {})));
    expect(error.kind).toBe("UnsupportedValue");
    expect(error.code).toBe("KELD-COMPAT-100");
  }
});

test("forged tags are not invoked and symbol-tagged values are refused", () => {
  let getterCalls = 0;
  const tagged = {
    get [Symbol.toStringTag]() {
      getterCalls += 1;
      return "Map";
    },
  };
  expect(caught(() => encodeEndpointValue(tagged)).code).toBe(
    "KELD-COMPAT-101",
  );
  expect(getterCalls).toBe(0);
});

test("genuine codec refusals retain identity while forged and nested throws normalize", () => {
  const unsupported = caught(() => encodeEndpointValue(Symbol("unsupported")));
  expect(unsupported.kind).toBe("UnsupportedValue");
  expect(unsupported.code).toBe("KELD-COMPAT-100");

  const forged = Object.create(CompatCodecError.prototype) as Error;
  const forgedTrap = new Proxy(
    {},
    {
      getPrototypeOf() {
        throw forged;
      },
    },
  );
  const forgedFailure = caught(() => encodeEndpointValue(forgedTrap));
  expect(forgedFailure.kind).toBe("MalformedValue");
  expect(forgedFailure.code).toBe("KELD-COMPAT-101");

  // Even an actual exported CompatCodecError object constructed and thrown
  // by untrusted user code is not a refusal minted by the codec.
  const callerConstructed = new CompatCodecError(
    "CodecLimit",
    "untrusted caller-created refusal",
  );
  const forgedRealInstance = new Proxy(
    {},
    {
      getPrototypeOf() {
        throw callerConstructed;
      },
    },
  );
  const normalized = caught(() => encodeEndpointValue(forgedRealInstance));
  expect(normalized.kind).toBe("MalformedValue");
  expect(normalized.code).toBe("KELD-COMPAT-101");
  expect(normalized).not.toBe(callerConstructed);

  const cached = caught(() => encodeEndpointValue(Symbol("cached refusal")));
  expect(cached.code).toBe("KELD-COMPAT-100");
  const replayed = new Proxy(
    { value: 1 },
    {
      getOwnPropertyDescriptor() {
        throw cached;
      },
    },
  );
  const replayFailure = caught(() => encodeEndpointValue(replayed));
  expect(replayFailure.kind).toBe("MalformedValue");
  expect(replayFailure.code).toBe("KELD-COMPAT-101");
  expect(replayFailure).not.toBe(cached);
  // Native-prototype classification must also reject replayed old errors.
  // A previously minted Unsupported100 has no authority in this invocation.
  let prototypeCalls = 0;
  const replayedFromPrototype = new Proxy(
    {},
    {
      getPrototypeOf() {
        prototypeCalls += 1;
        throw cached;
      },
    },
  );
  const directReplay = caught(() => encodeEndpointValue(replayedFromPrototype));
  expect(prototypeCalls).toBeGreaterThan(0);
  expect(directReplay.kind).toBe("MalformedValue");
  expect(directReplay.code).toBe("KELD-COMPAT-101");
  expect(directReplay).not.toBe(cached);

  const nested = {
    child: new Proxy(
      {},
      {
        ownKeys() {
          throw new Error("trap");
        },
      },
    ),
  };
  const nestedFailure = caught(() => encodeEndpointValue(nested));
  expect(nestedFailure.kind).toBe("MalformedValue");
  expect(nestedFailure.code).toBe("KELD-COMPAT-101");
});

test("nested public codec calls cannot launder their refusals into a caller", () => {
  const seventeen = Array.from({ length: 17 }, () => null);
  const validSeventeenWire = encodeEndpointValue(seventeen);
  const nestedOperations = [
    () => encodeEndpointValue(Symbol("nested refusal")),
    () => encodeEndpointArguments(seventeen),
    () => decodeEndpointArguments(validSeventeenWire),
  ];
  for (const nestedOperation of nestedOperations) {
    let trapCalls = 0;
    const hostile = new Proxy(
      { value: 1 },
      {
        getOwnPropertyDescriptor(target, key) {
          trapCalls += 1;
          nestedOperation();
          return Reflect.getOwnPropertyDescriptor(target, key);
        },
      },
    );
    const error = caught(() => encodeEndpointValue(hostile));
    expect(trapCalls).toBeGreaterThan(0);
    expect(error.kind).toBe("MalformedValue");
    expect(error.code).toBe("KELD-COMPAT-101");
    let prototypeCalls = 0;
    const prototypeReentry = new Proxy(
      {},
      {
        getPrototypeOf() {
          prototypeCalls += 1;
          nestedOperation();
          return Object.prototype;
        },
      },
    );
    const nestedError = caught(() => encodeEndpointValue(prototypeReentry));
    expect(prototypeCalls).toBeGreaterThan(0);
    expect(nestedError.kind).toBe("MalformedValue");
    expect(nestedError.code).toBe("KELD-COMPAT-101");
  }
  expect(caught(() => encodeEndpointArguments(seventeen)).code).toBe(
    "KELD-COMPAT-102",
  );
  expect(caught(() => decodeEndpointArguments(validSeventeenWire)).code).toBe(
    "KELD-COMPAT-102",
  );
  // The private decoder's node-boundary rejection must not be remapped by
  // a second public guard into MalformedValue.
  expect(
    caught(() => decodeEndpointArguments(Uint8Array.from([6, 0, 1]))).code,
  ).toBe("KELD-COMPAT-102");
});

test("revoked proxy argument arrays produce genuine malformed-value errors", () => {
  const revocable = Proxy.revocable([], {});
  revocable.revoke();
  const error = caught(() => encodeEndpointArguments(revocable.proxy));
  expect(error.kind).toBe("MalformedValue");
  expect(error.code).toBe("KELD-COMPAT-101");
});

test("array probe budget rejects huge own-key sets before descriptor inspection", () => {
  let descriptorCalls = 0;
  const target = [1];
  const keys = [
    "0",
    "length",
    ...Array.from({ length: 30_000 }, (_, i) => `x${i}`),
  ];
  const hostile = new Proxy(target, {
    ownKeys() {
      return keys;
    },
    getOwnPropertyDescriptor(object, key) {
      descriptorCalls += 1;
      if (key === "length")
        return Reflect.getOwnPropertyDescriptor(object, key);
      return {
        configurable: true,
        enumerable: false,
        writable: true,
        value: 0,
      };
    },
  });
  const failure = caught(() => encodeEndpointValue(hostile));
  expect(failure.kind).toBe("CodecLimit");
  expect(failure.code).toBe("KELD-COMPAT-102");
  expect(descriptorCalls).toBe(1);
});

test("ordinary arrays use semantic node budgets independently of snapshot probes", () => {
  const eightyFive = Array<null>(85).fill(null);
  const wire = Uint8Array.from([6, 85, 0, ...Array<number>(85).fill(1)]);
  expect(wire.byteLength).toBe(88);
  expect(encodeEndpointValue(eightyFive)).toEqual(wire);
  expect(decodeEndpointValue(wire)).toEqual(eightyFive);

  // 255 children plus the root occupy all 256 semantic value nodes.
  const maximum = Array<null>(255).fill(null);
  expect(decodeEndpointValue(encodeEndpointValue(maximum))).toEqual(maximum);
  expect(caught(() => encodeEndpointValue([...maximum, null])).code).toBe(
    "KELD-COMPAT-102",
  );

  // A root with 255 distinct empty arrays is also 256 semantic nodes.
  // The separate probe cap must accommodate this normal shape.
  const forest = Array.from({ length: 255 }, () => [] as unknown[]);
  expect(decodeEndpointValue(encodeEndpointValue(forest))).toEqual(forest);

  const nested = { values: eightyFive };
  expect(decodeEndpointValue(encodeEndpointValue(nested))).toEqual(nested);
  class Payload {
    values = eightyFive;
    get inherited(): never {
      throw new Error("inherited getter must not run");
    }
  }
  expect(decodeEndpointValue(encodeEndpointValue(new Payload()))).toEqual(
    nested,
  );
});

test("oversized array Proxy refuses from its length before ownKeys", () => {
  let ownKeysCalls = 0;
  const oversized = new Proxy(Array<null>(512).fill(null), {
    ownKeys() {
      ownKeysCalls += 1;
      throw new Error("oversized array visited ownKeys");
    },
  });
  const error = caught(() => encodeEndpointValue(oversized));
  expect(error.kind).toBe("CodecLimit");
  expect(error.code).toBe("KELD-COMPAT-102");
  expect(ownKeysCalls).toBe(0);
});

test("private non-enumerable descriptor traps normalize without invoking getters", () => {
  let reads = 0;
  const target = [null];
  Object.defineProperty(target, "private", {
    configurable: true,
    enumerable: false,
    get() {
      reads += 1;
      throw new Error("private getter must not run");
    },
  });
  const hostile = new Proxy(target, {
    getOwnPropertyDescriptor(value, key) {
      if (key === "private") throw new Error("private descriptor denied");
      return Reflect.getOwnPropertyDescriptor(value, key);
    },
  });
  expect(caught(() => encodeEndpointValue(hostile)).code).toBe(
    "KELD-COMPAT-101",
  );
  expect(reads).toBe(0);
});

test("small arrays share an independent finite snapshot-probe budget", () => {
  let descriptorCalls = 0;
  const children = Array.from({ length: 100 }, () => {
    const item = [null];
    for (let index = 0; index < 10; index++)
      Object.defineProperty(item, `hidden${index}`, {
        value: index,
        configurable: true,
      });
    return new Proxy(item, {
      getOwnPropertyDescriptor(target, key) {
        descriptorCalls += 1;
        return Reflect.getOwnPropertyDescriptor(target, key);
      },
    });
  });
  const failure = caught(() => encodeEndpointValue(children));
  expect(failure.kind).toBe("CodecLimit");
  expect(failure.code).toBe("KELD-COMPAT-102");
  expect(descriptorCalls).toBeLessThan(2_048);
});

test("normal arrays and the 256 aggregate visit boundary remain accepted", () => {
  const boundary = Array.from({ length: 84 }, (_, index) => index);
  expect(decodeEndpointValue(encodeEndpointValue(boundary))).toEqual(boundary);
  expect(decodeEndpointValue(encodeEndpointValue([...boundary, 84]))).toEqual([
    ...boundary,
    84,
  ]);
});

test("hostile inherited descriptor get/set cannot escape typed refusal or corrupt decode", () => {
  const originalGet = Object.getOwnPropertyDescriptor(Object.prototype, "get");
  const originalSet = Object.getOwnPropertyDescriptor(Object.prototype, "set");
  const arrayWire = encodeEndpointValue([null, { active: true }]);
  const errorWire = encodeEndpointValue(new Error("bounded"));
  let trapCalls = 0;
  let thrown: unknown;
  let decodedArray: unknown;
  let decodedError: unknown;
  let decodeFailure: unknown;
  const dataDescriptor = (value: unknown): PropertyDescriptor => {
    const descriptor: PropertyDescriptor = Object.create(null);
    descriptor.value = value;
    descriptor.configurable = true;
    descriptor.writable = true;
    return descriptor;
  };
  const hostile = new Proxy(
    { active: true },
    {
      ownKeys() {
        trapCalls += 1;
        // These are ordinary data fields, intentionally named like descriptor
        // fields. ToPropertyDescriptor on an inherited-key object must not
        // observe them when the decoder defines its own output slots.
        Object.defineProperty(
          Object.prototype,
          "get",
          dataDescriptor(() => {}),
        );
        Object.defineProperty(
          Object.prototype,
          "set",
          dataDescriptor(() => {}),
        );
        throw 0;
      },
    },
  );
  try {
    try {
      encodeEndpointValue(hostile);
    } catch (error) {
      thrown = error;
    }
    try {
      decodedArray = decodeEndpointValue(arrayWire);
      decodedError = decodeEndpointValue(errorWire);
    } catch (error) {
      decodeFailure = error;
    }
  } finally {
    // Restore before assertions or Bun's own test/formatter internals run.
    Reflect.deleteProperty(Object.prototype, "get");
    Reflect.deleteProperty(Object.prototype, "set");
    if (originalGet)
      Object.defineProperty(Object.prototype, "get", originalGet);
    if (originalSet)
      Object.defineProperty(Object.prototype, "set", originalSet);
  }
  expect(trapCalls).toBe(1);
  expect(thrown).toBeInstanceOf(CompatCodecError);
  expect((thrown as CompatCodecError).kind).toBe("MalformedValue");
  expect((thrown as CompatCodecError).code).toBe("KELD-COMPAT-101");
  expect(decodeFailure).toBeUndefined();
  expect(decodedArray).toEqual([null, { active: true }]);
  expect(decodedError).toBeInstanceOf(Error);
  expect((decodedError as Error).message).toBe("bounded");
});

test("a native object grafted onto Error.prototype keeps its internal-slot refusal", () => {
  for (const [value, label] of [
    [new Map([["secret", 1]]), "Map"],
    [new Set([1]), "Set"],
    [new ArrayBuffer(4), "ArrayBuffer"],
    [/hostile/gu, "RegExp"],
  ] as const) {
    const prototype = Object.getPrototypeOf(value);
    Object.setPrototypeOf(value, Error.prototype);
    try {
      const failure = caught(() => encodeEndpointValue(value));
      expect(failure.kind).toBe("UnsupportedValue");
      expect(failure.code).toBe("KELD-COMPAT-100");
      expect(failure.message).toContain(`unsupported ${label} object`);
    } finally {
      Object.setPrototypeOf(value, prototype);
    }
  }
  const genuine = new Error("valid");
  const roundTrip = decodeEndpointValue(encodeEndpointValue(genuine));
  expect(roundTrip).toBeInstanceOf(Error);
  expect((roundTrip as Error).message).toBe("valid");
});

test("early trusted codec import keeps authentic Map/WeakMap brands across late constructor poison", () => {
  const moduleUrl = new URL("./endpoint-value-codec.ts", import.meta.url).href;
  for (const globalName of ["Map", "WeakMap"] as const) {
    for (const style of ["throwing", "forged-native-comment"] as const) {
      const script = `
      const original = globalThis[${JSON.stringify(globalName)}];
      const path = ${JSON.stringify(moduleUrl)} + "?early-${globalName}-${style}";
      // The approved Host-owned API evaluates this same module BEFORE
      // application-controlled constructors can be changed.
      const codec = await import(path);
      const retained = new Map([["private", 1]]);
      const weakKey = {};
      const retainedWeak = new WeakMap([[weakKey, 1]]);
      let fakeHasCalls = 0;
      const fake = ${JSON.stringify(style)} === "throwing"
        ? (() => { throw Error("hostile constructor"); })
        : Function("return function ${globalName}() { /* [native code] */ }")();
      if (${JSON.stringify(style)} !== "throwing")
        fake.prototype.has = eval("(function has() { /* [native code] */ fakeHasCalls++; return true; })");
      globalThis[${JSON.stringify(globalName)}] = fake;
      try {
        if ((await import(path)) !== codec)
          throw Error("late caller evaluated a second endpoint codec");
        const restored = codec.decodeEndpointValue(
          codec.encodeEndpointValue({ own: "safe" }));
        if (JSON.stringify(restored) !== '{"own":"safe"}')
          throw Error("legitimate record lost after late constructor poison");
        const args = codec.decodeEndpointArguments(
          codec.encodeEndpointArguments([true]));
        if (args.length !== 1 || args[0] !== true)
          throw Error("legitimate argument vector lost after poison");
        for (const value of [retained, retainedWeak]) {
          let error;
          try { codec.encodeEndpointValue(value); } catch (caught) { error = caught; }
          if (error?.code !== "KELD-COMPAT-100" || error.kind !== "UnsupportedValue")
            throw Error("genuine native brand was lost: " + String(error));
        }
        if (fakeHasCalls !== 0)
          throw Error("late forged has was invoked as a native brand probe");
      } finally {
        globalThis[${JSON.stringify(globalName)}] = original;
      }
      console.log("PASS ${globalName}-${style}");
    `;
      const child = Bun.spawnSync([process.execPath, "-e", script], {
        stdout: "pipe",
        stderr: "pipe",
      });
      expect(child.exitCode, new TextDecoder().decode(child.stderr)).toBe(0);
      expect(new TextDecoder().decode(child.stdout).trim()).toBe(
        `PASS ${globalName}-${style}`,
      );
    }
  }
});

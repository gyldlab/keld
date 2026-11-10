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
  expect(() => encodeEndpointArguments(changingLength)).toThrow(
    "KELD-COMPAT-102",
  );
});

test("native slots survive prototype changes and Dates keep timestamps", () => {
  for (const value of [
    new Map([["k", 1]]),
    new Set([1]),
    new ArrayBuffer(4),
    new Error("failure"),
  ]) {
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

  const date = new Date("2024-02-03T04:05:06.789Z");
  Object.setPrototypeOf(date, null);
  const decoded = decodeEndpointValue(encodeEndpointValue(date));
  expect(decoded).toBeInstanceOf(Date);
  expect((decoded as Date).getTime()).toBe(1_706_933_106_789);

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

test("default proxy wrappers over native-slot values stay unsupported", () => {
  const nativeValues: object[] = [
    new Uint8Array([1, 2]),
    new Int16Array([3]),
    new DataView(new ArrayBuffer(4)),
    new Date(1),
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
  const unsupported = caught(() => encodeEndpointValue(new (class Box {})()));
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
    {},
    {
      getPrototypeOf() {
        throw cached;
      },
    },
  );
  const replayFailure = caught(() => encodeEndpointValue(replayed));
  expect(replayFailure.kind).toBe("MalformedValue");
  expect(replayFailure.code).toBe("KELD-COMPAT-101");
  expect(replayFailure).not.toBe(cached);

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
    const hostile = new Proxy(
      {},
      {
        getPrototypeOf() {
          nestedOperation();
          return Object.prototype;
        },
      },
    );
    const error = caught(() => encodeEndpointValue(hostile));
    expect(error.kind).toBe("MalformedValue");
    expect(error.code).toBe("KELD-COMPAT-101");
  }
  expect(caught(() => encodeEndpointArguments(seventeen)).code).toBe(
    "KELD-COMPAT-102",
  );
  expect(caught(() => decodeEndpointArguments(validSeventeenWire)).code).toBe(
    "KELD-COMPAT-102",
  );
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

test("small arrays share the aggregate descriptor-probe budget", () => {
  let descriptorCalls = 0;
  const children = Array.from({ length: 30 }, () => {
    const item = [null];
    for (let index = 0; index < 3; index++)
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
  expect(descriptorCalls).toBeLessThan(256);
});

test("normal arrays and the 256 aggregate visit boundary remain accepted", () => {
  const boundary = Array.from({ length: 84 }, (_, index) => index);
  expect(decodeEndpointValue(encodeEndpointValue(boundary))).toEqual(boundary);
  expect(caught(() => encodeEndpointValue([...boundary, 84])).code).toBe(
    "KELD-COMPAT-102",
  );
});

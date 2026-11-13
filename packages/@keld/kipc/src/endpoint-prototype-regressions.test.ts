import { expect, test } from "bun:test";
import {
  CompatCodecError,
  decodeEndpointArguments,
  decodeEndpointValue,
  encodeEndpointArguments,
  encodeEndpointValue,
} from "./endpoint-value-codec.ts";

test("writer finalization ignores caller-controlled typed-array species", () => {
  const original = Object.getOwnPropertyDescriptor(
    Uint8Array.prototype,
    "constructor",
  )!;
  for (const throwing of [false, true]) {
    let speciesReads = 0;
    let wire: Uint8Array | undefined;
    try {
      wire = encodeEndpointValue(
        new Proxy(
          { a: undefined },
          {
            ownKeys(target) {
              Object.defineProperty(Uint8Array.prototype, "constructor", {
                configurable: true,
                value: {
                  get [Symbol.species]() {
                    speciesReads += 1;
                    if (throwing) throw new Error("species getter ran");
                    return function () {
                      return new Uint8Array([7, 1, 0, 1, 0, 97, 1]);
                    };
                  },
                },
              });
              return Reflect.ownKeys(target);
            },
          },
        ),
      );
    } finally {
      Object.defineProperty(Uint8Array.prototype, "constructor", original);
    }
    expect(speciesReads).toBe(0);
    expect(Array.from(wire!)).toEqual([7, 1, 0, 1, 0, 97, 0]);
    expect(decodeEndpointValue(wire!)).toEqual({ a: undefined });
  }
  expect(
    decodeEndpointArguments(encodeEndpointArguments([null, false])),
  ).toEqual([null, false]);
});

test("refusals ignore an inherited Error name setter installed by a trap", () => {
  const original = Object.getOwnPropertyDescriptor(Error.prototype, "name")!;
  let caught: unknown;
  let setterCalls = 0;
  try {
    encodeEndpointValue(
      new Proxy(
        {},
        {
          ownKeys() {
            Object.defineProperty(Error.prototype, "name", {
              configurable: true,
              set() {
                setterCalls += 1;
                throw new RangeError("raw poison");
              },
            });
            throw new Error("input trap");
          },
        },
      ),
    );
  } catch (error) {
    caught = error;
  } finally {
    Object.defineProperty(Error.prototype, "name", original);
  }
  expect(setterCalls).toBe(0);
  expect(caught).toBeInstanceOf(CompatCodecError);
  expect((caught as CompatCodecError).code).toBe("KELD-COMPAT-101");
  expect((caught as CompatCodecError).kind).toBe("MalformedValue");
  expect((caught as Error).name).toBe("CompatCodecError");
  expect(() => encodeEndpointValue(() => null)).toThrow("KELD-COMPAT-100");
  expect(decodeEndpointValue(encodeEndpointValue({ ok: true }))).toEqual({
    ok: true,
  });
});

test("JSON constructor and prototype data keys round-trip without prototype pollution", () => {
  const json = JSON.parse(
    '{"constructor":42,"prototype":{"constructor":"nested","prototype":false}}',
  );
  const result = decodeEndpointValue(encodeEndpointValue(json)) as Record<
    string,
    unknown
  >;
  expect(result).toEqual(json);
  expect(Object.getPrototypeOf(result)).toBe(Object.prototype);
  expect(Object.hasOwn(result, "constructor")).toBe(true);
  expect(Object.hasOwn(result, "prototype")).toBe(true);

  const forbidden = JSON.parse('{"__proto__":{"polluted":true}}');
  expect(() => encodeEndpointValue(forbidden)).toThrow("KELD-COMPAT-101");
  expect(() =>
    decodeEndpointValue(
      Uint8Array.from([
        7,
        1,
        0,
        9,
        0,
        ...new TextEncoder().encode("__proto__"),
        1,
      ]),
    ),
  ).toThrow("KELD-COMPAT-101");
  expect(({} as Record<string, unknown>).polluted).toBeUndefined();
});

test("literal tag-07 golden admits constructor and prototype as own data", () => {
  // Independent wire oracle: tag07, two UTF-8 sorted keys, f64LE(42), false.
  // Do not derive these bytes from the encoder or its key-order comparator.
  const wire = Uint8Array.from([
    7, 2, 0, 11, 0, 99, 111, 110, 115, 116, 114, 117, 99, 116, 111, 114, 4, 0,
    0, 0, 0, 0, 0, 69, 64, 9, 0, 112, 114, 111, 116, 111, 116, 121, 112, 101, 2,
  ]);
  const value = JSON.parse('{"constructor":42,"prototype":false}');
  expect(encodeEndpointValue(value)).toEqual(wire);
  const decoded = decodeEndpointValue(wire) as Record<string, unknown>;
  expect(decoded).toEqual(value);
  expect(Object.getOwnPropertyDescriptor(decoded, "constructor")?.value).toBe(
    42,
  );
  expect(Object.getOwnPropertyDescriptor(decoded, "prototype")?.value).toBe(
    false,
  );
  expect(Object.getPrototypeOf(decoded)).toBe(Object.prototype);
});

test("decoder defines JSON data keys without calling inherited setters", () => {
  const originalConstructor = Object.getOwnPropertyDescriptor(
    Object.prototype,
    "constructor",
  )!;
  const originalPrototype = Object.getOwnPropertyDescriptor(
    Object.prototype,
    "prototype",
  );
  const wire = encodeEndpointValue(
    JSON.parse('{"constructor":42,"prototype":false}'),
  );
  let setterCalls = 0;
  let decoded: Record<string, unknown> | undefined;
  try {
    for (const key of ["constructor", "prototype"]) {
      Object.defineProperty(Object.prototype, key, {
        configurable: true,
        set() {
          setterCalls++;
          throw new Error("inherited setter must not run");
        },
      });
    }
    decoded = decodeEndpointValue(wire) as Record<string, unknown>;
  } finally {
    Object.defineProperty(Object.prototype, "constructor", originalConstructor);
    if (originalPrototype === undefined)
      delete (Object.prototype as Record<string, unknown>).prototype;
    else
      Object.defineProperty(Object.prototype, "prototype", originalPrototype);
  }
  expect(setterCalls).toBe(0);
  expect(decoded).toEqual({ constructor: 42, prototype: false });
  expect(Object.getPrototypeOf(decoded)).toBe(Object.prototype);
});

test("codec descriptors ignore inherited get and set planted by a proxy trap", () => {
  const arrayWire = encodeEndpointValue([false]);
  const objectWire = encodeEndpointValue({ constructor: 42, prototype: false });
  const errorWire = encodeEndpointValue(new Error("safe"));
  const poison = {
    __proto__: null,
    configurable: true,
    value: () => undefined,
  } as PropertyDescriptor;

  for (const field of ["get", "set"] as const) {
    const original = Object.getOwnPropertyDescriptor(Object.prototype, field);
    let caught: unknown;
    try {
      encodeEndpointValue(
        new Proxy(
          {},
          {
            ownKeys() {
              Object.defineProperty(Object.prototype, field, poison);
              throw new Error("trap must be converted to a typed refusal");
            },
          },
        ),
      );
    } catch (error) {
      caught = error;
    } finally {
      Reflect.deleteProperty(Object.prototype, field);
      if (original) Object.defineProperty(Object.prototype, field, original);
    }
    expect(caught).toBeInstanceOf(CompatCodecError);
    expect((caught as CompatCodecError).code).toBe("KELD-COMPAT-101");
    expect((caught as CompatCodecError).kind).toBe("MalformedValue");

    let decoded: unknown[] = [];
    try {
      Object.defineProperty(Object.prototype, field, poison);
      decoded = [
        decodeEndpointValue(arrayWire),
        decodeEndpointValue(objectWire),
        decodeEndpointValue(errorWire),
      ];
    } finally {
      Reflect.deleteProperty(Object.prototype, field);
      if (original) Object.defineProperty(Object.prototype, field, original);
    }
    expect(decoded[0]).toEqual([false]);
    expect(decoded[1]).toEqual({ constructor: 42, prototype: false });
    expect((decoded[2] as Error).message).toBe("safe");
    expect((decoded[2] as Error).name).toBe("Error");
  }
});

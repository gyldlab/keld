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

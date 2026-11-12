import { expect, test } from "bun:test";
import {
  CompatCodecError,
  decodeEndpointValue,
  encodeEndpointValue,
} from "./endpoint-value-codec.ts";

test("unsupported native values report a static type label", () => {
  const cases: [object, string][] = [
    [new Map(), "Map"],
    [new Set(), "Set"],
    [/x/, "RegExp"],
    [new Uint8Array(2), "TypedArray"],
    [new ArrayBuffer(2), "ArrayBuffer"],
  ];
  for (const [value, label] of cases) {
    Object.defineProperty(value, "constructor", {
      get() {
        throw new Error("constructor getter must not run");
      },
    });
    Object.defineProperty(value, Symbol.toStringTag, {
      get() {
        throw new Error("type tag getter must not run");
      },
    });
    let caught: unknown;
    try {
      encodeEndpointValue(value);
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(CompatCodecError);
    expect((caught as CompatCodecError).code).toBe("KELD-COMPAT-100");
    expect((caught as Error).message).toContain(label);
  }
  const stripped = Object.setPrototypeOf(new Map(), null);
  expect(() => encodeEndpointValue(stripped)).toThrow("unsupported Map");
  expect(decodeEndpointValue(encodeEndpointValue({ ok: true }))).toEqual({
    ok: true,
  });
});

test("boxed native primitives refuse without reading caller-defined type getters", () => {
  const cases: [object, string][] = [
    [new Number(42), "Number"],
    [new Boolean(false), "Boolean"],
    [new String("abc"), "String"],
    [Object(123n), "BigInt"],
    [Object(Symbol("secret")), "Symbol"],
  ];
  let getterCalls = 0;
  for (const [value, label] of cases) {
    Object.defineProperty(value, "constructor", {
      get() {
        getterCalls++;
        throw new Error("untrusted constructor getter");
      },
    });
    Object.defineProperty(value, "valueOf", {
      get() {
        getterCalls++;
        throw new Error("untrusted valueOf getter");
      },
    });
    Object.defineProperty(value, Symbol.toStringTag, {
      get() {
        getterCalls++;
        throw new Error("untrusted type tag getter");
      },
    });
    let caught: unknown;
    try {
      encodeEndpointValue(value);
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(CompatCodecError);
    expect((caught as CompatCodecError).code).toBe("KELD-COMPAT-100");
    expect((caught as CompatCodecError).kind).toBe("UnsupportedValue");
    expect((caught as Error).message).toContain(`unsupported ${label} object`);

    // A replaced prototype must not disguise the native internal slot.
    Object.setPrototypeOf(value, null);
    expect(() => encodeEndpointValue(value)).toThrow(`unsupported ${label}`);
  }
  expect(getterCalls).toBe(0);

  class OrdinaryValue {
    public field = 7;
  }
  expect(decodeEndpointValue(encodeEndpointValue(new OrdinaryValue()))).toEqual(
    {
      field: 7,
    },
  );
  const forgedPrototype = Object.setPrototypeOf({ field: 9 }, Number.prototype);
  expect(decodeEndpointValue(encodeEndpointValue(forgedPrototype))).toEqual({
    field: 9,
  });
});

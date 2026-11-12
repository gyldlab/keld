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

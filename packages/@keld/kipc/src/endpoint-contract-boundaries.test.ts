import { expect, test } from "bun:test";
import * as codec from "./endpoint-value-codec.ts";

function nested(count: number): unknown {
  let value: unknown = null;
  for (let index = 0; index < count; index += 1) value = [value];
  return value;
}
function nestedBytes(count: number): Uint8Array {
  return Uint8Array.from([
    ...Array.from({ length: count }, () => [6, 1, 0]).flat(),
    1,
  ]);
}
function objectWire(count: number, extraChild = false): Uint8Array {
  const bytes = [7, count & 255, count >>> 8];
  for (let index = 0; index < count; index += 1) {
    bytes.push(
      4,
      0,
      ...new TextEncoder().encode("k" + index.toString().padStart(3, "0")),
    );
    bytes.push(...(extraChild && index === count - 1 ? [6, 1, 0, 1] : [1]));
  }
  return Uint8Array.from(bytes);
}
function objectValue(
  count: number,
  extraChild = false,
): Record<string, unknown> {
  return Object.fromEntries(
    Array.from({ length: count }, (_, index) => [
      "k" + index.toString().padStart(3, "0"),
      extraChild && index === count - 1 ? [null] : null,
    ]),
  );
}
test("wire depth16 includes root and scalar leaf independently at both endpoints", () => {
  expect(codec.encodeEndpointValue(nested(15))).toEqual(nestedBytes(15));
  expect(codec.decodeEndpointValue(nestedBytes(15))).toEqual(nested(15));
  expect(() => codec.encodeEndpointValue(nested(16))).toThrow(
    "KELD-COMPAT-102",
  );
  expect(() => codec.decodeEndpointValue(nestedBytes(16))).toThrow(
    "KELD-COMPAT-102",
  );
});
test("aggregate object key plus tagged-value visits 255/256 pass and257 refuse", () => {
  for (const extra of [false, true]) {
    expect(codec.encodeEndpointValue(objectValue(127, extra))).toEqual(
      objectWire(127, extra),
    );
    expect(codec.decodeEndpointValue(objectWire(127, extra))).toEqual(
      objectValue(127, extra),
    );
  }
  expect(() => codec.encodeEndpointValue(objectValue(128))).toThrow(
    "KELD-COMPAT-102",
  );
  expect(() => codec.decodeEndpointValue(objectWire(128))).toThrow(
    "KELD-COMPAT-102",
  );
});
test("sixteen IPC arguments are separate from a valid seventeen-element data array", () => {
  const args = codec as typeof codec & {
    encodeEndpointArguments?: (args: readonly unknown[]) => Uint8Array;
    decodeEndpointArguments?: (bytes: Uint8Array) => unknown[];
  };
  expect(typeof args.encodeEndpointArguments).toBe("function");
  expect(typeof args.decodeEndpointArguments).toBe("function");
  const sixteen = Array(16).fill(null),
    seventeen = Array(17).fill(null);
  expect(
    args.decodeEndpointArguments!(args.encodeEndpointArguments!(sixteen)),
  ).toEqual(sixteen);
  expect(() => args.encodeEndpointArguments!(seventeen)).toThrow(
    "KELD-COMPAT-102",
  );
  expect(() =>
    args.decodeEndpointArguments!(codec.encodeEndpointValue(seventeen)),
  ).toThrow("KELD-COMPAT-102");
  expect(
    codec.decodeEndpointValue(codec.encodeEndpointValue(seventeen)),
  ).toEqual(seventeen);
});

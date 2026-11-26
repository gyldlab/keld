/** Independent postcard literals; fixture identities grant no native authority. */
import { expect, test } from "bun:test";
import {
  decodeCompatControlEvent,
  encodeCompatControlEvent,
  type CompatControlEvent,
} from "./compat-control.generated.ts";

type Bound = Extract<CompatControlEvent, { variant: "DocumentBound" }>;
const bound: Bound = {
  variant: "DocumentBound",
  webview: 1,
  navigation: 2,
  window_id: 7,
  window_incarnation: 258n,
};
const literal = Uint8Array.of(2, 1, 2, 7, 0x82, 2);

test("DocumentBound appends tag 2 with exact native identity bytes", () => {
  expect(encodeCompatControlEvent(bound)).toEqual(literal);
  expect(decodeCompatControlEvent(literal)).toEqual(bound);
  expect(
    encodeCompatControlEvent({
      variant: "DocumentRetired",
      webview: 1,
      navigation: 2,
    }),
  ).toEqual(Uint8Array.of(1, 1, 2));
  const data: CompatControlEvent = {
    variant: "RendererMessage",
    webview: 1,
    navigation: 2,
    message: 3,
    window_id: 7,
    window_incarnation: 258n,
    url: "x",
    origin: "null",
    topic: "y",
    payload: Uint8Array.of(255),
  };
  expect(encodeCompatControlEvent(data)).toEqual(
    Uint8Array.of(
      0,
      1,
      2,
      3,
      7,
      0x82,
      2,
      1,
      120,
      4,
      110,
      117,
      108,
      108,
      1,
      121,
      1,
      255,
    ),
  );
});

test("DocumentBound keeps nonzero u32/u64 identity bounds and a 26-byte ceiling", () => {
  for (const key of ["webview", "navigation", "window_id"] as const) {
    for (const value of [0, -1, 1.5, Number.NaN, 0x1_0000_0000]) {
      expect(() =>
        encodeCompatControlEvent({ ...bound, [key]: value }),
      ).toThrow("KELD-IPC-003");
    }
  }
  for (const value of [0n, -1n, 0x1_0000_0000_0000_0000n]) {
    expect(() =>
      encodeCompatControlEvent({ ...bound, window_incarnation: value }),
    ).toThrow("KELD-IPC-003");
  }
  const maximum: Bound = {
    variant: "DocumentBound",
    webview: 0xffffffff,
    navigation: 0xffffffff,
    window_id: 0xffffffff,
    window_incarnation: 0xffff_ffff_ffff_ffffn,
  };
  const maximumWire = Uint8Array.of(
    2,
    255,
    255,
    255,
    255,
    15,
    255,
    255,
    255,
    255,
    15,
    255,
    255,
    255,
    255,
    15,
    255,
    255,
    255,
    255,
    255,
    255,
    255,
    255,
    255,
    1,
  );
  expect(maximumWire.byteLength).toBe(26);
  expect(encodeCompatControlEvent(maximum)).toEqual(maximumWire);
  expect(decodeCompatControlEvent(maximumWire)).toEqual(maximum);
});

test("DocumentBound decode rejects truncation, zero identities, unknown tags and trailing bytes", () => {
  for (let length = 0; length < literal.byteLength; length++) {
    expect(() => decodeCompatControlEvent(literal.slice(0, length))).toThrow();
  }
  for (const invalid of [
    Uint8Array.of(2, 0, 2, 7, 1),
    Uint8Array.of(2, 1, 0, 7, 1),
    Uint8Array.of(2, 1, 2, 0, 1),
    Uint8Array.of(2, 1, 2, 7, 0),
    Uint8Array.of(3),
    Uint8Array.of(...literal, 0),
  ]) {
    expect(() => decodeCompatControlEvent(invalid)).toThrow("KELD-IPC-003");
  }
});

test("compat payload decode never invokes mutated typed-array slice", () => {
  const event: CompatControlEvent = {
    variant: "RendererMessage",
    webview: 1,
    navigation: 2,
    message: 3,
    window_id: 7,
    window_incarnation: 258n,
    url: "x",
    origin: "null",
    topic: "safety",
    payload: Uint8Array.of(1, 2, 3),
  };
  const encoded = encodeCompatControlEvent(event);
  const originalSlice = Uint8Array.prototype.slice;
  let hijacked = 0;
  let decoded: CompatControlEvent | undefined;
  try {
    Uint8Array.prototype.slice = function () {
      hijacked++;
      return Uint8Array.of(0xde, 0xad, 0xbe, 0xef);
    };
    decoded = decodeCompatControlEvent(encoded);
  } finally {
    Uint8Array.prototype.slice = originalSlice;
  }
  expect(hijacked).toBe(0);
  expect(decoded).toEqual(event);
  encoded[encoded.length - 1] = 0;
  if (decoded?.variant !== "RendererMessage")
    throw new Error("decoded wrong event");
  expect(decoded.payload).toEqual(Uint8Array.of(1, 2, 3));
});

test("compat byte ownership uses a module-captured allocator", async () => {
  const source = await Bun.file(
    new URL("./compat-control.generated.ts", import.meta.url),
  ).text();
  const copy = /function readBytes\([\s\S]*?\n}/.exec(source)?.[0];
  expect(copy).toBeDefined();
  const capture = source.indexOf("const OwnedPayloadBytes = Uint8Array;");
  expect(capture).toBeGreaterThanOrEqual(0);
  expect(capture).toBeLessThan(source.indexOf("function readBytes("));
  expect(copy).toContain("const copied = new OwnedPayloadBytes(count);");
  expect(copy).not.toMatch(/\.(?:slice|subarray)\(|new Uint8Array\(/);
});

test("compat byte ownership preserves a nonzero-offset wire window", () => {
  const event: CompatControlEvent = {
    variant: "RendererMessage",
    webview: 1,
    navigation: 2,
    message: 3,
    window_id: 7,
    window_incarnation: 258n,
    url: "x",
    origin: "null",
    topic: "copy",
    payload: Uint8Array.of(0, 255, 42),
  };
  const encoded = encodeCompatControlEvent(event);
  const backing = Uint8Array.of(99, ...encoded, 98);
  const wire = backing.subarray(1, backing.length - 1);
  const decoded = decodeCompatControlEvent(wire);
  expect(decoded).toEqual(event);
  if (decoded.variant !== "RendererMessage")
    throw new Error("decoded wrong event");
  expect(decoded.payload.constructor).toBe(Uint8Array);
  wire[wire.length - 1] = 7;
  expect(decoded.payload).toEqual(Uint8Array.of(0, 255, 42));
  decoded.payload[0] = 8;
  expect(wire[wire.length - 3]).toBe(0);
});

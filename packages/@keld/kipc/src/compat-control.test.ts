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

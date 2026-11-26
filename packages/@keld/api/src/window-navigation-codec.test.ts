/** Independent appended postcard literals; no native identity/operation is created. */
import { expect, test } from "bun:test";
import {
  encodeWindowRequest,
  decodeWindowRequest,
  encodeWindowResponse,
  decodeWindowResponse,
  encodeWindowEvent,
  decodeWindowEvent,
} from "./window-codec.ts";

test("append-only LoadUrl7 Loaded9 and native navigation events8/9", () => {
  const window = { id: 7, incarnation: 258n };
  const load = { variant: "LoadUrl" as const, window, uri: "file:///a?q=1" };
  const bytes = Uint8Array.of(
    7,
    7,
    0x82,
    2,
    13,
    102,
    105,
    108,
    101,
    58,
    47,
    47,
    47,
    97,
    63,
    113,
    61,
    49,
  );
  expect(encodeWindowRequest(load)).toEqual(bytes);
  expect(decodeWindowRequest(bytes)).toEqual(load);
  expect(encodeWindowResponse({ variant: "Loaded" })).toEqual(Uint8Array.of(9));
  expect(decodeWindowResponse(Uint8Array.of(9))).toEqual({ variant: "Loaded" });
  const committed = {
    variant: "NavigationCommitted" as const,
    window,
    document_generation: 4,
    native_url: "keld://a",
  };
  const event = Uint8Array.of(
    8,
    7,
    0x82,
    2,
    4,
    8,
    107,
    101,
    108,
    100,
    58,
    47,
    47,
    97,
  );
  expect(encodeWindowEvent(committed)).toEqual(event);
  expect(decodeWindowEvent(event)).toEqual(committed);
  const finished = {
    variant: "NavigationFinished" as const,
    window,
    document_generation: 4,
  };
  expect(encodeWindowEvent(finished)).toEqual(Uint8Array.of(9, 7, 0x82, 2, 4));
  expect(decodeWindowEvent(Uint8Array.of(9, 7, 0x82, 2, 4))).toEqual(finished);
});

test("preload6 and all appended records reject trailing data and oversized body", () => {
  const value = {
    variant: "CreateWithPreload" as const,
    width: 1,
    height: 2,
    x: null,
    y: -1,
    preload_absolute: "/p",
    additional_arguments: ["x"],
  };
  const bytes = Uint8Array.of(6, 1, 2, 0, 1, 1, 2, 47, 112, 1, 1, 120);
  expect(encodeWindowRequest(value)).toEqual(bytes);
  expect(decodeWindowRequest(bytes)).toEqual(value);
  expect(() => decodeWindowResponse(Uint8Array.of(9, 0))).toThrow("trailing");
  expect(() => decodeWindowEvent(Uint8Array.of(9, 7, 0x82, 2, 4, 0))).toThrow(
    "trailing",
  );
  expect(() =>
    encodeWindowRequest({
      variant: "LoadUrl",
      window: { id: 1, incarnation: 1n },
      uri: "x".repeat(8193),
    }),
  ).toThrow("bound");
});

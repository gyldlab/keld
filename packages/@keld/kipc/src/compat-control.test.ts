import { readFileSync } from "node:fs";
import { expect, test } from "bun:test";
import {
  decodeCompatControlEvent,
  decodeCompatControlRequest,
  decodeCompatControlResponse,
  encodeCompatControlEvent,
  encodeCompatControlRequest,
  encodeCompatControlResponse,
  type CompatControlEvent,
  type CompatControlRequest,
  type CompatControlResponse,
} from "./compat-control.generated.ts";
import { decodeCallError, encodeCallError } from "./transport.ts";

const payload = Uint8Array.of(0, 1, 255);
const current = {
  variant: "DeliverToCurrentMain",
  webview: 1,
  topic: "x",
  payload,
} as const;
const release = {
  variant: "ReleaseConsumed",
  webview: 1,
  navigation: 2,
  message: 3,
} as const;
const renderer = {
  variant: "RendererMessage",
  webview: 1,
  navigation: 2,
  message: 3,
  url: "file:///a",
  origin: "null",
  topic: "x",
  payload,
} as const;
const retired = {
  variant: "DocumentRetired",
  webview: 1,
  navigation: 2,
} as const;
const error = { code: "KELD-GUARD003", message: "x" };

const requests: Record<string, CompatControlRequest> = {
  "deliver-current": current,
  "deliver-original": {
    ...current,
    variant: "DeliverToOriginalDocument",
    navigation: 2,
  },
  consumed: { ...release, disposition: { variant: "Consumed" } },
  "no-listener": { ...release, disposition: { variant: "NoListener" } },
  "deliver-current-max": {
    ...current,
    webview: 0xffffffff,
    payload: new Uint8Array(),
  },
};
const events: Record<string, CompatControlEvent> = {
  "renderer-message": renderer,
  "document-retired": retired,
  "document-retired-max": {
    ...retired,
    webview: 0xffffffff,
    navigation: 0xffffffff,
  },
};
const responses: Record<string, CompatControlResponse> = {
  "deliver-consumed": {
    variant: "Deliver",
    disposition: { variant: "Consumed" },
  },
  "deliver-no-listener": {
    variant: "Deliver",
    disposition: { variant: "NoListener" },
  },
  "deliver-stale": { variant: "Deliver", disposition: { variant: "Stale" } },
  "route-not-dispatched": {
    variant: "Deliver",
    disposition: {
      variant: "RouteRetired",
      certainty: { variant: "NotDispatched" },
    },
  },
  "route-outcome-unknown": {
    variant: "Deliver",
    disposition: {
      variant: "RouteRetired",
      certainty: { variant: "OutcomeUnknown" },
    },
  },
  released: { variant: "Released" },
};

test("compat DTOs encode and decode the approved postcard literals", () => {
  const source = readFileSync(
    new URL(
      "../../../../crates/keld-ipc/tests/fixtures/compat-control-v0.tsv",
      import.meta.url,
    ),
    "utf8",
  );
  const seen: string[] = [];
  for (const line of source.split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [id, kind, hex] = line.split("\t");
    seen.push(id);
    const wire = new Uint8Array(Buffer.from(hex.replaceAll(" ", ""), "hex"));
    switch (kind) {
      case "request":
        expect(decodeCompatControlRequest(wire)).toEqual(requests[id]);
        expect(encodeCompatControlRequest(requests[id])).toEqual(wire);
        break;
      case "event":
        expect(decodeCompatControlEvent(wire)).toEqual(events[id]);
        expect(encodeCompatControlEvent(events[id])).toEqual(wire);
        break;
      case "response":
        expect(decodeCompatControlResponse(wire)).toEqual(responses[id]);
        expect(encodeCompatControlResponse(responses[id])).toEqual(wire);
        break;
      case "error": {
        expect(decodeCallError(wire)).toEqual(error);
        expect(encodeCallError(error.code, error.message)).toEqual(wire);
        const refusal = { variant: "Refused", error } as const;
        const request = { ...release, disposition: refusal };
        const response = { variant: "Deliver", disposition: refusal } as const;
        const requestWire = Uint8Array.of(2, 1, 2, 3, 2, ...wire);
        const responseWire = Uint8Array.of(0, 3, ...wire);
        expect(decodeCompatControlRequest(requestWire)).toEqual(request);
        expect(encodeCompatControlRequest(request)).toEqual(requestWire);
        expect(decodeCompatControlResponse(responseWire)).toEqual(response);
        expect(encodeCompatControlResponse(response)).toEqual(responseWire);
        break;
      }
      default:
        throw new Error("Unknown compat fixture kind " + kind);
    }
  }
  expect(seen.sort()).toEqual(
    [
      ...Object.keys(requests),
      ...Object.keys(events),
      ...Object.keys(responses),
      "call-error",
    ].sort(),
  );
});

test("compat fields admit exact UTF-8 and value bounds and refuse max plus one", () => {
  const largest = {
    ...renderer,
    url: "é".repeat(2048),
    origin: "é".repeat(512),
    topic: "é".repeat(128),
    payload: new Uint8Array(4096),
  };
  expect(decodeCompatControlEvent(encodeCompatControlEvent(largest))).toEqual(
    largest,
  );
  const bom = { ...current, topic: "\uFEFF" };
  expect(
    decodeCompatControlRequest(
      Uint8Array.of(0, 1, 3, 0xef, 0xbb, 0xbf, 3, 0, 1, 255),
    ),
  ).toEqual(bom);
  expect(decodeCompatControlRequest(Uint8Array.of(0, 1, 0, 0))).toEqual({
    ...current,
    topic: "",
    payload: new Uint8Array(),
  });
  for (const invalid of [
    { ...largest, url: largest.url + "x" },
    { ...largest, origin: largest.origin + "x" },
    { ...largest, topic: largest.topic + "x" },
    { ...largest, topic: "\ud800" },
    { ...largest, payload: new Uint8Array(4097) },
    { ...largest, message: 0 },
    { ...largest, navigation: 0x1_0000_0000 },
    { ...largest, webview: 1.5 },
  ]) {
    expect(() => encodeCompatControlEvent(invalid)).toThrow("KELD-IPC-003");
  }
});

test("compat decoders reject invalid or out-of-domain bytes", () => {
  const eventWire = Uint8Array.of(0, 1, 2, 3, 1, 97, 0, 0, 1, 255);
  for (let length = 0; length < eventWire.byteLength; length += 1) {
    expect(() =>
      decodeCompatControlEvent(eventWire.subarray(0, length)),
    ).toThrow("KELD-IPC-003");
  }
  for (const invalid of [
    Uint8Array.of(2, 1, 2, 7, 0x82, 2),
    Uint8Array.of(1, 0, 2),
    Uint8Array.of(1, 1, 2, 0),
    Uint8Array.of(0, 1, 2, 3, 1, 0xff, 0, 0, 0),
    new Uint8Array(16385),
  ]) {
    expect(() => decodeCompatControlEvent(invalid)).toThrow("KELD-IPC-003");
  }
  for (const invalid of [
    Uint8Array.of(2, 1, 2, 3, 3),
    Uint8Array.of(0, 1, 0, 0x81, 0x20, ...new Uint8Array(4097)),
    Uint8Array.of(0, 1, 0x81, 0x02, ...new Uint8Array(257), 0),
  ]) {
    expect(() => decodeCompatControlRequest(invalid)).toThrow("KELD-IPC-003");
  }
  for (const invalid of [
    Uint8Array.of(0, 4, 2),
    Uint8Array.of(1, 0),
    new Uint8Array(4097),
  ]) {
    expect(() => decodeCompatControlResponse(invalid)).toThrow("KELD-IPC-003");
  }
});

test("decoding snapshots opaque endpoint bytes from a Buffer", () => {
  const bytes = Buffer.from([0, 1, 1, 120, 3, 0, 1, 255]);
  const decoded = decodeCompatControlRequest(bytes);
  expect(decoded.variant).toBe("DeliverToCurrentMain");
  bytes.fill(0);
  expect(decoded).toEqual(current);
});

test("DocumentRetired refuses a 12-byte body before decoding its selectors", () => {
  // The extended varint still denotes tag 1, with two valid maximum selectors.
  const wire = Uint8Array.of(
    0x81,
    0,
    0xff,
    0xff,
    0xff,
    0xff,
    0x0f,
    0xff,
    0xff,
    0xff,
    0xff,
    0x0f,
  );
  expect(() => decodeCompatControlEvent(wire)).toThrow(
    "retirement exceeds control limit",
  );
});

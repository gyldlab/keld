import { describe, expect, test } from "bun:test";

import {
  ECHO_CHANNEL,
  LIFECYCLE_CHANNEL,
  decodePostcardStringAt,
  decodeVarint,
  encodePostcardString,
  encodeVarint,
} from "../../kipc/src/transport.ts";
import {
  channels,
  dispatchApplicationCall,
  echoChannel,
  type AppChannel,
  type EchoRequest,
  type EchoResponse,
} from "./channels.ts";

function encodeEcho(message: string, count: number): Uint8Array {
  const text = encodePostcardString(message, "echo test message");
  const number = encodeVarint(count);
  const out = new Uint8Array(text.length + number.length);
  out.set(text);
  out.set(number, text.length);
  return out;
}
function decodeEcho(bytes: Uint8Array): EchoResponse {
  const [message, afterMessage] = decodePostcardStringAt(bytes, 0);
  const [count, end] = decodeVarint(bytes, afterMessage);
  if (end !== bytes.length) throw new Error("test decoder saw trailing bytes");
  return { message, count };
}

function fakeChannel(id: number): AppChannel<EchoRequest, EchoResponse> {
  return {
    id,
    decodeRequest: echoChannel.decodeRequest,
    encodeResponse: echoChannel.encodeResponse,
  };
}

describe("@keld/api channels", () => {
  test("declared Echo descriptor decodes request and encodes response", () => {
    expect(echoChannel.id).toBe(ECHO_CHANNEL);
    expect(echoChannel.decodeRequest(encodeEcho("keld", 7))).toEqual({
      message: "keld",
      count: 7,
    });
    expect(decodeEcho(echoChannel.encodeResponse({ message: "ok", count: 8 }))).toEqual({
      message: "ok",
      count: 8,
    });
  });
  test("rejects reserved and caller-constructed descriptors", () => {
    expect(() => channels.handle(fakeChannel(LIFECYCLE_CHANNEL), (request) => request)).toThrow(
      "KELD-API-001",
    );
    expect(() => channels.handle(fakeChannel(ECHO_CHANNEL), (request) => request)).toThrow(
      "KELD-API-001",
    );
  });

  test("rejects duplicate registration and unsubscribe is idempotent", () => {
    const unsubscribe = channels.handle(echoChannel, (request) => request);
    try {
      expect(() => channels.handle(echoChannel, (request) => request)).toThrow(
        "KELD-API-001",
      );
    } finally {
      unsubscribe();
      unsubscribe();
    }
    const second = channels.handle(echoChannel, (request) => request);
    second();
  });

  test("missing handler rejects before any app effect", async () => {
    await expect(dispatchApplicationCall(ECHO_CHANNEL, encodeEcho("none", 0))).rejects.toThrow(
      "KELD-API-001",
    );
  });
  test("dispatch decodes once, invokes once, and encodes the typed response", async () => {
    let calls = 0;
    const unsubscribe = channels.handle(echoChannel, async (request) => {
      calls += 1;
      return { message: request.message + "-reply", count: request.count + 1 };
    });
    try {
      const reply = await dispatchApplicationCall(ECHO_CHANNEL, encodeEcho("hello", 4));
      expect(decodeEcho(reply)).toEqual({ message: "hello-reply", count: 5 });
      expect(calls).toBe(1);
    } finally {
      unsubscribe();
    }
  });

  test("malformed declared payload has zero handler effect", async () => {
    let calls = 0;
    const unsubscribe = channels.handle(echoChannel, (request) => {
      calls += 1;
      return request;
    });
    try {
      await expect(
        dispatchApplicationCall(ECHO_CHANNEL, new Uint8Array([0x05, 0x61])),
      ).rejects.toThrow("KELD-IPC-003");
      expect(calls).toBe(0);
    } finally {
      unsubscribe();
    }
  });
});

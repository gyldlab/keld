import { describe, expect, test } from "bun:test";

import {
  ECHO_CHANNEL,
  FrameKind,
  MAX_FRAME_LEN,
  decodeCallError,
  encodePostcardString,
  encodeVarint,
} from "../../kipc/src/transport.ts";
import {
  channels,
  dispatchApplicationCall,
  echoChannel,
  type EchoRequest,
  type EchoResponse,
} from "./channels.ts";
import { resolveEchoCall, type EchoCallHandler } from "./echo-call.ts";

function echoRequest(message: string, count: number): Uint8Array {
  const text = encodePostcardString(message, "echo test message");
  const number = encodeVarint(count);
  const out = new Uint8Array(text.length + number.length);
  out.set(text);
  out.set(number, text.length);
  return out;
}

const PAYLOAD = new Uint8Array([0x00, 0x00]);

// The transport Worker validates each host CALL under its KEL-133 policy
// before any handler runs and writes the answer with the CALL's channel and
// correlation id (GH-527 §4.6, §4.7); `resolveEchoCall` owns only the answer.
describe("Echo CALL answers", () => {
  test("one admitted handler yields one Reply with its bytes", async () => {
    let calls = 0;
    const seen: number[] = [];
    const handler: EchoCallHandler = async (channel, payload) => {
      calls += 1;
      seen.push(channel);
      return Uint8Array.from([...payload, 0x7f]);
    };
    const reply = await resolveEchoCall(ECHO_CHANNEL, PAYLOAD, handler);
    expect(reply.kind).toBe(FrameKind.Reply);
    expect(Array.from(reply.payload)).toEqual([0x00, 0x00, 0x7f]);
    expect(calls).toBe(1);
    expect(seen).toEqual([ECHO_CHANNEL]);
  });

  test("missing or throwing handler becomes one KELD-API-001 Err", async () => {
    for (const handler of [
      undefined,
      (async () => {
        throw new Error("secret stack detail");
      }) as EchoCallHandler,
    ]) {
      const reply = await resolveEchoCall(ECHO_CHANNEL, PAYLOAD, handler);
      expect(reply.kind).toBe(FrameKind.Err);
      const error = decodeCallError(reply.payload);
      expect(error.code).toBe("KELD-API-001");
      expect(error.message).toBe("KELD-API-001: application channel call failed");
      expect(error.message).not.toContain("secret stack detail");
    }
  });

  test("oversized handler reply becomes one KELD-API-001 Err", async () => {
    const handler: EchoCallHandler = async () => new Uint8Array(MAX_FRAME_LEN + 1);
    const reply = await resolveEchoCall(ECHO_CHANNEL, PAYLOAD, handler);
    expect(reply.kind).toBe(FrameKind.Err);
    expect(decodeCallError(reply.payload).code).toBe("KELD-API-001");
  });
});

// GH-1095 AT-API2a (spec §4.2 S11–S14): one host Echo CALL answered through
// the shipped composition `resolveEchoCall(..., dispatchApplicationCall)`.
// The census counts handler entries and committed effects, so it is the
// oracle for "did app code run", independent of the answer's code.
describe("Echo CALL answers by stage (GH-1095)", () => {
  const census = { entered: 0, committed: 0 };
  const reset = () => {
    census.entered = 0;
    census.committed = 0;
  };
  const answer = async (payload: Uint8Array = echoRequest("stage", 1)) =>
    await resolveEchoCall(ECHO_CHANNEL, payload, dispatchApplicationCall);
  const errCode = (reply: { kind: number; payload: Uint8Array }) => {
    expect(reply.kind).toBe(FrameKind.Err);
    return decodeCallError(reply.payload).code;
  };
  const register = (body: (request: EchoRequest) => EchoResponse) =>
    channels.handle(echoChannel, async (request) => {
      census.entered += 1;
      return body(request);
    });

  test("S11 no handler registered: KELD-API-002, no app code runs", async () => {
    reset();
    const reply = await answer();
    expect(errCode(reply)).toBe("KELD-API-002");
    expect(decodeCallError(reply.payload).message.startsWith("KELD-API-002: ")).toBe(true);
    expect(census).toEqual({ entered: 0, committed: 0 });
  });

  test("S11 late registration: KELD-API-002 first, then the one resend runs once", async () => {
    reset();
    expect(errCode(await answer())).toBe("KELD-API-002");
    expect(census).toEqual({ entered: 0, committed: 0 });
    const unsubscribe = register((request) => {
      census.committed += 1;
      return { message: request.message, count: request.count };
    });
    try {
      const reply = await answer();
      expect(reply.kind).toBe(FrameKind.Reply);
      expect(census).toEqual({ entered: 1, committed: 1 });
    } finally {
      unsubscribe();
    }
  });

  test("S11 unsubscribed handler: KELD-API-002, no app code runs", async () => {
    reset();
    const unsubscribe = register((request) => request);
    unsubscribe();
    expect(errCode(await answer())).toBe("KELD-API-002");
    expect(census).toEqual({ entered: 0, committed: 0 });
  });

  test("S12 decode failure: the handler never runs", async () => {
    reset();
    const unsubscribe = register((request) => request);
    try {
      expect(errCode(await answer(new Uint8Array([0x05, 0x61])))).toBe("KELD-API-001");
      expect(census).toEqual({ entered: 0, committed: 0 });
    } finally {
      unsubscribe();
    }
  });

  test("S13 a handler that throws a look-alike message is still KELD-API-001", async () => {
    reset();
    const unsubscribe = register(() => {
      census.committed += 1;
      throw new Error("KELD-API-002: Echo channel has no registered handler");
    });
    try {
      expect(errCode(await answer())).toBe("KELD-API-001");
      expect(census).toEqual({ entered: 1, committed: 1 });
    } finally {
      unsubscribe();
    }
  });

  test("S13 handler throws after its effect committed", async () => {
    reset();
    const unsubscribe = register(() => {
      census.committed += 1;
      throw new Error("after the effect");
    });
    try {
      expect(errCode(await answer())).toBe("KELD-API-001");
      expect(census).toEqual({ entered: 1, committed: 1 });
    } finally {
      unsubscribe();
    }
  });

  test("S14 oversize reply after the handler ran", async () => {
    reset();
    const unsubscribe = register((request) => {
      census.committed += 1;
      return { message: "x".repeat(MAX_FRAME_LEN), count: request.count };
    });
    try {
      expect(errCode(await answer())).toBe("KELD-API-001");
      expect(census).toEqual({ entered: 1, committed: 1 });
    } finally {
      unsubscribe();
    }
  });
});

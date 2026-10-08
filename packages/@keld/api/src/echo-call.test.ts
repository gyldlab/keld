import { describe, expect, test } from "bun:test";

import {
  ECHO_CHANNEL,
  FrameKind,
  MAX_FRAME_LEN,
  decodeCallError,
} from "../../kipc/src/transport.ts";
import { resolveEchoCall, type EchoCallHandler } from "./echo-call.ts";

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

import { describe, expect, test } from "bun:test";

import {
  DrainSignal,
  ECHO_CHANNEL,
  FrameKind,
  HEADER_LEN,
  WriteQueue,
  decodeCallError,
  decodeHeader,
  type DecodedFrame,
} from "../../kipc/src/transport.ts";
import { handleEchoCall, type EchoCallHandler } from "./echo-call.ts";

function callFrame(channel = ECHO_CHANNEL): DecodedFrame {
  return {
    header: { kind: FrameKind.Call, flags: 0, channel, corr: 19, len: 2 },
    payload: new Uint8Array([0x00, 0x00]),
  };
}

function captureWriter(): { writes: WriteQueue; bytes: number[] } {
  const bytes: number[] = [];
  const socket = {
    write(data: Uint8Array): number {
      bytes.push(...data);
      return data.length;
    },
    end(): void {},
  };
  return { writes: new WriteQueue(socket, new DrainSignal()), bytes };
}
function decodedOutput(bytes: number[]): {
  kind: number;
  channel: number;
  corr: number;
  payload: Uint8Array;
} {
  const raw = Uint8Array.from(bytes);
  const header = decodeHeader(raw.subarray(0, HEADER_LEN));
  return {
    kind: header.kind,
    channel: header.channel,
    corr: header.corr,
    payload: raw.subarray(HEADER_LEN),
  };
}

describe("Echo CALL routing", () => {
  test("one admitted handler yields one correlated Reply", async () => {
    const { writes, bytes } = captureWriter();
    let calls = 0;
    const handler: EchoCallHandler = async (_channel, payload) => {
      calls += 1;
      return Uint8Array.from([...payload, 0x7f]);
    };
    await handleEchoCall(callFrame(), handler, writes);
    const out = decodedOutput(bytes);
    expect(out.kind).toBe(FrameKind.Reply);
    expect(out.channel).toBe(ECHO_CHANNEL);
    expect(out.corr).toBe(19);
    expect(Array.from(out.payload)).toEqual([0x00, 0x00, 0x7f]);
    expect(calls).toBe(1);
  });
  test("missing or throwing handler becomes one KELD-API-001 Err", async () => {
    for (const handler of [
      undefined,
      (async () => {
        throw new Error("secret stack detail");
      }) as EchoCallHandler,
    ]) {
      const { writes, bytes } = captureWriter();
      await handleEchoCall(callFrame(), handler, writes);
      const out = decodedOutput(bytes);
      expect(out.kind).toBe(FrameKind.Err);
      expect(out.channel).toBe(ECHO_CHANNEL);
      expect(out.corr).toBe(19);
      const error = decodeCallError(out.payload);
      expect(error.code).toBe("KELD-API-001");
      expect(error.message).toBe("KELD-API-001: application channel call failed");
      expect(error.message).not.toContain("secret stack detail");
    }
  });

  test("header/session-shape failure stays link-terminal and writes nothing", async () => {
    const { writes, bytes } = captureWriter();
    const handler: EchoCallHandler = async () => new Uint8Array();
    await expect(handleEchoCall(callFrame(99), handler, writes)).rejects.toThrow(
      "KELD-IPC-005",
    );
    expect(bytes).toHaveLength(0);
  });
});

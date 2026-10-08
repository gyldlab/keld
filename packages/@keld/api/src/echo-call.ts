import {
  FrameKind,
  MAX_FRAME_LEN,
  encodeCallError,
  type WorkerCallReply,
} from "../../kipc/src/transport.ts";

export type EchoCallHandler =
  (channel: number, payload: Uint8Array) => Promise<Uint8Array>;

/**
 * The answer rule for one host Echo CALL (KEL-142; GH-527 §4.6): the
 * application handler's bytes as a REPLY, or a `KELD-API-001` ERR when the
 * handler is missing, fails, or answers above `MAX_FRAME_LEN`. The transport
 * Worker has already validated the CALL (§4.7) and writes the answer with its
 * channel and correlation id.
 */
export async function resolveEchoCall(
  channel: number,
  payload: Uint8Array,
  handler: EchoCallHandler | undefined,
): Promise<WorkerCallReply> {
  try {
    if (!handler) throw new Error("Echo handler is not registered");
    const reply = await handler(channel, payload);
    if (reply.byteLength > MAX_FRAME_LEN) {
      throw new Error("Echo reply exceeds MAX_FRAME_LEN");
    }
    return { kind: FrameKind.Reply, payload: reply };
  } catch {
    return {
      kind: FrameKind.Err,
      payload: encodeCallError(
        "KELD-API-001",
        "KELD-API-001: application channel call failed",
      ),
    };
  }
}

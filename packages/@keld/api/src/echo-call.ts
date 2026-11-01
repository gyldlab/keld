import {
  FrameKind,
  MAX_FRAME_LEN,
  encodeCallError,
  type WorkerCallReply,
} from "../../kipc/src/transport.ts";
import { dispatchApplicationCall, noApplicationHandlerRegistered } from "./channels.ts";

export type EchoCallHandler =
  (channel: number, payload: Uint8Array) => Promise<Uint8Array>;

/**
 * The answer rule for one host Echo CALL (KEL-142; GH-527 §4.6; GH-1095
 * §4.2): the application handler's bytes as a REPLY; a `KELD-API-002` ERR
 * when no handler was registered, so no app code ran; otherwise a
 * `KELD-API-001` ERR when the call fails to decode, the handler fails, or it
 * answers above `MAX_FRAME_LEN`, because app code may have run. The transport
 * Worker has already validated the CALL (§4.7) and writes the answer with its
 * channel and correlation id.
 */
export async function resolveEchoCall(
  channel: number,
  payload: Uint8Array,
  handler: EchoCallHandler | undefined,
): Promise<WorkerCallReply> {
  // Only the canonical dispatcher can prove registry absence before invocation.
  // After calling any handler, even a NoApplicationHandlerError is outcome unknown.
  if (handler === dispatchApplicationCall && noApplicationHandlerRegistered(channel)) {
    return {
      kind: FrameKind.Err,
      payload: encodeCallError(
        "KELD-API-002",
        "KELD-API-002: application channel has no registered handler; no application code ran",
      ),
    };
  }
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

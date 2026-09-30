import {
  FrameKind,
  RECEIVE_POLICIES,
  WriteQueue,
  encodeCallError,
  validateReceivedHeader,
  withIoDeadline,
  type DecodedFrame,
} from "../../kipc/src/transport.ts";

export type EchoCallHandler =
  (channel: number, payload: Uint8Array) => Promise<Uint8Array>;

interface EchoCallResult {
  kind: number;
  payload: Uint8Array;
}

async function resolveEchoCall(
  frame: DecodedFrame,
  handler: EchoCallHandler | undefined,
): Promise<EchoCallResult> {
  validateReceivedHeader(RECEIVE_POLICIES.echoReceiver, frame.header);
  try {
    if (!handler) throw new Error("Echo handler is not registered");
    return {
      kind: FrameKind.Reply,
      payload: await handler(frame.header.channel, frame.payload),
    };
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

export async function handleEchoCall(
  frame: DecodedFrame,
  handler: EchoCallHandler | undefined,
  writes: WriteQueue,
): Promise<void> {
  const reply = await resolveEchoCall(frame, handler);
  await withIoDeadline(
    writes.writeFrame(
      reply.kind,
      0,
      frame.header.channel,
      frame.header.corr,
      reply.payload,
    ),
  );
}

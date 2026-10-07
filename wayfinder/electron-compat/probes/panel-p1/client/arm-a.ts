// PANEL-P1 arm A (control) and arm C (credit) client: the kipc socket lives on the
// Bun main thread (the current @keld/kipc shape). The main thread issues a CALL and
// then parks in Atomics.wait, as a blocking host CALL on the main-thread transport
// would. With --credit W it first grants W EVENT frames over FrameKind.Grant, and
// after wake re-grants W/2 per W/2 EVENTs consumed (arm C).
import { K, PROBE_CHANNEL, OrderCheck, arg, decodeEcho, decodeProbeEvent, emit, encodeEcho, handshake, probeEventPolicy, wallUs } from "./common.ts";

const arm = arg("arm", "a");
const parkMs = Number(arg("park-ms", "12000"));
const credit = Number(arg("credit", "0"));
const expected = Number(arg("expect-events", "11000"));
const link = K.parseAppLink(process.env.KELD_APP_LINK ?? "");

let bytesIn = 0;
const reader = new K.FrameReader();
const drain = new K.DrainSignal();
const counting = {
  push(chunk: Uint8Array) {
    bytesIn += chunk.byteLength;
    reader.push(chunk);
  },
  fail(e: Error) {
    reader.fail(e);
  },
} as unknown as K.FrameReader;
const socket = await K.connectKipcSocket(link.endpoint, counting, drain);
const wq = new K.WriteQueue(socket, drain);
await handshake(wq, reader, link.token);
emit({ ev: "authenticated", arm, wall_us: wallUs() });

function grantPayload(n: number): Uint8Array {
  return K.encodeVarint(n); // postcard u32 (scratch GRANT payload)
}
if (credit > 0) {
  await wq.writeFrame(K.FrameKind.Grant, 0, PROBE_CHANNEL, 0, grantPayload(credit));
}

const corr = 1;
await wq.writeFrame(K.FrameKind.Call, 0, K.ECHO_CHANNEL, corr, encodeEcho("scenario", 7));
// Park: no Bun socket callback can run on this thread until Atomics.wait returns.
const i32 = new Int32Array(new SharedArrayBuffer(4));
const bytesBeforePark = bytesIn;
const t0 = performance.now();
const parkStartWall = wallUs();
const w = Atomics.wait(i32, 0, 0, parkMs);
const parkedMs = performance.now() - t0;
emit({
  ev: "parked",
  arm,
  wait: w,
  parked_ms: +parkedMs.toFixed(1),
  park_start_wall_us: parkStartWall,
  bytes_in_before_park: bytesBeforePark,
  bytes_in_during_park: bytesIn - bytesBeforePark,
  sync_reply_while_parked: false,
});

// After wake: drain what the link still holds, re-granting credit for arm C.
const order = new OrderCheck();
let replyAt: number | null = null;
let replyOk = false;
let consumedSinceGrant = 0;
let endError: string | null = null;
const wakeWall = wallUs();
try {
  for (;;) {
    const f = await K.withIoDeadline(reader.readFrame(), 3000);
    if (f.header.kind === K.FrameKind.Event) {
      K.validateReceivedHeader(probeEventPolicy, f.header);
      order.push(decodeProbeEvent(f.payload));
      if (credit > 0 && ++consumedSinceGrant >= credit / 2) {
        consumedSinceGrant = 0;
        await wq.writeFrame(K.FrameKind.Grant, 0, PROBE_CHANNEL, 0, grantPayload(credit / 2));
      }
      continue;
    }
    K.validateReceivedHeader(K.echoReplyWaiter(corr), f.header);
    replyOk = decodeEcho(f.payload).message === "scenario";
    replyAt = wallUs();
    break;
  }
} catch (e) {
  endError = (e as Error).message;
}
emit({
  ev: "after_wake",
  arm,
  order: order.result(expected),
  reply_received: replyAt !== null,
  reply_ok: replyOk,
  reply_after_wake_ms: replyAt === null ? null : (replyAt - wakeWall) / 1000,
  end_error: endError,
  bytes_in_total: bytesIn,
});
if (replyAt !== null) {
  // Clean end: lifecycle Quit (real LifecycleRequest::Quit = 0x00).
  await wq.writeFrame(K.FrameKind.Call, 0, K.LIFECYCLE_CHANNEL, 2, new Uint8Array([0]));
  try {
    const q = await K.withIoDeadline(reader.readFrame(), 3000);
    K.validateReceivedHeader(K.lifecycleReplyWaiter(2), q.header);
    emit({ ev: "quit", ok: q.header.kind === K.FrameKind.Reply && q.payload[0] === 0 });
  } catch (e) {
    emit({ ev: "quit", ok: false, error: (e as Error).message });
  }
}
socket.end();
process.exit(0);

/**
 * Hello echo adapter over the canonical kipc transport (KEL-136 / KEL-30).
 *
 * Framing, HELLO, deadlines, buffering, and serialized writes live in
 * `kipc-transport.ts`. This file owns echo postcard codecs and `AppLinkSession`.
 * `keld create` concatenates this file with `main-body.ts` into `src/main.ts`.
 */
export * from "./kipc-transport.ts";

import {
  CLIENT_AWAIT_HELLO,
  DirectedReader,
  DrainSignal,
  ECHO_CHANNEL,
  FrameKind,
  FrameReader,
  RECEIVE_POLICIES,
  WriteQueue,
  connectKipcSocket,
  decodeVarint,
  encodeVarint,
  echoReplyWaiter,
  kipcError,
  parseAppLink,
  timingSafeEqual,
  withIoDeadline,
  type DecodedFrame,
  type FrameKindValue,
  type ReceivePolicy,
} from "./kipc-transport.ts";

export interface EchoRequest {
  message: string;
  count: number;
}

export interface EchoResponse {
  message: string;
  count: number;
}

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder("utf-8", { fatal: true });

function encodeString(s: string): Uint8Array {
  const utf8 = textEncoder.encode(s);
  const lenPrefix = encodeVarint(utf8.length);
  const out = new Uint8Array(lenPrefix.length + utf8.length);
  out.set(lenPrefix, 0);
  out.set(utf8, lenPrefix.length);
  return out;
}

function decodeString(bytes: Uint8Array, offset: number): [string, number] {
  const [len, afterLen] = decodeVarint(bytes, offset);
  const end = afterLen + len;
  if (end > bytes.length) {
    throw kipcError("KELD-IPC-003", "truncated string payload");
  }
  return [textDecoder.decode(bytes.subarray(afterLen, end)), end];
}

/** Postcard encoding of `EchoRequest`: struct-as-tuple, field order = declaration order. */
export function encodeEchoRequest(req: EchoRequest): Uint8Array {
  const message = encodeString(req.message);
  const count = encodeVarint(req.count);
  const out = new Uint8Array(message.length + count.length);
  out.set(message, 0);
  out.set(count, message.length);
  return out;
}

/** Postcard decoding of `EchoResponse`. Rejects trailing bytes (mirrors `keld_ipc::codec::decode`). */
export function decodeEchoResponse(bytes: Uint8Array): EchoResponse {
  const [message, afterMessage] = decodeString(bytes, 0);
  const [count, afterCount] = decodeVarint(bytes, afterMessage);
  if (afterCount !== bytes.length) {
    throw kipcError("KELD-IPC-003", "trailing bytes after EchoResponse");
  }
  return { message, count };
}

/**
 * One `HELLO` plus N sequential `CALL`/`REPLY` pairs on a single app-link
 * socket. Mirrors `keld_ipc::{handshake_client, echo_invoke}`: a second
 * `HELLO` on this stream is `KELD-IPC-005`.
 *
 * `echoRoundtrip` is the one-shot wrapper (connect, one CALL, close).
 */
export class AppLinkSession {
  #socket: { end(): void };
  #reader: FrameReader;
  #drain: DrainSignal;
  #directed: DirectedReader;
  #writes: WriteQueue;
  #nextCorr = 1;
  #closed = false;

  private constructor(
    socket: { end(): void },
    reader: FrameReader,
    drain: DrainSignal,
    directed: DirectedReader,
    writes: WriteQueue,
  ) {
    this.#socket = socket;
    this.#reader = reader;
    this.#drain = drain;
    this.#directed = directed;
    this.#writes = writes;
  }

  /**
   * Connects and completes the v2 `HELLO` handshake.
   *
   * @throws on I/O, protocol, or auth failure — messages carry `KELD-IPC-*`.
   */
  static async connect(link: string): Promise<AppLinkSession> {
    const { endpoint, token } = parseAppLink(link);
    const reader = new FrameReader();
    const drain = new DrainSignal();
    const socket = await connectKipcSocket(endpoint, reader, drain);
    const writes = new WriteQueue(socket, drain);
    const directed = new DirectedReader(reader);
    const session = new AppLinkSession(socket, reader, drain, directed, writes);
    try {
      await withIoDeadline(writes.writeFrame(FrameKind.Hello, 0, 0, 0, token));
      const helloReply = await withIoDeadline(directed.receive(CLIENT_AWAIT_HELLO));
      if (!timingSafeEqual(helloReply.payload, token)) {
        throw kipcError("KELD-IPC-007", "HELLO session token mismatch");
      }
      return session;
    } catch (err) {
      session.close();
      throw err;
    }
  }

  #allocCorr(): number {
    const corr = this.#nextCorr;
    let next = (corr + 1) >>> 0;
    if (next === 0) next = 1;
    this.#nextCorr = next;
    return corr;
  }

  /** Next CALL correlation id; `0` is reserved for `HELLO`. */
  allocCorr(): number {
    return this.#allocCorr();
  }

  /** Frames parked while waiting for another policy (typically lifecycle Events). */
  parkedCount(): number {
    return this.#directed.parkedCount();
  }

  /**
   * One validated frame on this HELLO'd link. Pass
   * `RECEIVE_POLICIES.lifecycleEventReceiver` as `park` so a `Ready` Event
   * cannot fail an Echo Reply wait. Does not decode lifecycle payloads or send Quit.
   */
  async receive(want: ReceivePolicy, park?: ReceivePolicy): Promise<DecodedFrame> {
    if (this.#closed) {
      throw kipcError("KELD-IPC-001", "session is closed");
    }
    try {
      return await withIoDeadline(this.#directed.receive(want, park));
    } catch (err) {
      this.close();
      throw err;
    }
  }

  /**
   * Waits without a request deadline for an unsolicited host Event.
   *
   * A healthy window may stay open indefinitely, so LastWindowClosed cannot
   * use the bounded Echo/Quit response wait. Reader close, socket failure, and
   * validation errors still fail this same directed read and close the session.
   */
  async receiveWhileIdle(want: ReceivePolicy, park?: ReceivePolicy): Promise<DecodedFrame> {
    if (this.#closed) {
      throw kipcError("KELD-IPC-001", "session is closed");
    }
    try {
      return await this.#directed.receive(want, park);
    } catch (err) {
      this.close();
      throw err;
    }
  }

  /**
   * One serialized frame write on this HELLO'd link. Callers own channel,
   * correlation, and payload codecs (KEL-185 sends Quit here; this method does not).
   */
  async writeFrame(
    kind: FrameKindValue,
    channel: number,
    corr: number,
    payload: Uint8Array,
    flags = 0,
  ): Promise<void> {
    if (this.#closed) {
      throw kipcError("KELD-IPC-001", "session is closed");
    }
    try {
      await withIoDeadline(this.#writes.writeFrame(kind, flags, channel, corr, payload));
    } catch (err) {
      this.close();
      throw err;
    }
  }

  /**
   * One echo `Call`/`Reply` on this connection. Does not handshake again.
   * Lifecycle Events that arrive before the Reply are parked, not treated as
   * a protocol violation.
   *
   * @throws on I/O, protocol, or codec error — messages carry `KELD-IPC-*`.
   */
  async echo(request: EchoRequest): Promise<EchoResponse> {
    if (this.#closed) {
      throw kipcError("KELD-IPC-001", "session is closed");
    }
    const corr = this.#allocCorr();
    const payload = encodeEchoRequest(request);
    await this.writeFrame(FrameKind.Call, ECHO_CHANNEL, corr, payload);
    const reply = await this.receive(
      echoReplyWaiter(corr),
      RECEIVE_POLICIES.lifecycleEventReceiver,
    );
    return decodeEchoResponse(reply.payload);
  }

  /**
   * Ends the socket and wakes leftover I/O. `withIoDeadline` does not cancel
   * the inner promise; fail/fire here so a timed-out read cannot leave
   * `FrameReader.#pending` set and a timed-out write cannot stay in
   * `DrainSignal.wait()`. Safe to call more than once.
   */
  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#reader.fail(kipcError("KELD-IPC-001", "session is closed"));
    this.#drain.fire();
    this.#socket.end();
  }
}

/**
 * Performs one echo round-trip: connect, `HELLO` handshake, one `Call`/`Reply`.
 *
 * One-shot wrapper over [`AppLinkSession`]. `link` is the `KELD_APP_LINK`
 * value (`<endpoint>#<64 hex chars>`).
 *
 * @throws on I/O failure, protocol mismatch, auth failure, or codec error —
 * error messages carry the matching `KELD-IPC-*` code from `keld-ipc`.
 */
export async function echoRoundtrip(link: string, request: EchoRequest): Promise<EchoResponse> {
  const session = await AppLinkSession.connect(link);
  try {
    return await session.echo(request);
  } finally {
    session.close();
  }
}

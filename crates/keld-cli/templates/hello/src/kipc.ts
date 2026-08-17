/**
 * kipc v2 client — hand-written, wire-exact (KEL-30).
 *
 * Full schema-driven codegen (`keld gen`, `@keld/schema`) is a later slice;
 * this is the v0 vertical slice speaking the same bytes as `keld-ipc`
 * (`crates/keld-ipc/src/{frame,codec,link,echo,session}.rs`,
 * `docs/architecture/02-ipc.md`). Every constant and byte layout here is
 * copied from that crate, not reverse-engineered — see `kipc.test.ts` for
 * cross-language golden vectors pinned against the Rust source.
 *
 * Wire summary:
 * - Frame header: 16 bytes LE — magic:u16 "KI" | ver:u8 | kind:u8 | flags:u16
 *   | channel:u16 | corr:u32 | len:u32.
 * - `HELLO` payload: 32 raw bytes (the session token from `KELD_APP_LINK`),
 *   not postcard-encoded.
 * - `Call`/`Reply` payload: postcard — LEB128 unsigned varints, strings as a
 *   byte-length varint followed by UTF-8 bytes, structs as concatenated
 *   fields in declaration order (no field names, no struct-level framing).
 */

const MAGIC_BYTES = new Uint8Array([0x4b, 0x49]); // "KI", matches Rust `u16::from_le_bytes(*b"KI")`
const PROTOCOL_VERSION = 2;
const HEADER_LEN = 16;
const ECHO_CHANNEL = 1;
/** Control-plane frame payload cap — mirrors `keld_ipc::MAX_FRAME_LEN` (16 MiB). */
const MAX_FRAME_LEN = 16 * 1024 * 1024;

/** Frame kinds carried in the header's `kind` byte — mirrors `keld_ipc::FrameKind`. */
export const FrameKind = {
  Hello: 0,
  Call: 1,
  Reply: 2,
  Err: 3,
  Event: 4,
  StreamOpen: 5,
  StreamChunk: 6,
  StreamClose: 7,
  Cancel: 8,
  Grant: 9,
  Ping: 10,
} as const;

export type FrameKindValue = (typeof FrameKind)[keyof typeof FrameKind];

const KNOWN_FRAME_KINDS: ReadonlySet<number> = new Set(Object.values(FrameKind));

export interface FrameHeader {
  kind: FrameKindValue;
  flags: number;
  channel: number;
  corr: number;
  len: number;
}

export interface DecodedFrame {
  header: FrameHeader;
  payload: Uint8Array;
}

export interface EchoRequest {
  message: string;
  count: number;
}

export interface EchoResponse {
  message: string;
  count: number;
}

function kipcError(code: string, detail: string): Error {
  return new Error(`${code}: ${detail}`);
}

// --- frame header ------------------------------------------------------

export function encodeHeader(h: FrameHeader): Uint8Array {
  const out = new Uint8Array(HEADER_LEN);
  const view = new DataView(out.buffer);
  out.set(MAGIC_BYTES, 0);
  out[2] = PROTOCOL_VERSION;
  out[3] = h.kind;
  view.setUint16(4, h.flags, true);
  view.setUint16(6, h.channel, true);
  view.setUint32(8, h.corr, true);
  view.setUint32(12, h.len, true);
  return out;
}

export function decodeHeader(bytes: Uint8Array): FrameHeader {
  if (bytes.length < HEADER_LEN) {
    throw kipcError(
      "KELD-IPC-002",
      `short frame header: ${bytes.length} bytes (expected ${HEADER_LEN})`,
    );
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes[0] !== MAGIC_BYTES[0] || bytes[1] !== MAGIC_BYTES[1]) {
    const magic = view.getUint16(0, true);
    throw kipcError(
      "KELD-IPC-002",
      `bad kipc magic: 0x${magic.toString(16).padStart(4, "0")} (expected 0x494b 'KI')`,
    );
  }
  const version = bytes[2];
  if (version !== PROTOCOL_VERSION) {
    throw kipcError(
      "KELD-IPC-002",
      `unsupported kipc version: ${version} (expected ${PROTOCOL_VERSION})`,
    );
  }
  const kindByte = bytes[3];
  if (!KNOWN_FRAME_KINDS.has(kindByte)) {
    throw kipcError("KELD-IPC-002", `unknown kipc frame kind: ${kindByte} (valid kinds are 0..=10)`);
  }
  return {
    kind: kindByte as FrameKindValue,
    flags: view.getUint16(4, true),
    channel: view.getUint16(6, true),
    corr: view.getUint32(8, true),
    len: view.getUint32(12, true),
  };
}

// --- postcard payload codec (varint LEB128 + UTF-8 strings) -----------

/** Encodes a non-negative integer (up to u32 range) as an unsigned LEB128 varint. */
export function encodeVarint(n: number): Uint8Array {
  if (!Number.isInteger(n) || n < 0) {
    throw kipcError("KELD-IPC-003", `varint value must be a non-negative integer, got ${n}`);
  }
  const bytes: number[] = [];
  let v = n;
  do {
    // Division, not `>>>`, so values above 2**31 (still valid u32) are not
    // truncated by JS's 32-bit bitwise operators.
    let byte = v % 128;
    v = Math.floor(v / 128);
    if (v !== 0) byte |= 0x80;
    bytes.push(byte);
  } while (v !== 0);
  return new Uint8Array(bytes);
}

/** Decodes an unsigned LEB128 varint starting at `offset`. Returns `[value, nextOffset]`. */
export function decodeVarint(bytes: Uint8Array, offset: number): [number, number] {
  let result = 0;
  let placeValue = 1;
  let pos = offset;
  for (;;) {
    if (pos >= bytes.length) {
      throw kipcError("KELD-IPC-003", "truncated varint");
    }
    const byte = bytes[pos];
    pos += 1;
    result += (byte & 0x7f) * placeValue;
    if ((byte & 0x80) === 0) break;
    placeValue *= 128;
  }
  return [result, pos];
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
  const count = encodeVarint(req.count >>> 0);
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

// --- app-link parsing ----------------------------------------------------

export interface AppLink {
  endpoint: string;
  token: Uint8Array;
}

/** Parses `<endpoint>#<64 hex chars>` — splits on the LAST `#`, matching `parse_app_link`. */
export function parseAppLink(link: string): AppLink {
  const hashIndex = link.lastIndexOf("#");
  if (hashIndex <= 0) {
    throw kipcError("KELD-IPC-007", "KELD_APP_LINK must be <endpoint>#<64 hex chars>");
  }
  const endpoint = link.slice(0, hashIndex);
  const hex = link.slice(hashIndex + 1);
  if (!/^[0-9a-fA-F]{64}$/.test(hex)) {
    throw kipcError("KELD-IPC-007", "KELD_APP_LINK token must be 64 hex characters");
  }
  const token = new Uint8Array(32);
  for (let i = 0; i < 32; i += 1) {
    token[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return { endpoint, token };
}

function timingSafeEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i += 1) diff |= a[i] ^ b[i];
  return diff === 0;
}

// --- incremental frame reader --------------------------------------------

/**
 * Buffers socket chunks and resolves one `readFrame()` call per complete
 * frame. Exported for `kipc.test.ts` to feed it untrusted byte sequences
 * directly (oversized `len`, malformed headers) without a live socket — not
 * meant as a public transport API.
 *
 * v0 usage is strictly sequential per CALL (write, then await one reply),
 * so a single pending waiter is enough — no queue needed. Multiple CALLs
 * on one connection are issued one after another by `AppLinkSession`.
 */
export class FrameReader {
  #buf = new Uint8Array(0);
  #pending: { resolve: (f: DecodedFrame) => void; reject: (e: Error) => void } | null = null;
  #closed = false;
  #closeError: Error | null = null;

  push(chunk: Uint8Array): void {
    const merged = new Uint8Array(this.#buf.length + chunk.length);
    merged.set(this.#buf, 0);
    merged.set(chunk, this.#buf.length);
    this.#buf = merged;
    this.#tryResolve();
  }

  fail(err: Error): void {
    this.#closed = true;
    this.#closeError = err;
    const pending = this.#pending;
    this.#pending = null;
    pending?.reject(err);
  }

  #tryResolve(): void {
    if (!this.#pending || this.#buf.length < HEADER_LEN) return;
    let header: FrameHeader;
    try {
      header = decodeHeader(this.#buf);
    } catch (err) {
      // A malformed header is unrecoverable — later bytes cannot be
      // reinterpreted as a fresh frame boundary. Fail the whole reader
      // instead of throwing out of a socket `data` callback, matching how
      // `fail()` is used for every other terminal transport error.
      this.fail(err instanceof Error ? err : kipcError("KELD-IPC-002", String(err)));
      return;
    }
    if (header.len > MAX_FRAME_LEN) {
      // Same guard as `keld_ipc::link::ensure_payload_len`: reject before
      // waiting for that many bytes to arrive, so a forged length cannot
      // wedge the reader waiting on data that will never come.
      this.fail(
        kipcError(
          "KELD-IPC-004",
          `frame payload exceeds MAX_FRAME_LEN (${MAX_FRAME_LEN} bytes). ` +
            "Shrink the payload or move large transfers to the bulk plane.",
        ),
      );
      return;
    }
    const total = HEADER_LEN + header.len;
    if (this.#buf.length < total) return;
    const payload = this.#buf.slice(HEADER_LEN, total);
    this.#buf = this.#buf.slice(total);
    const pending = this.#pending;
    this.#pending = null;
    pending?.resolve({ header, payload });
  }

  readFrame(): Promise<DecodedFrame> {
    if (this.#closed) {
      return Promise.reject(this.#closeError ?? kipcError("KELD-IPC-001", "connection closed"));
    }
    return new Promise((resolve, reject) => {
      this.#pending = { resolve, reject };
      this.#tryResolve();
    });
  }
}

/** Single-slot "wait for the next drain event" signal for backpressure. */
class DrainSignal {
  #waiter: (() => void) | null = null;

  fire(): void {
    const waiter = this.#waiter;
    this.#waiter = null;
    waiter?.();
  }

  wait(): Promise<void> {
    return new Promise((resolve) => {
      this.#waiter = resolve;
    });
  }
}

// --- socket transport ------------------------------------------------

interface KipcSocket {
  write(data: Uint8Array): number;
  end(): void;
}

async function writeFrame(
  socket: KipcSocket,
  drain: DrainSignal,
  kind: FrameKindValue,
  flags: number,
  channel: number,
  corr: number,
  payload: Uint8Array,
): Promise<void> {
  const header = encodeHeader({ kind, flags, channel, corr, len: payload.length });
  const frame = new Uint8Array(header.length + payload.length);
  frame.set(header, 0);
  frame.set(payload, header.length);

  let offset = 0;
  while (offset < frame.length) {
    const written = socket.write(frame.subarray(offset));
    if (written < 0) {
      throw kipcError("KELD-IPC-001", "socket closed during write");
    }
    offset += written;
    if (written === 0) {
      // Backpressure: the OS send buffer is full. Wait for `drain`, matching
      // the documented Bun.Socket contract, rather than busy-spinning.
      await drain.wait();
    }
  }
}

/**
 * One `HELLO` plus N sequential `CALL`/`REPLY` pairs on a single app-link
 * socket. Mirrors `keld_ipc::{handshake_client, echo_invoke}`: a second
 * `HELLO` on this stream is `KELD-IPC-005`.
 *
 * `echoRoundtrip` is the one-shot wrapper (connect, one CALL, close).
 */
export class AppLinkSession {
  #socket: KipcSocket;
  #reader: FrameReader;
  #drain: DrainSignal;
  #nextCorr = 1;
  #closed = false;

  private constructor(socket: KipcSocket, reader: FrameReader, drain: DrainSignal) {
    this.#socket = socket;
    this.#reader = reader;
    this.#drain = drain;
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

    const handlers = {
      binaryType: "uint8array" as const,
      data(_socket: unknown, data: Uint8Array) {
        reader.push(data);
      },
      drain(_socket: unknown) {
        drain.fire();
      },
      error(_socket: unknown, err: Error) {
        reader.fail(kipcError("KELD-IPC-001", err.message));
      },
      close(_socket: unknown) {
        reader.fail(kipcError("KELD-IPC-001", "connection closed by peer"));
      },
      connectError(_socket: unknown, err: Error) {
        reader.fail(kipcError("KELD-IPC-001", err.message));
      },
    };

    const socket: KipcSocket =
      process.platform === "win32"
        ? await Bun.connect({
            hostname: "127.0.0.1",
            port: Number.parseInt(endpoint, 10),
            socket: handlers,
          })
        : await Bun.connect({
            unix: endpoint,
            socket: handlers,
          });

    const session = new AppLinkSession(socket, reader, drain);
    try {
      await writeFrame(socket, drain, FrameKind.Hello, 0, 0, 0, token);
      const helloReply = await reader.readFrame();
      if (helloReply.header.kind !== FrameKind.Hello) {
        throw kipcError("KELD-IPC-005", "expected HELLO from peer");
      }
      if (helloReply.header.channel !== 0 || helloReply.header.corr !== 0) {
        throw kipcError("KELD-IPC-005", "HELLO must have reserved channel/corr 0");
      }
      if (helloReply.payload.length !== 32) {
        throw kipcError("KELD-IPC-007", "HELLO payload must be a 32-byte session token");
      }
      if (!timingSafeEqual(helloReply.payload, token)) {
        throw kipcError("KELD-IPC-007", "HELLO session token mismatch");
      }
      return session;
    } catch (err) {
      session.close();
      throw err;
    }
  }

  /** Next CALL correlation id; `0` is reserved for `HELLO`. */
  #allocCorr(): number {
    const corr = this.#nextCorr;
    let next = (corr + 1) >>> 0;
    if (next === 0) next = 1;
    this.#nextCorr = next;
    return corr;
  }

  /**
   * One echo `Call`/`Reply` on this connection. Does not handshake again.
   *
   * @throws on I/O, protocol, or codec error — messages carry `KELD-IPC-*`.
   */
  async echo(request: EchoRequest): Promise<EchoResponse> {
    if (this.#closed) {
      throw kipcError("KELD-IPC-001", "session is closed");
    }
    const corr = this.#allocCorr();
    const payload = encodeEchoRequest(request);
    await writeFrame(this.#socket, this.#drain, FrameKind.Call, 0, ECHO_CHANNEL, corr, payload);

    const reply = await this.#reader.readFrame();
    if (
      reply.header.kind !== FrameKind.Reply ||
      reply.header.corr !== corr ||
      reply.header.channel !== ECHO_CHANNEL
    ) {
      throw kipcError("KELD-IPC-005", "expected REPLY for echo CALL");
    }
    return decodeEchoResponse(reply.payload);
  }

  /** Ends the socket. Safe to call more than once. */
  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#socket.end();
  }
}

/**
 * Performs one echo round-trip: connect, `HELLO` handshake, one `Call`/`Reply`.
 *
 * One-shot wrapper over [`AppLinkSession`]. `link` is the `KELD_APP_LINK`
 * value (`<endpoint>#<64 hex chars>`); on Windows the endpoint is a loopback
 * TCP port, on Unix a domain socket path — the same branch
 * `crates/keld-cli/src/echo_link.rs::echo_roundtrip` takes, decided by
 * platform (not by sniffing the endpoint string).
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

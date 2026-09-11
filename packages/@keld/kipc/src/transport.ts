/**
 * Canonical TypeScript kipc v2 app-link transport (KEL-136).
 *
 * Wire constants match `keld_ipc::{frame,lib,echo,lifecycle}` — not reverse-engineered.
 * `keld create` embeds this file as `src/kipc-transport.ts`. `@keld/electron` imports
 * it. Do not add a second reader/writer/constant owner.
 *
 * Frame layout: 16-byte LE header, HELLO token is raw 32 bytes, payloads are postcard.
 */

const MAGIC_BYTES = new Uint8Array([0x4b, 0x49]); // "KI", matches Rust `u16::from_le_bytes(*b"KI")`
/** Mirrors `keld_ipc::PROTOCOL_VERSION`. */
export const PROTOCOL_VERSION = 2;
/** Mirrors `keld_ipc::HEADER_LEN`. */
export const HEADER_LEN = 16;
/** Control-plane frame payload cap — mirrors `keld_ipc::MAX_FRAME_LEN` (16 MiB). */
export const MAX_FRAME_LEN = 16 * 1024 * 1024;
/** Mirrors `keld_ipc::echo::ECHO_CHANNEL`. */
export const ECHO_CHANNEL = 1;
/** Mirrors `keld_ipc::LIFECYCLE_CHANNEL`. */
export const LIFECYCLE_CHANNEL = 3;
/** Mirrors `keld_ipc::APP_LINK_IO_DEADLINE` (arch/02 §7). Bun has no `SO_RCVTIMEO`. */
export const APP_LINK_IO_DEADLINE_MS = 5_000;
/** Header flag mirroring `keld_ipc::frame::FLAG_RAW`. */
export const FLAG_RAW = 1 << 0;

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
  kind: number;
  flags: number;
  channel: number;
  corr: number;
  len: number;
}

export interface DecodedFrame {
  header: FrameHeader;
  payload: Uint8Array;
}

export interface AppLink {
  endpoint: string;
  token: Uint8Array;
}

export interface KipcSocket {
  write(data: Uint8Array): number;
  end(): void;
}

/**
 * Mirror of `keld_ipc::receive::ReceivePolicy` (KEL-133 spec §4): the
 * host/app-selected static semantic contract for one receiver state. The
 * shared corpus `crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv`
 * is the single semantic table both languages are tested against; this
 * implementation is a consumer of that contract, not a second owner.
 */
export interface ReceivePolicy {
  /** Declared channel for the structured kinds. */
  channel: number;
  /** Second declared channel for the one multiplexed primary session. */
  alsoChannel?: number;
  /** Structured frame kinds this policy admits. */
  kinds: readonly number[];
  /** Correlation rule: exact zero, any nonzero, or one awaited id. */
  corr: { rule: "zero" } | { rule: "non-zero" } | { rule: "exactly"; id: number };
  /** Exact declared payload length, if the policy pins one. */
  exactLen?: number;
  /** Whether the payload must be empty. */
  emptyPayload?: boolean;
  /** Whether the live v0 `PING` probe is admissible. */
  allowPing?: boolean;
}

export const RECEIVE_POLICIES = {
  serverPreAuthHello: {
    channel: 0,
    kinds: [FrameKind.Hello],
    corr: { rule: "zero" },
    exactLen: 32,
  } as ReceivePolicy,
  clientAwaitHello: {
    channel: 0,
    kinds: [FrameKind.Hello],
    corr: { rule: "zero" },
    exactLen: 32,
  } as ReceivePolicy,
  echoReceiver: {
    channel: ECHO_CHANNEL,
    kinds: [FrameKind.Call],
    corr: { rule: "non-zero" },
    allowPing: true,
  } as ReceivePolicy,
  lifecycleReceiver: {
    channel: LIFECYCLE_CHANNEL,
    kinds: [FrameKind.Call],
    corr: { rule: "non-zero" },
    allowPing: true,
  } as ReceivePolicy,
  lifecycleEventReceiver: {
    channel: LIFECYCLE_CHANNEL,
    kinds: [FrameKind.Event],
    corr: { rule: "zero" },
    allowPing: true,
  } as ReceivePolicy,
} as const;

/** Scaffold alias for `RECEIVE_POLICIES.clientAwaitHello`. */
export const CLIENT_AWAIT_HELLO: ReceivePolicy = RECEIVE_POLICIES.clientAwaitHello;

export function echoReplyWaiter(corr: number): ReceivePolicy {
  return { channel: ECHO_CHANNEL, kinds: [FrameKind.Reply], corr: { rule: "exactly", id: corr } };
}

export function lifecycleReplyWaiter(corr: number): ReceivePolicy {
  return {
    channel: LIFECYCLE_CHANNEL,
    kinds: [FrameKind.Reply, FrameKind.Err],
    corr: { rule: "exactly", id: corr },
  };
}

export function privilegedCallReceiver(channel: number): ReceivePolicy {
  return { channel, kinds: [FrameKind.Call], corr: { rule: "non-zero" } };
}

export function primaryAppReceiver(): ReceivePolicy {
  return {
    channel: ECHO_CHANNEL,
    alsoChannel: LIFECYCLE_CHANNEL,
    kinds: [FrameKind.Call],
    corr: { rule: "non-zero" },
    allowPing: true,
  };
}

export function kipcError(code: string, detail: string): Error {
  return new Error(`${code}: ${detail}`);
}

function ioDeadlineExceeded(): Error {
  return kipcError(
    "KELD-IPC-006",
    "app-link I/O deadline exceeded. Check the peer is still running and sending kipc frames; a silent or wedged process will not be waited on forever.",
  );
}

export function encodeHeader(header: FrameHeader): Uint8Array;
export function encodeHeader(
  kind: number,
  flags: number,
  channel: number,
  corr: number,
  len: number,
): Uint8Array;
export function encodeHeader(
  kindOrHeader: number | FrameHeader,
  flags?: number,
  channel?: number,
  corr?: number,
  len?: number,
): Uint8Array {
  const h: FrameHeader =
    typeof kindOrHeader === "object"
      ? kindOrHeader
      : {
          kind: kindOrHeader,
          flags: flags ?? 0,
          channel: channel ?? 0,
          corr: corr ?? 0,
          len: len ?? 0,
        };
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
    kind: kindByte,
    flags: view.getUint16(4, true),
    channel: view.getUint16(6, true),
    corr: view.getUint32(8, true),
    len: view.getUint32(12, true),
  };
}

/**
 * Mirror of `keld_ipc::receive::validate_received_header` with the same
 * fixed check order (kind → flags → channel → correlation → declared length)
 * and the same `KELD-IPC-005` details, so both languages produce identical
 * corpus results. Throws; returns the header unchanged on admission.
 */
export function validateReceivedHeader(policy: ReceivePolicy, header: FrameHeader): FrameHeader {
  if (policy.allowPing === true && header.kind === FrameKind.Ping) {
    if (header.flags !== 0) {
      throw kipcError("KELD-IPC-005", "PING flags must be 0");
    }
    if (header.len !== 0) {
      throw kipcError("KELD-IPC-005", "PING payload must be empty");
    }
    return header;
  }
  if (!policy.kinds.includes(header.kind)) {
    throw kipcError("KELD-IPC-005", "frame kind is not declared by the session policy");
  }
  if ((header.flags & FLAG_RAW) !== 0) {
    throw kipcError("KELD-IPC-005", "FLAG_RAW is invalid for a structured session");
  }
  if (header.flags !== 0) {
    throw kipcError("KELD-IPC-005", "unknown flag bits are reserved");
  }
  if (header.channel !== policy.channel && header.channel !== policy.alsoChannel) {
    throw kipcError("KELD-IPC-005", "wrong channel for the session policy");
  }
  switch (policy.corr.rule) {
    case "zero":
      if (header.corr !== 0) {
        throw kipcError("KELD-IPC-005", "correlation must be 0 for this frame");
      }
      break;
    case "non-zero":
      if (header.corr === 0) {
        throw kipcError("KELD-IPC-005", "correlation 0 is reserved");
      }
      break;
    case "exactly":
      if (header.corr !== policy.corr.id) {
        throw kipcError("KELD-IPC-005", "correlation does not match the awaited call");
      }
      break;
  }
  if (policy.exactLen !== undefined && header.len !== policy.exactLen) {
    throw kipcError("KELD-IPC-005", "payload length does not match the declared exact shape");
  }
  if (policy.emptyPayload === true && header.len !== 0) {
    throw kipcError("KELD-IPC-005", "payload must be empty for this frame");
  }
  return header;
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

/** True only for a host-minted Keld Windows named-pipe endpoint. */
export function isWin32PipeEndpoint(endpoint: string): boolean {
  return /^\\\\\.\\pipe\\keld-[0-9a-f]{64}$/.test(endpoint);
}

/**
 * Legacy Windows diagnostic `KELD_APP_LINK` endpoints are strict decimal
 * loopback ports — not `Number.parseInt`, which accepts `"127.0.0.1:9000"` as `127`.
 */
export function parseWin32DiagnosticPort(endpoint: string): number {
  if (!/^[1-9][0-9]{0,4}$/.test(endpoint)) {
    throw kipcError(
      "KELD-IPC-007",
      "KELD_APP_LINK Windows endpoint must be an exact Keld pipe or decimal diagnostic port",
    );
  }
  const port = Number(endpoint);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw kipcError(
      "KELD-IPC-007",
      "KELD_APP_LINK Windows endpoint must be an exact Keld pipe or decimal diagnostic port",
    );
  }
  return port;
}

/** Alias retained for `@keld/electron` call sites. */
export const parseWin32Port = parseWin32DiagnosticPort;

export function timingSafeEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i += 1) diff |= a[i] ^ b[i];
  return diff === 0;
}

/**
 * Buffers socket chunks as a queue plus cursor (linear work, bounded memory).
 * Prefix-concatenating the whole buffer on every chunk is quadratic on a
 * fragmented max-size frame.
 *
 * One in-flight `readFrame()` only. A second call while a waiter is set is
 * `KELD-IPC-005`; it must not overwrite the waiter.
 */
export class FrameReader {
  #chunks: Uint8Array[] = [];
  #head = 0;
  #length = 0;
  #pending: { resolve: (f: DecodedFrame) => void; reject: (e: Error) => void } | null = null;
  #closed = false;
  #closeError: Error | null = null;

  /** Unread buffered bytes. Independent of how many socket chunks carried them. */
  bufferedBytes(): number {
    return this.#length;
  }

  /**
   * Unread socket chunks. A prefix-concat reader collapses this to 1 after the
   * second `push`; the chunk-queue reader keeps one entry per unread chunk.
   */
  pendingChunkCount(): number {
    return this.#chunks.length;
  }

  push(chunk: Uint8Array): void {
    if (this.#closed || chunk.byteLength === 0) return;
    this.#chunks.push(chunk);
    this.#length += chunk.byteLength;
    this.#tryResolve();
  }

  fail(err: Error): void {
    this.#closed = true;
    this.#closeError = err;
    this.#chunks = [];
    this.#head = 0;
    this.#length = 0;
    const pending = this.#pending;
    this.#pending = null;
    pending?.reject(err);
  }

  #copyOut(n: number): Uint8Array {
    const out = new Uint8Array(n);
    let written = 0;
    let idx = 0;
    let offset = this.#head;
    while (written < n) {
      const chunk = this.#chunks[idx];
      if (chunk === undefined) {
        throw kipcError("KELD-IPC-001", "frame reader underrun");
      }
      const take = Math.min(chunk.byteLength - offset, n - written);
      out.set(chunk.subarray(offset, offset + take), written);
      written += take;
      idx += 1;
      offset = 0;
    }
    return out;
  }

  #consume(n: number): void {
    let left = n;
    this.#length -= n;
    while (left > 0) {
      const chunk = this.#chunks[0];
      if (chunk === undefined) {
        throw kipcError("KELD-IPC-001", "frame reader underrun");
      }
      const avail = chunk.byteLength - this.#head;
      if (left < avail) {
        this.#head += left;
        return;
      }
      left -= avail;
      this.#chunks.shift();
      this.#head = 0;
    }
  }

  #tryResolve(): void {
    if (!this.#pending || this.#length < HEADER_LEN) return;
    let header: FrameHeader;
    try {
      header = decodeHeader(this.#copyOut(HEADER_LEN));
    } catch (err) {
      this.fail(err instanceof Error ? err : kipcError("KELD-IPC-002", String(err)));
      return;
    }
    if (header.len > MAX_FRAME_LEN) {
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
    if (this.#length < total) return;
    this.#consume(HEADER_LEN);
    const payload = header.len === 0 ? new Uint8Array(0) : this.#copyOut(header.len);
    if (header.len > 0) this.#consume(header.len);
    const pending = this.#pending;
    this.#pending = null;
    pending?.resolve({ header, payload });
  }

  readFrame(): Promise<DecodedFrame> {
    if (this.#closed) {
      return Promise.reject(this.#closeError ?? kipcError("KELD-IPC-001", "connection closed"));
    }
    if (this.#pending) {
      return Promise.reject(
        kipcError(
          "KELD-IPC-005",
          "overlapping readFrame(); FrameReader allows one in-flight read. Await the first read before calling readFrame again.",
        ),
      );
    }
    return new Promise((resolve, reject) => {
      this.#pending = { resolve, reject };
      this.#tryResolve();
    });
  }
}

/** Bounded parked-frame budget for one app-link mux (Ready + LastWindowClosed + PING). */
export const MAX_PARKED_FRAMES = 8;

function admitsHeader(policy: ReceivePolicy, header: FrameHeader): boolean {
  try {
    validateReceivedHeader(policy, header);
    return true;
  } catch (err) {
    if (err instanceof Error && err.message.startsWith("KELD-IPC-")) {
      return false;
    }
    throw err;
  }
}

/**
 * Single reader of one `FrameReader` that can wait for a policy while parking
 * unmatched lifecycle Events/PINGs. Stock echo must not treat a preceding
 * `Ready` Event as a failed Echo Reply (KEL-185 same-stream seam). This is
 * not a Close/Quit consumer: adapters still own codecs and when to Quit.
 */
export class DirectedReader {
  readonly #reader: FrameReader;
  readonly #parked: DecodedFrame[] = [];

  constructor(reader: FrameReader) {
    this.#reader = reader;
  }

  /** Frames parked because they matched `park` while waiting for another policy. */
  parkedCount(): number {
    return this.#parked.length;
  }

  /**
   * Returns the next frame admitted by `want`. When `park` is set, admitted
   * `park` frames are queued in arrival order instead of failing the wait.
   * Anything else stays `KELD-IPC-005`. Park overflow is also `KELD-IPC-005`.
   */
  async receive(want: ReceivePolicy, park?: ReceivePolicy): Promise<DecodedFrame> {
    const parkedHit = this.#takeParked(want);
    if (parkedHit !== undefined) {
      return parkedHit;
    }
    for (;;) {
      const frame = await this.#reader.readFrame();
      if (admitsHeader(want, frame.header)) {
        return frame;
      }
      if (park !== undefined && admitsHeader(park, frame.header)) {
        if (this.#parked.length >= MAX_PARKED_FRAMES) {
          throw kipcError(
            "KELD-IPC-005",
            "parked-frame queue is full. Drain parked lifecycle frames before waiting for another policy.",
          );
        }
        this.#parked.push(frame);
        continue;
      }
      validateReceivedHeader(want, frame.header);
      throw kipcError("KELD-IPC-005", "frame kind is not declared by the session policy");
    }
  }

  #takeParked(want: ReceivePolicy): DecodedFrame | undefined {
    for (let i = 0; i < this.#parked.length; i += 1) {
      const frame = this.#parked[i];
      if (frame !== undefined && admitsHeader(want, frame.header)) {
        this.#parked.splice(i, 1);
        return frame;
      }
    }
    return undefined;
  }
}

/**
 * Wakes every waiter parked on `wait()`. A single-slot signal drops the
 * first waiter when ping-reply and `quit()` both hit backpressure.
 */
export class DrainSignal {
  #waiters: Array<() => void> = [];

  fire(): void {
    const waiters = this.#waiters;
    this.#waiters = [];
    for (const waiter of waiters) waiter();
  }

  wait(): Promise<void> {
    return new Promise((resolve) => {
      this.#waiters.push(resolve);
    });
  }
}

async function writeOneFrame(
  socket: KipcSocket,
  drain: DrainSignal,
  kind: number,
  flags: number,
  channel: number,
  corr: number,
  payload: Uint8Array,
): Promise<void> {
  const header = encodeHeader(kind, flags, channel, corr, payload.length);
  const frame = new Uint8Array(header.length + payload.length);
  frame.set(header, 0);
  frame.set(payload, header.length);
  const deadlineAt = Date.now() + APP_LINK_IO_DEADLINE_MS;
  let offset = 0;
  while (offset < frame.length) {
    const remaining = deadlineAt - Date.now();
    if (remaining <= 0) {
      throw ioDeadlineExceeded();
    }
    const written = socket.write(frame.subarray(offset));
    if (written < 0) {
      throw kipcError("KELD-IPC-001", "socket closed during write");
    }
    offset += written;
    if (written === 0) {
      await withIoDeadline(drain.wait(), remaining);
    }
  }
}

/**
 * One-at-a-time frame writer. Concurrent `writeOneFrame` calls interleave
 * bytes on the stream. The first failure poisons the queue.
 */
export class WriteQueue {
  #chain: Promise<void> = Promise.resolve();
  #socket: KipcSocket;
  #drain: DrainSignal;
  #poison: Error | null = null;

  constructor(socket: KipcSocket, drain: DrainSignal) {
    this.#socket = socket;
    this.#drain = drain;
  }

  writeFrame(
    kind: number,
    flags: number,
    channel: number,
    corr: number,
    payload: Uint8Array,
  ): Promise<void> {
    if (this.#poison) {
      return Promise.reject(this.#poison);
    }
    const run = this.#chain.then(() => {
      if (this.#poison) {
        throw this.#poison;
      }
      return writeOneFrame(this.#socket, this.#drain, kind, flags, channel, corr, payload);
    });
    this.#chain = run.then(
      () => undefined,
      () => {
        this.#poison ??= kipcError(
          "KELD-IPC-001",
          "write queue stopped after a previous write failed. Close the session and open a new app-link; do not send another frame after a truncated write.",
        );
      },
    );
    return run;
  }
}

export async function withIoDeadline<T>(
  promise: Promise<T>,
  deadlineMs: number = APP_LINK_IO_DEADLINE_MS,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      reject(ioDeadlineExceeded());
    }, deadlineMs);
  });
  try {
    return await Promise.race([promise, timeout]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
    void promise.catch(() => undefined);
  }
}

/**
 * Connects the v0 app-link socket. Windows named pipes use the same `unix:`
 * option as Unix domain sockets (KEL-101); only the retained decimal diagnostic
 * port uses TCP loopback.
 */
export async function connectKipcSocket(
  endpoint: string,
  reader: FrameReader,
  drain: DrainSignal,
): Promise<KipcSocket> {
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
      drain.fire();
    },
    close(_socket: unknown) {
      reader.fail(kipcError("KELD-IPC-001", "connection closed by peer"));
      drain.fire();
    },
    connectError(_socket: unknown, err: Error) {
      reader.fail(kipcError("KELD-IPC-001", err.message));
      drain.fire();
    },
  };
  const socket =
    process.platform === "win32" && !isWin32PipeEndpoint(endpoint)
      ? await Bun.connect({
          hostname: "127.0.0.1",
          port: parseWin32DiagnosticPort(endpoint),
          socket: handlers,
        })
      : await Bun.connect({
          unix: endpoint,
          socket: handlers,
        });
  return socket;
}

/** Encodes a `u32` as an unsigned LEB128 varint. Rejects anything outside that range. */
export function encodeVarint(n: number): Uint8Array {
  if (!Number.isInteger(n) || n < 0 || n > 0xffff_ffff) {
    throw kipcError("KELD-IPC-003", `varint value must be an integer in [0, 4294967295], got ${n}`);
  }
  const bytes: number[] = [];
  let v = n;
  do {
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

/**
 * Reads one postcard string starting at `offset`; returns it with the index
 * one past its last byte.
 */
export function decodePostcardStringAt(bytes: Uint8Array, offset: number): [string, number] {
  let len = 0;
  let shift = 0;
  let i = offset;
  while (i < bytes.length) {
    const b = bytes[i]!;
    i += 1;
    len |= (b & 0x7f) << shift;
    if ((b & 0x80) === 0) {
      const text = bytes.subarray(i, i + len);
      if (text.length !== len) {
        throw kipcError("KELD-IPC-003", "postcard string length does not match payload");
      }
      return [new TextDecoder("utf-8", { fatal: true }).decode(text), i + len];
    }
    shift += 7;
    if (shift > 28) {
      throw kipcError("KELD-IPC-003", "postcard string length overflow");
    }
  }
  throw kipcError("KELD-IPC-003", "truncated postcard string");
}

/**
 * A rejected privileged `Call`: the `Error` carries the registered `KELD-*`
 * code as a field, so callers branch on `code` instead of parsing `message`.
 */
export type KeldCallError = Error & { code: string };

/** True when `e` is a rejected `Call` carrying a registered `KELD-*` code. */
export function isCallError(e: unknown): e is KeldCallError {
  if (!(e instanceof Error)) return false;
  const { code } = e as Error & { code?: unknown };
  return typeof code === "string" && code.startsWith("KELD-");
}

export function decodeCallError(payload: Uint8Array): { code: string; message: string } {
  const [code, afterCode] = decodePostcardStringAt(payload, 0);
  const [message, end] = decodePostcardStringAt(payload, afterCode);
  if (end !== payload.length) {
    throw kipcError("KELD-IPC-003", "trailing bytes after CallError");
  }
  if (!code.startsWith("KELD-")) {
    throw kipcError("KELD-IPC-003", "CallError code is not a KELD-* identifier");
  }
  return { code, message };
}

export function errorFromErrFrame(payload: Uint8Array): Error {
  if (payload.length === 0) {
    return kipcError("KELD-IPC-005", "peer sent Err with empty payload");
  }
  let call: { code: string; message: string };
  try {
    call = decodeCallError(payload);
  } catch {
    return kipcError("KELD-IPC-005", "peer sent an Err payload that is not a CallError");
  }
  const error = new Error(
    call.message.startsWith(call.code) ? call.message : `${call.code}: ${call.message}`,
  );
  Object.defineProperty(error, "code", {
    value: call.code,
    enumerable: true,
    writable: true,
    configurable: true,
  });
  return error;
}

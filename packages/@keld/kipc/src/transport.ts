/**
 * Canonical TypeScript kipc v2 app-link transport (KEL-136).
 *
 * Wire constants match `keld_ipc::{frame,lib}` — not reverse-engineered — and the channel ids
 * are generated from `keld_ipc::channel_table` into the region below (GH-508).
 * `keld create` embeds this file as `src/kipc-transport.ts`. `@keld/electron` imports
 * it. Do not add a second reader/writer/constant owner.
 *
 * Frame layout: 16-byte LE header, HELLO token is raw 32 bytes, payloads are postcard.
 *
 * GH-527 `WorkerLink` (end of file) moves the role's link into a transport
 * Worker whose entry is this same file; see that section for its contract.
 */

import {
  MessageChannel,
  Worker,
  isMainThread,
  parentPort,
  receiveMessageOnPort,
  workerData,
  type MessagePort,
} from "node:worker_threads";

const MAGIC_BYTES = new Uint8Array([0x4b, 0x49]); // "KI", matches Rust `u16::from_le_bytes(*b"KI")`
/** Mirrors `keld_ipc::PROTOCOL_VERSION`. */
export const PROTOCOL_VERSION = 2;
/** Mirrors `keld_ipc::HEADER_LEN`. */
export const HEADER_LEN = 16;
/** Control-plane frame payload cap — mirrors `keld_ipc::MAX_FRAME_LEN` (16 MiB). */
export const MAX_FRAME_LEN = 16 * 1024 * 1024;
// @generated-begin channel-table: packages/@keld/kipc/scripts/echo-codegen.ts from
// crates/keld-ipc/src/channel_table.rs. Do not edit by hand; run bun run echo:generate.
/** Reserved `HELLO` channel (`keld_ipc::channel_table::HANDSHAKE_CHANNEL`). */
export const HANDSHAKE_CHANNEL = 0;
/** Channel `echo` (`keld_ipc::channel_table::ECHO`). */
export const ECHO_CHANNEL = 1;
/** Channel `fs` (`keld_ipc::channel_table::FS`). */
export const FS_CHANNEL = 2;
/** Channel `lifecycle` (`keld_ipc::channel_table::LIFECYCLE`). */
export const LIFECYCLE_CHANNEL = 3;
// @generated-end channel-table
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
    channel: HANDSHAKE_CHANNEL,
    kinds: [FrameKind.Hello],
    corr: { rule: "zero" },
    exactLen: 32,
  } as ReceivePolicy,
  clientAwaitHello: {
    channel: HANDSHAKE_CHANNEL,
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
  lifecycleEventReceiver: eventReceiverOn(LIFECYCLE_CHANNEL),
} as const;

/** Scaffold alias for `RECEIVE_POLICIES.clientAwaitHello`. */
export const CLIENT_AWAIT_HELLO: ReceivePolicy = RECEIVE_POLICIES.clientAwaitHello;

export function echoReplyWaiter(corr: number): ReceivePolicy {
  return { channel: ECHO_CHANNEL, kinds: [FrameKind.Reply], corr: { rule: "exactly", id: corr } };
}

export function lifecycleReplyWaiter(corr: number): ReceivePolicy {
  return replyWaiter(LIFECYCLE_CHANNEL, corr);
}

/**
 * Mirror of `keld_ipc::receive::ReceivePolicy::reply_waiter` (GH-527 §4.7,
 * corpus `reply-waiter:<channel>:<corr>`): `REPLY` or the CallError-carrying
 * `ERR` on `channel` with exactly `corr`, no `PING`. Channel 0 carries only
 * `HELLO`, and echo replies keep KEL-133 row 4's REPLY-only
 * `echoReplyWaiter`, so both are `KELD-IPC-005`.
 */
export function replyWaiter(channel: number, corr: number): ReceivePolicy {
  if (channel === 0) {
    throw kipcError("KELD-IPC-005", "channel 0 carries only HELLO");
  }
  if (channel === ECHO_CHANNEL) {
    throw kipcError("KELD-IPC-005", "echo replies use the REPLY-only echo reply waiter");
  }
  return { channel, kinds: [FrameKind.Reply, FrameKind.Err], corr: { rule: "exactly", id: corr } };
}

function eventReceiverOn(channel: number): ReceivePolicy {
  return { channel, kinds: [FrameKind.Event], corr: { rule: "zero" }, allowPing: true };
}

/**
 * Mirror of `keld_ipc::receive::ReceivePolicy::event_receiver` (GH-527 §4.7,
 * corpus `event-receiver:<channel>`): uncorrelated `EVENT`s on `channel` plus
 * the live `PING` probe. Lifecycle is the only channel with host EVENTs until
 * the channel table (#613) declares another, so any other channel is
 * `KELD-IPC-005`.
 */
export function eventReceiver(channel: number): ReceivePolicy {
  if (channel !== LIFECYCLE_CHANNEL) {
    throw kipcError("KELD-IPC-005", "channel carries no host EVENTs");
  }
  return eventReceiverOn(channel);
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

function payloadTooLarge(detail?: string): Error {
  return kipcError(
    "KELD-IPC-004",
    detail ??
      `frame payload exceeds MAX_FRAME_LEN (${MAX_FRAME_LEN} bytes). ` +
        "Shrink the payload or move large transfers to the bulk plane.",
  );
}

function ioDeadlineExceeded(): Error {
  return kipcError(
    "KELD-IPC-006",
    "app-link I/O deadline exceeded. Check the peer is still running and sending kipc frames; a silent or wedged process will not be waited on forever.",
  );
}

function requireHeaderInteger(field: string, value: unknown, max: number): number {
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0 || value > max) {
    throw kipcError(
      "KELD-IPC-003",
      `${field} must be an unsigned integer no greater than ${max}`,
    );
  }
  return value;
}

function validateHeaderForEncoding(header: unknown): FrameHeader {
  if (header === null || typeof header !== "object") {
    throw kipcError("KELD-IPC-003", "frame header must be an object");
  }
  const candidate = header as Partial<FrameHeader>;
  const kind = requireHeaderInteger("frame kind", candidate.kind, 0xff);
  if (!KNOWN_FRAME_KINDS.has(kind)) {
    throw kipcError("KELD-IPC-003", `frame kind ${kind} is not declared by kipc v${PROTOCOL_VERSION}`);
  }
  return {
    kind,
    flags: requireHeaderInteger("frame flags", candidate.flags, 0xffff),
    channel: requireHeaderInteger("frame channel", candidate.channel, 0xffff),
    corr: requireHeaderInteger("frame correlation", candidate.corr, 0xffff_ffff),
    len: requireHeaderInteger("frame payload length", candidate.len, 0xffff_ffff),
  };
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
  const candidate: unknown =
    typeof kindOrHeader === "object"
      ? kindOrHeader
      : {
          kind: kindOrHeader,
          flags,
          channel,
          corr,
          len,
        };
  const header = validateHeaderForEncoding(candidate);
  const out = new Uint8Array(HEADER_LEN);
  const view = new DataView(out.buffer);
  out.set(MAGIC_BYTES, 0);
  out[2] = PROTOCOL_VERSION;
  out[3] = header.kind;
  view.setUint16(4, header.flags, true);
  view.setUint16(6, header.channel, true);
  view.setUint32(8, header.corr, true);
  view.setUint32(12, header.len, true);
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
 * `KELD-IPC-005`; it must not overwrite the waiter. An idle waiter has no
 * deadline; its first buffered byte starts one absolute frame deadline that
 * later chunks cannot renew.
 */
const CHUNK_COMPACT_THRESHOLD = 1024;

/** Maximum unread fragments retained for one in-flight frame. */
export const MAX_PENDING_CHUNKS = 65_536;

export class FrameReader {
  #chunks: Array<Uint8Array | undefined> = [];
  #chunkArrivals: Array<number | undefined> = [];
  #chunkIndex = 0;
  #head = 0;
  #length = 0;
  #censusChunkIndex = 0;
  #censusOffset = 0;
  #censusBytes = 0;
  #pending: { resolve: (f: DecodedFrame) => void; reject: (e: Error) => void } | null = null;
  #closed = false;
  #closeError: Error | null = null;
  #ending: Error | null = null;
  #frameDeadline: ReturnType<typeof setTimeout> | undefined;

  /** Unread buffered bytes. Independent of how many socket chunks carried them. */
  bufferedBytes(): number {
    return this.#length;
  }

  /**
   * Unread socket chunks. A prefix-concat reader collapses this to 1 after the
   * second `push`; the chunk-queue reader keeps one entry per unread chunk.
   */
  pendingChunkCount(): number {
    return this.#chunks.length - this.#chunkIndex;
  }

  push(chunk: Uint8Array): void {
    const arrivedAt = performance.now();
    if (this.#closed || this.#ending !== null || chunk.byteLength === 0) return;
    // Bun documents the callback value's type but not a retain-after-callback
    // lifetime. Own each queued chunk so a reused/mutated producer buffer
    // cannot rewrite a partially received frame after `push` returns.
    this.#chunks.push(new Uint8Array(chunk));
    this.#chunkArrivals.push(arrivedAt);
    this.#length += chunk.byteLength;
    this.#ensureFrameDeadline();
    this.#tryResolve();
    if (!this.#closed && this.#length > HEADER_LEN + MAX_FRAME_LEN) {
      this.fail(
        payloadTooLarge(
          `buffered app-link data exceeds one maximum frame envelope (${HEADER_LEN + MAX_FRAME_LEN} bytes). ` +
            "Stop the flooding peer and open a fresh app-link.",
        ),
      );
    }
    if (!this.#closed && this.pendingChunkCount() > MAX_PENDING_CHUNKS) {
      this.fail(
        payloadTooLarge(
          `buffered app-link data exceeds ${MAX_PENDING_CHUNKS} unread fragments. ` +
            "Stop the fragment-flooding peer and open a fresh app-link.",
        ),
      );
    }
  }

  /**
   * The peer closed the stream. Complete frames already buffered are still
   * returned in arrival order; once none remains (a truncated frame included),
   * reads fail with `err`. `fail` stays the abort path, which drops them.
   */
  end(err: Error): void {
    if (this.#closed || this.#ending !== null) return;
    this.#ending = err;
    if (this.#pending === null) return;
    this.#tryResolve();
    if (this.#pending !== null && !this.#closed) this.fail(err);
  }

  fail(err: Error): void {
    this.#clearFrameDeadline();
    this.#closed = true;
    this.#closeError = err;
    this.#chunks = [];
    this.#chunkArrivals = [];
    this.#chunkIndex = 0;
    this.#head = 0;
    this.#length = 0;
    this.#censusChunkIndex = 0;
    this.#censusOffset = 0;
    this.#censusBytes = 0;
    const pending = this.#pending;
    this.#pending = null;
    pending?.reject(err);
  }

  #copyOut(n: number): Uint8Array {
    const out = new Uint8Array(n);
    let written = 0;
    let idx = this.#chunkIndex;
    let offset = this.#head;
    while (written < n) {
      const chunk = this.#chunks[idx];
      if (chunk === undefined) throw kipcError("KELD-IPC-001", "frame reader underrun");
      const take = Math.min(chunk.byteLength - offset, n - written);
      out.set(chunk.subarray(offset, offset + take), written);
      written += take;
      idx += 1;
      offset = 0;
    }
    return out;
  }

  #arrivalAt(skip: number): number {
    let idx = this.#chunkIndex;
    let offset = this.#head;
    let leftToSkip = skip;
    while (true) {
      const chunk = this.#chunks[idx];
      const arrival = this.#chunkArrivals[idx];
      if (chunk === undefined || arrival === undefined) {
        throw kipcError("KELD-IPC-001", "frame reader arrival underrun");
      }
      const available = chunk.byteLength - offset;
      if (leftToSkip < available) return arrival;
      leftToSkip -= available;
      idx += 1;
      offset = 0;
    }
  }

  #consume(n: number): void {
    const resetCensus = n > this.#censusBytes;
    this.#censusBytes = Math.max(0, this.#censusBytes - n);
    let left = n;
    this.#length -= n;
    while (left > 0) {
      const chunk = this.#chunks[this.#chunkIndex];
      if (chunk === undefined) {
        throw kipcError("KELD-IPC-001", "frame reader underrun");
      }
      const avail = chunk.byteLength - this.#head;
      if (left < avail) {
        this.#head += left;
        left = 0;
        break;
      }
      left -= avail;
      // Release consumed bytes immediately and advance the logical queue head
      // without relying on Array.shift() reindexing behavior.
      this.#chunks[this.#chunkIndex] = undefined;
      this.#chunkArrivals[this.#chunkIndex] = undefined;
      this.#chunkIndex += 1;
      this.#head = 0;
    }
    this.#compactChunks();
    if (resetCensus) {
      this.#censusChunkIndex = this.#chunkIndex;
      this.#censusOffset = this.#head;
    }
  }

  #compactChunks(): void {
    if (this.#chunkIndex === this.#chunks.length) {
      this.#chunks = [];
      this.#chunkArrivals = [];
      this.#chunkIndex = 0;
      this.#censusChunkIndex = 0;
      this.#censusOffset = 0;
      return;
    }
    if (
      this.#chunkIndex >= CHUNK_COMPACT_THRESHOLD &&
      this.#chunkIndex * 2 >= this.#chunks.length
    ) {
      const removed = this.#chunkIndex;
      this.#chunks = this.#chunks.slice(this.#chunkIndex);
      this.#chunkArrivals = this.#chunkArrivals.slice(this.#chunkIndex);
      this.#censusChunkIndex -= removed;
      this.#chunkIndex = 0;
    }
  }

  #tryResolve(): void {
    if (!this.#pending || this.#length < HEADER_LEN) return;
    const startedAt = this.#arrivalAt(0);
    const headerCompletedAt = this.#arrivalAt(HEADER_LEN - 1);
    if (headerCompletedAt - startedAt >= APP_LINK_IO_DEADLINE_MS) {
      this.fail(ioDeadlineExceeded());
      return;
    }
    let header: FrameHeader;
    try {
      header = decodeHeader(this.#copyOut(HEADER_LEN));
    } catch (err) {
      this.fail(err instanceof Error ? err : kipcError("KELD-IPC-002", String(err)));
      return;
    }
    if (header.len > MAX_FRAME_LEN) {
      this.fail(payloadTooLarge());
      return;
    }
    const total = HEADER_LEN + header.len;
    if (this.#length < total) return;
    const completedAt = this.#arrivalAt(total - 1);
    if (completedAt - startedAt >= APP_LINK_IO_DEADLINE_MS) {
      this.fail(
        kipcError(
          "KELD-IPC-006",
          "started app-link frame did not finish within the I/O deadline",
        ),
      );
      return;
    }
    this.#consume(HEADER_LEN);
    const payload = header.len === 0 ? new Uint8Array(0) : this.#copyOut(header.len);
    if (header.len > 0) this.#consume(header.len);
    this.#clearFrameDeadline();
    this.#ensureFrameDeadline();
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
      this.#ensureFrameDeadline();
      this.#tryResolve();
      if (this.#pending !== null && this.#ending !== null && !this.#closed) this.fail(this.#ending);
    });
  }

  #ensureFrameDeadline(): void {
    if (this.#censusBytes >= this.#length || this.#frameDeadline !== undefined) return;
    this.#scheduleFrameDeadline(this.#censusArrival());
  }

  #censusArrival(): number {
    const value = this.#chunkArrivals[this.#censusChunkIndex];
    if (value === undefined) {
      throw kipcError("KELD-IPC-001", "frame reader deadline census underrun");
    }
    return value;
  }

  #scheduleFrameDeadline(startedAt: number): void {
    if (this.#frameDeadline !== undefined) return;
    const remaining = Math.max(0, startedAt + APP_LINK_IO_DEADLINE_MS - performance.now());
    this.#frameDeadline = setTimeout(() => {
      this.#frameDeadline = undefined;
      // After end() no byte can arrive: the next read reports the close.
      if (this.#ending !== null) return;
      let incompleteStartedAt: number | undefined;
      try {
        incompleteStartedAt = this.#incompleteFrameStartedAt();
      } catch (err) {
        this.fail(err instanceof Error ? err : kipcError("KELD-IPC-002", String(err)));
        return;
      }
      if (incompleteStartedAt === undefined) return;
      if (performance.now() < incompleteStartedAt + APP_LINK_IO_DEADLINE_MS) {
        this.#scheduleFrameDeadline(incompleteStartedAt);
        return;
      }
      this.fail(
        kipcError(
          "KELD-IPC-006",
          "started app-link frame did not finish within the I/O deadline",
        ),
      );
    }, remaining);
  }

  #incompleteFrameStartedAt(): number | undefined {
    let idx = this.#censusChunkIndex;
    let chunkOffset = this.#censusOffset;
    let remaining = this.#length - this.#censusBytes;

    const currentArrival = (): number => {
      const value = this.#chunkArrivals[idx];
      if (value === undefined) {
        throw kipcError("KELD-IPC-001", "frame reader deadline census underrun");
      }
      return value;
    };

    const advance = (n: number, copy?: Uint8Array): number => {
      let left = n;
      let written = 0;
      let lastArrival = currentArrival();
      while (left > 0) {
        const chunk = this.#chunks[idx];
        const arrival = this.#chunkArrivals[idx];
        if (chunk === undefined || arrival === undefined) {
          throw kipcError("KELD-IPC-001", "frame reader deadline census underrun");
        }
        lastArrival = arrival;
        const take = Math.min(chunk.byteLength - chunkOffset, left);
        copy?.set(chunk.subarray(chunkOffset, chunkOffset + take), written);
        written += take;
        left -= take;
        chunkOffset += take;
        if (chunkOffset === chunk.byteLength) {
          idx += 1;
          chunkOffset = 0;
        }
      }
      remaining -= n;
      return lastArrival;
    };

    const encodedHeader = new Uint8Array(HEADER_LEN);
    while (remaining > 0) {
      const startedAt = currentArrival();
      if (remaining < HEADER_LEN) return startedAt;
      const headerCompletedAt = advance(HEADER_LEN, encodedHeader);
      if (headerCompletedAt - startedAt >= APP_LINK_IO_DEADLINE_MS) {
        throw ioDeadlineExceeded();
      }
      const header = decodeHeader(encodedHeader);
      if (header.len > MAX_FRAME_LEN) throw payloadTooLarge();
      if (remaining < header.len) return startedAt;
      const completedAt = header.len === 0 ? headerCompletedAt : advance(header.len);
      if (completedAt - startedAt >= APP_LINK_IO_DEADLINE_MS) {
        throw ioDeadlineExceeded();
      }
      this.#censusChunkIndex = idx;
      this.#censusOffset = chunkOffset;
      this.#censusBytes += HEADER_LEN + header.len;
    }
    return undefined;
  }

  #clearFrameDeadline(): void {
    if (this.#frameDeadline === undefined) return;
    clearTimeout(this.#frameDeadline);
    this.#frameDeadline = undefined;
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
  const deadlineAt = performance.now() + APP_LINK_IO_DEADLINE_MS;
  let offset = 0;
  while (offset < frame.length) {
    const remaining = deadlineAt - performance.now();
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
    // Admission failures emit no bytes and leave this queue usable. Only a
    // failure from the serialized write chain can imply a partial frame and
    // poison subsequent writes.
    if (payload.byteLength > MAX_FRAME_LEN) {
      return Promise.reject(payloadTooLarge());
    }
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
      reader.end(kipcError("KELD-IPC-001", "connection closed by peer"));
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

function decodeVarintAt(
  bytes: Uint8Array,
  offset: number,
  truncatedDetail: string,
): [number, number] {
  if (!Number.isInteger(offset) || offset < 0 || offset > bytes.length) {
    throw kipcError(
      "KELD-IPC-003",
      `varint offset must be an integer in [0, ${bytes.length}], got ${offset}`,
    );
  }
  let result = 0;
  let placeValue = 1;
  let pos = offset;
  for (let byteIndex = 0; byteIndex < 5; byteIndex += 1) {
    if (pos >= bytes.length) {
      throw kipcError("KELD-IPC-003", truncatedDetail);
    }
    const byte = bytes[pos];
    pos += 1;
    if (byteIndex === 4 && byte > 0x0f) {
      throw kipcError(
        "KELD-IPC-003",
        "u32 varint exceeds five bytes or overflows its fifth byte",
      );
    }
    result += (byte & 0x7f) * placeValue;
    if ((byte & 0x80) === 0) return [result, pos];
    placeValue *= 128;
  }
  throw kipcError("KELD-IPC-003", "u32 varint exceeds five bytes");
}

/** Decodes an unsigned LEB128 varint starting at `offset`. Returns `[value, nextOffset]`. */
export function decodeVarint(bytes: Uint8Array, offset: number): [number, number] {
  return decodeVarintAt(bytes, offset, "truncated varint");
}

/**
 * Reads one postcard string starting at `offset`; returns it with the index
 * one past its last byte. Preserves every Unicode scalar, including U+FEFF.
 */
export function decodePostcardStringAt(bytes: Uint8Array, offset: number): [string, number] {
  const [len, afterLen] = decodeVarintAt(bytes, offset, "truncated postcard string");
  const end = afterLen + len;
  const text = bytes.subarray(afterLen, end);
  if (text.length !== len) {
    throw kipcError("KELD-IPC-003", "postcard string length does not match payload");
  }
  try {
    // A postcard String carries data, not a text-file encoding signature.
    // Preserve U+FEFF rather than consuming it as a byte-order mark.
    return [new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(text), end];
  } catch {
    throw kipcError("KELD-IPC-003", "invalid UTF-8 in postcard string");
  }
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

export function encodePostcardString(value: string, field = "postcard string"): Uint8Array {
  if (typeof value !== "string") {
    throw kipcError("KELD-IPC-003", `${field} must be a string`);
  }
  for (let i = 0; i < value.length; i += 1) {
    const unit = value.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = value.charCodeAt(i + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff)) {
        throw kipcError("KELD-IPC-003", `${field} contains an unpaired UTF-16 surrogate`);
      }
      i += 1;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      throw kipcError("KELD-IPC-003", `${field} contains an unpaired UTF-16 surrogate`);
    }
  }
  const utf8 = new TextEncoder().encode(value);
  const prefix = encodeVarint(utf8.length);
  const out = new Uint8Array(prefix.length + utf8.length);
  out.set(prefix, 0);
  out.set(utf8, prefix.length);
  return out;
}

export function encodeCallError(code: string, message: string): Uint8Array {
  if (!code.startsWith("KELD-")) {
    throw kipcError("KELD-IPC-003", "CallError code is not a KELD-* identifier");
  }
  const encodedCode = encodePostcardString(code, "CallError code");
  const encodedMessage = encodePostcardString(message, "CallError message");
  const out = new Uint8Array(encodedCode.length + encodedMessage.length);
  out.set(encodedCode, 0);
  out.set(encodedMessage, encodedCode.length);
  return out;
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
  return codedError(call.code, call.message);
}

// ---------------------------------------------------------------------------
// GH-527 WorkerLink (spec docs/specs/gh527-worker-owned-blocking-call-transport.md).
//
// A transport Worker owns the role generation's one authenticated link. It
// drains the socket into one bounded, ordered SharedArrayBuffer ring, puts the
// one blocking reply in its own slot, and wakes a parked main thread with
// Atomics.notify. The Worker entry is this file (self-entry, §4.2); it runs only
// when `WorkerLink.open` passes WORKER_LINK_MARKER in `workerData`.
// ---------------------------------------------------------------------------

/** Default ring byte capacity (§4.3). */
export const DEFAULT_RING_BYTES = 1 << 20;
/** Default ring record capacity (§4.3). */
export const DEFAULT_RING_RECORDS = 16_384;
/** Default blocking-reply slot size (§4.3). */
export const DEFAULT_REPLY_BYTES = 1 << 16;
/** Largest ring byte capacity; keeps `(W - R) >>> 0` unambiguous (§4.3). */
export const MAX_RING_BYTES = 1 << 30;
/** Largest deadline `callBlocking` and `call` accept (§4.5). */
export const MAX_BLOCKING_CALL_DEADLINE_MS = 24 * 60 * 60 * 1000;
/** Transport Worker heartbeat period; a parked main re-checks at least this often. */
export const WORKER_HEARTBEAT_INTERVAL_MS = 100;
/** A heartbeat unchanged this long while parked is `KELD-IPC-025` (§4.5). */
export const WORKER_LIVENESS_WINDOW_MS = 1_000;
/** Abandoned (expired, unanswered) calls the Worker tracks before `KELD-IPC-027`. */
export const MAX_ABANDONED_CALLS = 256;

const MIN_RING_BYTES = 1 << 16;
const MIN_REPLY_BYTES = 4_096;
const CONTROL_BYTES = 64;
const CONTROL_WORDS = CONTROL_BYTES / 4;

/** Control-block word indices (§4.3). Words 2 and 15 are reserved and stay zero. */
export const WORKER_LINK_CONTROL = Object.freeze({
  SEQ: 0,
  STATE: 1,
  W_BYTES: 3,
  A_BYTES: 4,
  R_BYTES: 5,
  W_RECS: 6,
  R_RECS: 7,
  HEARTBEAT: 8,
  BLOCKING: 9,
  REPLY_READY: 10,
  REPLY_KIND: 11,
  REPLY_LEN: 12,
  REPLY_AT: 13,
  KICK: 14,
});
const SEQ = WORKER_LINK_CONTROL.SEQ;
const STATE = WORKER_LINK_CONTROL.STATE;
const W_BYTES = WORKER_LINK_CONTROL.W_BYTES;
const A_BYTES = WORKER_LINK_CONTROL.A_BYTES;
const R_BYTES = WORKER_LINK_CONTROL.R_BYTES;
const W_RECS = WORKER_LINK_CONTROL.W_RECS;
const R_RECS = WORKER_LINK_CONTROL.R_RECS;
const HEARTBEAT = WORKER_LINK_CONTROL.HEARTBEAT;
const BLOCKING = WORKER_LINK_CONTROL.BLOCKING;
const REPLY_READY = WORKER_LINK_CONTROL.REPLY_READY;
const REPLY_KIND = WORKER_LINK_CONTROL.REPLY_KIND;
const REPLY_LEN = WORKER_LINK_CONTROL.REPLY_LEN;
const REPLY_AT = WORKER_LINK_CONTROL.REPLY_AT;
const KICK = WORKER_LINK_CONTROL.KICK;

/** `STATE` values: the KELD-IPC number that ended the link (§4.5). */
const STATE_CLOSED = 22;
const STATE_WORKER_LOST = 25;
const STATE_OVERFLOW = 26;
const STATE_ABANDONED = 27;

/**
 * Message and fix guidance per terminal code (§4.4). Literal codes keep the
 * `crates/keld-cli/tests/error_registry.rs` scan exact.
 */
const TERMINAL_CODES: ReadonlyMap<number, { code: string; text: string }> = new Map([
  [
    STATE_CLOSED,
    {
      code: "KELD-IPC-022",
      text:
        "link closed before the host replied; no reply was received and the call's host effect is unknown. " +
        "Treat the call as not answered: the role's link is gone and cannot reconnect, so check the host log " +
        "for the close cause and do not retry on this link",
    },
  ],
  [
    STATE_WORKER_LOST,
    {
      code: "KELD-IPC-025",
      text:
        "transport Worker dead or unresponsive; the role's link is lost. The role cannot reconnect: report the " +
        "crash (the host restarts the role per its policy) and check the role log for the Worker's last error",
    },
  ],
  [
    STATE_OVERFLOW,
    {
      code: "KELD-IPC-026",
      text:
        "parked ring or reply slot full; the link was closed rather than drop a frame. Raise ringBytes, " +
        "ringRecords or replyBytes at WorkerLink.open, or reduce the host event rate toward this role; " +
        "retained events were delivered in order",
    },
  ],
  [
    STATE_ABANDONED,
    {
      code: "KELD-IPC-027",
      text:
        "too many unanswered calls; the link was closed rather than track another abandoned id. The host is " +
        "not answering this role's calls: check the host log for the stalled handler, and raise call deadlines " +
        "only if the host is slow rather than stuck. The role's link is gone and cannot reconnect",
    },
  ],
]);

/** Builds an `Error` that carries its registered code as a field. */
function codedError(code: string, message: string): KeldCallError {
  const error = new Error(message.startsWith(code) ? message : `${code}: ${message}`);
  Object.defineProperty(error, "code", {
    value: code,
    enumerable: true,
    writable: true,
    configurable: true,
  });
  return error as KeldCallError;
}

function terminalError(state: number, cause: string | undefined): KeldCallError {
  const entry = TERMINAL_CODES.get(state);
  if (entry === undefined) {
    return codedError("KELD-IPC-005", `link ended with an unknown state ${state}`);
  }
  const detail = cause === undefined ? "" : ` (cause: ${cause})`;
  return codedError(entry.code, `${entry.code}: ${entry.text}${detail}`);
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function codeOfText(text: string): string {
  const match = /^KELD-[A-Z]+-\d{3}/.exec(text);
  return match === null ? "KELD-IPC-001" : match[0];
}

/** Role-supplied inbound frames beyond replies to its own calls; narrows, never grants (§4.7). */
export interface WorkerReceiveTable {
  /** Channels on which the host may send EVENT frames (correlation id 0). */
  readonly eventChannels: readonly number[];
  /** Host-originated CALL receivers, named by KEL-133 constructor, at most one per channel. */
  readonly callReceivers: readonly WorkerCallReceiver[];
}

/** Names a KEL-133 constructor; the Worker builds the policy, never the caller. */
export type WorkerCallReceiver =
  | { readonly policy: "echoReceiver" }
  | { readonly policy: "privilegedCallReceiver"; readonly channel: number };

/** How main answers one host CALL: REPLY bytes, or an ERR's encoded CallError. */
export interface WorkerCallReply {
  readonly kind: typeof FrameKind.Reply | typeof FrameKind.Err;
  readonly payload: Uint8Array;
}

/** `WorkerLink.open` options; bounds are fixed for the link's lifetime (§4.3). */
export interface WorkerLinkOptions {
  /** `KELD_APP_LINK` text; parsed only in the Worker. */
  readonly link: string;
  /** Role-supplied inbound table (§4.7). */
  readonly receive: WorkerReceiveTable;
  /** Ring byte capacity: a power of two from 65,536 to `MAX_RING_BYTES`. */
  readonly ringBytes?: number;
  /** Ring record capacity: a positive integer. */
  readonly ringRecords?: number;
  /** Blocking-reply slot size: an integer from 4,096 to `MAX_FRAME_LEN`. */
  readonly replyBytes?: number;
}

/** One `receive` table after validation: the policies the Worker selects from. */
export interface InboundTable {
  readonly eventChannels: ReadonlySet<number>;
  readonly callPolicies: ReadonlyMap<number, ReceivePolicy>;
}

/** One Worker-owned pending-CALL map entry (§4.5). */
export interface PendingCallEntry {
  readonly channel: number;
  readonly blocking: boolean;
  abandoned: boolean;
}

/** What the Worker does with one inbound frame after its policy admits it (§4.7). */
export type InboundAction = "ping" | "append" | "claim" | "discard";

/** A policy that admits no kind: validating under it is the §4.7 "no match" `KELD-IPC-005`. */
const NO_FRAME_POLICY: ReceivePolicy = { channel: 0, kinds: [], corr: { rule: "zero" } };

/**
 * The GH-527 §4.7 per-frame selection: trusted state (the frame's kind and
 * channel, the role's `receive` table and the Worker's pending-CALL map) picks
 * one existing-shape KEL-133 policy, and the unchanged validator then runs
 * under it. Wire bytes never select their own policy. A frame with no selected
 * policy gets one that admits no kind, so the validator's first check is the
 * `KELD-IPC-005`.
 */
export function selectInboundPolicy(
  header: FrameHeader,
  table: InboundTable,
  pending: ReadonlyMap<number, PendingCallEntry>,
): { policy: ReceivePolicy; action: InboundAction } {
  switch (header.kind) {
    case FrameKind.Ping:
      return { policy: RECEIVE_POLICIES.lifecycleEventReceiver, action: "ping" };
    case FrameKind.Reply:
    case FrameKind.Err: {
      const entry = pending.get(header.corr);
      if (entry === undefined) return { policy: NO_FRAME_POLICY, action: "append" };
      const policy =
        entry.channel === ECHO_CHANNEL
          ? echoReplyWaiter(header.corr)
          : replyWaiter(entry.channel, header.corr);
      if (entry.abandoned) return { policy, action: "discard" };
      return { policy, action: entry.blocking ? "claim" : "append" };
    }
    case FrameKind.Event:
      return table.eventChannels.has(header.channel)
        ? { policy: eventReceiver(header.channel), action: "append" }
        : { policy: NO_FRAME_POLICY, action: "append" };
    case FrameKind.Call:
      return { policy: table.callPolicies.get(header.channel) ?? NO_FRAME_POLICY, action: "append" };
    default:
      return { policy: NO_FRAME_POLICY, action: "append" };
  }
}

function linkError(code: string, detail: string): KeldCallError {
  return codedError(code, `${code}: ${detail}`);
}

/** Runs a shared policy rule and re-raises its `kipcError` with the code as a field. */
function withCode<T>(rule: () => T): T {
  try {
    return rule();
  } catch (err) {
    const text = errorText(err);
    throw codedError(codeOfText(text), text);
  }
}

function requirePositiveInteger(name: string, value: number, min: number, max: number): number {
  if (typeof value !== "number" || !Number.isInteger(value) || value < min || value > max) {
    throw linkError("KELD-IPC-005", `${name} must be an integer from ${min} to ${max}; got ${String(value)}`);
  }
  return value;
}

function requireChannel(name: string, value: unknown): number {
  if (typeof value !== "number" || !Number.isInteger(value) || value < 1 || value > 0xffff) {
    throw linkError("KELD-IPC-005", `${name} must be a channel from 1 to 65535; got ${String(value)}`);
  }
  return value;
}

/** The validated, fixed configuration of one `WorkerLink` (§4.3, §4.7). */
interface WorkerLinkConfig {
  link: string;
  ringBytes: number;
  ringRecords: number;
  replyBytes: number;
  eventChannels: number[];
  callReceivers: Array<{ policy: "echoReceiver" | "privilegedCallReceiver"; channel: number }>;
  table: InboundTable;
}

/** Validates every open bound and the `receive` table before the Worker spawns (§4.3, §4.7). */
function validateWorkerLinkOptions(options: WorkerLinkOptions): WorkerLinkConfig {
  if (options === null || typeof options !== "object") {
    throw linkError("KELD-IPC-005", "WorkerLink.open needs an options object");
  }
  if (typeof options.link !== "string") {
    throw linkError("KELD-IPC-005", "WorkerLink.open needs the KELD_APP_LINK text as `link`");
  }
  const ringBytes = options.ringBytes ?? DEFAULT_RING_BYTES;
  if (
    typeof ringBytes !== "number" ||
    !Number.isInteger(ringBytes) ||
    ringBytes < MIN_RING_BYTES ||
    ringBytes > MAX_RING_BYTES ||
    (ringBytes & (ringBytes - 1)) !== 0
  ) {
    throw linkError(
      "KELD-IPC-005",
      `ringBytes must be a power of two from ${MIN_RING_BYTES} to ${MAX_RING_BYTES}; got ${String(ringBytes)}`,
    );
  }
  const ringRecords = requirePositiveInteger(
    "ringRecords",
    options.ringRecords ?? DEFAULT_RING_RECORDS,
    1,
    Number.MAX_SAFE_INTEGER,
  );
  const replyBytes = requirePositiveInteger(
    "replyBytes",
    options.replyBytes ?? DEFAULT_REPLY_BYTES,
    MIN_REPLY_BYTES,
    MAX_FRAME_LEN,
  );
  const receive = options.receive;
  if (
    receive === null ||
    typeof receive !== "object" ||
    !Array.isArray(receive.eventChannels) ||
    !Array.isArray(receive.callReceivers)
  ) {
    throw linkError("KELD-IPC-005", "receive must declare eventChannels and callReceivers arrays");
  }
  const seen = new Set<number>();
  const claim = (channel: number | undefined): void => {
    if (channel === undefined) return;
    if (channel === 0 || seen.has(channel)) {
      throw linkError(
        "KELD-IPC-005",
        `receive table names channel ${channel} ${channel === 0 ? "(reserved for HELLO)" : "twice"}`,
      );
    }
    seen.add(channel);
  };
  const eventChannels: number[] = [];
  for (const value of receive.eventChannels) {
    const channel = requireChannel("an eventChannels entry", value);
    withCode(() => eventReceiver(channel));
    claim(channel);
    eventChannels.push(channel);
  }
  const callReceivers: WorkerLinkConfig["callReceivers"] = [];
  const callPolicies = new Map<number, ReceivePolicy>();
  for (const receiver of receive.callReceivers) {
    let policy: ReceivePolicy;
    if (receiver?.policy === "echoReceiver") {
      policy = RECEIVE_POLICIES.echoReceiver;
    } else if (receiver?.policy === "privilegedCallReceiver") {
      policy = privilegedCallReceiver(requireChannel("a privilegedCallReceiver channel", receiver.channel));
    } else {
      throw linkError("KELD-IPC-005", "a callReceivers entry must name echoReceiver or privilegedCallReceiver");
    }
    claim(policy.channel);
    claim(policy.alsoChannel);
    callReceivers.push({ policy: receiver.policy, channel: policy.channel });
    callPolicies.set(policy.channel, policy);
    if (policy.alsoChannel !== undefined) callPolicies.set(policy.alsoChannel, policy);
  }
  return {
    link: options.link,
    ringBytes,
    ringRecords,
    replyBytes,
    eventChannels,
    callReceivers,
    table: { eventChannels: new Set(eventChannels), callPolicies },
  };
}

function inboundTable(
  eventChannels: readonly number[],
  callReceivers: WorkerLinkConfig["callReceivers"],
): InboundTable {
  const callPolicies = new Map<number, ReceivePolicy>();
  for (const receiver of callReceivers) {
    const policy =
      receiver.policy === "echoReceiver"
        ? RECEIVE_POLICIES.echoReceiver
        : privilegedCallReceiver(receiver.channel);
    callPolicies.set(policy.channel, policy);
    if (policy.alsoChannel !== undefined) callPolicies.set(policy.alsoChannel, policy);
  }
  return { eventChannels: new Set(eventChannels), callPolicies };
}

function requireDeadline(deadlineMs: number): void {
  if (
    typeof deadlineMs !== "number" ||
    !Number.isFinite(deadlineMs) ||
    deadlineMs <= 0 ||
    deadlineMs > MAX_BLOCKING_CALL_DEADLINE_MS
  ) {
    throw linkError(
      "KELD-IPC-005",
      `a call deadline must be a finite number of milliseconds above 0 and at most ` +
        `${MAX_BLOCKING_CALL_DEADLINE_MS}; got ${String(deadlineMs)}. Every call needs a finite deadline`,
    );
  }
}

function requireOutboundFrame(channel: number, payload: Uint8Array): void {
  if (typeof channel !== "number" || !Number.isInteger(channel) || channel < 0 || channel > 0xffff) {
    throw linkError("KELD-IPC-003", "frame channel must be an unsigned integer no greater than 65535");
  }
  if (channel === 0) {
    throw linkError("KELD-IPC-005", "channel 0 carries only HELLO");
  }
  if (!(payload instanceof Uint8Array)) {
    throw linkError("KELD-IPC-003", "frame payload must be a Uint8Array");
  }
  if (payload.byteLength > MAX_FRAME_LEN) {
    throw linkError(
      "KELD-IPC-004",
      `frame payload exceeds MAX_FRAME_LEN (${MAX_FRAME_LEN} bytes). Shrink the payload or move large transfers to the bulk plane.`,
    );
  }
}

/** Test-only words shared by main, the Worker and a test thread (criteria 8, 18, 26). */
export const WORKER_LINK_TEST_WORDS = Object.freeze({
  /** Main's liveness branch ran (count). */
  LIVENESS_BRANCH: 0,
  /** The Worker's exit handler ran (count). */
  EXIT_HANDLER: 1,
  /** Main's deadline compare-and-exchange on `BLOCKING` failed (set to 1, notified). */
  DEADLINE_CAS_FAILED: 2,
  /** Main's post-claim bound ran (count). */
  POST_CLAIM_BRANCH: 3,
  /** Word a wedged test Worker waits on. */
  WEDGE: 4,
  /** Free for the test's own synchronization. */
  TEST_0: 5,
  /** The Worker's claim compare-and-exchange on `BLOCKING` succeeded (set to 1, notified). */
  CLAIMED: 6,
  TEST_2: 7,
  LENGTH: 8,
});

/** Fault the test Worker injects after it writes a blocking CALL (criteria 8 and 14). */
export interface WorkerBlockingFault {
  readonly kind: "throw" | "exit" | "wedge" | "stall-then-close";
  readonly delayMs: number;
  /** For `stall-then-close`: how long the Worker's loop stops before it closes. */
  readonly stallMs?: number;
}

/** Test-only hooks; accepted only by `openWorkerLinkForTest` under `KELD_KIPC_TEST_HOOKS=1`. */
export interface WorkerLinkTestHooks {
  /** `WORKER_LINK_TEST_WORDS.LENGTH` words on a SharedArrayBuffer. */
  readonly words: Int32Array;
  /** Start value of every byte counter (criterion 25). */
  readonly counterStart?: number;
  /** Runs each time a dispatch task returns without requesting another (criterion 19). */
  readonly onDispatchIdle?: (rRecs: number, wRecs: number) => void;
  /** Runs on wake before the step-1 drain (criterion 25). */
  readonly beforeWakeDrain?: () => void;
  /** Runs in the park between the `REPLY_READY` and `STATE` loads (reply-then-close order). */
  readonly beforeStateCheck?: () => void;
  /**
   * Receives every dispatch task instead of running it, and runs it when it
   * chooses: withholding the task after wake lets an overdue `call()` timer run
   * first, an event-loop order the runtime does not specify (§4.6).
   */
  readonly deferDispatch?: (run: () => void) => void;
  /**
   * Runs in the park just before main's deadline compare-and-exchange on
   * `BLOCKING`. Holding it until the Worker sets `CLAIMED` makes "the claim
   * came first" a fact at any host round-trip speed (criteria 18 and 26).
   * Returning `false` skips the deadline step for this pass, so the park's own
   * `STATE` and liveness checks run first (a Worker that died before its claim).
   */
  readonly beforeDeadlineCas?: () => boolean;
  readonly onBlockingCall?: WorkerBlockingFault;
  /** Claim-step faults (criteria 18 and 26). */
  readonly claimFault?: "skip-publish" | "throw" | "stall-until-deadline-cas";
  /** Hold the Worker's message handling until `BLOCKING` is nonzero (criterion 20). */
  readonly holdMessagesUntilBlocking?: boolean;
}

interface WorkerHookData {
  words: SharedArrayBuffer;
  onBlockingCall?: WorkerBlockingFault;
  claimFault?: WorkerLinkTestHooks["claimFault"];
  holdMessagesUntilBlocking?: boolean;
}

const WORKER_LINK_MARKER = "keld-kipc-worker-link/v1";

interface WorkerLinkBoot {
  marker: typeof WORKER_LINK_MARKER;
  sab: SharedArrayBuffer;
  link: string;
  ringBytes: number;
  ringRecords: number;
  replyBytes: number;
  eventChannels: number[];
  callReceivers: WorkerLinkConfig["callReceivers"];
  causePort: MessagePort;
  hooks?: WorkerHookData;
}

type ToWorker =
  | { t: "call"; corr: number; channel: number; payload: Uint8Array; blocking: boolean }
  | { t: "event"; channel: number; payload: Uint8Array }
  | { t: "answer"; channel: number; corr: number; kind: number; payload: Uint8Array }
  | { t: "abandon"; corr: number }
  | { t: "close" };

type FromWorker = { t: "open" } | { t: "open-failed"; message: string } | { t: "kick" };

/** A cause the Worker posts before its `STATE` compare-and-exchange (§4.4 detail). */
interface WorkerCause {
  state: number;
  detail: string;
}

function isWorkerLinkBoot(data: unknown): data is WorkerLinkBoot {
  return (
    data !== null &&
    typeof data === "object" &&
    (data as { marker?: unknown }).marker === WORKER_LINK_MARKER
  );
}

function readU32(bytes: Uint8Array, mask: number, pos: number): number {
  return (
    (bytes[pos & mask] |
      (bytes[(pos + 1) & mask] << 8) |
      (bytes[(pos + 2) & mask] << 16) |
      (bytes[(pos + 3) & mask] << 24)) >>>
    0
  );
}

/** Reads the 16-byte frame header of the ring record starting at counter `pos`. */
function ringHeader(ring: Uint8Array, mask: number, pos: number): FrameHeader {
  return {
    kind: ring[(pos + 3) & mask],
    flags: ring[(pos + 4) & mask] | (ring[(pos + 5) & mask] << 8),
    channel: ring[(pos + 6) & mask] | (ring[(pos + 7) & mask] << 8),
    corr: readU32(ring, mask, pos + 8),
    len: readU32(ring, mask, pos + 12),
  };
}

/** Copies `len` ring bytes starting at counter `pos` into a fresh array. */
function ringCopy(ring: Uint8Array, mask: number, pos: number, len: number): Uint8Array {
  const out = new Uint8Array(len);
  const at = pos & mask;
  const first = Math.min(len, ring.length - at);
  out.set(ring.subarray(at, at + first), 0);
  if (first < len) out.set(ring.subarray(0, len - first), first);
  return out;
}

let workerLinkOpened = false;

/** Bound by `WorkerLink`'s static block; the only path to its private constructor with hooks. */
let openWorkerLinkWithHooks: (
  options: WorkerLinkOptions,
  hooks: WorkerLinkTestHooks,
) => Promise<{ link: WorkerLink; control: Int32Array }>;

interface PendingAsyncCall {
  resolve: (bytes: Uint8Array) => void;
  reject: (err: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

/**
 * The role's one link, owned by a transport Worker (GH-527 §4.5).
 *
 * Every main-thread frame passes through the Worker. Outbound frames are
 * posted to it in send order; inbound frames reach main through the ring (the
 * blocking reply through its slot). `callBlocking` parks the calling thread and
 * returns the host's real REPLY bytes or throws a `KeldCallError`; it never
 * returns a fabricated value.
 */
export class WorkerLink {
  readonly #ctrl: Int32Array;
  readonly #slot: Uint8Array;
  readonly #ring: Uint8Array;
  readonly #mask: number;
  readonly #worker: Worker;
  readonly #causePort: MessagePort;
  readonly #table: InboundTable;
  readonly #hooks: WorkerLinkTestHooks | undefined;
  readonly #pending = new Map<number, PendingAsyncCall>();
  readonly #appliers = new Map<number, (payload: Uint8Array) => void>();
  readonly #listeners = new Map<number, Set<(payload: Uint8Array) => void>>();
  readonly #callHandlers = new Map<number, (payload: Uint8Array) => Promise<WorkerCallReply>>();
  #nextCorr = 1;
  #blockingInFlight = false;
  #applying = false;
  #blockingCorr = 0;
  #localCause: string | undefined;
  #workerCause: string | undefined;
  #deliveryStopAt: number | undefined;
  #deliveryStopped = false;
  #finalized = false;
  #terminated = false;
  #opened:
    | { resolve: (link: WorkerLink) => void; reject: (err: Error) => void }
    | undefined;

  static {
    openWorkerLinkWithHooks = async (options, hooks) => {
      const link = await WorkerLink.#open(options, hooks);
      return { link, control: link.#ctrl };
    };
  }

  private constructor(
    sab: SharedArrayBuffer,
    config: WorkerLinkConfig,
    worker: Worker,
    causePort: MessagePort,
    hooks: WorkerLinkTestHooks | undefined,
  ) {
    this.#ctrl = new Int32Array(sab, 0, CONTROL_WORDS);
    this.#slot = new Uint8Array(sab, CONTROL_BYTES, config.replyBytes);
    this.#ring = new Uint8Array(sab, CONTROL_BYTES + config.replyBytes, config.ringBytes);
    this.#mask = config.ringBytes - 1;
    this.#worker = worker;
    this.#causePort = causePort;
    this.#table = config.table;
    this.#hooks = hooks;
    worker.on("message", (message: FromWorker) => this.#onWorkerMessage(message));
    worker.on("error", (err: Error) => this.#onWorkerGone(`transport Worker error: ${errorText(err)}`));
    worker.on("exit", (code: number) => this.#onWorkerGone(`transport Worker exited with code ${code}`));
  }

  /** Spawns the transport Worker, which connects and completes HELLO. At most once per realm. */
  static open(options: WorkerLinkOptions): Promise<WorkerLink> {
    return WorkerLink.#open(options, undefined);
  }

  static async #open(
    options: WorkerLinkOptions,
    hooks: WorkerLinkTestHooks | undefined,
  ): Promise<WorkerLink> {
    const config = validateWorkerLinkOptions(options);
    if (workerLinkOpened) {
      throw linkError(
        "KELD-IPC-005",
        "WorkerLink.open runs at most once per realm: a role generation has one link. Reuse the open WorkerLink",
      );
    }
    workerLinkOpened = true;
    const sab = new SharedArrayBuffer(CONTROL_BYTES + config.replyBytes + config.ringBytes);
    if (hooks?.counterStart !== undefined) {
      const ctrl = new Int32Array(sab, 0, CONTROL_WORDS);
      for (const word of [W_BYTES, A_BYTES, R_BYTES]) Atomics.store(ctrl, word, hooks.counterStart | 0);
    }
    const { port1, port2 } = new MessageChannel();
    const boot: WorkerLinkBoot = {
      marker: WORKER_LINK_MARKER,
      sab,
      link: config.link,
      ringBytes: config.ringBytes,
      ringRecords: config.ringRecords,
      replyBytes: config.replyBytes,
      eventChannels: config.eventChannels,
      callReceivers: config.callReceivers,
      causePort: port2,
      hooks:
        hooks === undefined
          ? undefined
          : {
              words: hooks.words.buffer as SharedArrayBuffer,
              onBlockingCall: hooks.onBlockingCall,
              claimFault: hooks.claimFault,
              holdMessagesUntilBlocking: hooks.holdMessagesUntilBlocking,
            },
    };
    const worker = new Worker(new URL(import.meta.url), { workerData: boot, transferList: [port2] });
    const link = new WorkerLink(sab, config, worker, port1, hooks);
    return new Promise((resolve, reject) => {
      link.#opened = { resolve, reject };
    });
  }

  /** Parks the calling thread; returns the host REPLY bytes or throws a KeldCallError. */
  callBlocking(channel: number, payload: Uint8Array, deadlineMs: number): Uint8Array {
    requireDeadline(deadlineMs);
    if (this.#blockingInFlight || this.#applying) {
      throw linkError(
        "KELD-IPC-005",
        "a blocking call is already in flight, or a state applier is running; an applier must not call callBlocking",
      );
    }
    requireOutboundFrame(channel, payload);
    this.#throwIfTerminal();
    this.#blockingInFlight = true;
    try {
      const corr = this.#allocateCorr();
      this.#blockingCorr = corr;
      Atomics.store(this.#ctrl, BLOCKING, corr | 0);
      this.#post({ t: "call", corr, channel, payload, blocking: true });
      return this.#park(corr, deadlineMs);
    } finally {
      this.#blockingCorr = 0;
      this.#blockingInFlight = false;
    }
  }

  /** Same correlation and ring path, Promise-shaped, for non-blocking callers. */
  call(channel: number, payload: Uint8Array, deadlineMs: number): Promise<Uint8Array> {
    try {
      requireDeadline(deadlineMs);
      requireOutboundFrame(channel, payload);
      this.#throwIfTerminal();
    } catch (err) {
      return Promise.reject(err);
    }
    const corr = this.#allocateCorr();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (this.#pending.get(corr)?.timer !== timer) return;
        // A reply retained in the ring before this decision is the call's
        // answer, whichever task the event loop ran first after a park (§4.6):
        // run the dispatch step first, exactly as the dispatch task would.
        this.#drainRing();
        if (this.#pending.get(corr)?.timer !== timer) return;
        // The link already ended: every end path requests a dispatch task,
        // whose finalize rejects this call with the code STATE records (§4.6),
        // never 006, and no abandon goes to an ended Worker.
        if (Atomics.load(this.#ctrl, STATE) !== 0) return;
        this.#pending.delete(corr);
        this.#post({ t: "abandon", corr });
        reject(linkError("KELD-IPC-006", `call deadline of ${deadlineMs} ms expired with no host reply`));
      }, deadlineMs);
      this.#pending.set(corr, { resolve, reject, timer });
      this.#post({ t: "call", corr, channel, payload, blocking: false });
    });
  }

  /** Sends one EVENT through the Worker's write queue, in send order. */
  sendEvent(channel: number, payload: Uint8Array): void {
    requireOutboundFrame(channel, payload);
    this.#throwIfTerminal();
    this.#post({ t: "event", channel, payload });
  }

  /** Framework-only synchronous state applier; at most one per channel; never user code. */
  setStateApplier(channel: number, applier: (payload: Uint8Array) => void): void {
    this.#requireEventChannel(channel);
    if (this.#appliers.has(channel)) {
      throw linkError("KELD-IPC-005", `channel ${channel} already has a state applier`);
    }
    this.#appliers.set(channel, applier);
  }

  /** Framework-only answer to host CALLs on a `receive.callReceivers` channel; at most one per channel. */
  setCallHandler(channel: number, handler: (payload: Uint8Array) => Promise<WorkerCallReply>): void {
    if (!this.#table.callPolicies.has(channel)) {
      throw linkError("KELD-IPC-005", `receive.callReceivers does not name channel ${channel}`);
    }
    if (this.#callHandlers.has(channel)) {
      throw linkError("KELD-IPC-005", `channel ${channel} already has a call handler`);
    }
    this.#callHandlers.set(channel, handler);
  }

  /** Registers an EVENT listener; it runs after the caller's synchronous continuation. */
  onEvent(channel: number, listener: (payload: Uint8Array) => void): () => void {
    this.#requireEventChannel(channel);
    let set = this.#listeners.get(channel);
    if (set === undefined) {
      set = new Set();
      this.#listeners.set(channel, set);
    }
    set.add(listener);
    return () => {
      set.delete(listener);
    };
  }

  /** Ends the link; pending calls reject with `KELD-IPC-022` once retained records are delivered. */
  close(): void {
    if (this.#recordMain(STATE_CLOSED, "WorkerLink.close() ended the link")) {
      this.#post({ t: "close" });
    }
    this.#requestDispatch();
  }

  #requireEventChannel(channel: number): void {
    if (!this.#table.eventChannels.has(channel)) {
      throw linkError("KELD-IPC-005", `receive.eventChannels does not name channel ${channel}`);
    }
  }

  #post(message: ToWorker): void {
    this.#worker.postMessage(message);
  }

  #allocateCorr(): number {
    for (;;) {
      const corr = this.#nextCorr;
      this.#nextCorr = corr === 0xffff_ffff ? 1 : corr + 1;
      if (corr !== this.#blockingCorr && !this.#pending.has(corr)) return corr;
    }
  }

  /** Records a main-side terminal code; the first compare-and-exchange on `STATE` wins (§4.5). */
  #recordMain(state: number, cause: string): boolean {
    if (Atomics.compareExchange(this.#ctrl, STATE, 0, state) !== 0) return false;
    this.#localCause = cause;
    Atomics.add(this.#ctrl, SEQ, 1);
    Atomics.notify(this.#ctrl, SEQ);
    return true;
  }

  /** The error for the code `STATE` records, with the cause its recorder gave. */
  #terminalError(state: number): KeldCallError {
    return terminalError(state, this.#localCause ?? this.#readWorkerCause(state));
  }

  #readWorkerCause(state: number): string | undefined {
    while (this.#workerCause === undefined) {
      const received = receiveMessageOnPort(this.#causePort);
      if (received === undefined) break;
      const cause = received.message as WorkerCause;
      if (cause.state === state) this.#workerCause = cause.detail;
    }
    return this.#workerCause;
  }

  #throwIfTerminal(): void {
    const state = Atomics.load(this.#ctrl, STATE);
    if (state !== 0) throw this.#terminalError(state);
  }

  /** Fails the link from main with a session-contract violation, recorded as 22 (§4.4). */
  #failLink(detail: string): void {
    const cause = `KELD-IPC-005: ${detail}`;
    if (this.#recordMain(STATE_CLOSED, cause)) {
      console.error(`kipc WorkerLink: ${cause}`);
      this.#post({ t: "close" });
    }
    this.#requestDispatch();
  }

  #terminateWorker(): void {
    if (this.#terminated) return;
    this.#terminated = true;
    void this.#worker.terminate();
  }

  /** Main's wait loop (§4.5 step 3). */
  #park(corr: number, deadlineMs: number): Uint8Array {
    const ctrl = this.#ctrl;
    const words = this.#hooks?.words;
    const started = performance.now();
    const deadlineAt = started + deadlineMs;
    let heartbeat = Atomics.load(ctrl, HEARTBEAT);
    let heartbeatAt = started;
    let claimedAt: number | undefined;
    for (;;) {
      const seen = Atomics.load(ctrl, SEQ);
      if (Atomics.load(ctrl, REPLY_READY) === 1) return this.#takeReply();
      this.#hooks?.beforeStateCheck?.();
      if (Atomics.load(ctrl, STATE) !== 0) return this.#endPark(corr);
      const now = performance.now();
      const beat = Atomics.load(ctrl, HEARTBEAT);
      if (beat !== heartbeat) {
        heartbeat = beat;
        heartbeatAt = now;
      } else if (now - heartbeatAt >= WORKER_LIVENESS_WINDOW_MS) {
        if (words !== undefined) Atomics.add(words, WORKER_LINK_TEST_WORDS.LIVENESS_BRANCH, 1);
        this.#recordMain(
          STATE_WORKER_LOST,
          `the transport Worker heartbeat did not move for ${WORKER_LIVENESS_WINDOW_MS} ms while a call was parked`,
        );
        this.#terminateWorker();
        this.#requestDispatch();
        return this.#endPark(corr);
      }
      if (claimedAt === undefined && now >= deadlineAt) {
        if (this.#hooks?.beforeDeadlineCas?.() === false) continue;
        if (Atomics.compareExchange(ctrl, BLOCKING, corr | 0, 0) === (corr | 0)) {
          this.#post({ t: "abandon", corr });
          throw linkError("KELD-IPC-006", `blocking call deadline of ${deadlineMs} ms expired with no host reply`);
        }
        // The Worker already claimed the reply: wait for its publish, bounded.
        claimedAt = performance.now();
        if (words !== undefined) {
          Atomics.store(words, WORKER_LINK_TEST_WORDS.DEADLINE_CAS_FAILED, 1);
          Atomics.notify(words, WORKER_LINK_TEST_WORDS.DEADLINE_CAS_FAILED);
        }
      }
      if (claimedAt !== undefined && now - claimedAt >= WORKER_LIVENESS_WINDOW_MS) {
        if (words !== undefined) Atomics.add(words, WORKER_LINK_TEST_WORDS.POST_CLAIM_BRANCH, 1);
        this.#recordMain(
          STATE_WORKER_LOST,
          `the transport Worker claimed the reply but did not publish it within ${WORKER_LIVENESS_WINDOW_MS} ms`,
        );
        this.#terminateWorker();
        this.#requestDispatch();
        return this.#endPark(corr);
      }
      const nextCheck = claimedAt === undefined ? deadlineAt : claimedAt + WORKER_LIVENESS_WINDOW_MS;
      const slice = Math.max(1, Math.min(WORKER_HEARTBEAT_INTERVAL_MS, nextCheck - now));
      Atomics.wait(ctrl, SEQ, seen, slice);
    }
  }

  /**
   * Leaves a park once `STATE` is not 0. The Worker publishes a claimed reply
   * before it records any code, so a reply visible now arrived before the end
   * and wins (§4.1 edge A8 to A6); otherwise the call throws the recorded code.
   */
  #endPark(corr: number): Uint8Array {
    Atomics.compareExchange(this.#ctrl, BLOCKING, corr | 0, 0);
    if (Atomics.load(this.#ctrl, REPLY_READY) === 1) return this.#takeReply();
    throw this.#terminalError(Atomics.load(this.#ctrl, STATE));
  }

  /** Wake drain and copy-out (§4.5 step 3, §4.6 step 1). */
  #takeReply(): Uint8Array {
    const ctrl = this.#ctrl;
    const kind = Atomics.load(ctrl, REPLY_KIND);
    const len = Atomics.load(ctrl, REPLY_LEN);
    const at = Atomics.load(ctrl, REPLY_AT) >>> 0;
    let bytes: Uint8Array;
    try {
      this.#hooks?.beforeWakeDrain?.();
      this.#applyUpTo(at);
      bytes = new Uint8Array(len);
      bytes.set(this.#slot.subarray(0, len));
    } finally {
      Atomics.store(ctrl, REPLY_READY, 0);
      this.#requestDispatch();
    }
    if (kind === FrameKind.Err) throw this.#callErrorOrFail(bytes);
    return bytes;
  }

  /** The `CallError` an ERR carries; a malformed payload ends the link (§4.4). */
  #callErrorOrFail(payload: Uint8Array): KeldCallError {
    try {
      decodeCallError(payload);
    } catch (err) {
      this.#failLink(`an ERR payload is not a CallError: ${errorText(err)}`);
      return this.#terminalError(Atomics.load(this.#ctrl, STATE));
    }
    return errorFromErrFrame(payload) as KeldCallError;
  }

  /** Runs state appliers for EVENT records from `A_BYTES` up to `limit` (§4.6 step 1). */
  #applyUpTo(limit: number): void {
    let a = Atomics.load(this.#ctrl, A_BYTES) >>> 0;
    while (a !== limit) {
      const header = ringHeader(this.#ring, this.#mask, a);
      if (!this.#applyRecord(a, header)) {
        throw this.#terminalError(Atomics.load(this.#ctrl, STATE));
      }
      a = (a + HEADER_LEN + header.len) >>> 0;
      Atomics.store(this.#ctrl, A_BYTES, a | 0);
    }
  }

  /** Applies one record's state fact; a throwing applier ends the link (§4.6). */
  #applyRecord(pos: number, header: FrameHeader): boolean {
    if (header.kind !== FrameKind.Event) return true;
    const applier = this.#appliers.get(header.channel);
    if (applier === undefined) return true;
    this.#applying = true;
    try {
      applier(ringCopy(this.#ring, this.#mask, pos + HEADER_LEN, header.len));
      return true;
    } catch (err) {
      // A mirror that cannot apply a fact must not serve stale state: stop
      // delivery at this record and end the link.
      this.#deliveryStopAt = pos;
      this.#failLink(`the state applier for channel ${header.channel} threw: ${errorText(err)}`);
      return false;
    } finally {
      this.#applying = false;
    }
  }

  #requestDispatch(): void {
    if (Atomics.compareExchange(this.#ctrl, KICK, 0, 1) === 0) {
      setImmediate(() => this.#runDispatchTask());
    }
  }

  #runDispatchTask(): void {
    const defer = this.#hooks?.deferDispatch;
    if (defer === undefined) this.#dispatch();
    else defer(() => this.#dispatch());
  }

  /** One dispatch task (§4.6 steps 2 and 3); runs only while this side holds `KICK`. */
  #dispatch(): void {
    const ctrl = this.#ctrl;
    const r = this.#drainRing();
    Atomics.store(ctrl, KICK, 0);
    // STATE is loaded before W_BYTES: every record appended before the Worker
    // recorded STATE is then visible to the W_BYTES load (§4.6 step 3).
    const state = Atomics.load(ctrl, STATE);
    if ((Atomics.load(ctrl, W_BYTES) >>> 0) !== r) {
      this.#requestDispatch();
    } else {
      this.#hooks?.onDispatchIdle?.(Atomics.load(ctrl, R_RECS) >>> 0, Atomics.load(ctrl, W_RECS) >>> 0);
      if (state !== 0) this.#finalize(state);
    }
  }

  /**
   * The dispatch step (§4.6 step 2): delivers the records from `R_BYTES` up to
   * the `W_BYTES` loaded at the start, in issue order, and returns the new
   * `R_BYTES`. It owns no `KICK` state: the dispatch task runs it, and so does an
   * expiring `call()` timer before it decides, so the requested task still
   * re-checks the ring afterwards and no record is stranded.
   */
  #drainRing(): number {
    const ctrl = this.#ctrl;
    const end = Atomics.load(ctrl, W_BYTES) >>> 0;
    let r = Atomics.load(ctrl, R_BYTES) >>> 0;
    let rRecs = Atomics.load(ctrl, R_RECS) >>> 0;
    let failed = false;
    let firstError: unknown;
    while (r !== end) {
      const header = ringHeader(this.#ring, this.#mask, r);
      const envelope = HEADER_LEN + header.len;
      if (r === this.#deliveryStopAt) this.#deliveryStopped = true;
      // After finalize only frames that arrived after a main-recorded end
      // remain; the link is gone, so they are released, never delivered.
      let deliver = !this.#deliveryStopped && !this.#finalized;
      if (deliver && (Atomics.load(ctrl, A_BYTES) >>> 0) === r) {
        deliver = this.#applyRecord(r, header);
        if (deliver) Atomics.store(ctrl, A_BYTES, (r + envelope) | 0);
        else this.#deliveryStopped = true;
      }
      const payload = deliver ? ringCopy(this.#ring, this.#mask, r + HEADER_LEN, header.len) : undefined;
      const release = (): void => {
        r = (r + envelope) >>> 0;
        rRecs = (rRecs + 1) >>> 0;
        Atomics.store(ctrl, R_BYTES, r | 0);
        Atomics.store(ctrl, R_RECS, rRecs | 0);
      };
      if (payload === undefined) {
        release();
        continue;
      }
      switch (header.kind) {
        case FrameKind.Event:
          for (const listener of this.#listeners.get(header.channel) ?? []) {
            try {
              listener(payload);
            } catch (err) {
              if (!failed) {
                failed = true;
                firstError = err;
              }
            }
          }
          release();
          break;
        case FrameKind.Reply:
        case FrameKind.Err: {
          release();
          const waiter = this.#pending.get(header.corr);
          if (waiter === undefined) break; // already rejected at its deadline
          this.#pending.delete(header.corr);
          clearTimeout(waiter.timer);
          if (header.kind === FrameKind.Reply) waiter.resolve(payload);
          else waiter.reject(this.#callErrorOrFail(payload));
          break;
        }
        case FrameKind.Call:
          release();
          this.#startCallHandler(header, payload);
          break;
        default:
          release();
          break;
      }
    }
    if (failed) {
      setImmediate(() => {
        throw firstError;
      });
    }
    return r;
  }

  /** Runs the host-CALL handler; its answer goes back through the Worker (§4.6). */
  #startCallHandler(header: FrameHeader, payload: Uint8Array): void {
    const handler = this.#callHandlers.get(header.channel);
    if (handler === undefined) {
      this.#failLink(`host CALL on channel ${header.channel} has no call handler`);
      return;
    }
    let answer: Promise<WorkerCallReply>;
    try {
      answer = handler(payload);
    } catch (err) {
      this.#failLink(`the call handler for channel ${header.channel} threw: ${errorText(err)}`);
      return;
    }
    Promise.resolve(answer).then(
      (reply) => {
        if (reply === null || typeof reply !== "object") {
          this.#failLink(`the call handler for channel ${header.channel} returned no WorkerCallReply`);
          return;
        }
        if (reply.kind !== FrameKind.Reply && reply.kind !== FrameKind.Err) {
          this.#failLink(`the call handler for channel ${header.channel} returned kind ${String(reply.kind)}`);
          return;
        }
        if (!(reply.payload instanceof Uint8Array) || reply.payload.byteLength > MAX_FRAME_LEN) {
          this.#failLink(`the call handler for channel ${header.channel} returned an invalid payload`);
          return;
        }
        if (Atomics.load(this.#ctrl, STATE) !== 0) return;
        this.#post({
          t: "answer",
          channel: header.channel,
          corr: header.corr,
          kind: reply.kind,
          payload: reply.payload,
        });
      },
      (err: unknown) => {
        this.#failLink(`the call handler for channel ${header.channel} failed: ${errorText(err)}`);
      },
    );
  }

  /** Once every retained record is delivered, each unresolved call rejects with the recorded code. */
  #finalize(state: number): void {
    if (this.#finalized) return;
    this.#finalized = true;
    const pending = [...this.#pending.values()];
    this.#pending.clear();
    for (const waiter of pending) {
      clearTimeout(waiter.timer);
      waiter.reject(this.#terminalError(state));
    }
  }

  #onWorkerMessage(message: FromWorker): void {
    switch (message.t) {
      case "open": {
        const opened = this.#opened;
        this.#opened = undefined;
        opened?.resolve(this);
        return;
      }
      case "open-failed": {
        const opened = this.#opened;
        this.#opened = undefined;
        opened?.reject(codedError(codeOfText(message.message), message.message));
        return;
      }
      case "kick":
        // The Worker won KICK's 0 -> 1 exchange: this task owns the dispatcher.
        this.#runDispatchTask();
        return;
    }
  }

  #onWorkerGone(cause: string): void {
    const opened = this.#opened;
    this.#opened = undefined;
    if (opened !== undefined) {
      opened.reject(linkError("KELD-IPC-025", `${cause} before HELLO completed`));
    }
    this.#recordMain(STATE_WORKER_LOST, cause);
    // The Worker can no longer race on KICK, so main takes the dispatcher.
    Atomics.store(this.#ctrl, KICK, 1);
    setImmediate(() => this.#runDispatchTask());
  }
}

/**
 * Test-only `WorkerLink.open` with the GH-528 hooks (spec §7), returning the
 * link's control words for assertions. Refuses to run unless the environment
 * sets `KELD_KIPC_TEST_HOOKS=1`; production roles never set it, so the hooks
 * stay inert there.
 */
export function openWorkerLinkForTest(
  options: WorkerLinkOptions,
  hooks: WorkerLinkTestHooks,
): Promise<{ link: WorkerLink; control: Int32Array }> {
  if (process.env.KELD_KIPC_TEST_HOOKS !== "1") {
    return Promise.reject(
      linkError(
        "KELD-IPC-005",
        "openWorkerLinkForTest runs only under KELD_KIPC_TEST_HOOKS=1; roles open their link with WorkerLink.open",
      ),
    );
  }
  if (
    !(hooks?.words instanceof Int32Array) ||
    !(hooks.words.buffer instanceof SharedArrayBuffer) ||
    hooks.words.length < WORKER_LINK_TEST_WORDS.LENGTH
  ) {
    return Promise.reject(
      linkError("KELD-IPC-005", "test hooks need WORKER_LINK_TEST_WORDS.LENGTH Int32 words on a SharedArrayBuffer"),
    );
  }
  return openWorkerLinkWithHooks(options, hooks);
}

/**
 * The transport Worker (GH-527 §4.2): owns the socket from connect to close,
 * validates every inbound frame under its §4.7 policy, appends to the ring or
 * fills the reply slot, and writes every outbound frame through one
 * `WriteQueue`. It runs no application code.
 */
class TransportWorker {
  readonly #port: MessagePort;
  readonly #causePort: MessagePort;
  readonly #ctrl: Int32Array;
  readonly #slot: Uint8Array;
  readonly #ring: Uint8Array;
  readonly #mask: number;
  readonly #ringBytes: number;
  readonly #ringRecords: number;
  readonly #replyBytes: number;
  readonly #table: InboundTable;
  readonly #pending = new Map<number, PendingCallEntry>();
  readonly #reader = new FrameReader();
  readonly #drain = new DrainSignal();
  readonly #header = new Uint8Array(HEADER_LEN);
  readonly #headerView = new DataView(this.#header.buffer);
  readonly #words: Int32Array | undefined;
  readonly #hooks: WorkerHookData | undefined;
  readonly #heartbeat: ReturnType<typeof setInterval>;
  #socket: KipcSocket | undefined;
  #writes: WriteQueue | undefined;
  #lastWrite: Promise<void> = Promise.resolve();
  #abandoned = 0;
  #ended = false;
  #released = false;

  constructor(port: MessagePort, boot: WorkerLinkBoot) {
    this.#port = port;
    this.#causePort = boot.causePort;
    this.#ctrl = new Int32Array(boot.sab, 0, CONTROL_WORDS);
    this.#slot = new Uint8Array(boot.sab, CONTROL_BYTES, boot.replyBytes);
    this.#ring = new Uint8Array(boot.sab, CONTROL_BYTES + boot.replyBytes, boot.ringBytes);
    this.#mask = boot.ringBytes - 1;
    this.#ringBytes = boot.ringBytes;
    this.#ringRecords = boot.ringRecords;
    this.#replyBytes = boot.replyBytes;
    this.#table = inboundTable(boot.eventChannels, boot.callReceivers);
    this.#hooks = boot.hooks;
    this.#words = boot.hooks === undefined ? undefined : new Int32Array(boot.hooks.words);
    this.#heartbeat = setInterval(() => {
      Atomics.add(this.#ctrl, HEARTBEAT, 1);
    }, WORKER_HEARTBEAT_INTERVAL_MS);
  }

  /** Connects, completes HELLO, then drains the link until it ends. */
  async run(link: string): Promise<void> {
    // The Worker's exit handler (§4.5 "orderly exit"): an uncaught error
    // records 25, or keeps the code recorded first.
    const onFault = (err: unknown): void => {
      if (this.#words !== undefined) Atomics.add(this.#words, WORKER_LINK_TEST_WORDS.EXIT_HANDLER, 1);
      this.#end(STATE_WORKER_LOST, `transport Worker fault: ${errorText(err)}`);
    };
    process.on("uncaughtException", onFault);
    process.on("unhandledRejection", onFault);
    this.#port.on("message", (message: ToWorker) => this.#onMessage(message));
    try {
      const { endpoint, token } = parseAppLink(link);
      this.#socket = await connectKipcSocket(endpoint, this.#reader, this.#drain);
      this.#writes = new WriteQueue(this.#socket, this.#drain);
      await withIoDeadline(this.#writes.writeFrame(FrameKind.Hello, 0, 0, 0, token));
      const hello = await withIoDeadline(this.#reader.readFrame());
      validateReceivedHeader(RECEIVE_POLICIES.clientAwaitHello, hello.header);
      if (!timingSafeEqual(hello.payload, token)) {
        throw kipcError("KELD-IPC-007", "HELLO session token mismatch");
      }
    } catch (err) {
      this.#port.postMessage({ t: "open-failed", message: errorText(err) } satisfies FromWorker);
      this.#ended = true;
      clearInterval(this.#heartbeat);
      this.#socket?.end();
      process.exit(0);
      return;
    }
    this.#port.postMessage({ t: "open" } satisfies FromWorker);
    await this.#readLoop();
  }

  async #readLoop(): Promise<void> {
    for (;;) {
      let frame: DecodedFrame;
      try {
        frame = await this.#reader.readFrame();
      } catch (err) {
        this.#end(STATE_CLOSED, errorText(err));
        process.exit(0);
        return;
      }
      if (this.#ended) continue; // stopped reading; wait for the close
      try {
        this.#onFrame(frame);
      } catch (err) {
        // A frame the selected policy rejects closes the link before any
        // append (§4.7); its KELD-IPC-005 is the 22's cause (§4.4).
        this.#end(STATE_CLOSED, errorText(err));
      }
      Atomics.add(this.#ctrl, HEARTBEAT, 1);
    }
  }

  #onFrame(frame: DecodedFrame): void {
    // Main recorded an end (close, applier or handler failure, liveness): no
    // inbound frame is retained after it. Its queued `close` ends the socket.
    if (Atomics.load(this.#ctrl, STATE) !== 0) return;
    const header = frame.header;
    const { policy, action } = selectInboundPolicy(header, this.#table, this.#pending);
    validateReceivedHeader(policy, header);
    switch (action) {
      case "ping":
        this.#write(FrameKind.Ping, header.channel, header.corr, new Uint8Array(0));
        return;
      case "discard":
        // A late reply for an abandoned call: discarded, never returned (§4.5).
        this.#pending.delete(header.corr);
        this.#abandoned -= 1;
        return;
      case "claim":
        this.#pending.delete(header.corr);
        this.#claim(frame);
        return;
      case "append":
        if (header.kind === FrameKind.Reply || header.kind === FrameKind.Err) {
          this.#pending.delete(header.corr);
        }
        this.#append(frame);
        return;
    }
  }

  /** Claim, copy and publish the blocking reply in one synchronous step (§4.5 step 2). */
  #claim(frame: DecodedFrame): void {
    const ctrl = this.#ctrl;
    const corr = frame.header.corr | 0;
    // A failed claim means main abandoned the call at its deadline: a late
    // reply, discarded whatever its size.
    if (Atomics.compareExchange(ctrl, BLOCKING, corr, 0) !== corr) return;
    if (this.#words !== undefined) {
      Atomics.store(this.#words, WORKER_LINK_TEST_WORDS.CLAIMED, 1);
      Atomics.notify(this.#words, WORKER_LINK_TEST_WORDS.CLAIMED);
    }
    if (frame.payload.byteLength > this.#replyBytes) {
      this.#end(
        STATE_OVERFLOW,
        `a blocking reply of ${frame.payload.byteLength} payload bytes exceeds replyBytes (${this.#replyBytes})`,
      );
      return;
    }
    try {
      if (this.#hooks?.claimFault === "throw") throw new Error("test-only claim-step fault");
      Atomics.store(ctrl, REPLY_AT, Atomics.load(ctrl, W_BYTES));
      this.#slot.set(frame.payload, 0);
      Atomics.store(ctrl, REPLY_KIND, frame.header.kind);
      Atomics.store(ctrl, REPLY_LEN, frame.payload.byteLength);
      if (this.#hooks?.claimFault === "skip-publish") return;
      if (this.#hooks?.claimFault === "stall-until-deadline-cas" && this.#words !== undefined) {
        Atomics.wait(this.#words, WORKER_LINK_TEST_WORDS.DEADLINE_CAS_FAILED, 0, 60_000);
      }
      Atomics.store(ctrl, REPLY_READY, 1);
      Atomics.add(ctrl, SEQ, 1);
      Atomics.notify(ctrl, SEQ);
    } catch (err) {
      this.#end(STATE_WORKER_LOST, `the claim step failed after the reply was claimed: ${errorText(err)}`);
    }
  }

  /** Appends one admitted frame as a ring record, or fails closed with 26 (§4.7). */
  #append(frame: DecodedFrame): void {
    const ctrl = this.#ctrl;
    const len = frame.payload.byteLength;
    const envelope = HEADER_LEN + len;
    const w = Atomics.load(ctrl, W_BYTES) >>> 0;
    const free = this.#ringBytes - ((w - (Atomics.load(ctrl, R_BYTES) >>> 0)) >>> 0);
    const wRecs = Atomics.load(ctrl, W_RECS) >>> 0;
    const records = (wRecs - (Atomics.load(ctrl, R_RECS) >>> 0)) >>> 0;
    if (envelope > free) {
      this.#end(STATE_OVERFLOW, `a ${envelope}-byte frame did not fit the ${free} free ring bytes`);
      return;
    }
    if (records >= this.#ringRecords) {
      this.#end(STATE_OVERFLOW, `the ring already held ringRecords (${this.#ringRecords}) records`);
      return;
    }
    const view = this.#headerView;
    const header = frame.header;
    this.#header.set(MAGIC_BYTES, 0);
    this.#header[2] = PROTOCOL_VERSION;
    this.#header[3] = header.kind;
    view.setUint16(4, header.flags, true);
    view.setUint16(6, header.channel, true);
    view.setUint32(8, header.corr, true);
    view.setUint32(12, header.len, true);
    this.#ringWrite(w, this.#header);
    this.#ringWrite((w + HEADER_LEN) >>> 0, frame.payload);
    Atomics.store(ctrl, W_BYTES, (w + envelope) | 0);
    Atomics.store(ctrl, W_RECS, (wRecs + 1) | 0);
    this.#kick();
  }

  #ringWrite(pos: number, bytes: Uint8Array): void {
    const at = pos & this.#mask;
    const first = Math.min(bytes.length, this.#ringBytes - at);
    this.#ring.set(bytes.subarray(0, first), at);
    if (first < bytes.length) this.#ring.set(bytes.subarray(first), 0);
  }

  /** Requests one main dispatch task: only the side that sets KICK 0 -> 1 posts (§4.6 step 3). */
  #kick(): void {
    if (Atomics.compareExchange(this.#ctrl, KICK, 0, 1) === 0) {
      this.#port.postMessage({ t: "kick" } satisfies FromWorker);
    }
  }

  #write(kind: number, channel: number, corr: number, payload: Uint8Array): void {
    const writes = this.#writes;
    if (writes === undefined || this.#ended) return;
    const write = writes.writeFrame(kind, 0, channel, corr, payload);
    this.#lastWrite = write;
    write.catch((err: unknown) => {
      this.#end(STATE_CLOSED, `app-link write failed: ${errorText(err)}`);
    });
  }

  #onMessage(message: ToWorker): void {
    if (this.#hooks?.holdMessagesUntilBlocking === true && !this.#released) {
      // Test hook (criterion 20): every frame main posts is queued before any is handled.
      while (Atomics.load(this.#ctrl, BLOCKING) === 0) {
        Atomics.wait(this.#ctrl, BLOCKING, 0, 5);
      }
      this.#released = true;
    }
    // Frames main posted before it recorded an end are still written in post
    // order; main posts `close` after every end it records itself (§4.2).
    if (this.#ended) return;
    switch (message.t) {
      case "call": {
        if (this.#pending.has(message.corr)) {
          this.#end(
            STATE_CLOSED,
            `KELD-IPC-005: correlation id ${message.corr} is still pending; the CALL was not written`,
          );
          return;
        }
        this.#pending.set(message.corr, {
          channel: message.channel,
          blocking: message.blocking,
          abandoned: false,
        });
        this.#write(FrameKind.Call, message.channel, message.corr, message.payload);
        if (message.blocking) this.#armBlockingFault();
        return;
      }
      case "event":
        this.#write(FrameKind.Event, message.channel, 0, message.payload);
        return;
      case "answer":
        this.#write(message.kind, message.channel, message.corr, message.payload);
        return;
      case "abandon": {
        const entry = this.#pending.get(message.corr);
        if (entry === undefined || entry.abandoned) return; // already answered or discarded
        entry.abandoned = true;
        this.#abandoned += 1;
        if (this.#abandoned > MAX_ABANDONED_CALLS) {
          this.#end(STATE_ABANDONED, `${this.#abandoned} calls are abandoned unanswered`);
        }
        return;
      }
      case "close":
        this.#end(STATE_CLOSED, "WorkerLink.close() ended the link");
        return;
    }
  }

  /** Test hook (criteria 8 and 14): a fault after the Worker has written a blocking CALL. */
  #armBlockingFault(): void {
    const fault = this.#hooks?.onBlockingCall;
    const words = this.#words;
    if (fault === undefined || words === undefined) return;
    setTimeout(() => {
      switch (fault.kind) {
        case "throw":
          throw new Error("test-only uncaught transport Worker error");
        case "exit":
          process.exit(3);
          break;
        case "wedge":
          Atomics.wait(words, WORKER_LINK_TEST_WORDS.WEDGE, 0);
          break;
        case "stall-then-close":
          Atomics.wait(words, WORKER_LINK_TEST_WORDS.WEDGE, 0, fault.stallMs ?? WORKER_LIVENESS_WINDOW_MS);
          this.#end(STATE_CLOSED, "test-only close after a stall");
          break;
      }
    }, fault.delayMs);
  }

  /**
   * Ends the link from the Worker: one `STATE` compare-and-exchange (the first
   * recorder wins), a wake, a dispatch request, then the socket closes (§4.5).
   */
  #end(state: number, detail: string): void {
    if (this.#ended) return;
    this.#ended = true;
    clearInterval(this.#heartbeat);
    const ctrl = this.#ctrl;
    this.#causePort.postMessage({ state, detail } satisfies WorkerCause);
    if (Atomics.compareExchange(ctrl, STATE, 0, state) === 0) {
      const code = TERMINAL_CODES.get(state)?.code ?? "KELD-IPC-005";
      console.error(`kipc transport Worker: ${code}: ${detail}`);
    }
    Atomics.add(ctrl, SEQ, 1);
    Atomics.notify(ctrl, SEQ);
    this.#kick();
    const socket = this.#socket;
    void this.#lastWrite
      .then(
        () => undefined,
        () => undefined,
      )
      .then(() => {
        socket?.end();
        // The socket close fails the reader, which exits this Worker; this
        // bound covers a peer that never completes the close.
        setTimeout(() => process.exit(0), APP_LINK_IO_DEADLINE_MS).unref();
      });
  }
}

/** Self-entry (§4.2): runs only in a Worker that `WorkerLink.open` spawned with the marker. */
if (!isMainThread && parentPort !== null && isWorkerLinkBoot(workerData)) {
  const boot = workerData;
  void new TransportWorker(parentPort, boot).run(boot.link);
}

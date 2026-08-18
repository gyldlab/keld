/**
 * Minimal kipc client for the host-lifecycle channel (KEL-72).
 *
 * Wire constants match `keld_ipc::{frame,lifecycle}` — not reverse-engineered.
 * Frame layout: 16-byte LE header, HELLO token is raw 32 bytes, payloads are
 * postcard unit-enum varints (`Ready` 0x00, `LastWindowClosed` 0x01, `Quit` 0x00).
 */

const MAGIC_BYTES = new Uint8Array([0x4b, 0x49]);
const PROTOCOL_VERSION = 2;
const HEADER_LEN = 16;
const MAX_FRAME_LEN = 16 * 1024 * 1024;
/** Mirrors `keld_ipc::{APP_LINK_IO_DEADLINE}` (arch/02 §7). Bun has no `SO_RCVTIMEO`. */
export const APP_LINK_IO_DEADLINE_MS = 5_000;
/** Mirrors `keld_ipc::LIFECYCLE_CHANNEL`. */
export const LIFECYCLE_CHANNEL = 3;

export const FrameKind = {
  Hello: 0,
  Call: 1,
  Reply: 2,
  Err: 3,
  Event: 4,
  Ping: 10,
} as const;

export type LifecycleEventName = "ready" | "last-window-closed";

function kipcError(code: string, detail: string): Error {
  return new Error(`${code}: ${detail}`);
}

export function encodeHeader(
  kind: number,
  flags: number,
  channel: number,
  corr: number,
  len: number,
): Uint8Array {
  const out = new Uint8Array(HEADER_LEN);
  const view = new DataView(out.buffer);
  out.set(MAGIC_BYTES, 0);
  out[2] = PROTOCOL_VERSION;
  out[3] = kind;
  view.setUint16(4, flags, true);
  view.setUint16(6, channel, true);
  view.setUint32(8, corr, true);
  view.setUint32(12, len, true);
  return out;
}

function decodeHeader(bytes: Uint8Array): {
  kind: number;
  flags: number;
  channel: number;
  corr: number;
  len: number;
} {
  if (bytes.length < HEADER_LEN) {
    throw kipcError("KELD-IPC-002", `short frame header: ${bytes.length} bytes`);
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes[0] !== MAGIC_BYTES[0] || bytes[1] !== MAGIC_BYTES[1]) {
    throw kipcError("KELD-IPC-002", "bad kipc magic (expected 'KI')");
  }
  if (bytes[2] !== PROTOCOL_VERSION) {
    throw kipcError(
      "KELD-IPC-002",
      `unsupported kipc version: ${bytes[2]} (expected ${PROTOCOL_VERSION})`,
    );
  }
  return {
    kind: bytes[3],
    flags: view.getUint16(4, true),
    channel: view.getUint16(6, true),
    corr: view.getUint32(8, true),
    len: view.getUint32(12, true),
  };
}

export interface AppLink {
  endpoint: string;
  token: Uint8Array;
}

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

/**
 * Windows `KELD_APP_LINK` endpoint is a decimal loopback port, matching
 * `u16::from_str` on the host — not `Number.parseInt`, which accepts
 * `"127.0.0.1:9000"` as `127`.
 */
export function parseWin32Port(endpoint: string): number {
  if (!/^[0-9]+$/.test(endpoint)) {
    throw kipcError(
      "KELD-IPC-007",
      "KELD_APP_LINK Windows endpoint must be a TCP port in 1–65535",
    );
  }
  const port = Number(endpoint);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw kipcError(
      "KELD-IPC-007",
      "KELD_APP_LINK Windows endpoint must be a TCP port in 1–65535",
    );
  }
  return port;
}

function timingSafeEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i += 1) diff |= a[i] ^ b[i];
  return diff === 0;
}

interface DecodedFrame {
  header: ReturnType<typeof decodeHeader>;
  payload: Uint8Array;
}

/**
 * Buffers socket chunks and resolves one `readFrame()` call per complete
 * frame. Exported so tests can feed untrusted bytes without a live socket.
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
    let header: ReturnType<typeof decodeHeader>;
    try {
      header = decodeHeader(this.#buf);
    } catch (err) {
      this.fail(err instanceof Error ? err : kipcError("KELD-IPC-002", String(err)));
      return;
    }
    if (header.len > MAX_FRAME_LEN) {
      this.fail(
        kipcError(
          "KELD-IPC-004",
          `frame payload exceeds MAX_FRAME_LEN (${MAX_FRAME_LEN} bytes). Shrink the payload.`,
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

interface KipcSocket {
  write(data: Uint8Array): number;
  end(): void;
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
  let offset = 0;
  while (offset < frame.length) {
    const written = socket.write(frame.subarray(offset));
    if (written < 0) {
      throw kipcError("KELD-IPC-001", "socket closed during write");
    }
    offset += written;
    if (written === 0) {
      await drain.wait();
    }
  }
}

/**
 * One-at-a-time frame writer. Concurrent `writeOneFrame` calls interleave
 * bytes on the stream (and used to hang on a single-slot drain).
 */
export class WriteQueue {
  #chain: Promise<void> = Promise.resolve();
  #socket: KipcSocket;
  #drain: DrainSignal;

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
    const run = this.#chain.then(() =>
      writeOneFrame(this.#socket, this.#drain, kind, flags, channel, corr, payload),
    );
    this.#chain = run.then(
      () => undefined,
      () => undefined,
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
      reject(
        kipcError(
          "KELD-IPC-006",
          "app-link I/O deadline exceeded. Check the peer is still running and sending kipc frames; a silent or wedged process will not be waited on forever.",
        ),
      );
    }, deadlineMs);
  });
  try {
    return await Promise.race([promise, timeout]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
    void promise.catch(() => undefined);
  }
}

function decodeUnitEnum(bytes: Uint8Array): number {
  if (bytes.length !== 1) {
    throw kipcError("KELD-IPC-003", "lifecycle enum must be a single postcard varint byte");
  }
  return bytes[0];
}

function decodeEvent(bytes: Uint8Array): LifecycleEventName {
  const disc = decodeUnitEnum(bytes);
  if (disc === 0) return "ready";
  if (disc === 1) return "last-window-closed";
  throw kipcError("KELD-IPC-003", `unknown LifecycleEvent discriminant ${disc}`);
}

function decodePostcardString(bytes: Uint8Array): string {
  if (bytes.length === 0) {
    throw kipcError("KELD-IPC-003", "empty postcard string");
  }
  let len = 0;
  let shift = 0;
  let i = 0;
  while (i < bytes.length) {
    const b = bytes[i];
    i += 1;
    len |= (b & 0x7f) << shift;
    if ((b & 0x80) === 0) {
      const text = bytes.subarray(i, i + len);
      if (text.length !== len || i + len !== bytes.length) {
        throw kipcError("KELD-IPC-003", "postcard string length does not match payload");
      }
      return new TextDecoder().decode(text);
    }
    shift += 7;
    if (shift > 28) {
      throw kipcError("KELD-IPC-003", "postcard string length overflow");
    }
  }
  throw kipcError("KELD-IPC-003", "truncated postcard string");
}

function errorFromErrFrame(payload: Uint8Array): Error {
  let detail: string;
  try {
    detail = decodePostcardString(payload);
  } catch {
    detail =
      payload.length === 0
        ? "peer sent Err with empty payload"
        : new TextDecoder().decode(payload);
  }
  if (detail.startsWith("KELD-")) return new Error(detail);
  return kipcError("KELD-IPC-005", detail || "peer sent Err for in-flight Call");
}

export type LifecycleHandler = {
  onReady: () => void;
  onLastWindowClosed: () => void;
};

/**
 * One HELLO'd app-link that demuxes lifecycle Events vs the Quit Reply.
 */
export class LifecycleLink {
  #socket: KipcSocket;
  #reader: FrameReader;
  #writes: WriteQueue;
  #nextCorr = 1;
  #closed = false;
  #quitWaiter: { corr: number; resolve: () => void; reject: (e: Error) => void } | null = null;
  #loopStarted = false;

  private constructor(socket: KipcSocket, reader: FrameReader, writes: WriteQueue) {
    this.#socket = socket;
    this.#reader = reader;
    this.#writes = writes;
  }

  static async connect(link: string, handlers: LifecycleHandler): Promise<LifecycleLink> {
    const { endpoint, token } = parseAppLink(link);
    const reader = new FrameReader();
    const drain = new DrainSignal();
    const socketHandlers = {
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
            port: parseWin32Port(endpoint),
            socket: socketHandlers,
          })
        : await Bun.connect({
            unix: endpoint,
            socket: socketHandlers,
          });
    const writes = new WriteQueue(socket, drain);
    const session = new LifecycleLink(socket, reader, writes);
    try {
      await writes.writeFrame(FrameKind.Hello, 0, 0, 0, token);
      const helloReply = await withIoDeadline(reader.readFrame());
      if (helloReply.header.kind !== FrameKind.Hello) {
        throw kipcError("KELD-IPC-005", "expected HELLO from peer");
      }
      if (helloReply.payload.length !== 32 || !timingSafeEqual(helloReply.payload, token)) {
        throw kipcError("KELD-IPC-007", "HELLO session token mismatch");
      }
      session.#startLoop(handlers);
      return session;
    } catch (err) {
      session.close();
      throw err;
    }
  }

  #startLoop(handlers: LifecycleHandler): void {
    if (this.#loopStarted) return;
    this.#loopStarted = true;
    const run = async (): Promise<void> => {
      for (;;) {
        const frame = await this.#reader.readFrame();
        if (frame.header.kind === FrameKind.Event && frame.header.channel === LIFECYCLE_CHANNEL) {
          const event = decodeEvent(frame.payload);
          if (event === "ready") handlers.onReady();
          else handlers.onLastWindowClosed();
          continue;
        }
        if (
          frame.header.kind === FrameKind.Reply &&
          frame.header.channel === LIFECYCLE_CHANNEL &&
          this.#quitWaiter &&
          frame.header.corr === this.#quitWaiter.corr
        ) {
          const waiter = this.#quitWaiter;
          this.#quitWaiter = null;
          waiter.resolve();
          continue;
        }
        if (
          frame.header.kind === FrameKind.Err &&
          this.#quitWaiter &&
          frame.header.corr === this.#quitWaiter.corr
        ) {
          const waiter = this.#quitWaiter;
          this.#quitWaiter = null;
          waiter.reject(errorFromErrFrame(frame.payload));
          continue;
        }
        if (frame.header.kind === FrameKind.Ping) {
          await this.#writes.writeFrame(
            FrameKind.Ping,
            0,
            frame.header.channel,
            frame.header.corr,
            new Uint8Array(),
          );
          continue;
        }
      }
    };
    void run().catch((err: Error) => {
      const waiter = this.#quitWaiter;
      this.#quitWaiter = null;
      waiter?.reject(err);
    });
  }

  async quit(): Promise<void> {
    if (this.#closed) {
      throw kipcError("KELD-IPC-001", "session is closed");
    }
    const corr = this.#nextCorr;
    let next = (corr + 1) >>> 0;
    if (next === 0) next = 1;
    this.#nextCorr = next;
    try {
      await withIoDeadline(
        new Promise<void>((resolve, reject) => {
          this.#quitWaiter = { corr, resolve, reject };
          void this.#writes.writeFrame(
            FrameKind.Call,
            0,
            LIFECYCLE_CHANNEL,
            corr,
            new Uint8Array([0x00]),
          ).catch((err: Error) => {
            this.#quitWaiter = null;
            reject(err);
          });
        }),
      );
    } catch (err) {
      this.#quitWaiter = null;
      throw err;
    } finally {
      this.close();
    }
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#socket.end();
  }
}

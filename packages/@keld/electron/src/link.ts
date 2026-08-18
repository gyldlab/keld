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

function encodeHeader(
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

class FrameReader {
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

interface KipcSocket {
  write(data: Uint8Array): number;
  end(): void;
}

async function writeFrame(
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
  #drain: DrainSignal;
  #nextCorr = 1;
  #closed = false;
  #quitWaiter: { corr: number; resolve: () => void; reject: (e: Error) => void } | null = null;
  #loopStarted = false;

  private constructor(socket: KipcSocket, reader: FrameReader, drain: DrainSignal) {
    this.#socket = socket;
    this.#reader = reader;
    this.#drain = drain;
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
            port: Number.parseInt(endpoint, 10),
            socket: socketHandlers,
          })
        : await Bun.connect({
            unix: endpoint,
            socket: socketHandlers,
          });
    const session = new LifecycleLink(socket, reader, drain);
    try {
      await writeFrame(socket, drain, FrameKind.Hello, 0, 0, 0, token);
      const helloReply = await reader.readFrame();
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
        if (frame.header.kind === FrameKind.Ping) {
          await writeFrame(
            this.#socket,
            this.#drain,
            FrameKind.Ping,
            0,
            frame.header.channel,
            frame.header.corr,
            new Uint8Array(),
          );
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
    await new Promise<void>((resolve, reject) => {
      this.#quitWaiter = { corr, resolve, reject };
      void writeFrame(
        this.#socket,
        this.#drain,
        FrameKind.Call,
        0,
        LIFECYCLE_CHANNEL,
        corr,
        new Uint8Array([0x00]),
      ).catch(reject);
    });
    this.close();
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#socket.end();
  }
}

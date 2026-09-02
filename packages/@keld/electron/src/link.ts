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
  StreamOpen: 5,
  StreamChunk: 6,
  StreamClose: 7,
  Cancel: 8,
  Grant: 9,
  Ping: 10,
} as const;

/** Header flag mirroring `keld_ipc::frame::FLAG_RAW`. */
export const FLAG_RAW = 1 << 0;

export type LifecycleEventName = "ready" | "last-window-closed";

function kipcError(code: string, detail: string): Error {
  return new Error(`${code}: ${detail}`);
}

function ioDeadlineExceeded(): Error {
  return kipcError(
    "KELD-IPC-006",
    "app-link I/O deadline exceeded. Check the peer is still running and sending kipc frames; a silent or wedged process will not be waited on forever.",
  );
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
  if (bytes[3] > 10) {
    throw kipcError("KELD-IPC-002", `unknown kipc frame kind: ${bytes[3]} (valid kinds are 0..=10)`);
  }
  return {
    kind: bytes[3],
    flags: view.getUint16(4, true),
    channel: view.getUint16(6, true),
    corr: view.getUint32(8, true),
    len: view.getUint32(12, true),
  };
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
    channel: 1,
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

export function echoReplyWaiter(corr: number): ReceivePolicy {
  return { channel: 1, kinds: [FrameKind.Reply], corr: { rule: "exactly", id: corr } };
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
    channel: 1,
    alsoChannel: LIFECYCLE_CHANNEL,
    kinds: [FrameKind.Call],
    corr: { rule: "non-zero" },
    allowPing: true,
  };
}

/**
 * Mirror of `keld_ipc::receive::validate_received_header` with the same
 * fixed check order (kind → flags → channel → correlation → declared length)
 * and the same `KELD-IPC-005` details, so both languages produce identical
 * corpus results. Throws; returns the header unchanged on admission.
 *
 * @throws Error whose message begins with `KELD-IPC-005` naming the first
 * rule the header violates.
 */
export function validateReceivedHeader(
  policy: ReceivePolicy,
  header: { kind: number; flags: number; channel: number; corr: number; len: number },
): { kind: number; flags: number; channel: number; corr: number; len: number } {
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
 * Legacy Windows diagnostic `KELD_APP_LINK` endpoints are strict decimal
 * loopback ports — not `Number.parseInt`, which accepts
 * `"127.0.0.1:9000"` as `127`.
 */
export function parseWin32Port(endpoint: string): number {
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

/** True only for a host-minted Keld Windows named-pipe endpoint. */
export function isWin32PipeEndpoint(endpoint: string): boolean {
  return /^\\\\\.\\pipe\\keld-[0-9a-f]{64}$/.test(endpoint);
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
 *
 * One in-flight `readFrame()` only. A second call while `#pending` is set is
 * `KELD-IPC-005` (unexpected session state); it must not overwrite the waiter.
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
  // One wall-clock for every write + drain.wait in this frame. Restarting
  // withIoDeadline per drain.wait() lets a peer that keeps draining (tiny
  // chunks, or write()==0 forever) never hit KELD-IPC-006.
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
      // Bun `socket.write` returning 0 parks on drain. A missing drain
      // event must not hang HELLO, ping, or quit writes.
      await withIoDeadline(drain.wait(), remaining);
    }
  }
}

/**
 * One-at-a-time frame writer. Concurrent `writeOneFrame` calls interleave
 * bytes on the stream (and used to hang on a single-slot drain).
 *
 * The first `writeOneFrame` failure poisons the queue: later `writeFrame`
 * calls reject without sending bytes. Swallowing the failure used to let a
 * second frame follow a truncated one (peer `KELD-IPC-002`).
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

/**
 * Reads one postcard string starting at `offset`; returns it with the index
 * one past its last byte. It does not require the string to span the whole
 * buffer, so a multi-field payload (`CallError`) is decoded field by field.
 */
function decodePostcardStringAt(bytes: Uint8Array, offset: number): [string, number] {
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
  return (
    e instanceof Error &&
    typeof (e as { code?: unknown }).code === "string" &&
    (e as { code: string }).code.startsWith("KELD-")
  );
}

/**
 * Decodes the `Err` payload every privileged channel writes: a postcard
 * `CallError { code, message }` (`crates/keld-ipc/src/call_error.rs`,
 * `docs/architecture/02-ipc.md` §2).
 *
 * `code` is read as a field — never parsed back out of `message` — so a peer
 * branches on the registered `KELD-*` code directly.
 */
export function decodeCallError(payload: Uint8Array): { code: string; message: string } {
  const [code, afterCode] = decodePostcardStringAt(payload, 0);
  const [message, end] = decodePostcardStringAt(payload, afterCode);
  if (end !== payload.length) {
    throw kipcError("KELD-IPC-003", "trailing bytes after CallError");
  }
  if (!code.startsWith("KELD-")) {
    // A `code` is a registered `KELD-*` identifier by contract (spec 02 §2).
    // Refusing here keeps the decoded value and `isCallError` from disagreeing.
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
    // Not a CallError: a pre-KEL-102 host, or a corrupt frame. Surface it as a
    // protocol error rather than guessing at the bytes.
    return kipcError("KELD-IPC-005", "peer sent an Err payload that is not a CallError");
  }
  const error = new Error(
    call.message.startsWith(call.code) ? call.message : `${call.code}: ${call.message}`,
  );
  // Machine-readable: callers match on `code`, not on the message text.
  // Writable/configurable so a consumer may re-wrap or forward it without a
  // TypeError under ESM strict mode.
  Object.defineProperty(error, "code", {
    value: call.code,
    enumerable: true,
    writable: true,
    configurable: true,
  });
  return error;
}

export type LifecycleHandler = {
  onReady: () => void;
  onLastWindowClosed: () => void;
  /**
   * Read loop died after HELLO. `app.whenReady()` waiters must reject here;
   * a throw must not skip `#quitWaiter` drain.
   */
  onLinkDead: (err: Error) => void;
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
  #quitPromise: Promise<void> | undefined;
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
      process.platform === "win32" && !isWin32PipeEndpoint(endpoint)
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
      await withIoDeadline(writes.writeFrame(FrameKind.Hello, 0, 0, 0, token));
      const helloReply = await withIoDeadline(reader.readFrame());
      // kel133 AC4: shape (kind/flags/channel/corr/exact 32-byte length) is
      // the shared validator's 005; 007 stays reserved for an exactly shaped
      // foreign token.
      validateReceivedHeader(RECEIVE_POLICIES.clientAwaitHello, helloReply.header);
      if (!timingSafeEqual(helloReply.payload, token)) {
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
        // kel133 AC1-AC2/AC5: every frame is admitted by the shared corpus
        // rules before any dispatch. An undeclared frame is KELD-IPC-005 and
        // tears the session down — the old silent fall-through is deleted.
        if (frame.header.kind === FrameKind.Ping) {
          validateReceivedHeader(RECEIVE_POLICIES.lifecycleEventReceiver, frame.header);
          await withIoDeadline(
            this.#writes.writeFrame(
              FrameKind.Ping,
              0,
              frame.header.channel,
              frame.header.corr,
              new Uint8Array(),
            ),
          );
          continue;
        }
        if (frame.header.kind === FrameKind.Event) {
          validateReceivedHeader(RECEIVE_POLICIES.lifecycleEventReceiver, frame.header);
          const event = decodeEvent(frame.payload);
          if (event === "ready") handlers.onReady();
          else handlers.onLastWindowClosed();
          continue;
        }
        if (
          (frame.header.kind === FrameKind.Reply || frame.header.kind === FrameKind.Err) &&
          this.#quitWaiter !== null
        ) {
          const waiter = this.#quitWaiter;
          validateReceivedHeader(lifecycleReplyWaiter(waiter.corr), frame.header);
          this.#quitWaiter = null;
          if (frame.header.kind === FrameKind.Reply) {
            waiter.resolve();
          } else {
            waiter.reject(errorFromErrFrame(frame.payload));
          }
          continue;
        }
        // Anything else is adjudicated by the shared validator against the
        // session's event policy so the rejection detail has one owner.
        validateReceivedHeader(RECEIVE_POLICIES.lifecycleEventReceiver, frame.header);
        throw kipcError("KELD-IPC-005", "frame kind is not declared by the session policy");
      }
    };
    void run().catch((err: Error) => {
      const localClose = this.#closed;
      try {
        this.close();
        // Local quit()/close() ends the socket; that is not a peer drop.
        if (!localClose) {
          try {
            handlers.onLinkDead(err);
          } catch {
            // Isolate: a throwing listener must not skip quit-waiter drain.
          }
        }
      } finally {
        const waiter = this.#quitWaiter;
        this.#quitWaiter = null;
        waiter?.reject(err);
      }
    });
  }

  async quit(): Promise<void> {
    if (this.#quitPromise) return this.#quitPromise;
    if (this.#closed) {
      throw kipcError("KELD-IPC-001", "session is closed");
    }
    this.#quitPromise = this.#quitOnce();
    return this.#quitPromise;
  }

  async #quitOnce(): Promise<void> {
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

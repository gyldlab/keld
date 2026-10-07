// PANEL-P1 scratch client helpers (gyldlab/keld#418). Scratch prototype only.
// Every frame is built and parsed by the real @keld/kipc transport (read-only
// checkout, imported by absolute path; no install).
import * as K from "/Users/centillionaire/WORK/keld/packages/@keld/kipc/src/transport.ts";

export { K };

/** Scratch-only EVENT/GRANT channel; mirrors the host's PROBE_CHANNEL. */
export const PROBE_CHANNEL = 9;

export const probeEventPolicy: K.ReceivePolicy = {
  channel: PROBE_CHANNEL,
  kinds: [K.FrameKind.Event],
  corr: { rule: "zero" },
};

export function emit(obj: Record<string, unknown>): void {
  console.log(JSON.stringify(obj));
}

export function wallUs(): number {
  return Math.round((performance.timeOrigin + performance.now()) * 1000);
}

export function arg(name: string, dflt: string): string {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && i + 1 < process.argv.length ? (process.argv[i + 1] as string) : dflt;
}

export function flag(name: string): boolean {
  return process.argv.includes(`--${name}`);
}

/** postcard `(seq: u32, pad: Vec<u8>)`; rejects trailing bytes. */
export function decodeProbeEvent(payload: Uint8Array): number {
  const [seq, off] = K.decodeVarint(payload, 0);
  const [padLen, off2] = K.decodeVarint(payload, off);
  if (off2 + padLen !== payload.length) {
    throw K.kipcError("KELD-IPC-003", "probe EVENT payload length mismatch");
  }
  return seq;
}

/** postcard `EchoRequest { message: String, count: u32 }`. */
export function encodeEcho(message: string, count: number): Uint8Array {
  const s = K.encodePostcardString(message, "EchoRequest.message");
  const c = K.encodeVarint(count);
  const out = new Uint8Array(s.length + c.length);
  out.set(s, 0);
  out.set(c, s.length);
  return out;
}

export function decodeEcho(payload: Uint8Array): { message: string; count: number } {
  const [message, off] = K.decodePostcardStringAt(payload, 0);
  const [count, end] = K.decodeVarint(payload, off);
  if (end !== payload.length) throw K.kipcError("KELD-IPC-003", "trailing bytes after EchoResponse");
  return { message, count };
}

/** Client HELLO over the real WriteQueue/FrameReader. */
export async function handshake(
  wq: K.WriteQueue,
  reader: K.FrameReader,
  token: Uint8Array,
): Promise<void> {
  await wq.writeFrame(K.FrameKind.Hello, 0, 0, 0, token);
  const f = await K.withIoDeadline(reader.readFrame());
  K.validateReceivedHeader(K.CLIENT_AWAIT_HELLO, f.header);
  if (!K.timingSafeEqual(f.payload, token)) {
    throw K.kipcError("KELD-IPC-007", "server HELLO token mismatch");
  }
}

/** Sequence oracle for the in-order replay criterion. */
export class OrderCheck {
  count = 0;
  next = 0;
  dupes = 0;
  gaps = 0;
  outOfOrder = 0;
  #seen = new Set<number>();
  push(seq: number): void {
    this.count += 1;
    if (this.#seen.has(seq)) {
      this.dupes += 1;
      return;
    }
    this.#seen.add(seq);
    if (seq === this.next) {
      this.next += 1;
    } else if (seq > this.next) {
      this.gaps += seq - this.next;
      this.outOfOrder += 1;
      this.next = seq + 1;
    } else {
      this.outOfOrder += 1;
    }
  }
  result(expected: number): Record<string, unknown> {
    const inOrder =
      this.count === expected && this.dupes === 0 && this.gaps === 0 && this.outOfOrder === 0;
    return {
      received: this.count,
      expected,
      dupes: this.dupes,
      gaps: this.gaps,
      outOfOrder: this.outOfOrder,
      inOrder,
    };
  }
}

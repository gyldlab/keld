// PANEL-P1 arm B worker: owns the role's ONE kipc socket for the whole session.
// Drains the link continuously (the main thread may be parked), appends every
// EVENT frame to a bounded SharedArrayBuffer byte ring in arrival order, and posts
// REPLY/ERR/close into a reply slot followed by Atomics.notify on CTRL.SEQ.
import { K, PROBE_CHANNEL, emit, handshake, wallUs } from "./common.ts";
import { CTRL, CLOSE, CTRL_BYTES, STATE } from "./ring.ts";

declare var self: Worker;

interface Init {
  sab: SharedArrayBuffer;
  ringBytes: number;
  replyBytes: number;
  link: string;
  noNotify: boolean;
  creditFrames: number;
  dieAfterCallMs: number;
}

let ctrl: Int32Array;
let ring: Uint8Array;
let reply: Uint8Array;
let ringBytes = 0;
let noNotify = false;
let dieAfterCallMs = -1;
let wq: K.WriteQueue;
let sock: K.KipcSocket;
const pending = new Set<number>(); // corr ids of CALLs main has issued
let eventsIn = 0;
let lastEventWall = 0;
let repliesFromHost = 0;
let kickArmed = true;

function wake(): void {
  Atomics.add(ctrl, CTRL.SEQ, 1);
  if (!noNotify) Atomics.notify(ctrl, CTRL.SEQ);
}

function closeWith(code: number, detail: string): void {
  if (Atomics.load(ctrl, CTRL.STATE) !== STATE.OPEN) return;
  Atomics.store(ctrl, CTRL.CLOSE_CODE, code);
  Atomics.store(ctrl, CTRL.STATE, code === CLOSE.OVERFLOW ? STATE.OVERFLOW : STATE.CLOSED);
  wake();
  emit({ ev: "worker_closed", code, detail, events_in: eventsIn, replies_from_host: repliesFromHost, wall_us: wallUs() });
  self.postMessage({ op: "closed", code, detail });
}

function appendFrame(header: Uint8Array, payload: Uint8Array): boolean {
  const n = header.length + payload.length;
  const w = Atomics.load(ctrl, CTRL.W);
  const r = Atomics.load(ctrl, CTRL.R);
  const used = w - r;
  if (used + n > ringBytes) return false;
  let pos = w % ringBytes;
  for (const part of [header, payload]) {
    const first = Math.min(part.length, ringBytes - pos);
    ring.set(part.subarray(0, first), pos);
    if (first < part.length) ring.set(part.subarray(first), 0);
    pos = (pos + part.length) % ringBytes;
  }
  Atomics.store(ctrl, CTRL.W, w + n); // publish after the bytes are written
  Atomics.add(ctrl, CTRL.EVENTS, 1);
  if (used + n > Atomics.load(ctrl, CTRL.HIGH_WATER)) Atomics.store(ctrl, CTRL.HIGH_WATER, used + n);
  return true;
}

async function pump(reader: K.FrameReader): Promise<void> {
  for (;;) {
    let f: K.DecodedFrame;
    try {
      f = await reader.readFrame();
    } catch (e) {
      closeWith(CLOSE.PEER, (e as Error).message);
      return;
    }
    const h = f.header;
    if (h.kind === K.FrameKind.Event) {
      eventsIn += 1;
      lastEventWall = wallUs();
      if (!appendFrame(K.encodeHeader(h), f.payload)) {
        // Fail closed: dropping an EVENT would break the in-order contract.
        closeWith(CLOSE.OVERFLOW, `parked replay ring full (${ringBytes} bytes)`);
        sock.end(); // close the role's one link; never drop silently
        return;
      }
      if (kickArmed) {
        kickArmed = false;
        self.postMessage({ op: "kick" }); // delivered when main's event loop runs
      }
      continue;
    }
    if ((h.kind === K.FrameKind.Reply || h.kind === K.FrameKind.Err) && pending.has(h.corr)) {
      if (f.payload.length > reply.length) {
        closeWith(CLOSE.PEER, "reply larger than reply slot");
        return;
      }
      pending.delete(h.corr);
      repliesFromHost += 1;
      reply.set(f.payload, 0);
      Atomics.store(ctrl, CTRL.REPLY_KIND, h.kind);
      Atomics.store(ctrl, CTRL.REPLY_CHANNEL, h.channel);
      Atomics.store(ctrl, CTRL.REPLY_CORR, h.corr);
      Atomics.store(ctrl, CTRL.REPLY_LEN, f.payload.length);
      emit({ ev: "worker_reply", corr: h.corr, events_in: eventsIn, last_event_wall_us: lastEventWall, wall_us: wallUs() });
      wake();
      continue;
    }
    closeWith(CLOSE.PEER, `KELD-IPC-005: undeclared frame kind ${h.kind} corr ${h.corr}`);
    return;
  }
}

self.onmessage = async (e: MessageEvent) => {
  const m = e.data as { op: string } & Record<string, unknown>;
  if (m.op === "init") {
    const init = m as unknown as Init;
    ringBytes = init.ringBytes;
    noNotify = init.noNotify;
    dieAfterCallMs = init.dieAfterCallMs;
    ctrl = new Int32Array(init.sab, 0, CTRL_BYTES / 4);
    ring = new Uint8Array(init.sab, CTRL_BYTES, ringBytes);
    reply = new Uint8Array(init.sab, CTRL_BYTES + ringBytes, init.replyBytes);
    const link = K.parseAppLink(init.link);
    const reader = new K.FrameReader();
    const drain = new K.DrainSignal();
    sock = await K.connectKipcSocket(link.endpoint, reader, drain);
    wq = new K.WriteQueue(sock, drain);
    await handshake(wq, reader, link.token);
    if (init.creditFrames > 0) {
      // Arm B+C: frame credit equals free ring slots (64-byte EVENT frames).
      await wq.writeFrame(K.FrameKind.Grant, 0, PROBE_CHANNEL, 0, K.encodeVarint(init.creditFrames));
    }
    self.postMessage({ op: "ready", wall_us: wallUs() });
    void pump(reader);
    return;
  }
  if (m.op === "call") {
    const corr = m.corr as number;
    pending.add(corr);
    if (dieAfterCallMs >= 0) {
      // Probe only: the transport worker dies while main is parked.
      setTimeout(() => {
        emit({ ev: "worker_dying", wall_us: wallUs() });
        self.close();
      }, dieAfterCallMs);
    }
    try {
      await wq.writeFrame(K.FrameKind.Call, 0, m.channel as number, corr, m.payload as Uint8Array);
    } catch (err) {
      closeWith(CLOSE.PEER, (err as Error).message);
    }
    return;
  }
  if (m.op === "freed") {
    wq.writeFrame(K.FrameKind.Grant, 0, PROBE_CHANNEL, 0, K.encodeVarint(m.n as number)).catch(() => undefined);
    return;
  }
  if (m.op === "rearm-kick") {
    kickArmed = true;
    if (Atomics.load(ctrl, CTRL.W) !== Atomics.load(ctrl, CTRL.R)) {
      kickArmed = false;
      self.postMessage({ op: "kick" });
    }
    return;
  }
  if (m.op === "stats") {
    self.postMessage({ op: "stats", events_in: eventsIn, last_event_wall_us: lastEventWall, replies_from_host: repliesFromHost });
  }
};

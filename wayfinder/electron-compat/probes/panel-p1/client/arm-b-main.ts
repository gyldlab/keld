// PANEL-P1 arm B (and arm E) main thread. The role's one kipc socket is owned by
// arm-b-worker.ts. Main issues a blocking host CALL: it posts the CALL to the worker
// and parks in Atomics.wait on CTRL.SEQ until the worker posts the REPLY/ERR (or a
// typed close). EVENTs received meanwhile sit in the shared ring and are replayed to
// main-thread listeners in arrival order after wake.
//
// Modes: scenario (B), rtt (diagnostic), retire (E1), quit (E2), nc-notify (NC2).
// Flags: --no-notify (worker skips Atomics.notify), --drop-seq N (NC3: replay path
// drops one EVENT), --ring BYTES, --second-link, --expect-events N.
import { K, OrderCheck, arg, decodeEcho, decodeProbeEvent, emit, encodeEcho, flag, probeEventPolicy, wallUs } from "./common.ts";
import { CLOSE, CTRL, CTRL_BYTES, STATE } from "./ring.ts";

const mode = arg("mode", "scenario");
const ringBytes = Number(arg("ring", String(1 << 20)));
const replyBytes = 64 * 1024;
const dropSeq = Number(arg("drop-seq", "-1"));
const expected = Number(arg("expect-events", "11000"));
const linkText = process.env.KELD_APP_LINK ?? "";
const creditFrames = flag("credit-ring") ? Math.floor(ringBytes / 64) : 0;

const sab = new SharedArrayBuffer(CTRL_BYTES + ringBytes + replyBytes);
const ctrl = new Int32Array(sab, 0, CTRL_BYTES / 4);
const ring = new Uint8Array(sab, CTRL_BYTES, ringBytes);
const reply = new Uint8Array(sab, CTRL_BYTES + ringBytes, replyBytes);

const order = new OrderCheck();
const listeners: Array<(seq: number) => void> = [(seq) => order.push(seq)];
let replayed = 0;

function ringRead(pos: number, n: number): Uint8Array {
  const out = new Uint8Array(n);
  const at = pos % ringBytes;
  const first = Math.min(n, ringBytes - at);
  out.set(ring.subarray(at, at + first), 0);
  if (first < n) out.set(ring.subarray(0, n - first), first);
  return out;
}

/** Replays every EVENT the worker has published, in ring (= arrival) order. */
function drainRing(): number {
  let r = Atomics.load(ctrl, CTRL.R);
  const w = Atomics.load(ctrl, CTRL.W);
  let n = 0;
  while (r < w) {
    const header = K.decodeHeader(ringRead(r, K.HEADER_LEN));
    K.validateReceivedHeader(probeEventPolicy, header);
    const seq = decodeProbeEvent(ringRead(r + K.HEADER_LEN, header.len));
    r += K.HEADER_LEN + header.len;
    Atomics.store(ctrl, CTRL.R, r); // frees ring space for the worker
    n += 1;
    if (seq === dropSeq) continue; // NC3 only
    for (const l of listeners) l(seq);
  }
  replayed += n;
  if (creditFrames > 0 && n > 0) worker.postMessage({ op: "freed", n });
  return n;
}

const worker = new Worker(new URL("./arm-b-worker.ts", import.meta.url).href);
let closedMsg: { code: number; detail: string } | null = null;
let onClosed: (() => void) | null = null;
const ready = new Promise<void>((resolve) => {
  worker.onmessage = (e: MessageEvent) => {
    const m = e.data as { op: string; code?: number; detail?: string };
    if (m.op === "ready") resolve();
    else if (m.op === "kick") {
      drainRing();
      worker.postMessage({ op: "rearm-kick" });
    } else if (m.op === "closed") {
      closedMsg = { code: m.code ?? 0, detail: m.detail ?? "" };
      onClosed?.();
    }
  };
});
worker.postMessage({ op: "init", sab, ringBytes, replyBytes, link: linkText, noNotify: flag("no-notify"), creditFrames, dieAfterCallMs: Number(arg("worker-die-ms", "-1")) });
await ready;
emit({ ev: "ready", mode, ring_bytes: ringBytes, wall_us: wallUs() });

if (flag("second-link")) {
  const { endpoint } = K.parseAppLink(linkText);
  let outcome: string;
  try {
    const s = await Bun.connect({ unix: endpoint, socket: { data() {}, open() {}, close() {}, error() {} } });
    outcome = "connected";
    s.end();
  } catch (e) {
    outcome = `refused: ${(e as { code?: string }).code ?? (e as Error).message}`;
  }
  emit({ ev: "second_link", outcome, refused: outcome.startsWith("refused") });
}

let nextCorr = 1;
interface SyncResult {
  kind: number;
  payload: Uint8Array;
  parkedMs: number;
  waits: number;
  lastWait: string;
}
function typedError(code: string, detail: string): Error & { code: string } {
  const e = new Error(`${code}: ${detail}`) as Error & { code: string };
  e.code = code;
  return e;
}

/** Blocking host CALL: no Promise is observable to the caller. */
function callSync(channel: number, payload: Uint8Array, deadlineMs: number): SyncResult {
  const corr = nextCorr++;
  let seq = Atomics.load(ctrl, CTRL.SEQ);
  worker.postMessage({ op: "call", channel, corr, payload });
  const t0 = performance.now();
  let waits = 0;
  let lastWait = "none";
  for (;;) {
    // The deadline is authoritative: a reply that was never signalled before it
    // expired is a timeout, not a late success.
    if (lastWait === "timed-out" && performance.now() - t0 >= deadlineMs) {
      throw typedError("KELD-IPC-006", `blocking CALL deadline ${deadlineMs} ms exceeded`);
    }
    const now = Atomics.load(ctrl, CTRL.SEQ);
    if (now !== seq) {
      if (Atomics.load(ctrl, CTRL.REPLY_CORR) === corr) {
        const kind = Atomics.load(ctrl, CTRL.REPLY_KIND);
        const len = Atomics.load(ctrl, CTRL.REPLY_LEN);
        const header = { kind, flags: 0, channel: Atomics.load(ctrl, CTRL.REPLY_CHANNEL), corr, len };
        K.validateReceivedHeader(channel === K.ECHO_CHANNEL ? K.echoReplyWaiter(corr) : K.lifecycleReplyWaiter(corr), header);
        const out = reply.slice(0, len);
        if (kind === K.FrameKind.Err) throw K.errorFromErrFrame(out);
        return { kind, payload: out, parkedMs: performance.now() - t0, waits, lastWait };
      }
      const st = Atomics.load(ctrl, CTRL.STATE);
      if (st !== STATE.OPEN) {
        const code = Atomics.load(ctrl, CTRL.CLOSE_CODE);
        throw code === CLOSE.OVERFLOW
          ? typedError("KELD-IPC-004", "parked EVENT replay ring is full; the link was closed, no reply was received")
          : typedError("KELD-IPC-001", "link closed (peer close or generation retire) during a blocking CALL; no reply was received");
      }
      seq = now;
    }
    const elapsed = performance.now() - t0;
    if (elapsed >= deadlineMs) {
      throw typedError("KELD-IPC-006", `blocking CALL deadline ${deadlineMs} ms exceeded`);
    }
    lastWait = Atomics.wait(ctrl, CTRL.SEQ, seq, deadlineMs - elapsed);
    waits += 1;
  }
}

function waitClosed(ms: number): Promise<boolean> {
  if (closedMsg) return Promise.resolve(true);
  return new Promise((resolve) => {
    const t = setTimeout(() => resolve(false), ms);
    onClosed = () => {
      clearTimeout(t);
      resolve(true);
    };
  });
}

function quitSync(): Record<string, unknown> {
  try {
    const q = callSync(K.LIFECYCLE_CHANNEL, new Uint8Array([0]), 5000);
    return { quit_reply: q.kind === K.FrameKind.Reply && q.payload.length === 1 && q.payload[0] === 0, parked_ms: +q.parkedMs.toFixed(2) };
  } catch (e) {
    return { quit_reply: false, error: (e as Error).message };
  }
}

const deadline = Number(arg("call-deadline-ms", "30000"));
const parkWall = wallUs();
if (mode === "scenario") {
  let r: SyncResult | null = null;
  let error: { code?: string; message: string } | null = null;
  try {
    r = callSync(K.ECHO_CHANNEL, encodeEcho("scenario", 7), deadline);
  } catch (e) {
    error = { code: (e as { code?: string }).code, message: (e as Error).message };
  }
  const wakeWall = wallUs();
  const echo = r ? decodeEcho(r.payload) : null;
  let n = 0;
  let replayError: string | null = null;
  try {
    n = drainRing();
  } catch (e) {
    replayError = (e as Error).message;
  }
  emit({
    ev: "woke",
    mode,
    reply_ok: echo !== null && echo.message === "scenario" && echo.count === 7,
    error,
    replay_error: replayError,
    parked_ms: r ? +r.parkedMs.toFixed(1) : (wakeWall - parkWall) / 1000,
    waits: r?.waits ?? null,
    last_wait: r?.lastWait ?? null,
    park_start_wall_us: parkWall,
    wake_wall_us: wakeWall,
    ring_events_published: Atomics.load(ctrl, CTRL.EVENTS),
    ring_high_water_bytes: Atomics.load(ctrl, CTRL.HIGH_WATER),
    replayed_after_wake: n,
    order: order.result(expected),
  });
  if (r && creditFrames > 0) {
    // Credit-suspended EVENTs resume only after wake; await them via kick-driven drains.
    const t = performance.now();
    if (order.count < expected) {
      await new Promise<void>((res) => {
        const guard = setTimeout(res, 15000); // bound only; completion is listener-driven
        listeners.push(() => {
          if (order.count >= expected) {
            clearTimeout(guard);
            res();
          }
        });
      });
    }
    emit({ ev: "post_wake_delivery", ms: +(performance.now() - t).toFixed(1), order: order.result(expected) });
  }
  if (r) {
    const q = quitSync();
    const closed = await waitClosed(3000);
    emit({ ev: "end", ...q, link_closed_after_quit: closed, close: closedMsg });
  } else {
    emit({ ev: "end", close: closedMsg });
  }
} else if (mode === "rtt" || mode === "nc-notify") {
  const calls = mode === "rtt" ? Number(arg("calls", "1000")) : 1;
  const per: number[] = [];
  let err: string | null = null;
  for (let i = 0; i < calls; i += 1) {
    const t = performance.now();
    try {
      const r = callSync(K.ECHO_CHANNEL, encodeEcho("rtt", i), deadline);
      if (decodeEcho(r.payload).count !== i) throw new Error("echo mismatch");
    } catch (e) {
      err = (e as Error).message;
      per.push(performance.now() - t);
      break;
    }
    per.push(performance.now() - t);
  }
  const sorted = [...per].sort((a, b) => a - b);
  const pct = (p: number) => +(sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))] ?? 0).toFixed(4);
  emit({ ev: "rtt", mode, calls: per.length, error: err, p50_ms: pct(0.5), p99_ms: pct(0.99), max_ms: +(sorted.at(-1) ?? 0).toFixed(3) });
  const q = quitSync();
  emit({ ev: "end", ...q });
} else if (mode === "retire") {
  let returned: unknown = null;
  let error: { code?: string; message: string } | null = null;
  try {
    returned = callSync(K.ECHO_CHANNEL, encodeEcho("scenario", 7), deadline);
  } catch (e) {
    error = { code: (e as { code?: string }).code, message: (e as Error).message };
  }
  const wakeWall = wallUs();
  const n = drainRing();
  emit({
    ev: "woke",
    mode,
    fabricated_reply: returned !== null,
    error,
    park_start_wall_us: parkWall,
    wake_wall_us: wakeWall,
    parked_ms: (wakeWall - parkWall) / 1000,
    replayed_after_wake: n,
    order: order.result(order.count),
    close: closedMsg,
  });
} else if (mode === "quit") {
  let q: SyncResult | null = null;
  let error: string | null = null;
  try {
    q = callSync(K.LIFECYCLE_CHANNEL, new Uint8Array([0]), deadline);
  } catch (e) {
    error = (e as Error).message;
  }
  const wakeWall = wallUs();
  const n = drainRing();
  const closed = await waitClosed(3000);
  emit({
    ev: "woke",
    mode,
    quit_reply: q !== null && q.kind === K.FrameKind.Reply && q.payload.length === 1 && q.payload[0] === 0,
    error,
    parked_ms: q ? +q.parkedMs.toFixed(1) : null,
    park_start_wall_us: parkWall,
    wake_wall_us: wakeWall,
    replayed_after_wake: n,
    order: order.result(expected),
    link_closed_after_quit: closed,
    close: closedMsg,
  });
}
emit({ ev: "exit", replayed_total: replayed, wall_us: wallUs() });
worker.terminate();
process.exit(0);

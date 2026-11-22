/**
 * GH-1081 round 3 (research only; never-merge draft PR #1082): the exact code
 * a caller gets today, Bun side, with a fake host on a real socket and the
 * real public-API role (`gh1081-r3.test-role.ts`) on the real WorkerLink.
 *
 * Oracles are independent of the code under test: the exact ERR/REPLY bytes
 * the host reads off the wire, a per-correlation answer census over every
 * frame until the role's socket closes, and the role's own handler census.
 * Every negative control changes one input and must fail the treatment's
 * oracle; each test asserts that it does. Waits are on observable frames,
 * reports or socket close. Kill switches are deadlines only.
 */
import { describe, expect, test } from "bun:test";
import { chmodSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  APP_LINK_IO_DEADLINE_MS,
  DrainSignal,
  ECHO_CHANNEL,
  FS_CHANNEL,
  FrameKind,
  FrameReader,
  HEADER_LEN,
  LIFECYCLE_CHANNEL,
  WriteQueue,
  decodeCallError,
  decodePostcardStringAt,
  decodeVarint,
  encodeHeader,
  encodePostcardString,
  encodeVarint,
  type DecodedFrame,
} from "../../kipc/src/transport.ts";

const TOKEN = new Uint8Array(32).fill(0x72);
/** Kill switch only: no wait below is satisfied by elapsed time. */
const KILL_SWITCH_MS = 30_000;
const API_001 = { code: "KELD-API-001", message: "KELD-API-001: application channel call failed" };

function within<T>(promise: Promise<T>, what: string, ms = KILL_SWITCH_MS): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const expired = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`gh1081 r3 kill switch: ${what}`)), ms);
  });
  return Promise.race([promise, expired]).finally(() => clearTimeout(timer));
}

function echoRequest(message: string, count: number): Uint8Array {
  const text = encodePostcardString(message, "gh1081 r3 echo message");
  const number = encodeVarint(count);
  const out = new Uint8Array(text.length + number.length);
  out.set(text);
  out.set(number, text.length);
  return out;
}

function echoResponse(bytes: Uint8Array): { message: string; count: number } {
  const [message, afterMessage] = decodePostcardStringAt(bytes, 0);
  const [count, end] = decodeVarint(bytes, afterMessage);
  if (end !== bytes.length) throw new Error("trailing bytes after Echo response");
  return { message, count };
}

function frameBytes(kind: number, channel: number, corr: number, payload: Uint8Array): Uint8Array {
  const out = new Uint8Array(HEADER_LEN + payload.length);
  out.set(encodeHeader(kind, 0, channel, corr, payload.length));
  out.set(payload, HEADER_LEN);
  return out;
}

/** What one Echo CALL got back, from the wire only. */
type Answer =
  | { kind: "reply"; message: string; count: number }
  | { kind: "err"; code: string; message: string };

function answerOf(frame: DecodedFrame): Answer {
  if (frame.header.kind === FrameKind.Reply) return { kind: "reply", ...echoResponse(frame.payload) };
  if (frame.header.kind === FrameKind.Err) return { kind: "err", ...decodeCallError(frame.payload) };
  throw new Error(`unexpected answer kind ${frame.header.kind}`);
}

interface Host {
  /** Next frame from the role; every frame is kept for the census. */
  next(): Promise<DecodedFrame>;
  send(kind: number, channel: number, corr: number, payload: Uint8Array): Promise<void>;
  /** Raw bytes on the socket, bypassing the frame writer (truncation arms). */
  raw(bytes: Uint8Array): void;
  /** One host release byte on the role's stdin. */
  release(): void;
  reported(key: string, ms?: number): Promise<void>;
  value(key: string): unknown;
  /** Reads until the role closes the socket, answering its Quit; returns all frames. */
  drain(): Promise<DecodedFrame[]>;
  end(): void;
  exit(): Promise<number>;
  dispose(): Promise<void>;
}

async function start(scenario: string): Promise<Host> {
  const root = mkdtempSync(join(tmpdir(), "k3"));
  chmodSync(root, 0o700);
  const path = join(root, "a.s");
  const reader = new FrameReader();
  const drainSignal = new DrainSignal();
  const frames: DecodedFrame[] = [];
  let open!: (socket: Bun.Socket<undefined>) => void;
  const opened = new Promise<Bun.Socket<undefined>>((resolve) => { open = resolve; });
  let closedResolve!: () => void;
  const closed = new Promise<void>((resolve) => { closedResolve = resolve; });
  let connections = 0;
  const listener = Bun.listen<undefined>({ unix: path, socket: {
    binaryType: "uint8array",
    open(socket) { connections += 1; open(socket); },
    data(_socket, data) { reader.push(data); },
    drain() { drainSignal.fire(); },
    error(_socket, error) { reader.end(error); },
    close() { reader.end(new Error("host observed role close")); closedResolve(); },
  } });
  const role = Bun.spawn(["bun", join(import.meta.dir, "gh1081-r3.test-role.ts"), scenario], {
    env: { ...process.env, KELD_APP_LINK: `${path}#${"72".repeat(32)}` },
    stdin: "pipe", stdout: "pipe", stderr: "pipe",
  });
  let text = "";
  const waiters: Array<{ key: string; resolve: () => void; reject: (error: Error) => void }> = [];
  const stdout = (async () => {
    const stream = role.stdout.getReader();
    const decoder = new TextDecoder();
    for (;;) {
      const part = await stream.read();
      if (part.done) {
        for (const waiter of waiters.splice(0)) {
          waiter.reject(new Error(`role exited before reporting ${waiter.key}; output=${text}`));
        }
        return text;
      }
      text += decoder.decode(part.value, { stream: true });
      for (let index = waiters.length - 1; index >= 0; index -= 1) {
        if (text.includes(`${waiters[index]!.key}=`)) waiters.splice(index, 1)[0]!.resolve();
      }
    }
  })();
  const stderr = new Response(role.stderr).text();
  const dispose = async () => {
    if (role.exitCode === null) role.kill();
    await role.exited;
    listener.stop(true);
    rmSync(root, { recursive: true, force: true });
  };
  try {
    const socket = await within(opened, "role connect");
    const writer = new WriteQueue(socket, drainSignal);
    const send = (kind: number, channel: number, corr: number, payload: Uint8Array) =>
      writer.writeFrame(kind, 0, channel, corr, payload);
    const hello = await within(reader.readFrame(), "role HELLO");
    expect(hello.header.kind).toBe(FrameKind.Hello);
    expect(hello.payload).toEqual(TOKEN);
    await send(FrameKind.Hello, 0, 0, TOKEN);
    await send(FrameKind.Event, LIFECYCLE_CHANNEL, 0, new Uint8Array([0]));
    const next = async () => {
      const frame = await within(reader.readFrame(), "next role frame");
      frames.push(frame);
      return frame;
    };
    return {
      next,
      send,
      raw: (bytes) => {
        // Small truncation prefixes fit the socket buffer in one write.
        expect(socket.write(bytes)).toBe(bytes.length);
      },
      release: () => { role.stdin.write(new Uint8Array([1])); role.stdin.flush(); },
      reported: (key, ms) => within(new Promise<void>((resolve, reject) => {
        waiters.push({ key, resolve, reject });
        if (text.includes(`${key}=`)) waiters.splice(waiters.length - 1, 1)[0]!.resolve();
      }), `report ${key}; output=${text}`, ms),
      value: (key) => {
        const line = text.split("\n").find((entry) => entry.startsWith(`${key}=`));
        if (line === undefined) throw new Error(`no ${key} report in ${text}`);
        return JSON.parse(line.slice(key.length + 1));
      },
      async drain() {
        for (;;) {
          let frame: DecodedFrame;
          try { frame = await next(); } catch (error) {
            if (String(error).includes("host observed role close")) break;
            throw error;
          }
          if (frame.header.channel === LIFECYCLE_CHANNEL && frame.header.kind === FrameKind.Call) {
            expect(frame.payload).toEqual(new Uint8Array([0]));
            await send(FrameKind.Reply, LIFECYCLE_CHANNEL, frame.header.corr, new Uint8Array([0]));
          }
        }
        await within(closed, "role socket close");
        expect(connections).toBe(1);
        return frames;
      },
      end: () => socket.end(),
      async exit() {
        const [code, , errors] = await within(Promise.all([role.exited, stdout, stderr]), "role exit");
        if (code !== 0) throw new Error(`role exit ${code}: ${errors}\n${text}`);
        return code;
      },
      dispose,
    };
  } catch (error) {
    await dispose();
    throw error;
  }
}

/** Every Echo answer frame grouped by correlation id, from the host census. */
function echoAnswers(frames: DecodedFrame[]): Map<number, Answer[]> {
  const out = new Map<number, Answer[]>();
  for (const frame of frames) {
    if (frame.header.channel !== ECHO_CHANNEL) continue;
    const rows = out.get(frame.header.corr) ?? [];
    rows.push(answerOf(frame));
    out.set(frame.header.corr, rows);
  }
  return out;
}

function record(name: string, value: unknown): void {
  // One greppable line per arm for the round-3 results file.
  console.log(`GH1081_R3_BUN ${name} ${JSON.stringify(value, (_key, entry: unknown) => (entry instanceof Map ? [...entry] : entry))}`);
}

/** Sends one Echo CALL and returns the one answer frame the host reads for it. */
async function call(host: Host, corr: number, label: string): Promise<Answer> {
  await host.send(FrameKind.Call, ECHO_CHANNEL, corr, echoRequest(label, corr));
  const frame = await host.next();
  expect(frame.header.channel).toBe(ECHO_CHANNEL);
  expect(frame.header.corr).toBe(corr);
  return answerOf(frame);
}

/** Runs one handler-shape scenario: two Echo CALLs, then a full census. */
async function twoCalls(scenario: string, labels: [string, string]) {
  const host = await start(scenario);
  try {
    await host.reported("ready");
    const answers = [await call(host, 11, labels[0]), await call(host, 12, labels[1])];
    host.release();
    const frames = await host.drain();
    await host.exit();
    return { answers, census: echoAnswers(frames), handled: host.value("handled"), effects: host.value("effects") };
  } finally {
    await host.dispose();
  }
}

function isApi001(answer: Answer): boolean {
  return answer.kind === "err" && answer.code === API_001.code && answer.message === API_001.message;
}

function exactlyOnce(census: Map<number, Answer[]>, corrs: number[]): boolean {
  return census.size === corrs.length && corrs.every((corr) => census.get(corr)?.length === 1);
}

describe.skipIf(process.platform === "win32")("GH-1081 r3: handler missing, late, throwing", () => {
  test("gh1081_r3 missing handler: KELD-API-001, handler census 0; NC registered handler replies", async () => {
    const treatment = await twoCalls("missing", ["missing-1", "missing-2"]);
    const control = await twoCalls("ok-control", ["missing-1", "missing-2"]);
    record("missing", treatment);
    record("missing-nc", control);
    expect(treatment.answers.every(isApi001)).toBe(true);
    expect(treatment.handled).toEqual([]);
    expect(exactlyOnce(treatment.census, [11, 12])).toBe(true);
    // NC: the same two CALLs with a registered handler fail the API-001 oracle.
    expect(control.answers.some(isApi001)).toBe(false);
    expect(control.answers).toEqual([
      { kind: "reply", message: "missing-1-reply", count: 12 },
      { kind: "reply", message: "missing-2-reply", count: 13 },
    ]);
    expect(control.handled).toEqual(["missing-1", "missing-2"]);
    expect(exactlyOnce(control.census, [11, 12])).toBe(true);
  }, 60_000);

  test("gh1081_r3 late registration: first call KELD-API-001 and never replayed; NC later call replies", async () => {
    const host = await start("late");
    try {
      await host.reported("unregistered");
      const before = await call(host, 21, "late-before");
      host.release();
      await host.reported("registered");
      const after = await call(host, 22, "late-after");
      host.release();
      const census = echoAnswers(await host.drain());
      await host.exit();
      const handled = host.value("handled");
      record("late", { before, after, handled, census: [...census] });
      expect(isApi001(before)).toBe(true);
      // NC: the call made after registration fails the API-001 oracle.
      expect(isApi001(after)).toBe(false);
      expect(after).toEqual({ kind: "reply", message: "late-after-reply", count: 23 });
      // No replay: registration never ran the earlier call, and each CALL got one answer.
      expect(handled).toEqual(["late-after"]);
      expect(exactlyOnce(census, [21, 22])).toBe(true);
    } finally {
      await host.dispose();
    }
  }, 60_000);

  test("gh1081_r3 throwing handler after its effect: KELD-API-001, effect committed; NC healthy call replies", async () => {
    const run = await twoCalls("throw-after-effect", ["throw-after-effect", "ok"]);
    record("throw-after-effect", run);
    expect(isApi001(run.answers[0]!)).toBe(true);
    expect(run.effects).toEqual(["throw-after-effect"]);
    expect(run.handled).toEqual(["throw-after-effect", "ok"]);
    // NC: the healthy call on the same handler fails the API-001 oracle.
    expect(isApi001(run.answers[1]!)).toBe(false);
    expect(run.answers[1]).toEqual({ kind: "reply", message: "ok-reply", count: 13 });
    expect(exactlyOnce(run.census, [11, 12])).toBe(true);
  }, 60_000);

  test("gh1081_r3 synchronously throwing handler: KELD-API-001 with no effect; bytes equal the missing case", async () => {
    const run = await twoCalls("throw-sync", ["throw-sync-1", "throw-sync-2"]);
    const missing = await twoCalls("missing", ["throw-sync-1", "throw-sync-2"]);
    record("throw-sync", run);
    expect(run.answers.every(isApi001)).toBe(true);
    expect(run.effects).toEqual([]);
    // The handler ran (census 2) although the wire bytes equal the never-ran case.
    expect(run.handled).toEqual(["throw-sync-1", "throw-sync-2"]);
    expect(missing.handled).toEqual([]);
    expect(run.answers).toEqual(missing.answers);
    expect(exactlyOnce(run.census, [11, 12])).toBe(true);
  }, 60_000);
});

/**
 * A truncated host Echo CALL (the Bun half of a partial host write) followed by
 * close or silence. Returns the role's link-end code, its handler census, and
 * every Echo answer frame the host read before the socket closed.
 */
async function truncatedCall(prefix: number, after: "close" | "stall" | "complete") {
  const host = await start("partial");
  try {
    await host.reported("ready");
    const fsCall = await host.next();
    expect(fsCall.header.channel).toBe(FS_CHANNEL);
    // The largest renderer payload (4,096 bytes) as a valid Echo request.
    const message = "p".repeat(4_096 - 3);
    const payload = echoRequest(message, 9);
    expect(payload.length).toBe(4_096);
    const frame = frameBytes(FrameKind.Call, ECHO_CHANNEL, 31, payload);
    host.raw(frame.subarray(0, prefix));
    let answered: Answer | undefined;
    if (after === "complete") {
      // NC: the same prefix, then the remainder of the same frame.
      host.raw(frame.subarray(prefix));
      const reply = await host.next();
      expect(reply.header.corr).toBe(31);
      answered = answerOf(reply);
      host.end();
    } else if (after === "close") {
      host.end();
    }
    // `stall`: the socket stays open; only the role's own frame deadline can end it.
    // `handled` is reported after `link-end`, so both are present.
    await host.reported("handled", KILL_SWITCH_MS);
    const linkEnd = host.value("link-end");
    const callDeadlinesBeforeEnd = host.value("call-deadlines-before-end");
    const handled = host.value("handled");
    await host.exit();
    const frames = await host.drain();
    return { prefix, after, frameLength: frame.length, linkEnd, callDeadlinesBeforeEnd, handled, answered,
      echoFrames: frames.filter((entry) => entry.header.channel === ECHO_CHANNEL).length };
  } finally {
    await host.dispose();
  }
}

describe.skipIf(process.platform === "win32")("GH-1081 r3: a truncated CALL never reaches the handler", () => {
  const frameLength = HEADER_LEN + 4_096;
  for (const prefix of [1, HEADER_LEN - 1, HEADER_LEN, 3_000, frameLength - 1]) {
    test(`gh1081_r3 truncated CALL prefix=${prefix} then close: handler census 0, no answer`, async () => {
      const run = await truncatedCall(prefix, "close");
      record(`truncated-close-${prefix}`, run);
      expect(run.handled).toEqual([]);
      expect(run.echoFrames).toBe(0);
    }, 60_000);
  }

  test("gh1081_r3 truncated CALL then silence: the role's existing frame deadline ends the link; census 0", async () => {
    const started = performance.now();
    const run = await truncatedCall(3_000, "stall");
    const waited = Math.round(performance.now() - started);
    record("truncated-stall-3000", { ...run, waitedMs: waited });
    expect(run.handled).toEqual([]);
    expect(run.echoFrames).toBe(0);
    expect(waited).toBeGreaterThanOrEqual(APP_LINK_IO_DEADLINE_MS);
  }, 60_000);

  test("gh1081_r3 NC: the same prefix completed with its remainder is dispatched and answered once", async () => {
    const run = await truncatedCall(3_000, "complete");
    record("truncated-completed-nc-3000", run);
    // Fails the treatment oracle: the handler ran and the host read an answer.
    expect(run.handled).toEqual(["p".repeat(4_096 - 3)]);
    expect(run.answered).toEqual({ kind: "reply", message: `${"p".repeat(4_093)}-reply`, count: 10 });
    expect(run.echoFrames).toBe(1);
  }, 60_000);
});

describe.skipIf(process.platform === "win32")("GH-1081 r3: Bun-side outbound calls", () => {
  test("gh1081_r3 outbound FS call: link lost after send and call after link end share one code", async () => {
    const host = await start("fs-dead");
    try {
      await host.reported("ready");
      const call = await host.next();
      expect(call.header.channel).toBe(FS_CHANNEL);
      host.end();
      await host.reported("same-error-object");
      const result = {
        afterSend: host.value("after-send"),
        beforeSend: host.value("before-send"),
        sameErrorObject: host.value("same-error-object"),
      };
      await host.exit();
      const frames = await host.drain();
      const fsCalls = frames.filter((frame) => frame.header.channel === FS_CHANNEL).length;
      record("outbound-link-lost", { ...result, hostFsCallCensus: fsCalls });
      expect(result.afterSend).toBe("KELD-IPC-022");
      expect(result.beforeSend).toBe("KELD-IPC-022");
      expect(fsCalls).toBe(1);
    } finally {
      await host.dispose();
    }
  }, 60_000);

  test("gh1081_r3 outbound FS call deadline: KELD-IPC-006, late reply ignored, no resend; NC fresh call answered", async () => {
    const host = await start("fs-deadline");
    try {
      await host.reported("ready");
      const first = await host.next();
      expect(first.header.channel).toBe(FS_CHANNEL);
      // The host never answers the first call; the role's own existing
      // APP_LINK_IO_DEADLINE_MS decides the outcome.
      await host.reported("first-waited-ms");
      const firstCode = host.value("first");
      const waited = host.value("first-waited-ms");
      // A late REPLY for the expired correlation, then the release.
      await host.send(FrameKind.Reply, FS_CHANNEL, first.header.corr, new Uint8Array([0, 1, 0]));
      host.release();
      const second = await host.next();
      expect(second.header.channel).toBe(FS_CHANNEL);
      expect(second.header.corr).not.toBe(first.header.corr);
      await host.send(FrameKind.Reply, FS_CHANNEL, second.header.corr, new Uint8Array([0, 1, 0]));
      await host.reported("second");
      const secondResult = host.value("second");
      const frames = await host.drain();
      await host.exit();
      const fsCalls = frames.filter((frame) => frame.header.channel === FS_CHANNEL);
      record("outbound-deadline", { firstCode, waited, secondResult, hostFsCorrs: fsCalls.map((f) => f.header.corr) });
      expect(firstCode).toBe("KELD-IPC-006");
      expect(waited).toBeGreaterThanOrEqual(APP_LINK_IO_DEADLINE_MS - 1);
      // NC: the fresh call on the same link resolves, so 006 is not a dead link.
      expect(secondResult).toBe("resolved:1");
      // No resend of the expired call: exactly two FS CALL frames, two correlations.
      expect(fsCalls.length).toBe(2);
    } finally {
      await host.dispose();
    }
  }, 60_000);
});

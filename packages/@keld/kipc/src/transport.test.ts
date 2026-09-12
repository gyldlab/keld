/**
 * Contract tests for the shared TypeScript app-link transport (KEL-136).
 *
 * Oracles: keld-ipc constants and error codes, the canonical TSV path (no row
 * copy), chunk-queue shape, and fail-closed unknown kind / wrong-channel Err.
 */
import {
  mkdirSync,
  lstatSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";
import { describe, expect, test } from "bun:test";

import {
  APP_LINK_IO_DEADLINE_MS,
  DirectedReader,
  DrainSignal,
  ECHO_CHANNEL,
  FLAG_RAW,
  FrameKind,
  FrameReader,
  HEADER_LEN,
  LIFECYCLE_CHANNEL,
  MAX_FRAME_LEN,
  MAX_PENDING_CHUNKS,
  MAX_PARKED_FRAMES,
  PROTOCOL_VERSION,
  RECEIVE_POLICIES,
  WriteQueue,
  connectKipcSocket,
  decodePostcardStringAt,
  decodeHeader,
  decodeVarint,
  echoReplyWaiter,
  encodeHeader,
  kipcError,
  lifecycleReplyWaiter,
  validateReceivedHeader,
  withIoDeadline,
} from "./transport.ts";

const REPO_ROOT = join(import.meta.dir, "../../../..");
const CORPUS_PATH = join(
  import.meta.dir,
  "../../../../crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv",
);
const CORPUS_SHA256 = "375f50c4bea1b690dbf7f385aee0464eae0946218058445306240b997d7e9746";
const SKIP_DIR_NAMES = new Set([".git", "node_modules", "target"]);
const SKIP_REPO_DIRS = new Set([
  join(REPO_ROOT, "competitors"),
  join(REPO_ROOT, "docs", "research"),
]);

function skipDirectory(path: string, name: string): boolean {
  return SKIP_DIR_NAMES.has(name) || SKIP_REPO_DIRS.has(path);
}

function encodeFrame(kind: number, channel: number, corr: number, payload: Uint8Array): Uint8Array {
  const header = encodeHeader(kind, 0, channel, corr, payload.length);
  const frame = new Uint8Array(header.length + payload.length);
  frame.set(header, 0);
  frame.set(payload, header.length);
  return frame;
}

function walkFiles(root: string, suffix: string, into: string[]): void {
  for (const name of readdirSync(root)) {
    const path = join(root, name);
    const stat = lstatSync(path);
    if (stat.isSymbolicLink()) continue;
    if (stat.isDirectory()) {
      if (skipDirectory(path, name)) continue;
      walkFiles(path, suffix, into);
      continue;
    }
    if (name.endsWith(suffix)) into.push(path);
  }
}

describe("wire constants match keld-ipc", () => {
  test("Rust source pins the same numbers this module exports", () => {
    const lib = readFileSync(join(REPO_ROOT, "crates/keld-ipc/src/lib.rs"), "utf8");
    const frame = readFileSync(join(REPO_ROOT, "crates/keld-ipc/src/frame.rs"), "utf8");
    const echo = readFileSync(join(REPO_ROOT, "crates/keld-ipc/src/echo.rs"), "utf8");
    const lifecycle = readFileSync(join(REPO_ROOT, "crates/keld-ipc/src/lifecycle.rs"), "utf8");
    expect(lib).toContain("pub const PROTOCOL_VERSION: u8 = 2;");
    expect(lib).toContain("pub const HEADER_LEN: usize = 16;");
    expect(lib).toContain("pub const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;");
    expect(lib).toContain("Duration::from_secs(5)");
    expect(frame).toContain("pub const FLAG_RAW: u16 = 1 << 0;");
    expect(frame).toContain("Ping = 10");
    expect(echo).toContain("ChannelId(1)");
    expect(lifecycle).toContain("ChannelId(3)");
    expect(PROTOCOL_VERSION).toBe(2);
    expect(HEADER_LEN).toBe(16);
    expect(MAX_FRAME_LEN).toBe(16 * 1024 * 1024);
    expect(APP_LINK_IO_DEADLINE_MS).toBe(5_000);
    expect(FLAG_RAW).toBe(1);
    expect(ECHO_CHANNEL).toBe(1);
    expect(LIFECYCLE_CHANNEL).toBe(3);
    expect(FrameKind.Ping).toBe(10);
  });
});

describe("fail-closed header semantics", () => {
  test("unknown kind is KELD-IPC-002", () => {
    const encoded = encodeHeader({ kind: FrameKind.Ping, flags: 0, channel: 0, corr: 0, len: 0 });
    encoded[3] = 11;
    expect(() => decodeHeader(encoded)).toThrow("KELD-IPC-002");
    expect(() => decodeHeader(encoded)).toThrow("unknown kipc frame kind: 11");
  });

  test("wrong-channel Err fails closed as KELD-IPC-005", () => {
    const waiter = lifecycleReplyWaiter(7);
    const wrongChannelErr = {
      kind: FrameKind.Err,
      flags: 0,
      channel: ECHO_CHANNEL,
      corr: 7,
      len: 4,
    };
    expect(() => validateReceivedHeader(waiter, wrongChannelErr)).toThrow("KELD-IPC-005");
    expect(() => validateReceivedHeader(waiter, wrongChannelErr)).toThrow("wrong channel");
    const good = { kind: FrameKind.Err, flags: 0, channel: LIFECYCLE_CHANNEL, corr: 7, len: 4 };
    expect(validateReceivedHeader(waiter, good)).toEqual(good);
  });
});

describe("FrameReader chunk queue", () => {
  test("overlapping readFrame() rejects; the first waiter still gets the frame", async () => {
    const reader = new FrameReader();
    const first = reader.readFrame();
    await expect(reader.readFrame()).rejects.toThrow("KELD-IPC-005");
    reader.push(encodeFrame(FrameKind.Ping, 0, 1, new Uint8Array()));
    const frame = await first;
    expect(frame.header.kind).toBe(FrameKind.Ping);
    expect(frame.header.corr).toBe(1);
  });

  test(
    "first byte starts one frame deadline that later fragments cannot renew",
    async () => {
      const reader = new FrameReader();
      const pending = reader.readFrame();
      const header = encodeHeader({
        kind: FrameKind.Event,
        flags: 0,
        channel: LIFECYCLE_CHANNEL,
        corr: 0,
        len: 1,
      });
      let offset = 0;
      const started = performance.now();
      reader.push(header.subarray(offset, ++offset));
      const drip = setInterval(() => {
        if (offset < header.length - 1) reader.push(header.subarray(offset, ++offset));
      }, 400);
      try {
        await expect(pending).rejects.toThrow("KELD-IPC-006");
      } finally {
        clearInterval(drip);
      }
      const elapsed = performance.now() - started;
      expect(elapsed).toBeGreaterThanOrEqual(APP_LINK_IO_DEADLINE_MS);
      expect(elapsed).toBeLessThan(APP_LINK_IO_DEADLINE_MS + 2_000);
    },
    8_000,
  );

  test(
    "deadline census progress survives later arrivals, consumption, and compaction",
    async () => {
      const reader = new FrameReader();
      const batchSize = 1_024;
      for (let i = 0; i < batchSize; i += 1) {
        reader.push(encodeFrame(FrameKind.Ping, 17, i + 1, new Uint8Array()));
      }
      const originalSubarray = Uint8Array.prototype.subarray;
      let chunkVisits = 0;
      Object.defineProperty(Uint8Array.prototype, "subarray", {
        configurable: true,
        value(this: Uint8Array, begin?: number, end?: number): Uint8Array {
          chunkVisits += 1;
          return originalSubarray.call(this, begin, end);
        },
      });
      try {
        await new Promise((resolve) => setTimeout(resolve, APP_LINK_IO_DEADLINE_MS + 100));
        for (let i = 0; i < batchSize; i += 1) {
          reader.push(encodeFrame(FrameKind.Ping, 17, batchSize + i + 1, new Uint8Array()));
        }
        await new Promise((resolve) => setTimeout(resolve, APP_LINK_IO_DEADLINE_MS + 100));
      } finally {
        Reflect.deleteProperty(Uint8Array.prototype, "subarray");
      }
      expect(Uint8Array.prototype.subarray).toBe(originalSubarray);
      expect(chunkVisits).toBeLessThanOrEqual(batchSize * 2 + 4);
      const consumeCount = batchSize + 512;
      for (let i = 0; i < consumeCount; i += 1) {
        expect((await reader.readFrame()).header.corr).toBe(i + 1);
      }
      expect(reader.pendingChunkCount()).toBe(batchSize * 2 - consumeCount);
      reader.push(new Uint8Array([0x4b]));
      await new Promise((resolve) => setTimeout(resolve, APP_LINK_IO_DEADLINE_MS + 100));
      await expect(reader.readFrame()).rejects.toThrow("KELD-IPC-006");
    },
    19_000,
  );

  test(
    "arrival is captured before copy and late completion cannot beat a delayed timer",
    async () => {
      const reader = new FrameReader();
      const pending = reader.readFrame();
      const frame = encodeFrame(FrameKind.Ping, 17, 23, new Uint8Array());
      const started = performance.now();
      const constructorDescriptor = Object.getOwnPropertyDescriptor(globalThis, "Uint8Array");
      if (constructorDescriptor === undefined) throw new Error("global Uint8Array missing");
      const originalConstructor = globalThis.Uint8Array;
      let delayCopy = true;
      const delayedConstructor = new Proxy(originalConstructor, {
        construct(target, args, newTarget) {
          if (delayCopy && args[0] instanceof originalConstructor) {
            delayCopy = false;
            const copyStarted = performance.now();
            while (performance.now() - copyStarted < 25) {
              // Inject copy work after the transport observes callback entry.
            }
          }
          return Reflect.construct(target, args, newTarget);
        },
      });
      Object.defineProperty(globalThis, "Uint8Array", {
        ...constructorDescriptor,
        value: delayedConstructor,
      });
      try {
        reader.push(frame.subarray(0, 1));
      } finally {
        Object.defineProperty(globalThis, "Uint8Array", constructorDescriptor);
      }
      while (performance.now() - started < APP_LINK_IO_DEADLINE_MS + 10) {
        // Hold the JS turn so the already-due timer callback cannot run first.
      }
      reader.push(frame.subarray(1));
      await expect(pending).rejects.toThrow("KELD-IPC-006");
    },
    8_000,
  );

  test(
    "wall-clock rollback cannot extend a monotonic frame deadline",
    async () => {
      const originalDateNow = Date.now;
      const reader = new FrameReader();
      const pending = reader.readFrame();
      const started = performance.now();
      try {
        Date.now = () => -60_000;
        reader.push(new Uint8Array([0x4b]));
        await expect(pending).rejects.toThrow("KELD-IPC-006");
      } finally {
        Date.now = originalDateNow;
      }
      const elapsed = performance.now() - started;
      expect(elapsed).toBeGreaterThanOrEqual(APP_LINK_IO_DEADLINE_MS - 25);
      expect(elapsed).toBeLessThan(APP_LINK_IO_DEADLINE_MS + 1_500);
    },
    8_000,
  );

  test(
    "no-waiter late completion keeps deadline precedence over a later bad header",
    async () => {
      const reader = new FrameReader();
      const frame = encodeFrame(FrameKind.Ping, 17, 23, new Uint8Array());
      const malformed = encodeHeader({ kind: FrameKind.Ping, flags: 0, channel: 0, corr: 0, len: 0 });
      malformed[3] = 99;
      reader.push(frame.subarray(0, 1));
      const started = performance.now();
      while (performance.now() - started < APP_LINK_IO_DEADLINE_MS + 10) {
        // Keep the overdue timer queued until both later inputs are present.
      }
      reader.push(frame.subarray(1));
      reader.push(malformed);
      await new Promise((resolve) => setTimeout(resolve, 1));
      await expect(reader.readFrame()).rejects.toThrow("KELD-IPC-006");
    },
    8_000,
  );

  test(
    "active waiter checks a late malformed header against the deadline first",
    async () => {
      const reader = new FrameReader();
      const pending = reader.readFrame();
      const malformed = encodeHeader({ kind: FrameKind.Ping, flags: 0, channel: 0, corr: 0, len: 0 });
      malformed[3] = 99;
      reader.push(malformed.subarray(0, 1));
      const started = performance.now();
      while (performance.now() - started < APP_LINK_IO_DEADLINE_MS + 10) {
        // Hold the JS turn so the overdue callback cannot decide first.
      }
      reader.push(malformed.subarray(1));
      await expect(pending).rejects.toThrow("KELD-IPC-006");
    },
    8_000,
  );

  test(
    "queued partial frame keeps its first-byte clock before a waiter exists",
    async () => {
      const reader = new FrameReader();
      const complete = encodeFrame(FrameKind.Ping, 17, 23, new Uint8Array());
      const partial = encodeHeader({
        kind: FrameKind.Event,
        flags: 0,
        channel: LIFECYCLE_CHANNEL,
        corr: 0,
        len: 1,
      });
      reader.push(complete);
      await new Promise((resolve) => setTimeout(resolve, 300));
      reader.push(partial.subarray(0, 1));
      const partialArrived = performance.now();

      await new Promise((resolve) => setTimeout(resolve, 1_000));
      expect((await reader.readFrame()).header.kind).toBe(FrameKind.Ping);
      const waiterStarted = performance.now();
      await expect(reader.readFrame()).rejects.toThrow("KELD-IPC-006");
      const failedAt = performance.now();
      expect(failedAt - partialArrived).toBeGreaterThanOrEqual(APP_LINK_IO_DEADLINE_MS - 25);
      expect(failedAt - partialArrived).toBeLessThan(APP_LINK_IO_DEADLINE_MS + 1_500);
      expect(failedAt - waiterStarted).toBeLessThan(APP_LINK_IO_DEADLINE_MS - 500);
    },
    8_000,
  );

  test(
    "deadline census follows the logical head after in-chunk consumption",
    async () => {
      const reader = new FrameReader();
      const firstPending = reader.readFrame();
      const first = encodeFrame(FrameKind.Ping, 17, 1, new Uint8Array());
      const second = encodeFrame(FrameKind.Ping, 17, 2, new Uint8Array());
      const prefix = new Uint8Array(first.length + 1);
      prefix.set(first, 0);
      prefix[first.length] = second[0];
      reader.push(prefix);
      expect((await firstPending).header.corr).toBe(1);
      reader.push(second.subarray(1));

      const originalSubarray = Uint8Array.prototype.subarray;
      const starts: Array<number | undefined> = [];
      Object.defineProperty(Uint8Array.prototype, "subarray", {
        configurable: true,
        value(this: Uint8Array, begin?: number, end?: number): Uint8Array {
          starts.push(begin);
          return originalSubarray.call(this, begin, end);
        },
      });
      try {
        await new Promise((resolve) => setTimeout(resolve, APP_LINK_IO_DEADLINE_MS + 100));
      } finally {
        Reflect.deleteProperty(Uint8Array.prototype, "subarray");
      }
      expect(starts[0]).toBe(first.length);
      expect((await reader.readFrame()).header.corr).toBe(2);
    },
    8_000,
  );

  test("Ready before Echo Reply parks; later event receive drains FIFO", async () => {
    const ready = encodeFrame(FrameKind.Event, LIFECYCLE_CHANNEL, 0, new Uint8Array([0x00]));
    const reply = encodeFrame(FrameKind.Reply, ECHO_CHANNEL, 1, new Uint8Array([0x01]));
    const reader = new FrameReader();
    const directed = new DirectedReader(reader);
    const waiting = directed.receive(
      echoReplyWaiter(1),
      RECEIVE_POLICIES.lifecycleEventReceiver,
    );
    reader.push(ready);
    reader.push(reply);
    const got = await waiting;
    expect(got.header.kind).toBe(FrameKind.Reply);
    expect(got.header.corr).toBe(1);
    expect(directed.parkedCount()).toBe(1);
    const parked = await directed.receive(RECEIVE_POLICIES.lifecycleEventReceiver);
    expect(parked.header.kind).toBe(FrameKind.Event);
    expect(parked.payload).toEqual(new Uint8Array([0x00]));
    expect(directed.parkedCount()).toBe(0);
  });

  test("without park, Ready before Echo Reply fails closed", async () => {
    const ready = encodeFrame(FrameKind.Event, LIFECYCLE_CHANNEL, 0, new Uint8Array([0x00]));
    const reader = new FrameReader();
    const directed = new DirectedReader(reader);
    const waiting = directed.receive(echoReplyWaiter(1));
    reader.push(ready);
    await expect(waiting).rejects.toThrow("KELD-IPC-005");
  });

  test("park overflow is KELD-IPC-005 and does not merge frames", async () => {
    const reader = new FrameReader();
    const directed = new DirectedReader(reader);
    const waiting = directed.receive(
      echoReplyWaiter(1),
      RECEIVE_POLICIES.lifecycleEventReceiver,
    );
    const event = encodeFrame(FrameKind.Event, LIFECYCLE_CHANNEL, 0, new Uint8Array([0x00]));
    for (let i = 0; i < MAX_PARKED_FRAMES; i += 1) {
      reader.push(event);
    }
    reader.push(event);
    await expect(waiting).rejects.toThrow("KELD-IPC-005");
    await expect(waiting).rejects.toThrow("parked-frame queue is full");
    expect(directed.parkedCount()).toBe(MAX_PARKED_FRAMES);
  });

  test("fragmented pushes keep a chunk queue, not a merged prefix", async () => {
    const payload = new Uint8Array(4096).fill(0x5a);
    const frame = encodeFrame(FrameKind.Reply, ECHO_CHANNEL, 1, payload);
    const reader = new FrameReader();
    const pending = reader.readFrame();
    const chunkSize = 1;
    for (let i = 0; i < frame.length; i += chunkSize) {
      reader.push(frame.subarray(i, i + chunkSize));
    }
    // After the last byte the frame resolves and the queue is consumed.
    const decoded = await pending;
    expect(decoded.payload.length).toBe(4096);
    expect(reader.bufferedBytes()).toBe(0);
    expect(reader.pendingChunkCount()).toBe(0);
  });

  test("an incomplete fragmented frame reports N chunks, not one concatenated buffer", () => {
    const reader = new FrameReader();
    const n = 64;
    for (let i = 0; i < n; i += 1) {
      reader.push(new Uint8Array([i & 0xff]));
    }
    expect(reader.bufferedBytes()).toBe(n);
    expect(reader.pendingChunkCount()).toBe(n);
  });

  test("an incomplete max-size frame keeps a chunk queue, not a 16 MiB prefix", async () => {
    const header = encodeHeader({
      kind: FrameKind.Reply,
      flags: 0,
      channel: ECHO_CHANNEL,
      corr: 1,
      len: MAX_FRAME_LEN,
    });
    const reader = new FrameReader();
    const pending = reader.readFrame();
    for (let i = 0; i < header.length; i += 1) {
      reader.push(header.subarray(i, i + 1));
    }
    const extra = 128;
    for (let i = 0; i < extra; i += 1) {
      reader.push(new Uint8Array([i & 0xff]));
    }
    expect(reader.bufferedBytes()).toBe(HEADER_LEN + extra);
    expect(reader.pendingChunkCount()).toBe(HEADER_LEN + extra);
    expect(reader.bufferedBytes()).toBeLessThan(MAX_FRAME_LEN);
    reader.fail(new Error("test teardown: incomplete max-size frame"));
    await expect(pending).rejects.toThrow("test teardown");
  });

  test("retains an owned copy when a borrowed socket chunk is still incomplete", async () => {
    const reader = new FrameReader();
    const pending = reader.readFrame();
    reader.push(encodeHeader(FrameKind.Reply, 0, ECHO_CHANNEL, 1, 2));
    const borrowed = new Uint8Array([0x41]);
    reader.push(borrowed);
    borrowed[0] = 0x5a;
    reader.push(new Uint8Array([0x42]));
    expect((await pending).payload).toEqual(new Uint8Array([0x41, 0x42]));
  });

  test("consumes fragmented chunks without calling Array.shift", async () => {
    const shift = Object.getOwnPropertyDescriptor(Array.prototype, "shift");
    if (shift === undefined) throw new Error("Array.prototype.shift descriptor is missing");
    Object.defineProperty(Array.prototype, "shift", {
      ...shift,
      value(): never {
        throw new Error("Array.shift reindexes the frame queue");
      },
    });
    try {
      const reader = new FrameReader();
      const pending = reader.readFrame();
      reader.push(encodeHeader(FrameKind.Reply, 0, ECHO_CHANNEL, 1, 2));
      reader.push(new Uint8Array([0x41]));
      reader.push(new Uint8Array([0x42]));
      expect((await pending).payload).toEqual(new Uint8Array([0x41, 0x42]));
    } finally {
      Object.defineProperty(Array.prototype, "shift", shift);
    }
  });

  test("fails closed instead of retaining more than one maximum frame envelope", async () => {
    const reader = new FrameReader();
    reader.push(encodeHeader(FrameKind.Reply, 0, ECHO_CHANNEL, 1, 0));
    reader.push(new Uint8Array(MAX_FRAME_LEN + 1));
    expect(reader.bufferedBytes()).toBe(0);
    await expect(reader.readFrame()).rejects.toThrow("KELD-IPC-004");
  });

  test("fails closed when tiny fragments exceed the pending-chunk cap", async () => {
    const reader = new FrameReader();
    for (let i = 0; i <= MAX_PENDING_CHUNKS; i += 1) {
      reader.push(new Uint8Array([i & 0xff]));
    }
    expect(reader.bufferedBytes()).toBe(0);
    expect(reader.pendingChunkCount()).toBe(0);
    await expect(reader.readFrame()).rejects.toThrow("KELD-IPC-004");
  });
});

describe("u32 postcard varint bounds", () => {
  test("rejects overflow in the fifth byte and any sixth byte", () => {
    expect(() => decodeVarint(new Uint8Array([0x80, 0x80, 0x80, 0x80, 0x10]), 0)).toThrow(
      "KELD-IPC-003",
    );
    expect(() =>
      decodeVarint(new Uint8Array([0x80, 0x80, 0x80, 0x80, 0x80, 0x00]), 0),
    ).toThrow("KELD-IPC-003");
    expect(decodeVarint(new Uint8Array([0xff, 0xff, 0xff, 0xff, 0x0f]), 0)).toEqual([
      0xffff_ffff,
      5,
    ]);
  });

  test("rejects non-integer and out-of-range offsets", () => {
    const bytes = new Uint8Array([0]);
    for (const offset of [-1, 0.5, 2]) {
      expect(() => decodeVarint(bytes, offset)).toThrow("KELD-IPC-003");
    }
  });

  test("postcard strings reuse the bounded decoder", () => {
    expect(() =>
      decodePostcardStringAt(new Uint8Array([0x80, 0x80, 0x80, 0x80, 0x10]), 0),
    ).toThrow("KELD-IPC-003");
  });
});

describe("DrainSignal / WriteQueue", () => {
  test("one fire wakes every waiter", async () => {
    const drain = new DrainSignal();
    const first = drain.wait();
    const second = drain.wait();
    drain.fire();
    await Promise.all([first, second]);
  });

  test("wall-clock jumps do not expire a progressing write", async () => {
    const originalDateNow = Date.now;
    let wallOffsetMs = 0;
    let writes = 0;
    let allowCompletion = false;
    let pending: Promise<void> | undefined;
    let observeFirstWrite!: () => void;
    let observeSecondWrite!: () => void;
    const firstWrite = new Promise<void>((resolve) => {
      observeFirstWrite = resolve;
    });
    const secondWrite = new Promise<void>((resolve) => {
      observeSecondWrite = resolve;
    });
    const beforeWatchdog = async <T>(promise: Promise<T>, stage: string): Promise<T> => {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const watchdog = new Promise<T>((_resolve, reject) => {
        timer = setTimeout(() => reject(new Error(`test watchdog expired: ${stage}`)), 500);
      });
      try {
        return await Promise.race([promise, watchdog]);
      } finally {
        if (timer !== undefined) clearTimeout(timer);
      }
    };
    const drain = new DrainSignal();
    const queue = new WriteQueue(
      {
        write(data: Uint8Array): number {
          writes += 1;
          if (writes === 1) observeFirstWrite();
          if (writes === 2) observeSecondWrite();
          return allowCompletion ? data.length : 0;
        },
        end(): void {},
      },
      drain,
    );
    Date.now = () => originalDateNow() + wallOffsetMs;

    try {
      const writing = queue.writeFrame(FrameKind.Ping, 0, 0, 1, new Uint8Array());
      pending = writing;
      await beforeWatchdog(firstWrite, "first write");
      expect(writes).toBe(1);

      wallOffsetMs = -60_000;
      drain.fire();
      await beforeWatchdog(secondWrite, "second write after rollback");
      expect(writes).toBe(2);

      wallOffsetMs = 60_000;
      allowCompletion = true;
      drain.fire();
      await expect(
        beforeWatchdog(writing, "write completion after forward jump"),
      ).resolves.toBeUndefined();
      expect(writes).toBe(3);
    } finally {
      Date.now = originalDateNow;
      allowCompletion = true;
      drain.fire();
      await pending?.catch(() => undefined);
    }
  });

  test("a failed mid-frame write poisons later writes", async () => {
    const out: number[] = [];
    let writes = 0;
    const queue = new WriteQueue(
      {
        write(data: Uint8Array): number {
          writes += 1;
          if (writes === 1) {
            for (let i = 0; i < 8; i += 1) out.push(data[i]!);
            return 8;
          }
          if (writes === 2) return -1;
          for (let i = 0; i < data.length; i += 1) out.push(data[i]!);
          return data.length;
        },
        end(): void {},
      },
      new DrainSignal(),
    );
    await expect(queue.writeFrame(FrameKind.Ping, 0, 0, 1, new Uint8Array())).rejects.toThrow(
      "KELD-IPC-001",
    );
    await expect(queue.writeFrame(FrameKind.Ping, 0, 0, 2, new Uint8Array())).rejects.toThrow(
      "KELD-IPC-001",
    );
    expect(out.length).toBe(8);
  });

  test("an oversized payload fails before socket effects without poisoning later writes", async () => {
    let writes = 0;
    const queue = new WriteQueue(
      {
        write(data: Uint8Array): number {
          writes += 1;
          return data.length;
        },
        end(): void {},
      },
      new DrainSignal(),
    );
    await expect(
      queue.writeFrame(
        FrameKind.Call,
        0,
        ECHO_CHANNEL,
        1,
        new Uint8Array(MAX_FRAME_LEN + 1),
      ),
    ).rejects.toThrow("KELD-IPC-004");
    expect(writes).toBe(0);
    await expect(
      queue.writeFrame(FrameKind.Ping, 0, 0, 2, new Uint8Array()),
    ).resolves.toBeUndefined();
    expect(writes).toBe(1);
  });
});

describe("deadline leftover I/O", () => {
  test("withIoDeadline leaves FrameReader.#pending until fail() closes it", async () => {
    const reader = new FrameReader();
    await expect(withIoDeadline(reader.readFrame(), 20)).rejects.toThrow("KELD-IPC-006");
    await expect(reader.readFrame()).rejects.toThrow("KELD-IPC-005");
    reader.fail(kipcError("KELD-IPC-001", "session is closed"));
    await expect(reader.readFrame()).rejects.toThrow("KELD-IPC-001");
  });

  test("fail() then drain.fire() rejects the parked read and wakes writers", async () => {
    const reader = new FrameReader();
    const drain = new DrainSignal();
    const pending = reader.readFrame();
    const waiting = drain.wait();
    reader.fail(kipcError("KELD-IPC-001", "connection closed by peer"));
    drain.fire();
    await expect(pending).rejects.toThrow("KELD-IPC-001");
    await waiting;
  });

  test("connect failure handlers reject the reader before waking writers", async () => {
    const originalConnect = Bun.connect;
    let handlers: Record<string, (...args: never[]) => void> = {};
    Bun.connect = (async (opts: { socket?: Record<string, (...args: never[]) => void> }) => {
      handlers = opts.socket ?? {};
      return { write: () => 1, end: () => undefined };
    }) as unknown as typeof Bun.connect;

    try {
      for (const name of ["error", "close", "connectError"] as const) {
        const reader = new FrameReader();
        const drain = new DrainSignal();
        const endpoint = process.platform === "win32" ? "9000" : "/tmp/keld-kipc-test.sock";
        await connectKipcSocket(endpoint, reader, drain);

        const outcomes: string[] = [];
        const read = reader.readFrame().then(
          () => outcomes.push("read-resolved"),
          () => outcomes.push("read-rejected"),
        );
        const waiting = drain.wait().then(() => outcomes.push("drain-resolved"));
        const handler = handlers[name];
        expect(handler).toBeDefined();
        if (name === "close") {
          handler?.();
        } else {
          handler?.(undefined as never, new Error(`${name} test`) as never);
        }
        await Promise.all([read, waiting]);
        expect(outcomes).toEqual(["read-rejected", "drain-resolved"]);
      }
    } finally {
      Bun.connect = originalConnect;
    }
  });
});

describe("one source / no second copy", () => {
  test("repository walks skip only owned reference roots and directory symlinks", () => {
    const root = mkdtempSync(join(tmpdir(), "keld-kipc-walk-"));
    try {
      const source = join(root, "source");
      const target = join(root, "target");
      mkdirSync(source);
      mkdirSync(target);
      writeFileSync(join(target, "linked.ts"), "export {};\n");
      symlinkSync(target, join(source, "linked"), process.platform === "win32" ? "junction" : "dir");
      for (const skipped of ["node_modules", "target"]) {
        mkdirSync(join(source, skipped));
        writeFileSync(join(source, skipped, `${skipped}.ts`), "export {};\n");
      }
      writeFileSync(join(source, "owned.ts"), "export {};\n");
      const files: string[] = [];
      walkFiles(source, ".ts", files);
      expect(files.map((file) => relative(source, file).replaceAll("\\", "/"))).toEqual([
        "owned.ts",
      ]);
      expect(skipDirectory(join(REPO_ROOT, "competitors"), "competitors")).toBe(true);
      expect(skipDirectory(join(REPO_ROOT, "docs", "research"), "research")).toBe(true);
      expect(
        skipDirectory(join(REPO_ROOT, "packages", "@keld", "example", "research"), "research"),
      ).toBe(false);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("MAGIC_BYTES and class FrameReader exist only in the canonical transport", () => {
    const files: string[] = [];
    walkFiles(join(REPO_ROOT, "packages"), ".ts", files);
    walkFiles(join(REPO_ROOT, "crates/keld-cli/templates"), ".ts", files);
    const magicOwners: string[] = [];
    const readerOwners: string[] = [];
    for (const file of files) {
      if (file.endsWith(".test.ts")) continue;
      const text = readFileSync(file, "utf8");
      if (text.includes("const MAGIC_BYTES = new Uint8Array([0x4b, 0x49])")) {
        magicOwners.push(relative(REPO_ROOT, file).replaceAll("\\", "/"));
      }
      if (text.includes("export class FrameReader")) {
        readerOwners.push(relative(REPO_ROOT, file).replaceAll("\\", "/"));
      }
    }
    expect(magicOwners).toEqual(["packages/@keld/kipc/src/transport.ts"]);
    expect(readerOwners).toEqual(["packages/@keld/kipc/src/transport.ts"]);
  });

  test("DirectedReader exists only in the canonical transport", () => {
    const files: string[] = [];
    walkFiles(join(REPO_ROOT, "packages"), ".ts", files);
    walkFiles(join(REPO_ROOT, "crates/keld-cli/templates"), ".ts", files);
    const owners: string[] = [];
    for (const file of files) {
      if (file.endsWith(".test.ts")) continue;
      const text = readFileSync(file, "utf8");
      if (text.includes("export class DirectedReader")) {
        owners.push(relative(REPO_ROOT, file).replaceAll("\\", "/"));
      }
    }
    expect(owners).toEqual(["packages/@keld/kipc/src/transport.ts"]);
  });

  test("hello scaffold shim is a re-export, not a second implementation", () => {
    const shim = readFileSync(
      join(REPO_ROOT, "crates/keld-cli/templates/hello/src/kipc-transport.ts"),
      "utf8",
    );
    expect(shim).toContain('export * from "../../../../../packages/@keld/kipc/src/transport.ts"');
    expect(shim).not.toContain("export class FrameReader");
    expect(shim).not.toContain("const MAGIC_BYTES");
  });

  test("canonical corpus file is the one TSV owner; consumers do not copy rows", () => {
    const tsvFiles: string[] = [];
    walkFiles(REPO_ROOT, ".tsv", tsvFiles);
    const corpus = tsvFiles
      .map((file) => relative(REPO_ROOT, file).replaceAll("\\", "/"))
      .filter((file) => file.endsWith("receiver-semantics-v0.tsv"));
    expect(corpus).toEqual(["crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv"]);
    const bytes = readFileSync(CORPUS_PATH);
    const digest = new Bun.CryptoHasher("sha256").update(bytes).digest("hex");
    expect(digest).toBe(CORPUS_SHA256);
  });

  test("public TypeScript sources do not use any", () => {
    const files: string[] = [];
    walkFiles(join(REPO_ROOT, "packages/@keld/kipc/src"), ".ts", files);
    walkFiles(join(REPO_ROOT, "packages/@keld/electron/src"), ".ts", files);
    const hits: string[] = [];
    for (const file of files) {
      if (file.endsWith(".test.ts")) continue;
      const text = readFileSync(file, "utf8");
      if (/(?::|\bas\b|<)\s*any\b/.test(text) || /\bany\s*[\[<|]/.test(text)) {
        hits.push(relative(REPO_ROOT, file).replaceAll("\\", "/"));
      }
    }
    expect(hits).toEqual([]);
  });
});

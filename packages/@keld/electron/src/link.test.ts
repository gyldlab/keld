/**
 * Contract tests for the KEL-72 lifecycle kipc client.
 *
 * Oracles: keld_ipc error codes, APP_LINK_IO_DEADLINE = 5s, Win32 endpoint
 * as `u16` (not parseInt), and concatenated frame bytes under backpressure.
 */
import { afterAll, describe, expect, test } from "bun:test";
import { mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import {
  APP_LINK_IO_DEADLINE_MS,
  DrainSignal,
  FrameKind,
  FrameReader,
  LIFECYCLE_CHANNEL,
  LifecycleLink,
  WriteQueue,
  encodeHeader,
  parseWin32Port,
  withIoDeadline,
} from "./link";

const TOKEN_HEX = "72".repeat(32);
const TOKEN = new Uint8Array(32).fill(0x72);

function encodeFrame(kind: number, channel: number, corr: number, payload: Uint8Array): Uint8Array {
  const header = encodeHeader(kind, 0, channel, corr, payload.length);
  const frame = new Uint8Array(header.length + payload.length);
  frame.set(header, 0);
  frame.set(payload, header.length);
  return frame;
}

function encodePostcardString(text: string): Uint8Array {
  const utf8 = new TextEncoder().encode(text);
  if (utf8.length > 127) {
    throw new Error("test helper only encodes short postcard strings");
  }
  const out = new Uint8Array(1 + utf8.length);
  out[0] = utf8.length;
  out.set(utf8, 1);
  return out;
}

function writeAll(socket: { write(data: Uint8Array | string): number }, data: Uint8Array): void {
  let offset = 0;
  while (offset < data.length) {
    const n = socket.write(data.subarray(offset));
    if (n <= 0) {
      throw new Error("peer write returned no bytes");
    }
    offset += n;
  }
}

async function rejectWithin(ms: number, promise: Promise<unknown>, why: string): Promise<void> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const kill = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(why)), ms);
  });
  try {
    await Promise.race([promise, kill]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

describe("parseWin32Port", () => {
  test("accepts a decimal port in 1–65535", () => {
    expect(parseWin32Port("1")).toBe(1);
    expect(parseWin32Port("9000")).toBe(9000);
    expect(parseWin32Port("65535")).toBe(65535);
  });

  test("rejects host:port that parseInt would treat as 127", () => {
    // Independent oracle: Number.parseInt stops at the first non-digit, so
    // this string is port 127. The host mints a bare u16; connecting to
    // 127.0.0.1:127 would be the wrong socket.
    expect(Number.parseInt("127.0.0.1:9000", 10)).toBe(127);
    expect(() => parseWin32Port("127.0.0.1:9000")).toThrow("KELD-IPC-007");
  });

  test("rejects 0, 65536, and trailing junk", () => {
    for (const bad of ["0", "65536", "9000abc", "", " 9000"]) {
      expect(() => parseWin32Port(bad)).toThrow("KELD-IPC-007");
    }
  });
});

describe("DrainSignal", () => {
  test("one fire wakes every waiter (single-slot would hang the first)", async () => {
    const drain = new DrainSignal();
    const first = drain.wait();
    const second = drain.wait();
    drain.fire();
    await rejectWithin(
      200,
      Promise.all([first, second]),
      "DrainSignal dropped a waiter — a single-slot signal overwrites the first resolve",
    );
  });
});

describe("WriteQueue", () => {
  test("serializes concurrent frames across drain waits", async () => {
    const drain = new DrainSignal();
    const out: number[] = [];
    // One 8-byte chunk per writeOneFrame turn, then 0 so the caller awaits
    // drain. A second concurrent writer can then take its own chunk — that is
    // the interleave (A[0:8]+B[0:8]+…) unless WriteQueue holds B.
    let chunkTaken = false;
    const socket = {
      write(data: Uint8Array): number {
        if (data.length === 0) return 0;
        if (chunkTaken) {
          chunkTaken = false;
          return 0;
        }
        const n = Math.min(data.length, 8);
        for (let i = 0; i < n; i += 1) out.push(data[i]!);
        chunkTaken = true;
        return n;
      },
      end(): void {},
    };
    const queue = new WriteQueue(socket, drain);
    const ping = encodeFrame(FrameKind.Ping, 0, 1, new Uint8Array());
    const quit = encodeFrame(FrameKind.Call, LIFECYCLE_CHANNEL, 2, new Uint8Array([0x00]));
    const done = Promise.all([
      queue.writeFrame(FrameKind.Ping, 0, 0, 1, new Uint8Array()),
      queue.writeFrame(FrameKind.Call, 0, LIFECYCLE_CHANNEL, 2, new Uint8Array([0x00])),
    ]);
    const kill = Date.now() + 2_000;
    while (out.length < ping.length + quit.length) {
      if (Date.now() > kill) {
        throw new Error(
          `WriteQueue hung or interleaved after ${out.length} bytes (need ${ping.length + quit.length})`,
        );
      }
      drain.fire();
      await Promise.resolve();
    }
    await done;
    expect(out).toEqual([...ping, ...quit]);
  });
});

describe("withIoDeadline", () => {
  test("is 5 seconds, matching keld_ipc::APP_LINK_IO_DEADLINE", () => {
    expect(APP_LINK_IO_DEADLINE_MS).toBe(5_000);
  });

  test("rejects KELD-IPC-006 when the promise never settles", async () => {
    const start = Date.now();
    await expect(withIoDeadline(new Promise(() => {}), 50)).rejects.toThrow("KELD-IPC-006");
    const elapsed = Date.now() - start;
    expect(elapsed).toBeLessThan(1_000);
  }, 2_000);
});

describe.skipIf(process.platform === "win32")("LifecycleLink over a Unix peer", () => {
  const root = join(import.meta.dir, "..", ".test-run");

  afterAll(() => {
    rmSync(root, { recursive: true, force: true });
  });

  function bindPeer(): {
    link: string;
    listener: ReturnType<typeof Bun.listen>;
    reader: FrameReader;
    opened: Promise<Bun.Socket>;
  } {
    mkdirSync(root, { recursive: true, mode: 0o700 });
    const dir = join(root, `${process.pid}-${Date.now().toString(36)}-${Math.random().toString(16).slice(2)}`);
    mkdirSync(dir, { mode: 0o700 });
    const path = join(dir, "e.sock");
    const reader = new FrameReader();
    let resolveOpen: (socket: Bun.Socket) => void = () => undefined;
    const opened = new Promise<Bun.Socket>((resolve) => {
      resolveOpen = resolve;
    });
    const listener = Bun.listen({
      unix: path,
      socket: {
        binaryType: "uint8array" as const,
        open(socket) {
          resolveOpen(socket);
        },
        data(_socket, data) {
          reader.push(data);
        },
        error() {},
        close() {},
      },
    });
    return { link: `${path}#${TOKEN_HEX}`, listener, reader, opened };
  }

  const handlers = { onReady(): void {}, onLastWindowClosed(): void {} };

  test(
    "HELLO readFrame against a live silent peer is KELD-IPC-006",
    async () => {
      const peer = bindPeer();
      const start = Date.now();
      try {
        const err = await LifecycleLink.connect(peer.link, handlers).then(
          () => null,
          (e: unknown) => e as Error,
        );
        expect(err?.message).toContain("KELD-IPC-006");
        expect(err?.message).toContain("deadline");
        expect(err?.message).not.toContain("KELD-IPC-001");
        const elapsed = Date.now() - start;
        expect(elapsed).toBeGreaterThanOrEqual(4_000);
        expect(elapsed).toBeLessThan(12_000);
      } finally {
        peer.listener.stop(true);
      }
    },
    15_000,
  );

  test("host Err on Quit rejects and does not hang the waiter", async () => {
    const peer = bindPeer();
    try {
      const connectP = LifecycleLink.connect(peer.link, handlers);
      const socket = await peer.opened;
      const hello = await peer.reader.readFrame();
      expect(hello.header.kind).toBe(FrameKind.Hello);
      writeAll(socket, encodeFrame(FrameKind.Hello, 0, 0, TOKEN));
      const session = await connectP;

      const quitP = session.quit();
      const call = await peer.reader.readFrame();
      expect(call.header.kind).toBe(FrameKind.Call);
      expect(call.header.channel).toBe(LIFECYCLE_CHANNEL);
      writeAll(
        socket,
        encodeFrame(
          FrameKind.Err,
          LIFECYCLE_CHANNEL,
          call.header.corr,
          encodePostcardString("KELD-IPC-005: quit refused"),
        ),
      );
      await expect(quitP).rejects.toThrow("KELD-IPC-005");
    } finally {
      peer.listener.stop(true);
    }
  });

  test(
    "Quit Reply wait against a live silent peer is KELD-IPC-006",
    async () => {
      const peer = bindPeer();
      try {
        const connectP = LifecycleLink.connect(peer.link, handlers);
        const socket = await peer.opened;
        await peer.reader.readFrame();
        writeAll(socket, encodeFrame(FrameKind.Hello, 0, 0, TOKEN));
        const session = await connectP;
        const start = Date.now();
        const err = await session.quit().then(
          () => null,
          (e: unknown) => e as Error,
        );
        expect(err?.message).toContain("KELD-IPC-006");
        expect(err?.message).not.toContain("KELD-IPC-001");
        const elapsed = Date.now() - start;
        expect(elapsed).toBeGreaterThanOrEqual(4_000);
        expect(elapsed).toBeLessThan(12_000);
      } finally {
        peer.listener.stop(true);
      }
    },
    15_000,
  );
});

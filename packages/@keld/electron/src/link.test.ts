/**
 * Contract tests for the KEL-72 lifecycle kipc client.
 *
 * Oracles: keld_ipc error codes, APP_LINK_IO_DEADLINE = 5s, Win32 endpoint
 * as `u16` (not parseInt), and concatenated frame bytes under backpressure.
 */
import { afterAll, describe, expect, test } from "bun:test";
import { chmodSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
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

/** Independent oracle: kipc magic is `b"KI"` (`keld_ipc::MAGIC`), not WriteQueue layout. */
function countKiMagic(bytes: number[]): number {
  let n = 0;
  for (let i = 0; i + 1 < bytes.length; i += 1) {
    if (bytes[i] === 0x4b && bytes[i + 1] === 0x49) n += 1;
  }
  return n;
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

async function rejectWithin<T>(ms: number, promise: Promise<T>, why: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const kill = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(why)), ms);
  });
  try {
    return await Promise.race([promise, kill]);
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

  test(
    "never-draining write is KELD-IPC-006 (drain.wait inside writeOneFrame)",
    async () => {
      // Independent of LifecycleLink.connect / outer writeFrame wraps.
      // socket.write returning 0 and no drain.fire() must surface 006;
      // omitting the writeOneFrame wall-clock hangs this test.
      const drain = new DrainSignal();
      const queue = new WriteQueue(
        {
          write(): number {
            return 0;
          },
          end(): void {},
        },
        drain,
      );
      const start = Date.now();
      await expect(
        queue.writeFrame(FrameKind.Ping, 0, 0, 1, new Uint8Array()),
      ).rejects.toThrow("KELD-IPC-006");
      const elapsed = Date.now() - start;
      expect(elapsed).toBeGreaterThanOrEqual(4_000);
      expect(elapsed).toBeLessThan(12_000);
    },
    15_000,
  );

  test(
    "never-finishing partial writes are KELD-IPC-006 within one frame deadline",
    async () => {
      // Independent of LifecycleLink.connect / outer writeFrame wraps.
      // write() returns 0 (no offset progress) but drain keeps firing.
      // Restarting withIoDeadline per drain.wait() never hits KELD-IPC-006.
      // One wall-clock for the whole writeOneFrame must.
      const drain = new DrainSignal();
      let parked = 0;
      const queue = new WriteQueue(
        {
          write(): number {
            parked += 1;
            setTimeout(() => drain.fire(), 10);
            return 0;
          },
          end(): void {},
        },
        drain,
      );
      const start = Date.now();
      await expect(
        queue.writeFrame(FrameKind.Ping, 0, 0, 1, new Uint8Array()),
      ).rejects.toThrow("KELD-IPC-006");
      const elapsed = Date.now() - start;
      expect(parked).toBeGreaterThan(1);
      expect(elapsed).toBeGreaterThanOrEqual(4_000);
      expect(elapsed).toBeLessThan(12_000);
    },
    15_000,
  );

  test("a failed mid-frame write poisons the queue so a later frame cannot follow a truncated one", async () => {
    // Independent oracle: a second `KI` after 8 truncated header bytes is a
    // new frame on a torn stream (peer sees KELD-IPC-002). Swallowing
    // writeOneFrame in `#chain.then(..., () => undefined)` used to allow that.
    const out: number[] = [];
    let writes = 0;
    const queue = new WriteQueue(
      {
        write(data: Uint8Array): number {
          writes += 1;
          if (writes === 1) {
            const n = 8;
            for (let i = 0; i < n; i += 1) out.push(data[i]!);
            return n;
          }
          if (writes === 2) {
            return -1;
          }
          for (let i = 0; i < data.length; i += 1) out.push(data[i]!);
          return data.length;
        },
        end(): void {},
      },
      new DrainSignal(),
    );
    await expect(
      queue.writeFrame(FrameKind.Ping, 0, 0, 1, new Uint8Array()),
    ).rejects.toThrow("KELD-IPC-001");
    await expect(
      queue.writeFrame(FrameKind.Ping, 0, 0, 2, new Uint8Array()),
    ).rejects.toThrow("KELD-IPC-001");
    expect(countKiMagic(out)).toBe(1);
    expect(out.length).toBe(8);
  });
});

describe("FrameReader", () => {
  test("overlapping readFrame() rejects; the first waiter still gets the frame", async () => {
    // Single-slot `#pending` overwrite left the first promise unsettled.
    // keld_ipc::IpcError::Protocol is KELD-IPC-005 (unexpected session state);
    // KELD-IPC-003 is postcard codec and is not this contract.
    const reader = new FrameReader();
    const first = reader.readFrame();
    const second = reader.readFrame();
    await expect(
      rejectWithin(
        200,
        second,
        "overlapping readFrame() hung — concurrent call overwrote #pending",
      ),
    ).rejects.toThrow("KELD-IPC-005");
    reader.push(encodeFrame(FrameKind.Ping, 0, 1, new Uint8Array()));
    const frame = await rejectWithin(
      200,
      first,
      "first readFrame() hung — overlapping call overwrote #pending",
    );
    expect(frame.header.kind).toBe(FrameKind.Ping);
    expect(frame.header.corr).toBe(1);
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

describe("LifecycleLink write deadlines", () => {
  const originalConnect = Bun.connect;

  function installConnect(
    socket: { write(data: Uint8Array): number; end(): void },
  ): { handlers: Record<string, (...args: never[]) => void> } {
    const captured: { handlers: Record<string, (...args: never[]) => void> } = {
      handlers: {},
    };
    Bun.connect = (async (opts: { socket?: Record<string, (...args: never[]) => void> }) => {
      captured.handlers = opts.socket ?? {};
      return socket;
    }) as typeof Bun.connect;
    return captured;
  }

  function mockLink(): string {
    return process.platform === "win32" ? `9000#${TOKEN_HEX}` : `/tmp/keld-kel72-deadline.sock#${TOKEN_HEX}`;
  }

  afterAll(() => {
    Bun.connect = originalConnect;
  });

  test(
    "HELLO write against a never-draining send buffer is KELD-IPC-006",
    async () => {
      // Independent oracle: socket.write returning 0 parks on DrainSignal.wait
      // with no drain event. withIoDeadline around HELLO write must surface
      // KELD-IPC-006; omitting it hangs until the test runner kills the file.
      installConnect({
        write(): number {
          return 0;
        },
        end(): void {},
      });
      try {
        const start = Date.now();
        const err = await LifecycleLink.connect(mockLink(), {
          onReady(): void {},
          onLastWindowClosed(): void {},
          onLinkDead(): void {},
        }).then(
          () => null,
          (e: unknown) => e as Error,
        );
        expect(err?.message).toContain("KELD-IPC-006");
        expect(err?.message).toContain("deadline");
        const elapsed = Date.now() - start;
        expect(elapsed).toBeGreaterThanOrEqual(4_000);
        expect(elapsed).toBeLessThan(12_000);
      } finally {
        Bun.connect = originalConnect;
      }
    },
    15_000,
  );

  test(
    "Ping reply write against a never-draining send buffer is KELD-IPC-006",
    async () => {
      let writes = 0;
      const captured = installConnect({
        write(data: Uint8Array): number {
          writes += 1;
          if (writes === 1) return data.length;
          return 0;
        },
        end(): void {},
      });
      try {
        let resolveDead: (err: Error) => void = () => undefined;
        const died = new Promise<Error>((resolve) => {
          resolveDead = resolve;
        });
        const connectP = LifecycleLink.connect(mockLink(), {
          onReady(): void {},
          onLastWindowClosed(): void {},
          onLinkDead(err: Error): void {
            resolveDead(err);
          },
        });
        const helloKill = Date.now() + 2_000;
        while (writes < 1) {
          if (Date.now() > helloKill) {
            throw new Error("HELLO write never reached the mock socket");
          }
          await Promise.resolve();
        }
        const data = captured.handlers.data as (socket: unknown, chunk: Uint8Array) => void;
        data(undefined, encodeFrame(FrameKind.Hello, 0, 0, TOKEN));
        await connectP;
        const start = Date.now();
        data(undefined, encodeFrame(FrameKind.Ping, 0, 1, new Uint8Array()));
        const dead = await rejectWithin(
          12_000,
          died,
          "Ping reply write hung without withIoDeadline — onLinkDead never ran",
        );
        expect(dead.message).toContain("KELD-IPC-006");
        expect(Date.now() - start).toBeGreaterThanOrEqual(4_000);
        expect(Date.now() - start).toBeLessThan(12_000);
      } finally {
        Bun.connect = originalConnect;
      }
    },
    15_000,
  );
});

describe.skipIf(process.platform === "win32")("LifecycleLink over a Unix peer", () => {
  // `sockaddr_un.sun_path` is 104 bytes on macOS (108 on Linux). A path under
  // the package tree plus `.test-run/<pid>-<ts>-<rand>/e.sock` overflows.
  // Short unique 0o700 dir under tmpdir, same contract as keld-cli bind_unix_echo.
  const root = mkdtempSync(join(tmpdir(), "ke"));
  chmodSync(root, 0o700);
  let sockN = 0;

  afterAll(() => {
    rmSync(root, { recursive: true, force: true });
  });

  function bindPeer(): {
    link: string;
    listener: ReturnType<typeof Bun.listen>;
    reader: FrameReader;
    opened: Promise<Bun.Socket>;
    closed: Promise<void>;
  } {
    sockN += 1;
    const path = join(root, `${sockN}.s`);
    const reader = new FrameReader();
    let resolveOpen: (socket: Bun.Socket) => void = () => undefined;
    const opened = new Promise<Bun.Socket>((resolve) => {
      resolveOpen = resolve;
    });
    let resolveClosed: () => void = () => undefined;
    const closed = new Promise<void>((resolve) => {
      resolveClosed = resolve;
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
        close() {
          resolveClosed();
        },
      },
    });
    return { link: `${path}#${TOKEN_HEX}`, listener, reader, opened, closed };
  }

  const handlers = {
    onReady(): void {},
    onLastWindowClosed(): void {},
    onLinkDead(): void {},
  };

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

  test("local close() after HELLO does not fire onLinkDead", async () => {
    const peer = bindPeer();
    try {
      let dead: Error | undefined;
      const connectP = LifecycleLink.connect(peer.link, {
        onReady(): void {},
        onLastWindowClosed(): void {},
        onLinkDead(err: Error): void {
          dead = err;
        },
      });
      const socket = await peer.opened;
      await peer.reader.readFrame();
      writeAll(socket, encodeFrame(FrameKind.Hello, 0, 0, TOKEN));
      const session = await connectP;
      session.close();
      await rejectWithin(2_000, peer.closed, "peer never saw local close()");
      await Promise.resolve();
      await Promise.resolve();
      expect(dead).toBeUndefined();
    } finally {
      peer.listener.stop(true);
    }
  });

  test("local quit() after Quit Reply does not fire onLinkDead", async () => {
    const peer = bindPeer();
    try {
      let dead: Error | undefined;
      const connectP = LifecycleLink.connect(peer.link, {
        onReady(): void {},
        onLastWindowClosed(): void {},
        onLinkDead(err: Error): void {
          dead = err;
        },
      });
      const socket = await peer.opened;
      await peer.reader.readFrame();
      writeAll(socket, encodeFrame(FrameKind.Hello, 0, 0, TOKEN));
      const session = await connectP;
      const quitP = session.quit();
      const call = await peer.reader.readFrame();
      expect(call.header.kind).toBe(FrameKind.Call);
      writeAll(
        socket,
        encodeFrame(FrameKind.Reply, LIFECYCLE_CHANNEL, call.header.corr, new Uint8Array()),
      );
      await rejectWithin(2_000, quitP, "quit hung after Quit Reply");
      await rejectWithin(2_000, peer.closed, "peer never saw local close after quit");
      await Promise.resolve();
      await Promise.resolve();
      expect(dead).toBeUndefined();
    } finally {
      peer.listener.stop(true);
    }
  });

  test("peer close after HELLO fails whenReady-side onLinkDead with KELD-IPC-001", async () => {
    const peer = bindPeer();
    try {
      let resolveDead: (err: Error) => void = () => undefined;
      const died = new Promise<Error>((resolve) => {
        resolveDead = resolve;
      });
      const connectP = LifecycleLink.connect(peer.link, {
        onReady(): void {},
        onLastWindowClosed(): void {},
        onLinkDead(err: Error): void {
          resolveDead(err);
        },
      });
      const socket = await peer.opened;
      await peer.reader.readFrame();
      writeAll(socket, encodeFrame(FrameKind.Hello, 0, 0, TOKEN));
      await connectP;
      socket.end();
      const dead = await rejectWithin(
        2_000,
        died,
        "onLinkDead was not called after peer close — ready waiters would hang",
      );
      expect(dead.message).toContain("KELD-IPC-001");
    } finally {
      peer.listener.stop(true);
    }
  });

  test("throwing onLinkDead still rejects an in-flight quit", async () => {
    const peer = bindPeer();
    try {
      const connectP = LifecycleLink.connect(peer.link, {
        onReady(): void {},
        onLastWindowClosed(): void {},
        onLinkDead(): void {
          throw new Error("KEL72_ON_LINK_DEAD_THROW");
        },
      });
      const socket = await peer.opened;
      await peer.reader.readFrame();
      writeAll(socket, encodeFrame(FrameKind.Hello, 0, 0, TOKEN));
      const session = await connectP;
      const quitP = session.quit();
      await peer.reader.readFrame();
      const start = Date.now();
      socket.end();
      await expect(quitP).rejects.toThrow("KELD-IPC-001");
      expect(Date.now() - start).toBeLessThan(1_000);
    } finally {
      peer.listener.stop(true);
    }
  });

  test("concurrent quit() shares the in-flight promise", async () => {
    const peer = bindPeer();
    try {
      const connectP = LifecycleLink.connect(peer.link, handlers);
      const socket = await peer.opened;
      await peer.reader.readFrame();
      writeAll(socket, encodeFrame(FrameKind.Hello, 0, 0, TOKEN));
      const session = await connectP;
      const first = session.quit();
      const second = session.quit();
      const call = await peer.reader.readFrame();
      expect(call.header.kind).toBe(FrameKind.Call);
      writeAll(socket, encodeFrame(FrameKind.Reply, LIFECYCLE_CHANNEL, call.header.corr, new Uint8Array()));
      await rejectWithin(
        2_000,
        Promise.all([first, second]),
        "second quit() overwrote #quitWaiter — the first waiter never resolved",
      );
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

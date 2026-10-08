/**
 * Contract tests for the KEL-72 lifecycle kipc client.
 *
 * Oracles: keld_ipc error codes, APP_LINK_IO_DEADLINE = 5s, Win32 endpoint
 * as `u16` (not parseInt), and concatenated frame bytes under backpressure.
 *
 * `LifecycleLink` runs on the GH-527 `WorkerLink`, which a realm opens at most
 * once, so each link case below spawns `fixtures/lifecycle_link_role.ts` as its
 * own role process against a Unix host bound here. The host half asserts the
 * frames; the role half reports what its handlers and promises saw. Write
 * deadlines are the transport Worker's `WriteQueue` contract, tested above and
 * in `@keld/kipc`.
 */
import { afterAll, afterEach, describe, expect, test } from "bun:test";
import { chmodSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  APP_LINK_IO_DEADLINE_MS,
  decodeCallError,
  DrainSignal,
  ECHO_CHANNEL,
  encodeHeader,
  errorFromErrFrame,
  FLAG_RAW,
  FrameKind,
  FrameReader,
  isCallError,
  isWin32PipeEndpoint,
  type KeldCallError,
  LIFECYCLE_CHANNEL,
  parseWin32Port,
  withIoDeadline,
  WriteQueue,
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

/** Encodes the postcard `CallError { code, message }` a host writes on Err. */
function encodeCallError(code: string, message: string): Uint8Array {
  const c = encodePostcardString(code);
  const m = encodePostcardString(message);
  const out = new Uint8Array(c.length + m.length);
  out.set(c, 0);
  out.set(m, c.length);
  return out;
}

function encodePostcardString(text: string): Uint8Array {
  const utf8 = new TextEncoder().encode(text);
  // Real LEB128 varint: postcard uses multi-byte lengths past 127, and the
  // production decoder must handle them, so the fixture must be able to emit them.
  const len: number[] = [];
  let n = utf8.length;
  do {
    let byte = n & 0x7f;
    n >>>= 7;
    if (n > 0) byte |= 0x80;
    len.push(byte);
  } while (n > 0);
  const out = new Uint8Array(len.length + utf8.length);
  out.set(len, 0);
  out.set(utf8, len.length);
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
    for (const bad of ["0", "01", "65536", "9000abc", "", " 9000"]) {
      expect(() => parseWin32Port(bad)).toThrow("KELD-IPC-007");
    }
  });
});

describe("isWin32PipeEndpoint", () => {
  test("accepts only the exact host-minted lowercase pipe shape", () => {
    const valid = String.raw`\\.\pipe\keld-${"ab".repeat(32)}`;
    expect(isWin32PipeEndpoint(valid)).toBe(true);
    for (const bad of [
      String.raw`\\.\pipe\other-${"ab".repeat(32)}`,
      String.raw`\\.\pipe\keld-${"AB".repeat(32)}`,
      String.raw`\\.\pipe\keld-${"ab".repeat(31)}`,
      "9000",
    ]) {
      expect(isWin32PipeEndpoint(bad)).toBe(false);
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

describe("CallError payload (platform-independent)", () => {
  // The SAME 16 bytes pinned in crates/keld-ipc/src/call_error.rs::PINNED, so the
  // two implementations share one wire oracle. Runs on every OS: the socket tests
  // below are Unix-only, which left KEL-102 with no Windows coverage at all.
  const PINNED = new Uint8Array([
    0x0d, 0x4b, 0x45, 0x4c, 0x44, 0x2d, 0x47, 0x55, 0x41, 0x52, 0x44, 0x30, 0x30, 0x31, 0x01,
    0x78,
  ]);

  test("decodes the bytes the Rust encoder pins", () => {
    expect(decodeCallError(PINNED)).toEqual({ code: "KELD-GUARD001", message: "x" });
  });

  test("a pre-KEL-102 bare postcard string is refused", () => {
    const legacy = encodePostcardString("KELD-GUARD001: capability is not granted.");
    expect(() => decodeCallError(legacy)).toThrow("KELD-IPC-003");
    const err = errorFromErrFrame(legacy);
    expect(err.message).toContain("not a CallError");
    expect(isCallError(err)).toBe(false);
  });

  test("trailing bytes are rejected", () => {
    const extra = new Uint8Array([...PINNED, 0x00]);
    expect(() => decodeCallError(extra)).toThrow("KELD-IPC-003");
  });

  test("a code that is not a KELD-* identifier is refused", () => {
    const bogus = new Uint8Array([...encodePostcardString("oops"), ...encodePostcardString("x")]);
    expect(() => decodeCallError(bogus)).toThrow("KELD-IPC-003");
    // ...and never reaches a consumer as a half-valid error.
    expect(isCallError(errorFromErrFrame(bogus))).toBe(false);
  });

  test("an empty payload is a protocol error, not a CallError", () => {
    const err = errorFromErrFrame(new Uint8Array());
    expect(err.message).toContain("KELD-IPC-005");
    expect(isCallError(err)).toBe(false);
  });

  test("a well-formed payload yields a code field and keeps the fix text", () => {
    const payload = new Uint8Array([
      ...encodePostcardString("KELD-GUARD001"),
      ...encodePostcardString(
        "KELD-GUARD001: capability `fs.read` is not granted. Append \"/tmp/x\" to " +
          "`/app/fs/read` in keld.permissions.jsonc.",
      ),
    ]);
    const err = errorFromErrFrame(payload);
    expect(isCallError(err)).toBe(true);
    expect((err as KeldCallError).code).toBe("KELD-GUARD001");
    expect(err.message).toContain("keld.permissions.jsonc");
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

describe.skipIf(process.platform === "win32")("LifecycleLink over a Unix host, one role process per case", () => {
  // `sockaddr_un.sun_path` is 104 bytes on macOS (108 on Linux). A path under
  // the package tree plus `.test-run/<pid>-<ts>-<rand>/e.sock` overflows.
  // Short unique 0o700 dir under tmpdir, same contract as keld-cli bind_unix_echo.
  const root = mkdtempSync(join(tmpdir(), "ke"));
  chmodSync(root, 0o700);
  const roleScript = join(import.meta.dir, "..", "fixtures", "lifecycle_link_role.ts");
  let sockN = 0;
  let role: ReturnType<typeof Bun.spawn> | undefined;
  let listener: ReturnType<typeof Bun.listen> | undefined;

  afterEach(() => {
    if (role && !role.killed) role.kill();
    role = undefined;
    listener?.stop(true);
    listener = undefined;
  });

  afterAll(() => {
    rmSync(root, { recursive: true, force: true });
  });

  interface Host {
    socket: Bun.Socket<undefined>;
    reader: FrameReader;
    closed: Promise<void>;
    /** Reads the role's HELLO; writes the HELLO reply unless the case writes its own. */
    hello(reply?: Uint8Array): Promise<void>;
    write(frame: Uint8Array): void;
    next(why: string): Promise<{ header: { kind: number; channel: number; corr: number }; payload: Uint8Array }>;
    /** The role's role-side report once it exits by itself (kill switch only). */
    finish(): Promise<Map<string, string>>;
  }

  async function start(scenario: string): Promise<Host> {
    sockN += 1;
    const path = join(root, `${sockN}.s`);
    const reader = new FrameReader();
    let resolveOpen: (socket: Bun.Socket<undefined>) => void = () => undefined;
    const opened = new Promise<Bun.Socket<undefined>>((resolve) => {
      resolveOpen = resolve;
    });
    let resolveClosed: () => void = () => undefined;
    const closed = new Promise<void>((resolve) => {
      resolveClosed = resolve;
    });
    listener = Bun.listen<undefined>({
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
          reader.end(new Error("KELD-IPC-001: the role closed the link"));
          resolveClosed();
        },
      },
    });
    const proc = Bun.spawn(["bun", roleScript, scenario], {
      env: { ...process.env, KELD_APP_LINK: `${path}#${TOKEN_HEX}` },
      stdout: "pipe",
      stderr: "pipe",
    });
    role = proc;
    const socket = await rejectWithin(10_000, opened, "the role never connected");
    return {
      socket,
      reader,
      closed,
      async hello(reply = encodeFrame(FrameKind.Hello, 0, 0, TOKEN)) {
        const hello = await rejectWithin(5_000, reader.readFrame(), "the role sent no HELLO");
        expect(hello.header.kind).toBe(FrameKind.Hello);
        writeAll(socket, reply);
      },
      write(frame) {
        writeAll(socket, frame);
      },
      next(why) {
        return rejectWithin(5_000, reader.readFrame(), why);
      },
      async finish() {
        const [stdout, stderr, code] = await rejectWithin(
          20_000,
          Promise.all([
            new Response(proc.stdout as ReadableStream<Uint8Array>).text(),
            new Response(proc.stderr as ReadableStream<Uint8Array>).text(),
            proc.exited,
          ]),
          "the role did not exit",
        );
        const out = new Map<string, string>();
        for (const line of stdout.split("\n")) {
          const rest = line.startsWith("KELD_LL ") ? line.slice("KELD_LL ".length) : undefined;
          const at = rest?.indexOf("=") ?? -1;
          if (rest !== undefined && at > 0) out.set(rest.slice(0, at), rest.slice(at + 1));
        }
        expect({ code, done: out.get("done"), stderr: code === 0 ? "" : stderr }).toEqual({
          code: 0,
          done: "true",
          stderr: "",
        });
        return out;
      },
    };
  }

  function headerWithFlags(kind: number, flags: number, channel: number, corr: number, payload: Uint8Array): Uint8Array {
    const header = encodeHeader(kind, flags, channel, corr, payload.length);
    const frame = new Uint8Array(header.length + payload.length);
    frame.set(header, 0);
    frame.set(payload, header.length);
    return frame;
  }

  test("host Echo Call returns one correlated Reply on the shared lifecycle session", async () => {
    const host = await start("echo-call");
    await host.hello();
    host.write(encodeFrame(FrameKind.Call, ECHO_CHANNEL, 41, new Uint8Array([0x11, 0x22])));
    const reply = await host.next("the shared app-link did not return the Echo Reply");
    expect(reply.header.kind).toBe(FrameKind.Reply);
    expect(reply.header.channel).toBe(ECHO_CHANNEL);
    expect(reply.header.corr).toBe(41);
    expect(Array.from(reply.payload)).toEqual([0x11, 0x22, 0x7f]);
    host.socket.end();
    const report = await host.finish();
    expect(report.get("echo-calls")).toBe("1");
    expect(report.get("echo-channel")).toBe(String(ECHO_CHANNEL));
  }, 30_000);

  test("a slow application handler does not block Ping or the shared reader", async () => {
    const host = await start("slow-handler");
    await host.hello();
    host.write(encodeFrame(FrameKind.Call, ECHO_CHANNEL, 41, new Uint8Array([0x11])));
    host.write(encodeFrame(FrameKind.Ping, 0, 1, new Uint8Array()));
    const pong = await host.next("a slow application handler blocked the Ping");
    expect(pong.header.kind).toBe(FrameKind.Ping);
    expect(pong.header.corr).toBe(1);
    // LastWindowClosed releases the handler; its Reply follows.
    host.write(encodeFrame(FrameKind.Event, LIFECYCLE_CHANNEL, 0, new Uint8Array([0x01])));
    const reply = await host.next("the Echo Reply did not follow the handler's release");
    expect(reply.header.kind).toBe(FrameKind.Reply);
    expect(reply.header.corr).toBe(41);
    expect(Array.from(reply.payload)).toEqual([0x11]);
    host.socket.end();
    const report = await host.finish();
    expect(report.get("last-window-closed")).toBe("true");
  }, 30_000);

  test("an application handler can quit: the Quit is the link's last frame", async () => {
    const host = await start("handler-quits");
    await host.hello();
    host.write(encodeFrame(FrameKind.Call, ECHO_CHANNEL, 51, new Uint8Array([0x21])));
    const quit = await host.next("the handler's Quit did not reach the host");
    expect(quit.header.kind).toBe(FrameKind.Call);
    expect(quit.header.channel).toBe(LIFECYCLE_CHANNEL);
    expect(Array.from(quit.payload)).toEqual([0x00]);
    host.write(encodeFrame(FrameKind.Reply, LIFECYCLE_CHANNEL, quit.header.corr, new Uint8Array([0x00])));
    // The link closes on the Quit REPLY: the handler's own answer is never written.
    await expect(host.next("the role's close")).rejects.toThrow("KELD-IPC-001");
    const report = await host.finish();
    expect(report.get("handler-quit-resolved")).toBe("true");
    expect(report.get("handler-finished")).toBe("true");
  }, 30_000);

  test("lifecycle EVENTs dispatch; an undecodable one ends the link with its cause", async () => {
    const host = await start("events");
    await host.hello();
    host.write(encodeFrame(FrameKind.Event, LIFECYCLE_CHANNEL, 0, new Uint8Array([0x00])));
    host.write(encodeFrame(FrameKind.Event, LIFECYCLE_CHANNEL, 0, new Uint8Array([0x01])));
    host.write(encodeFrame(FrameKind.Event, LIFECYCLE_CHANNEL, 0, new Uint8Array([0x07])));
    const report = await host.finish();
    expect(report.get("ready")).toBe("true");
    expect(report.get("last-window-closed")).toBe("true");
    expect(report.get("dead-code")).toBe("KELD-IPC-022");
    expect(report.get("dead-message")).toContain("KELD-IPC-003");
    expect(report.get("dead-message")).toContain("discriminant 7");
  }, 30_000);

  test("HELLO against a silent host is KELD-IPC-006", async () => {
    const host = await start("connect");
    await host.next("the role sent no HELLO");
    const report = await host.finish();
    expect(report.get("connect-resolved")).toBe("false");
    expect(report.get("connect-code")).toBe("KELD-IPC-006");
  }, 30_000);

  test("a HELLO reply of 31 bytes is a KELD-IPC-005 shape failure, never 007", async () => {
    const host = await start("connect");
    await host.hello(encodeFrame(FrameKind.Hello, 0, 0, TOKEN.slice(0, 31)));
    const report = await host.finish();
    expect(report.get("connect-code")).toBe("KELD-IPC-005");
    expect(report.get("connect-message")).not.toContain("KELD-IPC-007");
  }, 30_000);

  test("a HELLO reply carrying FLAG_RAW is KELD-IPC-005", async () => {
    const host = await start("connect");
    await host.hello(headerWithFlags(FrameKind.Hello, FLAG_RAW, 0, 0, TOKEN));
    const report = await host.finish();
    expect(report.get("connect-code")).toBe("KELD-IPC-005");
    expect(report.get("connect-message")).toContain("FLAG_RAW");
  }, 30_000);

  test("an undeclared kind after HELLO ends the link with a KELD-IPC-005 cause", async () => {
    const host = await start("events");
    await host.hello();
    host.write(encodeFrame(FrameKind.Grant, LIFECYCLE_CHANNEL, 0, new Uint8Array()));
    const report = await host.finish();
    expect(report.get("dead-code")).toBe("KELD-IPC-022");
    expect(report.get("dead-message")).toContain("KELD-IPC-005");
  }, 30_000);

  test("an EVENT with a nonzero correlation ends the link and is not dispatched", async () => {
    const host = await start("events");
    await host.hello();
    host.write(encodeFrame(FrameKind.Event, LIFECYCLE_CHANNEL, 4, new Uint8Array([0x00])));
    const report = await host.finish();
    expect(report.get("ready")).toBeUndefined();
    expect(report.get("dead-code")).toBe("KELD-IPC-022");
    expect(report.get("dead-message")).toContain("KELD-IPC-005");
  }, 30_000);

  test("host Err on Quit rejects with its CallError and closes the link", async () => {
    const host = await start("quit");
    await host.hello();
    const call = await host.next("the role's Quit");
    expect(call.header.channel).toBe(LIFECYCLE_CHANNEL);
    host.write(
      encodeFrame(
        FrameKind.Err,
        LIFECYCLE_CHANNEL,
        call.header.corr,
        encodeCallError(
          "KELD-GUARD003",
          "KELD-GUARD003: channel `lifecycle` is not granted to this principal. " +
            "Add `lifecycle` to this principal's channels list in keld.permissions.jsonc.",
        ),
      ),
    );
    await expect(host.next("the role's close")).rejects.toThrow("KELD-IPC-001");
    const report = await host.finish();
    expect(report.get("quit-resolved")).toBe("false");
    // The code arrives as a field; the role never parses it out of the text.
    expect(report.get("quit-callerror")).toBe("true");
    expect(report.get("quit-code")).toBe("KELD-GUARD003");
    expect(report.get("quit-message")).toContain("keld.permissions.jsonc");
    expect(report.get("again-resolved")).toBe("false");
    expect(report.get("dead")).toBeUndefined();
  }, 30_000);

  test("a REPLY with the wrong correlation cannot complete the Quit", async () => {
    const host = await start("quit");
    await host.hello();
    const call = await host.next("the role's Quit");
    host.write(encodeFrame(FrameKind.Reply, LIFECYCLE_CHANNEL, call.header.corr + 1, new Uint8Array([0x00])));
    const report = await host.finish();
    expect(report.get("quit-resolved")).toBe("false");
    expect(report.get("quit-code")).toBe("KELD-IPC-022");
    expect(report.get("quit-message")).toContain("KELD-IPC-005");
  }, 30_000);

  test("a pre-KEL-102 bare-string Err payload is refused, never half-decoded", async () => {
    const host = await start("quit");
    await host.hello();
    const call = await host.next("the role's Quit");
    host.write(
      encodeFrame(
        FrameKind.Err,
        LIFECYCLE_CHANNEL,
        call.header.corr,
        // The pre-KEL-102 shape: one bare postcard string. It is not a
        // CallError, so the link ends on it rather than surface a
        // plausible-looking error, and a host left on the old encoding cannot
        // go unnoticed in a mixed rollout.
        encodePostcardString("KELD-GUARD001: capability `fs.read` is not granted."),
      ),
    );
    const report = await host.finish();
    expect(report.get("quit-resolved")).toBe("false");
    expect(report.get("quit-code")).toBe("KELD-IPC-022");
    expect(report.get("quit-message")).toContain("not a CallError");
    // The old text never surfaces as if it had been understood.
    expect(report.get("quit-message")).not.toContain("fs.read");
  }, 30_000);

  test("the Quit REPLY resolves quit and closes the link; onLinkDead does not fire", async () => {
    const host = await start("quit");
    await host.hello();
    const call = await host.next("the role's Quit");
    expect(Array.from(call.payload)).toEqual([0x00]);
    host.write(encodeFrame(FrameKind.Reply, LIFECYCLE_CHANNEL, call.header.corr, new Uint8Array([0x00])));
    await expect(host.next("the role's close")).rejects.toThrow("KELD-IPC-001");
    const report = await host.finish();
    expect(report.get("quit-resolved")).toBe("true");
    expect(report.get("again-resolved")).toBe("true");
    expect(report.get("dead")).toBeUndefined();
  }, 30_000);

  test("concurrent quit() shares one Quit", async () => {
    const host = await start("quit-twice");
    await host.hello();
    const call = await host.next("the role's Quit");
    host.write(encodeFrame(FrameKind.Reply, LIFECYCLE_CHANNEL, call.header.corr, new Uint8Array([0x00])));
    await expect(host.next("a second Quit")).rejects.toThrow("KELD-IPC-001");
    const report = await host.finish();
    expect(report.get("same-promise")).toBe("true");
    expect(report.get("first-resolved")).toBe("true");
    expect(report.get("second-resolved")).toBe("true");
  }, 30_000);

  test("Quit against a silent host is KELD-IPC-006, and the link closes", async () => {
    const host = await start("quit");
    await host.hello();
    await host.next("the role's Quit");
    const report = await host.finish();
    await rejectWithin(5_000, host.closed, "the role's Quit deadline did not close the link");
    expect(report.get("quit-code")).toBe("KELD-IPC-006");
  }, 30_000);

  test("local close() after HELLO does not fire onLinkDead", async () => {
    const host = await start("local-close");
    await host.hello();
    await rejectWithin(5_000, host.closed, "the host never saw the local close()");
    const report = await host.finish();
    expect(report.get("closed")).toBe("true");
    expect(report.get("quit-after-close-resolved")).toBe("false");
    expect(report.get("quit-after-close-message")).toStartWith("KELD-IPC-001");
    expect(report.get("dead")).toBeUndefined();
  }, 30_000);

  test("host close after HELLO fails onLinkDead with KELD-IPC-022", async () => {
    const host = await start("events");
    await host.hello();
    host.socket.end();
    const report = await host.finish();
    expect(report.get("dead-code")).toBe("KELD-IPC-022");
    expect(report.get("dead-callerror")).toBe("true");
  }, 30_000);

  test("a throwing onLinkDead is isolated", async () => {
    const host = await start("throwing-dead");
    await host.hello();
    host.socket.end();
    const report = await host.finish();
    expect(report.get("after-throw")).toBe("true");
    expect(report.get("uncaught-count")).toBe("0");
  }, 30_000);
});

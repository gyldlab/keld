/**
 * Contract tests for the shared TypeScript app-link transport (KEL-136).
 *
 * Oracles: keld-ipc constants and error codes, the canonical TSV path (no row
 * copy), chunk-queue shape, and fail-closed unknown kind / wrong-channel Err.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
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
  MAX_PARKED_FRAMES,
  PROTOCOL_VERSION,
  RECEIVE_POLICIES,
  WriteQueue,
  decodeHeader,
  echoReplyWaiter,
  encodeHeader,
  lifecycleReplyWaiter,
  validateReceivedHeader,
} from "./transport.ts";

const REPO_ROOT = join(import.meta.dir, "../../../..");
const CORPUS_PATH = join(
  import.meta.dir,
  "../../../../crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv",
);
const CORPUS_SHA256 = "375f50c4bea1b690dbf7f385aee0464eae0946218058445306240b997d7e9746";

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
    const stat = statSync(path);
    if (stat.isDirectory()) {
      if (name === "node_modules" || name === "target" || name === ".git") continue;
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
});

describe("DrainSignal / WriteQueue", () => {
  test("one fire wakes every waiter", async () => {
    const drain = new DrainSignal();
    const first = drain.wait();
    const second = drain.wait();
    drain.fire();
    await Promise.all([first, second]);
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
});

describe("one source / no second copy", () => {
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

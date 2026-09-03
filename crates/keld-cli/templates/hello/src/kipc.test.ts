/**
 * Golden-vector tests for `kipc.ts` (KEL-30).
 *
 * Not part of `keld create`'s scaffold output — `template.rs`'s
 * `HELLO_TEMPLATE` list is an explicit allow-list, not a directory glob, and
 * this file is deliberately absent from it.
 *
 * Every expected byte sequence here is copied from the corresponding Rust
 * test (`crates/keld-ipc/src/{frame,codec,token}.rs`), not derived from this
 * file's own implementation — a wire-format bug on either side must fail
 * these, not just an internal roundtrip.
 */
import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import {
  CLIENT_AWAIT_HELLO,
  FLAG_RAW,
  FrameKind,
  echoReplyWaiter,
  validateReceivedHeader,
  FrameReader,
  decodeEchoResponse,
  decodeHeader,
  decodeVarint,
  encodeEchoRequest,
  encodeHeader,
  encodeVarint,
  isWin32PipeEndpoint,
  parseAppLink,
  parseWin32DiagnosticPort,
} from "./kipc";

describe("frame header", () => {
  test("is 16 bytes and round-trips every kind", () => {
    for (const kind of Object.values(FrameKind)) {
      const header = { kind, flags: 1, channel: 42, corr: 0xdead_beef, len: 1024 };
      const encoded = encodeHeader(header);
      expect(encoded.length).toBe(16);
      expect(decodeHeader(encoded)).toEqual(header);
    }
  });

  test("magic bytes are literal ASCII 'KI'", () => {
    // Pinned against `keld_ipc::MAGIC = u16::from_le_bytes(*b"KI")`: on the
    // wire that is just the two ASCII bytes, in order.
    const encoded = encodeHeader({ kind: FrameKind.Ping, flags: 0, channel: 0, corr: 0, len: 0 });
    expect(encoded[0]).toBe(0x4b); // 'K'
    expect(encoded[1]).toBe(0x49); // 'I'
    expect(encoded[2]).toBe(2); // PROTOCOL_VERSION
  });

  test("rejects bad magic", () => {
    const encoded = encodeHeader({ kind: FrameKind.Ping, flags: 0, channel: 0, corr: 0, len: 0 });
    encoded[0] = 0x58; // 'X'
    expect(() => decodeHeader(encoded)).toThrow("KELD-IPC-002");
  });

  test("rejects unsupported version", () => {
    const encoded = encodeHeader({ kind: FrameKind.Hello, flags: 0, channel: 0, corr: 0, len: 0 });
    encoded[2] = 99;
    const err = (() => {
      try {
        decodeHeader(encoded);
        return null;
      } catch (e) {
        return e as Error;
      }
    })();
    expect(err?.message).toContain("KELD-IPC-002");
    expect(err?.message).toContain("99");
  });

  test("rejects a header shorter than 16 bytes", () => {
    const full = encodeHeader({ kind: FrameKind.Ping, flags: 0, channel: 0, corr: 0, len: 0 });
    expect(() => decodeHeader(full.subarray(0, 8))).toThrow("KELD-IPC-002");
  });

  test("rejects an unknown frame kind (11 is one past the valid range)", () => {
    const encoded = encodeHeader({ kind: FrameKind.Ping, flags: 0, channel: 0, corr: 0, len: 0 });
    encoded[3] = 11;
    expect(() => decodeHeader(encoded)).toThrow("KELD-IPC-002");
  });
});

describe("postcard varint (LEB128)", () => {
  test("single-byte value round-trips", () => {
    const bytes = encodeVarint(3);
    expect(Array.from(bytes)).toEqual([0x03]);
    expect(decodeVarint(bytes, 0)).toEqual([3, 1]);
  });

  test("u32::MAX encodes as 5 bytes, matching manual LEB128 grouping", () => {
    // 0xFFFFFFFF split into 7-bit groups from the LSB: 7F 7F 7F 7F 0F, with
    // the continuation bit set on every byte but the last.
    const bytes = encodeVarint(0xffff_ffff);
    expect(Array.from(bytes)).toEqual([0xff, 0xff, 0xff, 0xff, 0x0f]);
    expect(decodeVarint(bytes, 0)).toEqual([0xffff_ffff, 5]);
  });

  test("zero round-trips as a single zero byte", () => {
    const bytes = encodeVarint(0);
    expect(Array.from(bytes)).toEqual([0x00]);
    expect(decodeVarint(bytes, 0)).toEqual([0, 1]);
  });

  test("truncated varint is a codec error", () => {
    // 0x80 alone has its continuation bit set with nothing following.
    expect(() => decodeVarint(new Uint8Array([0x80]), 0)).toThrow("KELD-IPC-003");
  });

  test("rejects a value above u32::MAX", () => {
    // The wire field is a Rust u32. encodeVarint rejected negatives and
    // non-integers but had no upper bound, so 2**32 encoded happily as a
    // 5-byte varint the peer cannot represent. The bound belongs here, with
    // the rest of the invariant, not in each caller.
    expect(() => encodeVarint(0x1_0000_0000)).toThrow("KELD-IPC-003");
    expect(() => encodeVarint(Number.MAX_SAFE_INTEGER)).toThrow("KELD-IPC-003");
    // The boundary itself stays valid.
    expect(Array.from(encodeVarint(0xffff_ffff))).toEqual([0xff, 0xff, 0xff, 0xff, 0x0f]);
  });
});

describe("EchoRequest / EchoResponse postcard framing", () => {
  test('matches the pinned Rust vector for {message:"kipc",count:3}', () => {
    // Rust: crates/keld-ipc/src/codec.rs `echo_request_postcard_bytes_are_pinned`
    // assert_eq!(bytes, [0x04, b'k', b'i', b'p', b'c', 0x03]);
    const bytes = encodeEchoRequest({ message: "kipc", count: 3 });
    expect(Array.from(bytes)).toEqual([0x04, 0x6b, 0x69, 0x70, 0x63, 0x03]);
  });

  test("empty message and zero count round-trip", () => {
    // Rust: crates/keld-ipc/src/echo.rs `empty_message_and_zero_count_roundtrip`
    const bytes = encodeEchoRequest({ message: "", count: 0 });
    expect(Array.from(bytes)).toEqual([0x00, 0x00]);
    expect(decodeEchoResponse(bytes)).toEqual({ message: "", count: 0 });
  });

  test("unicode message uses UTF-8 byte length, not JS string length", () => {
    // Rust: crates/keld-ipc/src/echo.rs `unicode_and_max_count_roundtrip`.
    // "héllo 🦀" is 11 UTF-8 bytes: h(1) é(2) l(1) l(1) o(1) space(1) 🦀(4).
    // JS `.length` would report 8 (UTF-16 code units) — the wrong prefix if
    // used by mistake.
    const message = "héllo 🦀";
    const bytes = encodeEchoRequest({ message, count: 0xffff_ffff });
    expect(bytes[0]).toBe(11);
    const decoded = decodeEchoResponse(bytes);
    expect(decoded.message).toBe(message);
    expect(decoded.count).toBe(0xffff_ffff);
  });

  test("rejects an out-of-range count instead of wrapping it", () => {
    // REGRESSION (KEL-121). encodeEchoRequest encoded `req.count >>> 0`, and
    // `>>>` converts modulo 2**32 BEFORE encodeVarint's own guard can see the
    // value. So -1 arrived as 4294967295 and 2**32 arrived as 0: both passed a
    // validation that was already there, one function away, and the peer
    // received a different request from the one that was made.
    //
    // A wrong-but-well-formed request is worse than a rejected one, because
    // nothing downstream can tell it happened.
    expect(() => encodeEchoRequest({ message: "kipc", count: -1 })).toThrow("KELD-IPC-003");
    expect(() => encodeEchoRequest({ message: "kipc", count: 0x1_0000_0000 })).toThrow("KELD-IPC-003");
    expect(() => encodeEchoRequest({ message: "kipc", count: 1.5 })).toThrow("KELD-IPC-003");
  });

  test("count boundary values still encode", () => {
    expect(Array.from(encodeEchoRequest({ message: "", count: 0 }))).toEqual([0x00, 0x00]);
    const max = encodeEchoRequest({ message: "", count: 0xffff_ffff });
    expect(Array.from(max)).toEqual([0x00, 0xff, 0xff, 0xff, 0xff, 0x0f]);
    expect(decodeEchoResponse(max)).toEqual({ message: "", count: 0xffff_ffff });
  });

  test("rejects trailing bytes after a valid EchoResponse", () => {
    const bytes = encodeEchoRequest({ message: "kipc", count: 3 });
    const withGarbage = new Uint8Array([...bytes, 0x00]);
    expect(() => decodeEchoResponse(withGarbage)).toThrow("KELD-IPC-003");
  });

  test("empty payload is a codec error, not an empty response", () => {
    expect(() => decodeEchoResponse(new Uint8Array())).toThrow("KELD-IPC-003");
  });
});

describe("parseAppLink", () => {
  test("splits endpoint and 64-hex token on the last '#'", () => {
    // Rust: crates/keld-ipc/src/token.rs `parse_app_link_rsplits_on_last_hash`
    const hex = "a5".repeat(32);
    const { endpoint, token } = parseAppLink(`/tmp/a#b#${hex}`);
    expect(endpoint).toBe("/tmp/a#b");
    expect(Array.from(token)).toEqual(new Array(32).fill(0xa5));
  });

  test("rejects a missing or empty endpoint", () => {
    const hex = "11".repeat(32);
    for (const bad of ["", "nopath", `#${hex}`]) {
      expect(() => parseAppLink(bad)).toThrow("KELD-IPC-007");
    }
  });

  test("rejects a token that is not exactly 64 hex characters", () => {
    for (const bad of ["/e#", "/e#aa", `/e#${"a5".repeat(31)}`, `/e#${"zz".repeat(32)}`]) {
      expect(() => parseAppLink(bad)).toThrow("KELD-IPC-007");
    }
  });

  test("accepts uppercase hex", () => {
    const hex = "A5".repeat(32);
    const { token } = parseAppLink(`/tmp/e.sock#${hex}`);
    expect(Array.from(token)).toEqual(new Array(32).fill(0xa5));
  });
});

describe("Windows endpoint selection", () => {
  test("accepts the exact named-pipe shape", () => {
    const valid = String.raw`\\.\pipe\keld-${"3c".repeat(32)}`;
    expect(isWin32PipeEndpoint(valid)).toBe(true);
    expect(isWin32PipeEndpoint(String.raw`\\.\pipe\keld-${"3C".repeat(32)}`)).toBe(false);
    expect(isWin32PipeEndpoint("9000")).toBe(false);
  });

  test("retains only strict decimal diagnostic ports", () => {
    expect(parseWin32DiagnosticPort("1")).toBe(1);
    expect(parseWin32DiagnosticPort("65535")).toBe(65535);
    for (const bad of ["0", "01", "65536", "9000x", "127.0.0.1:9000", ""]) {
      expect(() => parseWin32DiagnosticPort(bad)).toThrow("KELD-IPC-007");
    }
  });
});

describe("FrameReader — untrusted peer bytes", () => {
  test("resolves once a complete frame has been pushed, even split across chunks", async () => {
    const reader = new FrameReader();
    const encoded = encodeHeader({ kind: FrameKind.Ping, flags: 0, channel: 7, corr: 9, len: 3 });
    const framePromise = reader.readFrame();
    reader.push(encoded.subarray(0, 10)); // partial header
    reader.push(encoded.subarray(10)); // rest of header, no payload yet
    reader.push(new Uint8Array([1, 2, 3])); // payload arrives in its own chunk
    const frame = await framePromise;
    expect(frame.header.kind).toBe(FrameKind.Ping);
    expect(frame.header.channel).toBe(7);
    expect(Array.from(frame.payload)).toEqual([1, 2, 3]);
  });

  test("rejects a peer-claimed length above MAX_FRAME_LEN instead of waiting forever", async () => {
    const reader = new FrameReader();
    const framePromise = reader.readFrame();
    // 16 MiB + 1 — one byte over the protocol cap, claimed by the header
    // alone; no payload bytes are ever sent, matching a hostile or buggy
    // peer that sends a header and stalls.
    const encoded = encodeHeader({
      kind: FrameKind.Reply,
      flags: 0,
      channel: 1,
      corr: 1,
      len: 16 * 1024 * 1024 + 1,
    });
    reader.push(encoded);
    await expect(framePromise).rejects.toThrow("KELD-IPC-004");
  });

  test("a malformed header rejects the pending readFrame(), not an uncaught throw", async () => {
    const reader = new FrameReader();
    const framePromise = reader.readFrame();
    const garbage = new Uint8Array(16).fill(0xff); // bad magic, bad everything
    reader.push(garbage);
    await expect(framePromise).rejects.toThrow("KELD-IPC-002");
  });

  test("fail() rejects a readFrame() call made after the reader is already closed", async () => {
    const reader = new FrameReader();
    reader.fail(new Error("KELD-IPC-001: connection closed by peer"));
    await expect(reader.readFrame()).rejects.toThrow("KELD-IPC-001");
  });
});

describe("shared receiver rules (KEL-133)", () => {
  test("HELLO shape failures are KELD-IPC-005 and foreign length never 007", () => {
    const good = { kind: FrameKind.Hello, flags: 0, channel: 0, corr: 0, len: 32 };
    expect(validateReceivedHeader(CLIENT_AWAIT_HELLO, good)).toEqual(good);
    for (const [bad, detail] of [
      [{ ...good, len: 31 }, "declared exact shape"],
      [{ ...good, flags: FLAG_RAW }, "FLAG_RAW"],
      [{ ...good, flags: 2 }, "unknown flag bits"],
      [{ ...good, channel: 1 }, "wrong channel"],
      [{ ...good, corr: 1 }, "correlation must be 0"],
      [{ ...good, kind: FrameKind.Ping }, "not declared"],
    ] as const) {
      expect(() => validateReceivedHeader(CLIENT_AWAIT_HELLO, bad)).toThrow(detail);
      expect(() => validateReceivedHeader(CLIENT_AWAIT_HELLO, bad)).toThrow("KELD-IPC-005");
    }
  });

  test("only the awaited echo REPLY completes the waiter", () => {
    const waiter = echoReplyWaiter(7);
    const good = { kind: FrameKind.Reply, flags: 0, channel: 1, corr: 7, len: 6 };
    expect(validateReceivedHeader(waiter, good)).toEqual(good);
    expect(() => validateReceivedHeader(waiter, { ...good, corr: 8 })).toThrow("does not match the awaited call");
    expect(() => validateReceivedHeader(waiter, { ...good, kind: FrameKind.Err })).toThrow("not declared");
    expect(() => validateReceivedHeader(waiter, { ...good, channel: 3 })).toThrow("wrong channel");
    expect(() => validateReceivedHeader(waiter, { ...good, flags: FLAG_RAW })).toThrow("FLAG_RAW");
  });

  // In the Keld repository the canonical corpus is reachable relative to the
  // template; a generated app has no fixture and skips (its rules are pinned
  // above and by the @keld/electron corpus runner).
  const corpusPath = join(import.meta.dir, "../../../../keld-ipc/tests/fixtures/receiver-semantics-v0.tsv");
  test.skipIf(!existsSync(corpusPath))("agrees with the canonical corpus for its two policies", () => {
    const lines = readFileSync(corpusPath, "utf8").split("\n").filter((l) => l.length > 0);
    expect(lines[0].startsWith("receiver-semantics-v0\tv1\t")).toBe(true);
    let checked = 0;
    for (const line of lines.slice(1)) {
      const [id, policyName, headerHex, , expectedCode] = line.split("\t");
      const base = policyName.split(":", 1)[0];
      if (base !== "client-await-hello" && base !== "echo-reply-waiter") continue;
      if (headerHex.length !== 32) continue; // truncated-header rows belong to the reader, not the validator
      const bytes = Uint8Array.from(headerHex.match(/../g)!.map((h) => Number.parseInt(h, 16)));
      const view = new DataView(bytes.buffer);
      const header = {
        kind: bytes[3] as never,
        flags: view.getUint16(4, true),
        channel: view.getUint16(6, true),
        corr: view.getUint32(8, true),
        len: view.getUint32(12, true),
      };
      const policy =
        base === "client-await-hello" ? CLIENT_AWAIT_HELLO : echoReplyWaiter(Number(policyName.split(":")[1]));
      let got: string;
      try {
        validateReceivedHeader(policy, header);
        got = "ok";
      } catch (err) {
        got = (err as Error).message.slice(0, 12);
      }
      // Rows that reject at the header stage must agree exactly; rows the
      // header admits continue to token/codec stages this test does not run.
      if (expectedCode === "KELD-IPC-005") expect(`${id}: ${got}`).toBe(`${id}: KELD-IPC-005`);
      else expect(`${id}: ${got}`).toBe(`${id}: ok`);
      checked += 1;
    }
    expect(checked).toBeGreaterThanOrEqual(6);
  });
});

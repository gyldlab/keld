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
import {
  FrameKind,
  decodeEchoResponse,
  decodeHeader,
  decodeVarint,
  encodeEchoRequest,
  encodeHeader,
  encodeVarint,
  parseAppLink,
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

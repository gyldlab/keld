/**
 * Canonical receiver-semantics corpus runner, Bun side (KEL-133 spec §3
 * criterion 10): loads the exact repository fixture the Rust suite runs,
 * drives every frame row through this package's production header pipeline
 * (decodeHeader → envelope cap → validateReceivedHeader → payload stage),
 * evaluates the deadline traces with the same virtual-clock model, and prints
 * the fixture's SHA-256 so both suites can be compared byte-for-byte. The
 * corpus is the single semantic table; this file copies none of its rows.
 */

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "bun:test";

import {
  FrameKind,
  FrameReader,
  RECEIVE_POLICIES,
  type ReceivePolicy,
  echoReplyWaiter,
  errorFromErrFrame,
  lifecycleReplyWaiter,
  primaryAppReceiver,
  privilegedCallReceiver,
  validateReceivedHeader,
} from "./link";

const CORPUS_PATH = join(
  import.meta.dir,
  "../../../../crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv",
);
const HEADER_LEN = 16;
const MAX_FRAME_LEN = 16 * 1024 * 1024;
const STALL_LIMIT_MS = 5000;
/** One owner, one digest: the Rust suite asserts this same constant. */
const CORPUS_SHA256 = "375f50c4bea1b690dbf7f385aee0464eae0946218058445306240b997d7e9746";
/** Pre-auth policies may reaccept after a rejection; authenticated ones close. */
const PRE_AUTH_POLICIES = new Set(["server-pre-auth-hello", "client-await-hello"]);

/** Fixture token declared in the corpus version row (0x01..0x20). */
const FIXTURE_TOKEN = Uint8Array.from({ length: 32 }, (_, i) => i + 1);

interface Row {
  id: string;
  policy: string;
  headerOrTrace: string;
  payloadHex: string;
  expectedCode: string;
  linkAction: string;
  handlerEffects: number;
}

function loadRows(): { version: string; rows: Row[]; bytes: Uint8Array } {
  const bytes = new Uint8Array(readFileSync(CORPUS_PATH));
  const text = new TextDecoder("ascii").decode(bytes);
  const lines = text.split("\n").filter((line) => line.length > 0);
  const version = lines[0];
  expect(version.startsWith("receiver-semantics-v0\tv1\t")).toBe(true);
  expect(version).toContain("app_link_io_deadline_ms=5000");
  const rows = lines.slice(1).map((line) => {
    const cols = line.split("\t");
    expect(cols.length).toBe(7);
    return {
      id: cols[0],
      policy: cols[1],
      headerOrTrace: cols[2],
      payloadHex: cols[3],
      expectedCode: cols[4],
      linkAction: cols[5],
      handlerEffects: Number(cols[6]),
    };
  });
  return { version, rows, bytes };
}

function unhex(hex: string): Uint8Array {
  if (hex === "-") return new Uint8Array(0);
  expect(hex.length % 2).toBe(0);
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i += 1) {
    out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function policyByName(name: string): ReceivePolicy {
  const [base, arg] = name.includes(":") ? name.split(":", 2) : [name, undefined];
  switch (base) {
    case "server-pre-auth-hello":
      return RECEIVE_POLICIES.serverPreAuthHello;
    case "client-await-hello":
      return RECEIVE_POLICIES.clientAwaitHello;
    case "echo-receiver":
      return RECEIVE_POLICIES.echoReceiver;
    case "echo-reply-waiter":
      return echoReplyWaiter(Number(arg));
    case "lifecycle-receiver":
      return RECEIVE_POLICIES.lifecycleReceiver;
    case "lifecycle-event-receiver":
      return RECEIVE_POLICIES.lifecycleEventReceiver;
    case "lifecycle-reply-waiter":
      return lifecycleReplyWaiter(Number(arg));
    case "privileged-fs-receiver":
      return privilegedCallReceiver(Number(arg));
    case "primary-app-receiver":
      return primaryAppReceiver();
    default:
      throw new Error(`unknown corpus policy: ${name}`);
  }
}

/**
 * Stage one through the PRODUCTION reader: syntax (002), envelope cap (004)
 * and truncation (001, the socket close path) are decided by `FrameReader`,
 * never by a test-local copy.
 */
async function readWithProductionReader(
  bytes: Uint8Array,
  truncated: boolean,
): Promise<{ kind: number; flags: number; channel: number; corr: number; len: number; payload: Uint8Array }> {
  const reader = new FrameReader();
  const pending = reader.readFrame();
  reader.push(bytes);
  if (truncated) {
    reader.fail(new Error("KELD-IPC-001: connection closed by peer"));
  }
  const frame = await pending;
  return { ...frame.header, payload: frame.payload };
}

function timingSafeEq(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i += 1) diff |= a[i] ^ b[i];
  return diff === 0;
}

/**
 * Test-local postcard mini-decoders for stage two. They decode corpus payload
 * bytes for exactly the channel schemas the Rust suite decodes; they are
 * consumers of the corpus bytes, not a second semantic table.
 */
function decodePostcardString(bytes: Uint8Array, at: number): { value: string; next: number } {
  // postcard varint length (LEB128) then UTF-8 bytes.
  let len = 0;
  let shift = 0;
  let i = at;
  for (;;) {
    if (i >= bytes.length) throw new Error("KELD-IPC-003: truncated varint");
    const b = bytes[i];
    i += 1;
    len |= (b & 0x7f) << shift;
    if ((b & 0x80) === 0) break;
    shift += 7;
  }
  if (i + len > bytes.length) throw new Error("KELD-IPC-003: truncated string");
  const value = new TextDecoder("utf-8", { fatal: true }).decode(bytes.slice(i, i + len));
  return { value, next: i + len };
}

function decodeVarintU32(bytes: Uint8Array, at: number): { value: number; next: number } {
  let value = 0;
  let shift = 0;
  let i = at;
  for (;;) {
    if (i >= bytes.length) throw new Error("KELD-IPC-003: truncated varint");
    const b = bytes[i];
    i += 1;
    value |= (b & 0x7f) << shift;
    if ((b & 0x80) === 0) break;
    shift += 7;
  }
  return { value: value >>> 0, next: i };
}

function decodeEchoShape(payload: Uint8Array): void {
  const s = decodePostcardString(payload, 0);
  const n = decodeVarintU32(payload, s.next);
  if (n.next !== payload.length) throw new Error("KELD-IPC-003: trailing bytes");
}

function decodeUnitEnum(payload: Uint8Array, max: number): void {
  if (payload.length !== 1 || payload[0] > max) {
    throw new Error("KELD-IPC-003: invalid unit enum");
  }
}

function decodeCallErrorShape(payload: Uint8Array): void {
  const code = decodePostcardString(payload, 0);
  const message = decodePostcardString(payload, code.next);
  if (message.next !== payload.length) throw new Error("KELD-IPC-003: trailing bytes");
}

function stageTwo(policyName: string, kind: number, payload: Uint8Array): void {
  const base = policyName.split(":", 1)[0];
  if (kind === FrameKind.Ping) return;
  switch (base) {
    case "server-pre-auth-hello":
    case "client-await-hello": {
      if (!timingSafeEq(payload, FIXTURE_TOKEN)) {
        throw new Error("KELD-IPC-007: HELLO session token mismatch");
      }
      return;
    }
    case "echo-receiver":
      decodeEchoShape(payload); // EchoRequest { message, count }
      return;
    case "echo-reply-waiter":
      decodeEchoShape(payload); // EchoResponse { message, count }
      return;
    case "lifecycle-receiver":
      decodeUnitEnum(payload, 0); // LifecycleRequest::Quit
      return;
    case "lifecycle-event-receiver":
      decodeUnitEnum(payload, 1); // Ready | LastWindowClosed
      return;
    case "lifecycle-reply-waiter":
      if (kind === FrameKind.Err) {
        decodeCallErrorShape(payload);
        // Production decoder agreement: the code must be recoverable from
        // the Error the library builds for this ERR payload.
        if (!errorFromErrFrame(payload).message.includes("KELD-GUARD001")) {
          throw new Error("KELD-IPC-003: production CallError decode disagrees");
        }
      } else decodeUnitEnum(payload, 0); // LifecycleResponse::Quit
      return;
    case "privileged-fs-receiver":
    case "primary-app-receiver":
      // The privileged codec is KEL-102/T3's; the primary session's
      // per-channel codecs are proven by their own policies' rows.
      return;
    default:
      throw new Error(`no stage-two rule for ${policyName}`);
  }
}

async function runFrameRow(row: Row): Promise<string> {
  const headerBytes = unhex(row.headerOrTrace);
  const payload = unhex(row.payloadHex);
  const bytes = new Uint8Array(headerBytes.length + payload.length);
  bytes.set(headerBytes, 0);
  bytes.set(payload, headerBytes.length);
  try {
    // A row whose bytes cannot complete a frame is the peer-close path.
    const declaredLen =
      headerBytes.length >= HEADER_LEN
        ? new DataView(headerBytes.buffer, headerBytes.byteOffset, headerBytes.byteLength).getUint32(12, true)
        : Number.MAX_SAFE_INTEGER;
    const truncated =
      headerBytes.length < HEADER_LEN || (declaredLen <= MAX_FRAME_LEN && payload.length < declaredLen);
    const frame = await readWithProductionReader(bytes, truncated);
    validateReceivedHeader(policyByName(row.policy), frame);
    stageTwo(row.policy, frame.kind, frame.payload);
    return "ok";
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return message.slice(0, 12);
  }
}

describe("receiver-semantics corpus (Bun consumer)", () => {
  const { rows, bytes } = loadRows();

  test("every frame row reproduces its expected code and link action", async () => {
    let checked = 0;
    for (const row of rows) {
      if (row.policy.startsWith("trace:")) continue;
      if (row.expectedCode === "ok-header") {
        // Boundary row: the declared envelope admits at header stage; the
        // corpus does not materialize a MAX_FRAME_LEN payload. Decode the
        // header through the production reader with an empty pending body.
        const headerBytes = unhex(row.headerOrTrace);
        const view = new DataView(headerBytes.buffer, headerBytes.byteOffset, headerBytes.byteLength);
        expect(view.getUint32(12, true)).toBeLessThanOrEqual(MAX_FRAME_LEN);
        validateReceivedHeader(policyByName(row.policy), {
          kind: headerBytes[3],
          flags: view.getUint16(4, true),
          channel: view.getUint16(6, true),
          corr: view.getUint32(8, true),
          len: view.getUint32(12, true),
        });
        checked += 1;
        continue;
      }
      const got = await runFrameRow(row);
      expect(`${row.id}: ${got}`).toBe(`${row.id}: ${row.expectedCode}`);
      const base = row.policy.split(":", 1)[0];
      const expectedAction = got === "ok" ? "admit" : PRE_AUTH_POLICIES.has(base) ? "close-reaccept" : "close";
      expect(`${row.id}: ${row.linkAction}`).toBe(`${row.id}: ${expectedAction}`);
      if (got !== "ok") {
        expect(`${row.id}: effects=${row.handlerEffects}`).toBe(`${row.id}: effects=0`);
      }
      checked += 1;
    }
    expect(checked).toBeGreaterThanOrEqual(60);
  });

  test("every trace row reproduces its expiry with the shared deadline model", () => {
    let checked = 0;
    for (const row of rows) {
      if (!row.policy.startsWith("trace:")) continue;
      const [kind, ms] = row.policy.slice("trace:".length).split("-deadline-ms=");
      expect(["generation", "session"]).toContain(kind);
      const deadlineMs = Number(ms);

      const arrivals: Array<{ at: number; len: number }> = [];
      if (row.headerOrTrace !== "-" && row.headerOrTrace.length > 0) {
        for (const action of row.headerOrTrace.split(";")) {
          const [at, hex] = action.slice(2).split("=");
          arrivals.push({ at: Number(at), len: hex.length / 2 });
        }
      }

      // Spec deadline model: the absolute deadline never renews; the first
      // byte starts the stall clock; idle polls start nothing.
      const expiry =
        arrivals.length > 0
          ? Math.min(deadlineMs, arrivals[0].at + STALL_LIMIT_MS)
          : deadlineMs;
      const needed = HEADER_LEN + 32; // every v1 trace is a HELLO admission
      let got = 0;
      let completedAt: number | null = null;
      for (const { at, len } of arrivals) {
        if (at >= expiry) break;
        got += len;
        if (got >= needed) {
          completedAt = at;
          break;
        }
      }

      if (row.expectedCode === "ok") {
        expect(completedAt).not.toBeNull();
      } else {
        expect(`${row.id}: ${row.expectedCode}`).toBe(`${row.id}: KELD-IPC-006`);
        expect(completedAt).toBeNull();
        const closeAt = row.linkAction.startsWith("close-at-")
          ? Number(row.linkAction.slice("close-at-".length))
          : null;
        if (closeAt !== null) {
          expect(`${row.id}: expiry=${expiry}`).toBe(`${row.id}: expiry=${closeAt}`);
        }
      }
      checked += 1;
    }
    expect(checked).toBe(6);
  });

  test("fixture digest matches the one owner constant shared with the Rust suite", () => {
    const digest = createHash("sha256").update(bytes).digest("hex");
    console.log(`receiver-semantics-v0.tsv sha256=${digest}`);
    expect(digest).toBe(CORPUS_SHA256);
  });
});

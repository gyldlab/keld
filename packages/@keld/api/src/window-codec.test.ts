/** The Rust-owned literal TSV is the one postcard oracle for both runtimes. */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "bun:test";
import {
  decodeCloseVerdict,
  decodeRect,
  decodeWindowEvent,
  decodeWindowRef,
  decodeWindowRequest,
  decodeWindowResponse,
  decodeWindowState,
  encodeCloseVerdict,
  encodeRect,
  encodeWindowEvent,
  encodeWindowRef,
  encodeWindowRequest,
  encodeWindowResponse,
  encodeWindowState,
  type CloseVerdict,
  type Rect,
  type WindowEvent,
  type WindowRef,
  type WindowRequest,
  type WindowResponse,
  type WindowState,
} from "./window-codec.ts";

const vectors = new Map<string, { family: string; bytes: Uint8Array }>();
for (const line of readFileSync(
  join(
    import.meta.dir,
    "../../../../crates/keld-ipc/tests/fixtures/window-v0.tsv",
  ),
  "utf8",
)
  .trimEnd()
  .split("\n")) {
  if (line.startsWith("#")) continue;
  const fields = line.split("\t");
  if (
    fields.length !== 3 ||
    !/^(?:[0-9a-f]{2})+$/.test(fields[2]) ||
    vectors.has(fields[0])
  ) {
    throw new Error(`Malformed or duplicate shared vector: ${line}`);
  }
  vectors.set(fields[0], {
    family: fields[1],
    bytes: Uint8Array.from(Buffer.from(fields[2], "hex")),
  });
}
const replayed = new Set<string>();
const WINDOW: WindowRef = { id: 7, incarnation: 300n };
const BOUNDS: Rect = { x: -3, y: 4, width: 640, height: 480 };
const STATE: WindowState = {
  bounds: BOUNDS,
  focused: true,
  maximized: false,
  full_screen: true,
};

function replay<T>(
  family: string,
  cases: readonly (readonly [string, T])[],
  encode: (value: T) => Uint8Array,
  decode: (payload: Uint8Array) => T,
): void {
  for (const [id, value] of cases) {
    if (replayed.has(id)) throw new Error(`Duplicate replay case: ${id}`);
    replayed.add(id);
    test(`${family}: ${id} matches the shared literal bytes`, () => {
      const vector = vectors.get(id);
      expect(vector?.family).toBe(family);
      if (!vector) throw new Error(`Missing shared vector: ${id}`);
      expect(encode(value)).toEqual(vector.bytes);
      expect(decode(vector.bytes)).toEqual(value);
      for (let end = 0; end < vector.bytes.length; end += 1) {
        expect(() => decode(vector.bytes.subarray(0, end))).toThrow(
          "KELD-IPC-003",
        );
      }
      expect(() => decode(new Uint8Array([...vector.bytes, 0]))).toThrow(
        "KELD-IPC-003",
      );
    });
  }
}

replay<WindowRef>(
  "reference",
  [
    ["reference-distinct-identities", WINDOW],
    [
      "reference-integer-maxima",
      { id: 0xffff_ffff, incarnation: 0xffff_ffff_ffff_ffffn },
    ],
  ],
  encodeWindowRef,
  decodeWindowRef,
);
replay<Rect>(
  "rect",
  [
    ["rect-signed-coordinates", BOUNDS],
    [
      "rect-integer-boundaries",
      { x: -0x8000_0000, y: 0x7fff_ffff, width: 0xffff_ffff, height: 0 },
    ],
  ],
  encodeRect,
  decodeRect,
);
replay<WindowState>(
  "state",
  [
    ["state-independent-flags", STATE],
    ["state-focused-only", { ...STATE, full_screen: false }],
    [
      "state-maximized-only",
      { ...STATE, focused: false, maximized: true, full_screen: false },
    ],
    ["state-full-screen-only", { ...STATE, focused: false }],
  ],
  encodeWindowState,
  decodeWindowState,
);
replay<WindowEvent>(
  "event",
  [
    [
      "event-created",
      { variant: "Created", window: WINDOW, state: STATE, pending_close: null },
    ],
    [
      "event-created-pending-close",
      { variant: "Created", window: WINDOW, state: STATE, pending_close: 300n },
    ],
    ["event-bounds", { variant: "Bounds", window: WINDOW, bounds: BOUNDS }],
    ["event-focus", { variant: "Focus", window: WINDOW, focused: true }],
    [
      "event-maximized",
      { variant: "Maximized", window: WINDOW, maximized: false },
    ],
    [
      "event-full-screen",
      { variant: "FullScreen", window: WINDOW, full_screen: true },
    ],
    [
      "event-close-requested",
      { variant: "CloseRequested", window: WINDOW, close_seq: 300n },
    ],
    ["event-destroyed", { variant: "Destroyed", window: WINDOW }],
  ],
  encodeWindowEvent,
  decodeWindowEvent,
);
replay<WindowRequest>(
  "request",
  [
    [
      "request-create-positioned",
      { variant: "Create", width: 640, height: 480, x: -3, y: 4 },
    ],
    [
      "request-create-no-position",
      { variant: "Create", width: 0, height: 0, x: null, y: null },
    ],
    [
      "request-create-maxima",
      {
        variant: "Create",
        width: 0xffff_ffff,
        height: 0xffff_ffff,
        x: -0x8000_0000,
        y: 0x7fff_ffff,
      },
    ],
    [
      "request-maximized",
      { variant: "SetMaximized", window: WINDOW, maximized: true },
    ],
    [
      "request-full-screen",
      { variant: "SetFullScreen", window: WINDOW, full_screen: false },
    ],
    ["request-close", { variant: "RequestClose", window: WINDOW }],
    [
      "request-close-allow",
      {
        variant: "CloseReply",
        window: WINDOW,
        close_seq: 300n,
        verdict: "Allow",
      },
    ],
    [
      "request-close-veto",
      {
        variant: "CloseReply",
        window: WINDOW,
        close_seq: 300n,
        verdict: "Veto",
      },
    ],
    ["request-destroy", { variant: "Destroy", window: WINDOW }],
  ],
  encodeWindowRequest,
  decodeWindowRequest,
);
replay<WindowResponse>(
  "response",
  [
    ["response-created", { variant: "Created", window: WINDOW }],
    ["response-accepted", { variant: "Accepted" }],
    [
      "response-close-requested",
      { variant: "CloseRequested", close_seq: 300n },
    ],
    ["response-merged", { variant: "Merged", close_seq: 300n }],
    ["response-already-closing", { variant: "AlreadyClosing" }],
    ["response-reply-applied", { variant: "ReplyApplied" }],
    ["response-reply-stale", { variant: "ReplyStale" }],
    ["response-destroyed", { variant: "Destroyed" }],
    ["response-adopted", { variant: "Adopted", window: WINDOW }],
    [
      "response-close-sequence-maximum",
      { variant: "CloseRequested", close_seq: 0xffff_ffff_ffff_ffffn },
    ],
  ],
  encodeWindowResponse,
  decodeWindowResponse,
);
replay<CloseVerdict>(
  "verdict",
  [
    ["verdict-allow", "Allow"],
    ["verdict-veto", "Veto"],
  ],
  encodeCloseVerdict,
  decodeCloseVerdict,
);

test("every shared vector is replayed exactly once", () => {
  expect(vectors.size).toBe(37);
  expect([...replayed].sort()).toEqual([...vectors.keys()].sort());
});

test("unknown discriminants and malformed scalar bytes fail closed", () => {
  for (const [decode, bytes] of [
    [decodeWindowEvent, [7]],
    [decodeWindowRequest, [6]],
    [decodeWindowResponse, [9]],
    [decodeCloseVerdict, [2]],
    [decodeWindowEvent, [2, 7, 0xac, 2, 2]],
    [decodeWindowRequest, [0, 0, 0, 2]],
    [decodeWindowRequest, [0, 0, 0, 0, 2]],
    [decodeWindowRef, [0xff, 0xff, 0xff, 0xff, 0x10, 0]],
    [decodeWindowRef, [1, ...new Array<number>(10).fill(0xff)]],
  ] as const) {
    expect(() => decode(Uint8Array.from(bytes))).toThrow("KELD-IPC-003");
  }
  const created = vectors.get("event-created")!.bytes.slice();
  created[created.length - 1] = 2;
  expect(() => decodeWindowEvent(created)).toThrow("KELD-IPC-003");
});

test("all payload decoders reject non-byte input", () => {
  for (const decode of [
    decodeWindowRef,
    decodeRect,
    decodeWindowState,
    decodeWindowEvent,
    decodeWindowRequest,
    decodeWindowResponse,
    decodeCloseVerdict,
  ]) {
    for (const invalid of [null, undefined, [], {}, "bytes"]) {
      expect(() => decode(invalid as unknown as Uint8Array)).toThrow(
        "KELD-IPC-003",
      );
    }
  }
});

test("encoders reject invalid scalar types and range violations without coercion", () => {
  const hostile = {
    toString(): string {
      throw new Error("input coercion must not run");
    },
  };
  for (const value of [
    -1,
    0x1_0000_0000,
    0.5,
    NaN,
    Infinity,
    "7",
    7n,
    Symbol("id"),
    hostile,
    undefined,
    null,
  ]) {
    expect(() =>
      encodeWindowRef({ id: value, incarnation: 1n } as unknown as WindowRef),
    ).toThrow("KELD-IPC-003");
  }
  for (const value of [
    -1n,
    0x1_0000_0000_0000_0000n,
    7,
    "7",
    Symbol("sequence"),
    hostile,
    undefined,
    null,
  ]) {
    expect(() =>
      encodeWindowRef({ id: 1, incarnation: value } as unknown as WindowRef),
    ).toThrow("KELD-IPC-003");
    expect(() =>
      encodeWindowResponse({
        variant: "CloseRequested",
        close_seq: value,
      } as unknown as WindowResponse),
    ).toThrow("KELD-IPC-003");
  }
  for (const value of [
    -0x8000_0001,
    0x8000_0000,
    0.5,
    NaN,
    Infinity,
    "1",
    1n,
    Symbol("coordinate"),
    hostile,
    undefined,
    null,
  ]) {
    expect(() =>
      encodeRect({ ...BOUNDS, x: value } as unknown as Rect),
    ).toThrow("KELD-IPC-003");
  }
  for (const value of [0, 1, "true", null, undefined]) {
    expect(() =>
      encodeWindowState({ ...STATE, focused: value } as unknown as WindowState),
    ).toThrow("KELD-IPC-003");
    expect(() =>
      encodeWindowRequest({
        variant: "SetMaximized",
        window: WINDOW,
        maximized: value,
      } as unknown as WindowRequest),
    ).toThrow("KELD-IPC-003");
  }
  expect(() =>
    encodeWindowRequest({
      variant: "Create",
      width: 1,
      height: 1,
      y: null,
    } as unknown as WindowRequest),
  ).toThrow("KELD-IPC-003");
  expect(() =>
    encodeWindowEvent({
      variant: "Created",
      window: WINDOW,
      state: STATE,
    } as unknown as WindowEvent),
  ).toThrow("KELD-IPC-003");
});

test("encoders reject missing objects and unknown variants with the codec error", () => {
  for (const encode of [
    encodeWindowRef,
    encodeRect,
    encodeWindowState,
    encodeWindowEvent,
    encodeWindowRequest,
    encodeWindowResponse,
    encodeCloseVerdict,
  ]) {
    for (const invalid of [
      null,
      undefined,
      1,
      "Unknown",
      {},
      { variant: "Unknown" },
    ]) {
      expect(() => encode(invalid as never)).toThrow("KELD-IPC-003");
    }
  }
  expect(() =>
    encodeWindowEvent({
      variant: "Destroyed",
      window: null,
    } as unknown as WindowEvent),
  ).toThrow("KELD-IPC-003");
  expect(() =>
    encodeWindowRequest({
      variant: "CloseReply",
      window: WINDOW,
      close_seq: 1n,
      verdict: "Unknown",
    } as unknown as WindowRequest),
  ).toThrow("KELD-IPC-003");
});

test("u64 extrema remain bigint across every close-sequence surface", () => {
  for (const close_seq of [0n, 0xffff_ffff_ffff_ffffn]) {
    const request: WindowRequest = {
      variant: "CloseReply",
      window: WINDOW,
      close_seq,
      verdict: "Allow",
    };
    const event: WindowEvent = {
      variant: "CloseRequested",
      window: WINDOW,
      close_seq,
    };
    const replay: WindowEvent = {
      variant: "Created",
      window: WINDOW,
      state: STATE,
      pending_close: close_seq,
    };
    expect(decodeWindowRequest(encodeWindowRequest(request))).toEqual(request);
    expect(decodeWindowEvent(encodeWindowEvent(event))).toEqual(event);
    expect(decodeWindowEvent(encodeWindowEvent(replay))).toEqual(replay);
  }
});

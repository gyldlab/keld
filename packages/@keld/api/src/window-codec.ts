/**
 * Thin postcard adapter for keld-ipc/src/window.rs (approved GH-531 §4.a–b).
 * Integer parsing belongs to the canonical transport; the Rust-owned window-v0.tsv
 * pins field and variant order. These payloads grant no identity or authority:
 * the host registry must validate every supplied reference and operation.
 */
import {
  stateIsUint8Array,
  stateByteLength,
  decodeU64Varint,
  decodeVarint,
  encodeU64Varint,
  encodeVarint,
  kipcError,
} from "@keld/kipc";

/** Both host-minted identity halves, compared without numeric coercion. */
export interface WindowRef {
  id: number;
  incarnation: bigint;
}
/** Outer-frame bounds in logical points from the primary display's top-left. */
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}
/** Host-confirmed state; wire names follow the Rust owner. */
export interface WindowState {
  bounds: Rect;
  focused: boolean;
  maximized: boolean;
  full_screen: boolean;
}
/** Application verdict on one host-minted pending close sequence. */
export type CloseVerdict = "Allow" | "Veto";
/** Host-to-app facts, in the Rust enum's append-only declaration order. */
export type WindowEvent =
  | {
      variant: "Created";
      window: WindowRef;
      state: WindowState;
      pending_close: bigint | null;
    }
  | { variant: "Bounds"; window: WindowRef; bounds: Rect }
  | { variant: "Focus"; window: WindowRef; focused: boolean }
  | { variant: "Maximized"; window: WindowRef; maximized: boolean }
  | { variant: "FullScreen"; window: WindowRef; full_screen: boolean }
  | { variant: "CloseRequested"; window: WindowRef; close_seq: bigint }
  | { variant: "Destroyed"; window: WindowRef };
/** App-to-host calls; optional positions use explicit null, matching postcard. */
export type WindowRequest =
  | {
      variant: "Create";
      width: number;
      height: number;
      x: number | null;
      y: number | null;
    }
  | { variant: "SetMaximized"; window: WindowRef; maximized: boolean }
  | { variant: "SetFullScreen"; window: WindowRef; full_screen: boolean }
  | { variant: "RequestClose"; window: WindowRef }
  | {
      variant: "CloseReply";
      window: WindowRef;
      close_seq: bigint;
      verdict: CloseVerdict;
    }
  | { variant: "Destroy"; window: WindowRef };
/** Host replies; failures use the existing CallError payload instead. */
export type WindowResponse =
  | { variant: "Created"; window: WindowRef }
  | { variant: "Accepted" }
  | { variant: "CloseRequested"; close_seq: bigint }
  | { variant: "Merged"; close_seq: bigint }
  | { variant: "AlreadyClosing" }
  | { variant: "ReplyApplied" }
  | { variant: "ReplyStale" }
  | { variant: "Destroyed" }
  | { variant: "Adopted"; window: WindowRef };

type Decoder<T> = (bytes: Uint8Array, offset: number) => [T, number];
function invalid(detail: string): Error {
  return kipcError("KELD-IPC-003", detail);
}
function requireObject(value: unknown): void {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw invalid("window payload value must be an object");
  }
}
function join(parts: readonly Uint8Array[]): Uint8Array {
  const payload = new Uint8Array(
    parts.reduce((length, part) => length + part.byteLength, 0),
  );
  let offset = 0;
  for (const part of parts) {
    payload.set(part, offset);
    offset += part.byteLength;
  }
  return payload;
}
function exact<T>(payload: Uint8Array, decode: Decoder<T>): T {
  if (!stateIsUint8Array(payload))
    throw invalid("window payload must be a Uint8Array");
  const decoded1 = decode(payload, 0);
  const value = decoded1[0];
  const end = decoded1[1];
  if (end !== stateByteLength(payload))
    throw invalid("trailing bytes after window payload");
  return value;
}
function encodeI32(value: number): Uint8Array {
  if (
    typeof value !== "number" ||
    !Number.isInteger(value) ||
    value < -0x8000_0000 ||
    value > 0x7fff_ffff
  ) {
    throw invalid("window coordinate must be a signed 32-bit integer");
  }
  return encodeVarint(value < 0 ? -value * 2 - 1 : value * 2);
}
function decodeI32(bytes: Uint8Array, offset: number): [number, number] {
  const decoded2 = decodeVarint(bytes, offset);
  const value = decoded2[0];
  const end = decoded2[1];
  return [value % 2 === 0 ? value / 2 : -(value + 1) / 2, end];
}
function encodeBool(value: boolean): Uint8Array {
  if (typeof value !== "boolean")
    throw invalid("window flag must be a boolean");
  return Uint8Array.of(value ? 1 : 0);
}
function decodeBool(bytes: Uint8Array, offset: number): [boolean, number] {
  const value = bytes[offset];
  if (value !== 0 && value !== 1)
    throw invalid("invalid or truncated window boolean");
  return [value === 1, offset + 1];
}
function encodeOption<T>(
  value: T | null,
  encode: (value: T) => Uint8Array,
): Uint8Array {
  return value === null
    ? Uint8Array.of(0)
    : join([Uint8Array.of(1), encode(value)]);
}
function decodeOption<T>(
  bytes: Uint8Array,
  offset: number,
  decode: Decoder<T>,
): [T | null, number] {
  const tag = bytes[offset];
  if (tag === 0) return [null, offset + 1];
  if (tag !== 1) throw invalid("invalid or truncated window option");
  return decode(bytes, offset + 1);
}

/** Encode a reference without replacing the registry's ownership checks. */
export function encodeWindowRef(value: WindowRef): Uint8Array {
  requireObject(value);
  return join([encodeVarint(value.id), encodeU64Varint(value.incarnation)]);
}
function windowRefAt(bytes: Uint8Array, offset: number): [WindowRef, number] {
  const decoded3 = decodeVarint(bytes, offset);
  const id = decoded3[0];
  const afterId = decoded3[1];
  const decoded4 = decodeU64Varint(bytes, afterId);
  const incarnation = decoded4[0];
  const end = decoded4[1];
  return [{ id, incarnation }, end];
}
/** Decode exactly one reference, retaining the complete u64 incarnation. */
export function decodeWindowRef(payload: Uint8Array): WindowRef {
  return exact(payload, windowRefAt);
}
/** Encode signed coordinates and unsigned dimensions in Rust field order. */
export function encodeRect(value: Rect): Uint8Array {
  requireObject(value);
  return join([
    encodeI32(value.x),
    encodeI32(value.y),
    encodeVarint(value.width),
    encodeVarint(value.height),
  ]);
}
function rectAt(bytes: Uint8Array, offset: number): [Rect, number] {
  const decoded5 = decodeI32(bytes, offset);
  const x = decoded5[0];
  const afterX = decoded5[1];
  const decoded6 = decodeI32(bytes, afterX);
  const y = decoded6[0];
  const afterY = decoded6[1];
  const decoded7 = decodeVarint(bytes, afterY);
  const width = decoded7[0];
  const afterWidth = decoded7[1];
  const decoded8 = decodeVarint(bytes, afterWidth);
  const height = decoded8[0];
  const end = decoded8[1];
  return [{ x, y, width, height }, end];
}
/** Decode exactly one bounds payload, including both signed extrema. */
export function decodeRect(payload: Uint8Array): Rect {
  return exact(payload, rectAt);
}
/** Encode all independently confirmed native state flags. */
export function encodeWindowState(value: WindowState): Uint8Array {
  requireObject(value);
  return join([
    encodeRect(value.bounds),
    encodeBool(value.focused),
    encodeBool(value.maximized),
    encodeBool(value.full_screen),
  ]);
}
function stateAt(bytes: Uint8Array, offset: number): [WindowState, number] {
  const decoded9 = rectAt(bytes, offset);
  const bounds = decoded9[0];
  const afterBounds = decoded9[1];
  const decoded10 = decodeBool(bytes, afterBounds);
  const focused = decoded10[0];
  const afterFocus = decoded10[1];
  const decoded11 = decodeBool(bytes, afterFocus);
  const maximized = decoded11[0];
  const afterMaximized = decoded11[1];
  const decoded12 = decodeBool(bytes, afterMaximized);
  const full_screen = decoded12[0];
  const end = decoded12[1];
  return [{ bounds, focused, maximized, full_screen }, end];
}
/** Decode exactly one state payload; invalid booleans fail closed. */
export function decodeWindowState(payload: Uint8Array): WindowState {
  return exact(payload, stateAt);
}
/** Encode the Rust CloseVerdict discriminant. */
export function encodeCloseVerdict(value: CloseVerdict): Uint8Array {
  switch (value) {
    case "Allow":
      return encodeVarint(0);
    case "Veto":
      return encodeVarint(1);
    default:
      throw invalid("unknown CloseVerdict variant");
  }
}
function verdictAt(bytes: Uint8Array, offset: number): [CloseVerdict, number] {
  const [variant, end] = decodeVarint(bytes, offset);
  switch (variant) {
    case 0:
      return ["Allow", end];
    case 1:
      return ["Veto", end];
    default:
      throw invalid("unknown CloseVerdict discriminant");
  }
}
/** Decode exactly one close verdict, rejecting unknown discriminants. */
export function decodeCloseVerdict(payload: Uint8Array): CloseVerdict {
  return exact(payload, verdictAt);
}

/** Encode a host fact in the Rust enum's declaration order. */
export function encodeWindowEvent(value: WindowEvent): Uint8Array {
  requireObject(value);
  switch (value.variant) {
    case "Created":
      return join([
        encodeVarint(0),
        encodeWindowRef(value.window),
        encodeWindowState(value.state),
        encodeOption(value.pending_close, encodeU64Varint),
      ]);
    case "Bounds":
      return join([
        encodeVarint(1),
        encodeWindowRef(value.window),
        encodeRect(value.bounds),
      ]);
    case "Focus":
      return join([
        encodeVarint(2),
        encodeWindowRef(value.window),
        encodeBool(value.focused),
      ]);
    case "Maximized":
      return join([
        encodeVarint(3),
        encodeWindowRef(value.window),
        encodeBool(value.maximized),
      ]);
    case "FullScreen":
      return join([
        encodeVarint(4),
        encodeWindowRef(value.window),
        encodeBool(value.full_screen),
      ]);
    case "CloseRequested":
      return join([
        encodeVarint(5),
        encodeWindowRef(value.window),
        encodeU64Varint(value.close_seq),
      ]);
    case "Destroyed":
      return join([encodeVarint(6), encodeWindowRef(value.window)]);
    default:
      throw invalid("unknown WindowEvent variant");
  }
}
function eventAt(bytes: Uint8Array, offset: number): [WindowEvent, number] {
  const decoded14 = decodeVarint(bytes, offset);
  const variant = decoded14[0];
  const afterVariant = decoded14[1];
  if (variant > 6) throw invalid("unknown WindowEvent discriminant");
  const decoded15 = windowRefAt(bytes, afterVariant);
  const window = decoded15[0];
  const afterWindow = decoded15[1];
  switch (variant) {
    case 0: {
      const decoded16 = stateAt(bytes, afterWindow);
      const state = decoded16[0];
      const afterState = decoded16[1];
      const decoded17 = decodeOption(bytes, afterState, decodeU64Varint);
      const pending_close = decoded17[0];
      const end = decoded17[1];
      return [{ variant: "Created", window, state, pending_close }, end];
    }
    case 1: {
      const decoded18 = rectAt(bytes, afterWindow);
      const bounds = decoded18[0];
      const end = decoded18[1];
      return [{ variant: "Bounds", window, bounds }, end];
    }
    case 2: {
      const decoded19 = decodeBool(bytes, afterWindow);
      const focused = decoded19[0];
      const end = decoded19[1];
      return [{ variant: "Focus", window, focused }, end];
    }
    case 3: {
      const decoded20 = decodeBool(bytes, afterWindow);
      const maximized = decoded20[0];
      const end = decoded20[1];
      return [{ variant: "Maximized", window, maximized }, end];
    }
    case 4: {
      const decoded21 = decodeBool(bytes, afterWindow);
      const full_screen = decoded21[0];
      const end = decoded21[1];
      return [{ variant: "FullScreen", window, full_screen }, end];
    }
    case 5: {
      const decoded22 = decodeU64Varint(bytes, afterWindow);
      const close_seq = decoded22[0];
      const end = decoded22[1];
      return [{ variant: "CloseRequested", window, close_seq }, end];
    }
    case 6:
      return [{ variant: "Destroyed", window }, afterWindow];
    default:
      throw invalid("unknown WindowEvent discriminant");
  }
}
/** Decode exactly one host fact; unknown facts must never be silently ignored. */
export function decodeWindowEvent(payload: Uint8Array): WindowEvent {
  return exact(payload, eventAt);
}
/** Encode a request; host authorization and native state remain host-owned. */
export function encodeWindowRequest(value: WindowRequest): Uint8Array {
  requireObject(value);
  switch (value.variant) {
    case "Create":
      return join([
        encodeVarint(0),
        encodeVarint(value.width),
        encodeVarint(value.height),
        encodeOption(value.x, encodeI32),
        encodeOption(value.y, encodeI32),
      ]);
    case "SetMaximized":
      return join([
        encodeVarint(1),
        encodeWindowRef(value.window),
        encodeBool(value.maximized),
      ]);
    case "SetFullScreen":
      return join([
        encodeVarint(2),
        encodeWindowRef(value.window),
        encodeBool(value.full_screen),
      ]);
    case "RequestClose":
      return join([encodeVarint(3), encodeWindowRef(value.window)]);
    case "CloseReply":
      return join([
        encodeVarint(4),
        encodeWindowRef(value.window),
        encodeU64Varint(value.close_seq),
        encodeCloseVerdict(value.verdict),
      ]);
    case "Destroy":
      return join([encodeVarint(5), encodeWindowRef(value.window)]);
    default:
      throw invalid("unknown WindowRequest variant");
  }
}
function requestAt(bytes: Uint8Array, offset: number): [WindowRequest, number] {
  const [variant, afterVariant] = decodeVarint(bytes, offset);
  if (variant === 0) {
    const [width, afterWidth] = decodeVarint(bytes, afterVariant);
    const [height, afterHeight] = decodeVarint(bytes, afterWidth);
    const [x, afterX] = decodeOption(bytes, afterHeight, decodeI32);
    const [y, end] = decodeOption(bytes, afterX, decodeI32);
    return [{ variant: "Create", width, height, x, y }, end];
  }
  if (variant > 5) throw invalid("unknown WindowRequest discriminant");
  const [window, afterWindow] = windowRefAt(bytes, afterVariant);
  switch (variant) {
    case 1: {
      const [maximized, end] = decodeBool(bytes, afterWindow);
      return [{ variant: "SetMaximized", window, maximized }, end];
    }
    case 2: {
      const [full_screen, end] = decodeBool(bytes, afterWindow);
      return [{ variant: "SetFullScreen", window, full_screen }, end];
    }
    case 3:
      return [{ variant: "RequestClose", window }, afterWindow];
    case 4: {
      const [close_seq, afterSequence] = decodeU64Varint(bytes, afterWindow);
      const [verdict, end] = verdictAt(bytes, afterSequence);
      return [{ variant: "CloseReply", window, close_seq, verdict }, end];
    }
    case 5:
      return [{ variant: "Destroy", window }, afterWindow];
    default:
      throw invalid("unknown WindowRequest discriminant");
  }
}
/** Decode exactly one request without accepting trailing payload data. */
export function decodeWindowRequest(payload: Uint8Array): WindowRequest {
  return exact(payload, requestAt);
}
/** Encode the native result payload; ERR frames retain the CallError codec. */
export function encodeWindowResponse(value: WindowResponse): Uint8Array {
  requireObject(value);
  switch (value.variant) {
    case "Created":
      return join([encodeVarint(0), encodeWindowRef(value.window)]);
    case "Accepted":
      return encodeVarint(1);
    case "CloseRequested":
      return join([encodeVarint(2), encodeU64Varint(value.close_seq)]);
    case "Merged":
      return join([encodeVarint(3), encodeU64Varint(value.close_seq)]);
    case "AlreadyClosing":
      return encodeVarint(4);
    case "ReplyApplied":
      return encodeVarint(5);
    case "ReplyStale":
      return encodeVarint(6);
    case "Destroyed":
      return encodeVarint(7);
    case "Adopted":
      return join([encodeVarint(8), encodeWindowRef(value.window)]);
    default:
      throw invalid("unknown WindowResponse variant");
  }
}
function responseAt(
  bytes: Uint8Array,
  offset: number,
): [WindowResponse, number] {
  const [variant, afterVariant] = decodeVarint(bytes, offset);
  switch (variant) {
    case 0: {
      const [window, end] = windowRefAt(bytes, afterVariant);
      return [{ variant: "Created", window }, end];
    }
    case 1:
      return [{ variant: "Accepted" }, afterVariant];
    case 2: {
      const [close_seq, end] = decodeU64Varint(bytes, afterVariant);
      return [{ variant: "CloseRequested", close_seq }, end];
    }
    case 3: {
      const [close_seq, end] = decodeU64Varint(bytes, afterVariant);
      return [{ variant: "Merged", close_seq }, end];
    }
    case 4:
      return [{ variant: "AlreadyClosing" }, afterVariant];
    case 5:
      return [{ variant: "ReplyApplied" }, afterVariant];
    case 6:
      return [{ variant: "ReplyStale" }, afterVariant];
    case 7:
      return [{ variant: "Destroyed" }, afterVariant];
    case 8: {
      const [window, end] = windowRefAt(bytes, afterVariant);
      return [{ variant: "Adopted", window }, end];
    }
    default:
      throw invalid("unknown WindowResponse discriminant");
  }
}
/** Decode exactly one reply, retaining all close sequence bits. */
export function decodeWindowResponse(payload: Uint8Array): WindowResponse {
  return exact(payload, responseAt);
}

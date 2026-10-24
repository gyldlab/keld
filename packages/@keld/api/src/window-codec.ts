/**
 * Thin postcard adapter for keld-ipc/src/window.rs (approved GH-531 §4.a–b).
 * Integer parsing belongs to the canonical transport; the Rust-owned window-v0.tsv
 * pins field and variant order. These payloads grant no identity or authority:
 * the host registry must validate every supplied reference and operation.
 */
import {
  decodeU64Varint, decodeVarint, encodeU64Varint, encodeVarint, kipcError,
} from "../../kipc/src/transport.ts";

/** Both host-minted identity halves, compared without numeric coercion. */
export interface WindowRef { id: number; incarnation: bigint }
/** Outer-frame bounds in logical points from the primary display's top-left. */
export interface Rect { x: number; y: number; width: number; height: number }
/** Host-confirmed state; wire names follow the Rust owner. */
export interface WindowState { bounds: Rect; focused: boolean; maximized: boolean; full_screen: boolean }
/** Application verdict on one host-minted pending close sequence. */
export type CloseVerdict = "Allow" | "Veto";
/** Host-to-app facts, in the Rust enum's append-only declaration order. */
export type WindowEvent =
  | { variant: "Created"; window: WindowRef; state: WindowState; pending_close: bigint | null }
  | { variant: "Bounds"; window: WindowRef; bounds: Rect }
  | { variant: "Focus"; window: WindowRef; focused: boolean }
  | { variant: "Maximized"; window: WindowRef; maximized: boolean }
  | { variant: "FullScreen"; window: WindowRef; full_screen: boolean }
  | { variant: "CloseRequested"; window: WindowRef; close_seq: bigint }
  | { variant: "Destroyed"; window: WindowRef };
/** App-to-host calls; optional positions use explicit null, matching postcard. */
export type WindowRequest =
  | { variant: "Create"; width: number; height: number; x: number | null; y: number | null }
  | { variant: "SetMaximized"; window: WindowRef; maximized: boolean }
  | { variant: "SetFullScreen"; window: WindowRef; full_screen: boolean }
  | { variant: "RequestClose"; window: WindowRef }
  | { variant: "CloseReply"; window: WindowRef; close_seq: bigint; verdict: CloseVerdict }
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
function invalid(detail: string): Error { return kipcError("KELD-IPC-003", detail); }
function requireObject(value: unknown): void {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw invalid("window payload value must be an object");
  }
}
function join(parts: readonly Uint8Array[]): Uint8Array {
  const payload = new Uint8Array(parts.reduce((length, part) => length + part.byteLength, 0));
  let offset = 0;
  for (const part of parts) { payload.set(part, offset); offset += part.byteLength; }
  return payload;
}
function exact<T>(payload: Uint8Array, decode: Decoder<T>): T {
  if (!(payload instanceof Uint8Array)) throw invalid("window payload must be a Uint8Array");
  const [value, end] = decode(payload, 0);
  if (end !== payload.byteLength) throw invalid("trailing bytes after window payload");
  return value;
}
function encodeI32(value: number): Uint8Array {
  if (typeof value !== "number" || !Number.isInteger(value) || value < -0x8000_0000 || value > 0x7fff_ffff) {
    throw invalid("window coordinate must be a signed 32-bit integer");
  }
  return encodeVarint(value < 0 ? -value * 2 - 1 : value * 2);
}
function decodeI32(bytes: Uint8Array, offset: number): [number, number] {
  const [value, end] = decodeVarint(bytes, offset);
  return [value % 2 === 0 ? value / 2 : -(value + 1) / 2, end];
}
function encodeBool(value: boolean): Uint8Array {
  if (typeof value !== "boolean") throw invalid("window flag must be a boolean");
  return Uint8Array.of(value ? 1 : 0);
}
function decodeBool(bytes: Uint8Array, offset: number): [boolean, number] {
  const value = bytes[offset];
  if (value !== 0 && value !== 1) throw invalid("invalid or truncated window boolean");
  return [value === 1, offset + 1];
}
function encodeOption<T>(value: T | null, encode: (value: T) => Uint8Array): Uint8Array {
  return value === null ? Uint8Array.of(0) : join([Uint8Array.of(1), encode(value)]);
}
function decodeOption<T>(bytes: Uint8Array, offset: number, decode: Decoder<T>): [T | null, number] {
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
  const [id, afterId] = decodeVarint(bytes, offset);
  const [incarnation, end] = decodeU64Varint(bytes, afterId);
  return [{ id, incarnation }, end];
}
/** Decode exactly one reference, retaining the complete u64 incarnation. */
export function decodeWindowRef(payload: Uint8Array): WindowRef { return exact(payload, windowRefAt); }
/** Encode signed coordinates and unsigned dimensions in Rust field order. */
export function encodeRect(value: Rect): Uint8Array {
  requireObject(value);
  return join([encodeI32(value.x), encodeI32(value.y), encodeVarint(value.width), encodeVarint(value.height)]);
}
function rectAt(bytes: Uint8Array, offset: number): [Rect, number] {
  const [x, afterX] = decodeI32(bytes, offset);
  const [y, afterY] = decodeI32(bytes, afterX);
  const [width, afterWidth] = decodeVarint(bytes, afterY);
  const [height, end] = decodeVarint(bytes, afterWidth);
  return [{ x, y, width, height }, end];
}
/** Decode exactly one bounds payload, including both signed extrema. */
export function decodeRect(payload: Uint8Array): Rect { return exact(payload, rectAt); }
/** Encode all independently confirmed native state flags. */
export function encodeWindowState(value: WindowState): Uint8Array {
  requireObject(value);
  return join([encodeRect(value.bounds), encodeBool(value.focused), encodeBool(value.maximized), encodeBool(value.full_screen)]);
}
function stateAt(bytes: Uint8Array, offset: number): [WindowState, number] {
  const [bounds, afterBounds] = rectAt(bytes, offset);
  const [focused, afterFocus] = decodeBool(bytes, afterBounds);
  const [maximized, afterMaximized] = decodeBool(bytes, afterFocus);
  const [full_screen, end] = decodeBool(bytes, afterMaximized);
  return [{ bounds, focused, maximized, full_screen }, end];
}
/** Decode exactly one state payload; invalid booleans fail closed. */
export function decodeWindowState(payload: Uint8Array): WindowState { return exact(payload, stateAt); }
/** Encode the Rust CloseVerdict discriminant. */
export function encodeCloseVerdict(value: CloseVerdict): Uint8Array {
  switch (value) {
    case "Allow": return encodeVarint(0);
    case "Veto": return encodeVarint(1);
    default: throw invalid("unknown CloseVerdict variant");
  }
}
function verdictAt(bytes: Uint8Array, offset: number): [CloseVerdict, number] {
  const [variant, end] = decodeVarint(bytes, offset);
  switch (variant) {
    case 0: return ["Allow", end];
    case 1: return ["Veto", end];
    default: throw invalid("unknown CloseVerdict discriminant");
  }
}
/** Decode exactly one close verdict, rejecting unknown discriminants. */
export function decodeCloseVerdict(payload: Uint8Array): CloseVerdict { return exact(payload, verdictAt); }

/** Encode a host fact in the Rust enum's declaration order. */
export function encodeWindowEvent(value: WindowEvent): Uint8Array {
  requireObject(value);
  switch (value.variant) {
    case "Created": return join([encodeVarint(0), encodeWindowRef(value.window), encodeWindowState(value.state), encodeOption(value.pending_close, encodeU64Varint)]);
    case "Bounds": return join([encodeVarint(1), encodeWindowRef(value.window), encodeRect(value.bounds)]);
    case "Focus": return join([encodeVarint(2), encodeWindowRef(value.window), encodeBool(value.focused)]);
    case "Maximized": return join([encodeVarint(3), encodeWindowRef(value.window), encodeBool(value.maximized)]);
    case "FullScreen": return join([encodeVarint(4), encodeWindowRef(value.window), encodeBool(value.full_screen)]);
    case "CloseRequested": return join([encodeVarint(5), encodeWindowRef(value.window), encodeU64Varint(value.close_seq)]);
    case "Destroyed": return join([encodeVarint(6), encodeWindowRef(value.window)]);
    default: throw invalid("unknown WindowEvent variant");
  }
}
function eventAt(bytes: Uint8Array, offset: number): [WindowEvent, number] {
  const [variant, afterVariant] = decodeVarint(bytes, offset);
  if (variant > 6) throw invalid("unknown WindowEvent discriminant");
  const [window, afterWindow] = windowRefAt(bytes, afterVariant);
  switch (variant) {
    case 0: {
      const [state, afterState] = stateAt(bytes, afterWindow);
      const [pending_close, end] = decodeOption(bytes, afterState, decodeU64Varint);
      return [{ variant: "Created", window, state, pending_close }, end];
    }
    case 1: {
      const [bounds, end] = rectAt(bytes, afterWindow);
      return [{ variant: "Bounds", window, bounds }, end];
    }
    case 2: {
      const [focused, end] = decodeBool(bytes, afterWindow);
      return [{ variant: "Focus", window, focused }, end];
    }
    case 3: {
      const [maximized, end] = decodeBool(bytes, afterWindow);
      return [{ variant: "Maximized", window, maximized }, end];
    }
    case 4: {
      const [full_screen, end] = decodeBool(bytes, afterWindow);
      return [{ variant: "FullScreen", window, full_screen }, end];
    }
    case 5: {
      const [close_seq, end] = decodeU64Varint(bytes, afterWindow);
      return [{ variant: "CloseRequested", window, close_seq }, end];
    }
    case 6: return [{ variant: "Destroyed", window }, afterWindow];
    default: throw invalid("unknown WindowEvent discriminant");
  }
}
/** Decode exactly one host fact; unknown facts must never be silently ignored. */
export function decodeWindowEvent(payload: Uint8Array): WindowEvent { return exact(payload, eventAt); }
/** Encode a request; host authorization and native state remain host-owned. */
export function encodeWindowRequest(value: WindowRequest): Uint8Array {
  requireObject(value);
  switch (value.variant) {
    case "Create": return join([encodeVarint(0), encodeVarint(value.width), encodeVarint(value.height), encodeOption(value.x, encodeI32), encodeOption(value.y, encodeI32)]);
    case "SetMaximized": return join([encodeVarint(1), encodeWindowRef(value.window), encodeBool(value.maximized)]);
    case "SetFullScreen": return join([encodeVarint(2), encodeWindowRef(value.window), encodeBool(value.full_screen)]);
    case "RequestClose": return join([encodeVarint(3), encodeWindowRef(value.window)]);
    case "CloseReply": return join([encodeVarint(4), encodeWindowRef(value.window), encodeU64Varint(value.close_seq), encodeCloseVerdict(value.verdict)]);
    case "Destroy": return join([encodeVarint(5), encodeWindowRef(value.window)]);
    default: throw invalid("unknown WindowRequest variant");
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
    case 3: return [{ variant: "RequestClose", window }, afterWindow];
    case 4: {
      const [close_seq, afterSequence] = decodeU64Varint(bytes, afterWindow);
      const [verdict, end] = verdictAt(bytes, afterSequence);
      return [{ variant: "CloseReply", window, close_seq, verdict }, end];
    }
    case 5: return [{ variant: "Destroy", window }, afterWindow];
    default: throw invalid("unknown WindowRequest discriminant");
  }
}
/** Decode exactly one request without accepting trailing payload data. */
export function decodeWindowRequest(payload: Uint8Array): WindowRequest { return exact(payload, requestAt); }
/** Encode the native result payload; ERR frames retain the CallError codec. */
export function encodeWindowResponse(value: WindowResponse): Uint8Array {
  requireObject(value);
  switch (value.variant) {
    case "Created": return join([encodeVarint(0), encodeWindowRef(value.window)]);
    case "Accepted": return encodeVarint(1);
    case "CloseRequested": return join([encodeVarint(2), encodeU64Varint(value.close_seq)]);
    case "Merged": return join([encodeVarint(3), encodeU64Varint(value.close_seq)]);
    case "AlreadyClosing": return encodeVarint(4);
    case "ReplyApplied": return encodeVarint(5);
    case "ReplyStale": return encodeVarint(6);
    case "Destroyed": return encodeVarint(7);
    case "Adopted": return join([encodeVarint(8), encodeWindowRef(value.window)]);
    default: throw invalid("unknown WindowResponse variant");
  }
}
function responseAt(bytes: Uint8Array, offset: number): [WindowResponse, number] {
  const [variant, afterVariant] = decodeVarint(bytes, offset);
  switch (variant) {
    case 0: {
      const [window, end] = windowRefAt(bytes, afterVariant);
      return [{ variant: "Created", window }, end];
    }
    case 1: return [{ variant: "Accepted" }, afterVariant];
    case 2: {
      const [close_seq, end] = decodeU64Varint(bytes, afterVariant);
      return [{ variant: "CloseRequested", close_seq }, end];
    }
    case 3: {
      const [close_seq, end] = decodeU64Varint(bytes, afterVariant);
      return [{ variant: "Merged", close_seq }, end];
    }
    case 4: return [{ variant: "AlreadyClosing" }, afterVariant];
    case 5: return [{ variant: "ReplyApplied" }, afterVariant];
    case 6: return [{ variant: "ReplyStale" }, afterVariant];
    case 7: return [{ variant: "Destroyed" }, afterVariant];
    case 8: {
      const [window, end] = windowRefAt(bytes, afterVariant);
      return [{ variant: "Adopted", window }, end];
    }
    default: throw invalid("unknown WindowResponse discriminant");
  }
}
/** Decode exactly one reply, retaining all close sequence bits. */
export function decodeWindowResponse(payload: Uint8Array): WindowResponse { return exact(payload, responseAt); }

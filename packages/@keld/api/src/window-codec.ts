/**
 * Thin postcard adapter for keld-ipc/src/window.rs (approved GH-531 §4.a–b).
 * Integer parsing belongs to the canonical transport; the Rust-owned window-v0.tsv
 * pins field and variant order. These payloads grant no identity or authority:
 * the host registry must validate every supplied reference and operation.
 */
import {
  stateApply,
  stateDefineProperty,
  stateIsArray,
  stateFloat64At,
  stateMap,
  stateMapHas,
  stateMapSet,
  stateIsUint8Array,
  stateByteLength,
  decodeU64Varint,
  decodeVarint,
  encodePostcardString,
  decodePostcardStringAt,
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
/** One native monitor. Geometry is checked integer DIP anchored to primary frame. */
export interface DesktopFact {
  id: number;
  bounds: Rect;
  work_area: Rect;
  scale_factor: number;
}
/** Current, complete native display/theme observation; no authority or OS handle. */
export interface DesktopFacts {
  primary_id: number;
  displays: DesktopFact[];
  should_use_dark_colors: boolean;
}
/** Counts the entire postcard WindowEvent body, excluding the KIPC header. */
export const MAX_DESKTOP_EVENT_BYTES = 32 * 1024;
const MAX_U64 = 0xffff_ffff_ffff_ffffn;
const NativeDataView = DataView;
const writeFloat64 = DataView.prototype.setFloat64;
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
  | { variant: "Destroyed"; window: WindowRef }
  | {
      /** GH481: appended postcard tag 7, no WindowRef on this global fact. */
      variant: "DesktopSnapshot";
      epoch: bigint;
      snapshot: DesktopFacts | null;
    }
  | WindowEventExtension;
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
  | { variant: "Destroy"; window: WindowRef }
  | WindowRequestExtension;
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
  | { variant: "Adopted"; window: WindowRef }
  | WindowResponseExtension;

type Decoder<T> = (bytes: Uint8Array, offset: number) => [T, number];
function invalid(detail: string): Error {
  return kipcError("KELD-IPC-003", detail);
}
function requireObject(value: unknown): void {
  if (value === null || typeof value !== "object" || stateIsArray(value)) {
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

function checkedU32(value: number): boolean {
  return (
    typeof value === "number" &&
    value % 1 === 0 &&
    value >= 0 &&
    value <= 0xffff_ffff
  );
}
function validateDesktopRect(value: Rect): void {
  requireObject(value);
  if (
    typeof value.x !== "number" ||
    value.x % 1 !== 0 ||
    value.x < -0x8000_0000 ||
    value.x > 0x7fff_ffff ||
    typeof value.y !== "number" ||
    value.y % 1 !== 0 ||
    value.y < -0x8000_0000 ||
    value.y > 0x7fff_ffff ||
    !checkedU32(value.width) ||
    value.width === 0 ||
    !checkedU32(value.height) ||
    value.height === 0
  ) {
    throw invalid(
      "desktop rectangles need representable positive logical dimensions",
    );
  }
}
/** Same validation owner for directly applied facts and decoded postcard facts. */
export function validateDesktopFacts(value: DesktopFacts): void {
  requireObject(value);
  if (!checkedU32(value.primary_id) || value.primary_id === 0)
    throw invalid("desktop primary id must be a nonzero u32");
  if (
    !stateIsArray(value.displays) ||
    value.displays.length === 0 ||
    value.displays.length > 32
  ) {
    throw invalid("desktop requires between one and 32 displays");
  }
  if (typeof value.should_use_dark_colors !== "boolean")
    throw invalid("desktop appearance must be a boolean");
  const observed = stateMap<number, true>();
  let matches = 0;
  for (let i = 0; i < value.displays.length; i += 1) {
    const display = value.displays[i]!;
    requireObject(display);
    if (
      !checkedU32(display.id) ||
      display.id === 0 ||
      stateMapHas(observed, display.id)
    )
      throw invalid("desktop display ids must be unique, nonzero u32");
    stateMapSet(observed, display.id, true);
    if (display.id === value.primary_id) matches += 1;
    validateDesktopRect(display.bounds);
    validateDesktopRect(display.work_area);
    if (
      typeof display.scale_factor !== "number" ||
      !(display.scale_factor > 0 && display.scale_factor < Infinity)
    )
      throw invalid("desktop scale factor must be finite and positive");
  }
  if (matches !== 1)
    throw invalid("desktop primary id must match exactly one display");
}
export function validateDesktopEpoch(value: bigint): void {
  if (
    typeof value !== "bigint" ||
    value === 0n ||
    value > MAX_U64 ||
    value < 0n
  )
    throw invalid("desktop epoch must be a nonzero u64");
}
function encodeFloat64(value: number): Uint8Array {
  if (!Number.isFinite(value) || value <= 0)
    throw invalid("desktop scale factor must be finite and positive");
  const bytes = new Uint8Array(8);
  stateApply(writeFloat64, new NativeDataView(bytes.buffer), [0, value, true]);
  return bytes;
}
function float64At(bytes: Uint8Array, offset: number): [number, number] {
  if (offset + 8 > stateByteLength(bytes))
    throw invalid("truncated desktop scale factor");
  const value = stateFloat64At(bytes, offset);
  if (!(value > 0 && value < Infinity))
    throw invalid("desktop scale factor must be finite and positive");
  return [value, offset + 8];
}
function encodeDesktopFacts(value: DesktopFacts): Uint8Array {
  validateDesktopFacts(value);
  const displays: Uint8Array[] = [encodeVarint(value.displays.length)];
  for (let i = 0; i < value.displays.length; i += 1) {
    const display = value.displays[i]!;
    displays.push(
      encodeVarint(display.id),
      encodeRect(display.bounds),
      encodeRect(display.work_area),
      encodeFloat64(display.scale_factor),
    );
  }
  return join([
    encodeVarint(value.primary_id),
    ...displays,
    encodeBool(value.should_use_dark_colors),
  ]);
}
function desktopFactsAt(
  bytes: Uint8Array,
  offset: number,
): [DesktopFacts, number] {
  const primary = decodeVarint(bytes, offset);
  const primary_id = primary[0];
  const countField = decodeVarint(bytes, primary[1]);
  const count = countField[0];
  if (count === 0 || count > 32)
    throw invalid("desktop requires between one and 32 displays");
  const displays: DesktopFact[] = [];
  let cursor = countField[1];
  for (let index = 0; index < count; index += 1) {
    const id = decodeVarint(bytes, cursor);
    const bounds = rectAt(bytes, id[1]);
    const work = rectAt(bytes, bounds[1]);
    const scale = float64At(bytes, work[1]);
    stateDefineProperty(displays, index, {
      __proto__: null,
      value: {
        id: id[0],
        bounds: bounds[0],
        work_area: work[0],
        scale_factor: scale[0],
      },
      enumerable: true,
      configurable: true,
      writable: true,
    } as PropertyDescriptor);
    cursor = scale[1];
  }
  const appearance = decodeBool(bytes, cursor);
  const value = { primary_id, displays, should_use_dark_colors: appearance[0] };
  validateDesktopFacts(value);
  return [value, appearance[1]];
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
    case "DesktopSnapshot": {
      validateDesktopEpoch(value.epoch);
      const bytes = join([
        encodeVarint(7),
        encodeU64Varint(value.epoch),
        encodeOption(value.snapshot, encodeDesktopFacts),
      ]);
      if (bytes.byteLength > MAX_DESKTOP_EVENT_BYTES)
        throw invalid("desktop WindowEvent exceeds the 32 KiB budget");
      return bytes;
    }
    default:
      return encodeWindowEventExtension(value);
  }
}
function eventAt(bytes: Uint8Array, offset: number): [WindowEvent, number] {
  const decoded14 = decodeVarint(bytes, offset);
  const variant = decoded14[0];
  const afterVariant = decoded14[1];
  if (variant >= 8) return windowEventExtensionAt(bytes, offset);
  if (variant === 7) {
    if (stateByteLength(bytes) > MAX_DESKTOP_EVENT_BYTES)
      throw invalid("desktop WindowEvent exceeds the 32 KiB budget");
    const decodedEpoch = decodeU64Varint(bytes, afterVariant);
    const epoch = decodedEpoch[0];
    validateDesktopEpoch(epoch);
    const decodedSnapshot = decodeOption(
      bytes,
      decodedEpoch[1],
      desktopFactsAt,
    );
    return [
      { variant: "DesktopSnapshot", epoch, snapshot: decodedSnapshot[0] },
      decodedSnapshot[1],
    ];
  }
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
function encodeWindowRequestUnchecked(value: WindowRequest): Uint8Array {
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
      return encodeWindowRequestExtension(value);
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
  if (variant >= 6) return windowRequestExtensionAt(bytes, offset);
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
  if (!stateIsUint8Array(payload))
    throw invalid("window payload must be a Uint8Array");
  if (payload.byteLength > 8192)
    throw invalid("window call exceeds its encoded body bound");
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
      return encodeWindowResponseExtension(value);
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
      return windowResponseExtensionAt(bytes, offset);
  }
}
/** Decode exactly one reply, retaining all close sequence bits. */
export function decodeWindowResponse(payload: Uint8Array): WindowResponse {
  return exact(payload, responseAt);
}

// Use canonical postcard string codecs; these checks do not authenticate a path.
function encodePreloadString(
  value: string,
  maximum: number,
  nonempty: boolean,
): Uint8Array {
  // For valid Unicode, UTF-8 byte length is at least UTF-16 code-unit length.
  // Reject this necessary bound before canonical validation/UTF-8 allocation.
  if (
    typeof value !== "string" ||
    value.length > maximum ||
    (nonempty && value.length === 0)
  )
    throw invalid("preload metadata exceeds its UTF-8 byte bound");
  const bytes = encodePostcardString(value, "preload metadata");
  const length = decodeVarint(bytes, 0)[0];
  if (length > maximum || (nonempty && length === 0))
    throw invalid("preload metadata exceeds its UTF-8 byte bound");
  return bytes;
}
function preloadStringAt(
  bytes: Uint8Array,
  offset: number,
  maximum: number,
  nonempty: boolean,
): [string, number] {
  const length = decodeVarint(bytes, offset)[0];
  if (length > maximum || (nonempty && length === 0))
    throw invalid("preload metadata exceeds its UTF-8 byte bound");
  return decodePostcardStringAt(bytes, offset);
}

/** Full existing Window CALL budget, including tag, reference and metadata. */
export function encodeWindowRequest(value: WindowRequest): Uint8Array {
  const bytes = encodeWindowRequestUnchecked(value);
  if (bytes.byteLength > 8192)
    throw invalid("window call exceeds its encoded body bound");
  return bytes;
}
function encodePreloadArguments(args: readonly string[]): Uint8Array {
  if (!stateIsArray(args) || args.length > 16)
    throw invalid("too many preload arguments");
  const parts = [encodeVarint(args.length)];
  let total = 0;
  for (let index = 0; index < args.length; index += 1) {
    const bytes = encodePreloadString(args[index], 256, false);
    total += decodeVarint(bytes, 0)[0];
    if (total > 2048)
      throw invalid("preload arguments exceed their byte bound");
    parts.push(bytes);
  }
  return join(parts);
}
function preloadArgumentsAt(
  bytes: Uint8Array,
  offset: number,
): [string[], number] {
  const [count, afterCount] = decodeVarint(bytes, offset);
  if (count > 16) throw invalid("too many preload arguments");
  const args: string[] = [];
  let at = afterCount,
    total = 0;
  for (let index = 0; index < count; index += 1) {
    total += decodeVarint(bytes, at)[0];
    if (total > 2048)
      throw invalid("preload arguments exceed their byte bound");
    const [value, next] = preloadStringAt(bytes, at, 256, false);
    stateDefineProperty(args, index, {
      __proto__: null,
      value,
      writable: true,
      enumerable: true,
      configurable: true,
    } as PropertyDescriptor);
    at = next;
  }
  return [args, at];
}
function encodeNativeDocumentGeneration(value: number): Uint8Array {
  if (!Number.isInteger(value) || value <= 0 || value > 0xffffffff)
    throw invalid("native document generation must be a nonzero u32");
  return encodeVarint(value);
}
function nativeDocumentGenerationAt(
  bytes: Uint8Array,
  offset: number,
): [number, number] {
  const decoded = decodeVarint(bytes, offset);
  if (decoded[0] === 0)
    throw invalid("native document generation must be nonzero");
  return decoded;
}
// @keld/window-n1-generated-begin
// @generated by echo-codegen.ts from keld-ipc/src/window.rs; do not edit.
type WindowRequestExtension =
  | {
      variant: "CreateWithPreload";
      width: number;
      height: number;
      x: number | null;
      y: number | null;
      preload_absolute: string;
      additional_arguments: readonly string[];
    }
  | { variant: "LoadUrl"; window: WindowRef; uri: string }
  | {
      variant: "DestroyOriginalDocument";
      window: WindowRef;
      document_generation: number;
    };
function encodeWindowRequestExtension(
  value: WindowRequestExtension,
): Uint8Array {
  switch (value.variant) {
    case "CreateWithPreload":
      return join([
        encodeVarint(6),
        encodeVarint(value.width),
        encodeVarint(value.height),
        encodeOption(value.x, encodeI32),
        encodeOption(value.y, encodeI32),
        encodePreloadString(value.preload_absolute, 2048, true),
        encodePreloadArguments(value.additional_arguments),
      ]);
    case "LoadUrl":
      return join([
        encodeVarint(7),
        encodeWindowRef(value.window),
        encodePreloadString(value.uri, 8192, true),
      ]);
    case "DestroyOriginalDocument":
      return join([
        encodeVarint(8),
        encodeWindowRef(value.window),
        encodeNativeDocumentGeneration(value.document_generation),
      ]);
    default:
      throw invalid("unknown WindowRequest extension");
  }
}
function windowRequestExtensionAt(
  wireBytes: Uint8Array,
  offset: number,
): [WindowRequestExtension, number] {
  const [tag, afterTag] = decodeVarint(wireBytes, offset);
  switch (tag) {
    case 6: {
      let at = afterTag;
      const [width, next_width] = decodeVarint(wireBytes, at);
      at = next_width;
      const [height, next_height] = decodeVarint(wireBytes, at);
      at = next_height;
      const [x, next_x] = decodeOption(wireBytes, at, decodeI32);
      at = next_x;
      const [y, next_y] = decodeOption(wireBytes, at, decodeI32);
      at = next_y;
      const [preload_absolute, next_preload_absolute] = preloadStringAt(
        wireBytes,
        at,
        2048,
        true,
      );
      at = next_preload_absolute;
      const [additional_arguments, next_additional_arguments] =
        preloadArgumentsAt(wireBytes, at);
      at = next_additional_arguments;
      return [
        {
          variant: "CreateWithPreload",
          width,
          height,
          x,
          y,
          preload_absolute,
          additional_arguments,
        },
        at,
      ];
    }
    case 7: {
      let at = afterTag;
      const [window, next_window] = windowRefAt(wireBytes, at);
      at = next_window;
      const [uri, next_uri] = preloadStringAt(wireBytes, at, 8192, true);
      at = next_uri;
      return [{ variant: "LoadUrl", window, uri }, at];
    }
    case 8: {
      let at = afterTag;
      const [window, next_window] = windowRefAt(wireBytes, at);
      at = next_window;
      const [document_generation, next_document_generation] =
        nativeDocumentGenerationAt(wireBytes, at);
      at = next_document_generation;
      return [
        { variant: "DestroyOriginalDocument", window, document_generation },
        at,
      ];
    }
    default:
      throw invalid("unknown WindowRequest extension tag");
  }
}
type WindowResponseExtension = { variant: "Loaded" };
function encodeWindowResponseExtension(
  value: WindowResponseExtension,
): Uint8Array {
  switch (value.variant) {
    case "Loaded":
      return join([encodeVarint(9)]);
    default:
      throw invalid("unknown WindowResponse extension");
  }
}
function windowResponseExtensionAt(
  wireBytes: Uint8Array,
  offset: number,
): [WindowResponseExtension, number] {
  const [tag, afterTag] = decodeVarint(wireBytes, offset);
  switch (tag) {
    case 9: {
      let at = afterTag;
      return [{ variant: "Loaded" }, at];
    }
    default:
      throw invalid("unknown WindowResponse extension tag");
  }
}
type WindowEventExtension =
  | {
      variant: "NavigationCommitted";
      window: WindowRef;
      document_generation: number;
      native_url: string;
    }
  | {
      variant: "NavigationFinished";
      window: WindowRef;
      document_generation: number;
    };
function encodeWindowEventExtension(value: WindowEventExtension): Uint8Array {
  switch (value.variant) {
    case "NavigationCommitted":
      return join([
        encodeVarint(8),
        encodeWindowRef(value.window),
        encodeNativeDocumentGeneration(value.document_generation),
        encodePreloadString(value.native_url, 8192, true),
      ]);
    case "NavigationFinished":
      return join([
        encodeVarint(9),
        encodeWindowRef(value.window),
        encodeNativeDocumentGeneration(value.document_generation),
      ]);
    default:
      throw invalid("unknown WindowEvent extension");
  }
}
function windowEventExtensionAt(
  wireBytes: Uint8Array,
  offset: number,
): [WindowEventExtension, number] {
  const [tag, afterTag] = decodeVarint(wireBytes, offset);
  switch (tag) {
    case 8: {
      let at = afterTag;
      const [window, next_window] = windowRefAt(wireBytes, at);
      at = next_window;
      const [document_generation, next_document_generation] =
        nativeDocumentGenerationAt(wireBytes, at);
      at = next_document_generation;
      const [native_url, next_native_url] = preloadStringAt(
        wireBytes,
        at,
        8192,
        true,
      );
      at = next_native_url;
      return [
        {
          variant: "NavigationCommitted",
          window,
          document_generation,
          native_url,
        },
        at,
      ];
    }
    case 9: {
      let at = afterTag;
      const [window, next_window] = windowRefAt(wireBytes, at);
      at = next_window;
      const [document_generation, next_document_generation] =
        nativeDocumentGenerationAt(wireBytes, at);
      at = next_document_generation;
      return [
        { variant: "NavigationFinished", window, document_generation },
        at,
      ];
    }
    default:
      throw invalid("unknown WindowEvent extension tag");
  }
}
// @keld/window-n1-generated-end

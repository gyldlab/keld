/**
 * GH-1186 executable contract proposal; not imported by the shipping Host.
 * Native facts enter the pinned GH-481 codec/WindowSource. No mirror is defined here.
 */
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { promisify } from "node:util";

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}
export interface NativePath {
  sourceAdapterLuid: { lowPart: number; highPart: number };
  sourceId: number;
  targetAdapterLuid: { lowPart: number; highPart: number };
  targetId: number;
  targetDevicePath: string;
}
export interface NativeDisplay {
  sourceName: string;
  observedHandleHex: string;
  primary: boolean;
  bounds: Rect;
  workArea: Rect;
  dpi: { x: number; y: number; deviceScaleFactor: number };
  physicalMode: { width: number; height: number };
  nativePaths: NativePath[];
}
export interface NativeSample {
  schemaVersion: number;
  status: string;
  processId: number;
  machine: { name: string; architecture: string; os: { build: number } };
  dpiContext: { exactRequestedContext: boolean; coordinates: string };
  cleanup: {
    ownedWindowsCreated: number;
    ownedNativeHandles: number;
    globalSettingsWritten: boolean;
  };
  sample: {
    desktop: {
      coordinateSpace: string;
      displays: NativeDisplay[];
      primarySourceName: string;
      primaryWorkAreaViaSpi: Rect;
    };
    theme: {
      appsUseLightTheme: { status: number; value: number | null };
      systemUsesLightTheme: { status: number; value: number | null };
      highContrast: { active: boolean; flags: number };
      nonForcedColorScheme: "light" | "dark" | null;
    };
  };
}
export interface TextScale {
  source: string;
  value: number;
  pid: number;
  powershell: string;
  edition: string;
}
export interface DesktopFacts {
  primary_id: number;
  displays: {
    id: number;
    bounds: Rect;
    work_area: Rect;
    scale_factor: number;
  }[];
  should_use_dark_colors: boolean;
}
export interface DesktopEvent {
  variant: "DesktopSnapshot";
  epoch: bigint;
  snapshot: DesktopFacts | null;
}
export interface SharedSource {
  decode(payload: Uint8Array): DesktopEvent;
  apply(fact: DesktopEvent): void;
  dispatch(fact: DesktopEvent): void;
  desktop(): DesktopFacts | undefined;
  end(error: Error): void;
}
export interface SharedFactories {
  WindowSource: new (callback: (event: DesktopEvent) => void) => SharedSource;
  encodeWindowEvent(event: DesktopEvent): Uint8Array;
  linkError(code: string, detail: string): Error & { code: string };
}

function requireFact(ok: unknown, message: string): asserts ok {
  if (!ok) throw new Error("W4 feasibility unavailable: " + message);
}

function sameRect(left: Rect, right: Rect): boolean {
  return (
    left.x === right.x &&
    left.y === right.y &&
    left.width === right.width &&
    left.height === right.height
  );
}

/** Validate the immutable dependency rather than maintaining a second mirror. */
export function verifyShared(root: string): void {
  const pin = JSON.parse(
    readFileSync(join(import.meta.dir, "shared-pin.json"), "utf8"),
  ) as { files: Record<string, string> };
  for (const [path, expected] of Object.entries(pin.files)) {
    const bytes = readFileSync(join(root, path));
    const actual = createHash("sha1")
      .update("blob " + bytes.length + "\0")
      .update(bytes)
      .digest("hex");
    requireFact(
      actual === expected,
      "shared source differs from pinned Git blob: " + path,
    );
  }
}

/** Load the original source; a missing or modified dependency is fatal. */
export async function loadShared(
  root = process.env.W4_SHARED_ROOT,
): Promise<SharedFactories> {
  requireFact(root, "set W4_SHARED_ROOT to the verified S1 export");
  verifyShared(root);
  const source = await import(
    pathToFileURL(join(root, "packages/@keld/api/src/window-source.ts")).href
  );
  const codec = await import(
    pathToFileURL(join(root, "packages/@keld/api/src/window-codec.ts")).href
  );
  const transport = await import(
    pathToFileURL(join(root, "packages/@keld/kipc/src/transport.ts")).href
  );
  requireFact(
    typeof source.WindowSource === "function" &&
      typeof codec.encodeWindowEvent === "function",
    "missing shared owner exports",
  );
  return {
    WindowSource: source.WindowSource,
    encodeWindowEvent: codec.encodeWindowEvent,
    linkError: transport.linkError,
  } as SharedFactories;
}

/** Full observed DisplayConfig target identity, excluding HMONITOR and array order. */
export function nativeIdentity(display: NativeDisplay): string {
  requireFact(
    display.nativePaths.length > 0 && display.nativePaths.length <= 32,
    "unbounded or missing native targets",
  );
  const targets = display.nativePaths
    .map((path) => {
      requireFact(
        path.targetDevicePath.length > 0 && path.targetDevicePath.length <= 128,
        "native device path is missing or overlong",
      );
      requireFact(
        Number.isInteger(path.targetAdapterLuid.highPart) &&
          path.targetAdapterLuid.highPart >= -2147483648 &&
          path.targetAdapterLuid.highPart <= 2147483647,
        "native adapter high word is outside i32",
      );
      for (const value of [path.targetAdapterLuid.lowPart, path.targetId]) {
        requireFact(
          Number.isInteger(value) && value >= 0 && value <= 4294967295,
          "native target identity is outside u32",
        );
      }
      return JSON.stringify([
        path.targetAdapterLuid.highPart,
        path.targetAdapterLuid.lowPart,
        path.targetId,
        path.targetDevicePath.toLowerCase(),
      ]);
    })
    .sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  requireFact(
    new Set(targets).size === targets.length,
    "duplicate native target identity",
  );
  return JSON.stringify(targets);
}

/**
 * PROPOSED opaque Host-session IDs, retained across removal, reorder and role recovery.
 * This bounded identity association stores no desktop facts. GH-481 owns the mirror.
 * The 33rd distinct native identity fails; no eviction or ID reuse is permitted.
 */
export class NativeDisplayIds {
  readonly #ids = new Map<string, number>();

  id(display: NativeDisplay): number {
    const key = nativeIdentity(display);
    const current = this.#ids.get(key);
    if (current !== undefined) return current;
    requireFact(
      this.#ids.size < 32,
      "Host-session native identity capacity exceeded",
    );
    const id = this.#ids.size + 1;
    this.#ids.set(key, id);
    return id;
  }

  get retainedIdentities(): number {
    return this.#ids.size;
  }
}

/** Chromium enclosing-rectangle rule for the qualified single primary display. */
export function enclosingDip(rect: Rect, scale: number): Rect {
  requireFact(Number.isFinite(scale) && scale > 0, "invalid effective scale");
  for (const value of [rect.x, rect.y, rect.width, rect.height]) {
    requireFact(Number.isSafeInteger(value), "non-integral physical rectangle");
  }
  requireFact(rect.width > 0 && rect.height > 0, "empty physical rectangle");
  const left = Math.floor(rect.x / scale);
  const top = Math.floor(rect.y / scale);
  requireFact(
    Number.isSafeInteger(rect.x + rect.width) &&
      Number.isSafeInteger(rect.y + rect.height),
    "physical rectangle edge overflow",
  );
  const right = Math.ceil((rect.x + rect.width) / scale);
  const bottom = Math.ceil((rect.y + rect.height) / scale);
  const width = right - left;
  const height = bottom - top;
  // The shared DesktopFacts codec requires signed i32 origins and positive
  // u32 sizes. Refuse a projected overflow before allocating a display ID.
  requireFact(
    Number.isInteger(left) &&
      left >= -0x8000_0000 &&
      left <= 0x7fff_ffff &&
      Number.isInteger(top) &&
      top >= -0x8000_0000 &&
      top <= 0x7fff_ffff &&
      Number.isInteger(width) &&
      width > 0 &&
      width <= 0xffff_ffff &&
      Number.isInteger(height) &&
      height > 0 &&
      height <= 0xffff_ffff,
    "projected logical rectangle is not representable by shared DesktopFacts",
  );
  return { x: left, y: top, width, height };
}

/**
 * Bounded single-primary, non-forced-colors feasibility mapping.
 * Other cases remain raw native evidence and fail here, without invented parity.
 */
export function nativeFacts(
  raw: NativeSample,
  textScale: TextScale,
  ids = new NativeDisplayIds(),
): DesktopFacts {
  requireFact(
    raw.schemaVersion === 1 && raw.status === "ok",
    "native sample failed",
  );
  requireFact(
    raw.dpiContext.exactRequestedContext === true &&
      raw.dpiContext.coordinates === "physical-pixels" &&
      raw.sample.desktop.coordinateSpace === "physical-pixels",
    "exact PMv2 physical sample required",
  );
  requireFact(
    raw.cleanup.globalSettingsWritten === false,
    "probe changed global settings",
  );
  requireFact(
    textScale.source ===
      "Windows.UI.ViewManagement.UISettings.TextScaleFactor" &&
      textScale.edition === "Desktop",
    "actual WinRT text scale required",
  );
  requireFact(
    Number.isFinite(textScale.value) &&
      textScale.value >= 1 &&
      textScale.value <= 2.25,
    "invalid native text scale",
  );
  const displays = raw.sample.desktop.displays;
  requireFact(
    displays.length === 1 && displays[0]?.primary === true,
    "multi-display layout is an explicit unqualified dependency",
  );
  const native = displays[0]!;
  requireFact(
    typeof native.sourceName === "string" &&
      native.sourceName.length > 0 &&
      raw.sample.desktop.primarySourceName === native.sourceName,
    "primary source name disagrees with selected display",
  );
  requireFact(
    native.bounds.x === 0 && native.bounds.y === 0,
    "primary-origin contract not established",
  );
  requireFact(
    native.bounds.width === native.physicalMode.width &&
      native.bounds.height === native.physicalMode.height,
    "monitor coordinates disagree with independent physical mode",
  );
  requireFact(
    sameRect(native.workArea, raw.sample.desktop.primaryWorkAreaViaSpi),
    "GetMonitorInfo/SPI work-area disagreement",
  );
  requireFact(
    Number.isInteger(native.dpi.x) &&
      native.dpi.x > 0 &&
      native.dpi.x <= 4294967295 &&
      native.dpi.x === native.dpi.y &&
      native.dpi.deviceScaleFactor === native.dpi.x / 96,
    "invalid independent native DPI inputs",
  );
  requireFact(
    raw.sample.theme.highContrast.active === false,
    "forced-color effective theme needs owner-qualified conversion",
  );
  requireFact(
    raw.sample.theme.nonForcedColorScheme === "light" ||
      raw.sample.theme.nonForcedColorScheme === "dark",
    "native effective scheme unavailable",
  );
  const theme = raw.sample.theme;
  requireFact(
    theme.systemUsesLightTheme?.status === 0 &&
      (theme.systemUsesLightTheme.value === 0 ||
        theme.systemUsesLightTheme.value === 1),
    "native system appearance query failed or incomplete",
  );
  requireFact(
    theme.appsUseLightTheme.status === 0 &&
      (theme.appsUseLightTheme.value === 0 ||
        theme.appsUseLightTheme.value === 1) &&
      theme.nonForcedColorScheme ===
        (theme.appsUseLightTheme.value === 0 ? "dark" : "light"),
    "native appearance query failed or contradicts effective scheme",
  );
  // The qualified single-primary work area may not extend
  // beyond its physical monitor. Reject before allocating a lifetime ID.
  requireFact(
    Number.isSafeInteger(native.workArea.x + native.workArea.width) &&
      Number.isSafeInteger(native.workArea.y + native.workArea.height) &&
      native.workArea.x >= native.bounds.x &&
      native.workArea.y >= native.bounds.y &&
      native.workArea.x + native.workArea.width <=
        native.bounds.x + native.bounds.width &&
      native.workArea.y + native.workArea.height <=
        native.bounds.y + native.bounds.height,
    "single-primary work area extends outside its monitor",
  );
  const scale = native.dpi.deviceScaleFactor * textScale.value;
  const bounds = enclosingDip(native.bounds, scale);
  const workArea = enclosingDip(native.workArea, scale);
  const id = ids.id(native);
  return {
    primary_id: id,
    displays: [
      {
        id,
        bounds,
        work_area: workArea,
        scale_factor: scale,
      },
    ],
    should_use_dark_colors: raw.sample.theme.nonForcedColorScheme === "dark",
  };
}

const run = promisify(execFile);

/** Invoke only after reserving the serialized Ramani native slot. */
export async function sampleNative(
  ids = new NativeDisplayIds(),
): Promise<{ raw: NativeSample; textScale: TextScale; facts: DesktopFacts }> {
  requireFact(process.env.W4_RUN_NATIVE === "1", "native slot not selected");
  const probe = process.env.W4_NATIVE_PROBE;
  const textScript =
    process.env.W4_TEXT_SCALE_SCRIPT ?? join(import.meta.dir, "text_scale.ps1");
  requireFact(probe, "set W4_NATIVE_PROBE to the owned compiled fixture");
  const sampled = await run(probe, ["--mode", "pmv2"], {
    timeout: 10000,
    windowsHide: true,
    maxBuffer: 1024 * 1024,
  });
  const raw = JSON.parse(sampled.stdout) as NativeSample;
  const scaled = await run(
    "powershell.exe",
    ["-NoProfile", "-NonInteractive", "-STA", "-File", textScript],
    { timeout: 10000, windowsHide: true, maxBuffer: 65536 },
  );
  const textScale = JSON.parse(scaled.stdout) as TextScale;
  const facts = nativeFacts(raw, textScale, ids);
  const record = { raw, textScale, facts };
  const receipt = process.env.W4_NATIVE_RECEIPT;
  if (receipt) writeFileSync(receipt, JSON.stringify(record, null, 2) + "\n");
  return record;
}

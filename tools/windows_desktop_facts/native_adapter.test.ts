/** Deterministic proposal tests. Values here are synthetic, never native receipts. */
import { expect, test } from "bun:test";
import {
  enclosingDip,
  nativeFacts,
  nativeIdentity,
  NativeDisplayIds,
  loadShared,
  type NativeDisplay,
  type NativeSample,
  type TextScale,
} from "./native_adapter.ts";

function display(target = 7): NativeDisplay {
  return {
    sourceName: "SYNTHETIC-GDI",
    observedHandleHex: "0xfeed",
    primary: true,
    bounds: { x: 0, y: 0, width: 1600, height: 1000 },
    workArea: { x: 0, y: 0, width: 1600, height: 950 },
    dpi: { x: 120, y: 120, deviceScaleFactor: 1.25 },
    physicalMode: { width: 1600, height: 1000 },
    nativePaths: [
      {
        sourceAdapterLuid: { highPart: 0, lowPart: 4 },
        sourceId: 0,
        targetAdapterLuid: { highPart: 0, lowPart: 4 },
        targetId: target,
        targetDevicePath: "SYNTHETIC-MONITOR-" + target,
      },
    ],
  };
}
function raw(): NativeSample {
  const native = display();
  return {
    schemaVersion: 1,
    status: "ok",
    processId: 1,
    machine: { name: "SYNTHETIC", architecture: "unit", os: { build: 0 } },
    dpiContext: { exactRequestedContext: true, coordinates: "physical-pixels" },
    cleanup: {
      ownedWindowsCreated: 0,
      ownedNativeHandles: 0,
      globalSettingsWritten: false,
    },
    sample: {
      desktop: {
        coordinateSpace: "physical-pixels",
        displays: [native],
        primarySourceName: native.sourceName,
        primaryWorkAreaViaSpi: { ...native.workArea },
      },
      theme: {
        appsUseLightTheme: { status: 0, value: 1 },
        systemUsesLightTheme: { status: 0, value: 1 },
        highContrast: { active: false, flags: 0 },
        nonForcedColorScheme: "light",
      },
    },
  };
}
const text: TextScale = {
  source: "Windows.UI.ViewManagement.UISettings.TextScaleFactor",
  value: 1,
  pid: 2,
  powershell: "5.1",
  edition: "Desktop",
};

test("physical work area and DPI/text scale feed enclosing logical rectangles", () => {
  const facts = nativeFacts(raw(), text);
  expect(facts.displays[0]!.bounds).toEqual({
    x: 0,
    y: 0,
    width: 1280,
    height: 800,
  });
  expect(facts.displays[0]!.work_area).toEqual({
    x: 0,
    y: 0,
    width: 1280,
    height: 760,
  });
  const enlarged = nativeFacts(raw(), { ...text, value: 1.25 });
  expect(enlarged.displays[0]!.scale_factor).toBe(1.5625);
  expect(enlarged.displays[0]!.bounds).toEqual({
    x: 0,
    y: 0,
    width: 1024,
    height: 640,
  });
});

test("enclosing conversion uses edges for negative and fractional boundaries", () => {
  expect(enclosingDip({ x: -101, y: 7, width: 100, height: 9 }, 1.25)).toEqual({
    x: -81,
    y: 5,
    width: 81,
    height: 8,
  });
  expect(() => enclosingDip({ x: 0, y: 0, width: 0, height: 1 }, 1)).toThrow(
    "empty",
  );
  expect(() => enclosingDip({ x: 0, y: 0, width: 1, height: 1 }, 0)).toThrow(
    "scale",
  );
});

test("identity follows full native key across handle/GDI rename and removal/recovery", () => {
  const ids = new NativeDisplayIds();
  const first = display(7);
  const second = display(8);
  const firstId = ids.id(first);
  const secondId = ids.id(second);
  expect(firstId).not.toBe(secondId);
  const same = structuredClone(first);
  same.observedHandleHex = "0xbeef";
  same.sourceName = "SYNTHETIC-RENAMED-GDI";
  same.nativePaths[0]!.sourceId = 99;
  same.nativePaths[0]!.targetDevicePath =
    same.nativePaths[0]!.targetDevicePath.toLowerCase();
  // No active-display-only replacement occurs: reappearance and a recovered role
  // continue using this Host-lifetime identity owner.
  expect(ids.id(same)).toBe(firstId);
  expect(ids.id(second)).toBe(secondId);
  expect(ids.retainedIdentities).toBe(2);
});

test("clone target order is canonical; changed tuple/path is a distinct identity", () => {
  const a = display(7);
  a.nativePaths.push(display(8).nativePaths[0]!);
  const b = structuredClone(a);
  b.nativePaths.reverse();
  expect(nativeIdentity(a)).toBe(nativeIdentity(b));
  b.nativePaths[0]!.targetAdapterLuid.highPart = 1;
  expect(nativeIdentity(a)).not.toBe(nativeIdentity(b));
  const duplicate = display(7);
  duplicate.nativePaths.push(structuredClone(duplicate.nativePaths[0]!));
  expect(() => nativeIdentity(duplicate)).toThrow("duplicate");
});

test("32 lifetime identities fit; 33 is refused without eviction or aliasing", () => {
  const ids = new NativeDisplayIds();
  const assigned = Array.from({ length: 32 }, (_, index) =>
    ids.id(display(index)),
  );
  expect(new Set(assigned).size).toBe(32);
  expect(() => ids.id(display(32))).toThrow("capacity");
  expect(ids.retainedIdentities).toBe(32);
  for (let index = 31; index >= 0; index -= 1)
    expect(ids.id(display(index))).toBe(assigned[index]);
});

for (const [name, mutate] of [
  [
    "failed source",
    (v: NativeSample) => {
      v.status = "error";
    },
  ],
  [
    "unaware source",
    (v: NativeSample) => {
      v.dpiContext.coordinates = "dpi-virtualized-caller-units";
    },
  ],
  [
    "fake DPI fallback",
    (v: NativeSample) => {
      v.sample.desktop.displays[0]!.dpi.x = 0;
    },
  ],
  [
    "wrong physical units",
    (v: NativeSample) => {
      v.sample.desktop.displays[0]!.physicalMode.width += 1;
    },
  ],
  [
    "wrong work area",
    (v: NativeSample) => {
      v.sample.desktop.primaryWorkAreaViaSpi.height -= 1;
    },
  ],
  [
    "forced colors",
    (v: NativeSample) => {
      v.sample.theme.highContrast.active = true;
    },
  ],
  [
    "unqualified multiple monitors",
    (v: NativeSample) => {
      v.sample.desktop.displays.push(display(8));
    },
  ],
  [
    "missing theme",
    (v: NativeSample) => {
      v.sample.theme.nonForcedColorScheme = null;
    },
  ],
] as const) {
  test("refuses " + name + " instead of publishing default facts", () => {
    const value = raw();
    mutate(value);
    expect(() => nativeFacts(value, text)).toThrow("unavailable");
  });
}

// These are malformed JSON-shaped inputs, not native Windows measurements.
const requiredNativeBooleans = [
  {
    name: "exactRequestedContext",
    key: "exactRequestedContext",
    container: (value: NativeSample) =>
      value.dpiContext as unknown as Record<string, unknown>,
  },
  {
    name: "globalSettingsWritten",
    key: "globalSettingsWritten",
    container: (value: NativeSample) =>
      value.cleanup as unknown as Record<string, unknown>,
  },
  {
    name: "highContrast.active",
    key: "active",
    container: (value: NativeSample) =>
      value.sample.theme.highContrast as unknown as Record<string, unknown>,
  },
] as const;
const invalidBooleanObservations = [
  ["missing", undefined],
  ["null", null],
  ["numeric zero", 0],
  ["numeric one", 1],
  ["string false", "false"],
  ["string true", "true"],
] as const;
for (const field of requiredNativeBooleans) {
  for (const [kind, bad] of invalidBooleanObservations) {
    test(`refuses ${kind} ${field.name} before allocating IDs`, () => {
      const sample = raw();
      const target = field.container(sample);
      if (kind === "missing") {
        expect(Reflect.deleteProperty(target, field.key)).toBe(true);
      } else {
        target[field.key] = bad;
      }
      // Allocation is a Host-lifetime side effect. An invalid receipt must
      // not consume an ID or change the next valid native target's identity.
      const ids = new NativeDisplayIds();
      expect(() => nativeFacts(sample, text, ids)).toThrow("unavailable");
      expect(ids.retainedIdentities).toBe(0);
      const healthy = raw();
      healthy.sample.desktop.displays[0]!.nativePaths[0]!.targetId = 99;
      expect(nativeFacts(healthy, text, ids).primary_id).toBe(1);
      expect(ids.retainedIdentities).toBe(1);
    });
  }
}

// A failed independent system-theme query must not be hidden by a successful
// application-theme query. These are synthetic malformed JSON receipts.
const invalidSystemThemeResults = [
  ["missing", undefined],
  ["null", null],
  ["nonzero status", { status: 5, value: null }],
  ["nonzero status with stale value", { status: 5, value: 1 }],
  ["missing status", { value: 1 }],
  ["missing value", { status: 0 }],
  ["null value", { status: 0, value: null }],
  ["out-of-range value", { status: 0, value: 2 }],
  ["string status", { status: "0", value: 1 }],
  ["string value", { status: 0, value: "1" }],
] as const;
for (const [name, malformed] of invalidSystemThemeResults) {
  test(`refuses ${name} system-theme observation before allocating IDs`, () => {
    const sample = raw();
    const theme = sample.sample.theme as unknown as Record<string, unknown>;
    if (name === "missing") {
      expect(Reflect.deleteProperty(theme, "systemUsesLightTheme")).toBe(true);
    } else {
      theme.systemUsesLightTheme = malformed;
    }
    const ids = new NativeDisplayIds();
    expect(() => nativeFacts(sample, text, ids)).toThrow("unavailable");
    expect(ids.retainedIdentities).toBe(0);
    expect(nativeFacts(raw(), text, ids).primary_id).toBe(1);
    expect(ids.retainedIdentities).toBe(1);
  });
}

test("complete native light and dark observations remain usable", () => {
  for (const [value, scheme] of [
    [1, "light"],
    [0, "dark"],
  ] as const) {
    const sample = raw();
    sample.sample.theme.appsUseLightTheme.value = value;
    // The shell/system and application theme preferences may legitimately differ.
    sample.sample.theme.systemUsesLightTheme.value = value === 1 ? 0 : 1;
    sample.sample.theme.nonForcedColorScheme = scheme;
    expect(nativeFacts(sample, text).should_use_dark_colors).toBe(
      scheme === "dark",
    );
  }
});

// These JSON-shaped cleanup counter mutations are not native Windows measurements.
// The probe currently reports exact zero owned windows and native handles. A
// malformed or nonzero attestation must not allocate a Host-session display ID.
const invalidCleanupCounts = [
  ["missing", undefined],
  ["null", null],
  ["positive", 1],
  ["negative", -1],
  ["fractional", 0.5],
  ["string zero", "0"],
  ["boolean false", false],
  ["NaN", NaN],
  ["infinity", Infinity],
] as const;
for (const field of ["ownedWindowsCreated", "ownedNativeHandles"] as const) {
  for (const [kind, invalid] of invalidCleanupCounts) {
    test(`refuses ${kind} ${field} cleanup receipt before allocating IDs`, () => {
      const sample = raw();
      const cleanup = sample.cleanup as unknown as Record<string, unknown>;
      if (kind === "missing") {
        expect(Reflect.deleteProperty(cleanup, field)).toBe(true);
      } else {
        cleanup[field] = invalid;
      }
      const ids = new NativeDisplayIds();
      expect(() => nativeFacts(sample, text, ids)).toThrow("unavailable");
      expect(ids.retainedIdentities).toBe(0);
      expect(nativeFacts(raw(), text, ids).primary_id).toBe(1);
      expect(ids.retainedIdentities).toBe(1);
    });
  }
}

test("an explicit global settings write refuses native facts", () => {
  const sample = raw();
  sample.cleanup.globalSettingsWritten = true;
  const ids = new NativeDisplayIds();
  expect(() => nativeFacts(sample, text, ids)).toThrow("unavailable");
  expect(ids.retainedIdentities).toBe(0);
});

test("invalid or registry-only text scale cannot become a default 1", () => {
  for (const value of [0, NaN, Infinity, 2.251]) {
    expect(() => nativeFacts(raw(), { ...text, value })).toThrow();
  }
  expect(() => nativeFacts(raw(), { ...text, source: "registry" })).toThrow(
    "WinRT",
  );
});

test.skipIf(!process.env.W4_SHARED_ROOT)(
  "the real shared owner applies these facts, rejects stale epochs and retires",
  async () => {
    const shared = await loadShared();
    const events: bigint[] = [];
    const owner = new shared.WindowSource((event) => events.push(event.epoch));
    const initial = {
      variant: "DesktopSnapshot" as const,
      epoch: 1n,
      snapshot: nativeFacts(raw(), text),
    };
    owner.apply(owner.decode(shared.encodeWindowEvent(initial)));
    const frozen = owner.desktop();
    expect(frozen).toEqual(initial.snapshot);
    expect(Object.isFrozen(frozen)).toBe(true);
    const update = {
      ...initial,
      epoch: 2n,
      snapshot: { ...initial.snapshot, should_use_dark_colors: true },
    };
    owner.apply(owner.decode(shared.encodeWindowEvent(update)));
    owner.dispatch(update);
    expect(owner.desktop()?.should_use_dark_colors).toBe(true);
    expect(frozen?.should_use_dark_colors).toBe(false);
    expect(events).toEqual([2n]);
    expect(() => owner.apply(initial)).toThrow();
    const recovered = new shared.WindowSource(() => {});
    recovered.apply(recovered.decode(shared.encodeWindowEvent(update)));
    expect(recovered.desktop()).toEqual(update.snapshot);
    recovered.end(shared.linkError("KELD-IPC-022", "role retired"));
    expect(() => recovered.desktop()).toThrow();
    expect(() => recovered.apply({ ...update, epoch: 3n })).toThrow();
  },
);

test("invalid native identity words are refused without allocating", () => {
  for (const mutate of [
    (d: NativeDisplay) => {
      d.nativePaths[0]!.targetAdapterLuid.lowPart = -1;
    },
    (d: NativeDisplay) => {
      d.nativePaths[0]!.targetAdapterLuid.highPart = 2147483648;
    },
    (d: NativeDisplay) => {
      d.nativePaths[0]!.targetId = 4294967296;
    },
  ]) {
    const d = display();
    const ids = new NativeDisplayIds();
    mutate(d);
    expect(() => ids.id(d)).toThrow();
    expect(ids.retainedIdentities).toBe(0);
  }
});

test("failed appearance status and contradicted scheme never become usable facts", () => {
  const failed = raw();
  failed.sample.theme.appsUseLightTheme.status = 2;
  expect(() => nativeFacts(failed, text)).toThrow(/appearance query failed/);
  const contradicted = raw();
  contradicted.sample.theme.nonForcedColorScheme = "dark";
  expect(() => nativeFacts(contradicted, text)).toThrow(/contradicts/);
});

test("an unencodable projected rectangle fails before it consumes an identity", () => {
  const sample = raw();
  const native = sample.sample.desktop.displays[0]!;
  native.bounds.width = 100_000_000;
  native.workArea.width = 100_000_000;
  native.physicalMode.width = 100_000_000;
  sample.sample.desktop.primaryWorkAreaViaSpi.width = 100_000_000;
  native.dpi = { x: 1, y: 1, deviceScaleFactor: 1 / 96 };
  const ids = new NativeDisplayIds();
  // Physical RECT dimensions fit the native i32/u32 shape, but projected
  // logical width (9.6 billion) cannot fit the shared DesktopFacts u32 field.
  expect(() => nativeFacts(sample, text, ids)).toThrow(/representable/);
  expect(ids.retainedIdentities).toBe(0);
});

test("rectangle validation completes before identity allocation", () => {
  const invalid = raw();
  invalid.sample.desktop.displays[0]!.workArea.width = 0;
  invalid.sample.desktop.primaryWorkAreaViaSpi.width = 0;
  const ids = new NativeDisplayIds();
  expect(() => nativeFacts(invalid, text, ids)).toThrow(
    /empty physical rectangle/,
  );
  expect(ids.retainedIdentities).toBe(0);
  expect(() =>
    enclosingDip({ x: Number.MAX_SAFE_INTEGER, y: 0, width: 1, height: 1 }, 1),
  ).toThrow(/edge overflow/);
});

test("off-monitor primary work area refuses before identity allocation", () => {
  const ids = new NativeDisplayIds();
  const malformed = raw();
  malformed.sample.desktop.displays[0]!.workArea.x = -1;
  malformed.sample.desktop.primaryWorkAreaViaSpi.x = -1;
  malformed.sample.desktop.displays[0]!.nativePaths[0]!.targetId = 99;
  expect(() => nativeFacts(malformed, text, ids)).toThrow(
    "single-primary work area extends outside",
  );
  expect(ids.retainedIdentities).toBe(0);
  const repaired = structuredClone(malformed);
  repaired.sample.desktop.displays[0]!.workArea.x = 0;
  repaired.sample.desktop.primaryWorkAreaViaSpi.x = 0;
  expect(nativeFacts(repaired, text, ids).primary_id).toBe(1);
  expect(ids.retainedIdentities).toBe(1);
});

test("non-Boolean or absent primary observations refuse before ID allocation", () => {
  for (const primary of ["false", "true", 1, 0, {}, null, undefined, false]) {
    const ids = new NativeDisplayIds();
    const malformed = raw();
    (
      malformed.sample.desktop.displays[0]! as unknown as { primary: unknown }
    ).primary = primary;
    expect(() => nativeFacts(malformed, text, ids)).toThrow("unqualified");
    expect(ids.retainedIdentities).toBe(0);
    expect(nativeFacts(raw(), text, ids).primary_id).toBe(1);
  }
});

// The native producer derives this label from the same primary sample.
// A contradictory or absent label is an incomplete native receipt, not a
// second display identity; never consume a Host-lifetime ID for one.
test("matching malformed primary source fields still refuse before allocation", () => {
  for (const name of ["", null, undefined, 7]) {
    const sample = raw();
    (
      sample.sample.desktop.displays[0]! as unknown as { sourceName: unknown }
    ).sourceName = name;
    (
      sample.sample.desktop as unknown as { primarySourceName: unknown }
    ).primarySourceName = name;
    const ids = new NativeDisplayIds();
    expect(() => nativeFacts(sample, text, ids)).toThrow("primary source");
    expect(ids.retainedIdentities).toBe(0);
    expect(nativeFacts(raw(), text, ids).primary_id).toBe(1);
  }
});

test("primary source label must agree with marked primary before allocation", () => {
  for (const label of ["\\\\.\\DISPLAY2", "", null, undefined, 7]) {
    const sample = raw();
    (
      sample.sample.desktop as unknown as { primarySourceName: unknown }
    ).primarySourceName = label;
    const ids = new NativeDisplayIds();
    expect(() => nativeFacts(sample, text, ids)).toThrow("primary source");
    expect(ids.retainedIdentities).toBe(0);
    expect(nativeFacts(raw(), text, ids).primary_id).toBe(1);
  }
});

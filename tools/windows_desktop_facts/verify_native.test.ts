/** Assertions over fresh native receipts; no expected machine dimensions or theme constants. */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, test } from "bun:test";
import {
  nativeFacts,
  nativeIdentity,
  NativeDisplayIds,
  loadShared,
} from "./native_adapter.ts";

const evidence = process.env.W4_NATIVE_EVIDENCE;
const enabled = Boolean(evidence);
function read(name: string): any {
  assert.ok(evidence, "select fresh native evidence explicitly");
  return JSON.parse(readFileSync(join(evidence, name), "utf8"));
}

describe.skipIf(!enabled)("fresh Windows desktop evidence", () => {
  test("actual held Subscribe precedes native capture, application and facade Ready", () => {
    const receipt = read("native-first-sample.json");
    assert.equal(receipt.coreAdmissionProven, false);
    assert.equal(receipt.originalCallerDeadlineMs, 5000);
    const t = receipt.monotonicMs;
    assert.ok(t.subscribedAt <= t.sampleStartedAt);
    assert.ok(t.sampleStartedAt < t.sampleCompletedAt);
    assert.ok(t.sampleCompletedAt <= t.settledAt && t.settledAt <= t.readyAt);
    assert.ok(t.settledAt - t.subscribedAt < 5000);
    assert.equal(receipt.sample.raw.processId > 0, true);
    assert.equal(receipt.sample.textScale.pid > 0, true);
    assert.notEqual(receipt.sample.raw.processId, receipt.sample.textScale.pid);
  });

  test("fresh native facts agree with an independent Electron geometry/theme oracle", () => {
    const captured = read("native-first-sample.json").sample;
    const oracle = read("electron.json");
    assert.equal(oracle.platform, "win32");
    assert.equal(oracle.electronVersion, "44.4.5");
    assert.equal(oracle.themeSource, "system");
    assert.equal(oracle.electronRunAsNodeEnv, null);
    assert.equal(oracle.samples[0].windowsOpen, 0);
    assert.equal(oracle.samples[1].windowsOpen, 1);
    const first = oracle.samples[0];
    const after = oracle.samples[1];
    assert.equal(first.all.length, 1, "bounded single-display projection");
    assert.equal(
      first.inForcedColorsMode,
      false,
      "forced-colors projection remains unqualified",
    );
    const native = captured.raw.sample.desktop.displays[0];
    const facts = nativeFacts(captured.raw, captured.textScale);
    assert.deepEqual(facts, captured.facts);
    assert.deepEqual(facts.displays[0]!.bounds, first.primary.bounds);
    assert.deepEqual(facts.displays[0]!.work_area, first.primary.workArea);
    assert.equal(facts.displays[0]!.scale_factor, first.primary.scaleFactor);
    assert.equal(facts.should_use_dark_colors, first.shouldUseDarkColors);
    assert.deepEqual(first.primaryBoundsDipToScreenRect, native.bounds);
    assert.deepEqual(first.primaryWorkAreaDipToScreenRect, native.workArea);
    assert.deepEqual(first.primary.bounds, after.primary.bounds);
    assert.deepEqual(first.primary.workArea, after.primary.workArea);
    assert.equal(first.shouldUseDarkColors, after.shouldUseDarkColors);
    assert.equal(native.dpi.deviceScaleFactor, native.dpi.x / 96);
    assert.equal(
      captured.textScale.source,
      "Windows.UI.ViewManagement.UISettings.TextScaleFactor",
    );
    assert.equal(captured.raw.cleanup.globalSettingsWritten, false);
  });

  test("a separate native process reacquires full identity and physical facts without a cache", () => {
    const first = read("native-first-sample.json").sample;
    const next = read("pmv2.json");
    assert.notEqual(first.raw.processId, next.processId);
    assert.equal(next.processId, read("pmv2.process.json").pid);
    const beforeDisplay = first.raw.sample.desktop.displays[0];
    const afterDisplay = next.sample.desktop.displays[0];
    assert.ok(beforeDisplay.nativePaths[0].targetDevicePath.length > 0);
    assert.equal(nativeIdentity(beforeDisplay), nativeIdentity(afterDisplay));
    assert.deepEqual(beforeDisplay.bounds, afterDisplay.bounds);
    assert.deepEqual(beforeDisplay.workArea, afterDisplay.workArea);
    assert.deepEqual(beforeDisplay.dpi, afterDisplay.dpi);
    const ids = new NativeDisplayIds();
    assert.deepEqual(
      nativeFacts(first.raw, first.textScale, ids),
      nativeFacts(next, first.textScale, ids),
    );
    assert.equal(ids.retainedIdentities, 1);
  });

  test("unaware units and physical-as-logical values are refused on the actual non-unit desktop", () => {
    const first = read("native-first-sample.json").sample;
    const unaware = read("unaware.json");
    const native = first.raw.sample.desktop.displays[0];
    const virtual = unaware.sample.desktop.displays[0];
    assert.notEqual(
      native.dpi.deviceScaleFactor,
      1,
      "unit-mismatch control requires a non-unit observed desktop",
    );
    assert.equal(virtual.dpi.x, 96);
    assert.notEqual(native.dpi.x, virtual.dpi.x);
    assert.deepEqual(virtual.physicalMode, native.physicalMode);
    assert.notDeepEqual(virtual.bounds, native.bounds);
    assert.throws(
      () => nativeFacts(unaware, first.textScale),
      /physical sample required/,
    );
    const logical = read("electron.json").primary.bounds;
    assert.throws(
      () => assert.deepEqual(native.bounds, logical),
      { name: "AssertionError" },
      "wrong physical-as-DIP implementation must fail the oracle assertion",
    );
  });

  test("real invalid-input API controls fail and never return usable desktop facts", () => {
    const sources: Record<string, string> = {
      "invalid-monitor": "GetMonitorInfoW",
      "failed-dpi": "GetDpiForMonitor",
      "high-contrast-size": "SystemParametersInfoW(SPI_GETHIGHCONTRAST)",
    };
    for (const [name, source] of Object.entries(sources)) {
      const result = read(name + ".json");
      assert.equal(read(name + ".process.json").exit, 1);
      assert.equal(result.status, "error");
      assert.equal(result.error.kind, "native-failure");
      assert.equal(result.error.source, source);
      assert.equal("sample" in result, false);
    }
  });

  test("actual native values pass the original shared codec and retired sources refuse reuse", async () => {
    const captured = read("native-first-sample.json").sample;
    const shared = await loadShared();
    const source = new shared.WindowSource(() => {});
    const first = {
      variant: "DesktopSnapshot" as const,
      epoch: 1n,
      snapshot: captured.facts,
    };
    source.apply(source.decode(shared.encodeWindowEvent(first)));
    assert.deepEqual(source.desktop(), captured.facts);
    const next = nativeFacts(read("pmv2.json"), captured.textScale);
    source.apply(
      source.decode(
        shared.encodeWindowEvent({ ...first, epoch: 2n, snapshot: next }),
      ),
    );
    assert.throws(
      () => source.apply(first),
      (error: unknown) => (error as { code?: string }).code === "KELD-IPC-005",
    );
    const retired = new shared.WindowSource(() => {});
    retired.apply(retired.decode(shared.encodeWindowEvent(first)));
    retired.end(shared.linkError("KELD-IPC-022", "test role retired"));
    assert.throws(() => retired.apply({ ...first, epoch: 3n }), /retired|022/i);
    const recovered = new shared.WindowSource(() => {});
    recovered.apply(
      recovered.decode(
        shared.encodeWindowEvent({ ...first, epoch: 2n, snapshot: next }),
      ),
    );
    assert.deepEqual(recovered.desktop(), next);
  });
});

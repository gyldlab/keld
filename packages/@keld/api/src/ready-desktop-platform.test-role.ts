/**
 * GH481 S1 current @keld/api lifecycle and Electron getter integration.
 * Child process captures the existing Host-defined immutable launch facts.
 * The socket is controlled: this is an independent source/readiness oracle,
 * never evidence of physical AppKit capture or WorkerLink frame counts.
 */
import assert from "node:assert/strict";
import { mock } from "bun:test";
import { hostLaunchFacts, nativeFacadeBootSelected } from "@keld/kipc";
import { encodeWindowEvent } from "./window-codec.ts";
import { WindowSource } from "./window-source.ts";
import type { LifecycleHandler } from "./link.ts";

const mode = process.argv[2];
assert.ok(
  [
    "other-facade",
    "other-renderer",
    "mac-valid",
    "mac-missing",
    "mac-no-mirror",
    "mac-invalidated",
  ].includes(mode ?? ""),
);
assert.equal(hostLaunchFacts !== undefined, mode?.startsWith("mac-"));
assert.equal(nativeFacadeBootSelected, mode !== "other-renderer");

let callbacks: LifecycleHandler | undefined;
let linkCloses = 0;
let hostCalls = 0;
const source = new WindowSource(() => {});
const session = {
  get windowSource() {
    return mode === "mac-no-mirror" || mode === "other-renderer"
      ? undefined
      : source;
  },
  close() {
    linkCloses += 1;
  },
  quit: async () => {},
  consumeAppliedLastWindowClosed: () => false,
  callWindow: async () => {
    hostCalls += 1;
    throw new Error("desktop getter issued an unexpected async Host CALL");
  },
  callWindowBlocking: () => {
    hostCalls += 1;
    throw new Error("desktop getter issued an unexpected sync Host CALL");
  },
};

mock.module("./link.ts", () => ({
  LifecycleLink: {
    connect: async (_address: string, handlers: LifecycleHandler) => {
      callbacks = handlers;
      handlers.onConnected?.(session as never);
      return session;
    },
  },
}));

process.env.KELD_APP_LINK = `/controlled-no-socket#${"a".repeat(64)}`;
const { app, getDesktopSnapshot, isDesktopS1Selected } =
  await import("./app.ts");
assert.equal(isDesktopS1Selected(), mode?.startsWith("mac-"));
const ready = app.whenReady();
assert.ok(callbacks, "the real app installs exactly one lifecycle handler");

const captured = (epoch: bigint, dark: boolean) => ({
  variant: "DesktopSnapshot" as const,
  epoch,
  snapshot: {
    primary_id: 17,
    displays: [
      {
        id: 17,
        bounds: { x: 0, y: 0, width: 1440, height: 900 },
        work_area: { x: 0, y: 24, width: 1440, height: 876 },
        scale_factor: 2,
      },
    ],
    should_use_dark_colors: dark,
  },
});
const apply = (event: Parameters<typeof encodeWindowEvent>[0]): void => {
  const fact = source.decode(encodeWindowEvent(event));
  source.apply(fact);
};

if (mode === "mac-valid" || mode === "mac-invalidated") {
  apply(captured(1n, false));
}
if (mode === "mac-invalidated") {
  apply({ variant: "DesktopSnapshot", epoch: 2n, snapshot: null });
}

callbacks.onReady();
assert.equal(app.isReady(), false, "raw Ready cannot commit synchronously");

if (
  mode === "mac-missing" ||
  mode === "mac-invalidated" ||
  mode === "mac-no-mirror"
) {
  const expectedCode =
    mode === "mac-no-mirror" ? "KELD-CORE-045" : "KELD-CORE-049";
  await assert.rejects(
    ready,
    (error: unknown) =>
      (error as { code?: string } | null)?.code === expectedCode,
  );
  assert.equal(
    app.isReady(),
    false,
    "unavailable S1 facts never authorize Ready",
  );
  assert.equal(linkCloses, 1, "failed current role's one link is closed");
  await assert.rejects(
    app.whenReady(),
    (error: unknown) =>
      (error as { code?: string } | null)?.code === expectedCode,
  );
} else {
  await ready;
  assert.equal(app.isReady(), true);
  assert.equal(linkCloses, 0);
}

if (mode === "other-facade" || mode === "other-renderer") {
  // Windows/Linux facade (or renderer) without the macOS-only immutable
  // development record MUST preserve the previous public Ready contract.
  // It does not manufacture desktop support on these platforms.
  const { screen } = await import("../../electron/src/screen.ts");
  const { nativeTheme } = await import("../../electron/src/native-theme.ts");
  for (const attempt of [
    () => screen.getAllDisplays(),
    () => screen.getPrimaryDisplay(),
    () => nativeTheme.shouldUseDarkColors,
  ]) {
    assert.throws(
      attempt,
      (error: unknown) =>
        (error as { code?: string } | null)?.code === "KELD-COMPAT-013",
    );
  }
} else if (mode === "mac-valid") {
  const { screen } = await import("../../electron/src/screen.ts");
  const { nativeTheme } = await import("../../electron/src/native-theme.ts");
  const observed = getDesktopSnapshot();
  assert.equal(Object.isFrozen(observed), true);
  assert.equal(observed.primary_id, 17);
  for (let index = 0; index < 1_000; index += 1) {
    const all = screen.getAllDisplays();
    const primary = screen.getPrimaryDisplay();
    assert.equal(all.length, 1);
    assert.equal(primary.id, 17);
    assert.deepEqual(primary.workArea, {
      x: 0,
      y: 24,
      width: 1440,
      height: 876,
    });
    assert.equal(nativeTheme.shouldUseDarkColors, false);
    all[0]!.workArea.y = 9000;
    assert.equal(screen.getPrimaryDisplay().workArea.y, 24);
  }
  assert.equal(hostCalls, 0, "local getter read must not call the Host seam");
  apply({ variant: "DesktopSnapshot", epoch: 2n, snapshot: null });
  assert.throws(
    () => getDesktopSnapshot(),
    (error: unknown) =>
      (error as { code?: string } | null)?.code === "KELD-CORE-049",
  );
  assert.equal(
    app.isReady(),
    true,
    "post-Ready loss only invalidates desktop reads",
  );
}
assert.equal(
  hostCalls,
  0,
  "no getter or Ready path can issue an outbound Host call",
);
console.log(`PASS GH481 ${mode}`);

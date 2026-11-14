/**
 * GH481 S1 current @keld/api lifecycle and Electron getter integration.
 * Child process captures the existing Host-defined immutable launch facts.
 * The socket is controlled: this is an independent source/readiness oracle,
 * never evidence of physical AppKit capture or WorkerLink frame counts.
 */
import assert from "node:assert/strict";
import { lstatSync, realpathSync } from "node:fs";
import { isAbsolute, join } from "node:path";
import { pathToFileURL } from "node:url";
import { mock } from "bun:test";
import {
  hostLaunchFacts,
  nativeFacadeBootSelected,
  windowsHostSelected,
  windowsDesktopSnapshotSelected,
} from "@keld/kipc";
import { encodeWindowEvent } from "./window-codec.ts";
import { WindowSource } from "./window-source.ts";
import type { LifecycleHandler } from "./link.ts";

const mode = process.argv[2];
const electronSourceRoot = process.argv[3];
assert.ok(
  [
    "other-facade",
    "other-renderer",
    "windows-facts",
    "windows-no-mirror",
    "windows-desktop-valid",
    "windows-desktop-missing",
    "windows-desktop-invalidated",
    "windows-desktop-no-mirror",
    "mac-valid",
    "mac-missing",
    "mac-no-mirror",
    "mac-invalidated",
  ].includes(mode ?? ""),
);
assert.ok(
  electronSourceRoot !== undefined && isAbsolute(electronSourceRoot),
  "GH481 role requires an absolute private Electron source root",
);
const privateElectronSource = realpathSync(electronSourceRoot);
for (const name of ["screen.ts", "native-theme.ts", "unsupported.ts"]) {
  const path = join(privateElectronSource, name);
  const metadata = lstatSync(path);
  assert.ok(metadata.isFile() && !metadata.isSymbolicLink());
  assert.equal(realpathSync(path), path);
}
assert.equal(
  hostLaunchFacts !== undefined,
  mode?.startsWith("mac-") || mode?.startsWith("windows-"),
);
if (mode?.startsWith("windows-")) {
  assert.equal(hostLaunchFacts?.schema, 3);
  assert.equal(hostLaunchFacts?.name, "gh1184-windows-s1");
  assert.equal(hostLaunchFacts?.version, "31.7.0");
  assert.equal(hostLaunchFacts?.locale, "en-US");
}
assert.equal(nativeFacadeBootSelected, mode !== "other-renderer");
assert.equal(windowsHostSelected, mode?.startsWith("windows-"));
assert.equal(
  windowsDesktopSnapshotSelected,
  mode?.startsWith("windows-desktop-"),
);

let callbacks: LifecycleHandler | undefined;
let linkCloses = 0;
let hostCalls = 0;
const source = new WindowSource(() => {});
const session = {
  get windowSource() {
    return mode === "mac-no-mirror" ||
      mode === "windows-no-mirror" ||
      mode === "windows-desktop-no-mirror" ||
      mode === "other-renderer"
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
const selectedDesktop =
  mode?.startsWith("mac-") || mode?.startsWith("windows-desktop-");
assert.equal(isDesktopS1Selected(), selectedDesktop);
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

if (
  mode === "mac-valid" ||
  mode === "mac-invalidated" ||
  mode === "windows-desktop-valid" ||
  mode === "windows-desktop-invalidated"
) {
  apply(captured(1n, false));
}
if (mode === "mac-invalidated" || mode === "windows-desktop-invalidated") {
  apply({ variant: "DesktopSnapshot", epoch: 2n, snapshot: null });
}

callbacks.onReady();
assert.equal(app.isReady(), false, "raw Ready cannot commit synchronously");

if (
  mode === "mac-missing" ||
  mode === "mac-invalidated" ||
  mode === "mac-no-mirror" ||
  mode === "windows-no-mirror" ||
  mode === "windows-desktop-missing" ||
  mode === "windows-desktop-invalidated" ||
  mode === "windows-desktop-no-mirror"
) {
  const expectedCode =
    mode === "mac-no-mirror" ||
    mode === "windows-no-mirror" ||
    mode === "windows-desktop-no-mirror"
      ? "KELD-CORE-045"
      : "KELD-CORE-049";
  await assert.rejects(
    ready,
    (error: unknown) =>
      (error as { code?: string } | null)?.code === expectedCode,
  );
  assert.equal(
    app.isReady(),
    false,
    "missing required Host state never authorizes Ready",
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

if (
  mode === "other-facade" ||
  mode === "other-renderer" ||
  (mode?.startsWith("windows-") && !selectedDesktop)
) {
  // Unselected desktop getters still refuse even when Windows carries real
  // launch facts. Readiness is not a fabricated desktop snapshot.
  const { screen } = await import(
    pathToFileURL(join(privateElectronSource, "screen.ts")).href
  );
  const { nativeTheme } = await import(
    pathToFileURL(join(privateElectronSource, "native-theme.ts")).href
  );
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
} else if (mode === "mac-valid" || mode === "windows-desktop-valid") {
  const { screen } = await import(
    pathToFileURL(join(privateElectronSource, "screen.ts")).href
  );
  const { nativeTheme } = await import(
    pathToFileURL(join(privateElectronSource, "native-theme.ts")).href
  );
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

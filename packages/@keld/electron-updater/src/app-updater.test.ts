/**
 * GH-491 spec §3 criteria 3-9, 11 and 13 against electron-updater 6.8.9
 * (`out/AppUpdater.js`, `out/BaseUpdater.js`; line numbers below are into that
 * artifact). Application facts are injected through the `AppAdapter` seam (spec
 * §4.2 D3): stubbing the Electron facade in this process would leak into other
 * test files, because `bun test` shares one module cache.
 *
 * Every case registers a listener for all nine upstream event names, so a
 * fabricated `update-*` or `checking-for-update` event fails the case that
 * produced it.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "bun:test";

import { isCallError } from "../../kipc/src/transport.ts";
import { AppUpdater, NoOpLogger, autoUpdater } from "./index.ts";
import { UPDATE_CHECK_REFUSED } from "./error.ts";
import type { AppAdapter, Logger, UpdaterEvent } from "./types.ts";

const EVENT_NAMES: readonly UpdaterEvent[] = [
  "error",
  "login",
  "checking-for-update",
  "update-not-available",
  "update-available",
  "update-downloaded",
  "download-progress",
  "update-cancelled",
  "appimage-filename-updated",
];

const NEVER_EMITTED: readonly UpdaterEvent[] = EVENT_NAMES.filter(
  (name) => name !== "error",
);

/** Draw.io's github options (`electron.js:2078-2083`), with and without `channel`. */
const DRAWIO_FEED = {
  provider: "github",
  repo: "drawio-desktop",
  owner: "jgraph",
} as const;
const DRAWIO_FEED_CHANNEL = { ...DRAWIO_FEED, channel: "alpha" } as const;

interface RecordingLogger extends Logger {
  lines: string[];
}

function recordingLogger(): RecordingLogger {
  return {
    lines: [],
    info(message?: unknown) {
      this.lines.push(`info ${String(message)}`);
    },
    warn(message?: unknown) {
      this.lines.push(`warn ${String(message)}`);
    },
    error(message?: unknown) {
      this.lines.push(`error ${String(message)}`);
    },
    debug(message: string) {
      this.lines.push(`debug ${message}`);
    },
  };
}

interface RecordingApp extends AppAdapter {
  quitCalls: number;
}

/** Reads the getter without TypeScript's assignment narrowing of `updater.logger`. */
function currentLogger(updater: AppUpdater): Logger | null {
  return updater.logger;
}

function recordingApp(isPackaged: boolean): RecordingApp {
  return {
    isPackaged,
    quitCalls: 0,
    quit() {
      this.quitCalls += 1;
    },
  };
}

interface Observed {
  counts: Record<string, number>;
  errors: Array<{ error: Error; message: string | undefined }>;
  sequence: string[];
}

/** Listeners for every upstream event name; `sequence` orders events against settlements. */
function observe(updater: AppUpdater): Observed {
  const observed: Observed = { counts: {}, errors: [], sequence: [] };
  for (const name of EVENT_NAMES) {
    observed.counts[name] = 0;
    if (name === "error") {
      updater.on("error", (error, message) => {
        observed.counts.error += 1;
        observed.errors.push({ error, message });
        observed.sequence.push("error");
      });
    } else {
      updater.on(name, () => {
        observed.counts[name] += 1;
        observed.sequence.push(name);
      });
    }
  }
  return observed;
}

function expectNothingFabricated(observed: Observed): void {
  for (const name of NEVER_EMITTED) {
    expect(observed.counts[name]).toBe(0);
  }
}

function fresh(isPackaged: boolean): {
  updater: AppUpdater;
  app: RecordingApp;
  log: RecordingLogger;
  observed: Observed;
} {
  const app = recordingApp(isPackaged);
  const updater = new AppUpdater(null, app);
  const log = recordingLogger();
  updater.logger = log;
  const observed = observe(updater);
  return { updater, app, log, observed };
}

// Negative control: return a wrapper from the getter; strict equality fails and
// the level write no longer lands on `log`.
test("electron-updater.logger.identity: the logger getter returns the assigned object itself, null installs a NoOpLogger, and the default is console", () => {
  const updater = new AppUpdater(null, recordingApp(false));
  expect(updater.logger).toBe(console);
  const log = Object.assign(recordingLogger(), {
    transports: { file: { level: "silly" }, console: { level: "silly" } },
  });
  updater.logger = log;
  expect(updater.logger).toBe(log);
  // `autoUpdater.logger.transports.file.level = 'error'` (electron.js:82).
  (updater.logger as typeof log).transports.file.level = "error";
  expect(log.transports.file.level).toBe("error");
  updater.logger = null;
  const installed = currentLogger(updater);
  expect(installed).toBeInstanceOf(NoOpLogger);
  expect(installed).not.toBeNull();
  expect(installed?.info("ignored")).toBeUndefined();
});

// Negative control: give one field a setter with an observable side effect.
test("electron-updater.flags.plain-booleans: autoDownload, autoInstallOnAppQuit and autoRunAppAfterInstall default to true and read back what is assigned with no event", () => {
  const { updater, observed } = fresh(false);
  expect(updater.autoDownload).toBe(true);
  expect(updater.autoInstallOnAppQuit).toBe(true);
  expect(updater.autoRunAppAfterInstall).toBe(true);
  updater.autoDownload = false;
  updater.autoInstallOnAppQuit = false;
  updater.autoRunAppAfterInstall = false;
  expect(updater.autoDownload).toBe(false);
  expect(updater.autoInstallOnAppQuit).toBe(false);
  expect(updater.autoRunAppAfterInstall).toBe(false);
  expect(observed.counts.error).toBe(0);
  expectNothingFabricated(observed);
});

// Negative control: `throw` inside setFeedURL. Upstream (:234-252) would build a
// provider client; the ▲ is that Keld ignores the application's feed.
test("electron-updater.set-feed-url.no-throw-no-effect: draw.io's github options with and without channel, and a plain string, return undefined, throw nothing and emit nothing", () => {
  const { updater, observed, log } = fresh(true);
  const before = updater.getFeedURL();
  expect(updater.setFeedURL(DRAWIO_FEED)).toBeUndefined();
  expect(updater.setFeedURL(DRAWIO_FEED_CHANNEL)).toBeUndefined();
  expect(updater.setFeedURL("https://example.invalid/feed")).toBeUndefined();
  expect(updater.getFeedURL()).toBe(before);
  expect(observed.counts.error).toBe(0);
  expectNothingFabricated(observed);
  expect(log.lines).toEqual([]);
});

test("electron-updater.get-feed-url.deprecated-literal: getFeedURL returns upstream's literal (:227-229)", () => {
  expect(new AppUpdater(null, recordingApp(false)).getFeedURL()).toBe(
    "Deprecated. Do not use it.",
  );
  expect(autoUpdater.getFeedURL()).toBe("Deprecated. Do not use it.");
});

// Negative control: remove the packaged gate; the unpackaged check takes the
// refusal path and emits an error.
test("electron-updater.check-for-updates.unpackaged-null: with facts not packaged, checkForUpdates resolves null, emits nothing and logs upstream's skip line (:277-284)", async () => {
  const { updater, observed, log } = fresh(false);
  await expect(updater.checkForUpdates()).resolves.toBeNull();
  expect(observed.counts.error).toBe(0);
  expectNothingFabricated(observed);
  expect(log.lines).toEqual([
    "info Skip checkForUpdates because application is not packed and dev update config is not forced",
  ]);
});

// Pinned to today's facade: `@keld/electron`'s app has no isPackaged (#452 open),
// which upstream's `=== true` reads as not packaged, as `keld dev` is.
test("electron-updater.check-for-updates.facade-today-unpackaged: the module autoUpdater reads the Electron facade, which is unpackaged under bun test, so it resolves null with no event", async () => {
  const log = recordingLogger();
  autoUpdater.logger = log;
  const observed = observe(autoUpdater);
  await expect(autoUpdater.checkForUpdates()).resolves.toBeNull();
  expect(observed.counts.error).toBe(0);
  expectNothingFabricated(observed);
  expect(log.lines).toHaveLength(1);
});

// Negative controls: resolve null when packaged (no error, no rejection); emit
// update-not-available instead (fails expectNothingFabricated).
test("electron-updater.check-for-updates.packaged-no-host-updater-error: with facts packaged and no host updater, exactly one error carrying KELD-COMPAT-050 fires, then the same promise rejects with the identical Error", async () => {
  const { updater, observed } = fresh(true);
  const pending = updater.checkForUpdates();
  let rejection: unknown = undefined;
  try {
    await pending;
  } catch (error) {
    rejection = error;
    observed.sequence.push("rejected");
  }
  expect(observed.counts.error).toBe(1);
  expectNothingFabricated(observed);
  expect(observed.sequence).toEqual(["error", "rejected"]);
  const [emitted] = observed.errors;
  expect(emitted).toBeDefined();
  expect(rejection).toBe(emitted?.error);
  expect(isCallError(emitted?.error)).toBe(true);
  expect(isCallError(emitted?.error) && emitted.error.code).toBe(
    UPDATE_CHECK_REFUSED,
  );
  expect(emitted?.error.message.startsWith(`${UPDATE_CHECK_REFUSED}: `)).toBe(
    true,
  );
  // The message states the fix (arch 07 §2).
  expect(emitted?.error.message).toContain("DRAWIO_DISABLE_UPDATE=true");
  // Upstream's second argument shape (:265-268).
  expect(emitted?.message?.startsWith("Cannot check for updates: ")).toBe(true);
  expect(emitted?.message).toContain(UPDATE_CHECK_REFUSED);
});

// Negative controls: reuse a host `KELD-UPDATE-*` code or one of keld-compat's
// 001..009 codes; delete the
// registry heading (the Rust registry test fails too; this is the independent oracle).
test("electron-updater.error-code.provenance: the refusal code is a KELD-COMPAT code outside 001..009, is no KELD-UPDATE host code, and has a registry heading", () => {
  const match = /^KELD-COMPAT-(\d{3})$/.exec(UPDATE_CHECK_REFUSED);
  expect(match).not.toBeNull();
  expect(Number(match?.[1])).toBeGreaterThan(9);
  expect(UPDATE_CHECK_REFUSED.startsWith("KELD-UPDATE-")).toBe(false);
  const registry = readFileSync(
    join(import.meta.dir, "../../../../docs/engineering/keld-error-codes.md"),
    "utf8",
  );
  expect(registry).toContain(`\n## ${UPDATE_CHECK_REFUSED}\n`);
  expect(registry).toContain("- crate: @keld/electron-updater");
});

test("electron-updater.check-for-updates-and-notify.same-outcome: checkForUpdatesAndNotify resolves null unpackaged (logging upstream's debug line) and refuses packaged with one error plus a rejection", async () => {
  const unpackaged = fresh(false);
  await expect(
    unpackaged.updater.checkForUpdatesAndNotify(),
  ).resolves.toBeNull();
  expect(unpackaged.observed.counts.error).toBe(0);
  expectNothingFabricated(unpackaged.observed);
  expect(unpackaged.log.lines).toContain(
    "debug checkForUpdatesAndNotify called, downloadPromise is null",
  );

  const packaged = fresh(true);
  const rejection = await packaged.updater
    .checkForUpdatesAndNotify({ title: "t", body: "b" })
    .then(
      () => undefined,
      (error: unknown) => error,
    );
  expect(packaged.observed.counts.error).toBe(1);
  expectNothingFabricated(packaged.observed);
  expect(rejection).toBe(packaged.observed.errors[0]?.error);
});

// Negative control: resolve with an empty array instead of rejecting.
test("electron-updater.download-update.check-first: downloadUpdate before any offer dispatches one error 'Please check update first' and rejects with the same Error (:435-441)", async () => {
  const { updater, observed } = fresh(true);
  const rejection = await updater.downloadUpdate().then(
    () => undefined,
    (error: unknown) => error,
  );
  expect(observed.counts.error).toBe(1);
  expectNothingFabricated(observed);
  const [emitted] = observed.errors;
  expect(rejection).toBe(emitted?.error);
  expect(emitted?.error.message).toBe("Please check update first");
  // dispatchError's second argument is `(e.stack || e).toString()` (:476-478).
  expect(emitted?.message).toBe(String(emitted?.error.stack));
});

// Negative control: call `this.app.quit()` from quitAndInstall; quitCalls becomes 1.
test("electron-updater.quit-and-install.nothing-staged-no-quit: quitAndInstall logs upstream's line, dispatches one error 'No update filepath provided' and never quits, also on a second call", () => {
  const { updater, observed, app, log } = fresh(true);
  expect(updater.quitAndInstall()).toBeUndefined();
  expect(observed.counts.error).toBe(1);
  expect(observed.errors[0]?.error.message).toBe(
    "No update filepath provided, can't quit and install",
  );
  expect(log.lines[0]).toBe("info Install on explicit quitAndInstall");
  updater.quitAndInstall(true, true);
  expect(observed.counts.error).toBe(2);
  expect(app.quitCalls).toBe(0);
  expectNothingFabricated(observed);
});

// Negative control: emit `update-not-available` from checkForUpdates when no
// host updater exists; the packaged sequence counts it.
test("electron-updater.never-fabricate: across setFeedURL, both check outcomes, downloadUpdate and quitAndInstall, no update-*, download-progress, checking-for-update, login or cancelled event is ever emitted", async () => {
  for (const isPackaged of [false, true]) {
    const { updater, observed, app } = fresh(isPackaged);
    updater.setFeedURL(DRAWIO_FEED);
    await updater.checkForUpdates().catch(() => undefined);
    await updater.checkForUpdatesAndNotify().catch(() => undefined);
    await updater.downloadUpdate().catch(() => undefined);
    updater.quitAndInstall();
    expectNothingFabricated(observed);
    // Only refusals and the two upstream match errors reach `error`.
    expect(observed.counts.error).toBe(isPackaged ? 4 : 2);
    expect(app.quitCalls).toBe(0);
  }
});

// Negative control: remove the constructor's own `error` listener; the emitter
// throws ERR_UNHANDLED_ERROR synchronously from downloadUpdate.
test("electron-updater.error-event.no-app-listener-logs: with no application error listener, a dispatched error is logged through the assigned logger instead of thrown by the emitter (:198-200)", async () => {
  const updater = new AppUpdater(null, recordingApp(true));
  const log = recordingLogger();
  updater.logger = log;
  expect(updater.listenerCount("error")).toBe(1);
  const rejection = await updater.downloadUpdate().then(
    () => undefined,
    (error: unknown) => error,
  );
  expect(rejection).toBeInstanceOf(Error);
  expect(log.lines).toHaveLength(1);
  expect(
    log.lines[0]?.startsWith("error Error: Error: Please check update first"),
  ).toBe(true);
});

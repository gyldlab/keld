/**
 * Replays the PANEL-P3-recorded draw.io updater call sequence against the
 * adapter (GH-491 spec §3 criterion 10). Source of the sequence: drawio-desktop
 * `src/main/electron.js` at the #589 pin — `:74-86` configuration, `:2228-2355`
 * module-level listeners, then the `whenReady` tail: `:2078-2083` `setFeedURL`
 * through `safeUpdaterCall` outside the guard and `:2092-2100` the guarded boot
 * check. This file copies none of draw.io's code: it performs the same calls in
 * the same order behind the same guard and reports what it observed as one
 * `GH491_RESULT` JSON line.
 *
 * Environment, set or cleared explicitly by the test, never inherited:
 * - `DRAWIO_DISABLE_UPDATE`: read exactly as draw.io reads it (`=== "true"`).
 * - `GH491_PACKAGED=1`: drive an `AppUpdater` built on injected application
 *   facts that report packaged with no host updater, instead of the module
 *   `autoUpdater` whose Electron facade reports the F01-T4 fact (unpackaged
 *   until #452 mints it).
 */
import electronUpdater, { AppUpdater, type Logger } from "../src/index.ts";

const { autoUpdater } = electronUpdater;

/** The six names draw.io listens to, plus the three it does not, all counted. */
const EVENT_NAMES = [
  "error",
  "update-not-available",
  "download-progress",
  "update-downloaded",
  "update-available",
  "checking-for-update",
  "login",
  "update-cancelled",
  "appimage-filename-updated",
] as const;

interface ElectronLogShape extends Logger {
  transports: { file: { level: string }; console: { level: string } };
}

/** An electron-log-shaped logger (`import log from 'electron-log'`, `:9`). */
const log: ElectronLogShape & { lines: string[] } = {
  lines: [],
  transports: { file: { level: "silly" }, console: { level: "silly" } },
  info(message?: unknown) {
    this.lines.push(`info ${String(message)}`);
  },
  warn(message?: unknown) {
    this.lines.push(`warn ${String(message)}`);
  },
  error(message?: unknown) {
    this.lines.push(`error ${String(message)}`);
  },
};

let quitCalls = 0;
const updater =
  process.env.GH491_PACKAGED === "1"
    ? new AppUpdater(null, {
        isPackaged: true,
        quit() {
          quitCalls += 1;
        },
      })
    : autoUpdater;

// :74-78 — the guard, from the role environment only.
const disableUpdate = process.env.DRAWIO_DISABLE_UPDATE === "true";
const silentUpdate = !disableUpdate;

// :81-86 — unconditional configuration, written through the logger getter.
updater.logger = log;
const viaGetter = updater.logger as ElectronLogShape | null;
const loggerIdentity = viaGetter === log;
if (viaGetter !== null && "transports" in viaGetter) {
  viaGetter.transports.file.level = "error";
  viaGetter.transports.console.level = "error";
}
const levelsLanded =
  log.transports.file.level === "error" &&
  log.transports.console.level === "error";
updater.autoDownload = false;
updater.autoInstallOnAppQuit = silentUpdate;

/** The `code` field a typed Keld refusal carries, or null for a plain Error. */
function codeOf(error: unknown): string | null {
  if (!(error instanceof Error)) return null;
  const { code } = error as Error & { code?: unknown };
  return typeof code === "string" ? code : null;
}

// :124-141 — draw.io's safeUpdaterCall: a synchronous throw or a rejection
// both reach notifyUpdateFailure (the Update Error dialog). Recorded here.
const syncThrows: string[] = [];
const rejections: Array<{
  label: string;
  code: string | null;
  message: string;
}> = [];
function safeUpdaterCall(label: string, fn: () => unknown): unknown {
  try {
    const result = fn();
    if (
      result !== null &&
      typeof result === "object" &&
      "catch" in result &&
      typeof result.catch === "function"
    ) {
      (result as Promise<unknown>).catch((err: unknown) => {
        rejections.push({
          label,
          code: codeOf(err),
          message: err instanceof Error ? err.message : String(err),
        });
      });
    }
    return result;
  } catch (error) {
    syncThrows.push(
      `${label}: ${error instanceof Error ? error.message : String(error)}`,
    );
    return undefined;
  }
}

// :2228-2355 — module-level listeners (error twice, as draw.io registers it twice).
const events: Record<string, number> = {};
const errorCodes: Array<string | null> = [];
for (const name of EVENT_NAMES) events[name] = 0;
updater.on("error", (e) => {
  events.error += 1;
  errorCodes.push(codeOf(e));
});
updater.on("error", () => {
  // the second draw.io `error` listener (`:2264`) only drives its progress bar
});
for (const name of EVENT_NAMES) {
  if (name === "error") continue;
  updater.on(name, () => {
    events[name] += 1;
  });
}

// whenReady tail. :2078-2083 setFeedURL runs outside the guard.
let setFeedURLReturn = "unset";
safeUpdaterCall("setFeedURL", () => {
  const returned = updater.setFeedURL({
    provider: "github",
    repo: "drawio-desktop",
    owner: "jgraph",
  });
  setFeedURLReturn = typeof returned;
  return returned;
});

// :2092-2100 — first boot, no store: the check runs unless disabled.
let checkForUpdatesCalls = 0;
let checkOutcome = "not-called";
if (!disableUpdate) {
  checkForUpdatesCalls += 1;
  const result = safeUpdaterCall("checkForUpdates (boot)", () =>
    updater.checkForUpdates(),
  ) as Promise<unknown> | undefined;
  if (result !== undefined) {
    try {
      const value = await result;
      checkOutcome = value === null ? "null" : `resolved:${typeof value}`;
    } catch (error) {
      checkOutcome = `rejected:${codeOf(error) ?? "no-code"}`;
    }
  }
}

// Let queued listeners and rejections flush before reporting.
await new Promise<void>((resolve) => setImmediate(resolve));
await new Promise<void>((resolve) => setImmediate(resolve));

process.stdout.write(
  `GH491_RESULT ${JSON.stringify({
    disableUpdate,
    packagedFacts: process.env.GH491_PACKAGED === "1",
    loggerIdentity,
    levelsLanded,
    setFeedURLReturn,
    checkForUpdatesCalls,
    checkOutcome,
    events,
    errorCodes,
    rejections,
    syncThrows,
    quitCalls,
    getFeedURL: updater.getFeedURL(),
    logLines: log.lines,
  })}\n`,
);

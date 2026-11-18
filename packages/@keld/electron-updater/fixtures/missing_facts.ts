/**
 * Exercise the synchronous application-fact boundary under the refusing I/O
 * probe. This injected error is a test sentinel, not a Host-delivered fact or
 * a production error-code allocation. electron-updater 6.8.9 reads its
 * AppAdapter.isPackaged getter before constructing a check Promise.
 */
import {
  AppUpdater,
  type AppAdapter,
  type UpdaterEvent,
} from "../src/index.ts";

const unavailable = Object.assign(new Error("app.isPackaged is unavailable"), {
  code: "TEST-APP-FACT-UNAVAILABLE",
});
let factReads = 0;
let quitCalls = 0;
const app: AppAdapter = {
  get isPackaged(): boolean {
    factReads += 1;
    throw unavailable;
  },
  quit() {
    quitCalls += 1;
  },
};
const updater = new AppUpdater(null, app);
const constructorReads = factReads;
const logLines: unknown[] = [];
updater.logger = {
  info: (line) => logLines.push(line),
  warn: (line) => logLines.push(line),
  error: (line) => logLines.push(line),
  debug: (line) => logLines.push(line),
};
const events: Record<string, number> = {};
const names: readonly UpdaterEvent[] = [
  "error",
  "update-not-available",
  "download-progress",
  "update-downloaded",
  "update-available",
  "checking-for-update",
  "login",
  "update-cancelled",
  "appimage-filename-updated",
];
for (const name of names) {
  events[name] = 0;
  updater.on(name, () => {
    events[name] += 1;
  });
}

const outcomes: Array<{
  method: "checkForUpdates" | "checkForUpdatesAndNotify";
  returnedPromise: boolean;
  sameError: boolean;
  code: unknown;
  factReads: number;
}> = [];
for (const method of ["checkForUpdates", "checkForUpdatesAndNotify"] as const) {
  const readsBefore = factReads;
  let returned: Promise<unknown> | undefined;
  let thrown: unknown;
  try {
    returned = updater[method]();
  } catch (error) {
    thrown = error;
  }
  // Consume an incorrectly returned rejection so the parent asserts the
  // timing defect directly instead of relying on an unhandled rejection.
  await returned?.catch(() => undefined);
  outcomes.push({
    method,
    returnedPromise: returned !== undefined,
    sameError: thrown === unavailable,
    code: thrown instanceof Error && "code" in thrown ? thrown.code : null,
    factReads: factReads - readsBefore,
  });
}

// Observe queued callbacks before publishing the event counts, as in replay.ts.
await new Promise<void>((resolve) => setImmediate(resolve));

process.stdout.write(
  `GH491_RESULT ${JSON.stringify({
    constructorReads,
    outcomes,
    events,
    quitCalls,
    logLines,
  })}\n`,
);

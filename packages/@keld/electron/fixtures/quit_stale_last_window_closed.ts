/** A Host LastWindowClosed can predate a new, already-mirrored replacement. */
import assert from "node:assert/strict";
import { mock } from "bun:test";

if (process.env.KELD_NATIVE_WINDOW_FACADE !== "1") {
  throw new Error(
    "fixture requires native facade selection in the launch environment",
  );
}
let notifyLastWindow: () => void = () => {
  throw new Error("the facade omitted the one last-window-closed listener");
};
let quitCalls = 0;
let live = true;
mock.module("../../api/src/app.ts", () => ({
  app: {
    isReady: () => true,
    whenReady: async () => {},
    quit: async () => {
      quitCalls += 1;
    },
    onLastWindowClosed: (listener: () => void) => {
      notifyLastWindow = listener;
      return () => {};
    },
  },
  hasObservedHostReady: () => true,
  getWindowSnapshots: () => (live ? [{ host_destroyed: false }] : []),
}));
const { app } = await import("../src/app.ts");
notifyLastWindow();
await Promise.resolve();
assert.equal(
  quitCalls,
  0,
  "queued LastWindowClosed must not kill a new live document",
);
process.env.KELD_NATIVE_WINDOW_FACADE = "0";
await assert.rejects(app.quit(), /KELD-COMPAT-012/);
assert.equal(
  quitCalls,
  0,
  "an app may not disable the boot-scoped Quit safety gate",
);
let allClosedEvents = 0;
const onAllClosed = () => {
  allClosedEvents += 1;
};
app.on("window-all-closed", onAllClosed);
notifyLastWindow();
assert.equal(
  allClosedEvents,
  0,
  "the queued zero-window event must not escape to a user listener after a replacement was created",
);
live = false;
notifyLastWindow();
assert.equal(
  allClosedEvents,
  1,
  "the actual last window still emits to an enrolled listener",
);
assert.equal(
  quitCalls,
  0,
  "an enrolled window-all-closed listener may keep the app alive",
);
app.off("window-all-closed", onAllClosed);
notifyLastWindow();
await Promise.resolve();
assert.equal(
  quitCalls,
  1,
  "real zero-window LastWindowClosed keeps the ordinary quit",
);
console.log("KELD_QUIT_STALE_LAST_WINDOW_SAFE");

/**
 * Electron default for `window-all-closed` (no host).
 *
 * Oracle: https://www.electronjs.org/docs/latest/api/app#event-window-all-closed
 * "If you do not subscribe to this event and all windows are closed, the
 * default behavior is to quit the app; however, if you subscribe, you
 * control whether the app quits or not."
 *
 * - LastWindowClosed with **no** `app.on("window-all-closed")` must call
 *   `link.quit()` (kipc `Quit`). A shim that only `emit`s leaves the host
 *   running — that is the CodeRabbit finding this fixture falsifies.
 * - After a listener is added, a later LastWindowClosed must emit and must
 *   not auto-quit. A throwing listener must not skip a later one (the same
 *   isolation as `ready`: an uncaught throw would abort the kipc read loop).
 *
 * Spawned as a child by `src/app.test.ts` so stubbing `LifecycleLink.connect`
 * cannot leak into `link.test.ts` in the same bun-test process.
 */
import { writeSync } from "node:fs";
import { LifecycleLink } from "../src/link.ts";

function marker(line: string): void {
  writeSync(1, `${line}\n`);
}

if (!process.env.KELD_APP_LINK) {
  process.env.KELD_APP_LINK = `/tmp/keld-kel72-unused.sock#${"a".repeat(64)}`;
}

type Handlers = {
  onReady: () => void;
  onLastWindowClosed: () => void;
  onLinkDead: (err: Error) => void;
};

let quitCalls = 0;
let lastHandlers: Handlers | undefined;

LifecycleLink.connect = (async (_link: string, handlers: Handlers) => {
  lastHandlers = handlers;
  return {
    async quit() {
      quitCalls += 1;
    },
    close() {},
  };
}) as typeof LifecycleLink.connect;

const { app } = await import("../src/app.ts");

const ready = app.whenReady();
await new Promise<void>((resolve) => {
  setImmediate(resolve);
});

if (!lastHandlers) {
  writeSync(2, "KEL72_CONNECT_NOT_CALLED\n");
  process.exit(1);
}

lastHandlers.onReady();
await ready;

// Phase 1: no window-all-closed listener → Electron default app.quit().
lastHandlers.onLastWindowClosed();
await new Promise<void>((resolve) => {
  setImmediate(resolve);
});
if (quitCalls !== 1) {
  writeSync(2, `KEL72_DEFAULT_QUIT_MISSING calls=${quitCalls}\n`);
  process.exit(1);
}
marker("KEL72_DEFAULT_QUIT");

// Phase 2: a subscriber owns quit. Isolation: a throw must not skip the
// later listener or increment the auto-quit count.
app.on("window-all-closed", () => {
  throw new Error("KEL72_WINDOW_ALL_CLOSED_THROW");
});
app.on("window-all-closed", () => {
  marker("KEL72_WINDOW_ALL_CLOSED_SECOND");
});
lastHandlers.onLastWindowClosed();
await new Promise<void>((resolve) => {
  setImmediate(resolve);
});
if (quitCalls !== 1) {
  writeSync(2, `KEL72_AUTOQUIT_WITH_LISTENER calls=${quitCalls}\n`);
  process.exit(1);
}

marker(`KEL72_QUIT_CALLS=${quitCalls}`);
process.exit(0);

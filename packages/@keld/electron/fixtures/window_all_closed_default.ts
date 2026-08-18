/**
 * Electron default for `window-all-closed` (no host).
 *
 * Oracle: https://www.electronjs.org/docs/latest/api/app#event-window-all-closed
 * "If you do not subscribe to this event and all windows are closed, the
 * default behavior is to quit the app; however, if you subscribe, you
 * control whether the app quits or not."
 *
 * Electron's own default is a built-in listener that quits when
 * `listenerCount('window-all-closed') === 1` (`lib/browser/init.ts`).
 * Removing the last *user* subscriber therefore restores default quit;
 * `removeListener` / `off` are Node EventEmitter
 * (https://nodejs.org/docs/latest/api/events.html#emitterremovelistenereventname-listener).
 *
 * - LastWindowClosed with **no** `app.on("window-all-closed")` must call
 *   `link.quit()` (kipc `Quit`). A shim that only `emit`s leaves the host
 *   running — that is the CodeRabbit finding this fixture falsifies.
 * - After a listener is added, a later LastWindowClosed must emit and must
 *   not auto-quit. A throwing listener must not skip a later one (the same
 *   isolation as `ready`: an uncaught throw would abort the kipc read loop).
 * - After `removeListener` drops the last subscriber, default quit returns.
 * - After `off` drops one of two subscribers, only the remaining listener
 *   runs and auto-quit must not fire.
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

function flush(): Promise<void> {
  return new Promise((resolve) => {
    setImmediate(resolve);
  });
}

// Phase 1: no window-all-closed listener → Electron default app.quit().
lastHandlers.onLastWindowClosed();
await flush();
if (quitCalls !== 1) {
  writeSync(2, `KEL72_DEFAULT_QUIT_MISSING calls=${quitCalls}\n`);
  process.exit(1);
}
marker("KEL72_DEFAULT_QUIT");

// Phase 2: a subscriber owns quit. Isolation: a throw must not skip the
// later listener or increment the auto-quit count.
const thrower = (): void => {
  throw new Error("KEL72_WINDOW_ALL_CLOSED_THROW");
};
const second = (): void => {
  marker("KEL72_WINDOW_ALL_CLOSED_SECOND");
};
app.on("window-all-closed", thrower);
app.on("window-all-closed", second);
lastHandlers.onLastWindowClosed();
await flush();
if (quitCalls !== 1) {
  writeSync(2, `KEL72_AUTOQUIT_WITH_LISTENER calls=${quitCalls}\n`);
  process.exit(1);
}

// Phase 3: removeListener of every subscriber restores default quit.
app.removeListener("window-all-closed", thrower);
app.removeListener("window-all-closed", second);
lastHandlers.onLastWindowClosed();
await flush();
if (quitCalls !== 2) {
  writeSync(2, `KEL72_DEFAULT_QUIT_AFTER_REMOVE_MISSING calls=${quitCalls}\n`);
  process.exit(1);
}
marker("KEL72_DEFAULT_QUIT_AFTER_REMOVE");

// Phase 4: off() one of two subscribers → remaining emits, no auto-quit.
const dropped = (): void => {
  writeSync(2, "KEL72_DROPPED_FIRED\n");
  process.exit(1);
};
const remaining = (): void => {
  marker("KEL72_REMAINING");
};
app.on("window-all-closed", dropped);
app.on("window-all-closed", remaining);
app.off("window-all-closed", dropped);
lastHandlers.onLastWindowClosed();
await flush();
if (quitCalls !== 2) {
  writeSync(2, `KEL72_AUTOQUIT_WITH_REMAINING calls=${quitCalls}\n`);
  process.exit(1);
}

marker(`KEL72_QUIT_CALLS=${quitCalls}`);
process.exit(0);

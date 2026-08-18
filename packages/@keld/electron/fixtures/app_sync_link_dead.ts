/**
 * Sync `onLinkDead` during `connect()` (no host).
 *
 * Oracle: a connect stub that fires `onLinkDead` before returning must not
 * throw TDZ on the session token, must reject `whenReady()` with KELD-IPC-001,
 * and must drop `linkPromise` so the next `whenReady()` retries connect.
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

let connectCalls = 0;
let lastHandlers: Handlers | undefined;

LifecycleLink.connect = (async (_link: string, handlers: Handlers) => {
  connectCalls += 1;
  lastHandlers = handlers;
  if (connectCalls === 1) {
    handlers.onLinkDead(new Error("KELD-IPC-001: connection closed by peer"));
  }
  return {
    async quit() {},
    close() {},
  };
}) as typeof LifecycleLink.connect;

const { app } = await import("../src/app.ts");

const firstErr = await app.whenReady().then(
  () => null,
  (err: unknown) => err,
);

if (!(firstErr instanceof Error) || !String(firstErr).includes("KELD-IPC-001")) {
  writeSync(2, `KEL72_SYNC_DEAD_SHOULD_REJECT=${String(firstErr)}\n`);
  process.exit(1);
}
if (String(firstErr).includes("before initialization")) {
  writeSync(2, `KEL72_SYNC_DEAD_TDZ=${String(firstErr)}\n`);
  process.exit(1);
}
if (app.isReady()) {
  writeSync(2, "KEL72_IS_READY_AFTER_SYNC_DEATH\n");
  process.exit(1);
}
marker("KEL72_SYNC_DEAD");

const retry = app.whenReady();
await new Promise<void>((resolve) => {
  setImmediate(resolve);
});

if (connectCalls !== 2) {
  writeSync(2, `KEL72_SYNC_DEAD_NOT_RETRIED calls=${connectCalls}\n`);
  process.exit(1);
}
if (!lastHandlers) {
  writeSync(2, "KEL72_SYNC_DEAD_RETRY_HANDLERS_MISSING\n");
  process.exit(1);
}

lastHandlers.onReady();
await retry;
marker("KEL72_SYNC_DEAD_RETRY_READY");
marker(`KEL72_CONNECT_CALLS=${connectCalls}`);

if (!app.isReady()) {
  writeSync(2, "KEL72_NOT_READY_AFTER_SYNC_DEAD_RETRY\n");
  process.exit(1);
}

process.exit(0);

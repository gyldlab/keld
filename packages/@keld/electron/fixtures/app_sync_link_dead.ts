/**
 * Sync `onLinkDead` during `connect()` (no host).
 *
 * Oracle: a connect stub that fires `onLinkDead` before returning must not
 * throw TDZ, must reject `whenReady()` with KELD-IPC-001, and stays the
 * answer (GH-528 T3): the next `whenReady()` rejects with the same error and
 * connect is not attempted again (one link per realm).
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

LifecycleLink.connect = (async (_link: string, handlers: Handlers) => {
  connectCalls += 1;
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

const again = await app.whenReady().then(
  () => null,
  (err: unknown) => err,
);
await new Promise<void>((resolve) => {
  setImmediate(resolve);
});
if (again !== firstErr) {
  writeSync(2, `KEL72_SYNC_DEAD_NOT_STICKY again=${String(again)}\n`);
  process.exit(1);
}
if (connectCalls !== 1) {
  writeSync(2, `KEL72_SYNC_DEAD_RETRIED calls=${connectCalls}\n`);
  process.exit(1);
}
marker("KEL72_SYNC_DEAD_STICKY");
marker(`KEL72_CONNECT_CALLS=${connectCalls}`);
process.exit(0);

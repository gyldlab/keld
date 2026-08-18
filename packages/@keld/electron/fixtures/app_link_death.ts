/**
 * HELLO-then-death probe for `app.ts` (no host).
 *
 * Oracles:
 * - Link death after connect (before Ready) must reject `whenReady()` with
 *   a typed kipc error (`KELD-IPC-001`), not hang.
 * - `linkPromise` must be dropped so a later `whenReady()` retries connect.
 * - A throwing `ready` listener must not skip waiter drain or the retry.
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
  return {
    async quit() {},
    close() {},
  };
}) as typeof LifecycleLink.connect;

const { app } = await import("../src/app.ts");

app.on("ready", () => {
  throw new Error("KEL72_READY_LISTENER_THROW");
});

const ready = app.whenReady();
await new Promise<void>((resolve) => {
  setImmediate(resolve);
});

if (!lastHandlers) {
  writeSync(2, "KEL72_CONNECT_NOT_CALLED\n");
  process.exit(1);
}

const afterHelloCalls = connectCalls;
lastHandlers.onLinkDead(new Error("KELD-IPC-001: connection closed by peer"));

const firstErr = await ready.then(
  () => null,
  (err: unknown) => err,
);

if (!(firstErr instanceof Error) || !String(firstErr).includes("KELD-IPC-001")) {
  writeSync(2, `KEL72_DEATH_SHOULD_REJECT=${String(firstErr)}\n`);
  process.exit(1);
}
if (app.isReady()) {
  writeSync(2, "KEL72_IS_READY_AFTER_DEATH\n");
  process.exit(1);
}
marker("KEL72_WHEN_READY_DEAD");

const retry = app.whenReady();
await new Promise<void>((resolve) => {
  setImmediate(resolve);
});

if (connectCalls < afterHelloCalls + 1) {
  writeSync(2, `KEL72_CONNECT_NOT_RETRIED calls=${connectCalls} afterHello=${afterHelloCalls}\n`);
  process.exit(1);
}
if (!lastHandlers) {
  writeSync(2, "KEL72_RETRY_HANDLERS_MISSING\n");
  process.exit(1);
}

lastHandlers.onReady();
await retry;
marker("KEL72_RETRY_READY");
marker(`KEL72_CONNECT_CALLS=${connectCalls}`);

if (!app.isReady()) {
  writeSync(2, "KEL72_NOT_READY_AFTER_RETRY\n");
  process.exit(1);
}

process.exit(0);

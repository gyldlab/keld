/**
 * HELLO-then-death probe for `app.ts` (no host).
 *
 * Oracles:
 * - Link death after connect (before Ready) must reject `whenReady()` with
 *   the typed error that ended the link, not hang.
 * - The death is sticky (GH-528 T3): a later `whenReady()` rejects with the
 *   same error, and connect is not attempted again (one link per realm).
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

const again = await app.whenReady().then(
  () => null,
  (err: unknown) => err,
);
await new Promise<void>((resolve) => {
  setImmediate(resolve);
});
if (again !== firstErr) {
  writeSync(2, `KEL72_DEATH_NOT_STICKY again=${String(again)}\n`);
  process.exit(1);
}
if (connectCalls !== afterHelloCalls) {
  writeSync(2, `KEL72_CONNECT_RETRIED calls=${connectCalls} afterHello=${afterHelloCalls}\n`);
  process.exit(1);
}
marker("KEL72_DEATH_STICKY");
marker(`KEL72_CONNECT_CALLS=${connectCalls}`);
process.exit(0);

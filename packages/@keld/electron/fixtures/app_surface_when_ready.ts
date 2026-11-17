/**
 * Observer for the GH-445 cell `app.when-ready.is-ready-agreement` (Electron
 * v44.4.5 app.md, `app.whenReady()`: "fulfilled when Electron is initialized.
 * May be used as a convenient alternative to checking `app.isReady()` and
 * subscribing to the `ready` event if the app is not ready yet.").
 *
 * `LifecycleLink.connect` is replaced, so this observer delivers host Ready
 * itself through the captured handlers. The live kipc path is `lifecycle.ts`.
 * "Not called" is observed through two event-loop turns after the late
 * `whenReady` settles; a replay scheduled later than that is outside this
 * observable. It prints the three `GH445_*` lines below and exits 0, or exits 2
 * when the facade never opened the link. `src/app-surface.test.ts` holds the
 * oracle.
 */
import { writeSync } from "node:fs";
import { LifecycleLink } from "../src/link.ts";

type Handlers = Parameters<typeof LifecycleLink.connect>[1];

let handlers: Handlers | undefined;
LifecycleLink.connect = (async (_link: string, received: Handlers) => {
  handlers = received;
  return {
    async quit() {},
    close() {},
  };
}) as typeof LifecycleLink.connect;

const { app } = await import("../src/index.ts");

function marker(line: string): void {
  writeSync(1, `${line}\n`);
}

/** One event-loop turn: every queued promise reaction runs before it resolves. */
function settled(): Promise<void> {
  return new Promise<void>((resolve) => {
    setImmediate(resolve);
  });
}

let fulfilled = false;
let readyAtFulfil: boolean | undefined;
const first = app.whenReady().then(() => {
  fulfilled = true;
  readyAtFulfil = app.isReady();
});
await settled();
marker(`GH445_BEFORE_READY isReady=${app.isReady()} fulfilled=${fulfilled}`);
if (!handlers) {
  writeSync(2, "GH445_LINK_NOT_OPENED\n");
  process.exit(2);
}

handlers.onReady();
await first;
marker(`GH445_AT_FULFIL isReady=${readyAtFulfil}`);

let lateListener = 0;
app.on("ready", () => {
  lateListener += 1;
});
let lateFulfilled = false;
await app.whenReady().then(() => {
  lateFulfilled = true;
});
await settled();
await settled();
marker(
  `GH445_AFTER_READY isReady=${app.isReady()} fulfilled=${lateFulfilled} lateListener=${lateListener}`,
);
process.exit(0);

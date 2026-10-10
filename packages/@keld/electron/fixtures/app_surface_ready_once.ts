/**
 * Observer for the GH-445 cell `app.ready.emitted-once` (Electron v44.4.5
 * app.md, Event: 'ready': "Emitted once, when Electron has finished
 * initializing.").
 *
 * `LifecycleLink.connect` is replaced, so this observer delivers host Ready
 * itself through the captured handlers, twice, as the link would for two Ready
 * frames. The live kipc path is `lifecycle.ts`. It uses neither `whenReady`
 * nor `isReady`, so those cells cannot change what it reports. It prints one
 * `GH445_READY_ONCE` line and exits 0, or exits 2 when the facade never opened
 * the link. `src/app-surface.test.ts` holds the oracle.
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

/** One event-loop turn: every queued promise reaction runs before it resolves. */
function settled(): Promise<void> {
  return new Promise<void>((resolve) => {
    setImmediate(resolve);
  });
}

let calls = 0;
app.on("ready", () => {
  calls += 1;
});
await settled();
if (!handlers) {
  writeSync(2, "GH445_LINK_NOT_OPENED\n");
  process.exit(2);
}
const before = calls;
handlers.onReady();
await settled();
const after = calls;
handlers.onReady();
await settled();

writeSync(1, `GH445_READY_ONCE before=${before} after=${after} repeat=${calls}\n`);
process.exit(0);

/**
 * Observer for a `ready` subscriber registered only after the API has received
 * Host Ready. Importing the Electron facade alone must not retroactively emit
 * its one-shot `ready` event to this later subscriber.
 *
 * The intercepted app-link reports Host Ready through the same handler interface
 * as the live link. A separate process isolates the module's singleton state.
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

const { app: apiApp } = await import("../../api/src/app.ts");
const { app } = await import("../src/index.ts");
const apiReady = apiApp.whenReady();
await new Promise<void>((resolve) => setImmediate(resolve));
if (!handlers) {
  writeSync(2, "GH445_LINK_NOT_OPENED\n");
  process.exit(2);
}

handlers.onReady();
await apiReady;
if (!apiApp.isReady() || !app.isReady()) {
  writeSync(2, "GH445_API_HOST_NOT_READY\n");
  process.exit(2);
}

let lateListener = 0;
app.on("ready", () => {
  lateListener += 1;
});
await new Promise<void>((resolve) => setImmediate(resolve));
await new Promise<void>((resolve) => setImmediate(resolve));
writeSync(
  1,
  `GH445_API_FIRST_READY hostReady=true lateListener=${lateListener}\n`,
);
process.exit(0);

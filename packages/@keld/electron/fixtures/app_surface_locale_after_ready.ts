/**
 * Observer for the GH-445 cell `app.get-locale.after-ready`. Pinned v44.4.5
 * app.md says of `app.getLocale()`: "This API must be called after the `ready`
 * event is emitted." So this observer calls it from after the `ready` event.
 *
 * `LifecycleLink.connect` is replaced, so this observer delivers host Ready
 * itself through the captured handlers. The live kipc path is `lifecycle.ts`.
 * It reaches Ready through a `ready` listener, not `whenReady` or `isReady`, so
 * those cells cannot change what it reports (explicit edge: it needs `ready` to
 * be emitted at all). It prints one `GH445_LOCALE_AFTER_READY` line and exits 0,
 * or exits 2 when the facade never opened the link. `src/app-surface.test.ts`
 * holds the oracle.
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

const ready = new Promise<void>((resolve) => {
  app.on("ready", resolve);
});
await new Promise<void>((resolve) => {
  setImmediate(resolve);
});
if (!handlers) {
  writeSync(2, "GH445_LINK_NOT_OPENED\n");
  process.exit(2);
}
handlers.onReady();
await ready;

let outcome: string;
try {
  const locale = (app as unknown as { getLocale(): unknown }).getLocale();
  outcome = `returned=${typeof locale}`;
} catch (error) {
  outcome = `threw=${error instanceof TypeError ? "TypeError" : String(error)}`;
}
writeSync(1, `GH445_LOCALE_AFTER_READY has=${Reflect.has(app, "getLocale")} ${outcome}\n`);
process.exit(0);

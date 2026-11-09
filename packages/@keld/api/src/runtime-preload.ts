/** Host-selected cold wrapper, imports the same stamped KIPC sidecar. */
import { isMainThread, parentPort } from "node:worker_threads";
import { runtimeProcessFactsSelected } from "@keld/kipc";
import { preloadRuntimeProcessFacts } from "./app.ts";
// Transport Workers inherit --preload. Only the real role-main realm captures
// metadata and starts the API-owned singleton; the transport module has no TLA.
// Bun Workers inherit --preload; a secondary transport Worker cannot create
// another authenticated Host link, even if a runtime misreports isMainThread.
// A genuine role-main realm has no parentPort.
if (isMainThread && parentPort === null && runtimeProcessFactsSelected) {
  await preloadRuntimeProcessFacts(process.versions);
}

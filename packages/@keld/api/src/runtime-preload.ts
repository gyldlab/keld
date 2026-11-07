/** Host-selected cold wrapper, imports the same stamped KIPC sidecar. */
import { isMainThread } from "node:worker_threads";
import { runtimeProcessFactsSelected } from "@keld/kipc";
import { preloadRuntimeProcessFacts } from "./app.ts";
// Transport Workers inherit --preload. Only the real role-main realm captures
// metadata and starts the API-owned singleton; the transport module has no TLA.
if (isMainThread && runtimeProcessFactsSelected) {
  await preloadRuntimeProcessFacts(process.versions);
}

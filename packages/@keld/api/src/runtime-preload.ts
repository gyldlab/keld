/** Host-selected cold wrapper, imports the same stamped KIPC sidecar. */
import { isMainThread, parentPort } from "node:worker_threads";
import { isAbsolute } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { linkError, runtimeProcessFactsSelected } from "@keld/kipc";
import { bootHostSelectedEntry, preloadRuntimeProcessFacts } from "./app.ts";
// Transport Workers inherit --preload. Only the real role-main realm captures
// metadata and starts the API-owned singleton; the transport module has no TLA.
// Bun Workers inherit --preload; a secondary transport Worker cannot create
// another authenticated Host link, even if a runtime misreports isMainThread.
// A genuine role-main realm has no parentPort.
if (import.meta.main) {
  // The installed Guardian selects this exact graph-verified package member
  // as the entry on runtimes that isolate CLI preload and entry module caches.
  // argv[2] is the original entry already admitted by the retained boot/graph.
  // It is not read from environment, a user import, or mutable app state.
  const verifiedEntry = process.argv[2];
  if (
    !isMainThread ||
    parentPort !== null ||
    !runtimeProcessFactsSelected ||
    process.argv.length !== 3 ||
    typeof verifiedEntry !== "string" ||
    !isAbsolute(verifiedEntry) ||
    verifiedEntry === fileURLToPath(import.meta.url)
  ) {
    throw linkError(
      "KELD-IPC-005",
      "Host-selected original entry requires an admitted absolute path and main role.",
    );
  }
  // No second module cache: the original entry loads in the same ESM graph
  // only after genuine Host Init ACK and both subscriptions have completed.
  // This changes import.meta.main/Bun.main of the imported entry, so the
  // installed original app must pass compatibility and CODEOWNERS review.
  await bootHostSelectedEntry(() => import(pathToFileURL(verifiedEntry).href));
} else if (isMainThread && parentPort === null && runtimeProcessFactsSelected) {
  // Keep legacy CLI preload behavior where that runtime shares the module
  // graph; no new automatic fallback or retry after a failed first link.
  await preloadRuntimeProcessFacts(process.versions);
}

import { isMainThread } from "node:worker_threads";
// Test-only observer: this module does not import the API or start a link.
// The production command contains only runtime-preload as its one --preload.
if (isMainThread) {
  globalThis.__keld_test_entry = false;
  process.on("message", (message) => {
    if (message?.kind === "probe")
      process.send?.({ kind: "probe", entered: globalThis.__keld_test_entry });
  });
}

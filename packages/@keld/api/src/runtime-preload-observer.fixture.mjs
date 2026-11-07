import { isMainThread } from "node:worker_threads";
// Test-only observer: it neither imports the API nor starts an app-link.
if (isMainThread) {
  globalThis.__keld_test_entry = false;
  process.on("message", (message) => {
    if (message?.kind === "probe")
      process.send?.({ kind: "probe", entered: globalThis.__keld_test_entry });
  });
}

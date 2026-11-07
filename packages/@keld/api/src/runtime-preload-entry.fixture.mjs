import { app } from "./app.ts";
globalThis.__keld_test_entry = true;
const before = { ...process.versions };
process.versions.__keld_late = "changed-after-entry";
process.send?.({ kind: "entry", before, after: { ...process.versions } });
await app.whenReady();
process.send?.({ kind: "ready" });
process.on("message", (message) => {
  if (message?.kind === "finish") process.exit(0);
});

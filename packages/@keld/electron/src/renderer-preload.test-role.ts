/** GH567 test-owned child: construct the current facade against the parent Host. */
import { writeSync } from "node:fs";
import { app, BrowserWindow, type BrowserWindowOptions } from "./index.ts";

const raw = process.argv[2];
if (raw === undefined)
  throw new Error("GH567 BrowserWindow options are required");
const options = JSON.parse(raw) as BrowserWindowOptions;

await app.whenReady();
const window = new BrowserWindow(options);
writeSync(1, `GH567_WINDOW_CREATED id=${window.id}\n`);
// This isolated role owns no persistent fixture state. The parent has already
// captured the complete Create request and terminates the test-owned app-link.
process.exit(0);

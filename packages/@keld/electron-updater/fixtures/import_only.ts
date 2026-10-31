/**
 * Imports the adapter the way draw.io does (`electron.js:17-18`) under the
 * refusing probe and reports the shape it got. Zero probe counts prove the
 * import-time contract (GH-491 spec §3 criteria 1-2).
 */
import { EventEmitter } from "node:events";

import electronUpdater, { autoUpdater } from "../src/index.ts";

const { autoUpdater: destructured } = electronUpdater;

process.stdout.write(
  `GH491_RESULT ${JSON.stringify({
    destructuredIsEventEmitter: destructured instanceof EventEmitter,
    destructuredIsNamedExport: destructured === autoUpdater,
    defaultHasAppUpdater: typeof electronUpdater.AppUpdater === "function",
    defaultHasNoOpLogger: typeof electronUpdater.NoOpLogger === "function",
    electronVersionShim: process.versions.electron ?? null,
  })}\n`,
);

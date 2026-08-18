/**
 * Import-only probe: module alias + process shims, no kipc.
 * Must not call whenReady (that needs a host).
 *
 * Markers use `writeSync(1, …)` so a piped stdout cannot hide them behind
 * block buffering.
 */
import { writeSync } from "node:fs";
import { app } from "electron";

function marker(line: string): void {
  writeSync(1, `${line}\n`);
}

if (typeof app.whenReady !== "function") {
  writeSync(2, "KEL72: app.whenReady missing\n");
  process.exit(1);
}

const proc = process as typeof process & { type?: string; versions: { electron?: string } };
marker(`KEL72_TYPE=${proc.type ?? ""}`);
marker(`KEL72_ELECTRON=${proc.versions.electron ?? ""}`);

/**
 * Lifecycle probe: whenReady / window-all-closed / quit over kipc.
 *
 * Markers are the test oracle (`writeSync` so piped stdout is not block-buffered).
 * `KEL72_READY` must not appear until the host sends Ready.
 * `KEL72_WINDOW_ALL_CLOSED` must not appear until the host closes its last
 * window — the shim must not `emit("window-all-closed")` itself.
 *
 * A throwing `ready` listener must not skip `KEL72_READY_SECOND`, leave
 * `whenReady()` pending, or kill the kipc read loop (`window-all-closed`
 * would then never arrive).
 */
import { writeSync } from "node:fs";
import { app } from "electron";

function marker(line: string): void {
  writeSync(1, `${line}\n`);
}

const proc = process as typeof process & { type?: string; versions: { electron?: string } };
marker(`KEL72_TYPE=${proc.type ?? ""}`);
marker(`KEL72_ELECTRON=${proc.versions.electron ?? ""}`);

app.on("ready", () => {
  throw new Error("KEL72_READY_LISTENER_THROW");
});
app.on("ready", () => {
  marker("KEL72_READY_SECOND");
});

app.on("window-all-closed", () => {
  marker("KEL72_WINDOW_ALL_CLOSED");
  void app.quit().then(
    () => process.exit(0),
    (err: unknown) => {
      writeSync(2, `KEL72_QUIT_FAILED=${String(err)}\n`);
      process.exit(1);
    },
  );
});

marker("KEL72_WAITING");
await app.whenReady();
marker("KEL72_READY");

/**
 * Observer for `ipcMain.removeAllListeners()` with no channel (GH-466). It runs
 * in its own process because the no-channel form also removes Electron's
 * internal no-op `error` listener (EventEmitter semantics, as in Electron),
 * which in-process tests must not do to the shared singleton. No kipc.
 */
import { writeSync } from "node:fs";
import { ipcMain } from "electron";

function marker(line: string): void {
  writeSync(1, `${line}\n`);
}

const noop = (): void => {};
ipcMain.on("gh466-a", noop).on("gh466-a", noop).once("gh466-b", noop);
ipcMain.addListener("gh466-c", noop);
marker(
  `GH466_REMOVE_ALL before error=${ipcMain.listenerCount("error")} a=${ipcMain.listenerCount("gh466-a")} b=${ipcMain.listenerCount("gh466-b")} c=${ipcMain.listenerCount("gh466-c")}`,
);
ipcMain.removeAllListeners("gh466-b");
marker(
  `GH466_REMOVE_ALL channel error=${ipcMain.listenerCount("error")} a=${ipcMain.listenerCount("gh466-a")} b=${ipcMain.listenerCount("gh466-b")} c=${ipcMain.listenerCount("gh466-c")}`,
);
ipcMain.removeAllListeners();
marker(
  `GH466_REMOVE_ALL none error=${ipcMain.listenerCount("error")} a=${ipcMain.listenerCount("gh466-a")} b=${ipcMain.listenerCount("gh466-b")} c=${ipcMain.listenerCount("gh466-c")} names=${ipcMain.eventNames().length}`,
);

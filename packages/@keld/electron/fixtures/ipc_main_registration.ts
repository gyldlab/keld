/**
 * Import-time registration probe for the GH-466 registration slice.
 *
 * Replays, by source line, the 26 `ipcMain.on`/`ipcMain.once` and 5
 * `ipcMain.removeListener` sites of pinned drawio-desktop
 * `src/main/electron.js` (2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e), then the
 * two library registrations that run at import: electron-store 11.0.2
 * `index.js:25` `ipcMain.on('electron-store-get-data')` and electron-log 5.4.4
 * `src/main/index.js:16/:28` `ipcMain.on`/`ipcMain.handle('__ELECTRON_LOG__')`.
 * The test unsets KELD_APP_LINK: any transport touch would surface as
 * KELD-IPC-007 on stderr and a non-zero exit. Must not call whenReady.
 *
 * Markers use `writeSync(1, …)` so a piped stdout cannot hide them.
 */
import { EventEmitter } from "node:events";
import { writeSync } from "node:fs";
import electron, { ipcMain } from "electron";

function marker(line: string): void {
  writeSync(1, `${line}\n`);
}

let calls = 0;
const listenerAt = new Map<number, () => void>();
function listenerFor(line: number): () => void {
  const listener = (): void => {
    calls += 1;
  };
  listenerAt.set(line, listener);
  return listener;
}

// Evaluation order of electron.js: the two module-top-level sites first, then
// the sites inside whenReady (:987-2187), createWindow (:702-982) and
// exportDiagram (:3058-3424).
const sites: ReadonlyArray<readonly [number, "on" | "once", string]> = [
  [3426, "on", "export"],
  [4431, "on", "rendererReq"],
  [851, "on", "isModified-result"],
  [913, "on", "saveAndClose-result"],
  [927, "once", "draftRemoved"],
  [1027, "on", "openDevTools"],
  [1034, "on", "newfile"],
  [1452, "once", "import-success"],
  [1453, "once", "import-error"],
  [1762, "once", "app-load-finished"],
  [1815, "once", "app-load-finished"],
  [1851, "on", "toggleSpellCheck"],
  [1864, "on", "toggleStoreBkp"],
  [1877, "on", "toggleGoogleFonts"],
  [1891, "on", "toggleFullscreen"],
  [2012, "on", "checkForUpdates"],
  [2013, "on", "zoomIn"],
  [2014, "on", "zoomOut"],
  [2015, "on", "resetZoom"],
  [2173, "once", "app-load-finished"],
  [3144, "once", "export-finalize"],
  [3349, "once", "svg-data"],
  [3371, "once", "render-finished"],
  [3372, "once", "export-error"],
  [3376, "once", "xml-data"],
  [3383, "once", "xml-data-error"],
];

// removeListener sites, each naming the registration site whose function it removes.
const removals: ReadonlyArray<readonly [number, string, number]> = [
  [900, "saveAndClose-result", 913],
  [1421, "import-success", 1452],
  [1422, "import-error", 1453],
  [3132, "render-finished", 3371],
  [3133, "export-error", 3372],
];

marker(
  `GH466_BINDINGS default-same=${electron.ipcMain === ipcMain} emitter=${ipcMain instanceof EventEmitter} error-listeners=${ipcMain.listenerCount("error")}`,
);

for (const [line, method, channel] of sites) {
  ipcMain[method](channel, listenerFor(line));
}
const channels = ipcMain
  .eventNames()
  .filter(
    (name): name is string => typeof name === "string" && name !== "error",
  )
  .sort();
marker(
  `GH466_REGISTERED sites=${sites.length} channels=${channels.length} ${channels
    .map((channel) => `${channel}=${ipcMain.listenerCount(channel)}`)
    .join(" ")}`,
);

for (const [, channel, registeredAt] of removals) {
  const listener = listenerAt.get(registeredAt);
  if (listener === undefined)
    throw new Error(`no listener for :${registeredAt}`);
  ipcMain.removeListener(channel, listener);
}
const remaining = channels.reduce(
  (sum, channel) => sum + ipcMain.listenerCount(channel),
  0,
);
marker(
  `GH466_REMOVED ${removals
    .map(([, channel]) => `${channel}=${ipcMain.listenerCount(channel)}`)
    .join(" ")} remaining=${remaining}`,
);

ipcMain.on("electron-store-get-data", listenerFor(0));
ipcMain.on("__ELECTRON_LOG__", listenerFor(-1));
ipcMain.handle("__ELECTRON_LOG__", () => {
  calls += 1;
  return undefined;
});
let secondHandle = "none";
try {
  ipcMain.handle("__ELECTRON_LOG__", () => undefined);
} catch (error) {
  secondHandle =
    error instanceof Error ? error.message.slice(0, 15) : "non-error";
}
marker(
  `GH466_LIBRARIES electron-store-get-data=${ipcMain.listenerCount("electron-store-get-data")} __ELECTRON_LOG__=${ipcMain.listenerCount("__ELECTRON_LOG__")} second-handle=${secondHandle}`,
);
marker(`GH466_DELIVERED calls=${calls}`);

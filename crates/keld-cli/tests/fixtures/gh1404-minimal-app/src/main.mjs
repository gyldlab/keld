import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { app, BrowserWindow } from "electron";

await app.whenReady();

const root = app.getAppPath();
const window = new BrowserWindow({
  width: 800,
  height: 500,
  webPreferences: {
    preload: join(root, "src/preload.mjs"),
  },
});
await window.loadURL(pathToFileURL(join(root, "web/index.html")).href);

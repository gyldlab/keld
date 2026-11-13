import { app, BrowserWindow } from "electron";

await app.whenReady();

const window = new BrowserWindow({ width: 800, height: 500 });
await window.loadFile("web/index.html");

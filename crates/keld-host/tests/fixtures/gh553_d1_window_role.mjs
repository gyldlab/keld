// First-party D1 qualification caller; never evaluates the original main app.
// The sole original executable image is the unchanged, Host-pinned preload.
import { app, BrowserWindow } from "electron";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

await app.whenReady();
// Same actual descendant and dev-lease owner as the GH589 inert regression.
const descendant = spawn("/bin/cat", [], {
  stdio: ["pipe", "ignore", "ignore"],
});
await once(descendant, "spawn");
const base = join(process.cwd(), "drawio/src/main");
const view = new BrowserWindow({
  width: 800,
  height: 600,
  webPreferences: {
    preload: join(process.cwd(), "src/main/electron-preload.js"),
    contextIsolation: true,
    nodeIntegration: false,
    webSecurity: true,
    additionalArguments: ["--initial-adaptive-colors=simple"],
  },
});
const index = pathToFileURL(join(base, "webapp/index.html"));
index.searchParams.set(
  "outside",
  pathToFileURL(join(base, "d1-outside.bin")).href,
);
await view.loadURL(index.href);
// No Ready/PID/control report. Native AX reads the actual loaded document;
// existing app-link ownership holds this role until the harness ends its lease.

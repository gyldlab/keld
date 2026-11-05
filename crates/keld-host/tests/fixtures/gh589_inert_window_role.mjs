// First-party mechanism regression only. No original application or control protocol.
import { app, BrowserWindow } from "electron";
import { spawn } from "node:child_process";
import { once } from "node:events";

await app.whenReady();
// A real descendant is held by its actual stdin pipe before the native
// window is created. No app READY/PID report substitutes for the OS census.
const descendant = spawn("/bin/cat", [], {
  stdio: ["pipe", "ignore", "ignore"],
});
await once(descendant, "spawn");
new BrowserWindow({ width: 640, height: 480 });
// Existing app-link ownership keeps the role alive. The harness ends this
// isolated mechanism test through the existing dev lease, not an app command.

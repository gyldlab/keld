// Separately admitted hostile startup fixture. It withholds registration only.
import { app, BrowserWindow } from "electron";
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

const port = Number(process.env.KELD_T1B_CONTROL);
if (!Number.isInteger(port) || port <= 0 || port > 65535)
  throw new Error("missing parent-owned fixture rendezvous");
const socket = createConnection({ host: "127.0.0.1", port });
const report = (kind, value = {}) =>
  socket.write(kind + " " + JSON.stringify(value) + "\n");
const reportFlushed = (kind, value = {}) =>
  new Promise((resolve, reject) => {
    socket.write(kind + " " + JSON.stringify(value) + "\n", (error) => {
      if (error) reject(error);
      else resolve();
    });
  });
let win;
let loaded = false;
socket.on("connect", () =>
  report("HELLO", { bunPid: process.pid, versions: { ...process.versions } }),
);
socket.on("error", (error) => {
  console.error("ORIGINAL_STARTUP_CONTROL_ERROR", String(error));
  void app.quit();
});
await app.whenReady();
report("READY", { windows: BrowserWindow.getAllWindows().length });
for await (const line of createInterface({
  input: socket,
  crlfDelay: Infinity,
})) {
  try {
    if (line === "CREATE") {
      if (win) throw new Error("one actual window");
      win = new BrowserWindow({
        width: 640,
        height: 480,
        show: true,
        title: "Keld original X5 missing registration",
        webPreferences: {
          preload: fileURLToPath(
            new URL("../bootstrap/native-preload.cjs", import.meta.url),
          ),
          nodeIntegration: false,
          contextIsolation: true,
          webSecurity: true,
          additionalArguments: ["--initial-adaptive-colors=auto"],
        },
      });
      report("CREATED", { id: win.id });
    } else if (line === "LOAD") {
      if (!win || loaded) throw new Error("one LOAD after actual CREATE");
      loaded = true;
      const entry = new URL("../client/assets/editor.html", import.meta.url);
      report("LOAD_ENTERED", { id: win.id });
      await win.loadURL(entry.href);
      await reportFlushed("LOADED", { id: win.id, url: entry.href });
      // No later control report is required. End only this test write side;
      // the read side and real app remain live for the native startup failure.
      socket.end();
    } else {
      throw new Error("unknown startup fixture command");
    }
  } catch (error) {
    report("FIXTURE_ERROR", { command: line, detail: String(error) });
    throw error;
  }
}

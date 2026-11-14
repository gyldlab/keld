// Real public facade and real IPC handler. This fixture supplies no Host authority.
import { app, BrowserWindow, ipcMain } from "electron";
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
const counts = { frame: 0, top: 0, invalid: 0 };
let win;
let opened = false;
let closing = false;
let terminalQuit;
const finishQuit = () => {
  if (terminalQuit) return terminalQuit;
  terminalQuit = (async () => {
    await app.quit();
    report("QUIT_TERMINAL_ACCEPTED");
    await reportFlushed("QUIT_ACCEPTED");
    socket.end();
  })();
  return terminalQuit;
};
app.on("window-all-closed", () => {
  if (!closing) {
    report("ALL_CLOSED");
    return;
  }
  void (async () => {
    await reportFlushed("ALL_CLOSED");
    await finishQuit();
  })().catch((error) => {
    report("FIXTURE_ERROR", {
      command: "QUIT after ALL_CLOSED",
      detail: String(error),
    });
    throw error;
  });
});

// The ONLY effect counter. It is reached through the real canonical IPC decoder
// and public Bun listener; no renderer readback can increment it.
ipcMain.on("app-load-finished", (_event, value) => {
  if (
    value?.probe !== "keld-original-x3" ||
    (value.path !== "frame" && value.path !== "top")
  ) {
    counts.invalid += 1;
    report("IPC_INVALID", { counts, value });
    return;
  }
  counts[value.path] += 1;
  report("IPC_EFFECT", { path: value.path, count: counts[value.path], value });
});

socket.on("connect", () =>
  report("HELLO", {
    bunPid: process.pid,
    versions: { ...process.versions },
  }),
);
socket.on("error", (error) => {
  console.error("ORIGINAL_FIXTURE_CONTROL_ERROR", String(error));
  void app.quit();
});
await app.whenReady();
report("READY", { windows: BrowserWindow.getAllWindows().length });

for await (const line of createInterface({
  input: socket,
  crlfDelay: Infinity,
})) {
  try {
    if (line === "OPEN frame") {
      if (opened || closing) throw new Error("one fresh document per process");
      opened = true;
      const mode = line.slice(5);
      const preload = fileURLToPath(
        new URL("../bootstrap/native-preload.cjs", import.meta.url),
      );
      win = new BrowserWindow({
        width: 640,
        height: 480,
        show: true,
        title: "Keld X3 initial blank frame fixture",
        webPreferences: {
          preload,
          nodeIntegration: false,
          contextIsolation: true,
          webSecurity: true,
          additionalArguments: ["--initial-adaptive-colors=auto"],
        },
      });
      win.on("close", () => report("CLOSE_REQUESTED", { id: win.id }));
      win.on("closed", () => report("CLOSED", { id: win.id }));
      report("CREATED", { id: win.id, mode });
      const entry = new URL("../client/assets/editor.html", import.meta.url);
      entry.searchParams.set("mode", mode);
      await win.loadURL(entry.href);
      report("LOADED", { id: win.id, mode, url: entry.href });
    } else if (line === "COUNTERS") {
      report("COUNTERS", { ...counts });
    } else if (line === "ARM_FRAME" || line === "TOP_CONTROL") {
      if (!win || !opened || closing)
        throw new Error("fixture coordination is out of order");
      const phase = line === "ARM_FRAME" ? "arm-frame" : "top-control";
      // The same reviewed grant, public facade and canonical DATA carriage.
      // This report is NOT a native delivery or consumer receipt.
      await win.webContents.send("args-obj", {
        probe: "keld-original-x3",
        phase,
      });
      report("COORDINATED", { phase });
    } else if (line === "QUIT") {
      if (closing) throw new Error("duplicate Quit");
      closing = true;
      if (win && !win.isDestroyed()) {
        win.close();
      } else {
        await finishQuit();
      }
    } else {
      throw new Error("unknown fixture command");
    }
  } catch (error) {
    report("FIXTURE_ERROR", { command: line, detail: String(error) });
    throw error;
  }
}

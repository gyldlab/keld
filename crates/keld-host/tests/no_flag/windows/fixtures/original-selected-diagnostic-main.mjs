// Fixed public-facade stimulus; the parent TCP socket grants no Host authority.
import { app, BrowserWindow, ipcMain } from "electron";
import { getWindowSnapshots, invokeWindow } from "@keld/api/internal/app";
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
let nativeWindow;
let destroyRequested = false;
let loaded = false;
let sent = false;
let crashed = false;
let effects = 0;
app.on("window-all-closed", () => report("ALL_CLOSED"));
ipcMain.on("app-load-finished", (_event, value) => {
  if (value?.probe !== "keld-original-x10-selected" || value.path !== "facade")
    throw new Error("unexpected real renderer message");
  effects += 1;
  report("IPC_EFFECT", { effects });
});
socket.on("connect", () =>
  report("HELLO", { bunPid: process.pid, versions: { ...process.versions } }),
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
    if (line === "OPEN") {
      if (win) throw new Error("one original window");
      win = new BrowserWindow({
        width: 640,
        height: 480,
        show: true,
        title: "Keld original X10 selected diagnostic",
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
      // Capture the actual accepted Host identity before LOAD starts the hold.
      const matches = getWindowSnapshots().filter(
        (item) => item.window.id === win.id && !item.host_destroyed,
      );
      if (matches.length !== 1)
        throw new Error("one live original Host window required");
      nativeWindow = Object.freeze({ ...matches[0].window });
      win.on("close", () => report("CLOSE_REQUESTED", { id: win.id }));
      win.on("closed", () => report("CLOSED", { id: win.id }));
      report("CREATED", {
        id: win.id,
        generation: String(nativeWindow.incarnation),
        mode: "facade",
      });
    } else if (line === "LOAD") {
      if (!win || loaded) throw new Error("one original navigation");
      loaded = true;
      // The real load Promise may await a reply behind the held physical packet.
      // Keep the existing parent control reader available; never fake its result.
      const loading = win.loadURL(
        new URL("../client/assets/editor.html", import.meta.url).href,
      );
      void loading
        .then(() => report("LOADED", { id: win.id }))
        .catch((error) =>
          report("LOAD_RETURNED_ERROR", { detail: String(error) }),
        );
      report("LOAD_REQUESTED", { id: win.id });
    } else if (line === "SEND") {
      if (!loaded || sent || !win || win.isDestroyed())
        throw new Error("one target Event in the original window");
      sent = true;
      win.webContents.send("args-obj", "keld-x10-selected-target-v1");
      report("SEND_RETURNED", { id: win.id });
    } else if (line === "DESTROY") {
      if (
        !sent ||
        destroyRequested ||
        !win ||
        !nativeWindow ||
        win.isDestroyed()
      )
        throw new Error("same once-selected target window required");
      destroyRequested = true;
      // The actual Destroy reply may wait behind the original physical VALUE.
      // Keep the existing parent reader alive; only native Close proves its effect.
      const destroying = invokeWindow({
        variant: "Destroy",
        window: nativeWindow,
      });
      void destroying.then(
        (reply) =>
          report("DESTROY_REPLY", { id: win.id, reply: reply.variant }),
        (error) => report("DESTROY_RETURNED_ERROR", { detail: String(error) }),
      );
      report("DESTROY_REQUESTED", {
        id: nativeWindow.id,
        generation: String(nativeWindow.incarnation),
      });
    } else if (line === "CRASH") {
      if (!sent || !destroyRequested || crashed || !win)
        throw new Error("one original child exit after target Destroy request");
      crashed = true;
      const closed = new Promise((resolve, reject) => {
        socket.once("error", reject);
        socket.once("close", (hadError) => {
          if (hadError) reject(new Error("fixture control closed with error"));
          else resolve();
        });
      });
      await reportFlushed("CRASH_ACK", {
        bunPid: process.pid,
        id: win.id,
        effects,
      });
      // Both fixture peers finish their send direction before intentional death.
      socket.end();
      await closed;
      process.exit(73);
    } else if (line === "COUNTERS") {
      report("COUNTERS", { effects });
    } else {
      throw new Error("unknown fixed fixture command");
    }
  } catch (error) {
    report("FIXTURE_ERROR", { command: line, detail: String(error) });
    throw error;
  }
}

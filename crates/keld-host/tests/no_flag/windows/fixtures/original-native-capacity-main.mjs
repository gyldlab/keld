// Fixed public-facade stimulus; the parent TCP socket grants no Host authority.
import { app, BrowserWindow, ipcMain } from "electron";
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { lstatSync, writeFileSync, renameSync } from "node:fs";
import { isAbsolute, join } from "node:path";
import { APP_LINK_IO_DEADLINE_MS } from "@keld/kipc";

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
let sent = false;
let closing = false;
let terminalQuit;
let consumerGate;
let effects = 0;
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

// This is fixture application code, before the unchanged synchronous provider
// consumer returns. A Promise would not hold the actual ring/release boundary.
const condition = new Int32Array(new SharedArrayBuffer(4));
const ordinaryFile = (path) => {
  let metadata;
  try {
    metadata = lstatSync(path);
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    throw error;
  }
  if (!metadata.isFile() || metadata.isSymbolicLink())
    throw new Error("consumer condition must be an ordinary file");
  return true;
};
const armConsumer = (directory) => {
  if (
    consumerGate ||
    loaded ||
    typeof directory !== "string" ||
    directory.length > 4096 ||
    !isAbsolute(directory) ||
    !directory.toLowerCase().startsWith("d:")
  )
    throw new Error("one parent-owned D consumer condition before LOAD");
  const metadata = lstatSync(directory);
  if (!metadata.isDirectory() || metadata.isSymbolicLink())
    throw new Error("consumer condition must be an ordinary directory");
  for (const name of [
    "entered",
    "entered.tmp",
    "leaving",
    "leaving.tmp",
    "release",
  ])
    if (ordinaryFile(join(directory, name)))
      throw new Error("consumer condition must begin without markers");
  consumerGate = {
    directory,
    deadline: performance.now() + APP_LINK_IO_DEADLINE_MS,
    entered: false,
  };
  report("CONSUMER_ARMED", {
    bunPid: process.pid,
    budgetMs: APP_LINK_IO_DEADLINE_MS,
  });
};
const holdConsumer = () => {
  const gate = consumerGate;
  if (!gate) return;
  if (
    gate.entered ||
    performance.now() >= gate.deadline ||
    ordinaryFile(join(gate.directory, "release"))
  )
    throw new Error(
      "original consumer condition expired or released before entry",
    );
  gate.entered = true;
  const record = (phase) => {
    const staging = join(gate.directory, phase + ".tmp");
    writeFileSync(
      staging,
      JSON.stringify({ bunPid: process.pid, effects, phase }),
      { flag: "wx" },
    );
    // Closed bytes become visible at one ordinary rename, not a partial file read.
    renameSync(staging, join(gate.directory, phase));
  };
  record("entered");
  for (;;) {
    // The one deadline begins before LOAD and the actual physical Packet.
    // Expiry wins over release; this never grants an extension to either owner.
    const remaining = gate.deadline - performance.now();
    if (remaining <= 0)
      throw new Error("original consumer condition deadline expired");
    if (ordinaryFile(join(gate.directory, "release"))) break;
    Atomics.wait(condition, 0, 0, Math.min(10, remaining));
  }
  record("leaving");
};
ipcMain.on("app-load-finished", (_event, value) => {
  if (value?.probe !== "keld-original-x11-pressure" || value.path !== "facade")
    throw new Error("unexpected real renderer message");
  effects += 1;
  holdConsumer();
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
        title: "Keld original X11 native pressure",
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
      win.on("close", () => report("CLOSE_REQUESTED", { id: win.id }));
      win.on("closed", () => report("CLOSED", { id: win.id }));
      report("CREATED", { id: win.id, mode: "facade" });
    } else if (line.startsWith("ARM_CONSUMER ")) {
      armConsumer(JSON.parse(line.slice("ARM_CONSUMER ".length)));
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
      win.webContents.send("args-obj", "keld-x11-pressure-target-v1");
      report("SEND_RETURNED", { id: win.id });
    } else if (line === "CLOSE") {
      if (!sent || !win || win.isDestroyed())
        throw new Error("same target window required");
      win.close();
      report("CLOSE_SENT", { id: win.id });
    } else if (line === "QUIT") {
      if (closing) throw new Error("duplicate Quit");
      closing = true;
      if (win && !win.isDestroyed()) win.close();
      else await finishQuit();
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

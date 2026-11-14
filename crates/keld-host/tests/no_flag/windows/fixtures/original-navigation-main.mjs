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
const counts = { A: 0, B: 0, invalid: 0 };
let win;
let opened = false;
let targetSent = false;
let navigated = false;
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
    value?.probe !== "keld-original-x10-nav" ||
    value.path !== "facade" ||
    (value.document !== "A" && value.document !== "B")
  ) {
    counts.invalid += 1;
    report("IPC_INVALID", { counts, value });
    return;
  }
  counts[value.document] += 1;
  report("IPC_EFFECT", {
    document: value.document,
    count: counts[value.document],
    value,
  });
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

async function sendReady() {
  // A successful load follows the blocking first author script. The marker
  // uses the existing grant and normal native DATA path; its no-listener
  // disposition is not consumer success. Only page receipt schedules work.
  const markerDeadline = performance.now() + 10_000;
  for (;;) {
    if (performance.now() >= markerDeadline)
      throw new Error("route marker target did not bind before deadline");
    let targetAbsent = false;
    try {
      // This public API returns void. Never await or retry an accepted send.
      win.webContents.send("args-obj", "keld-original-route-ready-v1");
    } catch (error) {
      // This exact synchronous refusal precedes request construction and
      // callCompat. Every other failure keeps its original failure path.
      if (
        error?.code !== "KELD-CORE-043" ||
        error?.message !==
          "KELD-CORE-043: no unique current Host-authenticated renderer target"
      )
        throw error;
      targetAbsent = true;
    }
    if (!targetAbsent) break;
    // A bounded condition wait, not a delay that grants route readiness.
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

for await (const line of createInterface({
  input: socket,
  crlfDelay: Infinity,
})) {
  try {
    if (line === "OPEN") {
      if (opened || closing) throw new Error("one fresh window per process");
      opened = true;
      const mode = "facade";
      const preload = fileURLToPath(
        new URL("../bootstrap/native-preload.cjs", import.meta.url),
      );
      win = new BrowserWindow({
        width: 640,
        height: 480,
        show: true,
        title: "Keld original X10 navigation fixture",
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
      entry.searchParams.set("document", "A");
      await win.loadURL(entry.href);
      report("LOADED", { id: win.id, mode, url: entry.href });
      await sendReady();
    } else if (line === "SEND") {
      if (!opened || targetSent || closing || !win || win.isDestroyed())
        throw new Error("one target Event on the existing live fixture window");
      win.webContents.send("args-obj", "keld-x10-target-v1");
      targetSent = true;
      // The void facade return is only a fixture acknowledgement. The parent
      // waits for the actual Core Entry/DATA admission before requesting B navigation.
      report("SEND_RETURNED", { id: win.id });
    } else if (line === "NAVIGATE_B") {
      if (!targetSent || navigated || closing || !win || win.isDestroyed())
        throw new Error("one B navigation after the actual A target stimulus");
      navigated = true;
      const entry = new URL("../client/assets/editor.html", import.meta.url);
      entry.searchParams.set("mode", "facade");
      entry.searchParams.set("document", "B");
      // This normal public load is scheduled by a later parent command. It never
      // runs inside the native Post callback and never waits for A consumption.
      report("NAVIGATION_REQUESTED", { id: win.id, url: entry.href });
      await win.loadURL(entry.href);
      report("NAVIGATED", { id: win.id, url: entry.href });
      // B observes its actual native route-state CONTROL. A may still own DATA;
      // no B readiness DATA is sent and no A-consumption barrier is introduced.
    } else if (line === "CLOSE") {
      if (!targetSent || closing || !win || win.isDestroyed())
        throw new Error("Close requires the same target window");
      win.close();
      report("CLOSE_SENT", { id: win.id });
    } else if (line === "COUNTERS") {
      report("COUNTERS", { ...counts });
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

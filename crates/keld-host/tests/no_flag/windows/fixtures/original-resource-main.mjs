// One real public facade, actual Bun generation and parent-owned control socket.
// Fixture reports are observations; they never supply native release receipts.
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
const flushedReport = (kind, value) =>
  new Promise((resolve, reject) => {
    socket.write(kind + " " + JSON.stringify(value) + "\n", (error) =>
      error ? reject(error) : resolve(),
    );
  });
let win;
let closing = false;
let opened = false;
let effects = 0;
app.on("window-all-closed", () => {
  report("ALL_CLOSED");
  if (closing) {
    void app.quit().then(
      () => report("QUIT_TERMINAL_ACCEPTED"),
      (error) => {
        report("FIXTURE_ERROR", {
          command: "QUIT after ALL_CLOSED",
          detail: String(error),
        });
        throw error;
      },
    );
  }
});
ipcMain.on("app-load-finished", (_event, value) => {
  if (value?.probe !== "keld-original-x14" || value.path !== "facade")
    throw new Error("unexpected resource fixture IPC payload");
  effects += 1;
  report("IPC_EFFECT", { count: effects, value, bunPid: process.pid });
});
socket.on("error", (error) => {
  console.error("ORIGINAL_RESOURCE_CONTROL_ERROR", String(error));
  void app.quit();
});
await new Promise((resolve, reject) => {
  socket.once("connect", resolve);
  socket.once("error", reject);
});
report("HELLO", { bunPid: process.pid, versions: { ...process.versions } });
await app.whenReady();
const existing = BrowserWindow.getAllWindows();
if (existing.length > 1) throw new Error("one actual fixture window expected");
win = existing[0];
const observeWindow = (window) => {
  // Recovery defaults to veto without an application close listener. This
  // fixture explicitly allows its own public Close; production policy stays.
  window.on("close", () =>
    report("CLOSE_EVENT", { id: window.id, bunPid: process.pid }),
  );
  window.on("closed", () => report("CLOSED", { id: window.id }));
};
if (win) observeWindow(win);
report("READY", {
  windows: existing.length,
  windowIds: existing.map((w) => w.id),
});

for await (const line of createInterface({
  input: socket,
  crlfDelay: Infinity,
})) {
  try {
    if (line === "OPEN") {
      if (win || opened || closing)
        throw new Error("one fresh fixture document");
      opened = true;
      win = new BrowserWindow({
        width: 640,
        height: 480,
        show: true,
        title: "Keld original X14 resource fixture",
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
      observeWindow(win);
      const id = win.id;
      report("CREATED", { id });
      const entry = new URL("../client/assets/editor.html", import.meta.url);
      // This detached continuation leaves the same control socket readable.
      // The held Image is requested only after the tested IPC send is consumed,
      // so actual load completion precedes marker delivery and the resource hold.
      void win
        .loadURL(entry.href)
        .then(
          async () => {
            report("LOADED", { id, url: entry.href });
            // A successful load follows the blocking first author script. The marker
            // uses the existing grant and normal native DATA path; its no-listener
            // disposition is not consumer success. Only page receipt schedules work.
            const markerDeadline = performance.now() + 10_000;
            for (;;) {
              if (performance.now() >= markerDeadline)
                throw new Error(
                  "route marker target did not bind before deadline",
                );
              let targetAbsent = false;
              try {
                // This public API returns void. Never await or retry an accepted send.
                win.webContents.send(
                  "args-obj",
                  "keld-original-route-ready-v1",
                );
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
          },
          (error) => report("LOAD_REJECTED", { id, detail: String(error) }),
        )
        .catch((error) => {
          report("FIXTURE_ERROR", {
            command: "route marker after LOADED",
            detail: String(error),
          });
          throw error;
        });
    } else if (line === "STATUS") {
      report("STATUS", {
        bunPid: process.pid,
        effects,
        windowIds: BrowserWindow.getAllWindows().map((window) => window.id),
      });
    } else if (line === "CRASH") {
      if (!win || closing)
        throw new Error("crash needs the actual live window");
      await flushedReport("CRASH_ACK", { bunPid: process.pid, id: win.id });
      // Finish only the fixture control write end before actual nonzero exit.
      await new Promise((resolve) => socket.end(resolve));
      process.exit(42);
    } else if (line === "CLOSE") {
      if (!win || closing)
        throw new Error("close needs the actual live window");
      const id = win.id;
      win.close();
      report("CLOSE_REQUESTED", { id, bunPid: process.pid });
    } else if (line === "QUIT") {
      if (closing) throw new Error("duplicate Quit");
      closing = true;
      await app.quit();
      await flushedReport("QUIT_ACCEPTED", {});
      socket.end();
    } else {
      throw new Error("unknown fixture command");
    }
  } catch (error) {
    report("FIXTURE_ERROR", { command: line, detail: String(error) });
    throw error;
  }
}

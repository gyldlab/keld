// Hand-authored caller fixture; never imports/evaluates the original app.
// Open/Save option-object spellings match pinned electron.js:4195-4219.
import { createConnection } from "node:net";
import { createInterface } from "node:readline";

const socket = process.env.KELD_T1B_CONTROL;
if (!socket) throw new Error("missing source-exact picker control");
const control = createConnection({ path: socket });
await new Promise<void>((resolve, reject) => {
  control.once("connect", resolve);
  control.once("error", reject);
});
function report(line: string): void {
  control.write(`${line}\n`);
}
report(`ROLE_BOUND ${process.pid} ${process.ppid}`);
app.on("window-all-closed", () => {});
await app.whenReady();
const win = new BrowserWindow({ width: 640, height: 480 });
report(`CREATED ${process.pid} ${win.id}`);

const openPath = __OPEN_PATH__;
const savePath = __SAVE_PATH__;
const sourceFilters = [
  { name: "draw.io", extensions: ["drawio", "xml"] },
  { name: "VSDX", extensions: ["vsdx"] },
  { name: "Mermaid", extensions: ["mmd", "mermaid"] },
  { name: "All Files", extensions: ["*"] },
];
type Filters = typeof sourceFilters;
type Properties = ("openFile" | "openDirectory" | "multiSelections")[];

function sourceOpen(
  defaultPath: string | undefined,
  filters: Filters | undefined,
  properties: Properties | undefined,
) {
  return dialog.showOpenDialog(win, {
    defaultPath: defaultPath,
    filters: filters,
    properties: properties,
  });
}

function sourceSave(
  defaultPath: string | undefined,
  filters: Filters | undefined,
) {
  return dialog.showSaveDialog(win, {
    defaultPath: defaultPath,
    filters: filters,
  });
}

for await (const command of createInterface({ input: control })) {
  if (command === "OPEN_CANCEL" || command === "OPEN_ABSENT_CANCEL") {
    const pending =
      command === "OPEN_CANCEL"
        ? sourceOpen(undefined, undefined, undefined)
        : dialog.showOpenDialog(win, {});
    void pending.then(
      (value) =>
        report(`OPEN_RESULT ${value.canceled} ${value.filePaths.length}`),
      (error) => report(`OPEN_ERROR ${error?.code ?? String(error)}`),
    );
  } else if (command === "SAVE_CANCEL" || command === "SAVE_ABSENT_CANCEL") {
    const pending =
      command === "SAVE_CANCEL"
        ? sourceSave(undefined, undefined)
        : dialog.showSaveDialog(win, {});
    void pending.then(
      (value) =>
        report(`SAVE_RESULT ${value.canceled} ${value.filePath === ""}`),
      (error) => report(`SAVE_ERROR ${error?.code ?? String(error)}`),
    );
  } else if (command === "DRAWIO_OPEN_SELECT") {
    const pending = sourceOpen(openPath, sourceFilters, [
      "openFile",
      "openDirectory",
      "multiSelections",
    ]);
    void pending.then(
      (value) => report(`DRAWIO_OPEN_SELECTED ${JSON.stringify(value)}`),
      (error) => report(`OPEN_ERROR ${error?.code ?? String(error)}`),
    );
  } else if (command === "DRAWIO_SAVE_SELECT") {
    const pending = sourceSave(savePath, sourceFilters);
    void pending.then(
      (value) => report(`DRAWIO_SAVE_SELECTED ${JSON.stringify(value)}`),
      (error) => report(`SAVE_ERROR ${error?.code ?? String(error)}`),
    );
  } else if (command === "MESSAGE_NEXT") {
    const pending = dialog.showMessageBox(win, {
      message: "Healthy next",
      type: "question",
      buttons: ["Save", "Cancel"],
      defaultId: 0,
      cancelId: 1,
    });
    void pending.then(
      (value) => report(`MESSAGE_RESULT ${value.response}`),
      (error) => report(`MESSAGE_ERROR ${error?.code ?? String(error)}`),
    );
  } else if (command === "FINISH") {
    win.destroy();
    await app.quit();
    report("QUIT_REPLY");
    break;
  } else {
    throw new Error(`unknown source-exact picker command ${command}`);
  }
  report("SUBMITTED");
}
control.end();

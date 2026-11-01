// The native fixture prepends imports from the actual public Electron facade.
// This is a framework acceptance scenario, not original draw.io or installed SDK proof.
import { createConnection } from "node:net";
import { createInterface } from "node:readline";

let early = "accepted";
try {
  screen.getPrimaryDisplay();
} catch (error) {
  early = error instanceof Error ? error.message.split(":")[0]! : "untyped";
}
if (early !== "KELD-CORE-040")
  throw new Error("desktop became available before public Ready: " + early);
app.on("window-all-closed", () => {});
let readyCount = 0;
app.on("ready", () => {
  readyCount += 1;
});
await app.whenReady();
if (readyCount !== 1 || !app.isReady())
  throw new Error("public Ready disagrees");
const read = () => ({
  primary: screen.getPrimaryDisplay(),
  displays: screen.getAllDisplays(),
  dark: nativeTheme.shouldUseDarkColors,
});
const initial = read();
const expected = JSON.stringify(initial);
for (let count = 0; count < 1000; count += 1) {
  if (JSON.stringify(read()) !== expected)
    throw new Error("desktop getter changed without a native observation");
}
const changed = screen.getAllDisplays();
changed[0]!.bounds.width = 1;
changed[0]!.workArea.y = 0x7fffffff;
changed.length = 0;
if (JSON.stringify(read()) !== expected)
  throw new Error("public result mutated the Host mirror");
const path = process.env.KELD_T1B_CONTROL;
if (!path) throw new Error("missing owned acceptance control");
const control = createConnection({ path });
await new Promise<void>((resolve, reject) => {
  control.once("connect", resolve);
  control.once("error", reject);
});
control.write("DESKTOP " + process.pid + " " + JSON.stringify(initial) + "\n");
let win: InstanceType<typeof BrowserWindow> | undefined;
for await (const command of createInterface({ input: control })) {
  if (command === "CREATE") {
    const primary = screen.getPrimaryDisplay();
    win = new BrowserWindow({
      width: 320,
      height: 240,
      x: primary.workArea.x + 40,
      y: primary.workArea.y + 40,
    });
    if (BrowserWindow.getAllWindows().length !== 1)
      throw new Error("first native window missing");
    control.write("CREATED " + win.id + "\n");
  } else if (command === "QUIT") {
    if (!win) throw new Error("native window was never constructed");
    win.destroy();
    await app.quit();
    control.write("QUIT_REPLY\n");
    break;
  } else throw new Error("unknown acceptance command");
}
control.end();

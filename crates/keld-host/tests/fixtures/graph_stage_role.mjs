import "./early.mjs";
import { app, screen } from "electron";
import { stateFloat64At } from "@keld/kipc";
import { createConnection } from "node:net";
import { createInterface } from "node:readline";

await app.whenReady();
const control = createConnection({ path: process.env.KELD_T1B_CONTROL });
await new Promise((resolve, reject) => {
  control.once("connect", resolve);
  control.once("error", reject);
});
control.write(
  `READY ${process.pid} ${process.ppid} ${screen.getAllDisplays().length}\n`,
);
for await (const command of createInterface({ input: control })) {
  if (command === "CHECK") {
    const bytes = Uint8Array.from([0, 0, 0, 0, 0, 0, 0, 64]);
    globalThis.__graphProbe.armed = true;
    const value = stateFloat64At(bytes, 0);
    globalThis.__graphProbe.armed = false;
    control.write(`CHECK ${globalThis.__graphProbe.hits} ${value}\n`);
  } else if (command === "CRASH") {
    process.exit(71);
  } else if (command === "QUIT") {
    await app.quit();
    control.write("QUIT_REPLY\n");
    break;
  } else {
    throw new Error("unknown graph acceptance command");
  }
}
control.end();

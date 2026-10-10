/** KEL-140 private test caller; its imports expose only the public API. */
import { app, fs, isCallError } from "../../../../../../packages/@keld/api/src/index.ts";

function report(key: string, value: string | number | boolean): void {
  console.log(`KELD_WL ${key}=${value}`);
}

const ready = app.whenReady();
const control: unknown = JSON.parse(await Bun.stdin.text());
if (typeof control !== "object" || control === null ||
    !("target" in control) || typeof control.target !== "string" ||
    !("lateTarget" in control) || typeof control.lateTarget !== "string") {
  throw new Error("invalid private public-FS fixture control");
}
await ready;
// This counts one public Promise handler, not wire ERRs or native syscall entries.
let settlements = 0;
const pending = fs.write(control.target, new Uint8Array([116, 50]));
await pending.then(
  () => { settlements += 1; report("call-returned", true); },
  (error: unknown) => {
    settlements += 1;
    if (!isCallError(error)) throw error;
    report("call-code", error.code);
    report("call-returned", false);
  },
);
try {
  await fs.write(control.lateTarget, new Uint8Array([108, 97, 116, 101]));
  throw new Error("post-retirement public write unexpectedly succeeded");
} catch (error: unknown) {
  if (!isCallError(error)) throw error;
  report("after-code", error.code);
}
report("settlements", settlements);
// No Quit or reconnect on this already-retired generation. Let its canonical
// Worker finish the terminal close and flush diagnostics before natural exit.

import assert from "node:assert/strict";
import { Worker } from "node:worker_threads";
import { app } from "electron";

assert.equal(typeof app, "object");
const worker = new Worker(new URL("./independent-worker.mjs", import.meta.url));
const result = await new Promise((resolve, reject) => {
  const timer = setTimeout(() => {
    void worker.terminate();
    reject(new Error("Worker realm timed out"));
  }, 5000);
  worker.once("message", (value) => {
    clearTimeout(timer);
    resolve(value);
  });
  worker.once("error", (error) => {
    clearTimeout(timer);
    reject(error);
  });
  worker.once("exit", (code) => {
    if (code !== 0) {
      clearTimeout(timer);
      reject(new Error("Worker exit " + code));
    }
  });
});
assert.equal(result.sameApp, true);
assert.deepEqual(result.keys, [
  "BrowserWindow",
  "ClipboardItem",
  "Menu",
  "app",
  "clipboard",
  "dialog",
  "nativeImage",
  "nativeTheme",
  "screen",
  "session",
  "shell",
]);
console.log("INDEPENDENT_WORKER_REALM");

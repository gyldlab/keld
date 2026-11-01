import { parentPort } from "node:worker_threads";
import facade, { app } from "electron";

parentPort.postMessage({
  sameApp: app === facade.app,
  keys: Object.keys(facade).sort(),
});

/**
 * GH-491 spec §3 criterion 1: the module shape draw.io consumes
 * (`electron.js:17-18`: `import elecUpPkg from 'electron-updater';
 * const {autoUpdater} = elecUpPkg`), mirroring electron-updater 6.8.9
 * `out/main.js`, whose CJS module object carries `autoUpdater`, `AppUpdater` and
 * `NoOpLogger`. Import-time I/O is proven by `replay.test.ts`, not here.
 */
import { EventEmitter } from "node:events";
import { expect, test } from "bun:test";

import electronUpdater, {
  AppUpdater,
  NoOpLogger,
  autoUpdater,
} from "./index.ts";

// Negative control: remove `export default`; the destructured value is undefined.
test("electron-updater.import.default-destructure: the default import's autoUpdater is an EventEmitter, an AppUpdater, and the same object as the named export", () => {
  const { autoUpdater: destructured } = electronUpdater;
  expect(destructured).toBeInstanceOf(EventEmitter);
  expect(destructured).toBeInstanceOf(AppUpdater);
  expect(destructured).toBe(autoUpdater);
});

test("electron-updater.import.module-object: the default export carries AppUpdater and NoOpLogger as upstream's module object does", () => {
  expect(electronUpdater.AppUpdater).toBe(AppUpdater);
  expect(electronUpdater.NoOpLogger).toBe(NoOpLogger);
  expect(Object.keys(electronUpdater).sort()).toEqual([
    "AppUpdater",
    "NoOpLogger",
    "autoUpdater",
  ]);
});

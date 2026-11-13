/**
 * `@keld/electron-updater` — the inert stand-in a migrated app's manifest
 * resolves for the `electron-updater` specifier instead of electron-updater
 * 6.8.9 (GH-491; X02-T3 / #499 own the manifest edit). It mirrors upstream's
 * `out/main.js` export shape for the members draw.io touches: the CJS module
 * object (`default`) carries `autoUpdater`, and the same names are ESM named
 * exports, so `import pkg from "electron-updater"; const {autoUpdater} = pkg`
 * (`electron.js:17-18`) works.
 *
 * Import-time contract (spec §3 criterion 2): no socket, Worker, child process,
 * file write or app-link frame. The only import-time work is constructing one
 * `EventEmitter`.
 */
import { AppUpdater, NoOpLogger } from "./app-updater.ts";
import { electronFacadeApp } from "./electron-app.ts";

export { AppUpdater, NoOpLogger };
export type {
  AppAdapter,
  AppUpdaterEventMap,
  DownloadNotification,
  FeedOptions,
  Logger,
  LoginAuthInfo,
  LoginCallback,
  ProgressInfo,
  PublishConfiguration,
  PublishProvider,
  ReleaseNoteInfo,
  UpdateCheckResult,
  UpdateDownloadedEvent,
  UpdateFileInfo,
  UpdateInfo,
  UpdaterEvent,
} from "./types.ts";

/** The process-wide updater, as `electron-updater` exposes one `autoUpdater`. */
export const autoUpdater: AppUpdater = new AppUpdater(null, electronFacadeApp);

const electronUpdater = { autoUpdater, AppUpdater, NoOpLogger };
export default electronUpdater;

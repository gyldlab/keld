/**
 * `@keld/electron` — Electron module alias target (architecture 04 §3.1).
 *
 * The installed `electron` dependency alias and its four subpaths share one
 * synchronous ESM entry. The existing SDK producer emits `dist/index.js` with
 * declared API/KIPC dependencies; the transport remains its own stamped Worker.
 *
 * `process.versions.electron` / `process.type` are installed at import so
 * `import "electron"` is enough to observe them — they do not wait on kipc.
 *
 * GH-565: eight further bindings draw.io's main imports by name link here
 * (`./unsupported-bindings.ts`); each of their members refuses with
 * `KELD-COMPAT-013` naming its owning issue until that slice lands. GH-478's
 * dialog module supplies the async methods and retains its remaining refusals. `ipcMain`
 * arrives with the GH-466 registration slice (PR #1365). Evaluating this module
 * sends no kipc frame.
 */

import "./entry-identity.ts";
import { app } from "./app.ts";
import { BrowserWindow } from "./browser-window.ts";
import { dialog } from "./dialog.ts";
import { nativeTheme } from "./native-theme.ts";
import { screen } from "./screen.ts";
import {
  ClipboardItem,
  Menu,
  clipboard,
  nativeImage,
  session,
  shell,
} from "./unsupported-bindings.ts";

/** Reported `process.versions.electron`. Keld 0.0.1, not an Electron release. */
const ELECTRON_VERSION_SHIM = "0.0.1";

const proc = process as typeof process & { type?: string };

(process.versions as { electron?: string }).electron = ELECTRON_VERSION_SHIM;
Object.defineProperty(proc, "type", {
  value: "browser",
  configurable: true,
  enumerable: true,
  writable: false,
});

export {
  app,
  BrowserWindow,
  ClipboardItem,
  Menu,
  clipboard,
  dialog,
  nativeImage,
  nativeTheme,
  screen,
  session,
  shell,
};
export type {
  MessageBoxOptions,
  MessageBoxResult,
  OpenDialogOptions,
  OpenDialogResult,
  SaveDialogOptions,
  SaveDialogResult,
} from "./dialog.ts";
export type {
  BrowserWindowEventMap,
  BrowserWindowOptions,
  CloseEvent,
  WebContents,
  WebContentsEventMap,
  WebPreferences,
} from "./browser-window.ts";
export type { KeldCallError } from "./link.ts";
export type { TrackingIssue, UnsupportedKind } from "./unsupported.ts";

const electron = {
  app,
  BrowserWindow,
  ClipboardItem,
  Menu,
  clipboard,
  dialog,
  nativeImage,
  nativeTheme,
  screen,
  session,
  shell,
};
export default electron;

/**
 * `@keld/electron` — Electron module alias target (architecture 04 §3.1).
 *
 * Bundler-side alias (`keld build`) is deferred: `keld build` is not live.
 * Dev module alias: Bun 1.3.14 honors `tsconfig.json` `compilerOptions.paths`
 * (`electron` → this file). `bunfig.toml` `[alias]` is the spec-named file
 * but does not remap runtime `import "electron"` on this Bun version.
 *
 * `process.versions.electron` / `process.type` are installed at import so
 * `import "electron"` is enough to observe them — they do not wait on kipc.
 *
 * GH-565: nine further bindings draw.io's main imports by name link here
 * (`./unsupported-bindings.ts`); each of their members refuses with
 * `KELD-COMPAT-013` naming its owning issue until that slice lands. `ipcMain`
 * arrives with the GH-466 registration slice (PR #1365). Evaluating this module
 * sends no kipc frame.
 */

import { app } from "./app";
import { BrowserWindow } from "./browser-window";
import {
  ClipboardItem,
  Menu,
  clipboard,
  dialog,
  nativeImage,
  nativeTheme,
  screen,
  session,
  shell,
} from "./unsupported-bindings";

/** Reported `process.versions.electron`. Keld 0.0.1, not an Electron release. */
export const ELECTRON_VERSION_SHIM = "0.0.1";

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
  BrowserWindowEventMap,
  BrowserWindowOptions,
  CloseEvent,
  WebContents,
  WebContentsEventMap,
  WebPreferences,
} from "./browser-window";
export { isCallError, type KeldCallError } from "./link";
export {
  UNSUPPORTED_MEMBER_CODE,
  UnsupportedMemberError,
  isUnsupportedMemberError,
  type TrackingIssue,
  type UnsupportedKind,
} from "./unsupported";

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

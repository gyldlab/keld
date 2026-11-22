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
 */

import { app } from "./app";
import { BrowserWindow } from "./browser-window";
import { dialog } from "./dialog";
import { nativeTheme, screen } from "./desktop.ts";
import { session } from "./session.ts";
import { Menu } from "./menu.ts";
import { ipcMain } from "./ipc-main.ts";
import {
  ClipboardItem,
  clipboard,
  nativeImage,
  shell,
} from "./unsupported-bindings.ts";

import { runtimeProcessFactsSelected } from "@keld/kipc";
import { installMainProcessFacts } from "./process-facts.ts";

installMainProcessFacts(process, runtimeProcessFactsSelected);

export {
  app,
  BrowserWindow,
  ClipboardItem,
  clipboard,
  dialog,
  ipcMain,
  Menu,
  nativeImage,
  nativeTheme,
  screen,
  session,
  shell,
};
export type { Display, DisplayRectangle } from "./desktop.ts";
export type {
  IpcMain,
  IpcMainEvent,
  IpcMainInvokeEvent,
  IpcMainInvokeHandler,
  IpcMainListener,
  WebFrameMain,
} from "./ipc-main.ts";
export type {
  MessageBoxOptions,
  MessageBoxResult,
  OpenDialogOptions,
  OpenDialogResult,
  SaveDialogOptions,
  SaveDialogResult,
} from "./dialog";
export type {
  BrowserWindowEventMap,
  BrowserWindowOptions,
  CloseEvent,
  WebContents,
  WebContentsEventMap,
  WebPreferences,
} from "./browser-window";
const electron = {
  app,
  BrowserWindow,
  ClipboardItem,
  clipboard,
  dialog,
  ipcMain,
  Menu,
  nativeImage,
  nativeTheme,
  screen,
  session,
  shell,
};
export default electron;

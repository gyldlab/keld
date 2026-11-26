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
import { Menu, MenuItem } from "./menu.ts";
import { nativeTheme } from "./native-theme.ts";
import { screen } from "./screen.ts";
import { session } from "./session.ts";
import { ipcMain, installIpcMainDelivery } from "./ipc-main.ts";
import {
  findIpcSenderContents,
  awaitOriginalIpcWindowCloseVeto,
  destroyOriginalIpcWindowDocument,
} from "./browser-window.ts";
import {
  registerCompatMainConsumer,
  sendCompatRequest,
} from "@keld/api/internal/app";
import { hostLaunchFacts } from "@keld/kipc";

installIpcMainDelivery({
  register: registerCompatMainConsumer,
  findSender: findIpcSenderContents,
  send: sendCompatRequest,
  awaitCloseVeto: awaitOriginalIpcWindowCloseVeto,
  destroyOriginalDocument: destroyOriginalIpcWindowDocument,
  report(error) {
    try {
      console.error("KELD main IPC delivery failed:", error);
    } catch {
      /* diagnostic only */
    }
  },
});
import {
  ClipboardItem,
  clipboard,
  nativeImage,
  shell,
} from "./unsupported-bindings.ts";

// One property owner is shared with the installed pre-entry bootstrap.
import { installMainProcessFacts } from "./process-facts.ts";
installMainProcessFacts(
  process,
  hostLaunchFacts?.kind === "local-development"
    ? { resourcesPath: hostLaunchFacts.resourcesRoot }
    : {},
);
export {
  app,
  BrowserWindow,
  ClipboardItem,
  Menu,
  MenuItem,
  clipboard,
  dialog,
  nativeImage,
  nativeTheme,
  screen,
  session,
  shell,
  ipcMain,
};
export type {
  IpcMainEvent,
  IpcMainListener,
  IpcMain,
  WebFrameMain,
} from "./ipc-main.ts";
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
  ipcMain,
  app,
  BrowserWindow,
  ClipboardItem,
  Menu,
  MenuItem,
  clipboard,
  dialog,
  nativeImage,
  nativeTheme,
  screen,
  session,
  shell,
};
export default electron;

export type {
  MenuItemConstructorOptions,
  MenuItemClick,
  MenuItemType,
} from "./menu.ts";

export type {
  DefaultSession,
  WebRequestFilter,
  WebRequestListener,
} from "./session.ts";

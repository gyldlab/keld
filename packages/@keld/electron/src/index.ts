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

import { runtimeProcessFactsSelected } from "@keld/kipc";
import {
  ELECTRON_VERSION_SHIM,
  installMainProcessFacts,
} from "./process-facts.ts";
export { ELECTRON_VERSION_SHIM };

installMainProcessFacts(process, runtimeProcessFactsSelected);

export { app, BrowserWindow, dialog };
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
export { isCallError, type KeldCallError } from "@keld/kipc";

const electron = { app, BrowserWindow, dialog };
export default electron;

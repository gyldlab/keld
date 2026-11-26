/**
 * `@keld/electron` — Electron module alias target (architecture 04 §3.1).
 *
 * Bundler-side alias (`keld build`) is deferred: `keld build` is not live.
 * Dev module alias: Bun 1.3.14 honors `tsconfig.json` `compilerOptions.paths`
 * (`electron` → this file). `bunfig.toml` `[alias]` is the spec-named file
 * but does not remap runtime `import "electron"` on this Bun version.
 *
 * Main-role `process.*` facts (`type`, `defaultApp`, `versions.electron`) are
 * installed by `./process-facts` at import so `import "electron"` is enough to
 * observe them — they do not wait on kipc. A Host-selected preload may install
 * them earlier through that same module (GH-570 spec T2).
 */

import { app } from "./app";
import { BrowserWindow } from "./browser-window";
import {
  ELECTRON_VERSION_SHIM,
  installMainProcessFacts,
} from "./process-facts";

installMainProcessFacts();

export { app, BrowserWindow };
export { ELECTRON_VERSION_SHIM };
export type {
  BrowserWindowEventMap,
  BrowserWindowOptions,
  CloseEvent,
  WebContents,
  WebContentsEventMap,
  WebPreferences,
} from "./browser-window";
export { isCallError, type KeldCallError } from "./link";

const electron = { app, BrowserWindow };
export default electron;

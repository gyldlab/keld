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

export { app };

const electron = { app };
export default electron;

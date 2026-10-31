/**
 * The Electron facade as the adapter's `AppAdapter`, mirroring upstream
 * `out/ElectronAppAdapter.js` (`require("electron").app`;
 * `get isPackaged() { return this.app.isPackaged === true }`).
 *
 * The packaged fact has one owner, F01-T4 (#452), surfaced as `app.isPackaged`
 * on `@keld/electron`. Until it is minted the property is absent, which reads as
 * "not packaged" exactly as upstream treats a non-`true` value, and as `keld dev`
 * is. Sibling packages are reached by relative source path, as `@keld/electron`
 * reaches `@keld/api` (spec §4.2 D2).
 */
import { app } from "../../electron/src/index.ts";

import type { AppAdapter } from "./types.ts";

export const electronFacadeApp: AppAdapter = {
  get isPackaged(): boolean {
    return Reflect.get(app, "isPackaged") === true;
  },
  quit(): Promise<void> {
    return app.quit();
  },
};

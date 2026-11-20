/**
 * The Electron facade as the adapter's `AppAdapter`, mirroring upstream
 * `out/ElectronAppAdapter.js` (`require("electron").app`;
 * `get isPackaged() { return this.app.isPackaged === true }`).
 *
 * The packaged fact has one owner, F01-T4 (#452), surfaced as `app.isPackaged`
 * on `@keld/electron`. Validated development facts report false. Missing facts
 * throw synchronously; that refusal propagates unchanged and cannot become a
 * successful unpackaged check. The bare `electron` peer selects the same
 * installed facade as the application; the package must not bundle a copy.
 */
import { app } from "electron";

import type { AppAdapter } from "./types.ts";

export const electronFacadeApp: AppAdapter = {
  get isPackaged(): boolean {
    return Reflect.get(app, "isPackaged") === true;
  },
  quit(): Promise<void> {
    return app.quit();
  },
};

/**
 * The Electron facade as the adapter's `AppAdapter`, mirroring upstream
 * `out/ElectronAppAdapter.js` (`require("electron").app`;
 * `get isPackaged() { return this.app.isPackaged === true }`).
 *
 * The packaged fact has one owner, F01-T4 (#452), surfaced as `app.isPackaged`
 * on `@keld/electron`. Validated development facts report false. Missing facts
 * throw synchronously; that refusal propagates unchanged and cannot become a
 * successful unpackaged check. Sibling packages are reached by relative source
 * path, as `@keld/electron` reaches `@keld/api` (spec §4.2 D2).
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

/**
 * The Electron facade as the adapter's `AppAdapter`, mirroring upstream
 * `out/ElectronAppAdapter.js` (`require("electron").app`;
 * `get isPackaged() { return this.app.isPackaged === true }`).
 *
 * The packaged fact has one owner, F01-T4 (#452), surfaced as `app.isPackaged`
 * on `@keld/electron`. At the source basis the property is absent, so upstream's
 * strict comparison returns false; that is not proof of a delivered dev fact.
 * A throwing facade getter propagates unchanged. No failure is caught or
 * converted to false here. Sibling packages are reached by relative source
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

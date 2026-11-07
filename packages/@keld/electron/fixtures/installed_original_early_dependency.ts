/** First application dependency: observations only, never an admission fixture. */
import { app } from "../src/app.ts";

function fact(read: () => unknown): unknown {
  try {
    return read();
  } catch (error) {
    if (!(error instanceof Error)) throw error;
    return {
      code: Reflect.get(error, "code"),
      fact: Reflect.get(error, "fact"),
      reason: Reflect.get(error, "reason"),
    };
  }
}

export const earlyFacts = {
  ready: app.isReady(),
  name: app.getName(),
  version: app.getVersion(),
  appPath: app.getAppPath(),
  isPackaged: app.isPackaged,
  appData: fact(() => app.getPath("appData")),
  userData: fact(() => app.getPath("userData")),
  documents: fact(() => app.getPath("documents")),
  locale: fact(() => app.getLocale()),
  singleInstance: fact(() => app.requestSingleInstanceLock()),
  defaultApp: Reflect.get(process, "defaultApp") ?? null,
  resourcesPath: Reflect.get(process, "resourcesPath") ?? null,
};

console.log("GH1404_ORIGINAL_DEPENDENCY_REACHED");

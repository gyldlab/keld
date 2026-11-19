/** Observe the current public surface without treating a missing method as a loader failure. */
import { app } from "../src/index.ts";
import * as transport from "../../kipc/src/transport.ts";

function readMethod(name: string): unknown {
  const member: unknown = Reflect.get(app, name);
  return typeof member === "function" ? member.call(app) : null;
}

console.log(
  "GH1404_INSTALLED_FACTS " +
    JSON.stringify({
      launchFacts: Reflect.get(transport, "hostLaunchFacts") ?? null,
      isPackaged: Reflect.get(app, "isPackaged") ?? null,
      appPath: readMethod("getAppPath"),
      name: readMethod("getName"),
      version: readMethod("getVersion"),
      resourcesPath: Reflect.get(process, "resourcesPath") ?? null,
      defaultApp: Reflect.get(process, "defaultApp") ?? null,
      execPath: process.execPath,
      argv: process.argv,
    }),
);

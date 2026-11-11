import { early, argvReference } from "./installed_launch_early_dependency.ts";
import { app } from "../src/index.ts";

console.log(
  "GH1404_EARLY_FACTS " +
    JSON.stringify({
      early,
      isPackaged: app.isPackaged,
      appPath: app.getAppPath(),
      resourcesPath: Reflect.get(process, "resourcesPath") ?? null,
      sameArgvObject: argvReference === process.argv,
      userArguments: process.argv.slice(
        Reflect.get(process, "defaultApp") ? 2 : 1,
      ),
    }),
);

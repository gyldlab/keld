/**
 * Stage as src/main.ts with early.ts beside it. This is an observer for the
 * actual installed Host; it never provides a fact, preload or substitute facade.
 */
import { early, initialArgvObject } from "./early.ts";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { app } from "@keld/electron";
import { fs, isCallError } from "@keld/api";
import { hostLaunchFacts } from "@keld/kipc";
import { publishReadyWitness } from "./ready-witness.ts";
// The test driver writes this literal module with a new evidence path and nonce.
import { readyWitnessConfig } from "./witness-config.ts";

// Independent fixture expectations come from the entry the bundled Bun actually
// opened. Production facts may not use this inference; this observer may.
const entry = fileURLToPath(import.meta.url);
const appRoot = dirname(dirname(entry));
const resourcesRoot = dirname(appRoot);
const bundledBun = join(resourcesRoot, "../Helpers/bun");
const observed = {
  early,
  entry,
  expected: { appRoot, resourcesRoot, bundledBun },
  launchFacts: hostLaunchFacts ?? null,
  launchFactsFrozen:
    hostLaunchFacts !== undefined && Object.isFrozen(hostLaunchFacts),
  launchFactsPrototypeIsNull:
    hostLaunchFacts !== undefined &&
    Object.getPrototypeOf(hostLaunchFacts) === null,
  app: {
    isPackaged: app.isPackaged,
    appPath: app.getAppPath(),
    name: app.getName(),
    version: app.getVersion(),
  },
  sameArgvObject: initialArgvObject === process.argv,
  userArguments: process.argv.slice(Reflect.get(process, "defaultApp") ? 2 : 1),
};
console.log("GH1404_NATIVE_FACTS_BEFORE_READY " + JSON.stringify(observed));

// Observe the actual public Quit Promise after the real last-window event.
// This listener replaces only the facade's ordinary zero-listener default.
app.on("window-all-closed", () => {
  void app.quit().then(
    () => console.log("GH1404_QUIT_REPLY_CONFIRMED"),
    (error: unknown) => {
      console.error("GH1404_QUIT_REPLY_FAILED", error);
      process.exitCode = 1;
    },
  );
});

const frozen = (descriptor: PropertyDescriptor | null, value: unknown) =>
  descriptor?.value === value &&
  descriptor.writable === false &&
  descriptor.enumerable === true &&
  descriptor.configurable === false;

if (
  early.type !== "browser" ||
  early.defaultApp !== true ||
  early.resourcesPath !== resourcesRoot ||
  early.execPath !== bundledBun ||
  early.argv.length !== 2 ||
  early.argv[0] !== bundledBun ||
  early.argv[1] !== entry ||
  !frozen(early.defaultDescriptor, true) ||
  !frozen(early.resourcesDescriptor, resourcesRoot) ||
  hostLaunchFacts?.schema !== 6 ||
  hostLaunchFacts.kind !== "local-development" ||
  hostLaunchFacts.root !== appRoot ||
  hostLaunchFacts.resourcesRoot !== resourcesRoot ||
  !observed.launchFactsFrozen ||
  !observed.launchFactsPrototypeIsNull ||
  !observed.app.isPackaged ||
  observed.app.appPath !== appRoot ||
  !observed.sameArgvObject ||
  observed.userArguments.length !== 0
) {
  throw new Error(
    "GH1404_NATIVE_FACTS_FAILED: installed observations do not match the actual entry layout",
  );
}

await app.whenReady();
if (!app.isReady()) {
  throw new Error("Host Ready resolved without the actual ready state");
}

// The sealed manifest grants only the separate allowed directory. Both files
// exist, so a missing-file error cannot impersonate a Host permission refusal.
const allowedFile = join(readyWitnessConfig.allowedRoot, "sentinel.txt");
const text = (bytes: Uint8Array) => new TextDecoder().decode(bytes);
if (text(await fs.read(allowedFile)) !== "GH1404_ALLOWED_SENTINEL\n") {
  throw new Error(
    "allowed native read did not return the independent sentinel",
  );
}
await fs.write(allowedFile, new TextEncoder().encode("GH1404_ALLOWED_WRITE\n"));
async function requireScopeDenial(
  operation: "read" | "write",
): Promise<string> {
  try {
    if (operation === "read") await fs.read(readyWitnessConfig.deniedFile);
    else
      await fs.write(
        readyWitnessConfig.deniedFile,
        new TextEncoder().encode("UNAUTHORIZED_WRITE\n"),
      );
  } catch (error: unknown) {
    if (isCallError(error) && error.code === "KELD-GUARD002") return error.code;
    throw error;
  }
  throw new Error(`Host unexpectedly granted the out-of-scope ${operation}`);
}
const denials = {
  read: await requireScopeDenial("read"),
  write: await requireScopeDenial("write"),
};
if (text(await fs.read(allowedFile)) !== "GH1404_ALLOWED_WRITE\n") {
  throw new Error(
    "healthy broker read after denial did not preserve the allowed write",
  );
}
console.log("GH1404_NATIVE_BROKER_CONTROLS " + JSON.stringify(denials));
await publishReadyWitness(readyWitnessConfig, observed);
console.log(
  "GH1404_NATIVE_FACTS_READY " +
    JSON.stringify({
      pid: process.pid,
      ppid: process.ppid,
      ready: app.isReady(),
      appPath: app.getAppPath(),
      resourcesPath: Reflect.get(process, "resourcesPath"),
      execPath: process.execPath,
      argv: process.argv,
    }),
);
// Keep the real app/link available for the native Quit and process-reap oracle.
// No fixture timer or direct process exit substitutes for Host shutdown.

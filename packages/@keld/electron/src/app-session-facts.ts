// GH452 app metadata and the GH453 no-argument Windows development verdict.
import {
  hostLaunchFacts,
  stateDefineProperty,
  stateErrorConstructor,
  stateJsonStringify,
  windowsOriginalDevelopmentSelected,
} from "@keld/kipc";

const pathNames = [
  "home",
  "appData",
  "assets",
  "userData",
  "sessionData",
  "temp",
  "exe",
  "module",
  "desktop",
  "documents",
  "downloads",
  "music",
  "pictures",
  "videos",
  "recent",
  "logs",
  "crashDumps",
] as const;

type AppPathName = (typeof pathNames)[number];
type AppSessionFact =
  | "name"
  | "version"
  | "appPath"
  | "locale"
  | "isPackaged"
  | "singleInstance"
  | `path.${AppPathName}`;

const documentedPathNames =
  "home, appData, assets, userData, sessionData, temp, exe, module, desktop, documents, downloads, music, pictures, videos, recent, logs, crashDumps";

function defineOwnValue(target: object, name: string, value: unknown): void {
  const descriptor = {
    __proto__: null,
    value,
    configurable: true,
    enumerable: true,
    writable: true,
  };
  stateDefineProperty(target, name, descriptor);
}

/** Typed refusal for an unavailable fact or unsupported fact access. */
export class AppSessionFactError extends stateErrorConstructor {
  declare readonly code:
    "KELD-COMPAT-040" | "KELD-COMPAT-041" | "KELD-COMPAT-042";
  declare readonly fact: AppSessionFact | undefined;
  constructor(
    code: AppSessionFactError["code"],
    fact: AppSessionFact | undefined,
    detail: string,
  ) {
    super(`${code}: ${detail}`);
    defineOwnValue(this, "name", "AppSessionFactError");
    defineOwnValue(this, "code", code);
    defineOwnValue(this, "fact", fact);
  }
}

function missing(fact: AppSessionFact, call: string): never {
  throw new AppSessionFactError(
    "KELD-COMPAT-040",
    fact,
    `${call} requires Host session fact ${fact}. Launch through the qualified Host. GH-452 owns delivery of this fact.`,
  );
}

function readText(
  field: "name" | "root" | "version" | "locale",
  fact: AppSessionFact,
  call: string,
): string {
  return hostLaunchFacts?.[field] ?? missing(fact, call);
}

function isAppPathName(name: unknown): name is AppPathName {
  for (let index = 0; index < pathNames.length; index++) {
    if (pathNames[index] === name) return true;
  }
  return false;
}

function readPath(name: string): string {
  if (typeof name !== "string" || !isAppPathName(name)) {
    const received =
      typeof name === "string" ? stateJsonStringify(name) : typeof name;
    throw new AppSessionFactError(
      "KELD-COMPAT-041",
      undefined,
      `app.getPath received ${received}. Pass a documented path name from ${documentedPathNames}.`,
    );
  }
  if (name === "appData" || name === "userData" || name === "documents") {
    const path = hostLaunchFacts?.paths[name];
    if (path !== undefined) return path;
  }
  return missing(`path.${name}`, "app.getPath()");
}

function readSingleInstanceVerdict(...args: readonly unknown[]): boolean {
  if (args.length > 1 || (args.length === 1 && args[0] !== undefined)) {
    const error = new stateErrorConstructor(
      "KELD-COMPAT-013: app.requestSingleInstanceLock(additionalData) is unsupported. Call without additional data. GH-453 owns this contract.",
    );
    defineOwnValue(error, "name", "UnsupportedMemberError");
    defineOwnValue(error, "code", "KELD-COMPAT-013");
    defineOwnValue(
      error,
      "member",
      "app.requestSingleInstanceLock(additionalData)",
    );
    defineOwnValue(error, "ticket", "GH-453");
    defineOwnValue(error, "kind", "method");
    throw error;
  }
  const verdict = hostLaunchFacts?.singleInstance?.verdict;
  if (verdict === undefined) {
    const reason = hostLaunchFacts === undefined ? "unavailable" : "undeclared";
    const error = new AppSessionFactError(
      "KELD-COMPAT-040",
      "singleInstance",
      `app.requestSingleInstanceLock() has ${reason} Host election facts. Declare singleInstance with the canonical application ID and launch through the qualified Host. GH-453 owns this contract.`,
    );
    defineOwnValue(error, "reason", reason);
    throw error;
  }
  return verdict === "acquired";
}

/** Facts available to an app on selected Windows original-development launch. */
export interface WindowsAppSessionFacts {
  getPath(name: string): string;
  getVersion(): string;
  getLocale(): string;
  getName(): string;
  getAppPath(): string;
  requestSingleInstanceLock(...args: readonly unknown[]): boolean;
  name: string;
  readonly isPackaged: boolean;
}

/** Install the selected getters on the existing lifecycle facade object. */
export function attachAppSessionFacts<T extends object>(
  target: T,
): T & Partial<WindowsAppSessionFacts> {
  if (!windowsOriginalDevelopmentSelected)
    return target as T & Partial<WindowsAppSessionFacts>;
  defineOwnValue(target, "getPath", readPath);
  defineOwnValue(target, "getVersion", () =>
    readText("version", "version", "app.getVersion()"),
  );
  defineOwnValue(target, "getLocale", () =>
    readText("locale", "locale", "app.getLocale()"),
  );
  defineOwnValue(target, "getName", () =>
    readText("name", "name", "app.getName()"),
  );
  defineOwnValue(target, "getAppPath", () =>
    readText("root", "appPath", "app.getAppPath()"),
  );
  defineOwnValue(
    target,
    "requestSingleInstanceLock",
    readSingleInstanceVerdict,
  );
  const name = {
    __proto__: null,
    get: () => readText("name", "name", "app.name"),
    set: () => {
      throw new AppSessionFactError(
        "KELD-COMPAT-042",
        undefined,
        "Declare the app name in keld.config.ts. The Host owns app.name for this session.",
      );
    },
  };
  const isPackaged = {
    __proto__: null,
    get: () =>
      hostLaunchFacts ? false : missing("isPackaged", "app.isPackaged"),
  };
  stateDefineProperty(target, "name", name);
  stateDefineProperty(target, "isPackaged", isPackaged);
  return target as T & Partial<WindowsAppSessionFacts>;
}

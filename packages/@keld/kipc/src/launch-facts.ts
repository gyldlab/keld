// Canonical decoder for the selected Windows original-development Host record.
// echo-codegen embeds this source in the standalone transport.
const LAUNCH_FACT_TEXT_BYTES = 4096;
const LAUNCH_FACT_RECORD_BYTES = 16 * 1024;
const launchPathNames = ["appData", "userData", "documents"] as const;
type LaunchPathName = (typeof launchPathNames)[number];

interface HostOriginalResourcePolicy {
  readonly contract: "gh473-original-editor-offline/v1";
  readonly graphSha256: string;
  readonly policySha256: string;
  readonly windowKey: string;
  readonly selector:
    "original-editor-offline" | "original-editor-offline-empty-style";
}

interface HostSingleInstanceFact {
  readonly verdict: "acquired" | "contended";
}

/** Data from the verified Windows development Host. Paths grant no access. */
export interface WindowsOriginalDevelopmentHostLaunchFacts {
  readonly schema: 8;
  readonly kind: "development";
  readonly name: string;
  readonly root: string;
  readonly version?: string;
  readonly locale?: string;
  readonly paths: Readonly<Partial<Record<LaunchPathName, string>>>;
  readonly originalResourcePolicy: HostOriginalResourcePolicy;
  readonly singleInstance?: HostSingleInstanceFact;
  readonly windowsApplicationMenu?: "absent-v1";
}

interface LaunchFactsIntrinsics {
  readonly apply: typeof Reflect.apply;
  readonly ownKeys: typeof Reflect.ownKeys;
  readonly descriptor: typeof Object.getOwnPropertyDescriptor;
  readonly defineProperty: typeof Object.defineProperty;
  readonly setPrototypeOf: typeof Object.setPrototypeOf;
  readonly freeze: typeof Object.freeze;
  readonly isArray: typeof Array.isArray;
  readonly isProxy: (value: unknown) => boolean;
  readonly startsWith: typeof String.prototype.startsWith;
  readonly slice: typeof String.prototype.slice;
  readonly codeUnit: typeof String.prototype.charCodeAt;
  readonly exec: typeof RegExp.prototype.exec;
  readonly stringify: typeof JSON.stringify;
  readonly encodedLength: (value: string) => number;
  readonly Error: ErrorConstructor;
}

/** @internal Bind the decoder once to the transport's captured primitives. */
function bindLaunchFactsCapture(intrinsic: LaunchFactsIntrinsics) {
  function invalidLaunchFacts(): never {
    const error = new intrinsic.Error(
      "KELD-IPC-029: invalid Host launch facts; restage through the qualified Host startup path",
    );
    const descriptor = {
      __proto__: null,
      value: "KELD-IPC-029",
      enumerable: true,
    };
    intrinsic.defineProperty(error, "code", descriptor);
    throw error;
  }

  function containsFactName(names: readonly string[], key: string): boolean {
    for (let index = 0; index < names.length; index++) {
      if (names[index] === key) return true;
    }
    return false;
  }

  function launchFactsFields(
    value: unknown,
    required: readonly string[],
    optional: readonly string[] = [],
  ): Readonly<Record<string, unknown>> {
    if (
      value === null ||
      typeof value !== "object" ||
      intrinsic.isProxy(value) ||
      intrinsic.isArray(value)
    )
      return invalidLaunchFacts();
    const copied: Record<string, unknown> = intrinsic.setPrototypeOf({}, null);
    const names = intrinsic.ownKeys(value);
    for (let index = 0; index < names.length; index++) {
      const key = names[index];
      if (
        typeof key !== "string" ||
        (!containsFactName(required, key) && !containsFactName(optional, key))
      )
        return invalidLaunchFacts();
      const field = intrinsic.descriptor(value, key);
      if (!field || intrinsic.descriptor(field, "value") === undefined)
        return invalidLaunchFacts();
      copied[key] = field.value;
    }
    for (let index = 0; index < required.length; index++) {
      const key = required[index];
      if (key === undefined || intrinsic.descriptor(copied, key) === undefined)
        return invalidLaunchFacts();
    }
    return copied;
  }

  function freezeLaunchFacts<T extends object>(facts: T): Readonly<T> {
    intrinsic.setPrototypeOf(facts, null);
    return intrinsic.freeze(facts);
  }

  function launchFactsText(value: unknown): value is string {
    if (
      typeof value !== "string" ||
      value.length === 0 ||
      value.length > LAUNCH_FACT_TEXT_BYTES
    )
      return false;
    for (let index = 0; index < value.length; index++) {
      const code = intrinsic.apply(intrinsic.codeUnit, value, [index]);
      if (code === 0 || (code >= 0xdc00 && code <= 0xdfff)) return false;
      if (code >= 0xd800 && code <= 0xdbff) {
        index++;
        if (index === value.length) return false;
        const next = intrinsic.apply(intrinsic.codeUnit, value, [index]);
        if (next < 0xdc00 || next > 0xdfff) return false;
      }
    }
    return intrinsic.encodedLength(value) <= LAUNCH_FACT_TEXT_BYTES;
  }

  function optionalLaunchFactsText(
    fields: Readonly<Record<string, unknown>>,
    key: string,
  ): string | undefined {
    if (intrinsic.descriptor(fields, key) === undefined) return undefined;
    const value = fields[key];
    return launchFactsText(value) ? value : invalidLaunchFacts();
  }

  /** Exact retained Windows local-drive spelling; never normalize or probe it. */
  function windowsDevelopmentRoot(value: string): boolean {
    return startsLaunchPath(value, "\\\\?\\") && windowsDrivePath(value, 4);
  }

  function startsLaunchPath(value: string, prefix: string): boolean {
    return intrinsic.apply(intrinsic.startsWith, value, [prefix]);
  }

  function windowsDrivePath(value: string, start: number): boolean {
    const tail = start + 3;
    if (value.length < tail) return false;
    const letter = value[start];
    if (
      letter === undefined ||
      !((letter >= "A" && letter <= "Z") || (letter >= "a" && letter <= "z")) ||
      value[start + 1] !== ":" ||
      value[start + 2] !== "\\"
    )
      return false;
    return value.length === tail || windowsPathComponents(value, tail, 1);
  }

  function windowsPathComponents(
    value: string,
    start: number,
    minimum: number,
  ): boolean {
    let count = 0;
    for (let index = start; index <= value.length; index++) {
      if (index !== value.length && value[index] !== "\\") continue;
      const component = intrinsic.apply(intrinsic.slice, value, [start, index]);
      if (!windowsPathComponent(component)) return false;
      start = index + 1;
      count++;
    }
    return count >= minimum;
  }

  function windowsPathComponent(value: string): boolean {
    return (
      value !== "" &&
      value !== "." &&
      value !== ".." &&
      intrinsic.apply(intrinsic.exec, /[\u0000-\u001f/:*?"<>|]/, [value]) ===
        null
    );
  }

  /** Actual KnownFolder metadata may be redirected to a native UNC path. */
  function windowsKnownFolderPath(value: string): boolean {
    if (startsLaunchPath(value, "\\\\?\\UNC\\"))
      return windowsPathComponents(value, 8, 2);
    if (startsLaunchPath(value, "\\\\?\\")) return windowsDrivePath(value, 4);
    if (startsLaunchPath(value, "\\\\"))
      return windowsPathComponents(value, 2, 2);
    return windowsDrivePath(value, 0);
  }

  function captureLaunchPaths(
    value: unknown,
  ): WindowsOriginalDevelopmentHostLaunchFacts["paths"] {
    const fields = launchFactsFields(value, [], launchPathNames);
    const paths: Partial<Record<LaunchPathName, string>> =
      intrinsic.setPrototypeOf({}, null);
    for (let index = 0; index < launchPathNames.length; index++) {
      const name = launchPathNames[index];
      if (name === undefined) return invalidLaunchFacts();
      const path = optionalLaunchFactsText(fields, name);
      if (path !== undefined) {
        if (!windowsKnownFolderPath(path)) return invalidLaunchFacts();
        paths[name] = path;
      }
    }
    return freezeLaunchFacts(paths);
  }

  function captureOriginalPolicy(value: unknown): HostOriginalResourcePolicy {
    const fields = launchFactsFields(value, [
      "contract",
      "graphSha256",
      "policySha256",
      "windowKey",
      "selector",
    ]);
    const { contract, graphSha256, policySha256, windowKey, selector } = fields;
    if (
      contract !== "gh473-original-editor-offline/v1" ||
      (selector !== "original-editor-offline" &&
        selector !== "original-editor-offline-empty-style") ||
      !launchFactsText(graphSha256) ||
      intrinsic.apply(intrinsic.exec, /^sha256:[0-9a-f]{64}$/, [
        graphSha256,
      ]) === null ||
      !launchFactsText(policySha256) ||
      intrinsic.apply(intrinsic.exec, /^sha256:[0-9a-f]{64}$/, [
        policySha256,
      ]) === null ||
      !launchFactsText(windowKey)
    )
      return invalidLaunchFacts();
    return freezeLaunchFacts({
      contract,
      graphSha256,
      policySha256,
      windowKey,
      selector,
    });
  }

  function captureSingleInstanceFact(value: unknown): HostSingleInstanceFact {
    const { verdict } = launchFactsFields(value, ["verdict"]);
    if (verdict !== "acquired" && verdict !== "contended")
      return invalidLaunchFacts();
    return freezeLaunchFacts({ verdict });
  }

  /** Capture only the approved schema 8 on the selected Host route. */
  return function captureHostLaunchFacts(
    value: unknown,
    selected: boolean,
  ): WindowsOriginalDevelopmentHostLaunchFacts | undefined {
    if (value === undefined) return undefined;
    if (!selected) return invalidLaunchFacts();
    const fields = launchFactsFields(
      value,
      ["schema", "kind", "name", "root", "paths", "originalResourcePolicy"],
      ["version", "locale", "singleInstance", "windowsApplicationMenu"],
    );
    const { schema, kind, name, root } = fields;
    if (
      schema !== 8 ||
      kind !== "development" ||
      !launchFactsText(name) ||
      !launchFactsText(root) ||
      !windowsDevelopmentRoot(root)
    )
      return invalidLaunchFacts();
    const version = optionalLaunchFactsText(fields, "version");
    const locale = optionalLaunchFactsText(fields, "locale");
    const singleInstance =
      intrinsic.descriptor(fields, "singleInstance") !== undefined
        ? captureSingleInstanceFact(fields.singleInstance)
        : undefined;
    let windowsApplicationMenu: WindowsOriginalDevelopmentHostLaunchFacts["windowsApplicationMenu"];
    if (intrinsic.descriptor(fields, "windowsApplicationMenu") !== undefined) {
      if (fields.windowsApplicationMenu !== "absent-v1")
        return invalidLaunchFacts();
      windowsApplicationMenu = fields.windowsApplicationMenu;
    }
    const captured =
      freezeLaunchFacts<WindowsOriginalDevelopmentHostLaunchFacts>({
        schema,
        kind,
        name,
        root,
        paths: captureLaunchPaths(fields.paths),
        originalResourcePolicy: captureOriginalPolicy(
          fields.originalResourcePolicy,
        ),
        ...(version === undefined ? {} : { version }),
        ...(locale === undefined ? {} : { locale }),
        ...(singleInstance === undefined ? {} : { singleInstance }),
        ...(windowsApplicationMenu === undefined ? {} : { windowsApplicationMenu }),
      });
    const json = intrinsic.stringify(captured);
    if (
      json === undefined ||
      intrinsic.encodedLength(json) > LAUNCH_FACT_RECORD_BYTES
    )
      return invalidLaunchFacts();
    return captured;
  };
}

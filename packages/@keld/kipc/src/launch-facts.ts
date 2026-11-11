/**
 * Closed, non-authority Host launch facts (GH-452; GH-1404 §4.5).
 *
 * This is the single JavaScript codec. echo-codegen embeds this module intact in
 * the standalone canonical transport. Installed bootstrap imports this same
 * source as a separately generated, exact-byte-checked module.
 */
import { isProxy as isLaunchFactsProxy } from "node:util/types";

// The qualified Host fixes this parse-time constant before application imports.
declare const __KELD_HOST_LAUNCH_FACTS__: unknown;

export interface DevelopmentHostLaunchFacts {
  readonly schema: 1 | 2 | 3 | 4 | 5;
  readonly kind: "development";
  readonly name: string;
  readonly root: string;
  readonly version?: string;
  readonly locale?: string;
  readonly singleInstance?: HostSingleInstanceFact;
  readonly originalResourcePolicy?: HostOriginalResourcePolicy;
  readonly paths?: Readonly<
    Partial<Record<"appData" | "userData" | "documents", string>>
  >;
}

/** A captured kernel-lock result; the descriptor and ownership stay with Host. */
export interface HostSingleInstanceFact {
  readonly verdict: "acquired" | "contended";
}

/** Prepared-source data; each native operation still needs its own Guard grant. */
export interface HostOriginalResourcePolicy {
  readonly contract: "gh473-original-editor-offline/v1";
  readonly graphSha256: string;
  readonly policySha256: string;
  readonly windowKey: string;
  readonly selector:
    "original-editor-offline" | "original-editor-offline-empty-style";
}

export interface InstalledHostLaunchFacts {
  readonly schema: 6;
  readonly kind: "local-development";
  readonly name: string;
  readonly root: string;
  readonly version: string;
  readonly resourcesRoot: string;
  readonly locale?: never;
  readonly paths?: never;
  readonly singleInstance?: never;
  readonly originalResourcePolicy?: never;
}

/** Closed installed-original extension; schema 6 retains its original meaning. */
export interface InstalledOriginalHostLaunchFacts {
  readonly schema: 7;
  readonly kind: "local-development";
  readonly name: string;
  readonly root: string;
  readonly version: string;
  readonly resourcesRoot: string;
  readonly paths: Readonly<
    Partial<Record<"appData" | "userData" | "documents", string>>
  >;
  readonly locale?: string;
  readonly singleInstance?: HostSingleInstanceFact;
  readonly originalResourcePolicy: HostOriginalResourcePolicy;
}

/** Data only. Neither a path nor a packaged-state fact grants access. */
export type HostLaunchFacts =
  | DevelopmentHostLaunchFacts
  | InstalledHostLaunchFacts
  | InstalledOriginalHostLaunchFacts;

function invalidLaunchFacts(): never {
  throw Object.assign(
    new Error(
      "KELD-IPC-029: invalid Host launch facts; restage through the qualified Host startup path",
    ),
    { code: "KELD-IPC-029" },
  );
}

/** @internal The single closed decoder, also used by the installed preload. */
export function captureHostLaunchFacts(
  value: unknown,
): HostLaunchFacts | undefined {
  if (value === undefined) return undefined;
  if (
    value === null ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    isLaunchFactsProxy(value)
  )
    return invalidLaunchFacts();
  const keys = Reflect.ownKeys(value);
  const baseKeys = ["schema", "kind", "name", "root"];
  for (const key of keys) {
    if (!Object.getOwnPropertyDescriptor(value, key)?.hasOwnProperty("value"))
      return invalidLaunchFacts();
  }
  const record = value as Record<string, unknown>;
  // Decide the closed variant from an own data descriptor. A missing schema
  // must not read a prototype accessor before the required-key refusal below.
  const schema = Object.getOwnPropertyDescriptor(value, "schema")?.value;
  const installed = schema === 6 || schema === 7;
  const required = installed
    ? [
        ...baseKeys,
        "version",
        "resourcesRoot",
        ...(schema === 7 ? ["paths", "originalResourcePolicy"] : []),
      ]
    : baseKeys;
  const allowed = installed
    ? [...required, ...(schema === 7 ? ["locale", "singleInstance"] : [])]
    : [
        ...baseKeys,
        "version",
        "paths",
        "locale",
        "singleInstance",
        "originalResourcePolicy",
      ];
  if (
    required.some((key) => !keys.includes(key)) ||
    keys.some((key) => typeof key !== "string" || !allowed.includes(key))
  )
    return invalidLaunchFacts();
  const encoder = new TextEncoder();
  const validText = (text: unknown): text is string =>
    typeof text === "string" &&
    text.length > 0 &&
    !text.includes("\0") &&
    encoder.encode(text).byteLength <= 4096;
  if (
    !validText(record.name) ||
    !validText(record.root) ||
    !record.root.startsWith("/")
  )
    return invalidLaunchFacts();

  if (
    installed
      ? record.kind !== "local-development"
      : (schema !== 1 &&
          schema !== 2 &&
          schema !== 3 &&
          schema !== 4 &&
          schema !== 5) ||
        record.kind !== "development"
  )
    return invalidLaunchFacts();
  if (schema === 1 && keys.length !== baseKeys.length)
    return invalidLaunchFacts();

  // Only the new installed schema has these canonical native-path requirements.
  // Earlier schemas retain their existing lexical contract.
  const canonicalPath = (path: string): boolean =>
    path === "/" ||
    (path.startsWith("/") &&
      path
        .slice(1)
        .split("/")
        .every((part) => part !== "" && part !== "." && part !== ".."));
  let version: string | undefined;
  if (keys.includes("version")) {
    if (!validText(record.version)) return invalidLaunchFacts();
    version = record.version;
  }
  let locale: string | undefined;
  if (keys.includes("locale")) {
    if (
      (schema !== 3 && schema !== 4 && schema !== 5 && schema !== 7) ||
      !validText(record.locale)
    )
      return invalidLaunchFacts();
    // The native owner already selected the first ordered preference. Preserve
    // those bounded UTF-8 bytes; do not invent a second localization policy.
    locale = record.locale;
  }
  let paths: DevelopmentHostLaunchFacts["paths"];
  if (schema !== 1 && schema !== 6) {
    if (!keys.includes("paths")) return invalidLaunchFacts();
    const input = record.paths;
    if (
      input === null ||
      typeof input !== "object" ||
      Array.isArray(input) ||
      isLaunchFactsProxy(input)
    )
      return invalidLaunchFacts();
    const copied: Partial<
      Record<"appData" | "userData" | "documents", string>
    > = Object.create(null);
    for (const key of Reflect.ownKeys(input)) {
      if (key !== "appData" && key !== "userData" && key !== "documents")
        return invalidLaunchFacts();
      const descriptor = Object.getOwnPropertyDescriptor(input, key);
      if (!descriptor?.hasOwnProperty("value")) return invalidLaunchFacts();
      const path: unknown = descriptor.value;
      if (
        !validText(path) ||
        !path.startsWith("/") ||
        (schema === 7 && !canonicalPath(path))
      )
        return invalidLaunchFacts();
      copied[key] = path;
    }
    paths = Object.freeze(copied);
  }
  let singleInstance: HostSingleInstanceFact | undefined;
  if (keys.includes("singleInstance")) {
    const input = record.singleInstance;
    if (
      (schema !== 4 && schema !== 5 && schema !== 7) ||
      input === null ||
      typeof input !== "object" ||
      Array.isArray(input) ||
      isLaunchFactsProxy(input)
    )
      return invalidLaunchFacts();
    const fields = Reflect.ownKeys(input);
    const descriptor = Object.getOwnPropertyDescriptor(input, "verdict");
    if (
      fields.length !== 1 ||
      fields[0] !== "verdict" ||
      !descriptor?.hasOwnProperty("value") ||
      (descriptor.value !== "acquired" && descriptor.value !== "contended")
    )
      return invalidLaunchFacts();
    const copy = Object.create(null) as { verdict: "acquired" | "contended" };
    copy.verdict = descriptor.value;
    singleInstance = Object.freeze(copy);
  }
  let originalResourcePolicy: HostOriginalResourcePolicy | undefined;
  if (
    (schema === 5 || schema === 7) !== keys.includes("originalResourcePolicy")
  )
    return invalidLaunchFacts();
  if (keys.includes("originalResourcePolicy")) {
    const input = record.originalResourcePolicy;
    if (
      input === null ||
      typeof input !== "object" ||
      Array.isArray(input) ||
      isLaunchFactsProxy(input)
    )
      return invalidLaunchFacts();
    const fields = [
      "contract",
      "graphSha256",
      "policySha256",
      "windowKey",
      "selector",
    ] as const;
    const own = Reflect.ownKeys(input);
    if (
      own.length !== fields.length ||
      own.some(
        (key) =>
          typeof key !== "string" ||
          !fields.includes(key as (typeof fields)[number]),
      )
    )
      return invalidLaunchFacts();
    const copied = Object.create(null) as Record<
      (typeof fields)[number],
      string
    >;
    for (const key of fields) {
      const descriptor = Object.getOwnPropertyDescriptor(input, key);
      if (!descriptor?.hasOwnProperty("value") || !validText(descriptor.value))
        return invalidLaunchFacts();
      copied[key] = descriptor.value;
    }
    if (
      copied.contract !== "gh473-original-editor-offline/v1" ||
      (copied.selector !== "original-editor-offline" &&
        !(
          schema === 7 &&
          copied.selector === "original-editor-offline-empty-style"
        )) ||
      !/^sha256:[0-9a-f]{64}$/.test(copied.graphSha256) ||
      !/^sha256:[0-9a-f]{64}$/.test(copied.policySha256)
    )
      return invalidLaunchFacts();
    originalResourcePolicy = Object.freeze(
      copied,
    ) as HostOriginalResourcePolicy;
  }

  let captured: HostLaunchFacts;
  if (installed) {
    if (
      !validText(version) ||
      !validText(record.resourcesRoot) ||
      !record.resourcesRoot.startsWith("/") ||
      record.root !== record.resourcesRoot + "/app"
    )
      return invalidLaunchFacts();
    const metadata = {
      kind: "local-development" as const,
      name: record.name,
      root: record.root,
      version,
      resourcesRoot: record.resourcesRoot,
    };
    if (schema === 7) {
      if (
        paths === undefined ||
        originalResourcePolicy === undefined ||
        record.name === "." ||
        record.name === ".." ||
        record.name.includes("/") ||
        !canonicalPath(record.root) ||
        !canonicalPath(record.resourcesRoot)
      )
        return invalidLaunchFacts();
      if (singleInstance !== undefined && paths.appData === undefined)
        return invalidLaunchFacts();
      if (
        paths.userData !== undefined &&
        (paths.appData === undefined ||
          paths.userData !==
            (paths.appData === "/" ? "/" : paths.appData + "/") + record.name)
      )
        return invalidLaunchFacts();
      captured = {
        schema: 7,
        ...metadata,
        paths,
        ...(locale === undefined ? {} : { locale }),
        ...(singleInstance === undefined ? {} : { singleInstance }),
        originalResourcePolicy,
      };
    } else {
      captured = { schema: 6, ...metadata };
    }
  } else {
    captured = {
      schema,
      kind: "development",
      name: record.name,
      root: record.root,
      ...(version === undefined ? {} : { version }),
      ...(locale === undefined ? {} : { locale }),
      ...(paths === undefined ? {} : { paths }),
      ...(singleInstance === undefined ? {} : { singleInstance }),
      ...(originalResourcePolicy === undefined
        ? {}
        : { originalResourcePolicy }),
    };
  }
  // Do this before serialization too: an inherited toJSON must not run.
  Object.setPrototypeOf(captured, null);
  if (encoder.encode(JSON.stringify(captured)).byteLength > 16 * 1024)
    return invalidLaunchFacts();
  // Missing facts must not become readable through later prototype writes.
  return Object.freeze(captured);
}

/** @internal Frozen at preload/import; the application has no setter. */
export const hostLaunchFacts = captureHostLaunchFacts(
  typeof __KELD_HOST_LAUNCH_FACTS__ === "undefined"
    ? undefined
    : __KELD_HOST_LAUNCH_FACTS__,
);

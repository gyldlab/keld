/** Real Bun ordering probes; native installed admission is a separate Host gate. */
import { expect, test } from "bun:test";
import { join } from "node:path";
import { installMainProcessFacts } from "./process-facts.ts";

const entry = join(
  import.meta.dir,
  "../fixtures/installed_launch_early_entry.ts",
);
const preload = join(import.meta.dir, "installed-launch-preload.ts");
const sidecar = join(
  import.meta.dir,
  "../fixtures/installed_launch_sidecar_probe.ts",
);
const record = {
  schema: 6,
  kind: "local-development",
  name: "Early Facts",
  root: "/Applications/Early Facts.app/Contents/Resources/app",
  version: "1.2.3",
  resourcesRoot: "/Applications/Early Facts.app/Contents/Resources",
};

function observe(
  value: unknown,
  withPreload = true,
  sidecarOrder: "none" | "first" | "last" = "none",
) {
  return Bun.spawnSync({
    cmd: [
      process.execPath,
      `--define=__KELD_HOST_LAUNCH_FACTS__:${value === undefined ? "undefined" : JSON.stringify(value)}`,
      '--define=process.env.KELD_NATIVE_WINDOW_FACADE:"0"',
      ...(sidecarOrder === "first" ? [`--preload=${sidecar}`] : []),
      ...(withPreload ? [`--preload=${preload}`] : []),
      ...(sidecarOrder === "last" ? [`--preload=${sidecar}`] : []),
      "run",
      entry,
    ],
    env: {
      ...process.env,
      BUN_OPTIONS: "",
      KELD_APP_LINK: `1#${"ab".repeat(32)}`,
      KELD_RESOURCES_PATH: "/decoy",
      KELD_IS_PACKAGED: "true",
    },
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4_000,
  });
}

function result(child: ReturnType<typeof observe>) {
  expect(child.exitCode).toBe(0);
  expect(child.stderr.toString()).toBe("");
  const line = child.stdout
    .toString()
    .split("\n")
    .find((value) => value.startsWith("GH1404_EARLY_FACTS "));
  expect(line).toBeDefined();
  return JSON.parse(line!.slice("GH1404_EARLY_FACTS ".length));
}

test("installed preload supplies immutable process facts before an entry dependency and preserves Bun argv", () => {
  const observed = result(observe(record));
  expect(observed).toEqual({
    early: {
      type: "browser",
      defaultApp: true,
      resourcesPath: record.resourcesRoot,
      execPath: process.execPath,
      argv: [process.execPath, entry],
      defaultDescriptor: {
        value: true,
        writable: false,
        enumerable: true,
        configurable: false,
      },
      resourcesDescriptor: {
        value: record.resourcesRoot,
        writable: false,
        enumerable: true,
        configurable: false,
      },
    },
    isPackaged: true,
    appPath: record.root,
    resourcesPath: record.resourcesRoot,
    sameArgvObject: true,
    userArguments: [],
  });
});

test("without the installed preload the early dependency misses facts even though facade import later supplies them", () => {
  const observed = result(observe(record, false));
  expect(observed.early.type).toBeNull();
  expect(observed.early.defaultApp).toBeNull();
  expect(observed.early.resourcesPath).toBeNull();
  expect(observed.isPackaged).toBe(true);
  expect(observed.resourcesPath).toBe(record.resourcesRoot);
});

test("installed Host two-preload order supplies facts before an application-side transport dependency", () => {
  const observed = result(observe(record, true, "last"));
  expect(observed.early.defaultApp).toBe(true);
  expect(observed.early.resourcesPath).toBe(record.resourcesRoot);
});

test("the old sidecar-first order is a negative control that misses early facts", () => {
  const observed = result(observe(record, true, "first"));
  expect(observed.early.defaultApp).toBeNull();
  expect(observed.early.resourcesPath).toBeNull();
  expect(observed.isPackaged).toBe(true);
  expect(observed.resourcesPath).toBe(record.resourcesRoot);
});

test("missing forged mixed and unsupported installed facts refuse before the first entry dependency", () => {
  for (const rejected of [
    undefined,
    { ...record, kind: "development" },
    { ...record, schema: 4 },
    { ...record, schema: 5 },
    { ...record, schema: 7 },
    { ...record, resourcesRoot: "/foreign" },
    { ...record, root: "/foreign/app" },
    { ...record, grant: "all" },
    { schema: 3, kind: "development", name: "Dev", root: "/stage", paths: {} },
  ]) {
    const child = observe(rejected);
    expect(child.exitCode).not.toBe(0);
    expect(child.stderr.toString()).toContain("KELD-IPC-029:");
    expect(child.stdout.toString()).not.toContain("GH1404_DEPENDENCY_REACHED");
  }
});

test("a conflicting resourcesPath prevents every process-property write", () => {
  const target = { versions: {} };
  Object.defineProperty(target, "resourcesPath", {
    value: "/foreign",
    enumerable: true,
    configurable: true,
  });
  expect(() =>
    installMainProcessFacts(target, { resourcesPath: record.resourcesRoot }),
  ).toThrow("KELD-COMPAT-014");
  expect(Object.hasOwn(target, "type")).toBe(false);
  expect(Object.hasOwn(target, "defaultApp")).toBe(false);
  expect(Object.hasOwn(target.versions, "electron")).toBe(false);
  expect(Reflect.get(target, "resourcesPath")).toBe("/foreign");
});

/** Owned KEL72 fixture preparation/launch only; this does not admit application graphs. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  realpathSync,
  writeFileSync,
} from "node:fs";
import { createRequire, isBuiltin } from "node:module";
import { delimiter, dirname, join, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import { checkInstalledData } from "./installed-alias-data.ts";
import { packTypeInputs } from "./installed-alias-types.ts";

const scripts = ["shim_env.ts", "lifecycle.ts", "app_ready.ts"] as const;
const runtimePackages = ["electron", "@keld/api", "@keld/kipc"] as const;
const MAX_PACKED_ARCHIVE_BYTES = 8 * 1024 * 1024;
const MAX_RUNTIME_PACKAGE_FILES = 512;
const MAX_RUNTIME_PACKAGE_BYTES = 64 * 1024 * 1024;
const source = resolve(import.meta.dir, "../../../..");

type PackageFile = { size: number; sha256: string };
type PackageFileInventory = Record<string, PackageFile>;
function json(path: string) {
  return JSON.parse(readFileSync(path, "utf8"));
}
function writeJson(path: string, data: unknown): void {
  writeFileSync(path, JSON.stringify(data, null, 2) + "\n");
}
function sha256(path: string): string {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function rejectCompetingConfig(directory: string): void {
  const tsconfig = join(directory, "tsconfig.json");
  if (existsSync(tsconfig)) {
    const paths = json(tsconfig).compilerOptions?.paths ?? {};
    assert(
      !Object.keys(paths).some(
        (key) => key === "electron" || key.startsWith("electron/"),
      ),
      "KEL72_COMPETING_TSCONFIG",
    );
  }
  // These owned fixtures need no Bun configuration. The old file contained an
  // inert alias; reject its presence instead of inventing a TOML admission parser.
  assert(!existsSync(join(directory, "bunfig.toml")), "KEL72_COMPETING_BUNFIG");
}

interface Custody {
  versions: Record<string, string>;
  nodeTypesVersion: string;
  dependencies: Record<string, string>;
  lockSha256: string;
  installedPackages: { name: string; artifact: string; integrity: string }[];
  /** Independently read from verified packed first-party tarballs at prepare time. */
  packedRuntimeFiles: Record<string, PackageFileInventory>;
  scriptSha256: Record<string, string>;
}

/** Recheck the actual packed archive against its original, lock-bound SHA-512. */
function packedArchiveBytes(
  selected: Custody["installedPackages"][number],
): Uint8Array {
  const metadata = lstatSync(selected.artifact);
  assert(
    metadata.isFile() &&
      metadata.size > 0 &&
      metadata.size <= MAX_PACKED_ARCHIVE_BYTES,
    "KEL72_PACKED_ARCHIVE_LIMIT: " + selected.name,
  );
  const bytes = readFileSync(selected.artifact);
  assert.equal(
    "sha512-" + createHash("sha512").update(bytes).digest("base64"),
    selected.integrity,
    "KEL72_PACKED_ARTIFACT_CHANGED: " + selected.name,
  );
  return bytes;
}

/** Build expected bytes from only the canonical first-party packed artifacts. */
async function packedRuntimeFiles(
  installed: Custody["installedPackages"],
): Promise<Custody["packedRuntimeFiles"]> {
  const result: Custody["packedRuntimeFiles"] = Object.create(null);
  for (const name of runtimePackages) {
    const selected = installed.find((item) => item.name === name);
    assert(selected, "KEL72_MISSING_PACKED_ARTIFACT: " + name);
    const bytes = packedArchiveBytes(selected);
    // Bun 1.4.4 Archive.files() returns regular files from tar.gz in memory.
    // It omits symlink metadata; the *installed* directory walker below must
    // independently reject symlinks and every unexpected installed file.
    const files = await new Bun.Archive(new Uint8Array(bytes)).files();
    assert(
      files.size > 0 && files.size <= MAX_RUNTIME_PACKAGE_FILES,
      "KEL72_PACKED_FILE_LIMIT: " + name,
    );
    const expected: PackageFileInventory = Object.create(null);
    let total = 0;
    for (const [archivePath, file] of files) {
      const relative = archivePath.startsWith("package/")
        ? archivePath.slice("package/".length)
        : "";
      assert(
        relative.length > 0 &&
          !relative.includes("\\") &&
          relative
            .split("/")
            .every((part) => part !== "" && part !== "." && part !== "..") &&
          !Object.hasOwn(expected, relative),
        "KEL72_PACKED_FILE_NAME: " + archivePath,
      );
      total += file.size;
      assert(
        total <= MAX_RUNTIME_PACKAGE_BYTES,
        "KEL72_PACKED_BYTE_LIMIT: " + name,
      );
      const content = new Uint8Array(await file.arrayBuffer());
      assert.equal(
        content.byteLength,
        file.size,
        "KEL72_PACKED_FILE_TRUNCATED",
      );
      expected[relative] = {
        size: file.size,
        sha256: createHash("sha256").update(content).digest("hex"),
      };
    }
    assert(Object.hasOwn(expected, "package.json"), "KEL72_PACKED_NO_MANIFEST");
    result[name] = expected;
  }
  return result;
}

/** Check package path membership and bytes without evaluating installed code. */
function installedRuntimeFiles(
  consumer: string,
  name: (typeof runtimePackages)[number],
): PackageFileInventory {
  const modules = join(consumer, "node_modules");
  const packageRoot = join(modules, name);
  const parents = name.startsWith("@")
    ? [modules, join(modules, "@keld")]
    : [modules];
  for (const directory of [...parents, packageRoot]) {
    const meta = lstatSync(directory);
    assert(!meta.isSymbolicLink(), "KEL72_INSTALLED_SYMLINK: " + directory);
    assert(meta.isDirectory(), "KEL72_INSTALLED_NOT_DIRECTORY: " + directory);
  }
  const observed: PackageFileInventory = Object.create(null);
  let count = 0;
  let total = 0;
  function visit(directory: string, prefix: string): void {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const nameInPackage = prefix ? prefix + "/" + entry.name : entry.name;
      const path = join(directory, entry.name);
      assert(
        !entry.isSymbolicLink(),
        "KEL72_INSTALLED_SYMLINK: " + nameInPackage,
      );
      if (entry.isDirectory()) {
        visit(path, nameInPackage);
        continue;
      }
      assert(entry.isFile(), "KEL72_INSTALLED_FILE_TYPE: " + nameInPackage);
      count += 1;
      assert(count <= MAX_RUNTIME_PACKAGE_FILES, "KEL72_INSTALLED_FILE_LIMIT");
      const meta = lstatSync(path);
      assert(
        meta.isFile() && !meta.isSymbolicLink(),
        "KEL72_INSTALLED_FILE_TYPE",
      );
      total += meta.size;
      assert(total <= MAX_RUNTIME_PACKAGE_BYTES, "KEL72_INSTALLED_BYTE_LIMIT");
      const content = readFileSync(path);
      observed[nameInPackage] = {
        size: content.byteLength,
        sha256: createHash("sha256").update(content).digest("hex"),
      };
    }
  }
  visit(packageRoot, "");
  return observed;
}

function checkInstalledRuntimeBytes(
  consumer: string,
  expected: Custody["packedRuntimeFiles"],
): void {
  assert.deepEqual(
    Object.keys(expected).sort(),
    [...runtimePackages].sort(),
    "KEL72_PACKED_PACKAGE_SET_CHANGED",
  );
  for (const name of runtimePackages) {
    const packed = expected[name];
    assert(
      packed && Object.hasOwn(packed, "package.json"),
      "KEL72_PACKED_FILES_MISSING",
    );
    const installed = installedRuntimeFiles(consumer, name);
    assert.deepEqual(
      Object.keys(installed).sort(),
      Object.keys(packed).sort(),
      "KEL72_INSTALLED_FILE_SET_CHANGED: " + name,
    );
    for (const [path, file] of Object.entries(packed)) {
      assert.deepEqual(
        installed[path],
        file,
        "KEL72_INSTALLED_BYTE_MISMATCH: " + name + "/" + path,
      );
    }
  }
}

/** Validate only the fixed, locally produced fixture graph before its entry runs. */
export function admitInstalledLifecycle(consumer: string): void {
  consumer = realpathSync(consumer);
  rejectCompetingConfig(consumer);
  const root = dirname(consumer);
  const custody = json(join(root, "fixture-custody.json")) as Custody;
  const manifest = json(join(consumer, "package.json"));
  assert.deepEqual(
    manifest.dependencies,
    custody.dependencies,
    "KEL72_ALIAS_DEPENDENCIES_CHANGED",
  );
  assert.equal(
    sha256(join(consumer, "package-lock.json")),
    custody.lockSha256,
    "KEL72_INSTALLED_LOCK_CHANGED",
  );
  const lock = json(join(consumer, "package-lock.json"));
  for (const item of custody.installedPackages) {
    const installed = lock.packages["node_modules/" + item.name];
    assert(
      installed && installed.integrity === item.integrity,
      "KEL72_ARTIFACT_LOCK_IDENTITY: " + item.name,
    );
    assert.equal(
      realpathSync(resolve(consumer, installed.resolved.replace(/^file:/, ""))),
      item.artifact,
    );
  }
  for (const name of ["electron", "@keld/api", "@keld/kipc"])
    assert(
      existsSync(join(consumer, "node_modules", name, "package.json")),
      "KEL72_MISSING_DEPENDENCY: " + name,
    );
  for (const name of runtimePackages) {
    const selected = custody.installedPackages.find(
      (item) => item.name === name,
    );
    assert(selected, "KEL72_MISSING_PACKED_ARTIFACT: " + name);
    packedArchiveBytes(selected);
  }
  checkInstalledRuntimeBytes(consumer, custody.packedRuntimeFiles);
  checkInstalledData(
    consumer,
    custody.versions,
    custody.nodeTypesVersion,
    root,
    [],
  );
  const scanner = new Bun.Transpiler({ loader: "js" });
  for (const [name, expected] of Object.entries(custody.scriptSha256)) {
    const path = join(consumer, name);
    assert.equal(sha256(path), expected, "KEL72_OWNED_SCRIPT_CHANGED: " + name);
    for (const edge of scanner.scanImports(readFileSync(path, "utf8"))) {
      if (isBuiltin(edge.path)) continue;
      assert(
        edge.path === "electron" || edge.path === "@keld/api/internal/link",
        "KEL72_UNDECLARED_FIXTURE_EDGE: " + edge.path,
      );
      const resolved = realpathSync(createRequire(path).resolve(edge.path));
      const packageDirectory =
        edge.path === "electron" ? "electron" : "@keld/api";
      assert(
        resolved.startsWith(
          join(consumer, "node_modules", packageDirectory) + sep,
        ),
      );
    }
  }
}

export async function prepareInstalledLifecycle(root: string): Promise<string> {
  root = realpathSync(root);
  rejectCompetingConfig(import.meta.dir);
  const artifacts = join(root, "artifacts");
  const consumer = join(root, "consumer");
  mkdirSync(artifacts);
  mkdirSync(consumer);
  const env = {
    ...process.env,
    PATH: [dirname(process.execPath), process.env.PATH ?? ""].join(delimiter),
  };
  const commands: unknown[] = [];
  function run(command: string[], cwd: string, label: string) {
    const result = Bun.spawnSync(command, {
      cwd,
      env,
      stdout: "pipe",
      stderr: "pipe",
      timeout: 120_000,
    });
    const stdout = new TextDecoder().decode(result.stdout);
    const stderr = new TextDecoder().decode(result.stderr);
    const stem = String(commands.length + 1).padStart(3, "0") + "-" + label;
    writeFileSync(join(root, stem + ".stdout"), stdout);
    writeFileSync(join(root, stem + ".stderr"), stderr);
    commands.push({
      command,
      cwd,
      exit: result.exitCode,
      stdout: stem + ".stdout",
      stderr: stem + ".stderr",
    });
    writeJson(join(root, "commands.json"), commands);
    assert.equal(result.exitCode, 0, label + "\n" + stdout + stderr);
    return { stdout, stderr, exitCode: result.exitCode };
  }
  const versions = Object.fromEntries(
    ["electron", "api", "kipc"].map((name) => [
      name,
      json(join(source, "packages/@keld", name, "package.json")).version,
    ]),
  ) as Record<string, string>;
  run(
    [
      process.execPath,
      join(source, "packages/@keld/kipc/scripts/package-artifacts.ts"),
      artifacts,
      "--electron",
    ],
    source,
    "pack-sdk",
  );
  const typeInputs = packTypeInputs(source, artifacts, run);
  const packed = Object.fromEntries(
    ["electron", "api", "kipc"].map((name) => [
      name,
      realpathSync(join(artifacts, `keld-${name}-${versions[name]}.tgz`)),
    ]),
  ) as Record<string, string>;
  const dependencies = {
    electron: "file:" + packed.electron,
    "@keld/api": "file:" + packed.api,
  };
  writeJson(join(consumer, "package.json"), {
    name: "keld-installed-lifecycle-fixture",
    private: true,
    type: "module",
    dependencies,
    overrides: {
      "@keld/api": "file:" + packed.api,
      "@keld/kipc": "file:" + packed.kipc,
      ...Object.fromEntries(
        Object.entries(typeInputs).map(([name, value]) => [
          name,
          "file:" + value.path,
        ]),
      ),
    },
  });
  const npm = Bun.which("npm");
  assert(npm, "npm is required for the owned offline fixture install");
  run(
    [
      npm,
      "install",
      "--offline",
      "--ignore-scripts",
      "--no-audit",
      "--no-fund",
      "--cache",
      join(root, "cache"),
    ],
    consumer,
    "offline-install",
  );
  const scriptSha256: Record<string, string> = {};
  const cwd = process.cwd();
  try {
    process.chdir(import.meta.dir);
    for (const script of scripts) {
      const built = await Bun.build({
        entrypoints: [join(import.meta.dir, script)],
        target: "bun",
        format: "esm",
        packages: "external",
      });
      assert.equal(
        built.outputs.length,
        1,
        "One owned lifecycle fixture entry is required",
      );
      const name = script.replace(/\.ts$/, ".mjs");
      writeFileSync(join(consumer, name), await built.outputs[0].text());
      scriptSha256[name] = sha256(join(consumer, name));
    }
  } finally {
    process.chdir(cwd);
  }
  const installedPackages = Object.entries({
    electron: packed.electron,
    "@keld/api": packed.api,
    "@keld/kipc": packed.kipc,
    ...Object.fromEntries(
      Object.entries(typeInputs).map(([name, value]) => [name, value.path]),
    ),
  }).map(([name, artifact]) => ({
    name,
    artifact,
    integrity:
      "sha512-" +
      createHash("sha512").update(readFileSync(artifact)).digest("base64"),
  }));
  const custody: Custody = {
    versions,
    nodeTypesVersion: typeInputs["@types/node"].version,
    dependencies,
    lockSha256: sha256(join(consumer, "package-lock.json")),
    installedPackages,
    packedRuntimeFiles: await packedRuntimeFiles(installedPackages),
    scriptSha256,
  };
  writeJson(join(root, "fixture-custody.json"), custody);
  admitInstalledLifecycle(consumer);
  return consumer;
}

if (import.meta.main) {
  const [mode, directory, script, ...extra] = process.argv.slice(2);
  assert(
    directory && extra.length === 0,
    "Usage: installed-lifecycle.ts prepare <owned-root> | run <consumer> <script.ts>",
  );
  if (mode === "prepare") {
    assert(script === undefined);
    console.log(
      JSON.stringify({ consumer: await prepareInstalledLifecycle(directory) }),
    );
  } else {
    assert(
      mode === "run" && scripts.includes(script as (typeof scripts)[number]),
      "Unknown owned lifecycle fixture",
    );
    const consumer = realpathSync(directory);
    admitInstalledLifecycle(consumer);
    await import(
      pathToFileURL(join(consumer, script.replace(/\.ts$/, ".mjs"))).href
    );
  }
}

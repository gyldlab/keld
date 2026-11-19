import assert from "node:assert/strict";
import {
  copyFileSync,
  existsSync,
  readFileSync,
  realpathSync,
  renameSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { createRequire } from "node:module";
import { dirname, isAbsolute, join, sep } from "node:path";

type Run = (
  command: string[],
  cwd: string,
  label: string,
  success?: boolean,
) => { stdout: string; stderr: string; exitCode: number };

/** Pack the actual locked declaration inputs; never synthesize a type package. */
export function packTypeInputs(source: string, destination: string, run: Run) {
  const nodeManifest = Bun.resolveSync(
    "@types/node/package.json",
    join(source, "packages/@keld/electron"),
  );
  const undiciManifest = createRequire(nodeManifest).resolve(
    "undici-types/package.json",
  );
  return Object.fromEntries(
    [nodeManifest, undiciManifest].map((path) => {
      const manifest = JSON.parse(readFileSync(path, "utf8"));
      assert(
        !manifest.bin &&
          !manifest.main &&
          !Object.keys(manifest.scripts ?? {}).length,
        "Type-only fixture inputs must have no executable entry or lifecycle script",
      );
      run(
        [
          process.execPath,
          "pm",
          "pack",
          "--ignore-scripts",
          "--destination",
          destination,
        ],
        dirname(path),
        "pack-" + manifest.name.replaceAll("/", "-"),
      );
      const artifact = join(
        destination,
        manifest.name.replace(/^@/, "").replaceAll("/", "-") +
          "-" +
          manifest.version +
          ".tgz",
      );
      assert(existsSync(artifact));
      return [
        manifest.name,
        { version: manifest.version as string, path: artifact },
      ];
    }),
  ) as Record<string, { version: string; path: string }>;
}

/** Strict consumer with no ambient auto-inclusion, Bun types or skipped library checks. */
export function prepareTypeConsumer(consumer: string): void {
  copyFileSync(
    join(import.meta.dir, "installed-alias/consumer.mts"),
    join(consumer, "consumer.mts"),
  );
  writeFileSync(
    join(consumer, "tsconfig.json"),
    JSON.stringify({
      compilerOptions: {
        strict: true,
        noEmit: true,
        target: "ES2024",
        lib: ["ES2024", "DOM"],
        module: "NodeNext",
        moduleResolution: "NodeNext",
        types: [],
        skipLibCheck: false,
      },
      files: ["consumer.mts"],
    }),
  );
  assert(!existsSync(join(consumer, "node_modules/@types/bun")));
  assert(!existsSync(join(consumer, "node_modules/bun-types")));
}

/**
 * Admit only physical descendants of the installed consumer or the pinned
 * TypeScript compiler library. On Windows an 8.3 root and its long spelling
 * can refer to the same directory, so comparing pathname prefixes is unsound.
 */
export function assertInstalledTypeFilesWithinRoots(
  files: readonly string[],
  consumer: string,
  compilerLib: string,
): void {
  const roots = [consumer, compilerLib].map((root) => {
    const directory = statSync(realpathSync(root), { bigint: true });
    assert(
      directory.isDirectory(),
      "Trusted installed type root must be a directory",
    );
    assert(directory.ino !== 0n, "Trusted root inode unavailable");
    return { dev: directory.dev, ino: directory.ino };
  });
  for (const file of files) {
    // Resolve junctions/symlinks before walking the actual physical ancestors.
    const physical = realpathSync(file);
    let parent = dirname(physical);
    let contained = false;
    while (true) {
      const directory = statSync(parent, { bigint: true });
      if (
        roots.some(
          (root) => root.dev === directory.dev && root.ino === directory.ino,
        )
      ) {
        contained = true;
        break;
      }
      const next = dirname(parent);
      if (next === parent) break;
      parent = next;
    }
    assert(contained, "Compiler escaped the installed graph: " + physical);
    const slashPath = physical.split(sep).join("/");
    assert(
      !slashPath.includes("/bun-types/") && !slashPath.includes("/@types/bun/"),
      "Compiler loaded Bun declarations: " + physical,
    );
  }
}

export function checkInstalledDeclarations(
  source: string,
  consumer: string,
  scratch: string,
  run: Run,
) {
  prepareTypeConsumer(consumer);
  const electron = join(source, "packages/@keld/electron");
  const command = [
    join(electron, "node_modules/.bin/tsc"),
    "-p",
    join(consumer, "tsconfig.json"),
    "--listFiles",
  ];
  const compiled = run(command, consumer, "strict-installed-nodenext");
  // The pinned TypeScript 7 CLI delegates to its declared native platform package;
  // only that compiler's built-in lib directory may live outside this consumer.
  const compilerManifestPath = realpathSync(
    Bun.resolveSync("typescript/package.json", electron),
  );
  const compilerManifest = JSON.parse(
    readFileSync(compilerManifestPath, "utf8"),
  );
  const platformPackage = `@typescript/typescript-${process.platform}-${process.arch}`;
  assert.equal(
    compilerManifest.optionalDependencies[platformPackage],
    compilerManifest.version,
  );
  const platformManifestPath = realpathSync(
    createRequire(compilerManifestPath).resolve(
      platformPackage + "/package.json",
    ),
  );
  const platformManifest = JSON.parse(
    readFileSync(platformManifestPath, "utf8"),
  );
  assert.equal(platformManifest.version, compilerManifest.version);
  const compilerLib = join(dirname(platformManifestPath), "lib");
  const files = compiled.stdout
    .split(/\r?\n/)
    .filter((line) => isAbsolute(line));
  assert(
    files.some((file) => file.includes("/node_modules/@types/node/")),
    "Consumer must use its installed Node declarations",
  );
  assertInstalledTypeFilesWithinRoots(files, consumer, compilerLib);
  const installed = join(consumer, "node_modules/@types/node");
  const removed = join(scratch, "removed-node-types");
  renameSync(installed, removed);
  try {
    const rejected = run(
      command,
      consumer,
      "missing-installed-node-types",
      false,
    );
    assert.match(rejected.stdout + rejected.stderr, /TS2688/);
  } finally {
    renameSync(removed, installed);
  }
  run(command, consumer, "restored-installed-node-types");
  const entry = join(consumer, "node_modules/electron/dist/index.d.ts");
  const original = readFileSync(entry, "utf8");
  const reference = '/// <reference types="node" />\n';
  assert(
    original.startsWith(reference),
    "Packed public types must declare their Node type dependency",
  );
  try {
    writeFileSync(entry, original.slice(reference.length));
    const rejected = run(
      command,
      consumer,
      "missing-emitted-node-reference",
      false,
    );
    assert.match(rejected.stdout + rejected.stderr, /node:events/);
  } finally {
    writeFileSync(entry, original);
  }
  run(command, consumer, "restored-emitted-node-reference");
  // Test this exported declaration independently so its reference cannot mask
  // the public entry's Node dependency negative (or vice versa).
  const configPath = join(consumer, "tsconfig.json");
  const publicConfig = readFileSync(configPath, "utf8");
  const privateEntry = join(
    consumer,
    "node_modules/electron/dist/original-ipc.d.ts",
  );
  const privateOriginal = readFileSync(privateEntry, "utf8");
  copyFileSync(
    join(import.meta.dir, "installed-alias/original-ipc-consumer.mts"),
    join(consumer, "original-ipc-consumer.mts"),
  );
  try {
    const config = JSON.parse(publicConfig);
    config.files = ["original-ipc-consumer.mts"];
    writeFileSync(configPath, JSON.stringify(config));
    assert(
      privateOriginal.startsWith(reference),
      "Packed internal helper types must declare Node dependency",
    );
    run(command, consumer, "strict-installed-original-ipc");
    try {
      writeFileSync(privateEntry, privateOriginal.slice(reference.length));
      const rejected = run(
        command,
        consumer,
        "missing-internal-emitted-node-reference",
        false,
      );
      assert.match(rejected.stdout + rejected.stderr, /node:events/);
    } finally {
      writeFileSync(privateEntry, privateOriginal);
    }
    run(command, consumer, "restored-internal-emitted-node-reference");
  } finally {
    writeFileSync(configPath, publicConfig);
  }
  return {
    compiler: command[0],
    compilerVersion: compilerManifest.version,
    compilerPlatformPackage: platformPackage,
    files,
    strict: true,
    automaticAmbientTypes: [],
    bunTypes: false,
    skipLibCheck: false,
  };
}

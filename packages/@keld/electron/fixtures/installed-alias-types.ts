import assert from "node:assert/strict";
import {
  copyFileSync,
  existsSync,
  readFileSync,
  realpathSync,
  renameSync,
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
  for (const file of files) {
    const path = realpathSync(file);
    assert(
      path.startsWith(consumer + sep) || path.startsWith(compilerLib + sep),
      "Compiler escaped the installed graph: " + path,
    );
    assert(!path.includes("/bun-types/") && !path.includes("/@types/bun/"));
  }
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

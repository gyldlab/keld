/**
 * Installed package regression. The source baseline and artifact provider have
 * separate owners; this scenario exercises their actual composed package output.
 * This is not application graph admission and never consumes foreign Electron.
 *
 * bun <this-file> <owned-source-root> <receipt-directory> <node26> <npm> <duplicate-code>
 */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  appendFileSync,
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";
import {
  checkInstalledDeclarations,
  packTypeInputs,
  prepareTypeConsumer,
} from "./installed-alias-types.ts";
import { checkInstalledData } from "./installed-alias-data.ts";

const [sourceArg, receiptsArg, nodeArg, npmArg, duplicateCode] =
  process.argv.slice(2);
assert(
  sourceArg && receiptsArg && nodeArg && npmArg && duplicateCode,
  "Provide source root, receipt directory, exact Node26/npm paths and reserved duplicate code",
);
assert.match(duplicateCode, /^KELD-COMPAT-\d{3}$/);
assert.equal(Bun.version, "1.4.4");
const source = realpathSync(sourceArg);
const receipts = resolve(receiptsArg);
mkdirSync(receipts, { recursive: true });
const scratch = realpathSync(
  mkdtempSync(join(tmpdir(), "keld-gh502-installed-")),
);
let passed = false;
try {
  const bun = process.execPath;
  const node = realpathSync(nodeArg);
  const npm = realpathSync(npmArg);
  const env = {
    ...process.env,
    PATH: [
      dirname(bun),
      dirname(node),
      dirname(npm),
      process.env.PATH ?? "",
    ].join(delimiter),
  };
  const specifiers = [
    "electron",
    "electron/main",
    "electron/renderer",
    "electron/common",
    "electron/utility",
  ];
  // Independent current binding oracle. Add ipcMain only with its separately reviewed integration.
  const members = [
    "app",
    "BrowserWindow",
    "ClipboardItem",
    "Menu",
    "clipboard",
    "dialog",
    "nativeImage",
    "nativeTheme",
    "screen",
    "session",
    "shell",
  ].sort();
  const runtimeReceipts: unknown[] = [];
  const commands: unknown[] = [];
  let sequence = 0;

  function json(path: string): any {
    return JSON.parse(readFileSync(path, "utf8"));
  }
  function writeJson(path: string, value: unknown): void {
    writeFileSync(path, JSON.stringify(value, null, 2) + "\n");
  }
  function hash(path: string): string {
    return createHash("sha256").update(readFileSync(path)).digest("hex");
  }
  function run(command: string[], cwd: string, label: string, success = true) {
    const result = Bun.spawnSync(command, {
      cwd,
      env,
      stdout: "pipe",
      stderr: "pipe",
      timeout: 120_000,
    });
    const stdout = new TextDecoder().decode(result.stdout);
    const stderr = new TextDecoder().decode(result.stderr);
    const stem = String(++sequence).padStart(3, "0") + "-" + label;
    writeFileSync(join(receipts, stem + ".stdout"), stdout);
    writeFileSync(join(receipts, stem + ".stderr"), stderr);
    commands.push({
      command,
      cwd,
      exit: result.exitCode,
      stdout: stem + ".stdout",
      stderr: stem + ".stderr",
    });
    writeJson(join(receipts, "commands.json"), commands);
    if (success)
      assert.equal(result.exitCode, 0, label + "\n" + stderr + stdout);
    else assert.notEqual(result.exitCode, 0, label + " unexpectedly succeeded");
    return { ...result, stdout, stderr };
  }

  function matrixSource(): string {
    const statements = specifiers.flatMap((specifier, index) => [
      `import def${index}, { app as named${index} } from ${JSON.stringify(specifier)};`,
      `import * as ns${index} from ${JSON.stringify(specifier)};`,
    ]);
    statements.push(
      'import { createRequire } from "node:module";',
      "const require = createRequire(import.meta.url);",
    );
    statements.push(
      "const values = [" +
        specifiers
          .map(
            (specifier, index) =>
              `[${JSON.stringify(specifier)}, def${index}, named${index}, ns${index}, require(${JSON.stringify(specifier)}), import.meta.resolve(${JSON.stringify(specifier)}), require.resolve(${JSON.stringify(specifier)})]`,
          )
          .join(",") +
        "];",
      "export { values };",
    );
    return statements.join("\n") + "\n";
  }

  const nodeVersion = run(
    [node, "--version"],
    scratch,
    "node-version",
  ).stdout.trim();
  assert.match(nodeVersion, /^v26\./);
  const versions = Object.fromEntries(
    ["electron", "api", "kipc"].map((name) => [
      name,
      json(join(source, "packages/@keld", name, "package.json")).version,
    ]),
  ) as Record<string, string>;
  const artifactDir = join(scratch, "artifacts");
  const secondDir = join(scratch, "second-artifacts");
  mkdirSync(artifactDir);
  mkdirSync(secondDir);
  const producer = join(
    source,
    "packages/@keld/kipc/scripts/package-artifacts.ts",
  );
  assert(
    existsSync(producer),
    "Compose the existing reviewed SDK provider before this harness",
  );
  run([bun, producer, artifactDir, "--electron"], source, "pack-from-root");
  run(
    [bun, producer, secondDir, "--electron"],
    join(source, "packages/@keld/api"),
    "pack-from-api",
  );
  const artifacts = Object.fromEntries(
    ["electron", "api", "kipc"].map((name) => {
      const basename = `keld-${name}-${versions[name]}.tgz`;
      const path = join(artifactDir, basename);
      assert(
        existsSync(path),
        "Production producer did not emit required " + basename,
      );
      assert.deepEqual(
        readFileSync(path),
        readFileSync(join(secondDir, basename)),
        "Cross-CWD artifact bytes: " + name,
      );
      return [name, { path, sha256: hash(path) }];
    }),
  ) as Record<string, { path: string; sha256: string }>;
  writeJson(join(receipts, "artifact-custody.json"), {
    source,
    scratch,
    producer,
    versions,
    artifacts,
    bun,
    bunVersion: Bun.version,
    bunRevision: Bun.revision,
    node,
    nodeVersion,
    platform: process.platform,
    arch: process.arch,
  });

  const dependency = join(scratch, "probe-dependency");
  mkdirSync(dependency);
  writeJson(join(dependency, "package.json"), {
    name: "@keld/gh502-probe-dependency",
    version: "0.0.0",
    type: "module",
    exports: "./index.js",
    files: ["index.js"],
    dependencies: { electron: "*" },
  });
  writeFileSync(join(dependency, "index.js"), matrixSource());
  run(
    [bun, "pm", "pack", "--ignore-scripts", "--destination", artifactDir],
    dependency,
    "pack-nested-caller",
  );
  const dependencyArtifact = join(
    artifactDir,
    "keld-gh502-probe-dependency-0.0.0.tgz",
  );
  assert(existsSync(dependencyArtifact));

  const typeInputs = packTypeInputs(source, artifactDir, run);

  function install(consumer: string, electronArtifact: string): void {
    mkdirSync(consumer);
    writeJson(join(consumer, "package.json"), {
      name: "gh502-installed-alias-regression",
      private: true,
      type: "module",
      dependencies: {
        electron: "file:" + electronArtifact,
        "@keld/gh502-probe-dependency": "file:" + dependencyArtifact,
      },
      overrides: {
        "@keld/api": "file:" + artifacts.api.path,
        "@keld/kipc": "file:" + artifacts.kipc.path,
        ...Object.fromEntries(
          Object.entries(typeInputs).map(([name, artifact]) => [
            name,
            "file:" + artifact.path,
          ]),
        ),
      },
    });
    run(
      [
        npm,
        "install",
        "--offline",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
        "--cache",
        join(consumer, "cache"),
      ],
      consumer,
      "offline-install",
    );
  }
  const consumer = join(scratch, "consumer");
  install(consumer, artifacts.electron.path);
  checkInstalledData(
    consumer,
    versions,
    typeInputs["@types/node"].version,
    receipts,
  );
  const declarations = checkInstalledDeclarations(
    source,
    consumer,
    scratch,
    run,
  );
  writeFileSync(join(consumer, "root-matrix.mjs"), matrixSource());
  copyFileSync(
    join(import.meta.dir, "installed-alias/matrix.mjs"),
    join(consumer, "matrix.mjs"),
  );
  const fresh = join(consumer, "fresh-require.cjs");
  writeFileSync(
    fresh,
    'const assert = require("node:assert/strict"); const facade = require("electron"); assert.equal(typeof facade.app, "object"); assert.equal(typeof facade.BrowserWindow, "function"); console.log("SYNC_FACADE");\n',
  );
  for (const [name, runtime] of [
    ["bun", bun],
    ["node", node],
  ]) {
    run([runtime, fresh], consumer, name + "-fresh-require");
    const matrix = run(
      [runtime, join(consumer, "matrix.mjs")],
      consumer,
      name + "-installed-matrix",
    );
    const result = JSON.parse(matrix.stdout);
    assert.equal(result.observations, 40);
    runtimeReceipts.push(result);
  }

  const entry = join(consumer, "node_modules/electron/dist/index.js");
  const originalEntry = readFileSync(entry);
  try {
    appendFileSync(entry, '\nimport "../../@keld/api/app.js";\n');
    assert.throws(
      () =>
        checkInstalledData(
          consumer,
          versions,
          typeInputs["@types/node"].version,
          receipts,
        ),
      /Relative import left its package root/,
      "A sibling package escape must fail even when its target is inside the consumer",
    );
  } finally {
    writeFileSync(entry, originalEntry);
  }
  const duplicatePackage = join(consumer, "node_modules/electron-duplicate");
  cpSync(join(consumer, "node_modules/electron"), duplicatePackage, {
    recursive: true,
  });
  const secondEntry = join(duplicatePackage, "dist/index.js");
  copyFileSync(
    join(import.meta.dir, "installed-alias/duplicate.mjs"),
    join(consumer, "duplicate.mjs"),
  );
  for (const [name, runtime] of [
    ["bun", bun],
    ["node", node],
  ]) {
    run(
      [runtime, join(consumer, "duplicate.mjs"), secondEntry],
      consumer,
      name + "-duplicate-entry",
    );
    // A new process must not inherit another realm's guard.
    run([runtime, fresh], consumer, name + "-independent-process");
  }

  copyFileSync(
    join(import.meta.dir, "installed-alias/independent-worker.mjs"),
    join(consumer, "independent-worker.mjs"),
  );
  copyFileSync(
    join(import.meta.dir, "installed-alias/worker-realm.mjs"),
    join(consumer, "worker-realm.mjs"),
  );
  for (const [name, runtime] of [
    ["bun", bun],
    ["node", node],
  ])
    run(
      [runtime, join(consumer, "worker-realm.mjs")],
      consumer,
      name + "-worker-realm",
    );

  const aliasManifestPath = join(
    consumer,
    "node_modules/electron/package.json",
  );
  const originalAliasManifest = readFileSync(aliasManifestPath);
  const alternateEntry = join(dirname(entry), "gh502-alternate.js");
  copyFileSync(entry, alternateEntry);
  try {
    for (const mode of ["split-subpath", "split-condition"]) {
      const altered = JSON.parse(originalAliasManifest.toString());
      if (mode === "split-subpath")
        altered.exports["./utility"] = "./dist/gh502-alternate.js";
      else
        for (const key of Object.keys(altered.exports))
          altered.exports[key] = {
            import: "./dist/index.js",
            require: "./dist/gh502-alternate.js",
          };
      writeJson(aliasManifestPath, altered);
      writeJson(join(receipts, mode + "-manifest.json"), altered);
      for (const [name, runtime] of [
        ["bun", bun],
        ["node", node],
      ]) {
        const rejected = run(
          [runtime, join(consumer, "matrix.mjs")],
          consumer,
          name + "-" + mode,
          false,
        );
        assert(
          rejected.stderr.includes(duplicateCode),
          "the alternate actual entry must refuse",
        );
      }
    }
  } finally {
    writeFileSync(aliasManifestPath, originalAliasManifest);
    rmSync(alternateEntry);
  }

  const originalEntryText = originalEntry.toString();
  const defaultLine = /^[ \t]*[\w$]+ as default,?\r?\n/m;
  assert(
    defaultLine.test(originalEntryText),
    "the pinned compiler must expose its actual default export",
  );
  try {
    writeFileSync(entry, originalEntryText.replace(defaultLine, ""));
    for (const [name, runtime] of [
      ["bun", bun],
      ["node", node],
    ]) {
      const rejected = run(
        [runtime, join(consumer, "matrix.mjs")],
        consumer,
        name + "-missing-default",
        false,
      );
      assert.match(rejected.stderr, /default/);
    }
    writeFileSync(
      entry,
      originalEntryText +
        "\nconst gh502UnexpectedHelper = true; export { gh502UnexpectedHelper as isCallError };\n",
    );
    for (const [name, runtime] of [
      ["bun", bun],
      ["node", node],
    ]) {
      const rejected = run(
        [runtime, join(consumer, "matrix.mjs")],
        consumer,
        name + "-helper-leak",
        false,
      );
      assert.match(rejected.stderr, /public namespace parity/);
    }
  } finally {
    writeFileSync(entry, originalEntry);
  }

  // This mutant edits only the real installed entry; each attempt starts a new Node process.
  try {
    appendFileSync(entry, "\nawait Promise.resolve();\n");
    const rejected = run(
      [node, fresh],
      consumer,
      "node-rejects-transitive-tla",
      false,
    );
    assert.match(rejected.stderr, /ERR_REQUIRE_ASYNC_MODULE/);
  } finally {
    writeFileSync(entry, originalEntry);
  }
  run([node, fresh], consumer, "node-restored-entry");

  const apiManifestPath = join(consumer, "node_modules/@keld/api/package.json");
  const originalApiManifest = readFileSync(apiManifestPath);
  try {
    const altered = JSON.parse(originalApiManifest.toString());
    delete altered.exports["./internal/dialog-codec"];
    writeJson(apiManifestPath, altered);
    for (const [name, runtime] of [
      ["bun", bun],
      ["node", node],
    ]) {
      const rejected = run(
        [runtime, fresh],
        consumer,
        name + "-missing-private-codec",
        false,
      );
      assert.match(rejected.stderr, /dialog-codec/);
    }
  } finally {
    writeFileSync(apiManifestPath, originalApiManifest);
  }

  const mutant = join(scratch, "missing-api-edge");
  mkdirSync(mutant);
  const mutantPackage = join(mutant, "package");
  cpSync(join(consumer, "node_modules/electron"), mutantPackage, {
    recursive: true,
  });
  const mutantManifest = json(join(mutantPackage, "package.json"));
  delete mutantManifest.dependencies["@keld/api"];
  writeJson(join(mutantPackage, "package.json"), mutantManifest);
  run(
    [bun, "pm", "pack", "--ignore-scripts", "--destination", mutant],
    mutantPackage,
    "pack-missing-api-edge",
  );
  const negative = join(scratch, "missing-api-consumer");
  install(negative, join(mutant, `keld-electron-${versions.electron}.tgz`));
  assert(
    !existsSync(join(negative, "node_modules/@keld/api")),
    "An override must not mask the missing declared API edge",
  );
  copyFileSync(fresh, join(negative, "fresh-require.cjs"));
  for (const [name, runtime] of [
    ["bun", bun],
    ["node", node],
  ]) {
    const rejected = run(
      [runtime, join(negative, "fresh-require.cjs")],
      negative,
      name + "-missing-declared-api",
      false,
    );
    assert.match(rejected.stderr, /@keld\/api/);
  }

  const typeMutant = join(scratch, "missing-node-type-edge");
  mkdirSync(typeMutant);
  const typePackage = join(typeMutant, "package");
  cpSync(join(consumer, "node_modules/electron"), typePackage, {
    recursive: true,
  });
  const typeManifest = json(join(typePackage, "package.json"));
  delete typeManifest.dependencies["@types/node"];
  writeJson(join(typePackage, "package.json"), typeManifest);
  run(
    [bun, "pm", "pack", "--ignore-scripts", "--destination", typeMutant],
    typePackage,
    "pack-missing-node-type-edge",
  );
  const missingTypes = join(scratch, "missing-declared-types-consumer");
  install(
    missingTypes,
    join(typeMutant, `keld-electron-${versions.electron}.tgz`),
  );
  assert(
    !existsSync(join(missingTypes, "node_modules/@types/node")),
    "An override must not mask the missing declared Node type edge",
  );
  prepareTypeConsumer(missingTypes);
  const missingTypeResult = run(
    [declarations.compiler, "-p", join(missingTypes, "tsconfig.json")],
    missingTypes,
    "missing-declared-node-types",
    false,
  );
  assert.match(missingTypeResult.stdout + missingTypeResult.stderr, /TS2688/);

  // Explicit retained runs preserve evidence; ordinary test success cleans only this owned fixture.
  writeJson(join(receipts, "installed-receipt.json"), {
    scope:
      "GH502 bounded installed package fixture; not R07/R08 app admission or product acceptance",
    source,
    scratch,
    consumer,
    artifacts,
    runtimes: runtimeReceipts,
    packageLockSha256: hash(join(consumer, "package-lock.json")),
    entrySha256: hash(entry),
    duplicateCode,
    declarations,
    typeInputs: Object.fromEntries(
      Object.entries(typeInputs).map(([name, artifact]) => [
        name,
        { ...artifact, sha256: hash(artifact.path) },
      ]),
    ),
  });
  console.log(
    "INSTALLED_ALIAS_COMPONENT_PASS " +
      join(receipts, "installed-receipt.json"),
  );
  passed = true;
} finally {
  if (passed && process.env.KELD_ELECTRON_KEEP_FIXTURE !== "1")
    rmSync(scratch, { recursive: true, force: true });
  else console.error("INSTALLED_ALIAS_FIXTURE " + scratch);
}

import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { runLayout } from "../../kipc/test/bundle-layout.ts";

const producer = resolve(
  import.meta.dir,
  "../../kipc/scripts/package-artifacts.ts",
);

test("local dependency and sibling source share canonical transport identity", () => {
  const apiRoot = resolve(import.meta.dir, "..");
  const identity = Bun.spawnSync(
    [
      "bun",
      "-e",
      `
import { strict as assert } from "node:assert";
import { realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";
import * as declared from "@keld/kipc";
import * as sibling from "../kipc/src/transport.ts";
assert.equal(realpathSync(fileURLToPath(import.meta.resolve("@keld/kipc"))), realpathSync("../kipc/src/transport.ts"));
assert.equal(declared, sibling);
assert.equal(declared.WorkerLink, sibling.WorkerLink);
`,
    ],
    { cwd: apiRoot, stdout: "pipe", stderr: "pipe" },
  );
  expect(identity.exitCode, new TextDecoder().decode(identity.stderr)).toBe(0);
});

test("real packed SDK installs offline with one API app identity", async () => {
  const scratch = mkdtempSync(join(tmpdir(), "keld-sdk-package-"));
  try {
    const source = Object.fromEntries(
      ["api", "kipc"].map((name) => [
        name,
        JSON.parse(
          readFileSync(
            resolve(import.meta.dir, `../../${name}/package.json`),
            "utf8",
          ),
        ),
      ]),
    );
    const build = Bun.spawnSync(["bun", producer, scratch], {
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(build.exitCode, new TextDecoder().decode(build.stderr)).toBe(0);
    const second = join(scratch, "second");
    mkdirSync(second);
    const rebuild = Bun.spawnSync(["bun", producer, second], {
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(rebuild.exitCode, new TextDecoder().decode(rebuild.stderr)).toBe(0);
    const packageHashes: Record<string, string> = {};
    for (const name of ["api", "kipc"]) {
      const packed = readFileSync(
        join(scratch, `keld-${name}-${source[name].version}.tgz`),
      );
      expect(packed).toEqual(
        readFileSync(join(second, `keld-${name}-${source[name].version}.tgz`)),
      );
      packageHashes[`@keld/${name}`] = createHash("sha256")
        .update(packed)
        .digest("hex");
    }
    const api = join(scratch, `keld-api-${source.api.version}.tgz`);
    const kipc = join(scratch, `keld-kipc-${source.kipc.version}.tgz`);
    const consumer = join(scratch, "consumer");
    mkdirSync(consumer);
    writeFileSync(
      join(consumer, "package.json"),
      JSON.stringify({
        name: "sdk-consumer",
        private: true,
        type: "module",
        dependencies: { "@keld/api": `file:${api}` },
        overrides: { "@keld/kipc": `file:${kipc}` },
      }),
    );
    const install = Bun.spawnSync(
      [
        "npm",
        "install",
        "--offline",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
        "--cache",
        join(scratch, "cache"),
      ],
      { cwd: consumer, stdout: "pipe", stderr: "pipe" },
    );
    expect(install.exitCode, new TextDecoder().decode(install.stderr)).toBe(0);
    const manifest = JSON.parse(
      readFileSync(
        join(consumer, "node_modules/@keld/api/package.json"),
        "utf8",
      ),
    );
    expect(manifest.dependencies["@keld/kipc"]).toBe(source.kipc.version);
    for (const name of ["api", "kipc"]) {
      const installed = JSON.parse(
        readFileSync(
          join(consumer, `node_modules/@keld/${name}/package.json`),
          "utf8",
        ),
      );
      expect(installed.devDependencies).toEqual(source[name].devDependencies);
    }
    const probe = join(consumer, "probe.mjs");
    writeFileSync(
      probe,
      `import {strict as assert} from "node:assert";
import {createRequire} from "node:module";
import {createHash} from "node:crypto";
import {readFileSync, realpathSync, statSync} from "node:fs";
import {fileURLToPath, pathToFileURL} from "node:url";
import * as api from "@keld/api";
import {app} from "@keld/api/internal/app";
import * as installedLink from "@keld/api/internal/link";
import * as kipc from "@keld/kipc";
import "@keld/api/internal/window-codec";
assert.equal(api.app, app);
assert.equal(api.app, createRequire(import.meta.url)("@keld/api").app);
for (const name of ["FrameReader", "DrainSignal", "RECEIVE_POLICIES", "isCallError"])
  assert.equal(installedLink[name], kipc[name], "canonical transport identity: " + name);
assert.deepEqual(Object.keys(api).sort(), ["app", "auth", "channels", "echoChannel", "fs", "isCallError"].sort());
function identityFile(specifier) {
  const path = specifier.startsWith("file:") ? fileURLToPath(specifier) : specifier;
  const native = realpathSync.native(path);
  const stat = statSync(path, {bigint: true});
  return {path, url: pathToFileURL(path).href, native, nativeUrl: pathToFileURL(native).href,
    dev: String(stat.dev), ino: String(stat.ino), fileIdUsable: stat.dev !== 0n && stat.ino !== 0n,
    sha256: createHash("sha256").update(readFileSync(path)).digest("hex")};
}
console.log(JSON.stringify({
  sdkConsumer: true,
  runtime: typeof Bun === "undefined" ? "node" : "bun",
  version: typeof Bun === "undefined" ? process.version : Bun.version,
  identityFiles: [
    identityFile(${JSON.stringify(join(consumer, "node_modules/@keld/api/link.js"))}),
    identityFile(import.meta.resolve("@keld/api/internal/link")),
    identityFile(${JSON.stringify(join(consumer, "node_modules/@keld/kipc/transport.js"))}),
    identityFile(import.meta.resolve("@keld/kipc")),
  ],
}));`,
    );
    const runtimes: Record<string, string> = {};
    const runtimeIdentityFiles: Record<string, unknown> = {};
    for (const runtime of ["bun", "node"]) {
      const run = Bun.spawnSync([runtime, probe], {
        cwd: consumer,
        stdout: "pipe",
        stderr: "pipe",
      });
      expect(
        run.exitCode,
        `${runtime}: ${new TextDecoder().decode(run.stderr)}`,
      ).toBe(0);
      const result = JSON.parse(new TextDecoder().decode(run.stdout));
      expect(result.sdkConsumer).toBe(true);
      expect(result.runtime).toBe(runtime);
      expect(typeof result.version).toBe("string");
      runtimes[runtime] = result.version;
      runtimeIdentityFiles[runtime] = result.identityFiles;
    }
    // Emit real artifact/runtime custody before a later installed role can fail.
    console.log(
      "SDK_INSTALLED_PRE_CASES " +
        JSON.stringify({
          sdkPackageArtifacts: packageHashes,
          producerCwd: process.cwd(),
          runtimes,
          bunRevision: Bun.revision,
          platform: process.platform,
          arch: process.arch,
          identityFiles: runtimeIdentityFiles,
        }),
    );
    // A distinct real transport must not pass the installed identity oracle.
    const identityLink = join(consumer, "node_modules/@keld/api/link.js");
    const identityLinkBytes = readFileSync(identityLink, "utf8");
    const duplicateTransport = join(consumer, "identity-negative-transport.js");
    writeFileSync(
      duplicateTransport,
      readFileSync(join(consumer, "node_modules/@keld/kipc/transport.js")),
    );
    expect(identityLinkBytes).toContain('"@keld/kipc"');
    try {
      writeFileSync(
        identityLink,
        identityLinkBytes.replaceAll(
          '"@keld/kipc"',
          JSON.stringify(pathToFileURL(duplicateTransport).href),
        ),
      );
      for (const runtime of ["bun", "node"]) {
        const splitIdentity = Bun.spawnSync([runtime, probe], {
          cwd: consumer,
          stdout: "pipe",
          stderr: "pipe",
        });
        expect(splitIdentity.exitCode).not.toBe(0);
        expect(new TextDecoder().decode(splitIdentity.stderr)).toContain(
          "canonical transport identity",
        );
      }
    } finally {
      writeFileSync(identityLink, identityLinkBytes);
      rmSync(duplicateTransport, { force: true });
    }
    // Repack the actual produced API after deleting only its declared edge.
    const mutant = join(scratch, "mutant");
    mkdirSync(mutant);
    // GNU tar treats a Windows C:-qualified archive as a remote host; open
    // the same produced file by its name relative to the owned scratch root.
    const unpack = Bun.spawnSync(["tar", "-xzf", basename(api), "-C", mutant], {
      cwd: scratch,
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(unpack.exitCode, new TextDecoder().decode(unpack.stderr)).toBe(0);
    const mutantPackage = join(mutant, "package");
    const mutantManifestPath = join(mutantPackage, "package.json");
    const mutantManifest = JSON.parse(readFileSync(mutantManifestPath, "utf8"));
    delete mutantManifest.dependencies["@keld/kipc"];
    writeFileSync(mutantManifestPath, JSON.stringify(mutantManifest));
    const repack = Bun.spawnSync(
      ["bun", "pm", "pack", "--ignore-scripts", "--destination", mutant],
      { cwd: mutantPackage, stdout: "pipe", stderr: "pipe" },
    );
    expect(repack.exitCode, new TextDecoder().decode(repack.stderr)).toBe(0);
    const negative = join(mutant, "consumer");
    mkdirSync(negative);
    writeFileSync(
      join(negative, "package.json"),
      JSON.stringify({
        name: "sdk-negative",
        private: true,
        type: "module",
        dependencies: {
          "@keld/api": `file:${join(mutant, `keld-api-${source.api.version}.tgz`)}`,
        },
        overrides: { "@keld/kipc": `file:${kipc}` },
      }),
    );
    const negativeInstall = Bun.spawnSync(
      [
        "npm",
        "install",
        "--offline",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
        "--cache",
        join(scratch, "negative-cache"),
      ],
      { cwd: negative, stdout: "pipe", stderr: "pipe" },
    );
    expect(
      negativeInstall.exitCode,
      new TextDecoder().decode(negativeInstall.stderr),
    ).toBe(0);
    expect(existsSync(join(negative, "node_modules/@keld/kipc"))).toBe(false);
    writeFileSync(join(negative, "probe.mjs"), readFileSync(probe));
    for (const runtime of ["bun", "node"]) {
      const negativeImport = Bun.spawnSync([runtime, "probe.mjs"], {
        cwd: negative,
        stdout: "pipe",
        stderr: "pipe",
      });
      expect(negativeImport.exitCode).not.toBe(0);
      expect(new TextDecoder().decode(negativeImport.stderr)).toContain(
        "@keld/kipc",
      );
    }
    writeFileSync(
      join(consumer, "consumer.ts"),
      `import { app, isCallError } from "@keld/api";
import { app as internalApp } from "@keld/api/internal/app";
import type { LifecycleLink } from "@keld/api/internal/link";
import type { WindowRef } from "@keld/api/internal/window-codec";
void [app, internalApp, isCallError]; let link: LifecycleLink; let ref: WindowRef; void [link!, ref!];`,
    );
    writeFileSync(
      join(consumer, "tsconfig.json"),
      JSON.stringify({
        compilerOptions: {
          strict: true,
          noEmit: true,
          target: "ES2024",
          module: "Preserve",
          moduleResolution: "Bundler",
          lib: ["ES2024", "DOM"],
        },
        files: ["consumer.ts"],
      }),
    );
    const tsc = resolve(import.meta.dir, "../node_modules/.bin/tsc");
    const typecheck = Bun.spawnSync([tsc, "-p", "tsconfig.json"], {
      cwd: consumer,
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(
      typecheck.exitCode,
      new TextDecoder().decode(typecheck.stdout) +
        new TextDecoder().decode(typecheck.stderr),
    ).toBe(0);
    // Reuse the source retention and alias witnesses against installed emitted JS.
    // Only their module paths and Rust-owned fixture path change; their assertions remain intact.
    const apiSource = import.meta.dir;
    const emitted = join(consumer, "node_modules/@keld/api");
    const vector = resolve(
      apiSource,
      "../../../../crates/keld-ipc/tests/fixtures/window-v0.tsv",
    );
    for (const name of [
      "window-source.test.ts",
      "window-source-release.test-role.ts",
      "window-source-semantic-release.test-role.ts",
      "window-source-diagnostic-release.test-role.ts",
      "link-quit-release.test.ts",
      "link-quit-release.test-role.ts",
      "link-last-window.test.ts",
      "link-last-window.test-role.ts",
      "window-state-phase.test-role.ts",
    ]) {
      let sourceText = readFileSync(join(apiSource, name), "utf8");
      for (const module of [
        "window-source",
        "window-codec",
        "link",
        "mirror",
      ]) {
        sourceText = sourceText.replaceAll(
          `"./${module}.ts"`,
          JSON.stringify(join(emitted, `${module}.js`)),
        );
      }
      sourceText = sourceText.replace(
        /join\(\s*import\.meta\.dir,\s*"\.\.\/\.\.\/\.\.\/\.\.\/crates\/keld-ipc\/tests\/fixtures\/window-v0\.tsv",\s*\)/,
        JSON.stringify(vector),
      );
      sourceText = sourceText.replaceAll(
        'resolve(import.meta.dir, "../node_modules/typescript")',
        JSON.stringify(resolve(apiSource, "../node_modules/typescript")),
      );
      // Installed API source consumers must retain the same WorkerLink class.
      // Bun's package-key mock did not intercept the compiled module on Windows,
      // so make the two isolated role processes override the existing class's
      // `open` method directly. The full installed transport and API remain
      // unchanged; an unshared class still fails all original assertions.
      if (
        name === "link-quit-release.test-role.ts" ||
        name === "link-last-window.test-role.ts"
      ) {
        const moduleMock = [
          'mock.module("@keld/kipc", () => ({',
          "  ...transport,",
          "  WorkerLink: { open: async () => worker },",
          "}));",
        ].join("\n");
        expect(sourceText.split(moduleMock)).toHaveLength(2);
        sourceText = sourceText.replace(
          moduleMock,
          [
            'const workerOpen = Object.getOwnPropertyDescriptor(transport.WorkerLink, "open");',
            'assert.equal(workerOpen?.writable, true, "canonical WorkerLink.open must remain writable");',
            "const originalWorkerLink = transport.WorkerLink;",
            "const originalWorkerOpen = transport.WorkerLink.open;",
            "let canonicalOpenCalls = 0;",
            "const controlledWorkerOpen = async function(this: typeof originalWorkerLink, options: Parameters<typeof originalWorkerOpen>[0]) {",
            '  assert.equal(this, originalWorkerLink, "canonical WorkerLink receiver");',
            '  assert.equal(options.link, "controlled-link");',
            "  canonicalOpenCalls += 1;",
            "  return worker;",
            "};",
            "transport.WorkerLink.open = controlledWorkerOpen;",
          ].join("\n"),
        );
        const linkImport = `const { LifecycleLink } = await import(${JSON.stringify(join(emitted, "link.js"))});`;
        expect(sourceText.split(linkImport)).toHaveLength(2);
        sourceText = sourceText.replace(
          linkImport,
          [
            `const installedLink = await import(${JSON.stringify(join(emitted, "link.js"))});`,
            // Freeze original references before diagnostics can warm another cache key.
            'const originalIdentities = Object.freeze(["FrameReader", "DrainSignal", "RECEIVE_POLICIES", "isCallError"].map(name => Object.freeze({ name, actual: installedLink[name], expected: transport[name] })));',
            'const originalWorkerState = Object.freeze({ sameClass: transport.WorkerLink === originalWorkerLink, sameOpen: Object.getOwnPropertyDescriptor(transport.WorkerLink, "open")?.value === controlledWorkerOpen, calls: canonicalOpenCalls });',
            "try {",
            "  for (const { name, actual, expected } of originalIdentities)",
            '    assert.equal(actual, expected, "canonical transport identity: " + name);',
            "} catch (originalError) {",
            "  try {",
            '    const fs = await import("node:fs");',
            '    const path = await import("node:path");',
            '    const url = await import("node:url");',
            '    const crypto = await import("node:crypto");',
            `    const explicit = ${JSON.stringify(join(emitted, "link.js"))};`,
            "    const canonical = fs.realpathSync.native(explicit);",
            "    function identityFile(specifier: string) {",
            '      const file = specifier.startsWith("file:") ? url.fileURLToPath(specifier) : specifier;',
            "      const native = fs.realpathSync.native(file);",
            "      const stat = fs.statSync(file, { bigint: true });",
            '      return { path: file, url: url.pathToFileURL(file).href, native, nativeUrl: url.pathToFileURL(native).href, dev: String(stat.dev), ino: String(stat.ino), fileIdUsable: stat.dev !== 0n && stat.ino !== 0n, sha256: crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex") };',
            "    }",
            '    const bare = import.meta.resolve("@keld/api/internal/link");',
            '    const roleKipc = import.meta.resolve("@keld/kipc");',
            '    const explicitKipc = Bun.resolveSync("@keld/kipc", path.dirname(explicit));',
            '    const canonicalKipc = Bun.resolveSync("@keld/kipc", path.dirname(canonical));',
            '    console.error("SDK_INSTALLED_IDENTITY_ORIGINAL " + JSON.stringify({ role: import.meta.url, runtime: { bun: Bun.version, revision: Bun.revision, platform: process.platform, arch: process.arch }, identities: originalIdentities.map(({ name, actual, expected }) => ({ name, equal: actual === expected })), worker: originalWorkerState, files: { explicit: identityFile(explicit), bare: identityFile(bare), roleKipc: identityFile(roleKipc), explicitKipc: identityFile(explicitKipc), canonicalKipc: identityFile(canonicalKipc) } }));',
            "    const providers = [];",
            '    for (const [name, specifier] of [["explicit", explicit], ["bare", "@keld/api/internal/link"], ["native-file-url", url.pathToFileURL(canonical).href]]) {',
            "      const provider = await import(specifier);",
            "      providers.push({ name, specifier, equalsOriginalActual: provider.FrameReader === originalIdentities[0]!.actual, equalsOriginalExpected: provider.FrameReader === originalIdentities[0]!.expected });",
            "    }",
            '    console.error("SDK_INSTALLED_IDENTITY_PROVIDERS " + JSON.stringify({ providers, originalEqualStill: originalIdentities[0]!.actual === originalIdentities[0]!.expected }));',
            "  } catch (diagnosticError) {",
            '    console.error("SDK_INSTALLED_IDENTITY_DIAGNOSTIC_UNAVAILABLE " + String(diagnosticError));',
            "  }",
            // Later convergence cannot replace the exact original failed assertion.
            "  throw originalError;",
            "}",
            'assert.equal(transport.WorkerLink, originalWorkerLink, "canonical WorkerLink class after import");',
            'assert.equal(Object.getOwnPropertyDescriptor(transport.WorkerLink, "open")?.value, controlledWorkerOpen, "canonical open override after import");',
            "const { LifecycleLink } = installedLink;",
          ].join("\n"),
        );
        // Each role has its own process and restores its exact original class
        // method after its source/terminal checks complete successfully.
        sourceText += [
          "",
          'assert.equal(canonicalOpenCalls, 1, "canonical WorkerLink.open invocation count");',
          'assert.equal(transport.WorkerLink.open, controlledWorkerOpen, "canonical open override after connect");',
          'Object.defineProperty(transport.WorkerLink, "open", workerOpen!);',
          "",
        ].join("\n");
      }
      writeFileSync(join(consumer, name), sourceText);
    }
    const sourceProof = Bun.spawnSync(
      [
        "bun",
        "test",
        "window-source.test.ts",
        "link-quit-release.test.ts",
        "link-last-window.test.ts",
      ],
      {
        cwd: consumer,
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    expect(
      sourceProof.exitCode,
      new TextDecoder().decode(sourceProof.stdout) +
        new TextDecoder().decode(sourceProof.stderr),
    ).toBe(0);
    for (const mode of [
      "push",
      "freeze",
      "get",
      "iterator",
      "integer",
      "bigint",
      "byteLength",
      "brand",
      "invocation",
      "inherited-slot",
      "public-copy-slot",
      "native-primitives",
      "failure-error",
      "failure-define",
      "failure-startsWith",
      "view-custody",
      "escape-values",
      "escape-clear",
      "escape-iterator",
      "lifecycle-length",
      "err-iterator",
      "err-startsWith",
      "err-subarray",
      "err-length",
      "err-textdecoder",
      "err-decode",
    ]) {
      const phase = Bun.spawnSync(
        ["bun", join(consumer, "window-state-phase.test-role.ts"), mode],
        {
          cwd: consumer,
          env: { ...process.env, KELD_NATIVE_WINDOW_FACADE: "0" },
          stdout: "pipe",
          stderr: "pipe",
        },
      );
      expect(phase.exitCode, new TextDecoder().decode(phase.stderr)).toBe(0);
      expect(JSON.parse(new TextDecoder().decode(phase.stdout))).toEqual({
        mode,
        hits: 0,
        variant: mode.startsWith("failure-") ? "rejected" : "Created",
      });
    }
    // The installed API's real LifecycleLink, not a WorkerLink mock, reaches a private peer.
    const lifecycleRole = join(consumer, "package-lifecycle.test-role.ts");
    writeFileSync(
      lifecycleRole,
      readFileSync(
        resolve(apiSource, "../fixtures/package-lifecycle.test-role.ts"),
      ),
    );
    function lifecycle(generation: "initial" | "recovery") {
      return Bun.spawnSync(["bun", lifecycleRole, generation], {
        cwd: consumer,
        env: { ...process.env, KELD_NATIVE_WINDOW_FACADE: "1" },
        stdout: "pipe",
        stderr: "pipe",
        timeout: 10_000,
      });
    }
    const live = lifecycle("initial");
    expect(live.exitCode, new TextDecoder().decode(live.stderr)).toBe(0);
    expect(new TextDecoder().decode(live.stdout).trim()).toBe(
      "PASS installed-api-real-worker-source-end initial",
    );
    // The first child has exited and disposed its peer before the new realm starts.
    const recovered = lifecycle("recovery");
    expect(recovered.exitCode, new TextDecoder().decode(recovered.stderr)).toBe(
      0,
    );
    expect(new TextDecoder().decode(recovered.stdout).trim()).toBe(
      "PASS installed-api-real-worker-source-end recovery",
    );
    const installedLink = join(emitted, "link.js");
    const originalLink = readFileSync(installedLink, "utf8");
    try {
      for (const statement of [
        "attachMirror(workerLink, windowSource);",
        "windowSource?.end(err);",
      ]) {
        expect(originalLink.split(statement)).toHaveLength(2);
        writeFileSync(
          installedLink,
          originalLink.replace(statement, "void 0;"),
        );
        const rejected = lifecycle("recovery");
        expect(rejected.exitCode).not.toBe(0);
        expect(new TextDecoder().decode(rejected.stderr)).toContain(
          statement.startsWith("attachMirror")
            ? "GENERATION_REPLAY_NOT_APPLIED"
            : "Missing expected exception",
        );
      }
    } finally {
      writeFileSync(installedLink, originalLink);
    }
    const transport = join(consumer, "node_modules/@keld/kipc/transport.js");
    const original = readFileSync(transport, "utf8");
    async function workerProbe() {
      return runLayout({
        closeOnOpen: true,
        ready: (stdout) => stdout.includes("main-open="),
        async write(dir) {
          writeFileSync(
            join(dir, "main.js"),
            `import { appendFileSync } from "node:fs";
appendFileSync(process.env.KELD_TEST_EVALS, "eval\\n");
import { WorkerLink, isCallError } from ${JSON.stringify(transport)};
WorkerLink.open({ link: process.env.KELD_APP_LINK, receive: { eventChannels: [3], callReceivers: [] } }).then(
  () => console.log("main-open=opened"),
  (err) => console.log("main-open=" + (isCallError(err) ? err.code : "untyped")));
`,
          );
          return "main.js";
        },
      });
    }
    const intact = await workerProbe();
    expect(intact.connections).toBe(1);
    expect(intact.evals).toBe(1);
    const newline = original.indexOf("\n");
    const first = original.slice(0, newline);
    const last = first.at(-1);
    writeFileSync(
      transport,
      first.slice(0, -1) + (last === "0" ? "1" : "0") + original.slice(newline),
    );
    const corrupt = await workerProbe();
    expect(corrupt.connections).toBe(0);
    expect(corrupt.stdout.trim()).toBe("main-open=KELD-IPC-005");
    writeFileSync(transport, original);
    const installedKipc = join(consumer, "node_modules/@keld/kipc");
    renameSync(installedKipc, `${installedKipc}-removed`);
    for (const runtime of ["bun", "node"]) {
      const missing = Bun.spawnSync([runtime, probe], {
        cwd: consumer,
        stdout: "pipe",
        stderr: "pipe",
      });
      expect(missing.exitCode).not.toBe(0);
      expect(new TextDecoder().decode(missing.stderr)).toContain("@keld/kipc");
    }
    renameSync(`${installedKipc}-removed`, installedKipc);
    console.log(
      JSON.stringify({
        sdkPackageArtifacts: packageHashes,
        producerCwd: process.cwd(),
        runtimes,
      }),
    );
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}, 30_000);

import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { TEST_HOOK_NAMES } from "../../kipc/scripts/package-artifacts.ts";

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
  const scratch = realpathSync.native(
    mkdtempSync(join(tmpdir(), "keld-sdk-package-")),
  );
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
      cwd: resolve(import.meta.dir, "../../../.."),
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(build.exitCode, new TextDecoder().decode(build.stderr)).toBe(0);
    const second = join(scratch, "second");
    mkdirSync(second);
    const rebuild = Bun.spawnSync(["bun", producer, second], {
      cwd: resolve(import.meta.dir, ".."),
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
      expect(
        ["scripts", "packageManager"].filter((key) => key in installed),
        `packed @keld/${name} manifest ships source-only fields; the producer must allowlist manifest keys`,
      ).toEqual([]);
    }
    // The shipped Worker is KIPC's release build: packing keeps no test hook.
    const packedTransport = readFileSync(
      join(consumer, "node_modules/@keld/kipc/transport.js"),
      "utf8",
    );
    expect(
      TEST_HOOK_NAMES.filter((name) => packedTransport.includes(name)),
      "packed @keld/kipc transport.js names WorkerLink test hooks; its release build must define KELD_KIPC_RELEASE",
    ).toEqual([]);
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
import {encodeDialogRequest} from "@keld/api/internal/dialog-codec";
import * as installedLink from "@keld/api/internal/link";
import * as kipc from "@keld/kipc";
import "@keld/api/internal/window-codec";
assert.equal(api.app, app);
assert.equal(api.app, createRequire(import.meta.url)("@keld/api").app);
assert.deepEqual(Array.from(encodeDialogRequest({variant: "Cancel", nonce: 1n})), [1, 1]);
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
    // The new private dialog codec is a declared installed API boundary. Keep
    // its physical file and remove only the export: direct source reach-through
    // must not make this exact downstream import succeed.
    const apiManifestPath = join(
      consumer,
      "node_modules/@keld/api/package.json",
    );
    const originalApiManifest = readFileSync(apiManifestPath, "utf8");
    try {
      const withoutDialogCodec = JSON.parse(originalApiManifest);
      delete withoutDialogCodec.exports["./internal/dialog-codec"];
      writeFileSync(apiManifestPath, JSON.stringify(withoutDialogCodec));
      for (const runtime of ["bun", "node"]) {
        const missingCodec = Bun.spawnSync([runtime, probe], {
          cwd: consumer,
          stdout: "pipe",
          stderr: "pipe",
        });
        expect(missingCodec.exitCode).not.toBe(0);
        expect(new TextDecoder().decode(missingCodec.stderr)).toContain(
          "dialog-codec",
        );
      }
    } finally {
      writeFileSync(apiManifestPath, originalApiManifest);
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
    const unpack = Bun.spawnSync(
      ["tar", "-xzf", basename(api), "-C", basename(mutant)],
      {
        cwd: scratch,
        stdout: "pipe",
        stderr: "pipe",
      },
    );
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
import { encodeDialogRequest, type DialogReplyV1 } from "@keld/api/internal/dialog-codec";
import type { LifecycleLink } from "@keld/api/internal/link";
import type { WindowRef } from "@keld/api/internal/window-codec";
void [app, internalApp, isCallError, encodeDialogRequest]; let link: LifecycleLink; let ref: WindowRef; let reply: DialogReplyV1; void [link!, ref!, reply!];`,
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
    // Package graph proof only. Native/lifecycle acceptance remains separately gated.
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

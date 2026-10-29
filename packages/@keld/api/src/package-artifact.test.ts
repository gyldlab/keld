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
import * as api from "@keld/api";
import {app} from "@keld/api/internal/app";
import "@keld/api/internal/link";
import "@keld/api/internal/window-codec";
assert.equal(api.app, app);
assert.equal(api.app, createRequire(import.meta.url)("@keld/api").app);
assert.deepEqual(Object.keys(api).sort(), ["app", "auth", "channels", "echoChannel", "fs", "isCallError"].sort());
console.log(JSON.stringify({
  sdkConsumer: true,
  runtime: typeof Bun === "undefined" ? "node" : "bun",
  version: typeof Bun === "undefined" ? process.version : Bun.version,
}));`,
    );
    const runtimes: Record<string, string> = {};
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

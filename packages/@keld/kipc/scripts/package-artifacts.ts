import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { stampTransport, verifyTransportStamp } from "../src/transport.ts";

const kipc = resolve(import.meta.dir, "..");

/**
 * Every WorkerLink test-hook name and the hook entry's env gate and seam key.
 * A release build names none of them: the KIPC release-build test checks
 * `buildReleaseTransport`, and the API artifact test checks the packed file.
 */
export const TEST_HOOK_NAMES: readonly string[] = Object.freeze([
  "openWorkerLinkForTest",
  "deferDispatch",
  "claimFault",
  "onBlockingCall",
  "holdMessagesUntilBlocking",
  "beforeStateCheck",
  "beforeWakeDrain",
  "beforeDeadlineCas",
  "onDispatchIdle",
  "counterStart",
  "wedgeHoldingKick",
  "onWorkerExit",
  "beforeWorkerSpawn",
  "KELD_KIPC_TEST_HOOKS",
  "worker-link-test-seam",
]);

/**
 * KIPC's one transport release build (#528 T3, GH-1287): `src/transport.ts`
 * built on its own with `KELD_KIPC_RELEASE` defined as `true` and syntax
 * minification, so every test-hook branch is removed. The packer stamps and
 * ships this output, and `src/release-build.test.ts` checks the same build;
 * `release: false` omits only the constant, for that test's negative control.
 *
 * Bun emits caller-relative source comments, and its `root` option does not
 * change them, so the build runs from the KIPC directory and restores the
 * caller's. Identical source and toolchain inputs then produce identical
 * bytes from any caller directory. A failed build throws Bun's AggregateError.
 */
export async function buildReleaseTransport(release = true): Promise<string> {
  const callerCwd = process.cwd();
  let built: Awaited<ReturnType<typeof Bun.build>>;
  try {
    process.chdir(kipc);
    built = await Bun.build({
      entrypoints: [join(kipc, "src/transport.ts")],
      target: "node",
      format: "esm",
      minify: { syntax: true },
      ...(release ? { define: { KELD_KIPC_RELEASE: "true" } } : {}),
    });
  } finally {
    process.chdir(callerCwd);
  }
  if (built.outputs.length !== 1)
    throw new Error(
      `KIPC release build emitted ${built.outputs.length} files; expected one self-Worker transport file`,
    );
  return built.outputs[0].text();
}

/** Existing SDK producer; add --electron to include the installed alias artifact. */
if (import.meta.main) {
  const [destination, ...flags] = process.argv.slice(2);
  if (flags.length > 1 || (flags.length === 1 && flags[0] !== "--electron"))
    throw new Error(
      "Usage: bun package-artifacts.ts <output-directory> [--electron]",
    );
  await pack(destination, flags[0] === "--electron");
}

async function pack(
  destination: string | undefined,
  includeElectron = false,
): Promise<void> {
  const api = resolve(kipc, "../api");
  if (!destination) throw new Error("Provide an artifact output directory");
  const output = resolve(destination);
  mkdirSync(output, { recursive: true });
  const scratch = mkdtempSync(join(tmpdir(), "keld-artifacts-build-"));
  try {
    const apiOut = join(scratch, "api");
    const kipcOut = join(scratch, "kipc");
    mkdirSync(apiOut);
    mkdirSync(kipcOut);

    function run(command: string[], cwd: string): void {
      const result = Bun.spawnSync(command, {
        cwd,
        stdout: "inherit",
        stderr: "inherit",
      });
      if (result.exitCode !== 0)
        throw new Error(`${command[0]} failed with ${result.exitCode}`);
    }
    interface SourceManifest {
      name: string;
      version: string;
      exports: Record<string, string>;
      dependencies?: Record<string, string>;
      devDependencies?: Record<string, string>;
      [key: string]: unknown;
    }
    function manifest(path: string): SourceManifest {
      return JSON.parse(
        readFileSync(join(path, "package.json"), "utf8"),
      ) as SourceManifest;
    }
    const apiManifest = manifest(api);
    const kipcManifest = manifest(kipc);
    function exactPinnedDevVersion(
      source: SourceManifest,
      name: "typescript" | "@types/bun",
    ): string {
      const version = source.devDependencies?.[name];
      if (typeof version !== "string" || !/^\d+\.\d+\.\d+$/.test(version))
        throw new Error(`${source.name} must pin exact ${name}`);
      return version;
    }
    const typescriptVersion = exactPinnedDevVersion(apiManifest, "typescript");
    const bunTypesVersion = exactPinnedDevVersion(apiManifest, "@types/bun");
    if (
      exactPinnedDevVersion(kipcManifest, "typescript") !== typescriptVersion ||
      exactPinnedDevVersion(kipcManifest, "@types/bun") !== bunTypesVersion
    )
      throw new Error(
        "API and KIPC must share the exact pinned TypeScript compiler closure",
      );
    const compiler = join(api, "node_modules/typescript/bin/tsc");
    const typeRoots = join(api, "node_modules/@types");
    try {
      const installedTypescript = JSON.parse(
        readFileSync(join(api, "node_modules/typescript/package.json"), "utf8"),
      ).version;
      const installedBunTypes = JSON.parse(
        readFileSync(join(typeRoots, "bun/package.json"), "utf8"),
      ).version;
      readFileSync(compiler);
      if (
        installedTypescript !== typescriptVersion ||
        installedBunTypes !== bunTypesVersion
      )
        throw new Error("installed compiler closure version mismatch");
    } catch (error) {
      throw new Error(
        `Pinned API TypeScript compiler closure is unavailable: ${String(error)}`,
      );
    }
    const apiExports = {
      ".": "./src/index.ts",
      "./internal/app": "./src/app.ts",
      "./internal/dialog-codec": "./src/dialog.generated.ts",
      "./internal/menu-codec": "./src/menu.generated.ts",
      "./internal/link": "./src/link.ts",
      "./internal/window-codec": "./src/window-codec.ts",
      "./runtime-preload": "./src/runtime-preload.ts",
    };
    if (
      JSON.stringify(apiManifest.exports) !== JSON.stringify(apiExports) ||
      apiManifest.dependencies?.["@keld/kipc"] !== "link:../kipc" ||
      JSON.stringify(kipcManifest.exports) !==
        JSON.stringify({
          ".": "./src/transport.ts",
          "./endpoint-value-codec": "./src/endpoint-value-codec.ts",
          "./compat-control": "./src/compat-control.generated.ts",
          "./runtime-process-facts": "./src/runtime-process.generated.ts",
        })
    ) {
      throw new Error("Unexpected source package exports or KIPC dependency");
    }
    run(
      [
        process.execPath,
        compiler,
        "-p",
        join(api, "tsconfig.build.json"),
        "--outDir",
        apiOut,
        "--typeRoots",
        typeRoots,
      ],
      api,
    );
    run(
      [
        process.execPath,
        compiler,
        "-p",
        join(kipc, "tsconfig.build.json"),
        "--outDir",
        kipcOut,
        "--typeRoots",
        typeRoots,
      ],
      kipc,
    );
    const transport = stampTransport(await buildReleaseTransport());
    verifyTransportStamp(new TextEncoder().encode(transport), "transport.js");
    writeFileSync(join(kipcOut, "transport.js"), transport);
    // The existing TypeScript emission above also emits companion codecs.
    // Only transport.js is replaced by its canonical stripped, stamped build.
    function distExports(
      entries: Record<string, string>,
    ): Record<string, { types: string; default: string }> {
      return Object.fromEntries(
        Object.entries(entries).map(([key, value]) => {
          const base = value.replace(/^\.\/src\//, "./").replace(/\.ts$/, "");
          return [key, { types: `${base}.d.ts`, default: `${base}.js` }];
        }),
      );
    }
    // Ship an allowlist, never the whole source manifest: source scripts name
    // files the tarball omits, and an install script would run on consumers'
    // machines. Development dependencies stay as declared (criterion 5).
    function distManifest(
      source: SourceManifest,
      exports: Record<string, string>,
      dependencies: Record<string, string> | undefined,
    ) {
      return {
        name: source.name,
        version: source.version,
        description: source.description,
        license: source.license,
        type: source.type,
        engines: source.engines,
        exports: distExports(exports),
        dependencies,
        peerDependencies: source.peerDependencies,
        devDependencies: source.devDependencies,
        files: ["*.js", "*.d.ts"],
      };
    }
    const apiDist = distManifest(apiManifest, apiExports, {
      ...apiManifest.dependencies,
      "@keld/kipc": kipcManifest.version,
    });
    const kipcDist = distManifest(
      kipcManifest,
      kipcManifest.exports,
      kipcManifest.dependencies,
    );
    interface ArtifactManifest {
      exports: Record<string, string | { types: string; default: string }>;
      [key: string]: unknown;
    }
    const artifacts: [string, ArtifactManifest][] = [
      [apiOut, apiDist],
      [kipcOut, kipcDist],
    ];
    if (includeElectron) {
      const electron = resolve(kipc, "../electron");
      const electronManifest = manifest(electron);
      const nodeTypesVersion = manifest(
        dirname(Bun.resolveSync("@types/node/package.json", electron)),
      ).version;
      const electronExports = {
        ".": "./src/index.ts",
        "./main": "./src/index.ts",
        "./renderer": "./src/index.ts",
        "./common": "./src/index.ts",
        "./utility": "./src/index.ts",
        "./internal/original-ipc": "./src/original-ipc.ts",
      };
      if (
        JSON.stringify(electronManifest.exports) !==
          JSON.stringify(electronExports) ||
        electronManifest.dependencies?.["@keld/api"] !== "link:../api" ||
        electronManifest.dependencies?.["@keld/kipc"] !== "link:../kipc" ||
        electronManifest.dependencies?.["@types/node"] !== nodeTypesVersion ||
        !/^\d+\.\d+\.\d+$/.test(nodeTypesVersion) ||
        Object.keys(electronManifest.dependencies).length !== 3
      )
        throw new Error(
          "Unexpected Electron source exports or declared SDK dependencies",
        );
      const electronOut = join(scratch, "electron");
      const electronDist = join(electronOut, "dist");
      mkdirSync(electronDist, { recursive: true });
      run(
        [
          join(electron, "node_modules/.bin/tsc"),
          "-p",
          join(electron, "tsconfig.build.json"),
          "--outDir",
          electronDist,
        ],
        electron,
      );
      // Consumers need Node declarations for the public EventEmitter base. Add
      // that dependency at the packed declaration boundary: a source directive
      // would pull this package's development Node types into sibling tests.
      for (const name of ["index.d.ts", "original-ipc.d.ts"]) {
        const declarationEntry = join(electronDist, name);
        writeFileSync(
          declarationEntry,
          '/// <reference types="node" />\n' +
            readFileSync(declarationEntry, "utf8"),
        );
      }
      const callerCwd = process.cwd();
      let built: Awaited<ReturnType<typeof Bun.build>>;
      try {
        process.chdir(electron);
        built = await Bun.build({
          entrypoints: [
            join(electron, "src/index.ts"),
            join(electron, "src/original-ipc.ts"),
          ],
          outdir: electronDist,
          splitting: true,
          target: "node",
          format: "esm",
          packages: "external",
        });
      } finally {
        process.chdir(callerCwd);
      }
      // Both entries share ipc-main's private capability/registry chunk. A
      // separately bundled helper would silently create a second facade state.
      if (
        !built.success ||
        !["index.js", "original-ipc.js"].every((name) =>
          built.outputs.some(
            (output) => output.path === join(electronDist, name),
          ),
        )
      )
        throw new Error(
          "Electron release build must emit the canonical facade and shared original IPC wrapper",
        );
      artifacts.push([
        electronOut,
        {
          ...distManifest(electronManifest, electronExports, {
            "@keld/api": apiManifest.version,
            "@keld/kipc": kipcManifest.version,
            "@types/node": nodeTypesVersion,
          }),
          exports: Object.fromEntries(
            Object.entries(electronExports).map(([key, path]) => [
              key,
              path.replace("./src/", "./dist/").replace(/\.ts$/, ".js"),
            ]),
          ),
          types: "./dist/index.d.ts",
          files: ["dist"],
        },
      ]);
    }
    for (const [path, source] of artifacts) {
      for (const entry of Object.values(source.exports)) {
        if (typeof entry === "string") {
          readFileSync(join(path, entry));
          readFileSync(join(path, entry.replace(/\.js$/, ".d.ts")));
        } else {
          readFileSync(join(path, entry.types));
          readFileSync(join(path, entry.default));
        }
      }
      writeFileSync(
        join(path, "package.json"),
        `${JSON.stringify(source, null, 2)}\n`,
      );
      run(
        ["bun", "pm", "pack", "--ignore-scripts", "--destination", output],
        path,
      );
    }
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

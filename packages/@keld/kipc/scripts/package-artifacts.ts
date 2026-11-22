import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
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

if (import.meta.main) await pack(process.argv[2]);

async function pack(destination: string | undefined): Promise<void> {
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
      [key: string]: unknown;
    }
    function manifest(path: string): SourceManifest {
      return JSON.parse(
        readFileSync(join(path, "package.json"), "utf8"),
      ) as SourceManifest;
    }
    const apiManifest = manifest(api);
    const kipcManifest = manifest(kipc);
    const apiExports = {
      ".": "./src/index.ts",
      "./internal/app": "./src/app.ts",
      "./internal/link": "./src/link.ts",
      "./internal/window-codec": "./src/window-codec.ts",
    };
    if (
      JSON.stringify(apiManifest.exports) !== JSON.stringify(apiExports) ||
      apiManifest.dependencies?.["@keld/kipc"] !== "link:../kipc" ||
      JSON.stringify(kipcManifest.exports) !==
        JSON.stringify({ ".": "./src/transport.ts" })
    ) {
      throw new Error("Unexpected source package exports or KIPC dependency");
    }
    run(
      [
        join(api, "node_modules/.bin/tsc"),
        "-p",
        join(api, "tsconfig.build.json"),
        "--outDir",
        apiOut,
      ],
      api,
    );
    run(
      [
        join(kipc, "node_modules/.bin/tsc"),
        "-p",
        join(kipc, "tsconfig.build.json"),
        "--outDir",
        kipcOut,
      ],
      kipc,
    );
    const transport = stampTransport(await buildReleaseTransport());
    verifyTransportStamp(new TextEncoder().encode(transport), "transport.js");
    writeFileSync(join(kipcOut, "transport.js"), transport);
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
    for (const [path, source] of [
      [apiOut, apiDist],
      [kipcOut, kipcDist],
    ] as const) {
      for (const entry of Object.values(source.exports)) {
        readFileSync(join(path, entry.types));
        readFileSync(join(path, entry.default));
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

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
const api = resolve(kipc, "../api");
const destination = process.argv[2];
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
  const built = await Bun.build({
    entrypoints: [join(kipc, "src/transport.ts")],
    target: "node",
    format: "esm",
    minify: { syntax: true },
    define: { KELD_KIPC_RELEASE: "true" },
  });
  if (!built.success || built.outputs.length !== 1)
    throw new Error(`KIPC release build failed: ${built.logs}`);
  const transport = stampTransport(await built.outputs[0].text());
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
  const apiDist = {
    ...apiManifest,
    exports: distExports(apiExports),
    dependencies: {
      ...apiManifest.dependencies,
      "@keld/kipc": kipcManifest.version,
    },
    files: ["*.js", "*.d.ts"],
  };
  const kipcDist = {
    ...kipcManifest,
    exports: distExports(kipcManifest.exports),
    files: ["*.js", "*.d.ts"],
  };
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

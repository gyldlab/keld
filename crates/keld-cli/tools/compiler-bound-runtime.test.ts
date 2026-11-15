// Windows native acceptance for the exact projected local compiler entry.
// Supply KELD_COMPILER_TEST_TYPESCRIPT_ROOT and KELD_COMPILER_TEST_TSGO from
// the already qualified toolchain. KELD_COMPILER_TEST_BUN defaults to Bun.
// A successful test requires a real Bun child, TypeScript 7.0.2 and tsgo.
import { expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const repo = resolve(import.meta.dir, "../../..");
const tools = import.meta.dir;
const typescript = process.env.KELD_COMPILER_TEST_TYPESCRIPT_ROOT;
const parser = process.env.KELD_COMPILER_TEST_TSGO;
const bun = process.env.KELD_COMPILER_TEST_BUN ?? process.execPath;
const runtimeTest = typescript && parser ? test : test.skip;
const renderer = new TextEncoder().encode(
  "<html><head><title>Keld</title></head><body><main>Bound fixture</main></body></html>",
);
const bytes = (value: string): Uint8Array => new TextEncoder().encode(value);
const sha256 = (value: Uint8Array): Buffer =>
  createHash("sha256").update(value).digest();

function checkedProjection(
  path: string,
  bindings: readonly (readonly [string, string])[],
): string {
  // Model the reviewed Rust binder's public file URL output; the execution
  // oracle below is Bun's actual compiler response, not this string transform.
  let text = readFileSync(path, "utf8");
  for (const [literal, target] of bindings) {
    const source = JSON.stringify(literal);
    if (text.split(source).length !== 2)
      throw new Error(
        "Exact compiler import source no longer matches its binding",
      );
    text = text.replace(source, JSON.stringify(pathToFileURL(target).href));
  }
  return text;
}

function appendNumber(chunks: Buffer[], value: number): void {
  const number = Buffer.alloc(4);
  number.writeUInt32LE(value, 0);
  chunks.push(number);
}

function request(modules: readonly (readonly [string, string])[]): Buffer {
  const chunks = [Buffer.from("KELDCIN3")];
  const entry = bytes("main.js");
  appendNumber(chunks, entry.length);
  chunks.push(Buffer.from(entry));
  appendNumber(chunks, modules.length);
  for (const [name, source] of modules) {
    const member = bytes(name);
    const content = bytes(source);
    appendNumber(chunks, member.length);
    chunks.push(Buffer.from(member));
    appendNumber(chunks, content.length);
    chunks.push(Buffer.from(content));
  }
  appendNumber(chunks, renderer.length);
  chunks.push(Buffer.from(renderer));
  return Buffer.concat(chunks);
}

function assertOfficialCompiler(root: string): void {
  const manifest: Record<string, string> = JSON.parse(
    readFileSync(
      join(repo, "crates/keld-cli/src/build/typescript-files.json"),
      "utf8",
    ),
  ) as Record<string, string>;
  expect(Object.keys(manifest)).toHaveLength(416);
  for (const [name, expected] of Object.entries(manifest)) {
    const actual = sha256(readFileSync(join(root, name))).toString("hex");
    expect(actual, name).toBe(expected);
  }
}

function stageCompiler(stage: string): string {
  if (!typescript || !parser) throw new Error("Qualified Bun inputs required");
  const compiler = checkedProjection(join(tools, "closed-app.ts"), [
    ["typescript/unstable/ast", join(typescript, "dist/ast/index.js")],
    ["typescript/unstable/async", join(typescript, "dist/api/async/api.js")],
    ["typescript/unstable/fs", join(typescript, "dist/api/fs.js")],
  ]);
  const boundCompiler = join(stage, "closed-app-bound.ts");
  writeFileSync(boundCompiler, compiler, { flag: "wx" });
  const entry = checkedProjection(join(tools, "closed-app-runner.ts"), [
    ["./closed-app.ts", boundCompiler],
    ["./protocol.ts", join(tools, "protocol.ts")],
    ["./static-renderer.ts", join(tools, "static-renderer.ts")],
    [
      "../../../packages/@keld/kipc/scripts/package-artifacts.ts",
      join(repo, "packages/@keld/kipc/scripts/package-artifacts.ts"),
    ],
    [
      "../../../packages/@keld/kipc/src/transport.ts",
      join(repo, "packages/@keld/kipc/src/transport.ts"),
    ],
  ]);
  const runner = join(stage, "compiler-bound-runner.ts");
  writeFileSync(runner, entry, { flag: "wx" });
  writeFileSync(
    join(stage, "compiler-preload.ts"),
    "// Fixed explicit compiler imports; no runtime resolver hook.\n",
    { flag: "wx" },
  );
  writeFileSync(
    join(stage, "bunfig.toml"),
    "# KELD empty local compiler configuration\n",
    {
      flag: "wx",
    },
  );
  writeFileSync(
    join(stage, ".bunfig.toml"),
    "# KELD empty global compiler configuration\n",
    {
      flag: "wx",
    },
  );
  writeFileSync(join(stage, "tsconfig.json"), '{"compilerOptions":{}}\n', {
    flag: "wx",
  });
  return runner;
}

function execute(
  modules: readonly (readonly [string, string])[],
  hostileAmbient: boolean,
): {
  status: number | null;
  stdout: Buffer;
  stderr: string;
  artifacts: string[];
} {
  if (!typescript || !parser) throw new Error("Qualified Bun inputs required");
  const outer = mkdtempSync(join(tmpdir(), "keld-projected-compiler-"));
  try {
    const stage = join(outer, "private");
    mkdirSync(stage);
    const runner = stageCompiler(stage);
    const hostileHome = join(outer, "ambient-home");
    const sourceParent = join(outer, "node_modules", "ambient-test-only");
    if (hostileAmbient) {
      mkdirSync(hostileHome);
      mkdirSync(sourceParent, { recursive: true });
      writeFileSync(
        join(hostileHome, ".bunfig.toml"),
        'preload = ["./poison.ts"]\n',
      );
      writeFileSync(
        join(hostileHome, "poison.ts"),
        'throw new Error("AMBIENT_BUN_CONFIG_USED");',
      );
      writeFileSync(
        join(outer, "tsconfig.json"),
        '{"compilerOptions":{"paths":{"*":["node_modules/ambient-test-only/index.js"]}}}',
      );
      writeFileSync(
        join(sourceParent, "index.js"),
        'throw new Error("AMBIENT_MODULE_USED");',
      );
      writeFileSync(
        join(sourceParent, "package.json"),
        '{"name":"ambient-test-only","type":"module","main":"index.js"}',
      );
    }
    const args = [
      "--no-install",
      "--no-env-file",
      "--config=" + join(stage, "bunfig.toml"),
      "--tsconfig-override",
      join(stage, "tsconfig.json"),
      "--preload",
      join(stage, "compiler-preload.ts"),
      runner,
      parser,
    ];
    const output = spawnSync(bun, args, {
      cwd: stage,
      input: request(modules),
      env: {
        ...process.env,
        HOME: stage,
        USERPROFILE: stage,
        APPDATA: stage,
        LOCALAPPDATA: stage,
        TEMP: stage,
        TMP: stage,
        NODE_PATH: join(outer, "node_modules"),
      },
      windowsHide: true,
      timeout: 90_000,
      maxBuffer: 8 * 1024 * 1024,
    });
    if (output.error) throw output.error;
    const artifacts = [
      join(stage, "app.keld.tar.zst.candidate"),
      join(stage, "signed-app.exe"),
      join(stage, "manifest.sig"),
    ].filter(existsSync);
    return {
      status: output.status,
      stdout: output.stdout,
      stderr: output.stderr.toString("utf8"),
      artifacts,
    };
  } finally {
    rmSync(outer, { recursive: true, force: true });
  }
}

runtimeTest(
  "real Bun projected runner emits the captured app and stamped transport despite ambient configuration",
  () => {
    assertOfficialCompiler(typescript!);
    const result = execute(
      [
        [
          "main.js",
          'import { answer } from "./value.js"; console.log(answer);',
        ],
        ["value.js", "export const answer = 45272;"],
      ],
      true,
    );
    expect(result.status, result.stderr).toBe(0);
    expect(result.stdout.subarray(0, 8).toString("utf8")).toBe("KELDCOT2");
    expect(result.stdout.subarray(8, 40)).toEqual(sha256(renderer));
    const appLength = result.stdout.readUInt32LE(40);
    const app = result.stdout.subarray(44, 44 + appLength).toString("utf8");
    expect(app).toContain("45272");
    const transportLength = result.stdout.readUInt32LE(44 + appLength);
    const transport = result.stdout.subarray(48 + appLength);
    expect(transport.length).toBe(transportLength);
    expect(transport.toString("utf8")).toContain(
      "@keld/kipc-transport sha256:",
    );
    expect(result.artifacts).toEqual([]);
  },
  120_000,
);

for (const [name, source] of [
  ["hostile ambient package", 'import "ambient-test-only";'],
  ["missing captured member", 'import "./outside.js";'],
  ["filesystem escape", 'import "node:fs";'],
] as const) {
  runtimeTest(
    "real projected runner refuses " + name + " with no output or signed file",
    () => {
      const result = execute([["main.js", source]], true);
      expect(result.status, result.stderr).toBe(1);
      expect(result.stdout.length).toBe(0);
      expect(result.stderr).toContain(
        "Captured application compiler refused input or output",
      );
      expect(result.artifacts).toEqual([]);
    },
    120_000,
  );
}

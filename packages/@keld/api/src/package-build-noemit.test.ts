import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import {
  appendFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, relative, resolve } from "node:path";

const packages = resolve(import.meta.dir, "../..");
const api = resolve(packages, "api");
const apiManifest = JSON.parse(
  readFileSync(join(api, "package.json"), "utf8"),
) as {
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
};
const apiLock = Bun.JSONC.parse(
  readFileSync(join(api, "bun.lock"), "utf8"),
) as {
  packages?: Record<string, [string, ...unknown[]]>;
};

function lockedVersion(name: string): string {
  const locked = apiLock.packages?.[name]?.[0];
  const prefix = `${name}@`;
  if (typeof locked !== "string" || !locked.startsWith(prefix))
    throw new Error(`API lock does not pin ${name}`);
  return locked.slice(prefix.length);
}

for (const name of ["typescript", "@types/bun"] as const) {
  const declared = apiManifest.devDependencies?.[name];
  if (
    typeof declared !== "string" ||
    !/^\d+\.\d+\.\d+$/.test(declared) ||
    declared !== lockedVersion(name)
  )
    throw new Error(`API manifest and lock must pin the same exact ${name}`);
}

function installedPackage(name: string, from: string): string {
  const manifest = realpathSync(Bun.resolveSync(`${name}/package.json`, from));
  const root = dirname(manifest);
  const version = (
    JSON.parse(readFileSync(manifest, "utf8")) as { version?: unknown }
  ).version;
  if (version !== lockedVersion(name))
    throw new Error(`Installed ${name} does not match API lock`);
  return root;
}

function optionalInstalledPackage(
  name: string,
  from: string,
): string | undefined {
  let resolved: string;
  try {
    resolved = Bun.resolveSync(`${name}/package.json`, from);
  } catch {
    return undefined;
  }
  const manifest = realpathSync(resolved);
  const root = dirname(manifest);
  const version = (
    JSON.parse(readFileSync(manifest, "utf8")) as { version?: unknown }
  ).version;
  if (version !== lockedVersion(name))
    throw new Error(`Installed ${name} does not match API lock`);
  return root;
}

function copyCompilerClosure(root: string): void {
  const nodeModules = join(root, "packages/@keld/api/node_modules");
  mkdirSync(nodeModules, { recursive: true });
  function copyPackage(name: string, from: string): string {
    const source = installedPackage(name, from);
    const destination = join(nodeModules, ...name.split("/"));
    mkdirSync(dirname(destination), { recursive: true });
    cpSync(source, destination, {
      recursive: true,
      dereference: true,
    });
    return source;
  }
  function copyOptionalPackage(name: string, from: string): boolean {
    const source = optionalInstalledPackage(name, from);
    if (source === undefined) return false;
    const destination = join(nodeModules, ...name.split("/"));
    mkdirSync(dirname(destination), { recursive: true });
    cpSync(source, destination, {
      recursive: true,
      dereference: true,
    });
    return true;
  }

  const typescript = copyPackage("typescript", api);
  const bunDeclarations = copyPackage("@types/bun", api);
  const bunTypes = copyPackage("bun-types", bunDeclarations);
  const nodeTypes = copyPackage("@types/node", bunTypes);
  copyPackage("undici-types", nodeTypes);

  const compilerManifest = JSON.parse(
    readFileSync(join(typescript, "package.json"), "utf8"),
  ) as { optionalDependencies?: Record<string, string> };
  let platformCompiler = 0;
  for (const name of Object.keys(compilerManifest.optionalDependencies ?? {})) {
    if (!name.startsWith("@typescript/typescript-")) continue;
    if (copyOptionalPackage(name, typescript)) platformCompiler += 1;
  }
  expect(platformCompiler).toBe(1);

  const fixtureKipc = join(root, "packages/@keld/kipc");
  const installedKipc = join(nodeModules, "@keld/kipc");
  expect(apiManifest.dependencies?.["@keld/kipc"]).toBe("link:../kipc");
  mkdirSync(join(nodeModules, "@keld"), { recursive: true });
  cpSync(fixtureKipc, installedKipc, {
    recursive: true,
    dereference: true,
    filter: (source) => basename(source) !== "node_modules",
  });
  expect(existsSync(join(nodeModules, "typescript/bin/tsc"))).toBe(true);
  expect(existsSync(join(nodeModules, "@types/bun/package.json"))).toBe(true);
  expect(existsSync(join(nodeModules, "@types/node/package.json"))).toBe(true);
  expect(existsSync(join(nodeModules, "bun-types/package.json"))).toBe(true);
  expect(existsSync(join(nodeModules, "undici-types/package.json"))).toBe(true);
  expect(existsSync(join(nodeModules, "@keld/kipc/package.json"))).toBe(true);
}

function fixture(check: (root: string) => void): void {
  const root = mkdtempSync(join(tmpdir(), "keld-noemit-"));
  let passed = false;
  try {
    for (const name of ["kipc", "api"]) {
      const destination = join(root, "packages/@keld", name);
      cpSync(join(packages, name), destination, {
        recursive: true,
        filter: (source) => basename(source) !== "node_modules",
      });
    }
    copyCompilerClosure(root);
    check(root);
    passed = true;
  } finally {
    if (passed) rmSync(root, { recursive: true, force: true });
    else console.error("NOEMIT_FAILED_FIXTURE " + root);
  }
}

function inventory(root: string): Record<string, string> {
  const values: Record<string, string> = {};
  function visit(directory: string): void {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      if (entry.name === "node_modules") continue;
      const path = join(directory, entry.name);
      if (entry.isDirectory()) visit(path);
      else if (entry.isFile())
        values[relative(root, path)] = createHash("sha256")
          .update(readFileSync(path))
          .digest("hex");
    }
  }
  visit(root);
  return values;
}

function refusal(root: string, command: string[], diagnostic: string): void {
  const before = inventory(root);
  const result = Bun.spawnSync(command, {
    cwd: root,
    stdout: "pipe",
    stderr: "pipe",
  });
  const output =
    new TextDecoder().decode(result.stdout) +
    new TextDecoder().decode(result.stderr);
  const after = inventory(root);
  // A failed oracle retains this owned fixture and its inventories. The fixture
  // owner removes successful cases after every assertion has completed.
  writeFileSync(
    join(root, "noemit-receipt.json"),
    JSON.stringify(
      { command, exit: result.exitCode, output, before, after },
      null,
      2,
    ),
  );
  expect(result.exitCode).not.toBe(0);
  expect(output).toContain(diagnostic);
  expect(after).toEqual(before);
}

test("package producer refuses mismatched pinned compiler manifests", () => {
  fixture((root) => {
    const kipcManifest = join(root, "packages/@keld/kipc/package.json");
    const manifest = JSON.parse(readFileSync(kipcManifest, "utf8"));
    manifest.devDependencies.typescript = "7.0.1";
    writeFileSync(kipcManifest, JSON.stringify(manifest, null, 2) + "\n");
    const output = join(root, "artifacts");
    mkdirSync(output);
    refusal(
      root,
      [
        process.execPath,
        join(root, "packages/@keld/kipc/scripts/package-artifacts.ts"),
        output,
      ],
      "API and KIPC must share the exact pinned TypeScript compiler closure",
    );
  });
});

test("package producer refuses a missing pinned compiler closure", () => {
  fixture((root) => {
    rmSync(join(root, "packages/@keld/api/node_modules/typescript"), {
      recursive: true,
      force: true,
    });
    const output = join(root, "artifacts");
    mkdirSync(output);
    refusal(
      root,
      [
        process.execPath,
        join(root, "packages/@keld/kipc/scripts/package-artifacts.ts"),
        output,
      ],
      "Pinned API TypeScript compiler closure is unavailable",
    );
  });
});

for (const seeded of [false, true]) {
  test(`failed API relative-edge build does not ${seeded ? "overwrite" : "create"} sibling files`, () => {
    fixture((root) => {
      const generated = join(
        root,
        "packages/@keld/api/src/dialog.generated.ts",
      );
      const original = readFileSync(generated, "utf8");
      expect(original.split('from "@keld/kipc";')).toHaveLength(2);
      writeFileSync(
        generated,
        original.replace(
          'from "@keld/kipc";',
          'from "../../kipc/src/transport.ts";',
        ),
      );
      if (seeded) {
        for (const suffix of ["js", "d.ts"])
          writeFileSync(
            join(root, "packages/@keld/kipc/src/transport." + suffix),
            "owned sentinel: failed compile must preserve these bytes\n",
          );
      }
      const output = join(root, "artifacts");
      mkdirSync(output);
      refusal(
        root,
        [
          process.execPath,
          join(root, "packages/@keld/kipc/scripts/package-artifacts.ts"),
          output,
        ],
        "TS6059",
      );
    });
  }, 15_000);
}

test("failed KIPC declaration build neither emits nor overwrites output", () => {
  fixture((root) => {
    appendFileSync(
      join(root, "packages/@keld/kipc/src/transport.ts"),
      '\nconst gh1374NoEmit: number = "must fail";\n',
    );
    const output = join(root, "output");
    mkdirSync(output);
    writeFileSync(
      join(output, "transport.d.ts"),
      "owned declaration sentinel\n",
    );
    refusal(
      root,
      [
        process.execPath,
        join(root, "packages/@keld/api/node_modules/typescript/bin/tsc"),
        "-p",
        join(root, "packages/@keld/kipc/tsconfig.build.json"),
        "--outDir",
        output,
        "--typeRoots",
        join(root, "packages/@keld/api/node_modules/@types"),
      ],
      "TS2322",
    );
  });
}, 15_000);

/** Test-only ownership of canonical KIPC source, worker and corpus. */
import {
  mkdirSync,
  lstatSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";
import { describe, expect, test } from "bun:test";

const REPO_ROOT = join(import.meta.dir, "../../../..");
const CORPUS_PATH = join(
  import.meta.dir,
  "../../../../crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv",
);
const CORPUS_SHA256 =
  "0cebb6e00c15a03028c6a29c725eb0e607ff66ee1cae04e227d18b9e49d0213e";
const SKIP_DIR_NAMES = new Set([".git", "node_modules", "target"]);
const SKIP_REPO_DIRS = new Set([
  join(REPO_ROOT, ".keld-work"),
  join(REPO_ROOT, "competitors"),
  join(REPO_ROOT, "docs", "research"),
]);

function skipDirectory(path: string, name: string): boolean {
  return SKIP_DIR_NAMES.has(name) || SKIP_REPO_DIRS.has(path);
}

function walkFiles(root: string, suffix: string, into: string[]): void {
  for (const name of readdirSync(root)) {
    const path = join(root, name);
    const stat = lstatSync(path);
    if (stat.isSymbolicLink()) continue;
    if (stat.isDirectory()) {
      if (skipDirectory(path, name)) continue;
      walkFiles(path, suffix, into);
      continue;
    }
    if (name.endsWith(suffix)) into.push(path);
  }
}

describe("one source / no second copy", () => {
  test("repository walks skip only owned reference roots and directory symlinks", () => {
    const root = mkdtempSync(join(tmpdir(), "keld-kipc-walk-"));
    try {
      const source = join(root, "source");
      const target = join(root, "target");
      mkdirSync(source);
      mkdirSync(target);
      writeFileSync(join(target, "linked.ts"), "export {};\n");
      symlinkSync(
        target,
        join(source, "linked"),
        process.platform === "win32" ? "junction" : "dir",
      );
      for (const skipped of ["node_modules", "target"]) {
        mkdirSync(join(source, skipped));
        writeFileSync(join(source, skipped, `${skipped}.ts`), "export {};\n");
      }
      writeFileSync(join(source, "owned.ts"), "export {};\n");
      const files: string[] = [];
      walkFiles(source, ".ts", files);
      expect(
        files.map((file) => relative(source, file).replaceAll("\\", "/")),
      ).toEqual(["owned.ts"]);
      expect(skipDirectory(join(REPO_ROOT, "competitors"), "competitors")).toBe(
        true,
      );
      expect(skipDirectory(join(REPO_ROOT, ".keld-work"), ".keld-work")).toBe(
        true,
      );
      expect(
        skipDirectory(join(REPO_ROOT, "packages", ".keld-work"), ".keld-work"),
      ).toBe(false);
      expect(
        skipDirectory(join(REPO_ROOT, "docs", "research"), "research"),
      ).toBe(true);
      expect(
        skipDirectory(
          join(REPO_ROOT, "packages", "@keld", "example", "research"),
          "research",
        ),
      ).toBe(false);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("MAGIC_BYTES and class FrameReader exist only in the canonical transport", () => {
    const files: string[] = [];
    walkFiles(join(REPO_ROOT, "packages"), ".ts", files);
    walkFiles(join(REPO_ROOT, "crates/keld-cli/templates"), ".ts", files);
    const magicOwners: string[] = [];
    const readerOwners: string[] = [];
    for (const file of files) {
      if (file.endsWith(".test.ts")) continue;
      const text = readFileSync(file, "utf8");
      if (text.includes("const MAGIC_BYTES = new Uint8Array([0x4b, 0x49])")) {
        magicOwners.push(relative(REPO_ROOT, file).replaceAll("\\", "/"));
      }
      if (text.includes("export class FrameReader")) {
        readerOwners.push(relative(REPO_ROOT, file).replaceAll("\\", "/"));
      }
    }
    expect(magicOwners).toEqual(["packages/@keld/kipc/src/transport.ts"]);
    expect(readerOwners).toEqual(["packages/@keld/kipc/src/transport.ts"]);
  });

  // GH-527 §4.13 removal check: the main-thread client path is gone. The one
  // production `connectKipcSocket` call is the transport Worker's, and no
  // `DirectedReader` remains anywhere.
  test("the transport Worker is the only production connectKipcSocket caller", () => {
    const files: string[] = [];
    walkFiles(join(REPO_ROOT, "packages"), ".ts", files);
    walkFiles(join(REPO_ROOT, "crates/keld-cli/templates"), ".ts", files);
    const callers: string[] = [];
    const directed: string[] = [];
    for (const file of files) {
      const text = readFileSync(file, "utf8");
      const path = relative(REPO_ROOT, file).replaceAll("\\", "/");
      if (text.includes("DirectedReader")) directed.push(path);
      if (file.endsWith(".test.ts")) continue;
      for (const match of text.matchAll(
        /(?<!function )\bconnectKipcSocket\(/g,
      )) {
        const owner = text.lastIndexOf("\nclass ", match.index);
        const ownerName =
          owner < 0 ? "" : (/class (\w+)/.exec(text.slice(owner))?.[1] ?? "");
        callers.push(`${path}:${ownerName}`);
      }
    }
    expect(callers).toEqual([
      "packages/@keld/kipc/src/transport.ts:TransportWorker",
    ]);
    expect(
      directed.filter(
        (path) =>
          path !== "packages/@keld/kipc/src/transport-source-ownership.test.ts",
      ),
    ).toEqual([]);
  });

  test("hello scaffold shim is a re-export, not a second implementation", () => {
    const shim = readFileSync(
      join(REPO_ROOT, "crates/keld-cli/templates/hello/src/kipc-transport.ts"),
      "utf8",
    );
    expect(shim).toContain(
      'export * from "../../../../../packages/@keld/kipc/src/transport.ts"',
    );
    expect(shim).not.toContain("export class FrameReader");
    expect(shim).not.toContain("const MAGIC_BYTES");
  });

  test("canonical corpus file is the one TSV owner; consumers do not copy rows", () => {
    const tsvFiles: string[] = [];
    walkFiles(REPO_ROOT, ".tsv", tsvFiles);
    const corpus = tsvFiles
      .map((file) => relative(REPO_ROOT, file).replaceAll("\\", "/"))
      .filter((file) => file.endsWith("receiver-semantics-v0.tsv"));
    expect(corpus).toEqual([
      "crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv",
    ]);
    const bytes = readFileSync(CORPUS_PATH);
    const digest = new Bun.CryptoHasher("sha256").update(bytes).digest("hex");
    expect(digest).toBe(CORPUS_SHA256);
  });

  test("public TypeScript sources do not use any", () => {
    const files: string[] = [];
    walkFiles(join(REPO_ROOT, "packages/@keld/kipc/src"), ".ts", files);
    walkFiles(join(REPO_ROOT, "packages/@keld/electron/src"), ".ts", files);
    const hits: string[] = [];
    for (const file of files) {
      if (file.endsWith(".test.ts")) continue;
      const text = readFileSync(file, "utf8");
      if (/(?::|\bas\b|<)\s*any\b/.test(text) || /\bany\s*[\[<|]/.test(text)) {
        hits.push(relative(REPO_ROOT, file).replaceAll("\\", "/"));
      }
    }
    expect(hits).toEqual([]);
  });
});

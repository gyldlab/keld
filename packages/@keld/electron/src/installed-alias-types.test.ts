import { expect, test } from "bun:test";
import {
  linkSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { assertInstalledTypeFilesWithinRoots } from "../fixtures/installed-alias-types.ts";

test("installed type graph admits aliased roots but rejects sibling and junction escapes", () => {
  const scratch = mkdtempSync(join(tmpdir(), "keld-installed-type-path-"));
  try {
    const consumer = join(scratch, "consumer");
    const alias = join(scratch, "CONSUMER~1");
    const compilerLib = join(scratch, "compiler", "lib");
    const outside = join(scratch, "outside");
    const sibling = join(scratch, "consumer-sibling");
    const nodeType = join(
      consumer,
      "node_modules",
      "@types",
      "node",
      "index.d.ts",
    );
    const compilerType = join(compilerLib, "lib.es2024.d.ts");
    const escapedType = join(outside, "escape.d.ts");
    const siblingType = join(sibling, "sibling.d.ts");
    const bunType = join(
      consumer,
      "node_modules",
      "@types",
      "bun",
      "index.d.ts",
    );
    for (const file of [
      nodeType,
      compilerType,
      escapedType,
      siblingType,
      bunType,
    ]) {
      mkdirSync(dirname(file), { recursive: true });
      writeFileSync(file, "export {};\n");
    }

    // On Windows a junction is an unprivileged real filesystem alias. The
    // hosted installed-package test additionally supplies its actual 8.3 path.
    const linkType = process.platform === "win32" ? "junction" : "dir";
    symlinkSync(consumer, alias, linkType);
    symlinkSync(outside, join(consumer, "outside-link"), linkType);
    symlinkSync(outside, join(compilerLib, "outside-link"), linkType);
    linkSync(nodeType, join(sibling, "same-file-hardlink.d.ts"));

    expect(() =>
      assertInstalledTypeFilesWithinRoots(
        [
          join(alias, "node_modules", "@types", "node", "index.d.ts"),
          compilerType,
        ],
        alias,
        compilerLib,
      ),
    ).not.toThrow();
    expect(() =>
      assertInstalledTypeFilesWithinRoots([nodeType], consumer, compilerLib),
    ).not.toThrow();

    // Each path below exists; denial proves physical confinement rather
    // than a missing-file error or a string-prefix comparison.
    for (const forbidden of [
      siblingType,
      join(sibling, "same-file-hardlink.d.ts"),
      join(alias, "..", "outside", "escape.d.ts"),
      join(alias, "outside-link", "escape.d.ts"),
      join(compilerLib, "outside-link", "escape.d.ts"),
    ]) {
      expect(() =>
        assertInstalledTypeFilesWithinRoots([forbidden], alias, compilerLib),
      ).toThrow(/Compiler escaped the installed graph/);
    }
    expect(() =>
      assertInstalledTypeFilesWithinRoots([bunType], alias, compilerLib),
    ).toThrow(/Compiler loaded Bun declarations/);
    expect(() =>
      assertInstalledTypeFilesWithinRoots(
        [join(consumer, "missing.d.ts")],
        alias,
        compilerLib,
      ),
    ).toThrow();
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});

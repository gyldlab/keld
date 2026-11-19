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
    const nestedCaseType = join(
      consumer,
      "node_modules",
      "dep-case",
      "node_modules",
      "@types",
      "BUN",
      "index.d.ts",
    );
    const nestedBunTypes = join(
      consumer,
      "node_modules",
      "dep-types",
      "node_modules",
      "BuN-tYpEs",
      "index.d.ts",
    );
    const vendorRoot = join(consumer, "vendor", "types");
    const vendorType = join(vendorRoot, "index.d.ts");
    const aliasRoot = join(
      consumer,
      "node_modules",
      "dep-alias",
      "node_modules",
    );
    const nestedAlias = join(aliasRoot, "@types", "bun");
    const typesAlias = join(aliasRoot, "bun-types");
    const physicalAlias = join(aliasRoot, "neutral-types");
    for (const file of [
      nodeType,
      compilerType,
      escapedType,
      siblingType,
      bunType,
      nestedCaseType,
      nestedBunTypes,
      vendorType,
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
    mkdirSync(dirname(nestedAlias), { recursive: true });
    symlinkSync(vendorRoot, nestedAlias, linkType);
    symlinkSync(vendorRoot, typesAlias, linkType);
    symlinkSync(dirname(nestedCaseType), physicalAlias, linkType);

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
      assertInstalledTypeFilesWithinRoots(
        [nodeType, vendorType],
        consumer,
        compilerLib,
      ),
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
    // Both transitive spelling variants are inside the physical consumer,
    // and both junctions resolve to an otherwise allowed vendor file.
    for (const forbidden of [
      bunType,
      nestedCaseType,
      nestedBunTypes,
      join(nestedAlias, "index.d.ts"),
      join(typesAlias, "index.d.ts"),
      join(physicalAlias, "index.d.ts"),
    ]) {
      expect(() =>
        assertInstalledTypeFilesWithinRoots([forbidden], alias, compilerLib),
      ).toThrow(/Compiler loaded Bun declarations/);
    }
    if (process.platform === "win32") {
      expect(() =>
        assertInstalledTypeFilesWithinRoots(
          [nestedCaseType.replaceAll("\\", "/")],
          alias,
          compilerLib,
        ),
      ).toThrow(/Compiler loaded Bun declarations/);
    }
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

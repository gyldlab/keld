/**
 * Exact-byte Host-owned installed original-graph bootstrap.
 *
 * Pack generates this file at the app root from bytes compiled into Core.
 * Installed Host/Guardian verify those bytes against retained no-follow
 * handles before selecting this module as a pre-entry preload. Bun runs the
 * independently admitted original as its truthful main only after this
 * authenticated preload completes.
 */
import { basename, dirname, isAbsolute, join } from "node:path";
import { realpathSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
// Only the fixed installed provider files under the closed, verified graph
// may supply bootstrap authority. A user package with a matching self-name or
// an app-root exports map cannot redirect these imports before authentication.
import { bootHostSelectedEntry } from "./node_modules/@keld/api/app.js";
import {
  linkError,
  runtimeProcessFactsSelected,
} from "./node_modules/@keld/kipc/transport.js";

const selectedMain = process.argv[1];
const sealedBootstrap = fileURLToPath(import.meta.url);
// Core compiled this relative path into the signed app-root file. It is the
// sole original-entry selector; argv[1] is Bun's normal original-main slot.
// No app-private path may be passed as an additional user argument.
const sealedOriginalRelative = "__KELD_SELECTED_ENTRY__";
// The Host's fixed launch selector is required *before* filesystem reads.
// This shape is a secondary check: authority remains the signed boot,
// retained no-follow graph and Host-minted app-link, not mutable CLI argv.
const shapeError =
  import.meta.main || !runtimeProcessFactsSelected
    ? "not-sealed-preload"
    : process.argv.length !== 2
      ? "unexpected-argv-arity"
      : typeof selectedMain !== "string" ||
          !isAbsolute(selectedMain) ||
          basename(sealedBootstrap) !== "keld-installed-runtime-entry.mjs"
        ? "unexpected-main-path"
        : undefined;
if (shapeError) {
  throw linkError(
    "KELD-IPC-005",
    `Installed original entry must be one Host-admitted app-root member (${shapeError}).`,
  );
}

// Refuse malformed generated owner source before any user module import. Core
// already enforces this invariant when rendering the sealed literal, and the
// Guardian validates the corresponding boot/graph member before Bun starts.
if (
  !/^[A-Za-z0-9/._-]+$/.test(sealedOriginalRelative) ||
  sealedOriginalRelative
    .split(/[\\/]/)
    .some((part) => part === "" || part === "." || part === "..")
) {
  throw linkError(
    "KELD-IPC-005",
    "Sealed original entry has an invalid relative source path.",
  );
}

// Bun's Windows 8.3 short names and macOS /var vs /private/var aliases can
// describe the SAME physical directory using different lexical prefixes.
// Resolve the literal from the signed app root, then require that its physical
// parent chain contains the signed bootstrap directory inode. This preserves
// symlink-escape refusal without relying on mutable argv path spelling.
let canonicalRoot;
let canonicalEntry;
try {
  canonicalRoot = dirname(realpathSync(sealedBootstrap));
  canonicalEntry = realpathSync(join(canonicalRoot, sealedOriginalRelative));
} catch {
  throw linkError("KELD-IPC-005", "Host-admitted entry cannot be resolved.");
}
let withinRoot = false;
try {
  const rootIdentity = statSync(canonicalRoot, { bigint: true });
  const targetIdentity = statSync(canonicalEntry, { bigint: true });
  const mainIdentity = statSync(selectedMain, { bigint: true });
  const bunMainIdentity = statSync(Bun.main, { bigint: true });
  if (!targetIdentity.isFile() || rootIdentity.ino === 0n) {
    throw new Error("filesystem cannot establish an original entry identity");
  }
  if (
    targetIdentity.dev !== mainIdentity.dev ||
    targetIdentity.ino !== mainIdentity.ino ||
    targetIdentity.dev !== bunMainIdentity.dev ||
    targetIdentity.ino !== bunMainIdentity.ino
  ) {
    throw new Error("Bun main is not the sealed original entry");
  }
  let ancestor = dirname(canonicalEntry);
  for (let depth = 0; depth < 64; depth++) {
    const identity = statSync(ancestor, { bigint: true });
    if (
      identity.ino !== 0n &&
      identity.dev === rootIdentity.dev &&
      identity.ino === rootIdentity.ino
    ) {
      withinRoot = true;
      break;
    }
    const next = dirname(ancestor);
    if (next === ancestor) break;
    ancestor = next;
  }
} catch {
  // Any unsupported or ambiguous filesystem identity is a refusal, not a
  // reason to fall back to lexical checks or an unverified import.
}
if (!withinRoot) {
  throw linkError(
    "KELD-IPC-005",
    "Host-selected original entry is outside its app root.",
  );
}

// Only the signed Host selects this preload. Bun evaluates the original entry
// as its real main AFTER this await, preserving truthful import.meta.main and
// Bun.main, while keeping the existing one-link Init ACK/subscription gate.
await bootHostSelectedEntry(async () => undefined);

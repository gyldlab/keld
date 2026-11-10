/**
 * Exact-byte Host-owned installed original-graph bootstrap.
 *
 * Pack generates this file at the app root from bytes compiled into Core.
 * Installed Host/Guardian verify those bytes against retained no-follow
 * handles before selecting this module as Bun's *one* entry. The original
 * entry path comes only from the independently admitted boot descriptor.
 */
import { basename, dirname, isAbsolute } from "node:path";
import { realpathSync, statSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { bootHostSelectedEntry } from "@keld/api/internal/app";
import { linkError, runtimeProcessFactsSelected } from "@keld/kipc";

const selectedMain = process.argv[1];
const selectedEntry = process.argv[2];
// The Host's fixed launch selector is required *before* filesystem reads.
// This shape is a secondary check: authority remains the signed boot,
// retained no-follow graph and Host-minted app-link, not mutable CLI argv.
const shapeError =
  !import.meta.main || !runtimeProcessFactsSelected
    ? "not-main"
    : process.argv.length !== 3
      ? "unexpected-argv-arity"
      : typeof selectedMain !== "string" ||
          !isAbsolute(selectedMain) ||
          basename(selectedMain) !== "keld-installed-runtime-entry.mjs"
        ? "unexpected-main-path"
        : typeof selectedEntry !== "string" || !isAbsolute(selectedEntry)
          ? "nonabsolute-entry"
          : undefined;
if (shapeError) {
  throw linkError(
    "KELD-IPC-005",
    `Installed original entry must be one Host-admitted app-root member (${shapeError}).`,
  );
}

// Pinned Bun 1.4.4 does not resolve literal '?' filename components even via
// a correctly escaped file: URL. An original graph containing this spelling
// must fail closed *before* establishing Host IPC; never accidentally import
// a URL query or a different app module. Pack/original naming compatibility
// review remains required if this constraint is widened in a later runtime.
if (selectedEntry.includes("?")) {
  throw linkError(
    "KELD-IPC-005",
    "Host-selected original entry uses a path unsupported by the pinned Bun runtime.",
  );
}

// Never accept a lexical traversal component in the Host-selected original
// pathname. The Host also independently validates this member from retained,
// no-follow boot+graph files before it starts the role.
if (
  selectedEntry.split(/[\\/]/).some((part) => part === "." || part === "..")
) {
  throw linkError(
    "KELD-IPC-005",
    "Host-selected original entry contains a traversal component.",
  );
}

// Bun's Windows 8.3 short names and macOS /var vs /private/var aliases can
// describe the SAME physical directory using different lexical prefixes.
// Resolve both paths, then require that the original entry's physical parent
// chain includes the exact signed bootstrap directory inode. This preserves
// symlink-escape refusal without relying on path spelling.
let canonicalRoot;
let canonicalEntry;
try {
  canonicalRoot = dirname(realpathSync(selectedMain));
  canonicalEntry = realpathSync(selectedEntry);
} catch {
  throw linkError("KELD-IPC-005", "Host-admitted entry cannot be resolved.");
}
let withinRoot = false;
try {
  const rootIdentity = statSync(canonicalRoot, { bigint: true });
  const targetIdentity = statSync(canonicalEntry, { bigint: true });
  const mainIdentity = statSync(selectedMain, { bigint: true });
  if (!targetIdentity.isFile() || rootIdentity.ino === 0n) {
    throw new Error("filesystem cannot establish an original entry identity");
  }
  if (
    targetIdentity.dev === mainIdentity.dev &&
    targetIdentity.ino === mainIdentity.ino
  ) {
    throw new Error("original entry cannot be the runtime bootstrap itself");
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

// Import from this *app-root module graph*, not from CLI --preload's isolated
// module cache. Never evaluate user code before a genuine authenticated Init
// ACK plus subscriptions. Ready still comes exclusively from the Host.
const originalUrl = pathToFileURL(canonicalEntry).href;
await bootHostSelectedEntry(() => import(originalUrl));

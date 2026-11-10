/**
 * Exact-byte Host-owned installed original-graph bootstrap.
 *
 * Pack generates this file at the app root from bytes compiled into Core.
 * Installed Host/Guardian verify those bytes against retained no-follow
 * handles before selecting this module as Bun's *one* entry. The original
 * entry path comes only from the independently admitted boot descriptor.
 */
import { dirname, isAbsolute, relative, sep } from "node:path";
import { realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { bootHostSelectedEntry } from "@keld/api/internal/app";
import { linkError, runtimeProcessFactsSelected } from "@keld/kipc";

const selectedEntry = process.argv[2];
// The Host's fixed launch selector is required *before* filesystem reads.
// This shape is a secondary check: authority remains the signed boot,
// retained no-follow graph and Host-minted app-link, not mutable CLI argv.
const shapeError =
  !import.meta.main || !runtimeProcessFactsSelected
    ? "not-main"
    : process.argv.length !== 3
      ? "unexpected-argv-arity"
      : typeof selectedEntry !== "string" || !isAbsolute(selectedEntry)
        ? "nonabsolute-entry"
        : undefined;
if (shapeError) {
  throw linkError(
    "KELD-IPC-005",
    `Installed original entry must be one Host-admitted app-root member (${shapeError}).`,
  );
}

// Bun may canonicalize macOS /var to /private/var or case-fold a Win32
// drive. Compare physical identities, not spelling, while still rejecting
// symlinks outside the signed application root. Core independently checked
// these files through retained no-follow handles before running the child.
let appRoot;
let canonicalEntry;
try {
  appRoot = realpathSync(dirname(fileURLToPath(import.meta.url)));
  canonicalEntry = realpathSync(selectedEntry);
} catch {
  throw linkError(
    "KELD-IPC-005",
    "Host-admitted entry is not a regular resolved member.",
  );
}
const selectedRelative = relative(appRoot, canonicalEntry);
if (
  !selectedRelative ||
  selectedRelative === ".." ||
  selectedRelative.startsWith(`..${sep}`) ||
  isAbsolute(selectedRelative)
) {
  throw linkError(
    "KELD-IPC-005",
    "Host-selected original entry is outside its app root.",
  );
}

// Import from this *app-root module graph*, not from CLI --preload's isolated
// module cache. Never evaluate user code before a genuine authenticated Init
// ACK plus subscriptions. Ready still comes exclusively from the Host.
const specifier = `./${selectedRelative.split(sep).join("/")}`;
await bootHostSelectedEntry(() => import(specifier));

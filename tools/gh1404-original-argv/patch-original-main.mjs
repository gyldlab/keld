/**
 * Proposed source-only correction for the pinned draw.io original migration.
 * The actual migrated application is an independently approved external input;
 * this module is not a KELD runtime or Pack hook.
 *
 * The signed original entry checks physical identities and authenticates the
 * Host IPC link before importing original electron.js. Its fixed argv[2] is
 * an internal selected source, never evidence of user open-file intent.
 */
export function originalParserArgv(
  rawArgv,
  defaultApp,
  runtimeSelected,
  hostFacts,
) {
  if (!Array.isArray(rawArgv)) {
    throw new Error("KELD-IPC-005: original process arguments are unavailable");
  }
  if (runtimeSelected === true) {
    // The Host's immutable runtime selection MUST NOT be folded into a
    // boolean with decoded launch facts. A missing/mismatched schema-7
    // record must refuse BEFORE upstream draw.io parses any user file.
    if (
      hostFacts?.schema !== 7 ||
      hostFacts.kind !== "local-development" ||
      hostFacts.originalResourcePolicy?.contract !==
        "gh473-original-editor-offline/v1"
    ) {
      throw new Error(
        "KELD-IPC-005: selected original requires matching schema-7 Host launch facts",
      );
    }
    if (
      rawArgv.length !== 3 ||
      typeof rawArgv[0] !== "string" ||
      typeof rawArgv[1] !== "string" ||
      typeof rawArgv[2] !== "string"
    ) {
      throw new Error(
        "KELD-IPC-005: Host-selected original arguments are malformed",
      );
    }
    // [Bun, fixed bootstrap, sealed original], with NO CLI user operands.
    // The signed Host refuses argc!=3 before evaluating the original.
    // File intent remains with existing native open-file / picker owners.
    // Never promote a mutable argv tail into a user-authorized file.
    return [rawArgv[0], null];
  }
  // An invalid selection or a schema-7 installed original without that
  // selection is a mismatch, not permission to parse the signed entry.
  if (runtimeSelected !== false || hostFacts?.schema === 7) {
    throw new Error(
      "KELD-IPC-005: original runtime selection and Host facts disagree",
    );
  }
  // Preserve ordinary original/Electron parsing without changing process.argv.
  return defaultApp === true ? [...rawArgv] : [null, ...rawArgv];
}

const SOURCE_IMPORT =
  "import { parseDrawioArgs, formatHelp, validFormatRegExp as validFormatRegExpImport } from './args.js';";
const LEGACY_ARGV = [
  "    let argv = process.argv",
  "",
  "    // https://github.com/electron/electron/issues/4690#issuecomment-217435222",
  "    if (process.defaultApp != true)",
  "    {",
  "        argv.unshift(null)",
  "    }",
].join("\n");
const SELECTED_ARGV = [
  "    // Runtime selection and decoded Host facts must agree; missing or",
  "    // mismatched facts refuse before any draw.io file-argument parsing.",
  "    // The signed Host independently verified the entry and Init ACK.",
  "    let argv = originalParserArgv(",
  "        process.argv, process.defaultApp, runtimeProcessFactsSelected, hostLaunchFacts);",
].join("\n");

function replaceExactlyOnce(source, oldText, newText, label) {
  const first = source.indexOf(oldText);
  if (first < 0 || source.indexOf(oldText, first + oldText.length) >= 0) {
    throw new Error(
      "KELD-IPC-005: expected one " +
        label +
        " in independently qualified original source",
    );
  }
  return (
    source.slice(0, first) + newText + source.slice(first + oldText.length)
  );
}

/** Produce a proposed patch, never write or auto-select an original source. */
export function patchOriginalMainText(source) {
  if (typeof source !== "string") {
    throw new Error("KELD-IPC-005: original source is not text");
  }
  let patched = replaceExactlyOnce(
    source,
    LEGACY_ARGV,
    SELECTED_ARGV,
    "original parser anchor",
  );
  patched = replaceExactlyOnce(
    patched,
    SOURCE_IMPORT,
    SOURCE_IMPORT +
      "\nimport { hostLaunchFacts, runtimeProcessFactsSelected } from '@keld/kipc';",
    "original args import anchor",
  );
  patched = replaceExactlyOnce(
    patched,
    "let store;",
    originalParserArgv.toString() + "\n\nlet store;",
    "original process helper insertion",
  );
  return patched;
}

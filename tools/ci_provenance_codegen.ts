import { readFileSync, writeFileSync } from "node:fs";

const begin = "            // __IMMUTABLE_CHANGED_PATHS_BEGIN__";
const end = "            // __IMMUTABLE_CHANGED_PATHS_END__";
const owner = ".github/workflows/ci-provenance.yml";
const output = ".github/workflows/ci-verisentry-refresh.yml";

function range(source: string): [number, number] {
  const start = source.indexOf(begin);
  const stop = source.indexOf(end);
  if (
    start < 0 ||
    stop <= start ||
    source.indexOf(begin, start + begin.length) >= 0 ||
    source.indexOf(end, stop + end.length) >= 0
  ) {
    throw new Error("Expected one complete immutable census block");
  }
  return [start, stop + end.length];
}

// Generation is a development operation, never a privileged workflow step.
// The automatic verifier owns the policy; the refresh workflow embeds its bytes.
export function generateRefreshCensus(
  canonical: string,
  refresh: string,
): string {
  const [sourceStart, sourceEnd] = range(canonical);
  const [targetStart, targetEnd] = range(refresh);
  return (
    refresh.slice(0, targetStart) +
    canonical.slice(sourceStart, sourceEnd) +
    refresh.slice(targetEnd)
  );
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  if (args.length > 1 || (args.length === 1 && args[0] !== "--write")) {
    throw new Error("Usage: bun tools/ci_provenance_codegen.ts [--write]");
  }
  const current = readFileSync(output, "utf8");
  const generated = generateRefreshCensus(readFileSync(owner, "utf8"), current);
  if (args[0] === "--write") {
    if (generated !== current) writeFileSync(output, generated);
  } else if (generated !== current) {
    throw new Error(
      "Stale census output: bun tools/ci_provenance_codegen.ts --write",
    );
  }
}

#!/bin/bash
# 0) baseline: plain `bun src/main/electron.js` (no harness) to capture the exact import error,
# 1) unpackaged dev-tree variant, 2) packaged-like primary (last, so fakehome/ and fixtures/ on disk
# reflect the primary trace), 3) summary.
D="$(cd "$(dirname "$0")" && pwd)"
P3="$(cd "$D/.." && pwd)"
rm -rf "$P3/fakehome"; mkdir -p "$P3/fakehome"
( cd "$P3/drawio-desktop" && HOME="$P3/fakehome" BUN_RUNTIME_TRANSPILER_CACHE_PATH=0 \
  perl -e 'alarm 60; exec @ARGV' bun --no-install src/main/electron.js ) > "$D/baseline-error.txt" 2>&1
echo "baseline exit=$? (expected non-zero)"
"$D/run.sh" unpackaged 2>&1 | tail -3
"$D/run.sh" packaged 2>&1 | tail -3
HOME="$P3/fakehome" bun --no-install "$D/summarize.mjs"

#!/bin/bash
# Usage: harness/run.sh [packaged|unpackaged]   (default: packaged)
# Everything this touches is under p3/. No network: Bun auto-install is disabled and the harness
# refuses every outbound call it can see (and records it).
set -u
VARIANT="${1:-packaged}"
P3="$(cd "$(dirname "$0")/.." && pwd)"
cd "$P3" || exit 1
if [ "$VARIANT" = "unpackaged" ]; then PK=0; SUF=".unpackaged"; else PK=1; SUF=""; fi
rm -rf "$P3/fakehome" "$P3/trace$SUF.jsonl" "$P3/harness/run-result$SUF.json"
mkdir -p "$P3/fakehome"
rm -f "$P3"/fixtures/saved-as.drawio "$P3"/fixtures/export-out.drawio "$P3"/fixtures/.\$* "$P3"/fixtures/unblessed.drawio
# Fixtures come from an untraced bun run (no preload).
HOME="$P3/fakehome" BUN_RUNTIME_TRANSPILER_CACHE_PATH=0 bun --no-install "$P3/harness/setup-fixtures.mjs" || exit 1
cd "$P3/drawio-desktop" || exit 1
HOME="$P3/fakehome" BUN_RUNTIME_TRANSPILER_CACHE_PATH=0 HARNESS_PACKAGED="$PK" \
  perl -e 'alarm 180; exec @ARGV' bun --no-install --preload "$P3/harness/preload.cjs" "$P3/harness/run.mjs"
echo "bun exit=$? variant=$VARIANT"

#!/usr/bin/env bash
# Install reviewed checkout reminder hooks and Bun formatting pre-commit (`just hooks-install`).
# The one copy: the justfile recipe and tools/hooks_test.sh both run this
# script, so the hook test needs no `just` executable (#650).
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
COMMON_DIR="$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir)"
HOOKS_DIR="$COMMON_DIR/keld-hooks"
mkdir -p "$HOOKS_DIR"
cp -- "$ROOT/.githooks/post-merge" "$HOOKS_DIR/post-merge"
cp -- "$ROOT/.githooks/post-checkout" "$HOOKS_DIR/post-checkout"
cp -- "$ROOT/.githooks/pre-commit" "$HOOKS_DIR/pre-commit"
chmod +x "$HOOKS_DIR/post-merge" "$HOOKS_DIR/post-checkout" "$HOOKS_DIR/pre-commit"
git -C "$ROOT" config core.hooksPath "$HOOKS_DIR"
echo "hooks-install: installed reviewed reminder and Bun format-check hooks at $HOOKS_DIR (local)."
echo "hooks-install: checkout/merge will not execute working-tree code."

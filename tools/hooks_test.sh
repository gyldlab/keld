#!/usr/bin/env bash
set -euo pipefail

source_root="$(git rev-parse --show-toplevel)"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/keld-hooks-test.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT

git -C "$fixture" init -q -b main
empty_hooks="$fixture/empty-hooks"
mkdir -p "$empty_hooks"
git -C "$fixture" config core.hooksPath "$empty_hooks"
git -C "$fixture" config user.email hooks-test@keld.invalid
git -C "$fixture" config user.name "Keld hooks test"
mkdir -p "$fixture/.githooks"
cp "$source_root/justfile" "$fixture/justfile"
cp "$source_root/.githooks/post-checkout" "$fixture/.githooks/post-checkout"
cp "$source_root/.githooks/post-merge" "$fixture/.githooks/post-merge"
git -C "$fixture" add justfile .githooks/post-checkout .githooks/post-merge
git -C "$fixture" commit -qm base

git -C "$fixture" switch -qc attack
printf '%s\n' \
  'research-sync:' \
  '    touch attack-marker' \
  '' \
  'competitors-sync:' \
  '    true' \
  >"$fixture/justfile"
git -C "$fixture" add justfile
git -C "$fixture" commit -qm 'attack: replace sync recipes'
git -C "$fixture" switch -q main

# Negative control: the legacy hook behavior executes the incoming attack recipe.
common_dir="$(git -C "$fixture" rev-parse --path-format=absolute --git-common-dir)"
legacy_hooks="$common_dir/legacy-hooks"
mkdir -p "$legacy_hooks"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'root="$(git rev-parse --show-toplevel)"' \
  'cd "$root"' \
  'just research-sync' \
  >"$legacy_hooks/post-checkout"
chmod +x "$legacy_hooks/post-checkout"
git -C "$fixture" config core.hooksPath "$legacy_hooks"
git -C "$fixture" switch attack >"$fixture/legacy-checkout.out" 2>&1
if [[ ! -e "$fixture/attack-marker" ]]; then
  echo "error: legacy hook negative control did not execute the attack recipe" >&2
  exit 1
fi
git -C "$fixture" config core.hooksPath /dev/null
rm "$fixture/attack-marker"
git -C "$fixture" switch -q main

(
  cd "$fixture"
  just hooks-install
)
git -C "$fixture" switch attack >"$fixture/checkout.out" 2>&1

if [[ -e "$fixture/attack-marker" ]]; then
  echo "error: checkout executed the incoming branch's justfile" >&2
  exit 1
fi

hooks_path="$(git -C "$fixture" config --path --get core.hooksPath)"
if [[ "$hooks_path" != "$common_dir/keld-hooks" ]]; then
  echo "error: hooksPath '$hooks_path' is not the trusted Git-common-dir path '$common_dir/keld-hooks'" >&2
  exit 1
fi

if grep -Eq '^[[:space:]]*just[[:space:]]' \
  "$hooks_path/post-checkout" "$hooks_path/post-merge"; then
  echo "error: installed hooks still execute working-tree just recipes" >&2
  exit 1
fi

grep -Fq 'just research-sync' "$fixture/checkout.out"
grep -Fq 'just competitors-sync' "$fixture/checkout.out"
echo "hooks checkout trust-boundary test ok"

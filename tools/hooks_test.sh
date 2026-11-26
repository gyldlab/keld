#!/usr/bin/env bash
set -euo pipefail

source_root="$(git rev-parse --show-toplevel)"
# The hooks under test (and the legacy negative control) execute `just`, so the
# real executable is required; a missing one is a failure, never a skip (#650).
if ! command -v just >/dev/null 2>&1; then
  echo "HOOKS-TEST: the \`just\` executable is not installed. Install just and rerun \`just hooks-test\`; this test is local-only until CI pins a just (dependency-gate decision, not taken)." >&2
  exit 1
fi
if ! command -v bun >/dev/null 2>&1; then
  echo "HOOKS-TEST: Bun is required to exercise the installed pre-commit formatter; no test may be skipped." >&2
  exit 1
fi
fixture="$(mktemp -d "${TMPDIR:-/tmp}/keld-hooks-test.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT

git -C "$fixture" init -q -b main
# No detached `git maintenance` child may outlive a command and race cleanup (#670).
git -C "$fixture" config maintenance.auto false

empty_hooks="$fixture/empty-hooks"
mkdir -p "$empty_hooks"
git -C "$fixture" config core.hooksPath "$empty_hooks"
git -C "$fixture" config user.email hooks-test@keld.invalid
git -C "$fixture" config user.name "Keld hooks test"
mkdir -p "$fixture/.githooks"
cp "$source_root/justfile" "$fixture/justfile"
cp "$source_root/.githooks/post-checkout" "$fixture/.githooks/post-checkout"
cp "$source_root/.githooks/post-merge" "$fixture/.githooks/post-merge"
cp "$source_root/.githooks/pre-commit" "$fixture/.githooks/pre-commit"
git -C "$fixture" add justfile .githooks/post-checkout .githooks/post-merge .githooks/pre-commit
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
  "$source_root/tools/hooks_install.sh"
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

# Run actual Git commits through the installed hook, not a hand-run script.
# A no-op pre-commit mutation must fail the staged-unformatted case.
printf 'const ready = { ok: true };\n' >"$fixture/checked.ts"
git -C "$fixture" add checked.ts
git -C "$fixture" commit -qm 'hook: permit formatted staged content'
if [[ "$(git -C "$fixture" show HEAD:checked.ts)" != 'const ready = { ok: true };' ]]; then
  echo "error: a formatted staged file did not reach the commit intact" >&2
  exit 1
fi

printf 'const broken={ok:true};\n' >"$fixture/checked.ts"
git -C "$fixture" add checked.ts
if git -C "$fixture" commit -qm 'hook: must reject unformatted stage' \
  >"$fixture/deny-unformatted.out" 2>&1; then
  echo "error: installed pre-commit admitted an unformatted staged blob" >&2
  exit 1
fi
grep -Fq 'needs Bun formatting' "$fixture/deny-unformatted.out"

# A formatted working copy must not hide an unformatted staged blob.
printf 'const broken = { ok: true };\n' >"$fixture/checked.ts"
if git -C "$fixture" commit -qm 'hook: must not trust formatted worktree' \
  >"$fixture/deny-staged.out" 2>&1; then
  echo "error: installed pre-commit checked the working tree, not the staged blob" >&2
  exit 1
fi
grep -Fq 'needs Bun formatting' "$fixture/deny-staged.out"

# A dirty working copy cannot block a well-formatted staged blob. Only the
# index content is committed; different worktree bytes stay untouched.
git -C "$fixture" add checked.ts
printf 'const dirty={ok:true};\n' >"$fixture/checked.ts"
git -C "$fixture" commit -qm 'hook: accept formatted stage despite dirty worktree'
if [[ "$(git -C "$fixture" show HEAD:checked.ts)" != 'const broken = { ok: true };' ]] ||
  [[ "$(cat "$fixture/checked.ts")" != 'const dirty={ok:true};' ]]; then
  echo "error: pre-commit changed or selected the wrong source of staged bytes" >&2
  exit 1
fi
# Legitimate tracked symlinks with a source-language suffix have link target
# text in the index, not JavaScript/TypeScript. Their bytes must stay unchanged
# and a staged source formatter must skip the symlink itself. A glob-looking
# pathname also checks that stage-mode lookup uses a literal pathspec.
ln -s 'target-which-is-not-typescript' "$fixture/[generated].ts"
git -C "$fixture" add -- '[generated].ts'
git -C "$fixture" commit -qm 'hook: allow tracked source-suffix symlink'
if [[ "$(git -C "$fixture" ls-tree HEAD -- '[generated].ts' | cut -d' ' -f1)" != '120000' ]] ||
  [[ "$(git -C "$fixture" show 'HEAD:[generated].ts')" != 'target-which-is-not-typescript' ]]; then
  echo "error: staged symbolic source path was changed or not committed" >&2
  exit 1
fi

echo "hooks checkout and installed staged pre-commit trust-boundary tests ok"

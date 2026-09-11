#!/usr/bin/env bash
set -euo pipefail

script_dir=$(dirname -- "$(readlink -f -- "${BASH_SOURCE[0]}")")
source "$script_dir/linux_media_checkout.sh"

test_root=$(mktemp -d "${RUNNER_TEMP:-/tmp}/keld-media-checkout-test.XXXXXX")
cleanup() {
  local status=$?
  chmod -R u+w -- "$test_root" 2>/dev/null || true
  rm -rf -- "$test_root"
  exit "$status"
}
trap cleanup EXIT

repo="$test_root/repo"
mkdir -- "$repo"
git -C "$repo" init -q
git -C "$repo" config user.email keld-test@example.invalid
git -C "$repo" config user.name "Keld Test"
printf 'tracked\n' >"$repo/tracked"
git -C "$repo" add tracked
git -C "$repo" commit -q -m baseline

evidence="$repo/evidence"
mkdir -- "$evidence"
printf 'generated\n' >"$evidence/result.json"
keld_checkout_is_clean "$repo" evidence

mkdir -- "$repo/subdir"
(
  cd "$repo/subdir"
  keld_checkout_is_clean "$repo" evidence
)
rm -- "$evidence/result.json"
rmdir -- "$evidence"

outside="$test_root/outside-evidence"
mkdir -- "$outside"
printf 'outside\n' >"$outside/result.json"
keld_checkout_is_clean "$repo" ""

expect_dirty() {
  if keld_checkout_is_clean "$repo" evidence; then
    echo "checkout cleanliness accepted $1" >&2
    exit 1
  fi
}

printf 'prefix\n' >"$repo/evidence-sibling"
expect_dirty "a sibling prefix"
rm -- "$repo/evidence-sibling"

newline_name=$'untracked\nname'
printf 'newline\n' >"$repo/$newline_name"
expect_dirty "an untracked newline filename"
rm -- "$repo/$newline_name"

ln -s tracked "$repo/untracked-link"
expect_dirty "an untracked symlink"
rm -- "$repo/untracked-link"

printf 'changed\n' >>"$repo/tracked"
expect_dirty "a tracked modification"
git -C "$repo" restore tracked

printf 'staged\n' >"$repo/staged"
git -C "$repo" add staged
expect_dirty "a staged addition"
git -C "$repo" restore --staged staged
rm -- "$repo/staged"

git() {
  if [ "${3:-}" = ls-files ]; then
    return 73
  fi
  command git "$@"
}
expect_dirty "a failed untracked-file census"
unset -f git

git_dir=$(readlink -f -- "$(git -C "$repo" rev-parse --absolute-git-dir)")
git_common_dir=$(readlink -f -- "$(git -C "$repo" rev-parse --path-format=absolute --git-common-dir)")
if keld_validate_evidence_root "$git_dir/keld-evidence" "$git_dir" "$git_common_dir"; then
  echo "Git administrative evidence path was accepted" >&2
  exit 1
fi
if ! keld_validate_evidence_root "$repo/.git-evidence" "$git_dir" "$git_common_dir"; then
  echo "Git directory sibling was rejected as an administrative path" >&2
  exit 1
fi

linked="$test_root/linked"
git -C "$repo" worktree add -q --detach "$linked" HEAD
linked_git_dir=$(readlink -f -- "$(git -C "$linked" rev-parse --absolute-git-dir)")
linked_common_dir=$(readlink -f -- \
  "$(git -C "$linked" rev-parse --path-format=absolute --git-common-dir)")
if [ "$linked_git_dir" = "$linked_common_dir" ]; then
  echo "linked-worktree test did not produce distinct Git directories" >&2
  exit 1
fi
if keld_validate_evidence_root \
  "$linked_git_dir/keld-evidence" "$linked_git_dir" "$linked_common_dir"; then
  echo "linked-worktree administrative evidence path was accepted" >&2
  exit 1
fi
if keld_validate_evidence_root \
  "$linked_common_dir/objects/keld-evidence" "$linked_git_dir" "$linked_common_dir"; then
  echo "common Git administrative evidence path was accepted" >&2
  exit 1
fi

source_repo="$test_root/source-repo"
ambient_repo="$test_root/ambient-repo"
mkdir -p -- "$source_repo/crates/keld-wv/tests" "$ambient_repo"
cp -- "$script_dir/linux_media_guard.sh" \
  "$source_repo/crates/keld-wv/tests/linux_media_guard.sh"
cp -- "$script_dir/linux_media_checkout.sh" \
  "$source_repo/crates/keld-wv/tests/linux_media_checkout.sh"
git -C "$source_repo" init -q
git -C "$source_repo" config user.email keld-test@example.invalid
git -C "$source_repo" config user.name "Keld Test"
git -C "$source_repo" add crates/keld-wv/tests
git -C "$source_repo" commit -q -m baseline
git -C "$ambient_repo" init -q
git -C "$ambient_repo" config user.email keld-test@example.invalid
git -C "$ambient_repo" config user.name "Keld Test"
printf 'ambient\n' >"$ambient_repo/tracked"
git -C "$ambient_repo" add tracked
git -C "$ambient_repo" commit -q -m baseline
printf 'source-dirty\n' >"$source_repo/untracked"
set +e
(
  cd "$ambient_repo"
  KELD_MEDIA_EVIDENCE_DIRECTORY="$test_root/cross-evidence" \
    bash "$source_repo/crates/keld-wv/tests/linux_media_guard.sh" /bin/true /bin/true
) >"$test_root/cross.out" 2>&1
cross_status=$?
set -e
if [ "$cross_status" -eq 0 ] ||
  ! grep -q '^media evidence checkout is dirty before acceptance$' "$test_root/cross.out"; then
  echo "runner provenance followed the ambient checkout" >&2
  exit 1
fi

cp -- "$source_repo/crates/keld-wv/tests/linux_media_guard.sh" \
  "$source_repo/copied-linux-media-guard.sh"
set +e
bash "$source_repo/copied-linux-media-guard.sh" /bin/true /bin/true \
  >"$test_root/copied.out" 2>&1
copied_status=$?
set -e
if [ "$copied_status" -eq 0 ] ||
  ! grep -q '^media evidence runner must be the canonical tracked checkout source$' \
    "$test_root/copied.out"; then
  echo "copied runner was allowed to attest tracked source" >&2
  exit 1
fi

echo "linux media checkout cleanliness tests passed"

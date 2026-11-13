#!/usr/bin/env bash
# Regression for PR #923 review r4304684819: `just cache-check` must never
# execute a worktree-cache binary left by an earlier run. A sentinel program is
# planted at the exact $HOME cache path the previous recipe trusted whenever it
# was executable; the recipe must still run a freshly built real tool.
# Run by `just cache-check-test`. Hermetic: throwaway repository and HOME.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/keld-cache-provenance.XXXXXX")"
cleanup() {
    rm -rf "$temp_dir"
}
trap cleanup EXIT

fixture="$temp_dir/repo"
home="$temp_dir/home"
mkdir -p "$fixture/tools" "$home"
# The isolated HOME must not hide the caller's rustup toolchains: on Unix,
# rustup derives RUSTUP_HOME from HOME. The fixture also keeps the repo pin.
if [[ -z "${RUSTUP_HOME:-}" ]] && command -v rustup >/dev/null 2>&1; then
    RUSTUP_HOME="$(rustup show home)"
    export RUSTUP_HOME
fi
cp "$repo_root/rust-toolchain.toml" "$fixture/rust-toolchain.toml"
cp "$repo_root/justfile" "$fixture/justfile"
cp "$repo_root/tools/worktree_cache.rs" "$repo_root/tools/worktree_cache_delete.rs" \
    "$repo_root/tools/worktree_cache_delete_tests.rs" "$fixture/tools/"
git -C "$fixture" init -q
git -C "$fixture" config maintenance.auto false
# If the isolated HOME cannot see the caller's toolchains (RUSTUP_HOME neither
# set by the caller nor derived above, or naming the wrong home), rustup would
# silently install the pinned toolchain (about 1.4 GB) into the throwaway HOME
# and the test would still pass. Refuse auto-install (rustup's
# RUSTUP_AUTO_INSTALL=0) and fail loudly up front.
export RUSTUP_AUTO_INSTALL=0
(cd "$fixture" && HOME="$home" rustc -V >/dev/null) || {
    echo "FAIL: the pinned toolchain is not visible from the isolated HOME;" \
        "RUSTUP_HOME must name the caller's rustup home" >&2
    exit 1
}

# The previous recipe's cache key: tool sources plus `rustc -vV`.
stamp=$(
    cd "$fixture"
    {
        git hash-object tools/worktree_cache.rs
        git hash-object tools/worktree_cache_delete.rs
        git hash-object tools/worktree_cache_delete_tests.rs
        rustc -vV
    } | git hash-object --stdin
)
cached="$home/.cache/keld/worktree-tool/$stamp"
mkdir -p "$cached"
printf 'fn main() { println!("KELD_CACHE_SENTINEL_EXECUTED"); }\n' >"$temp_dir/sentinel.rs"
rustc --edition=2024 "$temp_dir/sentinel.rs" -o "$temp_dir/sentinel"
sentinel="$temp_dir/sentinel"
[[ -x "$sentinel" ]] || sentinel="$temp_dir/sentinel.exe"
for name in worktree-cache worktree-cache.exe; do
    cp "$sentinel" "$cached/$name"
    chmod +x "$cached/$name"
done

# Negative control: the planted file really is an executable sentinel.
"$cached/worktree-cache" | grep -Fqx KELD_CACHE_SENTINEL_EXECUTED

output=$(cd "$fixture" && HOME="$home" just cache-check 2>&1) || {
    printf '%s\n' "$output" >&2
    echo "FAIL: just cache-check failed in the provenance fixture" >&2
    exit 1
}
if grep -Fq KELD_CACHE_SENTINEL_EXECUTED <<<"$output"; then
    printf '%s\n' "$output" >&2
    echo "FAIL: just cache-check executed a previously cached binary" >&2
    exit 1
fi
grep -Fq "Registered Git worktrees: 1" <<<"$output" || {
    printf '%s\n' "$output" >&2
    echo "FAIL: just cache-check did not run the freshly built tool" >&2
    exit 1
}
echo "cache tool provenance: OK (cached sentinel never executed)"

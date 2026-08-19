#!/usr/bin/env bash
# Contract tests for tools/ci_changes.sh. Run by the always-created router job.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
router="$repo_root/tools/ci_changes.sh"

result_for_paths() {
    printf '%s\0' "$@" | "$router" classify
}

expect_flags() {
    local label="$1"
    local expected="$2"
    local actual="$3"
    actual="$(grep -Ev '^(packages|nongtk_packages)=' <<<"$actual")"
    if [[ "$actual" != "$expected" ]]; then
        echo "FAIL: $label" >&2
        echo "expected:" >&2
        printf '%s\n' "$expected" >&2
        echo "actual:" >&2
        printf '%s\n' "$actual" >&2
        exit 1
    fi
    echo "ok: $label"
}

expect_package_token() {
    local label="$1"
    local token="$2"
    local actual="$3"
    local package_line
    package_line="$(grep '^packages=' <<<"$actual")"
    if ! grep -Eq "(^| )${token}( |$)" <<<"${package_line#packages=}"; then
        echo "FAIL: $label: package set does not contain $token" >&2
        printf '%s\n' "$actual" >&2
        exit 1
    fi
    echo "ok: $label"
}

expect_nongtk_excludes() {
    local label="$1"
    local token="$2"
    local actual="$3"
    local line
    line="$(grep '^nongtk_packages=' <<<"$actual")"
    if grep -Eq "(^| )${token}( |$)" <<<"${line#nongtk_packages=}"; then
        echo "FAIL: $label: Ubuntu no-GTK set unexpectedly contains $token" >&2
        printf '%s\n' "$actual" >&2
        exit 1
    fi
    echo "ok: $label"
}

expect_empty_packages() {
    local label="$1"
    local actual="$2"
    if ! grep -Fxq 'packages=' <<<"$actual"; then
        echo "FAIL: $label: expected no selected workspace package" >&2
        printf '%s\n' "$actual" >&2
        exit 1
    fi
    echo "ok: $label"
}

all_false=$'rust=false\ndocs=false\nhygiene=false\ngui=false\nmsrv=false\ndeny=false\nwebkitgtk=false'
runtime_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=false\nmsrv=true\ndeny=false\nwebkitgtk=false'
docs_only=$'rust=false\ndocs=true\nhygiene=false\ngui=false\nmsrv=false\ndeny=false\nwebkitgtk=false'
hygiene_only=$'rust=false\ndocs=false\nhygiene=true\ngui=false\nmsrv=false\ndeny=false\nwebkitgtk=false'
host_dependency=$'rust=true\ndocs=false\nhygiene=false\ngui=true\nmsrv=true\ndeny=false\nwebkitgtk=false'
compat_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=false\nmsrv=true\ndeny=false\nwebkitgtk=false'
wv_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=true\nmsrv=true\ndeny=false\nwebkitgtk=true'
manifest=$'rust=true\ndocs=false\nhygiene=false\ngui=true\nmsrv=true\ndeny=true\nwebkitgtk=true'
workflow_all=$'rust=true\ndocs=true\nhygiene=true\ngui=true\nmsrv=true\ndeny=true\nwebkitgtk=false'
all_true=$'rust=true\ndocs=true\nhygiene=true\ngui=true\nmsrv=true\ndeny=true\nwebkitgtk=true'

empty_classification="$(printf '' | "$router" classify)"
expect_flags "empty diff skips conditional lanes" "$all_false" "$empty_classification"
expect_empty_packages "empty diff selects no package" "$empty_classification"

runtime_classification="$(result_for_paths crates/keld-runtime/src/lib.rs)"
expect_flags "runtime-only Rust change avoids host GUI smoke and Ubuntu GTK apt" "$runtime_flags" "$runtime_classification"
expect_package_token "runtime-only change includes its owner package" keld-runtime "$runtime_classification"
expect_package_token "runtime-only change still clippy's keld-cli consumers on macOS/Windows" keld-cli "$runtime_classification"
expect_nongtk_excludes "runtime-only Ubuntu clippy does not compile keld-cli without GTK" keld-cli "$runtime_classification"

docs_classification="$(result_for_paths docs/architecture/01-overview.md)"
expect_flags "docs-only change avoids Rust and GUI lanes" "$docs_only" "$docs_classification"
expect_empty_packages "docs-only change selects no package" "$docs_classification"

hygiene_classification="$(result_for_paths .github/CODEOWNERS)"
expect_flags "hygiene input runs only hygiene contract" "$hygiene_only" "$hygiene_classification"

host_classification="$(result_for_paths crates/keld-ipc/src/lib.rs)"
expect_flags "host dependency closure routes IPC change to GUI smoke without extra GTK apt" "$host_dependency" "$host_classification"
expect_package_token "IPC change includes host consumer" keld-host "$host_classification"

compat_classification="$(result_for_paths crates/keld-compat/src/lib.rs)"
expect_flags "compat-only change skips GUI smoke and Ubuntu WebKitGTK apt" "$compat_flags" "$compat_classification"
expect_package_token "compat-only change includes its owner package" keld-compat "$compat_classification"

wv_classification="$(result_for_paths crates/keld-wv/src/lib.rs)"
expect_flags "keld-wv change enables GUI smoke and Ubuntu WebKitGTK apt" "$wv_flags" "$wv_classification"
expect_package_token "keld-wv change includes host consumer" keld-host "$wv_classification"

manifest_classification="$(result_for_paths Cargo.lock)"
expect_flags "workspace manifest routes every dependent Rust lane" "$manifest" "$manifest_classification"
expect_package_token "workspace manifest selects host" keld-host "$manifest_classification"

unknown_classification="$(result_for_paths packages/new-package/index.ts)"
expect_flags "unknown input fails safe" "$all_true" "$unknown_classification"
expect_package_token "unknown input selects all workspace packages" keld-host "$unknown_classification"

workflow_classification="$(result_for_paths .github/workflows/ci.yml)"
expect_flags "workflow input exercises all jobs; GTK apt stays on GUI smoke only" "$workflow_all" "$workflow_classification"

actual_host_dirs="$(cd "$repo_root" && "$router" host-dirs | sort)"
for required_dir in crates/keld-host crates/keld-core crates/keld-guard crates/keld-ipc crates/keld-wv; do
    if ! grep -Fxq "$required_dir" <<<"$actual_host_dirs"; then
        echo "FAIL: keld-host dependency closure omits $required_dir" >&2
        exit 1
    fi
done
echo "ok: cargo metadata derives current keld-host closure"

temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/keld-ci-changes.XXXXXX")"
cleanup() {
    rm -rf "$temp_dir"
}
trap cleanup EXIT

git -C "$temp_dir" init -q
git -C "$temp_dir" config user.email ci-router@example.invalid
git -C "$temp_dir" config user.name ci-router-test
mkdir -p "$temp_dir/crates/keld-runtime/src" "$temp_dir/fake-bin"
printf 'base\n' >"$temp_dir/README.md"
git -C "$temp_dir" add README.md
git -C "$temp_dir" commit -qm base
base_sha="$(git -C "$temp_dir" rev-parse HEAD)"

# The production script has no bypass/override: it always asks `cargo metadata`.
# This temporary executable is a controlled external dependency fixture so PR/push
# diff tests can create a minimal Git repository without copying the Keld workspace.
printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    'root="$(pwd -P)"' \
    'printf "{\\\"packages\\\":[{\\\"name\\\":\\\"keld-host\\\",\\\"manifest_path\\\":\\\"%s/crates/keld-host/Cargo.toml\\\",\\\"dependencies\\\":[{\\\"name\\\":\\\"keld-core\\\",\\\"path\\\":\\\"%s/crates/keld-core\\\"}]},{\\\"name\\\":\\\"keld-core\\\",\\\"manifest_path\\\":\\\"%s/crates/keld-core/Cargo.toml\\\",\\\"dependencies\\\":[{\\\"name\\\":\\\"keld-ipc\\\",\\\"path\\\":\\\"%s/crates/keld-ipc\\\"}]},{\\\"name\\\":\\\"keld-ipc\\\",\\\"manifest_path\\\":\\\"%s/crates/keld-ipc/Cargo.toml\\\",\\\"dependencies\\\":[]},{\\\"name\\\":\\\"keld-runtime\\\",\\\"manifest_path\\\":\\\"%s/crates/keld-runtime/Cargo.toml\\\",\\\"dependencies\\\":[]}]}\\n" "$root" "$root" "$root" "$root" "$root" "$root"' \
    >"$temp_dir/fake-bin/cargo"
chmod +x "$temp_dir/fake-bin/cargo"

printf 'runtime\n' >"$temp_dir/crates/keld-runtime/src/lib.rs"
git -C "$temp_dir" add crates/keld-runtime/src/lib.rs
git -C "$temp_dir" commit -qm runtime
runtime_sha="$(git -C "$temp_dir" rev-parse HEAD)"
pr_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=pull_request KELD_CI_BASE_SHA="$base_sha" KELD_CI_HEAD_SHA="$runtime_sha" "$router" github)"
fake_runtime_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=false\nmsrv=true\ndeny=false\nwebkitgtk=false'
expect_flags "pull-request base/head classifies the actual diff" "$fake_runtime_flags" "$pr_result"
expect_package_token "pull-request base/head selects changed package" keld-runtime "$pr_result"

mkdir -p "$temp_dir/docs"
printf 'docs\n' >"$temp_dir/docs/guide.md"
git -C "$temp_dir" add docs/guide.md
git -C "$temp_dir" commit -qm docs
docs_sha="$(git -C "$temp_dir" rev-parse HEAD)"
push_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA="$runtime_sha" GITHUB_SHA="$docs_sha" "$router" github)"
expect_flags "push before/head classifies the actual diff" "$docs_only" "$push_result"

empty_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA="$docs_sha" GITHUB_SHA="$docs_sha" "$router" github)"
expect_flags "same push base/head is an empty diff" "$all_false" "$empty_result"

unknown_base_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA=0000000000000000000000000000000000000000 GITHUB_SHA="$docs_sha" "$router" github)"
fake_all_true=$'rust=true\ndocs=true\nhygiene=true\ngui=true\nmsrv=true\ndeny=true\nwebkitgtk=true'
expect_flags "missing comparison base fails safe" "$fake_all_true" "$unknown_base_result"

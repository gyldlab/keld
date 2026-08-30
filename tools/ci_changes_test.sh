#!/usr/bin/env bash
# Contract tests for tools/ci_changes.sh. Run by the always-created router job.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
router="$repo_root/tools/ci_changes.sh"
required="$repo_root/tools/ci_required.sh"

"$required" test

result_for_paths() {
    printf '%s\0' "$@" | "$router" classify
}

expect_flags() {
    local label="$1"
    local expected="$2"
    local actual="$3"
    actual="$(grep -Ev '^(packages|nongtk_packages|ubuntu_packages|ts_packages)=' <<<"$actual")"
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
    expect_output_package_token "$1" packages "$2" "$3"
}

expect_output_package_token() {
    local label="$1"
    local output_name="$2"
    local token="$3"
    local actual="$4"
    local package_line
    package_line="$(grep "^${output_name}=" <<<"$actual")"
    if ! grep -Eq "(^| )${token}( |$)" <<<"${package_line#"${output_name}"=}"; then
        echo "FAIL: $label: ${output_name} does not contain $token" >&2
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
    expect_empty_output "$1" packages "$2"
}

expect_no_package_selection() {
    local label="$1"
    local actual="$2"
    local output_name
    for output_name in packages nongtk_packages ubuntu_packages ts_packages; do
        expect_empty_output "$label ($output_name)" "$output_name" "$actual"
    done
}

expect_empty_output() {
    local label="$1"
    local output_name="$2"
    local actual="$3"
    if ! grep -Fxq "${output_name}=" <<<"$actual"; then
        echo "FAIL: $label: expected an empty ${output_name} selection" >&2
        printf '%s\n' "$actual" >&2
        exit 1
    fi
    echo "ok: $label"
}

all_false=$'rust=false\ndocs=false\nhygiene=false\ngui=false\nmsrv=false\ndeny=false\nts=false\nwebkitgtk=false'
# keld-core depends on keld-runtime (KEL-30 host-owned session), so runtime lives
# in the keld-host dependency closure and a runtime-only path change enables GUI smoke.
runtime_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=true\nmsrv=true\ndeny=false\nts=false\nwebkitgtk=true'
docs_only=$'rust=false\ndocs=true\nhygiene=false\ngui=false\nmsrv=false\ndeny=false\nts=false\nwebkitgtk=false'
hygiene_only=$'rust=false\ndocs=false\nhygiene=true\ngui=false\nmsrv=false\ndeny=false\nts=false\nwebkitgtk=false'
docs_hygiene=$'rust=false\ndocs=true\nhygiene=true\ngui=false\nmsrv=false\ndeny=false\nts=false\nwebkitgtk=false'
host_dependency=$'rust=true\ndocs=false\nhygiene=false\ngui=true\nmsrv=true\ndeny=false\nts=false\nwebkitgtk=true'
compat_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=false\nmsrv=true\ndeny=false\nts=false\nwebkitgtk=true'
# A TypeScript package change owns the Bun lane and the crates that read that
# package directory (keld-compat spawns the @keld/electron fixtures). It cannot
# change rustc, the workspace dependency policy, or the host window, so MSRV,
# cargo-deny and GUI smoke stay off; GTK follows the selected Rust closure.
ts_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=false\nmsrv=false\ndeny=false\nts=true\nwebkitgtk=true'
wv_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=true\nmsrv=true\ndeny=false\nts=false\nwebkitgtk=true'
manifest=$'rust=true\ndocs=false\nhygiene=false\ngui=true\nmsrv=true\ndeny=true\nts=false\nwebkitgtk=true'
workflow_all=$'rust=true\ndocs=true\nhygiene=true\ngui=true\nmsrv=true\ndeny=true\nts=true\nwebkitgtk=false'
all_true=$'rust=true\ndocs=true\nhygiene=true\ngui=true\nmsrv=true\ndeny=true\nts=true\nwebkitgtk=true'

empty_classification="$(printf '' | "$router" classify)"
expect_flags "empty diff skips conditional lanes" "$all_false" "$empty_classification"
expect_empty_packages "empty diff selects no package" "$empty_classification"
expect_empty_output "empty diff selects no Bun suite" ts_packages "$empty_classification"

runtime_classification="$(result_for_paths crates/keld-runtime/src/lib.rs)"
expect_flags "runtime-only Rust change enables host GUI smoke and Ubuntu GTK for its selected test closure" "$runtime_flags" "$runtime_classification"
expect_package_token "runtime-only change includes its owner package" keld-runtime "$runtime_classification"
expect_output_package_token "runtime-only Ubuntu selection includes its owner package" ubuntu_packages keld-runtime "$runtime_classification"
expect_package_token "runtime-only change still clippy's keld-cli consumers on macOS/Windows" keld-cli "$runtime_classification"
expect_package_token "runtime-only change clippy's host-owned session consumer" keld-core "$runtime_classification"
expect_nongtk_excludes "runtime-only Ubuntu clippy does not compile keld-cli without GTK" keld-cli "$runtime_classification"

docs_classification="$(result_for_paths docs/architecture/01-overview.md)"
expect_flags "docs-only change avoids Rust and GUI lanes" "$docs_only" "$docs_classification"
expect_empty_packages "docs-only change selects no package" "$docs_classification"

hygiene_classification="$(result_for_paths .github/CODEOWNERS)"
expect_flags "hygiene input runs only hygiene contract" "$hygiene_only" "$hygiene_classification"

atomic_checker_classification="$(result_for_paths tools/atomic_protocol.rs)"
expect_flags "atomic protocol checker runs only its hygiene contract" "$hygiene_only" "$atomic_checker_classification"
expect_empty_packages "atomic protocol checker selects no package" "$atomic_checker_classification"

agent_context_classification="$(result_for_paths tools/agent_context.rs tools/markdown_contract.rs .agents/instruction-budget.tsv)"
expect_flags "instruction budget inputs run only hygiene" "$hygiene_only" "$agent_context_classification"
expect_no_package_selection "instruction budget inputs select no package/suite" "$agent_context_classification"

agent_instruction_classification="$(result_for_paths AGENTS.md crates/keld-wv/AGENTS.md .agents/index.md .agents/skills/instruction-review/SKILL.md .agents/new.txt docs/agents/workflow.md)"
expect_flags "agent instruction Markdown runs docs and merge-blocking hygiene" "$docs_hygiene" "$agent_instruction_classification"
expect_no_package_selection "agent instruction Markdown selects no package/suite" "$agent_instruction_classification"

agent_assembly_classification="$(result_for_paths .codex/config.toml)"
expect_flags "agent assembly config runs merge-blocking hygiene" "$hygiene_only" "$agent_assembly_classification"
expect_no_package_selection "agent assembly config selects no package/suite" "$agent_assembly_classification"

host_classification="$(result_for_paths crates/keld-ipc/src/lib.rs)"
expect_flags "host dependency closure routes IPC change to GUI smoke and GTK for its selected test closure" "$host_dependency" "$host_classification"
expect_package_token "IPC change includes host consumer" keld-host "$host_classification"

compat_classification="$(result_for_paths crates/keld-compat/src/lib.rs)"
expect_flags "compat-only change skips GUI smoke but installs GTK for its selected test closure" "$compat_flags" "$compat_classification"
expect_package_token "compat-only change includes its owner package" keld-compat "$compat_classification"

compat_test_classification="$(result_for_paths crates/keld-compat/tests/electron_lifecycle.rs)"
expect_flags "Rust-only change does not select the TypeScript lane" "$compat_flags" "$compat_test_classification"
expect_empty_output "Rust-only change selects no Bun suite" ts_packages "$compat_test_classification"

ts_classification="$(result_for_paths packages/@keld/electron/src/link.ts)"
expect_flags "TypeScript package change runs the Bun lane and its Rust consumer only" "$ts_flags" "$ts_classification"
expect_output_package_token "TypeScript package change selects its Bun suite root" ts_packages packages/@keld/electron "$ts_classification"
expect_package_token "TypeScript package change re-runs the crate that spawns its fixtures" keld-compat "$ts_classification"

ts_fixture_classification="$(result_for_paths packages/@keld/electron/fixtures/app_ready.ts)"
expect_flags "TypeScript fixture change routes like its owning package" "$ts_flags" "$ts_fixture_classification"
expect_output_package_token "TypeScript fixture change selects the owning Bun suite root" ts_packages packages/@keld/electron "$ts_fixture_classification"
expect_package_token "TypeScript fixture change re-runs its Rust conformance consumer" keld-compat "$ts_fixture_classification"

ts_docs_classification="$(result_for_paths packages/@keld/electron/README.md)"
expect_flags "Markdown inside a TypeScript package stays in the docs lane" "$docs_only" "$ts_docs_classification"
expect_empty_output "Markdown inside a TypeScript package selects no Bun suite" ts_packages "$ts_docs_classification"

wv_classification="$(result_for_paths crates/keld-wv/src/lib.rs)"
expect_flags "keld-wv change enables GUI smoke and Ubuntu WebKitGTK apt" "$wv_flags" "$wv_classification"
expect_package_token "keld-wv change includes host consumer" keld-host "$wv_classification"

manifest_classification="$(result_for_paths Cargo.lock)"
expect_flags "workspace manifest routes every dependent Rust lane" "$manifest" "$manifest_classification"
expect_package_token "workspace manifest selects host" keld-host "$manifest_classification"

unknown_classification="$(result_for_paths packages/new-package/index.ts)"
expect_flags "packages path that no package.json owns fails safe" "$all_true" "$unknown_classification"
expect_package_token "unowned packages path selects all workspace packages" keld-host "$unknown_classification"

unknown_root_classification="$(result_for_paths some-future-dir/thing.bin)"
expect_flags "unknown input fails safe" "$all_true" "$unknown_root_classification"
expect_package_token "unknown input selects all workspace packages" keld-host "$unknown_root_classification"
expect_output_package_token "unknown input still exercises the Bun lane" ts_packages packages/@keld/electron "$unknown_root_classification"

workflow_classification="$(result_for_paths .github/workflows/ci.yml)"
expect_flags "workflow input exercises all jobs; GTK apt stays on GUI smoke only" "$workflow_all" "$workflow_classification"
expect_output_package_token "workflow input exercises the Bun lane over every suite" ts_packages packages/@keld/electron "$workflow_classification"

keldbot_workflow_classification="$(result_for_paths .github/workflows/keldbot.yml)"
expect_flags "KeldBot workflow exercises every conditional lane" "$workflow_all" "$keldbot_workflow_classification"
expect_output_package_token "KeldBot workflow exercises the Bun lane over every suite" ts_packages packages/@keld/electron "$keldbot_workflow_classification"

other_workflow_classification="$(result_for_paths .github/workflows/unrelated-bot.yml)"
expect_flags "every workflow edit exercises every conditional lane" "$workflow_all" "$other_workflow_classification"
expect_output_package_token "new workflow exercises the Bun lane over every suite" ts_packages packages/@keld/electron "$other_workflow_classification"

router_script_classification="$(result_for_paths tools/ci_changes.sh)"
expect_flags "router script edit still exercises all jobs" "$workflow_all" "$router_script_classification"

router_test_classification="$(result_for_paths tools/ci_changes_test.sh)"
expect_flags "router test edit still exercises all jobs" "$workflow_all" "$router_test_classification"

required_script_classification="$(result_for_paths tools/ci_required.sh)"
expect_flags "required-result evaluator edit still exercises all jobs" "$workflow_all" "$required_script_classification"

actual_host_dirs="$(cd "$repo_root" && "$router" host-dirs | sort)"
for required_dir in crates/keld-host crates/keld-core crates/keld-guard crates/keld-ipc crates/keld-runtime crates/keld-wv; do
    if ! grep -Fxq "$required_dir" <<<"$actual_host_dirs"; then
        echo "FAIL: keld-host dependency closure omits $required_dir" >&2
        exit 1
    fi
done
if grep -Fxq "crates/keld-compat" <<<"$actual_host_dirs"; then
    echo "FAIL: host-dirs must exclude keld-runtime's cargo kind=dev edge to keld-compat" >&2
    printf '%s\n' "$actual_host_dirs" >&2
    exit 1
fi
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
fake_runtime_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=false\nmsrv=true\ndeny=false\nts=false\nwebkitgtk=false'
expect_flags "pull-request base/head classifies the actual diff" "$fake_runtime_flags" "$pr_result"
expect_package_token "pull-request base/head selects changed package" keld-runtime "$pr_result"
expect_output_package_token "pull-request base/head selects the same Ubuntu package" ubuntu_packages keld-runtime "$pr_result"

mkdir -p "$temp_dir/docs"
printf 'docs\n' >"$temp_dir/docs/guide.md"
git -C "$temp_dir" add docs/guide.md
git -C "$temp_dir" commit -qm docs
docs_sha="$(git -C "$temp_dir" rev-parse HEAD)"
push_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA="$runtime_sha" GITHUB_SHA="$docs_sha" "$router" github)"
expect_flags "push before/head classifies the actual diff" "$docs_only" "$push_result"

# A packages/ diff with no Bun suite anywhere must fail the router, not emit a
# selected-but-empty TypeScript lane. This runs before the suite fixture below
# exists, which is the only moment that state is reachable.
mkdir -p "$temp_dir/packages/@fake/untested/src"
printf '{"name":"@fake/untested","type":"module"}\n' >"$temp_dir/packages/@fake/untested/package.json"
printf 'export const noop = () => {};\n' >"$temp_dir/packages/@fake/untested/src/index.ts"
git -C "$temp_dir" add packages
git -C "$temp_dir" commit -qm untested
untested_sha="$(git -C "$temp_dir" rev-parse HEAD)"
if no_suite_output="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=pull_request KELD_CI_BASE_SHA="$docs_sha" KELD_CI_HEAD_SHA="$untested_sha" "$router" github 2>&1)"; then
    echo "FAIL: a selected TypeScript lane with no Bun suite must fail before it emits a skipped-green success" >&2
    printf '%s\n' "$no_suite_output" >&2
    exit 1
fi
if ! grep -Fq "ci router: the TypeScript lane is selected but no packages/ Bun suite was found" <<<"$no_suite_output"; then
    echo "FAIL: empty Bun suite selection did not report the fail-closed router error" >&2
    printf '%s\n' "$no_suite_output" >&2
    exit 1
fi
echo "ok: empty Bun suite selection fails closed"

# Pins the router's suite-discovery set against bun's own, per filename shape.
#
# Every fixture elsewhere in this file uses `unit.test.ts`, the single shape the
# original pattern matched — so a wrong pattern stayed invisible. Measured on
# bun 1.4.0: of 21 planted filenames it runs 18, skipping only `plain.ts`,
# `test.ts` and `tests.ts`. A shape bun runs that the router misses is a suite
# silently dropped from a green lane; a shape bun skips that the router selects
# makes `bun test` exit 1 on a package with nothing to run.
discovery_shape_case() {
    local label="$1" filename="$2" expectation="$3" pkg="probe$4"
    local before after out
    before="$(git -C "$temp_dir" rev-parse HEAD)"
    mkdir -p "$temp_dir/packages/@shape/$pkg/src"
    printf '{"name":"@shape/%s","type":"module"}\n' "$pkg" >"$temp_dir/packages/@shape/$pkg/package.json"
    printf 'import { test } from "bun:test";\n' >"$temp_dir/packages/@shape/$pkg/src/$filename"
    git -C "$temp_dir" add packages >/dev/null
    git -C "$temp_dir" commit -qm "shape-$pkg"
    after="$(git -C "$temp_dir" rev-parse HEAD)"
    # stdout only, and the exit status is kept: `2>&1 || true` would let a
    # router that failed outright pass a `selected` case on a package path that
    # happened to appear in its error text.
    local status=0
    out="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=pull_request \
        KELD_CI_BASE_SHA="$before" KELD_CI_HEAD_SHA="$after" "$router" github)" || status=$?
    if [[ "$expectation" == selected ]]; then
        if [[ "$status" -ne 0 ]]; then
            echo "FAIL: router exited $status for $filename ($label); a selected shape must classify cleanly" >&2
            printf '%s\n' "$out" >&2
            exit 1
        fi
        # The package must appear in the `ts_packages=` line, not merely
        # somewhere in the output.
        local selection
        selection="$(grep -E '^ts_packages=' <<<"$out" || true)"
        if ! grep -Fq "packages/@shape/$pkg" <<<"$selection"; then
            echo "FAIL: bun runs $filename but the router did not select its package ($label)" >&2
            printf 'ts_packages line: %s\nfull output:\n%s\n' "$selection" "$out" >&2
            exit 1
        fi
    elif grep -E '^ts_packages=' <<<"$out" | grep -Fq "packages/@shape/$pkg"; then
        echo "FAIL: bun skips $filename but the router selected its package ($label); bun test would exit 1 there" >&2
        printf '%s\n' "$out" >&2
        exit 1
    fi
    rm -rf "$temp_dir/packages/@shape/$pkg"
    git -C "$temp_dir" add -A packages >/dev/null
    git -C "$temp_dir" commit -qm "shape-$pkg-cleanup"
}

shape_index=0
# Generated from the same 4 separators x 8 extensions the router encodes, not
# hand-listed. A hand-list covered 16 of the 32 patterns, and each of the other
# 16 could be deleted individually with this suite still green — a pin that
# reported coverage it did not have.
for sep in .test. _test. .spec. _spec.; do
    for ext in ts tsx js jsx mts cts mjs cjs; do
        shape_index=$((shape_index + 1))
        discovery_shape_case "bun runs it" "a${sep}${ext}" selected "$shape_index"
    done
done
# Case-insensitivity is a separate axis: bun runs these, and dropping `-iname`
# for `-name` in the router would pass every shape above.
for shape in A.Test.ts A.SPEC.ts B_Test.js B_Spec.js; do
    shape_index=$((shape_index + 1))
    discovery_shape_case "bun runs it, mixed case" "$shape" selected "$shape_index"
done
for shape in plain.ts test.ts tests.ts; do
    shape_index=$((shape_index + 1))
    discovery_shape_case "bun skips it" "$shape" ignored "$shape_index"
done
echo "ok: router suite discovery matches bun 1.4.0 across all 32 patterns, 4 case variants and 3 skipped shapes"

# A Bun suite the Keld workspace does not own: this fixture proves the lane is
# derived from the checked-out packages/ tree, not from a hard-coded path.
mkdir -p "$temp_dir/packages/@fake/pkg/src"
printf '{"name":"@fake/pkg","type":"module"}\n' >"$temp_dir/packages/@fake/pkg/package.json"
printf 'import { test } from "bun:test";\n' >"$temp_dir/packages/@fake/pkg/src/unit.test.ts"
git -C "$temp_dir" add packages
git -C "$temp_dir" commit -qm packages
ts_sha="$(git -C "$temp_dir" rev-parse HEAD)"
ts_pr_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=pull_request KELD_CI_BASE_SHA="$untested_sha" KELD_CI_HEAD_SHA="$ts_sha" "$router" github)"
fake_ts_flags=$'rust=false\ndocs=false\nhygiene=false\ngui=false\nmsrv=false\ndeny=false\nts=true\nwebkitgtk=false'
expect_flags "pull-request TypeScript-only diff runs the Bun lane with no Rust consumer" "$fake_ts_flags" "$ts_pr_result"
expect_output_package_token "pull-request TypeScript-only diff selects the changed Bun suite" ts_packages 'packages/@fake/pkg' "$ts_pr_result"
expect_empty_packages "pull-request TypeScript-only diff selects no workspace package" "$ts_pr_result"

ts_push_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA="$untested_sha" GITHUB_SHA="$ts_sha" "$router" github)"
expect_flags "push TypeScript-only diff runs the Bun lane" "$fake_ts_flags" "$ts_push_result"
expect_output_package_token "push TypeScript-only diff selects the changed Bun suite" ts_packages 'packages/@fake/pkg' "$ts_push_result"

empty_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA="$ts_sha" GITHUB_SHA="$ts_sha" "$router" github)"
expect_flags "same push base/head is an empty diff" "$all_false" "$empty_result"

unknown_base_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA=0000000000000000000000000000000000000000 GITHUB_SHA="$docs_sha" "$router" github)"
fake_all_true=$'rust=true\ndocs=true\nhygiene=true\ngui=true\nmsrv=true\ndeny=true\nts=true\nwebkitgtk=true'
expect_flags "missing comparison base fails safe" "$fake_all_true" "$unknown_base_result"
expect_output_package_token "missing comparison base still exercises the Bun lane" ts_packages 'packages/@fake/pkg' "$unknown_base_result"

mkdir -p "$temp_dir/empty-bin"
printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    'printf "{\\\"packages\\\":[]}\\n"' \
    >"$temp_dir/empty-bin/cargo"
chmod +x "$temp_dir/empty-bin/cargo"

if empty_metadata_output="$(cd "$temp_dir" && PATH="$temp_dir/empty-bin:$PATH" KELD_CI_EVENT_NAME=pull_request KELD_CI_BASE_SHA="$base_sha" KELD_CI_HEAD_SHA="$runtime_sha" "$router" github 2>&1)"; then
    echo "FAIL: missing workspace metadata must fail before it can emit an empty Ubuntu package set" >&2
    printf '%s\n' "$empty_metadata_output" >&2
    exit 1
fi
if ! grep -Fq "ci router: Rust checks selected no Ubuntu packages" <<<"$empty_metadata_output"; then
    echo "FAIL: empty Ubuntu package selection did not report the fail-closed router error" >&2
    printf '%s\n' "$empty_metadata_output" >&2
    exit 1
fi
echo "ok: empty Ubuntu package selection fails closed"

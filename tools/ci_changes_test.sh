#!/usr/bin/env bash
# Contract tests for tools/ci_changes.sh. Run by the always-created router job.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
router="$repo_root/tools/ci_changes.sh"
required="$repo_root/tools/ci_required.sh"

temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/keld-ci-changes.XXXXXX")"
cleanup() {
    rm -rf "$temp_dir"
}
trap cleanup EXIT

"$required" test
"$repo_root/tools/dependency_review_metadata.sh" test

result_for_paths() {
    printf '%s\0' "$@" | "$router" classify
}

expect_flags() {
    local label="$1"
    local expected="$2"
    local actual="$3"
    actual="$(grep -Ev '^(local_|codeql_|(mermaid|packages|nongtk_packages|ubuntu_packages|ts_packages|workspace|check_os|rust_documentation_only|doctest|doctest_packages)=)' <<<"$actual")"
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

# CodeQL applicability is its own per-language dimension (#624): a Rust lane
# selected through a documentation read analyses no changed source.
expect_codeql() {
    local label="$1"
    local expected="$2"
    local actual="$3"
    actual="$(grep -E '^codeql_' <<<"$actual" || true)"
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

expect_exact_output() {
    local label="$1"
    local output_name="$2"
    local expected="$3"
    local actual="$4"
    if ! grep -Fxq "${output_name}=${expected}" <<<"$actual"; then
        echo "FAIL: $label: expected ${output_name}=${expected}" >&2
        printf '%s\n' "$actual" >&2
        exit 1
    fi
    echo "ok: $label"
}

expect_package_absent() {
    local label="$1"
    local token="$2"
    local actual="$3"
    local package_line
    package_line="$(grep '^packages=' <<<"$actual")"
    if grep -Eq "(^| )${token}( |$)" <<<"${package_line#packages=}"; then
        echo "FAIL: $label: packages unexpectedly contains $token" >&2
        printf '%s\n' "$actual" >&2
        exit 1
    fi
    echo "ok: $label"
}

expect_mermaid_flag() {
    local label="$1"
    local expected="$2"
    local actual="$3"
    local line
    line="$(grep '^mermaid=' <<<"$actual")"
    if [[ "$line" != "mermaid=$expected" ]]; then
        echo "FAIL: $label: expected mermaid=$expected, got '${line#mermaid=}'" >&2
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

expect_output_package_absent() {
    local label="$1"
    local output_name="$2"
    local token="$3"
    local actual="$4"
    local line
    line="$(grep "^${output_name}=" <<<"$actual")"
    if grep -Eq "(^| )${token}( |$)" <<<"${line#"${output_name}"=}"; then
        echo "FAIL: $label: ${output_name} unexpectedly contains $token" >&2
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
docs_rust=$'rust=true\ndocs=true\nhygiene=false\ngui=false\nmsrv=false\ndeny=false\nts=false\nwebkitgtk=true'
# A Rust selection reached only through documentation reads runs on Windows
# alone, so no Ubuntu leg installs WebKitGTK (#624).
docs_reader_rust=$'rust=true\ndocs=true\nhygiene=false\ngui=false\nmsrv=false\ndeny=false\nts=false\nwebkitgtk=false'
hygiene_rust=$'rust=true\ndocs=false\nhygiene=true\ngui=false\nmsrv=false\ndeny=false\nts=false\nwebkitgtk=true'
docs_hygiene_rust=$'rust=true\ndocs=true\nhygiene=true\ngui=false\nmsrv=false\ndeny=false\nts=false\nwebkitgtk=true'
host_dependency=$'rust=true\ndocs=false\nhygiene=false\ngui=true\nmsrv=true\ndeny=false\nts=false\nwebkitgtk=true'
compat_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=false\nmsrv=true\ndeny=false\nts=false\nwebkitgtk=true'
ipc_fixture_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=true\nmsrv=true\ndeny=false\nts=true\nwebkitgtk=true'
# A TypeScript package change owns the Bun lane and the crates that read that
# package directory (keld-compat spawns the @keld/electron fixtures). It cannot
# change rustc, the workspace dependency policy, or the host window, so MSRV,
# cargo-deny and GUI smoke stay off; GTK follows the selected Rust closure.
ts_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=false\nmsrv=false\ndeny=false\nts=true\nwebkitgtk=true'
wv_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=true\nmsrv=true\ndeny=false\nts=false\nwebkitgtk=true'
manifest=$'rust=true\ndocs=false\nhygiene=false\ngui=true\nmsrv=true\ndeny=true\nts=false\nwebkitgtk=true'
workflow_all=$'rust=true\ndocs=true\nhygiene=true\ngui=true\nmsrv=true\ndeny=true\nts=true\nwebkitgtk=false'
all_true=$'rust=true\ndocs=true\nhygiene=true\ngui=true\nmsrv=true\ndeny=true\nts=true\nwebkitgtk=true'
codeql_none=$'codeql_rust=false\ncodeql_javascript_typescript=false\ncodeql_actions=false'
codeql_all=$'codeql_rust=true\ncodeql_javascript_typescript=true\ncodeql_actions=true'
codeql_rust_only=$'codeql_rust=true\ncodeql_javascript_typescript=false\ncodeql_actions=false'
codeql_js_only=$'codeql_rust=false\ncodeql_javascript_typescript=true\ncodeql_actions=false'
check_os_all='["ubuntu-latest","macos-latest","windows-latest"]'
check_os_documentation='["windows-latest"]'

# A developer checkout can contain unknown inputs. Prove the live fallback,
# then run clean-path exclusion controls in a separate tracked-byte snapshot.
# The unknown input is retained in place; it is never ignored or special-cased.
if [[ -n "$(git ls-files --others --exclude-standard)" ]]; then
    live_unknown="$("$router" local)"
    expect_flags "live untracked inputs select every hosted lane" "$all_true" "$live_unknown"
    if grep -Eq '^local_[^=]+=false$' <<<"$live_unknown"; then
        echo "FAIL: an unknown local input omitted a local gate" >&2
        exit 1
    fi
    echo "ok: live untracked inputs select every local gate"
fi
case "$(uname -s)" in
    MINGW* | MSYS*) python_command=python ;;
    *) python_command=python3 ;;
esac
"$python_command" -B "$repo_root/tools/test_ci_local.py" --tracked-snapshot "$temp_dir/bound"
cd "$temp_dir/bound"

empty_classification="$(printf '' | "$router" classify)"
expect_flags "empty diff skips conditional lanes" "$all_false" "$empty_classification"
expect_mermaid_flag "empty diff does not select Mermaid" false "$empty_classification"
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
expect_flags "docs corpus change includes its Rust embed consumers" "$docs_reader_rust" "$docs_classification"
expect_mermaid_flag "path-only Markdown classification fails safe without old/new content" true "$docs_classification"
expect_package_token "docs corpus selects the CLI consumer" keld-cli "$docs_classification"

# #624 contract cases. Each positive case has a paired negative control that
# differs in the one input the assertion depends on.
#
# Docs-only PR (the #612 diff): no Rust lane, no CodeQL language.
docs_only_pr="$(result_for_paths docs/specs/gh532-first-proof-evidence-rules.md docs/specs/kel74-compat-evidence-schema.md)"
expect_flags "docs-only PR runs only the docs lane" "$docs_only" "$docs_only_pr"
expect_codeql "docs-only PR selects no CodeQL language" "$codeql_none" "$docs_only_pr"
expect_no_package_selection "docs-only PR selects no package/suite" "$docs_only_pr"
# Negative control: the same directory with a Rust source extension selects
# the Rust analysis, so the docs-only result is not a constant.
docs_rust_source="$(result_for_paths docs/specs/gh532-first-proof-evidence-rules.md docs/specs/example.rs)"
expect_codeql "a *.rs path anywhere selects CodeQL rust only" "$codeql_rust_only" "$docs_rust_source"

# Reader-doc PR (the #610 diff): only the declared documentation reader runs,
# on Windows alone, so no Ubuntu leg installs WebKitGTK (keld-cli's own closure,
# keld-core -> keld-wv, links it on Linux).
reader_doc_pr="$(result_for_paths README.md docs/architecture/02-ipc.md docs/specs/gh527-worker-owned-blocking-call-transport.md llms-full.txt)"
expect_flags "reader-doc PR runs docs plus its Rust reader without Ubuntu GTK apt" "$docs_reader_rust" "$reader_doc_pr"
expect_exact_output "reader-doc PR selects keld-cli only" packages keld-cli "$reader_doc_pr"
expect_empty_output "reader-doc PR has no Ubuntu leg package set" ubuntu_packages "$reader_doc_pr"
expect_package_absent "reader-doc PR does not add keld-cli's reverse dependent" keld-host "$reader_doc_pr"
expect_codeql "reader-doc PR selects no CodeQL language" "$codeql_none" "$reader_doc_pr"
# Negative control: a real keld-cli source change in the same diff keeps
# today's reverse-dependent expansion.
reader_doc_with_source="$(result_for_paths README.md docs/architecture/02-ipc.md llms-full.txt crates/keld-cli/src/lib.rs)"
expect_package_token "reader doc plus CLI source still expands to keld-host" keld-host "$reader_doc_with_source"
expect_codeql "reader doc plus CLI source selects CodeQL rust" "$codeql_rust_only" "$reader_doc_with_source"
# A non-documentation input of the same registry edge (the recursive tools
# scan) is not a documentation read and keeps the expansion.
reader_doc_with_tool="$(result_for_paths llms-full.txt tools/atomic_protocol.rs)"
expect_package_token "registry edge with a tools source still expands to keld-host" keld-host "$reader_doc_with_tool"
# The package-scoped documentation edge narrows the same way.
package_doc_pr="$(result_for_paths docs/specs/kel53-full-package-activation.md)"
expect_exact_output "package documentation read selects its reader only" packages keld-update "$package_doc_pr"
expect_codeql "package documentation read selects no CodeQL language" "$codeql_none" "$package_doc_pr"
package_doc_with_source="$(result_for_paths docs/specs/kel53-full-package-activation.md crates/keld-update/src/lib.rs)"
expect_package_token "package documentation plus reader source still expands to keld-core" keld-core "$package_doc_with_source"

# crates/keld-cli/src change: reverse-dependent expansion exactly as before.
cli_source="$(result_for_paths crates/keld-cli/src/lib.rs)"
expect_package_token "CLI source change still expands to keld-host" keld-host "$cli_source"
expect_exact_output "CLI source change keeps the Ubuntu GTK selection" webkitgtk true "$cli_source"
expect_exact_output "CLI source change runs keld-cli and keld-host on Ubuntu" ubuntu_packages "keld-cli keld-host" "$cli_source"
expect_codeql "CLI source change selects CodeQL rust only" "$codeql_rust_only" "$cli_source"
# (Negative control: reader_doc_pr above omits keld-host for the same reader.)

# The JavaScript extractor's other file types, and an extensionless shebang
# script, select its analysis wherever they live.
for input in docs/diagrams/flow.dot crates/keld-cli/templates/hello/view.erb crates/keld-cli/src/x.xsjs .githooks/post-merge; do
    expect_exact_output "$input selects CodeQL javascript-typescript" codeql_javascript_typescript true "$(result_for_paths "$input")"
done
# A deleted or unreadable extensionless path may have been a script.
expect_exact_output "missing extensionless path selects CodeQL javascript-typescript" \
    codeql_javascript_typescript true "$(result_for_paths crates/keld-ipc/fuzz/corpus/raw_receive/removed-entry)"
# Negative control: an existing extensionless binary fixture without a shebang.
expect_codeql "extensionless binary fixture selects no CodeQL language" "$codeql_none" \
    "$(result_for_paths crates/keld-ipc/fuzz/corpus/raw_receive/ping-echo-session)"

# TypeScript source selects only its own analysis, even though the Rust lane
# runs for the crate that spawns those fixtures.
ts_codeql="$(result_for_paths packages/@keld/electron/src/link.ts)"
expect_codeql "TypeScript source selects CodeQL javascript-typescript only" "$codeql_js_only" "$ts_codeql"
expect_exact_output "TypeScript source still runs its Rust consumer lane" rust true "$ts_codeql"

# Workflow change: every CodeQL language.
workflow_codeql="$(result_for_paths .github/workflows/ci.yml)"
expect_codeql "workflow change selects every CodeQL language" "$codeql_all" "$workflow_codeql"
# Negative control: a non-workflow .github hygiene input selects none.
codeowners_codeql="$(result_for_paths .github/CODEOWNERS)"
expect_codeql "CODEOWNERS change selects no CodeQL language" "$codeql_none" "$codeowners_codeql"

# Unknown path: every lane, CodeQL included.
unknown_codeql="$(result_for_paths some-future-dir/thing.bin)"
expect_flags "unknown path selects every lane" "$all_true" "$unknown_codeql"
expect_codeql "unknown path selects every CodeQL language" "$codeql_all" "$unknown_codeql"
# (Negative control: docs_only_pr above is a known path that selects none.)
for input in tools/ci_changes.sh tools/ci_required.sh tools/ci_inputs.py tools/ci-inputs.json; do
    expect_codeql "router owner $input selects every CodeQL language" "$codeql_all" "$(result_for_paths "$input")"
done

# gitleaks configuration: the unconditional gitleaks job is its only reader.
for input in .gitleaks.toml .gitleaksignore; do
    gitleaks_config="$(result_for_paths "$input")"
    expect_flags "$input selects no conditional lane" "$all_false" "$gitleaks_config"
    expect_codeql "$input selects no CodeQL language" "$codeql_none" "$gitleaks_config"
    expect_exact_output "$input selects no workspace contracts" workspace false "$gitleaks_config"
    expect_no_package_selection "$input selects no package/suite" "$gitleaks_config"
done
# Negative control: an undeclared sibling name is still an unknown input.
gitleaks_sibling="$(result_for_paths .gitleaks.toml.orig)"
expect_flags "undeclared gitleaks sibling still fails safe" "$all_true" "$gitleaks_sibling"

# Workspace contracts: selected by their inputs and by every fallback, not by
# an ordinary Rust change.
for input in tools/workspace.py tools/test_workspace.py tools/session_closeout.py; do
    workspace_input="$(result_for_paths "$input")"
    expect_exact_output "$input selects the workspace contracts job" workspace true "$workspace_input"
    expect_flags "$input keeps the unknown fallback" "$all_true" "$workspace_input"
done
expect_exact_output "unknown path selects the workspace contracts job" workspace true "$unknown_codeql"
expect_exact_output "workflow edit selects the workspace contracts job" workspace true "$workflow_codeql"
# Negative controls: a Rust source change and a docs-only PR do not.
expect_exact_output "CLI source change does not select workspace contracts" workspace false "$cli_source"
expect_exact_output "docs-only PR does not select workspace contracts" workspace false "$docs_only_pr"

# Check OS list: Windows alone only when documentation reads alone selected
# Rust. The exact match also fails if Ubuntu or macOS is chosen instead.
expect_exact_output "reader-doc PR runs its reader on Windows only" check_os "$check_os_documentation" "$reader_doc_pr"
expect_exact_output "package documentation read runs on Windows only" check_os "$check_os_documentation" "$package_doc_pr"
expect_exact_output "package documentation read installs no Ubuntu GTK" webkitgtk false "$package_doc_pr"
expect_exact_output "reader-doc PR reports a documentation-only Rust selection" rust_documentation_only true "$reader_doc_pr"
expect_exact_output "package documentation read reports a documentation-only Rust selection" rust_documentation_only true "$package_doc_pr"
# Negative controls: code, mixed, docs-only (no Rust) and fallback selections.
for selection in "$cli_source" "$reader_doc_with_source" "$reader_doc_with_tool" "$package_doc_with_source" \
    "$docs_only_pr" "$workflow_codeql" "$unknown_codeql"; do
    expect_exact_output "non-documentation selection reports rust_documentation_only=false" \
        rust_documentation_only false "$selection"
done
expect_empty_output "package documentation read has no Ubuntu leg package set" ubuntu_packages "$package_doc_pr"
# Negative controls: any changed package, tools input, workflow or unknown path
# keeps all three OSes.
expect_exact_output "CLI source change runs on every OS" check_os "$check_os_all" "$cli_source"
expect_exact_output "reader doc plus CLI source runs on every OS" check_os "$check_os_all" "$reader_doc_with_source"
expect_exact_output "registry doc plus tools source runs on every OS" check_os "$check_os_all" "$reader_doc_with_tool"
expect_exact_output "package doc plus reader source runs on every OS" check_os "$check_os_all" "$package_doc_with_source"
expect_exact_output "TypeScript consumer change runs on every OS" check_os "$check_os_all" "$ts_codeql"
expect_exact_output "workflow edit runs on every OS" check_os "$check_os_all" "$workflow_codeql"
expect_exact_output "unknown path runs on every OS" check_os "$check_os_all" "$unknown_codeql"

# #632: doctests run for exactly the selected packages that have a library
# target; bin-only packages have none and `cargo test --doc` rejects them.
ipc_doctest="$(result_for_paths crates/keld-ipc/src/codec.rs)"
expect_exact_output "IPC source selects the doctest lane" doctest true "$ipc_doctest"
expect_output_package_token "IPC source doctests keld-ipc" doctest_packages keld-ipc "$ipc_doctest"
expect_output_package_token "IPC source doctests its library dependent keld-cli" doctest_packages keld-cli "$ipc_doctest"
expect_output_package_absent "IPC source does not doctest bin-only keld-host" doctest_packages keld-host "$ipc_doctest"
expect_package_token "IPC source still tests keld-host" keld-host "$ipc_doctest"
expect_exact_output "CLI source doctests keld-cli alone" doctest_packages keld-cli "$cli_source"
expect_exact_output "reader-doc PR doctests its reader" doctest_packages keld-cli "$reader_doc_pr"
# Negative controls: a bin-only package inside the selection is dropped, and a
# no-Rust diff selects no doctest. (Every crates/* path also reaches keld-cli
# through the registry edge, so the all-bin-only case uses the fake metadata
# repository below, whose packages have no library target.)
host_source="$(result_for_paths crates/keld-host/src/main.rs)"
expect_exact_output "keld-host change still tests keld-cli and keld-host" packages "keld-cli keld-host" "$host_source"
expect_exact_output "keld-host change doctests only the library keld-cli" doctest_packages keld-cli "$host_source"
expect_exact_output "docs-only PR skips the doctest lane" doctest false "$docs_only_pr"
expect_empty_output "docs-only PR doctests nothing" doctest_packages "$docs_only_pr"
# Fallbacks doctest every library package.
for selection in "$unknown_codeql" "$workflow_codeql"; do
    expect_exact_output "fallback selects the doctest lane" doctest true "$selection"
    expect_exact_output "fallback doctests every library package" doctest_packages \
        "keld-cli keld-compat keld-core keld-guard keld-ipc keld-native keld-pack keld-runtime keld-update keld-wv" "$selection"
done

audit_docs_classification="$(result_for_paths docs/audits/verify.py docs/audits/evidence/example.json)"
expect_flags "audit documentation without a Rust reader omits Rust" "$docs_only" "$audit_docs_classification"

hygiene_classification="$(result_for_paths .github/CODEOWNERS)"
expect_flags "hygiene input runs only hygiene contract" "$hygiene_only" "$hygiene_classification"

atomic_checker_classification="$(result_for_paths tools/atomic_protocol.rs)"
expect_flags "atomic checker also feeds the CLI error registry" "$hygiene_rust" "$atomic_checker_classification"
expect_package_token "recursive tools scan selects its CLI reader" keld-cli "$atomic_checker_classification"

justfile_contract_classification="$(result_for_paths tools/justfile_contract.rs)"
expect_flags "shared parser feeds hygiene and Rust registry" "$hygiene_rust" "$justfile_contract_classification"
expect_mermaid_flag "shared justfile parser re-checks the Mermaid gate contract" true "$justfile_contract_classification"
expect_package_token "shared parser selects CLI registry" keld-cli "$justfile_contract_classification"

agent_context_classification="$(result_for_paths tools/agent_context.rs tools/markdown_contract.rs .agents/instruction-budget.tsv)"
expect_flags "instruction checker feeds hygiene and registry" "$hygiene_rust" "$agent_context_classification"

agent_instruction_classification="$(result_for_paths AGENTS.md crates/keld-wv/AGENTS.md .agents/index.md .agents/skills/instruction-review/SKILL.md .agents/new.txt docs/agents/workflow.md)"
expect_flags "crate documentation can also feed Rust readers" "$docs_hygiene_rust" "$agent_instruction_classification"

agent_assembly_classification="$(result_for_paths .codex/config.toml)"
expect_flags "agent assembly config runs merge-blocking hygiene" "$hygiene_only" "$agent_assembly_classification"
expect_no_package_selection "agent assembly config selects no package/suite" "$agent_assembly_classification"

host_classification="$(result_for_paths crates/keld-ipc/src/lib.rs)"
expect_flags "IPC source also feeds Bun constant assertions" "$ipc_fixture_flags" "$host_classification"
expect_package_token "IPC change includes host consumer" keld-host "$host_classification"

compat_classification="$(result_for_paths crates/keld-compat/src/lib.rs)"
expect_flags "compat-only change skips GUI smoke but installs GTK for its selected test closure" "$compat_flags" "$compat_classification"
expect_package_token "compat-only change includes its owner package" keld-compat "$compat_classification"

compat_test_classification="$(result_for_paths crates/keld-compat/tests/electron_lifecycle.rs)"
expect_flags "Rust-only change does not select the TypeScript lane" "$compat_flags" "$compat_test_classification"
expect_empty_output "Rust-only change selects no Bun suite" ts_packages "$compat_test_classification"

ipc_fixture_classification="$(result_for_paths crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv)"
expect_flags "shared IPC fixture change runs the Bun lane" "$ipc_fixture_flags" "$ipc_fixture_classification"
expect_output_package_token "shared IPC fixture change selects its consuming Bun suite" ts_packages packages/@keld/electron "$ipc_fixture_classification"

ts_classification="$(result_for_paths packages/@keld/electron/src/link.ts)"
expect_flags "TypeScript package change runs the Bun lane and its Rust consumer only" "$ts_flags" "$ts_classification"
expect_output_package_token "TypeScript package change selects its Bun suite root" ts_packages packages/@keld/electron "$ts_classification"
expect_package_token "TypeScript package change re-runs the crate that spawns its fixtures" keld-compat "$ts_classification"

kipc_classification="$(result_for_paths packages/@keld/kipc/src/transport.ts)"
expect_flags "shared kipc transport change runs the Bun lane and its Rust embed consumers" "$ts_flags" "$kipc_classification"
expect_output_package_token "shared kipc transport change selects its Bun suite root" ts_packages packages/@keld/kipc "$kipc_classification"
expect_package_token "shared kipc transport change re-runs the crate that embeds it" keld-cli "$kipc_classification"

ts_fixture_classification="$(result_for_paths packages/@keld/electron/fixtures/app_ready.ts)"
expect_flags "TypeScript fixture change routes like its owning package" "$ts_flags" "$ts_fixture_classification"
expect_output_package_token "TypeScript fixture change selects the owning Bun suite root" ts_packages packages/@keld/electron "$ts_fixture_classification"
expect_package_token "TypeScript fixture change re-runs its Rust conformance consumer" keld-compat "$ts_fixture_classification"

ts_docs_classification="$(result_for_paths packages/@keld/electron/README.md)"
ts_docs_flags=$'rust=true\ndocs=true\nhygiene=false\ngui=false\nmsrv=false\ndeny=false\nts=true\nwebkitgtk=true'
expect_flags "package input scope conservatively includes documentation readers" "$ts_docs_flags" "$ts_docs_classification"

wv_classification="$(result_for_paths crates/keld-wv/src/lib.rs)"
expect_flags "keld-wv change enables GUI smoke and Ubuntu WebKitGTK apt" "$wv_flags" "$wv_classification"
expect_package_token "keld-wv change includes host consumer" keld-host "$wv_classification"

wv_fuzz_classification="$(result_for_paths crates/keld-wv/fuzz/Cargo.toml)"
expect_flags "keld-wv fuzz workspace routes through the owning Rust closure" "$wv_flags" "$wv_fuzz_classification"
expect_package_token "keld-wv fuzz workspace includes host consumer" keld-host "$wv_fuzz_classification"

update_fuzz_classification="$(result_for_paths crates/keld-update/fuzz/Cargo.toml)"
expect_flags "keld-update fuzz workspace routes through the owning Rust closure" "$runtime_flags" "$update_fuzz_classification"
expect_package_token "keld-update fuzz workspace includes its owner package" keld-update "$update_fuzz_classification"
# keld-core reads keld-update on Windows (KEL-254 T3 Part B), so the closure reaches the host.
expect_package_token "keld-update fuzz workspace includes host consumer" keld-host "$update_fuzz_classification"

# The host_identity fuzz target reads keld-pack, so a keld-pack edit must reach
# the same rust-routed lane that builds the keld-update fuzz workspace.
pack_classification="$(result_for_paths crates/keld-pack/src/host_identity.rs)"
expect_flags "keld-pack change routes the Rust lane that builds the keld-update fuzz workspace" "$runtime_flags" "$pack_classification"
expect_package_token "keld-pack change includes its keld-update consumer" keld-update "$pack_classification"

# The elevated updater helper's ban list is a cargo-deny input as well as a crate file
# that its edge-set test reads (KEL-53 §4, KEL-270 T4d S9c); its other files are not.
helper_deny_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=false\nmsrv=true\ndeny=true\nts=false\nwebkitgtk=true'
helper_source_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=false\nmsrv=true\ndeny=false\nts=false\nwebkitgtk=true'
helper_deny_classification="$(result_for_paths crates/keld-updater-helper/deny.toml)"
expect_flags "updater helper ban list routes cargo-deny and the helper's Rust lane" "$helper_deny_flags" "$helper_deny_classification"
expect_package_token "updater helper ban list selects the helper" keld-updater-helper "$helper_deny_classification"
helper_source_classification="$(result_for_paths crates/keld-updater-helper/src/main.rs)"
expect_flags "updater helper source routes only the Rust lane" "$helper_source_flags" "$helper_source_classification"
expect_package_token "updater helper source selects the helper" keld-updater-helper "$helper_source_classification"

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
expect_mermaid_flag "router and required-result changes run the full Mermaid lane" true "$router_script_classification"
for input in tools/ci_inputs.py tools/ci_local.py tools/test_ci_local.py tools/ci-inputs.json; do
    helper_classification="$(result_for_paths "$input")"
    expect_flags "router owner $input selects every job without duplicate GTK apt" "$workflow_all" "$helper_classification"
done

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

cd "$repo_root"
git -C "$temp_dir" init -q
git -C "$temp_dir" config user.email ci-router@example.invalid
git -C "$temp_dir" config user.name ci-router-test
mkdir -p "$temp_dir/crates/keld-runtime/src" "$temp_dir/fake-bin" "$temp_dir/tools"
printf '%s\n' '{"schema":"keld.ci-inputs/v1","known_inputs":["*"],"reader_sets":{},"consumers":[]}' >"$temp_dir/tools/ci-inputs.json"
mkdir -p "$temp_dir/packages/@fake/bootstrap/src"
printf '{"name":"@fake/bootstrap"}\n' >"$temp_dir/packages/@fake/bootstrap/package.json"
printf 'import { test } from "bun:test";\n' >"$temp_dir/packages/@fake/bootstrap/src/unit.test.ts"
printf 'fake-bin/\ntarget/\nbound/\n' >"$temp_dir/.gitignore"
printf 'base\n' >"$temp_dir/README.md"
git -C "$temp_dir" add README.md .gitignore tools/ci-inputs.json packages
git -C "$temp_dir" commit -qm base
base_sha="$(git -C "$temp_dir" rev-parse HEAD)"
real_jq="$(command -v jq)"

# The production script has no bypass/override: it always asks `cargo metadata`.
# This temporary executable is a controlled external dependency fixture so PR/push
# diff tests can create a minimal Git repository without copying the Keld workspace.
# It emits JSON-escaped drive-qualified paths on Windows and native Unix paths
# on Unix. Together the matrix pins Windows normalization without inventing a
# root-relative path that cannot match a drive-qualified repository root.
printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    'root="$(git rev-parse --show-toplevel)"' \
    'literal_package=""' \
    'case "$root" in' \
    '  /*)' \
    '    json_unix_root="${root//\\/\\\\}"' \
    '    literal_package=",{\"name\":\"keld-literal\",\"manifest_path\":\"${json_unix_root}/crates/literal\\\\component/Cargo.toml\",\"dependencies\":[]}"' \
    '    ;;' \
    'esac' \
    'case "$root" in' \
    '  [A-Za-z]:/*)' \
    '    root="${root//\//\\}"' \
    '    root="${root//\\/\\\\}"' \
    '    ;;' \
    '  *) root="${root//\\/\\\\}" ;;' \
    'esac' \
    'printf "{\\\"packages\\\":[{\\\"name\\\":\\\"keld-host\\\",\\\"manifest_path\\\":\\\"%s/crates/keld-host/Cargo.toml\\\",\\\"dependencies\\\":[{\\\"name\\\":\\\"keld-core\\\",\\\"path\\\":\\\"%s/crates/keld-core\\\"}]},{\\\"name\\\":\\\"keld-core\\\",\\\"manifest_path\\\":\\\"%s/crates/keld-core/Cargo.toml\\\",\\\"dependencies\\\":[{\\\"name\\\":\\\"keld-ipc\\\",\\\"path\\\":\\\"%s/crates/keld-ipc\\\"}]},{\\\"name\\\":\\\"keld-ipc\\\",\\\"manifest_path\\\":\\\"%s/crates/keld-ipc/Cargo.toml\\\",\\\"dependencies\\\":[]},{\\\"name\\\":\\\"keld-runtime\\\",\\\"manifest_path\\\":\\\"%s/crates/keld-runtime/Cargo.toml\\\",\\\"dependencies\\\":[],\\\"targets\\\":[{\\\"kind\\\":[\\\"lib\\\"]}]}%s]}\\n" "$root" "$root" "$root" "$root" "$root" "$root" "$literal_package"' \
    >"$temp_dir/fake-bin/cargo"
chmod +x "$temp_dir/fake-bin/cargo"

# Native Windows jq emits CRLF. Force that representation in the isolated
# fixture on every OS so line-oriented metadata consumers cannot regress only
# on Windows while Linux CI stays green.
printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    "\"$real_jq\" \"\$@\" | sed 's/\$/\\r/'" \
    >"$temp_dir/fake-bin/jq"
chmod +x "$temp_dir/fake-bin/jq"

printf 'runtime\n' >"$temp_dir/crates/keld-runtime/src/lib.rs"
git -C "$temp_dir" add crates/keld-runtime/src/lib.rs
git -C "$temp_dir" commit -qm runtime
runtime_sha="$(git -C "$temp_dir" rev-parse HEAD)"
pr_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=pull_request KELD_CI_BASE_SHA="$base_sha" KELD_CI_HEAD_SHA="$runtime_sha" "$router" github)"
fake_runtime_flags=$'rust=true\ndocs=false\nhygiene=false\ngui=false\nmsrv=true\ndeny=false\nts=false\nwebkitgtk=false'
expect_flags "pull-request base/head classifies the actual diff" "$fake_runtime_flags" "$pr_result"
expect_package_token "pull-request base/head selects changed package" keld-runtime "$pr_result"
expect_output_package_token "pull-request base/head selects the same Ubuntu package" ubuntu_packages keld-runtime "$pr_result"
expect_exact_output "pull-request Rust change selects the doctest lane" doctest true "$pr_result"
expect_exact_output "pull-request Rust change doctests its library package" doctest_packages keld-runtime "$pr_result"
# Push mode: a Rust change selects doctests; the docs-only push below is the
# negative control, and so is this push's own no-library case further down.
push_rust_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA="$base_sha" GITHUB_SHA="$runtime_sha" "$router" github)"
expect_exact_output "push Rust change selects the doctest lane" doctest true "$push_rust_result"
expect_exact_output "push Rust change doctests its library package" doctest_packages keld-runtime "$push_rust_result"

# Fail closed: a library-target read that fails must stop the router before it
# publishes, even though github mode clears errexit inside its substitution.
# Artifacts stay under the fixture's git-ignored target/ so later local-mode
# cases still see a clean checkout.
failing_jq_dir="$temp_dir/target/failing-jq"
mkdir -p "$failing_jq_dir"
printf '%s\n' '#!/usr/bin/env bash' \
    'for arg in "$@"; do case "$arg" in *proc-macro*) exit 7 ;; esac; done' \
    "exec \"$temp_dir/fake-bin/jq\" \"\$@\"" >"$failing_jq_dir/jq"
chmod +x "$failing_jq_dir/jq"
rm -f "$temp_dir/target/failed-library-output"
if failed_library="$(cd "$temp_dir" && PATH="$failing_jq_dir:$temp_dir/fake-bin:$PATH" GITHUB_OUTPUT="$temp_dir/target/failed-library-output" KELD_CI_EVENT_NAME=pull_request KELD_CI_BASE_SHA="$base_sha" KELD_CI_HEAD_SHA="$runtime_sha" "$router" github 2>&1)"; then
    echo "FAIL: a failed library-target read published a router selection" >&2
    printf '%s\n' "$failed_library" >&2
    exit 1
fi
if grep -q '^rust=' <<<"$failed_library" || [[ -e "$temp_dir/target/failed-library-output" ]]; then
    echo "FAIL: a failed library-target read wrote router outputs" >&2
    exit 1
fi
if ! grep -Fq "ci router: cannot list library packages from cargo metadata" <<<"$failed_library"; then
    echo "FAIL: a failed library-target read did not report the fail-closed router error" >&2
    printf '%s\n' "$failed_library" >&2
    exit 1
fi
echo "ok: a failed library-target read fails the router before any output"
# Negative control: the same invocation with a working jq publishes doctests.
rm -f "$temp_dir/target/library-output"
(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" GITHUB_OUTPUT="$temp_dir/target/library-output" KELD_CI_EVENT_NAME=pull_request KELD_CI_BASE_SHA="$base_sha" KELD_CI_HEAD_SHA="$runtime_sha" "$router" github >/dev/null)
if ! grep -Fxq 'doctest=true' "$temp_dir/target/library-output"; then
    echo "FAIL: the working library-target read did not publish the doctest selection" >&2
    exit 1
fi
echo "ok: a working library-target read publishes the doctest selection"

# A backslash is a legal Unix filename byte, not a path separator. On Unix,
# prove ingestion preserves an embedded backslash. Windows cannot create this
# name, while its matrix run independently exercises drive-qualified metadata.
case "$(uname -s)" in
    MINGW* | MSYS*) ;;
    *)
        literal_dir='crates/literal\component'
        mkdir -p "$temp_dir/$literal_dir/src"
        printf 'literal\n' >"$temp_dir/$literal_dir/src/lib.rs"
        git -C "$temp_dir" add -- "$literal_dir/src/lib.rs"
        git -C "$temp_dir" commit -qm literal-backslash
        literal_sha="$(git -C "$temp_dir" rev-parse HEAD)"
        literal_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=pull_request KELD_CI_BASE_SHA="$runtime_sha" KELD_CI_HEAD_SHA="$literal_sha" "$router" github)"
        expect_flags "Unix backslash component remains one package path" "$fake_runtime_flags" "$literal_result"
        expect_package_token "Unix backslash component selects its exact package" keld-literal "$literal_result"
        runtime_sha=$literal_sha
        echo "ok: Unix backslash component is not normalized as a Windows separator"
        ;;
esac

mkdir -p "$temp_dir/docs"
printf 'docs\n' >"$temp_dir/docs/guide.md"
git -C "$temp_dir" add docs/guide.md
git -C "$temp_dir" commit -qm docs
docs_sha="$(git -C "$temp_dir" rev-parse HEAD)"
push_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA="$runtime_sha" GITHUB_SHA="$docs_sha" "$router" github)"
expect_flags "push before/head classifies the actual diff" "$docs_only" "$push_result"
expect_exact_output "docs-only push selects no doctest" doctest false "$push_result"
expect_empty_output "docs-only push doctests nothing" doctest_packages "$push_result"
expect_mermaid_flag "prose-only docs outside diagrams skip Mermaid" false "$push_result"
expect_codeql "docs-only push still analyses every CodeQL language" "$codeql_all" "$push_result"
# Negative control: the identical docs-only diff as a pull request skips CodeQL.
docs_pr_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=pull_request KELD_CI_BASE_SHA="$runtime_sha" KELD_CI_HEAD_SHA="$docs_sha" "$router" github)"
expect_flags "docs-only pull request classifies the same diff" "$docs_only" "$docs_pr_result"
expect_codeql "docs-only pull request selects no CodeQL language" "$codeql_none" "$docs_pr_result"

cat >"$temp_dir/diagram.md" <<'MERMAID'
# Diagram

```mermaid
flowchart LR
    accTitle: Sample
    accDescr: Sample diagram for CI routing.
    A["external"] --> B["target"]
```
MERMAID
git -C "$temp_dir" add diagram.md
git -C "$temp_dir" commit -qm diagram
diagram_sha="$(git -C "$temp_dir" rev-parse HEAD)"
diagram_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA="$docs_sha" GITHUB_SHA="$diagram_sha" "$router" github)"
expect_mermaid_flag "diagram addition selects Mermaid" true "$diagram_result"

python3 - "$temp_dir/diagram.md" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
path.write_text(path.read_text().replace("# Diagram", "# Prose-only edit"))
PY
prose_diagram_sha="$(git -C "$temp_dir" add diagram.md && git -C "$temp_dir" commit -qm prose-in-diagram-file && git -C "$temp_dir" rev-parse HEAD)"
prose_diagram_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA="$diagram_sha" GITHUB_SHA="$prose_diagram_sha" "$router" github)"
expect_mermaid_flag "prose-only edit in a diagram file skips Mermaid" false "$prose_diagram_result"

python3 - "$temp_dir/diagram.md" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
path.write_text(path.read_text().replace('A["external"]', 'A["changed external"]'))
PY
changed_diagram_sha="$(git -C "$temp_dir" add diagram.md && git -C "$temp_dir" commit -qm diagram-content && git -C "$temp_dir" rev-parse HEAD)"
changed_diagram_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA="$prose_diagram_sha" GITHUB_SHA="$changed_diagram_sha" "$router" github)"
expect_mermaid_flag "diagram body change selects Mermaid" true "$changed_diagram_result"

git -C "$temp_dir" mv diagram.md renamed.md
renamed_diagram_sha="$(git -C "$temp_dir" commit -qm diagram-rename && git -C "$temp_dir" rev-parse HEAD)"
renamed_diagram_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA="$changed_diagram_sha" GITHUB_SHA="$renamed_diagram_sha" "$router" github)"
expect_mermaid_flag "diagram file rename selects Mermaid" true "$renamed_diagram_result"

python3 - "$temp_dir/renamed.md" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
path.write_text(path.read_text().replace("```mermaid", "```mermaidx"))
PY
malformed_diagram_sha="$(git -C "$temp_dir" add renamed.md && git -C "$temp_dir" commit -qm malformed-diagram && git -C "$temp_dir" rev-parse HEAD)"
malformed_diagram_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA="$renamed_diagram_sha" GITHUB_SHA="$malformed_diagram_sha" "$router" github)"
expect_mermaid_flag "malformed Mermaid fence change selects validation" true "$malformed_diagram_result"

# A packages/ diff with no Bun suite anywhere must fail the router, not emit a
# selected-but-empty TypeScript lane. This runs before the suite fixture below
# exists, which is the only moment that state is reachable.
mkdir -p "$temp_dir/packages/@fake/untested/src"
printf '{"name":"@fake/untested","type":"module"}\n' >"$temp_dir/packages/@fake/untested/package.json"
printf 'export const noop = () => {};\n' >"$temp_dir/packages/@fake/untested/src/index.ts"
git -C "$temp_dir" add packages
git -C "$temp_dir" commit -qm untested
untested_sha="$(git -C "$temp_dir" rev-parse HEAD)"
mv "$temp_dir/packages/@fake/bootstrap/src/unit.test.ts" "$temp_dir/fake-bin/bootstrap-test"
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
mv "$temp_dir/fake-bin/bootstrap-test" "$temp_dir/packages/@fake/bootstrap/src/unit.test.ts"

# Pins the router's suite-discovery set against bun's own, per filename shape.
#
# Every fixture elsewhere in this file uses `unit.test.ts`, the single shape the
# original pattern matched - so a wrong pattern stayed invisible. Measured on
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
# 16 could be deleted individually with this suite still green - a pin that
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
echo "ok: router suite discovery matches the active Bun runtime across all 32 patterns, 4 case variants and 3 skipped shapes"

# A crate fixture with no Bun test consumer must not select the TypeScript
# lane merely because it lives under tests/fixtures. The matching case below
# then proves the edge is derived from the checked-out test reference.
fixture_path='crates/keld-ipc/tests/fixtures/shared.tsv'
fixture_without_consumer="$(cd "$temp_dir" && printf '%s\0' "$fixture_path" | PATH="$temp_dir/fake-bin:$PATH" "$router" classify)"
fake_fixture_without_consumer=$'rust=true\ndocs=false\nhygiene=false\ngui=true\nmsrv=true\ndeny=false\nts=false\nwebkitgtk=false'
expect_flags "unreferenced crate fixture does not invent a Bun consumer" "$fake_fixture_without_consumer" "$fixture_without_consumer"
expect_empty_output "unreferenced crate fixture selects no Bun suite" ts_packages "$fixture_without_consumer"
# A Rust selection whose packages (keld-ipc and its dependents here) have no
# library target selects no doctest.
expect_exact_output "Rust selection without a library target selects no doctest" doctest false "$fixture_without_consumer"
expect_empty_output "Rust selection without a library target doctests nothing" doctest_packages "$fixture_without_consumer"

# A Bun suite the Keld workspace does not own: this fixture proves the lane is
# derived from the checked-out packages/ tree, not from a hard-coded path.
mkdir -p "$temp_dir/packages/@fake/pkg/src"
printf '{"name":"@fake/pkg","type":"module"}\n' >"$temp_dir/packages/@fake/pkg/package.json"
printf 'import { test } from "bun:test"; // %s\n' "$fixture_path" >"$temp_dir/packages/@fake/pkg/src/unit.test.ts"
git -C "$temp_dir" add packages
git -C "$temp_dir" commit -qm packages
ts_sha="$(git -C "$temp_dir" rev-parse HEAD)"
ts_pr_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=pull_request KELD_CI_BASE_SHA="$untested_sha" KELD_CI_HEAD_SHA="$ts_sha" "$router" github)"
fake_ts_flags=$'rust=false\ndocs=false\nhygiene=false\ngui=false\nmsrv=false\ndeny=false\nts=true\nwebkitgtk=false'
expect_flags "pull-request TypeScript-only diff runs the Bun lane with no Rust consumer" "$fake_ts_flags" "$ts_pr_result"
expect_output_package_token "pull-request TypeScript-only diff selects the changed Bun suite" ts_packages 'packages/@fake/pkg' "$ts_pr_result"
expect_empty_packages "pull-request TypeScript-only diff selects no workspace package" "$ts_pr_result"

fixture_with_consumer="$(cd "$temp_dir" && printf '%s\0' "$fixture_path" | PATH="$temp_dir/fake-bin:$PATH" "$router" classify)"
fake_fixture_with_consumer=$'rust=true\ndocs=false\nhygiene=false\ngui=true\nmsrv=true\ndeny=false\nts=true\nwebkitgtk=false'
expect_flags "crate fixture discovers a checked-out Bun test consumer" "$fake_fixture_with_consumer" "$fixture_with_consumer"
expect_output_package_token "crate fixture selects the discovered Bun suite" ts_packages 'packages/@fake/pkg' "$fixture_with_consumer"

ts_push_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA="$untested_sha" GITHUB_SHA="$ts_sha" "$router" github)"
expect_flags "push TypeScript-only diff runs the Bun lane" "$fake_ts_flags" "$ts_push_result"
expect_output_package_token "push TypeScript-only diff selects the changed Bun suite" ts_packages 'packages/@fake/pkg' "$ts_push_result"

empty_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA="$ts_sha" GITHUB_SHA="$ts_sha" "$router" github)"
expect_flags "same push base/head is an empty diff" "$all_false" "$empty_result"
expect_mermaid_flag "same push base/head skips Mermaid" false "$empty_result"
expect_codeql "an empty push still analyses every CodeQL language" "$codeql_all" "$empty_result"

git -C "$temp_dir" update-ref refs/remotes/origin/main "$ts_sha"
local_plain_markdown="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_BASE_REF="$ts_sha" "$router" local)"
expect_mermaid_flag "local clean/source-only state skips unchanged Mermaid blocks" false "$local_plain_markdown"
printf '# Untracked prose only\n' >"$temp_dir/local.md"
local_untracked_prose="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_BASE_REF="$ts_sha" "$router" local)"
expect_mermaid_flag "untracked input has no established contract and fails closed" true "$local_untracked_prose"
cat >"$temp_dir/local.md" <<'MERMAID'
# Local diagram

```mermaid
flowchart LR
    accTitle: Local sample
    accDescr: Local untracked diagram for route selection.
    A["external"] --> B["target"]
```
MERMAID
local_untracked_diagram="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_BASE_REF="$ts_sha" "$router" local)"
expect_mermaid_flag "local untracked diagram selects Mermaid" true "$local_untracked_diagram"
local_unknown_base="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_BASE_REF=missing-base "$router" local)"
expect_mermaid_flag "local unknown base fails safe to full Mermaid" true "$local_unknown_base"

mkdir -p "$temp_dir/failing-git"
real_git="$(command -v git)"
printf '%s\n' '#!/usr/bin/env bash' 'if [[ "$1" == diff ]]; then exit 31; fi' \
    "exec \"$real_git\" \"\$@\"" >"$temp_dir/failing-git/git"
chmod +x "$temp_dir/failing-git/git"
if failed_diff="$(cd "$temp_dir" && PATH="$temp_dir/failing-git:$temp_dir/fake-bin:$PATH" KELD_CI_BASE_REF="$ts_sha" "$router" local 2>&1)"; then
    echo "FAIL: successful untracked census hid a failed tracked diff" >&2
    exit 1
fi
if grep -q '^rust=' <<<"$failed_diff"; then
    echo "FAIL: failed Git comparison published a selection" >&2
    exit 1
fi
echo "ok: tracked diff failure cannot publish a skipped-green plan"

unknown_base_result="$(cd "$temp_dir" && PATH="$temp_dir/fake-bin:$PATH" KELD_CI_EVENT_NAME=push KELD_CI_BEFORE_SHA=0000000000000000000000000000000000000000 GITHUB_SHA="$docs_sha" "$router" github)"
fake_all_true=$'rust=true\ndocs=true\nhygiene=true\ngui=true\nmsrv=true\ndeny=true\nts=true\nwebkitgtk=true'
expect_flags "missing comparison base fails safe" "$fake_all_true" "$unknown_base_result"
expect_mermaid_flag "missing comparison base runs the full Mermaid lane" true "$unknown_base_result"
expect_output_package_token "missing comparison base still exercises the Bun lane" ts_packages 'packages/@fake/pkg' "$unknown_base_result"
expect_codeql "missing comparison base selects every CodeQL language" "$codeql_all" "$unknown_base_result"

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

# These contracts need only Python and real Git, including on hosted runners
# where Just is deliberately not installed. Local ci-router-test also exercises
# the real Just executor separately.
case "$(uname -s)" in
    MINGW* | MSYS*) python_command=python ;;
    *) python_command=python3 ;;
esac
"$python_command" -B "$repo_root/tools/test_ci_local.py" InputContractTests ProductionConsumerTests FreshnessGateTests RouterFailureBoundaryTests

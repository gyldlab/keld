#!/usr/bin/env bash
# Agent playbook router and unsafe-coverage inventory (root AGENTS.md:
# `just agents-md`). The one copy of this check: the justfile recipe and the
# hosted change-router job both run this script (#650).
set -euo pipefail

# Self-test: a copy of this script runs against fixture trees (#650).
if [[ "${1:-}" == test ]]; then
    self="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/$(basename "${BASH_SOURCE[0]}")"
    temp="$(mktemp -d "${TMPDIR:-/tmp}/keld-agents-md.XXXXXX")"
    trap 'chmod -R u+rwx "$temp" 2>/dev/null; rm -rf "$temp"' EXIT
    fixture() {
        rm -rf "$temp/repo"
        mkdir -p "$temp/repo/tools" "$temp/repo/.agents" "$temp/repo/crates/demo/src"
        cp "$self" "$temp/repo/tools/agents_md.sh"
        for playbook in testing.md research.md dependencies.md; do
            printf 'x\n' >"$temp/repo/.agents/$playbook"
            printf -- '- [%s](%s)\n' "$playbook" "$playbook" >>"$temp/repo/.agents/index.md"
        done
        printf 'pub fn safe() {}\n' >"$temp/repo/crates/demo/src/lib.rs"
    }
    expect() {
        local label="$1" want="$2" status=0
        "$temp/repo/tools/agents_md.sh" >"$temp/out" 2>&1 || status=$?
        if [[ "$want" == pass && "$status" -ne 0 ]] || [[ "$want" == fail && "$status" -eq 0 ]]; then
            cat "$temp/out" >&2
            echo "agents-md self-test '$label': expected $want, exit $status" >&2
            exit 1
        fi
        echo "ok: $label"
    }
    fixture
    expect "a crate without unsafe passes" pass
    printf 'unsafe fn f() {}\n' >"$temp/repo/crates/demo/src/lib.rs"
    expect "unsafe without a crate AGENTS.md fails" fail
    printf 'rules\n' >"$temp/repo/crates/demo/AGENTS.md"
    expect "unsafe with a crate AGENTS.md passes" pass
    # Negative control: an unreadable crate directory is a grep error, not a
    # clean scan. Skipped only when the runner reads it anyway (root).
    fixture
    mkdir -p "$temp/repo/crates/locked/src"
    printf 'unsafe fn hidden() {}\n' >"$temp/repo/crates/locked/src/lib.rs"
    chmod 000 "$temp/repo/crates/locked"
    if [[ -r "$temp/repo/crates/locked" ]]; then
        echo "agents-md self-test: cannot make a directory unreadable as this user; the grep-error case needs a non-root runner" >&2
        exit 1
    fi
    expect "an unreadable crates/ directory fails instead of passing" fail
    grep -q "grep failed" "$temp/out" || { cat "$temp/out" >&2; echo "agents-md self-test: grep error was not reported" >&2; exit 1; }
    echo "agents-md self-tests ok"
    exit 0
fi

# Paths below are repository-relative.
cd "$(dirname "${BASH_SOURCE[0]}")/.."
fail=0
if [[ ! -f ".agents/index.md" ]]; then
    echo "error: .agents/index.md is missing (create the agent playbook router)"
    fail=1
fi
for playbook in testing.md research.md dependencies.md; do
    if [[ ! -f ".agents/$playbook" ]]; then
        echo "error: .agents/$playbook is missing (restore the expected agent playbook)"
        fail=1
    elif [[ -f ".agents/index.md" ]] && ! grep -Fq "($playbook)" ".agents/index.md"; then
        echo "error: .agents/index.md does not link $playbook (add it to the task router)"
        fail=1
    fi
done
matcher='allow\([[:space:]]*unsafe_code[[:space:]]*\)|unsafe[[:space:]]*(extern|fn|impl|trait|\{)'
# String fixtures — do not plant unsafe in crates/ just to exercise the matcher.
# Arrays (not heredocs): just 1.58+ still lexes heredoc bodies as justfile syntax.
must_match=(
    '#[allow(unsafe_code)]'
    '#[allow( unsafe_code )]'
    'unsafe extern "C" fn f()'
    'unsafe fn f()'
    'unsafe impl Foo {}'
    'unsafe trait Bar {}'
    'unsafe { }'
    'unsafe{'
)
for sample in "${must_match[@]}"; do
    if ! printf '%s\n' "$sample" | grep -E -q "$matcher"; then
        echo "error: agents-md matcher missed fixture: $sample"
        fail=1
    fi
done
must_not_match=(
    'fn safe() {}'
    '#[allow(dead_code)]'
    'extern "C" fn f()'
)
for sample in "${must_not_match[@]}"; do
    if printf '%s\n' "$sample" | grep -E -q "$matcher"; then
        echo "error: agents-md matcher false-positive: $sample"
        fail=1
    fi
done
# grep exits 1 when nothing matches, which is fine; 2 or more is an error
# (unreadable tree, I/O failure) and must not pass the unsafe check.
grep_status=0
files=$(grep -R -l -E "$matcher" crates --include='*.rs') || grep_status=$?
if [[ "$grep_status" -gt 1 ]]; then
    echo "error: grep failed (exit $grep_status) while scanning crates/ for unsafe; refusing to pass the unsafe-coverage check"
    exit 1
fi
crates=$(printf '%s\n' "$files" | awk -F/ '$1=="crates" && NF>=2 {print $2}' | sort -u)
for crate in $crates; do
    if [[ ! -f "crates/$crate/AGENTS.md" ]]; then
        echo "error: crates/$crate uses unsafe but has no AGENTS.md (root AGENTS.md § Working invariants)"
        fail=1
    fi
done
if [[ "$fail" -ne 0 ]]; then exit 1; fi
echo "agents-md ok"

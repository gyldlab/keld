#!/usr/bin/env bash
# Agent playbook router and unsafe-coverage inventory (root AGENTS.md:
# `just agents-md`). The one copy of this check: the justfile recipe and the
# hosted change-router job both run this script (#650).
set -euo pipefail
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
files=$(grep -R -l -E "$matcher" crates --include='*.rs' || true)
crates=$(printf '%s\n' "$files" | awk -F/ '$1=="crates" && NF>=2 {print $2}' | sort -u)
for crate in $crates; do
    if [[ ! -f "crates/$crate/AGENTS.md" ]]; then
        echo "error: crates/$crate uses unsafe but has no AGENTS.md (root AGENTS.md § Working invariants)"
        fail=1
    fi
done
if [[ "$fail" -ne 0 ]]; then exit 1; fi
echo "agents-md ok"

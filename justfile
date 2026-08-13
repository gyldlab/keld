# Keld task runner — mirrors the CI gates in .github/workflows/ci.yml.
# `just ci` before pushing == what CI will run (minus the 3-OS matrix).

# Open hello window (macOS only, Phase 1 slice).
hello:
    cargo run -p keld-host -- --hello

# Run every CI gate locally (deny requires `cargo install cargo-deny --locked`).
ci: agents-md llms-test llms-check fmt-check clippy test doc deny

# Check playbook routing and require crate AGENTS.md wherever Rust opts into unsafe.
agents-md:
    #!/usr/bin/env bash
    set -euo pipefail
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
    files=$(grep -R -l -E 'allow\(unsafe_code\)|unsafe[[:space:]]+(fn|impl|trait|\{)' crates --include='*.rs' || true)
    crates=$(printf '%s\n' "$files" | awk -F/ '$1=="crates" && NF>=2 {print $2}' | sort -u)
    for crate in $crates; do
        if [[ ! -f "crates/$crate/AGENTS.md" ]]; then
            echo "error: crates/$crate uses unsafe but has no AGENTS.md (root AGENTS.md § Working rules)"
            fail=1
        fi
    done
    if [[ "$fail" -ne 0 ]]; then exit 1; fi
    echo "agents-md ok"

# Generate the checked-in agent-readable docs index and full corpus.
llms:
    mkdir -p target/llms-docs
    rustc --edition=2024 -D warnings tools/llms_docs.rs -o target/llms-docs/llms-docs
    target/llms-docs/llms-docs generate .

# CI gate: generated docs must match their authoritative Markdown sources.
llms-check:
    mkdir -p target/llms-docs
    rustc --edition=2024 -D warnings tools/llms_docs.rs -o target/llms-docs/llms-docs
    target/llms-docs/llms-docs check .

# Contract tests for ordering, determinism, stale detection, and exclusions.
llms-test:
    mkdir -p target/llms-docs
    rustc --edition=2024 -D warnings --test tools/llms_docs.rs -o target/llms-docs/llms-docs-test
    target/llms-docs/llms-docs-test

# Format the workspace in place.
fmt:
    cargo fmt --all

# CI gate: formatting.
fmt-check:
    cargo fmt --all --check

# CI gate: lints (warnings are errors, matching CI).
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# CI gate: tests (unit + integration + doctests). Matches CI nextest profile.
test:
    cargo nextest run --workspace --profile ci

# CI gate: rustdoc builds cleanly.
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# Supply-chain checks (requires `cargo install cargo-deny --locked`).
deny:
    cargo deny check

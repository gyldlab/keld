# Keld task runner — mirrors the CI gates in .github/workflows/ci.yml.
# `just ci` before pushing == what CI will run (minus the 3-OS matrix).

# Open hello window (macOS only, Phase 1 slice).
hello:
    cargo run -p keld-host -- --hello

# Run every CI gate locally (deny requires `cargo install cargo-deny --locked`).
ci: agents-md fmt-check clippy test doc deny

# Fail if a crate that opts into unsafe has no crate AGENTS.md (root AGENTS.md § Working rules).
agents-md:
    #!/usr/bin/env bash
    set -euo pipefail
    fail=0
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

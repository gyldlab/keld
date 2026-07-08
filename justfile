# Keld task runner — mirrors the CI gates in .github/workflows/ci.yml.
# `just ci` before pushing == what CI will run (minus the 3-OS matrix).

# Open hello window (macOS only, Phase 1 slice).
hello:
    cargo run -p keld-host -- --hello

# Run every CI gate locally (deny requires `cargo install cargo-deny --locked`).
ci: fmt-check clippy test doc deny

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

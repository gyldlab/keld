# Keld task runner — mirrors the CI gates in .github/workflows/ci.yml.
# `just ci` before pushing == what CI will run (minus the 3-OS matrix).

# Open hello window (macOS only, Phase 1 slice).
hello:
    cargo run -p keld-host -- --hello

# Run every CI gate locally (deny requires `cargo install cargo-deny --locked`).
# gitleaks stays GitHub-only (pinned OSS CLI in .github/workflows/ci.yml).
ci: agents-md llms-test llms-check hygiene fmt-check clippy test doc deny

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

# KEL-39: CODEOWNERS, templates, Action SHA pin, .github not gitignored.
hygiene:
    mkdir -p target/ci-hygiene
    rustc --edition=2024 -D warnings --test tools/ci_hygiene.rs -o target/ci-hygiene/ci-hygiene-test
    target/ci-hygiene/ci-hygiene-test
    rustc --edition=2024 -D warnings tools/ci_hygiene.rs -o target/ci-hygiene/ci-hygiene
    target/ci-hygiene/ci-hygiene check .

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

# ── Maintainer local-only sync (never CI; trees stay gitignored) ─────────────

# Clone or ff-only pull private research into gitignored docs/research/.
# HTTPS first, SSH fallback. No access → warn on stderr and exit 0 (hooks-safe).
research-sync:
    #!/usr/bin/env bash
    set -uo pipefail
    ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
        echo "warning: research-sync: not inside a git work tree; skip" >&2
        exit 0
    }
    DEST="$ROOT/docs/research"
    HTTPS="https://github.com/0monish/keld-research.git"
    SSH="git@github.com:0monish/keld-research.git"

    warn_skip() {
        echo "warning: research-sync: $1" >&2
        exit 0
    }

    if [[ -d "$DEST/.git" ]]; then
        nested="$(git -C "$DEST" rev-parse --show-toplevel 2>/dev/null)" || warn_skip "cannot read nested git root"
        parent="$(cd "$ROOT" && pwd -P)"
        nested_p="$(cd "$nested" && pwd -P)"
        if [[ "$nested_p" == "$parent" ]]; then
            warn_skip "docs/research/ is not a separate git checkout; refusing to pull into the Keld monorepo"
        fi
        if git -C "$DEST" pull --ff-only; then
            echo "research-sync: updated $DEST"
            exit 0
        fi
        warn_skip "git pull --ff-only failed (no access, auth, or non-ff). Left docs/research/ unchanged."
    fi

    if [[ -e "$DEST" ]]; then
        warn_skip "docs/research/ exists but is not a git checkout; not overwriting. Init/convert to a clone of keld-research, or remove and re-run."
    fi

    mkdir -p "$(dirname "$DEST")"
    err="$(mktemp)"
    if git clone "$HTTPS" "$DEST" 2>"$err"; then
        rm -f "$err"
        echo "research-sync: cloned $HTTPS → $DEST"
        exit 0
    fi
    if git clone "$SSH" "$DEST" 2>>"$err"; then
        rm -f "$err"
        echo "research-sync: cloned $SSH → $DEST"
        exit 0
    fi
    detail="$(tr '\n' ' ' <"$err" | head -c 240)"
    rm -f "$err"
    warn_skip "cannot clone keld-research via HTTPS or SSH (${detail:-auth/network}). Private repo — grant access or skip."

# Commit + push inside the nested docs/research/ checkout only (never the Keld parent).
# Optional: just research-push "your message"
research-push message="chore: sync research notes":
    #!/usr/bin/env bash
    set -uo pipefail
    ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
        echo "warning: research-push: not inside a git work tree; skip" >&2
        exit 0
    }
    DEST="$ROOT/docs/research"
    MSG={{quote(message)}}

    if [[ ! -d "$DEST/.git" ]]; then
        echo "warning: research-push: docs/research/ is not a nested git checkout; nothing pushed" >&2
        exit 0
    fi

    nested="$(git -C "$DEST" rev-parse --show-toplevel 2>/dev/null)" || {
        echo "warning: research-push: cannot read nested git root; nothing pushed" >&2
        exit 0
    }
    parent="$(cd "$ROOT" && pwd -P)"
    nested_p="$(cd "$nested" && pwd -P)"
    if [[ "$nested_p" == "$parent" ]]; then
        echo "error: research-push: refusing — docs/research resolves to the Keld monorepo. Nested private checkout required." >&2
        exit 1
    fi

    if [[ -n "$(git -C "$DEST" status --porcelain 2>/dev/null)" ]]; then
        # Stage/commit only inside the nested repo (cwd = DEST).
        git -C "$DEST" add -A
        if ! git -C "$DEST" commit -m "$MSG"; then
            echo "error: research-push: commit failed inside docs/research/ (Keld parent untouched)." >&2
            exit 1
        fi
        echo "research-push: committed inside $DEST"
    else
        echo "research-push: no local changes in $DEST"
    fi

    if git -C "$DEST" push; then
        echo "research-push: pushed $DEST"
        exit 0
    fi
    echo "error: research-push: git push failed (auth or network). Nested repo only; Keld parent untouched." >&2
    exit 1

# Shallow clone/update framework reference trees from competitors.lock.toml → competitors/.
# Pass --dry-run to parse + print planned paths without cloning.
competitors-sync *args:
    #!/usr/bin/env bash
    set -euo pipefail
    ROOT="$(git rev-parse --show-toplevel)"
    mkdir -p "$ROOT/target/competitors-sync"
    rustc --edition=2024 -D warnings "$ROOT/tools/competitors_sync.rs" \
        -o "$ROOT/target/competitors-sync/competitors-sync"
    "$ROOT/target/competitors-sync/competitors-sync" {{args}} "$ROOT"

# Point this clone at tracked .githooks/ (local config only — not --global).
hooks-install:
    #!/usr/bin/env bash
    set -euo pipefail
    ROOT="$(git rev-parse --show-toplevel)"
    git -C "$ROOT" config core.hooksPath .githooks
    chmod +x "$ROOT/.githooks/post-merge" "$ROOT/.githooks/post-checkout"
    echo "hooks-install: core.hooksPath=.githooks (local). pull/checkout will run research-sync then competitors-sync."

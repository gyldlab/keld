# Keld task runner — exact local verification of applicable gates.
# Mermaid follows the shared changed-input router; `just ci-full` forces its whole corpus.

# `set minimum-version` exists from just 1.55.0. That release also postdates
# `[parallel]` (1.42.0) and the 1.47.1 fix for parallel recipes that did not
# wait on a shared running dependency (casey/just#3138).
set minimum-version := "1.55.0"

# Shebang recipes can use "$@" for *args without collapsing spaces.
set positional-arguments

# Windows installs expose python; Unix hosts need not provide that alias.
python_command := if os() == "windows" { "python" } else { "python3" }

# Open the current platform hello backend (Phase 1 slice).
hello:
    cargo run -p keld-host --bin keld-host -- --hello

# Run every applicable CI gate locally (deny requires `cargo install cargo-deny --locked`).
# Independent policy gates run together. TypeScript then Rust run one at a time:
# measured compiler and host-test deadlines cannot share the policy batch's load.
# gitleaks stays GitHub-only (pinned OSS CLI in .github/workflows/ci.yml).
ci:
    {{python_command}} -B tools/ci_local.py ci-inventory

# Executable inventory; direct invocation runs every declared gate.
[private]
ci-inventory: ci-policy typescript fmt-check clippy test doc deny

# Full assurance mode explicitly validates every tracked Mermaid block.
ci-full:
    {{python_command}} -B tools/ci_local.py ci-full-inventory

[private]
ci-full-inventory: ci-policy-full typescript fmt-check clippy test doc deny

# Reuse Just's configured platform shell when the Python executor asks for a plan.
[private]
ci-route:
    tools/ci_changes.sh local

[parallel]
ci-policy: agents-md atomic-protocol agent-context-test agent-context ci-router-test hooks-test audit-docs-test audit-docs doc-placeholders-test doc-placeholders-check mermaid-ci product-status-test product-status-check llms-test llms-check hygiene

[parallel]
ci-policy-full: agents-md atomic-protocol agent-context-test agent-context ci-router-test hooks-test audit-docs-test audit-docs doc-placeholders-test doc-placeholders-check mermaid-full product-status-test product-status-check llms-test llms-check hygiene

mermaid-ci:
    #!/usr/bin/env bash
    set -euo pipefail
    selected=$(tools/ci_changes.sh local | sed -n 's/^mermaid=//p')
    case "$selected" in
        true) just mermaid-full ;;
        false) echo "Mermaid route: skipped; no diagram or renderer input changed." ;;
        *) echo "Mermaid route: invalid applicability '$selected'; refusing skipped-green result." >&2; exit 1 ;;
    esac

mermaid-full: mermaid-test mermaid-check mermaid-render-check

# Verify the package compiler and runtime contracts from one frozen dependency graph.
typescript:
    cd packages/@keld/kipc && bun install --frozen-lockfile
    cd packages/@keld/kipc && bun run typecheck
    cd packages/@keld/kipc && bun test
    cd packages/@keld/electron && bun install --frozen-lockfile
    cd packages/@keld/electron && bun run typecheck
    cd packages/@keld/electron && bun test

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
            echo "error: crates/$crate uses unsafe but has no AGENTS.md (root AGENTS.md § Working invariants)"
            fail=1
        fi
    done
    if [[ "$fail" -ne 0 ]]; then exit 1; fi
    echo "agents-md ok"

# Live Git publication history/ancestry is not a tracked-file diff input.
audit-docs: audit-docs-test
    {{python_command}} -B docs/audits/verify.py

audit-docs-test:
    {{python_command}} -B docs/audits/test_verify.py

# KEL-145: one canonical atomic problem-solving protocol plus narrow references.
atomic-protocol:
    mkdir -p target/atomic-protocol
    rustc --edition=2024 -D warnings --test tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol-test
    target/atomic-protocol/atomic-protocol-test
    rustc --edition=2024 -D warnings tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol
    target/atomic-protocol/atomic-protocol check .

# KEL-147: instruction inventory, routing, and Codex automatic-chain budgets.
# Live ignored workspace/task records are checked even with an empty Git diff.
agent-context: agent-context-test
    mkdir -p target/agent-context
    rustc --edition=2024 -D warnings tools/agent_context.rs -o target/agent-context/agent-context
    target/agent-context/agent-context check .
    {{python_command}} -B tools/workspace.py check

agent-context-test:
    mkdir -p target/agent-context
    rustc --edition=2024 -D warnings --test tools/agent_context.rs -o target/agent-context/agent-context-test
    target/agent-context/agent-context-test
    {{python_command}} -B tools/test_session_closeout.py
    {{python_command}} -B tools/test_session_closeout_hook.py
    {{python_command}} -B tools/test_workspace.py

# KEL-245: local-only allocation and execution; all roots derive from Git identity.
work-start *args:
    #!/usr/bin/env bash
    set -euo pipefail
    {{python_command}} -B tools/workspace.py start "$@"

work-status *args:
    #!/usr/bin/env bash
    set -euo pipefail
    {{python_command}} -B tools/workspace.py status "$@"

work-run *args:
    #!/usr/bin/env bash
    set -euo pipefail
    {{python_command}} -B tools/workspace.py run "$@"

work-finish *args:
    #!/usr/bin/env bash
    set -euo pipefail
    {{python_command}} -B tools/workspace.py finish "$@"

work-clean *args:
    #!/usr/bin/env bash
    set -euo pipefail
    {{python_command}} -B tools/workspace.py clean "$@"

work-check:
    {{python_command}} -B tools/workspace.py check

work-test:
    {{python_command}} -B tools/test_workspace.py

# Generate the canonical Current/Target/Evidence status view.
product-status:
    mkdir -p target/product-status
    rustc --edition=2024 -D warnings tools/product_status.rs -o target/product-status/product-status
    target/product-status/product-status generate .

# Contract tests for schema closure, mutation detection, and deterministic rendering.
product-status-test:
    mkdir -p target/product-status
    rustc --edition=2024 -D warnings --test tools/product_status.rs -o target/product-status/product-status-test
    target/product-status/product-status-test

# Live evidence-object availability and ancestry can change without a file diff.
product-status-check:
    mkdir -p target/product-status
    rustc --edition=2024 -D warnings tools/product_status.rs -o target/product-status/product-status
    target/product-status/product-status check .

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

# CI gate: no unsubstituted template placeholder may reach checked-in prose (KEL-213).
doc-placeholders-check:
    mkdir -p target/doc-placeholders
    rustc --edition=2024 -D warnings tools/doc_placeholders.rs -o target/doc-placeholders/doc-placeholders
    target/doc-placeholders/doc-placeholders check .

# Contract tests for the placeholder inventory, fences, adjacency, and stale entries.
doc-placeholders-test:
    mkdir -p target/doc-placeholders
    rustc --edition=2024 -D warnings --test tools/doc_placeholders.rs -o target/doc-placeholders/doc-placeholders-test
    target/doc-placeholders/doc-placeholders-test

# Validate Mermaid fences, accessibility metadata, stable types, and semantic palette.
mermaid-check:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target/mermaid-docs
    rustc --edition=2024 -D warnings tools/mermaid_docs.rs -o target/mermaid-docs/mermaid-docs
    target/mermaid-docs/mermaid-docs check .
    if git -C docs/research rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        target/mermaid-docs/mermaid-docs check docs/research
    fi

# Contract tests for the Mermaid documentation validator.
mermaid-test:
    mkdir -p target/mermaid-docs
    rustc --edition=2024 -D warnings --test tools/mermaid_docs.rs -o target/mermaid-docs/mermaid-docs-test
    target/mermaid-docs/mermaid-docs-test

# Render every tracked Mermaid block in the isolated, digest-pinned official container.
mermaid-render-check:
    tools/mermaid_render_check.sh

# KEL-39: CODEOWNERS, templates, Action SHA pin, .github not gitignored.
hygiene:
    bun --no-install test tools/ci_workflow_security.test.ts
    {{python_command}} -B tools/test_hello_command.py
    mkdir -p target/ci-hygiene
    rustc --edition=2024 -D warnings --test tools/ci_hygiene.rs -o target/ci-hygiene/ci-hygiene-test
    target/ci-hygiene/ci-hygiene-test
    rustc --edition=2024 -D warnings tools/ci_hygiene.rs -o target/ci-hygiene/ci-hygiene
    target/ci-hygiene/ci-hygiene check .

# KEL-333: after reviewing reader changes, renew only the digests in tools/ci-inputs.json.
ci-inputs-rebind:
    {{python_command}} -B tools/ci_inputs.py --rebind

# KEL-81: keep change-based CI routing falsifiable outside GitHub Actions too.
ci-router-test:
    tools/ci_changes_test.sh
    {{python_command}} -B tools/test_ci_local.py ExecutorTests

# KEL-159: checkout hooks must never execute code from the incoming revision.
hooks-test:
    tools/hooks_test.sh

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
# The Linux host suite owns real GTK windows, so preserve its virtual-display
# contract for the full root gate as well as the routed Ubuntu package lane.
test:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "$(uname -s)" == "Linux" ]]; then
        xvfb-run -a cargo nextest run --workspace --profile ci
    else
        cargo nextest run --workspace --profile ci
    fi

# CI gate: rustdoc builds cleanly.
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# Live advisory updates can change the result without a file diff.
# Supply-chain checks (requires `cargo install cargo-deny --locked`).
deny:
    cargo deny check
    cargo deny --manifest-path crates/keld-updater-helper/Cargo.toml --all-features --config crates/keld-updater-helper/deny.toml check bans

# ── Maintainer local-only sync (never CI; trees stay gitignored) ─────────────

# Clone or ff-only pull private research into gitignored docs/research/.
# HTTPS first, SSH fallback. No access → warn on stderr and exit 0 (hooks-safe).
research-sync:
    {{python_command}} -B tools/workspace.py reference-run -- just _research-sync

[private]
_research-sync:
    #!/usr/bin/env bash
    set -uo pipefail
    ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
        echo "warning: research-sync: not inside a git work tree; skip" >&2
        exit 0
    }
    ROOT="$({{python_command}} -B "$ROOT/tools/workspace.py" reference-root)" || exit 1
    DEST="$ROOT/docs/research"
    HTTPS="https://github.com/0monish/keld-research.git"
    SSH="git@github.com:0monish/keld-research.git"

    warn_skip() {
        echo "warning: research-sync: $1" >&2
        exit 0
    }

    research_origin_ok() {
        local origin normalized
        origin="$(git -C "$DEST" remote get-url origin 2>/dev/null)" || return 1
        normalized="${origin%.git}"
        case "$normalized" in
            https://github.com/0monish/keld-research|git@github.com:0monish/keld-research|ssh://git@github.com/0monish/keld-research)
                return 0
                ;;
            *)
                return 1
                ;;
        esac
    }

    if [[ -d "$DEST/.git" ]]; then
        nested="$(git -C "$DEST" rev-parse --show-toplevel 2>/dev/null)" || warn_skip "cannot read nested git root"
        parent="$(cd "$ROOT" && pwd -P)"
        nested_p="$(cd "$nested" && pwd -P)"
        if [[ "$nested_p" == "$parent" ]]; then
            warn_skip "docs/research/ is not a separate git checkout; refusing to pull into the Keld monorepo"
        fi
        if ! research_origin_ok; then
            warn_skip "docs/research origin is not 0monish/keld-research (HTTPS or SSH). Fix remote or re-clone; left unchanged."
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
    set -euo pipefail
    {{python_command}} -B tools/workspace.py reference-run -- just _research-push "$1"

[private]
_research-push message:
    #!/usr/bin/env bash
    set -uo pipefail
    ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
        echo "warning: research-push: not inside a git work tree; skip" >&2
        exit 0
    }
    ROOT="$({{python_command}} -B "$ROOT/tools/workspace.py" reference-root)" || exit 1
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
    origin="$(git -C "$DEST" remote get-url origin 2>/dev/null)" || {
        echo "error: research-push: docs/research has no origin remote. Point origin at 0monish/keld-research." >&2
        exit 1
    }
    normalized="${origin%.git}"
    case "$normalized" in
        https://github.com/0monish/keld-research|git@github.com:0monish/keld-research|ssh://git@github.com/0monish/keld-research)
            ;;
        *)
            echo "error: research-push: refusing — origin is \`$origin\`, not 0monish/keld-research. Fix remote; Keld parent untouched." >&2
            exit 1
            ;;
    esac

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
# Pass --dry-run / --force as separate args (positional-arguments preserves spaces).
competitors-sync *args:
    #!/usr/bin/env bash
    set -euo pipefail
    {{python_command}} -B tools/workspace.py reference-run -- just _competitors-sync "$@"

[private]
_competitors-sync *args:
    #!/usr/bin/env bash
    set -euo pipefail
    ROOT="$(git rev-parse --show-toplevel)"
    ROOT="$({{python_command}} -B "$ROOT/tools/workspace.py" reference-root)"
    mkdir -p "$ROOT/target/competitors-sync"
    rustc --edition=2024 -D warnings "$ROOT/tools/competitors_sync.rs" \
        -o "$ROOT/target/competitors-sync/competitors-sync"
    "$ROOT/target/competitors-sync/competitors-sync" "$@" "$ROOT"

# Install reviewed hook copies outside the working tree (local config only — not --global).
hooks-install:
    #!/usr/bin/env bash
    set -euo pipefail
    ROOT="$(git rev-parse --show-toplevel)"
    COMMON_DIR="$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir)"
    HOOKS_DIR="$COMMON_DIR/keld-hooks"
    mkdir -p "$HOOKS_DIR"
    cp -- "$ROOT/.githooks/post-merge" "$HOOKS_DIR/post-merge"
    cp -- "$ROOT/.githooks/post-checkout" "$HOOKS_DIR/post-checkout"
    chmod +x "$HOOKS_DIR/post-merge" "$HOOKS_DIR/post-checkout"
    git -C "$ROOT" config core.hooksPath "$HOOKS_DIR"
    echo "hooks-install: installed reviewed reminder hooks at $HOOKS_DIR (local)."
    echo "hooks-install: checkout/merge will not execute working-tree code."

# Validate the actual session receipt; this is local/remote-evidence admission, not CI.
session-closeout receipt:
    #!/usr/bin/env bash
    set -euo pipefail
    {{python_command}} -B tools/session_closeout.py check "$1"

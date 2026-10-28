#!/usr/bin/env bash
# Maintainer-only research checkout commands. The caller owns the reference-run
# lock and passes the already verified primary reference root; this file never
# resolves shared workspace identity, unlocks it, or touches the Keld parent Git.
set -uo pipefail

MODE="${1-}"
ROOT="${2-}"
MSG="${3-}"
if [[ "$MODE" != sync && "$MODE" != push || -z "$ROOT" ]]; then
    echo "error: research-checkout: expected sync|push and the verified primary root" >&2
    exit 2
fi
DEST="$ROOT/docs/research"
HTTPS="https://github.com/0monish/keld-research.git"
SSH="git@github.com:0monish/keld-research.git"

# One exact allowlist owner for both workflows; do not accept rewritten hosts,
# case-folded lookalikes, or additional URL spellings.
research_origin_ok() {
    local normalized="${1%.git}"
    case "$normalized" in
        https://github.com/0monish/keld-research|git@github.com:0monish/keld-research|ssh://git@github.com/0monish/keld-research)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

sync_research() {
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
        origin="$(git -C "$DEST" remote get-url origin 2>/dev/null)" || warn_skip "docs/research origin is not 0monish/keld-research (HTTPS or SSH). Fix remote or re-clone; left unchanged."
        if ! research_origin_ok "$origin"; then
            warn_skip "docs/research origin is not 0monish/keld-research (HTTPS or SSH). Fix remote or re-clone; left unchanged."
        fi
        branch="$(git -C "$DEST" symbolic-ref --quiet --short HEAD 2>/dev/null)" || warn_skip "docs/research HEAD is detached; left unchanged."
        if git -C "$DEST" pull --ff-only origin "$branch"; then
            echo "research-sync: updated $DEST"
            exit 0
        fi
        warn_skip "git pull --ff-only failed (no access, auth, or non-ff). Left docs/research/ unchanged."
    fi

    if [[ -e "$DEST" ]]; then
        warn_skip "docs/research/ exists but is not a git checkout; not overwriting. Init/convert to a clone of keld-research, or remove and re-run."
    fi

    mkdir -p "$(dirname "$DEST")"
    if git clone "$HTTPS" "$DEST" 2>/dev/null; then
        echo "research-sync: cloned $HTTPS ? $DEST"
        exit 0
    fi
    if git clone "$SSH" "$DEST" 2>/dev/null; then
        echo "research-sync: cloned $SSH ? $DEST"
        exit 0
    fi
    warn_skip "cannot clone keld-research via HTTPS or SSH (auth/network). Private repo ? grant access or skip."

}

push_research() {
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
    if ! research_origin_ok "$origin"; then
        echo "error: research-push: refusing ? origin is not 0monish/keld-research (HTTPS or SSH). Fix remote; Keld parent untouched." >&2
        exit 1
    fi

    # Git can read a different push URL than the fetch URL. Refuse both an
    # unauthorized destination and ambiguous multi-destination push URLs.
    push_origin="$(git -C "$DEST" remote get-url --push --all origin 2>/dev/null)" || {
        echo "error: research-push: docs/research has no origin push URL. Fix remote; Keld parent untouched." >&2
        exit 1
    }
    if [[ -z "$push_origin" || "$push_origin" == *$'\n'* ]] || ! research_origin_ok "$push_origin"; then
        echo "error: research-push: refusing ? origin push URL is not one exact 0monish/keld-research destination. Fix remote; Keld parent untouched." >&2
        exit 1
    fi
    branch="$(git -C "$DEST" symbolic-ref --quiet --short HEAD 2>/dev/null)" || {
        echo "error: research-push: docs/research HEAD is detached. Checkout a branch before pushing; Keld parent untouched." >&2
        exit 1
    }
    pending="$(git -C "$DEST" status --porcelain 2>/dev/null)" || {
        echo "error: research-push: git status failed inside docs/research. Fix checkout access; Keld parent untouched." >&2
        exit 1
    }
    if [[ -n "$pending" ]]; then
        # Stage/commit only inside the nested repo (cwd = DEST).
        if ! git -C "$DEST" add -A; then
            echo "error: research-push: git add failed inside docs/research. Fix checkout access; Keld parent untouched." >&2
            exit 1
        fi
        if ! git -C "$DEST" commit -m "$MSG"; then
            echo "error: research-push: commit failed inside docs/research/ (Keld parent untouched)." >&2
            exit 1
        fi
        echo "research-push: committed inside $DEST"
    else
        echo "research-push: no local changes in $DEST"
    fi

    # Pin both the remote and the destination ref: ignore pushDefault,
    # branch.pushRemote, upstream, and caller-selected push refspec defaults.
    if git -C "$DEST" push origin "HEAD:refs/heads/$branch"; then
        echo "research-push: pushed $DEST"
        exit 0
    fi
    echo "error: research-push: git push failed (auth or network). Nested repo only; Keld parent untouched." >&2
    exit 1
}

case "$MODE" in
    sync) sync_research ;;
    push) push_research ;;
esac

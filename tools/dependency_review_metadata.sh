#!/usr/bin/env bash
# GitHub dependency-review-action warns, but does not fail, on partial snapshots.
# This boundary refuses that warning on every page before invoking the action.
set -euo pipefail

fail() {
    echo "DEPENDENCY-METADATA: $1" >&2
    return 1
}

check_headers() {
    local line warning pages=0 in_headers=false body_seen=false
    while IFS= read -r line || [[ -n "$line" ]]; do
        line="${line%$'\r'}"
        if [[ "$line" == HTTP/* ]]; then
            if [[ "$in_headers" == true ]] || { [[ "$pages" -gt 0 ]] && [[ "$body_seen" != true ]]; }; then
                fail "truncated API response; restore complete dependency metadata before rerunning this head."
                return 1
            fi
            if [[ ! "$line" =~ ^HTTP/[0-9]+(\.[0-9]+)?[[:space:]]200([[:space:]]|$) ]]; then
                fail "non-200 dependency API response; verify dependency graph access before rerunning this head."
                return 1
            fi
            pages=$((pages + 1))
            in_headers=true
            body_seen=false
        elif [[ "$in_headers" == true ]]; then
            if [[ -z "$line" ]]; then
                in_headers=false
            elif [[ "$line" =~ ^[Xx]-[Gg][Ii][Tt][Hh][Uu][Bb]-[Dd][Ee][Pp][Ee][Nn][Dd][Ee][Nn][Cc][Yy]-[Gg][Rr][Aa][Pp][Hh]-[Ss][Nn][Aa][Pp][Ss][Hh][Oo][Tt]-[Ww][Aa][Rr][Nn][Ii][Nn][Gg][Ss]: ]]; then
                warning="${line#*:}"
                if [[ -n "${warning//[[:space:]]/}" ]]; then
                    fail "GitHub reports incomplete dependency snapshots; restore the missing base/head metadata before rerunning. No partial result is admitted."
                    return 1
                fi
            fi
        elif [[ -n "$line" ]]; then
            if [[ "$pages" -eq 0 ]]; then
                fail "missing API response headers; use gh api --include --paginate."
                return 1
            fi
            body_seen=true
        fi
    done
    if [[ "$pages" -eq 0 || "$in_headers" == true || "$body_seen" != true ]]; then
        fail "empty or truncated dependency API response; no metadata was admitted."
        return 1
    fi
    echo "dependency metadata headers ok ($pages pages; supported GitHub manifests only)"
}

check_refs() {
    local ref
    for ref in "$@"; do
        if [[ ! "$ref" =~ ^[0-9a-f]{40}$ || "$ref" == 0000000000000000000000000000000000000000 ]]; then
            fail "base/head must be nonzero immutable commit SHAs; restore the event SHA handoff."
            return 1
        fi
    done
}

self_test() {
    local complete=$'HTTP/2.0 200 OK\r\nContent-Type: application/json\r\n\r\n[]\n'
    local bad
    printf '%s' "$complete" | check_headers >/dev/null
    printf '%s%s' "$complete" "$complete" | check_headers >/dev/null
    # GitHub sends an empty warning header for complete comparisons.
    printf '%s' $'HTTP/2.0 200 OK\nX-Github-Dependency-Graph-Snapshot-Warnings: \n\n[]\n' | check_headers >/dev/null
    for bad in \
        '' \
        '[]' \
        $'HTTP/2.0 403 Forbidden\n\n{}\n' \
        $'HTTP/2.0 200 OK\nContent-Type: application/json\n' \
        $'HTTP/2.0 200 OK\n\n' \
        $'HTTP/2.0 200 OK\nx-github-dependency-graph-snapshot-warnings: e30=\n\n[]\n' \
        $'HTTP/2.0 200 OK\nX-Github-Dependency-Graph-Snapshot-Warnings: e30=\n\n[]\n'; do
        if printf '%s' "$bad" | check_headers >/dev/null 2>&1; then
            fail "self-test admitted missing, failed or incomplete metadata."
            return 1
        fi
    done
    if printf '%s%s' "$complete" $'HTTP/2.0 200 OK\nx-github-dependency-graph-snapshot-warnings: e30=\n\n[]\n' | check_headers >/dev/null 2>&1; then
        fail "self-test ignored an incomplete later page."
        return 1
    fi
    check_refs aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
    for bad in '' main 0000000000000000000000000000000000000000 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa 'abc?per_page=1'; do
        if check_refs "$bad" >/dev/null 2>&1; then
            fail "self-test admitted a mutable, missing or malformed ref."
            return 1
        fi
    done
    echo "dependency metadata contract tests ok"
}

case "${1:-}" in
    test)
        [[ "$#" -eq 1 ]] || { fail "test takes no arguments."; exit 1; }
        self_test
        ;;
    check)
        [[ "$#" -eq 4 ]] || { fail "use check OWNER/REPO BASE_SHA HEAD_SHA."; exit 1; }
        [[ "$2" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || { fail "invalid repository locator."; exit 1; }
        check_refs "$3" "$4"
        gh api --include --paginate \
            -H 'Accept: application/vnd.github+json' \
            -H 'X-GitHub-Api-Version: 2022-11-28' \
            "repos/$2/dependency-graph/compare/$3...$4?per_page=100" | check_headers
        ;;
    *)
        fail "use test, or check OWNER/REPO BASE_SHA HEAD_SHA."
        exit 1
        ;;
esac

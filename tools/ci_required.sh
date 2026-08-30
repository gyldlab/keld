#!/usr/bin/env bash
# One observable merge-admission decision over the jobs in ci.yml.
set -euo pipefail

fail() {
    echo "CI-REQUIRED: $1" >&2
    return 1
}

require_success() {
    local label="$1"
    local result="$2"
    if [[ "$result" != "success" ]]; then
        fail "$label must succeed, got '$result'. Re-run the exact head after the CI service is healthy; do not treat missing, skipped, cancelled, or failed evidence as a pass."
    fi
}

require_success_or_skip() {
    local label="$1"
    local result="$2"
    case "$result" in
        success | skipped) ;;
        *)
            fail "$label must succeed when selected or report skipped when the router proves it inapplicable, got '$result'. Fix or re-run the exact failing head."
            ;;
    esac
}

check_results() {
    if [[ "$#" -ne 9 ]]; then
        fail "expected 9 job results (changes, fmt, check, bun-test, linux-gui-smoke, msrv, deny, secrets, hygiene), got $#; restore the required job's complete needs handoff."
        return
    fi

    require_success "change router" "$1" || return 1
    require_success_or_skip "rustfmt" "$2" || return 1
    require_success_or_skip "cross-platform clippy + test" "$3" || return 1
    require_success_or_skip "Bun TypeScript tests" "$4" || return 1
    require_success_or_skip "Linux GUI smoke" "$5" || return 1
    require_success_or_skip "MSRV" "$6" || return 1
    require_success_or_skip "cargo-deny" "$7" || return 1
    require_success "gitleaks" "$8" || return 1
    require_success_or_skip "documentation and repository hygiene" "$9" || return 1
}

expect_pass() {
    local label="$1"
    shift
    if ! check_results "$@" >/dev/null 2>&1; then
        fail "self-test '$label' unexpectedly failed"
    fi
}

expect_fail() {
    local label="$1"
    shift
    if check_results "$@" >/dev/null 2>&1; then
        fail "self-test '$label' unexpectedly passed"
    fi
}

self_test() {
    expect_pass "all applicable jobs succeed" \
        success success success success success success success success success
    expect_pass "router-proven inapplicable jobs skip" \
        success skipped skipped skipped skipped skipped skipped success skipped

    expect_fail "missing gitleaks is not green" \
        success skipped skipped skipped skipped skipped skipped skipped skipped
    expect_fail "cancelled gitleaks is not green" \
        success skipped skipped skipped skipped skipped skipped cancelled skipped
    expect_fail "failed selected test is not green" \
        success success failure skipped skipped success skipped success success
    expect_fail "cancelled router cannot skip everything green" \
        cancelled skipped skipped skipped skipped skipped skipped success skipped
    expect_fail "missing result handoff is rejected" \
        success skipped skipped skipped skipped skipped skipped success

    echo "ci-required contract tests ok"
}

case "${1:-}" in
    check)
        shift
        check_results "$@"
        echo "ci-required ok"
        ;;
    test)
        if [[ "$#" -ne 1 ]]; then
            fail "test takes no additional arguments. Run 'tools/ci_required.sh test'."
            exit 1
        fi
        self_test
        ;;
    *)
        fail "unknown or missing command '${1:-}'. Use 'check' with 9 job results, or 'test'."
        exit 1
        ;;
esac

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

require_routed_result() {
    local label="$1"
    local result="$2"
    local selected="$3"
    case "$selected" in
        true)
            require_success "$label" "$result"
            ;;
        false)
            if [[ "$result" != "skipped" ]]; then
                fail "$label must report skipped when the router marks it inapplicable, got '$result'. Keep the job condition and router output aligned."
            fi
            ;;
        *)
            fail "$label received invalid router applicability '$selected'. The router must emit exactly 'true' or 'false'."
            ;;
    esac
}

check_results() {
    if [[ "$#" -ne 16 ]]; then
        fail "expected 9 job results plus 7 router outputs, got $#; restore the required job's complete needs and applicability handoff."
        return
    fi

    require_success "change router" "$1" || return 1
    require_routed_result "rustfmt" "$2" "${10}" || return 1
    require_routed_result "cross-platform clippy + test" "$3" "${10}" || return 1
    require_routed_result "Bun TypeScript tests" "$4" "${11}" || return 1
    require_routed_result "Linux GUI smoke" "$5" "${12}" || return 1
    require_routed_result "MSRV" "$6" "${13}" || return 1
    require_routed_result "cargo-deny" "$7" "${14}" || return 1
    require_success "gitleaks" "$8" || return 1
    case "${15}:${16}" in
        true:true | true:false | false:true)
            require_routed_result "documentation and repository hygiene" "$9" true || return 1
            ;;
        false:false)
            require_routed_result "documentation and repository hygiene" "$9" false || return 1
            ;;
        *)
            fail "documentation and repository hygiene received invalid router applicability '${15}:${16}'. Both router outputs must be exactly 'true' or 'false'."
            return 1
            ;;
    esac
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
        success success success success success success success success success \
        true true true true true true true
    expect_pass "router-proven inapplicable jobs skip" \
        success skipped skipped skipped skipped skipped skipped success skipped \
        false false false false false false false

    expect_fail "missing gitleaks is not green" \
        success skipped skipped skipped skipped skipped skipped skipped skipped \
        false false false false false false false
    expect_fail "cancelled gitleaks is not green" \
        success skipped skipped skipped skipped skipped skipped cancelled skipped \
        false false false false false false false
    expect_fail "failed selected test is not green" \
        success success failure skipped skipped success skipped success success \
        true false false true false true false
    expect_fail "selected job cannot disappear as skipped" \
        success skipped skipped skipped skipped skipped skipped success skipped \
        true false false false false false false
    expect_fail "unselected job cannot silently run" \
        success success skipped skipped skipped skipped skipped success skipped \
        false false false false false false false
    expect_fail "invalid router output is not evidence" \
        success skipped skipped skipped skipped skipped skipped success skipped \
        missing false false false false false false
    expect_fail "cancelled router cannot skip everything green" \
        cancelled skipped skipped skipped skipped skipped skipped success skipped \
        false false false false false false false
    expect_fail "missing result handoff is rejected" \
        success skipped skipped skipped skipped skipped skipped success skipped \
        false false false false false false

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
        fail "unknown or missing command '${1:-}'. Use 'check' with 9 job results and 7 router outputs, or 'test'."
        exit 1
        ;;
esac

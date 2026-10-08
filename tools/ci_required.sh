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

# Argument layout (27): 1 change router; 2-10 core job results; 11-18 lane
# router outputs; 19-21 CodeQL rust, javascript-typescript and actions job
# results; 22-24 their router outputs; 25 dependency review; 26 workspace
# contracts job result; 27 its router output.
check_results() {
    if [[ "$#" -ne 27 ]]; then
        fail "expected 10 routed/core job results, 8 router outputs, 3 CodeQL job results, 3 CodeQL router outputs, 1 dependency review result, and the workspace contracts result and router output, got $#; restore the required job's complete needs and applicability handoff."
        return
    fi

    require_success "change router" "$1" || return 1
    require_routed_result "rustfmt" "$2" "${11}" || return 1
    require_routed_result "cross-platform clippy + test" "$3" "${11}" || return 1
    require_routed_result "Bun TypeScript tests" "$4" "${12}" || return 1
    require_routed_result "Linux GUI smoke" "$5" "${13}" || return 1
    require_routed_result "MSRV" "$6" "${14}" || return 1
    require_routed_result "cargo-deny" "$7" "${15}" || return 1
    require_success "gitleaks" "$8" || return 1
    case "${16}:${17}" in
        true:true | true:false | false:true)
            require_routed_result "documentation and repository hygiene" "$9" true || return 1
            ;;
        false:false)
            require_routed_result "documentation and repository hygiene" "$9" false || return 1
            ;;
        *)
            fail "documentation and repository hygiene received invalid router applicability '${16}:${17}'. Both router outputs must be exactly 'true' or 'false'."
            return 1
            ;;
    esac
    require_routed_result "Mermaid diagram validation/render" "${10}" "${18}" || return 1
    # Pull requests route each language on its own inputs; push and every
    # unknown/all input select all three, which the router owns (#624).
    require_routed_result "CodeQL rust analysis and upload" "${19}" "${22}" || return 1
    require_routed_result "CodeQL javascript-typescript analysis and upload" "${20}" "${23}" || return 1
    require_routed_result "CodeQL actions analysis and upload" "${21}" "${24}" || return 1
    require_success "dependency review" "${25}" || return 1
    require_routed_result "workspace path and process contracts" "${26}" "${27}" || return 1
}

# Older self-test rows predate Mermaid (18 arguments) and per-language CodeQL
# routing (20 arguments, one always-selected CodeQL result). Widen them with
# the meaning they had then; check_results itself accepts only 25.
normalize_self_test_args() {
    local -a args=("$@")
    if [[ "${#args[@]}" -eq 18 ]]; then
        args=("${args[@]:0:9}" skipped "${args[@]:9:7}" false "${args[@]:16:2}")
    fi
    if [[ "${#args[@]}" -eq 20 ]]; then
        args=("${args[@]:0:18}" "${args[18]}" "${args[18]}" "${args[18]}" true true true "${args[19]}")
    fi
    # Rows before the workspace contracts job had no such job: it is skipped.
    if [[ "${#args[@]}" -eq 25 ]]; then
        args=("${args[@]}" skipped false)
    fi
    normalized_args=("${args[@]}")
}

expect_pass() {
    local label="$1"
    shift
    local -a normalized_args=()
    normalize_self_test_args "$@"
    if ! check_results "${normalized_args[@]}" >/dev/null 2>&1; then
        fail "self-test '$label' unexpectedly failed"
    fi
}

expect_fail() {
    local label="$1"
    shift
    local -a normalized_args=()
    normalize_self_test_args "$@"
    if check_results "${normalized_args[@]}" >/dev/null 2>&1; then
        fail "self-test '$label' unexpectedly passed"
    fi
}

# Docs-only pull request: every lane and every CodeQL language is inapplicable.
docs_only_prefix=(success skipped skipped skipped skipped skipped skipped success skipped skipped
    false false false false false false false false)

codeql_self_test() {
    expect_pass "docs-only pull request skips every CodeQL language" \
        "${docs_only_prefix[@]}" skipped skipped skipped false false false success
    expect_pass "every selected CodeQL language succeeds" \
        "${docs_only_prefix[@]}" success success success true true true success
    local index result
    local -a results routes
    for index in 0 1 2; do
        results=(skipped skipped skipped)
        routes=(false false false)
        results[index]=success
        routes[index]=true
        expect_pass "only selected CodeQL language $index runs" \
            "${docs_only_prefix[@]}" "${results[@]}" "${routes[@]}" success
        # Negative control for the row above: the same selection, skipped.
        results[index]=skipped
        expect_fail "selected CodeQL language $index cannot be skipped" \
            "${docs_only_prefix[@]}" "${results[@]}" "${routes[@]}" success
        for result in failure cancelled missing ''; do
            results[index]="$result"
            expect_fail "selected CodeQL language $index '$result' is not analysis evidence" \
                "${docs_only_prefix[@]}" "${results[@]}" "${routes[@]}" success
        done
        results=(skipped skipped skipped)
        routes=(false false false)
        for result in success failure cancelled; do
            results[index]="$result"
            expect_fail "unselected CodeQL language $index '$result' must be skipped" \
                "${docs_only_prefix[@]}" "${results[@]}" "${routes[@]}" success
        done
        results=(skipped skipped skipped)
        for result in missing '' TRUE; do
            routes=(false false false)
            routes[index]="$result"
            expect_fail "invalid CodeQL applicability '$result' for language $index is not evidence" \
                "${docs_only_prefix[@]}" "${results[@]}" "${routes[@]}" success
        done
    done
    if check_results "${docs_only_prefix[@]}" skipped skipped skipped false false false skipped false >/dev/null 2>&1; then
        fail "missing dependency review handoff was accepted"
    fi
    if check_results "${docs_only_prefix[@]}" skipped skipped skipped false false success skipped false >/dev/null 2>&1; then
        fail "missing CodeQL route handoff was accepted"
    fi
    if check_results "${docs_only_prefix[@]}" success success >/dev/null 2>&1; then
        fail "the pre-#624 single CodeQL handoff was accepted"
    fi
    echo "ok: CodeQL applicability is checked per language"
}

workspace_self_test() {
    local -a prefix=("${docs_only_prefix[@]}" skipped skipped skipped false false false success)
    expect_pass "unselected workspace contracts skip" "${prefix[@]}" skipped false
    expect_pass "selected workspace contracts succeed" "${prefix[@]}" success true
    # Negative controls for the two rows above.
    expect_fail "selected workspace contracts cannot be skipped" "${prefix[@]}" skipped true
    expect_fail "unselected workspace contracts must not run" "${prefix[@]}" success false
    local result
    for result in failure cancelled missing ''; do
        expect_fail "selected workspace contracts '$result' is not evidence" "${prefix[@]}" "$result" true
    done
    expect_fail "invalid workspace applicability is not evidence" "${prefix[@]}" skipped missing
    if check_results "${prefix[@]}" skipped >/dev/null 2>&1; then
        fail "missing workspace route handoff was accepted"
    fi
    echo "ok: workspace contracts applicability is checked"
}

self_test() {
    expect_pass "all applicable jobs succeed" \
        success success success success success success success success success \
        true true true true true true true success success
    expect_pass "router-proven inapplicable jobs skip" \
        success skipped skipped skipped skipped skipped skipped success skipped \
        false false false false false false false success success

    expect_pass "selected Mermaid validation and rendering succeeds" \
        success success success success success success success success success success \
        true true true true true true true true success success
    expect_fail "selected Mermaid rendering cannot be skipped" \
        success skipped skipped skipped skipped skipped skipped success skipped skipped \
        false false false false false false false true success success
    expect_fail "unselected Mermaid job must be skipped" \
        success skipped skipped skipped skipped skipped skipped success skipped success \
        false false false false false false false false success success
    expect_fail "invalid Mermaid applicability is not evidence" \
        success skipped skipped skipped skipped skipped skipped success skipped skipped \
        false false false false false false false missing success success

    expect_fail "missing gitleaks is not green" \
        success skipped skipped skipped skipped skipped skipped skipped skipped \
        false false false false false false false success success
    expect_fail "cancelled gitleaks is not green" \
        success skipped skipped skipped skipped skipped skipped cancelled skipped \
        false false false false false false false success success
    expect_fail "failed selected test is not green" \
        success success failure skipped skipped success skipped success success \
        true false false true false true false success success
    expect_fail "selected job cannot disappear as skipped" \
        success skipped skipped skipped skipped skipped skipped success skipped \
        true false false false false false false success success
    expect_fail "unselected job cannot silently run" \
        success success skipped skipped skipped skipped skipped success skipped \
        false false false false false false false success success
    expect_fail "invalid router output is not evidence" \
        success skipped skipped skipped skipped skipped skipped success skipped \
        missing false false false false false false success success
    expect_fail "cancelled router cannot skip everything green" \
        cancelled skipped skipped skipped skipped skipped skipped success skipped \
        false false false false false false false success success
    expect_fail "missing result handoff is rejected" \
        success skipped skipped skipped skipped skipped skipped success skipped \
        false false false false false false success success
    if check_results success skipped skipped skipped skipped skipped skipped success skipped \
        false false false false false false false false success \
        success success success true true true success skipped false >/dev/null 2>&1; then
        fail "missing Mermaid result handoff was accepted"
    fi
    echo "ok: missing Mermaid result handoff is rejected"

    local result
    for result in skipped cancelled failure missing ''; do
        expect_fail "CodeQL '$result' is not analysis evidence" \
            success skipped skipped skipped skipped skipped skipped success skipped \
            false false false false false false false "$result" success
        expect_fail "dependency review '$result' is not evidence" \
            success skipped skipped skipped skipped skipped skipped success skipped \
            false false false false false false false success "$result"
    done
    expect_fail "old handoff cannot omit both security jobs" \
        success skipped skipped skipped skipped skipped skipped success skipped \
        false false false false false false false
    codeql_self_test
    workspace_self_test

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
        fail "unknown or missing command '${1:-}'. Use 'check' with 10 core job results, 8 router outputs, 3 CodeQL job results, 3 CodeQL router outputs, 1 dependency review result, and the workspace contracts result and router output, or 'test'."
        exit 1
        ;;
esac

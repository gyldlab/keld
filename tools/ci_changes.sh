#!/usr/bin/env bash
# Classify CI lanes from changed-path ownership.
#
# The workflow always starts. This helper controls job-level conditions because
# GitHub leaves required checks pending when a whole workflow is path-filtered.
# See KEL-81 and AGENTS.md "CI dependency routing".
set -euo pipefail

readonly TRUE=true
readonly FALSE=false

rust="$FALSE"
docs="$FALSE"
mermaid="$FALSE"
markdown_changed="$FALSE"
hygiene="$FALSE"
gui="$FALSE"
msrv="$FALSE"
deny="$FALSE"
ts="$FALSE"
webkitgtk="$FALSE"
packages=""
nongtk_packages=""
ubuntu_packages=""
ts_packages=""
codeql_rust="$FALSE"
codeql_javascript_typescript="$FALSE"
codeql_actions="$FALSE"
workspace="$FALSE"
# The check job's OS matrix, as JSON for fromJSON. Every Rust selection uses all
# three OSes except one reached only through documentation reads (#624).
readonly ALL_CHECK_OS='["ubuntu-latest","macos-latest","windows-latest"]'
readonly DOCUMENTATION_CHECK_OS='["windows-latest"]'
check_os="$ALL_CHECK_OS"
rust_documentation_only="$FALSE"
# `cargo test --doc` for the selected packages that have a library target
# (#632); nextest does not run doctests.
doctest="$FALSE"
doctest_packages=""
all_workspace_packages="$FALSE"
workspace_metadata_cache=""
host_dependency_dirs_cache=""
consumer_contract=""
local_force_all="$FALSE"
declare -a changed_package_roots=()
# Packages selected only because a reviewed documentation read changed. They
# are tested themselves, without Cargo reverse-dependent expansion (#624).
declare -a documentation_reader_roots=()
declare -a changed_ts_package_dirs=()

usage() {
    echo "usage: $0 {classify|github|local|host-dirs}" >&2
    echo "  classify   read NUL-delimited changed paths from stdin" >&2
    echo "  github     derive the comparison from GitHub Actions environment" >&2
    echo "  local      derive the comparison from origin/main to the working tree" >&2
    echo "  host-dirs  print the current keld-host local dependency closure" >&2
    exit 2
}

mark_all() {
    local_force_all="$TRUE"
    rust="$TRUE"
    docs="$TRUE"
    mermaid="$TRUE"
    hygiene="$TRUE"
    gui="$TRUE"
    msrv="$TRUE"
    deny="$TRUE"
    ts="$TRUE"
    all_workspace_packages="$TRUE"
    workspace="$TRUE"
    select_every_codeql_language
}

# A push to main always analyses every language: skipping one there leaves the
# default branch without the baseline that pull-request alerts compare against.
# Unknown, all and workflow/router inputs use the same complete selection.
select_every_codeql_language() {
    codeql_rust="$TRUE"
    codeql_javascript_typescript="$TRUE"
    codeql_actions="$TRUE"
}

# An extensionless file can be a JavaScript entry point through its shebang.
# Only a readable regular file without a `#!` first line is proven not to be
# one; a deleted or unreadable path stays a possible input.
codeql_extensionless_may_be_javascript() {
    local root first=""
    root="$(git rev-parse --show-toplevel)"
    if [[ ! -f "$root/$1" || -L "$root/$1" || ! -r "$root/$1" ]]; then
        return 0
    fi
    IFS= read -r -n 2 first <"$root/$1" || true
    [[ "$first" == "#!" ]]
}

# CodeQL analyses source by language, independent of Cargo package ownership:
# a documentation read that selects a Rust test package changes no analysed
# source. The patterns follow the extractors' file types in CodeQL's supported
# languages list and the JavaScript extractor's HTML/JS types; Rust also takes
# its build inputs. Every other path selects no analysis unless it falls back
# to all lanes; push always selects every language.
classify_codeql_path() {
    case "$1" in
        *.rs | Cargo.toml | */Cargo.toml | Cargo.lock | */Cargo.lock | rust-toolchain.toml | .cargo/*)
            codeql_rust="$TRUE"
            ;;
    esac
    case "$1" in
        packages/* | *.ts | *.tsx | *.mts | *.cts | *.js | *.jsx | *.mjs | *.cjs | *.es | *.es6 | \
            *.xsjs | *.xsjslib | *.htm | *.html | *.xhtm | *.xhtml | *.vue | *.hbs | *.ejs | *.njk | \
            *.erb | *.jsp | *.dot | *.json | *.yaml | *.yml | *.raml | *.xml)
            codeql_javascript_typescript="$TRUE"
            ;;
        *)
            if [[ "${1##*/}" != *.* ]] && codeql_extensionless_may_be_javascript "$1"; then
                codeql_javascript_typescript="$TRUE"
            fi
            ;;
    esac
    case "$1" in
        .github/workflows/* | .github/actions/* | action.yml | action.yaml | */action.yml | */action.yaml)
            codeql_actions="$TRUE"
            ;;
    esac
}

# Unknown/shared inputs must not skip Linux GTK clippy. Workflow/router edits
# still enable every *job* (including GUI smoke, which installs WebKitGTK) but
# MUST NOT also install GTK on Ubuntu clippy and MSRV: those extra apt-get
# update calls contend with the smoke job and hang on Azure Ubuntu mirrors.
mark_unknown() {
    mark_all
    webkitgtk="$TRUE"
}

emit() {
    printf 'rust=%s\n' "$rust"
    printf 'docs=%s\n' "$docs"
    printf 'mermaid=%s\n' "$mermaid"
    printf 'hygiene=%s\n' "$hygiene"
    printf 'gui=%s\n' "$gui"
    printf 'msrv=%s\n' "$msrv"
    printf 'deny=%s\n' "$deny"
    printf 'ts=%s\n' "$ts"
    printf 'webkitgtk=%s\n' "$webkitgtk"
    printf 'packages=%s\n' "$packages"
    printf 'nongtk_packages=%s\n' "$nongtk_packages"
    printf 'ubuntu_packages=%s\n' "$ubuntu_packages"
    printf 'ts_packages=%s\n' "$ts_packages"
    printf 'codeql_rust=%s\n' "$codeql_rust"
    printf 'codeql_javascript_typescript=%s\n' "$codeql_javascript_typescript"
    printf 'codeql_actions=%s\n' "$codeql_actions"
    printf 'workspace=%s\n' "$workspace"
    printf 'check_os=%s\n' "$check_os"
    printf 'rust_documentation_only=%s\n' "$rust_documentation_only"
    printf 'doctest=%s\n' "$doctest"
    printf 'doctest_packages=%s\n' "$doctest_packages"
    if [[ -n "$consumer_contract" ]]; then
        if [[ "$local_force_all" == "$TRUE" ]]; then
            printf '%s\n' "$consumer_contract" | grep '^local_' | sed 's/=false$/=true/'
        else
            printf '%s\n' "$consumer_contract" | grep '^local_'
        fi
        local gate
        for gate in mermaid-ci mermaid-test mermaid-check mermaid-render-check; do
            printf 'local_%s=%s\n' "$gate" "$mermaid"
        done
    fi
}

# The same reviewed reader/input contract supplements both local and hosted
# selections. A stale reader binding enables its consumers; malformed contracts
# fail before any selection is published. No source-text inference is involved.
apply_consumer_contract() {
    local source_root python_command
    source_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
    case "$(uname -s)" in
        MINGW* | MSYS*) python_command=python ;;
        *) python_command=python3 ;;
    esac
    # github/local nest this assignment inside another command substitution,
    # where Bash clears errexit. An explicit exit owns this failure boundary.
    consumer_contract="$("$python_command" -B "$source_root/tools/ci_inputs.py" "$(git rev-parse --show-toplevel)" "$@")" || exit 1
    # Native Windows Python writes CRLF; all shell consumers use LF records.
    consumer_contract="${consumer_contract//$'\r'/}"
    if grep -Fxq 'input_all=true' <<<"$consumer_contract"; then
        mark_unknown
    elif grep -Fxq 'input_router=true' <<<"$consumer_contract"; then
        mark_all
    fi
    if grep -Fxq 'input_ts=true' <<<"$consumer_contract"; then
        ts="$TRUE"
    fi
    if grep -Fxq 'input_mermaid=true' <<<"$consumer_contract"; then
        mermaid="$TRUE"
    fi
}

load_workspace_metadata() {
    if [[ -n "$workspace_metadata_cache" ]]; then
        return
    fi
    command -v cargo >/dev/null || {
        echo "ci router: cargo is required to derive the workspace dependency graph" >&2
        exit 1
    }
    command -v jq >/dev/null || {
        echo "ci router: jq is required to parse cargo metadata on the Ubuntu GitHub runner" >&2
        exit 1
    }
    # Cargo emits native Windows paths when this Bash script calls cargo.exe
    # from Git Bash. Normalize only metadata filesystem fields at ingestion so
    # every downstream ownership rule compares one slash-form representation.
    workspace_metadata_cache="$(
        cargo metadata --no-deps --format-version 1 |
            jq '
                def normalize_windows_path:
                    if test("^[A-Za-z]:\\\\|^\\\\\\\\") then gsub("\\\\"; "/")
                    else .
                    end;

                .packages |= map(
                    .manifest_path |= normalize_windows_path
                    | .dependencies |= map(
                        if .path == null then .
                        else .path |= normalize_windows_path
                        end
                    )
                )
            '
    )"
}

host_dependency_dirs() {
    load_workspace_metadata
    local repo_root
    repo_root="$(git rev-parse --show-toplevel)"
    printf '%s\n' "$workspace_metadata_cache" |
        jq -r --arg root "$repo_root" '
            def package($name): .packages[] | select(.name == $name);
            # Shipping host closure only — exclude cargo kind "dev"/"build" so a
            # test-only edge (e.g. keld-runtime → keld-compat) cannot pull a crate
            # into GUI smoke ownership.
            def local_deps($name):
                [package($name).dependencies[]
                 | select(.path != null and (.kind == null))
                 | .name];
            def walk($pending; $seen):
                if ($pending | length) == 0 then $seen
                else $pending[0] as $next
                    | if ($seen | index($next)) != null then walk($pending[1:]; $seen)
                      else walk(($pending[1:] + local_deps($next)); ($seen + [$next]))
                      end
                end;
            walk(["keld-host"]; []) as $names
            | .packages[]
            | select(.name as $name | $names | index($name))
            | .manifest_path
            | sub("/Cargo.toml$"; "")
            | ltrimstr($root + "/")
        ' | tr -d '\r'
}

workspace_package_entries() {
    load_workspace_metadata
    local repo_root
    repo_root="$(git rev-parse --show-toplevel)"
    printf '%s\n' "$workspace_metadata_cache" |
        jq -r --arg root "$repo_root" '
            .packages[]
            | .name + "\t" + (.manifest_path | sub("/Cargo.toml$"; "") | ltrimstr($root + "/"))
        ' | tr -d '\r'
}

package_for_path() {
    local changed_file="$1"
    local entry_name
    local entry_dir
    while IFS=$'\t' read -r entry_name entry_dir; do
        case "$changed_file" in
            "$entry_dir"/*)
                printf '%s\n' "$entry_name"
                return
                ;;
        esac
    done < <(workspace_package_entries)
    return 1
}

reverse_dependency_closure() {
    local root_package="$1"
    load_workspace_metadata
    printf '%s\n' "$workspace_metadata_cache" |
        jq -r --arg root "$root_package" '
            def local_dependents($name):
                [.packages[]
                 | select(any(.dependencies[]?; .path != null and .name == $name))
                 | .name];
            def walk($pending; $seen):
                if ($pending | length) == 0 then $seen
                else $pending[0] as $next
                    | if ($seen | index($next)) != null then walk($pending[1:]; $seen)
                      else walk(($pending[1:] + local_dependents($next)); ($seen + [$next]))
                      end
                end;
            walk([$root]; [])[]
        ' | tr -d '\r'
}

all_workspace_package_names() {
    workspace_package_entries | cut -f1
}

package_requires_webkitgtk() {
    local root_package="$1"
    load_workspace_metadata
    printf '%s\n' "$workspace_metadata_cache" |
        jq -e --arg root "$root_package" '
            def package($name): .packages[] | select(.name == $name);
            def local_deps($name):
                # CI builds `--all-targets`, so dev-dependencies can be required
                # to compile selected package test targets on Ubuntu.
                [package($name).dependencies[] | select(.path != null) | .name];
            def walk($pending; $seen):
                if ($pending | length) == 0 then $seen
                else $pending[0] as $next
                    | if ($seen | index($next)) != null then walk($pending[1:]; $seen)
                      else walk(($pending[1:] + local_deps($next)); ($seen + [$next]))
                      end
                end;
            walk([$root]; []) | index("keld-wv") != null
        ' >/dev/null
}

add_changed_package_root() {
    local package_name="$1"
    changed_package_roots+=("$package_name")
}

# An external consumer edge (the CLI registry reader or a declared
# input_package_* reader) adds its package. When tools/ci_inputs.py reports
# that every changed path behind that edge is a reviewed documentation read,
# the package is tested without its Cargo reverse dependents: documentation
# bytes change no reverse dependent's compiled API or test input. Any other
# matching path keeps today's reverse-dependent expansion.
add_consumer_package_root() {
    local package_name="$1"
    local edge="$2"
    if grep -Fxq "documentation_only_${edge}=true" <<<"$consumer_contract"; then
        documentation_reader_roots+=("$package_name")
    else
        add_changed_package_root "$package_name"
    fi
}

add_changed_ts_package_dir() {
    local package_dir="$1"
    local existing
    if [[ ${#changed_ts_package_dirs[@]} -gt 0 ]]; then
        for existing in "${changed_ts_package_dirs[@]}"; do
            if [[ "$existing" == "$package_dir" ]]; then
                return
            fi
        done
    fi
    changed_ts_package_dirs+=("$package_dir")
}

# TypeScript packages are not cargo members, so their owner comes from the
# checked-out package.json files instead of cargo metadata. The outermost
# manifest below packages/ owns the path: a nested fixture manifest
# (packages/@keld/electron/fixtures) is part of its parent package's contract,
# not a separate owner, and using the parent keeps the broadest consumer needle.
ts_package_dir_for_path() {
    local changed_file="$1"
    local repo_root
    repo_root="$(git rev-parse --show-toplevel)"
    local prefix="packages"
    local rest="${changed_file#packages/}"
    while [[ "$rest" == */* ]]; do
        prefix="$prefix/${rest%%/*}"
        rest="${rest#*/}"
        if [[ -f "$repo_root/$prefix/package.json" ]]; then
            printf '%s\n' "$prefix"
            return 0
        fi
    done
    return 1
}

# A crate that reads a package directory must re-run its tests when that
# directory changes: crates/keld-compat spawns the `@keld/electron` fixtures
# over a real kipc session, so a shim edit can break a Rust conformance test.
# cargo metadata cannot express a crate -> npm package edge, so derive the
# consumers from the crate sources that name the directory rather than from a
# hand-maintained list. A hit that no workspace package owns is not proof of
# "no consumer"; report failure so the caller fails safe.
ts_package_consumer_packages() {
    local package_dir="$1"
    local repo_root
    repo_root="$(git rev-parse --show-toplevel)"
    [[ -d "$repo_root/crates" ]] || return 0
    local hit
    local consumer
    while IFS= read -r hit; do
        [[ -z "$hit" ]] && continue
        if ! consumer="$(package_for_path "${hit#"$repo_root"/}")"; then
            return 1
        fi
        printf '%s\n' "$consumer"
    done < <(grep -rlF --exclude-dir=target -- "$package_dir" "$repo_root/crates" 2>/dev/null || true)
}

# Print every file below a package directory that `bun test` would actually
# run. Keeping this discovery in one helper lets both suite selection and
# cross-tree fixture consumers follow the same pinned Bun contract.
#
# The shapes below mirror bun 1.4.0's own discovery, measured rather than read
# off its error text (which understates the set). Of 21 planted filenames it
# runs 18: `.test.` and `.spec.` and `_test.` and `_spec.`, across
# `ts tsx js jsx mts cts mjs cjs`, matched CASE-INSENSITIVELY (`q.Test.ts` and
# `r.SPEC.ts` both run). It skips `plain.ts`, `test.ts` and `tests.ts`.
#
# This mirrors a rule Bun owns, which makes it a second owner and a drift risk.
# `discovery_shape_case` in tools/ci_changes_test.sh drives every one of the 32
# patterns through the real router, so deleting any single clause here fails
# that suite. It pins THIS side only: the test never invokes bun, so a change on
# Bun's side cannot fail it — re-measure when the pinned version moves. KEL-115's
# receipt — the lane reporting which suites it actually ran — removes the
# duplication entirely and is the real fix.
ts_test_files_in_dir() {
    local dir="$1"
    find "$dir" -name node_modules -prune -o \
        \( -iname '*.test.ts' -o -iname '*.test.tsx' -o -iname '*.test.js' \
            -o -iname '*.test.jsx' -o -iname '*.test.mts' -o -iname '*.test.cts' \
            -o -iname '*.test.mjs' -o -iname '*.test.cjs' \
            -o -iname '*.spec.ts' -o -iname '*.spec.tsx' -o -iname '*.spec.js' \
            -o -iname '*.spec.jsx' -o -iname '*.spec.mts' -o -iname '*.spec.cts' \
            -o -iname '*.spec.mjs' -o -iname '*.spec.cjs' \
            -o -iname '*_test.ts' -o -iname '*_test.tsx' -o -iname '*_test.js' \
            -o -iname '*_test.jsx' -o -iname '*_test.mts' -o -iname '*_test.cts' \
            -o -iname '*_test.mjs' -o -iname '*_test.cjs' \
            -o -iname '*_spec.ts' -o -iname '*_spec.tsx' -o -iname '*_spec.js' \
            -o -iname '*_spec.jsx' -o -iname '*_spec.mts' -o -iname '*_spec.cts' \
            -o -iname '*_spec.mjs' -o -iname '*_spec.cjs' \) -print0
}

# A Bun suite root is a package.json directory that owns at least one test
# file. The fixtures package owns none: it is spawned by the Rust conformance
# test, not by `bun test`.
ts_test_package_dirs() {
    local repo_root
    repo_root="$(git rev-parse --show-toplevel)"
    [[ -d "$repo_root/packages" ]] || return 0
    local manifest
    local dir
    local test_file
    while IFS= read -r manifest; do
        [[ -z "$manifest" ]] && continue
        dir="${manifest%/package.json}"
        if IFS= read -r -d '' test_file < <(ts_test_files_in_dir "$dir"); then
            printf '%s\n' "${dir#"$repo_root"/}"
        fi
    done < <(find "$repo_root/packages" -name package.json -not -path '*/node_modules/*' -print)
}

# A crate fixture can also be an input to Bun tests. Cargo metadata cannot
# express this crate-fixture -> npm-package edge, so derive it from actual Bun
# test files that name the changed path. Searching only suite roots preserves
# the existing no-empty-selection contract and excludes source-only mentions.
ts_fixture_consumer_package_dirs() {
    local changed_file="$1"
    local repo_root
    repo_root="$(git rev-parse --show-toplevel)"
    local package_dir
    local test_file
    local grep_status
    while IFS= read -r package_dir; do
        [[ -z "$package_dir" ]] && continue
        while IFS= read -r -d '' test_file; do
            if grep -Fq -- "$changed_file" "$test_file"; then
                printf '%s\n' "$package_dir"
                break
            else
                grep_status=$?
                if [[ "$grep_status" -gt 1 ]]; then
                    return 1
                fi
            fi
        done < <(ts_test_files_in_dir "$repo_root/$package_dir")
    done < <(ts_test_package_dirs)
}

resolve_crate_fixture_consumers() {
    local changed_file="$1"
    local consumers
    local package_dir
    if ! consumers="$(ts_fixture_consumer_package_dirs "$changed_file")"; then
        mark_unknown
        return
    fi
    for package_dir in $consumers; do
        ts="$TRUE"
        add_changed_ts_package_dir "$package_dir"
    done
}

# Runs before the Rust selection so a derived crate consumer joins the same
# reverse-dependency expansion an edit inside that crate would get.
resolve_ts_package_consumers() {
    if [[ ${#changed_ts_package_dirs[@]} -eq 0 ]]; then
        return
    fi
    local package_dir
    local consumers
    local consumer
    for package_dir in "${changed_ts_package_dirs[@]}"; do
        if ! consumers="$(ts_package_consumer_packages "$package_dir")"; then
            mark_unknown
            return
        fi
        for consumer in $consumers; do
            rust="$TRUE"
            add_changed_package_root "$consumer"
        done
    done
}

finalize_rust_packages() {
    if [[ "$rust" != "$TRUE" ]]; then
        return
    fi

    local package_name
    local expanded=""
    if [[ "$all_workspace_packages" == "$TRUE" ]]; then
        expanded="$(all_workspace_package_names)"
    elif [[ ${#changed_package_roots[@]} -gt 0 || ${#documentation_reader_roots[@]} -gt 0 ]]; then
        if [[ ${#changed_package_roots[@]} -gt 0 ]]; then
            for package_name in "${changed_package_roots[@]}"; do
                expanded+="$(reverse_dependency_closure "$package_name")"$'\n'
            done
        fi
        if [[ ${#documentation_reader_roots[@]} -gt 0 ]]; then
            for package_name in "${documentation_reader_roots[@]}"; do
                expanded+="$package_name"$'\n'
            done
        fi
    else
        # A Rust lane without an attributable workspace package must never
        # become an empty success. Use the complete workspace as the safe
        # compatibility fallback.
        expanded="$(all_workspace_package_names)"
    fi
    packages="$(printf '%s' "$expanded" | sed '/^$/d' | sort -u | paste -sd ' ' -)"

    local nongtk=""
    local selected_requires_webkitgtk="$FALSE"
    for package_name in $packages; do
        if package_requires_webkitgtk "$package_name"; then
            selected_requires_webkitgtk="$TRUE"
        else
            nongtk+="$package_name"$'\n'
        fi
    done
    nongtk_packages="$(printf '%s' "$nongtk" | sed '/^$/d' | sort -u | paste -sd ' ' -)"

    # Documentation bytes reach these packages' unchanged tests only as text,
    # and .gitattributes checks text out as LF on every OS, so one OS proves
    # them: no reader of a declared document is skipped or handles it under
    # cfg(windows) or cfg(target_os). Windows is that OS because it links no
    # WebKitGTK, so the leg has no network apt step (keld-cli and keld-update
    # reach keld-wv; on Ubuntu that step took 30-642 s and once hung 44 min).
    # Any changed package keeps all three OSes (#624).
    local ubuntu_leg="$TRUE"
    if [[ "$all_workspace_packages" != "$TRUE" && ${#changed_package_roots[@]} -eq 0 && \
        ${#documentation_reader_roots[@]} -gt 0 ]]; then
        check_os="$DOCUMENTATION_CHECK_OS"
        rust_documentation_only="$TRUE"
        ubuntu_leg="$FALSE"
    fi

    # An attributable selected `--all-targets` closure that reaches keld-wv
    # installs GTK and runs its original package set on Ubuntu. The workflow
    # consumes this derived selection directly instead of recomputing policy.
    # The all-workspace workflow/router fallback keeps its documented GTK-free
    # subset because GUI smoke is the sole live apt owner for that input class.
    if [[ "$selected_requires_webkitgtk" == "$TRUE" && "$all_workspace_packages" != "$TRUE" && \
        "$ubuntu_leg" == "$TRUE" ]]; then
        webkitgtk="$TRUE"
    fi
    if [[ "$ubuntu_leg" != "$TRUE" ]]; then
        # No Ubuntu leg runs, so it has no package set; the Windows leg runs
        # the full selection, which must still be non-empty.
        ubuntu_packages=""
        if [[ -z "$packages" ]]; then
            echo "ci router: documentation-only Rust checks selected no package; refusing to emit a skipped-green success" >&2
            exit 1
        fi
        return
    fi
    if [[ "$webkitgtk" == "$TRUE" ]]; then
        ubuntu_packages="$packages"
    else
        ubuntu_packages="$nongtk_packages"
    fi

    if [[ -z "$ubuntu_packages" ]]; then
        echo "ci router: Rust checks selected no Ubuntu packages; refusing to emit a skipped-green success" >&2
        exit 1
    fi
}

# Workspace packages with a library target, the only targets that carry
# doctests; `cargo test --doc` refuses a bin-only package.
library_package_names() {
    load_workspace_metadata
    printf '%s\n' "$workspace_metadata_cache" |
        jq -r '
            .packages[]
            | select(any(.targets[]?; any(.kind[]?; . == "lib" or . == "rlib" or . == "dylib" or . == "proc-macro")))
            | .name
        ' | tr -d '\r'
}

# The doctest lane uses the same selection as clippy/test, restricted to
# packages that can have doctests. A Rust selection of only bin-only packages
# has no doctest to run and leaves the lane unselected.
finalize_doctest_packages() {
    if [[ "$rust" != "$TRUE" ]]; then
        return
    fi
    local libraries package_name selected=""
    # github/local run this inside a command substitution, where Bash clears
    # errexit: a failed metadata or jq read must stop the router before it
    # publishes any output, never become an empty doctest selection.
    if ! libraries="$(library_package_names)"; then
        echo "ci router: cannot list library packages from cargo metadata; refusing to emit a doctest selection" >&2
        exit 1
    fi
    for package_name in $packages; do
        if grep -Fxq -- "$package_name" <<<"$libraries"; then
            selected+="$package_name"$'\n'
        fi
    done
    doctest_packages="$(printf '%s' "$selected" | sed '/^$/d' | sort -u | paste -sd ' ' -)"
    if [[ -n "$doctest_packages" ]]; then
        doctest="$TRUE"
    fi
}

finalize_ts_packages() {
    if [[ "$ts" != "$TRUE" ]]; then
        return
    fi
    ts_packages="$(ts_test_package_dirs | sort -u | paste -sd ' ' -)"
    if [[ -z "$ts_packages" ]]; then
        echo "ci router: the TypeScript lane is selected but no packages/ Bun suite was found; refusing to emit a skipped-green success" >&2
        exit 1
    fi
}

# The Rust selection stays first: its empty-Ubuntu guard owns the error a
# broken workspace metadata read must report.
finalize_selection() {
    resolve_ts_package_consumers
    if grep -Fxq 'input_registry=true' <<<"$consumer_contract"; then
        local registry_owner
        if [[ "$rust" == "$TRUE" && ${#changed_package_roots[@]} -eq 0 ]]; then
            # Shared Rust/build-graph inputs already require the whole workspace.
            # Adding one external reader must not narrow that fallback to CLI.
            all_workspace_packages="$TRUE"
        elif registry_owner="$(package_for_path crates/keld-cli/tests/error_registry.rs)"; then
            rust="$TRUE"
            add_consumer_package_root "$registry_owner" registry
        else
            mark_unknown
        fi
    fi
    # Runtime file/include edges are not Cargo dependency edges. Add their
    # declared consuming packages before expanding Cargo reverse dependents.
    local consumer_key consumer_selected consumer_package
    while IFS='=' read -r consumer_key consumer_selected; do
        if [[ "$consumer_key" == input_package_* && "$consumer_selected" == "$TRUE" ]]; then
            consumer_package="${consumer_key#input_package_}"
            if ! all_workspace_package_names | grep -Fxq -- "$consumer_package"; then
                mark_unknown
                break
            fi
            rust="$TRUE"
            add_consumer_package_root "$consumer_package" "package_${consumer_package}"
        fi
    done <<<"$consumer_contract"
    if grep -Fxq 'input_rust=true' <<<"$consumer_contract" && [[ "$rust" != "$TRUE" ]]; then
        rust="$TRUE"
        all_workspace_packages="$TRUE"
    fi
    finalize_rust_packages
    finalize_doctest_packages
    finalize_ts_packages
}

host_path_is_affected() {
    local changed_file="$1"
    local host_dir

    # Documentation alone cannot alter the `keld-host --hello` executable.
    # Any other file below an actual host dependency is deliberately treated as
    # relevant: an unrecognised build input must run the smoke, not skip it.
    case "$changed_file" in
        *.adoc | *.md | *.mdx | *.rst | *.txt) return 1 ;;
    esac

    while IFS= read -r host_dir; do
        [[ -z "$host_dir" ]] && continue
        case "$changed_file" in
            "$host_dir"/*) return 0 ;;
        esac
    done <<<"${host_dependency_dirs_cache:-}"
    return 1
}

classify_path() {
    local changed_file="$1"

    classify_codeql_path "$changed_file"

    # Crate-local reports/fixtures can be include_bytes!/include_str! inputs.
    # Documentation routing below is additive, never an exemption from its owner.
    case "$changed_file" in
        crates/*.md | crates/*.txt | crates/*.adoc | crates/*.mdx | crates/*.rst)
            local document_owner
            if document_owner="$(package_for_path "$changed_file")"; then
                rust="$TRUE"
                add_changed_package_root "$document_owner"
            else
                mark_unknown
                return
            fi
            ;;
    esac

    if [[ "$changed_file" == *.md ]]; then
        markdown_changed="$TRUE"
    fi

    case "$changed_file" in
        # Agent instruction and assembly changes must run both generated-doc
        # freshness and the merge-blocking instruction-context/hygiene gates.
        AGENTS.md | AGENTS.override.md | */AGENTS.md | */AGENTS.override.md | CLAUDE.md | \
        .agents/*.md | .agents/*.txt | docs/agents/*.md)
            docs="$TRUE"
            hygiene="$TRUE"
            ;;

        # Markdown-like content is documentation even if it lives beside a
        # crate. It cannot alter the compiled host executable.
        *.adoc | *.md | *.mdx | *.rst | *.txt)
            docs="$TRUE"
            ;;

        # The router, merge evaluator, and every workflow can change the
        # repository's automation or required-check surface. Any edit here must
        # exercise every conditional lane, otherwise a workflow can introduce a
        # false-green check while skipping the contracts that would expose it.
        .github/workflows/* | tools/ci_changes.sh | tools/ci_changes_test.sh | tools/ci_required.sh | \
        tools/ci_inputs.py | tools/ci_local.py | tools/test_ci_local.py | tools/ci-inputs.json)
            mark_all
            ;;

        # gitleaks loads its configuration and fingerprint ignores from the
        # repository root, and the gitleaks job runs on every event. No other
        # lane reads these files, so they select nothing else (#624).
        .gitleaks.toml | .gitleaksignore)
            ;;

        # The workspace contract job's whole input: test_workspace.py imports
        # workspace.py, which imports session_closeout.py. These also drive the
        # hygiene lane and local agent tooling, so they keep the unknown
        # fallback; naming the job here keeps it selected if that is narrowed.
        tools/workspace.py | tools/test_workspace.py | tools/session_closeout.py)
            mark_unknown
            workspace="$TRUE"
            ;;

        # Workspace and toolchain inputs can alter every Rust build, keld-host's
        # dependency closure, or dependency-policy resolution.
        Cargo.toml | Cargo.lock | rust-toolchain.toml | rustfmt.toml)
            rust="$TRUE"
            all_workspace_packages="$TRUE"
            gui="$TRUE"
            msrv="$TRUE"
            deny="$TRUE"
            webkitgtk="$TRUE"
            ;;
        .cargo/* | .config/nextest.toml)
            mark_unknown
            ;;
        deny.toml)
            deny="$TRUE"
            ;;

        crates/*)
            rust="$TRUE"
            case "$changed_file" in
                crates/*/tests/fixtures/*) resolve_crate_fixture_consumers "$changed_file" ;;
                # The elevated updater helper's own ban list is also a cargo-deny
                # input (KEL-53 §4 "Helper launch and self-anchor").
                crates/keld-updater-helper/deny.toml) deny="$TRUE" ;;
            esac
            local package_name
            if ! package_name="$(package_for_path "$changed_file")"; then
                mark_unknown
                return
            fi
            add_changed_package_root "$package_name"
            msrv="$TRUE"
            if host_path_is_affected "$changed_file"; then
                gui="$TRUE"
            fi
            # Ubuntu clippy/MSRV apt is for linking WebKitGTK, not for every
            # reverse-dependent that happens to compile keld-core. keld-compat
            # and keld-cli still get macOS/Windows clippy plus MSRV on macOS.
            case "$package_name" in
                keld-wv | keld-core | keld-host)
                    webkitgtk="$TRUE"
                    ;;
            esac
            ;;

        # A TypeScript/JavaScript package owns the Bun test lane. Its Rust
        # ownership is derived, never assumed: crates/keld-compat spawns the
        # `@keld/electron` fixtures, so a shim change must still re-run that
        # crate's conformance tests (see resolve_ts_package_consumers).
        # Markdown under packages/ already matched the documentation arm above.
        packages/*)
            ts="$TRUE"
            local ts_package_dir
            if ! ts_package_dir="$(ts_package_dir_for_path "$changed_file")"; then
                # No package.json owns this path, so no npm package declares
                # it. An unowned input fails closed like any other unknown.
                mark_unknown
                return
            fi
            add_changed_ts_package_dir "$ts_package_dir"
            ;;

        # These tools own the generated-doc and Mermaid contracts.
        docs/research | docs/* | llms.txt | llms-full.txt | README.md | CONTRIBUTING.md | \
        tools/llms_docs.rs | tools/mermaid_docs.rs | tools/mermaid_render_check.sh | tools/mermaid-render-config.json)
            docs="$TRUE"
            case "$changed_file" in
                docs/research | tools/mermaid_docs.rs | tools/mermaid_render_check.sh | tools/mermaid-render-config.json)
                    mermaid="$TRUE"
                    ;;
            esac
            ;;

        # These inputs own the repository-hygiene contract, but do not affect a
        # product build or graphical window.
        .github/CODEOWNERS | .github/PULL_REQUEST_TEMPLATE.md | .github/ISSUE_TEMPLATE/* | \
        .gitignore | justfile | .codex/* | .agents/instruction-budget.tsv | \
        tools/ci_hygiene.rs | tools/atomic_protocol.rs | tools/agent_context.rs | tools/markdown_contract.rs | \
        tools/justfile_contract.rs)
            hygiene="$TRUE"
            # The shared justfile parser decides which recipes count as the Mermaid gate.
            if [[ "$changed_file" == justfile || "$changed_file" == tools/ci_hygiene.rs || \
                "$changed_file" == tools/justfile_contract.rs ]]; then
                mermaid="$TRUE"
            fi
            ;;

        # A known non-executable top-level document stays in its docs lane.
        LICENSE | LICENSE.* | NOTICE | NOTICE.*)
            docs="$TRUE"
            ;;

        # A future package, build input, or unknown path has no proven owner.
        # Failing closed means execute all non-security lanes, never silently
        # omit a check based on a filename guess.
        *)
            mark_unknown
            ;;
    esac
}

route_mermaid_diff() {
    local mode="$1"
    local root
    local source_root
    local binary
    local selection
    root="$(git rev-parse --show-toplevel)"
    source_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
    binary="$root/target/ci-router/mermaid-docs"
    mkdir -p "$(dirname "$binary")"
    if ! rustc --edition=2024 -D warnings "$source_root/tools/mermaid_docs.rs" -o "$binary"; then
        echo "ci router: Mermaid diff classifier did not compile; selecting full Mermaid validation/rendering" >&2
        mermaid="$TRUE"
        return
    fi
    case "$mode" in
        range)
            selection="$("$binary" changes "$root" "$2" "$3")" || selection="unknown"
            ;;
        worktree)
            selection="$("$binary" worktree "$root" "$2")" || selection="unknown"
            ;;
        *)
            selection="unknown"
            ;;
    esac
    case "$selection" in
        true) mermaid="$TRUE" ;;
        false) mermaid="$FALSE" ;;
        *)
            echo "ci router: Mermaid applicability is unknown; selecting full Mermaid validation/rendering" >&2
            mermaid="$TRUE"
            ;;
    esac
}

classify_stream() {
    local mode="${1:-paths}"
    local base="${2:-}"
    local head="${3:-}"
    host_dependency_dirs_cache="$(host_dependency_dirs)"
    local changed_file
    local -a changed_files=()
    while IFS= read -r -d '' changed_file; do
        changed_files+=("$changed_file")
        classify_path "$changed_file"
    done
    local -a contract_options=()
    if [[ "$mode" == paths ]]; then
        contract_options+=(--paths-only)
    fi
    if [[ ${#changed_files[@]} -gt 0 ]]; then
        apply_consumer_contract "${contract_options[@]}" < <(printf '%s\0' "${changed_files[@]}")
    else
        apply_consumer_contract "${contract_options[@]}" </dev/null
    fi
    finalize_selection
    if [[ "$mode" == worktree && "$markdown_changed" != "$TRUE" ]]; then
        local root research_root research_top
        root="$(git rev-parse --show-toplevel)"
        research_root="$root/docs/research"
        if [[ -d "$research_root" && ! -L "$research_root" ]] && \
            research_top="$(git -C "$research_root" rev-parse --show-toplevel 2>/dev/null || true)" && \
            [[ -n "$research_top" && "$research_top" == "$(cd "$research_root" && pwd -P)" ]]; then
            markdown_changed="$TRUE"
        fi
    fi
    if [[ "$mermaid" != "$TRUE" && "$markdown_changed" == "$TRUE" ]]; then
        case "$mode" in
            range) route_mermaid_diff range "$base" "$head" ;;
            worktree) route_mermaid_diff worktree "$base" "$head" ;;
            *) mermaid="$TRUE" ;; # Path-only classification has no content oracle.
        esac
    fi
    emit
}

publish() {
    local result="$1"
    if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
        printf '%s\n' "$result" >>"$GITHUB_OUTPUT"
    fi
    printf '%s\n' "$result"
}

classify_github_event() {
    local event_name="${KELD_CI_EVENT_NAME:-${GITHUB_EVENT_NAME:-}}"
    local base_sha=""
    local head_sha=""

    case "$event_name" in
        pull_request)
            base_sha="${KELD_CI_BASE_SHA:-}"
            head_sha="${KELD_CI_HEAD_SHA:-${GITHUB_SHA:-}}"
            ;;
        push)
            base_sha="${KELD_CI_BEFORE_SHA:-}"
            head_sha="${GITHUB_SHA:-}"
            # Only pull requests skip unaffected CodeQL languages (#624).
            select_every_codeql_language
            ;;
        *)
            mark_unknown
            apply_consumer_contract --unknown </dev/null
            finalize_selection
            publish "$(emit)"
            return
            ;;
    esac

    # First push and an unavailable comparison base are not safe to classify.
    # Run every conditional lane rather than trusting a partial history.
    if [[ -z "$base_sha" || -z "$head_sha" || "$base_sha" =~ ^0+$ ]] || \
        ! git cat-file -e "${base_sha}^{commit}" 2>/dev/null || \
        ! git cat-file -e "${head_sha}^{commit}" 2>/dev/null; then
        mark_unknown
        apply_consumer_contract --unknown </dev/null
        finalize_selection
        publish "$(emit)"
        return
    fi

    local result
    result="$(git diff --no-renames --name-only -z "$base_sha" "$head_sha" | classify_stream range "$base_sha" "$head_sha")"
    publish "$result"
}

classify_local_worktree() {
    local base="${KELD_CI_BASE_REF:-origin/main}"
    local head
    head="$(git rev-parse HEAD 2>/dev/null || true)"
    if [[ -z "$head" ]] || ! git cat-file -e "${base}^{commit}" 2>/dev/null || \
        ! git merge-base --is-ancestor "$base" "$head" >/dev/null 2>&1; then
        mark_unknown
        apply_consumer_contract --unknown </dev/null
        finalize_selection
        publish "$(emit)"
        return
    fi
    local result
    # Do not let a successful untracked census hide a failed tracked diff.
    result="$( { git diff --no-renames --name-only -z "$base" -- && git ls-files --others --exclude-standard -z; } | classify_stream worktree "$base" "$head")"
    publish "$result"
}

case "${1:-}" in
    classify)
        classify_stream
        ;;
    github)
        classify_github_event
        ;;
    local)
        classify_local_worktree
        ;;
    host-dirs)
        host_dependency_dirs
        ;;
    *)
        usage
        ;;
esac

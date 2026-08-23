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
hygiene="$FALSE"
gui="$FALSE"
msrv="$FALSE"
deny="$FALSE"
webkitgtk="$FALSE"
packages=""
nongtk_packages=""
ubuntu_packages=""
all_workspace_packages="$FALSE"
workspace_metadata_cache=""
host_dependency_dirs_cache=""
declare -a changed_package_roots=()

usage() {
    echo "usage: $0 {classify|github|host-dirs}" >&2
    echo "  classify   read NUL-delimited changed paths from stdin" >&2
    echo "  github     derive the comparison from GitHub Actions environment" >&2
    echo "  host-dirs  print the current keld-host local dependency closure" >&2
    exit 2
}

mark_all() {
    rust="$TRUE"
    docs="$TRUE"
    hygiene="$TRUE"
    gui="$TRUE"
    msrv="$TRUE"
    deny="$TRUE"
    all_workspace_packages="$TRUE"
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
    printf 'hygiene=%s\n' "$hygiene"
    printf 'gui=%s\n' "$gui"
    printf 'msrv=%s\n' "$msrv"
    printf 'deny=%s\n' "$deny"
    printf 'webkitgtk=%s\n' "$webkitgtk"
    printf 'packages=%s\n' "$packages"
    printf 'nongtk_packages=%s\n' "$nongtk_packages"
    printf 'ubuntu_packages=%s\n' "$ubuntu_packages"
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
    workspace_metadata_cache="$(cargo metadata --no-deps --format-version 1)"
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
        '
}

workspace_package_entries() {
    load_workspace_metadata
    local repo_root
    repo_root="$(git rev-parse --show-toplevel)"
    printf '%s\n' "$workspace_metadata_cache" |
        jq -r --arg root "$repo_root" '
            .packages[]
            | .name + "\t" + (.manifest_path | sub("/Cargo.toml$"; "") | ltrimstr($root + "/"))
        '
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
        '
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

finalize_rust_packages() {
    if [[ "$rust" != "$TRUE" ]]; then
        return
    fi

    local package_name
    local expanded=""
    if [[ "$all_workspace_packages" == "$TRUE" ]]; then
        expanded="$(all_workspace_package_names)"
    elif [[ ${#changed_package_roots[@]} -gt 0 ]]; then
        for package_name in "${changed_package_roots[@]}"; do
            expanded+="$(reverse_dependency_closure "$package_name")"$'\n'
        done
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

    # An attributable selected `--all-targets` closure that reaches keld-wv
    # installs GTK and runs its original package set on Ubuntu. The workflow
    # consumes this derived selection directly instead of recomputing policy.
    # The all-workspace workflow/router fallback keeps its documented GTK-free
    # subset because GUI smoke is the sole live apt owner for that input class.
    if [[ "$selected_requires_webkitgtk" == "$TRUE" && "$all_workspace_packages" != "$TRUE" ]]; then
        webkitgtk="$TRUE"
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

    case "$changed_file" in
        # Markdown-like content is documentation even if it lives beside a
        # crate. It cannot alter the compiled host executable.
        *.adoc | *.md | *.mdx | *.rst | *.txt)
            docs="$TRUE"
            ;;

        # The router and workflow decide which jobs execute. Any edit here must
        # exercise every conditional lane, otherwise a classification defect can
        # hide the very job that would expose it.
        .github/workflows/* | tools/ci_changes.sh | tools/ci_changes_test.sh)
            mark_all
            ;;

        # Workspace and toolchain inputs can alter every Rust build, keld-host's
        # dependency closure, or dependency-policy resolution.
        Cargo.toml | Cargo.lock | rust-toolchain.toml | rustfmt.toml)
            rust="$TRUE"
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

        # These tools own the generated-doc and Mermaid contracts.
        docs/* | AGENTS.md | .agents/* | llms.txt | llms-full.txt | README.md | CONTRIBUTING.md | \
        tools/llms_docs.rs | tools/mermaid_docs.rs | tools/mermaid_render_check.sh | tools/mermaid-render-config.json)
            docs="$TRUE"
            ;;

        # These inputs own the repository-hygiene contract, but do not affect a
        # product build or graphical window.
        .github/CODEOWNERS | .github/PULL_REQUEST_TEMPLATE.md | .github/ISSUE_TEMPLATE/* | \
        .gitignore | justfile | tools/ci_hygiene.rs)
            hygiene="$TRUE"
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

classify_stream() {
    host_dependency_dirs_cache="$(host_dependency_dirs)"
    local changed_file
    while IFS= read -r -d '' changed_file; do
        classify_path "$changed_file"
    done
    finalize_rust_packages
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
            ;;
        *)
            mark_unknown
            finalize_rust_packages
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
        finalize_rust_packages
        publish "$(emit)"
        return
    fi

    local result
    result="$(git diff --no-renames --name-only -z "$base_sha" "$head_sha" | classify_stream)"
    publish "$result"
}

case "${1:-}" in
    classify)
        classify_stream
        ;;
    github)
        classify_github_event
        ;;
    host-dirs)
        host_dependency_dirs
        ;;
    *)
        usage
        ;;
esac

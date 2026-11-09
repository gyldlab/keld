#!/usr/bin/env bash
# rustfmt coverage for Rust files `cargo fmt` cannot reach (#834, #1055): the
# one copy of this check. `cargo fmt --all` follows `mod` items only and never
# expands macros, so it misses two kinds of file:
#   - a file pulled in by a literal `include!("x.rs")`, and every `mod` file
#     declared inside it;
#   - a file declared by `#[path = "x.rs"] mod m;` inside a macro body (for
#     example `keld_native_fs_observer_items! { ... }`), and every `mod` file
#     declared inside it, `#[path]` or not.
# Both kinds are discovered from every tracked `.rs` file, so a new declaration,
# and a `#[path]` nested in a `#[path]` file, is covered without editing a list.
# Each file is handed to rustfmt, which then also follows the `mod` items
# declared in it. A `#[path]` file `cargo fmt` already reaches is checked twice,
# harmlessly.
#
# `#[path]` discovery is fail-closed: a declared file that does not exist, a
# `#[path]` form this script cannot read (`cfg_attr`, multi-line, raw string)
# and a `#[path]` inside an inline `mod`/`fn` block (which rustc resolves
# against another directory) are errors, not silent skips. A top-level or
# macro-body `#[path]` resolves against the declaring file's directory, as in
# rustc.
#
#   tools/fmt_included.sh           format the files in place (`just fmt`)
#   tools/fmt_included.sh --check   fail on drift (`just fmt-check`, hosted CI)
#   tools/fmt_included.sh test      self-test against fixture trees
set -euo pipefail

# Must match the workspace edition in Cargo.toml: rustfmt defaults to 2015 when
# it is given a bare file, and cargo is what normally supplies the edition.
edition=2024

# Print one included file path per line: plain `include!("...rs");` statements
# on their own line. `concat!`/`env!` forms and mentions in comments or string
# literals do not match, and an include that resolves to a missing file is
# left to the compiler.
included_files() {
    # `|| true`: no match must reach the vacuous-pass refusal in `run`, not end
    # the script silently under `set -e`/`pipefail`.
    { git grep -nE '^[[:space:]]*include!\("[^"]+\.rs"\);?[[:space:]]*$' -- '*.rs' || true; } |
        sed -E 's#^(.*):[0-9]+:[[:space:]]*include!\("([^"]+)"\).*$#\1\t\2#' |
        while IFS=$'\t' read -r includer target; do
            path="$(dirname "$includer")/$target"
            if [[ -f "$path" ]]; then printf '%s\n' "$path"; fi
        done | sort -u
}

# Print one `#[path]`-declared file per line; fail on any declaration it cannot
# resolve. Every line that starts a `#[path` (or `#[cfg_attr(..., path = ...)]`)
# attribute is a declaration; awk reports each as
# `OK<TAB>declaring file<TAB>target` or `ERR<TAB>file:line<TAB>reason`. The
# block enclosing an indented line is the last less-indented line above it that
# is still open; only a macro invocation (`name! {`) is accepted there.
path_files() {
    local -a declaring
    mapfile -t declaring < <(git grep -lE \
        '^[[:space:]]*#[[:space:]]*\[[[:space:]]*(path[[:space:]]*[]=]|cfg_attr[[:space:]]*\(.*[(,[:space:]]path[[:space:]]*=)' \
        -- '*.rs' || true)
    ((${#declaring[@]})) || return 0
    local records status=0 kind where what
    records="$(awk '
        FNR == 1 { split("", last) }
        /^[ \t]*$/ { next }
        {
            match($0, /^[ \t]*/); ind = RLENGTH
            for (j in last) if (j + 0 >= ind) delete last[j]
        }
        /^[ \t]*#[ \t]*\[[ \t]*cfg_attr[ \t]*\(.*[(, \t]path[ \t]*=/ {
            printf "ERR\t%s:%d\tcfg_attr(..., path = ...) is not supported; use a plain #[path = \"...\"]\n", FILENAME, FNR
        }
        /^[ \t]*#[ \t]*\[[ \t]*path[ \t]*[]=]/ {
            if ($0 !~ /^[ \t]*#\[path = "[^"]+"\][ \t]*$/) {
                printf "ERR\t%s:%d\tunsupported #[path] form; write #[path = \"file.rs\"] on one line\n", FILENAME, FNR
            } else {
                ctx = ""; best = -1
                for (j in last) if (j + 0 < ind && j + 0 > best) { best = j + 0; ctx = last[j] }
                if (ind > 0 && ctx !~ /^[ \t]*([A-Za-z_][A-Za-z0-9_]*::)*[A-Za-z_][A-Za-z0-9_]*![ \t]*\{[ \t]*$/) {
                    printf "ERR\t%s:%d\t#[path] inside a non-macro block resolves against another directory; not supported\n", FILENAME, FNR
                } else {
                    t = $0; sub(/^[ \t]*#\[path = "/, "", t); sub(/"\].*$/, "", t)
                    printf "OK\t%s\t%s\n", FILENAME, t
                }
            }
        }
        { last[ind] = $0 }
    ' "${declaring[@]}")"
    while IFS=$'\t' read -r kind where what; do
        case "$kind" in
            OK)
                if [[ -f "$(dirname "$where")/$what" ]]; then
                    printf '%s\n' "$(dirname "$where")/$what"
                else
                    echo "fmt_included: $where declares #[path = \"$what\"], which does not exist." >&2
                    status=1
                fi
                ;;
            ERR) echo "fmt_included: $where: $what" >&2; status=1 ;;
        esac
    done <<<"$records"
    return "$status"
}

run() {
    local mode="$1" included declared
    cd "$(git rev-parse --show-toplevel)"
    included="$(included_files)"
    if [[ -z "$included" ]]; then
        echo "fmt_included: no include!d Rust files found; refusing a vacuous pass." >&2
        return 1
    fi
    if ! declared="$(path_files)"; then
        echo "fmt_included: #[path] discovery failed (see above)." >&2
        return 1
    fi
    if [[ -z "$declared" ]]; then
        echo "fmt_included: no #[path]-declared Rust files found; refusing a vacuous pass." >&2
        return 1
    fi
    local -a args=(--edition "$edition")
    [[ "$mode" == check ]] && args+=(--check)
    # shellcheck disable=SC2046 -- one path per line, no spaces in tracked paths
    rustfmt "${args[@]}" $(printf '%s\n%s\n' "$included" "$declared" | sort -u)
}

if [[ "${1:-}" == test ]]; then
    self="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/$(basename "${BASH_SOURCE[0]}")"
    temp="$(mktemp -d "${TMPDIR:-/tmp}/keld-fmt-included.XXXXXX")"
    trap 'rm -rf "$temp"' EXIT
    cp rustfmt.toml "$temp/" 2>/dev/null || cp "$(dirname "$self")/../rustfmt.toml" "$temp/"
    cd "$temp"
    git init -q .
    git config core.autocrlf false
    mkdir -p crates/demo/src/tests crates/demo/src/native tools
    cp "$self" tools/fmt_included.sh
    printf 'pub fn lib() {}\nmod obs;\n#[cfg(test)]\nmod tests;\n' >crates/demo/src/lib.rs
    printf 'include!("tests/split.rs");\n' >crates/demo/src/tests.rs
    printf 'mod nested;\nfn split() {}\n' >crates/demo/src/tests/split.rs
    printf 'fn nested() {}\n' >crates/demo/src/tests/nested.rs
    # A `#[path]` mod inside a macro body (cargo fmt never expands it), whose
    # file declares a further `#[path]` mod.
    cat >crates/demo/src/obs.rs <<'RS'
macro_rules! items {
    ($($i:item)*) => { $($i)* };
}
items! {
    #[cfg(test)]
    #[path = "native/outer.rs"]
    mod outer;
}
RS
    printf '#[path = "inner.rs"]\nmod inner;\nfn outer() {}\n' >crates/demo/src/native/outer.rs
    printf 'fn inner() {}\n' >crates/demo/src/native/inner.rs
    git add -A >/dev/null
    cp -R crates "$temp/.pristine"
    reset() { # restore the formatted fixture, tree and index
        git rm -rqf --cached crates >/dev/null
        rm -rf crates
        cp -R "$temp/.pristine" crates
        git add -A crates >/dev/null
    }
    check() { tools/fmt_included.sh --check >"$temp/out" 2>&1; }
    expect_fail() { # <what> [text the failure must print]
        if check; then echo "FAIL: $1 passed" >&2; exit 1; fi
        if [[ -n "${2:-}" ]] && ! grep -qF -- "$2" "$temp/out"; then
            echo "FAIL: $1 failed without '$2':" >&2; cat "$temp/out" >&2; exit 1
        fi
    }

    check || { echo "FAIL: formatted fixture was rejected" >&2; cat "$temp/out" >&2; exit 1; }
    # Negative control 1: cargo fmt cannot see the include!d file at all.
    printf 'fn split(   ) {}\n' >>crates/demo/src/tests/split.rs
    expect_fail "drift in an include!d file"
    # Negative control 2: the `mod` file declared inside it is reached too.
    tools/fmt_included.sh >/dev/null
    check || { echo "FAIL: write mode did not format the include!d file" >&2; exit 1; }
    printf 'fn   nested2() {}\n' >>crates/demo/src/tests/nested.rs
    expect_fail "drift in a mod file under an include!d file"
    reset
    # Negative control 3: a `#[path]` file declared inside a macro body (#1055).
    printf 'fn outer2(   ) {}\n' >>crates/demo/src/native/outer.rs
    expect_fail "drift in a #[path] file declared inside a macro body"
    tools/fmt_included.sh >/dev/null
    check || { echo "FAIL: write mode did not format the #[path] file" >&2; exit 1; }
    reset
    # Negative control 4: a `#[path]` mod nested in that file.
    printf 'fn   inner2() {}\n' >>crates/demo/src/native/inner.rs
    expect_fail "drift in a #[path] file nested in a #[path] file"
    reset
    # Negative control 5: a declared `#[path]` file that does not exist.
    git rm -qf crates/demo/src/native/inner.rs
    expect_fail "a missing #[path] file" 'declares #[path = "inner.rs"], which does not exist'
    reset
    # Negative control 6: `#[path]` inside an inline module resolves elsewhere.
    printf 'mod inline {\n    #[path = "x.rs"]\n    mod x;\n}\n' >>crates/demo/src/obs.rs
    expect_fail "a #[path] inside an inline module" 'inside a non-macro block'
    reset
    # Negative control 7: a `#[path]` form discovery cannot read.
    printf '#[cfg_attr(test, path = "native/inner.rs")]\nmod inner_alias;\n' >>crates/demo/src/obs.rs
    expect_fail "a cfg_attr #[path] form" 'cfg_attr(..., path = ...) is not supported'
    reset
    # Negative control 8: no `#[path]` declaration must not be a silent green.
    printf 'pub fn lib() {}\n#[cfg(test)]\nmod tests;\n' >crates/demo/src/lib.rs
    git rm -rqf crates/demo/src/obs.rs crates/demo/src/native
    expect_fail "a tree with no #[path] declaration" 'no #[path]-declared Rust files found'
    reset
    # Negative control 9: no include! at all must not be a silent green.
    printf '// nothing\n' >crates/demo/src/tests.rs
    expect_fail "a tree with no include!" 'no include!d Rust files found'
    echo "fmt_included self-test: ok"
    exit 0
fi

case "${1:-}" in
    --check) run check ;;
    "") run write ;;
    *) echo "usage: tools/fmt_included.sh [--check|test]" >&2; exit 2 ;;
esac

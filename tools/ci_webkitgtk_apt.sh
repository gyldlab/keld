#!/usr/bin/env bash
# WebKitGTK apt install for the Ubuntu CI legs, with a .deb cache that cannot
# bypass apt's signed metadata (#645).
#
# Trust chain. `apt-get update` verifies the signed InRelease files and, through
# them, the Packages indexes that carry each .deb's SHA256. apt itself does not
# re-hash a file it finds already present in its archive directory: it accepts
# it when only the size matches (apt-pkg/acquire-item.cc,
# pkgAcqArchive::QueueNext, apt 2.7.14 and main). So a restored cache file is
# placed there only after its SHA256 and size equal what the freshly fetched
# indexes say for the exact file apt is about to install
# (`apt-get install --print-uris -o Acquire::ForceHash=SHA256`). Every other
# package is downloaded and hash-checked by apt as usual. Index lists are never
# cached: a cached list would carry no fresh signature check.
set -euo pipefail

readonly CACHE_KEY_PREFIX=keld-webkitgtk-debs-v1

fail() {
    echo "ci-webkitgtk-apt: $1" >&2
    exit 1
}

usage() {
    echo "usage: $0 {key|install <cache-dir>|test}" >&2
    echo "  key      print the GitHub output line key=<cache key> for KELD_WEBKITGTK_PACKAGES" >&2
    echo "  install  apt-get update, stage verified cached .debs, install, refresh the cache dir" >&2
    echo "  test     run this script's self-tests" >&2
    exit 2
}

sha256_stdin() {
    if command -v sha256sum >/dev/null; then
        sha256sum | cut -d' ' -f1
    else
        shasum -a 256 | cut -d' ' -f1
    fi
}

require_packages() {
    local packages="${KELD_WEBKITGTK_PACKAGES:-}"
    if [[ -z "${packages//[[:space:]]/}" ]]; then
        fail "KELD_WEBKITGTK_PACKAGES is empty; the job must name its exact WebKitGTK package list."
    fi
    printf '%s\n' "$packages"
}

# The key binds the exact package list and the runner image, so a new image or
# list starts a fresh entry instead of reusing packages built for another one.
cache_key() {
    local packages digest
    packages="$(require_packages)"
    if [[ -z "${ImageOS:-}" || -z "${ImageVersion:-}" ]]; then
        fail "ImageOS or ImageVersion is unset; this runs only on a GitHub-hosted runner image, whose version must be part of the cache key."
    fi
    # shellcheck disable=SC2086 # the list is deliberately split into package names
    digest="$(printf '%s\n' $packages | LC_ALL=C sort -u | sha256_stdin)"
    printf 'key=%s-%s-%s-%s\n' "$CACHE_KEY_PREFIX" "$ImageOS" "$ImageVersion" "$digest"
}

apt_archives_dir() {
    local archives
    archives="$(apt-config shell ARCHIVES Dir::Cache::Archives/d | sed -n "s/^ARCHIVES='\(.*\)'$/\1/p")"
    if [[ "$archives" != /* || "$archives" == *"'"* ]]; then
        fail "cannot read apt's Dir::Cache::Archives (got '$archives')."
    fi
    printf '%s\n' "${archives%/}"
}

install_packages() {
    local cache_dir="$1"
    local packages archives plan start
    packages="$(require_packages)"
    archives="$(apt_archives_dir)"
    mkdir -p "$cache_dir"

    start=$SECONDS
    sudo apt-get update
    echo "ci-webkitgtk-apt: apt-get update took $((SECONDS - start)) s"

    # shellcheck disable=SC2086 # the list is deliberately split into package names
    plan="$(apt-get install --print-uris -qq -o Acquire::ForceHash=SHA256 -y --no-install-recommends $packages)" ||
        fail "apt-get could not plan the WebKitGTK install from the refreshed indexes."

    local line uri file size hash expected actual actual_size
    local planned=0 staged=0 rejected=0
    local -a planned_files=()
    while IFS= read -r line; do
        [[ "$line" == "'"* ]] || continue
        planned=$((planned + 1))
        read -r uri file size hash <<<"$line"
        expected="${hash#SHA256:}"
        if [[ "$hash" != SHA256:* || ! "$expected" =~ ^[0-9a-f]{64}$ || ! "$size" =~ ^[0-9]+$ ||
            ! "$file" =~ ^[A-Za-z0-9][A-Za-z0-9.+~%_-]*\.deb$ ]]; then
            rejected=$((rejected + 1))
            continue
        fi
        planned_files+=("$file")
        if [[ ! -f "$cache_dir/$file" || -L "$cache_dir/$file" ]]; then
            continue
        fi
        # Hash the root-owned copy apt will read, not the user-writable source.
        sudo cp -- "$cache_dir/$file" "$archives/$file"
        actual="$(sha256_stdin <"$archives/$file")"
        actual_size="$(wc -c <"$archives/$file" | tr -d ' ')"
        if [[ "$actual" == "$expected" && "$actual_size" == "$size" ]]; then
            staged=$((staged + 1))
        else
            sudo rm -f -- "$archives/$file"
            rejected=$((rejected + 1))
        fi
    done <<<"$plan"
    echo "ci-webkitgtk-apt: planned $planned package files, staged $staged verified from the cache, rejected $rejected"

    start=$SECONDS
    # shellcheck disable=SC2086 # the list is deliberately split into package names
    sudo apt-get install -y --no-install-recommends $packages
    echo "ci-webkitgtk-apt: apt-get install took $((SECONDS - start)) s"

    # Refresh the cache directory with exactly the planned files apt now holds;
    # each was hash-checked by apt on download or by the staging check above.
    if [[ ${#planned_files[@]} -gt 0 ]]; then
        for file in "${planned_files[@]}"; do
            if [[ -f "$archives/$file" ]]; then
                cp -- "$archives/$file" "$cache_dir/$file"
            fi
        done
    fi
}

# ---------------------------------------------------------------- self-test

self_test() {
    local self temp
    self="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/$(basename "${BASH_SOURCE[0]}")"
    temp="$(mktemp -d "${TMPDIR:-/tmp}/keld-webkitgtk-apt.XXXXXX")"
    # shellcheck disable=SC2064 # expand the path now
    trap "rm -rf '$temp'" EXIT

    # The cache dir sits one level deeper than the archive dir, so a planned
    # name that climbs out of one cannot alias the same file in the other.
    local cache="$temp/c/cache"
    mkdir -p "$temp/bin" "$temp/mirror" "$temp/archives" "$cache" "$temp/c/outside"
    # Fake commands; the script under test runs unchanged against them.
    printf '%s\n' '#!/usr/bin/env bash' 'exec "$@"' >"$temp/bin/sudo"
    printf '%s\n' '#!/usr/bin/env bash' "echo \"ARCHIVES='$temp/archives/'\"" >"$temp/bin/apt-config"
    cat >"$temp/bin/apt-get" <<EOF
#!/usr/bin/env bash
set -euo pipefail
case "\$1" in
    update)
        [[ -z "\${KELD_FAKE_UPDATE_FAIL:-}" ]] || exit 100
        echo update >>"$temp/calls"
        ;;
    install)
        if [[ " \$* " == *" --print-uris "* ]]; then
            [[ " \$* " == *" -o Acquire::ForceHash=SHA256 "* ]] || exit 101
            cat "$temp/plan"
            exit 0
        fi
        echo install >>"$temp/calls"
        while read -r uri file size hash; do
            [[ "\$uri" == "'"* ]] || continue
            if [[ ! -f "$temp/archives/\$file" ]]; then
                cp "$temp/mirror/\$file" "$temp/archives/\$file"
                echo "\$file" >>"$temp/downloads"
            fi
            # dpkg's view: the file it unpacks must be the mirror's bytes.
            cmp -s "$temp/archives/\$file" "$temp/mirror/\$file" || { echo "installed tampered \$file" >&2; exit 102; }
        done <"$temp/plan"
        ;;
esac
EOF
    chmod +x "$temp/bin/sudo" "$temp/bin/apt-config" "$temp/bin/apt-get"

    local sha_a sha_b
    printf 'package a\n' >"$temp/mirror/liba_1.0_amd64.deb"
    printf 'package b, longer\n' >"$temp/mirror/libb_2%3a1.0_amd64.deb"
    sha_a="$(sha256_stdin <"$temp/mirror/liba_1.0_amd64.deb")"
    sha_b="$(sha256_stdin <"$temp/mirror/libb_2%3a1.0_amd64.deb")"
    write_plan() {
        printf "'http://mirror/liba_1.0_amd64.deb' liba_1.0_amd64.deb %s SHA256:%s\n" \
            "$(wc -c <"$temp/mirror/liba_1.0_amd64.deb" | tr -d ' ')" "${1:-$sha_a}" >"$temp/plan"
        printf "'http://mirror/libb_2%%253a1.0_amd64.deb' libb_2%%3a1.0_amd64.deb %s SHA256:%s\n" \
            "$(wc -c <"$temp/mirror/libb_2%3a1.0_amd64.deb" | tr -d ' ')" "$sha_b" >>"$temp/plan"
    }
    run_install() {
        rm -f "$temp/archives/"* "$temp/downloads" "$temp/calls"
        PATH="$temp/bin:$PATH" KELD_WEBKITGTK_PACKAGES="liba libb" "$self" install "$cache" >"$temp/out" 2>&1
    }
    expect_downloads() {
        local label="$1" expected="$2" actual=""
        [[ -f "$temp/downloads" ]] && actual="$(sort "$temp/downloads" | paste -sd ' ' -)"
        [[ "$actual" == "$expected" ]] || { cat "$temp/out" >&2; fail "self-test '$label': downloads '$actual', expected '$expected'"; }
        echo "ok: $label"
    }

    # Cache miss: everything downloads, and the cache dir is refreshed with it.
    write_plan
    run_install || { cat "$temp/out" >&2; fail "self-test 'miss' failed"; }
    expect_downloads "a cache miss downloads every planned package" "liba_1.0_amd64.deb libb_2%3a1.0_amd64.deb"
    cmp -s "$cache/liba_1.0_amd64.deb" "$temp/mirror/liba_1.0_amd64.deb" ||
        fail "self-test: the cache dir was not refreshed after a miss"
    grep -q 'staged 0 verified' "$temp/out" || fail "self-test: a miss reported staged packages"

    # Cache hit: verified files are staged, so nothing downloads.
    run_install || { cat "$temp/out" >&2; fail "self-test 'hit' failed"; }
    expect_downloads "a verified cache hit downloads nothing" ""
    grep -q 'staged 2 verified from the cache, rejected 0' "$temp/out" || fail "self-test: a hit did not stage both packages"

    # Negative control: a same-size tampered cache file is rejected and fetched
    # fresh. apt alone would accept it by size and install it.
    printf 'package X\n' >"$cache/liba_1.0_amd64.deb"
    run_install || { cat "$temp/out" >&2; fail "self-test: a tampered cache file failed the install instead of being replaced"; }
    expect_downloads "a same-size tampered cache file is replaced from the mirror" "liba_1.0_amd64.deb"
    grep -q 'staged 1 verified from the cache, rejected 1' "$temp/out" || fail "self-test: the tampered file was not rejected"

    # Negative control: a plan line without SHA256 (MD5 only) is never staged.
    cp "$temp/mirror/liba_1.0_amd64.deb" "$cache/liba_1.0_amd64.deb"
    sed -i.bak 's/SHA256:[0-9a-f]*/MD5Sum:0123456789abcdef0123456789abcdef/' "$temp/plan" && rm -f "$temp/plan.bak"
    run_install || { cat "$temp/out" >&2; fail "self-test 'md5 plan' failed"; }
    expect_downloads "a plan without SHA256 stages nothing from the cache" "liba_1.0_amd64.deb libb_2%3a1.0_amd64.deb"

    # Negative control: a planned name that climbs out of the directories is never
    # staged, even when the cache holds a matching file at that path.
    write_plan
    cp "$temp/mirror/liba_1.0_amd64.deb" "$temp/c/outside/escape.deb"
    printf "'http://mirror/x' ../outside/escape.deb %s SHA256:%s\n" \
        "$(wc -c <"$temp/mirror/liba_1.0_amd64.deb" | tr -d ' ')" "$sha_a" >>"$temp/plan"
    run_install || true
    [[ ! -e "$temp/outside/escape.deb" ]] || fail "self-test: a hostile file name escaped the archive directory"
    grep -q 'rejected 1' "$temp/out" || { cat "$temp/out" >&2; fail "self-test: a hostile file name was not rejected"; }
    echo "ok: a hostile planned file name is rejected"

    # A failed index refresh stops before any install.
    write_plan
    if KELD_FAKE_UPDATE_FAIL=1 run_install; then fail "self-test: a failed apt-get update did not fail the install"; fi
    [[ ! -f "$temp/calls" ]] || ! grep -q install "$temp/calls" || fail "self-test: install ran after a failed update"
    echo "ok: a failed apt-get update stops before install"

    # The cache key binds the image and the exact package set, and fails closed.
    local key1 key2 key3
    key1="$(ImageOS=ubuntu24 ImageVersion=20261005.1 KELD_WEBKITGTK_PACKAGES="liba libb" "$self" key)"
    key2="$(ImageOS=ubuntu24 ImageVersion=20261005.1 KELD_WEBKITGTK_PACKAGES="libb  liba" "$self" key)"
    key3="$(ImageOS=ubuntu24 ImageVersion=20261012.1 KELD_WEBKITGTK_PACKAGES="liba libb" "$self" key)"
    [[ "$key1" == key=keld-webkitgtk-debs-v1-ubuntu24-20261005.1-* && "$key1" == "$key2" ]] ||
        fail "self-test: the key is not the image plus the package set ('$key1', '$key2')"
    [[ "$key1" != "$key3" ]] || fail "self-test: a new image version reused the cache key"
    [[ "$key1" != "$(ImageOS=ubuntu24 ImageVersion=20261005.1 KELD_WEBKITGTK_PACKAGES="liba libb libc" "$self" key)" ]] ||
        fail "self-test: a changed package list reused the cache key"
    if ImageOS=ubuntu24 ImageVersion='' KELD_WEBKITGTK_PACKAGES="liba" "$self" key >/dev/null 2>&1; then
        fail "self-test: a missing image version produced a cache key"
    fi
    if ImageOS=ubuntu24 ImageVersion=1 KELD_WEBKITGTK_PACKAGES=' ' "$self" key >/dev/null 2>&1; then
        fail "self-test: an empty package list produced a cache key"
    fi
    echo "ok: the cache key binds the runner image and the exact package set, and fails closed"
    echo "ci-webkitgtk-apt self-tests ok"
}

case "${1:-}" in
    key)
        [[ "$#" -eq 1 ]] || usage
        cache_key
        ;;
    install)
        [[ "$#" -eq 2 && -n "$2" ]] || usage
        install_packages "$2"
        ;;
    test)
        [[ "$#" -eq 1 ]] || usage
        self_test
        ;;
    *)
        usage
        ;;
esac

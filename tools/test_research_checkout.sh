#!/usr/bin/env bash
# Native Git contract tests for the local-only research helper. Never contact
# GitHub/private research: the only fetch/push/clone endpoints are owned bare Git.
set -euo pipefail
# A broken/missed fixture shim must fail, never fall through to the real
# GitHub research remote. Bare-repo tests are local-file protocol only.
export GIT_ALLOW_PROTOCOL=file
export GIT_TERMINAL_PROMPT=0
export GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_GLOBAL=/dev/null
# A caller's repository selection must not redirect fixture setup Git commands.
unset GIT_DIR GIT_WORK_TREE GIT_COMMON_DIR GIT_INDEX_FILE
unset GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX
ROOT="$(git rev-parse --show-toplevel)"
TOOL="$ROOT/tools/research_checkout.sh"
REAL_GIT="$(command -v git)"
TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/keld-research-contract-XXXXXX")"
trap 'rm -rf -- "$TMP_ROOT"' EXIT
export KELD_REAL_GIT="$REAL_GIT"
export KELD_BARE="$TMP_ROOT/remote.git"
export KELD_GIT_CALLS="$TMP_ROOT/git-calls.log"
HTTPS="https://github.com/0monish/keld-research.git"
SSH="git@github.com:0monish/keld-research.git"
# An omitted shim cannot accidentally contact the real private origin.
if "$REAL_GIT" ls-remote "$HTTPS" > "$TMP_ROOT/no-network.out" 2> "$TMP_ROOT/no-network.err"; then
    echo 'FAIL: research fixture admitted an HTTPS transport' >&2
    exit 1
fi
grep -Eq "transport .https. not allowed|protocol .https. is not allowed" "$TMP_ROOT/no-network.err"
mkdir -p "$TMP_ROOT/bin" "$TMP_ROOT/parent with spaces/docs"
PARENT="$TMP_ROOT/parent with spaces"
DEST="$PARENT/docs/research"

cat > "$TMP_ROOT/bin/git" <<'SHIM'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1-}" == -C && "${3-}" == status && "${KELD_FAIL_STATUS:-0}" == 1 ]]; then
    echo 'simulated git status failure' >&2
    exit 71
fi
if [[ "${1-}" == -C && "${3-}" == add && "${KELD_FAIL_ADD:-0}" == 1 ]]; then
    echo 'simulated git add failure' >&2
    exit 72
fi
if [[ "${1-}" == -C && "${3-}" == commit ]]; then
    printf 'commit invoked\n' >> "$KELD_GIT_CALLS"
fi
if [[ "${1-}" == -C && "${3-}" == pull ]]; then
    [[ "${4-}" == --ff-only && "${5-}" == origin && "${6-}" == main && "$#" == 6 ]] || exit 89
    "$KELD_REAL_GIT" -C "$2" fetch --quiet "$KELD_BARE" refs/heads/main || exit $?
    exec "$KELD_REAL_GIT" -C "$2" merge --ff-only FETCH_HEAD
fi
if [[ "${1-}" == -C && "${3-}" == push ]]; then
    # Exactly one named remote/ref: never force or infer caller-configured destinations.
    [[ "${4-}" == origin && "${5-}" == HEAD:refs/heads/main && "$#" == 5 ]] || exit 88
    printf 'unforced push\n' >> "$KELD_GIT_CALLS"
    exec "$KELD_REAL_GIT" -C "$2" push "$KELD_BARE" HEAD:refs/heads/main
fi
if [[ "${1-}" == clone ]]; then
    case "${2-}" in
        https://github.com/0monish/keld-research.git|git@github.com:0monish/keld-research.git) ;;
        *) echo 'test rejected external clone' >&2; exit 87 ;;
    esac
    if [[ "${KELD_FAIL_HTTPS:-0}" == 1 && "$2" == https:* ]]; then
        echo 'simulated private HTTPS refusal' >&2
        exit 128
    fi
    "$KELD_REAL_GIT" clone "$KELD_BARE" "$3" || exit $?
    "$KELD_REAL_GIT" -C "$3" remote set-url origin "$2"
    exit 0
fi
exec "$KELD_REAL_GIT" "$@"
SHIM
chmod +x "$TMP_ROOT/bin/git"

"$REAL_GIT" init --quiet --initial-branch=main "$PARENT"
"$REAL_GIT" -C "$PARENT" config user.name 'Keld Fixture'
"$REAL_GIT" -C "$PARENT" config user.email 'fixture@example.invalid'
printf 'docs/research/\n' > "$PARENT/.gitignore"
"$REAL_GIT" -C "$PARENT" add .gitignore
"$REAL_GIT" -C "$PARENT" commit -qm parent
parent_head="$("$REAL_GIT" -C "$PARENT" rev-parse HEAD)"
"$REAL_GIT" init --bare --quiet --initial-branch=main "$KELD_BARE"
"$REAL_GIT" clone --quiet "$KELD_BARE" "$TMP_ROOT/seed"
"$REAL_GIT" -C "$TMP_ROOT/seed" config user.name 'Keld Fixture'
"$REAL_GIT" -C "$TMP_ROOT/seed" config user.email 'fixture@example.invalid'
printf 'initial\n' > "$TMP_ROOT/seed/notes.txt"
"$REAL_GIT" -C "$TMP_ROOT/seed" add notes.txt
"$REAL_GIT" -C "$TMP_ROOT/seed" commit -qm initial
"$REAL_GIT" -C "$TMP_ROOT/seed" push --quiet -u origin main
"$REAL_GIT" clone --quiet "$KELD_BARE" "$DEST"
"$REAL_GIT" -C "$DEST" remote set-url origin "$HTTPS"
"$REAL_GIT" -C "$DEST" config user.name 'Keld Fixture'
"$REAL_GIT" -C "$DEST" config user.email 'fixture@example.invalid'

run() {
    if PATH="$TMP_ROOT/bin:$PATH" "$TOOL" "$@" > "$TMP_ROOT/stdout" 2> "$TMP_ROOT/stderr"; then
        status=0
    else
        status=$?
    fi
}
assert_parent_untouched() {
    [[ "$("$REAL_GIT" -C "$PARENT" rev-parse HEAD)" == "$parent_head" ]]
    [[ -z "$("$REAL_GIT" -C "$PARENT" status --porcelain)" ]]
}

# Clean push preserves the branch and does not stage the parent.
run push "$PARENT" 'ignored on clean push'
[[ "$status" == 0 ]]
grep -Fq 'research-push: no local changes' "$TMP_ROOT/stdout"
assert_parent_untouched

# The same allowlist admits only the three published URL spellings (and the
# optional .git suffix); test every spelling with actual local-only Git push.
for admitted in "$HTTPS" "$SSH" 'ssh://git@github.com/0monish/keld-research.git' 'https://github.com/0monish/keld-research'; do
    "$REAL_GIT" -C "$DEST" remote set-url origin "$admitted"
    run push "$PARENT" 'still clean'
    [[ "$status" == 0 ]]
    grep -Fq 'research-push: pushed' "$TMP_ROOT/stdout"
done
"$REAL_GIT" -C "$DEST" remote set-url origin "$HTTPS"

# Only the three exact URL forms are eligible; a near-match must not commit.
for denied in 'https://github.com/0monish/keld-research/' 'http://github.com/0monish/keld-research.git' 'https://github.com/0MONISH/keld-research.git' 'https://github.com/0monish/keld-research-other.git'; do
    "$REAL_GIT" -C "$DEST" remote set-url origin "$denied"
    before_calls="$(wc -l < "$KELD_GIT_CALLS" | tr -d ' ')"
    before_remote="$("$REAL_GIT" --git-dir="$KELD_BARE" rev-parse refs/heads/main)"
    before_local="$("$REAL_GIT" -C "$DEST" rev-parse HEAD)"
    run push "$PARENT" blocked
    [[ "$status" == 1 ]]
    grep -Fq 'refusing' "$TMP_ROOT/stderr"
    [[ "$(wc -l < "$KELD_GIT_CALLS" | tr -d ' ')" == "$before_calls" ]]
    [[ "$("$REAL_GIT" --git-dir="$KELD_BARE" rev-parse refs/heads/main)" == "$before_remote" ]]
    [[ "$("$REAL_GIT" -C "$DEST" rev-parse HEAD)" == "$before_local" ]]
done
"$REAL_GIT" -C "$DEST" remote set-url origin "$HTTPS"

# Reject credential-bearing Git remotes without ever printing the untrusted
# address. Only synthetic fixture secrets are used; external transport is disabled.
synthetic_secret='KELD_SYNTHETIC_SECRET_NEVER_LOG'
credential_origin="https://test-user:${synthetic_secret}@github.com/0monish/keld-research.git"
before_remote="$("$REAL_GIT" --git-dir="$KELD_BARE" rev-parse refs/heads/main)"
before_local="$("$REAL_GIT" -C "$DEST" rev-parse HEAD)"
before_calls="$(wc -l < "$KELD_GIT_CALLS" | tr -d ' ')"
"$REAL_GIT" -C "$DEST" remote set-url origin "$credential_origin"
run push "$PARENT" credentialed-origin-refusal
[[ "$status" == 1 ]]
grep -Fq 'refusing' "$TMP_ROOT/stderr"
if grep -Fq "$synthetic_secret" "$TMP_ROOT/stdout" "$TMP_ROOT/stderr"; then
    echo 'FAIL: rejected origin printed credentials' >&2
    exit 1
fi
"$REAL_GIT" -C "$DEST" remote set-url origin "$HTTPS"
"$REAL_GIT" -C "$DEST" config remote.origin.pushurl "$credential_origin"
run push "$PARENT" credentialed-pushurl-refusal
[[ "$status" == 1 ]]
grep -Fq 'origin push URL' "$TMP_ROOT/stderr"
if grep -Fq "$synthetic_secret" "$TMP_ROOT/stdout" "$TMP_ROOT/stderr"; then
    echo 'FAIL: rejected origin push URL printed credentials' >&2
    exit 1
fi
"$REAL_GIT" -C "$DEST" config --unset-all remote.origin.pushurl
[[ "$("$REAL_GIT" --git-dir="$KELD_BARE" rev-parse refs/heads/main)" == "$before_remote" ]]
[[ "$("$REAL_GIT" -C "$DEST" rev-parse HEAD)" == "$before_local" ]]
[[ "$(wc -l < "$KELD_GIT_CALLS" | tr -d ' ')" == "$before_calls" ]]

# Failed local-only clone rewrites must not publish arbitrary real Git stderr.
clone_parent="$TMP_ROOT/clone rewrite parent"
mkdir -p "$clone_parent"
synthetic_rewrite_marker='KELD_TEST_ONLY_REWRITE_MARKER_X71'
rewritten="file://$TMP_ROOT/$synthetic_rewrite_marker/"
clone_env=(
    GIT_CONFIG_COUNT=2
    "GIT_CONFIG_KEY_0=url.${rewritten}.insteadOf"
    "GIT_CONFIG_VALUE_0=$HTTPS"
    "GIT_CONFIG_KEY_1=url.${rewritten}.insteadOf"
    "GIT_CONFIG_VALUE_1=$SSH"
)
if env "${clone_env[@]}" "$REAL_GIT" clone "$HTTPS" "$TMP_ROOT/rewritten-probe" >"$TMP_ROOT/probe.out" 2>"$TMP_ROOT/probe.err"; then
    echo 'FAIL: intentionally missing rewritten clone unexpectedly succeeded' >&2; exit 1
fi
grep -Fq "$synthetic_rewrite_marker" "$TMP_ROOT/probe.err" || {
    echo 'FAIL: synthetic Git rewrite fixture did not reach real Git error path' >&2; exit 1;
}
if ! env "${clone_env[@]}" "$TOOL" sync "$clone_parent" >"$TMP_ROOT/clone.stdout" 2>"$TMP_ROOT/clone.stderr"; then
    echo 'FAIL: failed clone did not retain hook-safe successful skip' >&2; exit 1
fi
grep -Fq 'cannot clone keld-research via HTTPS or SSH' "$TMP_ROOT/clone.stderr"
if grep -Fq "$synthetic_rewrite_marker" "$TMP_ROOT/clone.stdout" "$TMP_ROOT/clone.stderr"; then
    echo 'FAIL: rewritten origin marker leaked through clone diagnostics' >&2; exit 1
fi
[[ ! -e "$clone_parent/docs/research/.git" ]]

# A failed status read must never be mistaken for an empty change list.
KELD_FAIL_STATUS=1; export KELD_FAIL_STATUS
before_calls="$(wc -l < "$KELD_GIT_CALLS" | tr -d ' ')"
run push "$PARENT" status-refusal
[[ "$status" == 1 ]]
grep -Fq 'git status failed' "$TMP_ROOT/stderr"
[[ "$(wc -l < "$KELD_GIT_CALLS" | tr -d ' ')" == "$before_calls" ]]
unset KELD_FAIL_STATUS

# A failed add must stop before git commit or git push (not reuse staged state).
printf 'pending add\n' > "$DEST/add-blocked.txt"
KELD_FAIL_ADD=1; export KELD_FAIL_ADD
before_calls="$(wc -l < "$KELD_GIT_CALLS" | tr -d ' ')"
run push "$PARENT" add-refusal
[[ "$status" == 1 ]]
grep -Fq 'git add failed' "$TMP_ROOT/stderr"
[[ "$(wc -l < "$KELD_GIT_CALLS" | tr -d ' ')" == "$before_calls" ]]
unset KELD_FAIL_ADD
rm -- "$DEST/add-blocked.txt"

# Fetch URL alone is not sufficient: an origin pushurl may point elsewhere.
"$REAL_GIT" init --bare --quiet --initial-branch=main "$TMP_ROOT/rogue.git"
"$REAL_GIT" -C "$DEST" config remote.origin.pushurl "$TMP_ROOT/rogue.git"
run push "$PARENT" pushurl-refusal
[[ "$status" == 1 ]]
grep -Fq 'origin push URL' "$TMP_ROOT/stderr"
if "$REAL_GIT" --git-dir="$TMP_ROOT/rogue.git" rev-parse --verify refs/heads/main >/dev/null 2>&1; then
    echo 'FAIL: untrusted push URL received the research branch' >&2
    exit 1
fi
"$REAL_GIT" -C "$DEST" config --unset-all remote.origin.pushurl
"$REAL_GIT" -C "$DEST" config --add remote.origin.pushurl "$HTTPS"
"$REAL_GIT" -C "$DEST" config --add remote.origin.pushurl "$SSH"
run push "$PARENT" ambiguous-multiple-pushurls
[[ "$status" == 1 ]]
grep -Fq 'origin push URL' "$TMP_ROOT/stderr"
"$REAL_GIT" -C "$DEST" config --unset-all remote.origin.pushurl

# Caller pushDefault and pushRemote must not override explicit origin.
"$REAL_GIT" -C "$DEST" remote add rogue "$TMP_ROOT/rogue.git"
"$REAL_GIT" -C "$DEST" config remote.pushDefault rogue
"$REAL_GIT" -C "$DEST" config branch.main.pushRemote rogue
run push "$PARENT" explicit-origin
[[ "$status" == 0 ]]
if "$REAL_GIT" --git-dir="$TMP_ROOT/rogue.git" rev-parse --verify refs/heads/main >/dev/null 2>&1; then
    echo 'FAIL: pushDefault/branch.pushRemote won over explicit origin' >&2
    exit 1
fi
"$REAL_GIT" -C "$DEST" config --unset remote.pushDefault
"$REAL_GIT" -C "$DEST" config --unset branch.main.pushRemote

# A detached HEAD cannot authorize a branch push or an implicit pull.
"$REAL_GIT" -C "$DEST" -c advice.detachedHead=false checkout --quiet --detach HEAD
run sync "$PARENT"
[[ "$status" == 0 ]]
grep -Fq 'HEAD is detached' "$TMP_ROOT/stderr"
run push "$PARENT" detached-refusal
[[ "$status" == 1 ]]
grep -Fq 'HEAD is detached' "$TMP_ROOT/stderr"
"$REAL_GIT" -C "$DEST" switch --quiet main

# Dirty nested changes commit with exact whitespace-bearing message.
printf 'nested only\n' > "$DEST/new note.txt"
run push "$PARENT" 'research: message with spaces'
[[ "$status" == 0 ]]
[[ "$("$REAL_GIT" --git-dir="$KELD_BARE" log -1 --format=%s)" == 'research: message with spaces' ]]
assert_parent_untouched

# A truly late competing write occurs from another real repo in pre-push,
# AFTER the helper's local commit but before the remote update. No force retry.
printf 'pending after clean push\n' > "$DEST/pending.txt"
"$REAL_GIT" clone --quiet "$KELD_BARE" "$TMP_ROOT/other writer"
"$REAL_GIT" -C "$TMP_ROOT/other writer" config user.name 'Other Fixture'
"$REAL_GIT" -C "$TMP_ROOT/other writer" config user.email 'other@example.invalid'
printf 'competing\n' > "$TMP_ROOT/other writer/competitor.txt"
"$REAL_GIT" -C "$TMP_ROOT/other writer" add competitor.txt
"$REAL_GIT" -C "$TMP_ROOT/other writer" commit -qm competing
other_head="$("$REAL_GIT" -C "$TMP_ROOT/other writer" rev-parse HEAD)"
cat > "$DEST/.git/hooks/pre-push" <<'HOOK'
#!/usr/bin/env bash
set -eu
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE
"$KELD_REAL_GIT" -C "$KELD_ACTOR" push --quiet "$KELD_BARE" HEAD:refs/heads/main
HOOK
chmod +x "$DEST/.git/hooks/pre-push"
"$REAL_GIT" -C "$DEST" config core.hooksPath "$DEST/.git/hooks"
export KELD_ACTOR="$TMP_ROOT/other writer"
late_push_calls="$(grep -c '^unforced push$' "$KELD_GIT_CALLS" || true)"
run push "$PARENT" 'local candidate must not force remote'
if [[ "$status" != 1 ]]; then
    echo "UNEXPECTED_LATE_PUSH_EXIT=$status" >&2
    cat "$TMP_ROOT/stdout" "$TMP_ROOT/stderr" >&2
    "$REAL_GIT" -C "$DEST" config --get core.hooksPath >&2
    "$REAL_GIT" -C "$DEST" status --short --branch >&2
    echo "REMOTE: $("$REAL_GIT" --git-dir="$KELD_BARE" rev-parse refs/heads/main)" >&2
    echo "ACTOR: $other_head" >&2
    exit 1
fi
grep -Fq 'git push failed' "$TMP_ROOT/stderr"
[[ "$("$REAL_GIT" --git-dir="$KELD_BARE" rev-parse refs/heads/main)" == "$other_head" ]]
[[ "$("$REAL_GIT" -C "$DEST" log -1 --format=%s)" == 'local candidate must not force remote' ]]
# Exactly one ordinary unforced attempt, even when the remote moves late.
[[ "$(grep -c '^unforced push$' "$KELD_GIT_CALLS")" == "$((late_push_calls + 1))" ]]
assert_parent_untouched

# Divergent pull remains a hook-safe warning; remote and local stay distinct.
rm -f "$DEST/.git/hooks/pre-push"
local_head="$("$REAL_GIT" -C "$DEST" rev-parse HEAD)"
run sync "$PARENT"
[[ "$status" == 0 ]]
grep -Fq 'git pull --ff-only failed' "$TMP_ROOT/stderr"
[[ "$("$REAL_GIT" -C "$DEST" rev-parse HEAD)" == "$local_head" ]]

# A separate fresh clone tests genuine fast-forward without relying on a
# divergent former branch or accidentally validating a reset of the product.
rm -rf -- "$DEST"
"$REAL_GIT" clone --quiet "$KELD_BARE" "$DEST"
"$REAL_GIT" -C "$DEST" remote set-url origin "$HTTPS"
"$REAL_GIT" -C "$DEST" config user.name 'Keld Fixture'
"$REAL_GIT" -C "$DEST" config user.email 'fixture@example.invalid'
printf 'later change\n' > "$TMP_ROOT/other writer/later.txt"
"$REAL_GIT" -C "$TMP_ROOT/other writer" add later.txt
"$REAL_GIT" -C "$TMP_ROOT/other writer" commit -qm later
"$REAL_GIT" -C "$TMP_ROOT/other writer" push --quiet "$KELD_BARE" HEAD:refs/heads/main
run sync "$PARENT"
[[ "$status" == 0 ]] || { cat "$TMP_ROOT/stdout" "$TMP_ROOT/stderr" >&2; exit 1; }
if [[ "$("$REAL_GIT" -C "$DEST" rev-parse HEAD)" != "$("$REAL_GIT" --git-dir="$KELD_BARE" rev-parse refs/heads/main)" ]]; then
    echo 'unexpected fast-forward outcome:' >&2
    cat "$TMP_ROOT/stdout" "$TMP_ROOT/stderr" >&2
    "$REAL_GIT" -C "$DEST" status --short --branch >&2
    "$REAL_GIT" -C "$DEST" remote -v >&2
    exit 1
fi
grep -Fq 'research-sync: updated' "$TMP_ROOT/stdout"
assert_parent_untouched

# No origin or a foreign origin must refuse before a commit or push.
printf 'pending\n' > "$DEST/pending.txt"
"$REAL_GIT" -C "$DEST" remote set-url origin https://example.invalid/fake.git
old_local="$("$REAL_GIT" -C "$DEST" rev-parse HEAD)"
run sync "$PARENT"
[[ "$status" == 0 ]]
grep -Fq 'docs/research origin is not' "$TMP_ROOT/stderr"
run push "$PARENT" reject
[[ "$status" == 1 ]]
grep -Fq 'refusing' "$TMP_ROOT/stderr"
grep -Fq 'not 0monish/keld-research' "$TMP_ROOT/stderr"
[[ "$("$REAL_GIT" -C "$DEST" rev-parse HEAD)" == "$old_local" ]]
"$REAL_GIT" -C "$DEST" remote remove origin
run push "$PARENT" reject
[[ "$status" == 1 ]]
grep -Fq 'has no origin remote' "$TMP_ROOT/stderr"
"$REAL_GIT" -C "$DEST" remote add origin "$HTTPS"
"$REAL_GIT" -C "$DEST" config branch.main.remote origin
"$REAL_GIT" -C "$DEST" config branch.main.merge refs/heads/main

# An existing non-repository leaf is never overwritten or staged.
rm -rf -- "$DEST"
printf 'keep me\n' > "$DEST"
run sync "$PARENT"
[[ "$status" == 0 ]]
grep -Fq 'exists but is not a git checkout' "$TMP_ROOT/stderr"
run push "$PARENT" ignored
[[ "$status" == 0 ]]
grep -Fq 'not a nested git checkout' "$TMP_ROOT/stderr"
[[ "$(cat "$DEST")" == 'keep me' ]]
rm -- "$DEST"

# Clone fallback must not contact GitHub: intercepted HTTPS refusal, then an
# actual local bare Git clone presented under the authorized SSH origin.
KELD_FAIL_HTTPS=1; export KELD_FAIL_HTTPS
run sync "$PARENT"
[[ "$status" == 0 ]]
grep -Fq "research-sync: cloned $SSH" "$TMP_ROOT/stdout"
[[ "$("$REAL_GIT" -C "$DEST" remote get-url origin)" == "$SSH" ]]
assert_parent_untouched

echo 'research checkout: real local Git scenarios OK (clean, dirty, wrong/missing origin, late non-FF veto, ff-only, missing leaf, fallback clone)'

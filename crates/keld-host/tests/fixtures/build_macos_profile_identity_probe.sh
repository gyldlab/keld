#!/bin/bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "fixture requires macOS" >&2
    exit 2
fi

source_file="$(cd "$(dirname "$0")" && pwd)/macos_profile_identity_probe.swift"
fixture_root="${TMPDIR:?work-run must supply a managed TMPDIR}/keld-profile-identity-fixtures"
if [[ -e "$fixture_root" ]]; then
    echo "refusing to replace existing fixture root: $fixture_root" >&2
    exit 2
fi

identity_hashes="$(security find-identity -v -p codesigning \
    | sed -nE '/CSSMERR_/!s/^[[:space:]]*[0-9]+\) ([0-9A-F]{40}).*$/\1/p' \
    )"
if [[ -z "$identity_hashes" ]]; then
    echo "no valid code-signing identity with a private key is available" >&2
    exit 2
fi
identity_hash="$(printf '%s\n' "$identity_hashes" | head -n 1)"

mkdir -p "$fixture_root"
xcrun swiftc -O -framework Foundation -framework Security "$source_file" \
    -o "$fixture_root/ProfileIdentityProbe"

make_app() {
    local app_name="$1"
    local bundle_id="$2"
    local signer_hash="$3"
    local app="$fixture_root/$app_name.app"
    mkdir -p "$app/Contents/MacOS"
    cp "$fixture_root/ProfileIdentityProbe" "$app/Contents/MacOS/ProfileIdentityProbe"
    /usr/libexec/PlistBuddy -c "Add :CFBundleIdentifier string $bundle_id" "$app/Contents/Info.plist"
    /usr/libexec/PlistBuddy -c 'Add :CFBundleExecutable string ProfileIdentityProbe' "$app/Contents/Info.plist"
    /usr/libexec/PlistBuddy -c "Add :CFBundleName string $app_name" "$app/Contents/Info.plist"
    /usr/libexec/PlistBuddy -c 'Add :CFBundlePackageType string APPL' "$app/Contents/Info.plist"
    /usr/libexec/PlistBuddy -c 'Add :LSUIElement bool true' "$app/Contents/Info.plist"
    codesign --force --sign "$signer_hash" --identifier "$bundle_id" --timestamp=none "$app"
    codesign --verify --deep --strict --verbose=2 "$app"
    "$app/Contents/MacOS/ProfileIdentityProbe" > "$fixture_root/$app_name.json"
}

make_app 'ProfileIdentityA' 'dev.keld.fixture.profile.a' "$identity_hash"
make_app 'ProfileIdentityB' 'dev.keld.fixture.profile.b' "$identity_hash"
probe="$fixture_root/ProfileIdentityA.app/Contents/MacOS/ProfileIdentityProbe"
"$probe" --webkit-seed-probe > "$fixture_root/webkit-seed.jsonl"
store_identifier="$(python3 -c 'import json,sys; print(json.loads(open(sys.argv[1]).read().splitlines()[1])["store_identifier"])' \
    "$fixture_root/webkit-seed.jsonl")"
"$probe" --webkit-purge-probe "$store_identifier" > "$fixture_root/webkit-purge.jsonl"
"$probe" --webkit-ephemeral-probe > "$fixture_root/webkit-ephemeral.jsonl"

primary_team="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["team_identifier"])' \
    "$fixture_root/ProfileIdentityA.json")"
other_publisher="false"
candidate_number=0
while IFS= read -r candidate_hash; do
    [[ "$candidate_hash" == "$identity_hash" ]] && continue
    candidate_number=$((candidate_number + 1))
    candidate_name="ProfileIdentityPublisherCandidate$candidate_number"
    candidate_id='dev.keld.fixture.profile.publisher-control'
    if make_app "$candidate_name" "$candidate_id" "$candidate_hash" \
        > "$fixture_root/$candidate_name.build.stdout" 2> "$fixture_root/$candidate_name.build.stderr"; then
        candidate_report="$fixture_root/$candidate_name.json"
        candidate_team="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["team_identifier"])' \
            "$candidate_report")"
        if [[ "$candidate_team" != "$primary_team" ]]; then
            cp "$candidate_report" "$fixture_root/ProfileIdentityOtherPublisher.json"
            other_publisher='true'
            break
        fi
    else
        echo "a valid signing identity failed its signed-app probe" >&2
        exit 1
    fi
done <<< "$identity_hashes"

invalid_app="$fixture_root/ProfileIdentityInvalid.app"
cp -R "$fixture_root/ProfileIdentityA.app" "$invalid_app"
/usr/libexec/PlistBuddy -c 'Set :CFBundleName TamperedAfterSigning' "$invalid_app/Contents/Info.plist"
if codesign --verify --deep --strict "$invalid_app" > "$fixture_root/invalid-codesign.stdout" 2> "$fixture_root/invalid-codesign.stderr"; then
    echo "tampered app unexpectedly passed codesign verification" >&2
    exit 1
fi
set +e
"$invalid_app/Contents/MacOS/ProfileIdentityProbe" \
    > "$fixture_root/invalid-probe.stdout" 2> "$fixture_root/invalid-probe.stderr"
invalid_status=$?
set -e
if [[ "$invalid_status" -eq 0 ]] || /usr/bin/grep -Eq 'team_identifier|signing_identifier' \
    "$fixture_root/invalid-probe.stdout" "$fixture_root/invalid-probe.stderr"; then
    echo "tampered running app was accepted or disclosed signing identity" >&2
    exit 1
fi

python3 - "$fixture_root" "$other_publisher" <<'PY'
import hashlib
import json
from pathlib import Path
import sys

root = Path(sys.argv[1])
has_other_publisher = sys.argv[2] == "true"
first = json.loads((root / "ProfileIdentityA.json").read_text())
second = json.loads((root / "ProfileIdentityB.json").read_text())
seed_lines = (root / "webkit-seed.jsonl").read_text().splitlines()
purge_lines = (root / "webkit-purge.jsonl").read_text().splitlines()
ephemeral_lines = (root / "webkit-ephemeral.jsonl").read_text().splitlines()
assert len(seed_lines) == len(purge_lines) == len(ephemeral_lines) == 2
webkit_identity = json.loads(seed_lines[0])
seed = json.loads(seed_lines[1])
purge_identity = json.loads(purge_lines[0])
purge = json.loads(purge_lines[1])
ephemeral_identity = json.loads(ephemeral_lines[0])
ephemeral = json.loads(ephemeral_lines[1])
assert first["status"] == second["status"] == "passed"
assert webkit_identity["status"] == purge_identity["status"] == ephemeral_identity["status"] == "passed"
assert seed["status"] == purge["status"] == ephemeral["status"] == "passed"
assert seed["persistent_store_is_persistent"] and seed["persistent_configuration_same_store"]
assert purge["store_identifier"] == seed["store_identifier"]
assert purge["removal_completion_succeeded"] and purge["store_absent_after_completion"]
assert not ephemeral["store_is_persistent"] and ephemeral["store_identifier"] is None
assert ephemeral["configuration_same_store"]
assert first["signature_validated_before_identity_read"]
assert second["signature_validated_before_identity_read"]
assert first["team_identifier"] == second["team_identifier"]
assert first["signing_identifier"] != second["signing_identifier"]

result = {
    "status": "passed",
    "same_publisher_a_b": first["team_identifier"],
    "app_a": first,
    "app_b": second,
    "webkit_seed": seed,
    "webkit_purge_after_creator_exit": purge,
    "webkit_ephemeral": ephemeral,
    "app_a_executable_sha256": hashlib.sha256(
        (root / "ProfileIdentityA.app/Contents/MacOS/ProfileIdentityProbe").read_bytes()
    ).hexdigest(),
    "app_b_executable_sha256": hashlib.sha256(
        (root / "ProfileIdentityB.app/Contents/MacOS/ProfileIdentityProbe").read_bytes()
    ).hexdigest(),
    "other_publisher_control_available": has_other_publisher,
    "tampered_app_rejected": True,
    "tampered_app_identity_not_read": True,
}
if has_other_publisher:
    other = json.loads((root / "ProfileIdentityOtherPublisher.json").read_text())
    assert other["signature_validated_before_identity_read"]
    assert other["team_identifier"] != first["team_identifier"]
    assert other["signing_identifier"] == "dev.keld.fixture.profile.publisher-control"
    result["other_publisher_control"] = other
elif (root / "ProfileIdentityOtherPublisher.json").exists():
    raise SystemExit("same-publisher candidate unexpectedly recorded as other publisher")
print(json.dumps(result, sort_keys=True))
PY

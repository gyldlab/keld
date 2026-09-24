#!/bin/bash
set -euo pipefail

repo_root=$(cd "$(dirname "$0")/../../../.." && pwd -P)
cd "$repo_root"

command -v /usr/bin/expect >/dev/null 2>&1 || {
  echo "KELD-KEL135-TEST: /usr/bin/expect is required to enter the generated password without argv or log exposure." >&2
  exit 2
}
command -v /usr/bin/openssl >/dev/null 2>&1 || {
  echo "KELD-KEL135-TEST: /usr/bin/openssl is required to generate a strong temporary password." >&2
  exit 2
}

/bin/mkdir -p "$repo_root/target"
cargo test -p keld-host --test no_flag_macos --features profile-test-hooks \
  --target aarch64-apple-darwin \
  kel135_macos_second_user_cannot_read_same_signed_profile_state \
  --no-run

account="keld135t3_$(/usr/bin/openssl rand -hex 4)"
user_home="/Users/$account"
test_log="$repo_root/target/kel135-macos-second-user-test-${account}.log"
account_creation_attempted=0
account_created=0
profile_test_started=0
secret_dir=""

account_matches_test_fixture() {
  local actual_name actual_home
  actual_name=$(/usr/bin/dscl . -read "/Users/$account" RealName \
    | /usr/bin/tr '\n' ' ' \
    | /usr/bin/sed -E 's/[[:space:]]+/ /g; s/^ //; s/ $//')
  actual_home=$(/usr/bin/dscl . -read "/Users/$account" NFSHomeDirectory \
    | /usr/bin/sed 's/^NFSHomeDirectory: //')
  [ "$actual_name" = "RealName: KELD-135 temporary second-user test" ] \
    && [ "$actual_home" = "$user_home" ]
}

if /usr/bin/dscl . -read "/Users/$account" UniqueID >/dev/null 2>&1 \
  || [ -e "$user_home" ] || [ -L "$user_home" ]; then
  echo "KELD-KEL135-TEST: generated login or home already exists; refusing to modify it." >&2
  exit 2
fi
secret_dir=$(/usr/bin/mktemp -d /private/tmp/kel135-account.XXXXXX)
/bin/chmod 700 "$secret_dir"
/usr/bin/openssl rand -hex 32 > "$secret_dir/password"
/bin/chmod 600 "$secret_dir/password"

cleanup() {
  status=$?
  trap - EXIT
  safe_to_delete_account=1
  if [ "$profile_test_started" -eq 1 ] \
    && { [ ! -f "$test_log" ] \
      || ! /usr/bin/grep -qF \
        "KELD_KEL135_MACOS_SECOND_USER_SAFE_TO_DELETE_ACCOUNT" "$test_log"; }; then
    safe_to_delete_account=0
  fi
  if [ "$account_creation_attempted" -eq 1 ] && [ "$account_created" -eq 0 ] \
    && /usr/bin/dscl . -read "/Users/$account" UniqueID >/dev/null 2>&1 \
    && account_matches_test_fixture; then
    account_created=1
  fi
  if [ "$account_created" -eq 1 ] && [ "$safe_to_delete_account" -eq 1 ]; then
    if /usr/bin/dscl . -read "/Users/$account" UniqueID >/dev/null 2>&1 \
      && ! /usr/bin/sudo -n /usr/sbin/sysadminctl -deleteUser "$account"; then
      echo "KELD-KEL135-TEST: sysadminctl could not delete the owned temporary account $account." >&2
      status=1
    fi
    if /usr/bin/dscl . -read "/Users/$account" UniqueID >/dev/null 2>&1; then
      echo "KELD-KEL135-TEST: temporary account record remains: $account" >&2
      status=1
    elif [ -e "$user_home" ] \
      && ! /usr/bin/sudo -n /bin/rm -rf -- "$user_home"; then
      echo "KELD-KEL135-TEST: could not remove the owned temporary home $user_home." >&2
      status=1
    fi
  elif [ "$account_created" -eq 1 ]; then
    echo "KELD-KEL135-TEST: profile purge was not verified; retaining the temporary account and home so profile metadata stays recoverable." >&2
    status=1
  elif [ "$account_creation_attempted" -eq 1 ] && [ -e "$user_home" ]; then
    echo "KELD-KEL135-TEST: unowned home appeared during failed account creation; preserving it for inspection: $user_home" >&2
    status=1
  fi
  if [ "$account_created" -eq 0 ] \
    && /usr/bin/dscl . -read "/Users/$account" UniqueID >/dev/null 2>&1; then
    echo "KELD-KEL135-TEST: unverified account record remains; preserving it for inspection: $account" >&2
    status=1
  fi
  if [ "$account_created" -eq 1 ] && [ "$safe_to_delete_account" -eq 1 ] \
    && [ -e "$user_home" ]; then
    echo "KELD-KEL135-TEST: temporary account home remains: $user_home" >&2
    status=1
  fi
  if [ -n "$secret_dir" ]; then
    /bin/rm -rf -- "$secret_dir"
  fi
  if [ -f "$test_log" ]; then
    /bin/cat "$test_log"
  fi
  if [ "$status" -eq 0 ] && [ "$account_created" -eq 1 ]; then
    echo "KELD-KEL135-TEST: temporary account record and home are removed."
  elif [ "$account_created" -eq 1 ] && [ "$safe_to_delete_account" -eq 0 ]; then
    echo "KELD-KEL135-TEST: account retained because profile purge was not proven complete."
  fi
  exit "$status"
}
trap cleanup EXIT

# The owner authenticates at this local prompt. Credentials are not saved or sent.
/usr/bin/sudo -v

cat > "$secret_dir/create-user.expect" <<'EOF'
set timeout 90
set f [open $env(KELD135_PASSWORD_FILE) r]
set password [string trim [read $f]]
close $f
log_user 1
spawn /usr/sbin/sysadminctl -addUser $env(KELD135_ACCOUNT) -fullName "KELD-135 temporary second-user test" -password -
expect {
  -re "(?i)password.*:" { send -- "$password\r"; exp_continue }
  eof {}
  timeout { puts stderr "KELD-KEL135-TEST: sysadminctl timed out creating the temporary account."; exit 124 }
}
set result [wait]
if {[lindex $result 2] != 0} {
  exit 1
}
exit [lindex $result 3]
EOF

account_creation_attempted=1
if ! /usr/bin/sudo -n /usr/bin/env \
  "KELD135_ACCOUNT=$account" \
  "KELD135_PASSWORD_FILE=$secret_dir/password" \
  /usr/bin/expect "$secret_dir/create-user.expect"; then
  echo "KELD-KEL135-TEST: temporary account creation failed; cleanup will remove only the verified account created here." >&2
  exit 1
fi
/bin/rm -f "$secret_dir/password"
if ! account_matches_test_fixture; then
  echo "KELD-KEL135-TEST: newly created account does not match the expected identity and home." >&2
  exit 1
fi
account_created=1

groups=$(/usr/bin/id -Gn "$account")
if printf '%s\n' "$groups" | /usr/bin/grep -Eq '(^|[[:space:]])admin($|[[:space:]])'; then
  echo "KELD-KEL135-TEST: generated account unexpectedly has admin membership." >&2
  exit 1
fi

: > "$test_log"
profile_test_started=1
if KELD_KEL135_SECOND_USER="$account" \
  cargo test -p keld-host --test no_flag_macos --features profile-test-hooks \
    --target aarch64-apple-darwin \
    kel135_macos_second_user_cannot_read_same_signed_profile_state \
    -- --exact --ignored --nocapture > "$test_log" 2>&1; then
  :
else
  status=$?
  /bin/cat "$test_log"
  exit "$status"
fi

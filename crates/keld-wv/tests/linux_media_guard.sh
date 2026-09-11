#!/usr/bin/env bash
set -euo pipefail

interposer=${1:?usage: linux_media_guard.sh <interposer.so> [probe-binary]}
probe_binary=${2:-target/debug/examples/linux_media_guard}
interposer=$(readlink -f -- "$interposer")
probe_binary=$(readlink -f -- "$probe_binary")
interposer_sha256=$(sha256sum -- "$interposer" | awk '{print $1}')
probe_sha256=$(sha256sum -- "$probe_binary" | awk '{print $1}')
keep_evidence=0
if [ -n "${KELD_MEDIA_EVIDENCE_DIRECTORY:-}" ]; then
  probe_root=$(readlink -m -- "$KELD_MEDIA_EVIDENCE_DIRECTORY")
  if [ -e "$probe_root" ]; then
    echo "media evidence directory must be new: $probe_root" >&2
    exit 1
  fi
  mkdir -m 700 -- "$probe_root"
  keep_evidence=1
else
  probe_root=$(mktemp -d "${RUNNER_TEMP:-/tmp}/keld-media-guard.XXXXXX")
fi
active_runner_pid=""
active_release_file=""
active_synthetic_pid=""
active_monitor_pid=""
camera_media_id=""
microphone_media_id=""
declare -a evidence_keys=()
declare -a evidence_media_ids=()
declare -a evidence_primer_counts=()
declare -a evidence_trace_hashes=()
declare -a evidence_output_hashes=()
declare -a evidence_identity_hashes=()

cleanup() {
  local status=$?
  set +e
  if [ -n "$active_release_file" ]; then
    : >"$active_release_file"
  fi
  if [ -n "$active_runner_pid" ] && kill -0 "$active_runner_pid" 2>/dev/null; then
    kill "$active_runner_pid" 2>/dev/null || true
    wait "$active_runner_pid" 2>/dev/null || true
  fi
  if [ -n "$active_synthetic_pid" ] && kill -0 "$active_synthetic_pid" 2>/dev/null; then
    kill "$active_synthetic_pid" 2>/dev/null || true
    wait "$active_synthetic_pid" 2>/dev/null || true
  fi
  if [ -n "$active_monitor_pid" ] && kill -0 "$active_monitor_pid" 2>/dev/null; then
    kill "$active_monitor_pid" 2>/dev/null || true
    wait "$active_monitor_pid" 2>/dev/null || true
  fi
  xprop -root -remove KELD_MEDIA_MONITOR >/dev/null 2>&1 || true
  xprop -root -remove KELD_MEDIA_MONITOR_FENCE >/dev/null 2>&1 || true
  if [ "$keep_evidence" -ne 1 ]; then
    rm -r -- "$probe_root"
  fi
  exit "$status"
}
trap cleanup EXIT

client_windows() {
  xprop -root _NET_CLIENT_LIST 2>/dev/null \
    | sed -n 's/^.*# //p' \
    | tr ',' '\n' \
    | tr -d ' \t' \
    | sed -n '/^0x[0-9a-fA-F][0-9a-fA-F]*$/p' \
    | sort -u
}

run_probe() {
  local kind=$1
  local expected=$2
  local callback=$3
  local case_name=${4:-${kind}-${expected}}
  local primer_count
  case "$kind" in
    camera) primer_count=1 ;;
    microphone) primer_count=2 ;;
    *) echo "unknown media kind: $kind" >&2; exit 1 ;;
  esac
  local nonce="${case_name}-${BASHPID}-${RANDOM}"
  local trace_file="$probe_root/${case_name}.trace"
  local output_file="$probe_root/${case_name}.out"
  local ready_file="$probe_root/${case_name}.ready"
  local release_file="$probe_root/${case_name}.release"
  local page_ready_file="$probe_root/${case_name}.page-ready"
  local request_release_file="$probe_root/${case_name}.request-release"
  local identity_file="$probe_root/${case_name}.identity"
  local event_file="$probe_root/${case_name}.xevents"
  local -a baseline_clients=()
  local policy_trace_file=$trace_file
  if [ "${KELD_MEDIA_DROP_POLICY_RECEIPT:-0}" = 1 ]; then
    policy_trace_file="$probe_root/${case_name}.discarded-policy"
  fi
  local -a environment=(
    "LD_PRELOAD=$interposer"
    "KELD_MEDIA_TRACE=$trace_file"
    "KELD_MEDIA_POLICY_TRACE=$policy_trace_file"
    "KELD_MEDIA_NONCE=$nonce"
    "KELD_MEDIA_READY=$ready_file"
    "KELD_MEDIA_RELEASE=$release_file"
    "KELD_MEDIA_PAGE_READY=$page_ready_file"
    "KELD_MEDIA_REQUEST_RELEASE=$request_release_file"
    "KELD_MEDIA_IDENTITY_RECEIPT=$identity_file"
  )
  if [ "$expected" = allowed ]; then
    environment+=("KELD_MEDIA_FORCE_ALLOW=1")
  fi

  timeout --signal=TERM --kill-after=5s 30s \
    env "${environment[@]}" "$probe_binary" "$kind" "$expected" "$nonce" "$primer_count" \
    >"$output_file" 2>&1 &
  local runner_pid=$!
  active_runner_pid=$runner_pid
  active_release_file=$release_file
  local deadline=$((SECONDS + 30))
  while [ ! -f "$page_ready_file" ]; do
    if ! kill -0 "$runner_pid" 2>/dev/null; then
      wait "$runner_pid" || true
      sed -n '1,120p' "$output_file" >&2
      echo "media probe exited before pre-request readiness" >&2
      exit 1
    fi
    if [ "$SECONDS" -ge "$deadline" ]; then
      kill "$runner_pid" 2>/dev/null || true
      wait "$runner_pid" || true
      echo "media page did not reach pre-request readiness" >&2
      exit 1
    fi
  done

  xprop -root -remove KELD_MEDIA_MONITOR >/dev/null 2>&1 || true
  xprop -root -remove KELD_MEDIA_MONITOR_FENCE >/dev/null 2>&1 || true
  stdbuf -oL xev -1 -root -event substructure -event property \
    >"$event_file" 2>&1 &
  local monitor_pid=$!
  active_monitor_pid=$monitor_pid
  local monitor_deadline=$((SECONDS + 10))
  while ! grep -q 'KELD_MEDIA_MONITOR' "$event_file"; do
    if ! kill -0 "$monitor_pid" 2>/dev/null; then
      echo "X event monitor exited before its readiness round-trip" >&2
      exit 1
    fi
    if [ "$SECONDS" -ge "$monitor_deadline" ]; then
      echo "X event monitor missed its readiness round-trip" >&2
      exit 1
    fi
    xprop -root -f KELD_MEDIA_MONITOR 8s -set KELD_MEDIA_MONITOR \
      "${nonce}-${RANDOM}" >/dev/null
  done
  mapfile -t baseline_clients < <(client_windows)
  local event_barrier_line
  event_barrier_line=$(wc -l <"$event_file")
  if [ "${KELD_MEDIA_KILL_MONITOR:-0}" = 1 ]; then
    kill "$monitor_pid"
    set +e
    wait "$monitor_pid"
    local killed_monitor_status=$?
    set -e
    if [ "$killed_monitor_status" -ne 143 ]; then
      echo "monitor-kill negative control exited $killed_monitor_status, expected SIGTERM status 143" >&2
      exit 1
    fi
    active_monitor_pid=""
  fi

  if [ "${KELD_MEDIA_SYNTHETIC_PROMPT:-0}" = 1 ]; then
    xmessage -title "Camera Permission" -buttons Allow,Deny "Allow camera access?" \
      >"$probe_root/${case_name}.prompt.log" 2>&1 &
    active_synthetic_pid=$!
    local prompt_deadline=$((SECONDS + 10))
    local prompt_ready=0
    while [ "$SECONDS" -lt "$prompt_deadline" ]; do
      local prompt_window
      while IFS= read -r prompt_window; do
        if xprop -id "$prompt_window" _NET_WM_NAME WM_NAME 2>/dev/null \
          | grep -Fq "Camera Permission"; then
          prompt_ready=1
          break
        fi
      done < <(client_windows)
      [ "$prompt_ready" -eq 0 ] || break
    done
    if [ "$prompt_ready" -ne 1 ]; then
      echo "synthetic external prompt did not become a managed top-level client" >&2
      exit 1
    fi
    kill "$active_synthetic_pid" 2>/dev/null || true
    wait "$active_synthetic_pid" 2>/dev/null || true
    active_synthetic_pid=""
  fi
  : >"$request_release_file"

  while [ ! -f "$ready_file" ]; do
    if ! kill -0 "$runner_pid" 2>/dev/null; then
      wait "$runner_pid" || true
      sed -n '1,120p' "$output_file" >&2
      echo "media probe exited before window-census readiness" >&2
      exit 1
    fi
    if [ "$SECONDS" -ge "$deadline" ]; then
      kill "$runner_pid" 2>/dev/null || true
      wait "$runner_pid" || true
      echo "media probe did not reach window-census readiness" >&2
      exit 1
    fi
  done

  local expected_exe
  expected_exe=$(readlink -f -- "$probe_binary")
  local expected_exe_ere
  expected_exe_ere=$(printf '%s' "$expected_exe" | sed 's/[][\\.^$*+?(){}|]/\\&/g')
  local expected_caller_ere
  expected_caller_ere=$(basename -- "$expected_exe" | sed 's/[][\\.^$*+?(){}|]/\\&/g')
  local setup_pattern="^setup nonce=${nonce} mock_capture_devices=true webview=0x[0-9a-f]+ exe=${expected_exe_ere} pid=[0-9]+ tid=[0-9]+ uri=http://127\\.0\\.0\\.1:[0-9]+/${nonce}/$"
  if [ "$(grep -Ec "$setup_pattern" "$trace_file")" -ne 1 ]; then
    echo "expected one mock-capture setup record for $kind/$expected" >&2
    exit 1
  fi
  local setup_line
  setup_line=$(grep -E "$setup_pattern" "$trace_file")
  local setup_line_number
  setup_line_number=$(grep -nE "$setup_pattern" "$trace_file" | cut -d: -f1)
  local setup_pid
  local setup_tid
  setup_pid=$(printf '%s\n' "$setup_line" | sed -E 's/.* pid=([0-9]+) tid=.*/\1/')
  setup_tid=$(printf '%s\n' "$setup_line" | sed -E 's/.* tid=([0-9]+) uri=.*/\1/')
  if [ "$setup_pid" != "$setup_tid" ]; then
    echo "mock setup left the process main thread: pid=$setup_pid tid=$setup_tid" >&2
    exit 1
  fi
  local setup_webview
  setup_webview=$(printf '%s\n' "$setup_line" | sed -E 's/.* webview=(0x[0-9a-f]+) exe=.*/\1/')

  local registration_line
  local registration_pattern="^registration nonce=${nonce} signal=permission-request handler=[1-9][0-9]* webview=${setup_webview} caller=.*${expected_caller_ere} exe=${expected_exe_ere} pid=[0-9]+ tid=[0-9]+$"
  if [ "$(grep -Ec "$registration_pattern" "$trace_file")" -ne 1 ]; then
    echo "expected one successful permission-request registration for $kind/$expected" >&2
    exit 1
  fi
  registration_line=$(grep -E "$registration_pattern" "$trace_file")
  local registration_line_number
  registration_line_number=$(grep -nE "$registration_pattern" "$trace_file" | cut -d: -f1)
  local registration_pid
  local registration_tid
  registration_pid=$(printf '%s\n' "$registration_line" | sed -E 's/.* pid=([0-9]+) tid=.*/\1/')
  registration_tid=$(printf '%s\n' "$registration_line" | sed -E 's/.* tid=([0-9]+).*/\1/')
  if [ "$registration_pid" != "$setup_pid" ] || [ "$registration_tid" != "$setup_pid" ]; then
    echo "permission handler was not registered on the setup process main thread: setup=$setup_pid registration=$registration_pid tid=$registration_tid" >&2
    exit 1
  fi
  if [ "$registration_line_number" -ge "$setup_line_number" ]; then
    echo "permission handler registration did not precede initial media navigation: registration=$registration_line_number setup=$setup_line_number" >&2
    exit 1
  fi

  local identity_line
  identity_line=$(tr -d '\r\n' <"$identity_file")
  local identity_pattern="^KELD_MEDIA_IDENTITY nonce=${nonce} kind=${kind} primer_count=${primer_count} primer_first=[1-9][0-9]* primer_last=[1-9][0-9]* stale_count=${primer_count} stale_code=KELD-WV-007 media_id=[1-9][0-9]* fresh=true$"
  if ! printf '%s\n' "$identity_line" | grep -Eq "$identity_pattern"; then
    echo "media lifecycle identity receipt was missing or malformed: $identity_line" >&2
    exit 1
  fi
  local primer_first
  local primer_last
  local media_id
  primer_first=$(printf '%s\n' "$identity_line" | sed -E 's/.* primer_first=([0-9]+) .*/\1/')
  primer_last=$(printf '%s\n' "$identity_line" | sed -E 's/.* primer_last=([0-9]+) .*/\1/')
  media_id=$(printf '%s\n' "$identity_line" | sed -E 's/.* media_id=([0-9]+) .*/\1/')
  if [ "$primer_first" -gt "$primer_last" ] || [ "$media_id" -le "$primer_last" ]; then
    echo "fresh media id did not follow the destroyed primer range: first=$primer_first last=$primer_last media=$media_id" >&2
    exit 1
  fi
  local stale_receipt_pattern="^KELD_MEDIA_STALE nonce=${nonce} kind=${kind} ordinal=[1-9][0-9]* primer_id=[1-9][0-9]* code=KELD-WV-007$"
  local -a stale_receipts=()
  mapfile -t stale_receipts < <(grep -E "$stale_receipt_pattern" "$output_file")
  if [ "${#stale_receipts[@]}" -ne "$primer_count" ]; then
    echo "expected $primer_count exact stale-navigation receipts for $kind/$expected" >&2
    exit 1
  fi
  local previous_primer_id=0
  local receipt_index
  for receipt_index in "${!stale_receipts[@]}"; do
    local expected_ordinal=$((receipt_index + 1))
    local actual_ordinal
    local actual_primer_id
    actual_ordinal=$(printf '%s\n' "${stale_receipts[$receipt_index]}" | sed -E 's/.* ordinal=([0-9]+) .*/\1/')
    actual_primer_id=$(printf '%s\n' "${stale_receipts[$receipt_index]}" | sed -E 's/.* primer_id=([0-9]+) .*/\1/')
    if [ "$actual_ordinal" -ne "$expected_ordinal" ] || [ "$actual_primer_id" -le "$previous_primer_id" ]; then
      echo "stale-navigation receipts were not ordered with increasing host ids: ordinal=$actual_ordinal id=$actual_primer_id" >&2
      exit 1
    fi
    previous_primer_id=$actual_primer_id
  done
  local first_stale_id
  local last_stale_id
  first_stale_id=$(printf '%s\n' "${stale_receipts[0]}" | sed -E 's/.* primer_id=([0-9]+) .*/\1/')
  last_stale_id=$(printf '%s\n' "${stale_receipts[$((primer_count - 1))]}" | sed -E 's/.* primer_id=([0-9]+) .*/\1/')
  if [ "$first_stale_id" -ne "$primer_first" ] || [ "$last_stale_id" -ne "$primer_last" ]; then
    echo "stale-navigation ids did not bind the lifecycle identity range" >&2
    exit 1
  fi
  local registration_count
  registration_count=$(grep -Ec "^registration nonce=${nonce} signal=permission-request handler=[1-9][0-9]* webview=0x[0-9a-f]+ " "$trace_file")
  local last_registration_line_number
  last_registration_line_number=$(grep -nE "^registration nonce=${nonce} signal=permission-request handler=[1-9][0-9]* webview=0x[0-9a-f]+ " "$trace_file" | tail -n 1 | cut -d: -f1)
  if [ "$registration_count" -ne "$((primer_count + 1))" ] || [ "$registration_line_number" -ne "$last_registration_line_number" ]; then
    echo "expected one permission handler registration per primer and the fresh media view last: count=$registration_count media_line=$registration_line_number last_line=$last_registration_line_number primers=$primer_count" >&2
    exit 1
  fi

  local callback_line
  local callback_caller_pattern=".*${expected_caller_ere}"
  if [ "$callback" = adapter_bypass ]; then
    callback_caller_pattern=linux_media_interpose
  fi
  local callback_pattern="^callback nonce=${nonce} kind=${kind} action=${callback} caller=${callback_caller_pattern} exe=${expected_exe_ere} pid=[0-9]+ tid=[0-9]+$"
  if [ "$(grep -Ec "$callback_pattern" "$trace_file")" -ne 1 ]; then
    echo "expected one $callback callback record for $kind/$expected" >&2
    exit 1
  fi
  callback_line=$(grep -E "$callback_pattern" "$trace_file")
  local callback_pid
  local callback_tid
  callback_pid=$(printf '%s\n' "$callback_line" | sed -E 's/.* pid=([0-9]+) tid=.*/\1/')
  callback_tid=$(printf '%s\n' "$callback_line" | sed -E 's/.* tid=([0-9]+).*/\1/')
  if [ "$callback_pid" != "$callback_tid" ] || [ "$callback_pid" != "$setup_pid" ]; then
    echo "media callback is not the setup process main thread: setup=$setup_pid callback=$callback_pid tid=$callback_tid" >&2
    exit 1
  fi

  local capability="web.${kind}"
  local policy_pattern="^policy nonce=${nonce} capability=${capability} principal=webview:${media_id}:0 manifest_fnv1a64=e117311975d9f419 decision=KELD-GUARD006 response=deny pid=${setup_pid}$"

  local -a current_clients=()
  mapfile -t current_clients < <(client_windows)
  local -a new_clients=()
  local candidate
  local baseline
  for candidate in "${current_clients[@]}"; do
    local existed=0
    for baseline in "${baseline_clients[@]}"; do
      if [ "$candidate" = "$baseline" ]; then
        existed=1
        break
      fi
    done
    if [ "$existed" -eq 0 ]; then
      new_clients+=("$candidate")
    fi
  done
  local visible_window_count=${#new_clients[@]}
  local unexpected_windows=""
  for candidate in "${new_clients[@]}"; do
    local window_properties
    window_properties=$(xprop -id "$candidate" _NET_WM_NAME WM_NAME 2>/dev/null || true)
    unexpected_windows+=$'\n'
    unexpected_windows+="${candidate}: ${window_properties//$'\n'/; }"
  done
  local monitor_error=""
  if ! kill -0 "$monitor_pid" 2>/dev/null; then
    monitor_error="X event monitor died before the post-result fence"
  else
    local fence_deadline=$((SECONDS + 10))
    while ! tail -n "+$((event_barrier_line + 1))" "$event_file" \
      | grep -q 'KELD_MEDIA_MONITOR_FENCE'; do
      if ! kill -0 "$monitor_pid" 2>/dev/null; then
        monitor_error="X event monitor died before acknowledging the final fence"
        break
      fi
      if [ "$SECONDS" -ge "$fence_deadline" ]; then
        monitor_error="X event monitor missed the final event-drain fence"
        break
      fi
      xprop -root -f KELD_MEDIA_MONITOR_FENCE 8s \
        -set KELD_MEDIA_MONITOR_FENCE "${nonce}-${RANDOM}" >/dev/null
    done
  fi
  if [ -z "$monitor_error" ]; then
    kill "$monitor_pid"
    set +e
    wait "$monitor_pid"
    local monitor_status=$?
    set -e
    if [ "$monitor_status" -ne 143 ]; then
      monitor_error="X event monitor exited $monitor_status instead of SIGTERM status 143"
    fi
  fi
  active_monitor_pid=""
  xprop -root -remove KELD_MEDIA_MONITOR >/dev/null 2>&1 || true
  xprop -root -remove KELD_MEDIA_MONITOR_FENCE >/dev/null 2>&1 || true
  local map_event_count
  map_event_count=$(tail -n "+$((event_barrier_line + 1))" "$event_file" \
    | grep -c '^MapNotify event' || true)
  : >"$release_file"
  if ! wait "$runner_pid"; then
    sed -n '1,120p' "$output_file" >&2
    echo "media probe failed after window census" >&2
    exit 1
  fi
  active_runner_pid=""
  active_release_file=""
  if [ -n "$active_synthetic_pid" ]; then
    kill "$active_synthetic_pid" 2>/dev/null || true
    wait "$active_synthetic_pid" 2>/dev/null || true
    active_synthetic_pid=""
  fi
  if [ -n "$monitor_error" ]; then
    echo "$monitor_error" >&2
    exit 1
  fi
  if [ "$visible_window_count" -ne 0 ]; then
    echo "permission request added unexpected top-level clients for $kind/$expected: total=$visible_window_count details=${unexpected_windows:-none}" >&2
    exit 1
  fi
  if [ "$map_event_count" -ne 0 ]; then
    echo "permission interval mapped $map_event_count transient top-level window(s) for $kind/$expected" >&2
    exit 1
  fi
  if [ "$expected" = denied ]; then
    grep -Eq "^KELD_MEDIA_RESULT nonce=${nonce} kind=${kind} secure_context=true outcome=(NotAllowedError|SecurityError) track_kind=none track_count=0 live_before_stop=false ended_after_stop=false$" "$output_file"
    if grep -q 'action=force_allow' "$trace_file"; then
      echo "deny run unexpectedly reached the force-allow control" >&2
      exit 1
    fi
  else
    local requested_track_kind
    requested_track_kind=$(if [ "$kind" = camera ]; then printf video; else printf audio; fi)
    grep -Eq "^KELD_MEDIA_RESULT nonce=${nonce} kind=${kind} secure_context=true outcome=resolved track_kind=${requested_track_kind} track_count=[1-9][0-9]* live_before_stop=true ended_after_stop=true$" "$output_file"
  fi
  if [ "$(grep -Ec "$policy_pattern" "$trace_file")" -ne 1 ]; then
    echo "expected one keld-guard policy receipt for $kind/$expected" >&2
    exit 1
  fi

  if [ "$case_name" = "${kind}-${expected}" ]; then
    evidence_keys+=("${kind}/${expected}")
    evidence_media_ids+=("$media_id")
    evidence_primer_counts+=("$primer_count")
    evidence_trace_hashes+=("$(sha256sum -- "$trace_file" | awk '{print $1}')")
    evidence_output_hashes+=("$(sha256sum -- "$output_file" | awk '{print $1}')")
    evidence_identity_hashes+=("$(sha256sum -- "$identity_file" | awk '{print $1}')")
    if [ "$kind" = camera ]; then
      if [ -n "$camera_media_id" ] && [ "$camera_media_id" != "$media_id" ]; then
        echo "camera rows did not reproduce the same host-returned media id: $camera_media_id vs $media_id" >&2
        exit 1
      fi
      camera_media_id=$media_id
    else
      if [ -n "$microphone_media_id" ] && [ "$microphone_media_id" != "$media_id" ]; then
        echo "microphone rows did not reproduce the same host-returned media id: $microphone_media_id vs $media_id" >&2
        exit 1
      fi
      microphone_media_id=$media_id
    fi
  fi

  printf 'media_guard kind=%s expected=%s callback=%s pid=%s tid=%s\n' \
    "$kind" "$expected" "$callback" "$callback_pid" "$callback_tid"
}

run_probe camera denied deny
run_probe microphone denied deny
run_probe camera allowed force_allow
run_probe microphone allowed force_allow

if [ -z "$camera_media_id" ] || [ -z "$microphone_media_id" ] || [ "$camera_media_id" = "$microphone_media_id" ]; then
  echo "camera and microphone did not use distinct nonconstant host-returned ids: camera=$camera_media_id microphone=$microphone_media_id" >&2
  exit 1
fi
if grep -Eq "^policy .* capability=web\.microphone principal=webview:${camera_media_id}:0 " "$probe_root/microphone-denied.trace"; then
  echo "hard-coded camera principal unexpectedly satisfied the microphone row" >&2
  exit 1
fi
echo "media_guard negative_control=hardcoded_principal rejected camera_id=$camera_media_id microphone_id=$microphone_media_id"

if (KELD_MEDIA_DISCONNECT_ADAPTER=1 run_probe camera denied adapter_bypass disconnected-adapter); then
  echo "disconnected adapter unexpectedly passed on platform denial alone" >&2
  exit 1
fi
if ! grep -Eq '^KELD_MEDIA_RESULT .* kind=camera secure_context=true outcome=(NotAllowedError|SecurityError) track_kind=none track_count=0 live_before_stop=false ended_after_stop=false$' "$probe_root/disconnected-adapter.out" ||
  ! grep -Eq '^callback .* kind=camera action=adapter_bypass caller=linux_media_interpose ' "$probe_root/disconnected-adapter.trace" ||
  grep -q '^policy ' "$probe_root/disconnected-adapter.trace"; then
  echo "disconnected-adapter control did not preserve platform denial while bypassing Keld policy" >&2
  exit 1
fi
disconnected_pid=$(sed -n -E 's/^setup .* pid=([0-9]+) tid=.*/\1/p' "$probe_root/disconnected-adapter.trace")
if [ -z "$disconnected_pid" ] || kill -0 "$disconnected_pid" 2>/dev/null; then
  echo "disconnected-adapter failure left the probe process alive" >&2
  exit 1
fi
echo "media_guard negative_control=disconnected_adapter rejected_platform_deny_without_policy"

if (KELD_MEDIA_SYNTHETIC_PROMPT=1 run_probe camera denied deny synthetic-prompt); then
  echo "synthetic media prompt unexpectedly passed the no-prompt census" >&2
  exit 1
fi
echo "media_guard negative_control=synthetic_prompt rejected"

if (KELD_MEDIA_DROP_POLICY_RECEIPT=1 run_probe camera denied deny missing-policy); then
  echo "missing-policy cleanup negative control unexpectedly passed" >&2
  exit 1
fi
missing_policy_pid=$(sed -n -E 's/^setup .* pid=([0-9]+) tid=.*/\1/p' \
  "$probe_root/missing-policy.trace")
if [ -z "$missing_policy_pid" ] || kill -0 "$missing_policy_pid" 2>/dev/null; then
  echo "missing-policy failure left the probe process alive" >&2
  exit 1
fi
echo "media_guard negative_control=missing_policy rejected_and_reaped"

if (KELD_MEDIA_KILL_MONITOR=1 run_probe camera denied deny killed-monitor); then
  echo "killed-monitor negative control unexpectedly passed" >&2
  exit 1
fi
killed_monitor_pid=$(sed -n -E 's/^setup .* pid=([0-9]+) tid=.*/\1/p' \
  "$probe_root/killed-monitor.trace")
if [ -z "$killed_monitor_pid" ] || kill -0 "$killed_monitor_pid" 2>/dev/null; then
  echo "killed-monitor failure left the probe process alive" >&2
  exit 1
fi
echo "media_guard negative_control=killed_monitor rejected_and_reaped"

if [ "${#evidence_keys[@]}" -ne 4 ] ||
  [ "${evidence_keys[0]}" != camera/denied ] ||
  [ "${evidence_keys[1]}" != microphone/denied ] ||
  [ "${evidence_keys[2]}" != camera/allowed ] ||
  [ "${evidence_keys[3]}" != microphone/allowed ]; then
  echo "media evidence rows were not the exact four-row matrix" >&2
  exit 1
fi
if [ "$(sha256sum -- "$probe_binary" | awk '{print $1}')" != "$probe_sha256" ] ||
  [ "$(sha256sum -- "$interposer" | awk '{print $1}')" != "$interposer_sha256" ]; then
  echo "media probe or interposer changed during acceptance" >&2
  exit 1
fi
cp -- "$probe_binary" "$probe_root/linux_media_guard"
cp -- "$interposer" "$probe_root/linux_media_interpose.so"
(cd "$probe_root" && find . -type f ! -name result.json ! -name sha256sums.txt -print0 |
  sort -z | xargs -0 sha256sum --) >"$probe_root/sha256sums.txt"
manifest_sha256=$(sha256sum -- "$probe_root/sha256sums.txt" | awk '{print $1}')
head_sha=${KELD_MEDIA_HEAD_SHA:-}
if [ -z "$head_sha" ]; then
  head_sha=$(git rev-parse HEAD)
fi
if ! [[ "$head_sha" =~ ^[0-9a-f]{40}$ ]]; then
  echo "media evidence head SHA was missing or malformed: $head_sha" >&2
  exit 1
fi
system=$(uname -srm)
device=$(hostname)
{
  printf '{\n'
  printf '  "schema": "keld.linux-media-fixture/v1",\n'
  printf '  "head_sha": "%s",\n' "$head_sha"
  printf '  "system": "%s",\n' "$system"
  printf '  "device": "%s",\n' "$device"
  printf '  "probe_sha256": "%s",\n' "$probe_sha256"
  printf '  "interposer_sha256": "%s",\n' "$interposer_sha256"
  printf '  "manifest_sha256": "%s",\n' "$manifest_sha256"
  printf '  "camera_media_id": %s,\n' "$camera_media_id"
  printf '  "microphone_media_id": %s,\n' "$microphone_media_id"
  printf '  "rows": [\n'
  for index in 0 1 2 3; do
    comma=,
    if [ "$index" -eq 3 ]; then comma=; fi
    printf '    {"key":"%s","primer_count":%s,"media_id":%s,"stale_code":"KELD-WV-007","registration_before_navigation":true,"track_lifecycle":true,"trace_sha256":"%s","output_sha256":"%s","identity_sha256":"%s"}%s\n' \
      "${evidence_keys[$index]}" "${evidence_primer_counts[$index]}" "${evidence_media_ids[$index]}" \
      "${evidence_trace_hashes[$index]}" "${evidence_output_hashes[$index]}" \
      "${evidence_identity_hashes[$index]}" "$comma"
  done
  printf '  ],\n'
  printf '  "controls": ["hardcoded-principal", "disconnected-adapter", "synthetic-prompt", "missing-policy", "killed-monitor"]\n'
  printf '}\n'
} >"$probe_root/result.json"
result_sha256=$(sha256sum -- "$probe_root/result.json" | awk '{print $1}')
echo "KELD_LINUX_MEDIA_RESULT rows=4 controls=5 result_sha256=$result_sha256 evidence=$probe_root"

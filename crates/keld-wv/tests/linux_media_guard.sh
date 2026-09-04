#!/usr/bin/env bash
set -euo pipefail

interposer=${1:?usage: linux_media_guard.sh <interposer.so> [probe-binary]}
probe_binary=${2:-target/debug/examples/linux_media_guard}
probe_root=$(mktemp -d "${RUNNER_TEMP:-/tmp}/keld-media-guard.XXXXXX")
active_runner_pid=""
active_release_file=""
active_synthetic_pid=""
active_monitor_pid=""

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
  rm -r -- "$probe_root"
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
    env "${environment[@]}" "$probe_binary" "$kind" "$expected" "$nonce" \
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
  local registration_pid
  local registration_tid
  registration_pid=$(printf '%s\n' "$registration_line" | sed -E 's/.* pid=([0-9]+) tid=.*/\1/')
  registration_tid=$(printf '%s\n' "$registration_line" | sed -E 's/.* tid=([0-9]+).*/\1/')
  if [ "$registration_pid" != "$setup_pid" ] || [ "$registration_tid" != "$setup_pid" ]; then
    echo "permission handler was not registered on the setup process main thread: setup=$setup_pid registration=$registration_pid tid=$registration_tid" >&2
    exit 1
  fi

  local callback_line
  local callback_pattern="^callback nonce=${nonce} kind=${kind} action=${callback} caller=.*${expected_caller_ere} exe=${expected_exe_ere} pid=[0-9]+ tid=[0-9]+$"
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

  local media_id
  media_id=$(tr -d '[:space:]' <"$identity_file")
  if ! [[ "$media_id" =~ ^[0-9]+$ ]] || [ "$media_id" -le 1 ]; then
    echo "media request was not bound to the independently returned non-first webview id: $media_id" >&2
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
  if [ "$(grep -Ec "$policy_pattern" "$trace_file")" -ne 1 ]; then
    echo "expected one keld-guard policy receipt for $kind/$expected" >&2
    exit 1
  fi

  if [ "$expected" = denied ]; then
    grep -Eq "^KELD_MEDIA_RESULT nonce=${nonce} kind=${kind} secure_context=true outcome=(NotAllowedError|SecurityError)$" "$output_file"
    if grep -q 'action=force_allow' "$trace_file"; then
      echo "deny run unexpectedly reached the force-allow control" >&2
      exit 1
    fi
  else
    grep -q "^KELD_MEDIA_RESULT nonce=${nonce} kind=${kind} secure_context=true outcome=resolved$" "$output_file"
  fi

  printf 'media_guard kind=%s expected=%s callback=%s pid=%s tid=%s\n' \
    "$kind" "$expected" "$callback" "$callback_pid" "$callback_tid"
}

run_probe camera denied deny
run_probe microphone denied deny
run_probe camera allowed force_allow
run_probe microphone allowed force_allow

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

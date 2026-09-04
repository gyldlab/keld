#!/usr/bin/env bash
set -euo pipefail

media_interposer=$1
media_probe=$2
keld_host=$3

window_manager_pid=""
hello_pid=""
cleanup_probe_pid=""
title_confirmed=0
pid_bound=0
close_confirmed=0
# A bare `trap '... || true' EXIT` clobbers $? with the trap's own
# last command status, so a script that hit `exit 1` would report
# success to the CI step. Capture the real exit code first, run
# cleanup, then re-exit with the captured code explicitly.
process_alive() {
  local process_pid=$1
  local process_state
  if ! process_state=$(ps -o stat= -p "$process_pid" 2>/dev/null \
    | tr -d '[:space:]'); then
    return 1
  fi
  case "$process_state" in
    ''|Z*) return 1 ;;
    *) return 0 ;;
  esac
}
terminate_child() {
  local child_pid=$1
  [ -n "$child_pid" ] || return
  if process_alive "$child_pid"; then
    kill "$child_pid" 2>/dev/null || true
    for _ in $(seq 1 20); do
      process_alive "$child_pid" || break
      sleep 0.1
    done
  fi
  if process_alive "$child_pid"; then
    kill -KILL "$child_pid" 2>/dev/null || true
    for _ in $(seq 1 20); do
      process_alive "$child_pid" || break
      sleep 0.1
    done
  fi
  if ! process_alive "$child_pid"; then
    wait "$child_pid" 2>/dev/null || true
  fi
}
cleanup() {
  ec=$?
  set +e
  terminate_child "$cleanup_probe_pid"
  terminate_child "$hello_pid"
  terminate_child "$window_manager_pid"
  exit "$ec"
}
trap cleanup EXIT

sh -c 'kill -STOP $$' &
cleanup_probe_pid=$!
probe_stopped=0
for _ in $(seq 1 20); do
  case "$(ps -o stat= -p "$cleanup_probe_pid" 2>/dev/null)" in
    T*) probe_stopped=1; break ;;
  esac
  sleep 0.05
done
if [ "$probe_stopped" -ne 1 ]; then
  echo "::error::cleanup negative-control child never stopped"
  exit 1
fi
terminate_child "$cleanup_probe_pid"
if process_alive "$cleanup_probe_pid"; then
  echo "::error::cleanup did not reap a stopped child within its bound"
  exit 1
fi
cleanup_probe_pid=""

if ! xdpyinfo -display "$DISPLAY" >/dev/null 2>&1; then
  echo "::error::xvfb-run display is unreachable: $DISPLAY"
  exit 1
fi

fluxbox -display "$DISPLAY" >"$RUNNER_TEMP/fluxbox.log" 2>&1 &
window_manager_pid=$!
window_manager_ready=0
for _ in $(seq 1 50); do
  if ! kill -0 "$window_manager_pid" 2>/dev/null; then
    echo "::error::Fluxbox exited before owning the X11 display"
    cat "$RUNNER_TEMP/fluxbox.log"
    break
  fi
  if root_check=$(xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null); then
    supporting_window=$(printf '%s\n' "$root_check" | awk '/window id/ { print $NF }')
    if [ -n "$supporting_window" ] && \
      child_check=$(xprop -id "$supporting_window" _NET_SUPPORTING_WM_CHECK 2>/dev/null) && \
      wm_name=$(xprop -id "$supporting_window" _NET_WM_NAME 2>/dev/null); then
      child_window=$(printf '%s\n' "$child_check" | awk '/window id/ { print $NF }')
      if [ "$child_window" = "$supporting_window" ] && \
        printf '%s\n' "$wm_name" | grep -q '= "Fluxbox"$'; then
        window_manager_ready=1
        break
      fi
    fi
  fi
  sleep 0.2
done
if [ "$window_manager_ready" -ne 1 ]; then
  echo "::error::Fluxbox never advertised its EWMH control window"
  exit 1
fi

crates/keld-wv/tests/linux_media_guard.sh "$media_interposer" "$media_probe"

"$keld_host" --hello --title CI-Linux-Smoke &
hello_pid=$!

window_id=""
for _ in $(seq 1 60); do
  if ! kill -0 "$hello_pid" 2>/dev/null; then
    echo "::error::keld-host --hello exited before a window was found"
    break
  fi
  if matches=$(xdotool search --all --onlyvisible --pid "$hello_pid" \
    --name '^CI-Linux-Smoke$' 2>/dev/null); then
    if [ "$(printf '%s\n' "$matches" | wc -l)" -ne 1 ]; then
      echo "::error::expected one exact Linux smoke window, got: $matches"
      exit 1
    fi
    window_id=$matches
    break
  else
    search_status=$?
    if [ "$search_status" -ne 1 ]; then
      echo "::error::xdotool window search failed with $search_status"
      exit 1
    fi
  fi
  sleep 0.5
done

if [ -z "$window_id" ]; then
  echo "::error::keld-host --hello never produced a titled window under Xvfb"
  exit 1
fi
if [ "$(xdotool getwindowname "$window_id")" != "CI-Linux-Smoke" ]; then
  echo "::error::Linux smoke window title is not exact"
  exit 1
fi
window_pid=$(xdotool getwindowpid "$window_id")
if [ "$window_pid" != "$hello_pid" ]; then
  echo "::error::Linux smoke window belongs to PID $window_pid, expected $hello_pid"
  exit 1
fi
title_confirmed=1
pid_bound=1

xdotool windowsize "$window_id" 800 600
resized=0
width=unknown
height=unknown
for _ in $(seq 1 50); do
  geometry=$(xdotool getwindowgeometry --shell "$window_id")
  width=$(printf '%s\n' "$geometry" | awk -F= '$1 == "WIDTH" { print $2 }')
  height=$(printf '%s\n' "$geometry" | awk -F= '$1 == "HEIGHT" { print $2 }')
  if [ "$width" = 800 ] && [ "$height" = 600 ]; then
    resized=1
    break
  fi
  sleep 0.1
done
if [ "$resized" -ne 1 ]; then
  echo "::error::resize requested 800x600, observed ${width}x${height}"
  exit 1
fi

xdotool windowminimize "$window_id"
minimized=0
for _ in $(seq 1 50); do
  if ! window_state=$(xprop -id "$window_id" WM_STATE 2>&1); then
    echo "::error::cannot read minimized WM_STATE: $window_state"
    exit 1
  fi
  if printf '%s\n' "$window_state" | grep -q 'window state: Iconic'; then
    minimized=1
    break
  fi
  sleep 0.1
done
if [ "$minimized" -ne 1 ]; then
  echo "::error::Linux smoke window did not become minimized"
  exit 1
fi

xdotool windowactivate "$window_id"
restored=0
for _ in $(seq 1 50); do
  if ! window_state=$(xprop -id "$window_id" WM_STATE 2>&1); then
    echo "::error::cannot read restored WM_STATE: $window_state"
    exit 1
  fi
  if printf '%s\n' "$window_state" | grep -q 'window state: Normal'; then
    restored=1
    break
  fi
  sleep 0.1
done
if [ "$restored" -ne 1 ]; then
  echo "::error::Linux smoke window did not restore"
  exit 1
fi

if ! process_alive "$hello_pid"; then
  echo "::error::keld-host stopped before the close request"
  exit 1
fi
window_hex=$(printf '0x%x' "$window_id")
wmctrl -ic "$window_hex"
exited=0
for _ in $(seq 1 100); do
  if ! process_alive "$hello_pid"; then
    exited=1
    break
  fi
  sleep 0.1
done
if [ "$exited" -ne 1 ]; then
  echo "::error::keld-host did not exit after the window close request"
  exit 1
fi
set +e
wait "$hello_pid"
hello_status=$?
set -e
hello_pid=""
if [ "$hello_status" -ne 0 ]; then
  echo "::error::keld-host exited $hello_status after window close"
  exit 1
fi
close_confirmed=1
if [ "$window_manager_ready" -ne 1 ] || [ "$title_confirmed" -ne 1 ] || \
  [ "$pid_bound" -ne 1 ] || [ "$resized" -ne 1 ] || \
  [ "$minimized" -ne 1 ] || [ "$restored" -ne 1 ] || \
  [ "$close_confirmed" -ne 1 ]; then
  echo "::error::Linux window-control receipt is incomplete"
  exit 1
fi
echo "Linux hello title, resize, minimize, restore, close, and reap confirmed under X11"

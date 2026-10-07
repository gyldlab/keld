#!/bin/sh
# usage: dismiss.sh <delay_seconds> <return|escape> <electron_main_pid> <sidecar_file>
# Method: after <delay>, press the dialog's button through System Events (AX AXPress via
# `click button`), targeted at the Electron pid so no keystroke can land in another app.
# return -> "Discard" (the defaultId button), escape -> "Cancel" (the cancelId button).
# Retries every 0.5 s (max 12) until the main process has logged modal:after / dialog:after.
# Earlier smoke runs used `set frontmost` + `key code 36/53`; ~2 of ~35 such runs never dismissed.
delay="$1"; key="$2"; pid="$3"; side="$4"; out="${side%.dismiss}"
case "$key" in escape) btn=Cancel;; *) btn=Discard;; esac
sleep "$delay"
n=0
while [ "$n" -lt 12 ]; do
  grep -q '"ev":"\(modal\|dialog\):after"' "$out" 2>/dev/null && break
  n=$((n+1))
  res=$(osascript <<AS 2>&1
tell application "System Events"
  tell (first process whose unix id is $pid)
    repeat with w in windows
      if exists (sheet 1 of w) then
        if exists (button "$btn" of sheet 1 of w) then
          click button "$btn" of sheet 1 of w
          return "sheet"
        end if
      end if
      if exists (button "$btn" of w) then
        click button "$btn" of w
        return "window"
      end if
    end repeat
  end tell
end tell
return "notfound"
AS
)
  rc=$?
  printf '{"proc":"dismiss.sh","ev":"dismiss:attempt","n":%s,"rc":%s,"button":"%s","result":"%s"}\n' \
    "$n" "$rc" "$btn" "$(printf '%s' "$res" | tr '"\n' "' ")" >> "$side"
  case "$res" in sheet|window) break;; esac
  sleep 0.5
done

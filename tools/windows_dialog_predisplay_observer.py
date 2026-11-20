"""Observe the isolated Windows pre-display cancellation integration test.

Observe one retained windows_dialog_native test child, its actual HWND generation,
and a same-parent follow-up. The child blocks on explicit stdin handoffs only
when this runner sets its test-only observer selector. Run with exclusive GUI
custody; this tool does not establish a caller-relative cleanup deadline.
"""
import argparse
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time
from types import SimpleNamespace

if os.name != "nt" or c.sizeof(c.c_void_p) != 8 or c.sizeof(w.MSG) != 48:
    raise SystemExit("Windows x64 with the expected MSG ABI is required")
user32 = c.WinDLL("user32", use_last_error=True)
event_callback = c.WINFUNCTYPE(None, w.HANDLE, w.DWORD, w.HWND, w.LONG, w.LONG, w.DWORD, w.DWORD)

def proto(dll, name, result, arguments):
    function = getattr(dll, name)
    function.restype = result
    function.argtypes = arguments
    return function

# Exact Win32 observer ABI: process/thread-filtered out-of-context callback;
# current-thread message pump; read-only final IsWindow query.
win = SimpleNamespace(
    u=user32, w=w, EVTCALL=event_callback, proto=proto,
    hook_event=proto(user32, "SetWinEventHook", w.HANDLE,
                     [w.DWORD, w.DWORD, w.HMODULE, event_callback, w.DWORD, w.DWORD, w.DWORD]),
    unhook=proto(user32, "UnhookWinEvent", w.BOOL, [w.HANDLE]),
    peek=proto(user32, "PeekMessageW", w.BOOL, [c.POINTER(w.MSG), w.HWND, w.UINT, w.UINT, w.UINT]),
    translate=proto(user32, "TranslateMessage", w.BOOL, [c.POINTER(w.MSG)]),
    dispatch=proto(user32, "DispatchMessageW", w.LPARAM, [c.POINTER(w.MSG)]),
    is_window=proto(user32, "IsWindow", w.BOOL, [w.HWND]),
)

wait_messages = win.proto(win.u, "MsgWaitForMultipleObjectsEx", win.w.DWORD,
                          [win.w.DWORD, c.c_void_p, win.w.DWORD, win.w.DWORD, win.w.DWORD])

parser = argparse.ArgumentParser()
parser.add_argument("--exe", type=Path, required=True)
parser.add_argument("--expect", choices=("visible", "hidden"), required=True)
parser.add_argument("--out", type=Path, required=True)
args = parser.parse_args()
exe = args.exe.resolve(strict=True)
out = args.out.resolve()
out.mkdir(parents=True, exist_ok=False)
test_name = "acceptance::message_cancel_after_entry_before_first_display"
command = [str(exe), test_name, "--exact", "--ignored", "--nocapture"]
stdout_path = out / "stdout.log"
stderr_path = out / "stderr.log"
receipt_path = out / "receipt.json"
events = []
callback_faults = []
lines = queue.Queue(maxsize=256)
ready = None
result = None
hook = None
child = None
status = None
error = None
hook_failed = False
with stdout_path.open("x+", encoding="utf-8", newline="") as stdout_file, stderr_path.open("x+", encoding="utf-8", newline="") as stderr_file:
    try:
        environment = os.environ.copy()
        environment["KELD_B6_EXTERNAL_OBSERVER"] = "1"
        child = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                 stderr=stderr_file, text=True, bufsize=1, env=environment)
        def reader():
            try:
                total = 0
                for line in child.stdout:
                    total += len(line)
                    if total > 65536:
                        raise RuntimeError("child stdout exceeded bounded capture")
                    stdout_file.write(line)
                    stdout_file.flush()
                    lines.put_nowait(line)
            except BaseException as exc:
                try:
                    lines.put_nowait("KELD_B6_READER_ERROR " + repr(exc))
                except queue.Full:
                    pass
        output_thread = threading.Thread(target=reader, daemon=True)
        output_thread.start()
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline and result is None:
            msg = win.w.MSG()
            for _ in range(128):
                if time.monotonic() >= deadline:
                    raise RuntimeError("event pump deadline")
                if not win.peek(c.byref(msg), None, 0, 0, 1):
                    break
                win.translate(c.byref(msg))
                win.dispatch(c.byref(msg))
            try:
                line = lines.get(timeout=0.01)
            except queue.Empty:
                if child.poll() is not None:
                    raise RuntimeError("test child exited before observer receipt")
                continue
            if "KELD_B6_READER_ERROR " in line:
                raise RuntimeError(line.strip())
            if "KELD_B6_READY " in line:
                if ready is not None:
                    raise RuntimeError("duplicate ready")
                pid, tid, parent = map(int, line.split("KELD_B6_READY ", 1)[1].split())
                if pid != child.pid or not tid or not parent:
                    raise RuntimeError("child identity mismatch")
                ready = {"pid": pid, "tid": tid, "parent": parent}
                @win.EVTCALL
                def observe(_hook, event, hwnd, obj, sub, event_tid, _event_time):
                    try:
                        if obj == 0 and sub == 0 and event_tid == tid and hwnd:
                            if len(events) >= 4096:
                                raise RuntimeError("event ledger full")
                            events.append({"kind": event, "hwnd": int(hwnd), "tid": event_tid})
                    except BaseException as exc:
                        if not callback_faults:
                            callback_faults.append(repr(exc))
                hook = win.hook_event(0x8000, 0x8002, None, observe, pid, tid, 0)
                if not hook:
                    raise c.WinError(c.get_last_error())
                child.stdin.write("go\n")
                child.stdin.flush()
            if "KELD_B6_RESULT " in line:
                if ready is None:
                    raise RuntimeError("result before ready")
                hwnd, visible = line.split("KELD_B6_RESULT ", 1)[1].split()
                result = {"hwnd": int(hwnd), "visible_at_cancel": visible == "true"}
        if result is None or ready is None:
            raise RuntimeError("test did not reach result handoff")
        # Wait for exact DESTROY on the same ordered hook, with the child and
        # parent retained. CREATE and SHOW, if emitted, must precede it.
        hwnd = result["hwnd"]
        while time.monotonic() < deadline and not any(e["kind"] == 0x8001 and e["hwnd"] == hwnd for e in events):
            msg = win.w.MSG()
            for _ in range(128):
                if time.monotonic() >= deadline:
                    break
                if not win.peek(c.byref(msg), None, 0, 0, 1):
                    break
                win.translate(c.byref(msg))
                win.dispatch(c.byref(msg))
            # Wake on actual GUI queue work; timeout is only a kill-switch tick.
            if wait_messages(0, None, 50, 0x04FF, 0x0004) == 0xFFFFFFFF:
                raise c.WinError(c.get_last_error())
        exact = [event for event in events if event["hwnd"] == hwnd]
        kinds = [event["kind"] for event in exact]
        if callback_faults or kinds.count(0x8000) != 1 or kinds.count(0x8001) != 1 or kinds.index(0x8000) >= kinds.index(0x8001):
            raise RuntimeError("inconclusive: observer fault or exact CREATE→DESTROY missing")
        shows = [i for i, kind in enumerate(kinds) if kind == 0x8002]
        if any(i <= kinds.index(0x8000) or i >= kinds.index(0x8001) for i in shows):
            raise RuntimeError("inconclusive: SHOW outside unique generation")
        if bool(win.is_window(hwnd)):
            raise RuntimeError("test dialog HWND remains live")
        if result["visible_at_cancel"] != (args.expect == "visible") or bool(shows) != (args.expect == "visible"):
            raise RuntimeError("visibility did not match the expected failure-first/control case")
        if not win.unhook(hook):
            hook_failed = True
            raise c.WinError(c.get_last_error())
        hook = None
        child.stdin.write("continue\n")
        child.stdin.flush()
        status = child.wait(timeout=max(1, deadline - time.monotonic()))
        output_thread.join(timeout=2)
        if output_thread.is_alive():
            raise RuntimeError("child stdout reader still active")
        expected_status = 0 if args.expect == "hidden" else 101
        if status != expected_status:
            raise RuntimeError(f"test exit {status}, expected {expected_status}")
        stdout_file.flush()
        stderr_file.flush()
        stdout_file.seek(0)
        stderr_file.seek(0)
        stdout_text = stdout_file.read()
        stderr_text = stderr_file.read()
        if args.expect == "visible":
            if (f"test {test_name} ... FAILED" not in stdout_text or
                "cancellation first observed only after the dialog became visible" not in stderr_text):
                raise RuntimeError("baseline failure did not match the visibility assertion")
        elif (f"test {test_name} ... ok" not in stdout_text or
              "KELD_NATIVE_DIALOG_DRAINED kind=message" not in stdout_text):
            raise RuntimeError("passing test or same-parent native follow-up is missing")
    except BaseException as exc:
        error = repr(exc)
    finally:
        if hook and not win.unhook(hook):
            hook_failed = True
        if child is not None and child.poll() is None:
            try:
                child.kill()  # Only the retained test child this process created.
                child.wait(timeout=5)
            except BaseException as exc:
                error = f"{error}; owned-child cleanup failed: {exc!r}"
        if child is not None:
            status = child.poll()
        stdout_file.flush()
        stderr_file.flush()
        receipt_path.write_text(json.dumps({"command": command, "exe_sha256": hashlib.sha256(exe.read_bytes()).hexdigest(),
            "observer_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "expect": args.expect, "ready": ready, "result": result, "events": events,
            "callback_faults": callback_faults, "status": status, "error": error,
            "limitations": ["event observation is not pixel census", "observer process must stay alive"]}, indent=2), encoding="utf-8")
        if hook_failed:
            # Do not drop a possibly still-registered Python callback and then
            # enter another case. Child cleanup was attempted above.
            print("UnhookWinEvent failed; observer exiting", file=sys.stderr, flush=True)
            __import__("os")._exit(1)
if error is not None:
    raise SystemExit(error)

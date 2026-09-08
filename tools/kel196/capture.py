"""Run an unchanged test command with a bounded independent CG observer."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

parser = argparse.ArgumentParser()
parser.add_argument("--source", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--observer", type=Path, required=True)
parser.add_argument("--trace", choices=("off", "on"), required=True)
parser.add_argument("command", nargs=argparse.REMAINDER)
args = parser.parse_args()
command = args.command[1:] if args.command[:1] == ["--"] else args.command
if not command:
    parser.error("missing command")
args.output.mkdir(parents=True, exist_ok=True)
env = os.environ.copy()
if args.trace == "on":
    env["KELD196_DIAGNOSTIC"] = "1"
    env["KELD196_TRACE"] = "1"
    env["KELD196_TRACE_FILE"] = str((args.output / "appkit.jsonl").resolve())
else:
    for key in ("KELD196_DIAGNOSTIC", "KELD196_TRACE", "KELD196_TRACE_FILE"):
        env.pop(key, None)
start = time.monotonic()
with (args.output / "coregraphics.jsonl").open("w") as census, (args.output / "test.log").open("w") as log, (args.output / "test-receipt-times.jsonl").open("w") as times:
    observer = subprocess.Popen([str(args.observer.resolve())], stdout=census)
    try:
        process = subprocess.Popen(command, cwd=args.source, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        for line in process.stdout:
            times.write(json.dumps({"received_mono_s": time.monotonic(), "line": line}) + "\n")
            times.flush()
            log.write(line)
            log.flush()
            print(line, end="", flush=True)
        code = process.wait()
    finally:
        if observer.poll() is None:
            observer.terminate()
        observer.wait()
record = {"command": command, "trace": args.trace, "test_exit": code, "observer_exit": observer.returncode, "start_mono_s": start, "end_mono_s": time.monotonic()}
(args.output / "result.json").write_text(json.dumps(record, indent=2) + "\n")
sys.exit(code if code >= 0 else 128 - code)

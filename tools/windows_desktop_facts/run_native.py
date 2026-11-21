"""Run W4 native evidence only inside the explicitly reserved Ramani slot.

The first native query is inside the real-client pending Subscribe test.
Each invocation owns one Windows Job Object; timeouts affect only its process family.
No global settings or another agent's process are modified.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import time

from owned_process import run_owned

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
ORACLE_SHA256 = "bd14928e0728366fd3f41499cb398ff3f4304dab259a3e605077899a6f8c748e"


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path: Path, data: object) -> None:
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def command(name: str, argv: list[str], out: Path, env: dict[str, str],
            timeout: int, expected: int | None = 0) -> dict:
    started = time.time()
    stdout_path = out / (name + ".stdout")
    stderr_path = out / (name + ".stderr")
    metadata_path = out / (name + ".owned.json")
    try:
        owned = run_owned(
            argv, cwd=REPO, env=env, stdout_path=stdout_path,
            stderr_path=stderr_path, metadata_path=metadata_path,
            timeout_seconds=timeout,
        )
    except Exception as exc:
        write_json(out / (name + ".process.json"), {
            "argv": argv, "startedUnix": started, "completedUnix": time.time(),
            "status": "runner-error", "error": str(exc),
            "ownedProcessReceipt": str(metadata_path),
        })
        raise
    result = {
        "argv": argv, "pid": owned.target_pid, "exit": owned.returncode,
        "bootstrapPid": owned.bootstrap_pid,
        "bootstrapExit": owned.bootstrap_returncode,
        "jobActiveAfter": owned.job_active_after,
        "timedOut": owned.timed_out, "forcedCleanup": owned.forced_cleanup,
        "elapsedSeconds": owned.elapsed_seconds, "setupError": owned.setup_error,
        "startedUnix": started, "completedUnix": time.time(),
        "ownedProcessReceipt": str(metadata_path),
    }
    write_json(out / (name + ".process.json"), result)
    if (owned.job_active_after != 0 or owned.timed_out or owned.forced_cleanup
            or owned.setup_error or owned.returncode is None):
        raise RuntimeError(name + ": process-family execution or cleanup failed; "
                           "inspect retained ownership receipt and output")
    if expected is not None and owned.returncode != expected:
        raise RuntimeError(name + ": unexpected exit " + str(owned.returncode) +
                           "; inspect retained stdout/stderr")
    return {**result, "stdout": stdout_path.read_bytes(),
            "stderr": stderr_path.read_bytes()}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", required=True, type=Path)
    parser.add_argument("--shared", required=True, type=Path)
    parser.add_argument("--oracle-exe", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    if os.name != "nt" or os.environ.get("W4_RUN_NATIVE") != "1":
        raise RuntimeError("reserve the native slot, then set W4_RUN_NATIVE=1 on Windows")
    for path in (args.probe, args.oracle_exe, HERE / "oracle.cjs"):
        if not path.is_file():
            raise RuntimeError("required input missing: " + str(path))
    if sha(args.oracle_exe) != ORACLE_SHA256:
        raise RuntimeError("oracle must be the preserved verified Electron 44.4.5 binary")
    pin = json.loads((HERE / "shared-pin.json").read_text(encoding="utf-8"))
    for relative, digest in pin["files"].items():
        data = (args.shared / relative).read_bytes()
        actual = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
        if actual != digest:
            raise RuntimeError("shared source pin mismatch: " + relative)
    if args.out.exists():
        raise RuntimeError("evidence output must be a fresh directory; preserve earlier failures")
    args.out.mkdir(parents=True)
    inputs = {"probe": {"path": str(args.probe), "sha256": sha(args.probe)},
              "oracle": {"path": str(args.oracle_exe), "sha256": sha(args.oracle_exe)},
              "script": {"path": str(HERE / "oracle.cjs"), "sha256": sha(HERE / "oracle.cjs")},
              "sharedCommit": pin["commit"], "globalSettingsChanged": False}
    write_json(args.out / "inputs.json", inputs)
    env = dict(os.environ)
    env.pop("ELECTRON_RUN_AS_NODE", None)
    env.update(W4_SHARED_ROOT=str(args.shared), W4_NATIVE_PROBE=str(args.probe),
               W4_TEXT_SCALE_SCRIPT=str(HERE / "text_scale.ps1"),
               W4_SUBSCRIBE_NATIVE_RECEIPT=str(args.out / "native-first-sample.json"))
    env.pop("W4_NATIVE_RECEIPT", None)

    # No preliminary display/theme query: the held Subscribe is the first sample.
    command("subscribe-native", ["bun", "test",
            str(HERE / "subscribe_contract.test.ts"), "-t", "W4 native first sample"],
            args.out, env, 25)
    first = json.loads((args.out / "native-first-sample.json").read_text(encoding="utf-8"))
    if first["coreAdmissionProven"] is not False:
        raise RuntimeError("fixture must not claim production Core admission")

    for mode in ("pmv2", "unaware"):
        result = command(mode, [str(args.probe), "--mode", mode], args.out, env, 10)
        data = json.loads(result["stdout"])
        if data["status"] != "ok" or data["processId"] != result["pid"]:
            raise RuntimeError(mode + ": sample not tied to its actual process")
        write_json(args.out / (mode + ".json"), data)

    sources = {"invalid-monitor": "GetMonitorInfoW",
               "failed-dpi": "GetDpiForMonitor",
               "high-contrast-size": "SystemParametersInfoW(SPI_GETHIGHCONTRAST)"}
    for control, expected_source in sources.items():
        result = command(control, [str(args.probe), "--mode", "pmv2",
                         "--control", control], args.out, env, 10, expected=None)
        data = json.loads(result["stdout"])
        if result["exit"] != 1 or data.get("status") != "error" or "sample" in data:
            raise RuntimeError(control + ": control did not produce a native error with no sample")
        if data["error"]["kind"] != "native-failure" or data["error"]["source"] != expected_source:
            raise RuntimeError(control + ": wrong native failure boundary")
        write_json(args.out / (control + ".json"), data)

    command("electron", [str(args.oracle_exe), str(HERE / "oracle.cjs"),
            "--out=" + str(args.out / "electron.json")], args.out, env, 65)
    oracle = json.loads((args.out / "electron.json").read_text(encoding="utf-8"))
    if oracle["electronVersion"] != "44.4.5" or oracle["electronRunAsNodeEnv"] is not None:
        raise RuntimeError("wrong oracle runtime or process-local environment")
    env["W4_NATIVE_EVIDENCE"] = str(args.out)
    command("verify-native", ["bun", "test", str(HERE / "verify_native.test.ts")],
            args.out, env, 15)
    manifest = {p.name: sha(p) for p in args.out.iterdir() if p.is_file()}
    write_json(args.out / "manifest.json", {"files": manifest, "status": "passed",
               "cleanup": "all invocation Jobs observed empty without forced cleanup; retain post-slot census"})
    print(json.dumps({"status": "passed", "evidence": str(args.out),
                      "firstNativeAction": "valid held Subscribe test",
                      "coreAdmissionProven": False}))


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print("W4 native run failed: " + str(exc), file=sys.stderr)
        raise SystemExit(1)

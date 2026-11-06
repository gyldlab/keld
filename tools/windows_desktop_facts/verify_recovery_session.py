#!/usr/bin/env python3
"""Execute exact W2 producer/capture components with controlled acquisition.

This does not execute Core, Windows APIs, public Ready or an integrated Host.
"""
import argparse
import datetime
import hashlib
import json
import re
import shutil
import subprocess
import time
from pathlib import Path

SOURCE_COMMIT = "cd87be23ea375118224f9687894ad24e7179cc73"
PINS = {
    "crates/keld-wv/src/webview2/desktop_session.rs": "5d5fcfa0288c257b78f3fb13943be5607e66673193289cc45917b9820d7f257a",
    "crates/keld-wv/src/engine/desktop.rs": "822fd08542962b3d5b888c64d5172f0030ced6f8146ac06b70c0fae26cc607ed",
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def replace_once(source, before, after):
    assert source.count(before) == 1, "mutation/source shape changed"
    return source.replace(before, after, 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--harness", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    sources = {path: (args.source_root / path).read_bytes() for path in PINS}
    for path, expected in PINS.items():
        assert sha(sources[path]) == expected, f"immutable source mismatch: {path}"
    # No compiler/process starts before exact input validation.
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    harness = args.harness.read_text()
    assert harness.count("W4_SESSION_SOURCE") == harness.count("W4_CAPTURE_SOURCE") == 1
    rustc = shutil.which("rustc")
    assert rustc, "rustc unavailable"
    commands = []

    def run(label, argv):
        started = time.monotonic()
        result = subprocess.run([str(x) for x in argv], cwd=args.out, capture_output=True, timeout=60)
        for stream in ("stdout", "stderr"):
            (args.out / f"{label}.{stream}").write_bytes(getattr(result, stream))
        commands.append({"name": label, "argv": [str(x) for x in argv], "exit_code": result.returncode,
                         "duration_seconds": time.monotonic() - started,
                         "stdout_sha256": sha(result.stdout), "stderr_sha256": sha(result.stderr)})
        return result

    version = run("rustc-version", [rustc, "--version", "--verbose"])
    assert version.returncode == 0
    session, capture = [sources[p].decode() for p in PINS]
    refresh_output = (
        '        let facts = self.sample();\n'
        '        events\n'
        '            .send(AppWindowEvent::WindowsDesktopObserved { capture, facts })\n'
        '            .map_err(|_| WvError::EventLoop("desktop observation receiver closed".to_owned()))\n'
    )
    cases = {
        "current": (session, capture, None, (6, 0)),
        "no-replacement-refresh": (replace_once(session, "            self.refresh(events)?;", "            let _ = events;"), capture,
                                   "webview2::replacement_binding_reports_its_new_query_failure", (0, 1)),
        "retired-bind": (replace_once(session, "        if capture.is_live() {", "        if true {"), capture,
                         "webview2::retired_bind_does_not_displace_current_producer", (0, 1)),
        "no-reader-identity": (session, replace_once(capture, "            && Arc::ptr_eq(&self.reader, reader)\n", ""),
                               "webview2::capture_matches_requires_the_same_live_reader_even_with_equal_attempt", (0, 1)),
        "no-op-refresh": (replace_once(session, refresh_output, "        let _ = (capture, events);\n        Ok(())\n"), capture,
                          "webview2::retired_current_producer_never_requests_another_query", (0, 1)),
        "suppress-unavailable": (replace_once(session,
                                              "        let facts = self.sample();\n        events\n",
                                              "        let facts = self.sample();\n        if facts.is_none() { return Ok(()); }\n        events\n"), capture,
                                 "webview2::replacement_binding_reports_its_new_query_failure", (0, 1)),
    }
    observations = []
    for name, (session_text, capture_text, test, expected) in cases.items():
        case = args.out / name
        case.mkdir()
        session_path, capture_path = case / "desktop_session.rs", case / "desktop.rs"
        session_path.write_text(session_text)
        capture_path.write_text(capture_text)
        generated = harness.replace("W4_SESSION_SOURCE", str(session_path)).replace("W4_CAPTURE_SOURCE", str(capture_path))
        program, binary = case / "harness.rs", case / "proof"
        program.write_text(generated)
        compile_result = run(name + "-compile", [rustc, "--edition=2024", "--test", "-Dwarnings", "--crate-name", "w4_recovery_session", program, "-o", binary])
        assert compile_result.returncode == 0, (name, compile_result.stderr.decode())
        argv = [binary, "--test-threads=1", "--nocapture"]
        if test:
            argv += ["--exact", test]
        result = run(name + "-run", argv)
        counts = re.search(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", result.stdout.decode())
        assert counts, (name, result.stdout.decode(), result.stderr.decode())
        actual = tuple(map(int, counts.groups()))
        observations.append({"variant": name, "test": test, "exit_code": result.returncode,
                             "passed": actual[0], "failed": actual[1], "ignored": actual[2],
                             "session_sha256": sha(session_path.read_bytes()), "capture_sha256": sha(capture_path.read_bytes()),
                             "harness_sha256": sha(program.read_bytes()), "binary_sha256": sha(binary.read_bytes())})
        assert actual == (*expected, 0), observations[-1]
        assert result.returncode == (0 if expected[1] == 0 else 101), observations[-1]
    receipt = {"schema": "keld.w4-current-producer-proof/v1", "source_commit": SOURCE_COMMIT,
               "completed_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
               "source_sha256": PINS, "compiler": version.stdout.decode(), "observations": observations, "commands": commands,
               "harness_sha256": sha(args.harness.read_bytes()), "driver_sha256": sha(Path(__file__).read_bytes()),
               "limits": ["Two current W2 source modules execute unchanged in the current case.",
                          "Native acquisition uses queued synthetic values/errors, not Windows APIs or actual changes.",
                          "The actual capture identity and native producer orchestration execute with std mpsc/atomics.",
                          "Core writer, Subscribe replay/049, public Ready, actual deadlines and native cleanup do not execute.",
                          "Current replacement resampling behavior is observed, not approved as the shared recovery policy."]}
    path = args.out / "receipt.json"
    path.write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"receipt": str(path), "sha256": sha(path.read_bytes()), "observations": observations}))


if __name__ == "__main__":
    main()

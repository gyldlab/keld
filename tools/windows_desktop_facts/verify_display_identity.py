#!/usr/bin/env python3
"""Exercise pinned, verbatim production allocator fragments; not Windows qualification."""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time


PINS = {
    "desktop_sample.rs": "f25b0d7ca75c7a12fdfe4354a26ddbc375b57ea51ac9e3f22d41b0d375112743",
    "desktop_sample/target.rs": "59444b0aa68ca8630b7ef32666217df7ac3a33eefecb717363ac0b439d466122",
    "tests": "b946e759f00711589fa11d19f65b5f6b8f3e0ec07114647d525aaea364300f2b",
}
FRAGMENT_PINS = {
    "Target": "1fa52fcb0d57b48d7dabe52c5a56c3eca271905bd04851535d94adc44070c338",
    "DesktopSampler": "7b810f66256857dbf2218710bd68c45601258e0668541c6dad93f33a317279ca",
    "unavailable": "849275d5eeff411c9117530d27b28237214d74581c64b8732f4a64e1ae6df707",
    "display_id": "120beefc73f8b7b0ed510fe6b1819343158f32e2c13729578ae5108eeb8c0f00",
}
PREFIX = "webview2::desktop_sample::target::tests::desktop_identity_"
TESTS = [PREFIX + suffix for suffix in (
    "uses_each_complete_target_field",
    "retains_history_across_intervening_targets",
    "refuses_thirty_third_target_without_mutation",
)]
LIMITS = [
    "Verbatim extracted allocator/Target component, not full Windows crate compilation.",
    "WvError::Window(String) is error-type scaffolding; its production integration is untested.",
    "No native capture, query failure, normalization, disconnect, or appearance is exercised.",
    "No current integration-head, original-app, lifecycle, or contract approval is claimed.",
]
SCAFFOLD = b'''// Component-only error-type scaffold; no native/error-transport claim.
#[derive(Debug)]
pub enum WvError { Window(String) }
impl std::fmt::Display for WvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { Self::Window(message) => f.write_str(message) }
    }
}
mod webview2 {
mod desktop_sample {
use crate::WvError;
use target::Target;
'''


def sha(data):
    return hashlib.sha256(data).hexdigest()


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def replace_once(data, old, new):
    require(data.count(old) == 1, "Mutation must have exactly one matching preimage")
    return data.replace(old, new, 1)


def render(fragments, tests):
    return (SCAFFOLD + fragments["unavailable"] + fragments["DesktopSampler"]
            + b"\nimpl DesktopSampler {\n" + fragments["display_id"]
            + b"}\nmod target {\n" + fragments["Target"] + tests + b"}\n}\n}\n")


def mutations(fragments):
    field_assertion = b'assert_eq!(sampler.display_id(alternative).unwrap(), 2, "{field}");'
    result = []
    constant = dict(fragments)
    constant["display_id"] = replace_once(constant["display_id"],
        b"u32::try_from(index + 1).map_err", b"u32::try_from(index + 1).map(|_| 1).map_err")
    result.append(("constant-id", constant, TESTS[0], field_assertion,
                   "assertion `left == right` failed: signed adapter high"))
    fields = ["adapter_high", "adapter_low", "target_id", "device_path"]
    labels = ["signed adapter high", "adapter low", "target ID", "complete device path"]
    for omitted, label in zip(fields, labels):
        changed = dict(fragments)
        target = replace_once(changed["Target"], b"#[derive(Eq, PartialEq)]", b"#[derive(Eq)]")
        comparison = " && ".join(f"self.{field} == other.{field}" for field in fields if field != omitted)
        target += ("impl PartialEq for Target {\n"
                   "fn eq(&self, other: &Self) -> bool { " + comparison + " }\n}\n").encode()
        changed["Target"] = target
        result.append(("ignore-" + omitted, changed, TESTS[0], field_assertion,
                       "assertion `left == right` failed: " + label))
    history = dict(fragments)
    history["display_id"] = replace_once(history["display_id"],
        b".position(|old| *old == target)", b".position(|_| false)")
    result.append(("forget-known-id", history, TESTS[1],
        b"for target_id in [8, 3, 7, 2, 6, 4, 5] {\n"
        b"            assert_eq!(sampler.display_id(target(target_id)).unwrap(), target_id);",
        "assertion `left == right` failed"))
    capacity = dict(fragments)
    capacity["display_id"] = replace_once(capacity["display_id"], b"if i >= 32 {", b"if i > 32 {")
    result.append(("admit-thirty-third", capacity, TESTS[2],
        b"assert!(\n            sampler.display_id(target(33)).is_err(),",
        "a thirty-third distinct identity must be refused"))
    mutation = dict(fragments)
    mutation["display_id"] = replace_once(mutation["display_id"],
        b"if i >= 32 {\n", b"if i >= 32 {\n                self.known.push(target);\n")
    result.append(("mutate-before-refusal", mutation, TESTS[2],
                   b"assert_eq!(history(&sampler), before);", "assertion `left == right` failed"))
    require(len(result) == 8, "Expected eight separate controls")
    return result


def main():
    if not __debug__:
        print(json.dumps({"status": "refused", "reason": "Optimized Python is unsupported; no compiler invoked."}))
        return 2
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", required=True, type=Path)
    parser.add_argument("--tests", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--rustc", default="rustc")
    args = parser.parse_args()
    out = args.out.absolute()
    try:
        out.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        print(json.dumps({"status": "refused", "reason": str(error), "out": str(out), "compiler_invoked": False}))
        return 2
    receipt = {"schema": "keld.w4-display-identity-component/v1", "status": "failed",
               "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
               "source_commit": "37da8f08c3738de9dcde0eb3b777564800a17e33",
               "source_binding": "Pinned exported production files; private Ramani-local ancestry.",
               "current_dirty_tree_equivalence": "unclaimed",
               "limits": LIMITS, "runs": [], "cases": [], "sources": {}, "fragments": {}}
    artifacts = {}

    def write(name, data):
        path = out / name
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("xb") as stream:
            stream.write(data)
        artifacts[name] = {"sha256": sha(data), "bytes": len(data)}
        return path

    def write_json(name, value):
        return write(name, (json.dumps(value, indent=2, sort_keys=True) + "\n").encode())

    def run(name, command):
        started = time.monotonic()
        entry = {"name": name, "command": list(map(str, command)), "timeout_seconds": 60,
                 "exit": None, "timed_out": False, "pid": None}
        stdout, stderr = b"", b""
        try:
            child = subprocess.Popen(entry["command"], cwd=out, stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE, start_new_session=(os.name == "posix"))
            entry["pid"] = child.pid
            try:
                stdout, stderr = child.communicate(timeout=60)
            except subprocess.TimeoutExpired:
                entry["timed_out"] = True
                if os.name == "posix":
                    try:
                        os.killpg(child.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                else:
                    child.kill()
                try:
                    stdout, stderr = child.communicate(timeout=5)
                except subprocess.TimeoutExpired as error:
                    stdout, stderr = error.output or b"", error.stderr or b""
                    entry["cleanup_drain_timed_out"] = True
                    child.stdout.close()
                    child.stderr.close()
            entry["exit"] = child.returncode
        except OSError as error:
            entry["spawn_error"] = str(error)
        entry["duration_seconds"] = time.monotonic() - started
        write(name + ".stdout", stdout)
        write(name + ".stderr", stderr)
        entry["stdout"] = artifacts[name + ".stdout"]
        entry["stderr"] = artifacts[name + ".stderr"]
        receipt["runs"].append(entry)
        write_json(name + ".json", entry)
        require(not entry["timed_out"], name + " exceeded its 60-second bound")
        require("spawn_error" not in entry, name + " could not start")
        return entry["exit"], stdout.decode("utf-8", "replace"), stderr.decode("utf-8", "replace")

    try:
        driver = Path(__file__).read_bytes()
        receipt["driver_path"] = str(Path(__file__).absolute())
        receipt["driver_sha256"] = sha(driver)
        inputs = {name: args.source_root / name for name in PINS if name != "tests"}
        inputs["tests"] = args.tests
        blobs = {}
        for name, path in inputs.items():
            data = path.read_bytes()
            receipt["sources"][name] = {"path": str(path.absolute()), "sha256": sha(data), "bytes": len(data)}
            require(sha(data) == PINS[name], "Source pin mismatch: " + name)
            blobs[name] = data
            write("inputs/" + ("test-module.rs" if name == "tests" else name), data)
        fragments = {}

        def extract(name, source, begin, end=None):
            data = blobs[source]
            require(data.count(begin) == 1, "Unique start missing for " + name)
            start = data.index(begin)
            if end is None:
                require(data.endswith(b"    }\n}\n"), "Unexpected sampler implementation suffix")
                stop = len(data) - 2
            else:
                require(data.count(end) == 1, "Unique end missing for " + name)
                stop = data.index(end, start)
            fragment = data[start:stop]
            require(sha(fragment) == FRAGMENT_PINS[name], "Fragment pin mismatch: " + name)
            fragments[name] = fragment
            receipt["fragments"][name] = {"source": source, "start_byte": start,
                "end_byte_exclusive": stop, "start_line": data[:start].count(b"\n") + 1,
                "sha256": sha(fragment), "bytes": len(fragment)}
            write("fragments/" + name + ".rs", fragment)

        extract("Target", "desktop_sample/target.rs", b"#[derive(Eq, PartialEq)]\npub(super) struct Target {", b"\nfn active_path()")
        extract("DesktopSampler", "desktop_sample.rs", b"#[derive(Default)]\npub(super) struct DesktopSampler {", b"\nimpl DesktopSampler {")
        extract("unavailable", "desktop_sample.rs", b"fn unavailable(reason: &str) -> WvError {", b"\nfn os_error(")
        extract("display_id", "desktop_sample.rs", b"    fn display_id(&mut self, target: Target) -> Result<u32, WvError> {")
        controls = mutations(fragments)
        baseline = render(fragments, blobs["tests"])
        write_json("admission.json", {"sources": receipt["sources"], "fragments": receipt["fragments"],
                                     "selectors": TESTS, "limits": LIMITS})
        rustc = shutil.which(args.rustc)
        require(rustc is not None, "Requested rustc executable is unavailable")
        rustc = os.path.abspath(rustc)
        code, version, _ = run("compiler-version", [rustc, "--version", "--verbose"])
        require(code == 0 and re.search(r"(?m)^release: 1\.99\.1$", version), "Rust 1.99.1 is required")
        receipt["compiler_version"] = version

        def execute_case(name, source, selector=None, needle=None, message=None):
            source_path = write(name + "/generated.rs", source)
            binary = out / name / ("identity-tests.exe" if os.name == "nt" else "identity-tests")
            require(not binary.exists() and not binary.is_symlink(), "Binary output already exists: " + str(binary))
            code, _, _ = run(name + "/compile", [rustc, "--edition=2024", "--test", "-Dwarnings",
                                                  source_path, "-o", binary])
            require(code == 0, name + " compilation failed; not test evidence")
            binary_hash = sha(binary.read_bytes())
            artifacts[str(binary.relative_to(out))] = {"sha256": binary_hash, "bytes": binary.stat().st_size}
            code, listed, _ = run(name + "/discovery", [binary, "--list"])
            require(code == 0 and sorted(re.findall(r"(?m)^(\S+): test$", listed)) == sorted(TESTS),
                    name + " did not discover exactly the three required tests")
            command = [binary] + ([selector, "--exact"] if selector else []) + ["--test-threads=1", "--nocapture"]
            code, stdout, stderr = run(name + "/test", command)
            case = {"name": name, "source_sha256": sha(source), "binary_sha256": binary_hash,
                    "selector": selector, "exit": code, "qualified": False}
            receipt["cases"].append(case)
            if selector:
                require(needle in source, "Expected assertion location missing")
                line = source[:source.index(needle)].count(b"\n") + 1
                if name == "forget-known-id":
                    line += 1
                case.update({"expected_assertion_line": line, "expected_message": message})
                require(code == 101 and f"test {selector} ... FAILED" in stdout,
                        name + " did not fail its selected test with exit 101")
                require("test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out;" in stdout,
                        name + " has unexpected test counts")
                failure = stdout + stderr
                require(message in failure and f"{source_path}:{line}:" in failure,
                        name + " failed outside its intended assertion")
                case["failure_text"] = stderr
            else:
                require(code == 0 and "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;" in stdout,
                        name + " did not pass all three tests")
                require(sorted(re.findall(r"(?m)^test (\S+) \.\.\. ok$", stdout)) == sorted(TESTS),
                        name + " passed-name mismatch")
            require(sha(binary.read_bytes()) == binary_hash, name + " binary changed during execution")
            case["qualified"] = True
            write_json(name + "/case.json", case)

        execute_case("baseline", baseline)
        for name, changed, selector, needle, message in controls:
            execute_case(name, render(changed, blobs["tests"]), selector, needle, message)
        restored = render(fragments, blobs["tests"])
        require(restored == baseline, "Restored component differs from original")
        execute_case("restored", restored)
        for name, path in inputs.items():
            require(sha(path.read_bytes()) == PINS[name], "Input changed during execution: " + name)
        receipt["status"] = "passed"
    except Exception as error:
        receipt["error"] = {"type": type(error).__name__, "message": str(error)}
    finally:
        receipt["finished_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        write_json("receipt.json", receipt)
        write("SHA256SUMS", "".join(f"{value['sha256']}  {name}\n" for name, value in sorted(artifacts.items())).encode())
    print(json.dumps({"status": receipt["status"], "receipt": str(out / "receipt.json"),
                      "receipt_sha256": artifacts["receipt.json"]["sha256"],
                      "qualified_cases": sum(case["qualified"] for case in receipt["cases"]),
                      "error": receipt.get("error"), "limits": LIMITS}))
    return 0 if receipt["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())

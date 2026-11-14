"""Real-Windows, non-GUI regression tests for the W4 evidence runner.

Failure-first receipt: chatgpt-w4-desktop-20261105/closeout/
p2-timeout-refutation/run/receipt.json (1s timeout returned after 2.697s).
Fixtures wait on owned events, retain inherited stdout/stderr, and have bounded
kill switches. Only the exact test runner Popen handle and its own Job are killed.
Set W4_OWNED_TEST_OUT to retain per-case receipts; other OSes explicitly skip.
"""
from __future__ import annotations

import ctypes
from ctypes import wintypes
from dataclasses import asdict
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
import uuid

HERE = Path(__file__).resolve().parent
HELPER = Path(os.environ.get("W4_OWNED_HELPER", HERE / "owned_process.py")).resolve()
WAIT_OBJECT_0, WAIT_TIMEOUT = 0, 258


class _Native:
    """Independent observation, using real kernel handles rather than PID scans."""

    def __init__(self) -> None:
        self.dll = ctypes.WinDLL("kernel32", use_last_error=True)
        signatures = {
            "CreateEventW": ([ctypes.c_void_p, wintypes.BOOL, wintypes.BOOL,
                              wintypes.LPCWSTR], wintypes.HANDLE),
            "OpenEventW": ([wintypes.DWORD, wintypes.BOOL, wintypes.LPCWSTR], wintypes.HANDLE),
            "SetEvent": ([wintypes.HANDLE], wintypes.BOOL),
            "OpenProcess": ([wintypes.DWORD, wintypes.BOOL, wintypes.DWORD], wintypes.HANDLE),
            "WaitForSingleObject": ([wintypes.HANDLE, wintypes.DWORD], wintypes.DWORD),
            "GetExitCodeProcess": ([wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)], wintypes.BOOL),
            "QueryInformationJobObject": ([wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p,
                                           wintypes.DWORD, ctypes.c_void_p], wintypes.BOOL),
            "CloseHandle": ([wintypes.HANDLE], wintypes.BOOL),
        }
        for name, (args, result) in signatures.items():
            function = getattr(self.dll, name)
            function.argtypes, function.restype = args, result

    @staticmethod
    def checked(value: int) -> int:
        if not value:
            raise ctypes.WinError(ctypes.get_last_error())
        return value

    def wait(self, handle: int, seconds: float = 0) -> int:
        result = self.dll.WaitForSingleObject(handle, math.ceil(max(0, seconds) * 1000))
        if result not in (WAIT_OBJECT_0, WAIT_TIMEOUT):
            raise ctypes.WinError(ctypes.get_last_error())
        return result

    def event(self, name: str, create: bool = False) -> int:
        return self.checked(self.dll.CreateEventW(None, True, False, name) if create
                            else self.dll.OpenEventW(0x100002, False, name))

    def signal(self, handle: int) -> None:
        self.checked(self.dll.SetEvent(handle))

    def process(self, pid: int) -> int:
        # Called for a just-published fixture PID while its owned gate holds it
        # alive. No process-name scans or PID-based termination are used.
        return self.checked(self.dll.OpenProcess(0x100000 | 0x1000, False, pid))

    def exit_code(self, handle: int) -> int:
        value = wintypes.DWORD()
        self.checked(self.dll.GetExitCodeProcess(handle, ctypes.byref(value)))
        return value.value

    def job_count(self, handle: int) -> int:
        # JOBOBJECT_BASIC_ACCOUNTING_INFORMATION: four 64-bit times followed by
        # four DWORDs; ActiveProcesses is at byte 40. This is an independent read
        # of the exact Job handle just before its owner closes it.
        data = ctypes.create_string_buffer(48)
        self.checked(self.dll.QueryInformationJobObject(handle, 1, data, 48, None))
        return ctypes.c_uint32.from_buffer(data, 40).value

    def close(self, handle: int) -> None:
        self.checked(self.dll.CloseHandle(handle))


def _write(path: Path, value: dict) -> None:
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, default=str) + "\n", encoding="utf-8")
    os.replace(temporary, path)


def _fixture(kind: str, directory: Path) -> int:
    native = _Native()
    config = json.loads((directory / "fixture.json").read_text(encoding="utf-8"))
    names = config["events"]
    if kind == "leaf":
        ready, release = native.event(names["leaf_ready"]), native.event(names["release_leaf"])
        print("leaf inherited stdout", flush=True)
        print("leaf inherited stderr", file=sys.stderr, flush=True)
        native.signal(ready)
        return 31 if native.wait(release, 10) == WAIT_OBJECT_0 else 88
    child = subprocess.Popen([sys.executable, "-B", str(Path(__file__).resolve()),
                              "--fixture", "leaf", str(directory)], close_fds=True)
    _write(directory / "family.json", {"nonce": config["nonce"],
                                        "targetPid": os.getpid(), "descendantPid": child.pid})
    ready, release = native.event(names["parent_ready"]), native.event(names["release_parent"])
    print("parent env=" + os.environ.get("W4_OWNED_ENV", "missing"), flush=True)
    native.signal(ready)
    if native.wait(release, 10) != WAIT_OBJECT_0:
        return 89
    if kind == "normal":
        return 17 if child.wait(timeout=5) == 31 else 90
    return 23


def _exercise(config_path: Path) -> int:
    config = json.loads(config_path.read_text(encoding="utf-8"))
    spec = importlib.util.spec_from_file_location("owned_under_test", config["helper"])
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    native = _Native()
    observed = {}
    close = module._Job.close

    def observe_close(job: object) -> None:
        observed["jobActiveAtClose"] = native.job_count(job.handle)
        close(job)

    module._Job.close = observe_close
    directory = config_path.parent
    env = dict(os.environ, W4_OWNED_ENV="preserved")
    observer_handles = []
    leaf_handle = None
    if config.get("schedule_after_zero"):
        real_count = module._Job.active_processes
        names = json.loads((directory / "fixture.json").read_text())["events"]

        def delayed_count(job: object) -> int:
            nonlocal leaf_handle
            count = real_count(job)
            if "firstActualJobCount" not in observed:
                observed["firstActualJobCount"] = count
                assert count == 0
                ready = native.event(names["parent_ready"])
                release = native.event(names["release_parent"])
                leaf_ready = native.event(names["leaf_ready"])
                observer_handles.extend((ready, release, leaf_ready))
                assert native.wait(ready, 2) == WAIT_OBJECT_0
                metadata = json.loads((directory / "metadata.json").read_text())
                bootstrap_handle = native.process(metadata["bootstrapPid"])
                observer_handles.append(bootstrap_handle)
                native.signal(release)
                assert native.wait(leaf_ready, 2) == WAIT_OBJECT_0
                family = json.loads((directory / "family.json").read_text())
                leaf_handle = native.process(family["descendantPid"])
                observer_handles.append(leaf_handle)
                observed["bootstrapWaitAfterEnrollment"] = native.wait(bootstrap_handle, 2)
                assert observed["bootstrapWaitAfterEnrollment"] == WAIT_OBJECT_0
                observed["jobAfterBootstrapExit"] = native.job_count(job.handle)
                observed["leafWaitAfterBootstrapExit"] = native.wait(leaf_handle)
                assert observed["jobAfterBootstrapExit"] > 0
                assert observed["leafWaitAfterBootstrapExit"] == WAIT_TIMEOUT
            # Preserve the real OS value; only delay delivery after its read.
            return count

        module._Job.active_processes = delayed_count
    try:
        result = module.run_owned(
            config["argv"], cwd=directory, env=env,
            stdout_path=directory / "target.stdout", stderr_path=directory / "target.stderr",
            metadata_path=directory / "metadata.json", timeout_seconds=config["timeout"],
            cleanup_timeout_seconds=config["cleanup"])
        observed["result"] = asdict(result)
    except module.OwnedProcessCleanupError as exc:
        observed["cleanupError"] = str(exc)
    finally:
        if leaf_handle is not None:
            observed["leafWaitAfterOwnerReturn"] = native.wait(leaf_handle, 1)
            observed["leafExitAfterOwnerReturn"] = native.exit_code(leaf_handle)
        for handle in observer_handles:
            native.close(handle)
    _write(directory / "exercise.json", observed)
    return 0


@unittest.skipUnless(os.name == "nt", "requires actual Windows Job Objects")
class OwnedProcessTests(unittest.TestCase):
    def setUp(self) -> None:
        self.native = _Native()
        self.temporary = None
        base = os.environ.get("W4_OWNED_TEST_OUT")
        if base:
            self.directory = Path(base) / (self._testMethodName + "-" + uuid.uuid4().hex[:8])
            self.directory.mkdir(parents=True)
        else:
            self.temporary = tempfile.TemporaryDirectory(prefix="w4-owned-")
            self.directory = Path(self.temporary.name)
        self.nonce = uuid.uuid4().hex
        names = {name: "Local\\w4-owned-" + self.nonce + "-" + name for name in
                 ("parent_ready", "leaf_ready", "release_parent", "release_leaf")}
        self.events = {name: self.native.event(value, create=True) for name, value in names.items()}
        _write(self.directory / "fixture.json", {"nonce": self.nonce, "events": names})
        self.runners, self.handles, self.files = [], [], []
        self.handle_pids = {}
        self.observations = {"nonce": self.nonce, "waits": []}

    def tearDown(self) -> None:
        # Also release an injected pre-enrollment gate after a failed assertion;
        # last-Job-handle close then stops that bootstrap before it can launch argv.
        for name in ("release_parent", "release_leaf"):
            self.native.signal(self.events[name])
        for child in self.runners:
            if child.poll() is None:
                child.kill()  # Exact runner created by this test; Job kill-on-close follows.
                child.wait(timeout=3)
        self.observations["teardownWaits"] = [self.native.wait(handle, 1) for handle in self.handles]
        for handle in self.handles + list(self.events.values()):
            self.native.close(handle)
        for file in self.files:
            file.close()
        _write(self.directory / "process-observations.json", self.observations)
        if self.temporary is not None:
            self.temporary.cleanup()

    def start(self, mode: str = "normal", *, timeout: float = 4, helper: Path = HELPER,
              argv: list[str] | None = None, cleanup: float = 1.5,
              schedule_after_zero: bool = False) -> subprocess.Popen:
        target = argv or [sys.executable, "-B", str(Path(__file__).resolve()),
                          "--fixture", mode, str(self.directory)]
        _write(self.directory / "exercise-config.json", {"helper": str(helper),
                                                        "argv": target, "timeout": timeout,
                                                        "cleanup": cleanup,
                                                        "schedule_after_zero": schedule_after_zero})
        stdout = (self.directory / "exercise.stdout").open("wb")
        stderr = (self.directory / "exercise.stderr").open("wb")
        self.files.extend((stdout, stderr))
        child = subprocess.Popen([sys.executable, "-B", str(Path(__file__).resolve()),
                                  "--exercise", str(self.directory / "exercise-config.json")],
                                 stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
                                 close_fds=True)
        self.runners.append(child)
        return child

    def observe_family(self) -> tuple[int, int, dict]:
        for name in ("parent_ready", "leaf_ready"):
            self.assertEqual(self.native.wait(self.events[name], 2), WAIT_OBJECT_0,
                             "fixture never reached observable readiness: " + name)
        family = json.loads((self.directory / "family.json").read_text(encoding="utf-8"))
        self.assertEqual(family["nonce"], self.nonce)
        target = self.native.process(family["targetPid"])
        leaf = self.native.process(family["descendantPid"])
        self.handles.extend((target, leaf))
        self.handle_pids.update({target: family["targetPid"], leaf: family["descendantPid"]})
        self.observations["family"] = family
        self.assertEqual(self.native.wait(target), WAIT_TIMEOUT)
        self.assertEqual(self.native.wait(leaf), WAIT_TIMEOUT)
        return target, leaf, family

    def finish(self, runner: subprocess.Popen, timeout: float = 5) -> dict:
        self.assertEqual(runner.wait(timeout=timeout), 0,
                         (self.directory / "exercise.stderr").read_text(encoding="utf-8"))
        value = json.loads((self.directory / "exercise.json").read_text(encoding="utf-8"))
        if "result" in value:
            self.assertEqual(value["jobActiveAtClose"], 0, "independent exact Job census was nonzero")
            self.assertEqual(value["result"]["job_active_after"], 0)
        return value

    def assert_exited(self, *handles: int) -> None:
        for handle in handles:
            observed = self.native.wait(handle, 1)
            self.observations["waits"].append({"pid": self.handle_pids[handle],
                                               "waitResult": observed,
                                               "exitCode": self.native.exit_code(handle)})
            self.assertEqual(observed, WAIT_OBJECT_0)

    def mutated(self, old: str, new: str) -> Path:
        source = HELPER.read_text(encoding="utf-8")
        self.assertEqual(source.count(old), 1)
        path = self.directory / "mutated_owned_process.py"
        path.write_text(source.replace(old, new), encoding="utf-8")
        return path

    def test_normal_family_exit_and_actual_nonzero_code(self) -> None:
        runner = self.start()
        target, leaf, family = self.observe_family()
        self.native.signal(self.events["release_leaf"])
        self.native.signal(self.events["release_parent"])
        result = self.finish(runner)["result"]
        self.assert_exited(target, leaf)
        self.assertEqual((self.native.exit_code(target), self.native.exit_code(leaf)), (17, 31))
        self.assertEqual(result["returncode"], 17)
        self.assertEqual(result["target_pid"], family["targetPid"])
        self.assertNotEqual(result["target_pid"], result["bootstrap_pid"])
        self.assertEqual(result["bootstrap_returncode"], 0)
        self.assertIsNone(result["setup_error"])
        self.assertFalse(result["timed_out"] or result["forced_cleanup"])
        self.assertIn("env=preserved", (self.directory / "target.stdout").read_text())
        self.assertIn("leaf inherited stderr", (self.directory / "target.stderr").read_text())

    def test_timeout_reaps_descendant_retaining_stdio(self) -> None:
        runner = self.start("timeout", timeout=1)
        target, leaf, _ = self.observe_family()
        result = self.finish(runner)["result"]
        self.assert_exited(target, leaf)
        self.assertTrue(result["timed_out"])
        self.assertTrue(result["forced_cleanup"])
        self.assertIsNone(result["returncode"])
        self.assertLess(result["elapsed_seconds"], 3)
        self.assertIn("leaf inherited stdout", (self.directory / "target.stdout").read_text())

    def test_normal_parent_allows_observed_child_tail(self) -> None:
        runner = self.start("orphan")
        target, leaf, _ = self.observe_family()
        self.native.signal(self.events["release_parent"])
        self.assertEqual(self.native.wait(target, 1), WAIT_OBJECT_0)
        self.assertEqual(self.native.wait(leaf), WAIT_TIMEOUT)
        self.native.signal(self.events["release_leaf"])
        result = self.finish(runner)["result"]
        self.assert_exited(target, leaf)
        self.assertEqual(result["returncode"], 23)
        self.assertFalse(result["forced_cleanup"] or result["timed_out"])

    def test_normal_parent_left_live_child_requires_forced_cleanup(self) -> None:
        runner = self.start("orphan")
        target, leaf, _ = self.observe_family()
        self.native.signal(self.events["release_parent"])
        result = self.finish(runner)["result"]
        self.assert_exited(target, leaf)
        self.assertEqual(result["returncode"], 23)
        self.assertTrue(result["forced_cleanup"])
        self.assertFalse(result["timed_out"])
        self.assertLess(result["elapsed_seconds"], 3)

    def test_failed_target_launch_is_not_bootstrap_success(self) -> None:
        result = self.finish(self.start(argv=[str(self.directory / "missing.exe")]))["result"]
        self.assertIsNone(result["returncode"])
        self.assertIsNone(result["target_pid"])
        self.assertEqual(result["bootstrap_returncode"], 70)
        self.assertIn("FileNotFoundError", result["setup_error"])
        self.assertFalse(result["timed_out"] or result["forced_cleanup"])

    def test_failed_enrollment_cannot_start_target(self) -> None:
        helper = self.mutated("api.assign_current(job_handle)",
                              "raise OSError('injected enrollment failure')")
        result = self.finish(self.start(helper=helper))["result"]
        self.assertIsNone(result["target_pid"])
        self.assertIn("injected enrollment failure", result["setup_error"])
        self.assertFalse((self.directory / "family.json").exists())

    def delayed_helper(self, query_failure: bool = False) -> Path:
        # Hold only trusted bootstrap code at an owned event before assignment.
        # No target code can execute during this deliberate enrollment delay.
        injection = "\n        ".join([
            "api.dll.OpenEventW.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.LPCWSTR]",
            "api.dll.OpenEventW.restype = wintypes.HANDLE",
            "api.dll.SetEvent.argtypes = [wintypes.HANDLE]",
            "api.dll.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]",
            "events = json.loads((metadata_path.parent / 'fixture.json').read_text())['events']",
            "ready = api.dll.OpenEventW(0x100002, False, events['parent_ready'])",
            "release = api.dll.OpenEventW(0x100002, False, events['release_parent'])",
            "_write_json(metadata_path, receipt)",
            "api.checked(api.dll.SetEvent(ready))",
            "if api.dll.WaitForSingleObject(release, 10000) != 0:",
            "    raise RuntimeError('test enrollment gate timed out')",
            "api.assign_current(job_handle)",
        ])
        helper = self.mutated("api.assign_current(job_handle)", injection)
        if query_failure:
            source = helper.read_text(encoding="utf-8")
            helper.write_text(source.replace("observed = _Accounting()",
                              "raise OSError('injected Job query failure')"), encoding="utf-8")
        return helper

    def delayed_enrollment(self, query_failure: bool = False) -> None:
        helper = self.delayed_helper(query_failure)
        runner = self.start(helper=helper, timeout=1)
        self.assertEqual(self.native.wait(self.events["parent_ready"], 2), WAIT_OBJECT_0)
        receipt = json.loads((self.directory / "metadata.json").read_text())
        handle = self.native.process(receipt["bootstrapPid"])
        self.handles.append(handle)
        self.handle_pids[handle] = receipt["bootstrapPid"]
        self.assertEqual(self.native.wait(handle), WAIT_TIMEOUT)
        value = self.finish(runner)
        self.assert_exited(handle)
        if query_failure:
            self.assertIn("injected Job query failure", value["cleanupError"])
            self.assertNotIn("result", value)
        else:
            result = value["result"]
            self.assertTrue(result["timed_out"] and result["forced_cleanup"])
            self.assertIsNone(result["target_pid"])
        self.assertFalse((self.directory / "family.json").exists())

    def test_timeout_before_enrollment_reaps_exact_bootstrap(self) -> None:
        self.delayed_enrollment()

    def test_query_failure_still_reaps_unenrolled_bootstrap(self) -> None:
        self.delayed_enrollment(query_failure=True)

    def test_zero_census_cannot_precede_bootstrap_exit_observation(self) -> None:
        # Between a real zero census and delivery of that value, let the gated
        # bootstrap enroll, create a held descendant, and exit. No count or poll
        # result is fabricated. The accepting census must follow exit observation.
        value = self.finish(self.start("orphan", helper=self.delayed_helper(), timeout=1,
                                       cleanup=3, schedule_after_zero=True))
        self.assertEqual(value["firstActualJobCount"], 0)
        self.assertEqual(value["bootstrapWaitAfterEnrollment"], WAIT_OBJECT_0)
        self.assertGreater(value["jobAfterBootstrapExit"], 0)
        self.assertEqual(value["leafWaitAfterBootstrapExit"], WAIT_TIMEOUT)
        self.assertEqual(value["leafWaitAfterOwnerReturn"], WAIT_OBJECT_0)
        self.assertTrue(value["result"]["forced_cleanup"])

    def test_cleanup_deadline_refuses_unproven_zero(self) -> None:
        helper = self.mutated("self.api.checked(self.api.dll.TerminateJobObject(self.handle, 1))",
                              "pass  # fault injection: termination removed")
        runner = self.start("orphan", helper=helper, cleanup=0.5)
        target, leaf, _ = self.observe_family()
        self.native.signal(self.events["release_parent"])
        value = self.finish(runner)
        self.assertIn("cleanup deadline exceeded", value["cleanupError"])
        self.assertNotIn("result", value)
        self.assertGreater(value["jobActiveAtClose"], 0)
        # Last-handle close is a backstop after refusal, not a success receipt.
        self.assert_exited(target, leaf)

    def test_runner_death_closes_sole_job_handle(self) -> None:
        runner = self.start("timeout", timeout=5)
        target, leaf, _ = self.observe_family()
        runner.kill()
        runner.wait(timeout=2)
        self.assert_exited(target, leaf)


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--fixture":
        raise SystemExit(_fixture(sys.argv[2], Path(sys.argv[3])))
    if len(sys.argv) > 1 and sys.argv[1] == "--exercise":
        raise SystemExit(_exercise(Path(sys.argv[2])))
    unittest.main()

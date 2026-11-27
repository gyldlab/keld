"""Bounded process custody for the W4 Windows test fixture only.

No product boundary change: the Rust Host Job owner is not callable from this
standalone Python evidence runner. This small standard-library adapter owns one
unnamed Job per command; it does not implement Host identity or sandbox policy.
CreateProcess descendants inherit membership; delegation to external services is
outside this fixture's contract. See Microsoft Learn, ``procthread/job-objects``.

The trusted bootstrap joins before launching argv, then closes its Job handle.
Only the caller retains the Job. File-backed output cannot hold a pipe drain open.
The execution deadline and one separate cleanup deadline bound every wait.
"""
from __future__ import annotations

import ctypes
from ctypes import wintypes
from dataclasses import dataclass
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time


@dataclass(frozen=True)
class OwnedProcessResult:
    argv: list[str]
    returncode: int | None
    target_pid: int | None
    bootstrap_pid: int | None
    bootstrap_returncode: int | None
    stdout_path: Path
    stderr_path: Path
    metadata_path: Path
    timed_out: bool
    forced_cleanup: bool
    elapsed_seconds: float
    job_active_after: int | None
    setup_error: str | None


class OwnedProcessCleanupError(RuntimeError):
    """The deadline expired or the OS could not prove exact owned-family zero."""


class _BasicLimits(ctypes.Structure):
    _fields_ = [("PerProcessUserTimeLimit", ctypes.c_longlong),
                ("PerJobUserTimeLimit", ctypes.c_longlong),
                ("LimitFlags", wintypes.DWORD),
                ("MinimumWorkingSetSize", ctypes.c_size_t),
                ("MaximumWorkingSetSize", ctypes.c_size_t),
                ("ActiveProcessLimit", wintypes.DWORD),
                ("Affinity", ctypes.c_size_t),
                ("PriorityClass", wintypes.DWORD),
                ("SchedulingClass", wintypes.DWORD)]


class _IoCounters(ctypes.Structure):
    _fields_ = [(name, ctypes.c_ulonglong) for name in
                ("ReadOperationCount", "WriteOperationCount", "OtherOperationCount",
                 "ReadTransferCount", "WriteTransferCount", "OtherTransferCount")]


class _ExtendedLimits(ctypes.Structure):
    _fields_ = [("BasicLimitInformation", _BasicLimits), ("IoInfo", _IoCounters),
                ("ProcessMemoryLimit", ctypes.c_size_t),
                ("JobMemoryLimit", ctypes.c_size_t),
                ("PeakProcessMemoryUsed", ctypes.c_size_t),
                ("PeakJobMemoryUsed", ctypes.c_size_t)]


class _Accounting(ctypes.Structure):
    _fields_ = [(name, ctypes.c_longlong) for name in
                ("TotalUserTime", "TotalKernelTime", "ThisPeriodTotalUserTime",
                 "ThisPeriodTotalKernelTime")] + [
                (name, wintypes.DWORD) for name in
                ("TotalPageFaultCount", "TotalProcesses", "ActiveProcesses",
                 "TotalTerminatedProcesses")]


class _WinAPI:
    def __init__(self) -> None:
        # Delayed loading keeps imports and unittest discovery safe on other OSes.
        self.dll = ctypes.WinDLL("kernel32", use_last_error=True)
        signatures = {
            "CreateJobObjectW": ([ctypes.c_void_p, wintypes.LPCWSTR], wintypes.HANDLE),
            "SetInformationJobObject": ([wintypes.HANDLE, ctypes.c_int,
                                         ctypes.c_void_p, wintypes.DWORD], wintypes.BOOL),
            "QueryInformationJobObject": ([wintypes.HANDLE, ctypes.c_int,
                                           ctypes.c_void_p, wintypes.DWORD,
                                           ctypes.c_void_p], wintypes.BOOL),
            "AssignProcessToJobObject": ([wintypes.HANDLE, wintypes.HANDLE], wintypes.BOOL),
            "IsProcessInJob": ([wintypes.HANDLE, wintypes.HANDLE,
                                ctypes.POINTER(wintypes.BOOL)], wintypes.BOOL),
            "GetCurrentProcess": ([], wintypes.HANDLE),
            "TerminateJobObject": ([wintypes.HANDLE, wintypes.UINT], wintypes.BOOL),
            "CloseHandle": ([wintypes.HANDLE], wintypes.BOOL),
        }
        for name, (args, result) in signatures.items():
            function = getattr(self.dll, name)
            function.argtypes, function.restype = args, result

    @staticmethod
    def checked(value: object) -> None:
        if not value:
            raise ctypes.WinError(ctypes.get_last_error())

    def assign_current(self, handle: int) -> None:
        self.checked(self.dll.AssignProcessToJobObject(handle, self.dll.GetCurrentProcess()))

    def is_member(self, process: int, job: int) -> bool:
        value = wintypes.BOOL()
        self.checked(self.dll.IsProcessInJob(process, job, ctypes.byref(value)))
        return bool(value.value)

    def close(self, handle: int) -> None:
        self.checked(self.dll.CloseHandle(handle))


class _Job:
    def __init__(self, api: _WinAPI) -> None:
        self.api = api
        self.handle = api.dll.CreateJobObjectW(None, None)
        api.checked(self.handle)
        try:
            limits = _ExtendedLimits()
            limits.BasicLimitInformation.LimitFlags = 0x2000  # KILL_ON_JOB_CLOSE only.
            api.checked(api.dll.SetInformationJobObject(
                self.handle, 9, ctypes.byref(limits), ctypes.sizeof(limits)))
            observed = _ExtendedLimits()
            api.checked(api.dll.QueryInformationJobObject(
                self.handle, 9, ctypes.byref(observed), ctypes.sizeof(observed), None))
            if observed.BasicLimitInformation.LimitFlags != 0x2000:
                raise RuntimeError("Job did not retain kill-on-close with no breakaway")
        except BaseException:
            api.close(self.handle)
            raise

    def active_processes(self) -> int:
        observed = _Accounting()
        self.api.checked(self.api.dll.QueryInformationJobObject(
            self.handle, 1, ctypes.byref(observed), ctypes.sizeof(observed), None))
        return observed.ActiveProcesses

    def terminate(self) -> None:
        self.api.checked(self.api.dll.TerminateJobObject(self.handle, 1))

    def close(self) -> None:
        self.api.close(self.handle)


def _write_json(path: Path, value: dict) -> None:
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    os.replace(temporary, path)


def _bootstrap(job_handle: int, request_path: Path, metadata_path: Path) -> int:
    receipt = {"bootstrapPid": os.getpid(), "targetPid": None, "targetExit": None,
               "enrolled": False, "targetWaitTimedOut": False, "setupError": None}
    api = _WinAPI()
    try:
        os.set_handle_inheritable(job_handle, False)
        api.assign_current(job_handle)
        if not api.is_member(api.dll.GetCurrentProcess(), job_handle):
            raise RuntimeError("bootstrap Job enrollment was not observed")
        api.close(job_handle)
        job_handle = 0
        receipt["enrolled"] = True
        _write_json(metadata_path, receipt)
        request = json.loads(request_path.read_text(encoding="utf-8"))
        # The Job handle is already closed; membership, not a leaked handle,
        # puts the target and its CreateProcess descendants in the same Job.
        child = subprocess.Popen(request["argv"], cwd=request["cwd"],
                                 stdin=subprocess.DEVNULL, close_fds=True)
        receipt["targetPid"] = child.pid
        _write_json(metadata_path, receipt)
        try:
            receipt["targetExit"] = child.wait(
                timeout=max(0.0, request["deadline"] - time.monotonic()))
        except subprocess.TimeoutExpired:
            receipt["targetWaitTimedOut"] = True
        _write_json(metadata_path, receipt)
        return 0
    except BaseException as exc:
        receipt["setupError"] = type(exc).__name__ + ": " + str(exc)
        _write_json(metadata_path, receipt)
        return 70
    finally:
        if job_handle:
            api.close(job_handle)


def _cleanup(job: _Job, child: subprocess.Popen | None, *, deadline: float,
             allow_grace: bool) -> tuple[int, bool]:
    """Observe one Job and one exact Popen handle, under one fixed deadline."""
    grace_end = min(deadline, time.monotonic() + min(1.0, max(
        0.0, deadline - time.monotonic()) / 2)) if allow_grace else time.monotonic()
    forced = False
    while True:
        direct_done = child is None or child.poll() is not None
        # A zero read before bootstrap exit can become stale if it enrolls and
        # spawns during the observation gap. Read the accepting census afterward.
        active = job.active_processes()
        if direct_done and active == 0:
            return active, forced
        now = time.monotonic()
        if now >= deadline:
            raise OwnedProcessCleanupError(
                f"owned cleanup deadline exceeded: Job ActiveProcesses={active}, "
                f"bootstrapExited={direct_done}")
        if not forced and now >= grace_end:
            # Kill an as-yet unenrolled bootstrap first. If enrollment races this
            # check, the following exact-Job termination still catches its family.
            if not direct_done and not job.api.is_member(int(child._handle), job.handle):
                child.kill()
            job.terminate()
            forced = True
        # This poll awaits observable OS accounting; elapsed time is only a guard.
        time.sleep(min(0.01, max(0.0, deadline - time.monotonic())))


def run_owned(argv: list[str], *, cwd: str | Path, env: dict[str, str] | None = None,
              stdout_path: str | Path, stderr_path: str | Path,
              timeout_seconds: float, metadata_path: str | Path | None = None,
              cleanup_timeout_seconds: float = 3.0) -> OwnedProcessResult:
    """Run one test command; reject success unless actual owned-family zero is read.

    Output paths must be fresh. A nonzero target exit is returned unchanged.
    On timeout its exit may be unavailable, never replaced by the bootstrap exit.
    Callers must reject timeout, forced cleanup, setup error, absent target exit,
    or absent Job census. A failed pre-launch Job setup has census None.
    OS process creation and local file operations are synchronous; deadlines bound
    waits, not a hung kernel/filesystem call. No process is reopened by PID.
    """
    if os.name != "nt":
        raise OSError("run_owned requires Windows Job Objects")
    if not argv or not all(isinstance(value, str) for value in argv):
        raise ValueError("argv must be a nonempty list of strings")
    if any(not math.isfinite(value) or value <= 0
           for value in (timeout_seconds, cleanup_timeout_seconds)):
        raise ValueError("execution and cleanup deadlines must be finite and positive")
    output, error = Path(stdout_path).resolve(), Path(stderr_path).resolve()
    metadata = (Path(metadata_path).resolve() if metadata_path is not None
                else output.with_name(output.name + ".owned.json"))
    if len({output, error, metadata}) != 3:
        raise ValueError("stdout, stderr and metadata require distinct paths")
    for path in (output, error, metadata):
        path.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    execution_deadline = started + timeout_seconds
    job, child, request_path = None, None, None
    setup_error = None
    timed_out = forced = False
    active_after = None
    receipt = {}
    # Exclusive creation preserves earlier evidence and never uses PIPE.
    with metadata.open("x"):
        pass
    with output.open("xb") as stdout, error.open("xb") as stderr:
        try:
            try:
                api = _WinAPI()
                job = _Job(api)
                with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", delete=False,
                                                 dir=metadata.parent, suffix=".request.json") as request:
                    request_path = Path(request.name)
                    json.dump({"argv": argv, "cwd": str(Path(cwd).resolve()),
                               "deadline": execution_deadline}, request)
                startup = subprocess.STARTUPINFO()
                startup.lpAttributeList = {"handle_list": [job.handle]}
                os.set_handle_inheritable(job.handle, True)
                try:
                    child = subprocess.Popen(
                        [sys.executable, "-I", "-S", "-B", str(Path(__file__).resolve()),
                         "--bootstrap", str(job.handle), str(request_path), str(metadata)],
                        cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=stdout,
                        stderr=stderr, close_fds=True, startupinfo=startup)
                finally:
                    os.set_handle_inheritable(job.handle, False)
                try:
                    child.wait(timeout=max(0.0, execution_deadline - time.monotonic()))
                except subprocess.TimeoutExpired:
                    timed_out = True
                timed_out = timed_out or time.monotonic() >= execution_deadline
            except Exception as exc:
                setup_error = type(exc).__name__ + ": " + str(exc)
            finally:
                if job is not None:
                    cleanup_deadline = time.monotonic() + cleanup_timeout_seconds
                    try:
                        active_after, forced = _cleanup(
                            job, child, deadline=cleanup_deadline,
                            allow_grace=not timed_out and setup_error is None)
                    except Exception as exc:
                        # An accounting failure must not skip disposal of a
                        # bootstrap that has not joined yet. Preserve refusal,
                        # attempt both owned handles, and never renew the budget.
                        errors = [str(exc)]
                        if child is not None and child.poll() is None:
                            try:
                                enrolled = job.api.is_member(int(child._handle), job.handle)
                            except Exception as membership_error:
                                errors.append("membership query: " + str(membership_error))
                                enrolled = False  # Unproven: exact owned bootstrap fallback.
                            if not enrolled:
                                try:
                                    child.kill()
                                except Exception as stop_error:
                                    errors.append("bootstrap abort: " + str(stop_error))
                        try:
                            job.terminate()
                        except Exception as stop_error:
                            errors.append("Job abort: " + str(stop_error))
                        if child is not None:
                            try:
                                child.wait(timeout=max(0.0, cleanup_deadline - time.monotonic()))
                            except Exception as wait_error:
                                errors.append("bootstrap exit observation: " + str(wait_error))
                        raise OwnedProcessCleanupError(
                            "owned cleanup could not be proven: " + "; ".join(errors)) from exc
            if child is not None:
                try:
                    receipt = json.loads(metadata.read_text(encoding="utf-8"))
                    if receipt.get("bootstrapPid") != child.pid:
                        raise ValueError("bootstrap receipt does not match the owned process")
                    setup_error = setup_error or receipt.get("setupError")
                    timed_out = timed_out or receipt.get("targetWaitTimedOut", False)
                    if not timed_out and not setup_error and (
                            child.returncode != 0 or receipt.get("targetExit") is None):
                        setup_error = "bootstrap exited without an observed target exit"
                except (ValueError, OSError) as exc:
                    setup_error = setup_error or "invalid bootstrap receipt: " + str(exc)
            result = OwnedProcessResult(
                list(argv), receipt.get("targetExit"), receipt.get("targetPid"),
                child.pid if child is not None else None,
                child.returncode if child is not None else None, output, error, metadata,
                timed_out, forced, time.monotonic() - started, active_after, setup_error)
            _write_json(metadata, {**receipt, "argv": argv,
                                  "bootstrapPid": result.bootstrap_pid,
                                  "targetPid": result.target_pid,
                                  "targetExit": result.returncode,
                                  "bootstrapExit": result.bootstrap_returncode,
                                  "jobActiveAfter": active_after, "timedOut": timed_out,
                                  "forcedCleanup": forced,
                                  "elapsedSeconds": result.elapsed_seconds,
                                  "setupError": setup_error})
            return result
        finally:
            # Closing is a final kill-on-close backstop, never zero proof.
            if job is not None:
                job.close()
            if child is not None and child.returncode is not None:
                child._handle.Close()
            if request_path is not None:
                request_path.unlink(missing_ok=True)


if __name__ == "__main__":
    if os.name != "nt" or len(sys.argv) != 5 or sys.argv[1] != "--bootstrap":
        raise SystemExit("internal Windows bootstrap only")
    raise SystemExit(_bootstrap(int(sys.argv[2]), Path(sys.argv[3]), Path(sys.argv[4])))

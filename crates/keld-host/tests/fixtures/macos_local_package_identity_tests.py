#!/usr/bin/env python3
"""Native SDK oracle for GH-1404's Python process birth/ownership census."""
import ctypes, pathlib, subprocess, sys, tempfile, time
from dataclasses import replace
from unittest import TestCase
from unittest.mock import patch
import macos_local_package_acceptance as smoke
C = r"""
#include <libproc.h>
#include <sys/proc_info.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc, char **argv) { if (argc != 2) return 2;
  int pid = atoi(argv[1]); struct proc_bsdinfo b = {0}; char path[PROC_PIDPATHINFO_MAXSIZE] = {0};
  if (proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &b, sizeof b) != sizeof b ||
      proc_pidpath(pid, path, sizeof path) <= 0) return 3;
  printf("%zu %zu %zu %zu %zu %zu %u %u %u %llu %llu %s\n", sizeof b,
      offsetof(struct proc_bsdinfo, pbi_pid), offsetof(struct proc_bsdinfo, pbi_ppid),
      offsetof(struct proc_bsdinfo, pbi_uid), offsetof(struct proc_bsdinfo, pbi_start_tvsec),
      offsetof(struct proc_bsdinfo, pbi_start_tvusec), b.pbi_pid, b.pbi_ppid, b.pbi_uid,
      (unsigned long long)b.pbi_start_tvsec, (unsigned long long)b.pbi_start_tvusec, path);
}
"""
def main() -> None:
    assert sys.platform == "darwin", "test requires the native macOS SDK"
    with patch.object(smoke.os, "kill", side_effect=PermissionError("protected process")):
        assert not smoke.definitely_exited(1), "permission denial is not proof of exit"
    with tempfile.TemporaryDirectory(prefix="gh1404-identity-") as scratch:
        source, binary = pathlib.Path(scratch) / "identity.c", pathlib.Path(scratch) / "identity"
        source.write_text(C)
        subprocess.run(["/usr/bin/xcrun", "--sdk", "macosx", "clang", "-Wall", "-Wextra",
                        "-Werror", str(source), "-o", str(binary)], check=True, timeout=30)
        child = subprocess.Popen([sys.executable, "-c",
            'import os,sys; print("READY",flush=True); sys.stdin.buffer.read(1);'
            ' os.execv("/bin/cat", ["/bin/cat"])'],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        try:
            assert child.stdout and child.stdout.readline() == b"READY\n"
            result = subprocess.run([str(binary), str(child.pid)], capture_output=True,
                text=True, check=True, timeout=10).stdout.strip().split(maxsplit=11)
            size, *numbers, path = result
            offsets = tuple(map(int, numbers[:5])); pid, ppid, uid, sec, usec = map(int, numbers[5:])
            assert ctypes.sizeof(smoke.ProcBsdInfo) == int(size) and tuple(
                getattr(smoke.ProcBsdInfo, key).offset for key in
                ("pbi_pid", "pbi_ppid", "pbi_uid", "pbi_start_tvsec", "pbi_start_tvusec")) == offsets
            assert (pid, ppid, uid, sec, usec, path) == smoke.native_process_identity(child.pid)
            fact = smoke.ProcessFact(pid, ppid, f"{sec}.{usec:06d}", path, False, uid)
            assert smoke.current_exact_role(fact)
            old_seconds = subprocess.check_output([
                "/bin/ps", "-p", str(pid), "-o", "lstart="], text=True, timeout=5).strip()
            assert int(time.mktime(time.strptime(old_seconds, "%a %b %d %H:%M:%S %Y"))) == sec
            assert usec != 0 and sec * 1_000_000 != sec * 1_000_000 + usec
            for changed in (replace(fact, start=f"{sec + 1}.{usec:06d}"),
                            replace(fact, start=f"{sec}.{(usec + 1) % 1_000_000:06d}"),
                            replace(fact, uid=uid + 1), replace(fact, ppid=ppid + 1),
                            replace(fact, executable=path + "-substituted")):
                assert not smoke.current_exact_role(changed), changed
            with patch.object(smoke, "executable_of", return_value=None):
                with TestCase().assertRaises(RuntimeError):
                    smoke.current_exact_role(fact)
            # A real exec replaces the image in the *same* PID and birth. An
            # aggregate scanner returning [] must not make that PID "retired".
            assert child.stdin
            child.stdin.write(b"x"); child.stdin.flush()
            deadline = time.monotonic() + 5
            while smoke.executable_of(pid) != "/bin/cat" and time.monotonic() < deadline:
                time.sleep(0.01)
            assert smoke.executable_of(pid) == "/bin/cat", "child did not exec /bin/cat"
            assert not smoke.current_exact_role(fact)
            assert smoke.current_birth_alive(replace(fact, ppid=ppid + 1,
                executable="/different/image")), "birth observation must ignore reparent/exec"
            evidence = type("Evidence", (), {"record": lambda self, **row: None})()
            with patch.object(smoke, "installed_processes", return_value=[]):
                with patch.object(smoke, "PROCESS_TIMEOUT", 0.15):
                    with TestCase().assertRaises(RuntimeError):
                        smoke.wait_roles_gone(pathlib.Path(sys.executable),
                            pathlib.Path("/bin/cat"), [fact], evidence, "same-birth-exec")
            old_row = [(pid, ppid, old_seconds,
                        "keld-host " + str(pathlib.Path(sys.executable).resolve()))]
            with patch.object(smoke, "process_rows", return_value=old_row):
                with patch.object(smoke, "native_process_identity",
                                  side_effect=RuntimeError("injected unreadable process")):
                    with TestCase().assertRaisesRegex(RuntimeError, "unknown identity"):
                        smoke.installed_processes(pathlib.Path(sys.executable), pathlib.Path("/bin/cat"))
            assert smoke.current_birth_alive(fact)
            cleanup_rows = []
            cleanup_evidence = type("CleanupEvidence", (), {
                "owned_roles": [fact],
                "record": lambda self, **row: cleanup_rows.append(row),
            })()
            # Emergency cleanup must neither signal a changed image nor claim
            # that the same still-live birth retired. The clock only bounds the
            # harness wait; the actual SDK process identity remains the oracle.
            with patch.object(smoke.time, "monotonic", side_effect=[0, 1, 5, 6, 7, 11]):
                with patch.object(smoke.time, "sleep", return_value=None):
                    with patch.object(smoke.os, "kill") as kill:
                        smoke.cleanup_verified_roles(cleanup_evidence, pathlib.Path(path).parent)
                        kill.assert_not_called()
            assert not any(row["step"] == "failure-cleanup-retired" for row in cleanup_rows), cleanup_rows
            assert any(row["step"] == "failure-cleanup-blocked" for row in cleanup_rows), cleanup_rows
            print(f"PASS C SDK layout={size} offsets={offsets} child={pid} uid={uid} "
                f"ppid={ppid} birth={sec}.{usec:06d} old-ps-seconds={old_seconds}")
        finally:
            assert child.stdin
            child.stdin.close()
            child.wait(timeout=5)
        assert not smoke.current_birth_alive(fact)
        # ps can finish before proc_pidinfo: ESRCH proves the candidate exited.
        assert smoke.definitely_exited(pid)
        old_row = [(pid, ppid, old_seconds,
                    "keld-host " + str(pathlib.Path(sys.executable).resolve()))]
        with patch.object(smoke, "process_rows", return_value=old_row):
            assert smoke.installed_processes(pathlib.Path(sys.executable), pathlib.Path("/bin/cat")) == []
        retired_rows = []
        retired_evidence = type("RetiredEvidence", (), {
            "record": lambda self, **row: retired_rows.append(row),
        })()
        with patch.object(smoke, "installed_processes", side_effect=[
            RuntimeError("transient retiring process identity"), []
        ]) as census:
            with patch.object(smoke.time, "sleep", return_value=None):
                smoke.wait_roles_gone(pathlib.Path(sys.executable), pathlib.Path("/bin/cat"),
                                      [fact], retired_evidence, "bounded-unknown")
            assert census.call_count == 2, "unknown observation must not satisfy retirement"
        assert [row["step"] for row in retired_rows] == [
            "bounded-unknown-native-identity-unknown", "bounded-unknown-native-processes-retired"
        ], retired_rows
        retired_rows.clear()
        with patch.object(smoke, "installed_processes", side_effect=RuntimeError("persistent unknown")):
            with patch.object(smoke.time, "monotonic", side_effect=[0, 1, 100]):
                with patch.object(smoke.time, "sleep", return_value=None):
                    with TestCase().assertRaises(RuntimeError):
                        smoke.wait_roles_gone(pathlib.Path(sys.executable), pathlib.Path("/bin/cat"),
                                              [fact], retired_evidence, "persistent-unknown")
        assert not any(row["step"].endswith("native-processes-retired") for row in retired_rows)
if __name__ == "__main__": main()

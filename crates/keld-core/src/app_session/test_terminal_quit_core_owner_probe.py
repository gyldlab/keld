"""Regression controls for the review-only projection runner."""
import os
import signal
import subprocess
import sys
import tempfile
import unittest

from terminal_quit_core_owner_probe import run_bounded


class ProjectionRunnerTests(unittest.TestCase):
    def run_isolated(self, mode):
        """Own a separate watchdog even if run_bounded loses its own deadline."""
        with tempfile.TemporaryFile() as diagnostics:
            child = subprocess.Popen(
                [sys.executable, "-B", __file__, "--probe-child", mode],
                start_new_session=True, stdout=diagnostics,
                stderr=subprocess.STDOUT,
            )
            try:
                status = child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                # The outer child still owns any inner process-group leader.
                # Kill its directly owned groups before reaping the outer child.
                try:
                    census = subprocess.run(
                        ["/bin/ps", "-axo", "pid=,ppid=,pgid=,stat="],
                        capture_output=True, text=True, check=True, timeout=5,
                    )
                    groups = set()
                    for row in census.stdout.splitlines():
                        pid, ppid, pgid, state = row.split(maxsplit=3)
                        if int(ppid) == child.pid and int(pgid) != child.pid:
                            groups.add(int(pgid))
                    for group in groups:
                        try:
                            os.killpg(group, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                finally:
                    try:
                        os.killpg(child.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    child.wait()
                self.fail(f"outer projection watchdog expired in {mode}")
            diagnostics.seek(0)
            output = diagnostics.read().decode("utf-8", errors="replace")
        self.assertEqual(status, 0, output)
        return output

    def test_stalled_child_fails_with_diagnostics(self):
        self.assertIn("STALLED_PROJECTION_REJECTED", self.run_isolated("stall"))

    def test_surviving_descendant_cannot_pass(self):
        self.assertIn("LIVE_DESCENDANT_REJECTED", self.run_isolated("survivor"))

    def test_normal_exit_and_negative_result_are_preserved(self):
        for status in (0, 101):
            result = run_bounded(
                [sys.executable, "-c", f"print('projection output'); raise SystemExit({status})"],
                "projection", 5,
            )
            self.assertEqual(result.returncode, status)
            self.assertEqual(result.stdout, "projection output\n")

    def test_compile_failure_is_not_accepted(self):
        with self.assertRaises(subprocess.CalledProcessError) as caught:
            run_bounded(
                [sys.executable, "-c", "raise SystemExit(1)"], "compile", 5, check=True,
            )
        self.assertEqual(caught.exception.returncode, 1)


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--probe-child":
        mode = sys.argv[2]
        if mode == "stall":
            command = [
                sys.executable, "-c",
                "import threading; print('projection started', flush=True); threading.Event().wait()",
            ]
            expected = "timed out"
            marker = "STALLED_PROJECTION_REJECTED"
            timeout = 1
        elif mode == "survivor":
            command = [
                sys.executable, "-c",
                "import subprocess,sys; "
                "subprocess.Popen([sys.executable, '-c', "
                "'import threading; threading.Event().wait()']); "
                "print('leader exited', flush=True)",
            ]
            expected = "live group members"
            marker = "LIVE_DESCENDANT_REJECTED"
            timeout = 5
        else:
            raise ValueError(f"unknown isolated probe {mode}")
        try:
            run_bounded(command, mode, timeout)
        except RuntimeError as error:
            assert expected in str(error), error
            assert ("projection started" if mode == "stall" else "leader exited") in str(error)
            print(marker, flush=True)
        else:
            raise AssertionError(f"{mode}: expected a bounded failure")
    else:
        unittest.main()

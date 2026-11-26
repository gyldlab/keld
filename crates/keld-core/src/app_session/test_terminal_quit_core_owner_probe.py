"""Regression controls for the review-only projection runner."""
import subprocess
import sys
import unittest

from terminal_quit_core_owner_probe import run_bounded


class ProjectionRunnerTests(unittest.TestCase):
    def test_stalled_child_fails_with_diagnostics(self):
        command = [
            sys.executable, "-c",
            "import threading; print('projection started', flush=True); threading.Event().wait()",
        ]
        with self.assertRaisesRegex(RuntimeError, r"stalled projection: timed out") as caught:
            run_bounded(command, "stalled projection", 2)
        self.assertIn("projection started", str(caught.exception))

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
    unittest.main()

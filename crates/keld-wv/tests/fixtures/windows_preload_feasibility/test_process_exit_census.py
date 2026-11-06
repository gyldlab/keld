"""Controlled Python children test wrapper bookkeeping, never Windows cleanup."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import run_native_matrix as matrix

CHILD = """
import json
from pathlib import Path
import sys
config = json.loads(Path(sys.argv[-1]).read_text())
mode = config["testChildMode"]
out = Path(config["outDir"])
if mode != "missing":
    out.mkdir()
    (out / "result.json").write_text("{" if mode == "malformed" else "{}")
sys.exit(1 if mode in ("missing", "unexpected-exit") else 0)
"""


class ConfirmedExitCensus(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="w1-confirmed-exit-")
        self.addCleanup(self.temporary.cleanup)
        self.case = Path(self.temporary.name)
        self.child = self.case / "controlled_child.py"
        self.child.write_text(CHILD, encoding="utf-8")
        self.real_popen = subprocess.Popen

    def child_process(self, command, **kwargs):
        return self.real_popen(
            [sys.executable, "-B", str(self.child), *command[1:]], **kwargs
        )

    def census_entry(self, case, profile):
        # This marker proves the wrapper called its independent census boundary.
        # It does not represent an OS observation or a native cleanup result.
        matrix.write_json(case / "post-exit-process-census.json", {
            "fixtureOnly": True, "profile": str(profile), "boundaryEntered": True
        })

    def run_child(self, mode, census=None):
        with patch.object(matrix.subprocess, "Popen", self.child_process):
            with patch.object(matrix, "observe_resources", census or self.census_entry):
                return matrix.launch(sys.executable, self.case, {
                    "outDir": str(self.case / "native"), "testChildMode": mode
                })

    def assert_failed_exit_was_censused(self, mode, exception):
        with self.assertRaises(exception):
            self.run_child(mode)
        self.assertTrue((self.case / "process.json").is_file())
        self.assertTrue((self.case / "post-exit-process-census.json").is_file())

    def test_missing_result_still_enters_census(self):
        self.assert_failed_exit_was_censused("missing", RuntimeError)

    def test_malformed_result_still_enters_census(self):
        self.assert_failed_exit_was_censused("malformed", ValueError)

    def test_unexpected_nonzero_exit_still_enters_census(self):
        self.assert_failed_exit_was_censused("unexpected-exit", RuntimeError)

    def test_original_and_census_errors_both_survive(self):
        def failed_census(case, profile):
            self.census_entry(case, profile)
            raise RuntimeError("controlled census failure")
        with self.assertRaisesRegex(RuntimeError, "Native result missing"):
            self.run_child("missing", failed_census)
        errors = json.loads((self.case / "exit-failures.json").read_text())
        self.assertIn("Native result missing", errors["native"]["message"])
        self.assertEqual(errors["census"]["message"], "controlled census failure")

    def test_healthy_child_is_censused_and_returns_result(self):
        self.assertEqual(self.run_child("healthy"), {})
        self.assertTrue((self.case / "post-exit-process-census.json").is_file())

    def test_unconfirmed_exit_keeps_quarantine_without_census(self):
        class StillRunning:
            pid = 123456
            def wait(self, timeout):
                raise subprocess.TimeoutExpired("controlled-not-launched", timeout)
        with patch.object(matrix.subprocess, "Popen", return_value=StillRunning()):
            with patch.object(matrix, "observe_resources") as census:
                with self.assertRaisesRegex(RuntimeError, "no cancellation inferred"):
                    matrix.launch(sys.executable, self.case, {
                        "outDir": str(self.case / "native")
                    })
                census.assert_not_called()
        self.assertTrue((self.case / "outer-timeout.json").is_file())
        self.assertFalse((self.case / "process.json").exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)

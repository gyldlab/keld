"""Focused source Git argv and exclusive receipt output tests."""
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
import json
from pathlib import Path
import sys
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch

import gh1404_b3_candidate_integrity as verifier


class BoundaryTests(unittest.TestCase):
    def test_git_runs_with_global_no_replace_switch(self):
        with TemporaryDirectory() as directory, patch.object(verifier.subprocess, "run") as run:
            verifier._git_run(Path(directory).resolve(strict=True), "rev-parse", "HEAD", text=True)
        command = run.call_args.args[0]
        self.assertEqual(command[:2], ["git", "--no-replace-objects"])
        self.assertEqual(command[-2:], ["rev-parse", "HEAD"])

    def test_receipt_creates_once_and_preserves_existing_output(self):
        with TemporaryDirectory() as directory:
            output = Path(directory) / "receipt.json"
            result, stdout, stderr = self.invoke(output)
            self.assertEqual((result, stderr), (0, ""))
            self.assertEqual(json.loads(stdout), {"result": "source only"})
            before = output.read_bytes()
            self.assertEqual(before.decode("utf8"), stdout)
            result, stdout, stderr = self.invoke(output)
            self.assertEqual((result, stdout), (2, ""))
            self.assertTrue(stderr.startswith("B3_CANDIDATE_REFUSED: "))
            self.assertEqual(output.read_bytes(), before)

    def test_missing_parent_returns_structured_error_without_success(self):
        with TemporaryDirectory() as directory:
            output = Path(directory) / "absent" / "receipt.json"
            result, stdout, stderr = self.invoke(output)
            self.assertEqual((result, stdout), (2, ""))
            self.assertTrue(stderr.startswith("B3_CANDIDATE_REFUSED: "))
            self.assertFalse(output.exists())

    def invoke(self, output):
        argv = ["checker", "--source-root", ".", "--selected-revision", "HEAD",
                "--candidate-root", ".", "--repro-dir", ".", "--output", str(output)]
        stdout, stderr = StringIO(), StringIO()
        with (patch.object(sys, "argv", argv),
              patch.object(verifier, "validate", return_value={"result": "source only"}),
              redirect_stdout(stdout), redirect_stderr(stderr)):
            result = verifier.main()
        return result, stdout.getvalue(), stderr.getvalue()

"""Subprocess admission controls for the first-Init proof driver."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


DRIVER = Path(__file__).with_name("verify_init_seam.py")
REFUSAL = "optimized Python disables proof validation"


class ProofAdmissionTests(unittest.TestCase):
    def invoke(self, root, flags=(), optimize=None, output="proof-output"):
        env = os.environ.copy()
        env.pop("PYTHONOPTIMIZE", None)
        if optimize is not None:
            env["PYTHONOPTIMIZE"] = optimize
        return subprocess.run(
            [sys.executable, *flags, str(DRIVER),
             "--repo", str(root), "--source-ref", self.blob,
             "--expected-source-sha256", "0" * 64,
             "--patch", str(root / "absent.patch"),
             "--red-patch", str(root / "absent-red.patch"),
             "--harness", str(root / "absent.rs"),
             "--out", str(root / output)],
            env=env, capture_output=True, text=True, timeout=15,
        )

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        subprocess.run(["git", "init", "--quiet", str(self.root)],
                       check=True, capture_output=True, timeout=15)
        self.blob = subprocess.check_output(
            ["git", "hash-object", "-w", "--stdin"], cwd=self.root,
            input=b"proof source pin control\n", timeout=15,
        ).decode().strip()

    def test_optimized_modes_refuse_before_proof_output(self):
        for flags, optimize in ((["-O"], None), (["-OO"], None),
                                ([], "1"), ([], "2")):
            with self.subTest(flags=flags, optimize=optimize):
                output = "proof-output-" + (flags[0] if flags else optimize)
                result = self.invoke(self.root, flags, optimize, output)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn(REFUSAL, result.stderr)
                self.assertEqual(result.stdout, "")
                self.assertFalse((self.root / output).exists())

    def test_normal_mode_still_rejects_wrong_source_pin(self):
        result = self.invoke(self.root)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("source bytes do not match the reviewed pin", result.stderr)
        self.assertNotIn(REFUSAL, result.stderr)
        self.assertFalse((self.root / "proof-output").exists())

    def test_normal_mode_still_accepts_help(self):
        env = os.environ.copy()
        env.pop("PYTHONOPTIMIZE", None)
        result = subprocess.run([sys.executable, str(DRIVER), "--help"],
                                env=env, capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--expected-source-sha256", result.stdout)


if __name__ == "__main__":
    unittest.main()

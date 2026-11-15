"""Independent byte mutations for the GH499 patch-custody preflight."""

import difflib
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch as mock_patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
from drawio_patch_admission import (  # noqa: E402
    AdmissionError,
    MAX_SOURCE_BYTES,
    PatchHashes,
    _read_regular_file,
    verify_patch_chain,
)


def patch(before: bytes, after: bytes) -> bytes:
    return "".join(
        difflib.unified_diff(
            before.decode().splitlines(keepends=True),
            after.decode().splitlines(keepends=True),
            fromfile="a/src/main/electron.js",
            tofile="b/src/main/electron.js",
        )
    ).encode()


def digest(blob: bytes) -> str:
    return hashlib.sha256(blob).hexdigest()


class PatchAdmissionTests(unittest.TestCase):
    def setUp(self):
        self.original = b"const one = 1;\nconst two = 2;\n"
        self.candidate = b"const one = 1;\nconst two = 3;\n"
        self.forward = patch(self.original, self.candidate)
        self.reverse = patch(self.candidate, self.original)
        self.expected = self.pins()

    def pins(self):
        return PatchHashes(
            digest(self.original),
            digest(self.candidate),
            digest(self.forward),
            digest(self.reverse),
        )

    def check(self):
        verify_patch_chain(
            self.original, self.candidate, self.forward, self.reverse, self.expected
        )

    def test_exact_forward_and_independent_reverse_are_admitted(self):
        self.check()

    def test_one_byte_wrong_original_is_refused(self):
        self.original = b"const one = 0;\nconst two = 2;\n"
        with self.assertRaisesRegex(AdmissionError, "original SHA-256 mismatch"):
            self.check()

    def test_one_byte_wrong_candidate_is_refused(self):
        self.candidate = b"const one = 1;\nconst two = 4;\n"
        with self.assertRaisesRegex(AdmissionError, "candidate SHA-256 mismatch"):
            self.check()

    def test_one_byte_wrong_forward_patch_is_refused(self):
        self.forward += b" "
        with self.assertRaisesRegex(AdmissionError, "forward SHA-256 mismatch"):
            self.check()

    def test_one_byte_wrong_reverse_patch_is_refused(self):
        self.reverse += b" "
        with self.assertRaisesRegex(AdmissionError, "reverse SHA-256 mismatch"):
            self.check()

    def test_valid_hashes_cannot_hide_second_modified_file(self):
        other = (
            b"--- a/src/main/other.js\n"
            b"+++ b/src/main/other.js\n"
            b"@@ -1 +1 @@\n-old\n+new\n"
        )
        self.forward += other
        self.expected = self.pins()
        with self.assertRaisesRegex(AdmissionError, "exactly one tracked source"):
            self.check()

    def test_valid_hashes_cannot_hide_wrong_source_file(self):
        self.forward = self.forward.replace(
            b"src/main/electron.js", b"src/main/wrong.js"
        )
        self.expected = self.pins()
        with self.assertRaisesRegex(AdmissionError, "unapproved path"):
            self.check()

    def test_candidate_digest_alone_cannot_replace_forward_replay(self):
        # All four inputs are individually accepted, but the named patch does
        # not turn this original into the claimed candidate.
        self.candidate = b"const one = 1;\nconst two = 4;\n"
        self.expected = self.pins()
        with self.assertRaisesRegex(AdmissionError, "did not produce"):
            self.check()

    def test_reverse_must_recover_original_independently(self):
        self.reverse = patch(self.candidate, b"const one = 1;\nconst two = 8;\n")
        self.expected = self.pins()
        with self.assertRaisesRegex(AdmissionError, "did not recover"):
            self.check()

    def test_input_symlink_and_oversize_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            data = root / "source.js"
            linked = root / "linked.js"
            data.write_bytes(b"test")
            linked.symlink_to(data)
            with self.assertRaisesRegex(AdmissionError, "linked input"):
                _read_regular_file(linked, MAX_SOURCE_BYTES)
            data.write_bytes(b"x" * (MAX_SOURCE_BYTES + 1))
            with self.assertRaisesRegex(AdmissionError, "exceeds"):
                _read_regular_file(data, MAX_SOURCE_BYTES)

    def test_public_cli_refuses_self_consistent_unapproved_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            values = {
                "original": self.original,
                "candidate": self.candidate,
                "forward": self.forward,
                "reverse": self.reverse,
            }
            args = [sys.executable, "-B", str(Path(__file__).with_name("drawio_patch_admission.py"))]
            for name, data in values.items():
                path = root / name
                path.write_bytes(data)
                args.extend((f"--{name}", str(path)))
            result = subprocess.run(
                args, check=False, capture_output=True, text=True, timeout=10
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("original SHA-256 mismatch", result.stderr)
            self.assertNotIn("verified", result.stdout)

    def test_inherited_git_worktree_cannot_redirect_patch_writes(self):
        with tempfile.TemporaryDirectory() as directory:
            outside = Path(directory)
            source = outside / "src/main/electron.js"
            source.parent.mkdir(parents=True)
            source.write_bytes(self.original)
            subprocess.run(
                ["git", "init", "-q", str(outside)], check=True, capture_output=True
            )
            with mock_patch.dict(
                os.environ,
                {
                    "GIT_DIR": str(outside / ".git"),
                    "GIT_WORK_TREE": str(outside),
                },
            ):
                self.check()
            self.assertEqual(source.read_bytes(), self.original)


if __name__ == "__main__":
    unittest.main()

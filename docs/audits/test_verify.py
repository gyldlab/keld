#!/usr/bin/env python3
"""Negative controls for the public-audit verifier."""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
MANIFEST = "2026-09-19-full-spectrum-technical-audit.json"
REPORT = "2026-09-19-full-spectrum-technical-audit.md"


class AuditVerifyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="keld-audit-verify-")
        self.repo = Path(self.temp.name) / "repo"
        (self.repo / "docs").mkdir(parents=True)
        shutil.copy2(REPO / "README.md", self.repo / "README.md")
        shutil.copytree(HERE, self.repo / "docs" / "audits", ignore=shutil.ignore_patterns("__pycache__"))

    def tearDown(self) -> None:
        self.temp.cleanup()
    @property
    def audits(self) -> Path:
        return self.repo / "docs" / "audits"

    @property
    def manifest_path(self) -> Path:
        return self.audits / "evidence" / MANIFEST

    @property
    def report_path(self) -> Path:
        return self.audits / REPORT

    def manifest(self) -> dict:
        return json.loads(self.manifest_path.read_text())

    def write_manifest(self, data: dict) -> None:
        self.manifest_path.write_text(json.dumps(data, indent=2) + "\n")

    def verify(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(self.audits / "verify.py")],
            cwd=self.repo,
            text=True,
            capture_output=True,
            check=False,
        )

    def assert_rejected(self, needle: str) -> None:
        result = self.verify()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(needle, result.stderr)
    def test_current_fixture_passes(self) -> None:
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_invalid_confidence_is_rejected(self) -> None:
        data = self.manifest()
        data["findings"][4]["confidence"] = "confirmed-exposure"
        self.write_manifest(data)
        self.assert_rejected("invalid confidence")

    def test_silent_report_rewrite_is_rejected_even_with_new_current_hash(self) -> None:
        self.report_path.write_text(self.report_path.read_text() + "\nsilent rewrite\n")
        data = self.manifest()
        data["report_sha256"] = hashlib.sha256(self.report_path.read_bytes()).hexdigest()
        self.write_manifest(data)
        self.assert_rejected("changed report requires correction metadata")

    def test_unlisted_manifest_is_rejected(self) -> None:
        shutil.copy2(self.manifest_path, self.manifest_path.with_name("unlisted.json"))
        self.assert_rejected("manifest is not listed in registry")

    def test_malformed_registry_row_is_rejected(self) -> None:
        registry = self.audits / "README.md"
        text = registry.read_text()
        marker = "| Published |\n"
        registry.write_text(text.replace(marker, marker + "| malformed |\n", 1))
        self.assert_rejected("malformed published audit row")
    def test_public_only_false_is_rejected(self) -> None:
        data = self.manifest()
        data["public_only"] = False
        self.write_manifest(data)
        self.assert_rejected("public_only must be true")

    def test_verification_count_drift_is_rejected(self) -> None:
        data = self.manifest()
        data["verification"]["rust_library_tests"]["passed"] -= 1
        self.write_manifest(data)
        self.assert_rejected("verification count differs from report")

    def test_stale_registry_current_hash_is_rejected(self) -> None:
        registry = self.audits / "README.md"
        current = self.manifest()["report_sha256"]
        old = f"Current report: `{REPORT}` · SHA-256 `{current}`."
        new = f"Current report: `{REPORT}` · SHA-256 `{'0' * 64}`."
        registry.write_text(registry.read_text().replace(old, new, 1))
        self.assert_rejected("registry current report hash is stale")

    def test_upstream_receipt_hash_drift_is_rejected(self) -> None:
        receipt = self.audits / "evidence" / "upstream" / "2026-09-19-platform-semantics.json"
        receipt.write_text(receipt.read_text() + " ")
        self.assert_rejected("upstream receipt hash mismatch")

    def test_evidence_root_drift_is_rejected(self) -> None:
        data = self.manifest()
        data["public_evidence_roots"].append("https://example.com/mutable")
        self.write_manifest(data)
        self.assert_rejected("unexpected public evidence root")
    def test_private_data_patterns_are_rejected(self) -> None:
        probes = [
            "/Users/alice/private.txt",
            "person@example.com",
            "github_pat_1234567890abcdef",
            "http://127.0.0.1:9000/private",
        ]
        original = self.manifest()
        for probe in probes:
            with self.subTest(probe=probe):
                data = json.loads(json.dumps(original))
                data["notes"].append(probe)
                self.write_manifest(data)
                self.assert_rejected("private-data pattern leaked")
        self.write_manifest(original)


if __name__ == "__main__":
    unittest.main()

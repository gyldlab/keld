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
import zlib
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
        shutil.copytree(
            HERE,
            self.repo / "docs" / "audits",
            ignore=shutil.ignore_patterns("__pycache__"),
        )

        report_path = self.repo / "docs" / "audits" / REPORT
        manifest_path = self.repo / "docs" / "audits" / "evidence" / MANIFEST
        registry = self.repo / "docs" / "audits" / "README.md"
        current_report = report_path.read_bytes()
        current_manifest = manifest_path.read_bytes()
        current_registry = registry.read_text()
        current_data = json.loads(current_manifest)
        real_anchor = current_data["publication_commit"]

        original_report = subprocess.check_output(
            ["git", "-C", str(REPO), "show", f"{real_anchor}:docs/audits/{REPORT}"]
        )
        original_manifest = subprocess.check_output(
            [
                "git",
                "-C",
                str(REPO),
                "show",
                f"{real_anchor}:docs/audits/evidence/{MANIFEST}",
            ]
        )
        report_path.write_bytes(original_report)
        manifest_path.write_bytes(original_manifest)

        subprocess.run(["git", "init", "-q"], cwd=self.repo, check=True)
        subprocess.run(["git", "config", "user.name", "audit-test"], cwd=self.repo, check=True)
        subprocess.run(
            ["git", "config", "user.email", "audit-test@example.invalid"],
            cwd=self.repo,
            check=True,
        )
        subprocess.run(["git", "add", "."], cwd=self.repo, check=True)
        subprocess.run(["git", "commit", "-qm", "snapshot"], cwd=self.repo, check=True)
        anchor = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=self.repo, text=True
        ).strip()

        report_path.write_bytes(current_report)
        manifest_path.write_bytes(current_manifest)
        registry.write_text(current_registry)
        data = json.loads(current_manifest)
        audited_keld_revision = data["audited_revisions"]["gyldlab/keld"]
        subprocess.run(
            ["git", "fetch", "-q", str(REPO), audited_keld_revision],
            cwd=self.repo,
            check=True,
        )
        subprocess.run(
            ["git", "replace", "--graft", anchor, audited_keld_revision],
            cwd=self.repo,
            check=True,
        )

        old_anchor = data["publication_commit"]
        old_current_manifest = hashlib.sha256(current_manifest).hexdigest()
        data["publication_commit"] = anchor
        data["original_manifest_sha256"] = hashlib.sha256(original_manifest).hexdigest()
        manifest_path.write_text(json.dumps(data, indent=2) + "\n")
        new_current_manifest = hashlib.sha256(manifest_path.read_bytes()).hexdigest()

        registry_text = registry.read_text()
        registry_text = registry_text.replace(old_anchor, anchor).replace(
            old_anchor[:12], anchor[:12]
        )
        registry_text = registry_text.replace(old_current_manifest, new_current_manifest, 1)
        registry.write_text(registry_text)

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

    def refresh_current_manifest_registry(self, old_hash: str) -> str:
        new_hash = hashlib.sha256(self.manifest_path.read_bytes()).hexdigest()
        registry = self.audits / "README.md"
        old = f"Current manifest: `evidence/{MANIFEST}` · SHA-256 `{old_hash}`."
        new = f"Current manifest: `evidence/{MANIFEST}` · SHA-256 `{new_hash}`."
        text = registry.read_text()
        if old not in text:
            raise AssertionError("current manifest registry anchor missing")
        registry.write_text(text.replace(old, new, 1))
        return new_hash

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

    def test_manifest_finding_classification_must_match_report(self) -> None:
        old_manifest = hashlib.sha256(self.manifest_path.read_bytes()).hexdigest()
        data = self.manifest()
        data["findings"][0]["severity"] = "S4"
        self.write_manifest(data)
        self.refresh_current_manifest_registry(old_manifest)
        self.assert_rejected("finding classification differs from report: F-01")

    def test_silent_report_rewrite_is_rejected_even_with_new_current_hash(self) -> None:
        self.report_path.write_text(self.report_path.read_text() + "\nsilent rewrite\n")
        data = self.manifest()
        data["report_sha256"] = hashlib.sha256(self.report_path.read_bytes()).hexdigest()
        data["corrections"] = []
        self.write_manifest(data)
        self.assert_rejected("changed audit requires correction metadata")

    def test_unlisted_manifest_is_rejected(self) -> None:
        shutil.copy2(self.manifest_path, self.manifest_path.with_name("unlisted.json"))
        self.assert_rejected("manifest is not listed in registry")

    def test_malformed_registry_row_is_rejected(self) -> None:
        registry = self.audits / "README.md"
        text = registry.read_text()
        marker = "| Published |\n"
        registry.write_text(text.replace(marker, marker + "| malformed |\n", 1))
        self.assert_rejected("malformed published audit row")

    def test_indented_registry_row_is_rejected(self) -> None:
        registry = self.audits / "README.md"
        text = registry.read_text()
        row = next(line for line in text.splitlines() if line.startswith("| 2026-09-19 |"))
        registry.write_text(text.replace(row, row + "\n  " + row, 1))
        self.assert_rejected("indented table row")

    def test_duplicate_report_row_is_rejected(self) -> None:
        registry = self.audits / "README.md"
        text = registry.read_text()
        row = next(line for line in text.splitlines() if line.startswith("| 2026-09-19 |"))
        duplicate = row.replace(
            "evidence/2026-09-19-full-spectrum-technical-audit.json",
            "evidence/duplicate-report.json",
        )
        registry.write_text(text.replace(row, row + "\n" + duplicate, 1))
        shutil.copy2(
            self.manifest_path,
            self.audits / "evidence" / "duplicate-report.json",
        )
        self.assert_rejected("duplicate report row")
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

    def test_stale_registry_current_manifest_hash_is_rejected(self) -> None:
        data = self.manifest()
        data["notes"].append("structural drift")
        self.write_manifest(data)
        self.assert_rejected("registry current manifest hash is stale")

    def test_semantic_manifest_only_change_requires_correction(self) -> None:
        old_manifest = hashlib.sha256(self.manifest_path.read_bytes()).hexdigest()
        data = self.manifest()
        old_original_report = data["original_report_sha256"]
        current_report = hashlib.sha256(self.report_path.read_bytes()).hexdigest()
        data["original_report_sha256"] = current_report
        data["notes"].append("semantic manifest drift")
        data["corrections"] = []
        self.write_manifest(data)

        registry = self.audits / "README.md"
        old = f"Original snapshot: `{REPORT}` · SHA-256 `{old_original_report}`."
        new = f"Original snapshot: `{REPORT}` · SHA-256 `{current_report}`."
        text = registry.read_text()
        if old not in text:
            raise AssertionError("original report registry anchor missing")
        registry.write_text(text.replace(old, new, 1))
        self.refresh_current_manifest_registry(old_manifest)
        self.assert_rejected("semantic manifest change requires correction metadata")

    def test_correction_date_must_be_iso_calendar_date(self) -> None:
        old_manifest = hashlib.sha256(self.manifest_path.read_bytes()).hexdigest()
        data = self.manifest()
        data["corrections"][-1]["date"] = "2026-99-99"
        self.write_manifest(data)
        self.refresh_current_manifest_registry(old_manifest)
        self.assert_rejected("correction 0 date is invalid")

    def test_fenced_errata_heading_is_not_reader_visible(self) -> None:
        old_manifest = hashlib.sha256(self.manifest_path.read_bytes()).hexdigest()
        data = self.manifest()
        data["corrections"][-1]["date"] = "2026-09-20"
        self.write_manifest(data)
        self.refresh_current_manifest_registry(old_manifest)

        old_report = hashlib.sha256(self.report_path.read_bytes()).hexdigest()
        text = self.report_path.read_text()
        text = text.replace(
            "### 2026-09-19 — Publication evidence hardening",
            "```markdown\n### 2026-09-20 — hidden correction\n```",
            1,
        )
        self.report_path.write_text(text)
        new_report = hashlib.sha256(self.report_path.read_bytes()).hexdigest()
        registry = self.audits / "README.md"
        current = registry.read_text()
        current = current.replace(
            f"Current report: `{REPORT}` · SHA-256 `{old_report}`.",
            f"Current report: `{REPORT}` · SHA-256 `{new_report}`.",
            1,
        )
        registry.write_text(current)
        self.assert_rejected("correction dates differ from Errata headings")

    def test_multiple_corrections_require_distinct_git_states(self) -> None:
        old_manifest = hashlib.sha256(self.manifest_path.read_bytes()).hexdigest()
        data = self.manifest()
        template = json.loads(json.dumps(data["corrections"][-1]))
        for day in ("20", "21"):
            correction = json.loads(json.dumps(template))
            correction["date"] = f"2026-09-{day}"
            correction["reason"] = f"Test correction {day}."
            data["corrections"].append(correction)
        self.write_manifest(data)
        self.refresh_current_manifest_registry(old_manifest)
        self.assert_rejected("multiple corrections introduced in current state")

    def test_correction_history_is_append_only(self) -> None:
        subprocess.run(["git", "add", "."], cwd=self.repo, check=True)
        subprocess.run(["git", "commit", "-qm", "corrected"], cwd=self.repo, check=True)

        old_manifest = hashlib.sha256(self.manifest_path.read_bytes()).hexdigest()
        data = self.manifest()
        data["corrections"] = []
        self.write_manifest(data)
        self.refresh_current_manifest_registry(old_manifest)
        self.assert_rejected("correction history is not append-only")

    def test_historical_correction_bindings_are_revalidated(self) -> None:
        old_manifest = hashlib.sha256(self.manifest_path.read_bytes()).hexdigest()
        data = self.manifest()
        data["corrections"][0]["report_sha256"] = "0" * 64
        self.write_manifest(data)
        self.refresh_current_manifest_registry(old_manifest)
        subprocess.run(["git", "add", "."], cwd=self.repo, check=True)
        subprocess.run(["git", "commit", "-qm", "bad-first-correction"], cwd=self.repo, check=True)

        old_report_hash = hashlib.sha256(self.report_path.read_bytes()).hexdigest()
        report_text = self.report_path.read_text()
        self.report_path.write_text(
            report_text + "\n### 2026-09-20 — second correction\n\nTest-only correction.\n"
        )
        new_report_hash = hashlib.sha256(self.report_path.read_bytes()).hexdigest()
        data = self.manifest()
        data["report_sha256"] = new_report_hash
        semantic_hash = data["corrections"][0]["manifest_semantic_sha256"]
        data["corrections"].append(
            {
                "date": "2026-09-20",
                "reason": "Test-only second correction.",
                "report_sha256": new_report_hash,
                "manifest_semantic_sha256": semantic_hash,
            }
        )
        old_manifest = hashlib.sha256(self.manifest_path.read_bytes()).hexdigest()
        self.write_manifest(data)

        registry = self.audits / "README.md"
        old = f"Current report: `{REPORT}` · SHA-256 `{old_report_hash}`."
        new = f"Current report: `{REPORT}` · SHA-256 `{new_report_hash}`."
        text = registry.read_text()
        if old not in text:
            raise AssertionError("current report registry anchor missing")
        registry.write_text(text.replace(old, new, 1))
        self.refresh_current_manifest_registry(old_manifest)
        self.assert_rejected("historical correction 0 report hash is invalid")

    def test_uncorrected_historical_report_rewrite_cannot_be_repaired_later(self) -> None:
        subprocess.run(["git", "add", "."], cwd=self.repo, check=True)
        subprocess.run(["git", "commit", "-qm", "valid-first-correction"], cwd=self.repo, check=True)

        self.report_path.write_text(
            self.report_path.read_text() + "\nuncorrected historical rewrite\n"
        )
        subprocess.run(["git", "add", str(self.report_path)], cwd=self.repo, check=True)
        subprocess.run(["git", "commit", "-qm", "bad-report-only-state"], cwd=self.repo, check=True)

        old_report_hash = self.manifest()["report_sha256"]
        report_text = self.report_path.read_text()
        report_text += "\n### 2026-09-20 — repair\n\nRepair the prior report state.\n"
        self.report_path.write_text(report_text)
        new_report_hash = hashlib.sha256(self.report_path.read_bytes()).hexdigest()

        old_manifest = hashlib.sha256(self.manifest_path.read_bytes()).hexdigest()
        data = self.manifest()
        data["report_sha256"] = new_report_hash
        semantic_hash = data["corrections"][-1]["manifest_semantic_sha256"]
        data["corrections"].append(
            {
                "date": "2026-09-20",
                "reason": "Repair prior report state.",
                "report_sha256": new_report_hash,
                "manifest_semantic_sha256": semantic_hash,
            }
        )
        self.write_manifest(data)

        registry = self.audits / "README.md"
        text = registry.read_text()
        old = f"Current report: `{REPORT}` · SHA-256 `{old_report_hash}`."
        new = f"Current report: `{REPORT}` · SHA-256 `{new_report_hash}`."
        if old not in text:
            raise AssertionError("current report registry anchor missing")
        registry.write_text(text.replace(old, new, 1))
        self.refresh_current_manifest_registry(old_manifest)
        subprocess.run(["git", "add", "."], cwd=self.repo, check=True)
        subprocess.run(["git", "commit", "-qm", "later-repair"], cwd=self.repo, check=True)

        self.assert_rejected("historical latest correction does not bind report state")

    def test_original_manifest_hash_cannot_be_rebased(self) -> None:
        data = self.manifest()
        original = data["original_manifest_sha256"]
        data["original_manifest_sha256"] = "0" * 64
        self.write_manifest(data)
        registry = self.audits / "README.md"
        old = f"Original manifest: `evidence/{MANIFEST}` · SHA-256 `{original}`."
        new = f"Original manifest: `evidence/{MANIFEST}` · SHA-256 `{'0' * 64}`."
        registry.write_text(registry.read_text().replace(old, new, 1))
        self.assert_rejected("original manifest hash differs from publication commit")

    def test_publication_anchor_cannot_move_to_later_commit(self) -> None:
        subprocess.run(["git", "add", "."], cwd=self.repo, check=True)
        subprocess.run(["git", "commit", "-qm", "later"], cwd=self.repo, check=True)
        later = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=self.repo, text=True
        ).strip()
        historical_manifest = subprocess.check_output(
            ["git", "show", f"{later}:docs/audits/evidence/{MANIFEST}"], cwd=self.repo
        )
        later_manifest_hash = hashlib.sha256(historical_manifest).hexdigest()

        data = self.manifest()
        old_anchor = data["publication_commit"]
        old_original_manifest = data["original_manifest_sha256"]
        old_current_manifest = hashlib.sha256(self.manifest_path.read_bytes()).hexdigest()
        data["publication_commit"] = later
        data["original_manifest_sha256"] = later_manifest_hash
        self.write_manifest(data)
        new_current_manifest = hashlib.sha256(self.manifest_path.read_bytes()).hexdigest()

        registry = self.audits / "README.md"
        text = registry.read_text()
        text = text.replace(old_anchor, later).replace(old_anchor[:12], later[:12])
        text = text.replace(old_current_manifest, new_current_manifest, 1)
        text = text.replace(old_original_manifest, later_manifest_hash, 1)
        registry.write_text(text)
        self.assert_rejected("publication_commit is not the report introduction commit")

    def test_original_hash_cannot_be_rebased_to_rewritten_files(self) -> None:
        data = self.manifest()
        original = data["original_report_sha256"]
        data["original_report_sha256"] = "0" * 64
        self.write_manifest(data)
        registry = self.audits / "README.md"
        old = f"Original snapshot: `{REPORT}` · SHA-256 `{original}`."
        new = f"Original snapshot: `{REPORT}` · SHA-256 `{'0' * 64}`."
        registry.write_text(registry.read_text().replace(old, new, 1))
        self.assert_rejected("original report hash differs from publication commit")

    def test_upstream_receipt_hash_drift_is_rejected(self) -> None:
        receipt = self.audits / "evidence" / "upstream" / "2026-09-19-platform-semantics.json"
        receipt.write_text(receipt.read_text() + " ")
        self.assert_rejected("upstream receipt hash mismatch")

    def test_empty_upstream_receipt_is_rejected(self) -> None:
        receipt = self.audits / "evidence" / "upstream" / "2026-09-19-platform-semantics.json"
        data = json.loads(receipt.read_text())
        data["receipts"] = []
        receipt.write_text(json.dumps(data, indent=2) + "\n")
        manifest = self.manifest()
        manifest["upstream_receipts"][0]["sha256"] = hashlib.sha256(receipt.read_bytes()).hexdigest()
        self.write_manifest(manifest)
        self.assert_rejected("upstream receipt must contain source entries")

    def test_evidence_root_drift_is_rejected(self) -> None:
        data = self.manifest()
        data["public_evidence_roots"].append("https://example.com/mutable")
        self.write_manifest(data)
        self.assert_rejected("unexpected public evidence root")

    def test_non_keld_revision_must_match_commit_receipt_object_id(self) -> None:
        data = self.manifest()
        fake = "1" * 40
        data["audited_revisions"]["gyldlab/keld-benches"] = fake
        receipt = next(
            item for item in data["repository_commit_receipts"]
            if item["repository"] == "gyldlab/keld-benches"
        )
        receipt["revision"] = fake
        self.write_manifest(data)
        self.assert_rejected("repository receipt object id mismatch")

    def test_audited_keld_revision_must_precede_publication_commit(self) -> None:
        data = self.manifest()
        benches_revision = data["audited_revisions"]["gyldlab/keld-benches"]
        benches_receipt = next(
            item for item in data["repository_commit_receipts"]
            if item["repository"] == "gyldlab/keld-benches"
        )
        keld_receipt = next(
            item for item in data["repository_commit_receipts"]
            if item["repository"] == "gyldlab/keld"
        )
        data["audited_revisions"]["gyldlab/keld"] = benches_revision
        keld_receipt["revision"] = benches_revision
        keld_receipt["path"] = benches_receipt["path"]
        keld_receipt["sha256"] = benches_receipt["sha256"]
        self.write_manifest(data)
        self.assert_rejected("audited KELD revision is not ancestor of publication commit")

    def test_duplicate_repository_commit_receipt_is_rejected(self) -> None:
        data = self.manifest()
        data["repository_commit_receipts"].append(
            json.loads(json.dumps(data["repository_commit_receipts"][0]))
        )
        self.write_manifest(data)
        self.assert_rejected("duplicate repository commit receipt")

    def test_repository_commit_receipt_cannot_escape_evidence_git(self) -> None:
        data = self.manifest()
        receipt = data["repository_commit_receipts"][0]
        source = self.audits / receipt["path"]
        escaped = self.audits / "escaped.gitobj"
        shutil.copy2(source, escaped)
        receipt["path"] = "evidence/git/../../escaped.gitobj"
        receipt["sha256"] = hashlib.sha256(escaped.read_bytes()).hexdigest()
        self.write_manifest(data)
        self.assert_rejected("repository commit receipt escapes evidence/git")

    def test_repository_commit_receipt_rejects_trailing_bytes(self) -> None:
        data = self.manifest()
        receipt = data["repository_commit_receipts"][0]
        receipt_path = self.audits / receipt["path"]
        receipt_path.write_bytes(receipt_path.read_bytes() + b"trailing")
        receipt["sha256"] = hashlib.sha256(receipt_path.read_bytes()).hexdigest()
        self.write_manifest(data)
        self.assert_rejected("trailing or incomplete compressed Git object receipt")

    def test_repository_commit_receipt_requires_canonical_header(self) -> None:
        data = self.manifest()
        receipt = data["repository_commit_receipts"][0]
        receipt_path = self.audits / receipt["path"]
        decompressor = zlib.decompressobj()
        loose = decompressor.decompress(receipt_path.read_bytes()) + decompressor.flush()
        _header, payload = loose.split(b"\0", 1)
        malformed = f"commit +{len(payload)}".encode() + b"\0" + payload
        receipt_path.write_bytes(zlib.compress(malformed, level=9))
        receipt["sha256"] = hashlib.sha256(receipt_path.read_bytes()).hexdigest()
        self.write_manifest(data)
        self.assert_rejected("malformed Git commit object receipt")

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

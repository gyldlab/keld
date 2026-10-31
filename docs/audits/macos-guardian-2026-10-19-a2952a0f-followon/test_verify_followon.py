#!/usr/bin/env python3
"""Mutation controls for Guardian CSV provenance and incomplete-review accounting."""
from __future__ import annotations

import csv
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("guardian_verify", HERE / "verify_followon.py")
assert SPEC is not None and SPEC.loader is not None
verifier = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verifier)


class LedgerMutations(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory(prefix="guardian-ledger-evidence-")
        self.addCleanup(self.directory.cleanup)
        self.folder = Path(self.directory.name)
        for name in ("ledger_77.csv", "review_append.csv", "summary.json"):
            (self.folder / name).write_bytes((HERE / name).read_bytes())

    def csv_edit(self, name: str, change) -> None:
        path = self.folder / name
        with path.open(newline="", encoding="utf-8") as source:
            reader = csv.DictReader(source)
            fields = reader.fieldnames
            assert fields is not None
            rows = list(reader)
        change(rows)
        with path.open("w", newline="", encoding="utf-8") as destination:
            writer = csv.DictWriter(destination, fieldnames=fields)
            writer.writeheader()
            writer.writerows(rows)

    def json_edit(self, change) -> None:
        path = self.folder / "summary.json"
        data = json.loads(path.read_text(encoding="utf-8"))
        change(data)
        path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")

    def rejects(self, expected: str) -> None:
        with self.assertRaisesRegex(verifier.Invalid, expected):
            verifier.verify(verifier.ROOT, self.folder)

    def test_original_records_pass(self) -> None:
        verifier.verify(verifier.ROOT, self.folder)

    def test_duplicate_historical_path_is_rejected(self) -> None:
        self.csv_edit("ledger_77.csv", lambda rows: rows.append(rows[0].copy()))
        self.rejects("duplicate followon path")

    def test_deleted_historical_path_is_rejected(self) -> None:
        self.csv_edit("ledger_77.csv", lambda rows: rows.pop())
        self.rejects("reconciled inventory")

    def test_wrong_historical_git_blob_is_rejected(self) -> None:
        self.csv_edit("ledger_77.csv", lambda rows: rows[0].__setitem__("tip_blob", "f" * 40))
        self.rejects("historical columns changed")

    def test_wrong_952_git_blob_is_rejected(self) -> None:
        self.csv_edit("ledger_77.csv", lambda rows: rows[0].__setitem__("blob_952", "f" * 40))
        self.rejects("parent source evidence changed")

    def test_nine_source_rows_cannot_be_doubled(self) -> None:
        self.csv_edit("review_append.csv", lambda rows: rows.append(rows[0].copy()))
        self.rejects("duplicate review path")

    def test_forged_additional_review_is_rejected(self) -> None:
        def forge(rows):
            original = rows[0].copy()
            original["path"] = "crates/keld-runtime/src/macos_guardian.rs"
            rows.append(original)
        self.csv_edit("review_append.csv", forge)
        self.rejects("new review path count")

    def test_new_review_wrong_blob_is_rejected(self) -> None:
        self.csv_edit("review_append.csv", lambda rows: rows[0].__setitem__("blob_949", "f" * 40))
        self.rejects("new review Git blob mismatch")

    def test_unreviewed_path_cannot_claim_completion(self) -> None:
        self.csv_edit("ledger_77.csv", lambda rows: rows[0].__setitem__("followon_source_status", "source-reviewed-new"))
        self.rejects("unreviewed status upgraded")

    def test_changed_blob_cannot_be_claimed_reviewed(self) -> None:
        self.csv_edit("ledger_77.csv", lambda rows: rows[1].__setitem__("followon_source_status", "source-reviewed-new"))
        self.rejects("unreviewed status upgraded")

    def test_new_9th_review_cannot_be_counted_as_prior_review(self) -> None:
        def mutate(rows):
            row = next(row for row in rows if row["followon_evidence_file"] == "review_append.csv")
            row["source_review_20261019"] = "source-reviewed-new"
        self.csv_edit("ledger_77.csv", mutate)
        self.rejects("parent source evidence changed")

    def test_summary_cannot_claim_full_architecture_review(self) -> None:
        self.json_edit(lambda obj: obj.__setitem__("all_77_source_complete", True))
        self.rejects("false audit completion")

    def test_summary_cannot_claim_merge_authority(self) -> None:
        self.json_edit(lambda obj: obj.__setitem__("merge_authorized", True))
        self.rejects("false audit completion")

    def test_status_histogram_and_pending_count_must_match(self) -> None:
        self.json_edit(lambda obj: obj.__setitem__("remaining_pending_historical_full_review", 0))
        self.rejects("summary pending count mismatch")

    def test_native_tests_not_falsely_recorded_as_run(self) -> None:
        self.json_edit(lambda obj: obj.__setitem__("native_tests_rerun_for_documentation", True))
        self.rejects("falsely claims a native run")


if __name__ == "__main__":
    unittest.main()

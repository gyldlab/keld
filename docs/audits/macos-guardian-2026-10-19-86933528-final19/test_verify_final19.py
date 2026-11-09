"""Independent negative controls for all 19 historical Guardian evidence receipts.

Each mutation touches only a temporary copy of CSV/JSON. Historical sources,
parent documents and the other agent's worktrees are never modified.
"""
from __future__ import annotations

import csv
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from verify_final19 import HERE, ROOT, SOURCE_SPANS, Invalid, verify


class FinalNineteenEvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.owned = tempfile.TemporaryDirectory(prefix="keld-guardian-final19-")
        self.addCleanup(self.owned.cleanup)
        self.folder = Path(self.owned.name)
        for name in ("ledger_77.csv", "review_append.csv", "summary.json"):
            shutil.copyfile(HERE / name, self.folder / name)

    def change_csv(self, name: str, mutation) -> None:
        target = self.folder / name
        with target.open(newline="", encoding="utf-8") as source:
            reader = csv.DictReader(source, strict=True)
            columns, rows = list(reader.fieldnames or []), list(reader)
        mutation(rows)
        with target.open("w", newline="", encoding="utf-8") as output:
            writer = csv.DictWriter(output, fieldnames=columns, lineterminator="\n")
            writer.writeheader()
            writer.writerows(rows)

    def change_summary(self, mutation) -> None:
        file = self.folder / "summary.json"
        summary = json.loads(file.read_text(encoding="utf-8"))
        mutation(summary)
        file.write_text(json.dumps(summary), encoding="utf-8")

    def must_reject(self, expected: str) -> None:
        with self.assertRaises(Invalid) as raised:
            verify(ROOT, self.folder)
        self.assertIn(expected, str(raised.exception))

    def test_positive_historical_77_rows_and_19_limited_receipts(self) -> None:
        verify(ROOT, self.folder)

    def test_every_pinned_source_interval_is_required(self) -> None:
        # Every named range in all 19 paths has its own independent deletion.
        # A mutation that cuts one cited source line must invalidate the claim,
        # even if all other receipts and the summary are still coherent.
        count = 0
        for path, citation in SOURCE_SPANS.items():
            leaf, intervals = citation.split(":")
            for index, interval in enumerate(intervals.split(",")):
                with self.subTest(path=path, interval=interval):
                    begin, end = (int(value) for value in interval.split("-"))
                    self.assertLess(begin, end)
                    narrowed = intervals.split(",")
                    narrowed[index] = f"{begin + 1}-{end}"
                    changed = f"{leaf}:{','.join(narrowed)}"

                    def remove_anchor(rows) -> None:
                        record = next(row for row in rows if row["path"] == path)
                        self.assertEqual(record["source_lines"], citation)
                        record["source_lines"] = changed

                    self.change_csv("review_append.csv", remove_anchor)
                    try:
                        self.must_reject("historical source interval/anchor removed")
                    finally:
                        shutil.copyfile(
                            HERE / "review_append.csv", self.folder / "review_append.csv"
                        )
                    count += 1
        self.assertGreaterEqual(count, 19)

    def test_duplicate_source_receipt_is_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows.append(dict(rows[0])))
        self.must_reject("duplicate/empty review path")

    def test_forged_nineteenth_source_path_is_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("path", "tests/fictitious.rs"))
        self.must_reject("final 19 exact source coverage changed")

    def test_wrong_949_blob_is_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("blob_949", "0" * 40))
        self.must_reject("wrong #949 Git blob")

    def test_wrong_952_blob_is_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[1].__setitem__("blob_952", "f" * 40))
        self.must_reject("wrong #952 Git blob")

    def test_forged_filename_in_citation_is_rejected(self) -> None:
        def forge(rows) -> None:
            record = rows[0]
            record["source_lines"] = record["source_lines"].replace("native_activity.rs:", "fiction.rs:")
        self.change_csv("review_append.csv", forge)
        self.must_reject("historical source interval/anchor removed")

    def test_nonexistent_citation_is_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("source_lines", "native_activity.rs:1-9000"))
        self.must_reject("historical source interval/anchor removed")

    def test_parent_historical_evidence_cannot_be_rewritten(self) -> None:
        self.change_csv("ledger_77.csv", lambda rows: rows[0].__setitem__("batch7_disposition", "faked native custody"))
        self.must_reject("parent #990 ledger field mutated")

    def test_parent_path_cannot_be_dropped(self) -> None:
        self.change_csv("ledger_77.csv", lambda rows: rows.pop(0))
        self.must_reject("historical 77-row inventory changed")

    def test_parent_paths_cannot_be_reordered(self) -> None:
        def swap(rows) -> None:
            rows[0], rows[1] = rows[1], rows[0]
        self.change_csv("ledger_77.csv", swap)
        self.must_reject("historical row order/identity changed")

    def test_missing_new_source_receipt_is_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows.pop())
        self.must_reject("final 19 exact source coverage changed")

    def test_newly_reviewed_status_cannot_be_downgraded(self) -> None:
        def downgrade(rows) -> None:
            record = next(row for row in rows if row["batch19_source_status"] == "source-reviewed-limited-batch19")
            record["batch19_source_status"] = "pending-source-review"
        self.change_csv("ledger_77.csv", downgrade)
        self.must_reject("incorrect batch19 status")

    def test_changed_version_overlay_cannot_be_promoted(self) -> None:
        def promote(rows) -> None:
            record = next(row for row in rows if row["path"].endswith("/authenticated_runner.rs"))
            record["batch19_source_status"] = "source-reviewed-limited-batch19"
        self.change_csv("ledger_77.csv", promote)
        self.must_reject("unreviewed historical path promoted")

    def test_other_historical_disposition_cannot_be_forged(self) -> None:
        def forge(rows) -> None:
            record = next(row for row in rows if row["batch19_source_status"] == "historical-scoped-only")
            record["batch19_disposition"] = "source reviewed"
        self.change_csv("ledger_77.csv", forge)
        self.must_reject("evidence invented for unreviewed historical path")

    def test_review_disposition_and_ledger_join_cannot_differ(self) -> None:
        def forge(rows) -> None:
            record = next(row for row in rows if row["batch19_evidence_file"])
            record["batch19_disposition"] = "full native OS custody verified"
        self.change_csv("ledger_77.csv", forge)
        self.must_reject("missing or unjoined batch19 disposition")

    def test_receipt_cannot_claim_full_review_authority(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("review_class", "source-reviewed-complete"))
        self.must_reject("unearned review authority")

    def test_receipt_cannot_claim_unscoped_semantic_completion(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("disposition_and_gap", "kernel custody proven"))
        self.must_reject("unearned semantic completion")

    def test_blank_security_invariant_is_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("invariant", ""))
        self.must_reject("blank scoped source evidence")

    def test_blank_native_fixture_oracle_is_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("fixture_and_independent_oracle", "  "))
        self.must_reject("blank scoped source evidence")

    def test_wrong_parent_head_is_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("parent_head", "0" * 40))
        self.must_reject("false parent #990 provenance")

    def test_wrong_historical_head_is_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("historical_949_head", "0" * 40))
        self.must_reject("false historical Git source identities")

    def test_wrong_source_tip_is_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("source_tip_952", "0" * 40))
        self.must_reject("false historical Git source identities")

    def test_forged_historical_inventory_is_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("historical_path_count", 76))
        self.must_reject("false 19-source review accounting")

    def test_false_parent_pending_count_is_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("parent_pending_historical", 0))
        self.must_reject("false 19-source review accounting")

    def test_false_new_review_count_is_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("new_limited_review_count", 20))
        self.must_reject("false 19-source review accounting")

    def test_false_current_pending_count_is_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("current_pending_historical", 1))
        self.must_reject("false 19-source review accounting")

    def test_tampered_status_histogram_is_rejected(self) -> None:
        self.change_summary(lambda doc: doc["historical_path_review_status_counts"].__setitem__("source-reviewed-limited-batch19", 77))
        self.must_reject("source ledger/status summary histogram diverged")

    def test_each_unreconciled_source_overlay_must_remain(self) -> None:
        for index in range(3):
            with self.subTest(changed_overlay=index):
                self.change_summary(lambda doc: doc["changed_blobs_pending"].pop(index))
                try:
                    self.must_reject("changed-overlay or additional #952-only file falsely cleared")
                finally:
                    shutil.copyfile(HERE / "summary.json", self.folder / "summary.json")

    def test_unreconciled_extra_952_only_file_must_remain(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("additional_952_only_file_pending", ""))
        self.must_reject("changed-overlay or additional #952-only file falsely cleared")

    def test_unreconciled_source_flag_must_remain(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("source_version_overlays_incomplete", False))
        self.must_reject("changed-overlay or additional #952-only file falsely cleared")

    def test_source_scope_cannot_expand_silently(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("source_review_scope", "all security proven"))
        self.must_reject("source scope wrongly certified")

    def test_each_false_security_or_completion_claim_is_rejected(self) -> None:
        for field in (
            "native_suites_rerun_for_docs",
            "all_77_historical_files_fully_reviewed",
            "kernel_custody_proven",
            "merge_authorized",
            "whole_824_architecture_complete",
        ):
            with self.subTest(claim=field):
                self.change_summary(lambda doc: doc.__setitem__(field, True))
                try:
                    self.must_reject("false product/security/merge-completion claim")
                finally:
                    shutil.copyfile(HERE / "summary.json", self.folder / "summary.json")

    def test_independent_review_pending_cannot_be_falsely_cleared(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("independent_exact_head_review_pending", False))
        self.must_reject("unperformed independent review falsely certified")


if __name__ == "__main__":
    unittest.main()

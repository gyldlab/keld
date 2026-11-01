"""Mutation controls for seven historically pinned Guardian source reviews."""

import csv
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from verify_seven import HERE, ROOT, Invalid, verify


class SevenSourceEvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.owned = tempfile.TemporaryDirectory(prefix="keld-guardian-seven-evidence-")
        self.folder = Path(self.owned.name)
        for name in ("ledger_77.csv", "review_append.csv", "summary.json"):
            shutil.copyfile(HERE / name, self.folder / name)

    def tearDown(self) -> None:
        self.owned.cleanup()

    def change_csv(self, filename: str, mutation) -> None:
        path = self.folder / filename
        with path.open(newline="", encoding="utf-8") as stream:
            reader = csv.DictReader(stream)
            fields, values = reader.fieldnames, list(reader)
        mutation(values)
        with path.open("w", newline="", encoding="utf-8") as stream:
            writer = csv.DictWriter(stream, fieldnames=fields, lineterminator="\n")
            writer.writeheader()
            writer.writerows(values)

    def change_summary(self, mutation) -> None:
        path = self.folder / "summary.json"
        summary = json.loads(path.read_text(encoding="utf-8"))
        mutation(summary)
        path.write_text(json.dumps(summary))

    def assert_invalid(self, expected: str | None = None) -> None:
        with self.assertRaises(Invalid) as caught:
            verify(ROOT, self.folder)
        if expected is not None:
            self.assertIn(expected, str(caught.exception))

    def change_citation(self, basename: str, old: str, new: str) -> None:
        def shrink(rows):
            record = next(row for row in rows if row["path"].endswith("/" + basename))
            assert record["source_lines"].count(old) == 1, basename
            record["source_lines"] = record["source_lines"].replace(old, new)
        self.change_csv("review_append.csv", shrink)

    def test_real_pinned_77_rows_and_seven_limited_receipts(self) -> None:
        verify(ROOT, self.folder)

    def test_duplicate_review_evidence_row_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows.append(dict(rows[-1])))
        self.assert_invalid()

    def test_forged_original_historical_blob_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("blob_949", "0" * 40))
        self.assert_invalid()

    def test_forged_source_tip_blob_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[1].__setitem__("blob_952", "f" * 40))
        self.assert_invalid()

    def test_unreviewed_historical_path_cannot_claim_source_review(self) -> None:
        def promote(rows):
            row = next(r for r in rows if r["batch7_source_status"] == "pending-source-review")
            row["batch7_source_status"] = "source-reviewed-limited-batch7"
        self.change_csv("ledger_77.csv", promote)
        self.assert_invalid()

    def test_changed_historical_source_still_requires_reconciliation(self) -> None:
        def promote(rows):
            row = next(r for r in rows if r["path"].endswith("/authenticated_runner.rs"))
            row["batch7_source_status"] = "source-reviewed-limited-batch7"
        self.change_csv("ledger_77.csv", promote)
        self.assert_invalid()

    def test_previous_source_disposition_cannot_be_rewritten(self) -> None:
        self.change_csv("ledger_77.csv", lambda rows: rows[0].__setitem__("batch8b_disposition", "new unsupported claim"))
        self.assert_invalid()

    def test_evidence_disposition_must_join_correct_review(self) -> None:
        def forge(rows):
            row = next(r for r in rows if r["batch7_evidence_file"])
            row["batch7_disposition"] = "all KELD source security complete"
        self.change_csv("ledger_77.csv", forge)
        self.assert_invalid()

    def test_invented_source_path_not_in_historical_scope_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("path", "crates/keld-runtime/src/macos_guardian/tests/fiction.rs"))
        self.assert_invalid()

    def test_empty_actual_caller_or_invariant_cannot_claim_review(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("invariant", ""))
        self.assert_invalid()

    def test_control_eof_actual_ack_success_oracle_citation_required(self) -> None:
        self.change_citation("control_acknowledgement.rs", "50-99", "50-85")
        self.assert_invalid()

    def test_generation_wire_real_last_byte_deadline_citation_required(self) -> None:
        self.change_citation("generation_control_wire.rs", "78-108", "78-100")
        self.assert_invalid()

    def test_original_guardian_birth_signal_citation_required(self) -> None:
        self.change_citation("generic_pre_registration_crash.rs", "95-131", "95-126")
        self.assert_invalid()

    def test_kill_instant_capture_citation_required(self) -> None:
        # killed_at (:124) is taken before the attested SIGKILL; dropping it from the cited spans must be refused.
        self.change_citation("generic_pre_registration_crash.rs", "95-131", "95-123,125-131")
        self.assert_invalid("line 124 omitted")

    def test_post_kill_three_second_bound_citation_required(self) -> None:
        # The corrected record rests on the post-loop killed_at.elapsed() < 3s assertion (:147-150).
        self.change_citation("generic_pre_registration_crash.rs", "132-168", "132-146,151-168")
        self.assert_invalid("line 147 omitted")

    def test_wrong_birth_stranger_signal_denial_citation_required(self) -> None:
        self.change_citation("cleanup_authority.rs", "26-52", "26-43")
        self.assert_invalid()

    def test_initial_kgc1_fatal_result_and_real_child_wait_citation_required(self) -> None:
        self.change_citation("initial_generation_adversaries.rs", "89-158", "89-123")
        self.assert_invalid()

    def test_authentic_malformed_registration_reaps_guardian_citation_required(self) -> None:
        self.change_citation("registration_record_adversaries.rs", "64-104", "64-80")
        self.assert_invalid()

    def test_supervisor_worker_join_after_native_stop_citation_required(self) -> None:
        self.change_citation("supervised_bootstrap.rs", "247-289", "247-273")
        self.assert_invalid()

    def test_wrong_source_filename_on_citation_rejected(self) -> None:
        self.change_citation("generation_control_wire.rs", "generation_control_wire.rs:", "control_acknowledgement.rs:")
        self.assert_invalid()

    def test_nonexistent_line_interval_is_not_source_review(self) -> None:
        self.change_citation("control_acknowledgement.rs", "161-255", "161-9000")
        self.assert_invalid()

    def test_false_full_historical_review_rejected(self) -> None:
        self.change_summary(lambda report: report.__setitem__("all_77_historical_source_fully_reviewed", True))
        self.assert_invalid()

    def test_false_kernel_custody_claim_rejected(self) -> None:
        self.change_summary(lambda report: report.__setitem__("kernel_custody_proven", True))
        self.assert_invalid()

    def test_false_merge_permission_rejected(self) -> None:
        self.change_summary(lambda report: report.__setitem__("merge_authorized", True))
        self.assert_invalid()

    def test_false_native_execution_report_rejected(self) -> None:
        self.change_summary(lambda report: report.__setitem__("native_suites_rerun_for_docs", True))
        self.assert_invalid()

    def test_unperformed_independent_semantic_review_cannot_be_certified(self) -> None:
        self.change_summary(lambda report: report.__setitem__("independent_exact_head_review_pending", False))
        self.assert_invalid()

    def test_remaining_historical_review_count_cannot_be_fabricated(self) -> None:
        self.change_summary(lambda report: report.__setitem__("current_pending_historical", 0))
        self.assert_invalid()

    def test_unreconciled_new_source_file_cannot_be_omitted(self) -> None:
        self.change_summary(lambda report: report.__setitem__("additional_952_only_file_pending", ""))
        self.assert_invalid()

    def test_parent_984_identity_cannot_be_reassigned(self) -> None:
        self.change_summary(lambda report: report.__setitem__("parent_head", "f" * 40))
        self.assert_invalid()


if __name__ == "__main__":
    unittest.main()

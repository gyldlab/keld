"""Mutation controls for the Guardian ten-file exact-blob audit delta."""

from __future__ import annotations

import csv
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from verify_tenfile import HERE, ROOT, Invalid, verify


class SourceAuditVerifierTests(unittest.TestCase):
    def setUp(self) -> None:
        self.scratch = tempfile.TemporaryDirectory(prefix="guardian-audit-ten-")
        self.folder = Path(self.scratch.name)
        for name in ("ledger_77.csv", "review_append.csv", "summary.json"):
            shutil.copyfile(HERE / name, self.folder / name)

    def tearDown(self) -> None:
        self.scratch.cleanup()

    def mutate_csv(self, name: str, change) -> None:
        path = self.folder / name
        with path.open(newline="", encoding="utf-8") as stream:
            reader = csv.DictReader(stream)
            columns, values = reader.fieldnames, list(reader)
        change(values)
        with path.open("w", newline="", encoding="utf-8") as stream:
            writer = csv.DictWriter(stream, fieldnames=columns)
            writer.writeheader()
            writer.writerows(values)

    def mutate_summary(self, change) -> None:
        path = self.folder / "summary.json"
        contents = json.loads(path.read_text(encoding="utf-8"))
        change(contents)
        path.write_text(json.dumps(contents))

    def assert_invalid(self) -> None:
        with self.assertRaises(Invalid):
            verify(ROOT, self.folder)

    def test_valid_immutable_source_evidence(self) -> None:
        verify(ROOT, self.folder)

    def test_duplicate_review_path_rejected(self) -> None:
        self.mutate_csv("review_append.csv", lambda values: values.append(dict(values[0])))
        self.assert_invalid()

    def test_forged_historical_blob_rejected(self) -> None:
        self.mutate_csv("review_append.csv", lambda values: values[0].__setitem__("blob_949", "0" * 40))
        self.assert_invalid()

    def test_forged_current_blob_rejected(self) -> None:
        self.mutate_csv("review_append.csv", lambda values: values[1].__setitem__("blob_956", "f" * 40))
        self.assert_invalid()

    def test_invented_source_path_rejected(self) -> None:
        self.mutate_csv("review_append.csv", lambda values: values[0].__setitem__("path", "crates/keld-runtime/src/macos_guardian/imaginary.rs"))
        self.assert_invalid()

    def test_original_parent_ledger_field_immutable(self) -> None:
        self.mutate_csv("ledger_77.csv", lambda values: values[0].__setitem__("blob_952", "0" * 40))
        self.assert_invalid()

    def test_unreviewed_historical_row_cannot_be_promoted(self) -> None:
        def promote(values):
            row = next(r for r in values if r["followon_source_status"] == "pending-source-review" and not r["batch10_evidence_file"])
            row["batch10_source_status"] = "source-reviewed-limited-batch10"
        self.mutate_csv("ledger_77.csv", promote)
        self.assert_invalid()

    def test_changed_overlay_cannot_be_certified_historical(self) -> None:
        def promote(values):
            row = next(r for r in values if r["path"].endswith("authenticated_runner.rs"))
            row["batch10_source_status"] = "source-reviewed-limited-batch10"
        self.mutate_csv("ledger_77.csv", promote)
        self.assert_invalid()

    def test_unmatched_disposition_cannot_claim_evidence(self) -> None:
        def rewrite(values):
            row = next(r for r in values if r["batch10_evidence_file"])
            row["batch10_disposition"] = "unreviewed replacement claim"
        self.mutate_csv("ledger_77.csv", rewrite)
        self.assert_invalid()

    def test_empty_invariant_cannot_claim_source_review(self) -> None:
        self.mutate_csv("review_append.csv", lambda values: values[0].__setitem__("invariant", ""))
        self.assert_invalid()

    def test_false_native_test_rerun_rejected(self) -> None:
        self.mutate_summary(lambda doc: doc.__setitem__("native_tests_rerun_for_documentation", True))
        self.assert_invalid()

    def test_false_all_files_complete_rejected(self) -> None:
        self.mutate_summary(lambda doc: doc.__setitem__("all_77_source_complete", True))
        self.assert_invalid()

    def test_false_merge_authorization_rejected(self) -> None:
        self.mutate_summary(lambda doc: doc.__setitem__("merge_authorized", True))
        self.assert_invalid()

    def test_changed_source_overlay_remains_pending(self) -> None:
        self.mutate_summary(lambda doc: doc.__setitem__("changed_historical_full_review_still_pending", []))
        self.assert_invalid()

    def test_omitted_attested_group_termination_body_rejected(self) -> None:
        def remove_critical_slice(values):
            record = next(r for r in values if r["path"].endswith("/process_group.rs"))
            record["source_lines"] = record["source_lines"].replace(",237-310", "")
        self.mutate_csv("review_append.csv", remove_critical_slice)
        self.assert_invalid()

    def test_omitted_kgr1_constants_and_facade_report_rejected(self) -> None:
        def remove_critical_slice(values):
            record = next(r for r in values if r["path"].endswith("/macos_guardian.rs"))
            record["source_lines"] = record["source_lines"].replace(",127-166", "")
        self.mutate_csv("review_append.csv", remove_critical_slice)
        self.assert_invalid()

    def test_omitted_post_reap_eperm_native_witness_rejected(self) -> None:
        def remove_critical_slice(values):
            record = next(r for r in values if r["path"].endswith("/process_group_failsafe.rs"))
            record["source_lines"] = record["source_lines"].replace(",113-201", "")
        self.mutate_csv("review_append.csv", remove_critical_slice)
        self.assert_invalid()

    def test_omitted_post_probe_eperm_deadline_guard_citation_rejected(self) -> None:
        def shrink_cited_body(values):
            record = next(r for r in values if r["path"].endswith("/process_group.rs"))
            record["source_lines"] = record["source_lines"].replace(",311-480", ",311-434")
        self.mutate_csv("review_append.csv", shrink_cited_body)
        self.assert_invalid()

    def test_omitted_stale_group_outsider_alive_assertion_rejected(self) -> None:
        def shrink_cited_body(values):
            record = next(r for r in values if r["path"].endswith("/process_group_failsafe.rs"))
            record["source_lines"] = record["source_lines"].replace(",380-413", ",380-393")
        self.mutate_csv("review_append.csv", shrink_cited_body)
        self.assert_invalid()

    def test_omitted_native_supervised_ack_test_body_rejected(self) -> None:
        def shrink_cited_body(values):
            record = next(r for r in values if r["path"].endswith("/supervised_ack.rs"))
            record["source_lines"] = record["source_lines"].replace(",127-208", ",127-127")
        self.mutate_csv("review_append.csv", shrink_cited_body)
        self.assert_invalid()

    def test_omitted_authentication_reject_spawn_child_wait_rejected(self) -> None:
        def remove_critical_slice(values):
            record = next(r for r in values if r["path"].endswith("/authenticated_runner.rs"))
            record["source_lines"] = record["source_lines"].replace(",110-138", "")
        self.mutate_csv("review_append.csv", remove_critical_slice)
        self.assert_invalid()

    def test_omitted_forged_kgc1_role_citation_rejected(self) -> None:
        def remove_critical_slice(values):
            record = next(r for r in values if r["path"].endswith("/rejected_registration_deadline.rs"))
            record["source_lines"] = record["source_lines"].replace(",113-171", "")
        self.mutate_csv("review_append.csv", remove_critical_slice)
        self.assert_invalid()

    def test_omitted_host_sigkill_native_role_citation_rejected(self) -> None:
        def remove_critical_slice(values):
            record = next(r for r in values if r["path"].endswith("/pre_registration_host_death.rs"))
            record["source_lines"] = record["source_lines"].replace(",121-169", "")
        self.mutate_csv("review_append.csv", remove_critical_slice)
        self.assert_invalid()

    def test_wrong_source_prefix_rejected(self) -> None:
        def redirect(values):
            values[0]["source_lines"] = values[0]["source_lines"].replace("macos_guardian.rs:", "process_group.rs:")
        self.mutate_csv("review_append.csv", redirect)
        self.assert_invalid()

    def test_unreviewed_citation_out_of_bounds_rejected(self) -> None:
        def redirect(values):
            record = next(r for r in values if r["path"].endswith("/supervised_ack.rs"))
            record["source_lines"] += ",9000-9001"
        self.mutate_csv("review_append.csv", redirect)
        self.assert_invalid()

    def test_false_parent_pr_provenance_rejected(self) -> None:
        self.mutate_summary(lambda doc: doc.__setitem__("parent_pr", 955))
        self.assert_invalid()

    def test_false_schema_revision_rejected(self) -> None:
        self.mutate_summary(lambda doc: doc.__setitem__("schema_version", 2))
        self.assert_invalid()

    def test_false_parent_unchanged_claim_rejected(self) -> None:
        self.mutate_summary(lambda doc: doc.__setitem__("parent_source_documents_unchanged", False))
        self.assert_invalid()

    def test_false_pending_count_rejected(self) -> None:
        self.mutate_summary(lambda doc: doc.__setitem__("pending_historical_full_review", 0))
        self.assert_invalid()


if __name__ == "__main__":
    unittest.main()

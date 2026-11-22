"""Independent mutation controls for #824 second eight-file Guardian receipts.

Only tempfile copies of CSV/summary are mutated; original historical Git objects,
source code, parent audits and other worktrees remain immutable.
"""
from __future__ import annotations

import csv
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from verify_next8 import HERE, ROOT, Invalid, verify


class GuardianNextEightEvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.scratch = tempfile.TemporaryDirectory(prefix="keld-guardian-next8-")
        self.folder = Path(self.scratch.name)
        for name in ("ledger_77.csv", "review_append.csv", "summary.json"):
            shutil.copyfile(HERE / name, self.folder / name)

    def tearDown(self) -> None:
        self.scratch.cleanup()

    def change_csv(self, name: str, change) -> None:
        file = self.folder / name
        with file.open(newline="", encoding="utf-8") as f:
            reader = csv.DictReader(f)
            columns, rows = list(reader.fieldnames or []), list(reader)
        change(rows)
        with file.open("w", newline="", encoding="utf-8") as f:
            writer = csv.DictWriter(f, fieldnames=columns, lineterminator="\n")
            writer.writeheader()
            writer.writerows(rows)

    def change_summary(self, change) -> None:
        file = self.folder / "summary.json"
        data = json.loads(file.read_text(encoding="utf-8"))
        change(data)
        file.write_text(json.dumps(data), encoding="utf-8")

    def must_reject(self, reason: str | None = None) -> None:
        with self.assertRaises(Invalid) as raised:
            verify(ROOT, self.folder)
        if reason:
            self.assertIn(reason, str(raised.exception))

    def shrink(self, source_suffix: str, before: str, after: str, missing_anchor: int) -> None:
        def mutate(rows):
            matches = [row for row in rows if row["path"].endswith("/" + source_suffix)]
            self.assertEqual(len(matches), 1, "source must be exactly owned by this test")
            target = matches[0]
            self.assertIn(before, target["source_lines"], "mutation must change existing source")
            target["source_lines"] = target["source_lines"].replace(before, after, 1)
        self.change_csv("review_append.csv", mutate)
        self.must_reject(f"source line {missing_anchor} omitted")

    def test_positive_exact_immutable_evidence(self) -> None:
        verify(ROOT, self.folder)

    def test_missing_g1_revoked_before_g2_provisioned_rejected(self) -> None:
        self.shrink('generation_host.rs', '93-194', '93-192,194-194', 193)

    def test_missing_g1_native_group_kill_rejected(self) -> None:
        self.shrink('generation_host.rs', '93-194', '93-141,144-194', 142)

    def test_missing_g2_unmarked_failure_terminal_rejected(self) -> None:
        self.shrink('generation_host.rs', '196-300', '196-280,283-300', 281)

    def test_missing_clear_wrong_birth_native_outsider_rejected(self) -> None:
        self.shrink('generation_host/clear_authority/cleanup_authority.rs', '63-81', '63-79,81-81', 80)

    def test_missing_clear_original_second_member_sigkill_rejected(self) -> None:
        self.shrink('generation_host/clear_authority/cleanup_authority.rs', '83-115', '83-111,113-115', 112)

    def test_missing_clear_second_member_same_group_rejected(self) -> None:
        self.shrink('generation_host/clear_authority/cleanup_authority.rs', '83-115', '83-102,104-115', 103)

    def test_missing_stale_spawned_typed_denial_rejected(self) -> None:
        self.shrink('generation_host/invalid_spawn_after_revoke.rs', '65-115', '65-88,91-115', 89)

    def test_missing_stale_spawned_original_native_child_survival_rejected(self) -> None:
        self.shrink('generation_host/invalid_spawn_after_revoke.rs', '65-115', '65-107,110-115', 108)

    def test_missing_stale_spawned_owned_birth_rescue_rejected(self) -> None:
        self.shrink('generation_host/invalid_spawn_after_revoke.rs', '13-35', '13-30,34-35', 31)

    def test_missing_early_s_native_lf_accept_rejected(self) -> None:
        self.shrink('bootstrap_shutdown_race.rs', '143-188', '143-154,156-188', 155)

    def test_missing_early_s_real_host_flush_receipt_rejected(self) -> None:
        self.shrink('bootstrap_shutdown_race.rs', '190-268', '190-226,230-268', 227)

    def test_missing_early_s_no_bun_js_execution_rejected(self) -> None:
        self.shrink('bootstrap_shutdown_race.rs', '190-268', '190-230,232-268', 231)

    def test_missing_early_s_refuses_pregeneration_quit_rejected(self) -> None:
        self.shrink('bootstrap_shutdown_race.rs', '270-344', '270-276,279-344', 277)

    def test_missing_denial_lock_before_s_rejected(self) -> None:
        self.shrink('bootstrap_shutdown_race/denial_order.rs', '70-106', '70-75,77-106', 76)

    def test_missing_denial_host_s_flushed_before_recovery_visible_rejected(self) -> None:
        self.shrink('bootstrap_shutdown_race/denial_order.rs', '70-106', '70-85,89-106', 86)

    def test_missing_denial_real_guardian_success_rejected(self) -> None:
        self.shrink('bootstrap_shutdown_race/denial_order.rs', '70-106', '70-94,96-106', 95)

    def test_missing_factory_original_error_stderr_marker_rejected(self) -> None:
        self.shrink('bootstrap_shutdown_race/factory_failure.rs', '43-91', '43-72,76-91', 73)

    def test_missing_factory_failure_host_outcome_error_rejected(self) -> None:
        self.shrink('bootstrap_shutdown_race/factory_failure.rs', '43-91', '43-78,80-91', 79)

    def test_missing_factory_failure_no_registered_bun_rejected(self) -> None:
        self.shrink('bootstrap_shutdown_race/factory_failure.rs', '43-91', '43-85,87-91', 86)

    def test_missing_post_s_real_write_flush_receipt_rejected(self) -> None:
        self.shrink('bootstrap_shutdown_race/post_s_spawn.rs', '95-126', '95-101,105-126', 102)

    def test_missing_post_s_forbids_registered_after_s_rejected(self) -> None:
        self.shrink('bootstrap_shutdown_race/post_s_spawn.rs', '127-162', '127-131,135-162', 132)

    def test_missing_post_s_original_group_absent_before_rescue_rejected(self) -> None:
        self.shrink('bootstrap_shutdown_race/post_s_spawn.rs', '127-162', '127-140,143-162', 141)

    def test_missing_post_s_failure_remains_fatal_rejected(self) -> None:
        self.shrink('bootstrap_shutdown_race/post_s_spawn.rs', '127-162', '127-156,160-162', 157)

    def test_missing_active_bun_marker_original_clock_rejected(self) -> None:
        self.shrink('active_generation_shutdown.rs', '75-118', '75-106,110-118', 107)

    def test_missing_active_real_descendant_same_group_rejected(self) -> None:
        self.shrink('active_generation_shutdown.rs', '119-172', '119-123,125-172', 124)

    def test_missing_active_orderly_host_s_shutdown_rejected(self) -> None:
        self.shrink('active_generation_shutdown.rs', '119-172', '119-133,137-172', 134)

    def test_missing_active_original_native_group_absent_rejected(self) -> None:
        self.shrink('active_generation_shutdown.rs', '119-172', '119-153,159-172', 154)

    def test_missing_active_guardian_clean_exit_status_rejected(self) -> None:
        self.shrink('active_generation_shutdown.rs', '119-172', '119-165,171-172', 166)

    def test_duplicate_source_record_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows.append(dict(rows[0])))
        self.must_reject("duplicate/empty source path")

    def test_forged_original_blob_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("blob_949", "0" * 40))
        self.must_reject("wrong #949 original source blob")

    def test_forged_source_tip_blob_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("blob_952", "f" * 40))
        self.must_reject("wrong #952 source blob")

    def test_invented_source_path_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("path", "crates/keld-runtime/src/macos_guardian/tests/fictitious.rs"))
        self.must_reject("changed eight-file coverage")

    def test_parent_original_field_immutable(self) -> None:
        self.change_csv("ledger_77.csv", lambda rows: rows[0].__setitem__("batch8_disposition", "fake"))
        self.must_reject("parent review field mutated")

    def test_unreviewed_row_cannot_be_upgraded(self) -> None:
        def promote(rows):
            row = next(row for row in rows if row["batch8b_source_status"] == "pending-source-review")
            row["batch8b_source_status"] = "source-reviewed-limited-batch8b"
        self.change_csv("ledger_77.csv", promote)
        self.must_reject("unreviewed original path promoted")

    def test_changed_historical_blob_cannot_be_upgraded(self) -> None:
        def promote(rows):
            row = next(row for row in rows if row["path"].endswith("/authenticated_runner.rs"))
            row["batch8b_source_status"] = "source-reviewed-limited-batch8b"
        self.change_csv("ledger_77.csv", promote)
        self.must_reject("unreviewed original path promoted")

    def test_joined_disposition_cannot_be_forged(self) -> None:
        def rewrite(rows):
            row = next(row for row in rows if row["batch8b_evidence_file"])
            row["batch8b_disposition"] = "kernel-pinned full review"
        self.change_csv("ledger_77.csv", rewrite)
        self.must_reject("unjoined evidence disposition")

    def test_blank_invariant_is_not_evidence(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("invariant", ""))
        self.must_reject("blank grounded evidence")

    def test_wrong_source_citation_prefix_rejected(self) -> None:
        def wrong(rows):
            rows[0]["source_lines"] = rows[0]["source_lines"].replace("generation_host.rs:", "factory_failure.rs:")
        self.change_csv("review_append.csv", wrong)
        self.must_reject("source citation references wrong file")

    def test_nonexistent_historical_source_lines_rejected(self) -> None:
        def wrong(rows):
            rows[0]["source_lines"] += ",9000-9001"
        self.change_csv("review_append.csv", wrong)
        self.must_reject("nonexistent, unordered or overlapping")

    def test_reversed_overlapping_source_lines_rejected(self) -> None:
        self.change_csv("review_append.csv", lambda rows: rows[0].__setitem__("source_lines", "generation_host.rs:28-62,50-91"))
        self.must_reject("nonexistent, unordered or overlapping")

    def test_forged_parent_pr_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("parent_pr", 955))
        self.must_reject("summary parent provenance false")

    def test_forged_parent_immutability_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("parent_source_documents_unchanged", False))
        self.must_reject("summary parent provenance false")

    def test_forged_historical_source_identity_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("source_tip_952", "0" * 40))
        self.must_reject("summary source Git identity false")

    def test_false_historical_pending_count_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("current_pending_historical", 0))
        self.must_reject("summary source/status inventory false")

    def test_mismatched_summary_status_histogram_rejected(self) -> None:
        self.change_summary(lambda doc: doc["source_review_status_counts"].__setitem__("pending-source-review", 0))
        self.must_reject("summary status histogram false")

    def test_changed_source_pending_cannot_be_erased(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("changed_blobs_pending", []))
        self.must_reject("earlier changed/extra source file falsely cleared")

    def test_extra_source_only_file_must_remain_pending(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("additional_952_only_file_pending", ""))
        self.must_reject("earlier changed/extra source file falsely cleared")

    def test_false_native_suite_execution_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("native_suites_rerun", True))
        self.must_reject("false product/completion claim")

    def test_false_kernel_custody_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("kernel_custody_proven", True))
        self.must_reject("false product/completion claim")

    def test_false_merge_authority_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("merge_authorized", True))
        self.must_reject("false product/completion claim")

    def test_false_full_source_completion_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("all_historical_files_fully_reviewed", True))
        self.must_reject("false product/completion claim")

    def test_false_independent_review_approval_rejected(self) -> None:
        self.change_summary(lambda doc: doc.__setitem__("independent_exact_head_review_pending", False))
        self.must_reject("independent source review falsely certified")


if __name__ == "__main__":
    unittest.main()

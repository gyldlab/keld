"""Mutation controls for four pinned #952 source overlays, no native execution.

Only temporary copies of this child audit's JSON and inherited 77-file CSV
are modified. All original Git blobs and other worktrees remain untouched.
"""
from __future__ import annotations

import csv
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from verify_overlays import HERE, RECEIPTS, ROOT, Invalid, verify


class Historical952OverlayTests(unittest.TestCase):
    def setUp(self) -> None:
        self.owned = tempfile.TemporaryDirectory(prefix="keld-guardian-952-overlay-")
        self.addCleanup(self.owned.cleanup)
        self.folder = Path(self.owned.name)
        for name in ("ledger_77.csv", "overlay_evidence.json", "summary.json", "composed_parent.json"):
            shutil.copyfile(HERE / name, self.folder / name)

    def change_evidence(self, mutation) -> None:
        source = self.folder / "overlay_evidence.json"
        data = json.loads(source.read_text(encoding="utf-8"))
        mutation(data)
        source.write_text(json.dumps(data, indent=2), encoding="utf-8")

    def change_review(self, which: int, mutation) -> None:
        self.change_evidence(lambda doc: mutation(doc["reviews"][which]))

    def change_summary(self, mutation) -> None:
        source = self.folder / "summary.json"
        data = json.loads(source.read_text(encoding="utf-8"))
        mutation(data)
        source.write_text(json.dumps(data, indent=2), encoding="utf-8")

    def change_ledger(self, mutation) -> None:
        file = self.folder / "ledger_77.csv"
        with file.open(newline="", encoding="utf-8") as stream:
            reader = csv.DictReader(stream, strict=True)
            columns, rows = list(reader.fieldnames or []), list(reader)
        mutation(rows)
        with file.open("w", newline="", encoding="utf-8") as stream:
            writer = csv.DictWriter(stream, fieldnames=columns, lineterminator="\n")
            writer.writeheader()
            writer.writerows(rows)

    def assert_rejected(self, reason: str) -> None:
        with self.assertRaises(Invalid) as raised:
            verify(ROOT, self.folder)
        self.assertIn(reason, str(raised.exception))

    def restore(self, name: str) -> None:
        shutil.copyfile(HERE / name, self.folder / name)

    def test_composed_parent_record_is_separate_and_exact(self) -> None:
        source = self.folder / "composed_parent.json"
        original = source.read_text(encoding="utf-8")
        for field, value in (("composed_parent", "0" * 40),
                             ("historical_parent", "0" * 40),
                             ("files", []), ("changed_paths", []),
                             ("native_acceptance", True),
                             ("historical_receipts_rewritten", True)):
            with self.subTest(field=field):
                data = json.loads(original)
                data[field] = value
                source.write_text(json.dumps(data), encoding="utf-8")
                self.assert_rejected("composed parent provenance changed")
        source.write_text(original, encoding="utf-8")

    def test_positive_four_exact_952_git_blobs_and_77_immutable_history(self) -> None:
        verify(ROOT, self.folder)

    def test_every_source_section_is_bound_to_full_pinned_source(self) -> None:
        for row_index, meta in enumerate(RECEIPTS.values()):
            for section_index in range(len(meta["sections"])):
                with self.subTest(source=meta["path"], section=section_index):
                    self.change_review(
                        row_index,
                        lambda record: record["source_sections"][section_index].__setitem__(
                            "first", record["source_sections"][section_index]["first"] + 1
                        ),
                    )
                    try:
                        self.assert_rejected("full immutable #952 source-section coverage changed")
                    finally:
                        self.restore("overlay_evidence.json")

    def test_every_individual_pinned_source_anchor_is_required(self) -> None:
        doc = json.loads((HERE / "overlay_evidence.json").read_text(encoding="utf-8"))
        anchor_count = 0
        for row_index, record in enumerate(doc["reviews"]):
            for anchor_index, line in enumerate(record["source_anchors"]):
                with self.subTest(source=record["id"], line=line):
                    self.change_review(
                        row_index,
                        lambda row: row["source_anchors"].pop(anchor_index),
                    )
                    try:
                        self.assert_rejected("named #952 source-line anchor removed or changed")
                    finally:
                        self.restore("overlay_evidence.json")
                    anchor_count += 1
        self.assertGreaterEqual(anchor_count, 160)

    def test_every_direct_caller_anchor_is_required(self) -> None:
        for row_index, meta in enumerate(RECEIPTS.values()):
            for caller_index, caller in enumerate(meta["callers"]):
                with self.subTest(source=meta["path"], caller=caller[0]):
                    self.change_review(row_index, lambda row: row["caller_anchors"].pop(caller_index))
                    try:
                        self.assert_rejected("source direct-caller provenance changed")
                    finally:
                        self.restore("overlay_evidence.json")

    def test_every_named_native_oracle_is_required(self) -> None:
        for row_index, meta in enumerate(RECEIPTS.values()):
            for oracle_index, oracle in enumerate(meta["oracles"]):
                with self.subTest(source=meta["path"], selector=oracle[1]):
                    self.change_review(row_index, lambda row: row["native_oracles"].pop(oracle_index))
                    try:
                        self.assert_rejected("named native negative oracle removed")
                    finally:
                        self.restore("overlay_evidence.json")

    def test_four_source_records_cannot_be_dropped_or_duplicated(self) -> None:
        self.change_evidence(lambda doc: doc["reviews"].pop(0))
        self.assert_rejected("four exact source-overlay identities changed")
        self.restore("overlay_evidence.json")
        self.change_evidence(lambda doc: doc["reviews"].append(dict(doc["reviews"][0])))
        self.assert_rejected("four exact source-overlay identities changed")

    def test_review_rows_cannot_be_reordered(self) -> None:
        def swap(doc) -> None:
            doc["reviews"][0], doc["reviews"][1] = doc["reviews"][1], doc["reviews"][0]
        self.change_evidence(swap)
        self.assert_rejected("four exact source-overlay identities changed")

    def test_changed_original_949_blob_rejected(self) -> None:
        self.change_review(0, lambda row: row.__setitem__("blob_949", "0" * 40))
        self.assert_rejected("forged original #949 source blob")

    def test_changed_original_952_blob_rejected(self) -> None:
        self.change_review(1, lambda row: row.__setitem__("blob_952", "f" * 40))
        self.assert_rejected("forged pinned #952 source blob")

    def test_new_952_only_file_cannot_be_invented_at_949(self) -> None:
        self.change_review(3, lambda row: row.__setitem__("blob_949", "0" * 40))
        self.assert_rejected("extra #952-only source falsely claimed as #949 file")

    def test_source_overlay_cannot_gain_unreviewed_path(self) -> None:
        self.change_review(1, lambda row: row.__setitem__("path", "crates/keld-runtime/src/macos_guardian/fiction.rs"))
        self.assert_rejected("source overlay kind/path/old-status forged")

    def test_existing_77_file_cannot_be_forged_as_new_952_file(self) -> None:
        self.change_review(3, lambda row: row.__setitem__("kind", "changed-949-to-952"))
        self.assert_rejected("source overlay kind/path/old-status forged")

    def test_existing_changed_source_cannot_be_promoted_to_complete(self) -> None:
        self.change_review(0, lambda row: row.__setitem__("review_status", "source-reviewed-complete"))
        self.assert_rejected("unsupported promotion")

    def test_new_source_only_status_cannot_be_promoted_into_old_77(self) -> None:
        self.change_review(3, lambda row: row.__setitem__("parent_historical_status", "source-reviewed-limited"))
        self.assert_rejected("source overlay kind/path/old-status forged")

    def test_added_source_lines_must_match_immutable_diff(self) -> None:
        self.change_review(2, lambda row: row.__setitem__("diff_added", row["diff_added"] + 1))
        self.assert_rejected("edited source diff counts")

    def test_deleted_source_lines_must_match_immutable_diff(self) -> None:
        self.change_review(1, lambda row: row.__setitem__("diff_deleted", row["diff_deleted"] + 1))
        self.assert_rejected("edited source diff counts")

    def test_invented_source_line_anchor_is_rejected(self) -> None:
        self.change_review(3, lambda row: row["source_anchors"].append(9000))
        self.assert_rejected("named #952 source-line anchor removed or changed")

    def test_blank_scope_cannot_be_claimed_as_limited_review(self) -> None:
        self.change_evidence(lambda doc: doc.__setitem__("scope", "complete KELD release"))
        self.assert_rejected("wrong limited review evidence scope")

    def test_changed_native_selector_cannot_be_attributed(self) -> None:
        self.change_review(2, lambda row: row["native_oracles"][0].__setitem__("selector", "imaginary_native_test"))
        self.assert_rejected("named native negative oracle removed")

    def test_changed_caller_line_cannot_be_attributed(self) -> None:
        self.change_review(1, lambda row: row["caller_anchors"][0].__setitem__("first", 1))
        self.assert_rejected("source direct-caller provenance changed")

    def test_changed_claim_of_kernel_custody_is_rejected(self) -> None:
        self.change_review(1, lambda row: row.__setitem__(
            "source_finding",
            row["source_finding"] + " This fully proves a kernel-pinned whole generation process group.",
        ))
        self.assert_rejected("reviewed limited source prose/known gaps changed")

    def test_changed_caller_description_is_rejected(self) -> None:
        self.change_review(0, lambda row: row["caller_anchors"][0].__setitem__(
            "role", "no review of the original caller required",
        ))
        self.assert_rejected("reviewed limited source prose/known gaps changed")

    def test_changed_oracle_limit_cannot_be_hidden(self) -> None:
        self.change_review(3, lambda row: row["native_oracles"][0].__setitem__(
            "observes", "This proves simultaneous host and Guardian SIGKILL custody.",
        ))
        self.assert_rejected("reviewed limited source prose/known gaps changed")

    def test_previous_19_row_evidence_is_immutable(self) -> None:
        def forge(rows) -> None:
            record = next(row for row in rows if row["batch19_source_status"] == "source-reviewed-limited-batch19")
            record["batch19_disposition"] = "kernel-pinned completed"
        self.change_ledger(forge)
        self.assert_rejected("inherited historical 77 ledger bytes changed")

    def test_original_three_changed_source_statuses_remain_pinned(self) -> None:
        def promote(rows) -> None:
            record = next(row for row in rows if row["batch19_source_status"] == "changed-since-949-pending")
            record["batch19_source_status"] = "source-reviewed-limited-952-overlay"
        self.change_ledger(promote)
        self.assert_rejected("inherited historical 77 ledger bytes changed")

    def test_parent_77_path_cannot_be_deleted(self) -> None:
        self.change_ledger(lambda rows: rows.pop(0))
        self.assert_rejected("inherited historical 77 ledger bytes changed")

    def test_summary_parent_head_cannot_change(self) -> None:
        self.change_summary(lambda s: s.__setitem__("parent_head", "0" * 40))
        self.assert_rejected("false historical/parent provenance")

    def test_summary_historical_949_sha_cannot_change(self) -> None:
        self.change_summary(lambda s: s.__setitem__("source_baseline_949", "0" * 40))
        self.assert_rejected("false historical/parent provenance")

    def test_summary_historical_952_sha_cannot_change(self) -> None:
        self.change_summary(lambda s: s.__setitem__("source_snapshot_952", "f" * 40))
        self.assert_rejected("false historical/parent provenance")

    def test_summary_historical_file_count_cannot_change(self) -> None:
        self.change_summary(lambda s: s.__setitem__("historical_77_file_count", 78))
        self.assert_rejected("false historical/overlay source-accounting")

    def test_summary_changed_overlay_count_cannot_change(self) -> None:
        self.change_summary(lambda s: s.__setitem__("reviewed_changed_952_overlay_count", 4))
        self.assert_rejected("false historical/overlay source-accounting")

    def test_summary_new_952_only_count_cannot_change(self) -> None:
        self.change_summary(lambda s: s.__setitem__("reviewed_added_952_only_count", 0))
        self.assert_rejected("false historical/overlay source-accounting")

    def test_summary_old_status_immutability_cannot_be_deleted(self) -> None:
        self.change_summary(lambda s: s.__setitem__("historical_statuses_retained_exactly", False))
        self.assert_rejected("historical inventory silently promoted")

    def test_summary_old_77_cannot_claim_extra_added_file(self) -> None:
        self.change_summary(lambda s: s.__setitem__("additional_952_only_original_77_membership", True))
        self.assert_rejected("historical inventory silently promoted")

    def test_summary_limited_review_promotion_meaning_cannot_change(self) -> None:
        self.change_summary(lambda s: s.__setitem__("resolved_meaning", "all 77 semantic source reviews complete"))
        self.assert_rejected("historical inventory silently promoted")

    def test_summary_open_technical_limits_cannot_be_hidden(self) -> None:
        self.change_summary(lambda s: s.__setitem__("remaining_gap", "all kernel issues proved fixed"))
        self.assert_rejected("historical inventory silently promoted")

    def test_each_false_os_native_merge_or_full_completion_is_rejected(self) -> None:
        for field in (
            "all_77_histories_semantically_complete",
            "current_latest_repo_head_source_review_complete",
            "repo_wide_file_census_complete",
            "native_guardian_suites_rerun_for_docs",
            "test_mutants_executed_against_rust_code",
            "kernel_pinned_process_group_custody_proven",
            "simultaneous_host_guardian_death_proven",
            "detached_descendant_containment_proven",
            "source_review_is_native_acceptance",
            "merge_authorized",
        ):
            with self.subTest(claim=field):
                self.change_summary(lambda summary: summary.__setitem__(field, True))
                try:
                    self.assert_rejected("false native/OS/merge completion claim")
                finally:
                    self.restore("summary.json")

    def test_independent_read_only_exact_tip_review_remains_pending(self) -> None:
        self.change_summary(lambda s: s.__setitem__("independent_exact_child_tip_review_pending", False))
        self.assert_rejected("unperformed independent review falsely certified")


if __name__ == "__main__":
    unittest.main()

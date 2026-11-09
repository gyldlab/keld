"""Mutation-negative controls for immutable eight-file Guardian evidence.

Every control names the refusal it must provoke. ``must_fail`` has no default
expectation, so a bare "some Invalid was raised" cannot be written: a mutation
that trips several guards would otherwise stay green after the one guard it was
written for is deleted. Where a mutation could also trip a count or summary
check, the matching summary is edited too, so only the targeted guard fires.
"""

import csv
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import verify_eightfile
from verify_eightfile import HERE, ROOT, TEST_ROOT, Invalid, verify

PENDING = "pending-source-review"
LIMITED = "source-reviewed-limited-batch8"
RETAINED_ACK = TEST_ROOT + "retained_ack.rs"
AUTHENTICATED_RUNNER = "crates/keld-runtime/src/macos_guardian/authenticated_runner.rs"


class EightFileEvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.owned = tempfile.TemporaryDirectory(prefix="keld-guardian-eight-audit-")
        self.folder = Path(self.owned.name)
        for name in ("ledger_77.csv", "review_append.csv", "summary.json"):
            shutil.copyfile(HERE / name, self.folder / name)

    def tearDown(self) -> None:
        self.owned.cleanup()

    def edit_csv(self, name: str, change) -> None:
        path = self.folder / name
        with path.open(newline="", encoding="utf-8") as stream:
            original = csv.DictReader(stream)
            columns, values = original.fieldnames, list(original)
        change(values)
        with path.open("w", newline="", encoding="utf-8") as stream:
            writer = csv.DictWriter(stream, fieldnames=columns, lineterminator="\n")
            writer.writeheader()
            writer.writerows(values)

    def edit_header(self, name: str, old: str, new: str) -> None:
        path = self.folder / name
        with path.open(newline="", encoding="utf-8") as stream:
            text = stream.read()
        header, newline, body = text.partition("\n")
        self.assertIn(old, header, "mutation must change the existing header")
        with path.open("w", newline="", encoding="utf-8") as stream:
            stream.write(header.replace(old, new, 1) + newline + body)

    def edit_summary(self, change) -> None:
        path = self.folder / "summary.json"
        value = json.loads(path.read_text(encoding="utf-8"))
        change(value)
        path.write_text(json.dumps(value))

    def row_for(self, rows, path: str) -> dict:
        matches = [row for row in rows if row["path"] == path]
        self.assertEqual(len(matches), 1, "mutation target must be exactly one row")
        return matches[0]

    def must_fail(self, expected: str) -> None:
        with self.assertRaises(Invalid) as refused:
            verify(ROOT, self.folder)
        self.assertIn(expected, str(refused.exception))

    def must_miss_anchor(self, anchor: int, suffix: str) -> None:
        self.must_fail(f"code citation line {anchor}: {TEST_ROOT}{suffix}")

    def summary_refused(self, key: str, value, expected: str) -> None:
        self.edit_summary(lambda report: report.__setitem__(key, value))
        self.must_fail(expected)

    def append_raw(self, name: str, text: str) -> None:
        with (self.folder / name).open("a", newline="", encoding="utf-8") as stream:
            stream.write(text)

    def parent_ledger_edit(self, change):
        """Make verify() see an edited parent ledger while the pinned bytes stay untouched."""
        real = verify_eightfile.table
        parent = ROOT / verify_eightfile.PARENT_REL / "ledger_77.csv"

        def table(path):
            fields, rows = real(path)
            if Path(path) == parent:
                change(rows)
            return fields, rows

        return mock.patch.object(verify_eightfile, "table", table)

    def test_valid_immutable_source_receipts(self) -> None:
        verify(ROOT, self.folder)

    # table(): CSV shape and path identity.
    def test_short_receipt_row_denied(self) -> None:
        self.append_raw("review_append.csv", "only,two\n")
        self.must_fail("invalid CSV shape: review_append.csv")

    def test_overlong_receipt_row_denied(self) -> None:
        self.append_raw("review_append.csv", "," * 40 + "\n")
        self.must_fail("invalid CSV shape: review_append.csv")

    def test_empty_receipt_table_denied(self) -> None:
        (self.folder / "review_append.csv").write_text("")
        self.must_fail("invalid CSV shape: review_append.csv")

    def test_duplicate_receipt_path_denied(self) -> None:
        self.edit_csv("review_append.csv", lambda rows: rows.append(dict(rows[-1])))
        self.must_fail("duplicate/empty source path: review_append.csv")

    def test_empty_receipt_path_denied(self) -> None:
        self.edit_csv("review_append.csv", lambda rows: rows[0].__setitem__("path", ""))
        self.must_fail("duplicate/empty source path: review_append.csv")

    # check_citations(): citation grammar, file identity and line bounds.
    def test_malformed_citation_denied(self) -> None:
        self.edit_csv("review_append.csv", lambda rows: rows[0].__setitem__("source_lines", "not-a-citation"))
        self.must_fail(f"not an ordered exact-source citation: {RETAINED_ACK}")

    def test_citation_of_another_file_denied(self) -> None:
        self.edit_csv("review_append.csv", lambda rows: rows[0].__setitem__("source_lines", "other_file.rs:31-79"))
        self.must_fail(f"cited file does not match original path: {RETAINED_ACK}")

    def test_overlapping_citation_intervals_denied(self) -> None:
        self.edit_csv("review_append.csv", lambda rows: rows[0].__setitem__("source_lines", "retained_ack.rs:31-79,70-153"))
        self.must_fail(f"overlapping or nonexistent source lines: {RETAINED_ACK}")

    def test_citation_past_end_of_file_denied(self) -> None:
        self.edit_csv("review_append.csv", lambda rows: rows[0].__setitem__("source_lines", "retained_ack.rs:31-99999"))
        self.must_fail(f"overlapping or nonexistent source lines: {RETAINED_ACK}")

    def test_reversed_citation_interval_denied(self) -> None:
        self.edit_csv("review_append.csv", lambda rows: rows[0].__setitem__("source_lines", "retained_ack.rs:79-31"))
        self.must_fail(f"overlapping or nonexistent source lines: {RETAINED_ACK}")

    # verify(): pinned identities and the immutable parent audit.
    def test_symbolic_parent_pin_denied(self) -> None:
        with mock.patch.object(verify_eightfile, "PARENT", "HEAD"):
            self.must_fail("wrong pinned Git identity: HEAD")

    def test_symbolic_baseline_pin_denied(self) -> None:
        with mock.patch.object(verify_eightfile, "BASELINE", "HEAD"):
            self.must_fail("wrong pinned Git identity: HEAD")

    def test_symbolic_source_pin_denied(self) -> None:
        with mock.patch.object(verify_eightfile, "SOURCE", "HEAD"):
            self.must_fail("wrong pinned Git identity: HEAD")

    def test_missing_parent_source_document_denied(self) -> None:
        with mock.patch.object(verify_eightfile, "PARENT_REL", "docs/audits/no-such-parent-audit"):
            self.must_fail("missing parent source document: README.md")

    def test_altered_parent_source_document_denied(self) -> None:
        real = subprocess.run
        pinned = f"{verify_eightfile.PARENT}:{verify_eightfile.PARENT_REL}/README.md"

        def altered(command, *args, **kwargs):
            done = real(command, *args, **kwargs)
            if tuple(command[3:5]) == ("show", pinned):
                return subprocess.CompletedProcess(command, 0, stdout=done.stdout + b"tampered\n", stderr=b"")
            return done

        with mock.patch.object(subprocess, "run", altered):
            self.must_fail("mutated original source review #976: README.md")

    def test_inherited_columns_must_not_change(self) -> None:
        self.edit_header("ledger_77.csv", "batch8_disposition", "batch8_dispositionX")
        self.must_fail("modified inherited 77-source columns")

    def test_receipt_schema_must_not_change(self) -> None:
        self.edit_header("review_append.csv", "disposition_and_gap", "disposition_and_gapX")
        self.must_fail("source evidence schema changed")

    def test_dropped_ledger_row_denied(self) -> None:
        self.edit_csv("ledger_77.csv", lambda rows: rows.pop())
        self.must_fail("historical 77-path inventory changed")

    def test_dropped_receipt_denied(self) -> None:
        self.edit_csv("review_append.csv", lambda rows: rows.pop())
        self.must_fail("eight independently-reviewed fixture paths changed")

    def test_nonexistent_review_path_rejected(self) -> None:
        self.edit_csv("review_append.csv", lambda rows: rows[0].__setitem__("path", "crates/keld-runtime/src/macos_guardian/fake.rs"))
        self.must_fail("eight independently-reviewed fixture paths changed")

    def test_renamed_ledger_path_denied(self) -> None:
        self.edit_csv("ledger_77.csv", lambda rows: rows[0].__setitem__("path", "crates/keld-runtime/src/macos_guardian/invented.rs"))
        self.must_fail("invented or dropped historical file")

    # verify(): per-row guards.
    def test_inherited_source_evidence_must_not_change(self) -> None:
        self.edit_csv("ledger_77.csv", lambda rows: rows[0].__setitem__("batch10_disposition", "rewritten prior proof"))
        self.must_fail("inherited review/status field mutated: crates/keld-runtime/src/macos_guardian.rs")

    def test_review_of_already_reviewed_parent_row_denied(self) -> None:
        settled = "source-reviewed-limited-batch10"

        def settle(rows):
            self.row_for(rows, RETAINED_ACK)["batch10_source_status"] = settled

        self.edit_csv("ledger_77.csv", settle)
        with self.parent_ledger_edit(settle):
            self.must_fail(f"double-counting already completed review: {RETAINED_ACK}")

    def test_full_source_review_class_denied(self) -> None:
        self.edit_csv("review_append.csv", lambda rows: rows[0].__setitem__("review_class", "source-reviewed-full"))
        self.must_fail(f"false full-source claim: {RETAINED_ACK}")

    def test_missing_security_invariant_rejected(self) -> None:
        self.edit_csv("review_append.csv", lambda rows: rows[0].__setitem__("invariant", ""))
        self.must_fail(f"missing manually grounded source evidence: {RETAINED_ACK}")

    def test_forged_original_blobs_denied(self) -> None:
        self.edit_csv("review_append.csv", lambda rows: rows[0].__setitem__("blob_949", "0" * 40))
        self.must_fail(f"wrong original #949 blob: {RETAINED_ACK}")

    def test_forged_source_tip_blobs_denied(self) -> None:
        self.edit_csv("review_append.csv", lambda rows: rows[0].__setitem__("blob_952", "f" * 40))
        self.must_fail(f"wrong original #952 blob: {RETAINED_ACK}")

    def test_changed_historical_blob_cannot_be_reviewed(self) -> None:
        # A row whose #949 and #952 blobs differ would otherwise pass every blob
        # check. Present authenticated_runner.rs as a pending parent row with a
        # receipt carrying its true blobs; only the changed-blob guard may refuse.
        with (HERE / "review_append.csv").open(newline="", encoding="utf-8") as stream:
            displaced = next(row for row in csv.DictReader(stream) if row["path"] == RETAINED_ACK)

        def pend(rows):
            self.row_for(rows, AUTHENTICATED_RUNNER)["batch10_source_status"] = PENDING

        def retarget_ledger(rows):
            pend(rows)
            freed = self.row_for(rows, RETAINED_ACK)
            freed.update(batch8_source_status=PENDING, batch8_evidence_file="", batch8_disposition="")
            row = self.row_for(rows, AUTHENTICATED_RUNNER)
            row.update(batch8_source_status=LIMITED, batch8_evidence_file="review_append.csv",
                       batch8_disposition=displaced["disposition_and_gap"])
            self.assertNotEqual(row["tip_blob"], row["blob_952"], "target must be a changed-source row")
            retarget_ledger.blobs = (row["tip_blob"], row["blob_952"])

        def retarget_receipt(rows):
            receipt = self.row_for(rows, RETAINED_ACK)
            receipt.update(path=AUTHENTICATED_RUNNER, blob_949=retarget_ledger.blobs[0], blob_952=retarget_ledger.blobs[1],
                           source_lines="authenticated_runner.rs:1-5")

        self.edit_csv("ledger_77.csv", retarget_ledger)
        self.edit_csv("review_append.csv", retarget_receipt)
        with mock.patch.dict(verify_eightfile.REVIEWED) as reviewed, self.parent_ledger_edit(pend):
            del reviewed[RETAINED_ACK]
            reviewed[AUTHENTICATED_RUNNER] = (1,)
            self.must_fail(f"changed historical blob must remain pending: {AUTHENTICATED_RUNNER}")

    def test_reviewed_row_must_carry_the_batch_status(self) -> None:
        # Relabel a reviewed row and move one count in the summary, so the
        # histogram still matches and only the status guard refuses.
        def relabel(rows):
            self.row_for(rows, RETAINED_ACK)["batch8_source_status"] = "source-reviewed-new"

        def recount(report):
            counts = report["source_review_status_counts"]
            counts[LIMITED] -= 1
            counts["source-reviewed-new"] += 1

        self.edit_csv("ledger_77.csv", relabel)
        self.edit_summary(recount)
        self.must_fail(f"incorrect conservative source status: {RETAINED_ACK}")

    def test_receipt_join_requires_same_disposition(self) -> None:
        def rewrite(rows):
            row = next(r for r in rows if r["batch8_evidence_file"])
            row["batch8_disposition"] = "invented verified physical custody"
        self.edit_csv("ledger_77.csv", rewrite)
        self.must_fail("review receipt/ledger join mismatch: " + TEST_ROOT + "guardian_crash_pre_registration/cleanup_authority.rs")

    def test_unreviewed_historical_path_cannot_gain_status(self) -> None:
        def promote(rows):
            row = next(r for r in rows if r["batch8_source_status"] == PENDING)
            row["batch8_source_status"] = LIMITED
        self.edit_csv("ledger_77.csv", promote)
        self.must_fail("unreviewed file incorrectly promoted: crates/keld-runtime/src/macos_guardian/guarded_lease/tests/native_activi")

    def test_non_pending_unreviewed_row_cannot_be_relabelled(self) -> None:
        # Pending count stays 34 and the summary histogram is edited to match,
        # so the promotion guard is the only one that can refuse this row.
        scoped = "historical-scoped-only"

        def relabel(rows):
            row = next(r for r in rows if r["batch8_source_status"] == scoped)
            relabel.path = row["path"]
            row["batch8_source_status"] = "source-reviewed-new"

        def recount(report):
            counts = report["source_review_status_counts"]
            counts[scoped] -= 1
            counts["source-reviewed-new"] += 1

        self.edit_csv("ledger_77.csv", relabel)
        self.edit_summary(recount)
        self.must_fail(f"unreviewed file incorrectly promoted: {relabel.path}")

    def test_changed_old_source_cannot_be_closed(self) -> None:
        def promote(rows):
            self.row_for(rows, AUTHENTICATED_RUNNER)["batch8_source_status"] = LIMITED
        self.edit_csv("ledger_77.csv", promote)
        self.must_fail(f"unreviewed file incorrectly promoted: {AUTHENTICATED_RUNNER}")

    def test_invented_evidence_file_for_unreviewed_row_denied(self) -> None:
        def invent(rows):
            row = next(r for r in rows if r["batch8_source_status"] == PENDING)
            invent.path = row["path"]
            row["batch8_evidence_file"] = "review_append.csv"
        self.edit_csv("ledger_77.csv", invent)
        self.must_fail(f"invented evidence for unreviewed: {invent.path}")

    def test_invented_disposition_for_unreviewed_row_denied(self) -> None:
        def invent(rows):
            row = next(r for r in rows if r["batch8_source_status"] == PENDING)
            invent.path = row["path"]
            row["batch8_disposition"] = "invented review result"
        self.edit_csv("ledger_77.csv", invent)
        self.must_fail(f"invented evidence for unreviewed: {invent.path}")

    # verify(): summary guards.
    def test_unknown_summary_schema_denied(self) -> None:
        self.summary_refused("schema_version", 2, "source provenance/schema ungrounded")

    def test_wrong_parent_pr_denied(self) -> None:
        self.summary_refused("parent_pr", 975, "source provenance/schema ungrounded")

    def test_switched_parent_head_denied(self) -> None:
        self.summary_refused("parent_head", "0" * 40, "historical source version switched")

    def test_switched_baseline_head_denied(self) -> None:
        self.summary_refused("baseline_949_head", "0" * 40, "historical source version switched")

    def test_switched_source_tip_denied(self) -> None:
        self.summary_refused("source_tip_952", "0" * 40, "historical source version switched")

    def test_forged_historical_path_count_denied(self) -> None:
        self.summary_refused("historical_path_count", 76, "historical scope or starting count false")

    def test_forged_starting_pending_count_denied(self) -> None:
        self.summary_refused("parent_pending_historical", 41, "historical scope or starting count false")

    def test_forged_historical_completed_count_rejected(self) -> None:
        self.summary_refused("current_pending_historical", 0, "review count or remaining scope misleading")

    def test_forged_new_review_count_denied(self) -> None:
        self.summary_refused("new_limited_review_count", 9, "review count or remaining scope misleading")

    def test_misreported_status_histogram_denied(self) -> None:
        def inflate(report):
            report["source_review_status_counts"]["source-reviewed-new"] += 1
        self.edit_summary(inflate)
        self.must_fail("summary misrepresents review statuses")

    def test_unreviewed_changed_source_count_must_remain(self) -> None:
        self.summary_refused("changed_blobs_pending", [], "changed-source overlays falsely closed")

    def test_extra_952_only_file_must_remain_pending(self) -> None:
        self.summary_refused("additional_952_only_file_pending", "", "new #952-only source falsely closed")

    def test_forged_native_success_rejected(self) -> None:
        self.summary_refused("native_suites_rerun", True, "falsely certifies native execution or security completion: native_suites_rerun")

    def test_forged_full_historical_review_rejected(self) -> None:
        self.summary_refused("all_historical_files_fully_reviewed", True, "falsely certifies native execution or security completion: all_historical_files_fully_reviewed")

    def test_fake_kernel_custodian_certificate_rejected(self) -> None:
        self.summary_refused("kernel_custody_proven", True, "falsely certifies native execution or security completion: kernel_custody_proven")

    def test_false_merge_permission_rejected(self) -> None:
        self.summary_refused("merge_authorized", True, "falsely certifies native execution or security completion: merge_authorized")

    def test_completed_independent_review_cannot_be_asserted(self) -> None:
        self.summary_refused("independent_exact_head_review_pending", False, "uncompleted independent source audit falsely certified")

    # check_citations(): each named native custody/clock anchor must stay cited.
    def test_late_r_original_ack_clock_anchor_required(self) -> None:
        def drop_r_cut(rows):
            row = next(r for r in rows if r["path"].endswith("/retained_ack.rs"))
            row["source_lines"] = row["source_lines"].replace("31-79,", "53-79,")
        self.edit_csv("review_append.csv", drop_r_cut)
        self.must_miss_anchor(48, "retained_ack.rs")

    def test_original_native_receipt_clock_anchor_required(self) -> None:
        def drop_lf_cut(rows):
            row = next(r for r in rows if r["path"].endswith("/retained_ack/observers.rs"))
            row["source_lines"] = row["source_lines"].replace("35-76,", "47-76,")
        self.edit_csv("review_append.csv", drop_lf_cut)
        self.must_miss_anchor(35, "retained_ack/observers.rs")

    def test_actual_ack_release_and_native_group_retirement_cannot_be_omitted(self) -> None:
        def shrink(rows):
            row = next(r for r in rows if r["path"].endswith("/retained_ack.rs"))
            row["source_lines"] = row["source_lines"].replace("225-310", "225-225")
        self.edit_csv("review_append.csv", shrink)
        self.must_miss_anchor(254, "retained_ack.rs")

    def test_dual_death_original_birth_and_group_membership_cannot_be_omitted(self) -> None:
        def shrink(rows):
            row = next(r for r in rows if r["path"].endswith("/dual_death.rs"))
            row["source_lines"] = row["source_lines"].replace("53-86", "53-53")
        self.edit_csv("review_append.csv", shrink)
        self.must_miss_anchor(65, "retained_ack/dual_death.rs")

    def test_detached_independent_session_and_original_birth_cannot_be_omitted(self) -> None:
        def shrink(rows):
            row = next(r for r in rows if r["path"].endswith("/escaped_descendant.rs"))
            row["source_lines"] = row["source_lines"].replace("105-135", "105-125")
        self.edit_csv("review_append.csv", shrink)
        self.must_miss_anchor(128, "retained_ack/escaped_descendant.rs")

    def test_successful_lf_cannot_hide_missing_read_and_accept_deadline_checks(self) -> None:
        def shrink(rows):
            row = next(r for r in rows if r["path"].endswith("/retained_ack/observers.rs"))
            row["source_lines"] = row["source_lines"].replace("35-76", "35-46")
        self.edit_csv("review_append.csv", shrink)
        self.must_miss_anchor(53, "retained_ack/observers.rs")

    def test_denial_winner_no_locator_and_eof_assertions_cannot_be_omitted(self) -> None:
        def shrink(rows):
            row = next(r for r in rows if r["path"].endswith("/ownership_races.rs"))
            row["source_lines"] = row["source_lines"].replace("208-242", "208-231")
        self.edit_csv("review_append.csv", shrink)
        self.must_miss_anchor(234, "runtime_race_validation/ownership_races.rs")

    def test_native_bun_image_and_both_kgr1_selectors_cannot_be_omitted(self) -> None:
        def shrink(rows):
            row = next(r for r in rows if r["path"].endswith("/retained_ack.rs"))
            row["source_lines"] = row["source_lines"].replace("311-354", "311-330")
        self.edit_csv("review_append.csv", shrink)
        self.must_miss_anchor(341, "retained_ack.rs")

    def test_host_group_second_member_sigkill_evidence_cannot_be_omitted(self) -> None:
        def shrink(rows):
            row = next(r for r in rows if r["path"].endswith("/cleanup_tests.rs"))
            row["source_lines"] = row["source_lines"].replace("277-336", "277-278")
        self.edit_csv("review_append.csv", shrink)
        self.must_miss_anchor(331, "retained_ack/escaped_descendant/cleanup_tests.rs")

    def test_group_gone_deadline_cannot_be_omitted(self) -> None:
        def shrink(rows):
            row = next(r for r in rows if r["path"].endswith("/retained_ack/observers.rs"))
            row["source_lines"] = row["source_lines"].replace("77-94", "77-80")
        self.edit_csv("review_append.csv", shrink)
        self.must_miss_anchor(88, "retained_ack/observers.rs")

    def test_original_guardian_group_sigkill_native_status_cannot_be_omitted(self) -> None:
        def shrink(rows):
            row = next(r for r in rows if r["path"].endswith("/guardian_crash_pre_registration/cleanup_authority.rs"))
            row["source_lines"] = row["source_lines"].replace("127-153", "127-127")
        self.edit_csv("review_append.csv", shrink)
        self.must_miss_anchor(148, "guardian_crash_pre_registration/cleanup_authority.rs")

    def test_second_unrelated_native_child_survival_cannot_be_omitted(self) -> None:
        def shrink(rows):
            row = next(r for r in rows if r["path"].endswith("/pre_registration_host_death/cleanup_authority.rs"))
            row["source_lines"] = row["source_lines"].replace("102-127", "102-125")
        self.edit_csv("review_append.csv", shrink)
        self.must_miss_anchor(126, "pre_registration_host_death/cleanup_authority.rs")

    def test_actual_provisioned_native_listener_and_denial_lock_cannot_be_omitted(self) -> None:
        def shrink(rows):
            row = next(r for r in rows if r["path"].endswith("/ownership_races.rs"))
            row["source_lines"] = row["source_lines"].replace("116-207", "116-146")
        self.edit_csv("review_append.csv", shrink)
        self.must_miss_anchor(156, "runtime_race_validation/ownership_races.rs")

    def test_dual_death_pre_rescue_native_survivor_required(self) -> None:
        def remove_survivor(rows):
            row = next(r for r in rows if r["path"].endswith("/dual_death.rs"))
            row["source_lines"] = row["source_lines"].replace("87-108", "87-100")
        self.edit_csv("review_append.csv", remove_survivor)
        self.must_miss_anchor(101, "retained_ack/dual_death.rs")

    def test_detached_child_before_test_only_rescue_cited(self) -> None:
        def remove_survival(rows):
            row = next(r for r in rows if r["path"].endswith("/escaped_descendant.rs"))
            row["source_lines"] = row["source_lines"].replace("136-177", "163-177")
        self.edit_csv("review_append.csv", remove_survival)
        self.must_miss_anchor(157, "retained_ack/escaped_descendant.rs")

    def test_unauthorized_stranger_native_sigkill_citation_required(self) -> None:
        def remove_sentinel(rows):
            row = next(r for r in rows if r["path"].endswith("/cleanup_tests.rs"))
            row["source_lines"] = row["source_lines"].replace("277-336,", "279-336,")
        self.edit_csv("review_append.csv", remove_sentinel)
        self.must_miss_anchor(278, "retained_ack/escaped_descendant/cleanup_tests.rs")


if __name__ == "__main__":
    unittest.main()

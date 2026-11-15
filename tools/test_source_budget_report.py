#!/usr/bin/env python3
"""Negative controls for proposed source budgets; never activate CI hard stops."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import source_budget_report as budget
import source_census as census


def row(name, lines, byte_count, category="B"):
    return {"path": name, "category": category,
            "physical_lines": lines, "bytes": byte_count}


class PureReportContracts(unittest.TestCase):
    def test_change_parser_preserves_nonascii_and_newline(self):
        raw = ("M\0space name.rs\0R100\0old.rs\0café.rs\0"
               "A\0line\nbreak.rs\0D\0deleted.rs\0").encode()
        self.assertEqual([
            ("M", "space name.rs", "space name.rs"),
            ("R", "old.rs", "café.rs"),
            ("A", "line\nbreak.rs", "line\nbreak.rs"),
            ("D", "deleted.rs", "deleted.rs"),
        ], budget.parse_changes(raw))
        with self.assertRaises(ValueError):
            budget.parse_changes(b"R100\0old.rs\0")
        with self.assertRaises(ValueError):
            budget.parse_changes(b"X\0fake.rs\0")

    def test_new_breach_is_reported_not_enforced(self):
        current = row("big.rs", 1201, 100)
        result = budget.assess_change("A", "big.rs", "big.rs", None, current,
                                      census.CANDIDATE)
        self.assertEqual("new-above-proposed-cap", result["finding"])
        self.assertTrue(result["hypothetical_block"])

    def test_grandfathered_shrink_and_growth(self):
        original = row("legacy.rs", 1300, 72000)
        smaller = row("legacy.rs", 1290, 70000)
        bigger = row("legacy.rs", 1301, 72000)
        shrink = budget.assess_change("M", "legacy.rs", "legacy.rs",
                                      original, smaller, census.CANDIDATE)
        grow = budget.assess_change("M", "legacy.rs", "legacy.rs",
                                    original, bigger, census.CANDIDATE)
        self.assertFalse(shrink["hypothetical_block"])
        self.assertEqual("grandfathered-shrink", shrink["finding"])
        self.assertTrue(shrink["legacy_over_proposed_cap"])
        self.assertEqual("grandfathered-growth", grow["finding"])
        self.assertTrue(grow["hypothetical_block"])

    def test_review_thresholds_follow_existing_test_entry_owner(self):
        entry = row("crates/keld-update/src/windows_baseline/tests.rs", 101, 3000, "C")
        result = budget.assess_change("M", entry["path"], entry["path"],
                                      entry, entry, census.CANDIDATE)
        self.assertEqual("review-trigger", result["finding"])
        self.assertTrue(result["test_layout_owner_review"])
        self.assertTrue(result["review_over_owner_threshold"])
        self.assertFalse(result["hypothetical_block"])

    def test_legacy_recovered_within_proposal(self):
        old = row("legacy.rs", 1300, 72000)
        new = row("legacy.rs", 100, 1000)
        result = budget.assess_change("M", "legacy.rs", "legacy.rs",
                                      old, new, census.CANDIDATE)
        self.assertEqual("legacy-now-within-proposal", result["finding"])
        self.assertFalse(result["hypothetical_block"])

    def test_legacy_under_both_caps_after_one_axis_grows_is_not_a_block(self):
        # Review finding on #738: category B hard caps are 1200 lines / 65536 bytes.
        # 1300 lines / 50000 B -> 1100 lines / 60000 B is under BOTH caps even though
        # bytes grew, so it must be "legacy-now-within-proposal", never a block.
        old = row("legacy.rs", 1300, 50000)
        new = row("legacy.rs", 1100, 60000)
        result = budget.assess_change("M", "legacy.rs", "legacy.rs",
                                      old, new, census.CANDIDATE)
        self.assertEqual("legacy-now-within-proposal", result["finding"])
        self.assertFalse(result["hypothetical_block"])
        self.assertTrue(result["legacy_over_proposed_cap"])
        self.assertFalse(result["currently_over_proposed_cap"])
        self.assertTrue(result["grew_on_either_axis"])

    def test_legacy_over_by_bytes_then_under_both_caps_with_line_growth(self):
        # Mirror image: over the byte cap before, lines grow, ends under both caps.
        old = row("legacy.rs", 1000, 70000)
        new = row("legacy.rs", 1100, 60000)
        result = budget.assess_change("M", "legacy.rs", "legacy.rs",
                                      old, new, census.CANDIDATE)
        self.assertEqual("legacy-now-within-proposal", result["finding"])
        self.assertFalse(result["hypothetical_block"])
        self.assertFalse(result["currently_over_proposed_cap"])
        self.assertTrue(result["grew_on_either_axis"])

    def test_legacy_still_over_a_cap_with_growth_stays_a_hypothetical_block(self):
        # Negative control for the reorder: the file is still over the line cap
        # (1250 > 1200) and bytes grew, so growth must still be reported as a block.
        old = row("legacy.rs", 1300, 50000)
        new = row("legacy.rs", 1250, 60000)
        result = budget.assess_change("M", "legacy.rs", "legacy.rs",
                                      old, new, census.CANDIDATE)
        self.assertEqual("grandfathered-growth", result["finding"])
        self.assertTrue(result["hypothetical_block"])
        self.assertTrue(result["currently_over_proposed_cap"])

    def test_only_bytes_can_trigger_proposed_budget(self):
        result = budget.assess_change("M", "code.rs", "code.rs",
                                      row("code.rs", 900, 50000),
                                      row("code.rs", 900, 65537), census.CANDIDATE)
        self.assertEqual("newly-over-proposed-cap", result["finding"])

    def test_review_only_is_never_a_hard_violation(self):
        r = budget.assess_change("M", "code.rs", "code.rs",
                                 row("code.rs", 810, 20000),
                                 row("code.rs", 820, 20100), census.CANDIDATE)
        self.assertEqual("review-trigger", r["finding"])
        self.assertFalse(r["hypothetical_block"])
        self.assertTrue(r["review_over_generic_threshold"])

    def test_removed_excluded_and_unchanged_rename(self):
        x = row("old.rs", 1250, 64000)
        self.assertEqual("removed", budget.assess_change(
            "D", "old.rs", "old.rs", x, None, census.CANDIDATE)["finding"])
        renamed = budget.assess_change("R", "old.rs", "new.rs",
                                       x, x, census.CANDIDATE)
        self.assertFalse(renamed["hypothetical_block"])
        self.assertEqual("old.rs", renamed["old_path"])
        excluded = {"category": "F", "exclusion_reason": "binary", "bytes": 15,
                    "physical_lines": None}
        self.assertEqual("excluded", budget.assess_change(
            "A", "b.bin", "b.bin", None, excluded,
            census.CANDIDATE)["finding"])


class RealGitReportContracts(unittest.TestCase):
    @staticmethod
    def commit(repo, msg):
        census.git(repo, "-c", "user.name=Keld Audit",
                   "-c", "user.email=audit@example.invalid",
                   "commit", "-qm", msg)

    def test_real_git_report_and_cli_stay_nonblocking(self):
        with tempfile.TemporaryDirectory(prefix="keld-budget-") as tmp:
            repo = Path(tmp)
            census.git(repo, "init", "-q")
            census.git(repo, "config", "gc.auto", "0")
            census.git(repo, "config", "maintenance.auto", "false")

            (repo / "large.rs").write_text("old\n" * 1201, encoding="utf-8")
            (repo / "move me.rs").write_text("fn old() {}\n", encoding="utf-8")
            (repo / "gone.rs").write_text("fn gone() {}\n", encoding="utf-8")
            census.git(repo, "add", ".")
            self.commit(repo, "baseline")
            base = census.git(repo, "rev-parse", "HEAD").decode().strip()

            (repo / "large.rs").write_text("old\n" * 1202, encoding="utf-8")
            census.git(repo, "mv", "move me.rs", "café.rs")
            (repo / "gone.rs").unlink()
            (repo / "big new.rs").write_text("fn n() {}\n" * 1201, encoding="utf-8")
            census.git(repo, "add", "-A")
            self.commit(repo, "follow-up")
            head = census.git(repo, "rev-parse", "HEAD").decode().strip()

            result = budget.report(repo, base, head, "candidate")
            findings = {r["path"]: r for r in result["findings"]}
            self.assertEqual("report-only", result["enforcement"])
            self.assertEqual(4, result["changed_count"])
            self.assertEqual("grandfathered-growth", findings["large.rs"]["finding"])
            self.assertEqual("new-above-proposed-cap", findings["big new.rs"]["finding"])
            self.assertEqual("removed", findings["gone.rs"]["finding"])
            self.assertEqual("R", findings["café.rs"]["change_kind"])
            self.assertEqual("move me.rs", findings["café.rs"]["old_path"])
            self.assertFalse(findings["café.rs"]["hypothetical_block"])

            output = repo / "report.json"
            script = Path(__file__).with_name("source_budget_report.py")
            run = subprocess.run(
                [sys.executable, "-B", str(script), "--repo", str(repo),
                 "--base", base, "--head", head, "--output", str(output)],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
            self.assertEqual(0, run.returncode, run.stderr.decode("utf-8", "replace"))
            self.assertEqual(result, json.loads(output.read_text(encoding="utf-8")))
            self.assertEqual(result, budget.report(repo, base, head, "candidate"))

            identical = budget.report(repo, head, head, "candidate")
            self.assertEqual(0, identical["changed_count"])
            self.assertEqual([], identical["findings"])
            with self.assertRaises(RuntimeError):
                budget.report(repo, "definitely-not-a-ref", head, "candidate")

    def test_real_git_legacy_recovery_does_not_inflate_headline_count(self):
        # Same case as the #738 review finding, end to end through report():
        # a 1300 line / ~50 kB file becomes 1100 lines / ~60 kB (under both caps).
        with tempfile.TemporaryDirectory(prefix="keld-budget-") as tmp:
            repo = Path(tmp)
            census.git(repo, "init", "-q")
            census.git(repo, "config", "gc.auto", "0")
            census.git(repo, "config", "maintenance.auto", "false")

            (repo / "legacy.rs").write_text(("x" * 37 + "\n") * 1300, encoding="utf-8", newline="\n")
            census.git(repo, "add", ".")
            self.commit(repo, "baseline over the line cap")
            base = census.git(repo, "rev-parse", "HEAD").decode().strip()

            (repo / "legacy.rs").write_text(("x" * 53 + "\n") * 1100, encoding="utf-8", newline="\n")
            census.git(repo, "add", "-A")
            self.commit(repo, "fewer lines, more bytes, under both caps")
            head = census.git(repo, "rev-parse", "HEAD").decode().strip()

            result = budget.report(repo, base, head, "candidate")
            (finding,) = result["findings"]
            self.assertEqual((1300, 49400), (finding["base_lines"], finding["base_bytes"]))
            self.assertEqual((1100, 59400), (finding["head_lines"], finding["head_bytes"]))
            self.assertEqual("legacy-now-within-proposal", finding["finding"])
            self.assertEqual(0, result["hypothetical_blocked_changed_count"])
            self.assertEqual({"legacy-now-within-proposal": 1}, result["finding_counts"])

    def test_empty_diff_parity_even_with_renamed_ref(self):
        with tempfile.TemporaryDirectory(prefix="keld-budget-") as tmp:
            repo = Path(tmp)
            census.git(repo, "init", "-q")
            census.git(repo, "config", "gc.auto", "0")
            census.git(repo, "config", "maintenance.auto", "false")
            (repo / "plain.rs").write_bytes(b"x\r\ny")
            census.git(repo, "add", "plain.rs")
            self.commit(repo, "root")
            head = census.git(repo, "rev-parse", "HEAD").decode().strip()
            self.assertEqual(0, budget.report(repo, head, head, "roomier")
                             ["hypothetical_blocked_changed_count"])


if __name__ == "__main__":
    unittest.main(verbosity=2)

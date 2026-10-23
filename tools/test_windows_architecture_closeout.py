"""Offline contract tests for the Windows SHA-pinned architecture review overlay."""
import csv
import hashlib
import io
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from windows_architecture_closeout import (
    AUDIT, EVIDENCE_FIELDS, FIELDS, QUEUE, QUEUE_SHA256, reconcile, snapshot)

ROOT = Path(__file__).resolve().parent.parent


def csv_data(fields, rows):
    output = io.StringIO(newline="")
    writer = csv.DictWriter(output, fieldnames=fields, lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    return output.getvalue()


def candidate(path, sha):
    return {
        **{field: "" for field in FIELDS},
        "path": path,
        "git_blob_sha": sha,
        "review_status": "PENDING_INDEPENDENT_SEMANTIC_REVIEW",
    }


def evidence(path="a.rs", sha="a" * 40, **changes):
    return {
        **{
            "path": path,
            "git_blob_sha": sha,
            "review_evidence": "https://github.com/gyldlab/keld/issues/824#issuecomment-6248828406",
            "review_disposition": "REVIEWED_NO_CHANGE",
            "review_owner": "runtime",
            "independent_falsifier": "An outsider may never execute before Job assignment",
        },
        **changes,
    }


class LedgerReconciliationTests(unittest.TestCase):
    def setUp(self):
        self.base = csv_data(FIELDS, [candidate("a.rs", "a" * 40), candidate("b.rs", "b" * 40)])

    def apply(self, claims):
        return reconcile(self.base, csv_data(EVIDENCE_FIELDS, claims))

    def test_valid_exact_blob_review_preserves_other_pending_path(self):
        output, totals = self.apply([evidence()])
        rows = list(csv.DictReader(io.StringIO(output)))
        self.assertEqual([r["review_status"] for r in rows],
                         ["REVIEWED_NO_CHANGE", "PENDING_INDEPENDENT_SEMANTIC_REVIEW"])
        self.assertEqual(totals, {
            "candidates": 2,
            "reviewed_no_change": 1,
            "reviewed_conditional_risk": 0,
            "reviewed_total": 1,
            "pending_independent_review": 1,
            "not_closed": 1,
        })
        self.assertEqual(rows[1]["review_evidence"], "")
        self.assertEqual(rows[0]["git_blob_sha"], "a" * 40)

    def test_conditional_risk_is_reviewed_but_remains_not_closed(self):
        output, totals = self.apply([evidence(review_disposition="REVIEWED_CONDITIONAL_RISK")])
        self.assertIn("REVIEWED_CONDITIONAL_RISK", output)
        self.assertEqual(totals["reviewed_total"], 1)
        self.assertEqual(totals["not_closed"], 2)

    def test_no_silent_review_when_no_claims(self):
        output, totals = self.apply([])
        self.assertEqual(totals["reviewed_total"], 0)
        self.assertEqual(totals["pending_independent_review"], 2)
        self.assertNotIn("REVIEWED_NO_CHANGE", output)

    def test_stale_sha_rejected(self):
        with self.assertRaisesRegex(ValueError, "stale review blob"):
            self.apply([evidence(sha="x" * 40)])

    def test_duplicate_claim_rejected(self):
        with self.assertRaisesRegex(ValueError, "duplicate review"):
            self.apply([evidence(), evidence()])

    def test_claim_outside_source_ref_rejected(self):
        with self.assertRaisesRegex(ValueError, "absent candidate"):
            self.apply([evidence(path="ghost.rs")])

    def test_empty_or_invented_evidence_rejected(self):
        with self.assertRaisesRegex(ValueError, "public evidence"):
            self.apply([evidence(review_evidence="https://example.com/not-a-review")])
        # A bare issue or PR page names no specific review comment.
        for bare in ("https://github.com/gyldlab/keld/issues/824",
                     "https://github.com/gyldlab/keld/pull/942"):
            with self.assertRaisesRegex(ValueError, "public evidence"):
                self.apply([evidence(review_evidence=bare)])

    def test_missing_falsifier_or_owner_rejected(self):
        with self.assertRaisesRegex(ValueError, "falsifier"):
            self.apply([evidence(independent_falsifier="")])
        with self.assertRaisesRegex(ValueError, "owner"):
            self.apply([evidence(review_owner="  ")])

    def test_unknown_disposition_rejected(self):
        with self.assertRaisesRegex(ValueError, "unrecognized disposition"):
            self.apply([evidence(review_disposition="REVIEWED_ALL_CLEAR")])

    def test_source_cannot_fake_review_status(self):
        rows = [candidate("a.rs", "a" * 40), candidate("b.rs", "b" * 40)]
        rows[0]["review_status"] = "REVIEWED_NO_CHANGE"
        self.base = csv_data(FIELDS, rows)
        with self.assertRaisesRegex(ValueError, "incomplete source/evidence"):
            self.apply([])

    def test_duplicate_source_path_rejected(self):
        self.base = csv_data(FIELDS, [candidate("a.rs", "a" * 40)] * 2)
        with self.assertRaisesRegex(ValueError, "duplicate candidate"):
            self.apply([])

    def test_unexpected_source_fields_rejected(self):
        self.base = csv_data(("path", "git_blob_sha"), [{"path": "a.rs", "git_blob_sha": "a"}])
        with self.assertRaisesRegex(ValueError, "unexpected base"):
            self.apply([])

    def test_unexpected_claim_fields_rejected(self):
        with self.assertRaisesRegex(ValueError, "unexpected review evidence"):
            reconcile(self.base, csv_data(("path",), [{"path": "a.rs"}]))


class TriageQueueBindingTests(unittest.TestCase):
    """The closeout binds the committed #891 queue bytes, never a stale copy (#942)."""

    def test_binding_matches_the_committed_queue(self):
        live = (ROOT / QUEUE).read_bytes().replace(b"\r\n", b"\n")
        self.assertEqual(hashlib.sha256(live).hexdigest(), QUEUE_SHA256)

    def test_changed_queue_is_refused_before_rendering(self):
        committed = (ROOT / QUEUE).read_bytes()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / AUDIT).mkdir(parents=True)
            # Negative control: one appended row must change the binding.
            (root / QUEUE).write_bytes(committed + b"extra,row\n")
            with self.assertRaisesRegex(ValueError, "#891 triage queue changed"):
                snapshot(root)


if __name__ == "__main__":
    unittest.main()

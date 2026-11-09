"""Dated retirement and replacement tests on real temporary learning files."""
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import learning_records as lr
import test_learning_records as fixtures

PROOF = f"https://github.com/gyldlab/keld/blob/{lr.SOURCE_COMMIT}/crates/keld-ipc/src/lib.rs#L1"
MARKERS = {"status_changed": "2026-10-31", "status_reason": "The cited contract no longer holds.", "status_evidence": PROOF}


class LearningLifecycleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.base = tempfile.TemporaryDirectory(prefix="learning-lifecycle-base-")
        cls.paths = fixtures.make_fixture(Path(cls.base.name), fixtures.original_source())

    @classmethod
    def tearDownClass(cls):
        cls.base.cleanup()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="learning-lifecycle-test-")
        self.root = Path(self.temp.name) / "repo"
        shutil.copytree(self.base.name, self.root)

    def tearDown(self):
        self.temp.cleanup()

    def change(self, path, *, remove=(), **updates):
        file = self.root / path
        with lr.open_record(file) as stream:
            data = lr.frontmatter(stream)
        body = file.read_text(encoding="utf-8").split("\n---\n", 1)[1]
        for field in remove:
            data.pop(field, None)
        data.update(updates)
        fixtures.write_record(self.root, path, data, body)

    def add(self, topic, *, status="verified", verified="2026-10-31"):
        name = "2026-10-31-" + topic
        path = f"{lr.LEARNING_ROOT}/ipc/{name}.md"
        data = {"name": name, "description": "Keep one complete transport fact.", "area": "ipc", "tags": ["ipc"],
                "platform": "all", "recorded": "2026-10-31", "status": status, "verified": verified,
                "source": PROOF, "related": []}
        fixtures.write_record(self.root, path, data, "\nKeep one complete transport fact.\n")
        return path

    def rejects(self, pattern):
        with self.assertRaisesRegex(lr.LearningError, pattern):
            lr.check(self.root)

    def pair(self):
        old = self.add("old-fact", status="historical", verified="unknown")
        new = self.add("new-fact")
        self.change(old, status="superseded", superseded_by=[new], **MARKERS)
        self.change(new, supersedes=[old])
        return old, new

    def test_retired_requires_each_dated_proof_field(self):
        path = self.add("retired", status="stale", verified="unknown")
        original = (self.root / path).read_bytes()
        self.rejects("retired status missing")
        for field in MARKERS:
            with self.subTest(field=field):
                (self.root / path).write_bytes(original)
                self.change(path, **{key: value for key, value in MARKERS.items() if key != field})
                self.rejects("missing " + field)
        (self.root / path).write_bytes(original);self.change(path, **MARKERS)
        self.assertEqual(lr.check(self.root), 169)

    def test_dates_reasons_proofs_and_closed_keys_are_strict(self):
        path = self.add("retired", status="stale")
        self.change(path, **MARKERS);original = (self.root / path).read_bytes()
        bad = [({"status_changed": "2026-02-30"}, "real ISO"),
               ({"status_changed": "unknown"}, "ISO date"),
               ({"status_reason": ""}, "one line"), ({"status_reason": "two\nlines"}, "one line"),
               ({"status_reason": "word " * 26}, "25 words"),
               ({"status_evidence": PROOF.replace(lr.SOURCE_COMMIT, "main")}, "immutable"),
               ({"status_evidence": PROOF.replace("src/lib.rs", "src/../lib.rs")}, "unsafe source path"),
               ({"verification_evidence": PROOF.replace("#L1", "#L0")}, "immutable"),
               ({"verification_evidence": "https://example.com/proof"}, "immutable"),
               ({"status_evidences": PROOF}, "unknown fields")]
        for updates, expected in bad:
            with self.subTest(updates=updates):
                (self.root / path).write_bytes(original);self.change(path, **updates);self.rejects(expected)

    def test_verified_unknown_is_rejected(self):
        self.add("unverified", verified="unknown")
        self.rejects("verified status requires a real verified date")

    def test_historical_promotion_needs_new_proof_and_keeps_migration(self):
        path = self.paths[0]
        with lr.open_record(self.root / path) as stream:
            original = lr.frontmatter(stream)["source"]
        self.change(path, status="verified", verified="2026-10-31")
        self.rejects("historical import requires verification_evidence")
        self.change(path, verification_evidence=PROOF)
        self.assertEqual(lr.check(self.root), 168)
        self.assertEqual(lr.metadata(self.root, path)["source"], original)
        self.assertEqual(lr.describe(self.root, [path])[0]["verification_evidence"], PROOF)

    def test_active_records_cannot_keep_retirement_markers(self):
        path = self.add("active");original = (self.root / path).read_bytes()
        for field, value in {**MARKERS, "superseded_by": []}.items():
            (self.root / path).write_bytes(original);self.change(path, **{field: value})
            self.rejects("nonretired status cannot retain retirement markers")
        (self.root / path).write_bytes(original);self.change(path, status="historical", verified="unknown", **MARKERS)
        self.rejects("nonretired status")

    def test_stale_cannot_claim_forward_replacement(self):
        old, new = self.pair()
        self.change(new, status="stale", superseded_by=[], **MARKERS)
        self.rejects("stale status cannot declare superseded_by")
        self.change(new, remove=["superseded_by"])
        self.assertEqual(lr.check(self.root), 170)
        self.assertEqual(lr.metadata(self.root, new)["supersedes"], [old])

    def test_superseded_requires_nonempty_replacement(self):
        path = self.add("old", status="superseded", verified="unknown")
        self.change(path, **MARKERS);self.rejects("nonempty superseded_by")
        self.change(path, superseded_by=[]);self.rejects("nonempty superseded_by")

    def test_broken_self_and_unreciprocated_edges(self):
        old, new = self.pair()
        self.change(new, supersedes=[]);self.rejects("reciprocal supersedes")
        self.change(new, supersedes=[old]);self.change(old, superseded_by=[old]);self.rejects("self superseded_by")
        self.change(old, superseded_by=[f"{lr.LEARNING_ROOT}/ipc/2026-10-31-absent.md"]);self.rejects("missing link")
        self.change(old, superseded_by=[new]);self.change(new, supersedes=[new]);self.rejects("self supersedes")
        self.change(new, supersedes=[old, old]);self.rejects("unique path list")

    def test_backward_edge_requires_reciprocal_forward_edge(self):
        old = self.add("old");new = self.add("new")
        self.change(new, supersedes=[old]);self.rejects("reciprocal superseded_by")

    def test_historical_or_unverified_replacement_is_rejected(self):
        old, new = self.pair()
        self.change(new, status="historical")
        self.rejects("replacement target requires")
        self.change(new, status="stale", verified="unknown", **MARKERS)
        self.rejects("replacement target requires")

    def test_imported_later_stale_target_keeps_verification_proof(self):
        new = self.paths[0];old = self.add("old", status="superseded", verified="unknown")
        self.change(old, superseded_by=[new], **MARKERS)
        self.change(new, status="stale", verified="2026-10-31", supersedes=[old], **MARKERS)
        self.rejects("historical replacement requires verification_evidence")
        self.change(new, verification_evidence=PROOF)
        self.assertEqual(lr.check(self.root), 169)

    def test_acyclic_chain_and_later_stale_target_preserve_history(self):
        old, middle = self.pair();new = self.add("latest")
        self.change(middle, status="superseded", superseded_by=[new], **MARKERS)
        self.change(new, supersedes=[middle])
        self.assertEqual(lr.check(self.root), 171)
        self.change(new, status="stale", **MARKERS)
        self.assertEqual(lr.check(self.root), 171)
        self.change(old, verified="2026-10-31", supersedes=[new])
        self.change(new, status="superseded", superseded_by=[old])
        self.rejects("must be acyclic")

    def test_describe_retirement_without_peer_or_body_reads(self):
        old, new = self.pair();file = self.root / old;raw = file.read_bytes();boundary = raw.index(b"\n---\n") + 5
        (self.root / new).write_text("BROKEN PEER HEADER MUST NOT BE READ", encoding="utf-8")
        calls = [];real_open = lr.open_record
        class Guarded:
            def __enter__(reader):return reader
            def __exit__(reader, *unused):reader.stream.close()
            def readline(reader, size=-1):
                self.assertLess(reader.stream.tell(), boundary)
                line = reader.stream.readline(size);self.assertLessEqual(reader.stream.tell(), boundary)
                return line
            def read(reader, *unused):raise AssertionError("body read")
        def opened(actual):
            self.assertEqual(actual, file);calls.append(actual)
            result = Guarded();result.stream = real_open(actual);return result
        with patch.object(lr, "open_record", opened), patch.object(lr, "record_files", side_effect=AssertionError("record scan")), \
             patch.object(lr.lifecycle, "validate_graph", side_effect=AssertionError("graph auto-load")):
            result = lr.describe(self.root, [old])[0]
        self.assertEqual(calls, [file]);self.assertTrue(result["retired"])
        for field in ["status", "verified", "status_changed", "status_reason", "status_evidence", "superseded_by"]:
            self.assertIn(field, result)
        self.assertEqual(result["superseded_by"], [new])

    def test_dates_do_not_use_a_clock_or_infer_break_time(self):
        path = self.add("dated", status="stale", verified="unknown")
        self.change(path, **{**MARKERS, "status_changed": "2999-12-31"})
        self.assertEqual(lr.check(self.root), 169)


if __name__ == "__main__":
    unittest.main()

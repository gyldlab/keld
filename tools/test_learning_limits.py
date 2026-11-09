"""Registered metadata bounds and one-pass budget loading on real files."""
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import learning_records as lr
import test_learning_records as fixtures
import test_learning_lifecycle as states

PROOF = f"https://github.com/gyldlab/keld/blob/{lr.SOURCE_COMMIT}/crates/keld-ipc/src/lib.rs#L1"


class LearningLimitTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.base = tempfile.TemporaryDirectory(prefix="learning-limits-base-")
        fixtures.make_fixture(Path(cls.base.name), fixtures.original_source())

    @classmethod
    def tearDownClass(cls):
        cls.base.cleanup()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="learning-limits-test-")
        self.root = Path(self.temp.name) / "repo"
        shutil.copytree(self.base.name, self.root)

    def tearDown(self):
        self.temp.cleanup()

    def data(self, topic="a", **updates):
        name = "2026-10-31-" + topic
        relative = f"{lr.LEARNING_ROOT}/ipc/{name}.md"
        data = {"name": name, "description": "Keep one complete transport fact.", "area": "ipc", "tags": ["ipc"],
                "platform": "all", "recorded": "2026-10-31", "status": "verified", "verified": "2026-10-31",
                "source": PROOF, "related": []}
        data.update(updates)
        return relative, data

    def register(self, relative, cap):
        with (self.root / ".agents/instruction-budget.tsv").open("a", encoding="utf-8") as stream:
            stream.write(f"{relative}\tevidence\t{cap}\tone-fact\tquery:ipc\n")

    def test_registered_large_reciprocal_metadata_and_lazy_describe(self):
        old, data = self.data(status="superseded", verified="unknown", status_changed="2026-10-31",
                              status_reason="The cited replacements preserve each scope.", status_evidence=PROOF,
                              size_exception={"tracker": "KEL-264", "reason": "Keep mandatory reciprocal identities together."})
        replacements = []
        for number in range(36):
            path, replacement = self.data(f"b{number:02d}", supersedes=[old])
            fixtures.write_record(self.root, path, replacement, "\nKeep this fact.\n")
            replacements.append(path)
        data["superseded_by"] = replacements
        fixtures.write_record(self.root, old, data, "\nPreserve the complete replacement identities.\n")
        file = self.root / old;raw = file.read_bytes();boundary = fixtures.frontmatter_boundary(raw)
        self.assertGreater(boundary, 2048)
        self.register(old, len(raw))
        self.assertEqual(lr.check(self.root), 205)
        # Only the requested header can be read, even when its permitted size is larger.
        (self.root / replacements[0]).write_text("PEER HEADER MUST NOT LOAD", encoding="utf-8")
        real_open = lr.open_record;calls = []
        class Guarded:
            def __enter__(reader):return reader
            def __exit__(reader, *unused):reader.stream.close()
            def readline(reader, size=-1):
                self.assertLess(reader.stream.tell(), boundary)
                line = reader.stream.readline(size);self.assertLessEqual(reader.stream.tell(), boundary)
                return line
            def read(reader, *unused):raise AssertionError("body read")
        def opened(path):
            self.assertEqual(path, file);calls.append(path)
            reader = Guarded();reader.stream = real_open(path);return reader
        with patch.object(lr, "open_record", opened):
            described = lr.describe(self.root, [old])[0]
        self.assertEqual(calls, [file]);self.assertEqual(described["superseded_by"], replacements)

    def test_default_and_registered_metadata_exact_cap_plus_one(self):
        for cap in [2048, 4096]:
            with self.subTest(cap=cap):
                relative, data = self.data("cap-" + str(cap), description="x")
                if cap > 2048:
                    data["size_exception"] = {"tracker": "KEL-264", "reason": "Preserve the complete indivisible metadata."}
                    self.register(relative, cap)
                fixtures.write_record(self.root, relative, data, "")
                file = self.root / relative
                data["description"] += "x" * (cap - file.stat().st_size)
                fixtures.write_record(self.root, relative, data, "")
                self.assertEqual(file.stat().st_size, cap)
                self.assertEqual(lr.metadata(self.root, relative)["description"], data["description"])
                data["description"] += "x";fixtures.write_record(self.root, relative, data, "")
                with self.assertRaisesRegex(lr.LearningError, f"frontmatter exceeds {cap}"):
                    lr.metadata(self.root, relative)

    def test_larger_read_needs_both_registration_and_exception(self):
        relative, data = self.data(description="x" * 2050,
                                  size_exception={"tracker": "KEL-264", "reason": "Preserve one fact."})
        fixtures.write_record(self.root, relative, data, "")
        with self.assertRaisesRegex(lr.LearningError, "frontmatter exceeds 2048"):
            lr.describe(self.root, [relative])
        self.register(relative, (self.root / relative).stat().st_size)
        del data["size_exception"];fixtures.write_record(self.root, relative, data, "")
        with self.assertRaisesRegex(lr.LearningError, "size_exception"):
            lr.describe(self.root, [relative])

    def test_budget_inventory_is_read_once_per_batch(self):
        one, data = self.data();fixtures.write_record(self.root, one, data, "\nOne fact.\n")
        two, data = self.data("b");fixtures.write_record(self.root, two, data, "\nAnother fact.\n")
        with patch.object(lr, "inventory_caps", wraps=lr.inventory_caps) as load:
            self.assertEqual(lr.check(self.root), 170)
            self.assertEqual(load.call_count, 1)
        with patch.object(lr, "inventory_caps", wraps=lr.inventory_caps) as load:
            self.assertEqual(len(lr.describe(self.root, [one, two])), 2)
            self.assertEqual(load.call_count, 1)

    def test_existing_read_witnesses_accept_lf_and_crlf(self):
        cases = [(fixtures.LearningRecordsTests, "test_describe_reads_only_requested_frontmatter", "add_new"),
                 (states.LearningLifecycleTests, "test_describe_retirement_without_peer_or_body_reads", "add")]
        for owner, method, create in cases:
            owner.setUpClass()
            try:
                for crlf in [False, True]:
                    with self.subTest(witness=method, crlf=crlf):
                        case = owner(method)
                        if crlf:
                            def wrap(original):
                                def converted(*args, **kwargs):
                                    result = original(*args, **kwargs)
                                    relative = result if isinstance(result, str) else args[0]
                                    path = case.root / relative
                                    path.write_bytes(path.read_bytes().replace(b"\r\n", b"\n").replace(b"\n", b"\r\n"))
                                    return result
                                return converted
                            setattr(case, create, wrap(getattr(case, create)))
                            case.change = wrap(case.change)
                        result = unittest.TestResult();case.run(result)
                        self.assertTrue(result.wasSuccessful(), str(result.errors + result.failures))
            finally:
                owner.tearDownClass()


if __name__ == "__main__":
    unittest.main()

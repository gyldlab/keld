"""Real temporary-repository tests for the learning record contract."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
import shutil
import tempfile
import unittest
from unittest.mock import patch

import learning_records as lr

REPO = Path(__file__).resolve().parents[1]


def frontmatter_boundary(raw: bytes) -> int:
    match = re.search(br"\r?\n---(?:\r?\n|$)", raw)
    if match is None:
        raise AssertionError("fixture has no closing frontmatter delimiter")
    return match.end()


def write_record(root: Path, relative: str, data: dict, body: str) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    header = "---\n" + "".join(f"{key}: {json.dumps(value, ensure_ascii=False)}\n" for key, value in data.items()) + "---\n"
    path.write_text(header + body, encoding="utf-8")


def make_fixture(root: Path, original: str) -> list[str]:
    directory = root / lr.LEARNING_ROOT
    directory.mkdir(parents=True)
    (directory / "AGENTS.md").write_text("# Fixture format owner\n", encoding="utf-8")
    (root / lr.SOURCE_PATH).write_text("# Learning links\n\n- [Rules](learnings/AGENTS.md)\n", encoding="utf-8")
    segments = []
    paths = []
    inventory = []
    position = 0
    matches = list(re.finditer(r"(?m)^- (\d{4}-\d{2}-\d{2}) \[([^]\n]+)\] [^\n]+", original))
    if len(matches) != 168:
        raise AssertionError("independent source fixture lost original entries")
    for number, match in enumerate(matches, 1):
        date, label = match.groups()
        area = label.split("/")[0]
        name = f"{date}-legacy-fact-{number:03d}"
        relative = f"{lr.LEARNING_ROOT}/{area}/{name}.md"
        line = original[:match.start()].count("\n") + 1
        data = {"name": name, "description": f"Retain historical fact {number}.", "area": area,
                "tags": [area], "platform": "unspecified", "recorded": date,
                "status": "historical", "verified": "unknown",
                "source": f"https://github.com/gyldlab/keld/blob/{lr.SOURCE_COMMIT}/{lr.SOURCE_PATH}#L{line}", "related": []}
        body = "\n## Fact\n\nKeep this historical condition.\n\n## Historical source\n\n> " + match[0] + "\n"
        write_record(root, relative, data, body)
        cap = 2048
        if (root / relative).stat().st_size > cap:
            data["size_exception"] = {"tracker": "KEL-264", "reason": "Preserve the indivisible historical fact."}
            write_record(root, relative, data, body)
            cap = (root / relative).stat().st_size
        inventory.append(f"{relative}\tevidence\t{cap}\tlearning-evidence\tquery:{area}\n")
        segments.extend([{"literal": original[position:match.start()]}, {"record": relative}])
        position = match.end()
        paths.append(relative)
    segments.append({"literal": original[position:]})
    ledger = {"schema": "keld.learning-migration/v1", "source_commit": lr.SOURCE_COMMIT,
              "source_path": lr.SOURCE_PATH, "source_sha256": lr.SOURCE_SHA256, "segments": segments}
    (directory / "migration.json").write_text(json.dumps(ledger, ensure_ascii=False), encoding="utf-8")
    (root / ".agents").mkdir()
    (root / ".agents/instruction-budget.tsv").write_text("".join(inventory), encoding="utf-8")
    return paths


def original_source() -> str:
    # The literal digest remains independent of the checker and candidate data.
    expected = "aa1c76e9079a06e4125d5fa74d5e395c19949266f35c9c68c830e463be5d05fc"
    original = (REPO / lr.SOURCE_PATH).read_bytes()
    if hashlib.sha256(original).hexdigest() != expected:
        ledger = json.loads((REPO / lr.LEARNING_ROOT / "migration.json").read_text(encoding="utf-8"))
        parts = []
        for segment in ledger["segments"]:
            if "literal" in segment:
                parts.append(segment["literal"])
            else:
                text = (REPO / segment["record"]).read_text(encoding="utf-8")
                quoted = re.findall(r"(?m)^> (- \d{4}-\d{2}-\d{2} \[[^]\n]+\] [^\n]+)$", text)
                if len(quoted) != 1:
                    raise AssertionError("fixture must retain one exact old line")
                parts.append(quoted[0])
        original = "".join(parts).encode("utf-8")
    if hashlib.sha256(original).hexdigest() != expected:
        raise AssertionError("immutable independent source oracle differs")
    return original.decode("utf-8")


class LearningRecordsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.original = original_source()
        cls.base = tempfile.TemporaryDirectory(prefix="learning-record-base-")
        cls.paths = make_fixture(Path(cls.base.name), cls.original)

    @classmethod
    def tearDownClass(cls):
        cls.base.cleanup()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="learning-record-test-")
        self.root = Path(self.temp.name) / "repo"
        shutil.copytree(self.base.name, self.root)

    def tearDown(self):
        self.temp.cleanup()

    def change(self, relative: str, **changes):
        path = self.root / relative
        with lr.open_record(path) as stream:
            data = lr.frontmatter(stream)
        body = path.read_text(encoding="utf-8").split("\n---\n", 1)[1]
        data.update(changes)
        write_record(self.root, relative, data, body)

    def ledger(self):
        path = self.root / lr.LEARNING_ROOT / "migration.json"
        return path, json.loads(path.read_text(encoding="utf-8"))

    def rejects(self, pattern):
        with self.assertRaisesRegex(lr.LearningError, pattern):
            lr.check(self.root)

    def add_new(self, name="2026-10-31-new-fact"):
        relative = f"{lr.LEARNING_ROOT}/ipc/{name}.md"
        data = {"name": name, "description": "Keep one transport identity.", "area": "ipc", "tags": ["ipc", "64-bit"],
                "platform": "all", "recorded": name[:10], "status": "verified", "verified": "2026-10-31",
                "source": f"https://github.com/gyldlab/keld/blob/{lr.SOURCE_COMMIT}/crates/keld-ipc/src/lib.rs#L1", "related": []}
        write_record(self.root, relative, data, "\nKeep one transport identity.\n")
        return relative

    def test_complete_source_and_new_records_pass(self):
        self.assertEqual(lr.check(self.root), 168)
        self.add_new()
        self.assertEqual(lr.check(self.root), 169)

    def test_originally_allowed_deps_area_remains_available(self):
        self.assertIn("`deps`", original_source().split("## Log", 1)[0])
        original = self.add_new()
        relative = original.replace("/ipc/", "/deps/")
        target = self.root / relative;target.parent.mkdir()
        (self.root / original).rename(target)
        self.change(relative, area="deps", tags=["deps"])
        self.assertEqual(lr.check(self.root), 169)

    def test_missing_or_changed_old_fact_rejects(self):
        path = self.root / self.paths[0]
        original = path.read_text(encoding="utf-8")
        path.unlink()
        self.rejects("missing migration record")
        path.write_text(original.replace("> - ", "> changed - ", 1), encoding="utf-8")
        self.rejects("invalid quoted legacy entry")
        path.write_text(original.replace(" (evidence:", " changed (evidence:", 1), encoding="utf-8")
        self.rejects("source hash mismatch")

    def test_ledger_drop_duplicate_extra_and_fixed_oracle(self):
        path, original = self.ledger()
        for mutation, expected in [
            (lambda d: d["segments"].pop(1), "missing/extra historical"),
            (lambda d: d["segments"].append({"record": self.paths[0]}), "duplicate migration"),
            (lambda d: d.update(source_sha256="0" * 64), "fixed original source"),
        ]:
            with self.subTest(expected=expected):
                data = json.loads(json.dumps(original));mutation(data)
                path.write_text(json.dumps(data), encoding="utf-8")
                self.rejects(expected)
        path.write_text(json.dumps(original), encoding="utf-8")
        extra = self.add_new()
        self.change(extra, source=lr.HISTORICAL_PREFIX + "999",
                    verification_evidence=f"https://github.com/gyldlab/keld/blob/{lr.SOURCE_COMMIT}/crates/keld-ipc/src/lib.rs#L1")
        self.rejects("missing/extra historical")

    def test_body_mutation_and_literal_entry_cannot_hide_loss(self):
        path, data = self.ledger()
        data["segments"][1] = {"literal": "- 2026-08-13 [ipc] fabricated old fact"}
        path.write_text(json.dumps(data), encoding="utf-8")
        self.rejects("must be mapped")

    def test_bad_metadata_closed_keys_types_and_dates(self):
        relative = self.add_new();path = self.root / relative;original = path.read_bytes()
        for fields, expected in [
            ({"extra": True}, "unknown fields"), ({"description": "word " * 26}, "25 words"),
            ({"description": "two\nlines"}, "one line"), ({"tags": []}, "tags"),
            ({"description": "two\u2028lines"}, "one line"),
            ({"tags": ["Bad Tag"]}, "tags"), ({"related": "not-list"}, "path list"),
            ({"recorded": "2026-02-30"}, "real ISO"), ({"recorded": "2026-10-30"}, "match filename"),
            ({"verified": True}, "ISO date"), ({"platform": "bsd"}, "platform"),
            ({"source": "https://github.com/gyldlab/keld/blob/main/file#L1"}, "immutable"),
            ({"name": "other"}, "filename stem"), ({"area": "host"}, "parent directory"),
        ]:
            with self.subTest(fields=fields):
                path.write_bytes(original);self.change(relative, **fields);self.rejects(expected)

    def test_duplicate_header_and_nested_json_keys_reject(self):
        path = self.root / self.add_new();original = path.read_text(encoding="utf-8")
        path.write_text(original.replace("---\n", '---\nname: "duplicate"\n', 1), encoding="utf-8")
        self.rejects("duplicate frontmatter")
        path.write_text(original.replace("---\n", '---\nsize_exception: {"tracker":"KEL-264","tracker":"KEL-1","reason":"one fact"}\n', 1), encoding="utf-8")
        self.rejects("duplicate JSON key")

    def test_related_cycle_does_not_follow_bodies(self):
        first = self.add_new();second = self.add_new("2026-10-31-second-fact")
        self.change(first, related=[second]);self.change(second, related=[first])
        self.assertEqual(lr.check(self.root), 170)

    def test_broken_escape_symlink_self_and_nonrecord_links_reject(self):
        relative = self.add_new();path = self.root / relative;original = path.read_bytes()
        for link, expected in [(relative, "self related"),
                               (f"{lr.LEARNING_ROOT}/ipc/2026-10-31-absent.md", "missing link"),
                               (f"{lr.LEARNING_ROOT}/ipc/../outside.md", "unsafe link"),
                               ("docs/specs/other.md", "escapes learning root")]:
            path.write_bytes(original);self.change(relative, related=[link]);self.rejects(expected)
        path.write_bytes(original)
        link = self.root / lr.LEARNING_ROOT / "ipc/2026-10-31-alias.md"
        link.symlink_to(path)
        self.rejects("symlink refused")

    def test_detail_plain_markdown_is_not_a_record(self):
        relative = self.add_new();detail = f"{lr.LEARNING_ROOT}/details/explanation.md"
        target = self.root / detail;target.parent.mkdir();target.write_text("# Evidence detail\n", encoding="utf-8")
        self.change(relative, detail=[detail])
        self.assertEqual(lr.check(self.root), 169)
        self.change(relative, related=[detail]);self.rejects("invalid record path")

    def test_exact_cap_max_plus_one_and_indivisible_exception(self):
        relative = self.add_new();path = self.root / relative
        path.write_bytes(path.read_bytes() + b"x" * (2048 - path.stat().st_size))
        self.assertEqual(lr.check(self.root), 169)
        path.write_bytes(path.read_bytes() + b"x");self.rejects("exceeds 2048")
        self.change(relative, size_exception={"tracker": "KEL-264", "reason": "Preserve the complete indivisible fact."})
        self.rejects("exact inventory allowance")
        with (self.root / ".agents/instruction-budget.tsv").open("a", encoding="utf-8") as stream:
            stream.write(f"{relative}\tevidence\t{path.stat().st_size}\tone-fact\tquery:ipc\n")
        self.assertEqual(lr.check(self.root), 169)
        path.write_bytes(path.read_bytes() + b"x");self.rejects("record exceeds")

    def test_unique_names_across_areas(self):
        relative = self.add_new();other = relative.replace("/ipc/", "/host/")
        target = self.root / other;target.parent.mkdir(exist_ok=True);shutil.copyfile(self.root / relative, target)
        self.change(other, area="host");self.rejects("names must be unique")

    def test_describe_reads_only_requested_frontmatter(self):
        selected = self.add_new();unselected = self.add_new("2026-10-31-unselected-fact")
        path = self.root / selected;raw = path.read_bytes();boundary = frontmatter_boundary(raw)
        path.write_bytes(raw + b"BODY_MUST_NOT_BE_READ" * 1000)
        (self.root / unselected).write_text("not metadata", encoding="utf-8")
        calls = []
        real_open = lr.open_record
        class GuardedReader:
            def __enter__(inner):return inner
            def __exit__(inner, *unused):inner.stream.close()
            def readline(inner, size=-1):
                self.assertLess(inner.stream.tell(), boundary, "body read attempted")
                line = inner.stream.readline(size);self.assertLessEqual(inner.stream.tell(), boundary)
                return line
            def read(inner, *unused):raise AssertionError("whole record read attempted")
        def guarded(actual):
            self.assertEqual(actual, path);calls.append(actual)
            reader = GuardedReader();reader.stream = real_open(actual);return reader
        with patch.object(lr, "open_record", guarded):
            result = lr.describe(self.root, [selected])
        self.assertEqual(calls, [path]);self.assertEqual(len(result), 1)
        self.assertEqual(set(result[0]), {"filename", "name", "description", "tags", "platform", "source", "status", "verified", "retired"})
        self.assertEqual(result[0]["filename"], selected)

    def test_describe_exposes_platform_and_source_changes(self):
        selected = self.add_new()
        first = lr.describe(self.root, [selected])[0]
        self.assertEqual(first["platform"], "all")
        self.change(selected, platform="windows")
        second = lr.describe(self.root, [selected])[0]
        self.assertEqual(second["platform"], "windows")
        changed = second["source"].replace("#L1", "#L2")
        self.change(selected, source=changed)
        third = lr.describe(self.root, [selected])[0]
        self.assertEqual(third["source"], changed)
        self.assertNotEqual(first, second)
        self.assertNotEqual(second, third)

    def test_frontmatter_limit_and_old_layout_reject(self):
        relative = self.add_new();path = self.root / relative
        path.write_bytes(b"---\nname: " + b"x" * 2048)
        self.rejects("frontmatter exceeds")
        with tempfile.TemporaryDirectory(prefix="learning-old-layout-") as temp:
            root = Path(temp);old = root / lr.SOURCE_PATH;old.parent.mkdir(parents=True)
            old.write_text(self.original, encoding="utf-8")
            with self.assertRaisesRegex(lr.LearningError, "missing learning record root"):
                lr.check(root)


if __name__ == "__main__":
    unittest.main()

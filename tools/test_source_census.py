#!/usr/bin/env python3
"""Independent boundary and live-Git tests for pinned source census."""

import os
import tempfile
import unittest
from unittest import mock
from pathlib import Path

import source_census as census


class CounterContracts(unittest.TestCase):
    def test_empty_and_line_ending_oracles(self):
        cases = [
            (b"", 0), (b"\n", 1), (b"hello", 1), (b"hello\n", 1),
            (b"hello\r", 1), (b"hello\r\n", 1), (b"a\r\nb", 2),
            (b"a\nb\n", 2), (b"a\rb\r", 2), (b"a\r\n\r\nb", 3),
            (b"last\n\n", 2), (b"\r\n", 1),
            ("caf\u00e9\nnext".encode("utf-8"), 2),
        ]
        for raw, expected in cases:
            with self.subTest(raw=raw):
                self.assertEqual(expected, census.physical_lines(raw))

    def test_tree_parser_keeps_metadata_and_non_ascii_path(self):
        raw = (b"100644 blob " + b"f" * 40 + b"\tcaf\xc3\xa9.rs\0"
               + b"120000 blob " + b"1" * 40 + b"\tlink\0"
               + b"160000 commit " + b"2" * 40 + b"\tsubmodule\0")
        e = census.parse_tree(raw)
        self.assertEqual(["caf\u00e9.rs", "link", "submodule"], [r["path"] for r in e])
        self.assertEqual(["100644", "120000", "160000"], [r["mode"] for r in e])
        with self.assertRaises(ValueError):
            census.parse_tree(raw + raw)

    def test_history_count_preserves_newline_unicode_and_duplicates(self):
        raw = "normal.rs\0line\nbreak.rs\0café.rs\0normal.rs\0".encode()
        result = census.change_counts(raw)
        self.assertEqual(2, result["normal.rs"])
        self.assertEqual(1, result["line\nbreak.rs"])
        self.assertEqual(1, result["café.rs"])
        self.assertEqual(3, len(result))
        self.assertNotIn("line", result)
        self.assertNotIn("break.rs", result)

    def test_exclusions_are_explicit(self):
        cases = [
            ("Cargo.lock", "100644", b"x", "F", "lock"),
            ("crates/a/fuzz/corpus/in", "100644", b"x", "F", "asset"),
            ("crates/a/tests/fixtures/data.rs", "100644", b"fn main() {}", "F", "fixture"),
            ("foo.rs", "100644", b"\x00", "F", "binary"),
            ("foo.rs", "100644", b"\xff", "F", "non-utf8"),
            ("arbitrary", "120000", b"relative/target", "F", "symlink"),
            ("submodule", "160000", None, "F", "gitlink"),
        ]
        for path, mode, data, category, encoding in cases:
            with self.subTest(path=path, encoding=encoding):
                got = census.classify(path, mode, data)
                self.assertEqual((category, encoding), (got[0], got[3]))
                self.assertTrue(got[2], "F must have specific exclusion reason")

    def test_generated_census_outputs_do_not_enter_authored_budgets(self):
        prefix = "docs/audits/source-census-2026-10-15/"
        for name in ("README.md", "summary.json", "manifest.csv"):
            category, reason, exclusion, encoding = census.classify(
                prefix + name, "100644", b"generated")
            self.assertEqual("F", category)
            self.assertEqual("generated", encoding)
            self.assertIn("source_census.py", exclusion)
        for name in ("review-notes.md", "analysis.json"):
            category, _, _, _ = census.classify(prefix + name,
                                                  "100644", b"human notes")
            self.assertNotEqual("F", category)
        self.assertEqual("E", census.classify(
            "docs/audits/site-audit.md", "100644", b"human authored")[0])

    def test_classify_entrypoints_tests_and_tooling(self):
        cases = {
            "crates/keld-runtime/src/lib.rs": "A",
            "crates/keld-update/src/windows_baseline/tests/transaction.rs": "C",
            "crates/keld-ipc/src/bootstrap.rs": "B",
            "tools/ci_hygiene.rs": "D",
            ".github/CODEOWNERS": "D",
            ".agents/instruction-budget.tsv": "D",
            "tools/ci_changes_test.sh": "C",
            ".github/workflows/windows.yml": "D",
            "docs/specs/auth.md": "E",
            "crates/keld-cli/templates/hello/src/main-body.ts": "F",
        }
        for path, expected in cases.items():
            with self.subTest(path=path):
                self.assertEqual(expected, census.classify(path, "100644", b"line\n")[0])

    def test_owner_local_test_directories_are_not_production(self):
        # Each path is introduced by an existing stacked architecture PR.
        test_paths = (
            "crates/keld-ipc/src/bootstrap/named_pipe_tests/generation_deadline.rs",
            "crates/keld-ipc/src/bootstrap/named_pipe_tests/stream_shutdown.rs",
            "crates/keld-ipc/src/bootstrap/named_pipe_tests/server_cancellation.rs",
            "crates/keld-ipc/src/bootstrap/named_pipe_tests/cancellation_ownership.rs",
            "crates/keld-ipc/src/link/validated_read_tests/capacity.rs",
        )
        for path in test_paths:
            with self.subTest(path=path):
                category, _, exclusion, encoding = census.classify(
                    path, "100644", b"#[test]\nfn contract() {}\n")
                self.assertEqual(("C", "", "utf-8"),
                                 (category, exclusion, encoding))
        # A substring match would accidentally recategorize production.
        for path in (
            "crates/keld-ipc/src/bootstrap/named_pipe.rs",
            "crates/keld-ipc/src/bootstrap/named_pipe_tests_helper.rs",
            "crates/keld-ipc/src/link/validated_read.rs",
            "crates/keld-ipc/src/link/validated_read_tests_helper.rs",
        ):
            with self.subTest(path=path):
                self.assertEqual("B", census.classify(
                    path, "100644", b"fn production() {}\n")[0])
        # Fixture exclusions still take precedence within test-only modules.
        self.assertEqual("F", census.classify(
            "crates/keld-ipc/src/bootstrap/named_pipe_tests/fixtures/input.rs",
            "100644", b"fixture")[0])

    def test_inline_test_ratio_is_explicitly_only_tail_proxy(self):
        body = b"fn production() {}\n#[cfg(test)]\nmod tests { fn t() {} }\n"
        e = [dict(path="crates/abc/src/code.rs", mode="100644", kind="blob", blob_sha="a")]
        r = census.collect(e, {"a": body})[0]
        self.assertTrue(r["inline_test_flag"])
        self.assertEqual(2, r["inline_test_tail_lines"])
        self.assertEqual(0.6667, r["inline_test_tail_ratio"])
        self.assertEqual(3, r["first_test_module_line"])
        self.assertEqual(0.3333, r["module_tail_ratio"])

    def test_hard_limit_union_and_edge(self):
        rows = [
            dict(path="foo.rs", category="B", physical_lines=1200, bytes=65536),
            dict(path="bar.rs", category="B", physical_lines=1201, bytes=200),
            dict(path="baz.rs", category="B", physical_lines=20, bytes=65537),
            dict(path="win.rs", category="B", physical_lines=1201, bytes=65537),
        ]
        i = [x for x in census.policy_impact(rows, census.CANDIDATE, "candidate")
             if x["category"] == "B"][0]
        self.assertEqual(2, i["hard_over_lines"])
        self.assertEqual(2, i["hard_over_bytes"])
        self.assertEqual(3, i["union_existing_grandfathered"])

    def test_alternate_counter_falsifies_wrong_record(self):
        e = [dict(path="src.rs", mode="100644", kind="blob", blob_sha="a")]
        records = census.collect(e, {"a": b"one\r\ntwo"})
        census.independently_validate(records, {"a": b"one\r\ntwo"})
        records[0]["physical_lines"] = 9
        with self.assertRaises(AssertionError):
            census.independently_validate(records, {"a": b"one\r\ntwo"})


class GitObjectProof(unittest.TestCase):
    @staticmethod
    def commit(repo, msg):
        return census.git(repo, "-c", "user.name=Keld CI", "-c",
                          "user.email=keld-ci@example.invalid", "commit", "-qm", msg)

    def test_real_tree_rename_symlink_and_blob_integrity(self):
        with tempfile.TemporaryDirectory(prefix="keld-census-test-") as tmp:
            repo = Path(tmp)
            census.git(repo, "init", "-q")
            census.git(repo, "config", "maintenance.auto", "false")
            (repo / "before.rs").write_bytes(b"first\r\nsecond")
            (repo / "empty.rs").write_bytes(b"")
            census.git(repo, "add", "before.rs", "empty.rs")
            self.commit(repo, "baseline")
            orig = census.parse_tree(census.git(repo, "ls-tree", "-r", "-z", "HEAD"))
            self.assertEqual(["before.rs", "empty.rs"], sorted(x["path"] for x in orig))
            # Construct a tracked link directly in Git's index. Windows symlink
            # filesystem privileges cannot affect this test of Git object semantics.
            link_sha = census.git(repo, "hash-object", "-w", "--stdin",
                                  data=b"before.rs").decode().strip()
            census.git(repo, "update-index", "--add", "--cacheinfo",
                       "120000," + link_sha + ",link")
            self.commit(repo, "index symlink")
            entries = census.parse_tree(census.git(repo, "ls-tree", "-r", "-z", "HEAD"))
            blobs = census.read_blobs(repo, entries)
            self.assertEqual(3, len(entries))
            records = census.collect(entries, blobs)
            self.assertEqual("F", next(r["category"] for r in records if r["path"] == "link"))
            self.assertEqual(2, next(r["physical_lines"] for r in records
                                     if r["path"] == "before.rs"))
            self.assertEqual(0, next(r["physical_lines"] for r in records
                                     if r["path"] == "empty.rs"))
            self.assertEqual(3, census.summarize(records, "test", {}, "2026-10-15", {})
                             ["tracked_manifest_entries"])
            census.git(repo, "mv", "before.rs", "renamed.rs")
            self.commit(repo, "rename")
            changed = census.parse_tree(census.git(repo, "ls-tree", "-r", "-z", "HEAD"))
            self.assertEqual(3, len(changed))
            self.assertIn("renamed.rs", [e["path"] for e in changed])
            self.assertNotIn("before.rs", [e["path"] for e in changed])
            self.assertEqual(0, census.independently_validate(census.collect(
                changed, census.read_blobs(repo, changed)),
                census.read_blobs(repo, changed))["mismatches"])

    def test_git_helper_ignores_inherited_repository_locations(self):
        with tempfile.TemporaryDirectory(prefix="keld-census-hook-") as temp:
            base = Path(temp)
            caller, isolated = base / "caller", base / "isolated"
            caller.mkdir()
            isolated.mkdir()
            census.git(caller, "init", "-q")
            census.git(caller, "config", "maintenance.auto", "false")
            (caller / "caller.rs").write_text("caller")
            census.git(caller, "add", "caller.rs")
            self.commit(caller, "caller baseline")
            original_head = census.git(caller, "rev-parse", "HEAD")
            original_index = (caller / ".git" / "index").read_bytes()
            (isolated / "only_here.rs").write_text("isolated")
            with mock.patch.dict(os.environ, {
                "GIT_DIR": str(caller / ".git"),
                "GIT_WORK_TREE": str(caller),
                "GIT_INDEX_FILE": str(caller / ".git" / "index"),
            }):
                census.git(isolated, "init", "-q")
                census.git(isolated, "config", "maintenance.auto", "false")
                census.git(isolated, "add", "only_here.rs")
                self.commit(isolated, "isolated baseline")
                entries = census.parse_tree(
                    census.git(isolated, "ls-tree", "-r", "-z", "HEAD"))
            self.assertEqual(["only_here.rs"], [e["path"] for e in entries])
            self.assertEqual(original_head, census.git(caller, "rev-parse", "HEAD"))
            self.assertEqual(original_index, (caller / ".git" / "index").read_bytes())
            self.assertEqual(b"", census.git(caller, "status", "--porcelain"))

    def test_independent_manifest_output_from_pinned_commit(self):
        with tempfile.TemporaryDirectory(prefix="keld-census-test-") as tmp:
            repo = Path(tmp)
            census.git(repo, "init", "-q")
            census.git(repo, "config", "maintenance.auto", "false")
            (repo / "source.rs").write_bytes(b"line 1\nline 2")
            census.git(repo, "add", "source.rs")
            self.commit(repo, "baseline")
            before = census.git(repo, "rev-parse", "HEAD").decode().strip()
            (repo / "source.rs").write_bytes(b"CHANGED BY WORKING TREE")
            entries = census.parse_tree(census.git(repo, "ls-tree", "-r", "-z", before))
            blobs = census.read_blobs(repo, entries)
            row = census.collect(entries, blobs)[0]
            self.assertEqual(2, row["physical_lines"])
            self.assertEqual(len(b"line 1\nline 2"), row["bytes"])
            self.assertEqual(before, census.git(repo, "rev-parse", "HEAD").decode().strip())


if __name__ == "__main__":
    unittest.main(verbosity=2)

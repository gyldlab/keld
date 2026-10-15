#!/usr/bin/env python3
"""Negative controls for added Rust test wildcard imports."""
import subprocess
import tempfile
import unittest
from pathlib import Path

import test_import_guard as guard

NEW = "crates/keld-ipc/src/link/tests/handshake.rs"
NESTED = "crates/keld-ipc/src/bootstrap/named_pipe_tests/generation_deadline.rs"
EXISTING = "crates/keld-ipc/src/link/deadline_contract_tests.rs"
PRODUCTION = "crates/keld-ipc/src/link.rs"


class TestDiffGuard(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="keld-test-import-")
        self.addCleanup(self.tmp.cleanup)
        self.repo = Path(self.tmp.name)
        self.run_git("init", "-q")
        self.run_git("config", "user.name", "Test Import Guard")
        self.run_git("config", "user.email", "test-import-guard@example.invalid")

    def run_git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.repo), *args],
                                       stderr=subprocess.PIPE).decode().strip()

    def put(self, path, data):
        f = self.repo / path
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text(data, encoding="utf-8")

    def commit(self):
        self.run_git("add", "-A")
        self.run_git("commit", "-qm", "boundary")
        return self.run_git("rev-parse", "HEAD")

    def test_new_local_module_glob_fails_and_explicit_import_passes(self):
        self.put(PRODUCTION, "pub fn example() {}\n")
        base = self.commit()
        self.put(NEW, "use super::*;\n#[test] fn check() {}\n")
        glob = self.commit()
        self.assertEqual([(NEW, 1)], guard.violations(self.repo, base, glob))
        self.put(NEW, "use super::{connected_pair, handshake_client};\n#[test] fn check() {}\n")
        explicit = self.commit()
        self.assertEqual([], guard.violations(self.repo, glob, explicit))

    def test_modified_existing_module_added_glob_fails(self):
        self.put(EXISTING, "use std::time::Duration;\n")
        base = self.commit()
        self.put(EXISTING, "use std::time::Duration;\nuse super::*;\n")
        head = self.commit()
        self.assertEqual([(EXISTING, 2)], guard.violations(self.repo, base, head))

    def test_unchanged_legacy_and_non_test_rust_do_not_block(self):
        self.put(EXISTING, "use super::*;\n")
        base = self.commit()
        self.put(EXISTING, "use super::*;\nfn extra() {}\n")
        self.put(PRODUCTION, "use super::*;\n")
        head = self.commit()
        self.assertEqual([], guard.violations(self.repo, base, head))

    def test_nested_test_paths_and_fixture_precedence(self):
        self.assertTrue(guard.is_owner_test(NESTED))
        self.assertTrue(guard.is_owner_test(NEW))
        self.assertFalse(guard.is_owner_test(PRODUCTION))
        self.assertFalse(guard.is_owner_test("crates/keld-ipc/src/bootstrap/named_pipe_tests_helper.rs"))
        self.put(PRODUCTION, "pub fn example() {}\n")
        base = self.commit()
        self.put(NESTED, "use super::*;\n")
        head = self.commit()
        self.assertEqual([(NESTED, 1)], guard.violations(self.repo, base, head))

    def test_comments_and_multiline_raw_strings_are_not_glob_imports(self):
        content = (
            "// use super::*;\n"
            "/* outer /* nested */ use super::*; */\n"
            'const FIXTURE: &str = r##"\n'
            "use super::*;\n"
            '"##;\n'
            "use super::{connected_pair};\n"
        )
        self.assertEqual(set(), guard.active_glob_lines(content))
        self.assertEqual({7}, guard.active_glob_lines(content + "use super::*;\n"))

    def test_braced_multiline_and_non_parent_glob_imports_refused(self):
        self.assertEqual({1}, guard.active_glob_lines("use super::{foo, *};\n"))
        self.assertEqual({1}, guard.active_glob_lines("use std::io::*;\n"))
        self.assertEqual({1, 2, 3}, guard.active_glob_lines(
            "use super::{\n    read_frame, *\n};\n"))
        self.put(PRODUCTION, "pub fn base() {}\n")
        base = self.commit()
        self.put(NEW, "use super::{read_frame, *};\n")
        head = self.commit()
        self.assertEqual([(NEW, 1)], guard.violations(self.repo, base, head))

    def test_source_only_legacy_removed_and_added_in_different_file(self):
        self.put(EXISTING, "use super::*;\n")
        base = self.commit()
        self.put(EXISTING, "use super::{a};\n")
        self.put(NEW, "use super::*;\n")
        head = self.commit()
        self.assertEqual([(NEW, 1)], guard.violations(self.repo, base, head))


if __name__ == "__main__":
    unittest.main()

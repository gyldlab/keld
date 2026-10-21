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
ROOT_TESTS = "crates/keld-update/src/tests.rs"
ROOT_NAMED_TESTS = "crates/keld-x/src/a_tests.rs"
ROOT_PRODUCTION = "crates/keld-x/src/lib.rs"


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

    def test_base_moving_after_fork_is_not_attributed_to_head(self):
        self.put(EXISTING, "use super::*;\n")
        fork = self.commit()
        self.put(EXISTING, "use super::{connected_pair};\n")
        moved_base = self.commit()
        self.run_git("checkout", "-q", fork)
        self.put(PRODUCTION, "pub fn example() {}\n")
        head = self.commit()
        self.assertEqual([], guard.violations(self.repo, moved_base, head))

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

    def test_crate_root_owner_test_paths_are_recognized(self):
        # Review finding on #746: crates/<c>/src/tests.rs and <name>_tests.rs have
        # four path parts and were skipped by a `len(parts) < 5` check.
        self.assertTrue(guard.is_owner_test(ROOT_TESTS))
        self.assertTrue(guard.is_owner_test("crates/keld-pack/src/tests.rs"))
        self.assertTrue(guard.is_owner_test(ROOT_NAMED_TESTS))
        # Deeper owner tests keep working.
        self.assertTrue(guard.is_owner_test("crates/keld-update/src/foo/tests.rs"))
        self.assertTrue(guard.is_owner_test("crates/keld-native/src/auth/browser_tests.rs"))
        # Negative controls: crate-root production, lookalikes and other layouts.
        self.assertFalse(guard.is_owner_test(ROOT_PRODUCTION))
        self.assertFalse(guard.is_owner_test("crates/keld-x/src/a_tests_helper.rs"))
        self.assertFalse(guard.is_owner_test("crates/keld-x/src/tests.txt"))
        self.assertFalse(guard.is_owner_test("crates/keld-x/tests.rs"))
        self.assertFalse(guard.is_owner_test("crates/keld-x/tests/integration.rs"))
        self.assertFalse(guard.is_owner_test("tools/tests.rs"))

    def test_crate_root_owner_test_added_glob_fails_and_production_does_not(self):
        self.put(ROOT_TESTS, "use std::time::Duration;\n")
        self.put(ROOT_PRODUCTION, "pub fn lib() {}\n")
        base = self.commit()
        self.put(ROOT_TESTS, "use std::time::Duration;\nuse super::*;\n")
        self.put(ROOT_NAMED_TESTS, "use super::*;\n")
        self.put(ROOT_PRODUCTION, "pub fn lib() {}\nuse super::*;\n")
        glob = self.commit()
        self.assertEqual([(ROOT_TESTS, 2), (ROOT_NAMED_TESTS, 1)],
                         sorted(guard.violations(self.repo, base, glob),
                                key=lambda found: found[0] != ROOT_TESTS))
        self.put(ROOT_TESTS, "use std::time::Duration;\nuse super::{helper};\n")
        self.put(ROOT_NAMED_TESTS, "use super::{helper};\n")
        explicit = self.commit()
        self.assertEqual([], guard.violations(self.repo, glob, explicit))

    def test_unchanged_crate_root_legacy_glob_does_not_block(self):
        self.put(ROOT_TESTS, "use super::*;\n")
        base = self.commit()
        self.put(ROOT_TESTS, "use super::*;\nfn extra() {}\n")
        head = self.commit()
        self.assertEqual([], guard.violations(self.repo, base, head))

    def test_quote_char_literals_do_not_hide_a_later_glob_import(self):
        # Review finding on #746: the `"` inside '"' / b'"' opened a fake string.
        self.assertEqual({2}, guard.active_glob_lines("const Q: char = '\"';\nuse super::*;\n"))
        self.assertEqual({2}, guard.active_glob_lines("let q = b'\"';\nuse super::*;\n"))
        # Escaped forms of the same literals.
        self.assertEqual({2}, guard.active_glob_lines("let q = '\\\"';\nuse super::*;\n"))
        self.assertEqual({2}, guard.active_glob_lines("let q = '\\u{22}';\nuse super::*;\n"))
        self.assertEqual({2}, guard.active_glob_lines("let q = b'\\x22';\nuse super::*;\n"))
        # A glob inside a real multi-line string after the literal stays masked: the
        # masker must not flip string/code state in either direction.
        self.assertEqual(set(), guard.active_glob_lines(
            "let q = '\"';\nlet s = \"\nuse super::*;\n\";\n"))
        # The glob is still found when the literal and the import share a line range.
        self.assertEqual({3}, guard.active_glob_lines(
            "fn f() { let a = '\"'; let b = '\"'; }\n\nuse super::*;\n"))

    def test_char_literal_handling_keeps_strings_comments_and_lifetimes_masked(self):
        # Negative controls: the char-literal state must not unmask anything real.
        self.assertEqual(set(), guard.active_glob_lines("let q = '\"'; // use super::*;\n"))
        self.assertEqual(set(), guard.active_glob_lines("let q = '\"'; /* use super::*; */\n"))
        # An escaped single-quote char literal must not swallow the next line.
        self.assertEqual({2}, guard.active_glob_lines("let q = '\\'';\nuse super::*;\n"))
        self.assertEqual({2}, guard.active_glob_lines("let q = '\\\\';\nuse super::*;\n"))
        # Lifetimes and labels stay in the normal state and never hide code.
        self.assertEqual({2}, guard.active_glob_lines(
            "fn f<'a>(x: &'a str) {}\nuse super::*;\n"))
        self.assertEqual({2}, guard.active_glob_lines(
            "struct S<'a, 'b>(&'a u8, &'b u8);\nuse super::*;\n"))
        self.assertEqual({2}, guard.active_glob_lines(
            "fn g() { 'outer: loop { break 'outer; } }\nuse super::*;\n"))
        # A plain glob is unchanged.
        self.assertEqual({1}, guard.active_glob_lines("use super::*;\n"))

    def test_string_line_continuation_keeps_line_numbers(self):
        # Found while validating the masker over every tracked Rust file: a string
        # literal ending a line with a backslash (line continuation) dropped one
        # newline from the masked text, so every later line number drifted (up to
        # 18 lines in crates/keld-guard/src/lib.rs).
        content = 'const MSG: &str = "first \\\n    second \\\n    third";\nuse super::*;\n'
        self.assertEqual({4}, guard.active_glob_lines(content))
        crlf = 'const MSG: &str = "first \\\r\n    second";\r\nuse super::*;\r\n'
        self.assertEqual({3}, guard.active_glob_lines(crlf))

    def test_added_glob_after_string_line_continuation_is_still_flagged(self):
        # False negative before the fix: the added glob was reported one line early,
        # so it never intersected the added line and the gate passed.
        self.put(NEW, 'const MSG: &str = "first \\\n    second";\n')
        base = self.commit()
        self.put(NEW, 'const MSG: &str = "first \\\n    second";\nuse super::*;\n')
        head = self.commit()
        self.assertEqual([(NEW, 3)], guard.violations(self.repo, base, head))

    def test_legacy_glob_after_string_line_continuation_is_not_misattributed(self):
        # False positive before the fix: the legacy glob drifted onto the added line.
        self.put(NEW, 'const MSG: &str = "first \\\n    second";\nuse super::*;\n')
        base = self.commit()
        self.put(NEW, 'const MSG: &str = "first \\\n    second";\nfn added() {}\nuse super::*;\n')
        head = self.commit()
        self.assertEqual([], guard.violations(self.repo, base, head))

    def test_source_only_legacy_removed_and_added_in_different_file(self):
        self.put(EXISTING, "use super::*;\n")
        base = self.commit()
        self.put(EXISTING, "use super::{a};\n")
        self.put(NEW, "use super::*;\n")
        head = self.commit()
        self.assertEqual([(NEW, 1)], guard.violations(self.repo, base, head))


if __name__ == "__main__":
    unittest.main()

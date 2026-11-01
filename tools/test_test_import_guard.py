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
        self.run_git("config", "maintenance.auto", "false")
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

    def test_visibility_qualified_globs_are_rejected_on_added_lines(self):
        self.put(PRODUCTION, "pub fn example() {}\n")
        base = self.commit()
        self.put(NEW, (
            "pub use super::*;\n"
            "pub(crate) use crate::helpers::*;\n"
            "pub(self) use self::helpers::*;\n"
            "pub(super) use super::{named, *};\n"
            "pub(in crate::tests) use crate::support::*;\n"
        ))
        head = self.commit()
        self.assertEqual([(NEW, line) for line in range(1, 6)],
                         guard.violations(self.repo, base, head))

    def test_visibility_qualified_multiline_import_keeps_exact_added_line(self):
        self.put(EXISTING, "pub(crate) use super::{named};\n")
        base = self.commit()
        self.put(EXISTING, "pub(crate) use super::{named, *};\n")
        head = self.commit()
        self.assertEqual([(EXISTING, 1)], guard.violations(self.repo, base, head))
        self.assertEqual({1, 2, 3}, guard.active_glob_lines(
            "pub(in crate::tests)\nuse super::{\n    named, *};\n"))

    def test_pub_visibility_delimiters_can_follow_newlines_and_comments(self):
        # rustc accepts whitespace between pub and the visibility group.
        content = (
            "pub\n(crate) use crate::helpers::*;\n"
            "pub\n(in crate::tests) use crate::support::*;\n"
            "pub /* comment */ (super) use super::*;\n"
        )
        self.assertEqual({1, 2, 3, 4, 5}, guard.active_glob_lines(content))
        self.put(PRODUCTION, "pub fn example() {}\n")
        base = self.commit()
        self.put(NEW, content)
        head = self.commit()
        self.assertEqual([(NEW, line) for line in range(1, 6)],
                         guard.violations(self.repo, base, head))

    def test_pub_group_may_touch_use_and_use_body_may_start_next_line(self):
        # Rust token spacing need not follow the canonical rustfmt layout.
        content = (
            "pub(crate)use super::*;\n"
            "pub(in crate::tests)use crate::support::*;\n"
            "pub use\nsuper::*;\n"
            "use\ncrate::support::*;\n"
        )
        self.put(PRODUCTION, "pub fn example() {}\n")
        base = self.commit()
        self.put(NEW, content)
        head = self.commit()
        self.assertEqual({1, 2, 3, 4, 5, 6}, guard.active_glob_lines(content))
        self.assertEqual([(NEW, line) for line in range(1, 7)],
                         guard.violations(self.repo, base, head))

    def test_import_tree_tokens_need_no_space_after_use(self):
        # Verified with rustc --edition=2024: punctuation separates use tokens.
        content = (
            "use::std::io::*;\n"
            "pub(crate)use::std::io::*;\n"
            "use{crate::support::*};\n"
            "pub(super)use{crate::support::*};\n"
        )
        self.put(PRODUCTION, "pub fn example() {}\n")
        base = self.commit()
        self.put(NEW, content)
        head = self.commit()
        self.assertEqual({1, 2, 3, 4}, guard.active_glob_lines(content))
        self.assertEqual([(NEW, line) for line in range(1, 5)],
                         guard.violations(self.repo, base, head))
        self.assertEqual(set(), guard.active_glob_lines(
            "pub(crate)use::std::io::Error;\nuse{crate::support::named};\n"))

    def test_visible_explicit_imports_and_masked_decoys_are_allowed(self):
        self.put(PRODUCTION, "pub use super::*;\n")
        base = self.commit()
        self.put(NEW, (
            "pub use super::named;\n"
            "pub(crate) use super::{left, right};\n"
            "pub(in crate::tests) use crate::support::helper;\n"
            "// pub(super) use super::*;\n"
            "/* pub(self) use self::*; */\n"
            'const S: &str = "pub(crate) use super::*;";\n'
            'const RAW: &str = r##"pub(in crate::tests) use super::*;"##;\n'
        ))
        head = self.commit()
        self.assertEqual(set(), guard.active_glob_lines(
            (self.repo / NEW).read_text(encoding="utf-8")))
        self.assertEqual([], guard.violations(self.repo, base, head))

    def test_unchanged_visible_legacy_and_new_production_glob_are_allowed(self):
        self.put(EXISTING, "pub(super) use super::*;\n")
        self.put(PRODUCTION, "pub fn example() {}\n")
        base = self.commit()
        self.put(EXISTING, "pub(super) use super::*;\nfn added() {}\n")
        self.put(PRODUCTION, "pub fn example() {}\npub(crate) use crate::*;\n")
        head = self.commit()
        self.assertEqual([], guard.violations(self.repo, base, head))

    def test_comment_only_edit_inside_legacy_visible_import_is_not_code(self):
        self.put(EXISTING, "pub(crate) use super::{\n    *,\n};\n")
        base = self.commit()
        self.put(EXISTING, "pub(crate) use super::{\n    /* detail */\n    *,\n};\n")
        head = self.commit()
        self.assertEqual([], guard.violations(self.repo, base, head))

    def test_changed_inline_comment_is_not_a_new_visible_glob(self):
        self.put(EXISTING, "pub(crate) use super::{/* old note */ *};\n")
        base = self.commit()
        self.put(EXISTING, "pub(crate) use super::{/* revised note */ *};\n")
        head = self.commit()
        self.assertEqual([], guard.violations(self.repo, base, head))

    def test_new_duplicate_visible_glob_still_fails_with_edited_legacy(self):
        self.put(EXISTING, "pub(crate) use super::{/* old note */ *};\n")
        base = self.commit()
        self.put(EXISTING, (
            "pub(crate) use super::{/* revised note */ *};\n"
            "pub(crate) use super::{*};\n"
        ))
        head = self.commit()
        self.assertEqual([(EXISTING, 2)], guard.violations(self.repo, base, head))

    def test_new_duplicate_before_edited_legacy_reports_added_line(self):
        self.put(EXISTING, "pub(crate) use super::{/* old note */ *};\n")
        base = self.commit()
        self.put(EXISTING, (
            "pub(crate) use super::{*};\n"
            "pub(crate) use super::{/* revised note */ *};\n"
        ))
        head = self.commit()
        self.assertEqual([(EXISTING, 1)], guard.violations(self.repo, base, head))

    def test_legacy_glob_removed_in_one_module_cannot_excuse_new_in_another(self):
        self.put(EXISTING, (
            "mod alpha {\n"
            "    pub(crate) use crate::items::*;\n"
            "}\n\n"
            "mod beta {\n"
            "    pub(crate) use crate::items::named;\n"
            "}\n"
        ))
        base = self.commit()
        self.put(EXISTING, (
            "mod alpha {\n"
            "    pub(crate) use crate::items::named;\n"
            "}\n\n"
            "mod beta {\n"
            "    pub(crate) use crate::items::*;\n"
            "}\n"
        ))
        head = self.commit()
        self.assertEqual([(EXISTING, 6)], guard.violations(self.repo, base, head))

    def test_deleted_legacy_duplicate_cannot_credit_new_glob_elsewhere(self):
        self.put(EXISTING, (
            "mod alpha {\n"
            "    pub(crate) use crate::items::*;\n"
            "}\n\n"
            "mod beta {\n"
            "    pub(crate) use crate::items::named;\n"
            "}\n\n"
            "mod gamma {\n"
            "    pub(crate) use crate::items::*;\n"
            "}\n"
        ))
        base = self.commit()
        self.put(EXISTING, (
            "mod alpha {\n"
            "    pub(crate) use crate::items::*;\n"
            "}\n\n"
            "mod beta {\n"
            "    pub(crate) use crate::items::*;\n"
            "}\n\n"
            "mod gamma {\n"
            "    pub(crate) use crate::items::named;\n"
            "}\n"
        ))
        head = self.commit()
        self.assertEqual([(EXISTING, 6)], guard.violations(self.repo, base, head))

    def test_identical_brace_headers_do_not_erase_new_glob_in_another_scope(self):
        # Two anonymous consts share a lexical header but are distinct blocks.
        self.put(EXISTING, (
            "pub mod items {\n    pub fn f() {}\n}\n"
            "#[rustfmt::skip] const _: () = { /* old first */\n"
            "    use crate::items::*; // old first import\n"
            "    let _old_first = f;\n"
            "}; // old first\n"
            "#[rustfmt::skip] const _: () = { /* old second */\n"
            "    use crate::items::f; // old second import\n"
            "    let _old_second = f;\n"
            "}; // old second\n"
        ))
        base = self.commit()
        self.put(EXISTING, (
            "pub mod items {\n    pub fn f() {}\n}\n"
            "#[rustfmt::skip] const _: () = { /* new first */\n"
            "    use crate::items::f; // new first import\n"
            "    let _new_first = f;\n"
            "}; // new first\n"
            "#[rustfmt::skip] const _: () = { /* new second */\n"
            "    use crate::items::*; // new second import\n"
            "    let _new_second = f;\n"
            "}; // new second\n"
        ))
        head = self.commit()
        self.assertEqual([(EXISTING, 9)], guard.violations(self.repo, base, head))

    def test_inserted_anonymous_scope_cannot_take_old_glob_identity(self):
        self.put(EXISTING, (
            "pub mod items {\n    pub fn f() {}\n}\n"
            "#[rustfmt::skip] const _: () = { /* old first */\n"
            "    use crate::items::*; // old original import\n"
            "    let _old_first = f;\n"
            "}; // old first\n"
            "#[rustfmt::skip] const _: () = { /* old second */\n"
            "    use crate::items::f; // old second explicit\n"
            "    let _old_second = f;\n"
            "}; // old second\n"
        ))
        base = self.commit()
        self.put(EXISTING, (
            "pub mod items {\n    pub fn f() {}\n}\n"
            "#[rustfmt::skip] const _: () = { /* inserted first */\n"
            "    use crate::items::*; // new first import\n"
            "    let _new_first = f;\n"
            "}; // inserted first\n"
            "#[rustfmt::skip] const _: () = { /* old first is now second */\n"
            "    use crate::items::f; // now explicit\n"
            "    let _shifted = f;\n"
            "}; // shifted first\n"
            "#[rustfmt::skip] const _: () = { /* old second is now third */\n"
            "    use crate::items::f; // old second remains explicit\n"
            "    let _old_second_changed = f;\n"
            "}; // changed second\n"
        ))
        head = self.commit()
        self.assertEqual([(EXISTING, 5)], guard.violations(self.repo, base, head))

    def test_inserted_anonymous_scope_keeps_old_commented_glob(self):
        self.put(EXISTING, (
            "pub mod items {\n    pub fn f() {}\n}\n"
            "#[rustfmt::skip] const _: () = { /* original */\n"
            "    use crate::items::*; // previous comment\n"
            "    let _old = f;\n"
            "}; // original\n"
        ))
        base = self.commit()
        self.put(EXISTING, (
            "pub mod items {\n    pub fn f() {}\n}\n"
            "#[rustfmt::skip] const _: () = { /* inserted */\n"
            "    use crate::items::f; // new explicit\n"
            "    let _inserted = f;\n"
            "}; // inserted\n"
            "#[rustfmt::skip] const _: () = { /* original */\n"
            "    use crate::items::*; // comment revised\n"
            "    let _old = f;\n"
            "}; // original\n"
        ))
        head = self.commit()
        self.assertEqual([], guard.violations(self.repo, base, head))

    def test_comment_only_edits_to_scope_header_and_legacy_glob_are_allowed(self):
        self.put(EXISTING, (
            "pub mod items { pub fn f() {} }\n"
            "#[rustfmt::skip]\n"
            "mod tests { /* old header comment */\n"
            "    pub(crate) use crate::items::{/* old inline comment */ *};\n"
            "}\n"
        ))
        base = self.commit()
        self.put(EXISTING, (
            "pub mod items { pub fn f() {} }\n"
            "#[rustfmt::skip]\n"
            "mod tests { /* changed header comment */\n"
            "    pub(crate) use crate::items::{/* changed inline comment */ *};\n"
            "}\n"
        ))
        head = self.commit()
        self.assertEqual([], guard.violations(self.repo, base, head))

    def test_shifted_unchanged_scope_maps_legacy_glob_with_other_code_edits(self):
        self.put(EXISTING, (
            "pub mod items { pub fn f() {} }\n"
            "mod tests {\n"
            "    pub(crate) use crate::items::{/* old */ *};\n"
            "    fn old() {}\n"
            "}\n"
        ))
        base = self.commit()
        self.put(EXISTING, (
            "pub mod items { pub fn f() {} }\n"
            "mod inserted { pub fn x() {} }\n"
            "mod tests {\n"
            "    pub(crate) use crate::items::{/* updated */ *};\n"
            "    fn renamed() {}\n"
            "}\n"
        ))
        head = self.commit()
        self.assertEqual([], guard.violations(self.repo, base, head))

    def test_intra_hunk_module_move_cannot_hide_new_visible_glob(self):
        self.put(EXISTING, (
            "mod old_alpha {\n"
            "    pub(crate) use crate::items::{/* old alpha scope */ *};\n"
            "} // old alpha\n"
            "mod old_beta {\n"
            "    pub fn old_beta_helper() {}\n"
            "} // old beta\n"
        ))
        base = self.commit()
        self.put(EXISTING, (
            "mod new_alpha {\n"
            "    pub fn new_alpha_helper() {}\n"
            "} // new alpha\n"
            "mod new_beta {\n"
            "    pub(crate) use crate::items::{/* new beta scope */ *};\n"
            "} // new beta\n"
        ))
        head = self.commit()
        self.assertEqual([(EXISTING, 5)], guard.violations(self.repo, base, head))

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

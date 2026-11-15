#!/usr/bin/env python3
"""Selection-rule and classifier tests for tools/windows_architecture_ledger.py.

The generator decides which tracked files enter the Windows security review queue,
and `--check` only compares the committed CSV with the tool's own output, so the
rule needs its own positive and negative fixtures (review of #891).
"""
import csv
import io
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import windows_architecture_ledger as ledger

NEUTRAL = "crates/keld-x/src/lib.rs"


def selected(blob, path=NEUTRAL):
    return ledger.is_candidate(path, blob)


class InclusionRule(unittest.TestCase):
    """In-memory blobs under a path that carries no Windows signal of its own."""

    def assertSelected(self, blob, path=NEUTRAL):
        self.assertTrue(selected(blob, path), f"{blob!r} should enter the queue")

    def assertNotSelected(self, blob, path=NEUTRAL):
        self.assertFalse(selected(blob, path), f"{blob!r} must not enter the queue")

    def test_windows_prefixed_identifiers_are_selected(self):
        # The trailing \b after `windows_` never matched `_` + letter.
        self.assertSelected(b"use crate::windows_job;\n")
        self.assertSelected(b"mod windows_fs;\n")
        self.assertSelected(b"use keld_ipc::windows_named_pipe::Pipe;\n")
        self.assertSelected(b"WINDOWS_JOB_LIMIT\n")

    def test_compound_cfg_forms_are_selected(self):
        self.assertSelected(b"#[cfg(windows)]\nfn a() {}\n")
        self.assertSelected(b"#[cfg(all(test, windows))]\nmod t {}\n")
        self.assertSelected(b"#[cfg(any(windows, test))]\nmod t {}\n")
        self.assertSelected(b"#[cfg(not(windows))]\nfn fallback() {}\n")
        self.assertSelected(b"#[cfg( all( unix , any( test , not( windows ) ) ) )]\n")
        self.assertSelected(b"#![cfg_attr(windows, allow(unsafe_code))]\n")
        self.assertSelected(b"if cfg!(not(windows)) { run(); }\n")
        self.assertSelected(b'#[cfg(all(test, target_family = "windows"))]\n')
        self.assertSelected(b'#[cfg(target_os = "windows")]\n')

    def test_existing_signals_still_select(self):
        for blob in (b"load Win32 api\n", b"WebView2 runtime\n", b"LPAC token\n",
                     b"Authenticode chain\n", b"runs-on: windows-latest\n",
                     b"Windows Server 2025\n", b"windows_server image\n"):
            self.assertSelected(blob)

    def test_markdown_and_toml_references_are_selected(self):
        self.assertSelected(b"production-unsafe allowlist: windows_named_pipe\n",
                            "crates/keld-ipc/AGENTS.md")
        self.assertSelected(
            b'disallowed-methods = [{ path = "keld_runtime::windows_job::Job::new" }]\n',
            "crates/keld-updater-helper/clippy.toml")
        self.assertSelected(b"#[cfg(all(test, windows))]\n", "crates/keld-native/src/lib.rs")

    def test_path_signal_selects_without_text(self):
        self.assertSelected(b"fn plain() {}\n", "crates/keld-runtime/src/windows_job.rs")
        self.assertSelected(b"fn plain() {}\n", "crates/keld-x/src/lpac.rs")

    def test_plain_non_windows_files_are_not_selected(self):
        self.assertNotSelected(b"fn main() {}\n// nothing platform specific here\n")
        self.assertNotSelected(b"#[cfg(unix)]\n#[cfg(test)]\n#[cfg(all(test, unix))]\n")
        self.assertNotSelected(b'#[cfg(target_os = "linux")]\n#[cfg(target_os = "macos")]\n')
        self.assertNotSelected(b"")

    def test_lookalike_words_are_not_selected(self):
        self.assertNotSelected(b"let swindows = 1;\n")
        self.assertNotSelected(b"let windowsill = 1;\n")
        self.assertNotSelected(b"let mywindows_job = 1;\n")

    def test_cfg_scan_requires_the_whole_windows_token(self):
        # Inside a cfg list `windows` must be a token, not a substring of one.
        self.assertNotSelected(b'#[cfg(feature = "swindowsx")]\n')
        self.assertNotSelected(b"#[cfg(any(unix, mywindowsish))]\n")
        self.assertSelected(b'#[cfg(feature = "windows-compat")]\n')

    def test_slice_windows_method_is_not_a_windows_reference(self):
        # `[T]::windows(n)` is a std slice method, not the platform.
        self.assertNotSelected(b"let pairs = data.windows(2).count();\n")

    def test_cfg_scan_stops_at_the_matching_close_paren(self):
        # `windows(` after the cfg attribute is outside its argument list.
        self.assertNotSelected(b"#[cfg(unix)] fn a() { v.windows(2); }\n")
        self.assertNotSelected(b"#[cfg(all(test, unix))]\nfn a() { v.windows(2); }\n")
        self.assertNotSelected(b"#[cfg(test)]\n#[cfg(not(unix))]\nfn a() { v.windows(2); }\n")
        # Without a balanced close the scan is bounded and still terminates.
        self.assertNotSelected(b"#[cfg(" + b"x, " * 10_000)


class Classifiers(unittest.TestCase):
    def test_class_by_path(self):
        self.assertEqual("ci-tooling", ledger.classify(".github/workflows/ci.yml"))
        self.assertEqual("ci-tooling", ledger.classify("tools/ci_inputs.py"))
        self.assertEqual("ci-tooling", ledger.classify("justfile"))
        self.assertEqual("test", ledger.classify("crates/keld-ipc/tests/pipe.rs"))
        self.assertEqual("test", ledger.classify("crates/keld-ipc/src/tests.rs"))
        self.assertEqual("production-or-fixture", ledger.classify("crates/keld-ipc/src/lib.rs"))
        self.assertEqual("support", ledger.classify("crates/keld-ipc/AGENTS.md"))

    def test_owner_by_path(self):
        self.assertEqual("keld-ipc", ledger.proposed_owner("crates/keld-ipc/AGENTS.md"))
        self.assertEqual("ci-security-maintainers", ledger.proposed_owner("justfile"))
        self.assertEqual("ci-security-maintainers", ledger.proposed_owner(".github/workflows/ci.yml"))
        self.assertEqual("unassigned", ledger.proposed_owner("docs/other.md"))


class RenderedLedger(unittest.TestCase):
    """Run the whole generator against a real tiny Git tree."""

    FILES = {
        # selected by the previously missed forms
        "crates/keld-ipc/AGENTS.md": b"unsafe allowlist: windows_named_pipe\n",
        "crates/keld-native/src/lib.rs": b"#[cfg(all(test, windows))]\nmod t {}\n",
        "crates/keld-x/src/only_not_windows.rs": b"#[cfg(not(windows))]\nfn f() {}\n",
        "crates/keld-updater-helper/clippy.toml":
            b'disallowed-methods = [{ path = "keld_runtime::windows_job::Job" }]\n',
        # selected by the old forms
        "crates/keld-x/src/win32_user.rs": b"fn f() {}\n",
        "crates/keld-x/src/old_form.rs": b"#[cfg(windows)]\nfn f() {}\n",
        # forced inputs
        "justfile": b"default:\n\techo hi\n",
        # never selected
        "crates/keld-x/src/plain.rs": b"fn plain() {}\n",
        "crates/keld-x/src/slices.rs": b"fn f(v: &[u8]) -> usize { v.windows(2).count() }\n",
        "docs/outside.md": b"windows_job is mentioned outside the scanned roots\n",
        "crates/keld-x/assets/logo.png": b"windows_job bytes in an excluded suffix\n",
        "crates/keld-x/Cargo.lock": b"windows_job excluded suffix\n",
        # the generator never queues itself
        "tools/windows_architecture_ledger.py": b"windows_job windows_fs\n",
    }
    EXPECTED = {
        "crates/keld-ipc/AGENTS.md",
        "crates/keld-native/src/lib.rs",
        "crates/keld-x/src/only_not_windows.rs",
        "crates/keld-updater-helper/clippy.toml",
        "crates/keld-x/src/win32_user.rs",
        "crates/keld-x/src/old_form.rs",
        "justfile",
    }

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="keld-win-ledger-")
        self.addCleanup(self.tmp.cleanup)
        self.repo = Path(self.tmp.name)
        self.git("init", "-q")
        self.git("config", "maintenance.auto", "false")
        for name, data in self.FILES.items():
            target = self.repo / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
        self.git("add", "-A")
        self.git("-c", "user.name=Ledger Test", "-c", "user.email=ledger@example.invalid",
                 "commit", "-qm", "fixture")

    def git(self, *args):
        return subprocess.run(["git", "-C", str(self.repo), *args], check=True,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout

    def rows(self):
        content, summary = ledger.render(self.repo, "HEAD")
        return list(csv.DictReader(io.StringIO(content))), summary

    def test_exact_selected_set(self):
        rows, summary = self.rows()
        self.assertEqual(self.EXPECTED, {row["path"] for row in rows})
        self.assertEqual(len(self.EXPECTED), summary["tracked_candidates"])
        self.assertEqual(0, summary["reviewed"])
        self.assertEqual(len(self.EXPECTED), summary["pending_independent_semantic_review"])

    def test_every_row_is_pending_and_bound_to_its_blob(self):
        rows, _ = self.rows()
        self.assertEqual(sorted(row["path"] for row in rows), [row["path"] for row in rows])
        for row in rows:
            self.assertEqual("PENDING_INDEPENDENT_SEMANTIC_REVIEW", row["review_status"])
            blob = self.git("rev-parse", "HEAD:" + row["path"]).decode().strip()
            self.assertEqual(blob, row["git_blob_sha"])
            self.assertEqual(str(len(self.FILES[row["path"]])), row["bytes"])

    def test_cli_generates_then_check_accepts_and_detects_drift(self):
        script = Path(__file__).with_name("windows_architecture_ledger.py")
        out = "ledger.csv"
        base = [sys.executable, "-B", str(script), "--repo", str(self.repo),
                "--ref", "HEAD", "--output", out]
        subprocess.run(base, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        summary = json.loads((self.repo / "ledger.summary.json").read_text(encoding="utf-8"))
        self.assertEqual(len(self.EXPECTED), summary["tracked_candidates"])
        ok = subprocess.run(base + ["--check"], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.assertEqual(0, ok.returncode, ok.stderr.decode())
        stale = (self.repo / out).read_bytes().replace(b"justfile", b"justfile2", 1)
        (self.repo / out).write_bytes(stale)
        bad = subprocess.run(base + ["--check"], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.assertNotEqual(0, bad.returncode)
        self.assertIn(b"Windows ledger byte drift", bad.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)

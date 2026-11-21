#!/usr/bin/env python3
"""Fail-first CLI checks that source staging cannot modify an app bundle."""

from __future__ import annotations

from contextlib import redirect_stderr, redirect_stdout
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).with_name("stage_production_source.py")
PINNED_SOURCE_ENV = "GH1404_PINNED_SOURCE_ROOT"
EXPECTED_PATCH_SHA256 = "d31486a5d7d1cbcfaec2aedd9309bb3d44e1b0efc06d11fb915164a98eb457e5"
PINNED_SOURCE_SHA256 = (
    ("src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js",
     "0daef9812e3b8c530bc9556791ffaea53fe9a57091a86534bd909ca31e4485b3"),
    ("src/main/webapp/mxgraph/src/shape/mxText.js",
     "a6c0f2c748403a74b11405c864a8278bfcdad216f80ea5b290c1e917d13f9772"),
)

stage_spec = importlib.util.spec_from_file_location("gh1404_stage_production_source", SCRIPT)
assert stage_spec is not None and stage_spec.loader is not None
stage = importlib.util.module_from_spec(stage_spec)
stage_spec.loader.exec_module(stage)


class ProductionStageDestinationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="gh1404-source-stage-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "pristine"
        self.source.mkdir()

    def stage(self, destination: Path, source: Path | None = None) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--source-root", str(source or self.source),
             "--output-root", str(destination)],
            capture_output=True,
            text=True,
            check=False,
        )

    def assert_bundle_output_refused(self, destination: Path) -> None:
        result = self.stage(destination)
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("refusing application bundle output", result.stderr)
        self.assertFalse(destination.exists(), "an app bundle was modified")

    def test_existing_app_parent_is_not_an_output_root(self) -> None:
        resources = self.root / "disposable.app" / "Contents" / "Resources"
        resources.mkdir(parents=True)
        self.assert_bundle_output_refused(resources / "new-overlay")

    def test_symlink_alias_to_app_parent_cannot_hide_bundle(self) -> None:
        resources = self.root / "disposable.app" / "Contents" / "Resources"
        resources.mkdir(parents=True)
        alias = self.root / "source-stage-alias"
        alias.symlink_to(resources, target_is_directory=True)
        self.assert_bundle_output_refused(alias / "new-overlay")

    def test_app_suffix_on_new_output_is_forbidden_case_insensitively(self) -> None:
        self.assert_bundle_output_refused(self.root / "new-overlay.APP")

    def test_ordinary_output_still_checks_immutable_input_sources(self) -> None:
        destination = self.root / "new-overlay"
        result = self.stage(destination)
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("missing real source file", result.stderr)
        self.assertFalse(destination.exists())

    def _sources(self, pinned: bool) -> Path:
        if pinned:
            configured = os.environ.get(PINNED_SOURCE_ENV)
            if not configured:
                self.skipTest(f"set {PINNED_SOURCE_ENV} to a pristine pinned jgraph checkout")
            source = Path(configured).resolve(strict=True)
            for name, expected in PINNED_SOURCE_SHA256:
                self.assertEqual(hashlib.sha256((source / name).read_bytes()).hexdigest(), expected)
            return source
        for name, _ in PINNED_SOURCE_SHA256:
            target = self.source / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(b"fixture source, generated output is a stub")
        return self.source

    def _parent_swap_race(self, pinned: bool) -> None:
        source = self._sources(pinned)
        parent = self.root / "staging-parent"
        parent.mkdir()
        parked = self.root / "original-parent-after-rename"
        bundle = self.root / "disposable.app" / "Contents" / "Resources"
        bundle.mkdir(parents=True)
        destination = parent / "new-overlay"

        original_run = subprocess.run
        calls = 0

        def generator_with_parent_swap(*args: object, **kwargs: object) -> subprocess.CompletedProcess[bytes]:
            nonlocal calls
            if pinned:
                result = original_run(*args, **kwargs)
            else:
                result = subprocess.CompletedProcess(args[0], 0, b"replacement" if calls % 2 == 0 else b"patch", b"")
            calls += 1
            if calls == 1:
                # Deterministic, after the first generator completes but before
                # mkdir/write: recreate the attacker's name swap into a .app.
                parent.rename(parked)
                parent.symlink_to(bundle, target_is_directory=True)
            return result

        stdout = io.StringIO()
        stderr = io.StringIO()
        args = [str(SCRIPT), "--source-root", str(source), "--output-root", str(destination)]
        with mock.patch.object(sys, "argv", args), \
                mock.patch.object(stage.subprocess, "run", side_effect=generator_with_parent_swap), \
                redirect_stdout(stdout), redirect_stderr(stderr):
            try:
                stage.main()
            except SystemExit as error:
                exit_code = error.code
            else:
                exit_code = 0

        self.assertEqual(calls, 4, "all four pinned-source generator phases must finish")
        self.assertEqual(exit_code, 2, f"staging accepted a rebound parent: {stdout.getvalue()}")
        self.assertIn("destination", stderr.getvalue())
        self.assertEqual(list(bundle.iterdir()), [], "overlay reached a disposable application bundle")
        self.assertFalse((parked / "new-overlay").exists(), "renamed output parent must fail closed")

    def test_parent_swap_after_generator_is_refused(self) -> None:
        self._parent_swap_race(pinned=False)

    def test_pinned_source_parent_swap_after_generator_is_refused(self) -> None:
        self._parent_swap_race(pinned=True)

    def test_pinned_source_stages_unchanged_overlay_without_race(self) -> None:
        source = self._sources(pinned=True)
        parent = self.root / "verified-output-parent"
        parent.mkdir()
        output = parent / "new-overlay"
        result = self.stage(output, source=source)
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = json.loads((output / "source-manifest.json").read_text())
        self.assertEqual(manifest["patch_sha256"], EXPECTED_PATCH_SHA256)
        self.assertEqual(
            hashlib.sha256((output / "gh1404-g1-production.patch").read_bytes()).hexdigest(),
            EXPECTED_PATCH_SHA256,
        )
        self.assertEqual(
            [(item["path"], item["original_sha256"]) for item in manifest["files"]],
            list(PINNED_SOURCE_SHA256),
        )

    def test_pinned_source_can_create_missing_nested_parent_without_aliases(self) -> None:
        source = self._sources(pinned=True)
        output = self.root / "new" / "nested" / "source-overlay"
        result = self.stage(output, source=source)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((output / "src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js").is_file())
        self.assertEqual(
            hashlib.sha256((output / "gh1404-g1-production.patch").read_bytes()).hexdigest(),
            EXPECTED_PATCH_SHA256,
        )

    def test_abnormal_stage_death_before_verification_has_no_success_manifest(self) -> None:
        # Real subprocess and SIGKILL; fake generators isolate filesystem
        # publication from the separate pinned-original byte-identity tests.
        source = self._sources(pinned=False)
        interrupted = self.root / "interrupted-stage"
        killed = subprocess.run(
            [sys.executable, "-B", str(Path(__file__).resolve()),
             "--crash-stage-child", "abort", str(source), str(interrupted)],
            text=True, capture_output=True, check=False, timeout=5,
        )
        self.assertEqual(killed.returncode, -signal.SIGKILL, killed.stderr)
        self.assertEqual(killed.stdout, "", "a dead stage must not emit success")
        self.assertTrue(
            (interrupted / "gh1404-g1-production.patch").is_file(),
            "the fault must occur after candidate files were written",
        )
        self.assertFalse(
            (interrupted / "source-manifest.json").exists(),
            "an unverified, interrupted overlay cannot advertise final source receipts",
        )
        self.assertEqual(
            self.stage(interrupted).returncode, 2,
            "an aborted overlay cannot be resumed as a qualified source output",
        )

        healthy = self.root / "healthy-successor"
        recovered = subprocess.run(
            [sys.executable, "-B", str(Path(__file__).resolve()),
             "--crash-stage-child", "complete", str(source), str(healthy)],
            text=True, capture_output=True, check=False, timeout=5,
        )
        self.assertEqual(recovered.returncode, 0, recovered.stderr)
        manifest = json.loads((healthy / "source-manifest.json").read_text())
        self.assertEqual(json.loads(recovered.stdout), manifest)

    def _directory_swap_during_write(self, target: str) -> None:
        self._sources(pinned=False)
        parent = self.root / "staging-parent"
        parent.mkdir()
        bundle = self.root / "disposable.app" / "Contents" / "Resources"
        bundle.mkdir(parents=True)
        destination = parent / "new-overlay"
        original_mkdir = os.mkdir
        changes = 0

        def create_and_swap(name: str, *args: object, **kwargs: object) -> None:
            nonlocal changes
            original_mkdir(name, *args, **kwargs)
            parent_fd = kwargs.get("dir_fd")
            if name == target and parent_fd is not None:
                changes += 1
                # Replace exactly the just-created name, before its O_NOFOLLOW
                # open. This is independent of pathname resolution in the tool.
                os.rename(name, f"parked-{target}", src_dir_fd=parent_fd,
                          dst_dir_fd=parent_fd)
                os.symlink(bundle, name, dir_fd=parent_fd)

        with mock.patch.object(stage.os, "mkdir", side_effect=create_and_swap), \
                mock.patch.object(stage.subprocess, "run",
                                  side_effect=lambda argv, **_kw: subprocess.CompletedProcess(
                                      argv, 0, b"replacement", b"")), \
                mock.patch.object(sys, "argv",
                                  [str(SCRIPT), "--source-root", str(self.source),
                                   "--output-root", str(destination)]), \
                redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()) as stderr:
            with self.assertRaises(SystemExit) as error:
                stage.main()

        self.assertEqual(error.exception.code, 2)
        self.assertEqual(changes, 1, f"did not exercise swap at {target}: {stderr.getvalue()}")
        self.assertIn("destination", stderr.getvalue())
        self.assertEqual(list(bundle.iterdir()), [], "source output crossed into disposable .app")

    def test_output_root_symlink_swap_after_mkdir_is_refused(self) -> None:
        self._directory_swap_during_write("new-overlay")

    def test_nested_source_directory_symlink_swap_after_mkdir_is_refused(self) -> None:
        self._directory_swap_during_write("src")

    def test_parent_swap_after_output_descriptor_open_stays_outside_bundle(self) -> None:
        self._sources(pinned=False)
        parent = self.root / "original-parent"
        parent.mkdir()
        parked = self.root / "parked-parent"
        bundle = self.root / "disposable.app" / "Contents" / "Resources"
        bundle.mkdir(parents=True)
        destination = parent / "new-overlay"
        original_write = stage.write_overlay
        changes = 0

        def write_after_swap(*args: object, **kwargs: object) -> object:
            nonlocal changes
            changes += 1
            parent.rename(parked)
            parent.symlink_to(bundle, target_is_directory=True)
            return original_write(*args, **kwargs)

        with mock.patch.object(stage, "write_overlay", side_effect=write_after_swap), \
                mock.patch.object(stage.subprocess, "run",
                                  side_effect=lambda argv, **_kw: subprocess.CompletedProcess(
                                      argv, 0, b"replacement", b"")), \
                mock.patch.object(sys, "argv",
                                  [str(SCRIPT), "--source-root", str(self.source),
                                   "--output-root", str(destination)]), \
                redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()) as stderr:
            with self.assertRaises(SystemExit) as error:
                stage.main()

        self.assertEqual(error.exception.code, 2)
        self.assertEqual(changes, 1)
        self.assertIn("destination changed after source staging", stderr.getvalue())
        self.assertEqual(list(bundle.iterdir()), [], "pinned output descriptor was redirected")
        self.assertTrue((parked / "new-overlay" / "gh1404-g1-production.patch").is_file(),
                        "stage writes must stay anchored to the original directory inode")
        self.assertFalse((parked / "new-overlay" / "source-manifest.json").exists(),
                         "rebound output must never advertise final source receipts")

    def _publication_rebind_control(self, *, nested_directory: bool) -> None:
        # Run the actual pinned generator pair. Only the newly staged output
        # namespace is hostile; the source checkout is untouched.
        source = self._sources(pinned=True)
        output = self.root / "source-overlay"
        bundle = self.root / "disposable.app" / "Contents" / "Resources"
        bundle.mkdir(parents=True)
        original_create = stage.create_directory
        original_write = stage.write_new_file
        swaps = 0

        def swap_dir_after_open(parent_fd: int, name: str, stack: object) -> int:
            nonlocal swaps
            opened = original_create(parent_fd, name, stack)
            if nested_directory and name == "src" and swaps == 0:
                os.rename(name, "parked-src", src_dir_fd=parent_fd, dst_dir_fd=parent_fd)
                os.symlink(bundle, name, dir_fd=parent_fd)
                swaps += 1
            return opened

        def swap_file_after_write(
            parent_fd: int, name: str, data: bytes
        ) -> tuple[int, int, int, str]:
            nonlocal swaps
            receipt = original_write(parent_fd, name, data)
            if not nested_directory and name == "mxSvgCanvas2D.js" and swaps == 0:
                os.rename(name, "parked-mxSvgCanvas2D.js",
                          src_dir_fd=parent_fd, dst_dir_fd=parent_fd)
                os.symlink(bundle / "missing.js", name, dir_fd=parent_fd)
                swaps += 1
            return receipt

        stdout = io.StringIO()
        stderr = io.StringIO()
        with mock.patch.object(stage, "create_directory", side_effect=swap_dir_after_open), \
                mock.patch.object(stage, "write_new_file", side_effect=swap_file_after_write), \
                mock.patch.object(sys, "argv",
                                  [str(SCRIPT), "--source-root", str(source),
                                   "--output-root", str(output)]), \
                redirect_stdout(stdout), redirect_stderr(stderr):
            try:
                stage.main()
            except SystemExit as failure:
                exit_code = failure.code
            else:
                exit_code = 0

        self.assertEqual(swaps, 1, "the requested exact name swap was not injected")
        self.assertEqual(exit_code, 2,
                         "rebound published path was falsely accepted: " + stdout.getvalue())
        self.assertEqual(stdout.getvalue(), "", "a rejected overlay must not print success JSON")
        self.assertIn("destination", stderr.getvalue())
        self.assertEqual(list(bundle.iterdir()), [], "staging changed a disposable app bundle")

    def test_pinned_source_nested_dir_rebound_after_open_fails_publication(self) -> None:
        self._publication_rebind_control(nested_directory=True)

    def test_pinned_source_file_rebound_after_write_fails_publication(self) -> None:
        self._publication_rebind_control(nested_directory=False)

    def _publication_file_content_control(self, *, same_bytes_new_inode: bool) -> None:
        source = self._sources(pinned=True)
        output = self.root / "source-overlay"
        original_write = stage.write_new_file
        changes = 0

        def modify_written_file(
            parent_fd: int, name: str, data: bytes
        ) -> tuple[int, int, int, str]:
            nonlocal changes
            receipt = original_write(parent_fd, name, data)
            if name == "mxSvgCanvas2D.js" and changes == 0:
                if same_bytes_new_inode:
                    os.rename(name, "parked-mxSvgCanvas2D.js",
                              src_dir_fd=parent_fd, dst_dir_fd=parent_fd)
                    fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL |
                                 os.O_NOFOLLOW, 0o600, dir_fd=parent_fd)
                    with os.fdopen(fd, "wb") as target:
                        target.write(data)
                else:
                    fd = os.open(name, os.O_WRONLY | os.O_NOFOLLOW, dir_fd=parent_fd)
                    with os.fdopen(fd, "wb") as target:
                        target.write(b"!")
                changes += 1
            return receipt

        stdout, stderr = io.StringIO(), io.StringIO()
        with mock.patch.object(stage, "write_new_file", side_effect=modify_written_file), \
                mock.patch.object(sys, "argv",
                                  [str(SCRIPT), "--source-root", str(source),
                                   "--output-root", str(output)]), \
                redirect_stdout(stdout), redirect_stderr(stderr):
            with self.assertRaises(SystemExit) as rejected:
                stage.main()

        self.assertEqual(changes, 1)
        self.assertEqual(rejected.exception.code, 2)
        self.assertEqual(stdout.getvalue(), "", "corrupt overlay cannot print success")
        self.assertIn("destination", stderr.getvalue())

    def test_pinned_source_in_place_byte_change_refuses_publication(self) -> None:
        self._publication_file_content_control(same_bytes_new_inode=False)

    def test_pinned_source_same_bytes_different_inode_refuses_publication(self) -> None:
        self._publication_file_content_control(same_bytes_new_inode=True)

    def _pinned_source_fifo_publication_control(self, *, directory: bool) -> None:
        source = self._sources(pinned=True)
        output = self.root / "staged-source"
        kind = "directory" if directory else "file"
        # Run the actual pinned source and file-descriptor code in a child.
        # The timeout is a kill switch for the vulnerable blocking FIFO open.
        try:
            result = subprocess.run(
                [sys.executable, "-B", str(Path(__file__).resolve()),
                 "--fifo-child", kind, str(source), str(output)],
                text=True, capture_output=True, check=False, timeout=5,
            )
        except subprocess.TimeoutExpired as error:
            self.fail(f"source publication blocked on {kind} FIFO: {error}")
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(result.stdout, "", "unsafe stage emitted a success receipt")
        self.assertIn("destination changed or unsafe during source publication", result.stderr)
        fifo = (output / "src") if directory else (
            output / "src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js"
        )
        self.assertTrue(stat.S_ISFIFO(fifo.lstat().st_mode),
                        f"the test did not substitute a {kind} FIFO")

    def test_pinned_source_rebound_js_fifo_refuses_without_hanging(self) -> None:
        self._pinned_source_fifo_publication_control(directory=False)

    def test_pinned_source_rebound_directory_fifo_refuses_without_hanging(self) -> None:
        self._pinned_source_fifo_publication_control(directory=True)


def _fifo_child(mode: str, source: Path, output: Path) -> None:
    """Isolated adversarial staging callback; only the test-owned output is mutated."""
    original_write = stage.write_new_file
    original_dir = stage.create_directory

    def publish_fifo_file(parent_fd: int, name: str, data: bytes) -> object:
        receipt = original_write(parent_fd, name, data)
        if mode == "file" and name == "mxSvgCanvas2D.js":
            os.rename(name, "parked-mxSvgCanvas2D.js",
                      src_dir_fd=parent_fd, dst_dir_fd=parent_fd)
            os.mkfifo(name, mode=0o600, dir_fd=parent_fd)
        return receipt

    def publish_fifo_directory(parent_fd: int, name: str, stack: object) -> int:
        fd = original_dir(parent_fd, name, stack)
        if mode == "directory" and name == "src":
            os.rename(name, "parked-src", src_dir_fd=parent_fd, dst_dir_fd=parent_fd)
            os.mkfifo(name, mode=0o600, dir_fd=parent_fd)
        return fd

    with mock.patch.object(sys, "argv",
                           [str(SCRIPT), "--source-root", str(source),
                            "--output-root", str(output)]), \
            mock.patch.object(stage, "write_new_file", side_effect=publish_fifo_file), \
            mock.patch.object(stage, "create_directory", side_effect=publish_fifo_directory):
        stage.main()


def _crash_stage_child(mode: str, source: Path, output: Path) -> None:
    """Stage real temporary POSIX files, then die before verification."""
    def candidate_generator(argv: list[str], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
        kind = Path(argv[1]).name
        data = ("PATCH:" if "--patch" in argv else "CANDIDATE:") + kind
        return subprocess.CompletedProcess(argv, 0, data.encode(), b"")

    with mock.patch.object(sys, "argv",
                           [str(SCRIPT), "--source-root", str(source),
                            "--output-root", str(output)]), \
            mock.patch.object(stage.subprocess, "run", side_effect=candidate_generator):
        if mode == "abort":
            with mock.patch.object(stage, "verify_published_overlay",
                                   side_effect=lambda *_args: os.kill(os.getpid(), signal.SIGKILL)):
                stage.main()
        else:
            stage.main()


if __name__ == "__main__":
    if len(sys.argv) == 5 and sys.argv[1] == "--fifo-child":
        _fifo_child(sys.argv[2], Path(sys.argv[3]), Path(sys.argv[4]))
    elif len(sys.argv) == 5 and sys.argv[1] == "--crash-stage-child":
        _crash_stage_child(sys.argv[2], Path(sys.argv[3]), Path(sys.argv[4]))
    else:
        unittest.main()

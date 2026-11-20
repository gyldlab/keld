#!/usr/bin/env python3
"""Fail-first CLI checks that source staging cannot modify an app bundle."""

from __future__ import annotations

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("stage_production_source.py")


class ProductionStageDestinationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="gh1404-source-stage-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "pristine"
        self.source.mkdir()

    def stage(self, destination: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--source-root", str(self.source),
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


if __name__ == "__main__":
    unittest.main()

"""Fixture evidence tests only; no Keld product or native acceptance claim."""
from __future__ import annotations

from contextlib import ExitStack
import copy
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest import mock

import fixture_census
import source_audit

ROOT = Path(__file__).resolve().parent
SETUP = json.loads((ROOT / "census_fixture.json").read_text(encoding="utf-8"))
GOLDEN = json.loads((ROOT / "census_golden.json").read_text(encoding="utf-8"))


def snapshot(root: Path) -> dict[str, bytes]:
    return {
        str(path.relative_to(root)): path.read_bytes()
        for path in root.rglob("*")
        if path.is_file()
    }


class SourceAuditTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix=".w1-test-", dir=ROOT)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        shutil.copytree(ROOT / "census_project", self.root / "project")
        self.project = self.root / "project"

    def compute(self, root: Path | None = None) -> dict:
        return fixture_census.census(
            root or self.project, SETUP["inputs"], SETUP["declared_preloads"]
        )

    def test_fixed_census_bytes_and_digest(self) -> None:
        observed = self.compute()
        self.assertEqual(bytes.fromhex(observed["canonical_hex"]), GOLDEN["canonical"].encode())
        self.assertEqual(observed["fixture_digest"], GOLDEN["fixture_digest"])

    def test_input_order_does_not_change_digest(self) -> None:
        observed = fixture_census.census(
            self.project, reversed(SETUP["inputs"]), reversed(SETUP["declared_preloads"])
        )
        self.assertEqual(observed["fixture_digest"], GOLDEN["fixture_digest"])

    def test_empty_vector(self) -> None:
        observed = fixture_census.census(self.project, [], [])
        self.assertEqual(observed["canonical_hex"], "")
        self.assertEqual(
            observed["fixture_digest"],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        )

    def test_utf8_byte_order_is_explicit(self) -> None:
        for name in ("z.js", "A.js", "é.js"):
            (self.project / name).write_bytes(b"")
        observed = fixture_census.census(self.project, ["é.js", "z.js", "A.js"], [])
        self.assertEqual([entry["path"] for entry in observed["entries"]], ["A.js", "z.js", "é.js"])

    def test_permissions_metadata_is_excluded(self) -> None:
        before = self.compute()["fixture_digest"]
        (self.project / "keld.permissions.jsonc").write_bytes(b'{"changed":"attestation"}\n')
        self.assertEqual(self.compute()["fixture_digest"], before)

    def test_other_preload_changes_paired_input_binding(self) -> None:
        own_preload_before = (self.project / "preload-a.js").read_bytes()
        before = self.compute()["fixture_digest"]
        other = self.project / "preload-b.js"
        data = bytearray(other.read_bytes())
        data[0] ^= 1
        other.write_bytes(data)
        self.assertEqual((self.project / "preload-a.js").read_bytes(), own_preload_before)
        self.assertNotEqual(self.compute()["fixture_digest"], before)

    def test_helper_changes_paired_input_binding(self) -> None:
        before = self.compute()["fixture_digest"]
        helper = self.project / "helper.cjs"
        helper.write_bytes(helper.read_bytes().replace(b"7", b"8"))
        self.assertNotEqual(self.compute()["fixture_digest"], before)

    def test_main_changes_paired_input_binding(self) -> None:
        before = self.compute()["fixture_digest"]
        main = self.project / "main.cjs"
        main.write_bytes(main.read_bytes() + b"// changed\n")
        self.assertNotEqual(self.compute()["fixture_digest"], before)

    def test_missing_declared_preload_is_not_ignored(self) -> None:
        (self.project / "preload-b.js").unlink()
        with self.assertRaises(FileNotFoundError):
            self.compute()

    def test_census_never_executes_project_or_config(self) -> None:
        shutil.copytree(ROOT / "no_execution", self.root / "hostile")
        hostile = self.root / "hostile"
        before = snapshot(hostile)
        intercepted = []
        with ExitStack() as stack:
            for name in ("builtins.exec", "builtins.eval", "subprocess.Popen", "os.system"):
                intercepted.append(stack.enter_context(mock.patch(name, side_effect=AssertionError(name))))
            observed = fixture_census.census(hostile, ["main.cjs", "keld.config.ts"], [])
        for call in intercepted:
            call.assert_not_called()
        self.assertEqual(len(observed["entries"]), 2)
        self.assertEqual(snapshot(hostile), before)
        self.assertFalse((hostile / "W1_EXECUTION_MARKER").exists())
        self.assertFalse((hostile / "W1_CONFIG_EXECUTION_MARKER").exists())

    def test_noncanonical_fixture_paths_refuse(self) -> None:
        for name in ("../outside.js", "/absolute.js", "a\tb.js", "a\nb.js", "file:///C:/x"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                fixture_census.census(self.project, [name], [])

    def vendor_copy(self) -> tuple[Path, dict]:
        shutil.copytree(ROOT / "vendor", self.root / "vendor")
        manifest = json.loads((ROOT / "source_manifest.json").read_text(encoding="utf-8"))
        return self.root, manifest

    def test_authentic_source_identities_and_review_anchors(self) -> None:
        manifest = json.loads((ROOT / "source_manifest.json").read_text(encoding="utf-8"))
        self.assertEqual(len(source_audit.verify_sources(ROOT, manifest)), 5)
        source_audit.verify_review_anchors(ROOT, manifest)
        self.assertEqual(len(manifest["hand_reviewed_preload_sites"]), 7)
        self.assertEqual(len(manifest["hand_reviewed_main_registrations"]), 24)
        self.assertEqual(manifest["enabled_grants"], [])
        self.assertEqual(manifest["main_window_site"]["status"], "UNRESOLVED")

    def test_preload_tamper_refused_by_input_auditor(self) -> None:
        root, manifest = self.vendor_copy()
        preload = root / "vendor/drawio/src/main/electron-preload.js"
        data = bytearray(preload.read_bytes())
        data[0] ^= 1
        preload.write_bytes(data)
        with self.assertRaisesRegex(ValueError, "source identity mismatch"):
            source_audit.verify_sources(root, manifest)

    def test_main_tamper_refused_by_input_auditor(self) -> None:
        root, manifest = self.vendor_copy()
        main = root / "vendor/drawio/src/main/electron.js"
        main.write_bytes(main.read_bytes() + b"\n")
        with self.assertRaisesRegex(ValueError, "source identity mismatch"):
            source_audit.verify_sources(root, manifest)

    def test_review_anchor_drift_refused(self) -> None:
        manifest = json.loads((ROOT / "source_manifest.json").read_text(encoding="utf-8"))
        altered = copy.deepcopy(manifest)
        altered["hand_reviewed_main_registrations"][0]["lines"] = [850]
        with self.assertRaisesRegex(ValueError, "review anchor mismatch"):
            source_audit.verify_review_anchors(ROOT, altered)

    def test_report_does_not_promote_preparation_to_product_acceptance(self) -> None:
        observed = source_audit.report(ROOT)
        self.assertFalse(observed["reviewed_sites_are_classifier_output"])
        self.assertEqual(observed["enabled_grants"], [])
        self.assertTrue(observed["product_AC39_AC44"].startswith("NOT RUN"))
        self.assertEqual(observed["production_main_digest"]["status"], "UNRESOLVED")
        self.assertEqual(observed["trust_root"]["status"], "UNRESOLVED")


if __name__ == "__main__":
    unittest.main(verbosity=2)

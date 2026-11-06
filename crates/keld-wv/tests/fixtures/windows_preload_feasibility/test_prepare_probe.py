"""Preparation controls; these do not execute JavaScript or launch WebView2."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import prepare_probe


class PreparationControls(unittest.TestCase):
    def test_exact_source_bytes_survive_without_transformation(self):
        with tempfile.TemporaryDirectory(prefix="w1-prepare-") as temporary:
            destination = Path(temporary) / "case"
            receipt = prepare_probe.prepare(destination, "document-created")
            source = (prepare_probe.ROOT / "source_binding/vendor/drawio/src/main/electron-preload.js").read_bytes()
            bundle = (destination / "preload-bundle.js").read_bytes()
            self.assertEqual(receipt["preloadBytes"], 3201)
            self.assertEqual(bundle.count(source), 1)
            self.assertEqual(receipt["preloadSha256"], prepare_probe.PRELOAD_SHA256)
            config = json.loads((destination / "config.json").read_text())
            self.assertEqual(config["bundleSha256"], prepare_probe.digest(bundle))

    def test_single_byte_source_change_produces_no_registration_input(self):
        with tempfile.TemporaryDirectory(prefix="w1-corrupt-") as temporary:
            root = Path(temporary)
            relative = Path("source_binding/vendor/drawio/src/main/electron-preload.js")
            source = bytearray((prepare_probe.ROOT / relative).read_bytes())
            source[0] ^= 1
            corrupted = root / "inputs" / relative
            corrupted.parent.mkdir(parents=True)
            corrupted.write_bytes(source)
            destination = root / "refused"
            with patch.object(prepare_probe, "ROOT", root / "inputs"):
                with self.assertRaisesRegex(ValueError, "Pinned preload bytes differ"):
                    prepare_probe.prepare(destination, "document-created")
            self.assertFalse(destination.exists())

    def test_late_injection_control_changes_only_scheduling_configuration(self):
        with tempfile.TemporaryDirectory(prefix="w1-order-") as temporary:
            root = Path(temporary)
            early = prepare_probe.prepare(root / "early", "document-created")
            late = prepare_probe.prepare(root / "late", "dom-content-loaded")
            self.assertEqual(early["bundleSha256"], late["bundleSha256"])
            self.assertEqual(early["htmlSha256"], late["htmlSha256"])
            self.assertEqual(early["preloadSha256"], late["preloadSha256"])


if __name__ == "__main__":
    unittest.main(verbosity=2)

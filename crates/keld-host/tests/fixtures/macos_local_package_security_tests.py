#!/usr/bin/env python3
"""Local source-selection regressions; native codesign is exercised by acceptance."""

import hashlib
import json
from pathlib import Path
import plistlib
import shutil
import stat
import tempfile
import unittest
from unittest.mock import patch

import macos_local_package_acceptance as smoke
import macos_local_package_security as security


class SignatureOracle:
    def __init__(self, application_id: str):
        self.application_id = application_id
        self.calls = []
        self.records = []

    def command(self, label, argv, **_):
        self.calls.append((label, argv))
        if label == "pristine-installed-signature-facts":
            return (0, "", f"Identifier={self.application_id}\nSignature=adhoc\n")
        return (0, "", "")

    def record(self, **fields):
        self.records.append(fields)


class InstalledSelectionTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="keld-package-security-source-")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.home = self.root / "user"
        (self.home / "Applications").mkdir(parents=True)
        self.accepted = self.root / "positive"
        artifacts = self.accepted / "artifacts"
        artifacts.mkdir(parents=True)
        self.installed_root = self.home / "Applications" / "gh1404-install"
        self.installed = self.installed_root / "relocated" / "Renamed-Installed-Hello.app"
        self.build = artifacts / f"{smoke.APP_NAME}.app"
        self.receipt = artifacts / "artifact-receipt.json"
        self.dmg = artifacts / f"{smoke.APP_NAME}.dmg"
        self.dmg.write_bytes(b"fixture installer bytes")
        self.app_id = "dev.gyldlab.macos.acceptance.rfixture"
        self.bun = b"qualified Bun bytes"
        self.host = b"qualified final Host bytes"
        files = {
            "Contents/MacOS/keld-host": self.host,
            "Contents/Helpers/bun": self.bun,
            "Contents/Resources/app/src/main.ts": b"main source",
            "Contents/Resources/app/keld.permissions.jsonc": b'{"profile":"legacy"}',
            "Contents/Resources/app/keld.boot.json": b'{"graph":"fixture"}',
            "Contents/Resources/app/keld.stage-graph.json": b'{"sources":["src/main.ts"]}',
            "Contents/Info.plist": plistlib.dumps({
                "CFBundleIdentifier": self.app_id,
                "CFBundleExecutable": "keld-host",
                "CFBundleName": smoke.APP_NAME,
                "KELDLocalBundle": {"schema": 1, "target": "macos-arm64",
                                    "bunSha256": hashlib.sha256(self.bun).hexdigest()},
            }),
            "Contents/_CodeSignature/CodeResources": b"fixture signature metadata",
        }
        for relative, data in files.items():
            leaf = self.installed / relative
            leaf.parent.mkdir(parents=True, exist_ok=True)
            leaf.write_bytes(data)
            leaf.chmod(0o755 if relative in (
                "Contents/MacOS/keld-host", "Contents/Helpers/bun") else 0o644)
        shutil.copytree(self.installed, self.build)
        signed = []
        for leaf in [self.installed, *self.installed.rglob("*")]:
            relative = "." if leaf == self.installed else leaf.relative_to(self.installed).as_posix()
            metadata = leaf.lstat()
            row = {"path": relative, "mode": stat.S_IMODE(metadata.st_mode)}
            if leaf.is_dir():
                row["kind"] = "directory"
            else:
                row.update(kind="file", size=metadata.st_size,
                           sha256=hashlib.sha256(leaf.read_bytes()).hexdigest())
            signed.append(row)
        signed.sort(key=lambda entry: entry["path"])
        prefix = "Contents/Resources/app"
        captured = [
            {**row, "path": "." if row["path"] == prefix
             else row["path"][len(prefix) + 1:]}
            for row in signed if row["path"] == prefix or row["path"].startswith(prefix + "/")
        ]
        self.receipt.write_text(json.dumps({
            "schema": 1, "artifactClass": "local-adhoc", "target": "macos-arm64",
            "application": {"id": self.app_id, "name": smoke.APP_NAME},
            "imagePayload": {"application": f"{smoke.APP_NAME}.app"},
            "dmgSha256": hashlib.sha256(self.dmg.read_bytes()).hexdigest(),
            "inputHost": {"file": {"sha256": hashlib.sha256(b"input Host").hexdigest()},
                          "declaredVersion": "host-label"},
            "inputBun": {"file": {"sha256": hashlib.sha256(self.bun).hexdigest()},
                         "declaredVersion": "bun-label"},
            "finalHostSha256": hashlib.sha256(self.host).hexdigest(),
            "finalBunSha256": hashlib.sha256(self.bun).hexdigest(),
            "capturedApplicationFiles": captured, "signedApplicationFiles": signed,
        }), encoding="utf-8")
        self.accepted.joinpath("events.jsonl").write_text("\n".join(json.dumps(row) for row in [
            {"step": "qualified-inputs", "application_id": self.app_id,
             "host_sha256": hashlib.sha256(b"input Host").hexdigest(),
             "bun_sha256": hashlib.sha256(self.bun).hexdigest(),
             "host_version": "host-label", "bun_version": "bun-label"},
            {"step": "published-content", "receipt_sha256": smoke.file_hash(self.receipt),
             "host_sha256": hashlib.sha256(self.host).hexdigest(),
             "bun_sha256": hashlib.sha256(self.bun).hexdigest(),
             "dmg_sha256": hashlib.sha256(self.dmg.read_bytes()).hexdigest()},
        ]) + "\n", encoding="utf-8")
        self.prior = {
            "state": "PASS", "evidence": str(self.accepted),
            "installed_lifecycle_verified": True, "native_quit_accepted": True,
            "installed_root": str(self.installed_root), "installed_app": str(self.installed),
            "app": str(self.build), "receipt": str(self.receipt), "dmg": str(self.dmg),
            "application_id": self.app_id,
            "launches": [
                {"phase": "installed", "app": str(self.installed_root / self.build.name),
                 "native_quit": {"state": "ACCEPTED"}, "host_exit": {"exit_code": 0},
                 "ready_echo_before_quit": True, "application_quit_reply": True,
                 "unsignaled_retirement": True},
                {"phase": "relocated", "app": str(self.installed),
                 "native_quit": {"state": "ACCEPTED"}, "host_exit": {"exit_code": 0},
                 "ready_echo_before_quit": True, "application_quit_reply": True,
                 "unsignaled_retirement": True},
            ],
        }
        self.oracle = SignatureOracle(self.app_id)
        self.home_patch = patch.object(Path, "home", return_value=self.home)
        self.home_patch.start()
        self.addCleanup(self.home_patch.stop)

    def verify(self):
        return security.verify_installed_source(self.prior, self.accepted, self.oracle)

    def test_positive_selects_relocated_copy_and_checks_signature_and_full_graph(self):
        app, host, bun = self.verify()
        self.assertEqual((app, host, bun), (self.installed, hashlib.sha256(self.host).hexdigest(),
                                             hashlib.sha256(self.bun).hexdigest()))
        self.assertTrue(self.oracle.calls)
        self.assertTrue(all(Path(argv[-1]).is_relative_to(self.installed)
                            for _, argv in self.oracle.calls))
        self.assertFalse(any(str(self.build) in row[1] for row in self.oracle.calls))

    def test_wrong_built_app_copy_is_refused_even_when_all_bytes_match(self):
        self.prior["installed_app"] = str(self.build)
        with self.assertRaisesRegex(RuntimeError, "relocated installed"):
            self.verify()
        self.assertEqual(self.oracle.calls, [])

    def test_changed_selected_graph_is_refused_before_variants(self):
        (self.installed / "Contents/Resources/app/keld.stage-graph.json").write_bytes(b"altered")
        with self.assertRaisesRegex(RuntimeError, "inventory"):
            self.verify()


if __name__ == "__main__":
    unittest.main()

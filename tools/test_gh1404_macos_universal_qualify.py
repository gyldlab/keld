"""Fail-closed input/oracle contracts for GH-1404 Universal native preflight.

These mocked command receipts are parser tests, not OS-signing or Intel execution.
Real native admission requires the signed installed app on hosted runners.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import plistlib
import subprocess
import tempfile
import unittest

import gh1404_macos_universal_qualify as gate


SOURCE = "d9805ea48cb4beba9e26d29c46b0b6519b7e62dc"
ENTITLEMENTS = plistlib.dumps({"com.apple.security.cs.allow-jit": True}, fmt=plistlib.FMT_XML)
TEAM = "ABCDEF1234"


class NativeToolFixture:
    def __init__(self) -> None:
        self.calls: list[tuple[str, ...]] = []
        self.fail_signature = False
        self.archs = b"x86_64 arm64\n"
        self.identity = "org.bun.fixture"
        self.outer_identity = "org.keld.native.fixture"
        self.team = TEAM
        self.flags = "0x10000"
        self.entitlements = ENTITLEMENTS
        self.on_bun_verify = None

    def __call__(self, argv: list[str], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
        self.calls.append(tuple(argv))
        if argv[:2] == [gate.NATIVE_LIPO, "-archs"]:
            return subprocess.CompletedProcess(argv, 0, self.archs, b"")
        if argv[0] == gate.NATIVE_SIGNATURE:
            if "--verify" in argv:
                if self.fail_signature:
                    return subprocess.CompletedProcess(argv, 1, b"", b"invalid signature")
                if str(argv[-1]).endswith("/bun") and self.on_bun_verify is not None:
                    mutate = self.on_bun_verify
                    self.on_bun_verify = None
                    mutate()
                return subprocess.CompletedProcess(argv, 0, b"", b"valid")
            if "--entitlements" in argv:
                return subprocess.CompletedProcess(argv, 0, self.entitlements, b"")
            if "--display" in argv:
                identifier = self.outer_identity if str(argv[-1]).endswith(".app") else self.identity
                output = (f"Identifier={identifier}\nTeamIdentifier={self.team}\n"
                          f"CodeDirectory v=20400 size=100 flags={self.flags}(runtime) hashes=4\n").encode()
                return subprocess.CompletedProcess(argv, 0, b"", output)
        raise AssertionError(f"Unexpected subprocess: {argv}")


class QualificationContracts(unittest.TestCase):
    def setUp(self) -> None:
        temp = tempfile.TemporaryDirectory(prefix="gh1404-macos-native-")
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.app = self.root / "Example.app"
        self.host = self.app / "Contents/MacOS/keld-host"
        self.bun = self.app / "Contents/Helpers/bun"
        self.info = self.app / "Contents/Info.plist"
        self.host.parent.mkdir(parents=True)
        self.bun.parent.mkdir(parents=True)
        self.host.write_bytes(b"host qualified bytes")
        self.bun.write_bytes(b"bun vendor signed qualified bytes")
        self.host.chmod(0o755)
        self.bun.chmod(0o755)
        self.native = NativeToolFixture()
        self.record: dict[str, object] = {
            "schema": 1,
            "sourceCommit": SOURCE,
            "target": "macos-universal",
            "bundleIdentifier": "org.keld.native.fixture",
            "hostSha256": gate._digest(self.host),
            "bunSha256": gate._digest(self.bun),
            "bunSignatureIdentifier": "org.bun.fixture",
            "bunTeamIdentifier": TEAM,
            "bunSigningFlags": "0x10000",
            "bunEntitlementsSha256": hashlib.sha256(ENTITLEMENTS).hexdigest(),
        }
        self.write_plist()

    def write_plist(self, *, target: str = "macos-universal", signed_bun: str | None = None) -> None:
        document = {
            "CFBundleExecutable": "keld-host",
            "CFBundleIdentifier": "org.keld.native.fixture",
            "CFBundlePackageType": "APPL",
            "LSMinimumSystemVersion": "14.0",
            "KELDLocalBundle": {"schema": 1, "target": target,
                                "bunSha256": signed_bun or self.record["bunSha256"]},
        }
        self.info.write_bytes(plistlib.dumps(document, fmt=plistlib.FMT_XML, sort_keys=True))

    def verify(self) -> dict[str, object]:
        record = gate.qualification_record(json.dumps(self.record))
        return gate.verify_bundle(self.app, record, SOURCE, self.native)

    def assert_refusal(self, needle: str) -> None:
        with self.assertRaisesRegex(gate.QualificationError, needle):
            self.verify()

    def test_valid_qualified_fixture_invokes_every_os_gate(self) -> None:
        result = self.verify()
        self.assertEqual(result["status"], "signed-bundle-preflight-only")
        self.assertEqual(result["archs"], ["arm64", "x86_64"])
        self.assertEqual(len(self.native.calls), 8)
        self.assertTrue(any("--deep" in call for call in self.native.calls))
        self.assertTrue(any("--entitlements" in call for call in self.native.calls))

    def test_modified_host_and_bun_rejected_against_independent_pins(self) -> None:
        self.host.write_bytes(b"different host bytes")
        self.assert_refusal("Host bytes differ")
        self.host.write_bytes(b"host qualified bytes")
        self.bun.write_bytes(b"another signed Bun could have these bytes")
        self.assert_refusal("Bun bytes differ")

    def test_native_signing_time_substitution_cannot_keep_old_digest(self) -> None:
        self.native.on_bun_verify = lambda: self.bun.write_bytes(b"replaced after input pin")
        self.assert_refusal("changed during native preflight")

    def test_self_consistent_plist_digest_is_insufficient(self) -> None:
        foreign = "ef" * 32
        self.write_plist(signed_bun=foreign)
        self.assert_refusal("sealed Bun digest differs")

    def test_missing_or_wrong_target_refused(self) -> None:
        self.write_plist(target="macos-x64")
        self.assert_refusal("bundle target mismatch")
        self.write_plist(target="macos-arm64")
        self.assert_refusal("bundle target mismatch")

    def test_wrong_architecture_or_unsigned_bun_refused(self) -> None:
        self.native.archs = b"arm64\n"
        self.assert_refusal("expected exactly arm64 and x86_64")
        self.native.archs = b"arm64 x86_64\n"
        self.native.fail_signature = True
        self.assert_refusal("required native tool failed")

    def test_wrong_signer_and_changed_entitlements_refused(self) -> None:
        for name, attribute, value in [
            ("signer", "identity", "org.unqualified.app"),
            ("team", "team", "ZZZZZZZZZZ"),
            ("flags", "flags", "0x0"),
            ("entitlements", "entitlements", plistlib.dumps({"changed": True})),
        ]:
            with self.subTest(name=name):
                previous = getattr(self.native, attribute)
                setattr(self.native, attribute, value)
                self.assert_refusal("differs from qualified|entitlements differ")
                setattr(self.native, attribute, previous)

    def test_outer_identifier_and_required_minimum_version_refused(self) -> None:
        original = self.info.read_bytes()
        self.info.write_bytes(original.replace(b"<string>14.0</string>", b"<string>broken</string>"))
        self.assert_refusal("minimum macOS version")
        self.info.write_bytes(original)
        self.record["bundleIdentifier"] = "org.keld.forged.fixture"
        self.assert_refusal("bundle identifier mismatch")
        self.record["bundleIdentifier"] = "org.keld.native.fixture"
        self.native.outer_identity = "org.keld.other.fixture"
        self.assert_refusal("outer signature identifier differs")

    def test_linked_executable_parent_refused(self) -> None:
        original = self.bun.read_bytes()
        self.bun.unlink()
        self.bun.parent.rmdir()
        external = self.root / "external-helpers"
        external.mkdir()
        (external / "bun").write_bytes(original)
        (external / "bun").chmod(0o755)
        self.bun.parent.symlink_to(external, target_is_directory=True)
        self.assert_refusal("executable parent is linked")

    def test_symlink_or_nonexecutable_nested_code_refused(self) -> None:
        self.bun.unlink()
        self.bun.symlink_to(self.host)
        self.assert_refusal("symlink")
        self.bun.unlink()
        self.bun.write_bytes(b"bun vendor signed qualified bytes")
        self.bun.chmod(0o644)
        self.assert_refusal("not executable")

    def test_source_commit_must_match_supplied_approved_head(self) -> None:
        with self.assertRaisesRegex(gate.QualificationError, "checkout differs"):
            gate.verify_bundle(self.app, self.record, "b" * 40, self.native)

    def test_qualification_json_strict_unknown_duplicate_and_missing(self) -> None:
        canonical = json.dumps(self.record)
        self.assertEqual(gate.qualification_record(canonical)["target"], "macos-universal")
        with self.assertRaisesRegex(gate.QualificationError, "duplicate qualification key"):
            gate.qualification_record(canonical[:-1] + ',"target":"macos-universal"}')
        for changed in [dict(self.record, unknown=True),
                        {key: val for key, val in self.record.items() if key != "bunSha256"},
                        dict(self.record, bunSha256="FA" * 32),
                        dict(self.record, target="macos-arm64"),
                        dict(self.record, bunTeamIdentifier="not set")]:
            with self.subTest(changed=changed):
                with self.assertRaises(gate.QualificationError):
                    gate.qualification_record(json.dumps(changed))

    def test_external_qualification_requires_a_separately_pinned_digest(self) -> None:
        raw = json.dumps(self.record).encode("utf-8")
        trusted_digest = hashlib.sha256(raw).hexdigest()
        self.assertEqual(gate.pinned_qualification_record(raw, trusted_digest)["target"],
                         "macos-universal")
        for incorrect in ["", "f" * 64, trusted_digest.upper()]:
            with self.subTest(incorrect=incorrect):
                with self.assertRaises(gate.QualificationError):
                    gate.pinned_qualification_record(raw, incorrect)
        with self.assertRaisesRegex(gate.QualificationError, "oversized"):
            gate.pinned_qualification_record(raw + b" " * 16384, trusted_digest)

    def test_runner_matrix_all_four_labels(self) -> None:
        for label, (version, machine, arch) in gate.RUNNERS.items():
            with self.subTest(label=label):
                result = gate.runner_observation(label, system="Darwin", machine=machine,
                    os_version=f"{version}.6.1", rosetta=False,
                    environment={"GITHUB_ACTIONS": "true", "RUNNER_OS": "macOS", "RUNNER_ARCH": arch})
                self.assertEqual(result["label"], label)
                self.assertEqual(result["machine"], machine)

    def test_native_runner_wrong_label_or_translation_or_env_refused(self) -> None:
        base = dict(system="Darwin", machine="x86_64", os_version="15.7.9", rosetta=False,
                    environment={"GITHUB_ACTIONS": "true", "RUNNER_OS": "macOS", "RUNNER_ARCH": "X64"})
        for override in [dict(machine="arm64"), dict(os_version="26.6"), dict(rosetta=True),
                         dict(environment={}), dict(environment={"GITHUB_ACTIONS": "true"})]:
            with self.subTest(override=override):
                with self.assertRaises(gate.QualificationError):
                    gate.runner_observation("macos-15-intel", **dict(base, **override))
        with self.assertRaises(gate.QualificationError):
            gate.runner_observation("macos-14", **base)

    def test_bundle_xml_rejection_and_no_success_from_wrong_shape(self) -> None:
        self.info.write_bytes(b"bplist00")
        self.assert_refusal("canonical XML")
        self.info.write_bytes(b"<not-a-plist/>")
        self.assert_refusal("canonical XML")
        self.write_plist()
        self.info.write_bytes(self.info.read_bytes() + b" " * 65537)
        self.assert_refusal("bounded canonical XML")


if __name__ == "__main__":
    unittest.main()

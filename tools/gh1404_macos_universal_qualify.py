#!/usr/bin/env python3
"""GH-1404 native macOS Universal *preflight*, not editor acceptance.

Run only with a separately authenticated qualification record. It checks the
observed runner and signed final .app; Core owns Mach-O parsing and app admission.
The caller must additionally run installed Host/Guardian/Bun and native workflows.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import stat
import subprocess
import sys
from typing import Callable, Mapping


RUNNERS = {
    "macos-15-intel": (15, "x86_64", "X64"),
    "macos-15": (15, "arm64", "ARM64"),
    "macos-26-intel": (26, "x86_64", "X64"),
    "macos-26": (26, "arm64", "ARM64"),
}
RECORD_FIELDS = frozenset({
    "schema", "sourceCommit", "target", "bundleIdentifier", "hostSha256",
    "bunSha256", "bunSignatureIdentifier", "bunTeamIdentifier",
    "bunSigningFlags", "bunEntitlementsSha256",
})
HEX_64 = re.compile(r"[0-9a-f]{64}\Z")
SHA = re.compile(r"[0-9a-f]{40}\Z")
NATIVE_SIGNATURE = "/usr/bin/codesign"
NATIVE_LIPO = "/usr/bin/lipo"


class QualificationError(ValueError):
    """A native observation failed a required, explicit admission predicate."""


def require(predicate: bool, reason: str) -> None:
    if not predicate:
        raise QualificationError(f"GH1404-CI-001: {reason}")


def non_duplicate_pairs(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate qualification key: {key}")
        result[key] = value
    return result


def qualification_record(text: str) -> dict[str, object]:
    try:
        record = json.loads(text, object_pairs_hook=non_duplicate_pairs)
    except (ValueError, json.JSONDecodeError) as exc:
        raise QualificationError(f"GH1404-CI-001: invalid qualification JSON: {exc}") from exc
    require(isinstance(record, dict), "qualification must be a JSON object")
    require(set(record) == RECORD_FIELDS, "qualification record has missing or unknown fields")
    require(type(record["schema"]) is int and record["schema"] == 1, "qualification schema must be 1")
    require(isinstance(record["sourceCommit"], str) and bool(SHA.fullmatch(record["sourceCommit"])),
            "qualification sourceCommit must be a full SHA-1")
    require(record["target"] == "macos-universal", "qualification requires macos-universal target")
    for field in ("hostSha256", "bunSha256", "bunEntitlementsSha256"):
        require(isinstance(record[field], str) and bool(HEX_64.fullmatch(record[field])),
                f"qualification {field} requires lowercase SHA-256")
    for field in ("bundleIdentifier", "bunSignatureIdentifier", "bunTeamIdentifier", "bunSigningFlags"):
        require(isinstance(record[field], str) and bool(record[field]) and len(record[field]) <= 255,
                f"qualification {field} must be a nonempty pinned value")
    require(re.fullmatch(r"[A-Z0-9]{10}", str(record["bunTeamIdentifier"])) is not None,
            "signed Bun needs a pinned 10-character TeamIdentifier")
    require(re.fullmatch(r"0x[0-9a-fA-F]+", str(record["bunSigningFlags"])) is not None,
            "signed Bun flags require a pinned hexadecimal value")
    return record


def pinned_qualification_record(contents: bytes, externally_pinned_sha256: str) -> dict[str, object]:
    """Compare an external receipt to a digest supplied by the independent CI owner.

    The caller must authenticate the digest's provenance outside this program.
    A hash taken from the same PR or bundle is not a trusted qualification.
    """
    require(isinstance(externally_pinned_sha256, str)
            and bool(HEX_64.fullmatch(externally_pinned_sha256)),
            "independent qualification digest must be a lowercase SHA-256")
    require(0 < len(contents) <= 16 * 1024, "qualification record is absent or oversized")
    require(hashlib.sha256(contents).hexdigest() == externally_pinned_sha256,
            "qualification record differs from independently supplied pin")
    return qualification_record(contents.decode("utf-8"))


def runner_observation(label: str, *, system: str, machine: str, os_version: str,
                       environment: Mapping[str, str], rosetta: bool) -> dict[str, object]:
    require(label in RUNNERS, "unlisted native runner label")
    major, required_machine, required_runner_arch = RUNNERS[label]
    require(system == "Darwin", "native qualification requires Darwin")
    require(machine == required_machine, f"runner {label} has wrong physical architecture {machine}")
    require(os_version.split(".", 1)[0] == str(major), f"runner {label} has wrong macOS version")
    require(not rosetta, "Rosetta translation cannot prove native Intel qualification")
    require(environment.get("GITHUB_ACTIONS") == "true", "qualification needs hosted Actions context")
    require(environment.get("RUNNER_OS") == "macOS", "runner OS witness is not macOS")
    require(environment.get("RUNNER_ARCH") == required_runner_arch, "runner architecture witness disagrees")
    return {"label": label, "os": os_version, "machine": machine, "runnerArch": required_runner_arch}


def _native(args: list[str], runner: Callable[..., subprocess.CompletedProcess[bytes]]) -> bytes:
    try:
        result = runner(args, capture_output=True, check=False, timeout=25)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise QualificationError(f"GH1404-CI-001: required native tool unavailable: {args[0]}") from exc
    require(result.returncode == 0, f"required native tool failed: {args[0]} {args[1]}")
    return result.stdout


def _read_code_facts(file: Path, runner: Callable[..., subprocess.CompletedProcess[bytes]]) -> dict[str, str]:
    result = runner([NATIVE_SIGNATURE, "--display", "--verbose=4", str(file)],
                    capture_output=True, check=False, timeout=25)
    require(result.returncode == 0, "Bun signing facts unavailable")
    output = (result.stdout + result.stderr).decode("utf-8", errors="replace")
    facts: dict[str, str] = {}
    for field, pattern in {
        "identifier": r"(?m)^Identifier=(\S+)$",
        "team": r"(?m)^TeamIdentifier=(\S+)$",
        "flags": r"(?m)^CodeDirectory .+ flags=(0x[0-9a-fA-F]+)",
    }.items():
        match = re.search(pattern, output)
        require(match is not None, f"Bun signing field missing: {field}")
        facts[field] = match.group(1)
    return facts


def _outer_identifier(app: Path, runner: Callable[..., subprocess.CompletedProcess[bytes]]) -> str:
    result = runner([NATIVE_SIGNATURE, "--display", "--verbose=4", str(app)],
                    capture_output=True, check=False, timeout=25)
    require(result.returncode == 0, "outer signature facts unavailable")
    output = (result.stdout + result.stderr).decode("utf-8", errors="replace")
    match = re.search(r"(?m)^Identifier=(\S+)$", output)
    require(match is not None, "outer signature identifier missing")
    return match.group(1)


def _pin_regular(path: Path, *, executable: bool) -> None:
    require(path.exists(), f"required bundle file missing: {path.name}")
    require(not path.is_symlink(), f"bundle member is a symlink: {path.name}")
    metadata = path.stat()
    require(stat.S_ISREG(metadata.st_mode), f"bundle member must be regular: {path.name}")
    if executable:
        require(bool(metadata.st_mode & stat.S_IXUSR), f"bundle member is not executable: {path.name}")


def _digest(file: Path) -> str:
    hash_ = hashlib.sha256()
    with file.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            hash_.update(chunk)
    return hash_.hexdigest()


def _archs(file: Path, runner: Callable[..., subprocess.CompletedProcess[bytes]]) -> list[str]:
    tokens = _native([NATIVE_LIPO, "-archs", str(file)], runner).decode("ascii", errors="replace").split()
    require(len(tokens) == 2 and set(tokens) == {"arm64", "x86_64"},
            f"expected exactly arm64 and x86_64 slices in {file.name}")
    return sorted(tokens)


def verify_bundle(app: Path, receipt: Mapping[str, object], source_commit: str,
                  runner: Callable[..., subprocess.CompletedProcess[bytes]] = subprocess.run) -> dict[str, object]:
    """Compare signed output to qualified external pins; no application execution."""
    require(app.name.endswith(".app") and app.is_dir() and not app.is_symlink(), "invalid .app root")
    require(source_commit == receipt["sourceCommit"], "checkout differs from approved source commit")
    host = app / "Contents/MacOS/keld-host"
    bun = app / "Contents/Helpers/bun"
    info = app / "Contents/Info.plist"
    for parent in (app / "Contents", host.parent, bun.parent):
        require(parent.is_dir() and not parent.is_symlink(),
                f"bundle executable parent is linked or missing: {parent.name}")
    for file, executable in ((host, True), (bun, True), (info, False)):
        _pin_regular(file, executable=executable)
    # Core's strict XML parser and Apple Security.framework are still authoritative.
    with info.open("rb") as stream:
        document = stream.read(65537)
    require(len(document) <= 65536 and document.startswith(b'<?xml version="1.0" encoding="UTF-8"?>'),
            "Info.plist must be bounded canonical XML")
    try:
        metadata = plistlib.loads(document)
    except (ValueError, TypeError, plistlib.InvalidFileException) as exc:
        raise QualificationError("GH1404-CI-001: invalid Info.plist") from exc
    require(isinstance(metadata, dict) and metadata.get("CFBundleExecutable") == "keld-host"
            and metadata.get("CFBundlePackageType") == "APPL", "invalid sealed application metadata")
    local = metadata.get("KELDLocalBundle")
    require(isinstance(local, dict) and type(local.get("schema")) is int and local["schema"] == 1,
            "missing closed KELD local bundle record")
    require(local.get("target") == receipt["target"] == "macos-universal", "bundle target mismatch")
    require(metadata.get("CFBundleIdentifier") == receipt["bundleIdentifier"], "bundle identifier mismatch")
    minimum = metadata.get("LSMinimumSystemVersion")
    require(isinstance(minimum, str) and re.fullmatch(r"[0-9]+(?:\.[0-9]+){1,2}", minimum) is not None,
            "missing or invalid minimum macOS version")
    require(local.get("bunSha256") == receipt["bunSha256"], "sealed Bun digest differs from external pin")
    host_hash, bun_hash = _digest(host), _digest(bun)
    metadata_hash = hashlib.sha256(document).hexdigest()
    require(host_hash == receipt["hostSha256"], "Host bytes differ from qualified input")
    require(bun_hash == receipt["bunSha256"], "Bun bytes differ from qualified signed input")
    _archs(host, runner)
    _archs(bun, runner)
    _native([NATIVE_SIGNATURE, "--verify", "--strict", "--verbose=2", str(host)], runner)
    _native([NATIVE_SIGNATURE, "--verify", "--strict", "--verbose=2", str(bun)], runner)
    _native([NATIVE_SIGNATURE, "--verify", "--deep", "--strict", "--verbose=2", str(app)], runner)
    require(_outer_identifier(app, runner) == receipt["bundleIdentifier"],
            "outer signature identifier differs from qualified bundle identity")
    bun_facts = _read_code_facts(bun, runner)
    for fact, key in (("identifier", "bunSignatureIdentifier"),
                      ("team", "bunTeamIdentifier"), ("flags", "bunSigningFlags")):
        require(bun_facts[fact] == receipt[key], f"Bun {fact} differs from qualified signing identity")
    entitlements = _native([NATIVE_SIGNATURE, "--display", "--entitlements", "-", str(bun)], runner)
    require(bool(entitlements), "Bun lacks independently pinned entitlements")
    require(hashlib.sha256(entitlements).hexdigest() == receipt["bunEntitlementsSha256"],
            "Bun entitlements differ from qualified signed input")
    require(_digest(host) == host_hash and _digest(bun) == bun_hash
            and _digest(info) == metadata_hash,
            "signed bundle files changed during native preflight")
    return {"status": "signed-bundle-preflight-only", "target": receipt["target"],
            "sourceCommit": source_commit, "hostSha256": host_hash, "bunSha256": bun_hash,
            "archs": ["arm64", "x86_64"], "minimumSystemVersion": minimum,
            "bunSigner": bun_facts}


def _observed_runner(label: str) -> dict[str, object]:
    version = _native(["/usr/bin/sw_vers", "-productVersion"], subprocess.run).decode().strip()
    translated = subprocess.run(["/usr/sbin/sysctl", "-in", "sysctl.proc_translated"],
                                capture_output=True, check=False, timeout=8)
    return runner_observation(label, system=platform.system(), machine=platform.machine(),
                              os_version=version, environment=os.environ,
                              rosetta=translated.returncode == 0 and translated.stdout.strip() == b"1")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner-label", required=True, choices=tuple(RUNNERS))
    parser.add_argument("--app", type=Path, required=True, help="Installed signed app copy")
    parser.add_argument("--qualification", type=Path, required=True,
                        help="Independently authenticated external signed-input record")
    parser.add_argument("--qualification-sha256", required=True,
                        help="SHA-256 pinned by an independent trusted CI owner, outside PR input")
    parser.add_argument("--source-commit", required=True, help="Exact checkout SHA")
    parser.add_argument("--output", type=Path, required=True, help="New JSON preflight receipt path")
    args = parser.parse_args(argv)
    try:
        observed = _observed_runner(args.runner_label)
        require(not args.qualification.is_symlink() and args.qualification.is_file(),
                "qualification record is absent or linked")
        require(not args.qualification.resolve().is_relative_to(args.app.resolve()),
                "qualification record must be outside the app")
        record = pinned_qualification_record(args.qualification.read_bytes(), args.qualification_sha256)
        evidence = {"runner": observed, "bundle": verify_bundle(args.app, record, args.source_commit),
                    "installedLaunch": "UNVERIFIED", "nativeEditorWorkflows": "UNVERIFIED"}
        # Exclusive output: never overwrite an older success receipt.
        with args.output.open("x", encoding="utf-8") as output:
            json.dump(evidence, output, indent=2, sort_keys=True)
            output.write("\n")
        print(f"GH1404-CI-PREFLIGHT: signed input and native runner verified: {args.output}")
        return 0
    except (QualificationError, FileExistsError, OSError, UnicodeError) as exc:
        print(f"GH1404-CI-REFUSED: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

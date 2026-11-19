#!/usr/bin/env python3
"""Real installed-bundle refusal controls; consumes a separately passing smoke run.

The original app and installed copy are never modified. Every mutation uses a new
owned copy. A valid signature is explicitly required for semantic controls, so a
broken seal cannot accidentally satisfy an identity/profile rejection assertion.
"""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shlex
import shutil
import stat
import subprocess
import sys
import time

import macos_local_package_acceptance as smoke


def exact_census(root: Path) -> list[dict[str, object]]:
    result = []
    for pid, ppid, started, command in smoke.process_rows():
        if "keld-host" not in command and "bun" not in command:
            continue
        executable = smoke.executable_of(pid)
        if executable and Path(executable).is_relative_to(root):
            result.append({"pid": pid, "ppid": ppid, "started": started,
                           "executable": executable})
    return result


def signed_file_inventory(app: Path) -> list[dict[str, object]]:
    """Observe the complete selected bundle, including its compiled boot graph.

    The publisher's signedApplicationFiles receipt is a full tree inventory:
    paths, kinds, modes, lengths and digests. Never follow a bundle symlink.
    """
    rows: list[dict[str, object]] = []

    def visit(leaf: Path, relative: str) -> None:
        smoke.require(len(rows) < 100_032 and relative.count("/") <= 64,
                      "installed app inventory exceeded publisher bounds")
        metadata = leaf.lstat()
        row: dict[str, object] = {"path": relative, "mode": stat.S_IMODE(metadata.st_mode)}
        if stat.S_ISDIR(metadata.st_mode):
            row["kind"] = "directory"
            rows.append(row)
            for child in sorted(leaf.iterdir(), key=lambda path: path.name):
                visit(child, child.name if relative == "." else relative + "/" + child.name)
        elif stat.S_ISREG(metadata.st_mode):
            row.update(kind="file", size=metadata.st_size, sha256=smoke.file_hash(leaf))
            rows.append(row)
        else:
            raise RuntimeError(f"selected installed app contains a link or special file: {relative}")

    visit(app, ".")
    return sorted(rows, key=lambda row: str(row["path"]))


def verify_installed_source(
    prior: dict[str, object], accepted: Path, evidence: smoke.Evidence,
) -> tuple[Path, str, str]:
    """Prove the exact relocated installation and its complete signed payload."""
    smoke.require(prior.get("state") == "PASS" and prior.get("evidence") == str(accepted)
                  and prior.get("installed_lifecycle_verified") is True
                  and prior.get("native_quit_accepted") is True,
                  "a separately passing installed and relocated lifecycle is required")
    install_root = Path(prior["installed_root"])
    app = Path(prior["installed_app"])
    built = Path(prior["app"])
    receipt_path = Path(prior["receipt"])
    dmg = Path(prior["dmg"])
    smoke.require(install_root.is_absolute() and
                  install_root.parent == Path.home() / "Applications" and
                  install_root.is_dir() and not install_root.is_symlink(),
                  "selected installation must be a real ~/Applications child")
    smoke.require(app == install_root / "relocated" / "Renamed-Installed-Hello.app"
                  and app != built, "selected app is not the relocated installed application")
    smoke.require(app.is_dir() and not app.is_symlink() and not app.parent.is_symlink(),
                  "relocated installed app is missing or is a symlink")
    smoke.require(built == accepted / "artifacts" / f"{smoke.APP_NAME}.app"
                  and receipt_path == accepted / "artifacts" / "artifact-receipt.json"
                  and receipt_path.is_file() and not receipt_path.is_symlink(),
                  "the original build output and external receipt lack accepted provenance")
    smoke.require(dmg == accepted / "artifacts" / f"{smoke.APP_NAME}.dmg"
                  and dmg.is_file() and not dmg.is_symlink(),
                  "the separately accepted installer image is missing")

    launches = prior.get("launches")
    smoke.require(isinstance(launches, list) and len(launches) == 2,
                  "two separately verified installed lifecycle launches are required")
    for record, phase, path in zip(
        launches, ("installed", "relocated"),
        (install_root / f"{smoke.APP_NAME}.app", app), strict=True
    ):
        smoke.require(
            isinstance(record, dict) and record.get("phase") == phase
            and record.get("app") == str(path)
            and record.get("native_quit", {}).get("state") == "ACCEPTED"
            and record.get("host_exit", {}).get("exit_code") == 0
            and all(record.get(key) is True for key in (
                "ready_echo_before_quit", "application_quit_reply", "unsignaled_retirement")),
            f"{phase} lifecycle evidence is incomplete")

    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    smoke.require(receipt.get("schema") == 1
                  and receipt.get("artifactClass") == "local-adhoc"
                  and receipt.get("target") == "macos-arm64"
                  and receipt.get("imagePayload", {}).get("application") == built.name
                  and receipt.get("dmgSha256") == smoke.file_hash(dmg),
                  "selected source receipt is not the accepted local macOS package")
    identity = receipt.get("application", {})
    smoke.require(identity.get("id") == prior.get("application_id")
                  and identity.get("name") == smoke.APP_NAME,
                  "selected app identity differs from its accepted receipt")
    host_digest = receipt.get("finalHostSha256")
    bun_digest = receipt.get("inputBun", {}).get("file", {}).get("sha256")
    input_host_digest = receipt.get("inputHost", {}).get("file", {}).get("sha256")
    smoke.require(all(isinstance(digest, str) and smoke.HEX_256.fullmatch(digest)
                      for digest in (host_digest, bun_digest, input_host_digest))
                  and receipt.get("finalBunSha256") == bun_digest,
                  "selected executable digests disagree with receipt qualification")
    accepted_events = [
        json.loads(line) for line in (accepted / "events.jsonl").read_text(encoding="utf-8").splitlines()
    ]
    qualified = [row for row in accepted_events if row.get("step") == "qualified-inputs"]
    published = [row for row in accepted_events if row.get("step") == "published-content"]
    smoke.require(len(qualified) == len(published) == 1
                  and qualified[0].get("application_id") == identity["id"]
                  and qualified[0].get("host_sha256") == input_host_digest
                  and qualified[0].get("bun_sha256") == bun_digest
                  and receipt.get("inputHost", {}).get("declaredVersion")
                      == qualified[0].get("host_version")
                  and receipt.get("inputBun", {}).get("declaredVersion")
                      == qualified[0].get("bun_version")
                  and published[0].get("receipt_sha256") == smoke.file_hash(receipt_path)
                  and published[0].get("host_sha256") == host_digest
                  and published[0].get("bun_sha256") == bun_digest
                  and published[0].get("dmg_sha256") == receipt["dmgSha256"],
                  "selected receipt/binary provenance differs from the passing acceptance run")

    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    smoke.require(info.get("CFBundleIdentifier") == identity["id"]
                  and info.get("CFBundleName") == identity["name"]
                  and info.get("CFBundleExecutable") == "keld-host"
                  and info.get("KELDLocalBundle") == {
                      "schema": 1, "target": "macos-arm64", "bunSha256": bun_digest,
                  }, "installed app signed identity or local boot seal differs from receipt")
    inventory = signed_file_inventory(app)
    smoke.require(inventory == receipt.get("signedApplicationFiles"),
                  "relocated installed app complete signed inventory differs from receipt")
    prefix = "Contents/Resources/app"
    selected_graph = [
        {**row, "path": "." if row["path"] == prefix else row["path"][len(prefix) + 1:]}
        for row in inventory
        if row["path"] == prefix or str(row["path"]).startswith(prefix + "/")
    ]
    smoke.require(selected_graph == receipt.get("capturedApplicationFiles"),
                  "installed graph provenance differs from captured publisher inputs")
    by_path = {str(row["path"]): row for row in inventory}
    smoke.require(all(by_path.get(relative, {}).get("kind") == "file" for relative in (
        "Contents/MacOS/keld-host", "Contents/Helpers/bun",
        "Contents/Resources/app/src/main.ts",
        "Contents/Resources/app/keld.boot.json",
        "Contents/Resources/app/keld.stage-graph.json",
        "Contents/Resources/app/keld.permissions.jsonc",
        "Contents/Info.plist", "Contents/_CodeSignature/CodeResources")),
        "selected signed app is missing its binary, signature or boot-graph source")
    smoke.require(by_path["Contents/MacOS/keld-host"]["sha256"] == host_digest
                  and by_path["Contents/Helpers/bun"]["sha256"] == bun_digest
                  and by_path["Contents/MacOS/keld-host"]["mode"] == 0o755
                  and by_path["Contents/Helpers/bun"]["mode"] == 0o755,
                  "selected Host/Bun bytes or executable modes differ from receipt")
    evidence.command("pristine-installed-signature",
                     ["/usr/bin/codesign", "--verify", "--deep", "--strict",
                      "--verbose=2", str(app)])
    evidence.command("pristine-installed-bun-signature",
                     ["/usr/bin/codesign", "--verify", "--strict",
                      str(app / "Contents/Helpers/bun")])
    _, stdout, stderr = evidence.command(
        "pristine-installed-signature-facts",
        ["/usr/bin/codesign", "--display", "--verbose=4", str(app)])
    facts = (stdout + "\n" + stderr).splitlines()
    smoke.require([row.removeprefix("Identifier=") for row in facts
                   if row.startswith("Identifier=")] == [identity["id"]]
                  and "Signature=adhoc" in facts,
                  "relocated installed app signature identity or class differs from receipt")
    evidence.record(step="pristine-relocated-source-verified", installed_app=str(app),
                    receipt=str(receipt_path), receipt_sha256=smoke.file_hash(receipt_path),
                    full_bundle_members=len(inventory), full_graph_members=len(selected_graph),
                    final_host_sha256=host_digest, final_bun_sha256=bun_digest)
    return app, host_digest, bun_digest


def update_plist(app: Path, change) -> None:
    leaf = app / "Contents/Info.plist"
    value = plistlib.loads(leaf.read_bytes())
    change(value)
    leaf.write_bytes(plistlib.dumps(value, fmt=plistlib.FMT_XML, sort_keys=False))


def mutate(app: Path, kind: str) -> bool:
    """Return whether a semantic mutation needs an outer signature refreshed."""
    root = app / "Contents/Resources/app"
    if kind == "modified-resource":
        with (root / "src/acceptance-resource.txt").open("ab") as file:
            file.write(b"UNSIGNED CHANGE\n")
    elif kind == "missing-entry":
        (root / "src/main.ts").unlink()
    elif kind == "missing-bun":
        (app / "Contents/Helpers/bun").unlink()
    elif kind == "modified-bun":
        with (app / "Contents/Helpers/bun").open("r+b") as file:
            file.seek(4096)
            original = file.read(1)
            smoke.require(len(original) == 1, "runtime mutation offset missing")
            file.seek(4096)
            file.write(bytes([original[0] ^ 1]))
    elif kind == "bun-not-executable":
        (app / "Contents/Helpers/bun").chmod(0o644)
    elif kind == "host-not-executable":
        (app / "Contents/MacOS/keld-host").chmod(0o644)
    elif kind == "missing-local-record":
        update_plist(app, lambda value: value.pop("KELDLocalBundle"))
        return True
    elif kind == "unknown-local-field":
        update_plist(app, lambda value: value["KELDLocalBundle"].update(extra="not-admitted"))
        return True
    elif kind == "wrong-runtime-pin":
        update_plist(app, lambda value: value["KELDLocalBundle"].update(bunSha256="00" * 32))
        return True
    elif kind == "invalid-app-identifier":
        update_plist(app, lambda value: value.update(CFBundleIdentifier="dev..invalid"))
        return True
    elif kind == "strict-rehashed-policy":
        policy = root / "keld.permissions.jsonc"
        policy.write_bytes(b'{"profile":"strict"}\n')
        boot = root / "keld.boot.json"
        value = json.loads(boot.read_bytes())
        value["permissions"]["content_sha256"] = "sha256:" + smoke.file_hash(policy)
        boot.write_text(json.dumps(value, separators=(",", ":")) + "\n")
        return True
    elif kind == "duplicate-local-key":
        leaf = app / "Contents/Info.plist"
        text = leaf.read_text()
        needle = "<key>schema</key><integer>1</integer>"
        smoke.require(text.count(needle) == 1, "unexpected producer XML shape")
        leaf.write_text(text.replace(needle, needle + needle))
        return True
    elif kind not in ("dev-lease-stdin-v1", "dev-lease-invalid", "unauthenticated-guardian"):
        raise RuntimeError("unknown mutation case")
    return False


def refusal(evidence: smoke.Evidence, app: Path, kind: str, hostile_path: Path) -> dict[str, object]:
    host = app / "Contents/MacOS/keld-host"
    app_root = app / "Contents/Resources/app"
    environment = {"HOME": str(Path.home()), "PATH": str(hostile_path) + ":/usr/bin:/bin"}
    arguments = [str(host)]
    pipe_read = pipe_write = None
    before_flags = None
    if kind.startswith("dev-lease-"):
        environment["KELD_DEV_LEASE"] = "stdin-v1" if kind.endswith("stdin-v1") else "not-a-lease"
        pipe_read, pipe_write = os.pipe()
        before_flags = fcntl.fcntl(pipe_read, fcntl.F_GETFL)
    if kind == "unauthenticated-guardian":
        entry = app_root / "src/main.ts"
        info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
        identity = entry.stat()
        arguments += [smoke.ROLE_ARG, str(app_root), "src/main.ts", str(identity.st_dev),
                      str(identity.st_ino), "0", "local-bundle-v1",
                      info["KELDLocalBundle"]["bunSha256"], smoke.file_hash(app_root / "keld.boot.json"),
                      smoke.file_hash(app_root / "keld.permissions.jsonc")]
    try:
        if kind == "host-not-executable":
            try:
                child = subprocess.Popen(arguments, cwd="/", env=environment,
                                         stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                         stderr=subprocess.DEVNULL)
            except PermissionError:
                evidence.record(step=kind, outcome="OS_EXECUTE_PERMISSION_REFUSAL")
                return {"case": kind, "pass": True, "oracle": "PermissionError before execution"}
            child.kill()
            child.wait(timeout=5)
            raise RuntimeError("non-executable Host unexpectedly started")
        status, stdout, stderr = evidence.command(
            kind + "-launch", arguments, cwd=Path("/"), env=environment,
            stdin=pipe_read, timeout=15, expect_success=False)
        smoke.require(status != 0, f"{kind}: invalid bundle returned success")
        smoke.require("KELD-CORE-" in stderr or "KELD-GUARD-" in stderr or "KELD-GUARD008:" in stderr,
                      f"{kind}: no typed refusal: {stderr[-2000:]}")
        if kind == "strict-rehashed-policy":
            smoke.require("KELD-GUARD008:" in stderr
                          and "required strict primitive" in stderr
                          and "[startup-resource-attempts listener=0 child=0 window=0]" in stderr,
                          "Strict must fail at the existing pre-resource Guard boundary")
        smoke.require("ipc-echo ok:" not in stdout + stderr, f"{kind}: application main ran")
        if pipe_read is not None:
            actual_flags = fcntl.fcntl(pipe_read, fcntl.F_GETFL)
            smoke.require(actual_flags == before_flags,
                          "installed dev-lease refusal changed inherited pipe status flags")
            evidence.record(step=kind + "-unchanged-pipe", before=before_flags, after=actual_flags)
        observed = exact_census(app)
        smoke.require(not observed, f"{kind}: owned executable survived rejection: {observed}")
        return {"case": kind, "pass": True, "exit_code": status, "stderr": stderr,
                "owned_processes_after": observed}
    finally:
        for descriptor in (pipe_read, pipe_write):
            if descriptor is not None:
                os.close(descriptor)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--accepted-evidence", required=True)
    parser.add_argument("--evidence", required=True)
    args = parser.parse_args()
    smoke.require(sys.platform == "darwin", "actual native macOS is required")
    accepted = Path(args.accepted_evidence)
    prior = json.loads((accepted / "result.json").read_text())
    output = Path(args.evidence)
    installed_root = Path(prior["installed_root"])
    built = Path(prior["app"])
    smoke.require(output.is_absolute() and not output.exists(),
                  "negative-control evidence must be a new absolute path")
    # A bad output argument must never turn an accepted/installed app into a
    # writable mutation directory; variants are owned only by this new root.
    target = output.resolve(strict=False)
    for protected in (accepted, installed_root, built):
        root = protected.resolve(strict=False)
        smoke.require(not (target == root or target.is_relative_to(root)
                           or root.is_relative_to(target)),
                      "negative-control evidence overlaps an accepted or installed artifact")
    smoke.require(not target.is_relative_to((Path.home() / "Applications").resolve()),
                  "negative-control evidence cannot be inside installed Applications")
    evidence = smoke.Evidence(Path(args.evidence))
    try:
        app, expected_host, expected_bun = verify_installed_source(prior, accepted, evidence)
    except BaseException as error:
        evidence.write_result({"state": "FAIL", "stage": "pristine-installed-source",
                               "reason": f"{type(error).__name__}: {error}"})
        raise
    variants = evidence.root / "owned-variants"
    variants.mkdir(mode=0o700)
    hostile_path = evidence.root / "hostile-path"
    hostile_path.mkdir(mode=0o700)
    marker = evidence.root / "UNAUTHORIZED_PATH_BUN_EXECUTED"
    replacement = hostile_path / "bun"
    replacement.write_text("#!/bin/sh\nprintf 'unexpected\\n' > " + shlex.quote(str(marker)) + "\nexit 90\n")
    replacement.chmod(0o755)
    results: list[dict[str, object]] = []
    try:
        cases = ["modified-resource", "missing-entry", "missing-bun", "modified-bun",
                 "bun-not-executable", "host-not-executable", "missing-local-record",
                 "unknown-local-field", "wrong-runtime-pin", "invalid-app-identifier",
                 "strict-rehashed-policy", "duplicate-local-key", "dev-lease-stdin-v1",
                 "dev-lease-invalid", "unauthenticated-guardian"]
        for kind in cases:
            copied = variants / (kind + ".app")
            evidence.command(kind + "-copy", ["/usr/bin/ditto", str(app), str(copied)])
            owned_inode = copied.stat().st_ino
            re_sign = mutate(copied, kind)
            if re_sign:
                evidence.command(kind + "-sign", ["/usr/bin/codesign", "--sign", "-", "--force",
                                                   "--timestamp=none", str(copied)])
            intact = kind not in ("modified-resource", "missing-entry", "missing-bun", "modified-bun")
            status, _, _ = evidence.command(kind + "-signature",
                ["/usr/bin/codesign", "--verify", "--deep", "--strict", str(copied)],
                expect_success=False)
            smoke.require((status == 0) == intact, f"{kind}: signature control did not establish intended state")
            row = refusal(evidence, copied, kind, hostile_path)
            smoke.require(not marker.exists(), f"{kind}: inherited PATH selected unauthorized Bun")
            results.append({**row, "signature_valid": intact, "outer_resigned": re_sign})
            evidence.record(step="negative-control-passed", **results[-1])
            smoke.require(copied.lstat().st_ino == owned_inode and not copied.is_symlink()
                          and copied.parent == variants, "owned disposable copy identity changed")
            shutil.rmtree(copied)
        verified_app, verified_host, verified_bun = verify_installed_source(prior, accepted, evidence)
        smoke.require((verified_app, verified_host, verified_bun)
                      == (app, expected_host, expected_bun),
                      "selected installed app provenance changed during negative controls")
        result = {"state": "PASS", "scope": "actual installed Host code, signature, identity, profile, mode and private-boot refusal",
                  "source_app": str(app), "source_host_sha256": expected_host,
                  "controls": results, "unrun": "authenticated denied capability, Bun/Host crash matrix, original draw.io and second Mac"}
        evidence.write_result(result)
        print(json.dumps({"state": "PASS", "controls": len(results), "evidence": str(evidence.root)}))
        return 0
    except BaseException as error:
        evidence.write_result({"state": "FAIL", "controls_completed": results,
                               "reason": f"{type(error).__name__}: {error}",
                               "remaining_census": exact_census(variants)})
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

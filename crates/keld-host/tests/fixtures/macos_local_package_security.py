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
    smoke.require(prior.get("state") == "PASS", "a separately passing installed smoke result is required")
    app = Path(prior["app"])
    smoke.require(app.is_dir() and not app.is_symlink(), "accepted app unavailable")
    receipt = json.loads(Path(prior["receipt"]).read_text())
    expected_host = receipt["finalHostSha256"]
    expected_bun = receipt["inputBun"]["file"]["sha256"]
    smoke.require(smoke.file_hash(app / "Contents/MacOS/keld-host") == expected_host,
                  "accepted Host bytes changed before negative controls")
    smoke.require(smoke.file_hash(app / "Contents/Helpers/bun") == expected_bun,
                  "accepted Bun bytes changed before negative controls")
    evidence = smoke.Evidence(Path(args.evidence))
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
        smoke.require(smoke.file_hash(app / "Contents/MacOS/keld-host") == expected_host,
                      "original Host changed during negative controls")
        smoke.require(smoke.file_hash(app / "Contents/Helpers/bun") == expected_bun,
                      "original Bun changed during negative controls")
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

#!/usr/bin/env python3
r"""GH-1404: installed launch facts through the real shipping Host and bundled Bun.

Run on an interactive arm64 Mac with already qualified binaries. This driver
never builds Rust. It reuses the stock installed acceptance driver's native
process/window/copy/cleanup owners. The only new observer is an atomic Ready
witness written by the fixture after its real app.whenReady().

Example (all paths absolute; evidence and install-root must be new):
  python3 crates/keld-host/tests/fixtures/macos_local_package_launch_facts.py \
    --cli /qualified/keld --cli-sha256 HEX64 \
    --host /qualified/keld-host --host-sha256 HEX64 --host-version LABEL \
    --bun /qualified/bun --bun-sha256 HEX64 --bun-version LABEL \
    --evidence /existing-parent/NEW-facts-evidence \
    --install-root /Users/USER/Applications/NEW-facts-install

Optional --census reuses an already built native_window_census executable.
Otherwise the driver compiles that existing Swift fixture. --source-root defaults
to this repository and supplies only the three current production SDK packages.
No package manager, dependency symlink, fake facade or hand-made transport stamp
is used. Success requires install, causal Ready, native window, targeted native
Quit, exact family retirement, relocation/repeat, and six re-signed generated
bootstrap removal/tamper refusals before the first dependency can run.
"""

from __future__ import annotations

import argparse
from dataclasses import asdict
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import secrets
import shutil
import stat
import sys
import time

import macos_local_package_acceptance as smoke
import macos_local_package_security as security


PACKAGES = ("api", "kipc", "electron")
BOOTSTRAP = (
    "keld-launch-facts/kipc/src/launch-facts.ts",
    "keld-launch-facts/electron/src/process-facts.ts",
    "keld-launch-facts/electron/src/installed-launch-preload.ts",
)
FIRST = "GH1404_NATIVE_FACTS_FIRST_DEPENDENCY "
BEFORE_READY = "GH1404_NATIVE_FACTS_BEFORE_READY "
READY = "GH1404_NATIVE_FACTS_READY "
BROKER = "GH1404_NATIVE_BROKER_CONTROLS "
WITNESS_LIMIT = 64 * 1024
LOG_LIMIT = 2 * 1024 * 1024
DECOYS = {
    "KELD_APP_ROOT": "/not-the-installed-application",
    "KELD_RESOURCES_PATH": "/not-the-installed-resources",
    "KELD_IS_PACKAGED": "false",
    "KELD_HOST_LAUNCH_FACTS": '{"schema":6,"name":"FORGED"}',
}


def copy_production_tree(source: Path, destination: Path) -> None:
    """Copy the selected package's src tree; never dereference dependency links."""
    smoke.require(source.is_dir() and not source.is_symlink(), f"real source directory required: {source}")
    destination.mkdir(mode=0o755)
    for entry in sorted(source.iterdir()):
        if entry.name in {"node_modules", "test", "tests", "__tests__", "fixtures", "bench", "__pycache__"}:
            continue
        if any(marker in entry.name for marker in (".test.", ".test-", ".spec.", ".typecheck.")):
            continue
        mode = entry.lstat().st_mode
        smoke.require(not stat.S_ISLNK(mode), f"production source symlink refused: {entry}")
        target = destination / entry.name
        if stat.S_ISDIR(mode):
            copy_production_tree(entry, target)
        else:
            smoke.require(stat.S_ISREG(mode), f"non-file production source refused: {entry}")
            shutil.copyfile(entry, target)
            target.chmod(0o644)


def stage_fixture(args: argparse.Namespace, evidence: smoke.Evidence, witness: Path,
                  nonce: str) -> tuple[Path, Path]:
    project, output = smoke.stage_hello(args, evidence)
    allowed = evidence.root / "broker-allowed"
    denied = evidence.root / "broker-denied"
    allowed.mkdir(mode=0o700)
    denied.mkdir(mode=0o700)
    (allowed / "sentinel.txt").write_bytes(b"GH1404_ALLOWED_SENTINEL\n")
    (denied / "sentinel.txt").write_bytes(b"GH1404_DENIED_SENTINEL\n")
    scope = str(allowed) + "/**"
    (project / "keld.permissions.jsonc").write_text(json.dumps({
        "profile": "legacy", "app": {"fs": {"read": [scope], "write": [scope]}}
    }) + "\n", encoding="utf-8")
    release_transport = (project / "src/kipc-transport.ts").read_bytes()
    smoke.require(release_transport.startswith(b"// @keld/kipc-transport sha256:"),
                  "keld create did not emit its stamped transport")
    package_root = project / "node_modules/@keld"
    package_root.mkdir(parents=True)
    sdk_files: dict[str, str] = {}
    for package in PACKAGES:
        source = args.source_root / "packages/@keld" / package
        destination = package_root / package
        destination.mkdir(mode=0o755)
        manifest = source / "package.json"
        smoke.require(manifest.is_file() and not manifest.is_symlink(), f"package manifest missing: {manifest}")
        smoke.require(json.loads(manifest.read_text())["name"] == f"@keld/{package}",
                      "SDK package identity differs from selected owner")
        shutil.copyfile(manifest, destination / "package.json")
        copy_production_tree(source / "src", destination / "src")
    # The producer owns the release stamp and release constant. Preserve its bytes.
    (package_root / "kipc/src/transport.ts").write_bytes(release_transport)
    for path in sorted(package_root.rglob("*")):
        smoke.require(not path.is_symlink(), f"staged SDK contains a symlink: {path}")
        if path.is_file():
            sdk_files[str(path.relative_to(project))] = smoke.file_hash(path)
    fixture = Path(__file__).with_name("macos-installed-launch-facts")
    for name in ("main.ts", "early.ts", "ready-witness.ts"):
        shutil.copyfile(fixture / name, project / "src" / name)
    shutil.copyfile(fixture / "transport-preload-observer.ts", project / "src/kipc-transport.ts")
    (project / "src/witness-config.ts").write_text(
        "export const readyWitnessConfig = " + json.dumps({"directory": str(witness), "nonce": nonce,
            "allowedRoot": str(allowed), "deniedFile": str(denied / "sentinel.txt")})
        + " as const;\n", encoding="utf-8")
    build = project / "keld.build.ts"
    source = build.read_text(encoding="utf-8")
    needle = 'resources: ["src"]'
    smoke.require(source.count(needle) == 1, "stock build resource declaration changed")
    resources = ["src", *(f"node_modules/@keld/{name}" for name in PACKAGES)]
    build.write_text(source.replace(needle, "resources: " + json.dumps(resources)), encoding="utf-8")
    evidence.record(step="installed-facts-fixture-staged", source_root=str(args.source_root),
                    sdk_sha256=sdk_files, release_transport_sha256=hashlib.sha256(release_transport).hexdigest(),
                    observer_sha256=smoke.file_hash(project / "src/kipc-transport.ts"),
                    policy_sha256=smoke.file_hash(project / "keld.permissions.jsonc"),
                    granted_scope=scope, denied_file=str(denied / "sentinel.txt"),
                    witness_directory=str(witness), witness_nonce=nonce,
                    fixture_sha256={name: smoke.file_hash(project / "src" / name)
                                    for name in ("main.ts", "early.ts", "ready-witness.ts", "witness-config.ts")})
    return project, output


def witness_snapshot(directory: Path) -> dict[str, tuple[int, int, int, int]]:
    entries = list(directory.iterdir())
    smoke.require(len(entries) <= 32, "unexpectedly large Ready witness directory")
    result = {}
    for path in entries:
        metadata = path.lstat()
        smoke.require(stat.S_ISREG(metadata.st_mode), f"non-file Ready witness: {path}")
        result[path.name] = (metadata.st_dev, metadata.st_ino, metadata.st_mtime_ns, metadata.st_size)
    return result


def wait_ready(evidence: smoke.Evidence, directory: Path, nonce: str,
               bun: smoke.ProcessFact, before: dict[str, tuple[int, int, int, int]]) -> dict[str, object]:
    path = directory / f"{bun.pid}.json"
    deadline = time.monotonic() + smoke.PROCESS_TIMEOUT
    while time.monotonic() < deadline:
        smoke.require(smoke.current_exact_role(bun), "Bun retired before its actual Ready witness")
        snapshot = witness_snapshot(directory)
        identity = snapshot.get(path.name)
        if identity is not None and identity != before.get(path.name):
            smoke.require(identity[3] <= WITNESS_LIMIT, "Ready witness exceeds 64 KiB")
            value = json.loads(path.read_text(encoding="utf-8"))
            smoke.require(witness_snapshot(directory).get(path.name) == identity,
                          "Ready witness changed while read")
            smoke.require(set(value) == {"nonce", "pid", "ppid", "ready", "facts"},
                          "Ready witness has an unexpected record shape")
            smoke.require(value["nonce"] == nonce and value["pid"] == bun.pid
                          and value["ppid"] == bun.ppid and value["ready"] is True,
                          "Ready witness does not identify this exact live Bun generation")
            evidence.record(step="causal-ready-witness", path=str(path), sha256=smoke.file_hash(path),
                            bun=asdict(bun), witness=value)
            return value
        time.sleep(0.05)  # Poll a fresh atomic record and exact live PID; time never passes the test.
    raise RuntimeError("actual app.whenReady() witness deadline exceeded")


def verify_facts(app: Path, roles: list[smoke.ProcessFact], witness: dict[str, object]) -> None:
    app = app.resolve(strict=True)
    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    app_root = str(app / "Contents/Resources/app")
    resources = str(app / "Contents/Resources")
    bun_path = str(app / "Contents/Helpers/bun")
    entry = app_root + "/src/main.ts"
    bun = next(role for role in roles if role.executable == bun_path)
    facts = witness["facts"]
    smoke.require(isinstance(facts, dict), "Ready witness has no facts object")
    early = facts.get("early")
    expected_early = {
        "pid": bun.pid, "ppid": bun.ppid, "type": "browser", "defaultApp": True,
        "resourcesPath": resources,
        "electronVersion": json.loads((Path(app_root) / "node_modules/@keld/electron/package.json").read_text())["version"],
        "execPath": bun_path, "argv": [bun_path, entry],
        "defaultDescriptor": {"value": True, "writable": False, "enumerable": True, "configurable": False},
        "resourcesDescriptor": {"value": resources, "writable": False, "enumerable": True, "configurable": False},
    }
    smoke.require(early == expected_early, f"first dependency facts differ from native identity/layout: {early}")
    smoke.require(facts.get("entry") == entry and facts.get("expected") == {
        "appRoot": app_root, "resourcesRoot": resources, "bundledBun": bun_path},
        "fixture entry expectation differs from independently copied bundle")
    smoke.require(facts.get("launchFacts") == {
        "schema": 6, "kind": "local-development", "name": info["CFBundleName"],
        "version": info["CFBundleShortVersionString"], "root": app_root, "resourcesRoot": resources},
        "closed launch record differs from the independently read sealed metadata")
    smoke.require(facts.get("app") == {"isPackaged": True, "appPath": app_root,
                  "name": info["CFBundleName"], "version": info["CFBundleShortVersionString"]},
                  "public application getters differ from sealed metadata")
    smoke.require(facts.get("launchFactsFrozen") is True
                  and facts.get("launchFactsPrototypeIsNull") is True
                  and facts.get("sameArgvObject") is True and facts.get("userArguments") == [],
                  "launch record immutability or unchanged Bun argv was not proved")


def bounded_logs(paths: tuple[Path, Path]) -> str:
    pieces = []
    for path in paths:
        smoke.require(path.stat().st_size <= LOG_LIMIT, f"application evidence log exceeds bound: {path}")
        pieces.append(path.read_text(encoding="utf-8", errors="replace"))
    return "\n".join(pieces)


def one_record(output: str, prefix: str) -> object:
    rows = [line[len(prefix):] for line in output.splitlines() if line.startswith(prefix)]
    smoke.require(len(rows) == 1, f"expected exactly one {prefix.strip()} record")
    return json.loads(rows[0])


def launch_facts(evidence: smoke.Evidence, app: Path, census: Path, witness_dir: Path,
                 nonce: str, phase: str) -> dict[str, object]:
    host, bun = app / "Contents/MacOS/keld-host", app / "Contents/Helpers/bun"
    logs = tuple(evidence.root / "logs" / f"{phase}-app.{stream}" for stream in ("stdout", "stderr"))
    for path in logs:
        with path.open("xb"):
            pass
    before = witness_snapshot(witness_dir)
    allowed = evidence.root / "broker-allowed/sentinel.txt"
    denied = evidence.root / "broker-denied/sentinel.txt"
    smoke.require(denied.read_bytes() == b"GH1404_DENIED_SENTINEL\n", "outside sentinel changed before launch")
    allowed.write_bytes(b"GH1404_ALLOWED_SENTINEL\n")
    observer = smoke.WindowObserver(census)
    try:
        env_args = [part for key, value in DECOYS.items() for part in ("--env", f"{key}={value}")]
        evidence.command(phase + "-launchservices", ["/usr/bin/open", "-n", "--stdout", str(logs[0]),
                         "--stderr", str(logs[1]), *env_args, str(app)], timeout=20)
        roles = smoke.live_roles(host, bun, evidence, phase)
        host_fact = next(role for role in roles if role.executable == str(host) and not role.guardian)
        bun_fact = next(role for role in roles if role.executable == str(bun))
        windows = observer.witness(host_fact.pid)
        witness = wait_ready(evidence, witness_dir, nonce, bun_fact, before)
        verify_facts(app, roles, witness)
        smoke.require(allowed.read_bytes() == b"GH1404_ALLOWED_WRITE\n", "authorized broker write not observed on disk")
        smoke.require(denied.read_bytes() == b"GH1404_DENIED_SENTINEL\n", "out-of-scope write changed the OS sentinel")
        smoke.require(all(smoke.current_exact_role(role) for role in roles), "observed process family changed before Quit")
        evidence.record(step=phase + "-facts-and-window", app=str(app), window_ids=windows,
                        roles=[asdict(role) for role in roles], decoys=DECOYS)
        native_reply = smoke.request_native_quit(evidence, app, host_fact, phase)
        smoke.wait_roles_gone(host, bun, roles, evidence, phase)
        output = bounded_logs(logs)
        smoke.require(one_record(output, FIRST) == witness["facts"]["early"], "final early log differs from live witness")
        smoke.require(one_record(output, BEFORE_READY) == witness["facts"], "final fact log differs from live witness")
        ready = one_record(output, READY)
        smoke.require(ready == {"pid": bun_fact.pid, "ppid": bun_fact.ppid, "ready": True,
                      "appPath": str(app / "Contents/Resources/app"),
                      "resourcesPath": str(app / "Contents/Resources"), "execPath": str(bun),
                      "argv": [str(bun), str(app / "Contents/Resources/app/src/main.ts")]},
                      "retained Ready output differs from the exact live generation")
        smoke.require("KELD-IPC-022" not in output, "normal Quit unexpectedly closed the authenticated link")
        smoke.require(output.splitlines().count("GH1404_QUIT_REPLY_CONFIRMED") == 1,
                      "a genuine awaited Quit Reply was not independently observed")
        smoke.require(one_record(output, BROKER) == {"read": "KELD-GUARD002", "write": "KELD-GUARD002"},
                      "actual broker refusals did not have the required scope-denial codes")
        smoke.require(denied.read_bytes() == b"GH1404_DENIED_SENTINEL\n", "outside sentinel changed during shutdown")
        evidence.command(phase + "-retained-signature", ["/usr/bin/codesign", "--verify", "--deep", "--strict", str(app)])
        return {"phase": phase, "app": str(app), "window_ids": windows,
                "roles": [asdict(role) for role in roles], "witness": witness,
                "application_quit_reply": True, "unsignaled_retirement": True,
                "broker_allowed_and_denied_verified": True, "native_quit": native_reply}
    finally:
        observer.close()


def bootstrap_controls(evidence: smoke.Evidence, app: Path, install_root: Path,
                       witness_dir: Path, rows: list[dict[str, object]]) -> None:
    variants = install_root / "bootstrap-refusal-controls"
    variants.mkdir(mode=0o700)
    empty_path = evidence.root / "empty-path"
    empty_path.mkdir(mode=0o700)
    for index, relative in enumerate(BOOTSTRAP):
        for mutation in ("remove", "tamper"):
            name = f"bootstrap-{index}-{mutation}"
            copied = variants / f"{name}.app"
            evidence.command(name + "-copy", ["/usr/bin/ditto", str(app), str(copied)], timeout=90)
            leaf = copied / "Contents/Resources/app" / relative
            smoke.require(leaf.is_file() and not leaf.is_symlink(), f"generated owner missing before mutation: {relative}")
            original_hash = smoke.file_hash(leaf)
            changed_hash = None
            if mutation == "remove":
                leaf.unlink()
            else:
                content = leaf.read_bytes()
                end = content.find(b"*/", 3)
                smoke.require(content.startswith(b"/**") and end > 3,
                              "generated owner lacks its expected leading comment")
                offset = next((index for index in range(3, end)
                               if 65 <= content[index] <= 90 or 97 <= content[index] <= 122), None)
                smoke.require(offset is not None, "generated comment has no byte for same-size mutation")
                changed = content[:offset] + bytes([content[offset] ^ 0x20]) + content[offset + 1:]
                smoke.require(len(changed) == len(content) and changed != content,
                              "tamper control must change one comment byte without changing size")
                leaf.write_bytes(changed)
                changed_hash = smoke.file_hash(leaf)
            evidence.command(name + "-resign", ["/usr/bin/codesign", "--sign", "-", "--force",
                             "--timestamp=none", "--identifier", smoke.APP_ID, str(copied)], timeout=60)
            evidence.command(name + "-valid-outer-signature", ["/usr/bin/codesign", "--verify",
                             "--deep", "--strict", "--verbose=2", str(copied)], timeout=60)
            before = witness_snapshot(witness_dir)
            refusal = security.refusal(evidence, copied, name, empty_path)
            stem = f"{evidence.count:03d}-{name}-launch"
            output = bounded_logs((evidence.root / "logs" / (stem + ".stdout"),
                                   evidence.root / "logs" / (stem + ".stderr")))
            smoke.require("installed launch bootstrap" in output,
                          f"{name}: refusal did not reach the exact generated-owner check")
            smoke.require(FIRST not in output and BEFORE_READY not in output and READY not in output,
                          f"{name}: application dependency or Ready effect occurred")
            smoke.require(witness_snapshot(witness_dir) == before, f"{name}: a new Ready witness appeared")
            row = {**refusal, "generated_path": relative, "mutation": mutation,
                   "original_sha256": original_hash, "changed_sha256": changed_hash,
                   "outer_signature_valid": True, "same_size_comment_tamper": mutation == "tamper",
                   "first_dependency_absent": True, "new_ready_witness_absent": True}
            rows.append(row)
            evidence.record(step="generated-bootstrap-refusal", **row)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for option in ("cli", "cli-sha256", "host", "host-sha256", "host-version", "bun", "bun-sha256",
                   "bun-version", "evidence", "install-root"):
        parser.add_argument("--" + option, required=True)
    parser.add_argument("--source-root", default=str(Path(__file__).resolve().parents[4]))
    parser.add_argument("--census")
    args = parser.parse_args()
    smoke.require(sys.platform == "darwin" and platform.machine() == "arm64", "native arm64 macOS required")
    for name in ("cli", "host", "bun"):
        setattr(args, name, smoke.regular_input(getattr(args, name), name, getattr(args, name + "_sha256")))
    for name in ("host_version", "bun_version"):
        value = getattr(args, name)
        smoke.require(0 < len(value.encode()) <= 128 and not any(ord(character) < 32 for character in value),
                      f"invalid declared qualification label: {name}")
    args.source_root = Path(args.source_root).resolve(strict=True)
    evidence_path, install_root = Path(args.evidence).expanduser(), Path(args.install_root).expanduser()
    applications = Path.home() / "Applications"
    smoke.require(applications.is_dir() and not applications.is_symlink(), "real ~/Applications required")
    smoke.require(evidence_path.is_absolute() and install_root.is_absolute(), "absolute output roots required")
    smoke.require(install_root.parent == applications and install_root.name not in ("", ".", ".."),
                  "install-root must be a new immediate child of ~/Applications")
    smoke.require(not evidence_path.exists() and not install_root.exists(), "both output roots must be NEW")
    smoke.require(evidence_path != install_root and evidence_path not in install_root.parents
                  and install_root not in evidence_path.parents, "output roots must not overlap")
    smoke.APP_ID += ".facts" + hashlib.sha256(str(evidence_path).encode()).hexdigest()[:12]
    evidence = smoke.Evidence(evidence_path)
    witness_dir = evidence.root / "ready-witnesses"
    witness_dir.mkdir(mode=0o700)
    nonce = secrets.token_hex(32)
    summary: dict[str, object] = {"state": "INCOMPLETE", "stage": "preflight", "evidence": str(evidence.root),
                                  "install_root": str(install_root), "launches": [], "controls": []}
    try:
        evidence.record(step="qualified-launch-facts-inputs", source_root=str(args.source_root),
                        inputs={name: {"path": str(getattr(args, name)), "sha256": getattr(args, name + "_sha256")}
                                for name in ("cli", "host", "bun")}, application_id=smoke.APP_ID)
        _, out, err = evidence.command("open-help", ["/usr/bin/open", "-h"], expect_success=False)
        smoke.require(all(option in out + err for option in ("--stdout", "--stderr", "--env", "[-n]")),
                      "native open lacks required isolated launch/log/environment options")
        project, output = stage_fixture(args, evidence, witness_dir, nonce)
        summary["stage"] = "build"
        evidence.write_result(summary)
        evidence.command("build-launch-facts", [str(args.cli), "build", "--target", "macos-arm64"], cwd=project, timeout=180)
        app, dmg = smoke.inspect_package(evidence, output, project, args)
        installed = smoke.copy_from_readonly_image(evidence, dmg, app, install_root)
        moved_source = project.with_name(project.name + "-renamed-before-launch")
        os.rename(project, moved_source)
        smoke.require(not project.exists(), "original project path remains")
        evidence.record(step="facts-source-renamed", old=str(project), new=str(moved_source))
        if args.census:
            census = smoke.regular_input(args.census, "native census", None)
        else:
            tools = evidence.root / "native-tools"
            tools.mkdir()
            source = Path(__file__).with_name("native_window_census.swift")
            shutil.copyfile(source, tools / source.name)
            census = tools / "native-window-census"
            evidence.command("compile-existing-census", ["/usr/bin/xcrun", "swiftc", "-O",
                             str(tools / source.name), "-o", str(census)], timeout=120)
        evidence.record(step="native-census-input", path=str(census), sha256=smoke.file_hash(census))
        summary["stage"] = "installed-launch"
        evidence.write_result(summary)
        summary["launches"].append(launch_facts(evidence, installed, census, witness_dir, nonce, "installed"))
        relocated_parent = install_root / "relocated"
        relocated_parent.mkdir()
        relocated = relocated_parent / "Renamed-Installed-Facts.app"
        os.rename(installed, relocated)
        evidence.command("codesign-relocated", ["/usr/bin/codesign", "--verify", "--deep", "--strict", str(relocated)])
        summary["stage"] = "relocated-launch"
        evidence.write_result(summary)
        summary["launches"].append(launch_facts(evidence, relocated, census, witness_dir, nonce, "relocated"))
        summary["stage"] = "resigned-bootstrap-controls"
        evidence.write_result(summary)
        bootstrap_controls(evidence, app, install_root, witness_dir, summary["controls"])
        native_accepted = all(row["native_quit"]["state"] == "ACCEPTED" for row in summary["launches"])
        summary.update(state="PASS" if native_accepted else "PARTIAL", stage="complete", app=str(app), dmg=str(dmg), installed_app=str(relocated),
                       receipt=str(output / "artifact-receipt.json"),
                       installed_lifecycle_verified=True, native_quit_accepted=native_accepted,
                       scope="actual installed launch facts, early preload order, broker allowed/denied effects, application Quit, relocation and six re-signed bootstrap refusals",
                       limitations=[] if native_accepted else ["AppleEvent Quit is canceled although application-controlled Quit completed (GH-451)"],
                       unrun="original draw.io graph/workflow, publisher identity and public distribution")
        evidence.write_result(summary)
        print(json.dumps(summary, indent=2))
        return 0 if native_accepted else 2
    except BaseException as error:
        summary.update(state="FAIL", reason=f"{type(error).__name__}: {error}")
        # Preserve the real failing result before any emergency application cleanup.
        evidence.write_result(summary)
        try:
            remaining = security.exact_census(install_root)
        except Exception as census_error:
            remaining = {"census_error": str(census_error)}
        evidence.record(step="launch-facts-failed-before-cleanup", stage=summary["stage"], reason=str(error),
                        remaining_census=remaining)
        try:
            smoke.cleanup_verified_roles(evidence, install_root)
        except Exception as cleanup_error:
            evidence.record(step="launch-facts-cleanup-blocked", error=str(cleanup_error))
        print(json.dumps(summary, indent=2), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

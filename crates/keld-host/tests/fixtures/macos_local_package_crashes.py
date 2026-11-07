#!/usr/bin/env python3
"""Installed stock-hello crash controls for GH-1404 acceptance A13.

Use an independently qualified signed app and the native window observer. The
driver copies the app to a NEW child of ~/Applications. It signals only an exact
PID/start/executable identity observed in that copy's Host -> Guardian -> Bun
chain. Product cleanup is observed before any emergency harness cleanup.

The test app publishes a per-PID witness only after authenticated Host Ready and
echo Reply. This proves Bun recovery and Host-death cleanup for the ordinary owned
family. Native Quit, detached descendants and simultaneous Host/Guardian death
remain separate acceptance cells. No previous PASS marker is trusted.
"""

from __future__ import annotations

import argparse
from dataclasses import asdict
import json
import os
from pathlib import Path
import signal
import stat
import sys
import time

import macos_local_package_acceptance as smoke


def inventory(root: Path) -> list[dict[str, object]]:
    result = []
    for path in [root, *sorted(root.rglob("*"))]:
        metadata = path.lstat()
        row: dict[str, object] = {
            "path": path.relative_to(root).as_posix(),
            "mode": stat.S_IMODE(metadata.st_mode),
        }
        if stat.S_ISREG(metadata.st_mode):
            row.update(kind="file", size=metadata.st_size, sha256=smoke.file_hash(path))
        elif stat.S_ISDIR(metadata.st_mode):
            row.update(kind="directory")
        elif stat.S_ISLNK(metadata.st_mode):
            row.update(kind="symlink", target=os.readlink(path))
        else:
            raise RuntimeError(f"unexpected bundle member: {path}")
        result.append(row)
    return result


def wait_ready(evidence: smoke.Evidence, fact: smoke.ProcessFact, directory: Path,
               nonce: str, phase: str) -> None:
    witness = directory / f"{fact.pid}.json"
    deadline = time.monotonic() + smoke.PROCESS_TIMEOUT
    while time.monotonic() < deadline:
        smoke.require(smoke.current_exact_role(fact), f"{phase}: Bun exited before authenticated Ready witness")
        if witness.exists():
            metadata = witness.lstat()
            smoke.require(stat.S_ISREG(metadata.st_mode) and metadata.st_size <= 1024,
                          f"{phase}: invalid Ready witness file")
            actual = json.loads(witness.read_bytes())
            expected = {"nonce": nonce, "pid": fact.pid, "ready": True,
                        "echo": {"message": "keld", "count": 1}}
            smoke.require(actual == expected, f"{phase}: stale or invalid Ready witness")
            evidence.record(step=f"{phase}-authenticated-ready-echo", fact=asdict(fact), witness=actual)
            return
        time.sleep(0.05)
    raise RuntimeError(f"{phase}: no causal Host Ready and echo witness; crash was not injected")


def launch(evidence: smoke.Evidence, app: Path, census: Path, phase: str,
           ready_directory: Path, nonce: str
           ) -> tuple[list[smoke.ProcessFact], tuple[Path, Path]]:
    host = app / "Contents/MacOS/keld-host"
    bun = app / "Contents/Helpers/bun"
    logs = tuple(evidence.root / "logs" / f"{phase}-app.{stream}"
                 for stream in ("stdout", "stderr"))
    for path in logs:
        with path.open("xb"):
            pass
    observer = smoke.WindowObserver(census)
    try:
        evidence.command(f"{phase}-launchservices", [
            "/usr/bin/open", "-n", "--stdout", str(logs[0]), "--stderr", str(logs[1]), str(app),
        ], timeout=20)
        roles = smoke.live_roles(host, bun, evidence, phase)
        host_fact = next(f for f in roles if f.executable == str(host) and not f.guardian)
        windows = observer.witness(host_fact.pid)
        evidence.record(step=f"{phase}-native-window", host=asdict(host_fact), window_ids=windows)
        bun_fact = next(f for f in roles if f.executable == str(bun))
        wait_ready(evidence, bun_fact, ready_directory, nonce, phase)
        return roles, logs
    finally:
        observer.close()


def crash_exact(evidence: smoke.Evidence, fact: smoke.ProcessFact,
                install_root: Path, phase: str) -> None:
    smoke.require(Path(fact.executable).is_relative_to(install_root),
                  "fault target is outside this driver's owned installed copy")
    smoke.require(smoke.current_exact_role(fact), "fault target PID/start/executable changed")
    evidence.record(step=f"{phase}-fault-request", target=asdict(fact), signal="SIGKILL")
    os.kill(fact.pid, signal.SIGKILL)


def wait_successor(evidence: smoke.Evidence, app: Path,
                   before: list[smoke.ProcessFact], ready_directory: Path,
                   nonce: str) -> list[smoke.ProcessFact]:
    host, bun = app / "Contents/MacOS/keld-host", app / "Contents/Helpers/bun"
    original_bun = next(f for f in before if f.executable == str(bun))
    retained = {(f.pid, f.start, f.executable) for f in before if f.executable == str(host)}
    deadline = time.monotonic() + smoke.PROCESS_TIMEOUT
    observed = []
    while time.monotonic() < deadline:
        observed = smoke.installed_processes(host, bun)
        actual_retained = {(f.pid, f.start, f.executable) for f in observed if f.executable == str(host)}
        successors = [f for f in observed if f.executable == str(bun)
                      and (f.pid, f.start) != (original_bun.pid, original_bun.start)]
        guardians = [f for f in observed if f.guardian]
        if (actual_retained == retained and len(observed) == 3 and len(successors) == 1
                and len(guardians) == 1 and successors[0].ppid == guardians[0].pid
                and not smoke.current_exact_role(original_bun)):
            evidence.owned_roles.extend(observed)
            evidence.record(step="bun-crash-successor", before=[asdict(f) for f in before],
                            after=[asdict(f) for f in observed])
            wait_ready(evidence, successors[0], ready_directory, nonce, "successor")
            return observed
        time.sleep(0.05)  # Poll independent OS identity; elapsed time never passes.
    evidence.record(step="bun-crash-no-successor", observed=[asdict(f) for f in observed])
    raise RuntimeError("Bun crash did not produce one successor under the same Host and Guardian")


def unchanged(evidence: smoke.Evidence, app: Path, baseline: list[dict[str, object]],
              document: Path, document_hash: str, phase: str) -> None:
    smoke.require(inventory(app) == baseline, f"{phase}: installed bundle changed during crash cleanup")
    smoke.require(document.is_file() and smoke.file_hash(document) == document_hash,
                  f"{phase}: adjacent document sentinel was removed or changed")
    evidence.command(f"{phase}-codesign", [
        "/usr/bin/codesign", "--verify", "--deep", "--strict", "--verbose=2", str(app),
    ], timeout=30)
    evidence.record(step=f"{phase}-retained-files", bundle_members=len(baseline),
                    adjacent_document_sha256=document_hash)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("app", "host-sha256", "bun-sha256", "census", "evidence", "install-root",
                 "ready-directory", "nonce"):
        parser.add_argument("--" + name, required=True)
    args = parser.parse_args()
    smoke.require(sys.platform == "darwin", "native macOS is required")
    source = Path(args.app).expanduser().resolve(strict=True)
    smoke.require(source.is_dir() and source.suffix == ".app", "qualified .app directory required")
    smoke.regular_input(str(source / "Contents/MacOS/keld-host"), "signed Host", args.host_sha256)
    smoke.regular_input(str(source / "Contents/Helpers/bun"), "signed Bun", args.bun_sha256)
    census = smoke.regular_input(args.census, "native window observer", None)
    ready_directory = Path(args.ready_directory).expanduser()
    smoke.require(ready_directory.is_absolute() and ready_directory.is_dir()
                  and not ready_directory.is_symlink() and not any(ready_directory.iterdir()),
                  "new empty real Ready witness directory required")
    smoke.require(smoke.HEX_256.fullmatch(args.nonce) is not None, "canonical unique witness nonce required")
    install_root = Path(args.install_root).expanduser()
    evidence_root = Path(args.evidence).expanduser()
    applications = Path.home() / "Applications"
    smoke.require(install_root.is_absolute() and install_root.parent == applications,
                  "install root must be a new immediate child of ~/Applications")
    smoke.require(applications.is_dir() and not applications.is_symlink(), "real Applications directory required")
    smoke.require(not install_root.exists() and not evidence_root.exists(), "new owned roots required")
    smoke.require(evidence_root.is_absolute() and evidence_root != install_root
                  and evidence_root not in install_root.parents and install_root not in evidence_root.parents,
                  "evidence and install roots must be absolute and disjoint")
    evidence = smoke.Evidence(evidence_root)
    summary: dict[str, object] = {"state": "INCOMPLETE", "stage": "preflight", "source": str(source),
                                  "scope": "installed ordinary-family authenticated Bun recovery and Host-death cleanup"}
    try:
        smoke.new_dir(install_root)
        app = install_root / "Crash-Qualified-Hello.app"
        evidence.command("copy-qualified-bundle", ["/usr/bin/ditto", str(source), str(app)], timeout=60)
        baseline = inventory(source)
        smoke.require(inventory(app) == baseline, "independent installed copy differs from qualified source")
        (evidence.root / "bundle-before.json").write_text(json.dumps(baseline, indent=2) + "\n")
        document = install_root / "retained-user-note.drawio"
        document.write_text('<mxfile><diagram name="retention-sentinel"/></mxfile>\n')
        document_hash = smoke.file_hash(document)
        unchanged(evidence, app, baseline, document, document_hash, "initial")
        summary.update(stage="bun-crash", installed_app=str(app), host_sha256=args.host_sha256,
                       bun_sha256=args.bun_sha256, evidence=str(evidence.root))
        evidence.write_result(summary)
        roles, logs = launch(evidence, app, census, "first", ready_directory, args.nonce)
        bun_fact = next(f for f in roles if f.executable.endswith("/Helpers/bun"))
        crash_exact(evidence, bun_fact, install_root, "bun")
        recovered = wait_successor(evidence, app, roles, ready_directory, args.nonce)
        host_fact = next(f for f in recovered if not f.guardian and f.executable.endswith("/MacOS/keld-host"))
        observer = smoke.WindowObserver(census)
        try:
            evidence.record(step="recovered-native-window", host=asdict(host_fact),
                            window_ids=observer.witness(host_fact.pid))
        finally:
            observer.close()
        summary["stage"] = "host-crash-after-recovery"
        evidence.write_result(summary)
        crash_exact(evidence, host_fact, install_root, "host-after-recovery")
        smoke.wait_roles_gone(app / "Contents/MacOS/keld-host", app / "Contents/Helpers/bun",
                              recovered, evidence, "host-crash")
        unchanged(evidence, app, baseline, document, document_hash, "after-host-crash")
        # Logs are diagnostic. The successor's separate causal witness above
        # proves Host Ready and an authenticated echo, not its PID/window alone.
        evidence.record(step="first-launch-logs", paths=[str(path) for path in logs])
        summary["stage"] = "relaunch"
        evidence.write_result(summary)
        relaunched, _ = launch(evidence, app, census, "relaunch", ready_directory, args.nonce)
        old = {(f.pid, f.start) for f in [*roles, *recovered]}
        smoke.require(all((f.pid, f.start) not in old for f in relaunched), "relaunch reused an old process identity")
        new_host = next(f for f in relaunched if not f.guardian and f.executable.endswith("/MacOS/keld-host"))
        crash_exact(evidence, new_host, install_root, "relaunched-host")
        smoke.wait_roles_gone(app / "Contents/MacOS/keld-host", app / "Contents/Helpers/bun",
                              relaunched, evidence, "relaunched-host-crash")
        unchanged(evidence, app, baseline, document, document_hash, "after-relaunch")
        summary.update(state="PASS", stage="complete", normal_quit="NOT_CLAIMED",
                       bun_crash="successor under same Host and Guardian; native window observed",
                       authenticated_successor_ready="per-PID nonce witness after Host Ready and echo Reply",
                       host_crash="ordinary owned family retired before harness cleanup",
                       unrun=["detached descendants", "simultaneous Host and Guardian death",
                              "draw.io document workflows", "normal native Quit"])
        evidence.write_result(summary)
        print(json.dumps(summary, indent=2))
        return 0
    except BaseException as error:
        summary.update(state="FAIL", reason=f"{type(error).__name__}: {error}")
        evidence.write_result(summary)
        evidence.record(step="product-observation-failed", reason=str(error))
        smoke.cleanup_verified_roles(evidence, install_root)
        print(json.dumps(summary, indent=2), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

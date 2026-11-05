#!/usr/bin/env python3
"""Export immutable GH481 source for the standalone W4 test fixture."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys

PINNED_COMMIT = "e09b520eaee1e4bd69bd0bde2137ec52f0fed18a"
PIN_SCHEMA = "keld.w4-shared-source/v1"


class ExportError(Exception):
    """A dependency or destination does not satisfy the fixture contract."""


def git_bytes(repo: Path, *arguments: str) -> bytes:
    result = subprocess.run(
        ["git", "-C", str(repo), *arguments],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        env={**os.environ, "GIT_OPTIONAL_LOCKS": "0"},
    )
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise ExportError(f"git {' '.join(arguments)} failed: {detail}")
    return result.stdout


def read_pin(path: Path) -> dict[str, str]:
    pin = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(pin, dict) or pin.get("schema") != PIN_SCHEMA:
        raise ExportError("unsupported shared-pin schema")
    if pin.get("commit") != PINNED_COMMIT:
        raise ExportError(f"shared-pin must select immutable commit {PINNED_COMMIT}")
    files = pin.get("files")
    if not isinstance(files, dict) or not files:
        raise ExportError("shared-pin files must be a nonempty object")
    for name, expected in files.items():
        if not isinstance(name, str) or not isinstance(expected, str):
            raise ExportError("shared-pin paths and Git blob IDs must be strings")
        path_parts = PurePosixPath(name)
        if (
            path_parts.is_absolute()
            or path_parts.as_posix() != name
            or any(part in ("", ".", "..") for part in name.split("/"))
            or any(character in name for character in ("\\", ":", "\0"))
        ):
            raise ExportError(f"unsafe relative export path: {name!r}")
        if re.fullmatch(r"[0-9a-f]{40}", expected) is None:
            raise ExportError(f"invalid Git blob ID for {name}")
    for required in (
        "packages/@keld/kipc/package.json",
        "packages/@keld/kipc/src/transport.ts",
    ):
        if required not in files:
            raise ExportError(f"shared-pin is missing package-link input {required}")
    return files


def require_fresh_destination(out: Path) -> None:
    is_junction = getattr(out, "is_junction", lambda: False)
    if out.is_symlink() or is_junction():
        raise ExportError("--out must be a real fresh directory, not a link")
    if os.path.lexists(out) and (not out.is_dir() or any(out.iterdir())):
        raise ExportError("--out must be absent or an empty directory")


def package_link(out: Path) -> str:
    target = out / "packages" / "@keld" / "kipc"
    link = out / "node_modules" / "@keld" / "kipc"
    link.parent.mkdir(parents=True, exist_ok=True)
    if os.name == "nt":
        # Pass path values as data, not interpolated PowerShell source.
        environment = {
            **os.environ,
            "KELD_W4_EXPORT_LINK": str(link),
            "KELD_W4_EXPORT_TARGET": str(target),
        }
        result = subprocess.run(
            [
                "powershell.exe",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$ErrorActionPreference='Stop'; "
                "New-Item -ItemType Junction -Path $env:KELD_W4_EXPORT_LINK "
                "-Value $env:KELD_W4_EXPORT_TARGET | Out-Null",
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            env=environment,
        )
        if result.returncode != 0:
            detail = result.stderr.decode("utf-8", errors="replace").strip()
            raise ExportError(f"could not create package directory junction: {detail}")
        kind = "junction"
    else:
        link.symlink_to(target, target_is_directory=True)
        kind = "directory-symlink"
    if not link.is_dir() or link.resolve() != target.resolve():
        raise ExportError("package link does not resolve to the exported kipc directory")
    return kind


def export(repo: Path, out: Path, pin_path: Path) -> dict[str, object]:
    files = read_pin(pin_path)
    require_fresh_destination(out)
    actual_commit = git_bytes(
        repo, "rev-parse", "--verify", f"{PINNED_COMMIT}^{{commit}}"
    ).decode("ascii").strip()
    if actual_commit != PINNED_COMMIT:
        raise ExportError("repository resolved a different shared commit")

    # Verify every source before creating output. Read blobs directly: checkout
    # filters, line-ending conversions, and dirty owner files cannot affect bytes.
    verified: list[tuple[str, bytes]] = []
    for name, expected in sorted(files.items()):
        source = f"{PINNED_COMMIT}:{name}"
        actual = git_bytes(repo, "rev-parse", "--verify", source).decode("ascii").strip()
        if actual != expected:
            raise ExportError(
                f"Git blob mismatch for {name}: expected {expected}, found {actual}"
            )
        payload = git_bytes(repo, "cat-file", "blob", source)
        actual_bytes = hashlib.sha1(
            b"blob " + str(len(payload)).encode("ascii") + b"\0" + payload
        ).hexdigest()
        if actual_bytes != expected:
            raise ExportError(f"exported byte hash mismatch for {name}")
        verified.append((name, payload))

    # Refuse a destination another process populated while Git was being read.
    require_fresh_destination(out)
    out.mkdir(parents=True, exist_ok=True)
    for name, payload in verified:
        destination = out.joinpath(*PurePosixPath(name).parts)
        destination.parent.mkdir(parents=True, exist_ok=True)
        with destination.open("xb") as stream:
            stream.write(payload)
    link_kind = package_link(out)
    return {
        "schema": "keld.w4-shared-export/v1",
        "commit": PINNED_COMMIT,
        "files": len(verified),
        "bytes": sum(len(payload) for _, payload in verified),
        "out": str(out),
        "packageLink": link_kind,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, type=Path, help="Git repository containing S1")
    parser.add_argument("--out", required=True, type=Path, help="absent or empty export directory")
    parser.add_argument(
        "--pin",
        type=Path,
        default=Path(__file__).with_name("shared-pin.json"),
        help="pin manifest; an override supports explicit integrity negative controls",
    )
    args = parser.parse_args()
    try:
        # absolute() preserves the final component so a supplied link is refused.
        receipt = export(args.repo.absolute(), args.out.absolute(), args.pin.absolute())
    except (ExportError, OSError, ValueError) as error:
        print(f"prepare_shared: {error}", file=sys.stderr)
        return 1
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

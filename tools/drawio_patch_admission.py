#!/usr/bin/env python3
"""Verify the approved GH499 draw.io source adaptation without changing its inputs.

This is a source-custody preflight, not a Host, Guard, or real-app acceptance test.
The immutable approved hashes are recorded in gyldlab/keld#499 comment 6697798167.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import hmac
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile


SOURCE_PATH = Path("src/main/electron.js")
MAX_SOURCE_BYTES = 2 * 1024 * 1024
MAX_PATCH_BYTES = 1024 * 1024


@dataclass(frozen=True)
class PatchHashes:
    original: str
    candidate: str
    forward: str
    reverse: str


# Approved v6 artifact hashes. Callers cannot replace them through CLI arguments.
APPROVED_V6 = PatchHashes(
    original="55505e690d37070344be172a68ad997ab48c687302dc5dee30a988d4f153770d",
    candidate="ef561d45036d167640651ec3e09c884dff458d2c1320bb416ef7120d23307f48",
    forward="32532c22fbfb11c19f8041fa1ef79c53e286be75e672a827cd72ca065c6630bb",
    reverse="cc7f3c9442348653e428d41c2e6df065d2d652555eb23bca1ac007e1d7d76e81",
)


class AdmissionError(Exception):
    """A deterministic refusal of missing, untrusted, or inconsistent source."""


def _read_regular_file(path: Path, limit: int) -> bytes:
    """Read one bounded, regular leaf without following a symlink."""
    # macOS mounts /var through /private/var. Parent paths are untrusted input
    # locators, not authority roots; only the opened leaf and admitted bytes
    # are used, and all patch writes stay inside a separate temporary tree.
    if path.is_symlink():
        raise AdmissionError(f"linked input path refused: {path}")
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as error:
        raise AdmissionError(f"cannot open input {path}: {error.strerror}") from error
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            raise AdmissionError(f"non-regular input refused: {path}")
        if metadata.st_size > limit:
            raise AdmissionError(f"input exceeds {limit} bytes: {path}")
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            content = stream.read(limit + 1)
        if len(content) > limit:
            raise AdmissionError(f"input grew beyond {limit} bytes: {path}")
        return content
    finally:
        os.close(descriptor)


def _require_digest(name: str, content: bytes, expected: str) -> None:
    observed = hashlib.sha256(content).hexdigest()
    if not hmac.compare_digest(observed, expected):
        raise AdmissionError(f"{name} SHA-256 mismatch: expected {expected}; got {observed}")


def _git_apply(root: Path, patch: Path, *args: str) -> str:
    git = shutil.which("git")
    if git is None:
        raise AdmissionError("git executable unavailable for independent patch replay")
    # Environment-supplied worktree and index overrides must never redirect an
    # admission replay into the caller's repository or source checkout.
    environment = {
        key: value for key, value in os.environ.items() if not key.startswith("GIT_")
    }
    environment.update({"GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1"})
    try:
        result = subprocess.run(
            [git, "-c", "core.quotepath=false", "apply", "-p1", *args, str(patch)],
            cwd=root,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=10,
            check=False,
            env=environment,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise AdmissionError(f"patch replay unavailable: {type(error).__name__}") from error
    if result.returncode != 0:
        raise AdmissionError(f"git apply {args[0]} refused the approved patch")
    return result.stdout


def _apply_exactly_one_source(root: Path, patch: Path) -> None:
    numstat = _git_apply(root, patch, "--numstat")
    entries = [line.split("\t", 2) for line in numstat.splitlines()]
    if len(entries) != 1 or len(entries[0]) != 3:
        raise AdmissionError("patch must modify exactly one tracked source file")
    if not entries[0][0].isdigit() or not entries[0][1].isdigit():
        raise AdmissionError("binary or uncountable patch refused")
    if entries[0][2] != SOURCE_PATH.as_posix():
        raise AdmissionError(f"patch modifies an unapproved path: {entries[0][2]}")
    _git_apply(root, patch, "--check", "--whitespace=nowarn")
    _git_apply(root, patch, "--whitespace=nowarn")


def verify_patch_chain(
    original: bytes,
    candidate: bytes,
    forward: bytes,
    reverse: bytes,
    expected: PatchHashes,
) -> None:
    """Prove exact input hashes and both independent replay directions."""
    for name, content, digest, limit in (
        ("original", original, expected.original, MAX_SOURCE_BYTES),
        ("candidate", candidate, expected.candidate, MAX_SOURCE_BYTES),
        ("forward", forward, expected.forward, MAX_PATCH_BYTES),
        ("reverse", reverse, expected.reverse, MAX_PATCH_BYTES),
    ):
        if len(content) > limit:
            raise AdmissionError(f"{name} exceeds {limit} bytes")
        _require_digest(name, content, digest)

    # Every untrusted input is digest-admitted before a patch is given to Git.
    with tempfile.TemporaryDirectory(prefix="keld-gh499-patch-") as directory:
        root = Path(directory)
        source = root / SOURCE_PATH
        source.parent.mkdir(parents=True)
        forward_path = root / "approved-forward.patch"
        reverse_path = root / "approved-reverse.patch"
        forward_path.write_bytes(forward)
        reverse_path.write_bytes(reverse)

        source.write_bytes(original)
        _apply_exactly_one_source(root, forward_path)
        if source.read_bytes() != candidate:
            raise AdmissionError("forward patch did not produce the approved candidate bytes")

        # Reverse is a separate approved artifact, not an inferred '-R' flag.
        _apply_exactly_one_source(root, reverse_path)
        if source.read_bytes() != original:
            raise AdmissionError("reverse patch did not recover the original source bytes")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--original", required=True, type=Path)
    parser.add_argument("--candidate", required=True, type=Path)
    parser.add_argument("--forward", required=True, type=Path)
    parser.add_argument("--reverse", required=True, type=Path)
    arguments = parser.parse_args(argv)
    try:
        verify_patch_chain(
            _read_regular_file(arguments.original, MAX_SOURCE_BYTES),
            _read_regular_file(arguments.candidate, MAX_SOURCE_BYTES),
            _read_regular_file(arguments.forward, MAX_PATCH_BYTES),
            _read_regular_file(arguments.reverse, MAX_PATCH_BYTES),
            APPROVED_V6,
        )
    except AdmissionError as error:
        print(f"GH499 patch admission REFUSED: {error}", file=sys.stderr)
        return 1
    print("GH499 approved v6 original, candidate, forward, and reverse bytes verified")
    print("Source-only result. Installed draw.io/Host/Guard/macOS behavior remains unverified here.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

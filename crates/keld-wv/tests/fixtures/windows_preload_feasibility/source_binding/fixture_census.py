"""Experimental, read-only digest vectors; not a Keld build/migrate implementation.

The explicit census is fixture data. The tab/LF encoding below is a test convention
only: W1 r17 does not approve a production mainDigest byte encoding or input census.
No source/config is imported, evaluated, transpiled, resolved, or executed.
"""
from __future__ import annotations

import hashlib
from pathlib import Path, PurePosixPath
from typing import Iterable

FORMAT = "w1-evidence-census-tab-lf/v0"
EXCLUDED = "keld.permissions.jsonc"


def read_fixture(root: Path, name: str) -> bytes:
    """Read an explicit test path, never a file URL or a product trust identity."""
    posix = PurePosixPath(name)
    if (
        not name
        or posix.is_absolute()
        or str(posix) != name
        or any(part in ("", ".", "..") for part in posix.parts)
        or any(char in name for char in ("\\", "\t", "\r", "\n", "\0"))
        or ":" in name
    ):
        raise ValueError(f"non-canonical fixture path: {name!r}")
    path = root.joinpath(*posix.parts)
    for part in (path, *path.parents):
        if part == root.parent:
            break
        if part.is_symlink():
            raise ValueError(f"fixture symlink is not an input: {name}")
    return path.read_bytes()


def census(root: Path, inputs: Iterable[str], declared_preloads: Iterable[str]) -> dict:
    """Hash explicit fixture inputs plus all fixture-declared preloads."""
    names = set(inputs) | set(declared_preloads)
    names.discard(EXCLUDED)
    ordered = sorted(names, key=lambda name: name.encode("utf-8"))
    entries = [
        {"path": name, "sha256": hashlib.sha256(read_fixture(root, name)).hexdigest()}
        for name in ordered
    ]
    canonical = b"".join(
        entry["path"].encode("utf-8")
        + b"\t"
        + entry["sha256"].encode("ascii")
        + b"\n"
        for entry in entries
    )
    return {
        "status": "planned-fixture-only",
        "format": FORMAT,
        "entries": entries,
        "canonical_hex": canonical.hex(),
        "fixture_digest": hashlib.sha256(canonical).hexdigest(),
    }

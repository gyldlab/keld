#!/usr/bin/env python3
"""Create a source-verified overlay for a NEW draw.io production source build.

The overlay contains real replacement mxgraph JavaScript at its original
application paths, plus a unified patch and SHA-256 input/output manifest.
Output creation is bound to no-follow directory handles captured before
source generation; a later pathname symlink swap cannot redirect its writes.
"""

from __future__ import annotations

import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys


ROOT = Path(__file__).resolve().parent
DIR_FD_SUPPORTED = os.open in os.supports_dir_fd and os.mkdir in os.supports_dir_fd
SOURCES = (
    ("src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js", "make_patch.py"),
    ("src/main/webapp/mxgraph/src/shape/mxText.js", "make_bbox_patch.py"),
)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def directory_flags() -> int:
    """A directory identity is acquired without following a renamed symlink."""
    flags = (getattr(os, "O_DIRECTORY", 0), getattr(os, "O_NOFOLLOW", 0))
    if not all(flags) or not DIR_FD_SUPPORTED:
        raise RuntimeError("destination staging needs POSIX directory handles and O_NOFOLLOW")
    return os.O_RDONLY | flags[0] | flags[1] | getattr(os, "O_CLOEXEC", 0)


def open_directory_prefix(path: Path, stack: ExitStack, *, allow_missing: bool) -> tuple[int, tuple[str, ...]]:
    """Pin the deepest existing directory; never follow aliases during the walk."""
    flags = directory_flags()
    fd = os.open(path.anchor, flags)
    stack.callback(os.close, fd)
    missing: list[str] = []
    for component in path.parts[1:]:
        if missing:
            missing.append(component)
            continue
        try:
            child = os.open(component, flags, dir_fd=fd)
        except FileNotFoundError:
            if not allow_missing:
                raise
            missing.append(component)
            continue
        stack.callback(os.close, child)
        fd = child
    return fd, tuple(missing)


def same_directory_at_path(path: Path, expected: int) -> bool:
    """Check whether the caller's pathname still names the pinned inode."""
    with ExitStack() as stack:
        try:
            found, missing = open_directory_prefix(path, stack, allow_missing=False)
        except OSError:
            return False
        if missing:
            return False
        actual = os.fstat(found)
        pinned = os.fstat(expected)
        return (actual.st_dev, actual.st_ino) == (pinned.st_dev, pinned.st_ino)


def create_directory(parent_fd: int, name: str, stack: ExitStack) -> int:
    """Create under a pinned parent and open the created name without symlinks."""
    os.mkdir(name, 0o700, dir_fd=parent_fd)
    child = os.open(name, directory_flags(), dir_fd=parent_fd)
    stack.callback(os.close, child)
    return child


def write_new_file(parent_fd: int, name: str, data: bytes) -> tuple[int, int, int, str]:
    """O_EXCL and O_NOFOLLOW refuse replacement names instead of following them."""
    flags = (os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW |
             getattr(os, "O_CLOEXEC", 0))
    fd = os.open(name, flags, 0o600, dir_fd=parent_fd)
    with os.fdopen(fd, "wb") as stream:
        stream.write(data)
        identity = os.fstat(stream.fileno())
    return identity.st_dev, identity.st_ino, len(data), sha256(data)


def write_overlay(
    root_fd: int, files: list[tuple[str, bytes]], stack: ExitStack
) -> tuple[dict[tuple[str, ...], int], dict[tuple[str, ...], tuple[int, int, int, str]]]:
    """Retain created directories and their file identities until verification."""
    dirs = {(): root_fd}
    written = {}
    for relative_path, data in files:
        parts = Path(relative_path).parts
        if (not parts or Path(relative_path).is_absolute() or
                any(part in ("", ".", "..") or part.casefold().endswith(".app")
                             for part in parts)):
            raise RuntimeError("invalid staged source path")
        for depth in range(1, len(parts)):
            key = parts[:depth]
            if key not in dirs:
                dirs[key] = create_directory(dirs[key[:-1]], parts[depth - 1], stack)
        written[parts] = write_new_file(dirs[parts[:-1]], parts[-1], data)
    return dirs, written


def verify_published_overlay(
    root_fd: int,
    dirs: dict[tuple[str, ...], int],
    files: dict[tuple[str, ...], tuple[int, int, int, str]],
    stack: ExitStack,
) -> None:
    """Reopen each advertised path without following symlinks before success.

    This checks the current named paths, independent of handles used to write.
    A later namespace mutation still requires build-owner custody at consume time.
    """
    verified = {(): root_fd}
    for key, pinned in dirs.items():
        if not key:
            continue
        fd = os.open(key[-1], directory_flags(), dir_fd=verified[key[:-1]])
        stack.callback(os.close, fd)
        observed, expected = os.fstat(fd), os.fstat(pinned)
        if (observed.st_dev, observed.st_ino) != (expected.st_dev, expected.st_ino):
            raise RuntimeError(f"destination published directory changed: {'/'.join(key)}")
        verified[key] = fd
    for parts, (device, inode, size, digest) in files.items():
        name = parts[-1]
        fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW |
                     getattr(os, "O_CLOEXEC", 0), dir_fd=verified[parts[:-1]])
        with os.fdopen(fd, "rb") as source:
            observed = os.fstat(source.fileno())
            if (not stat.S_ISREG(observed.st_mode) or
                    (observed.st_dev, observed.st_ino, observed.st_size) !=
                    (device, inode, size)):
                raise RuntimeError(f"destination published file changed: {'/'.join(parts)}")
            hasher = hashlib.sha256()
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                hasher.update(chunk)
            if hasher.hexdigest() != digest:
                raise RuntimeError(f"destination published content changed: {'/'.join(parts)}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True,
                        help="unmodified, pinned jgraph/drawio checkout")
    parser.add_argument("--output-root", type=Path, required=True,
                        help="new isolated directory for two replacement JS files and patch")
    args = parser.parse_args()

    source_root = args.source_root.resolve(strict=True)
    output_root = args.output_root.resolve()
    if output_root == source_root or source_root in output_root.parents:
        parser.error("output must be outside the original source tree")
    if output_root.exists():
        parser.error("output already exists; use a new isolated directory")
    if any(path.suffix.casefold() == ".app" for path in (source_root, *source_root.parents)):
        parser.error("refusing installed application input; use a source checkout")
    # Resolve aliases first: writing an overlay anywhere inside a bundle would
    # mutate its signed resource seal, even if the input checkout is pristine.
    if any(path.suffix.casefold() == ".app" for path in
           (output_root, *output_root.parents, args.output_root, *args.output_root.parents)):
        parser.error("refusing application bundle output; use an isolated staging directory")

    with ExitStack() as stack:
        # This handle precedes all four generators. A name swap during their
        # execution cannot redirect later mkdirs or source writes into a .app.
        try:
            parent_fd, missing = open_directory_prefix(
                output_root.parent, stack, allow_missing=True
            )
        except (OSError, RuntimeError) as error:
            parser.error(f"destination parent admission failed: {error}")
        prefix = output_root.parent
        for _ in missing:
            prefix = prefix.parent

        prepared = []
        full_patch = bytearray()
        for relative_path, generator in SOURCES:
            original_path = source_root / relative_path
            if original_path.is_symlink() or not original_path.is_file():
                parser.error(f"missing real source file: {relative_path}")
            if not original_path.resolve().is_relative_to(source_root):
                parser.error(f"source file escapes the checkout: {relative_path}")
            original = original_path.read_bytes()
            generated = []
            for as_patch in (False, True):
                argv = [sys.executable, str(ROOT / generator)]
                if as_patch:
                    argv.append("--patch")
                result = subprocess.run(argv, input=original, capture_output=True, check=False)
                if result.returncode:
                    parser.error(f"{generator}: {result.stderr.decode('utf-8', 'replace')}")
                generated.append(result.stdout)
            candidate, diff = generated
            if not diff or candidate == original:
                parser.error(f"patch did not change {relative_path}")
            prepared.append((relative_path, original, candidate))
            full_patch.extend(diff)

        # Detect a rebound pathname before creating anything. The held handle
        # also prevents redirection if a swap occurs immediately after this check.
        if not same_directory_at_path(prefix, parent_fd):
            parser.error("destination parent changed during source verification")
        try:
            for component in missing:
                parent_fd = create_directory(parent_fd, component, stack)
            output_fd = create_directory(parent_fd, output_root.name, stack)
        except OSError as error:
            parser.error(f"destination changed or unsafe during directory creation: {error}")

        manifest = {
            "upstream": "jgraph/drawio@24b76c2cbd55d88e354042e8d329a2e4708972bc",
            "scope": "replacement application source; full app/provider/build qualification pending",
            "files": [],
        }
        content: list[tuple[str, bytes]] = []
        for relative_path, original, candidate in prepared:
            content.append((relative_path, candidate))
            manifest["files"].append({
                "path": relative_path,
                "original_sha256": sha256(original),
                "candidate_sha256": sha256(candidate),
            })
        patch = bytes(full_patch)
        manifest["patch_sha256"] = sha256(patch)
        content.append(("gh1404-g1-production.patch", patch))
        content.append(("source-manifest.json", (json.dumps(manifest, indent=2) + "\n").encode()))
        try:
            dirs, records = write_overlay(output_fd, content, stack)
            verify_published_overlay(output_fd, dirs, records, stack)
        except (OSError, RuntimeError) as error:
            parser.error(f"destination changed or unsafe during source publication: {error}")
        if not same_directory_at_path(output_root, output_fd):
            parser.error("destination changed after source staging; output is unqualified")
        print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()

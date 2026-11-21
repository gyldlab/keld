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
import secrets
import stat
import subprocess
import sys
from typing import NamedTuple


ROOT = Path(__file__).resolve().parent
DIR_FD_SUPPORTED = os.open in os.supports_dir_fd and os.mkdir in os.supports_dir_fd
SOURCES = (
    ("src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js", "make_patch.py"),
    ("src/main/webapp/mxgraph/src/shape/mxText.js", "make_bbox_patch.py"),
)
PINNED_SOURCE_SHA256 = {
    "src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js":
        "0daef9812e3b8c530bc9556791ffaea53fe9a57091a86534bd909ca31e4485b3",
    "src/main/webapp/mxgraph/src/shape/mxText.js":
        "a6c0f2c748403a74b11405c864a8278bfcdad216f80ea5b290c1e917d13f9772",
}
MAX_ORIGINAL_BYTES = 8 * 1024 * 1024


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


class PinnedSource(NamedTuple):
    """One no-follow source inode, retained through manifest publication."""

    path: str
    directories: tuple[tuple[str, int], ...]
    fd: int
    identity: tuple[int, ...]
    data: bytes


def source_identity(info: os.stat_result) -> tuple[int, ...]:
    """Refuse source inode, length, mode, link count, or write-time changes."""
    return (info.st_dev, info.st_ino, info.st_mode, info.st_size,
            info.st_mtime_ns, info.st_ctime_ns, info.st_nlink)


def source_flags() -> int:
    if not getattr(os, "O_NOFOLLOW", 0) or not getattr(os, "O_NONBLOCK", 0):
        raise RuntimeError("source stage needs O_NOFOLLOW and O_NONBLOCK")
    return (os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK |
            getattr(os, "O_CLOEXEC", 0))


def read_source_fd(fd: int) -> bytes:
    """Bounded read of one pinned regular file; never reopen by pathname."""
    os.lseek(fd, 0, os.SEEK_SET)
    chunks: list[bytes] = []
    consumed = 0
    while True:
        chunk = os.read(fd, min(65536, MAX_ORIGINAL_BYTES + 1 - consumed))
        if not chunk:
            break
        consumed += len(chunk)
        if consumed > MAX_ORIGINAL_BYTES:
            raise RuntimeError("source file exceeds the bounded original size")
        chunks.append(chunk)
    return b"".join(chunks)


def pinned_source_file(root_fd: int, name: str, stack: ExitStack) -> PinnedSource:
    """Admit only the exact two pristine original bytes via root-relative FDs."""
    if name not in PINNED_SOURCE_SHA256:
        raise RuntimeError(f"unqualified source path: {name}")
    parts = Path(name).parts
    parent_fd = root_fd
    directories: list[tuple[str, int]] = []
    root_device = os.fstat(root_fd).st_dev
    for component in parts[:-1]:
        opened = os.open(component, directory_flags(), dir_fd=parent_fd)
        stack.callback(os.close, opened)
        if os.fstat(opened).st_dev != root_device:
            raise RuntimeError(f"source directory crosses root filesystem: {name}")
        directories.append((component, opened))
        parent_fd = opened

    opened = os.open(parts[-1], source_flags(), dir_fd=parent_fd)
    stack.callback(os.close, opened)
    before = os.fstat(opened)
    if (not stat.S_ISREG(before.st_mode) or before.st_nlink != 1 or
            before.st_dev != root_device or not 0 < before.st_size <= MAX_ORIGINAL_BYTES):
        raise RuntimeError(f"source is not a unique regular file inside root: {name}")
    original = read_source_fd(opened)
    if (len(original) != before.st_size or
            source_identity(os.fstat(opened)) != source_identity(before) or
            sha256(original) != PINNED_SOURCE_SHA256[name]):
        raise RuntimeError(f"source original changed or digest is not pinned: {name}")
    return PinnedSource(name, tuple(directories), opened, source_identity(before), original)


def verify_pinned_sources(root: Path, root_fd: int, pins: list[PinnedSource]) -> None:
    """Verify live source names still bind to the held, pristine source inodes."""
    if not same_directory_at_path(root, root_fd):
        raise RuntimeError("source root pathname no longer names pinned directory")
    for pinned in pins:
        with ExitStack() as temporary:
            parent_fd = root_fd
            for component, retained_fd in pinned.directories:
                opened = os.open(component, directory_flags(), dir_fd=parent_fd)
                temporary.callback(os.close, opened)
                live, retained = os.fstat(opened), os.fstat(retained_fd)
                if (live.st_dev, live.st_ino) != (retained.st_dev, retained.st_ino):
                    raise RuntimeError(f"source directory rebound: {pinned.path}")
                parent_fd = opened
            leaf = os.open(Path(pinned.path).name, source_flags(), dir_fd=parent_fd)
            temporary.callback(os.close, leaf)
            for descriptor in (pinned.fd, leaf):
                before = os.fstat(descriptor)
                if (not stat.S_ISREG(before.st_mode) or before.st_nlink != 1 or
                        source_identity(before) != pinned.identity or
                        read_source_fd(descriptor) != pinned.data or
                        source_identity(os.fstat(descriptor)) != pinned.identity):
                    raise RuntimeError(f"source inode, bytes, or link count changed: {pinned.path}")
    if not same_directory_at_path(root, root_fd):
        raise RuntimeError("source root changed during final verification")


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
        stream.flush()
        os.fsync(stream.fileno())
        identity = os.fstat(stream.fileno())
    return identity.st_dev, identity.st_ino, len(data), sha256(data)


def publish_atomic_manifest(parent_fd: int, data: bytes) -> tuple[int, int, int, str]:
    """Fsync a private full temp; link it no-replace only when complete.

    Crash during writing leaves no final manifest name. Link of a completed
    inode is atomic; a name attack cannot silently replace an existing final.
    """
    temp_name = f".source-manifest-{secrets.token_hex(16)}.tmp"
    flags = (os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW |
             getattr(os, "O_CLOEXEC", 0))
    fd = os.open(temp_name, flags, 0o600, dir_fd=parent_fd)
    try:
        pending = memoryview(data)
        while pending:
            n = os.write(fd, pending)
            if n <= 0:
                raise RuntimeError("manifest temporary file write did not advance")
            pending = pending[n:]
        os.fsync(fd)
        written = os.fstat(fd)
        if (not stat.S_ISREG(written.st_mode) or
                written.st_size != len(data) or written.st_nlink != 1):
            raise RuntimeError("manifest temporary file changed during complete write")
    finally:
        os.close(fd)

    # This is a single atomic insertion into the pinned directory, not a
    # rename that could overwrite a concurrent final manifest.
    os.link(temp_name, "source-manifest.json",
            src_dir_fd=parent_fd, dst_dir_fd=parent_fd, follow_symlinks=False)
    final = os.stat("source-manifest.json", dir_fd=parent_fd, follow_symlinks=False)
    if (not stat.S_ISREG(final.st_mode) or
            (final.st_dev, final.st_ino, final.st_size) !=
            (written.st_dev, written.st_ino, len(data))):
        raise RuntimeError("manifest final identity changed during publication")
    os.fsync(parent_fd)
    os.unlink(temp_name, dir_fd=parent_fd)
    os.fsync(parent_fd)
    return written.st_dev, written.st_ino, len(data), sha256(data)


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
        # O_NONBLOCK is essential here: a same-UID writer can replace the
        # published regular file with a FIFO after its source bytes were
        # written. Without it, the read-only open waits for a FIFO writer
        # forever and never reaches our S_ISREG refusal.
        fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK |
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
        # Pin the original source root before the generators run, separate
        # from the destination handles. A later source-name swap must fail.
        try:
            source_fd, source_missing = open_directory_prefix(
                source_root, stack, allow_missing=False
            )
            if source_missing or not same_directory_at_path(source_root, source_fd):
                raise RuntimeError("source root changed before source admission")
        except (OSError, RuntimeError) as error:
            parser.error(f"source root admission failed: {error}")
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
        pinned_inputs: list[PinnedSource] = []
        full_patch = bytearray()
        for relative_path, generator in SOURCES:
            try:
                pinned = pinned_source_file(source_fd, relative_path, stack)
            except (OSError, RuntimeError) as error:
                parser.error(f"missing real source file (pinned) {relative_path}: {error}")
            pinned_inputs.append(pinned)
            original = pinned.data
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

        # A swapped original, intermediate source directory or root pathname
        # cannot be laundered into a manifest by the generator's output bytes.
        try:
            verify_pinned_sources(source_root, source_fd, pinned_inputs)
        except (OSError, RuntimeError) as error:
            parser.error(f"source changed during generator execution: {error}")
        # Detect a rebound destination pathname before creating anything.
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
        manifest_bytes = (json.dumps(manifest, indent=2) + "\n").encode()
        try:
            dirs, records = write_overlay(output_fd, content, stack)
            # A killed staging process must not leave a final hash receipt
            # before verifying the files it describes. The manifest is the
            # last advertised output, after the pinned source/patch are proven.
            verify_published_overlay(output_fd, dirs, records, stack)
            if not same_directory_at_path(output_root, output_fd):
                raise RuntimeError("destination changed after source staging; output is unqualified")
            verify_pinned_sources(source_root, source_fd, pinned_inputs)
            # Persist all output source files and directory entries before
            # exposing the final manifest name, including abrupt death cases.
            for opened in reversed(tuple(dirs.values())):
                os.fsync(opened)
            records[("source-manifest.json",)] = publish_atomic_manifest(
                output_fd, manifest_bytes
            )
            verify_published_overlay(output_fd, dirs, records, stack)
            verify_pinned_sources(source_root, source_fd, pinned_inputs)
        except (OSError, RuntimeError) as error:
            parser.error(f"source or destination changed or unsafe during source publication: {error}")
        if not same_directory_at_path(output_root, output_fd):
            parser.error("destination changed after source staging; output is unqualified")
        print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()

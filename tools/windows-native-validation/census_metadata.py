"""Standalone read-only metadata census. No source bodies, plans or subprocesses."""
import hashlib
import json
import os
from pathlib import Path
import stat
import sys

PATHS = (
    'crates/keld-update/src/baseline/windows_v3.rs',
    'crates/keld-update/src/manifest.rs',
    'crates/keld-update/src/windows_baseline/initialize/observed_v3.rs',
    'crates/keld-update/src/windows_baseline/initialize/request_v3.rs',
    'crates/keld-update/src/windows_baseline/initialize/v3.rs',
    'crates/keld-update/src/windows_baseline/initialize.rs',
    'crates/keld-cli/src/build/local_archive.rs',
    'crates/keld-cli/src/build/local_archive_command.rs',
    'crates/keld-cli/src/build/local_baseline.rs',
    'crates/keld-cli/src/build/local_images.rs',
    'crates/keld-cli/src/build/local_initial_install.rs',
    'crates/keld-cli/src/build/local_manifest_session.rs',
    'crates/keld-cli/src/build/local_source.rs',
    'crates/keld-cli/src/build.rs',
    'crates/keld-update/src/windows_baseline.rs',
    'crates/keld-update/src/lib.rs',
    'crates/keld-core/Cargo.toml',
    'crates/keld-runtime/Cargo.toml',
    'crates/keld-host/Cargo.toml',
    'crates/keld-cli/src/build/windows_compiler.rs',
    'crates/keld-cli/tools/closed-app.ts',
    'crates/keld-cli/tools/closed-app-runner.ts',
    'crates/keld-cli/tools/protocol.ts',
    'crates/keld-cli/tools/static-renderer.ts',
)
MAX_BYTES = 512 * 1024

class Refusal(Exception):
    pass

def check_chain(target):
    """Reject observed symlink/reparse components, including before a missing leaf."""
    for component in reversed((target, *target.parents)):
        try:
            info = component.lstat()
        except FileNotFoundError:
            return False
        if stat.S_ISLNK(info.st_mode) or getattr(info, 'st_file_attributes', 0) & 0x400:
            raise Refusal('alias')
        if component != target and not stat.S_ISDIR(info.st_mode):
            raise Refusal('ancestor_not_directory')
    return True

def identity(info):
    return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns)

def observe(root, relative):
    target = root / relative
    if not check_chain(target):
        return {'path': relative, 'present': False}
    initial = target.lstat()
    if not stat.S_ISREG(initial.st_mode) or initial.st_size > MAX_BYTES:
        raise Refusal('not_bounded_regular_file')
    flags = os.O_RDONLY | getattr(os, 'O_BINARY', 0) | getattr(os, 'O_NOFOLLOW', 0)
    descriptor = os.open(target, flags)
    with os.fdopen(descriptor, 'rb') as stream:
        before = os.fstat(stream.fileno())
        if identity(before) != identity(initial) or not stat.S_ISREG(before.st_mode):
            raise Refusal('changed')
        raw = stream.read(MAX_BYTES + 1)
        if len(raw) > MAX_BYTES or len(raw) != before.st_size:
            raise Refusal('changed_or_oversized')
        stream.seek(0)
        second = stream.read(MAX_BYTES + 1)
        after = os.fstat(stream.fileno())
        if second != raw or identity(before) != identity(after):
            raise Refusal('changed')
        if not check_chain(target) or identity(target.lstat()) != identity(after):
            raise Refusal('changed')
    return {'path': relative, 'present': True, 'bytes': len(raw),
            'sha256': hashlib.sha256(raw).hexdigest()}

def census(root):
    if not root.is_absolute():
        raise Refusal('absolute_root_required')
    if not check_chain(root) or not root.is_dir():
        raise Refusal('root_missing')
    # All observations succeed before any output. No content-bearing exceptions.
    return [observe(root, relative) for relative in PATHS]

def main(argv):
    if len(argv) != 2:
        print('{"error":"one_absolute_root_required"}', file=sys.stderr)
        return 2
    try:
        rows = census(Path(argv[1]))
    except Refusal as error:
        print(json.dumps({'error': str(error)}), file=sys.stderr)
        return 2
    except (OSError, ValueError):
        print('{"error":"metadata_read_refused"}', file=sys.stderr)
        return 2
    for row in rows:
        print(json.dumps(row), flush=True)
    return 0

if __name__ == '__main__':
    raise SystemExit(main(sys.argv))

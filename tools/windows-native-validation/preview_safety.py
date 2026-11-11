"""Bounded regular-file reads and source-preview path checks."""
import hashlib
import stat

def sha(data):
    return hashlib.sha256(data).hexdigest()

def reject_alias_components(path):
    if '..' in path.parts:
        raise ValueError('Parent alias components refused')

def outside_repo(repo, output):
    return not (output == repo or repo in output.parents or output in repo.parents)

def read_plain(path):
    reject_alias_components(path)
    for parent in [path, *path.parents]:
        metadata = parent.lstat()
        if parent.is_symlink() or getattr(metadata, 'st_file_attributes', 0) & 0x400:
            raise ValueError('Reparse/symlink refused: ' + str(parent))
    if not stat.S_ISREG(path.stat().st_mode):
        raise ValueError('Non-regular source refused')
    if path.stat().st_size > 2 * 1024 * 1024:
        raise ValueError('Source bound exceeded')
    return path.read_bytes()

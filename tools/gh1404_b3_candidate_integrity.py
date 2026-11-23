#!/usr/bin/env python3
"""Independent B-3 source-only archive, lock and installed-byte observer.

This reviewer check does not select qualified Host/Bun/provider inputs, compute
the shipping graph hash, or authorize release. It requires a source-preserving
candidate lock and an actual offline-installed tree prepared outside the repo.
It checks supplied archive bytes, not independent producer executions.
Installed-byte observations are repeated, not an atomic filesystem snapshot.
"""

import argparse
import base64
from contextlib import contextmanager
from hashlib import sha1, sha256, sha512
from io import BytesIO
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tarfile
import zlib


MAX_ARCHIVE = 32 * 1024 * 1024
# Separate envelope for tar headers, extended metadata and record padding.
MAX_TAR_STREAM = 2 * MAX_ARCHIVE
MAX_MEMBER = 8 * 1024 * 1024
MAX_MEMBERS = 1000
MAX_TAR_METADATA = 256 * 1024
FIXTURE = "crates/keld-cli/tests/fixtures/gh1404-minimal-app"
SENTINELS = (
    "UNQUALIFIED_HOST_SHA256",
    "UNQUALIFIED_BUN_SHA256",
    "UNQUALIFIED_PROVIDER_API",
    "UNQUALIFIED_PROVIDER_ELECTRON",
    "UNQUALIFIED_PROVIDER_KIPC",
)
SELECTED_FILES = (
    FIXTURE + "/package.json",
    FIXTURE + "/keld.build.ts",
    "packages/@keld/kipc/scripts/package-artifacts.ts",
    "packages/@keld/api/package.json",
    "packages/@keld/kipc/package.json",
    "packages/@keld/electron/package.json",
)
DIRECTORY_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
FILE_FLAGS = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
INSTALLED_SCOPES = (
    ("node_modules", {"@keld", "@types", "electron", "undici-types"}, {".package-lock.json"}),
    ("node_modules/@keld", {"api", "kipc"}, set()),
    ("node_modules/@types", {"node"}, set()),
)


def require(condition, description):
    if not condition:
        raise ValueError(description)


def require_json_object(value, label):
    require(isinstance(value, dict), label + ": expected a JSON object")
    return value


def read_json(content, label):
    return require_json_object(json.loads(content), label)


class RootCustody:
    """Retain a no-follow descriptor chain from an OS anchor to one caller root."""

    def __init__(self, path, label):
        self.label = label
        self.path = os.fspath(path)
        self._fds = []
        self._components = []
        require(isinstance(self.path, str) and self.path and "\x00" not in self.path,
                label + ": unsafe directory root path")
        parsed = Path(self.path)
        anchor = parsed.anchor if parsed.is_absolute() else "."
        parts = parsed.parts[1:] if parsed.is_absolute() else parsed.parts
        require(anchor != "//" and all(part not in {"", ".", ".."} for part in parts),
                label + ": unsafe directory root traversal")
        try:
            self._fds.append(os.open(anchor, DIRECTORY_FLAGS))
            for part in parts:
                parent = self._fds[-1]
                before = os.stat(part, dir_fd=parent, follow_symlinks=False)
                require(stat.S_ISDIR(before.st_mode),
                        label + ": unsafe directory root component " + part)
                opened = os.open(part, DIRECTORY_FLAGS, dir_fd=parent)
                self._fds.append(opened)
                self._components.append(part)
                after = os.fstat(opened)
                require((before.st_dev, before.st_ino) ==
                        (after.st_dev, after.st_ino),
                        label + ": unsafe directory root replaced during open")
            self.verify()
        except OSError as error:
            self.close()
            raise ValueError(label + ": unsafe directory root: " + str(error)) from error
        except BaseException:
            self.close()
            raise

    @property
    def fd(self):
        return self._fds[-1]

    def verify(self):
        """Reject renamed or rebound ancestors while keeping the opened handles."""
        try:
            for index, component in enumerate(self._components):
                entry = os.stat(
                    component, dir_fd=self._fds[index], follow_symlinks=False,
                )
                pinned = os.fstat(self._fds[index + 1])
                require(stat.S_ISDIR(entry.st_mode) and
                        (entry.st_dev, entry.st_ino) == (pinned.st_dev, pinned.st_ino),
                        self.label + ": unsafe directory root replaced: " + component)
        except OSError as error:
            raise ValueError(
                self.label + ": unsafe directory root changed: " + str(error),
            ) from error

    def close(self):
        for fd in reversed(self._fds):
            os.close(fd)
        self._fds.clear()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


@contextmanager
def held_root(root, label):
    if isinstance(root, RootCustody):
        root.verify()
        yield root
    else:
        with RootCustody(root, label) as pinned:
            yield pinned


def _git_run(source, *args, text):
    # The source path might be replaced between admission and Git. Run Git in
    # the opened directory's cwd, not in a second pathname-based git -C.
    with held_root(source, "source root") as pinned:
        require(hasattr(os, "fchdir") and os.name == "posix",
                "source root: directory-descriptor Git unavailable")
        # Ambient Git environment can redirect worktree, index and objects
        # despite the descriptor-pinned cwd. Trust the root and repository data.
        git_environment = {
            key: value for key, value in os.environ.items()
            if not key.startswith("GIT_")
        }
        # Source checks must not execute global Git helpers before admission.
        # Local fsmonitor and hooks are also disabled. The checked-out source
        # remains the data boundary; global user preferences are not inputs.
        git_environment["GIT_CONFIG_NOSYSTEM"] = "1"
        git_environment["GIT_CONFIG_GLOBAL"] = os.devnull

        def enter_pinned_source():
            os.fchdir(pinned.fd)
            os.close(pinned.fd)

        result = subprocess.run(
            ["git", "--no-replace-objects", "-c", "core.fsmonitor=false",
             "-c", "core.hooksPath=/dev/null",
             "-c", "maintenance.auto=false", *args],
            capture_output=True, text=text, check=False,
            preexec_fn=enter_pinned_source, pass_fds=(pinned.fd,),
            env=git_environment,
        )
        pinned.verify()
        return result


def git(source, *args):
    completed = _git_run(source, *args, text=True)
    require(completed.returncode == 0,
            "source Git identity or tree mismatch: " + " ".join(args))
    return completed.stdout.strip()


def immutable_selected_bytes(source, selected, relative):
    # Capture a selected Git blob once, independently of the mutable worktree.
    result = _git_run(source, "show", selected + ":" + relative, text=False)
    require(result.returncode == 0, "selected source fixture Git blob missing: " + relative)
    return result.stdout


def git_bytes(source, *args):
    result = _git_run(source, *args, text=False)
    require(result.returncode == 0,
            "source Git identity or tree mismatch: " + " ".join(args))
    return result.stdout


def selected_source_packages_unchanged(source, selected):
    """Compare Git tree/index identities to raw physical bytes, without filters.

    Git status and working-tree diff can execute configured clean filters. Those
    helpers and local attributes are unqualified inputs to the source observer.
    Read selected blob IDs from Git's immutable tree, and read each physical
    file through the retained no-follow source root. Do not ask Git to hash a
    worktree file or run an external diff helper.
    """
    changed = "source package files have unpublished edits"
    require(git(source, "rev-parse", "--show-object-format") == "sha1",
            "source Git object format outside SHA-1 contract")
    tree = {}
    records = git_bytes(source, "ls-tree", "-r", "-z", "--full-tree",
                        selected, "--", "packages/@keld")
    for record in records.split(b"\0"):
        if not record:
            continue
        metadata, tab, raw_path = record.partition(b"\t")
        fields = metadata.split(b" ")
        require(bool(tab) and len(fields) == 3 and fields[1] == b"blob" and
                fields[0] in {b"100644", b"100755"} and
                re.fullmatch(rb"[0-9a-f]{40}", fields[2]) is not None,
                changed + ": unsupported selected tree entry")
        try:
            path = raw_path.decode("utf-8", "strict")
        except UnicodeDecodeError as error:
            raise ValueError(changed + ": invalid source path") from error
        require(path.startswith("packages/@keld/") and
                path not in tree and "\\" not in path and
                all(part not in {"", ".", ".."} for part in path.split("/")),
                changed + ": unsafe selected package path")
        tree[path] = (fields[0], fields[2])
    require(0 < len(tree) <= 10000, changed + ": source file census outside bound")

    staged = {}
    records = git_bytes(source, "ls-files", "--stage", "-z", "--", "packages/@keld")
    for record in records.split(b"\0"):
        if not record:
            continue
        metadata, tab, raw_path = record.partition(b"\t")
        fields = metadata.split(b" ")
        require(bool(tab) and len(fields) == 3 and fields[2] == b"0" and
                re.fullmatch(rb"[0-9a-f]{40}", fields[1]) is not None,
                changed + ": unmerged or invalid index entry")
        try:
            path = raw_path.decode("utf-8", "strict")
        except UnicodeDecodeError as error:
            raise ValueError(changed + ": invalid index path") from error
        require(path not in staged, changed + ": duplicate index path")
        staged[path] = (fields[0], fields[1])
    require(staged == tree, changed + ": selected index differs from commit")
    # Physical package inventory is the authority for untracked files. Do
    # not consult Git's mutable core.worktree or ignore rules for this census:
    # a local config symlink could redirect `git ls-files --others` between
    # root admission and the later Git child execution.
    physical_files, _ = installed_inventory(source, "packages/@keld", changed)
    require(physical_files == {
        path.removeprefix("packages/@keld/") for path in tree
    }, changed + ": extra or absent physical provider source files")
    total = 0
    for path, (mode, object_id) in sorted(tree.items()):
        physical, _, file_mode = read_owned_file(source, path, changed + ": " + path)
        total += len(physical)
        require(total <= MAX_TAR_STREAM, changed + ": source bytes outside bound")
        actual = sha1(b"blob " + str(len(physical)).encode("ascii") + b"\0" + physical)
        require(actual.hexdigest().encode("ascii") == object_id and
                (b"100755" if file_mode & 0o111 else b"100644") == mode,
                changed + ": physical byte or executable mode changed: " + path)


def require_pinned_git_worktree(source):
    # Even without inherited GIT_* variables, a repository-local core.worktree
    # setting can redirect status/diff to another clean worktree. It must not
    # change the physical source tree selected through RootCustody.
    reported = git(source, "rev-parse", "--show-toplevel")
    path = source.path if isinstance(source, RootCustody) else os.fspath(source)
    try:
        matches = os.path.samefile(reported, path)
    except OSError:
        matches = False
    require(matches, "source Git worktree differs from pinned root")


def open_owned(root, relative, label, directory=False):
    """Open root-relative members from the pinned, component-checked root FD."""
    parts = Path(relative).parts
    require(parts and not Path(relative).is_absolute() and
            all(part not in {"", ".", ".."} for part in parts),
            label + ": unsafe relative path")
    with held_root(root, label) as pinned:
        parent = os.dup(pinned.fd)
        try:
            for index, component in enumerate(parts):
                is_dir = directory or index < len(parts) - 1
                try:
                    opened = os.open(
                        component, DIRECTORY_FLAGS if is_dir else FILE_FLAGS,
                        dir_fd=parent,
                    )
                except OSError as error:
                    kind = "directory" if is_dir else "file"
                    raise ValueError(
                        label + ": unsafe " + kind + " component " + component +
                        ": " + str(error),
                    ) from error
                os.close(parent)
                parent = opened
            pinned.verify()
            return parent
        except BaseException:
            os.close(parent)
            raise


def read_owned_file(root, relative, label):
    with held_root(root, label) as pinned:
        result = _read_owned_file(pinned, relative, label)
        pinned.verify()
        return result


def _read_owned_file(root, relative, label):
    """Return a stable copy and inode identity, rejecting external hardlinks."""
    fd = open_owned(root, relative, label)
    with os.fdopen(fd, "rb") as opened:
        before = os.fstat(opened.fileno())
        require(stat.S_ISREG(before.st_mode), label + ": not a regular file")
        require(before.st_nlink == 1, label + ": uncustodied hardlink")
        require(before.st_size <= MAX_ARCHIVE, label + ": file too large")
        content = opened.read(MAX_ARCHIVE + 1)
        after = os.fstat(opened.fileno())
    def identity(row):
        return (
            row.st_dev, row.st_ino, row.st_size, row.st_mtime_ns, row.st_ctime_ns,
            row.st_mode, row.st_nlink,
        )
    require(identity(before) == identity(after) and len(content) == before.st_size,
            label + ": file changed while reading")
    return content, identity(after), stat.S_IMODE(after.st_mode)


def installed_inventory(root, location, label):
    with held_root(root, label) as pinned:
        result = _installed_inventory(pinned, location, label)
        pinned.verify()
        return result


def _installed_inventory(root, location, label):
    """Inventory installed files and directories through no-follow directory FDs."""
    found = set()
    directories = {}

    def visit(fd, prefix):
        for item in os.listdir(fd):
            require(item not in {"", ".", ".."}, label + ": unsafe directory member")
            relative = prefix + item
            info = os.stat(item, dir_fd=fd, follow_symlinks=False)
            if stat.S_ISDIR(info.st_mode):
                mode = stat.S_IMODE(info.st_mode)
                require(mode & 0o022 == 0, label + ": unsafe installed directory mode")
                directories[relative] = mode
                child = os.open(item, DIRECTORY_FLAGS, dir_fd=fd)
                try:
                    visit(child, relative + "/")
                finally:
                    os.close(child)
            else:
                require(stat.S_ISREG(info.st_mode), label + ": installed member linked or special")
                found.add(relative)
            require(len(found) + len(directories) <= MAX_MEMBERS,
                    label + ": excessive installed inventory")

    fd = open_owned(root, location, label, directory=True)
    try:
        root_mode = stat.S_IMODE(os.fstat(fd).st_mode)
        require(root_mode & 0o022 == 0, label + ": unsafe installed directory mode")
        visit(fd, "")
    finally:
        os.close(fd)
    return found, directories


def installed_package_closure(candidate):
    with held_root(candidate, "candidate root") as pinned:
        result = _installed_package_closure(pinned)
        pinned.verify()
        return result


def _installed_package_closure(candidate):
    """Reject every unselected package name, including linked and hidden entries."""
    inventory = {}
    for location, directories, optional_files in INSTALLED_SCOPES:
        label = "installed package scope " + location
        fd = open_owned(candidate, location, label, directory=True)
        try:
            root = os.fstat(fd)
            mode = stat.S_IMODE(root.st_mode)
            require(stat.S_ISDIR(root.st_mode) and mode & ~0o777 == 0 and
                    mode & 0o022 == 0 and root.st_uid == os.geteuid(),
                    label + ": unsafe directory mode or ownership")
            names = set(os.listdir(fd))
            require(names >= directories and names <= directories | optional_files,
                    label + ": unexpected installed package or missing selected package: " +
                    ",".join(sorted(names ^ directories)[:12]))
            entries = {}
            for name in sorted(names):
                info = os.stat(name, dir_fd=fd, follow_symlinks=False)
                mode = stat.S_IMODE(info.st_mode)
                allowed_kind = (stat.S_ISDIR(info.st_mode) if name in directories
                                else stat.S_ISREG(info.st_mode) and info.st_nlink == 1)
                require(allowed_kind and info.st_dev == root.st_dev and
                        info.st_uid == root.st_uid and mode & ~0o777 == 0 and
                        mode & 0o022 == 0,
                        label + ": unsafe installed " +
                        ("directory mode or metadata: " if name in directories
                         else "file mode or metadata: ") + name)
                entries[name] = (
                    info.st_dev, info.st_ino, info.st_mode, info.st_nlink,
                    info.st_uid, info.st_mtime_ns, info.st_ctime_ns,
                )
            inventory[location] = (root.st_dev, root.st_ino, root.st_mode, entries)
        finally:
            os.close(fd)
    return inventory


def iter_tar_members(archive, label):
    members = iter(archive)
    while True:
        try:
            member = next(members)
        except StopIteration:
            return
        except RecursionError as error:
            raise ValueError(label + ": tar metadata nesting outside bound") from error
        yield member


def require_tar_padding(data, payload_end, label):
    padded_end = (payload_end + 511) // 512 * 512
    require(0 <= payload_end <= padded_end <= len(data),
            label + ": incomplete tar payload padding")
    require(not any(data[payload_end:padded_end]),
            label + ": nonzero tar payload padding")


class BoundedTarInfo(tarfile.TarInfo):
    """Count actual parsed TAR headers, including invisible PAX metadata."""

    @classmethod
    def fromtarfile(cls, archive):
        # Older tarfile versions dispatch both initial and nested reads here.
        # Patched versions delegate to _fromtarfile, including nested reads.
        if not hasattr(tarfile.TarInfo, "_fromtarfile"):
            cls.observe_header(archive)
        return super().fromtarfile(archive)

    @classmethod
    def _fromtarfile(cls, archive, *, dircheck=True):
        cls.observe_header(archive)
        return super()._fromtarfile(archive, dircheck=dircheck)

    @staticmethod
    def observe_header(archive):
        # PAX records are not returned to TarFile iteration, so the visible
        # member count alone does not account for physical headers.
        # Permit one final read for the TAR end marker.
        count = getattr(archive, "_b3_physical_header_reads", 0) + 1
        require(count <= MAX_MEMBERS + 1,
                "archive physical tar headers outside bound")
        archive._b3_physical_header_reads = count
        raw = archive.fileobj.getvalue()
        header_start = archive.fileobj.tell()
        require(header_start == getattr(archive, "_b3_next_header", 0) and
                getattr(archive, "_b3_end_header", None) is None,
                "archive physical tar header observations are discontinuous")
        kind = bytes(raw[header_start + 156:header_start + 157])
        # Python's GNU sparse reader consumes arbitrary 512-byte extension
        # blocks without calling fromtarfile; malformed ones can raise
        # IndexError. A source-only npm package has no sparse file members.
        require(kind != tarfile.GNUTYPE_SPARSE,
                "archive GNU sparse members unsupported")
        pax_types = (tarfile.XHDTYPE, tarfile.XGLTYPE, tarfile.SOLARIS_XHDTYPE)
        metadata_types = pax_types + (tarfile.GNUTYPE_LONGNAME,
                                      tarfile.GNUTYPE_LONGLINK)
        # A PAX/GNU override cannot authorize an unsafe physical header path.
        # Metadata pseudoheaders are supported only in the producer formats
        # independently exercised by the first-party npm and PAX/GNU tests.
        physical_header = bytes(raw[header_start:header_start + 512])
        require(physical_header, "archive incomplete tar end marker")
        require(len(physical_header) == 512, "archive incomplete raw tar header")
        if not any(physical_header):
            archive._b3_end_header = header_start
        if any(physical_header):
            require(not any(physical_header[500:512]),
                    "archive raw tar header reserved bytes nonzero")
            for field in (physical_header[:100], physical_header[345:500]):
                ending = field.find(0)
                require(ending < 0 or not any(field[ending + 1:]),
                        "archive raw tar member name padding nonzero")
            physical_member = tarfile.TarInfo.frombuf(
                physical_header, archive.encoding, archive.errors)
            require(physical_member.size >= 0, "archive invalid raw tar member size")
            archive._b3_next_header = (
                header_start + 512 + ((physical_member.size + 511) // 512) * 512
            )
            physical_path = physical_member.name
            if kind in pax_types:
                npm_pax = kind == tarfile.XHDTYPE and physical_path.startswith("PaxHeader/")
                if npm_pax:
                    # npm 11 creates PaxHeader/<basename>, truncating to fit
                    # the 100-byte raw TAR name field. The next physical leaf
                    # must match that exact prefix before PAX rewrites its path.
                    leaf = physical_path[len("PaxHeader/"):]
                    try:
                        leaf_octets = leaf.encode("utf-8", "strict")
                    except UnicodeEncodeError as error:
                        raise ValueError("unsafe archive raw tar metadata path") from error
                    # npm uses UTF-8 package filenames, not only ASCII. Keep
                    # the physical prefix limited to one safe printable leaf.
                    require(leaf and leaf not in {".", ".."} and
                            0 < len(leaf_octets) <= 89 and
                            all(char.isprintable() and char not in "/\\" for char in leaf) and
                            getattr(archive, "_b3_npm_pax_leaf", None) is None,
                            "unsafe archive raw tar metadata path")
                    archive._b3_npm_pax_leaf = leaf
                else:
                    require(physical_path == "././@PaxHeader",
                            "unsafe archive raw tar metadata path")
            elif kind in (tarfile.GNUTYPE_LONGNAME, tarfile.GNUTYPE_LONGLINK):
                # GNU long-name records ignore the USTAR prefix field. It must
                # not carry a hidden path or metadata for another consumer.
                require(physical_path == "././@LongLink" and
                        not any(physical_header[345:500]),
                        "unsafe archive raw tar metadata path")
            else:
                components = physical_path.split("/")
                safe_root = physical_path == "package" and physical_member.isdir()
                safe_child = (
                    len(components) > 1 and components[0] == "package" and
                    all(part not in {"", ".", ".."} for part in components[1:]) and
                    all(ord(char) not in (0, 92) for char in physical_path)
                )
                require(safe_root or safe_child,
                        "unsafe archive raw tar member path")
                npm_leaf = getattr(archive, "_b3_npm_pax_leaf", None)
                if npm_leaf is not None:
                    basename = physical_path.rsplit("/", 1)[-1]
                    npm_leaf_octets = len(npm_leaf.encode("utf-8", "strict"))
                    require(basename.startswith(npm_leaf) and
                            (basename == npm_leaf or npm_leaf_octets == 89),
                            "unsafe archive raw tar metadata path")
                    npm_path = getattr(archive, "_b3_npm_pax_path", None)
                    name_field_len = len(physical_header[:100].split(b"\0", 1)[0])
                    require(npm_path is not None and
                            (npm_path == physical_path or
                             (npm_leaf_octets == 89 and name_field_len >= 89 and
                              npm_path.startswith(physical_path))),
                            "npm PAX path disagrees with physical tar header")
                    archive._b3_npm_pax_leaf = None
                    archive._b3_npm_pax_path = None
                local_size = getattr(archive, "_b3_local_pax_size", None)
                global_size = getattr(archive, "_b3_global_pax_size", None)
                for required_size in (local_size, global_size):
                    if required_size is not None:
                        require(physical_member.size == required_size,
                                "PAX size differs from physical tar header "
                                "(PAX size override unsupported)")
                archive._b3_local_pax_size = None
        if kind in metadata_types:
            size = tarfile.nti(bytes(raw[header_start + 124:header_start + 136]))
            total = getattr(archive, "_b3_metadata_bytes", 0) + size
            require(0 <= size <= MAX_TAR_METADATA and total <= MAX_TAR_METADATA,
                    "archive extended tar metadata outside bound")
            archive._b3_metadata_bytes = total
            begin = header_start + 512
            end = begin + size
            require_tar_padding(raw, end, "archive extended tar metadata")
            payload = bytes(raw[begin:end])
            if kind in pax_types:
                # A GNU sparse PAX map can consume additional blocks inside
                # tarfile without incrementing the header-read counter.
                pos = 0
                while pos < len(payload):
                    space = payload.find(b" ", pos, min(pos + 10, len(payload)))
                    require(space > pos and payload[pos:space].isdigit(),
                            "archive invalid PAX record")
                    length = int(payload[pos:space])
                    require(length >= 5 and pos + length <= len(payload) and
                            payload[pos + length - 1] == 10,
                            "archive invalid PAX record")
                    record = payload[space + 1:pos + length - 1]
                    key, separator, value = record.partition(b"=")
                    require(bool(separator) and bool(key), "archive invalid PAX record")
                    require(not key.startswith(b"GNU.sparse."),
                            "archive GNU sparse PAX metadata unsupported")
                    if key == b"path" and kind == tarfile.XHDTYPE and getattr(
                            archive, "_b3_npm_pax_leaf", None) is not None:
                        require(getattr(archive, "_b3_npm_pax_path", None) is None,
                                "archive duplicate npm PAX path")
                        try:
                            archive._b3_npm_pax_path = value.decode("utf-8", "strict")
                        except UnicodeDecodeError as error:
                            raise ValueError("archive non-UTF8 npm PAX path") from error
                    if key == b"size":
                        # An npm PAX size is metadata, never authority to
                        # extend/reframe the physical following TAR member.
                        require(value.isdigit() and (value == b"0" or not value.startswith(b"0")),
                                "archive noncanonical PAX size")
                        claimed = int(value)
                        require(0 <= claimed <= MAX_MEMBER,
                                "archive PAX size outside member bound")
                        if kind == tarfile.XGLTYPE:
                            require(getattr(archive, "_b3_global_pax_size", None) is None,
                                    "archive duplicate PAX size")
                            archive._b3_global_pax_size = claimed
                        else:
                            require(getattr(archive, "_b3_local_pax_size", None) is None,
                                    "archive duplicate PAX size")
                            archive._b3_local_pax_size = claimed
                    pos += length
                    records = getattr(archive, "_b3_pax_records", 0) + 1
                    require(records <= MAX_MEMBERS, "archive excessive PAX records")
                    archive._b3_pax_records = records
                if getattr(archive, "_b3_npm_pax_leaf", None) is not None:
                    require(getattr(archive, "_b3_npm_pax_path", None) is not None,
                            "archive npm PAX path metadata missing")
            else:
                nul = payload.find(b"\x00")
                require(nul < 0 or not any(payload[nul + 1:]),
                        "archive GNU longname hidden bytes")


def archive_members(data, label):
    require(0 < len(data) <= MAX_ARCHIVE, label + ": archive size outside bound")
    files = {}
    directories = {}
    seen = {}
    # Bound the complete expanded stream before tarfile can read metadata that
    # is not returned as an ordinary member.
    # Decode exactly one complete gzip member. GzipFile.read() silently joins
    # concatenated members, while tarfile ignores the unaccounted later bytes.
    try:
        decoder = zlib.decompressobj(wbits=16 + zlib.MAX_WBITS)
        tar_bytes = decoder.decompress(data, MAX_TAR_STREAM + 1)
    except zlib.error as error:
        raise ValueError(label + ": invalid gzip archive") from error
    require(0 < len(tar_bytes) <= MAX_TAR_STREAM, label + ": expanded tar stream outside bound")
    require(decoder.eof, label + ": incomplete gzip member")
    require(not decoder.unused_data, label + ": trailing gzip member or bytes")
    total = 0
    member_count = 0
    try:
        archive = tarfile.open(fileobj=BytesIO(tar_bytes), mode="r:",
                               tarinfo=BoundedTarInfo)
    except RecursionError as error:
        raise ValueError(label + ": tar metadata nesting outside bound") from error
    with archive:
        for member in iter_tar_members(archive, label):
            member_count += 1
            require(member_count <= MAX_MEMBERS, label + ": archive member count outside bound")
            if member.isdir() and member.name in {"package", "package/"}:
                require(member.mode & ~0o777 == 0 and member.mode & 0o022 == 0,
                        label + ": unsafe archive root directory mode")
                require(member.size == 0, label + ": archive root directory has data")
                continue
            require(member.name.startswith("package/"), label + ": archive escaped package root")
            require("\\" not in member.name and "\x00" not in member.name,
                    label + ": unsafe archive member path")
            normalized = member.name[:-1] if member.isdir() and member.name.endswith("/") else member.name
            parts = normalized.split("/")[1:]
            require(bool(parts) and all(p not in {"", ".", ".."} for p in parts),
                    label + ": unsafe archive member path")
            relative = "/".join(parts)
            kind = "directory" if member.isdir() else "file"
            if relative in seen:
                require(seen[relative] == kind, label + ": archive file/directory collision")
                raise ValueError(label + ": duplicate archive member")
            seen[relative] = kind
            if member.isdir():
                require(member.mode & ~0o777 == 0 and member.mode & 0o022 == 0,
                        label + ": unsafe archive directory mode")
                require(member.size == 0, label + ": archive directory has data")
                directories[relative] = member.mode & 0o777
                continue
            require(member.isfile(), label + ": non-regular archive member")
            require(not member.name.endswith("/"), label + ": unsafe archive file path")
            require(0 <= member.size <= MAX_MEMBER, label + ": oversized archive member")
            require(member.mode & ~0o777 == 0, label + ": invalid archive member mode")
            require(total + member.size <= MAX_ARCHIVE, label + ": excessive expanded bytes")
            require_tar_padding(tar_bytes, member.offset_data + member.size,
                                label + " member " + member.name)
            opened = archive.extractfile(member)
            require(opened is not None, label + ": unreadable archive member")
            content = opened.read(MAX_MEMBER + 1)
            require(len(content) == member.size, label + ": truncated archive member")
            total += len(content)
            files[relative] = (content, member.mode & 0o777)
    require(getattr(archive, "_b3_end_header", None) == archive.offset,
            label + ": archive physical tar end header was not observed")
    require(archive.offset + 1024 <= len(tar_bytes) and
            tar_bytes[archive.offset:archive.offset + 1024] == bytes(1024),
            label + ": incomplete tar end marker")
    require(len(tar_bytes.rstrip(bytes([0]))) <= archive.offset,
            label + ": trailing nonzero tar data")
    require(member_count > 0, label + ": archive member count outside bound")
    # The archive may omit explicit parent entries, but every file parent must
    # exist in the installed tree. Empty explicit directories must also exist.
    for relative in [*files, *directories]:
        parts = relative.split("/")
        for index in range(1, len(parts)):
            parent = "/".join(parts[:index])
            require(parent not in files, label + ": archive file/directory collision")
            directories.setdefault(parent, None)
    require("package.json" in files, label + ": missing package.json")
    return files, directories, member_count


def compare_installed(candidate, location, files, archive_directories, label):
    with held_root(candidate, "candidate root") as pinned:
        result = _compare_installed(pinned, location, files, archive_directories, label)
        pinned.verify()
        return result


def _compare_installed(candidate, location, files, archive_directories, label):
    found, actual_directories = installed_inventory(candidate, location, label)
    require(found == set(files), label + ": unexpected or absent installed members: " +
            ",".join(sorted(found ^ set(files))[:12]))
    require(set(actual_directories) == set(archive_directories),
            label + ": unexpected or absent installed directories: " +
            ",".join(sorted(set(actual_directories) ^ set(archive_directories))[:12]))
    for name, mode in archive_directories.items():
        if mode is not None:
            require(actual_directories[name] == mode,
                    label + ": installed directory mode changed: " + name)
    rows = []
    member_observations = {}
    # Core graph_contract.rs:1922-1948 owns the canonical provider fingerprint.
    # This independent reader computes the same *candidate* data over physical
    # installed file bytes; a matching value is not an owner approval or grant.
    core_fingerprint = sha256()
    for name, (archive_bytes, expected_mode) in sorted(
        files.items(), key=lambda item: item[0].encode("utf8")
    ):
        physical, identity, mode = read_owned_file(
            candidate, location + "/" + name, label + " installed " + name,
        )
        require(physical == archive_bytes, label + ": installed member bytes changed: " + name)
        require(mode == expected_mode, label + ": installed member mode changed: " + name)
        file_digest = sha256(physical).digest()
        member_observations[name] = (file_digest, identity)
        rows.append({"path": name, "sha256": file_digest.hex(), "mode": oct(mode)})
        # Core's sorted GraphDocument row algorithm (no file-mode bytes):
        # relative UTF-8 path, decimal length and *raw* SHA-256, each NUL-terminated.
        core_fingerprint.update(name.encode("utf8") + b"\0")
        core_fingerprint.update(str(len(physical)).encode("ascii") + b"\0")
        core_fingerprint.update(file_digest + b"\0")
    # A reviewer-defined file/mode witness, NOT Keld's shipping provider-tree digest.
    witness = sha256(json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return witness, core_fingerprint.hexdigest(), (member_observations, actual_directories)


def recheck_installed(candidate, location, observation, label):
    """Reobserve the known bounded member set without retaining archive bodies."""
    members, directories = observation
    found, current_directories = installed_inventory(candidate, location, label)
    require(found == set(members) and current_directories == directories,
            label + ": installed member inventory changed during verification")
    for name, (expected_digest, expected_identity) in members.items():
        physical, identity, _ = read_owned_file(
            candidate, location + "/" + name, label + " installed " + name,
        )
        require(identity == expected_identity and sha256(physical).digest() == expected_digest,
                label + ": installed member changed during verification: " + name)


def require_supplied_archive_match(packed, packed_identity, comparison, comparison_identity, name):
    # The reader returns device and inode first. Other metadata can change
    # and must not make one physical file count as two inputs.
    require(packed_identity[:2] != comparison_identity[:2],
            name + ": candidate and comparison archives are the same physical file")
    require(packed == comparison, name + ": supplied archive bytes differ")


def validate(source, selected, candidate, repro):
    # Root admission precedes Git and any physical input access. Keep these
    # directory chains live through all selected bytes, lock/SRI and receipts.
    with (RootCustody(source, "source root") as selected_root,
          RootCustody(candidate, "candidate root") as candidate_root,
          RootCustody(repro, "repro root") as repro_root):
        receipt = _validate_owned(
            selected_root, selected, candidate_root, repro_root,
        )
        for root in (selected_root, candidate_root, repro_root):
            root.verify()
        return receipt


def _validate_owned(source, selected, candidate, repro):
    require(re.fullmatch("[0-9a-f]{40}", selected) is not None,
            "source revision must be an exact 40-hex commit")
    require_pinned_git_worktree(source)
    git(source, "rev-parse", "--verify", selected + "^{commit}")
    git(source, "merge-base", "--is-ancestor", selected, "HEAD")
    # Source identity must never execute a configured external Git diff helper.
    git(source, "diff", "--no-ext-diff", "--quiet", selected, "HEAD", "--", "packages/@keld")
    selected_source_packages_unchanged(source, selected)
    # These Git blobs are captured exactly once for this invocation. The
    # physical tree must match both initially and at the final receipt boundary;
    # no selected source digest or sentinel is sourced from a later path read.
    immutable = {}
    initial_identities = {}
    for relative in SELECTED_FILES:
        selected_bytes = immutable_selected_bytes(source, selected, relative)
        physical, identity, _ = read_owned_file(
            source, relative, "selected source " + relative,
        )
        require(
            physical == selected_bytes,
            "selected source fixture changed after selected commit: " + relative,
        )
        immutable[relative] = selected_bytes
        initial_identities[relative] = identity
    selected_package = immutable[FIXTURE + "/package.json"]
    selected_build = immutable[FIXTURE + "/keld.build.ts"]
    require(all(marker in selected_build.decode("utf8") for marker in SENTINELS),
            "source fixture no longer has the five UNQUALIFIED sentinels")
    project_package, _, _ = read_owned_file(candidate, "package.json", "candidate source manifest")
    require(project_package == selected_package,
            "candidate package.json differs from selected source")
    manifest = read_json(project_package, "candidate source manifest")
    lock_content, _, _ = read_owned_file(candidate, "package-lock.json", "candidate lock")
    lock = read_json(lock_content, "candidate lock")
    require(lock.get("lockfileVersion") == 3 and lock.get("requires") is True,
            "candidate lock is not npm lock v3")
    require(lock.get("name") == manifest.get("name") and lock.get("version") == manifest.get("version"),
            "candidate lock identity differs from source")
    rows = lock.get("packages")
    require(isinstance(rows, dict) and isinstance(rows.get(""), dict),
            "candidate lock lacks root row")
    require(rows[""].get("dependencies") == manifest.get("dependencies"),
            "candidate lock root dependencies disagree with source alias")

    api_path = "packages/@keld/api/package.json"
    kipc_path = "packages/@keld/kipc/package.json"
    electron_path = "packages/@keld/electron/package.json"
    api = read_json(immutable[api_path], api_path)
    kipc = read_json(immutable[kipc_path], kipc_path)
    electron = read_json(immutable[electron_path], electron_path)
    node_version = electron["dependencies"]["@types/node"]
    undici_row = require_json_object(
        rows.get("node_modules/undici-types", {}), "candidate undici-types lock row",
    )
    undici_version = undici_row.get("version")
    require(isinstance(undici_version, str) and re.fullmatch(r"8\.3\.(?:0|[1-9][0-9]*)", undici_version),
            "candidate undici-types version outside selected type closure")
    expected = [
        ("@keld/api", "node_modules/@keld/api", f"keld-api-{api['version']}.tgz",
         api["version"],
         {"@keld/kipc": kipc["version"]}),
        ("@keld/kipc", "node_modules/@keld/kipc", f"keld-kipc-{kipc['version']}.tgz",
         kipc["version"], {}),
        ("@keld/electron", "node_modules/electron",
         f"keld-electron-{electron['version']}.tgz", electron["version"],
         {"@keld/api": api["version"], "@keld/kipc": kipc["version"],
          "@types/node": node_version}),
        ("@types/node", "node_modules/@types/node", f"types-node-{node_version}.tgz",
         node_version, {"undici-types": "~8.3.0"}),
        ("undici-types", "node_modules/undici-types",
         f"undici-types-{undici_version}.tgz", undici_version, {}),
    ]
    require(set(rows) == {"", *(location for _, location, _, _, _ in expected)},
            "candidate lock has missing or foreign package rows")
    require(manifest.get("dependencies") == {
        "@keld/api": api["version"], "@keld/kipc": kipc["version"],
        "electron": "npm:@keld/electron@" + electron["version"],
    }, "candidate source alias is not the closed first-party selector")

    installed_closure = installed_package_closure(candidate)
    installed_observations = []
    receipts = []
    for name, location, filename, source_version, dependencies in expected:
        row = require_json_object(rows[location], name + " lock row")
        lock_name = row.get("name", name)
        require(lock_name == name and row.get("version") == source_version,
                name + ": lock differs from selected source version")
        require(row.get("dependencies", {}) == dependencies,
                name + ": wrong installed dependency closure")
        require("hasInstallScript" not in row,
                name + ": lock contains unqualified install script metadata")
        require(row.get("resolved") == "file:vendor/" + filename,
                name + ": resolved path must select the exact local archive")
        packed, packed_identity, _ = read_owned_file(
            candidate, "vendor/" + filename, name + " archive",
        )
        comparison, comparison_identity, _ = read_owned_file(
            repro, filename, name + " supplied comparison archive",
        )
        require_supplied_archive_match(
            packed, packed_identity, comparison, comparison_identity, name,
        )
        sri = "sha512-" + base64.b64encode(sha512(packed).digest()).decode("ascii")
        require(row.get("integrity") == sri,
                name + ": lock integrity does not match actual archive")
        files, archive_directories, archive_entry_count = archive_members(packed, name)
        packed_manifest = read_json(files["package.json"][0], name + " archive manifest")
        require(packed_manifest.get("name") == name and
                packed_manifest.get("version") == row["version"],
                name + ": archive manifest does not match lock identity")
        require(packed_manifest.get("dependencies", {}) == dependencies,
                name + ": archive dependency closure differs from lock")
        if name.startswith("@keld/"):
            require("scripts" not in packed_manifest, name + ": package shipped lifecycle scripts")
            require(not any(
                "test-hooks" in member or ".test." in member or "/test/" in member
                for member in files
            ), name + ": release archive contains test-only source")
        if name == "@keld/kipc" and "transport.js" in files:
            transport = files["transport.js"][0]
            require(b"KELD_KIPC_TEST_HOOKS" not in transport and
                    b"openWorkerLinkForTest" not in transport,
                    "KIPC release transport retains test hooks")
        witness, candidate_core_tree, observation = compare_installed(
            candidate, location, files, archive_directories, name,
        )
        installed_observations.append((location, observation, name))
        row_receipt = {
            "name": name, "lockRow": location, "resolved": row["resolved"],
            "archiveSha256": sha256(packed).hexdigest(), "archiveSha512Integrity": sri,
            "memberCount": archive_entry_count, "installedByteModeWitness": witness,
        }
        if name in {"@keld/electron", "@keld/api", "@keld/kipc"}:
            row_receipt["candidateCoreUnpackedTreeSha256"] = candidate_core_tree
        receipts.append(row_receipt)
    for relative, selected_bytes in immutable.items():
        physical, identity, _ = read_owned_file(
            source, relative, "selected source " + relative,
        )
        require(physical == selected_bytes and identity == initial_identities[relative],
                "selected source changed during verification: " + relative)
    # Complete the other package and source checks before reobservation, then
    # close with the outer scope census. This detects differing observations;
    # it cannot prove a tree stayed unchanged after each descriptor was closed.
    for location, observation, name in installed_observations:
        recheck_installed(candidate, location, observation, name)
    require(installed_package_closure(candidate) == installed_closure,
            "installed package scope changed during verification")
    return {
        "schema": "keld.gh1404-b3-source-receipt/v1",
        "status": "UNQUALIFIED_SOURCE_ONLY",
        "selectedSourceHead": selected,
        "producerSourceSha256": sha256(
            immutable["packages/@keld/kipc/scripts/package-artifacts.ts"]
        ).hexdigest(),
        "selectedManifestSha256": sha256(selected_package).hexdigest(),
        "candidateLockSha256": sha256(lock_content).hexdigest(),
        "allFivePinsRemainUnqualified": True,
        "archiveComparison": "SUPPLIED_BYTE_EQUIVALENCE_ONLY",
        "note": "Core-format provider digests are derived from unqualified physical input bytes; "
                "independent producer runs are not verified; independent source-owner "
                "selection, signed Host/Pack/Guard and installed "
                "graph admission remain unproved.",
        "packages": receipts,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", required=True, type=Path)
    parser.add_argument("--selected-revision", required=True)
    parser.add_argument("--candidate-root", required=True, type=Path)
    parser.add_argument(
        "--repro-dir", required=True, type=Path,
        help="directory of separately supplied comparison archives; not producer-run attestations",
    )
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        receipt = validate(
            args.source_root, args.selected_revision,
            args.candidate_root, args.repro_dir,
        )
        result = json.dumps(receipt, sort_keys=True, indent=2) + "\n"
        if args.output is not None:
            # The caller owns this path. Never truncate an existing receipt or
            # follow a final symlink to a different file.
            with args.output.open("x", encoding="utf8") as output:
                output.write(result)
    except (ValueError, OSError, UnicodeError, tarfile.TarError, KeyError, TypeError,
            json.JSONDecodeError) as error:
        print("B3_CANDIDATE_REFUSED: " + str(error), file=sys.stderr)
        return 2
    print(result, end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())

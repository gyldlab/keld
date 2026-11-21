#!/usr/bin/env python3
"""Independent B-3 source-only archive, lock and installed-byte observer.

This reviewer check does not select qualified Host/Bun/provider inputs, compute
the shipping graph hash, or authorize release. It requires a source-preserving
candidate lock and an actual offline-installed tree prepared outside the repo.
"""

import argparse
import base64
from hashlib import sha256, sha512
from io import BytesIO
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tarfile


MAX_ARCHIVE = 32 * 1024 * 1024
MAX_MEMBER = 8 * 1024 * 1024
MAX_MEMBERS = 1000
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


def read_json(content):
    return json.loads(content)


def git(source, *args):
    completed = subprocess.run(
        ["git", "-C", str(source), *args],
        capture_output=True, text=True, check=False,
    )
    require(completed.returncode == 0, "source Git identity or tree mismatch: " + " ".join(args))
    return completed.stdout.strip()


def immutable_selected_bytes(source, selected, relative):
    # Capture a selected Git blob once, independently of the mutable worktree.
    result = subprocess.run(
        ["git", "-C", str(source), "show", selected + ":" + relative],
        capture_output=True, check=False,
    )
    require(result.returncode == 0, "selected source fixture Git blob missing: " + relative)
    return result.stdout


def open_owned(root, relative, label, directory=False):
    """Pin every component beneath the named root without following links."""
    parts = Path(relative).parts
    require(parts and not Path(relative).is_absolute() and
            all(part not in {"", ".", ".."} for part in parts),
            label + ": unsafe relative path")
    try:
        parent = os.open(root, DIRECTORY_FLAGS)
    except OSError as error:
        raise ValueError(label + ": unsafe directory root: " + str(error)) from error
    try:
        for index, component in enumerate(parts):
            is_dir = directory or index < len(parts) - 1
            try:
                opened = os.open(
                    component, DIRECTORY_FLAGS if is_dir else FILE_FLAGS, dir_fd=parent,
                )
            except OSError as error:
                kind = "directory" if is_dir else "file"
                raise ValueError(
                    label + ": unsafe " + kind + " component " + component + ": " +
                    str(error),
                ) from error
            os.close(parent)
            parent = opened
        return parent
    except BaseException:
        os.close(parent)
        raise


def read_owned_file(root, relative, label):
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


def archive_members(data, label):
    require(0 < len(data) <= MAX_ARCHIVE, label + ": archive size outside bound")
    files = {}
    directories = {}
    seen = {}
    total = 0
    with tarfile.open(fileobj=BytesIO(data), mode="r:gz") as archive:
        members = archive.getmembers()
        require(0 < len(members) <= MAX_MEMBERS, label + ": archive member count outside bound")
        for member in members:
            if member.isdir() and member.name in {"package", "package/"}:
                require(member.mode & ~0o777 == 0 and member.mode & 0o022 == 0,
                        label + ": unsafe archive root directory mode")
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
            opened = archive.extractfile(member)
            require(opened is not None, label + ": unreadable archive member")
            content = opened.read(MAX_MEMBER + 1)
            require(len(content) == member.size, label + ": truncated archive member")
            total += len(content)
            require(total <= MAX_ARCHIVE, label + ": excessive expanded bytes")
            files[relative] = (content, member.mode & 0o777)
    # The archive may omit explicit parent entries, but every file parent must
    # exist in the installed tree. Empty explicit directories must also exist.
    for relative in [*files, *directories]:
        parts = relative.split("/")
        for index in range(1, len(parts)):
            parent = "/".join(parts[:index])
            require(parent not in files, label + ": archive file/directory collision")
            directories.setdefault(parent, None)
    require("package.json" in files, label + ": missing package.json")
    return files, directories, len(members)


def compare_installed(candidate, location, files, archive_directories, label):
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
    # Core graph_contract.rs:1922-1948 owns the canonical provider fingerprint.
    # This independent reader computes the same *candidate* data over physical
    # installed file bytes; a matching value is not an owner approval or grant.
    core_fingerprint = sha256()
    for name, (archive_bytes, expected_mode) in sorted(
        files.items(), key=lambda item: item[0].encode("utf8")
    ):
        physical, _, mode = read_owned_file(
            candidate, location + "/" + name, label + " installed " + name,
        )
        require(physical == archive_bytes, label + ": installed member bytes changed: " + name)
        require(mode == expected_mode, label + ": installed member mode changed: " + name)
        rows.append({"path": name, "sha256": sha256(physical).hexdigest(), "mode": oct(mode)})
        # Core's sorted GraphDocument row algorithm (no file-mode bytes):
        # relative UTF-8 path, decimal length and *raw* SHA-256, each NUL-terminated.
        core_fingerprint.update(name.encode("utf8") + b"\0")
        core_fingerprint.update(str(len(physical)).encode("ascii") + b"\0")
        core_fingerprint.update(sha256(physical).digest() + b"\0")
    # A reviewer-defined file/mode witness, NOT Keld's shipping provider-tree digest.
    witness = sha256(json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return witness, core_fingerprint.hexdigest()


def validate(source, selected, candidate, repro):
    require(re.fullmatch("[0-9a-f]{40}", selected) is not None,
            "source revision must be an exact 40-hex commit")
    git(source, "rev-parse", "--verify", selected + "^{commit}")
    git(source, "merge-base", "--is-ancestor", selected, "HEAD")
    git(source, "diff", "--quiet", selected, "HEAD", "--", "packages/@keld")
    require(not git(source, "status", "--porcelain", "--", "packages/@keld"),
            "source package files have unpublished edits")
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
    manifest = read_json(project_package)
    lock_content, _, _ = read_owned_file(candidate, "package-lock.json", "candidate lock")
    lock = read_json(lock_content)
    require(lock.get("lockfileVersion") == 3 and lock.get("requires") is True,
            "candidate lock is not npm lock v3")
    require(lock.get("name") == manifest.get("name") and lock.get("version") == manifest.get("version"),
            "candidate lock identity differs from source")
    rows = lock.get("packages")
    require(isinstance(rows, dict) and isinstance(rows.get(""), dict),
            "candidate lock lacks root row")
    require(rows[""].get("dependencies") == manifest.get("dependencies"),
            "candidate lock root dependencies disagree with source alias")

    api = read_json(immutable["packages/@keld/api/package.json"])
    kipc = read_json(immutable["packages/@keld/kipc/package.json"])
    electron = read_json(immutable["packages/@keld/electron/package.json"])
    node_version = electron["dependencies"]["@types/node"]
    undici_row = rows.get("node_modules/undici-types", {})
    undici_version = undici_row.get("version")
    require(isinstance(undici_version, str) and re.fullmatch(r"8\.3\.\d+", undici_version),
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
    receipts = []
    for name, location, filename, source_version, dependencies in expected:
        row = rows[location]
        lock_name = row.get("name", name)
        require(lock_name == name and row.get("version") == source_version,
                name + ": lock differs from selected source version")
        require(row.get("dependencies", {}) == dependencies,
                name + ": wrong installed dependency closure")
        require("hasInstallScript" not in row,
                name + ": lock contains unqualified install script metadata")
        require(row.get("resolved") == "file:vendor/" + filename,
                name + ": resolved path must select the exact local archive")
        packed, _, _ = read_owned_file(
            candidate, "vendor/" + filename, name + " archive",
        )
        reproduced, _, _ = read_owned_file(repro, filename, name + " reproduced archive")
        require(packed == reproduced,
                name + ": cross-CWD archive reproducibility mismatch")
        sri = "sha512-" + base64.b64encode(sha512(packed).digest()).decode("ascii")
        require(row.get("integrity") == sri,
                name + ": lock integrity does not match actual archive")
        files, archive_directories, archive_entry_count = archive_members(packed, name)
        packed_manifest = json.loads(files["package.json"][0])
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
        witness, candidate_core_tree = compare_installed(
            candidate, location, files, archive_directories, name,
        )
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
        "note": "Core-format provider digests are derived from unqualified physical input bytes; "
                "independent source-owner selection, signed Host/Pack/Guard and installed "
                "graph admission remain unproved.",
        "packages": receipts,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", required=True, type=Path)
    parser.add_argument("--selected-revision", required=True)
    parser.add_argument("--candidate-root", required=True, type=Path)
    parser.add_argument("--repro-dir", required=True, type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        receipt = validate(
            args.source_root, args.selected_revision,
            args.candidate_root, args.repro_dir,
        )
    except (ValueError, OSError, UnicodeError, tarfile.TarError, KeyError, TypeError,
            json.JSONDecodeError) as error:
        print("B3_CANDIDATE_REFUSED: " + str(error), file=sys.stderr)
        return 2
    result = json.dumps(receipt, sort_keys=True, indent=2) + "\n"
    if args.output is not None:
        args.output.write_text(result, encoding="utf8")
    print(result, end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())

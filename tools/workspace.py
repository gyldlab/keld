"""Keld local workspace allocation and command execution (Python stdlib only).

Git owns checkout identity. This tool coordinates cooperative local agents; it is
not an OS sandbox and only deletes a validated released session's scratch directory.
"""
import argparse
from contextlib import contextmanager
from dataclasses import dataclass
import getpass
import json
import os
from pathlib import Path
import platform
import re
import stat
import subprocess
import sys
import tempfile
import threading
import time
import uuid

import session_closeout

DIRECTORY = ".keld-work"
SCHEMA = "keld.workspace-task/v1"
MIB = 1024 * 1024
MAX_RECORD = 64 * 1024
GIT_ENV = ("GIT_DIR", "GIT_COMMON_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY")


class WorkspaceError(ValueError):
    """A refused local operation with an actionable diagnostic."""


def require(condition, message):
    if not condition:
        raise WorkspaceError(message)


def same(left, right):
    return os.path.normcase(os.path.abspath(left)) == os.path.normcase(os.path.abspath(right))


def safe_path(path):
    """Reject links/reparse ancestors before cooperative filesystem operations."""
    path = Path(os.path.abspath(path))
    for part in (path, *path.parents):
        if not os.path.lexists(part):
            continue
        info = part.lstat()
        require(not stat.S_ISLNK(info.st_mode) and
                not getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT,
                f"Link/reparse path refused: {part}. Use a real directory; preserve its target.")
    return path


def component(value, label, maximum=128):
    require(isinstance(value, str) and len(value) <= maximum and
            re.fullmatch(r"[A-Za-z0-9_-]+", value), f"Invalid {label}. Use one ASCII name component.")
    require(value.upper() not in {"CON", "PRN", "AUX", "NUL", *(f"COM{i}" for i in range(1, 10)),
                                 *(f"LPT{i}" for i in range(1, 10))},
            f"Reserved {label}: {value}. Choose a portable name.")
    return value


def task_name(value):
    component(value, "task", 64)
    require(re.fullmatch(r"kel-[1-9][0-9]*-[a-z0-9]+(?:-[a-z0-9]+)*", value),
            "Invalid task. Use kel-<positive issue number>-<lowercase-kebab-slug>.")
    return value


def git(cwd, *args, check=True):
    result = subprocess.run(["git", "-C", str(cwd), *args], capture_output=True, timeout=30)
    if check and result.returncode:
        raise WorkspaceError("Git " + args[0] + " failed. Inspect repository state before retrying: " +
                             result.stderr.decode("utf-8", errors="replace").strip())
    return result


def git_text(cwd, *args):
    return git(cwd, *args).stdout.decode("utf-8").strip()


def worktrees(cwd):
    records = []
    record = {}
    for raw in git(cwd, "worktree", "list", "--porcelain", "-z").stdout.split(b"\0"):
        if not raw:
            if record:
                records.append(record)
                record = {}
            continue
        key, _, value = raw.decode("utf-8").partition(" ")
        record[key] = value
    if record:
        records.append(record)
    return records


@dataclass(frozen=True)
class Context:
    checkout: Path
    primary: Path
    common: Path

    @property
    def root(self):
        return self.primary / DIRECTORY


def admit_workspace_paths(ctx, session=None, task=None):
    """Refuse unqualified Windows runtimes/paths before operation-side writes."""
    if os.name != "nt":
        return
    # CVE-2024-4030: these os.mkdir(0o700) backports give mkdtemp a private
    # creation-time DACL. Other implementations/prereleases are not qualified.
    release = sys.version_info
    minimum = {(3, 9): 20, (3, 10): 15, (3, 11): 10, (3, 12): 4}.get(release[:2])
    patched = release[0] == 3 and (release[1] >= 13 or (minimum is not None and release[2] >= minimum))
    require(platform.python_implementation() == "CPython" and release[3] == "final" and patched,
            "Windows managed workspace requires final CPython with private directory creation: "
            "3.9.20+, 3.10.15+, 3.11.10+, 3.12.4+, or 3.13+ within Python 3. "
            "Run this command with a supported final CPython; preserve existing sessions and evidence.")
    # Deliberately use one conservative cell even on long-path-enabled hosts.
    # MAX_PATH includes NUL; directory creation also reserves an 8.3 filename.
    # https://learn.microsoft.com/windows/win32/fileio/maximum-file-path-limitation
    directories = [ctx.root / "tmp" / "00000000"]
    records = []
    files = [ctx.root / "reference.lock"]
    if task is not None:
        target = ctx.root / "worktrees" / task_name(task)
        directories.append(target)
        records.append(target.with_suffix(".json"))
        files.append(target.with_suffix(".lock"))
    if session is not None:
        root = ctx.root / "sessions" / component(session, "session")
        records.append(root / "scratch-owners" / "00000000.json")
        # clean- plus UUID is longer than run- plus UUID. Reserve its result,
        # both captured streams, and write_record's same-directory replacement.
        evidence = root / "evidence" / ("clean-" + "0" * 32)
        records.append(evidence / "result.json")
        files.extend(evidence / (stream + ".log") for stream in ("stdout", "stderr"))
    files.extend(records)
    files.extend(path.parent / "record-00000000.tmp" for path in records)
    directories.extend(path.parent for path in files)
    for paths, maximum, kind in [(directories, 247, "directory"), (files, 259, "file")]:
        for path in paths:
            absolute = os.path.abspath(path)
            units = len(absolute.encode("utf-16-le")) // 2
            require(units <= maximum,
                    f"Windows managed path exceeds the supported {kind} limit ({units} > {maximum} UTF-16 units): "
                    f"{absolute}. Use a shorter real primary checkout path; preserve the existing session identity and evidence.")


def context(cwd=None):
    require(not any(os.environ.get(key) for key in GIT_ENV),
            "Git directory overrides are set. Unset GIT_DIR/WORK_TREE/COMMON_DIR/INDEX_FILE/OBJECT_DIRECTORY for workspace commands.")
    launch = safe_path(cwd or Path.cwd())
    checkout = safe_path(git_text(launch, "rev-parse", "--show-toplevel"))
    common = safe_path(git_text(checkout, "rev-parse", "--path-format=absolute", "--git-common-dir"))
    records = worktrees(checkout)
    require(records and "bare" not in records[0] and "worktree" in records[0],
            "A normal primary checkout is required. Bare repositories are unsupported.")
    primary = safe_path(records[0]["worktree"])
    require(primary.is_dir() and (primary / ".git").is_dir() and same(common, primary / ".git"),
            "Primary checkout identity is missing or uses a separate Git directory. Restore a normal primary checkout.")
    require(same(git_text(primary, "rev-parse", "--show-toplevel"), primary) and
            same(git_text(primary, "rev-parse", "--path-format=absolute", "--git-common-dir"), common),
            "Primary/common Git identity mismatch. Repair the worktree registration first.")
    result = Context(checkout, primary, common)
    safe_path(result.root)
    check_namespace(result)
    return result


def check_namespace(ctx):
    require(same(ctx.checkout, ctx.primary) or not os.path.lexists(ctx.checkout / DIRECTORY),
            "Linked checkout has a nested .keld-work. Preserve it and reconcile into the primary workspace.")


def check_index(ctx):
    check_namespace(ctx)
    for checkout in {ctx.primary, ctx.checkout}:
        tracked = git(checkout, "ls-files", "-z", "--", DIRECTORY).stdout
        require(not tracked, "Managed workspace content is tracked. Unstage/untrack .keld-work without deleting local data.")
    ignored = git(ctx.primary, "check-ignore", "--quiet", "--no-index", "--", DIRECTORY + "/probe", check=False)
    require(ignored.returncode == 0, "Primary .keld-work is not ignored. Add /.keld-work/ to the primary ignore rules before allocation.")


def mkdir(path):
    safe_path(path)
    path.mkdir(parents=True, exist_ok=True)
    safe_path(path)


def read_record(path):
    safe_path(path)
    info = path.stat()
    require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_size <= MAX_RECORD,
            f"Invalid or oversized workspace record: {path}. Preserve it for inspection.")
    def pairs(items):
        obj = {}
        for key, value in items:
            require(key not in obj, "Duplicate workspace record key. Restore the original record.")
            obj[key] = value
        return obj
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=pairs)


def write_record(path, value, *, exclusive=False):
    mkdir(path.parent)
    safe_path(path)
    if exclusive:
        with path.open("x", encoding="utf-8") as stream:
            json.dump(value, stream, indent=2)
            stream.flush()
            os.fsync(stream.fileno())
        return
    if path.exists():
        read_record(path)
    fd, temp = tempfile.mkstemp(prefix="record-", suffix=".tmp", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            json.dump(value, stream, indent=2)
            stream.flush()
            os.fsync(stream.fileno())
        safe_path(path)
        os.replace(temp, path)
    finally:
        if os.path.lexists(temp):
            os.unlink(temp)


def allocate_scratch(ctx, session, run_id):
    """Allocate private, short physical scratch; retain its session ownership."""
    session = component(session, "session")
    admit_workspace_paths(ctx, session)
    parent = safe_path(ctx.root / "tmp")
    mkdir(parent)
    # mkdtemp supplies exclusive creation and owner-only mode. Full session and
    # run names belong in metadata, not every inherited Unix socket pathname.
    scratch = safe_path(tempfile.mkdtemp(prefix="", dir=parent))
    info = scratch.stat()
    record = dict(schema="keld.workspace-scratch/v1", session=session,
                  token=scratch.name, run=run_id, device=info.st_dev, inode=info.st_ino)
    write_record(ctx.root / "sessions" / session / "scratch-owners" / (scratch.name + ".json"),
                 record, exclusive=True)
    return scratch


def session_scratch(ctx, session):
    """Resolve only this session's recorded identities plus its legacy scratch."""
    session = component(session, "session")
    root = safe_path(ctx.root / "sessions" / session)
    targets = []
    legacy = safe_path(root / "scratch")
    if legacy.exists():
        targets.append(legacy)
    owners = safe_path(root / "scratch-owners")
    if owners.exists():
        for file in sorted(owners.iterdir()):
            value = read_record(file)
            require(set(value) == {"schema", "session", "token", "run", "device", "inode"} and
                    value["schema"] == "keld.workspace-scratch/v1" and value["session"] == session and
                    isinstance(value["token"], str) and re.fullmatch(r"[a-z0-9_]{8}", value["token"]) and
                    file.name == value["token"] + ".json" and
                    isinstance(value["run"], str) and
                    re.fullmatch(r"(?:run|reference)-[0-9a-f]{32}", value["run"]) and
                    type(value["device"]) is int and type(value["inode"]) is int,
                    f"Invalid scratch ownership record: {file}. Preserve it for inspection.")
            target = safe_path(ctx.root / "tmp" / value["token"])
            if not target.exists():
                # Previously removed targets remain recorded; never adopt an
                # unregistered directory or infer a replacement's ownership.
                continue
            info = target.stat()
            require(stat.S_ISDIR(info.st_mode) and
                    (info.st_dev, info.st_ino) == (value["device"], value["inode"]),
                    f"Scratch identity changed: {target}. Preserve the replacement for inspection.")
            targets.append(target)
    return targets


def owner():
    return getpass.getuser() + "@" + platform.node()


@contextmanager
def operation_lock(path):
    mkdir(path.parent)
    safe_path(path)
    try:
        stream = path.open("x", encoding="utf-8")
    except FileExistsError as error:
        raise WorkspaceError(f"Task operation lock exists: {path}. Wait for its owner; do not remove an unverified stale lock.") from error
    identity = path.stat()
    try:
        stream.write(str(os.getpid()))
        stream.flush()
        yield
    finally:
        stream.close()
        safe_path(path)
        current = path.stat()
        require((identity.st_dev, identity.st_ino) == (current.st_dev, current.st_ino),
                "Task lock identity changed. Preserve it and inspect concurrent writers.")
        path.unlink()


@contextmanager
def task_lock(ctx, name):
    path = ctx.root / "worktrees" / (task_name(name) + ".lock")
    with operation_lock(path):
        reference = safe_path(ctx.root / "reference.lock")
        require(not os.path.lexists(reference), "Reference operation is active. Wait for its owner before starting or running a task.")
        yield


def load_task(ctx, name, session=None, *, active=True):
    name = task_name(name)
    record = read_record(ctx.root / "worktrees" / (name + ".json"))
    fields = {"schema", "task", "issue", "branch", "base", "path", "common_dir", "owner", "sessions", "state"}
    released_fields = fields | {"receipt"}
    require(isinstance(record, dict) and set(record) in {frozenset(fields), frozenset(released_fields)} and record["schema"] == SCHEMA,
            "Unknown task record schema. Preserve it and use the matching tool version.")
    require(record["task"] == name and record["path"] == "worktrees/" + name and
            record["issue"] == "-".join(name.split("-")[:2]).upper() and
            record["branch"] == "agent/" + name and same(record["common_dir"], ctx.common) and
            isinstance(record["base"], str) and re.fullmatch(r"[0-9a-f]{40}", record["base"]) and
            record["state"] in {"active", "released"} and isinstance(record["sessions"], list) and record["sessions"],
            "Task identity mismatch. Restore its original ownership record; do not adopt foreign work.")
    require(record["state"] != "released" or
            (set(record) == released_fields and isinstance(record["receipt"], str) and Path(record["receipt"]).is_absolute()),
            "Released task has no valid closeout receipt. Preserve it and reconcile the release record.")
    for identifier in record["sessions"]:
        component(identifier, "session")
    path = safe_path(ctx.root / record["path"])
    matches = [item for item in worktrees(ctx.primary) if same(item.get("worktree", ""), path)]
    require(len(matches) == 1 and matches[0].get("branch") == "refs/heads/" + record["branch"] and
            "locked" not in matches[0] and "prunable" not in matches[0],
            "Task worktree registration/branch is missing, locked or changed. Reconcile it before continuing.")
    require(path.is_dir() and same(git_text(path, "rev-parse", "--show-toplevel"), path) and
            same(git_text(path, "rev-parse", "--path-format=absolute", "--git-common-dir"), ctx.common),
            "Task checkout resolves to a foreign repository. Preserve it and inspect registration.")
    check_index(Context(path, ctx.primary, ctx.common))
    if session is not None:
        component(session, "session")
        require((not active or record["state"] == "active") and record["owner"] == owner() and session in record["sessions"],
                "Task belongs to another session or is released. Use the owning session; reconcile the Linear claim before transfer.")
    return record, path


def start(ctx, issue, slug, session, base):
    require(re.fullmatch(r"kel-[1-9][0-9]*", issue), "Invalid issue. Use kel-<positive number>.")
    name = task_name(issue + "-" + slug)
    session = component(session, "session")
    require(base and not base.startswith("-"), "Invalid base. Fetch origin main or pass a commit/ref with --base.")
    admit_workspace_paths(ctx, session, name)
    check_index(ctx)
    target = ctx.root / "worktrees" / name
    metadata = target.with_suffix(".json")
    if metadata.exists():
        with task_lock(ctx, name):
            record, path = load_task(ctx, name, session)
            return dict(record, path=str(path))
    resolved = git(ctx.primary, "rev-parse", "--verify", "--end-of-options", base + "^{commit}", check=False)
    require(resolved.returncode == 0, "Base is unavailable. Run git fetch origin main or choose an existing --base before allocation.")
    sha = resolved.stdout.decode("ascii").strip()
    require(not os.path.lexists(target), f"Unmanaged destination already exists: {target}. Preserve it; choose a different task slug.")
    branch = "agent/" + name
    exists = git(ctx.primary, "show-ref", "--verify", "--quiet", "refs/heads/" + branch, check=False)
    require(exists.returncode == 1, "Task branch already exists or Git cannot inspect it. Reconcile the existing branch before starting.")
    with task_lock(ctx, name):
        require(not os.path.lexists(target) and not os.path.lexists(metadata), "Task was allocated concurrently. Re-read status.")
        git(ctx.primary, "-c", "branch.autoSetupMerge=false", "worktree", "add", "-b", branch, str(target), sha)
        record = dict(schema=SCHEMA, task=name, issue=issue.upper(), branch=branch, base=sha,
                      path="worktrees/" + name, common_dir=str(ctx.common), owner=owner(), sessions=[session], state="active")
        # On write failure retain the new Git tree/branch, never guess that it is safe to delete.
        write_record(metadata, record, exclusive=True)
    return dict(record, path=str(target))


def logical_bytes(root):
    total = 0
    pending = [safe_path(root)]
    while pending:
        directory = pending.pop()
        safe_path(directory)
        with os.scandir(directory) as entries:
            for entry in entries:
                info = entry.stat(follow_symlinks=False)
                if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT:
                    continue
                if stat.S_ISDIR(info.st_mode):
                    require(not os.path.ismount(entry.path), "Nested mount refused during size census. Inspect it separately.")
                    pending.append(Path(entry.path))
                else:
                    total += info.st_size
    return total


def status(ctx, sizes=False):
    tasks = []
    directory = safe_path(ctx.root / "worktrees")
    if directory.exists():
        for file in sorted(directory.glob("*.json")):
            record, path = load_task(ctx, file.stem)
            retained = "active task; cleanup is a separate step" if record["state"] == "active" else "released task; only scratch is cleanup-eligible"
            row = dict(task=record["task"], path=str(path), state=record["state"], sessions=record["sessions"],
                       dirty=bool(git(path, "status", "--porcelain", "-z").stdout), retained=retained)
            if sizes:
                row["bytes"] = logical_bytes(path)
            tasks.append(row)
    known = {os.path.normcase(row["path"]) for row in tasks}
    legacy = [item["worktree"] for item in worktrees(ctx.primary) if "worktree" in item and
              not same(item["worktree"], ctx.primary) and os.path.normcase(str(Path(item["worktree"]))) not in known]
    result = dict(root=str(ctx.root), tasks=tasks, unmanaged_worktrees=legacy)
    if sizes:
        result["retained_evidence_bytes"] = 0
        result["scratch_bytes"] = 0
        shallow = safe_path(ctx.root / "tmp")
        if shallow.exists():
            result["scratch_bytes"] += logical_bytes(shallow)
        session_root = safe_path(ctx.root / "sessions")
        if session_root.exists():
            for directory in session_root.iterdir():
                safe_path(directory)
                for category, key in [("evidence", "retained_evidence_bytes"), ("scratch", "scratch_bytes")]:
                    path = directory / category
                    if path.exists():
                        result[key] += logical_bytes(path)
    return result


class Capture:
    """Bound memory and retained bytes independently of streamed volume."""
    def __init__(self, limit):
        self.limit = limit
        self.total = 0
        self.tail = bytearray()
        self.lock = threading.Lock()
        self.stop = threading.Event()
        self.error = None

    def read(self, pipe, console):
        try:
            while not self.stop.is_set():
                block = os.read(pipe.fileno(), 65536)
                if not block:
                    break
                with self.lock:
                    if self.stop.is_set():
                        break
                    self.total += len(block)
                    self.tail.extend(block)
                    if len(self.tail) > self.limit:
                        del self.tail[:-self.limit]
                console.buffer.write(block)
                console.buffer.flush()
        except (OSError, ValueError) as error:
            self.error = type(error).__name__
        finally:
            pipe.close()

    def finish(self, destination):
        self.stop.set()
        with self.lock:
            with destination.open("xb") as stream:
                stream.write(self.tail)
            return dict(total_bytes=self.total, retained_bytes=len(self.tail), omitted_bytes=self.total - len(self.tail),
                        truncated=self.total > len(self.tail), capture_error=self.error)


def run(ctx, name, session, argv, limit_mib):
    require(argv, "No command supplied. Use work-run <task> --session <id> -- <command> [args].")
    require(type(limit_mib) is int and 1 <= limit_mib <= 64, "Log limit must be an integer from 1 to 64 MiB per stream.")
    admit_workspace_paths(ctx, session, name)
    check_index(ctx)
    with task_lock(ctx, name):
        record, checkout = load_task(ctx, name, session)
        session_root = safe_path(ctx.root / "sessions" / component(session, "session"))
        identifier = "run-" + uuid.uuid4().hex
        scratch = allocate_scratch(ctx, session, identifier)
        evidence = session_root / "evidence" / identifier
        mkdir(evidence)
        result_file = evidence / "result.json"
        value = dict(schema="keld.workspace-run/v1", task=name, session=session, state="running",
                     source_head=git_text(checkout, "rev-parse", "HEAD"), exit_code=None, runner_exit_code=None,
                     scratch=str(scratch), log_limit_bytes=limit_mib * MIB)
        write_record(result_file, value, exclusive=True)
        child_env = dict(os.environ, TMPDIR=scratch.as_posix(), TEMP=str(scratch), TMP=str(scratch),
                         KELD_WORK_SESSION=session)
        captures = [Capture(limit_mib * MIB), Capture(limit_mib * MIB)]
        started = time.monotonic()
        try:
            proc = subprocess.Popen(argv, cwd=checkout, env=child_env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        except OSError as error:
            value.update(state="spawn-failed", runner_exit_code=125, error_type=type(error).__name__)
            write_record(result_file, value)
            print(f"WORKSPACE: Command could not start. Check the executable; evidence: {evidence}", file=sys.stderr)
            return 125
        threads = [threading.Thread(target=capture.read, args=(pipe, console), daemon=True)
                   for capture, pipe, console in zip(captures, [proc.stdout, proc.stderr], [sys.stdout, sys.stderr])]
        for thread in threads:
            thread.start()
        interrupted = False
        try:
            code = proc.wait()
        except KeyboardInterrupt:
            interrupted = True
            proc.terminate()
            try:
                code = proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                code = proc.wait()
        deadline = time.monotonic() + 2
        for thread in threads:
            thread.join(timeout=max(0, deadline - time.monotonic()))
        complete = not any(thread.is_alive() for thread in threads) and not any(capture.error for capture in captures)
        streams = {key: capture.finish(evidence / (key + ".log")) for key, capture in zip(["stdout", "stderr"], captures)}
        exit_code = code if code >= 0 else 128 - code
        runner_code = 130 if interrupted else exit_code if complete or exit_code else 125
        value.update(state="interrupted" if interrupted else "complete" if complete else "capture-incomplete",
                     exit_code=code, runner_exit_code=runner_code, elapsed_seconds=time.monotonic() - started, **streams)
        write_record(result_file, value)
        print("WORKSPACE evidence: " + str(evidence), file=sys.stderr)
        return runner_code


def clean_checkout(checkout):
    require(not git(checkout, "status", "--porcelain", "-z").stdout,
            "Task checkout is dirty or has untracked files. Preserve and reconcile it before release or cleanup.")
    merged = git(checkout, "diff", "--quiet", "origin/main", "HEAD", check=False)
    require(merged.returncode == 0,
            "Task has content not present in origin/main. Preserve its branch and merged PR evidence before release.")


def finish(ctx, name, session, receipt):
    admit_workspace_paths(ctx, session, name)
    check_index(ctx)
    with task_lock(ctx, name):
        record, checkout = load_task(ctx, name, session)
        clean_checkout(checkout)
        require(session_closeout.check(receipt) == "complete",
                "Closeout receipt is not complete. Repair the receipt before release.")
        value = session_closeout.read_json(Path(receipt))
        require(same(value["repo"], checkout) and value["session_id"] == session,
                "Closeout receipt belongs to another task or session. Preserve it and use the owning receipt.")
        record.update(state="released", receipt=str(Path(receipt).resolve()))
        write_record(ctx.root / "worktrees" / (task_name(name) + ".json"), record)
        return dict(task=name, session=session, state="released", receipt=str(Path(receipt).resolve()))


def safe_disposable_tree(root):
    root = safe_path(root)
    if not root.exists():
        return []
    nodes = []
    pending = [root]
    while pending:
        path = pending.pop()
        info = path.lstat()
        require(stat.S_ISDIR(info.st_mode) and not stat.S_ISLNK(info.st_mode) and
                not getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT,
                f"Disposable cleanup target is not a real directory: {path}. Preserve it for inspection.")
        require(not os.path.ismount(path),
                f"Nested mount refused during cleanup: {path}. Preserve it for inspection.")
        require(not (path / ".git").exists(),
                f"Nested Git repository refused during cleanup: {path}. Preserve it for inspection.")
        nodes.append((path, info.st_dev, info.st_ino, True))
        with os.scandir(path) as entries:
            for entry in entries:
                child = Path(entry.path)
                child_info = child.lstat()
                require(not stat.S_ISLNK(child_info.st_mode) and
                        not getattr(child_info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT,
                        f"Link/reparse entry refused during cleanup: {child}. Preserve it for inspection.")
                if stat.S_ISDIR(child_info.st_mode):
                    pending.append(child)
                else:
                    require(stat.S_ISREG(child_info.st_mode),
                            f"Unsupported disposable entry: {child}. Preserve it for inspection.")
                    nodes.append((child, child_info.st_dev, child_info.st_ino, False))
    return nodes


def remove_disposable_tree(nodes, recorded):
    for path, device, inode, directory in sorted(nodes, key=lambda item: len(item[0].parts), reverse=True):
        info = path.lstat()
        require((info.st_dev, info.st_ino) == (device, inode),
                f"Disposable cleanup target changed: {path}. Stop and inspect it; no further deletion was attempted.")
        if directory:
            path.rmdir()
        else:
            path.unlink()
        recorded(path)


def receipt_retained_paths(value):
    """Collect hashed proof and resources whose continued existence a receipt promises."""
    if isinstance(value, dict):
        if (set(value) == {"path", "sha256"} or value.get("status") == "retained") and isinstance(value.get("path"), str):
            yield Path(value["path"])
        for nested in value.values():
            yield from receipt_retained_paths(nested)
    elif isinstance(value, list):
        for nested in value:
            yield from receipt_retained_paths(nested)


def clean(ctx, name, session, apply, receipt=None):
    admit_workspace_paths(ctx, session, name)
    check_index(ctx)
    with task_lock(ctx, name):
        record, checkout = load_task(ctx, name, session, active=False)
        require(record["state"] == "released", "Task is active. Finish it with a complete closeout receipt before cleanup.")
        directory = safe_path(ctx.root / "worktrees")
        releases = []
        if directory.exists():
            for file in directory.glob("*.json"):
                other, other_checkout = load_task(ctx, file.stem)
                if session not in other["sessions"]:
                    continue
                require(other["state"] != "active",
                        "Another active task owns this session scratch. Finish or use a distinct session before cleanup.")
                releases.append((other, other_checkout))
        clean_checkout(checkout)
        targets = session_scratch(ctx, session)
        receipts = []
        for released, released_checkout in releases:
            receipt_path = Path(released["receipt"])
            release_receipt = session_closeout.read_json(receipt_path)
            require(same(release_receipt["repo"], released_checkout) and
                    release_receipt["session_id"] == session and
                    session_closeout.check(receipt_path) == "complete",
                    "Released task closeout receipt no longer validates for this session. Preserve scratch and repair its evidence first.")
            receipts.append((receipt_path, release_receipt))
        if apply:
            require(receipt is not None, "Cleanup apply requires a prepared current-turn --receipt; preview is read-only.")
            cleanup_receipt_path = Path(receipt)
            receipt_value = session_closeout.read_json(cleanup_receipt_path)
            require(same(receipt_value.get("repo", ""), checkout) and receipt_value.get("session_id") == session,
                    "Cleanup receipt belongs to another task or session. Preserve scratch and use the owning receipt.")
            # A prepared receipt can name future removals, but its own artifacts must survive them.
            receipts.append((cleanup_receipt_path, receipt_value))
        for receipt_path, receipt_value in receipts:
            for evidence in [receipt_path, *receipt_retained_paths(receipt_value)]:
                require(not any(evidence.resolve().is_relative_to(target.resolve()) for target in targets),
                        f"Scratch is referenced by closeout evidence: {evidence}. Preserve it and archive/rewrite the evidence first.")
        nodes = [node for target in targets for node in safe_disposable_tree(target)]
        byte_count = sum(path.stat().st_size for path, _, _, directory in nodes if not directory)
        plan = dict(task=name, session=session, applied=apply, targets=[str(target) for target in targets],
                    scratch_bytes=byte_count, retained_evidence=str(ctx.root / "sessions" / session / "evidence"),
                    retained_worktree=str(checkout), reason="released task scratch only; evidence, source branch and worktree are retained")
        if apply:
            evidence = safe_path(ctx.root / "sessions" / session / "evidence" / ("clean-" + uuid.uuid4().hex))
            mkdir(evidence)
            result = dict(schema="keld.workspace-clean/v1", state="running", removed=[], **plan)
            result_file = evidence / "result.json"
            write_record(result_file, result, exclusive=True)
            def recorded(path):
                result["removed"].append(str(path))
                write_record(result_file, result)
            try:
                remove_disposable_tree(nodes, recorded)
                require(session_closeout.check(cleanup_receipt_path) == "complete",
                        "Cleanup receipt is not complete after deletion. Record an explicit handoff; scratch removal is not a successful cleanup.")
            except (WorkspaceError, session_closeout.Invalid, OSError) as error:
                result.update(state="failed", error_type=type(error).__name__)
                write_record(result_file, result)
                raise
            result["state"] = "complete"
            write_record(result_file, result)
        return plan


def main(argv=None):
    args = list(sys.argv[1:] if argv is None else argv)
    child = []
    if args and args[0] in {"run", "reference-run"} and "--" in args:
        split = args.index("--")
        args, child = args[:split], args[split + 1:]
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    commands.add_parser("root")
    commands.add_parser("check")
    commands.add_parser("reference-root")
    commands.add_parser("reference-run")
    finish_parser = commands.add_parser("finish")
    finish_parser.add_argument("task")
    finish_parser.add_argument("--session", default=os.environ.get("KELD_WORK_SESSION"), required=not os.environ.get("KELD_WORK_SESSION"))
    finish_parser.add_argument("--receipt", required=True)
    clean_parser = commands.add_parser("clean")
    clean_parser.add_argument("task")
    clean_parser.add_argument("--session", default=os.environ.get("KELD_WORK_SESSION"), required=not os.environ.get("KELD_WORK_SESSION"))
    clean_parser.add_argument("--apply", action="store_true")
    clean_parser.add_argument("--receipt")
    status_parser = commands.add_parser("status")
    status_parser.add_argument("--sizes", action="store_true")
    start_parser = commands.add_parser("start")
    start_parser.add_argument("issue")
    start_parser.add_argument("slug")
    start_parser.add_argument("--base", default="origin/main")
    start_parser.add_argument("--session", default=os.environ.get("KELD_WORK_SESSION"))
    run_parser = commands.add_parser("run")
    run_parser.add_argument("task")
    run_parser.add_argument("--session", default=os.environ.get("KELD_WORK_SESSION"), required=not os.environ.get("KELD_WORK_SESSION"))
    run_parser.add_argument("--log-limit-mib", type=int, default=4)
    options = parser.parse_args(args)
    try:
        ctx = context()
        if options.operation == "root":
            print(ctx.root)
        elif options.operation == "start":
            print(json.dumps(start(ctx, options.issue, options.slug, options.session or "manual-" + uuid.uuid4().hex, options.base), indent=2))
        elif options.operation == "run":
            return run(ctx, options.task, options.session, child, options.log_limit_mib)
        elif options.operation == "finish":
            print(json.dumps(finish(ctx, options.task, options.session, options.receipt), indent=2))
        elif options.operation == "clean":
            print(json.dumps(clean(ctx, options.task, options.session, options.apply, options.receipt), indent=2))
        elif options.operation == "reference-root":
            reference_admission(ctx)
            print(ctx.primary.as_posix())
        elif options.operation == "reference-run":
            require(child, "No reference command supplied. Use the public research/competitors recipes.")
            reference_admission(ctx)
            check_index(ctx)
            session = component(os.environ.get("KELD_WORK_SESSION") or "manual-" + uuid.uuid4().hex, "session")
            admit_workspace_paths(ctx, session)
            with operation_lock(ctx.root / "reference.lock"):
                reference_admission(ctx)
                scratch = allocate_scratch(ctx, session, "reference-" + uuid.uuid4().hex)
                env = dict(os.environ, TMPDIR=scratch.as_posix(), TEMP=str(scratch), TMP=str(scratch), KELD_WORK_SESSION=session)
                print("WORKSPACE reference inputs: " + git_text(ctx.checkout, "rev-parse", "HEAD"), file=sys.stderr)
                return subprocess.call(child, cwd=ctx.primary, env=env)
        else:
            check_index(ctx)
            value = status(ctx, getattr(options, "sizes", False))
            print(json.dumps(value, indent=2))
        return 0
    except (WorkspaceError, OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError) as error:
        print("WORKSPACE: " + str(error), file=sys.stderr)
        return 2


def reference_admission(ctx):
    require(same(ctx.primary, ctx.checkout), f"Reference sync from a linked tree is refused. Use the primary checkout {ctx.primary} after reconciling active reference consumers.")
    directory = ctx.root / "worktrees"
    require(not directory.exists() or not any(directory.glob("*.lock")), "Workspace operations are active. Wait for their owners before reference sync.")
    for record in status(ctx)["tasks"]:
        if record["state"] == "active":
            session = os.environ.get("KELD_WORK_SESSION")
            require(session, "Managed tasks are active. Set KELD_WORK_SESSION to their owning session after reconciling reference consumers; other active sessions must finish first.")
            load_task(ctx, record["task"], session)


if __name__ == "__main__":
    sys.exit(main())

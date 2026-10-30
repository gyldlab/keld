"""Reject local Git index/history overlays outside a signed CI reader head."""

import os
from pathlib import Path


def clean_git_environment() -> dict[str, str]:
    """Ignore caller-provided index, object, worktree and configuration redirects."""
    return {name: value for name, value in os.environ.items() if not name.startswith("GIT_")}


def index_or_graft_mismatch(root: Path, head_tree: dict[str, tuple[str, str]], git) -> str | None:
    """Return an actionable refusal for an index or history not matching HEAD."""
    graft_name = git(root, "rev-parse", "--git-path", "info/grafts").decode("utf-8", "strict").strip()
    if not graft_name:
        return "Git graft path unavailable"
    graft_file = Path(graft_name)
    if not graft_file.is_absolute():
        graft_file = root / graft_file
    if graft_file.exists():
        return "untrusted legacy Git grafts can rewrite reviewed history"

    # A cached diff hides `git add -N` (intent-to-add). Enumerate every index
    # entry, including unmerged stages and intent-to-add, and compare to HEAD.
    index = {}
    for row in git(root, "ls-files", "--stage", "-z", "--").split(b"\0"):
        if not row:
            continue
        try:
            metadata, raw_path = row.split(b"\t", 1)
            mode, oid, stage = metadata.decode("ascii").split(" ")
            path = raw_path.decode("utf-8", "strict")
        except (ValueError, UnicodeError):
            return "malformed staged Git index entry"
        if stage != "0" or path in index:
            return "unmerged or duplicated staged Git index entries"
        index[path] = (mode, oid)
    if index != head_tree:
        return "staged Git index differs from the reviewed exact head"
    return None

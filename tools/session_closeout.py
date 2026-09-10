"""Read-only closeout evidence validator; remote receipts remain human-verifiable links.

This binds an independently retained baseline to a final inventory. It cannot discover
unrecorded work, prove when a baseline was created, or authenticate remote ticket state.
Check source_head is declared metadata, not authentication of command execution.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
from urllib.parse import urlsplit


class Invalid(ValueError):
    """A receipt fails an observable closeout contract."""


def require(condition, message):
    if not condition:
        raise Invalid(message)


def fields(value, required, optional=()):
    require(isinstance(value, dict), "expected object")
    require(set(required) <= value.keys(), "missing fields: " + str(set(required) - value.keys()))
    require(value.keys() <= set(required) | set(optional), "unknown fields")


def meaningful(value, *, reject_marker_prefix=False):
    require(isinstance(value, str) and bool(value.strip()), "empty text")
    text = value.strip()
    require(text.lower() not in {"todo", "tbd", "none", "n/a", "placeholder", "..."},
            "placeholder text")
    require(not re.fullmatch(r"<[^>]+>", text), "placeholder text")
    if reject_marker_prefix:
        require(not re.match(r"(?:TODO|TBD|PLACEHOLDER)\b", text, re.IGNORECASE), "placeholder text")


def absolute(value):
    require(isinstance(value, str) and Path(value).is_absolute(), "path must be absolute")
    return Path(value)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate JSON key: " + key)
        result[key] = value
    return result


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8-sig"), object_pairs_hook=unique_object)


def evidence(value, *, allow_empty=False):
    fields(value, {"path", "sha256"})
    path = absolute(value["path"])
    require(isinstance(value["sha256"], str) and
            re.fullmatch(r"[0-9a-f]{64}", value["sha256"]), "invalid evidence SHA256")
    require(path.is_file(), "evidence file missing: " + str(path))
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    require(digest.hexdigest() == value["sha256"], "evidence hash mismatch: " + str(path))
    require(allow_empty or path.stat().st_size > 0, "empty evidence file")
    return path


def evidence_list(value):
    require(isinstance(value, list) and value, "evidence list must be nonempty")
    for item in value:
        evidence(item)


def rows(value, empty=False):
    require(isinstance(value, list) and (empty or value), "inventory must be nonempty")
    ids = set()
    for row in value:
        require(isinstance(row, dict), "inventory row must be object")
        identifier = row.get("id")
        meaningful(identifier)
        require(identifier not in ids, "duplicate inventory ID: " + identifier)
        ids.add(identifier)
    return ids


def blocked(row):
    meaningful(row.get("owner"), reject_marker_prefix=True)
    meaningful(row.get("next_action"), reject_marker_prefix=True)


def registered_worktrees(common):
    result = subprocess.run(["git", "--git-dir", str(common), "worktree", "list", "--porcelain", "-z"],
                            check=True, capture_output=True, text=True, encoding="utf-8", timeout=30)
    return {os.path.normcase(os.path.abspath(entry[len("worktree "):]))
            for entry in result.stdout.split("\0") if entry.startswith("worktree ")}


def repository_context(receipt, baseline):
    """Return (proven common Git directory, checkout exists); no remote/provenance claim."""
    repo = absolute(receipt["repo"])
    has_common = "git_common_dir" in baseline
    require(has_common == ("source_ref" in baseline), "baseline requires common directory and source ref together")
    live = repo.is_dir()
    if live:
        result = subprocess.run(["git", "-C", str(repo), "rev-parse", "--path-format=absolute",
                                 "--git-common-dir", "--show-toplevel", "HEAD"],
                                check=True, capture_output=True, text=True, encoding="utf-8", timeout=30)
        lines = result.stdout.splitlines()
        require(len(lines) == 3 and Path(lines[1]).resolve() == repo.resolve(), "repository root mismatch")
        require(lines[2] == receipt["head"], "stale git HEAD")
        common = Path(lines[0]).resolve()
        if has_common:
            require(absolute(baseline["git_common_dir"]).resolve() == common, "baseline common directory mismatch")
    else:
        require(not os.path.lexists(repo) and has_common, "removed repository requires retained source metadata")
        common = absolute(baseline["git_common_dir"]).resolve()
    if has_common:
        ref = baseline["source_ref"]
        require(isinstance(ref, str) and ref.startswith("refs/heads/"), "source_ref must name a retained branch")
        subprocess.run(["git", "check-ref-format", ref], check=True, capture_output=True, text=True, encoding="utf-8", timeout=30)
        result = subprocess.run(["git", "--git-dir", str(common), "rev-parse", "--verify", ref],
                                check=True, capture_output=True, text=True, encoding="utf-8", timeout=30)
        require(result.stdout.strip() == receipt["head"], "retained source ref changed")
    if not live:
        require(any(isinstance(row, dict) and row.get("status") == "removed" and
                    absolute(row.get("path")).resolve() == repo.resolve() for row in receipt["resources"]),
                "removed repository needs exact removed resource receipt")
        require(os.path.normcase(os.path.abspath(repo)) not in registered_worktrees(common),
                "removed repository remains registered worktree")
    return common, live


def check(receipt_path):
    """Raise Invalid/OSError on rejected evidence, otherwise return the declared outcome."""
    receipt = read_json(Path(receipt_path))
    fields(receipt, {"schema", "session_id", "repo", "head", "baseline", "objectives",
                     "findings", "findings_review", "resources", "checks", "outcome"},
           {"turn_id", "preserved_untracked"})
    require(receipt["schema"] == "keld.session-closeout/v1", "unsupported receipt schema")
    meaningful(receipt["session_id"])
    if "turn_id" in receipt:
        require(isinstance(receipt["turn_id"], str) and
                re.fullmatch(r"[A-Za-z0-9_-]{1,128}", receipt["turn_id"]), "invalid turn_id")
    repo = absolute(receipt["repo"])
    require(isinstance(receipt["head"], str) and
            re.fullmatch(r"[0-9a-f]{40}", receipt["head"]), "invalid git HEAD")
    baseline = read_json(evidence(receipt["baseline"]))
    fields(baseline, {"schema", "session_id", "repo", "objectives", "findings", "resources", "checks", "untracked"},
           {"git_common_dir", "source_ref"})
    require(baseline["schema"] == "keld.session-baseline/v1", "unsupported baseline schema")
    require(baseline["session_id"] == receipt["session_id"], "baseline session mismatch")
    require(absolute(baseline["repo"]).resolve() == repo.resolve(), "baseline repository mismatch")
    common, live = repository_context(receipt, baseline)
    for section in ("objectives", "findings", "resources"):
        current = rows(receipt[section], empty=section != "objectives")
        original = baseline[section]
        original_ids = rows(original, empty=section != "objectives")
        require(original_ids <= current, "dropped baseline " + section)
        current_rows = {row["id"]: row for row in receipt[section]}
        binding = "path" if section == "resources" else "summary"
        for row in original:
            fields(row, {"id", binding})
            meaningful(row[binding])
            expected = row[binding]
            actual = current_rows[row["id"]].get(binding)
            if binding == "path":
                expected = os.path.normcase(os.path.abspath(absolute(expected)))
                actual = os.path.normcase(os.path.abspath(absolute(actual)))
            require(expected == actual, "changed baseline " + section + " " + binding)

    required_checks = baseline["checks"]
    require(isinstance(required_checks, list) and required_checks, "baseline checks must be nonempty")
    for name in required_checks:
        meaningful(name)
    require(len(set(required_checks)) == len(required_checks), "duplicate baseline check")
    untracked = baseline["untracked"]
    require(isinstance(untracked, list), "baseline untracked must be a list")
    untracked_files = {}
    for item in untracked:
        fields(item, {"path", "sha256"})
        path = item["path"]
        require(isinstance(path, str) and path and not Path(path).is_absolute() and
                not re.match(r"^[A-Za-z]:|^[\\/]", path) and
                all(part not in {"", ".", ".."} for part in re.split(r"[\\/]", path)),
                "invalid baseline untracked path")
        require(isinstance(item["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", item["sha256"]),
                "invalid untracked SHA256")
        require(path not in untracked_files, "duplicate baseline untracked path")
        untracked_files[path] = item["sha256"]
    preserved = receipt.get("preserved_untracked", [])
    require(isinstance(preserved, list), "preserved_untracked must be a list")
    preserved_paths = set()
    for item in preserved:
        fields(item, {"path", "evidence"})
        path = item["path"]
        require(isinstance(path, str) and path in untracked_files, "unknown preserved untracked path")
        require(path not in preserved_paths, "duplicate preserved untracked path")
        preserved_paths.add(path)
        fields(item["evidence"], {"path", "sha256"})
        external = absolute(item["evidence"]["path"]).resolve()
        require(not external.is_relative_to(repo.resolve()), "preserved copy must be outside task checkout")
        require(item["evidence"]["sha256"] == untracked_files[path], "preserved untracked digest mismatch")
        evidence(item["evidence"], allow_empty=True)
    if not live:
        require(preserved_paths == untracked_files.keys(), "preserved untracked coverage mismatch")

    unresolved = False
    for row in receipt["objectives"]:
        fields(row, {"id", "summary", "status", "evidence"}, {"owner", "next_action"})
        meaningful(row["summary"])
        require(row["status"] in {"complete", "blocked", "deferred"}, "invalid objective status")
        evidence_list(row["evidence"])
        if row["status"] != "complete":
            blocked(row)
            unresolved = True

    evidence_list(receipt["findings_review"])
    for row in receipt["findings"]:
        fields(row, {"id", "summary", "status", "evidence"},
               {"issue", "remote_receipt", "owner", "next_action"})
        meaningful(row["summary"])
        require(row["status"] in {"tracked", "implemented", "refuted", "blocked"}, "invalid finding status")
        evidence_list(row["evidence"])
        if row["status"] in {"tracked", "implemented"}:
            blocked(row)
            issue = row.get("issue")
            require(isinstance(issue, str) and re.fullmatch(r"KEL-[1-9][0-9]*", issue),
                    "tracked finding requires KEL issue")
            url = row.get("remote_receipt")
            require(isinstance(url, str), "tracked finding requires remote receipt URL")
            parsed = urlsplit(url)
            require(parsed.scheme == "https" and parsed.hostname == "linear.app" and
                    parsed.username is None and parsed.password is None and
                    re.search(r"/issue/" + re.escape(issue) + r"(?:/|$)", parsed.path),
                    "remote receipt must link the matching Linear issue")
        elif row["status"] == "blocked":
            blocked(row)
            unresolved = True

    registered = registered_worktrees(common)
    paths = set()
    for row in receipt["resources"]:
        fields(row, {"id", "path", "status", "reason"}, {"owner", "next_action"})
        path = absolute(row["path"])
        key = os.path.normcase(os.path.abspath(path))
        require(key not in paths, "duplicate resource path")
        paths.add(key)
        meaningful(row["reason"])
        require(row["status"] in {"removed", "retained", "blocked"}, "invalid resource status")
        if row["status"] == "removed":
            require(not os.path.lexists(path), "removed resource still exists: " + str(path))
            require(key not in registered, "removed resource remains registered worktree: " + str(path))
        elif row["status"] == "retained":
            require(os.path.lexists(path), "retained resource missing: " + str(path))
        else:
            blocked(row)
            unresolved = True

    checks = receipt["checks"]
    require(isinstance(checks, list) and checks, "checks must be nonempty")
    names = set()
    for row in checks:
        fields(row, {"name", "status", "evidence", "source_head"}, {"owner", "next_action"})
        meaningful(row["name"])
        require(row["name"] not in names, "duplicate check name")
        names.add(row["name"])
        require(row["status"] in {"passed", "failed", "not-run"}, "invalid check status")
        require(isinstance(row["source_head"], str) and
                re.fullmatch(r"[0-9a-f]{40}", row["source_head"]), "invalid check source_head")
        if row["status"] == "passed":
            require(row["source_head"] == receipt["head"], "stale check source_head")
        evidence_list(row["evidence"])
        if row["status"] != "passed":
            blocked(row)
            unresolved = True
    require(receipt["outcome"] in {"complete", "handoff"}, "invalid outcome")
    require(set(required_checks) <= names, "dropped baseline checks")
    current_untracked = set()
    if live:
        census = subprocess.run(["git", "-C", str(repo), "ls-files", "--others", "--exclude-standard", "-z"],
                                check=True, capture_output=True, text=True, encoding="utf-8", timeout=30)
        current_untracked = set(census.stdout.rstrip("\0").split("\0")) - {""}
    new_untracked = current_untracked - untracked_files.keys()
    require(receipt["outcome"] != "complete" or not new_untracked,
            "unexplained untracked files: " + repr(sorted(new_untracked)))
    if live and receipt["outcome"] == "complete":
        require(current_untracked == untracked_files.keys(), "untracked inventory changed")
        for relative, expected in untracked_files.items():
            path = repo / relative
            for component in (path, *path.parents):
                if component == repo:
                    break
                info = component.lstat()
                require(not stat.S_ISLNK(info.st_mode) and
                        not getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT,
                        "unsupported untracked reparse/symlink; record handoff")
            require(stat.S_ISREG(path.lstat().st_mode), "unsupported untracked resource; record handoff")
            digest = hashlib.sha256()
            with path.open("rb") as source:
                for chunk in iter(lambda: source.read(1024 * 1024), b""):
                    digest.update(chunk)
            require(digest.hexdigest() == expected, "untracked file hash mismatch: " + relative)
    dirty = False
    if live:
        status = subprocess.run(["git", "-C", str(repo), "status", "--porcelain", "--untracked-files=no"],
                                check=True, capture_output=True, text=True, encoding="utf-8", timeout=30)
        dirty = bool(status.stdout.strip())
    if dirty:
        unfinished = (any(row["status"] != "complete" for row in receipt["objectives"]) or
                      any(row["status"] != "passed" for row in checks))
        require(receipt["outcome"] == "handoff" and unfinished,
            "dirty tracked files require handoff with unresolved objective or check")
    require(receipt["outcome"] != "complete" or not unresolved,
            "complete outcome contains unresolved work; record handoff")
    require(receipt["outcome"] != "handoff" or unresolved, "handoff requires explicit unresolved work")
    return receipt["outcome"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["check"])
    parser.add_argument("receipt", type=Path)
    args = parser.parse_args()
    try:
        outcome = check(args.receipt)
    except (ValueError, OSError, subprocess.SubprocessError, TypeError, KeyError) as error:
        print("closeout rejected: " + str(error), file=sys.stderr)
        return 1
    print("closeout evidence valid: " + outcome + "; remote state and inventory completeness not authenticated")
    return 0


if __name__ == "__main__":
    sys.exit(main())

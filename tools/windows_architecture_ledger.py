#!/usr/bin/env python3
"""Pin an exhaustive Windows-related tracked-source queue, NOT a security signoff.

The inclusion rule is deterministic: Rust, native tests, CI and tooling paths
whose names or tracked text mention Windows-specific execution. This captures
Windows gates in otherwise platform-neutral filenames. The decision is purely
syntactic and intentionally conservative; inclusion does not prove authority.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import io
import json
import re
import subprocess
from pathlib import Path

FIELDS = (
    "path", "git_blob_sha", "bytes", "line_count", "class",
    "proposed_owner", "risk_signal", "review_status", "evidence_required",
)
PATH_SIGNAL = re.compile(r"windows|win32|win64|webview2|lpac|authenticode", re.I)
TEXT_SIGNAL = re.compile(
    rb"(?i)\b(?:cfg\s*\(\s*windows|target_os\s*=\s*[\"']windows|"
    rb"windows[_\-\s]|Win32|WebView2|LPAC|Authenticode|"
    rb"windows-latest|windows_server|windows server)\b"
)
AUTHORITY = re.compile(
    rb"(?i)\b(?:unsafe|WinVerifyTrust|GetTokenInformation|"
    rb"CreateProcess|OpenProcess|CreateFileW|DeleteFileW|"
    rb"FILE_FLAG_OPEN_REPARSE_POINT|RevertToSelf|"
    rb"SetSecurityInfo|GetSecurityInfo|TerminateProcess|"
    rb"EventLoopProxy|JobObject|HANDLE)\b"
)
EXCLUDE = {".lock", ".png", ".jpg", ".ico", ".gitobj", ".jsonl"}
SUFFIXES = {".rs", ".py", ".ps1", ".sh", ".yml", ".yaml", ".toml", ".md", ".json"}
MANDATORY_CI_INPUTS = {"justfile", ".config/nextest.toml", ".agents/test-layout.md", ".agents/testing.md"}


def git(root: Path, *args: str, data: bytes | None = None) -> bytes:
    proc = subprocess.run(
        ["git", *args], cwd=root, input=data, capture_output=True, check=True
    )
    return proc.stdout


def entries(root: Path, ref: str) -> list[dict[str, str]]:
    tree = git(root, "ls-tree", "-r", "-z", "--full-tree", ref)
    records = []
    for item in tree.split(b"\0"):
        if not item:
            continue
        meta, path = item.split(b"\t", 1)
        mode, kind, blob = meta.decode().split()
        if kind != "blob":
            continue
        name = path.decode("utf-8", "surrogateescape")
        if name == "tools/windows_architecture_ledger.py":
            continue  # Avoid self-referential snapshot recursion; own PR reviews this tool.
        if name not in MANDATORY_CI_INPUTS and (not name.endswith(tuple(SUFFIXES)) or Path(name).suffix in EXCLUDE):
            continue
        if name not in MANDATORY_CI_INPUTS and not name.startswith(("crates/", "tools/", ".github/")):
            continue
        records.append(dict(path=name, oid=blob, mode=mode))
    return records


def classify(path: str) -> str:
    if path.startswith((".github/", "tools/", ".config/", ".agents/")) or path == "justfile":
        return "ci-tooling"
    if "/tests/" in path or path.endswith("/tests.rs") or "/test_" in path:
        return "test"
    if path.endswith(".rs"):
        return "production-or-fixture"
    return "support"


def proposed_owner(path: str) -> str:
    bits = path.split("/")
    if bits[0] == "crates" and len(bits) > 1:
        return bits[1]
    return "ci-security-maintainers" if bits[0] in (".github", "tools", ".config", ".agents") or path == "justfile" else "unassigned"


def render(root: Path, ref: str) -> tuple[str, dict[str, object]]:
    rows = []
    for item in entries(root, ref):
        path, oid = item["path"], item["oid"]
        blob = git(root, "cat-file", "blob", oid)
        if path not in MANDATORY_CI_INPUTS and not PATH_SIGNAL.search(path) and not TEXT_SIGNAL.search(blob):
            continue
        signal = []
        if AUTHORITY.search(blob):
            signal.append("potential-authority-or-unsafe")
        if b"#[ignore" in blob or b"#[ignore(" in blob:
            signal.append("ignored-test-selector")
        if b"symlink" in blob or b"reparse" in blob:
            signal.append("filesystem-link")
        if b"process" in blob.lower() or b"pid" in blob.lower():
            signal.append("process-identity")
        rows.append({
            "path": path, "git_blob_sha": oid, "bytes": str(len(blob)),
            "line_count": str(blob.count(b"\n") + (not blob.endswith(b"\n") and bool(blob))),
            "class": classify(path), "proposed_owner": proposed_owner(path),
            "risk_signal": ";".join(signal) or "review-needed",
            "review_status": "PENDING_INDEPENDENT_SEMANTIC_REVIEW",
            "evidence_required": "owner;imports;unsafe-security;test-selectors;native-ci;reviewer",
        })
    rows.sort(key=lambda x: x["path"])
    out = io.StringIO()
    writer = csv.DictWriter(out, fieldnames=FIELDS, lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    digest = hashlib.sha256(out.getvalue().encode()).hexdigest()
    count_by_class = {key: sum(r["class"] == key for r in rows) for key in sorted({r["class"] for r in rows})}
    summary = {
        "scope": "candidate Windows-related tracked paths; not all files nor security signoff",
        "pinned_ref": ref,
        "tracked_candidates": len(rows),
        "reviewed": 0,
        "pending_independent_semantic_review": len(rows),
        "classes": count_by_class,
        "manifest_sha256": digest,
    }
    return out.getvalue(), summary


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--ref", default="HEAD")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    root = args.repo.resolve()
    content, summary = render(root, args.ref)
    destination = root / args.output
    summary_path = destination.with_suffix(".summary.json")
    if args.check:
        assert destination.read_bytes() == content.encode("utf-8"), "Windows ledger byte drift"
        assert json.loads(summary_path.read_text(encoding="utf-8")) == summary, "summary drift"
    else:
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(content.encode("utf-8"))
        summary_path.write_bytes((json.dumps(summary, indent=2, sort_keys=True) + "\n").encode("utf-8"))
    print(json.dumps(summary, sort_keys=True))


if __name__ == "__main__":
    main()
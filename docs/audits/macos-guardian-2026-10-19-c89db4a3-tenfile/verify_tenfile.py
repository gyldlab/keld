#!/usr/bin/env python3
"""Structurally check an exact-blob, conservative 10-file Guardian audit delta.

This checks immutable Git identities, statuses and joined evidence, not prose
correctness, native tests, or runtime security acceptance.
"""
from __future__ import annotations

import collections
import csv
import json
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
BASE = HERE.parent / "macos-guardian-2026-10-19-a2952a0f-followon"
PARENT = "c89db4a37de966578f16405c45075c79ffeee897"
HIST = "899e287ea487f38ec1cd347f7c9db8f1927348ef"
SOURCE = "77b1c5aef9202a6cc0d84195fc1f4c32e1eb1cba"
PARENT_REL = "docs/audits/macos-guardian-2026-10-19-a2952a0f-followon"
NEW_COLS = ("batch10_source_status", "batch10_evidence_file", "batch10_disposition")
REVIEW_COLS = (
    "path", "blob_949", "blob_956", "review_class", "responsibility_and_callers",
    "invariant", "selectors_or_consumers", "fixture_and_independent_oracle",
    "source_finding", "disposition_and_gap", "source_lines",
)
CHANGED = {
    "crates/keld-runtime/src/macos_guardian/authenticated_runner.rs",
    "crates/keld-runtime/src/macos_guardian/process_group.rs",
    "crates/keld-runtime/src/macos_guardian/tests/process_group_failsafe.rs",
}
UNCHANGED = {
    "crates/keld-runtime/src/macos_guardian.rs",
    "crates/keld-runtime/src/macos_guardian/supervised_ack.rs",
    "crates/keld-runtime/src/macos_guardian/tests/rejected_registration_deadline.rs",
    "crates/keld-runtime/src/macos_guardian/tests/pre_registration_host_death.rs",
    "crates/keld-runtime/src/macos_guardian/tests/guardian_crash_pre_registration.rs",
    "crates/keld-runtime/src/macos_guardian/tests/terminal_liveness.rs",
    "crates/keld-runtime/src/macos_guardian/tests/worker_outcome.rs",
}
COUNTS = {
    "changed-since-949-pending": 3,
    "historical-scoped-only": 4,
    "pending-source-review": 42,
    "source-reviewed-limited": 5,
    "source-reviewed-limited-batch10": 7,
    "source-reviewed-new": 7,
    "source-reviewed-production-followon": 4,
    "source-reviewed-reused": 5,
}

# Deliberately finite source-owned callsite anchors, not a machine-generated
# semantic claim that all surrounding production/test behaviors are reviewed.
# Every anchor is a real historically pinned source line containing a reviewed
# operation, native selector, or a specifically documented unresolved cut.
SOURCE_ANCHORS = {
    "macos_guardian.rs": (128, 131, 142, 167, 206),
    "authenticated_runner.rs": (13, 109, 132, 139, 209),
    "process_group.rs": (196, 236, 269, 311, 410, 434, 469, 493, 515, 536),
    "supervised_ack.rs": (17, 26, 52, 127, 155, 185, 205),
    "process_group_failsafe.rs": (112, 150, 190, 268, 379, 393, 409, 414, 459, 527),
    "rejected_registration_deadline.rs": (61, 113, 173, 272, 288, 302, 328),
    "pre_registration_host_death.rs": (49, 120, 152, 236, 284),
    "guardian_crash_pre_registration.rs": (32, 152, 249, 253),
    "terminal_liveness.rs": (54, 102),
    "worker_outcome.rs": (18, 116, 195),
}


class Invalid(ValueError):
    """A claimed source record contradicts immutable evidence."""


def require(condition: bool, reason: str) -> None:
    if not condition:
        raise Invalid(reason)


def git(repo: Path, *args: str) -> str:
    result = subprocess.run(("git", "-C", str(repo), *args), capture_output=True, check=False)
    if result.returncode:
        raise Invalid(f"missing immutable Git evidence for {args[0]}: fetch pinned commits without rewriting refs")
    return result.stdout.decode().strip()


def original_bytes(repo: Path, path: str) -> bytes:
    result = subprocess.run(("git", "-C", str(repo), "show", f"{PARENT}:{path}"), capture_output=True, check=False)
    if result.returncode:
        raise Invalid(f"missing original parent document: {path}")
    return result.stdout


def rows(path: Path) -> tuple[list[str], list[dict[str, str]]]:
    with path.open(newline="", encoding="utf-8") as stream:
        reader = csv.DictReader(stream, strict=True)
        names = reader.fieldnames or []
        values = list(reader)
    require(bool(names), f"missing CSV header in {path.name}")
    require(all(None not in record and all(value is not None for value in record.values()) for record in values), f"malformed CSV: {path.name}")
    require(len({record["path"] for record in values}) == len(values) and all(record["path"] for record in values), f"duplicate or empty path in {path.name}")
    return names, values


def check_source_ranges(repo: Path, record: dict[str, str]) -> None:
    """Require real pinned line ranges for each named source-level claim.

    This cannot prove that prose is true. It catches a falsely green audit
    whose named native selector/cleanup branch was left OUTSIDE its citation.
    """
    path = record["path"]
    descriptor = record["source_lines"]
    match = re.fullmatch(r"([^:]+\.rs):(\d+-\d+(?:,\d+-\d+)*)", descriptor)
    require(match is not None, f"invalid source citation syntax: {path}")
    prefix, slices = match.groups()
    require(path.endswith("/" + prefix) or path == prefix, f"citation names wrong source file: {path}")
    lines = git(repo, "show", f"{SOURCE}:{path}").splitlines()
    last = 0
    ranges = []
    for interval in slices.split(","):
        begin, end = (int(n) for n in interval.split("-"))
        require(last < begin <= end <= len(lines), f"overlap, gap order, or invalid source interval {interval}: {path}")
        ranges.append((begin, end))
        last = end
    require(all(any(begin <= anchor <= end for begin, end in ranges) for anchor in SOURCE_ANCHORS[Path(path).name]), f"missing named source operation/native selector in reviewed citation: {path}")


def verify(repo: Path = ROOT, folder: Path = HERE) -> None:
    parent = repo / PARENT_REL
    for identity in (PARENT, HIST, SOURCE):
        require(git(repo, "rev-parse", identity) == identity, f"wrong immutable identity: {identity}")
    # The prior draft #967 evidence, including its structural proof, must be immutable.
    for name in ("README.md", "ledger_77.csv", "review_append.csv", "summary.json", "verify_followon.py", "test_verify_followon.py"):
        relative = f"{PARENT_REL}/{name}"
        require((parent / name).read_bytes().replace(b"\r\n", b"\n") == original_bytes(repo, relative), f"parent #967 changed: {name}")

    parent_columns, previous = rows(parent / "ledger_77.csv")
    columns, current = rows(folder / "ledger_77.csv")
    review_columns, evidence = rows(folder / "review_append.csv")
    summary = json.loads((folder / "summary.json").read_text(encoding="utf-8"))
    require(columns == parent_columns + list(NEW_COLS), "original ledger columns changed")
    require(review_columns == list(REVIEW_COLS), "review schema changed")
    require(len(previous) == len(current) == 77, "77-file historical ledger altered")
    require(len(evidence) == 10 and {row["path"] for row in evidence} == CHANGED | UNCHANGED, "unexpected ten-file coverage")
    old_by_path = {record["path"]: record for record in previous}
    new_by_path = {record["path"]: record for record in current}
    review_by_path = {record["path"]: record for record in evidence}
    require(set(new_by_path) == set(old_by_path), "historical file membership changed")
    classes = collections.Counter()
    for path, old in old_by_path.items():
        new = new_by_path[path]
        require(all(new[key] == old[key] for key in parent_columns), f"parent historical evidence mutated: {path}")
        if path in review_by_path:
            record = review_by_path[path]
            klass = "changed-overlay" if path in CHANGED else "facade" if path.endswith("macos_guardian.rs") else "ack" if path.endswith("supervised_ack.rs") else "test"
            require(record["review_class"] == klass, f"false source review class: {path}")
            require(all(record[field].strip() for field in REVIEW_COLS[4:]), f"unsubstantiated source evidence: {path}")
            check_source_ranges(repo, record)
            require(record["blob_949"] == old["tip_blob"] == git(repo, "rev-parse", f"{HIST}:{path}"), f"incorrect historical blob: {path}")
            require(record["blob_956"] == old["blob_952"] == git(repo, "rev-parse", f"{SOURCE}:{path}"), f"incorrect source-tip blob: {path}")
            expected = "changed-since-949-pending" if path in CHANGED else "source-reviewed-limited-batch10"
            inherited = "changed-since-949-pending" if path in CHANGED else "pending-source-review"
            require(old["followon_source_status"] == inherited, f"already-reviewed row upgraded again: {path}")
            require(new["batch10_source_status"] == expected, f"unsupported status upgrade: {path}")
            require(new["batch10_evidence_file"] == "review_append.csv" and new["batch10_disposition"] == record["disposition_and_gap"], f"evidence join mismatch: {path}")
        else:
            require(new["batch10_source_status"] == old["followon_source_status"], f"unreviewed row promoted: {path}")
            require(not new["batch10_evidence_file"] and not new["batch10_disposition"], f"unreviewed row claims evidence: {path}")
        classes[new["batch10_source_status"]] += 1
    require(dict(classes) == COUNTS, "status histogram mismatch")
    require(summary["status_counts"] == COUNTS, "summary status counts mismatch")
    require(summary["parent_head"] == PARENT and summary["historical_949_head"] == HIST and summary["source_tip_952"] == SOURCE, "summary source identities mismatch")
    require(summary["schema_version"] == 1 and summary["parent_pr"] == 967 and summary["parent_source_documents_unchanged"] is True, "summary provenance contract mismatch")
    require(summary["historical_paths"] == 77 and summary["source_tip_paths"] == 78, "scope size mismatch")
    require(summary["new_review_records"] == 10 and summary["unchanged_limited_reviews"] == 7 and summary["changed_source_limited_overlays"] == 3, "review counts mismatch")
    require(set(summary["changed_historical_full_review_still_pending"]) == CHANGED and summary["pending_historical_full_review"] == 42, "unresolved source rows falsely cleared")
    require(summary["additional_952_only_path_still_pending"] == ["crates/keld-runtime/src/macos_guardian/authenticated_runner/tests.rs"], "new #952 source file omitted")
    require(summary["all_77_source_complete"] is False and summary["merge_authorized"] is False, "false full acceptance/merge claim")
    require(summary["native_tests_rerun_for_documentation"] is False and summary["independent_semantic_review_of_new_ledger_pending"] is True, "unsubstantiated execution or review claim")
    print("Guardian 10-file structural evidence: PASS (77 immutable paths, 7 new limited, 3 changed limited overlays; prose and OS guarantees NOT certified)")


if __name__ == "__main__":
    try:
        verify()
    except (OSError, Invalid, KeyError, ValueError, subprocess.SubprocessError) as exc:
        print(f"Guardian 10-file structural evidence: FAIL: {exc}", file=sys.stderr)
        sys.exit(1)

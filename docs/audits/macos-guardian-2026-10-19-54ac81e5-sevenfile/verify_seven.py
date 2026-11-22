#!/usr/bin/env python3
"""Conservative structural verification of seven pinned Guardian file reviews.

Exact source identity, bounded source-citation coverage, joins and accounting
are checked. This does NOT verify review prose, physical custody, native test
results or the full KELD architecture.
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
PARENT = "7cf35e99dd02f4aa58158eb0b40f9a4ea82e0224"
BASELINE = "899e287ea487f38ec1cd347f7c9db8f1927348ef"
SOURCE = "77b1c5aef9202a6cc0d84195fc1f4c32e1eb1cba"
PARENT_REL = "docs/audits/macos-guardian-2026-10-19-2c488b9a-next8"
PATH_ROOT = "crates/keld-runtime/src/macos_guardian/tests/"
ADDED = ("batch7_source_status", "batch7_evidence_file", "batch7_disposition")
REVIEW_FIELDS = (
    "path", "blob_949", "blob_952", "review_class", "responsibility_and_callers",
    "invariant", "selectors_or_consumers", "fixture_and_independent_oracle",
    "source_finding", "disposition_and_gap", "source_lines",
)
LIMITED = "source-reviewed-limited-batch7"

# Source-reviewed historical snapshot operations, not an assertion that all
# intervening code or all OS schedules are verified. Anchors include the
# native/protocol negative assertion and cited test helper's known gap.
REVIEWED = {
    PATH_ROOT + "control_acknowledgement.rs": (24, 51, 63, 68, 85, 95, 101, 117, 125, 149, 162, 190, 225, 238, 250),
    PATH_ROOT + "generation_control_wire.rs": (15, 20, 28, 35, 41, 53, 57, 66, 70, 74, 79, 100, 104, 110, 118, 126, 136, 146, 152, 168, 195, 208),
    PATH_ROOT + "generic_pre_registration_crash.rs": (22, 28, 39, 60, 65, 87, 92, 107, 110, 124, 126, 130, 136, 142, 147, 151, 156, 163),
    PATH_ROOT + "generic_pre_registration_crash/cleanup_authority.rs": (15, 27, 35, 43, 46, 58, 67, 70, 85),
    PATH_ROOT + "initial_generation_adversaries.rs": (23, 33, 41, 49, 51, 63, 79, 89, 102, 105, 108, 117, 123, 125, 145, 154, 160, 175, 185, 199, 202),
    PATH_ROOT + "registration_record_adversaries.rs": (21, 31, 32, 40, 45, 49, 64, 76, 79, 80, 89, 94, 99, 106, 111, 116, 121, 126),
    PATH_ROOT + "supervised_bootstrap.rs": (17, 19, 25, 30, 52, 62, 68, 73, 83, 105, 115, 124, 134, 140, 145, 159, 163, 167, 194, 225, 248, 273, 283, 290, 318, 329, 340, 342, 347),
}
CITATION = re.compile(r"([^:]+\.rs):(\d+-\d+(?:,\d+-\d+)*)")
CHANGED = [
    "crates/keld-runtime/src/macos_guardian/authenticated_runner.rs",
    "crates/keld-runtime/src/macos_guardian/process_group.rs",
    "crates/keld-runtime/src/macos_guardian/tests/process_group_failsafe.rs",
]
EXTRA = "crates/keld-runtime/src/macos_guardian/authenticated_runner/tests.rs"


class Invalid(ValueError):
    """An evidence claim conflicts with immutable source/ledger structure."""


def require(condition: bool, why: str) -> None:
    if not condition:
        raise Invalid(why)


def git_bytes(repo: Path, *args: str) -> bytes:
    result = subprocess.run(("git", "-C", str(repo), *args), capture_output=True, check=False)
    if result.returncode:
        raise Invalid(f"missing original pinned Git evidence for {args[0]}")
    return result.stdout


def git_text(repo: Path, *args: str) -> str:
    return git_bytes(repo, *args).decode("utf-8").strip()


def read_csv(file: Path) -> tuple[list[str], list[dict[str, str]]]:
    with file.open(newline="", encoding="utf-8") as stream:
        reader = csv.DictReader(stream, strict=True)
        fields, rows = list(reader.fieldnames or []), list(reader)
    require(bool(fields), f"missing source review CSV schema: {file.name}")
    require(all(None not in row and all(v is not None for v in row.values()) for row in rows), f"malformed CSV: {file.name}")
    require(all(row.get("path") for row in rows) and len({row["path"] for row in rows}) == len(rows), f"duplicate or empty source path: {file.name}")
    return fields, rows


def verify_source_spans(repo: Path, path: str, expression: str) -> None:
    match = CITATION.fullmatch(expression)
    require(match is not None, f"malformed exact-blob source citation: {path}")
    leaf, spans = match.groups()
    require(path.endswith("/" + leaf), f"wrong file in source span: {path}")
    original = git_bytes(repo, "show", f"{SOURCE}:{path}").decode("utf-8").splitlines()
    intervals: list[tuple[int, int]] = []
    last = 0
    for span in spans.split(","):
        begin, end = (int(value) for value in span.split("-"))
        require(last < begin <= end <= len(original), f"nonexistent/overlapping line interval {span}: {path}")
        intervals.append((begin, end))
        last = end
    for source_line in REVIEWED[path]:
        require(any(begin <= source_line <= end for begin, end in intervals), f"named native/clock/security assertion line {source_line} omitted: {path}")


def verify(repo: Path = ROOT, folder: Path = HERE) -> None:
    for pinned in (PARENT, BASELINE, SOURCE):
        require(git_text(repo, "rev-parse", pinned) == pinned, f"wrong immutable review identity: {pinned}")
    parent = repo / PARENT_REL
    for name in ("README.md", "ledger_77.csv", "review_append.csv", "summary.json", "verify_next8.py", "test_verify_next8.py"):
        relative = f"{PARENT_REL}/{name}"
        checkout = (parent / name).read_bytes().replace(b"\r\n", b"\n")
        require(checkout == git_bytes(repo, "show", f"{PARENT}:{relative}"), f"altered parent #984 source evidence: {name}")

    original_cols, old_rows = read_csv(parent / "ledger_77.csv")
    new_cols, new_rows = read_csv(folder / "ledger_77.csv")
    review_cols, receipts = read_csv(folder / "review_append.csv")
    summary = json.loads((folder / "summary.json").read_text(encoding="utf-8"))
    require(new_cols == original_cols + list(ADDED), "historical ledger field mutation")
    require(review_cols == list(REVIEW_FIELDS), "source evidence schema changed")
    require(len(old_rows) == len(new_rows) == 77, "historical 77-file scope changed")
    require(len(receipts) == 7 and {row["path"] for row in receipts} == set(REVIEWED), "seven-source coverage not exact")
    old = {row["path"]: row for row in old_rows}
    new = {row["path"]: row for row in new_rows}
    reviews = {row["path"]: row for row in receipts}
    require(set(old) == set(new), "an inherited source path was invented or removed")
    statuses = collections.Counter()
    for path, original in old.items():
        current = new[path]
        require(all(current[col] == original[col] for col in original_cols), f"parent #984 evidence mutated: {path}")
        if path in reviews:
            record = reviews[path]
            require(original["batch8b_source_status"] == "pending-source-review", f"double counted earlier source review: {path}")
            require(record["review_class"] == "source-reviewed-limited", f"unearned full-source status: {path}")
            require(all(record[name].strip() for name in REVIEW_FIELDS[4:]), f"blank native/caller/invariant review: {path}")
            require(record["blob_949"] == original["tip_blob"] == git_text(repo, "rev-parse", f"{BASELINE}:{path}"), f"wrong baseline source blob: {path}")
            require(record["blob_952"] == original["blob_952"] == git_text(repo, "rev-parse", f"{SOURCE}:{path}"), f"wrong source-tip blob: {path}")
            require(record["blob_949"] == record["blob_952"], f"changed-source row cannot be upgraded: {path}")
            verify_source_spans(repo, path, record["source_lines"])
            require(current["batch7_source_status"] == LIMITED, f"unsupported exact-source status: {path}")
            require(current["batch7_evidence_file"] == "review_append.csv" and current["batch7_disposition"] == record["disposition_and_gap"], f"source evidence/ledger join mismatch: {path}")
        else:
            require(current["batch7_source_status"] == original["batch8b_source_status"], f"unreviewed source row promoted: {path}")
            require(not current["batch7_evidence_file"] and not current["batch7_disposition"], f"invented review receipt: {path}")
        statuses[current["batch7_source_status"]] += 1

    require(summary["schema_version"] == 1 and summary["parent_pr"] == 984 and summary["parent_head"] == PARENT and summary["parent_source_documents_unchanged"] is True, "false parent provenance")
    require(summary["historical_949_head"] == BASELINE and summary["source_tip_952"] == SOURCE, "historical source version changed")
    require(summary["historical_path_count"] == 77 and summary["parent_pending_historical"] == 26 and summary["new_limited_review_count"] == 7 and summary["current_pending_historical"] == 19, "historical source accounting false")
    require(statuses["pending-source-review"] == 19 and statuses[LIMITED] == 7 and dict(statuses) == summary["source_review_status_counts"], "source histogram inconsistent")
    require(summary["changed_blobs_pending"] == CHANGED and summary["additional_952_only_file_pending"] == EXTRA, "changed/additional source prematurely cleared")
    for flag in ("native_suites_rerun_for_docs", "all_77_historical_source_fully_reviewed", "kernel_custody_proven", "merge_authorized"):
        require(summary[flag] is False, f"invented native/OS/security/merge result: {flag}")
    require(summary["independent_exact_head_review_pending"] is True, "unperformed independent semantic review certified")
    print("Guardian seven-file structural evidence: PASS (77 immutable historical rows, 7 limited reviews, 19 still pending, no native/security certificate)")


if __name__ == "__main__":
    try:
        verify()
    except (OSError, Invalid, KeyError, ValueError, subprocess.SubprocessError) as exc:
        print(f"Guardian seven-file structural evidence: FAIL: {exc}", file=sys.stderr)
        sys.exit(1)

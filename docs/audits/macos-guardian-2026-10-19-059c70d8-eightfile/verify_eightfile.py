#!/usr/bin/env python3
"""Check eight exact-blob Guardian fixture reviews against immutable #976.

Structural and source-citation checks ONLY; does not certify review prose,
native test execution, kernel authority, physical custody, or merge safety.
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
PARENT = "059c70d88c1841c58bc85907bdf9424fc29adb99"
BASELINE = "899e287ea487f38ec1cd347f7c9db8f1927348ef"
SOURCE = "77b1c5aef9202a6cc0d84195fc1f4c32e1eb1cba"
PARENT_REL = "docs/audits/macos-guardian-2026-10-19-c89db4a3-tenfile"
TEST_ROOT = "crates/keld-runtime/src/macos_guardian/tests/"
ADDED = ("batch8_source_status", "batch8_evidence_file", "batch8_disposition")
REVIEW_FIELDS = (
    "path", "blob_949", "blob_952", "review_class", "responsibility_and_callers",
    "invariant", "selectors_or_consumers", "fixture_and_independent_oracle",
    "source_finding", "disposition_and_gap", "source_lines",
)

# Paths are exact and remain source-historically PENDING in #976. Numeric
# anchors insist that critical native survival/cleanup and own-time cuts are
# present in citations; they do not claim semantically exhaustive source read.
REVIEWED = {
    TEST_ROOT + "retained_ack.rs": (48, 52, 93, 166, 175, 225, 254, 275, 287, 299, 330, 341, 347, 352),
    TEST_ROOT + "retained_ack/dual_death.rs": (19, 65, 82, 90, 101, 106, 115),
    TEST_ROOT + "retained_ack/escaped_descendant.rs": (23, 71, 125, 128, 133, 157, 162),
    TEST_ROOT + "retained_ack/escaped_descendant/cleanup_tests.rs": (59, 98, 141, 168, 198, 235, 266, 278, 331, 337, 360),
    TEST_ROOT + "retained_ack/observers.rs": (17, 35, 46, 53, 68, 77, 80, 88, 95, 102, 106),
    TEST_ROOT + "guardian_crash_pre_registration/cleanup_authority.rs": (16, 69, 97, 127, 148, 155),
    TEST_ROOT + "pre_registration_host_death/cleanup_authority.rs": (15, 82, 103, 125, 126),
    TEST_ROOT + "runtime_race_validation/ownership_races.rs": (43, 116, 146, 156, 183, 195, 198, 203, 208, 231, 234, 238),
}
SOURCE_LINES = re.compile(r"([^:]+\.rs):(\d+-\d+(?:,\d+-\d+)*)")


class Invalid(ValueError):
    """A claimed audit relationship does not match its immutable source."""


def require(test: bool, why: str) -> None:
    if not test:
        raise Invalid(why)


def git(repo: Path, *args: str) -> str:
    result = subprocess.run(("git", "-C", str(repo), *args), capture_output=True, check=False)
    if result.returncode:
        raise Invalid(f"pinned Git source missing: {args[0]}, fetch original history")
    return result.stdout.decode().strip()


def table(path: Path) -> tuple[list[str], list[dict[str, str]]]:
    with path.open(newline="", encoding="utf-8") as stream:
        source = csv.DictReader(stream, strict=True)
        fields, rows = source.fieldnames or [], list(source)
    require(fields and all(None not in r and None not in r.values() for r in rows), f"invalid CSV shape: {path.name}")
    require(len(rows) == len(set(r["path"] for r in rows)) and all(r["path"] for r in rows), f"duplicate/empty source path: {path.name}")
    return fields, rows


def check_citations(repo: Path, path: str, citation: str) -> None:
    match = SOURCE_LINES.fullmatch(citation)
    require(match is not None, f"not an ordered exact-source citation: {path}")
    leaf, intervals = match.groups()
    require(path.endswith("/" + leaf), f"cited file does not match original path: {path}")
    count = len(git(repo, "show", f"{SOURCE}:{path}").splitlines())
    pairs: list[tuple[int, int]] = []
    ending = 0
    for interval in intervals.split(","):
        lo, hi = (int(part) for part in interval.split("-"))
        require(ending < lo <= hi <= count, f"overlapping or nonexistent source lines: {path}")
        pairs.append((lo, hi))
        ending = hi
    for anchor in REVIEWED[path]:
        require(any(lo <= anchor <= hi for lo, hi in pairs), f"missing native security-custody/clock code citation line {anchor}: {path}")


def verify(repo: Path = ROOT, folder: Path = HERE) -> None:
    for sha in (PARENT, BASELINE, SOURCE):
        require(git(repo, "rev-parse", sha) == sha, f"wrong pinned Git identity: {sha}")
    parent = repo / PARENT_REL
    for name in ("README.md", "review_append.csv", "ledger_77.csv", "summary.json", "verify_tenfile.py", "test_verify_tenfile.py"):
        original = subprocess.run(("git", "-C", str(repo), "show", f"{PARENT}:{PARENT_REL}/{name}"), capture_output=True, check=False)
        require(original.returncode == 0, f"missing parent source document: {name}")
        # Git may select CRLF text checkout on Windows. Compare the same
        # logical tracked bytes while refusing all other content changes.
        checked_out = (parent / name).read_bytes().replace(b"\r\n", b"\n")
        require(checked_out == original.stdout, f"mutated original source review #976: {name}")

    parent_fields, previous = table(parent / "ledger_77.csv")
    fields, updated = table(folder / "ledger_77.csv")
    added_fields, records = table(folder / "review_append.csv")
    summary = json.loads((folder / "summary.json").read_text(encoding="utf-8"))
    require(fields == parent_fields + list(ADDED), "modified inherited 77-source columns")
    require(added_fields == list(REVIEW_FIELDS), "source evidence schema changed")
    require(len(previous) == len(updated) == 77, "historical 77-path inventory changed")
    require(len(records) == 8 and {r["path"] for r in records} == set(REVIEWED), "eight independently-reviewed fixture paths changed")
    old = {r["path"]: r for r in previous}
    new = {r["path"]: r for r in updated}
    receipts = {r["path"]: r for r in records}
    require(set(old) == set(new), "invented or dropped historical file")
    statuses = collections.Counter()
    for path, oldrow in old.items():
        now = new[path]
        require(all(now[key] == oldrow[key] for key in parent_fields), f"inherited review/status field mutated: {path}")
        if path in receipts:
            evidence = receipts[path]
            require(oldrow["batch10_source_status"] == "pending-source-review", f"double-counting already completed review: {path}")
            require(evidence["review_class"] == "source-reviewed-limited", f"false full-source claim: {path}")
            require(all(evidence[name].strip() for name in REVIEW_FIELDS[4:]), f"missing manually grounded source evidence: {path}")
            require(evidence["blob_949"] == oldrow["tip_blob"] == git(repo, "rev-parse", f"{BASELINE}:{path}"), f"wrong original #949 blob: {path}")
            require(evidence["blob_952"] == oldrow["blob_952"] == git(repo, "rev-parse", f"{SOURCE}:{path}"), f"wrong original #952 blob: {path}")
            require(evidence["blob_949"] == evidence["blob_952"], f"changed historical blob must remain pending: {path}")
            check_citations(repo, path, evidence["source_lines"])
            require(now["batch8_source_status"] == "source-reviewed-limited-batch8", f"incorrect conservative source status: {path}")
            require(now["batch8_evidence_file"] == "review_append.csv" and now["batch8_disposition"] == evidence["disposition_and_gap"], f"review receipt/ledger join mismatch: {path}")
        else:
            require(now["batch8_source_status"] == oldrow["batch10_source_status"], f"unreviewed file incorrectly promoted: {path}")
            require(not now["batch8_evidence_file"] and not now["batch8_disposition"], f"invented evidence for unreviewed: {path}")
        statuses[now["batch8_source_status"]] += 1
    require(summary["schema_version"] == 1 and summary["parent_pr"] == 976, "source provenance/schema ungrounded")
    require(summary["parent_head"] == PARENT and summary["baseline_949_head"] == BASELINE and summary["source_tip_952"] == SOURCE, "historical source version switched")
    require(summary["historical_path_count"] == 77 and summary["parent_pending_historical"] == 42, "historical scope or starting count false")
    require(summary["new_limited_review_count"] == 8 and summary["current_pending_historical"] == 34 and statuses["pending-source-review"] == 34, "review count or remaining scope misleading")
    require(dict(statuses) == summary["source_review_status_counts"], "summary misrepresents review statuses")
    require(summary["changed_blobs_pending"] == ["crates/keld-runtime/src/macos_guardian/authenticated_runner.rs", "crates/keld-runtime/src/macos_guardian/process_group.rs", "crates/keld-runtime/src/macos_guardian/tests/process_group_failsafe.rs"], "changed-source overlays falsely closed")
    require(summary["additional_952_only_file_pending"] == "crates/keld-runtime/src/macos_guardian/authenticated_runner/tests.rs", "new #952-only source falsely closed")
    for flag in ("native_suites_rerun", "all_historical_files_fully_reviewed", "kernel_custody_proven", "merge_authorized"):
        require(summary[flag] is False, f"falsely certifies native execution or security completion: {flag}")
    require(summary["independent_exact_head_review_pending"] is True, "uncompleted independent source audit falsely certified")
    print("Guardian eight-file structural evidence: PASS (77 immutable paths, 8 exact unchanged limited reviews; 34 pending; no OS security certificate)")


if __name__ == "__main__":
    try:
        verify()
    except (OSError, Invalid, KeyError, ValueError, subprocess.SubprocessError) as exc:
        print(f"Guardian eight-file structural evidence: FAIL: {exc}", file=sys.stderr)
        sys.exit(1)

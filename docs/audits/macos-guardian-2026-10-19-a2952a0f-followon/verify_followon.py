#!/usr/bin/env python3
"""Offline, stdlib-only structural verifier for historical #824 Guardian CSV evidence.

Verifies Git object identities and accounting; never certifies prose source claims.
Requires locally available pinned commit objects (no network fetches).
"""
from __future__ import annotations

import collections
import csv
import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
HIST = "899e287ea487f38ec1cd347f7c9db8f1927348ef"
SOURCE = "77b1c5aef9202a6cc0d84195fc1f4c32e1eb1cba"
PARENT = "a2952a0f95eb58fe50de335601383294eccd0d59"
ORIGINAL = "docs/audits/macos-guardian-2026-10-18-ea728589/ledger.csv"
PARENT_LEDGER = "docs/audits/macos-guardian-2026-10-19-77b1c5ae/ledger_77.csv"
ROOTS = (
    "crates/keld-runtime/src/macos_guardian.rs",
    "crates/keld-runtime/src/macos_guardian/",
    "crates/keld-runtime/tests/macos_host_death_guardian.rs",
    "crates/keld-runtime/tests/macos_host_death_guardian/",
)
COUNTS = {
    "changed-since-949-pending": 3,
    "historical-scoped-only": 4,
    "pending-source-review": 49,
    "source-reviewed-limited": 5,
    "source-reviewed-new": 7,
    "source-reviewed-production-followon": 4,
    "source-reviewed-reused": 5,
}
HIST_FIELDS = ("path", "tip_blob", "main_blob", "main_relation", "category", "component", "review_status", "evidence_pr", "disposition")
PARENT_FIELDS = ("blob_952", "source_review_20261019", "evidence")
NEW_FIELDS = ("followon_source_status", "followon_evidence_file", "followon_disposition")
REVIEW_FIELDS = ("path", "blob_949", "blob_956", "review_class", "responsibility_and_callers", "invariant", "selectors_or_consumers", "fixture_and_independent_oracle", "source_finding", "disposition_and_gap", "source_lines")


class Invalid(ValueError):
    """A ledger claim did not match the immutable Git evidence."""


def require(ok: bool, message: str) -> None:
    if not ok:
        raise Invalid(message)


def git(repo: Path, *args: str) -> str:
    result = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True, check=False)
    if result.returncode:
        raise Invalid(f"required pinned Git object unavailable for {args[0]}: fetch {HIST}, {SOURCE} and {PARENT} into this checkout (no automatic fetch)")
    return result.stdout.strip()


def git_bytes(repo: Path, revision: str, path: str) -> bytes:
    result = subprocess.run(
        ["git", "-C", str(repo), "show", f"{revision}:{path}"],
        capture_output=True, check=False,
    )
    if result.returncode:
        raise Invalid(f"pinned historical file unavailable: {revision}:{path}")
    return result.stdout


def blob(repo: Path, revision: str, path: str) -> str:
    return git(repo, "rev-parse", f"{revision}:{path}")


def tree_paths(repo: Path, revision: str) -> set[str]:
    names = git(repo, "ls-tree", "-r", "--name-only", revision).splitlines()
    return {name for name in names if name in ROOTS or any(name.startswith(root) for root in ROOTS if root.endswith("/"))}


def read_csv(path: Path, fields: tuple[str, ...]) -> list[dict[str, str]]:
    require(path.is_file(), f"missing {path.name}")
    with path.open(newline="", encoding="utf-8") as stream:
        reader = csv.DictReader(stream, strict=True)
        require(reader.fieldnames == list(fields), f"unexpected {path.name} schema")
        records = list(reader)
    require(all(None not in record and all(value is not None for value in record.values()) for record in records), f"malformed {path.name} record")
    return records


def checkout_bytes(path: Path) -> bytes:
    # Git checkout may select CRLF on Windows. Only normalize line endings;
    # commit-object content is still compared exactly after that conversion.
    return path.read_bytes().replace(b"\r\n", b"\n")


def indexed(rows: list[dict[str, str]], name: str) -> dict[str, dict[str, str]]:
    keys = [r["path"] for r in rows]
    require(all(keys), f"empty {name} path")
    require(len(set(keys)) == len(keys), f"duplicate {name} path")
    return {r["path"]: r for r in rows}


def verify(repo: Path = ROOT, folder: Path = HERE) -> None:
    # Check immutable baselines directly, including the parent snapshot even
    # though this branch is a later docs-only child.
    require(git(repo, "rev-parse", HIST) == HIST, "wrong historical revision")
    require(git(repo, "rev-parse", SOURCE) == SOURCE, "wrong source revision")
    require(git(repo, "rev-parse", PARENT) == PARENT, "wrong parent revision")
    require(checkout_bytes(repo / ORIGINAL) == git_bytes(repo, HIST, ORIGINAL), "historical published ledger bytes changed")
    require(checkout_bytes(repo / PARENT_LEDGER) == git_bytes(repo, PARENT, PARENT_LEDGER), "parent #956 ledger bytes changed")
    original = read_csv(repo / ORIGINAL, HIST_FIELDS)
    parent = read_csv(repo / PARENT_LEDGER, HIST_FIELDS + PARENT_FIELDS)
    current = read_csv(folder / "ledger_77.csv", HIST_FIELDS + PARENT_FIELDS + NEW_FIELDS)
    reviews = read_csv(folder / "review_append.csv", REVIEW_FIELDS)
    summary = json.loads((folder / "summary.json").read_text(encoding="utf-8"))
    original_map, parent_map, current_map, review_map = (
        indexed(rows, label) for rows, label in ((original, "original"), (parent, "parent"), (current, "followon"), (reviews, "review"))
    )
    historical_paths = tree_paths(repo, HIST)
    source_paths = tree_paths(repo, SOURCE)
    require(len(original_map) == 77 and set(original_map) == historical_paths, "historical 77-path Git membership mismatch")
    require(set(parent_map) == historical_paths and set(current_map) == historical_paths, "reconciled inventory differs from historical 77 paths")
    require(len(source_paths) == 78, "source tree no longer has pinned 78 paths")
    expected_extra = {"crates/keld-runtime/src/macos_guardian/authenticated_runner/tests.rs"}
    require(source_paths - historical_paths == expected_extra, "unexpected new source path")
    require(len(review_map) == 9 and set(review_map) <= historical_paths, "new review path count/membership mismatch")
    require(sum(row["review_class"] == "limited" for row in reviews) == 5 and sum(row["review_class"] == "new-production" for row in reviews) == 4, "new review class counts mismatch")
    require(all(row["review_class"] in ("limited", "new-production") for row in reviews), "unrecognized review class")

    changed = set()
    classes = collections.Counter()
    for path in sorted(historical_paths):
        base, inherited, row = original_map[path], parent_map[path], current_map[path]
        require(all(row[k] == base[k] for k in HIST_FIELDS), f"historical columns changed: {path}")
        require(all(row[k] == inherited[k] for k in HIST_FIELDS + PARENT_FIELDS), f"parent source evidence changed: {path}")
        require(blob(repo, HIST, path) == row["tip_blob"], f"incorrect historical Git blob: {path}")
        require(blob(repo, SOURCE, path) == row["blob_952"], f"incorrect #952 Git blob: {path}")
        if row["tip_blob"] != row["blob_952"]:
            changed.add(path)
        if path in review_map:
            review = review_map[path]
            expected = "source-reviewed-limited" if review["review_class"] == "limited" else "source-reviewed-production-followon"
            require(inherited["source_review_20261019"] == "pending-source-review", f"prior reviewed file reclassified: {path}")
            require(row["followon_source_status"] == expected and row["followon_evidence_file"] == "review_append.csv", f"new review disposition mismatch: {path}")
            require(row["followon_disposition"] == review["disposition_and_gap"], f"new review evidence not joined: {path}")
            require(review["blob_949"] == row["tip_blob"] and review["blob_956"] == row["blob_952"], f"new review Git blob mismatch: {path}")
            require(all(review[field].strip() for field in REVIEW_FIELDS[4:]), f"empty scoped source evidence: {path}")
        else:
            require(row["followon_source_status"] == inherited["source_review_20261019"], f"unreviewed status upgraded: {path}")
            require(row["followon_evidence_file"] == "", f"unreviewed gained a new evidence claim: {path}")
            require(row["followon_disposition"] == "", f"unreviewed disposition added: {path}")
        classes[row["followon_source_status"]] += 1
    require(classes == COUNTS, f"status histogram mismatch: {classes}")
    require(changed == set(summary["changed_blobs_pending"]), "changed blobs not kept pending")
    require(all(current_map[path]["followon_source_status"] == "changed-since-949-pending" for path in changed), "changed source marked reviewed")
    require(summary["historical_949_head"] == HIST and summary["underlying_source_tip"] == SOURCE and summary["followon_parent_head"] == PARENT, "summary revision identity mismatch")
    require(summary["historical_paths"] == 77 and summary["current_guardian_paths"] == 78, "summary scope counts mismatch")
    require(summary["new_review_entries"] == 9 and summary["limited_integration_reviews"] == 5 and summary["new_production_reviews"] == 4, "summary new reviews count mismatch")
    require(summary["parent_existing_source_review_entries"] == 12, "summary inherited reviews mismatch")
    require(summary["remaining_pending_historical_full_review"] == 49, "summary pending count mismatch")
    require(summary["status_counts_historical_77"] == COUNTS, "summary status histogram mismatch")
    require(summary["new_source_path_pending"] == sorted(expected_extra), "summary new source path mismatch")
    require(summary["merge_authorized"] is False and summary["all_77_source_complete"] is False, "false audit completion/merge claim")
    require(summary["native_tests_rerun_for_documentation"] is False, "source ledger falsely claims a native run")
    print("Guardian follow-on structural evidence: PASS (77 Git paths, 9 exact blobs, statuses and partial summary; prose meaning unverified)")


if __name__ == "__main__":
    try:
        verify()
    except (Invalid, OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
        print(f"Guardian follow-on structural evidence: FAIL: {exc}", file=sys.stderr)
        sys.exit(1)

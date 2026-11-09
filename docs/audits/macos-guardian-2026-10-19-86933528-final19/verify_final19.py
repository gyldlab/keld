#!/usr/bin/env python3
"""Check immutable provenance and limited scope of 19 historical Guardian reviews.

This is a structural audit checker. It cannot certify the review prose,
kernel process custody, full architecture completion or native execution.
"""
from __future__ import annotations

import collections
import csv
import functools
import json
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PARENT = "334bae72d27613ca744e5e3be86dac5b850d2e9f"
BASELINE = "899e287ea487f38ec1cd347f7c9db8f1927348ef"
SOURCE = "77b1c5aef9202a6cc0d84195fc1f4c32e1eb1cba"
PARENT_REL = "docs/audits/macos-guardian-2026-10-19-54ac81e5-sevenfile"
SOURCE_ROOT = "crates/keld-runtime/src/macos_guardian/"
ADDED = ("batch19_source_status", "batch19_evidence_file", "batch19_disposition")
REVIEW_FIELDS = (
    "path", "blob_949", "blob_952", "review_class",
    "responsibility_and_callers", "invariant", "selectors_or_consumers",
    "fixture_and_independent_oracle", "source_finding",
    "disposition_and_gap", "source_lines",
)
LIMITED = "source-reviewed-limited-batch19"
CHANGED = [
    SOURCE_ROOT + "authenticated_runner.rs",
    SOURCE_ROOT + "process_group.rs",
    SOURCE_ROOT + "tests/process_group_failsafe.rs",
]
EXTRA = SOURCE_ROOT + "authenticated_runner/tests.rs"
SOURCE_SCOPE = (
    "All 19 remaining historical test files received LIMITED scoped source review; "
    "77 historical paths have statuses but scope-specific semantics and changed "
    "versions need reconciliation. This is not a whole-repo census or kernel "
    "custody guarantee."
)

# Literal source citations reviewed before this verifier was written. Independent
# expected strings prevent a mutated CSV from silently deleting an entire source
# interval. The Git blob checks below bind every cited line to historical bytes.
# Inclusion of an interval is NOT proof that the observation or oracle is sound.
SOURCE_SPANS = {
    SOURCE_ROOT + "guarded_lease/tests/native_activity.rs":
        "native_activity.rs:10-42,43-66,67-77,78-93,94-108",
    SOURCE_ROOT + "tests/guardian_crash_pre_registration/retained_ack.rs":
        "retained_ack.rs:17-49,50-87,88-96",
    SOURCE_ROOT + "tests/guardian_crash_pre_registration/retained_ack/observers.rs":
        "observers.rs:14-50,51-80",
    SOURCE_ROOT + "tests/liveness_and_shutdown.rs":
        "liveness_and_shutdown.rs:27-56,58-110,112-161,163-230,231-294,295-351,352-401",
    SOURCE_ROOT + "tests/liveness_and_shutdown/bootstrap_admission.rs":
        "bootstrap_admission.rs:13-42,43-63,64-83,84-123",
    SOURCE_ROOT + "tests/liveness_and_shutdown/cleanup_authority.rs":
        "cleanup_authority.rs:19-58,60-113,114-141,142-175",
    SOURCE_ROOT + "tests/preparation_fixture.rs":
        "preparation_fixture.rs:8-40",
    SOURCE_ROOT + "tests/process_group_failsafe/cleanup_authority.rs":
        "cleanup_authority.rs:14-24,25-86",
    SOURCE_ROOT + "tests/process_group_failsafe/native_descendant.rs":
        "native_descendant.rs:19-36,37-120,122-160,161-175",
    SOURCE_ROOT + "tests/recovery_and_revocation.rs":
        "recovery_and_revocation.rs:18-55,56-87,88-107,108-164,165-198,199-231",
    SOURCE_ROOT + "tests/rejected_registration_deadline/cleanup_authority.rs":
        "cleanup_authority.rs:14-37,38-75,76-106",
    SOURCE_ROOT + "tests/rejected_registration_deadline/witness.rs":
        "witness.rs:8-48",
    SOURCE_ROOT + "tests/runtime_race_validation/control_plane.rs":
        "control_plane.rs:18-80,82-113",
    SOURCE_ROOT + "tests/runtime_race_validation/delivery_assertions.rs":
        "delivery_assertions.rs:11-104,106-130",
    SOURCE_ROOT + "tests/runtime_race_validation/delivery_cut_oracles.rs":
        "delivery_cut_oracles.rs:10-29,30-106,107-163",
    SOURCE_ROOT + "tests/runtime_race_validation/delivery_fixture.rs":
        "delivery_fixture.rs:22-117,119-170,172-207,208-261,263-275",
    SOURCE_ROOT + "tests/runtime_race_validation/private_record_relay.rs":
        "private_record_relay.rs:15-54,55-93,94-126",
    SOURCE_ROOT + "tests/runtime_race_validation/records_and_cut_types.rs":
        "records_and_cut_types.rs:13-64,65-106,108-137",
    SOURCE_ROOT + "tests/test_support.rs":
        "test_support.rs:15-63,64-88,89-116,117-174,175-193,194-229",
}
CITATION = re.compile(r"([^:]+\.rs):(\d+-\d+(?:,\d+-\d+)*)")


class Invalid(ValueError):
    """The proposed evidence contradicts historical Git or the parent ledger."""


def require(condition: bool, why: str) -> None:
    if not condition:
        raise Invalid(why)


@functools.lru_cache(maxsize=256)
def git_bytes(repo: Path, *args: str) -> bytes:
    """Cache immutable Git object reads across independent mutation tests."""
    result = subprocess.run(
        ("git", "-C", str(repo), *args), capture_output=True, check=False
    )
    if result.returncode:
        raise Invalid(f"missing immutable Git evidence: {args[0]}")
    return result.stdout


def git_text(repo: Path, *args: str) -> str:
    return git_bytes(repo, *args).decode("utf-8").strip()


def read_csv(path: Path) -> tuple[list[str], list[dict[str, str]]]:
    with path.open(newline="", encoding="utf-8") as source:
        reader = csv.DictReader(source, strict=True)
        fields, rows = list(reader.fieldnames or []), list(reader)
    require(bool(fields) and len(set(fields)) == len(fields), f"invalid CSV columns: {path.name}")
    require(
        all(None not in row and all(value is not None for value in row.values()) for row in rows),
        f"malformed CSV record: {path.name}",
    )
    require(
        all(row.get("path") for row in rows)
        and len({row["path"] for row in rows}) == len(rows),
        f"duplicate/empty review path: {path.name}",
    )
    return fields, rows


def check_citation(repo: Path, path: str, actual: str) -> None:
    require(actual == SOURCE_SPANS[path], f"historical source interval/anchor removed: {path}")
    match = CITATION.fullmatch(actual)
    require(match is not None, f"malformed historical citation: {path}")
    leaf, spans = match.groups()
    require(path.endswith("/" + leaf), f"citation references wrong source: {path}")
    source_lines = git_bytes(repo, "show", f"{SOURCE}:{path}").decode("utf-8").splitlines()
    last = 0
    for item in spans.split(","):
        first, end = (int(value) for value in item.split("-"))
        require(
            last < first <= end <= len(source_lines),
            f"nonexistent/overlapping pinned source interval: {path}:{item}",
        )
        require(
            any(line.strip() for line in source_lines[first - 1:end]),
            f"empty pinned source interval: {path}:{item}",
        )
        last = end


def verify(repo: Path = ROOT, folder: Path = HERE) -> None:
    for revision in (PARENT, BASELINE, SOURCE):
        require(git_text(repo, "rev-parse", revision) == revision, f"wrong pinned revision: {revision}")

    parent_folder = repo / PARENT_REL
    for name in (
        "README.md", "ledger_77.csv", "review_append.csv", "summary.json",
        "verify_seven.py", "test_verify_seven.py",
    ):
        relative = f"{PARENT_REL}/{name}"
        checkout = (parent_folder / name).read_bytes().replace(b"\r\n", b"\n")
        require(
            checkout == git_bytes(repo, "show", f"{PARENT}:{relative}"),
            f"parent #990 original evidence changed: {name}",
        )

    parent_cols, old_rows = read_csv(parent_folder / "ledger_77.csv")
    new_cols, new_rows = read_csv(folder / "ledger_77.csv")
    review_cols, reviews = read_csv(folder / "review_append.csv")
    summary = json.loads((folder / "summary.json").read_text(encoding="utf-8"))

    require(new_cols == parent_cols + list(ADDED), "inherited 77-column schema changed")
    require(review_cols == list(REVIEW_FIELDS), "19-source review schema changed")
    require(len(old_rows) == len(new_rows) == 77, "historical 77-row inventory changed")
    require(
        [row["path"] for row in old_rows] == [row["path"] for row in new_rows],
        "historical row order/identity changed",
    )
    require(
        len(reviews) == 19 and {r["path"] for r in reviews} == set(SOURCE_SPANS),
        "final 19 exact source coverage changed",
    )
    receipts = {record["path"]: record for record in reviews}
    counters = collections.Counter()
    for original, current in zip(old_rows, new_rows):
        path = original["path"]
        require(
            all(current[col] == original[col] for col in parent_cols),
            f"parent #990 ledger field mutated: {path}",
        )
        if path in receipts:
            record = receipts[path]
            require(
                original["batch7_source_status"] == "pending-source-review",
                f"earlier reviewed path reclassified: {path}",
            )
            require(record["review_class"] == "source-reviewed-limited", f"unearned review authority: {path}")
            require(
                all(record[col].strip() for col in REVIEW_FIELDS[4:]),
                f"blank scoped source evidence: {path}",
            )
            require(
                record["disposition_and_gap"].startswith("source-reviewed-limited;"),
                f"unearned semantic completion: {path}",
            )
            require(
                record["blob_949"] == original["tip_blob"]
                == git_text(repo, "rev-parse", f"{BASELINE}:{path}"),
                f"wrong #949 Git blob: {path}",
            )
            require(
                record["blob_952"] == original["blob_952"]
                == git_text(repo, "rev-parse", f"{SOURCE}:{path}"),
                f"wrong #952 Git blob: {path}",
            )
            require(
                record["blob_949"] == record["blob_952"],
                f"historical source changed between #949 and #952: {path}",
            )
            check_citation(repo, path, record["source_lines"])
            require(current["batch19_source_status"] == LIMITED, f"incorrect batch19 status: {path}")
            require(
                current["batch19_evidence_file"] == "review_append.csv"
                and current["batch19_disposition"] == record["disposition_and_gap"],
                f"missing or unjoined batch19 disposition: {path}",
            )
        else:
            require(
                current["batch19_source_status"] == original["batch7_source_status"],
                f"unreviewed historical path promoted: {path}",
            )
            require(
                not current["batch19_evidence_file"] and not current["batch19_disposition"],
                f"evidence invented for unreviewed historical path: {path}",
            )
        counters[current["batch19_source_status"]] += 1

    require(
        summary["schema_version"] == 1 and summary["parent_pr"] == 990
        and summary["parent_head"] == PARENT,
        "false parent #990 provenance",
    )
    require(
        summary["historical_949_head"] == BASELINE
        and summary["source_tip_952"] == SOURCE,
        "false historical Git source identities",
    )
    require(
        summary["historical_path_count"] == 77
        and summary["parent_pending_historical"] == 19
        and summary["new_limited_review_count"] == 19
        and summary["current_pending_historical"] == 0,
        "false 19-source review accounting",
    )
    require(
        counters[LIMITED] == 19 and counters["pending-source-review"] == 0
        and dict(counters) == summary["historical_path_review_status_counts"],
        "source ledger/status summary histogram diverged",
    )
    require(
        summary["changed_blobs_pending"] == CHANGED
        and summary["additional_952_only_file_pending"] == EXTRA
        and summary["source_version_overlays_incomplete"] is True,
        "changed-overlay or additional #952-only file falsely cleared",
    )
    require(summary["source_review_scope"] == SOURCE_SCOPE, "source scope wrongly certified")
    for flag in (
        "native_suites_rerun_for_docs", "all_77_historical_files_fully_reviewed",
        "kernel_custody_proven", "merge_authorized", "whole_824_architecture_complete",
    ):
        require(summary[flag] is False, f"false product/security/merge-completion claim: {flag}")
    require(
        summary["independent_exact_head_review_pending"] is True,
        "unperformed independent review falsely certified",
    )
    print(
        "Guardian final19 structural evidence: PASS "
        "(77 immutable historical rows, 19 LIMITED, zero historical pending; "
        "three changed overlays and one #952-only path still pending)"
    )


if __name__ == "__main__":
    try:
        verify()
    except (OSError, Invalid, KeyError, ValueError, subprocess.SubprocessError) as error:
        print(f"Guardian final19 structural evidence: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1) from error

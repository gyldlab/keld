#!/usr/bin/env python3
"""Verify eight exact historical Guardian source receipts, not OS/security acceptance.

The source-line anchors are finite, reviewer-selected native control oracles.
Structural validity cannot establish the prose's semantic truth or current shipping OS
process custody. Immutable prior #981 and historical #949/#952 Git objects are required.
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
PARENT = "ca0e62cc0af8c972e83b1548d88f79c2e1af98be"
BASELINE = "899e287ea487f38ec1cd347f7c9db8f1927348ef"
SOURCE = "77b1c5aef9202a6cc0d84195fc1f4c32e1eb1cba"
PARENT_REL = "docs/audits/macos-guardian-2026-10-19-059c70d8-eightfile"
TEST_ROOT = "crates/keld-runtime/src/macos_guardian/tests/"
ADDED = ("batch8b_source_status", "batch8b_evidence_file", "batch8b_disposition")
REVIEW_FIELDS = (
    "path", "blob_949", "blob_952", "review_class",
    "responsibility_and_callers", "invariant", "selectors_or_consumers",
    "fixture_and_independent_oracle", "source_finding", "disposition_and_gap", "source_lines",
)
LIMITED = "source-reviewed-limited-batch8b"
CHANGED = [
    "crates/keld-runtime/src/macos_guardian/authenticated_runner.rs",
    "crates/keld-runtime/src/macos_guardian/process_group.rs",
    "crates/keld-runtime/src/macos_guardian/tests/process_group_failsafe.rs",
]
EXTRA = "crates/keld-runtime/src/macos_guardian/authenticated_runner/tests.rs"

# Exact whole paths avoid conflating cleanup_authority.rs in unrelated fixture folders.
# Each anchor is an original #952 operation or a named native assertion, NOT a
# comprehensive source review; a statement absent from this set is not checked.
ANCHORS = {
    TEST_ROOT + "generation_host.rs": (
        33, 74, 86, 109, 117, 122, 125, 135, 139, 142, 143, 146, 152, 165,
        175, 187, 193, 220, 233, 245, 254, 264, 270, 278, 281, 290, 296,
        303, 314, 338, 345, 355, 360, 364, 369, 371,
    ),
    TEST_ROOT + "generation_host/clear_authority/cleanup_authority.rs": (
        13, 25, 57, 63, 72, 76, 80, 83, 87, 99, 103, 106, 110, 112,
    ),
    TEST_ROOT + "generation_host/invalid_spawn_after_revoke.rs": (
        13, 24, 29, 31, 33, 37, 54, 59, 63, 68, 74, 76, 78,
        81, 84, 89, 93, 99, 103, 108, 112,
    ),
    TEST_ROOT + "bootstrap_shutdown_race.rs": (
        33, 43, 47, 49, 58, 63, 68, 81, 87, 93, 107, 118, 120, 133,
        137, 143, 147, 155, 166, 176, 180, 190, 213, 217, 225, 227,
        231, 234, 236, 243, 254, 260, 265, 271, 277, 283, 288, 297,
        303, 308, 313, 318, 327, 332, 336, 339, 346, 352,
    ),
    TEST_ROOT + "bootstrap_shutdown_race/denial_order.rs": (
        19, 42, 47, 53, 58, 64, 66, 70, 76, 78, 80, 83, 86,
        88, 89, 91, 95, 103, 105,
    ),
    TEST_ROOT + "bootstrap_shutdown_race/factory_failure.rs": (
        14, 37, 39, 41, 43, 49, 52, 54, 60, 64, 73, 74, 77, 79, 83, 86, 88,
    ),
    TEST_ROOT + "bootstrap_shutdown_race/post_s_spawn.rs": (
        23, 25, 35, 45, 49, 56, 66, 71, 78, 82, 85,
        89, 91, 95, 102, 105, 108, 110, 116, 119, 123,
        124, 128, 132, 137, 141, 144, 149, 157, 159,
    ),
    TEST_ROOT + "active_generation_shutdown.rs": (
        20, 26, 30, 38, 43, 47, 60, 69, 75, 95, 96, 100,
        107, 111, 117, 120, 124, 126, 131, 134, 137, 141,
        146, 153, 154, 158, 160, 166, 168,
    ),
}
# Independently check selected decisive source words at original pinned line,
# so this verifier cannot accidentally anchor a comment or an unrelated statement.
SOURCE_TOKENS = {
    TEST_ROOT + "generation_host.rs": ((193, "revoked < provisioned"), (281, "matches!(terminal,"), (371, "marker, 1")),
    TEST_ROOT + "generation_host/clear_authority/cleanup_authority.rs": ((80, "assert_fresh_native_echo"), (112, "Some(9)")),
    TEST_ROOT + "generation_host/invalid_spawn_after_revoke.rs": ((89, "invalid spawned transition"), (108, ".is_none()")),
    TEST_ROOT + "bootstrap_shutdown_race.rs": ((155, "byte[0] == b'\\n'"), (227, "sent_rx"), (303, "assert_eq!")),
    TEST_ROOT + "bootstrap_shutdown_race/denial_order.rs": ((76, "recovery.try_lock()"), (88, "native host S must be flushed")),
    TEST_ROOT + "bootstrap_shutdown_race/factory_failure.rs": ((74, 'stderr.contains("KELD_TEST_FACTORY_ERROR_RETAINED")'), (79, "result.is_err()")),
    TEST_ROOT + "bootstrap_shutdown_race/post_s_spawn.rs": ((132, "assert_eq!"), (141, "original_group_gone"), (157, "outcome.expect_err")),
    TEST_ROOT + "active_generation_shutdown.rs": ((107, "if let [bun, member]"), (154, "kill("), (168, "status.signal()")),
}
CITATION = re.compile(r"([^:]+\.rs):(\d+-\d+(?:,\d+-\d+)*)")


class Invalid(ValueError):
    """A review claim contradicts an immutable Git/file/line contract."""


def require(ok: bool, message: str) -> None:
    if not ok:
        raise Invalid(message)


def git_bytes(repo: Path, *args: str) -> bytes:
    result = subprocess.run(("git", "-C", str(repo), *args), capture_output=True, check=False)
    if result.returncode:
        raise Invalid(f"missing pinned Git evidence for {args[0]}: fetch exact historical commit")
    return result.stdout


def git_text(repo: Path, *args: str) -> str:
    return git_bytes(repo, *args).decode("utf-8").strip()


def read_csv(file: Path) -> tuple[list[str], list[dict[str, str]]]:
    with file.open(newline="", encoding="utf-8") as reader:
        data = csv.DictReader(reader, strict=True)
        cols, records = list(data.fieldnames or []), list(data)
    require(bool(cols), f"missing CSV schema: {file.name}")
    require(all(None not in r and all(v is not None for v in r.values()) for r in records), f"CSV shape error: {file.name}")
    require(all(r.get("path") for r in records) and len({r["path"] for r in records}) == len(records),
            f"duplicate/empty source path: {file.name}")
    return cols, records


def check_source_lines(repo: Path, path: str, citation: str) -> None:
    match = CITATION.fullmatch(citation)
    require(match is not None, f"malformed exact-source line citation: {path}")
    leaf, spans = match.groups()
    require(path.endswith("/" + leaf), f"source citation references wrong file: {path}")
    source = git_bytes(repo, "show", f"{SOURCE}:{path}").decode("utf-8").splitlines()
    intervals: list[tuple[int, int]] = []
    previous = 0
    for span in spans.split(","):
        begin, end = (int(x) for x in span.split("-"))
        require(previous < begin <= end <= len(source),
                f"nonexistent, unordered or overlapping source interval {span}: {path}")
        intervals.append((begin, end))
        previous = end
    for line in ANCHORS[path]:
        require(any(begin <= line <= end for begin, end in intervals),
                f"native/clock evidence source line {line} omitted: {path}")
    for line, needle in SOURCE_TOKENS[path]:
        require(1 <= line <= len(source) and needle in source[line - 1],
                f"bad historically pinned source-line anchor {line}: {path}")


def verify(repo: Path = ROOT, folder: Path = HERE) -> None:
    for sha in (PARENT, BASELINE, SOURCE):
        require(git_text(repo, "rev-parse", sha) == sha, f"historical Git SHA unavailable: {sha}")
    parent = repo / PARENT_REL
    # Six originally shipped #981 documents (and every original 77 ledger field)
    # must remain untouched; a docs-only child never edits its parent.
    for name in ("README.md", "ledger_77.csv", "review_append.csv",
                 "summary.json", "verify_eightfile.py", "test_verify_eightfile.py"):
        relative = f"{PARENT_REL}/{name}"
        require(parent.joinpath(name).read_bytes().replace(b"\r\n", b"\n")
                == git_bytes(repo, "show", f"{PARENT}:{relative}"),
                f"parent #981 historical evidence modified: {relative}")

    parent_cols, old = read_csv(parent / "ledger_77.csv")
    new_cols, new = read_csv(folder / "ledger_77.csv")
    review_cols, reviews = read_csv(folder / "review_append.csv")
    summary = json.loads((folder / "summary.json").read_text(encoding="utf-8"))
    require(new_cols == parent_cols + list(ADDED), "inherited 77-file ledger columns modified")
    require(review_cols == list(REVIEW_FIELDS), "source review schema changed")
    require(len(old) == len(new) == 77, "historical 77-path inventory modified")
    require(len(reviews) == 8 and {r["path"] for r in reviews} == set(ANCHORS), "changed eight-file coverage")
    original = {row["path"]: row for row in old}
    current = {row["path"]: row for row in new}
    review_by = {row["path"]: row for row in reviews}
    require(set(original) == set(current), "original parent path dropped/invented")
    status_counts = collections.Counter()
    for path, earlier in original.items():
        now = current[path]
        require(all(now[key] == earlier[key] for key in parent_cols), f"parent review field mutated: {path}")
        if path in review_by:
            evidence = review_by[path]
            require(earlier["batch8_source_status"] == "pending-source-review", f"double source review: {path}")
            require(evidence["review_class"] == "source-reviewed-limited", f"unsupported review authority: {path}")
            require(all(evidence[field].strip() for field in REVIEW_FIELDS[4:]), f"blank grounded evidence: {path}")
            require(evidence["blob_949"] == earlier["tip_blob"] == git_text(repo, "rev-parse", f"{BASELINE}:{path}"),
                    f"wrong #949 original source blob: {path}")
            require(evidence["blob_952"] == earlier["blob_952"] == git_text(repo, "rev-parse", f"{SOURCE}:{path}"),
                    f"wrong #952 source blob: {path}")
            require(evidence["blob_949"] == evidence["blob_952"], f"historical source blob changed: {path}")
            check_source_lines(repo, path, evidence["source_lines"])
            require(now["batch8b_source_status"] == LIMITED, f"incorrect batch8b limited disposition: {path}")
            require(now["batch8b_evidence_file"] == "review_append.csv"
                    and now["batch8b_disposition"] == evidence["disposition_and_gap"],
                    f"unjoined evidence disposition: {path}")
        else:
            require(now["batch8b_source_status"] == earlier["batch8_source_status"],
                    f"unreviewed original path promoted: {path}")
            require(not now["batch8b_evidence_file"] and not now["batch8b_disposition"],
                    f"new evidence attributed to unreviewed path: {path}")
        status_counts[now["batch8b_source_status"]] += 1

    require(summary["schema_version"] == 1 and summary["parent_pr"] == 981
            and summary["parent_head"] == PARENT and summary["parent_source_documents_unchanged"] is True,
            "summary parent provenance false")
    require(summary["baseline_949_head"] == BASELINE and summary["source_tip_952"] == SOURCE,
            "summary source Git identity false")
    require(summary["historical_path_count"] == 77 and summary["parent_pending_historical"] == 34
            and summary["new_limited_review_count"] == 8 and summary["current_pending_historical"] == 26,
            "summary source/status inventory false")
    require(status_counts["pending-source-review"] == 26 and status_counts[LIMITED] == 8,
            "source-ledger status counts false")
    require(dict(status_counts) == summary["source_review_status_counts"], "summary status histogram false")
    require(summary["changed_blobs_pending"] == CHANGED
            and summary["additional_952_only_file_pending"] == EXTRA,
            "earlier changed/extra source file falsely cleared")
    for flag in ("native_suites_rerun", "all_historical_files_fully_reviewed",
                 "kernel_custody_proven", "merge_authorized"):
        require(summary[flag] is False, f"false product/completion claim: {flag}")
    require(summary["independent_exact_head_review_pending"] is True,
            "independent source review falsely certified")
    print("Guardian second eight-file evidence: PASS (77 immutable paths, 8 limited, 26 pending; no semantic/OS certificate)")


if __name__ == "__main__":
    try:
        verify()
    except (OSError, Invalid, KeyError, ValueError, subprocess.SubprocessError) as error:
        print(f"Guardian second eight-file evidence: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1) from error

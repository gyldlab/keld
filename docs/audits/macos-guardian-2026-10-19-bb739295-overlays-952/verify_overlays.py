#!/usr/bin/env python3
"""Verify conservative #949->#952 Guardian source overlays against immutable Git.

This checks exact evidence provenance, review boundary, claims and source
line/caller anchors. It does NOT prove a native oracle ran, OS kernel custody,
simultaneous Guardian+Host death, or current-tip source semantics.
"""
from __future__ import annotations

import collections
import csv
import functools
import hashlib
import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PARENT = "bb739295d4dda0ef83acc8c38b521594a49a29c5"
BASELINE = "899e287ea487f38ec1cd347f7c9db8f1927348ef"
SNAPSHOT = "77b1c5aef9202a6cc0d84195fc1f4c32e1eb1cba"
PARENT_DIR = "docs/audits/macos-guardian-2026-10-19-86933528-final19"
ROOT_PATH = "crates/keld-runtime/src/macos_guardian/"
RUNNER = ROOT_PATH + "authenticated_runner.rs"
GROUP = ROOT_PATH + "process_group.rs"
FAILSAFE = ROOT_PATH + "tests/process_group_failsafe.rs"
NEW_TEST = ROOT_PATH + "authenticated_runner/tests.rs"

# These independent constants were pinned from manual full-source reading of
# both historical Git snapshots. A future reviewer must re-examine the source
# before changing any of the supported spans, claims or fingerprints.
RECEIPTS = {
    "authenticated-runner-952": {
        "path": RUNNER, "kind": "changed-949-to-952", "status": "changed-since-949-pending",
        "before": "f00e42607ef8f1c5b8877dd81d487ea7c0ef1092",
        "after": "8a3a1368f6bd5ff364ca181e4e9b2f181b216e07",
        "diff": (69, 9), "lines": 233,
        "sections": [(1, 12), (13, 49), (50, 108), (109, 138), (139, 165), (166, 202), (203, 233)],
        "anchor_hash": "3e299827c6e791ae575878f7222c714da2ac83015e3e9363eba37ff6b9e85785",
        "claim_hash": "1bb08f16455328ec320cd7e6287e5bdae6ab34142d07fa6dafbd7432d65d4258",
        "callers": [
            ("crates/keld-runtime/src/macos_guardian.rs", 167, 185, "run_with_authenticated_registration("),
            (NEW_TEST, 63, 68, "run_live_group(child, stale_birth"),
        ],
        "oracles": [(NEW_TEST, "denied_attested_kgr1_group_signal_never_blocks_on_live_direct_child_wait", 39)],
    },
    "process-group-952": {
        "path": GROUP, "kind": "changed-949-to-952", "status": "changed-since-949-pending",
        "before": "ab6e23fb010dc810deb91e4b83d5939dfb4a4cec",
        "after": "213047a5ece30b591d1af8460af5d543303ac663",
        "diff": (48, 4), "lines": 570,
        "sections": [(1, 64), (65, 128), (129, 164), (165, 228), (229, 300),
                     (301, 377), (378, 400), (401, 489), (490, 570)],
        "anchor_hash": "f459740e7cb34f767e5cbe17e3bda6350cc7496c9ad327d5fd870f39b480f02c",
        "claim_hash": "d16b5f2ce8c81d7b35cdca57d2f41734f3ca0b97296a25097a041d1b2e347c1f",
        "callers": [
            (ROOT_PATH + "guarded_lease.rs", 307, 324, "reconcile_denied_signal_after_reap(identity, error, reap_deadline)"),
            (ROOT_PATH + "generation_host.rs", 347, 365, "require_attested_group_absence(group, absence_deadline)"),
            (RUNNER, 126, 129, "terminate_attested_group(group)"),
            (ROOT_PATH + "bootstrap.rs", 326, 335, "reap_pending_guardian(&mut child)"),
        ],
        "oracles": [
            (FAILSAFE, "expired_post_reap_reconciliation_cannot_accept_late_native_absence", 268),
            (FAILSAFE, "guardian_signal_eperm_reconciles_only_after_native_group_absence", 112),
        ],
    },
    "process-group-failsafe-test-952": {
        "path": FAILSAFE, "kind": "changed-949-to-952", "status": "changed-since-949-pending",
        "before": "5a1002b04be7a26df047079829aa9c63d9abb66f",
        "after": "a1d3bc8722872ea03bc59f14a99d653ee0efce33",
        "diff": (123, 3), "lines": 590,
        "sections": [(1, 38), (39, 105), (106, 197), (198, 263), (264, 377),
                     (378, 413), (414, 457), (458, 525), (526, 590)],
        "anchor_hash": "e83d30706ae3b59d75d5673879cc0c6dbbe800541972ba5689e3632663711750",
        "claim_hash": "c75021a6fcfdc23ec05e294e28c06cf255db6b538f22bf030a45e54ec49dd341",
        "callers": [
            (ROOT_PATH + "tests.rs", 23, 30, "mod process_group_failsafe;"),
            (GROUP, 480, 489, "pub(super) fn reconcile_denied_signal_after_reap_for_test"),
        ],
        "oracles": [
            (FAILSAFE, "expired_post_reap_reconciliation_cannot_accept_late_native_absence", 268),
            (FAILSAFE, "reaped_leader_with_persistently_unreaped_member_must_not_claim_clear", 527),
            (FAILSAFE, "stale_group_birth_cannot_signal_a_live_unrelated_native_group", 379),
        ],
    },
    "authenticated-runner-native-test-952": {
        "path": NEW_TEST, "kind": "added-952-only", "status": "outside-original-77",
        "before": None, "after": "3fc77f68ed73e2f8f87391de515658dff200e42a",
        "diff": (122, 0), "lines": 122,
        "sections": [(1, 37), (38, 69), (70, 100), (101, 122)],
        "anchor_hash": "b79b72526a0665db80ac13bcfdb417c0796a27c9e709f53847d1cf41559c42fd",
        "claim_hash": "85c82a13843f08c397fd05c5925e89dc9292321f865ad0263db82d85657e8d18",
        "callers": [
            (RUNNER, 231, 233, "mod tests;"),
            ("crates/keld-runtime/src/macos_guardian.rs", 39, 42, "mod authenticated_runner;"),
        ],
        "oracles": [(NEW_TEST, "denied_attested_kgr1_group_signal_never_blocks_on_live_direct_child_wait", 39)],
    },
}

# Decisive, manually inspected, line-numbered source snippets. These checks
# bind source claims to actual #952 text, independently of the review JSON.
SIGNATURES = {
    RUNNER: {
        13: "fn run_with_authenticated_registration", 32: "validate_liveness_bootstrap",
        37: ".process_group(0)", 42: "SpawnAdmission::install",
        50: "command.spawn()", 68: "group_witness.take()",
        126: "if let Some(group)", 131: "if let Some(child)",
        132: "child.wait()", 145: "await_host_death", 148: "original_cleanup_deadline",
        159: "terminate_attested_group(group)", 174: "reap_owned_child_after_group_refusal",
        176: "child.wait()", 209: "fn reap_owned_child_after_group_refusal",
        216: "child.kill()?", 218: "child.try_wait()?",
        220: "Instant::now() < deadline", 232: "authenticated_runner/tests.rs",
    },
    GROUP: {
        21: "A PGID is only a number", 42: "validate_guardian_child_group",
        130: "fn validate_group_pid", 183: "libc::proc_pidinfo",
        196: "fn observe_group_identity", 208: "fn validate_guardian_child_group",
        213: "info.pbi_ppid != guardian_pid", 236: "fn terminate_attested_group",
        238: "current == group", 256: "reap_deadline",
        269: "Err(Errno::ESRCH)", 272: "Err(Errno::ESRCH), Ok(())",
        275: "Err(Errno::EPERM)", 311: "fn require_attested_group_absence",
        317: "Instant::now() >= deadline", 330: "leader == Err(Errno::ESRCH)",
        332: "Instant::now() < deadline", 379: "fn terminate_registered_group",
        388: "fn finish_group_termination", 394: "Err(Errno::EPERM)",
        410: "fn reconcile_denied_signal_after_reap", 434: "fn reconcile_denied_signal_after_reap_with_observers",
        444: "macOS guardian process-group signal", 457: "now() >= deadline",
        460: "let (leader, group_live) = probe(raw)", 469: "Err(Errno::ESRCH)",
        472: "Err(Errno::EPERM)", 481: "fn reconcile_denied_signal_after_reap_for_test",
        493: "fn reap_pending_guardian", 507: "child.kill()",
        515: "child.wait()", 528: "child.kill()", 536: "child.wait()",
        548: "fn reject_host_registration", 555: "reap_pending_guardian",
    },
    FAILSAFE: {
        28: "impl Drop for TestOwnedGroupChild", 35: "self.0.wait()",
        62: "fn reaped_leader_does_not_mask_live_process_group_when_signal_is_denied",
        85: "finish_group_termination", 99: "fn denied_group_signal_is_only_ignorable",
        112: "fn guardian_signal_eperm_reconciles_only_after_native_group_absence",
        140: "Ok(Pid::from_raw(raw))", 170: "observed_tx",
        178: "reconcile_denied_signal_after_reap", 190: "resolved.expect",
        212: "kill(Pid::from_raw(-raw), None)", 268: "fn expired_post_reap_reconciliation",
        312: "reconcile_denied_signal_after_reap", 319: "reconcile_denied_signal_after_reap_for_test",
        334: "expired_probes, 0", 349: "reconcile_denied_signal_after_reap_for_test",
        355: "clock_reads == 1", 361: "assert_eq!(leader", 367: "kernel_probes, 1",
        379: "fn stale_group_birth_cannot_signal_a_live_unrelated_native_group",
        415: "fn reaped_registered_group_leader_does_not_strand_live_descendant",
        442: "terminate_attested_group(birth)",
        459: "fn reaped_leader_with_temporarily_unreaped_dead_member",
        509: "observed_tx", 527: "fn reaped_leader_with_persistently_unreaped_member",
        568: "terminate_attested_group(original)",
    },
    NEW_TEST: {
        18: "struct PendingDirectChild", 26: "impl Drop for PendingDirectChild",
        34: "child.wait()", 39: "fn denied_attested_kgr1_group_signal_never_blocks",
        43: ".process_group(0)", 57: "observe_group_identity(pid)",
        60: "pipe()", 62: "altered_birth_for_test",
        66: "run_live_group(child, stale_birth", 76: "recv_timeout",
        77: "gone_before_rescue", 90: "WaitPidFlag::WNOHANG",
        91: "terminate_attested_group(original)", 93: ".join()",
        98: "native_waitpid", 106: "Err(Errno::ECHILD)",
        119: "process-group leader PID was reused",
    },
}

REVIEW_TEXT_FIELDS = (
    "responsibility_and_boundary", "source_finding",
    "native_falsifier_and_limit", "disposition",
)
FALSE_FLAGS = (
    "all_77_histories_semantically_complete",
    "current_latest_repo_head_source_review_complete",
    "repo_wide_file_census_complete",
    "native_guardian_suites_rerun_for_docs",
    "test_mutants_executed_against_rust_code",
    "kernel_pinned_process_group_custody_proven",
    "simultaneous_host_guardian_death_proven",
    "detached_descendant_containment_proven",
    "source_review_is_native_acceptance",
    "merge_authorized",
)
REMAINING_GAP = (
    "Current latest HEAD may differ from immutable #952; no kernel-held PGID handle, "
    "TOCTOU-freedom, dual death, detached setsid containment, OS-wide scheduling "
    "or production native acceptance has been proven by this docs-only review."
)
RESOLVED_MEANING = (
    "The three historically changed source rows have separately labeled LIMITED "
    "#952 version-review receipts; original 77 rows retain their historical "
    "changed-since-949-pending status and must not be promoted."
)


class Invalid(ValueError):
    """A claimed source review disagrees with pinned historical evidence."""


def require(condition: bool, why: str) -> None:
    if not condition:
        raise Invalid(why)


@functools.lru_cache(maxsize=512)
def git_bytes(repo: Path, *arguments: str) -> bytes:
    result = subprocess.run(
        ("git", "-C", str(repo), *arguments), capture_output=True, check=False
    )
    require(result.returncode == 0, f"missing immutable Git evidence for {arguments[0]}")
    return result.stdout


def git_text(repo: Path, *arguments: str) -> str:
    return git_bytes(repo, *arguments).decode("utf-8").strip()


def source_lines(repo: Path, path: str) -> list[str]:
    return git_bytes(repo, "show", f"{SNAPSHOT}:{path}").decode("utf-8").splitlines()


def hash_strings(values: list[str]) -> str:
    return hashlib.sha256(",".join(values).encode("utf-8")).hexdigest()


def claim_hash(row: dict) -> str:
    text = {name: row[name] for name in REVIEW_TEXT_FIELDS}
    text["sections"] = [section["purpose"] for section in row["source_sections"]]
    text["callers"] = [entry["role"] for entry in row["caller_anchors"]]
    text["oracles"] = [entry["observes"] for entry in row["native_oracles"]]
    return hashlib.sha256(
        json.dumps(text, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()


def verify(repo: Path = ROOT, folder: Path = HERE) -> None:
    for revision in (PARENT, BASELINE, SNAPSHOT):
        require(git_text(repo, "rev-parse", revision) == revision, f"wrong pinned Git SHA: {revision}")

    # A copy of the immutable inherited inventory is included for standalone
    # reviewer diff. Every byte, column and row/status must remain identical.
    inherited = f"{PARENT_DIR}/ledger_77.csv"
    parent_blob = git_bytes(repo, "show", f"{PARENT}:{inherited}")
    require(
        (repo / inherited).read_bytes().replace(b"\r\n", b"\n") == parent_blob,
        "parent #992 historical ledger changed",
    )
    require(
        (folder / "ledger_77.csv").read_bytes().replace(b"\r\n", b"\n") == parent_blob,
        "inherited historical 77 ledger bytes changed",
    )
    for filename in ("README.md", "review_append.csv", "summary.json", "verify_final19.py", "test_verify_final19.py"):
        path = f"{PARENT_DIR}/{filename}"
        require(
            (repo / path).read_bytes().replace(b"\r\n", b"\n")
            == git_bytes(repo, "show", f"{PARENT}:{path}"),
            f"parent #992 audit source altered: {filename}",
        )

    with (folder / "ledger_77.csv").open(newline="", encoding="utf-8") as stream:
        inventory = csv.DictReader(stream, strict=True)
        columns, old = list(inventory.fieldnames or []), list(inventory)
    require(
        len(columns) == 30 and len(old) == 77 and len({row["path"] for row in old}) == 77,
        "historical 77 file/column census altered",
    )
    historical = {row["path"]: row for row in old}
    original_changed = {
        path for path, row in historical.items()
        if row["batch19_source_status"] == "changed-since-949-pending"
    }
    require(original_changed == {RUNNER, GROUP, FAILSAFE}, "original changed-source overlay status altered")
    require(
        sum(row["batch19_source_status"] == "pending-source-review" for row in old) == 0,
        "original historical source-pending status altered",
    )
    parent_summary = json.loads((repo / PARENT_DIR / "summary.json").read_text(encoding="utf-8"))
    require(
        dict(collections.Counter(row["batch19_source_status"] for row in old))
        == parent_summary["historical_path_review_status_counts"],
        "parent historical source histogram no longer matches",
    )

    evidence = json.loads((folder / "overlay_evidence.json").read_text(encoding="utf-8"))
    require(evidence["schema_version"] == 1 and "LIMITED" in evidence["scope"], "wrong limited review evidence scope")
    reviews = evidence["reviews"]
    require(
        len(reviews) == 4 and [row["id"] for row in reviews] == list(RECEIPTS),
        "four exact source-overlay identities changed",
    )
    for row in reviews:
        expected = RECEIPTS[row["id"]]
        path = expected["path"]
        require(
            row["path"] == path and row["kind"] == expected["kind"]
            and row["parent_historical_status"] == expected["status"],
            f"source overlay kind/path/old-status forged: {path}",
        )
        expected_review_status = (
            "source-reviewed-limited-952-addition" if path == NEW_TEST
            else "source-reviewed-limited-952-overlay"
        )
        require(row["review_status"] == expected_review_status, f"unsupported promotion: {path}")
        require(
            (row["diff_added"], row["diff_deleted"]) == expected["diff"],
            f"edited source diff counts: {path}",
        )
        numstat = git_text(repo, "diff", "--numstat", BASELINE, SNAPSHOT, "--", path).splitlines()
        require(
            len(numstat) == 1
            and numstat[0].split("\t") == [str(expected["diff"][0]), str(expected["diff"][1]), path],
            f"immutable historical source diff disagrees with review: {path}",
        )
        if path == NEW_TEST:
            require(path not in historical, "new #952-only source counted as one of original 77")
            result = subprocess.run(
                ("git", "-C", str(repo), "cat-file", "-e", f"{BASELINE}:{path}"),
                capture_output=True, check=False,
            )
            require(result.returncode != 0 and row["blob_949"] is None,
                    "extra #952-only source falsely claimed as #949 file")
        else:
            require(historical[path]["batch19_source_status"] == expected["status"],
                    f"an original historical row was implicitly promoted: {path}")
            require(row["blob_949"] == expected["before"]
                    == git_text(repo, "rev-parse", f"{BASELINE}:{path}"),
                    f"forged original #949 source blob: {path}")
        require(row["blob_952"] == expected["after"]
                == git_text(repo, "rev-parse", f"{SNAPSHOT}:{path}"),
                f"forged pinned #952 source blob: {path}")

        source = source_lines(repo, path)
        require(len(source) == expected["lines"], f"source #952 line count changed: {path}")
        sections = row["source_sections"]
        actual_bounds = [(entry["first"], entry["last"]) for entry in sections]
        require(
            actual_bounds == expected["sections"]
            and actual_bounds[0][0] == 1 and actual_bounds[-1][1] == len(source)
            and all(actual_bounds[i][1] + 1 == actual_bounds[i + 1][0]
                    for i in range(len(actual_bounds) - 1)),
            f"full immutable #952 source-section coverage changed: {path}",
        )
        require(
            all(len(entry["purpose"].strip()) >= 16 for entry in sections),
            f"empty/unscoped source section purpose: {path}",
        )
        anchors = row["source_anchors"]
        require(
            isinstance(anchors, list)
            and all(type(index) is int for index in anchors)
            and anchors == sorted(set(anchors))
            and all(1 <= index <= len(source) and source[index - 1].strip() for index in anchors)
            and hash_strings([str(index) for index in anchors]) == expected["anchor_hash"],
            f"named #952 source-line anchor removed or changed: {path}",
        )
        for line, needle in SIGNATURES[path].items():
            require(
                line in anchors and needle in source[line - 1],
                f"real #952 production/native assertion no longer at cited line {line}: {path}",
            )

        callers = row["caller_anchors"]
        require(
            [(r["path"], r["first"], r["last"], r["needle"]) for r in callers]
            == expected["callers"],
            f"source direct-caller provenance changed: {path}",
        )
        for call in callers:
            lines = source_lines(repo, call["path"])
            begin, end = call["first"], call["last"]
            require(
                1 <= begin <= end <= len(lines)
                and call["needle"] in "\n".join(lines[begin - 1:end])
                and len(call["role"].strip()) >= 16,
                f"claimed direct caller/owner not found in #952 Git blob: {path}",
            )

        oracles = row["native_oracles"]
        require(
            [(r["path"], r["selector"], r["line"]) for r in oracles]
            == expected["oracles"],
            f"named native negative oracle removed or relabeled: {path}",
        )
        for oracle in oracles:
            code = source_lines(repo, oracle["path"])
            index = oracle["line"]
            require(
                2 <= index <= len(code) and "#[test]" in "\n".join(code[max(0, index - 4):index - 1])
                and f"fn {oracle['selector']}(" in code[index - 1]
                and oracle["needle"] in code[index - 1]
                and len(oracle["observes"].strip()) >= 32,
                f"native #952 oracle declaration/line not grounded: {path}",
            )

        require(
            all(len(row[name].strip()) >= 100 for name in REVIEW_TEXT_FIELDS)
            and row["disposition"].startswith("LIMITED #952")
            and claim_hash(row) == expected["claim_hash"],
            f"reviewed limited source prose/known gaps changed without new review: {path}",
        )

    summary = json.loads((folder / "summary.json").read_text(encoding="utf-8"))
    require(
        summary["schema_version"] == 1 and summary["parent_pr"] == 992
        and summary["parent_head"] == PARENT and summary["source_baseline_949"] == BASELINE
        and summary["source_snapshot_952"] == SNAPSHOT,
        "false historical/parent provenance",
    )
    require(
        summary["review_kind"] == "limited four-source historical-952-version overlay"
        and summary["historical_77_file_count"] == 77
        and summary["historical_77_column_count"] == 30
        and summary["historical_pending_source_count"] == 0
        and summary["historical_changed_since_949_pending_count"] == 3
        and summary["reviewed_changed_952_overlay_count"] == 3
        and summary["reviewed_added_952_only_count"] == 1
        and summary["reviewed_952_total"] == 4,
        "false historical/overlay source-accounting",
    )
    require(
        summary["historical_statuses_retained_exactly"] is True
        and summary["additional_952_only_original_77_membership"] is False
        and summary["resolved_meaning"] == RESOLVED_MEANING
        and summary["remaining_gap"] == REMAINING_GAP,
        "historical inventory silently promoted or source limits removed",
    )
    for name in FALSE_FLAGS:
        require(summary[name] is False, f"false native/OS/merge completion claim: {name}")
    require(
        summary["independent_exact_child_tip_review_pending"] is True,
        "unperformed independent review falsely certified",
    )
    print(
        "Guardian #952 overlay structural evidence: PASS "
        "(3 LIMITED historical version overlays + 1 separate #952-only test; "
        "77 old rows unchanged; current HEAD/dual death/kernel custody unproven)"
    )


if __name__ == "__main__":
    try:
        verify()
    except (OSError, Invalid, KeyError, TypeError, ValueError, subprocess.SubprocessError) as error:
        print(f"Guardian #952 overlay structural evidence: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1) from error

#!/usr/bin/env python3
"""Report-only comparison of pinned Git trees. Never enforce candidate budgets.

Reuses source_census as the sole counting/classification/policy table owner.
This command is not wired to required CI and never rejects a file for size.
"""

import argparse
import json
from collections import Counter
from pathlib import Path

import source_census as census


def parse_changes(raw):
    """Parse 'git diff --name-status -z -M' without losing unusual path bytes."""
    tokens = [t for t in raw.split(b"\0") if t]
    changes = []
    cursor = 0
    while cursor < len(tokens):
        status = tokens[cursor].decode("ascii")
        cursor += 1
        if status[0] not in "ADMTR" or (status[0] == "R" and not status[1:].isdigit()):
            raise ValueError(f"unsupported Git change status: {status!r}")
        count = 2 if status[0] == "R" else 1
        if cursor + count > len(tokens):
            raise ValueError("incomplete Git change status entry")
        paths = [p.decode("utf-8", "surrogateescape")
                 for p in tokens[cursor:cursor + count]]
        cursor += count
        changes.append((status[0], paths[0], paths[-1]))
    return changes


def tracked_rows(repo, commit):
    entries = census.parse_tree(census.git(repo, "ls-tree", "-r", "-z", "--full-tree", commit))
    blobs = census.read_blobs(repo, entries)
    return {row["path"]: row for row in census.collect(entries, blobs)}


def exceeds(row, ceiling_lines, ceiling_bytes):
    if row is None or row["category"] == "F":
        return False
    return ((ceiling_lines is not None
             and (row["physical_lines"] or 0) > ceiling_lines)
            or (ceiling_bytes is not None and (row["bytes"] or 0) > ceiling_bytes))


def growth(before, after):
    return (before is not None and after is not None
            and ((after["physical_lines"] or 0) > (before["physical_lines"] or 0)
                 or (after["bytes"] or 0) > (before["bytes"] or 0)))


def review_trigger(current, policy):
    """Reuse source_census's one test-entry/review threshold owner."""
    if current is None or current["category"] not in policy:
        return False
    c = current["category"]
    entry = next(r for r in census.policy_impact([current], policy, "simulation")
                 if r["category"] == c)
    return bool(entry["review_over_lines"] or entry["review_over_bytes"])


def assess_change(kind, old_path, new_path, old, current, policy):
    category = current["category"] if current is not None else old["category"]
    limits = policy.get(category)
    if limits is None or category == "F":
        return {
            "path": new_path, "old_path": old_path if kind == "R" else None,
            "change_kind": kind, "category": category, "finding": "excluded",
            "reason": (current or old)["exclusion_reason"], "hypothetical_block": False,
        }

    review_lines, review_bytes, hard_lines, hard_bytes = limits
    old_over = exceeds(old, hard_lines, hard_bytes)
    new_over = exceeds(current, hard_lines, hard_bytes)
    grew = growth(old, current)
    if kind == "D":
        finding = "removed"
    elif old is None and new_over:
        finding = "new-above-proposed-cap"
    elif old is not None and old_over and not new_over:
        # Checked before growth: a file back under both caps is never a block even
        # when one axis grew on the way (e.g. fewer lines, more bytes).
        finding = "legacy-now-within-proposal"
    elif old is not None and old_over and grew:
        finding = "grandfathered-growth"
    elif old is not None and old_over and (
        (current["physical_lines"] or 0) < (old["physical_lines"] or 0)
        or (current["bytes"] or 0) < (old["bytes"] or 0)
    ):
        finding = "grandfathered-shrink"
    elif old_over:
        finding = "grandfathered-no-growth"
    elif not old_over and new_over:
        finding = "newly-over-proposed-cap"
    elif review_trigger(current, policy):
        finding = "review-trigger"
    else:
        finding = "within-proposal"

    guidance = {
        "new-above-proposed-cap": "Evaluate real responsibilities; caps are not adopted.",
        "grandfathered-growth": "A future no-growth rule would require human review/exception; no rejection now.",
        "newly-over-proposed-cap": "Assess the ownership seam or reviewed exception before enforcing.",
        "grandfathered-shrink": "Reduction is welcome; a future protected-main ratchet could lower the ceiling.",
        "grandfathered-no-growth": "Existing over-limit file was not enlarged; do not force a split.",
        "legacy-now-within-proposal": "Legacy file is now below the simulated caps.",
        "review-trigger": "Human cohesion/test-layout review only; do not reject regression tests for size.",
        "within-proposal": "No size action; standard correctness review still applies.",
        "removed": "Removed tracked path; no size violation.",
    }
    return {
        "path": new_path, "old_path": old_path if kind == "R" else None,
        "change_kind": kind, "category": category, "finding": finding,
        "base_lines": old["physical_lines"] if old else None,
        "head_lines": current["physical_lines"] if current else None,
        "base_bytes": old["bytes"] if old else None,
        "head_bytes": current["bytes"] if current else None,
        "legacy_over_proposed_cap": old_over,
        "currently_over_proposed_cap": new_over,
        "grew_on_either_axis": grew,
        "review_over_generic_threshold": exceeds(current, review_lines, review_bytes),
        "review_over_owner_threshold": review_trigger(current, policy),
        "test_layout_owner_review": category == "C" and review_trigger(current, policy),
        "proposed_review_lines": review_lines,
        "proposed_review_bytes": review_bytes,
        "proposed_hard_lines": hard_lines,
        "proposed_hard_bytes": hard_bytes,
        "suggested_next_step": guidance[finding],
        "hypothetical_block": finding in (
            "new-above-proposed-cap", "grandfathered-growth", "newly-over-proposed-cap"),
    }


def report(repo, base_ref, head_ref, policy_name):
    policy = {"candidate": census.CANDIDATE, "roomier": census.ROOMIER}[policy_name]
    base_sha = census.git(repo, "rev-parse", f"{base_ref}^{{commit}}").decode().strip()
    head_sha = census.git(repo, "rev-parse", f"{head_ref}^{{commit}}").decode().strip()
    old = tracked_rows(repo, base_sha)
    now = tracked_rows(repo, head_sha)
    raw = census.git(repo, "diff", "--name-status", "-z", "--no-ext-diff",
                     "-M", base_sha, head_sha)
    changes = parse_changes(raw)

    findings = []
    for kind, old_path, new_path in changes:
        if kind != "A" and old_path not in old:
            raise ValueError(f"missing baseline Git path: {old_path!r}")
        if kind != "D" and new_path not in now:
            raise ValueError(f"missing current Git path: {new_path!r}")
        findings.append(assess_change(kind, old_path, new_path,
                                      old.get(old_path) if kind != "A" else None,
                                      now.get(new_path) if kind != "D" else None,
                                      policy))
    findings.sort(key=lambda x: (x["path"], x["old_path"] or ""))
    return {
        "schema": "keld.source-budget-report/v0",
        "enforcement": "report-only",
        "budget_status": "unapproved-simulated-policy",
        "policy": policy_name,
        "base_sha": base_sha, "head_sha": head_sha,
        "base_tracked_count": len(old), "head_tracked_count": len(now),
        "changed_count": len(findings),
        "finding_counts": dict(sorted(Counter(f["finding"] for f in findings).items())),
        "hypothetical_blocked_changed_count": sum(f["hypothetical_block"] for f in findings),
        "proposed_head_impacts": census.policy_impact(
            list(now.values()), policy, policy_name),
        "findings": findings,
        "limitations": [
            "Always exit 0 for proposed size violations; no active hard cap.",
            "Candidate values and category-specific review triggers are not approved.",
            "For test files, .agents/test-layout.md owns stricter review-only triggers.",
            "Git -M rename detection uses Git's similarity heuristic; low-similarity renames may appear as delete/add.",
            "A file's size cannot prove modularity, cohesion or agent navigation cost.",
            "Base should be the current protected branch at review time; no historical ratchet is silently persisted.",
            "Exceptions and human approval are not implemented in report-only mode.",
        ],
    }


def main():
    parser = argparse.ArgumentParser(
        description="Read-only, NONBLOCKING simulation of prospective KELD file budgets")
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--base", required=True, help="Pinned protected-base Git ref")
    parser.add_argument("--head", required=True, help="Candidate Git ref")
    parser.add_argument("--policy", choices=("candidate", "roomier"),
                        default="candidate")
    parser.add_argument("--output", type=Path, help="Optional JSON output path")
    args = parser.parse_args()

    result = report(args.repo.resolve(), args.base, args.head, args.policy)
    serialized = json.dumps(result, indent=2, sort_keys=True, ensure_ascii=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(serialized, encoding="utf-8", newline="\n")
    print(json.dumps({
        "enforcement": result["enforcement"],
        "base_sha": result["base_sha"], "head_sha": result["head_sha"],
        "changed_count": result["changed_count"],
        "hypothetical_blocked_changed_count":
            result["hypothetical_blocked_changed_count"],
        "finding_counts": result["finding_counts"],
    }, sort_keys=True))
    # Intentionally never return nonzero solely for a proposed over-budget source.


if __name__ == "__main__":
    main()

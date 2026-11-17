"""Synthetic D-9 object-set accounting. This is not native validation.

The objects below have deterministic fake digests and upper-bound lengths.
No real signatures, WinTrust, root metadata, protected files or recovery
journals are executed. See AC-3 and E-82 for the later native oracle.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
OBJECT_LIMIT = 24
BYTE_LIMIT = 1_310_720
SINGLE_LIMIT = 65_536
CURRENT_POLICY_LIMIT = 7

POLICY = [
    "repository-policy",
    "publisher-policy",  # Application
    "issuer-policy",  # Application
    "discovery-policy",
    "recovery-policy",
    "publisher-policy",  # Recovery
    "issuer-policy",  # Recovery
]


def record(kind: str, label: str, size: int = SINGLE_LIMIT) -> dict:
    return {
        "kind": kind,
        "sha256": hashlib.sha256(label.encode("ascii")).hexdigest(),
        "bytes": size,
    }


def policies(epoch: str) -> list[dict]:
    return [record(kind, f"{epoch}:policy:{i}") for i, kind in enumerate(POLICY)]


def count(records: list[dict]) -> tuple[int, int]:
    seen: dict[tuple[str, str], int] = {}
    for record_ in records:
        size = record_["bytes"]
        if not 1 <= size <= SINGLE_LIMIT:
            raise ValueError("individual evidence object size")
        if record_["kind"].startswith("signature-") and size != 64:
            raise ValueError("signature length")
        key = (record_["kind"], record_["sha256"])
        if key in seen and seen[key] != size:
            raise ValueError("contradictory duplicate evidence reference")
        seen[key] = size
    total = sum(seen.values())
    if len(seen) > OBJECT_LIMIT or total > BYTE_LIMIT:
        raise ValueError("set object/byte cap")
    return len(seen), total


def assert_case(name: str, work, expected: bool) -> dict:
    try:
        result = work()
        accepted = True
    except ValueError:
        result = None
        accepted = False
    if accepted != expected:
        raise ValueError(f"unexpected {name}: accepted={accepted}, expected={expected}")
    return {"name": name, "expected": "accept" if expected else "refuse",
            "result": "PASS", "observation": result}


def run(output_dir: Path | None = None) -> None:
    old = policies("before")
    new = policies("after")
    # Distinct application and recovery issuers in each policy cohort.
    if len({x["sha256"] for x in old + new}) != 14:
        raise ValueError("policy cohorts must contain 14 distinct objects")
    non_policy = [
        record("provenance", "before-provenance"),
        record("provenance", "after-provenance"),
        record("transition", "fully signed transition"),
        record("signature-u1", "U1 proof", 64),
        record("signature-u2", "U2 proof", 64),
        record("metadata-admission", "exact current revision"),
    ]
    transition = old + new + non_policy
    if len(transition) != 20:
        raise ValueError("transition must contain 20 required objects")
    if sum(v["bytes"] for v in transition) != 1_179_776:
        raise ValueError("transition reference-byte total changed")
    cases = [
        assert_case("current seven-reference cohort",
                    lambda: (len(old), len({x["sha256"] for x in old})),
                    True),
        assert_case("two issuers and distinct U1/U2 positive set",
                    lambda: count(transition), True),
        assert_case("one repeated kind/digest has one physical copy",
                    lambda: count(transition + [transition[0]]), True),
        assert_case("small four-object headroom still fits both caps",
                    lambda: count(transition + [
                        record("provenance", f"headroom-{i}", 8192)
                        for i in range(4)
                    ]), True),
        assert_case("25 distinct objects refuse",
                    lambda: count(transition + [
                        record("provenance", f"excess-{i}", 8192)
                        for i in range(5)
                    ]), False),
        assert_case("24 objects above byte cap refuse",
                    lambda: count(transition + [
                        record("provenance", f"large-{i}")
                        for i in range(4)
                    ]), False),
        assert_case("same kind/digest different length refuses",
                    lambda: count(transition + [{**transition[0], "bytes": 1}]),
                    False),
        assert_case("oversized object refuses",
                    lambda: count(transition + [record("provenance", "large", 65537)]),
                    False),
        assert_case("bad length on signature-u2 refuses",
                    lambda: count([*transition[:-2],
                                   {**transition[-2], "bytes": 65},
                                   transition[-1]]),
                    False),
    ]
    if len(old) != CURRENT_POLICY_LIMIT:
        raise ValueError("current policy cohort reference count changed")
    report = {
        "schema": "keld.evidence-capacity-definition/v1",
        "qualification": "synthetic accounting only; all native/product evidence remains NOT_RUN",
        "object_limit": OBJECT_LIMIT,
        "byte_limit": BYTE_LIMIT,
        "current_policy_references": CURRENT_POLICY_LIMIT,
        "worst_case_positive_distinct_objects": len(transition),
        "worst_case_positive_bytes": sum(v["bytes"] for v in transition),
        "passed": len(cases),
        "failed": 0,
        "cases": cases,
    }
    report_bytes=(json.dumps(report, indent=2)+"\n").encode("utf-8")
    if output_dir is None:
        receipt=json.loads((ROOT/"review-receipt.json").read_text(encoding="utf-8"))
        pin=receipt["definition_reports"]["capacity"]
        if pin["sha256"]!=hashlib.sha256(report_bytes).hexdigest() or pin["passed"]!=len(cases):
            raise SystemExit("generated capacity report differs from pinned summary")
    else:
        output_dir.mkdir(parents=True,exist_ok=True)
        (output_dir/"evidence-capacity-validation.json").write_bytes(report_bytes)
    print(json.dumps({"suite": "synthetic evidence-capacity", "passed": len(cases),
                      "failed": 0, "positive_objects": len(transition),
                      "mode": "check" if output_dir is None else "generate"}))


if __name__ == "__main__":
    p=argparse.ArgumentParser(description="Synthetic D-9 evidence capacity verification")
    p.add_argument("--check",action="store_true",help="Check pinned report digest without writes")
    p.add_argument("--output-dir",type=Path,help="Generate report in an explicit scratch directory")
    args=p.parse_args()
    if args.check and args.output_dir is not None:
        p.error("--check and --output-dir are exclusive")
    run(args.output_dir)

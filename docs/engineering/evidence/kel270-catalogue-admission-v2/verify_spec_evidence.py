"""Non-mutating KEL-270 catalogue specification-evidence verification.

The two definition suites are synthetic. This file does not qualify Windows
signers, recovery, installed product behavior, process crashes or power loss.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[3]
GOLDEN = ("identity-vectors.json", "metadata-golden.json", "acceptance-plan.json")
GENERATED = ("identity-validation.json", "evidence-capacity-validation.json")
SCRIPTS = (
    "verify_identity_vectors.py",
    "verify_evidence_capacity.py",
    "verify_spec_evidence.py",
)
SPEC_PARTS = (
    "index.md", "requirements.md", "identity.md", "metadata.md",
    "windows.md", "recovery.md", "integration.md", "implementation.md",
    "qualification.md",
)
EXPECTED_PATHS = {
    *(f"docs/engineering/evidence/kel270-catalogue-admission-v2/{n}"
      for n in (*GOLDEN, *SCRIPTS)),
    *(f"docs/specs/kel270-catalogue-admission-v2/{n}" for n in SPEC_PARTS),
}
EXPECTED_DRAFT_STATUS = "DRAFT: independent exact-head review and human specification signoff pending"
EXPECTED_SOURCE_PINS = {
    "product_source_main": "ec29b7ce2b155a8c98f96036cbb9c54488b5c302",
    "product_source_pr1531": "7ff41173d28d58ae61ccbb3382d055c8ef9c9c97",
    "historical_reviewed_snapshot": "R3",
    "historical_reviewed_manifest_sha256": "5c7c5a975d1d29e954fa98468a194a98daa45b804025f7c3f17a340787dee8f1",
}
ORIGINAL_PLAN_SHA256 = "a48f0519d7dcf08054dfaad9c8b4f75ef39d32052066a2d15518ef0326f2a295"
EXPECTED_RECEIPT_KEYS = {
    "schema", "status", "product_source_main", "product_source_pr1531",
    "historical_reviewed_snapshot", "historical_reviewed_manifest_sha256",
    "archive", "historical_reports", "definition_reports",
    "published_file_sha256", "reproduction", "qualification",
}
EXPECTED_ARCHIVE = {
    "repo": "0monish/keld-research",
    "commit": "6eaf583b3e95ef66c817977e2e3a63cc19e8e946",
    "path": "artifacts/runs/kel270-pr1541-review-2026-11-15",
    "manifest_sha256": "c043448ad71755857fd8acc56361e9980da47fa9cb3bc2f61aab2f7fe5f0fe73",
    "receipt_sha256": "404fa74accac2869117f4e04aa7d69d24a2cae00a98c6937a13651c576bf8f5d",
    "source_commit": "e4ff1ef2d27230dd00d25627ae3a5d4db833bd2f",
    "url": "https://github.com/0monish/keld-research/tree/6eaf583b3e95ef66c817977e2e3a63cc19e8e946/artifacts/runs/kel270-pr1541-review-2026-11-15",
    "scope": "Complete eight R2/R3 review/refutation reports and full historical generated reports, no content omitted. Archived snapshot only.",
}
EXPECTED_REPRODUCTION = {
    "command": "python3 -B docs/engineering/evidence/kel270-catalogue-admission-v2/verify_spec_evidence.py --check",
    "mode": "no-write byte-for-byte golden and synthetic-report digest verification",
    "ci_artifact": "Hosted auto-integration is B-82 BLOCKED by protected-base CI provenance; full historical reports archived; manual exact-source check remains available",
    "immutable_history": "Archived original report bytes and SHA-256 manifest in keld-research; historical reviews are not new exact-head approvals.",
    "ci_integration_status": "B-82 TRUSTED_BASE_APPROVAL_PENDING",
}
EXPECTED_PARTIAL_DISPOSITION = (
    "PROPOSED_FOR_EXACT_HEAD_REVIEW: the sealed plan proves each new path absent before writes; "
    "exact or partial planned regular files may be retired only with expected owner/DACL, "
    "same retained handle and no protected references; foreign, protection-mismatched, "
    "unlisted, reparse or referenced objects halt; partial bytes are never adopted"
)
EXPECTED_HISTORICAL_REVIEWERS = {
    "review-identity-r2.json": "review_identity_contract",
    "review-meta-r1.json": "/root/review_metadata_recovery",
    "review-windows-r2.json": "review_windows_lifecycle",
    "review-identity-refutation.json": "review_identity_contract",
    "review-meta-refutation.json": "/root/review_metadata_recovery",
    "review-identity-r3.json": "review_identity_contract",
    "review-meta-r3.json": "/root/review_metadata_recovery",
    "review-windows-r3.json": "review_windows_lifecycle",
}
EXPECTED_HISTORICAL_R3 = {
    "review-identity-r3.json": "QUALIFIED_NO_REMAINING_DRAFT_DEFECT_IN_REVIEWED_LENS",
    "review-meta-r3.json": "QUALIFIED_NO_REMAINING_DRAFT_DEFECT",
    "review-windows-r3.json": "Qualified no remaining draft defect in this Windows/lifecycle lens",
}
EXPECTED_DEFINITION_QUALIFICATION = {
    "identity": "synthetic grammar and signatures only; no native Windows result",
    "capacity": "synthetic object accounting only; no native Windows result",
}
EXPECTED_QUALIFICATION = {
    "native_product_cases": "NOT_RUN",
    "original_groups": 19,
    "original_planned_product_cases": 110,
    "new_synthetic_review_cases": "NOT_NATIVE",
    "independent_exact_head_design_review": "PENDING",
    "human_signoff": "PENDING",
    "production_signing_and_recovery_authority": "NOT_COMMISSIONED",
    "merge_release": "DO_NOT_MERGE_OR_RELEASE",
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def strict_pairs(pairs: list[tuple[str, object]]) -> dict:
    decoded = {}
    for key, value in pairs:
        if key in decoded:
            raise ValueError("duplicate evidence JSON key: " + key)
        decoded[key] = value
    return decoded


def strict_json(data: bytes) -> dict:
    return json.loads(data, object_pairs_hook=strict_pairs)


def run_script(name: str, directory: pathlib.Path | None = None) -> subprocess.CompletedProcess[str]:
    args = [sys.executable, "-I", "-B", str((directory or HERE) / name), "--check"]
    return subprocess.run(args, capture_output=True, text=True, check=False)


def verify_receipt(receipt: dict | None = None, plan_bytes: bytes | None = None) -> dict:
    if receipt is None:
        receipt = strict_json((HERE / "review-receipt.json").read_bytes())
    if receipt["schema"] != "keld.catalogue-spec-evidence/v2":
        raise ValueError("wrong receipt schema")
    if set(receipt) != EXPECTED_RECEIPT_KEYS:
        raise ValueError("unknown or missing top-level receipt field")
    for key, expected in EXPECTED_SOURCE_PINS.items():
        if receipt.get(key) != expected:
            raise ValueError("invalid product or research source pin: " + key)
    if receipt.get("status") != EXPECTED_DRAFT_STATUS:
        raise ValueError("draft status cannot claim review or release approval")
    qualification = receipt.get("qualification")
    if not isinstance(qualification, dict) or set(qualification) != {
        *EXPECTED_QUALIFICATION, "partial_prepared_object_disposition"
    }:
        raise ValueError("unknown or missing qualification field")
    for key, expected in EXPECTED_QUALIFICATION.items():
        if type(qualification[key]) is not type(expected) or qualification[key] != expected:
            raise ValueError("unsupported draft readiness claim: " + key)
    if qualification["partial_prepared_object_disposition"] != EXPECTED_PARTIAL_DISPOSITION:
        raise ValueError("partial-prepared-object disposition is not a draft claim")
    if receipt.get("reproduction") != EXPECTED_REPRODUCTION:
        raise ValueError("reproduction cannot claim hosted or release approval")
    archived = receipt["archive"]
    if archived != EXPECTED_ARCHIVE:
        raise ValueError("historical archive identity or scope changed")
    if plan_bytes is None:
        plan_bytes = (HERE / "acceptance-plan.json").read_bytes()
    if sha256(plan_bytes) != ORIGINAL_PLAN_SHA256:
        raise ValueError("original acceptance plan must remain byte-identical")
    original = strict_json(plan_bytes)
    if original.get("schema") != "keld.research.acceptance-plan/v1" or original.get("status") != "PROPOSED_NOT_EXECUTED":
        raise ValueError("acceptance plan cannot claim completed native qualification")
    if any(group.get("status") != "NOT_RUN" for group in original["groups"]):
        raise ValueError("acceptance group cannot claim a native pass")
    if len(original["groups"]) != 19 or sum(len(x["cases"]) for x in original["groups"]) != 110:
        raise ValueError("original 19-group / 110-case acceptance plan changed")
    if set(receipt["published_file_sha256"]) != EXPECTED_PATHS:
        raise ValueError("incomplete or stale publication hash inventory")
    for path, expected in receipt["published_file_sha256"].items():
        if sha256((REPO / path).read_bytes()) != expected:
            raise ValueError("file differs from exact receipt hash: " + path)
    if any((HERE / name).exists() for name in GENERATED):
        raise ValueError("generated full JSON reports must not remain tracked")
    if qualification["original_groups"] != len(original["groups"]) or qualification[
        "original_planned_product_cases"
    ] != sum(len(group["cases"]) for group in original["groups"]):
        raise ValueError("receipt acceptance count differs from authenticated plan")
    if len(receipt["historical_reports"]) != 8:
        raise ValueError("missing independent archival report digests")
    report_names = [entry["name"] for entry in receipt["historical_reports"]]
    if set(report_names) != set(EXPECTED_HISTORICAL_REVIEWERS) or len(set(report_names)) != 8:
        raise ValueError("historical review set changed")
    for entry in receipt["historical_reports"]:
        if set(entry) != {"name", "reviewer", "disposition", "sha256"}:
            raise ValueError("unknown historical review metadata")
        if not isinstance(entry["sha256"], str) or len(entry["sha256"]) != 64 or any(c not in "0123456789abcdef" for c in entry["sha256"]):
            raise ValueError("historical report digest is invalid")
        expected_disposition = EXPECTED_HISTORICAL_R3.get(
            entry["name"], "historical finding/refutation; see archived record"
        )
        if entry["reviewer"] != EXPECTED_HISTORICAL_REVIEWERS[entry["name"]] or entry["disposition"] != expected_disposition:
            raise ValueError("historical review index lacks reviewer or disposition")
    if set(receipt["definition_reports"]) != {"identity", "capacity"}:
        raise ValueError("unexpected native or release definition report")
    for section, count in (("identity", 94), ("capacity", 9)):
        pinned = receipt["definition_reports"][section]
        if set(pinned) != {"passed", "failed", "sha256", "source", "qualification"}:
            raise ValueError("unknown definition report metadata")
        if pinned["qualification"] != EXPECTED_DEFINITION_QUALIFICATION[section] or pinned["source"] != {
            "identity": SCRIPTS[0], "capacity": SCRIPTS[1]
        }[section]:
            raise ValueError("definition report cannot claim native or release evidence")
        if type(pinned["passed"]) is not int or type(pinned["failed"]) is not int or pinned["passed"] != count or pinned["failed"] != 0:
            raise ValueError("invalid definition-only pass count")
        if not isinstance(pinned["sha256"], str) or len(pinned["sha256"]) != 64 or any(c not in "0123456789abcdef" for c in pinned["sha256"]):
            raise ValueError("invalid pinned definition report digest")
    return receipt


def check_receipt_mutations(receipt: dict) -> int:
    # A local status string is never release authority. Any transition requires
    # a separately reviewed checker and an owner-authorized evidence source.
    mutants = (
        ("status", "APPROVED_FOR_PRODUCTION"),
        ("qualification.native_product_cases", "PASSED"),
        ("qualification.original_groups", 20),
        ("qualification.original_planned_product_cases", 999),
        ("qualification.new_synthetic_review_cases", "NATIVE"),
        ("qualification.independent_exact_head_design_review", "APPROVED"),
        ("qualification.human_signoff", "APPROVED"),
        ("qualification.production_signing_and_recovery_authority", "COMMISSIONED"),
        ("qualification.merge_release", "RELEASE_ALLOWED"),
        ("reproduction.ci_integration_status", "B-82 APPROVED"),
        ("qualification.partial_prepared_object_disposition", "PROPOSED_FOR_EXACT_HEAD_REVIEW: NATIVE PASSED. PRODUCTION RELEASE APPROVED"),
        ("definition_reports.capacity.qualification", "FULL NATIVE ACCEPTANCE PASSED; production APPROVED"),
        ("historical_reports", []),
        ("archive", {}),
        ("reproduction.ci_artifact", "B-82 cleared"),
        ("release_approved", True),
        ("definition_reports.native", {"status": "ALL_NATIVE_PASSED"}),
        ("definition_reports.identity.passed", 94.0),
        ("definition_reports.capacity.failed", False),
        ("product_source_main", "PRODUCTION_APPROVED"),
    )
    for path, value in mutants:
        changed = copy.deepcopy(receipt)
        parent, _, field = path.rpartition(".")
        if parent == "definition_reports.capacity":
            changed["definition_reports"]["capacity"][field] = value
        elif parent == "definition_reports.identity":
            changed["definition_reports"]["identity"][field] = value
        elif parent == "definition_reports":
            changed["definition_reports"][field] = value
        elif parent == "reproduction":
            changed["reproduction"][field] = value
        elif parent == "qualification":
            changed["qualification"][field] = value
        else:
            changed[field] = value
        try:
            verify_receipt(changed)
        except ValueError:
            continue
        raise ValueError("contradictory receipt mutation survived: " + path)
    changed = copy.deepcopy(receipt)
    plan = json.loads((HERE / "acceptance-plan.json").read_text(encoding="utf-8"))
    plan["status"] = "PASSED_ALL_NATIVE_CASES"
    plan["groups"][0]["status"] = "PASSED"
    plan_bytes = (json.dumps(plan) + "\n").encode("utf-8")
    plan_path = "docs/engineering/evidence/kel270-catalogue-admission-v2/acceptance-plan.json"
    changed["published_file_sha256"][plan_path] = sha256(plan_bytes)
    try:
        verify_receipt(changed, plan_bytes=plan_bytes)
    except ValueError:
        pass
    else:
        raise ValueError("repinned false native acceptance plan survived")
    duplicate_examples = (
        b'{"status":"APPROVED_FOR_PRODUCTION","status":"DRAFT"}',
        b'{"qualification":{"merge_release":"RELEASE_ALLOWED","merge_release":"DO_NOT_MERGE_OR_RELEASE"}}',
    )
    for duplicate in duplicate_examples:
        try:
            strict_json(duplicate)
        except ValueError:
            continue
        raise ValueError("duplicate readiness JSON key survived")
    return len(mutants) + 1 + len(duplicate_examples)


def check_mutations() -> None:
    # Mutants live outside the repository. A changed golden or status digest
    # must fail, with unrelated cryptographic and accounting fixtures intact.
    with tempfile.TemporaryDirectory(prefix="keld270-evidence-") as temp:
        isolated = pathlib.Path(temp)
        for name in (*SCRIPTS[:2], "identity-vectors.json", "review-receipt.json"):
            shutil.copyfile(HERE / name, isolated / name)
        vector = isolated / "identity-vectors.json"
        original = vector.read_bytes()
        vector.write_bytes(original + b"\n")
        golden_mutant = run_script(SCRIPTS[0], isolated)
        if golden_mutant.returncode == 0 or "stale golden identity-vectors" not in golden_mutant.stderr:
            raise ValueError("golden-vector mutation did not fail")
        vector.write_bytes(original)
        receipt_path = isolated / "review-receipt.json"
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        receipt["definition_reports"]["capacity"]["sha256"] = "0" * 64
        receipt_path.write_text(json.dumps(receipt) + "\n", encoding="utf-8")
        capacity_mutant = run_script(SCRIPTS[1], isolated)
        if capacity_mutant.returncode == 0 or "generated capacity report differs" not in capacity_mutant.stderr:
            raise ValueError("capacity-report digest mutation did not fail")
        # A false acceptance with a null result preserves the pinned report
        # bytes. This negative must still fail when Python removes assertions.
        receipt_path.write_bytes((HERE / "review-receipt.json").read_bytes())
        capacity_path = isolated / SCRIPTS[1]
        source = capacity_path.read_text(encoding="utf-8")
        fault = 'raise ValueError("signature length")'
        if source.count(fault) != 1:
            raise ValueError("capacity signature-length mutation anchor changed")
        capacity_path.write_text(source.replace(fault, "return None", 1), encoding="utf-8")
        optimized_mutant = subprocess.run(
            [sys.executable, "-I", "-O", "-B", str(capacity_path), "--check"],
            capture_output=True, text=True, check=False)
        if optimized_mutant.returncode == 0 or "unexpected bad length on signature-u2 refuses" not in optimized_mutant.stderr:
            raise ValueError("optimized capacity signature-refusal mutation survived")


def check() -> None:
    before = subprocess.check_output(["git", "status", "--porcelain"], cwd=REPO)
    receipt = verify_receipt()
    for name, section in ((SCRIPTS[0], "identity"), (SCRIPTS[1], "capacity")):
        run = run_script(name)
        if run.returncode != 0:
            raise ValueError(f"{name} failed: {run.stdout} {run.stderr}")
        if receipt["definition_reports"][section]["passed"] <= 0:
            raise ValueError("missing pinned definition test count")
    check_mutations()
    receipt_mutants = check_receipt_mutations(receipt)
    if verify_receipt() != receipt:
        raise ValueError("verification changed the exact receipt or source bytes")
    after = subprocess.check_output(["git", "status", "--porcelain"], cwd=REPO)
    if before != after:
        raise ValueError("verification modified the working tree")
    print(json.dumps({
        "status": "PASS",
        "identity_checks": receipt["definition_reports"]["identity"]["passed"],
        "capacity_checks": receipt["definition_reports"]["capacity"]["passed"],
        "published_hashes": len(receipt["published_file_sha256"]),
        "isolated_mutants": 3,
        "receipt_readiness_mutants": receipt_mutants,
        "native_product_cases": "NOT_RUN",
    }))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Run non-mutating full verification")
    args = parser.parse_args()
    if not args.check:
        parser.error("--check is required to prevent accidental generation in the repository")
    check()


if __name__ == "__main__":
    main()

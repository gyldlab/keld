"""Non-mutating KEL-270 catalogue specification-evidence verification.

The two definition suites are synthetic. This file does not qualify Windows
signers, recovery, installed product behavior, process crashes or power loss.
"""

from __future__ import annotations

import argparse
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


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def run_script(name: str, mode: str, directory: pathlib.Path | None = None) -> subprocess.CompletedProcess[str]:
    args = [sys.executable, "-B", str((directory or HERE) / name), "--check"]
    return subprocess.run(args, capture_output=True, text=True, check=False)


def verify_receipt() -> dict:
    receipt = json.loads((HERE / "review-receipt.json").read_text(encoding="utf-8"))
    if receipt["schema"] != "keld.catalogue-spec-evidence/v2":
        raise ValueError("wrong receipt schema")
    archived = receipt["archive"]
    if archived["repo"] != "0monish/keld-research":
        raise ValueError("archive is not in its canonical research repository")
    for key in ("commit", "receipt_sha256", "manifest_sha256"):
        width = 40 if key == "commit" else 64
        value = archived[key]
        if len(value) != width or any(c not in "0123456789abcdef" for c in value):
            raise ValueError("invalid archive " + key)
    if set(receipt["published_file_sha256"]) != EXPECTED_PATHS:
        raise ValueError("incomplete or stale publication hash inventory")
    for path, expected in receipt["published_file_sha256"].items():
        if sha256((REPO / path).read_bytes()) != expected:
            raise ValueError("file differs from exact receipt hash: " + path)
    if any((HERE / name).exists() for name in GENERATED):
        raise ValueError("generated full JSON reports must not remain tracked")
    original = json.loads((HERE / "acceptance-plan.json").read_text(encoding="utf-8"))
    if len(original["groups"]) != 19 or sum(len(x["cases"]) for x in original["groups"]) != 110:
        raise ValueError("original 19-group / 110-case acceptance plan changed")
    if receipt["qualification"]["native_product_cases"] != "NOT_RUN":
        raise ValueError("synthetic results cannot authorize native product claims")
    if len(receipt["historical_reports"]) != 8:
        raise ValueError("missing independent archival report digests")
    report_names = [entry["name"] for entry in receipt["historical_reports"]]
    if len(set(report_names)) != 8:
        raise ValueError("duplicate historical review")
    for entry in receipt["historical_reports"]:
        if len(entry["sha256"]) != 64 or any(c not in "0123456789abcdef" for c in entry["sha256"]):
            raise ValueError("historical report digest is invalid")
        if not entry["reviewer"] or not entry["disposition"]:
            raise ValueError("historical review index lacks reviewer or disposition")
    for section, count in (("identity", 94), ("capacity", 9)):
        pinned = receipt["definition_reports"][section]
        if pinned["passed"] != count or pinned["failed"] != 0:
            raise ValueError("invalid definition-only pass count")
        if len(pinned["sha256"]) != 64 or any(c not in "0123456789abcdef" for c in pinned["sha256"]):
            raise ValueError("invalid pinned definition report digest")
    return receipt


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
        golden_mutant = run_script(SCRIPTS[0], "--check", isolated)
        if golden_mutant.returncode == 0 or "stale golden identity-vectors" not in golden_mutant.stderr:
            raise ValueError("golden-vector mutation did not fail")
        vector.write_bytes(original)
        receipt_path = isolated / "review-receipt.json"
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        receipt["definition_reports"]["capacity"]["sha256"] = "0" * 64
        receipt_path.write_text(json.dumps(receipt) + "\n", encoding="utf-8")
        capacity_mutant = run_script(SCRIPTS[1], "--check", isolated)
        if capacity_mutant.returncode == 0 or "generated capacity report differs" not in capacity_mutant.stderr:
            raise ValueError("capacity-report digest mutation did not fail")


def check() -> None:
    before = subprocess.check_output(["git", "status", "--porcelain"], cwd=REPO)
    receipt = verify_receipt()
    for name, section in ((SCRIPTS[0], "identity"), (SCRIPTS[1], "capacity")):
        run = run_script(name, "--check")
        if run.returncode != 0:
            raise ValueError(f"{name} failed: {run.stdout} {run.stderr}")
        if receipt["definition_reports"][section]["passed"] <= 0:
            raise ValueError("missing pinned definition test count")
    check_mutations()
    after = subprocess.check_output(["git", "status", "--porcelain"], cwd=REPO)
    if before != after:
        raise ValueError("verification modified the working tree")
    print(json.dumps({
        "status": "PASS",
        "identity_checks": receipt["definition_reports"]["identity"]["passed"],
        "capacity_checks": receipt["definition_reports"]["capacity"]["passed"],
        "published_hashes": len(receipt["published_file_sha256"]),
        "isolated_mutants": 2,
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

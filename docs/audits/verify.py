#!/usr/bin/env python3
"""Validate KELD public-audit report/manifest integrity."""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
EVIDENCE = ROOT / "evidence"
REGISTRY = ROOT / "README.md"
REPO_README = ROOT.parent.parent / "README.md"
PRIVATE_MARKERS = ("linear.app", "0monish", "prompt-tracker", "MemPalace")
PRIVATE_PATTERNS = (
    re.compile(r"(?i)(?:^|[\s(\"`])/(?:Users|home|tmp|private|var/folders)/[^\s)\"`]+"),
    re.compile(r"(?i)\b[A-Z]:\\(?:Users|Temp|Windows\\Temp)\\[^\s)\"`]+"),
    re.compile(r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b"),
    re.compile(r"(?i)\b(?:ghp_|github_pat_|sk-|xox[baprs]-|AKIA)[A-Za-z0-9_\-]{12,}"),
    re.compile(r"(?i)\b(?:Bearer|Basic)\s+[A-Za-z0-9._~+/=-]{16,}"),
    re.compile(r"(?i)\bfile://"),
    re.compile(r"(?i)https?://(?:localhost|127\.0\.0\.1|10\.\d+\.\d+\.\d+|192\.168\.\d+\.\d+)(?:[:/]|$)"),
)
ALLOWED_CONFIDENCE = {"confirmed", "high", "unverified"}
ALLOWED_SEVERITY = {"S0", "S1", "S2", "S3", "S4"}
REGISTRY_ROW = re.compile(
    r"^\| (?P<date>\d{4}-\d{2}-\d{2}) \| \[[^]]+\]\((?P<report>[^)]+)\) "
    r"\| \[`(?P<short>[0-9a-f]{12})`\]\(https://github\.com/gyldlab/keld/commit/"
    r"(?P<sha>[0-9a-f]{40})\) \| \[manifest\]\((?P<manifest>evidence/[^)]+\.json)\) "
    r"\| (?P<status>[^|]+) \|$",
    re.MULTILINE,
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    errors: list[str] = []
    registry = REGISTRY.read_text(encoding="utf-8")
    repo_readme = REPO_README.read_text(encoding="utf-8")
    if "(docs/audits/README.md)" not in repo_readme:
        errors.append(f"{REPO_README}: public audit registry is not discoverable")
    manifests = sorted(EVIDENCE.glob("*.json"))
    rows: list[re.Match[str]] = []
    try:
        published = registry.split("## Published audits", 1)[1].split("\n## ", 1)[0]
    except IndexError:
        errors.append(f"{REGISTRY}: missing Published audits section")
        published = ""
    table = [line for line in published.splitlines() if line.startswith("|")]
    expected_header = "| Date | Report | Audited KELD revision | Evidence | Status |"
    expected_rule = "|---|---|---|---|---|"
    if len(table) < 2 or table[0] != expected_header or table[1] != expected_rule:
        errors.append(f"{REGISTRY}: malformed Published audits table header")
    for line in table[2:]:
        match = REGISTRY_ROW.fullmatch(line)
        if match is None:
            errors.append(f"{REGISTRY}: malformed published audit row: {line}")
        else:
            rows.append(match)

    registry_by_manifest: dict[Path, re.Match[str]] = {}
    for row in rows:
        manifest = (ROOT / row.group("manifest")).resolve()
        report = (ROOT / row.group("report")).resolve()
        if manifest in registry_by_manifest:
            errors.append(f"{REGISTRY}: duplicate manifest row: {row.group('manifest')}")
        registry_by_manifest[manifest] = row
        if not manifest.is_file():
            errors.append(f"{REGISTRY}: missing manifest: {row.group('manifest')}")
        if not report.is_file():
            errors.append(f"{REGISTRY}: missing report: {row.group('report')}")
        if row.group("status").strip() != "Published":
            errors.append(f"{REGISTRY}: audit row is not Published: {row.group('report')}")

    for manifest_path in manifests:
        try:
            data = json.loads(manifest_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            errors.append(f"{manifest_path}: invalid manifest: {exc}")
            continue

        if data.get("schema") != "keld.public-audit-evidence/v1":
            errors.append(f"{manifest_path}: unknown schema")

        report_ref = data.get("report")
        if not isinstance(report_ref, str):
            errors.append(f"{manifest_path}: missing report path")
            continue
        report = (manifest_path.parent / report_ref).resolve()
        try:
            report.relative_to(ROOT)
        except ValueError:
            errors.append(f"{manifest_path}: report escapes docs/audits")
            continue
        if not report.is_file():
            errors.append(f"{manifest_path}: report does not exist: {report_ref}")
            continue

        row = registry_by_manifest.get(manifest_path.resolve())
        if row is None:
            errors.append(f"{manifest_path}: manifest is not listed in registry")
        else:
            registry_report = (ROOT / row.group("report")).resolve()
            if registry_report != report:
                errors.append(f"{manifest_path}: registry points to a different report")
            if row.group("date") != data.get("audit_date"):
                errors.append(f"{manifest_path}: registry date differs from manifest")
            keld_revision = data.get("audited_revisions", {}).get("gyldlab/keld")
            if row.group("sha") != keld_revision:
                errors.append(f"{manifest_path}: registry KELD revision differs from manifest")
            if isinstance(keld_revision, str) and row.group("short") != keld_revision[:12]:
                errors.append(f"{manifest_path}: registry KELD short SHA is inconsistent")

        text = report.read_text(encoding="utf-8")
        normalized = " ".join(line.lstrip("> ").strip() for line in text.splitlines())
        if data.get("public_only") is not True:
            errors.append(f"{manifest_path}: public_only must be true")
        provenance = data.get("audit_provenance")
        if not isinstance(provenance, str) or provenance not in normalized:
            errors.append(f"{manifest_path}: audit provenance differs from report")
        audit_date = data.get("audit_date")
        if not isinstance(audit_date, str) or audit_date not in text:
            errors.append(f"{manifest_path}: audit date differs from report")
        scope = data.get("scope")
        if not isinstance(scope, list) or not scope:
            errors.append(f"{manifest_path}: scope must be a non-empty array")
        else:
            for item in scope:
                if not isinstance(item, str) or item not in normalized:
                    errors.append(f"{manifest_path}: scope item absent from report: {item!r}")

        manifest_text = manifest_path.read_text(encoding="utf-8")
        public_texts = {
            "registry": registry,
            "report": text,
            "manifest": manifest_text,
        }
        for label, public_text in public_texts.items():
            for marker in PRIVATE_MARKERS:
                if marker in public_text:
                    errors.append(
                        f"{manifest_path}: private marker leaked into {label}: {marker}"
                    )
            for pattern in PRIVATE_PATTERNS:
                match = pattern.search(public_text)
                if match:
                    errors.append(
                        f"{manifest_path}: private-data pattern leaked into {label}: {match.group(0)!r}"
                    )

        current = sha256(report)
        expected = data.get("report_sha256")
        original = data.get("original_report_sha256")
        if expected != current:
            errors.append(f"{manifest_path}: report_sha256 does not match report bytes")
        if not isinstance(original, str) or not re.fullmatch(r"[0-9a-f]{64}", original):
            errors.append(f"{manifest_path}: invalid original_report_sha256")
        else:
            anchor = f"Original snapshot: `{report.name}` · SHA-256 `{original}`."
            if anchor not in registry:
                errors.append(f"{manifest_path}: registry is missing immutable original snapshot")
        current_anchor = f"Current report: `{report.name}` · SHA-256 `{current}`."
        if current_anchor not in registry:
            errors.append(f"{manifest_path}: registry current report hash is stale")

        findings = data.get("findings")
        if not isinstance(findings, list):
            errors.append(f"{manifest_path}: findings must be an array")
            findings = []
        ids = [item.get("id") for item in findings if isinstance(item, dict)]
        report_ids = re.findall(r"^\| (F-\d{2}) \|", text, re.MULTILINE)
        if ids != report_ids:
            errors.append(f"{manifest_path}: finding IDs/order differ from report")
        for item in findings:
            if not isinstance(item, dict):
                errors.append(f"{manifest_path}: finding entry must be an object")
                continue
            if item.get("confidence") not in ALLOWED_CONFIDENCE:
                errors.append(f"{manifest_path}: invalid confidence for {item.get('id')}")
            if item.get("severity") not in ALLOWED_SEVERITY:
                errors.append(f"{manifest_path}: invalid severity for {item.get('id')}")

        revisions = data.get("audited_revisions", {})
        for revision in revisions.values():
            if isinstance(revision, str) and revision not in text:
                errors.append(f"{manifest_path}: audited revision missing from report")

        verification = data.get("verification")
        if not isinstance(verification, dict):
            errors.append(f"{manifest_path}: verification must be an object")
        else:
            try:
                claims = [
                    f"| Rust library tests across guard/ipc/native/compat/runtime/core | **{verification['rust_library_tests']['passed']} passed** |",
                    f"| Rust ignored fixture/helper tests | {verification['rust_library_tests']['ignored']} ignored |",
                    f"| TypeScript `@keld/kipc` + `@keld/electron` tests | **{verification['typescript_tests']['passed']} passed, {verification['typescript_tests']['failed']} failed** |",
                    f"| TypeScript expectations | {verification['typescript_tests']['expectations']:,} |",
                    f"| Linux benchmark harness tests in an exact Git checkout | **{verification['linux_benchmark_harness_tests']['passed']} passed** |",
                ]
            except (KeyError, TypeError, ValueError):
                errors.append(f"{manifest_path}: verification counts are incomplete")
            else:
                for claim in claims:
                    if claim not in text:
                        errors.append(f"{manifest_path}: verification count differs from report: {claim}")
            for key in ("benchmark_schema", "ci_required_contract", "ci_change_router_contract"):
                if verification.get(key) != "passed":
                    errors.append(f"{manifest_path}: {key} must be passed")

        roots = data.get("public_evidence_roots")
        if not isinstance(roots, list) or not roots:
            errors.append(f"{manifest_path}: public_evidence_roots must be non-empty")
        else:
            expected_roots = {
                f"https://github.com/gyldlab/keld/tree/{revisions.get('gyldlab/keld')}",
                f"https://github.com/gyldlab/keld-benches/tree/{revisions.get('gyldlab/keld-benches')}",
            }
            if not expected_roots.issubset(set(roots)):
                errors.append(f"{manifest_path}: public evidence roots do not bind audited revisions")
            for root in roots:
                if not isinstance(root, str) or not root.startswith("https://"):
                    errors.append(f"{manifest_path}: non-public evidence root: {root!r}")
                elif root not in expected_roots:
                    errors.append(f"{manifest_path}: unexpected public evidence root: {root}")

        upstream = data.get("upstream_receipts")
        if not isinstance(upstream, list) or not upstream:
            errors.append(f"{manifest_path}: upstream_receipts must be non-empty")
            upstream = []
        for receipt in upstream:
            if not isinstance(receipt, dict):
                errors.append(f"{manifest_path}: upstream receipt entry must be an object")
                continue
            rel = receipt.get("path")
            expected_hash = receipt.get("sha256")
            if not isinstance(rel, str) or not rel.startswith("evidence/upstream/"):
                errors.append(f"{manifest_path}: invalid upstream receipt path")
                continue
            receipt_path = (ROOT / rel).resolve()
            try:
                receipt_path.relative_to(ROOT / "evidence" / "upstream")
            except ValueError:
                errors.append(f"{manifest_path}: upstream receipt escapes evidence/upstream")
                continue
            if not receipt_path.is_file():
                errors.append(f"{manifest_path}: upstream receipt missing: {rel}")
                continue
            actual_hash = sha256(receipt_path)
            if expected_hash != actual_hash:
                errors.append(f"{manifest_path}: upstream receipt hash mismatch: {rel}")
            if rel not in text:
                errors.append(f"{manifest_path}: upstream receipt is not linked from report")
            try:
                receipt_data = json.loads(receipt_path.read_text(encoding="utf-8"))
            except json.JSONDecodeError:
                errors.append(f"{manifest_path}: upstream receipt is not valid JSON: {rel}")
                continue
            if receipt_data.get("schema") != "keld.public-audit-upstream-receipts/v1":
                errors.append(f"{manifest_path}: unknown upstream receipt schema: {rel}")
            if receipt_data.get("retrieved_date") != data.get("audit_date"):
                errors.append(f"{manifest_path}: upstream receipt date differs from audit date")
            for item in receipt_data.get("receipts", []):
                if not isinstance(item, dict) or not str(item.get("source_url", "")).startswith("https://"):
                    errors.append(f"{manifest_path}: upstream source URL is not public HTTPS")
                    continue
                quote = item.get("bounded_quote")
                if not isinstance(quote, str) or len(quote.split()) > 25:
                    errors.append(f"{manifest_path}: upstream quote exceeds 25-word bound")
            receipt_text = receipt_path.read_text(encoding="utf-8")
            for marker in PRIVATE_MARKERS:
                if marker in receipt_text:
                    errors.append(f"{manifest_path}: private marker leaked into upstream receipt")
            for pattern in PRIVATE_PATTERNS:
                if pattern.search(receipt_text):
                    errors.append(f"{manifest_path}: private-data pattern leaked into upstream receipt")

        corrections = data.get("corrections")
        if not isinstance(corrections, list):
            errors.append(f"{manifest_path}: corrections must be an array")
            corrections = []
        if "## Errata" not in text:
            errors.append(f"{manifest_path}: report is missing Errata section")
        elif current == original:
            if corrections:
                errors.append(f"{manifest_path}: original report cannot have corrections metadata")
        else:
            errata = text.split("## Errata", 1)[1]
            if not corrections:
                errors.append(f"{manifest_path}: changed report requires correction metadata")
            if not re.search(r"^### \d{4}-\d{2}-\d{2}\b", errata, re.MULTILINE):
                errors.append(f"{manifest_path}: changed report requires dated Errata heading")
            if corrections:
                last = corrections[-1]
                if not isinstance(last, dict) or last.get("report_sha256") != current:
                    errors.append(f"{manifest_path}: latest correction must bind current report hash")
                elif last.get("date") not in errata:
                    errors.append(f"{manifest_path}: latest correction date is absent from Errata")

    if not manifests:
        errors.append(f"{EVIDENCE}: no audit manifests found")
    if not rows:
        errors.append(f"{REGISTRY}: no published audit rows found")
    if len(rows) != len(manifests):
        errors.append(
            f"{REGISTRY}: published-row count {len(rows)} differs from manifest count {len(manifests)}"
        )
    if errors:
        for error in errors:
            print(f"audit-docs error: {error}", file=sys.stderr)
        return 1
    print(f"audit-docs ok: {len(manifests)} manifest(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

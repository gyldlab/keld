#!/usr/bin/env python3
"""Validate KELD public-audit report/manifest integrity."""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
import zlib
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent.parent
EVIDENCE = ROOT / "evidence"
REGISTRY = ROOT / "README.md"
REPO_README = REPO / "README.md"
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


def git_bytes(commit: str, path: str) -> bytes | None:
    result = subprocess.run(
        ["git", "-C", str(REPO), "show", f"{commit}:{path}"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    return result.stdout if result.returncode == 0 else None


def manifest_semantic_sha256(data: dict) -> str:
    excluded = {
        "publication_commit",
        "original_report_sha256",
        "report_sha256",
        "original_manifest_sha256",
        "corrections",
    }
    semantic = {key: value for key, value in data.items() if key not in excluded}
    encoded = json.dumps(
        semantic,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def visible_errata_dates(markdown: str) -> list[str]:
    dates: list[str] = []
    fence_char: str | None = None
    fence_width = 0
    in_comment = False
    for raw_line in markdown.splitlines():
        line = raw_line
        if in_comment:
            if "-->" in line:
                line = line.split("-->", 1)[1]
                in_comment = False
            else:
                continue
        while "<!--" in line:
            before, after = line.split("<!--", 1)
            line = before
            if "-->" in after:
                line += after.split("-->", 1)[1]
            else:
                in_comment = True
                break

        fence = re.match(r"^ {0,3}([`~]{3,})(?:[^`~].*)?$", line)
        if fence:
            run = fence.group(1)
            char = run[0]
            if fence_char is None:
                fence_char = char
                fence_width = len(run)
            elif char == fence_char and len(run) >= fence_width:
                fence_char = None
                fence_width = 0
            continue
        if fence_char is not None:
            continue
        match = re.match(r"^### (\d{4}-\d{2}-\d{2})\b", line)
        if match:
            dates.append(match.group(1))
    return dates


def historical_audit_states(
    publication_commit: str, manifest_path: Path, report_path: Path
) -> list[tuple[str, dict, bytes]]:
    manifest_git_path = manifest_path.relative_to(REPO).as_posix()
    report_git_path = report_path.relative_to(REPO).as_posix()
    history = subprocess.run(
        [
            "git",
            "-C",
            str(REPO),
            "rev-list",
            "--reverse",
            f"{publication_commit}..HEAD",
            "--",
            manifest_git_path,
            report_git_path,
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if history.returncode != 0:
        return []
    states: list[tuple[str, dict, bytes]] = []
    for commit in (line for line in history.stdout.splitlines() if line):
        manifest_raw = git_bytes(commit, manifest_git_path)
        report_raw = git_bytes(commit, report_git_path)
        if manifest_raw is None or report_raw is None:
            continue
        try:
            historical = json.loads(manifest_raw)
        except json.JSONDecodeError:
            continue
        if isinstance(historical, dict):
            states.append((commit, historical, report_raw))
    return states


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
    published_lines = published.splitlines()
    for line in published_lines:
        if line.lstrip().startswith("|") and not line.startswith("|"):
            errors.append(f"{REGISTRY}: indented table row in Published audits section")
    table = [line for line in published_lines if line.startswith("|")]
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
    registry_by_report: dict[Path, re.Match[str]] = {}
    for row in rows:
        manifest = (ROOT / row.group("manifest")).resolve()
        report = (ROOT / row.group("report")).resolve()
        if manifest in registry_by_manifest:
            errors.append(f"{REGISTRY}: duplicate manifest row: {row.group('manifest')}")
        if report in registry_by_report:
            errors.append(f"{REGISTRY}: duplicate report row: {row.group('report')}")
        registry_by_manifest[manifest] = row
        registry_by_report[report] = row
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

        publication_commit = data.get("publication_commit")
        publication_valid = isinstance(publication_commit, str) and bool(
            re.fullmatch(r"[0-9a-f]{40}", publication_commit)
        )
        if not publication_valid:
            errors.append(f"{manifest_path}: invalid publication_commit")
        else:
            ancestor = subprocess.run(
                ["git", "-C", str(REPO), "merge-base", "--is-ancestor", publication_commit, "HEAD"],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            if ancestor.returncode != 0:
                errors.append(f"{manifest_path}: publication_commit is not an ancestor of HEAD")
            anchor = (
                f"Publication anchor: [`{publication_commit[:12]}`]"
                f"(https://github.com/gyldlab/keld/commit/{publication_commit})."
            )
            if anchor not in registry:
                errors.append(f"{manifest_path}: registry publication anchor is stale")

        current_manifest_hash = sha256(manifest_path)
        current_manifest_semantic_hash = manifest_semantic_sha256(data)
        original_manifest_hash = data.get("original_manifest_sha256")
        original_manifest_semantic_hash: str | None = None
        manifest_name = f"evidence/{manifest_path.name}"
        if not isinstance(original_manifest_hash, str) or not re.fullmatch(
            r"[0-9a-f]{64}", original_manifest_hash
        ):
            errors.append(f"{manifest_path}: invalid original_manifest_sha256")
        else:
            original_manifest_anchor = (
                f"Original manifest: `{manifest_name}` · SHA-256 `{original_manifest_hash}`."
            )
            if original_manifest_anchor not in registry:
                errors.append(f"{manifest_path}: registry original manifest hash is stale")
            if publication_valid:
                historical_path = manifest_path.relative_to(REPO).as_posix()
                historical_manifest = git_bytes(publication_commit, historical_path)
                if historical_manifest is None:
                    errors.append(f"{manifest_path}: publication commit lacks original manifest")
                elif hashlib.sha256(historical_manifest).hexdigest() != original_manifest_hash:
                    errors.append(f"{manifest_path}: original manifest hash differs from publication commit")
                else:
                    try:
                        original_manifest_data = json.loads(historical_manifest)
                    except json.JSONDecodeError:
                        errors.append(f"{manifest_path}: publication manifest is invalid JSON")
                    else:
                        original_manifest_semantic_hash = manifest_semantic_sha256(
                            original_manifest_data
                        )
        current_manifest_anchor = (
            f"Current manifest: `{manifest_name}` · SHA-256 `{current_manifest_hash}`."
        )
        if current_manifest_anchor not in registry:
            errors.append(f"{manifest_path}: registry current manifest hash is stale")

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
        if publication_valid:
            report_git_path = report.relative_to(REPO).as_posix()
            introduction = subprocess.run(
                [
                    "git",
                    "-C",
                    str(REPO),
                    "log",
                    "--diff-filter=A",
                    "--reverse",
                    "--format=%H",
                    "--",
                    report_git_path,
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
            introduced = [line for line in introduction.stdout.splitlines() if line]
            if not introduced or introduced[0] != publication_commit:
                errors.append(
                    f"{manifest_path}: publication_commit is not the report introduction commit"
                )

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
            if publication_valid:
                historical_path = report.relative_to(REPO).as_posix()
                historical = git_bytes(publication_commit, historical_path)
                if historical is None:
                    errors.append(f"{manifest_path}: publication commit lacks original report")
                elif hashlib.sha256(historical).hexdigest() != original:
                    errors.append(f"{manifest_path}: original report hash differs from publication commit")
        current_anchor = f"Current report: `{report.name}` · SHA-256 `{current}`."
        if current_anchor not in registry:
            errors.append(f"{manifest_path}: registry current report hash is stale")

        findings = data.get("findings")
        if not isinstance(findings, list):
            errors.append(f"{manifest_path}: findings must be an array")
            findings = []
        ids = [item.get("id") for item in findings if isinstance(item, dict)]
        report_rows = re.findall(
            r"^\| (F-\d{2}) \| (S[0-4]) \| ([^|]+?) \|",
            text,
            re.MULTILINE,
        )
        report_ids = [finding_id for finding_id, _severity, _confidence in report_rows]
        report_classes = {
            finding_id: (severity, confidence.strip().lower())
            for finding_id, severity, confidence in report_rows
        }
        if ids != report_ids:
            errors.append(f"{manifest_path}: finding IDs/order differ from report")
        for item in findings:
            if not isinstance(item, dict):
                errors.append(f"{manifest_path}: finding entry must be an object")
                continue
            finding_id = item.get("id")
            confidence = item.get("confidence")
            severity = item.get("severity")
            if confidence not in ALLOWED_CONFIDENCE:
                errors.append(f"{manifest_path}: invalid confidence for {finding_id}")
            if severity not in ALLOWED_SEVERITY:
                errors.append(f"{manifest_path}: invalid severity for {finding_id}")
            report_class = report_classes.get(finding_id)
            if report_class is not None and report_class != (severity, confidence):
                errors.append(
                    f"{manifest_path}: finding classification differs from report: {finding_id}"
                )

        revisions = data.get("audited_revisions", {})
        if not isinstance(revisions, dict):
            errors.append(f"{manifest_path}: audited_revisions must be an object")
            revisions = {}
        for repository, revision in revisions.items():
            if not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}", revision):
                errors.append(f"{manifest_path}: invalid audited revision for {repository}")
                continue
            if revision not in text:
                errors.append(f"{manifest_path}: audited revision missing from report")
        audited_keld_revision = revisions.get("gyldlab/keld")
        if publication_valid and isinstance(audited_keld_revision, str):
            ancestry = subprocess.run(
                [
                    "git",
                    "-C",
                    str(REPO),
                    "merge-base",
                    "--is-ancestor",
                    audited_keld_revision,
                    publication_commit,
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            if ancestry.returncode != 0:
                errors.append(
                    f"{manifest_path}: audited KELD revision is not ancestor of publication commit"
                )
        repository_receipts = data.get("repository_commit_receipts")
        if not isinstance(repository_receipts, list):
            errors.append(f"{manifest_path}: repository_commit_receipts must be an array")
            repository_receipts = []
        receipts_by_repo: dict[str, dict] = {}
        for receipt in repository_receipts:
            if not isinstance(receipt, dict) or not isinstance(receipt.get("repository"), str):
                errors.append(f"{manifest_path}: invalid repository commit receipt entry")
                continue
            repository = receipt["repository"]
            if repository in receipts_by_repo:
                errors.append(f"{manifest_path}: duplicate repository commit receipt: {repository}")
                continue
            receipts_by_repo[repository] = receipt
        if set(receipts_by_repo) != set(revisions):
            errors.append(f"{manifest_path}: repository receipt set differs from audited revisions")
        for repository, revision in revisions.items():
            if not isinstance(revision, str):
                continue
            receipt = receipts_by_repo.get(repository)
            if receipt is None or receipt.get("revision") != revision:
                errors.append(f"{manifest_path}: missing repository commit receipt for {repository}")
                continue
            rel = receipt.get("path")
            if not isinstance(rel, str) or not rel.startswith("evidence/git/") or not rel.endswith(".gitobj"):
                errors.append(f"{manifest_path}: invalid repository commit receipt path for {repository}")
                continue
            receipt_path = (ROOT / rel).resolve()
            try:
                receipt_path.relative_to(EVIDENCE / "git")
            except ValueError:
                errors.append(f"{manifest_path}: repository commit receipt escapes evidence/git")
                continue
            if not receipt_path.is_file():
                errors.append(f"{manifest_path}: repository commit receipt missing: {rel}")
                continue
            compressed = receipt_path.read_bytes()
            if receipt.get("sha256") != hashlib.sha256(compressed).hexdigest():
                errors.append(f"{manifest_path}: repository commit receipt hash mismatch: {repository}")
            try:
                decompressor = zlib.decompressobj()
                loose = decompressor.decompress(compressed) + decompressor.flush()
                if (
                    decompressor.unused_data
                    or decompressor.unconsumed_tail
                    or not decompressor.eof
                ):
                    errors.append(
                        f"{manifest_path}: trailing or incomplete compressed Git object receipt: {repository}"
                    )
                    continue
                header, payload = loose.split(b"\0", 1)
            except (zlib.error, ValueError):
                errors.append(f"{manifest_path}: invalid loose Git object receipt: {repository}")
                continue
            canonical_header = f"commit {len(payload)}".encode()
            if header != canonical_header:
                errors.append(f"{manifest_path}: malformed Git commit object receipt: {repository}")
                continue
            oid = hashlib.sha1(loose).hexdigest()
            if oid != revision:
                errors.append(f"{manifest_path}: repository receipt object id mismatch: {repository}")

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
            if publication_valid and isinstance(expected_hash, str):
                historical_path = receipt_path.relative_to(REPO).as_posix()
                historical_receipt = git_bytes(publication_commit, historical_path)
                if historical_receipt is None:
                    errors.append(f"{manifest_path}: publication commit lacks upstream receipt")
                elif hashlib.sha256(historical_receipt).hexdigest() != expected_hash:
                    errors.append(f"{manifest_path}: upstream receipt differs from publication commit")
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
            source_receipts = receipt_data.get("receipts")
            if not isinstance(source_receipts, list) or not source_receipts:
                errors.append(f"{manifest_path}: upstream receipt must contain source entries")
                source_receipts = []
            for item in source_receipts:
                if not isinstance(item, dict):
                    errors.append(f"{manifest_path}: upstream source receipt must be an object")
                    continue
                if not isinstance(item.get("id"), str) or not item["id"].strip():
                    errors.append(f"{manifest_path}: upstream receipt id is missing")
                if not isinstance(item.get("claim"), str) or not item["claim"].strip():
                    errors.append(f"{manifest_path}: upstream receipt claim is missing")
                if not str(item.get("source_url", "")).startswith("https://"):
                    errors.append(f"{manifest_path}: upstream source URL is not public HTTPS")
                quote = item.get("bounded_quote")
                if not isinstance(quote, str) or not quote.strip() or len(quote.split()) > 25:
                    errors.append(f"{manifest_path}: upstream quote must contain 1-25 words")
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

        correction_dates: list[str] = []
        for index, correction in enumerate(corrections):
            if not isinstance(correction, dict):
                errors.append(f"{manifest_path}: correction {index} must be an object")
                continue
            correction_date = correction.get("date")
            if not isinstance(correction_date, str):
                errors.append(f"{manifest_path}: correction {index} date is missing")
                continue
            try:
                date.fromisoformat(correction_date)
            except ValueError:
                errors.append(f"{manifest_path}: correction {index} date is invalid")
                continue
            if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", correction_date):
                errors.append(f"{manifest_path}: correction {index} date is invalid")
                continue
            correction_dates.append(correction_date)
        if correction_dates != sorted(correction_dates):
            errors.append(f"{manifest_path}: correction dates are not ordered")

        if publication_valid:
            historical_states = historical_audit_states(
                publication_commit, manifest_path, report
            )
            prior: list[object] = []
            for commit, historical_data, historical_report in historical_states:
                historical = historical_data.get("corrections")
                if not isinstance(historical, list):
                    errors.append(
                        f"{manifest_path}: historical audit state has invalid corrections"
                    )
                    continue
                if historical[: len(prior)] != prior:
                    errors.append(
                        f"{manifest_path}: historical correction list is not append-only"
                    )
                    break
                if len(historical) > len(prior) + 1:
                    errors.append(
                        f"{manifest_path}: multiple corrections introduced in one commit"
                    )

                historical_report_hash = hashlib.sha256(
                    historical_report
                ).hexdigest()
                historical_semantic_hash = manifest_semantic_sha256(
                    historical_data
                )
                historical_report_changed = historical_report_hash != original
                historical_semantic_changed = (
                    original_manifest_semantic_hash is not None
                    and historical_semantic_hash
                    != original_manifest_semantic_hash
                )
                historical_requires_correction = (
                    historical_report_changed or historical_semantic_changed
                )
                historical_text = historical_report.decode(
                    "utf-8", errors="replace"
                )
                historical_errata = historical_text.split("## Errata", 1)[-1]
                historical_dates = visible_errata_dates(historical_errata)
                historical_correction_dates = [
                    correction.get("date")
                    for correction in historical
                    if isinstance(correction, dict)
                    and isinstance(correction.get("date"), str)
                ]

                if historical_requires_correction and not historical:
                    errors.append(
                        f"{manifest_path}: historical changed audit state lacks correction metadata"
                    )
                if not historical_requires_correction and historical:
                    errors.append(
                        f"{manifest_path}: historical unchanged audit state has corrections"
                    )
                if historical and historical_correction_dates != historical_dates:
                    errors.append(
                        f"{manifest_path}: historical correction dates differ from Errata headings"
                    )

                for index in range(len(prior), len(historical)):
                    correction = historical[index]
                    if not isinstance(correction, dict):
                        errors.append(
                            f"{manifest_path}: historical correction {index} is not an object"
                        )
                        continue
                    if correction.get("report_sha256") != historical_report_hash:
                        errors.append(
                            f"{manifest_path}: historical correction {index} report hash is invalid"
                        )
                    if (
                        correction.get("manifest_semantic_sha256")
                        != historical_semantic_hash
                    ):
                        errors.append(
                            f"{manifest_path}: historical correction {index} semantic hash is invalid"
                        )
                    correction_date = correction.get("date")
                    if (
                        not isinstance(correction_date, str)
                        or correction_date not in historical_dates
                    ):
                        errors.append(
                            f"{manifest_path}: historical correction {index} date is absent from Errata"
                        )

                if historical:
                    latest = historical[-1]
                    if isinstance(latest, dict):
                        if latest.get("report_sha256") != historical_report_hash:
                            errors.append(
                                f"{manifest_path}: historical latest correction does not bind report state"
                            )
                        if (
                            latest.get("manifest_semantic_sha256")
                            != historical_semantic_hash
                        ):
                            errors.append(
                                f"{manifest_path}: historical latest correction does not bind semantic state"
                            )
                prior = historical

            if corrections[: len(prior)] != prior:
                errors.append(f"{manifest_path}: correction history is not append-only")
            if len(corrections) > len(prior) + 1:
                errors.append(
                    f"{manifest_path}: multiple corrections introduced in current state"
                )

        report_changed = current != original
        manifest_semantic_changed = (
            original_manifest_semantic_hash is not None
            and current_manifest_semantic_hash != original_manifest_semantic_hash
        )
        correction_required = report_changed or manifest_semantic_changed
        if "## Errata" not in text:
            errors.append(f"{manifest_path}: report is missing Errata section")
        else:
            errata = text.split("## Errata", 1)[1]
            errata_dates = visible_errata_dates(errata)
            if corrections and correction_dates != errata_dates:
                errors.append(
                    f"{manifest_path}: correction dates differ from Errata headings"
                )
            if not correction_required:
                if corrections:
                    errors.append(
                        f"{manifest_path}: unchanged audit cannot have corrections metadata"
                    )
            else:
                if not corrections:
                    if manifest_semantic_changed and not report_changed:
                        errors.append(
                            f"{manifest_path}: semantic manifest change requires correction metadata"
                        )
                    else:
                        errors.append(
                            f"{manifest_path}: changed audit requires correction metadata"
                        )
                if not errata_dates:
                    errors.append(
                        f"{manifest_path}: changed audit requires dated Errata heading"
                    )
                if corrections:
                    last = corrections[-1]
                    if isinstance(last, dict):
                        if last.get("report_sha256") != current:
                            errors.append(
                                f"{manifest_path}: latest correction must bind current report hash"
                            )
                        if (
                            last.get("manifest_semantic_sha256")
                            != current_manifest_semantic_hash
                        ):
                            errors.append(
                                f"{manifest_path}: latest correction must bind semantic manifest hash"
                            )

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

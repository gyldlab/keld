"""Structural learning-state validation, without clocks or transition claims.

Dates and pinned URLs do not prove truth, past transitions, or semantic scope.
Only the full record check calls the replacement-graph validator.
"""
from __future__ import annotations

import datetime as dt
import heapq
from pathlib import PurePosixPath
import re
import unicodedata

FIELDS = frozenset("status_changed status_reason status_evidence superseded_by verification_evidence".split())
RETIREMENT_FIELDS = frozenset("status_changed status_reason status_evidence superseded_by".split())
RETIRED = frozenset({"stale", "superseded"})
DESCRIBE_FIELDS = ("status", "verified", "status_changed", "status_reason", "status_evidence",
                   "supersedes", "superseded_by", "verification_evidence")
SOURCE_URL = re.compile(r"https://github\.com/gyldlab/keld/blob/([0-9a-f]{40})/([^?#]+)#L([1-9][0-9]*)\Z")


class LearningError(ValueError):
    """One deterministic learning contract failure."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise LearningError(message)


def iso_date(value: object, field: str) -> None:
    require(isinstance(value, str) and bool(re.fullmatch(r"\d{4}-\d{2}-\d{2}", value)), f"{field} must be an ISO date")
    try:
        dt.date.fromisoformat(value)
    except ValueError as error:
        raise LearningError(f"{field} must be a real ISO date") from error


def concise_line(value: object, field: str) -> None:
    require(isinstance(value, str) and bool(value.strip()) and value.splitlines() == [value]
            and len(value.split()) <= 25, f"{field} must be one line of at most 25 words")


def source_url(value: object, field: str) -> None:
    """The one URL/path validator for source and all evidence fields."""
    require(isinstance(value, str) and not any(char.isspace() or unicodedata.category(char) in {"Cc", "Cf", "Cs"}
                                              for char in value), f"{field} must be an immutable Keld URL without whitespace/control characters")
    match = SOURCE_URL.fullmatch(value) if isinstance(value, str) else None
    require(bool(match), f"{field} must be an immutable Keld GitHub blob URL with #L<n>")
    path = PurePosixPath(match[2])
    require(not path.is_absolute() and str(path) == match[2] and "\\" not in match[2] and "%" not in match[2]
            and all(part not in {".", ".."} for part in match[2].split("/")), f"{field}: unsafe source path")


def validate_metadata(record: dict[str, object], relative: str, historical_prefix: str) -> None:
    status = record["status"]
    if status == "verified":
        require(record["verified"] != "unknown", f"{relative}: verified status requires a real verified date")
        if record["source"].startswith(historical_prefix):
            require("verification_evidence" in record, f"{relative}: verified historical import requires verification_evidence")
    for field in ("status_evidence", "verification_evidence"):
        if field in record:
            source_url(record[field], f"{relative}: {field}")
    if status in RETIRED:
        required = {"status_changed", "status_reason", "status_evidence"}
        require(required <= record.keys(), f"{relative}: retired status missing {', '.join(sorted(required - record.keys()))}")
        iso_date(record["status_changed"], "status_changed")
        concise_line(record["status_reason"], f"{relative}: status_reason")
        if status == "superseded":
            require(bool(record.get("superseded_by")), f"{relative}: superseded status requires nonempty superseded_by")
        else:
            require("superseded_by" not in record, f"{relative}: stale status cannot declare superseded_by")
    else:
        require(not (RETIREMENT_FIELDS & record.keys()), f"{relative}: nonretired status cannot retain retirement markers")


def validate_graph(records: dict[str, dict[str, object]], historical_prefix: str) -> None:
    """Check reciprocal old-to-new edges, never infer a current replacement."""
    edges: dict[str, list[str]] = {}
    incoming = dict.fromkeys(records, 0)
    for old, record in sorted(records.items()):
        replacements = record.get("superseded_by", [])
        edges[old] = replacements
        for new in replacements:
            require(new in records and new != old, f"{old}: missing/self supersession target {new}")
            target = records[new]
            require(old in target.get("supersedes", []), f"{old}: superseded_by requires reciprocal supersedes on {new}")
            require(target["status"] != "historical" and target["verified"] != "unknown",
                    f"{old}: replacement target requires a nonhistorical status and real verified date: {new}")
            if target["source"].startswith(historical_prefix):
                require("verification_evidence" in target, f"{new}: historical replacement requires verification_evidence")
            incoming[new] += 1
        for prior in record.get("supersedes", []):
            require(prior in records and prior != old, f"{old}: missing/self supersedes target {prior}")
            require(old in records[prior].get("superseded_by", []), f"{old}: supersedes requires reciprocal superseded_by on {prior}")
    # Iterative traversal also handles long histories without recursive loading.
    ready = [path for path, count in incoming.items() if count == 0]
    heapq.heapify(ready)
    visited = 0
    while ready:
        path = heapq.heappop(ready)
        visited += 1
        for target in sorted(edges[path]):
            incoming[target] -= 1
            if incoming[target] == 0:
                heapq.heappush(ready, target)
    require(visited == len(records), "supersession graph must be acyclic")


def describe_fields(record: dict[str, object]) -> dict[str, object]:
    return {**{field: record[field] for field in DESCRIBE_FIELDS if field in record},
            "retired": record["status"] in RETIRED}

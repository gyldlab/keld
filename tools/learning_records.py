#!/usr/bin/env python3
"""Validate bounded learning records; describe reads frontmatter only.

The prose owner requires a STE-inspired subset. This checker does not claim
formal dictionary certification or infer whether a paragraph contains one fact.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import sys
from typing import BinaryIO

import learning_lifecycle as lifecycle
from learning_lifecycle import LearningError, concise_line, iso_date, require, source_url

LEARNING_ROOT = "docs/agents/learnings"
SOURCE_COMMIT = "244fb285125c2ee9e14c81a1c7b02cd357c7f5b1"
SOURCE_PATH = "docs/agents/learnings.md"
SOURCE_SHA256 = "aa1c76e9079a06e4125d5fa74d5e395c19949266f35c9c68c830e463be5d05fc"
SOURCE_COUNT = 168
DEFAULT_CAP = 2048
FRONTMATTER_CAP = DEFAULT_CAP
AREAS = frozenset("bench bridge build ci cli compat core deps docs git guard host ipc native pack process runtime shell test tooling ts update wv".split())
REQUIRED = frozenset("name description area tags platform recorded status verified source related".split())
OPTIONAL = frozenset("detail supersedes size_exception".split()) | lifecycle.FIELDS
SLUG = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*\Z")
FILENAME = re.compile(r"(\d{4}-\d{2}-\d{2})-([a-z0-9]+(?:-[a-z0-9]+)*)\.md\Z")
OLD_ENTRY = re.compile(r"- \d{4}-\d{2}-\d{2} \[[^]\n]+\] .+\Z")
HISTORICAL_PREFIX = f"https://github.com/gyldlab/keld/blob/{SOURCE_COMMIT}/{SOURCE_PATH}#L"


def unique_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def json_value(raw: str) -> object:
    try:
        return json.loads(raw, object_pairs_hook=unique_object,
                          parse_constant=lambda value: (_ for _ in ()).throw(LearningError(f"invalid JSON value: {value}")))
    except json.JSONDecodeError as error:
        raise LearningError(f"invalid JSON value: {error.msg}") from error


def open_record(path: Path) -> BinaryIO:
    # Unbuffered reads cannot prefetch body bytes past the closing delimiter.
    return path.open("rb", buffering=0)


def is_link(path: Path) -> bool:
    return path.is_symlink() or getattr(path, "is_junction", lambda: False)()


def frontmatter(stream: BinaryIO, limit: int = FRONTMATTER_CAP) -> dict[str, object]:
    metadata: dict[str, object] = {}
    consumed = 0
    first = True
    while True:
        line = stream.readline(limit - consumed + 1)
        consumed += len(line)
        require(consumed <= limit, f"frontmatter exceeds {limit} bytes")
        require(bool(line), "frontmatter has no closing ---")
        if line.rstrip(b"\r\n") == b"---":
            if first:
                first = False
                continue
            return metadata
        require(not first, "record must start with --- frontmatter")
        try:
            text = line.decode("utf-8").rstrip("\r\n")
        except UnicodeDecodeError as error:
            raise LearningError("frontmatter must be UTF-8") from error
        key, separator, raw = text.partition(":")
        require(bool(separator) and bool(re.fullmatch(r"[a-z_]+", key)), "frontmatter requires key: JSON-value lines")
        require(key not in metadata, f"duplicate frontmatter key: {key}")
        metadata[key] = json_value(raw.strip())


def repository_path(root: Path, value: object, *, markdown: bool = True, allow_directory: bool = False) -> Path:
    require(isinstance(value, str) and bool(value), "link must be a nonempty repository-relative path")
    path = PurePosixPath(value)
    require(not path.is_absolute() and "\\" not in value and str(path) == value
            and all(part not in {".", ".."} for part in value.split("/")), f"unsafe link path: {value}")
    require(value.startswith(LEARNING_ROOT + "/"), f"link escapes learning root: {value}")
    require(not markdown or path.suffix == ".md", f"link must name a .md file: {value}")
    current = root
    for part in path.parts:
        current /= part
        require(not is_link(current), f"symlink refused: {value}")
    require(current.is_file() or (allow_directory and current.is_dir()), f"missing link/file: {value}")
    require(current.resolve().is_relative_to((root / LEARNING_ROOT).resolve()), f"link escapes learning root: {value}")
    return current


def record_path(root: Path, value: str) -> Path:
    path = repository_path(root, value)
    relative = PurePosixPath(value).relative_to(LEARNING_ROOT)
    require(len(relative.parts) == 2 and relative.parts[0] in AREAS
            and bool(FILENAME.fullmatch(relative.name)), f"invalid record path/area: {value}")
    return path


def metadata(root: Path, relative: str, caps: dict[str, int] | None = None) -> dict[str, object]:
    path = record_path(root, relative)
    if caps is None:
        caps = inventory_caps(root)
    cap = caps.get(relative, DEFAULT_CAP)
    with open_record(path) as stream:
        data = frontmatter(stream, cap)
    require(REQUIRED <= data.keys(), f"{relative}: missing fields: {', '.join(sorted(REQUIRED - data.keys()))}")
    require(data.keys() <= REQUIRED | OPTIONAL, f"{relative}: unknown fields: {', '.join(sorted(data.keys() - REQUIRED - OPTIONAL))}")
    require(data["name"] == path.stem, f"{relative}: name must equal filename stem")
    require(data["area"] == path.parent.name, f"{relative}: area must equal parent directory")
    concise_line(data["description"], f"{relative}: description")
    tags = data["tags"]
    require(isinstance(tags, list) and bool(tags) and all(isinstance(tag, str) and SLUG.fullmatch(tag) for tag in tags)
            and len(tags) == len(set(tags)), f"{relative}: tags must be unique slug keywords")
    require(data["platform"] in ["all", "macos", "windows", "linux", "unspecified"], f"{relative}: invalid platform")
    require(data["status"] in ["historical", "verified", "stale", "superseded"], f"{relative}: invalid status")
    iso_date(data["recorded"], "recorded")
    require(data["recorded"] == path.name[:10], f"{relative}: recorded date must match filename")
    if data["verified"] != "unknown":
        iso_date(data["verified"], "verified")
    source_url(data["source"], f"{relative}: source")
    for field in ["related", "detail", "supersedes", "superseded_by"]:
        links = data.get(field, [])
        require(isinstance(links, list) and all(isinstance(link, str) for link in links)
                and len(links) == len(set(links)), f"{relative}: {field} must be a unique path list")
        for link in links:
            target = repository_path(root, link)
            require(target.name != "AGENTS.md", f"{relative}: links must not load the instruction owner")
            if field != "detail":
                record_path(root, link)
            if field != "detail":
                require(link != relative, f"{relative}: self {field} link")
    if "size_exception" in data:
        exception = data["size_exception"]
        require(isinstance(exception, dict) and set(exception) == {"tracker", "reason"}
                and all(isinstance(value, str) and value.strip() and value.splitlines() == [value]
                        for value in exception.values()), f"{relative}: size_exception requires tracker and reason")
    require((cap > DEFAULT_CAP) == ("size_exception" in data), f"{relative}: larger cap requires exact inventory allowance and size_exception")
    lifecycle.validate_metadata(data, relative, HISTORICAL_PREFIX)
    return data


def record_files(root: Path) -> list[str]:
    directory = root / LEARNING_ROOT
    require(directory.is_dir() and not is_link(directory), f"missing learning record root: {LEARNING_ROOT}")
    repository_path(root, LEARNING_ROOT + "/AGENTS.md")
    result = []
    for parent, directories, files in os.walk(directory, followlinks=False):
        directories.sort()
        for name in sorted(directories + files):
            require(not is_link(Path(parent) / name), f"symlink refused: {Path(parent) / name}")
        for name in sorted(files):
            path = Path(parent) / name
            relative = path.relative_to(root).as_posix()
            if path.suffix == ".md" and path != directory / "AGENTS.md" and "details" not in path.relative_to(directory).parts:
                record_path(root, relative)
                result.append(relative)
    return sorted(result)


def inventory_caps(root: Path) -> dict[str, int]:
    path = root / ".agents/instruction-budget.tsv"
    caps: dict[str, int] = {}
    if not path.is_file():
        return caps
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if fields[0].startswith(LEARNING_ROOT + "/") or fields[0] == SOURCE_PATH:
            require(len(fields) == 5, f"invalid inventory row: {fields[0]}")
            require(fields[0] not in caps, f"duplicate inventory row: {fields[0]}")
            try:
                cap = int(fields[2])
            except ValueError as error:
                raise LearningError(f"invalid inventory cap: {fields[0]}") from error
            require(cap > 0, f"invalid inventory cap: {fields[0]}")
            caps[fields[0]] = cap
    return caps


def validate_index(root: Path, caps: dict[str, int]) -> None:
    path = root / SOURCE_PATH
    require(path.is_file() and not is_link(path), "missing or symlinked learning index")
    require(path.stat().st_size <= caps.get(SOURCE_PATH, DEFAULT_CAP), "learning index exceeds its registered cap")
    heading = False
    links = 0
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        if re.fullmatch(r"#{1,6} [^\r\n]+", line):
            heading = True
            continue
        match = re.fullmatch(r"[-*+] \[[^\]\r\n]+\]\(([^)\r\n]+)\)", line)
        require(heading and bool(match), f"learning index line {number} must be a heading or link-list item")
        target = match[1]
        require(not any(char.isspace() or ord(char) < 32 or ord(char) == 127 for char in target)
                and not any(char in target for char in "%?#"), f"unsafe learning index target: {target!r}")
        reference = target[:-1] if target.endswith("/") else target
        relative = f"{PurePosixPath(SOURCE_PATH).parent}/{reference}"
        resolved = repository_path(root, relative, markdown=False, allow_directory=True)
        require(not target.endswith("/") or resolved.is_dir(), f"learning index directory target is not a directory: {target}")
        links += 1
    require(heading and links > 0, "learning index requires a heading and local links")


def historical_line(body: str, relative: str) -> str:
    lines = body.splitlines()
    positions = [index for index, line in enumerate(lines) if line == "## Historical source"]
    require(len(positions) == 1, f"{relative}: exactly one Historical source section required")
    section = []
    for line in lines[positions[0] + 1:]:
        if line.startswith("## "):
            break
        if line.strip():
            section.append(line)
    require(len(section) == 1 and section[0].startswith("> "), f"{relative}: Historical source must contain one quoted legacy line")
    original = section[0][2:]
    require(bool(OLD_ENTRY.fullmatch(original)), f"{relative}: invalid quoted legacy entry")
    return original


def migration(root: Path, records: dict[str, dict[str, object]]) -> None:
    path = root / LEARNING_ROOT / "migration.json"
    require(path.is_file() and not is_link(path), "missing migration.json")
    ledger = json_value(path.read_text(encoding="utf-8"))
    require(isinstance(ledger, dict) and set(ledger) == {"schema", "source_commit", "source_path", "source_sha256", "segments"}, "invalid migration keys")
    require(ledger["schema"] == "keld.learning-migration/v1" and ledger["source_commit"] == SOURCE_COMMIT
            and ledger["source_path"] == SOURCE_PATH and ledger["source_sha256"] == SOURCE_SHA256, "migration must bind the fixed original source oracle")
    require(isinstance(ledger["segments"], list), "migration segments must be a list")
    seen: set[str] = set()
    reconstructed: list[str] = []
    line_number = 1
    for segment in ledger["segments"]:
        require(isinstance(segment, dict) and (set(segment) == {"literal"} or set(segment) == {"record"}), "invalid migration segment")
        if "literal" in segment:
            text = segment["literal"]
            require(isinstance(text, str), "migration literal must be a string")
            require(not re.search(r"(?m)^- \d{4}-\d{2}-\d{2} \[", text), "historical entries must be mapped, not embedded in literals")
        else:
            relative = segment["record"]
            require(isinstance(relative, str) and relative in records, f"missing migration record: {relative}")
            require(relative not in seen, f"duplicate migration record: {relative}")
            seen.add(relative)
            require(records[relative]["source"] == HISTORICAL_PREFIX + str(line_number), f"{relative}: historical source line does not match migration position")
            text = historical_line((root / relative).read_text(encoding="utf-8"), relative)
        reconstructed.append(text)
        line_number += text.count("\n")
    historical = {relative for relative, record in records.items() if record["source"].startswith(HISTORICAL_PREFIX)}
    require(seen == historical, "migration has missing/extra historical records")
    require(len(seen) == SOURCE_COUNT, f"migration requires all {SOURCE_COUNT} original entries")
    actual = hashlib.sha256("".join(reconstructed).encode("utf-8")).hexdigest()
    require(actual == SOURCE_SHA256, f"migration reconstructed source hash mismatch: {actual}")


def check(root: Path) -> int:
    caps = inventory_caps(root)
    records = {relative: metadata(root, relative, caps) for relative in record_files(root)}
    validate_index(root, caps)
    names = [record["name"] for record in records.values()]
    require(len(names) == len(set(names)), "record names must be unique across areas")
    for relative, record in records.items():
        cap = caps.get(relative, DEFAULT_CAP)
        require((root / relative).stat().st_size <= cap, f"{relative}: record exceeds {cap} bytes")
    lifecycle.validate_graph(records, HISTORICAL_PREFIX)
    migration(root, records)
    return len(records)


def describe(root: Path, paths: list[str]) -> list[dict[str, object]]:
    require(bool(paths), "describe requires explicit record paths")
    caps = inventory_caps(root)
    return [{"filename": path, **{key: data[key] for key in ["name", "description", "tags", "platform", "source"]},
             **lifecycle.describe_fields(data)}
            for path in sorted(set(paths)) for data in [metadata(root, path, caps)]]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["check", "describe"])
    parser.add_argument("paths", nargs="*")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args(argv)
    try:
        if args.command == "describe":
            print(json.dumps(describe(args.root, args.paths), indent=2, ensure_ascii=False))
        else:
            require(not args.paths, "check does not accept record paths")
            print(f"learning-records: {check(args.root)} records passed")
        return 0
    except (LearningError, OSError, UnicodeError) as error:
        print(f"learning-records: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

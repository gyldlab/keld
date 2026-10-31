#!/usr/bin/env python3
"""Reproducible source-size census of pinned tracked Git objects, not working-tree files."""

import argparse
import csv
import json
import os
import re
import subprocess
from collections import Counter
from pathlib import Path

# Proposal simulations only; these are NOT KELD policy.
CANDIDATE = {
    "A": (400, 16384, 600, 32768),
    "B": (800, 40960, 1200, 65536),
    "C": (400, 16384, 1000, 49152),
    "D": (1000, 49152, 1500, 81920),
    "E": (None, 65536, None, 131072),
}
ROOMIER = {
    "A": (400, 16384, 800, 49152),
    "B": (800, 40960, 1600, 81920),
    "C": (400, 16384, 1400, 65536),
    "D": (1000, 49152, 2000, 114688),
    "E": (None, 65536, None, 196608),
}
CODE = {".rs", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".py",
        ".go", ".swift", ".c", ".h", ".cc", ".cpp", ".hpp", ".sql",
        ".css", ".scss", ".html", ".vue", ".svelte"}
ASSETS = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".svg",
          ".pdf", ".woff", ".woff2", ".ttf", ".otf", ".zip", ".gz",
          ".wasm", ".mp4", ".mov", ".mp3", ".wav", ".snap", ".bin"}
LOCKS = {"cargo.lock", "bun.lock", "package-lock.json", "pnpm-lock.yaml", "yarn.lock"}
GENERATED = {"llms.txt", "llms-full.txt", "docs/engineering/product-status.md"}
INLINE = re.compile(rb"(?m)^[ \t]*#[ \t]*\[[ \t]*cfg[ \t]*\([ \t]*test[ \t]*\)[ \t]*\]")
TEST_MODULE = re.compile(rb"(?m)^[ \t]*mod[ \t]+\w*tests?\w*[ \t]*\{")
FIELDS = ("path", "blob_sha", "mode", "category", "classification_reason",
          "exclusion_reason", "bytes", "physical_lines", "encoding",
          "inline_test_flag", "inline_test_tail_lines", "inline_test_tail_ratio",
          "first_test_module_line", "module_tail_ratio")


GIT_LOCATION_VARS = (
    "GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR", "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_PREFIX",
)


def git(repo, *args, data=None):
    # Hooks may inherit the caller's Git location. -C does not override GIT_DIR.
    env = {key: value for key, value in os.environ.items() if key not in GIT_LOCATION_VARS}
    p = subprocess.run(["git", "-C", str(repo), *args], input=data,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False, env=env)
    if p.returncode:
        raise RuntimeError("git command failed: " + p.stderr.decode("utf-8", "replace"))
    return p.stdout


def parse_tree(raw):
    entries = []
    for record in raw.split(b"\0"):
        if not record:
            continue
        meta, sep, pathname = record.partition(b"\t")
        pieces = meta.split(b" ")
        if not sep or len(pieces) != 3:
            raise ValueError("malformed tracked tree record")
        mode, kind, sha = (x.decode("ascii") for x in pieces)
        entries.append(dict(path=pathname.decode("utf-8", "surrogateescape"),
                            mode=mode, kind=kind, blob_sha=sha))
    if len({e["path"] for e in entries}) != len(entries):
        raise ValueError("duplicate Git tree path")
    return entries


def change_counts(history):
    # -z is required: Git may quote non-ASCII paths or embed LF in filenames.
    return Counter(p.decode("utf-8", "surrogateescape")
                   for p in history.split(b"\0") if p)


def physical_lines(data):
    if not data:
        return 0
    endings = data.count(b"\n") + data.count(b"\r") - data.count(b"\r\n")
    return endings + int(not data.endswith((b"\n", b"\r")))


def read_blobs(repo, entries):
    hashes = list(dict.fromkeys(e["blob_sha"] for e in entries if e["kind"] == "blob"))
    if not hashes:
        return {}
    output = git(repo, "cat-file", "--batch", data=("\n".join(hashes) + "\n").encode())
    found, pos = {}, 0
    for sha in hashes:
        end = output.find(b"\n", pos)
        if end < 0:
            raise ValueError("missing Git cat-file header")
        parts = output[pos:end].split(b" ")
        if len(parts) != 3 or parts[0].decode() != sha or parts[1] != b"blob":
            raise ValueError("mismatched blob identity/type")
        n = int(parts[2])
        start = end + 1
        stop = start + n
        if stop >= len(output) or output[stop:stop + 1] != b"\n":
            raise ValueError("truncated tracked blob")
        found[sha] = output[start:stop]
        pos = stop + 1
    if pos != len(output):
        raise ValueError("extra bytes after blob batch")
    return found


def classify(path, mode, data):
    p = path.lower().replace("\\", "/")
    leaf = p.rsplit("/", 1)[-1]
    parts = p.split("/")
    ext = Path(leaf).suffix
    if mode == "160000":
        return "F", "tracked gitlink", "submodule reference, not a source file", "gitlink"
    if mode == "120000":
        return "F", "tracked symlink", "link target, not source bytes", "symlink"
    if data is None:
        raise ValueError("tracked regular file has no blob")
    if "corpus" in parts or "fuzz/corpus/" in p:
        return "F", "fuzz input corpus path", "corpus is a generated/fixed input", "asset"
    if any(x in parts for x in ("fixtures", "fixture", "templates", "template", "snapshots", "vendor")):
        return "F", "fixture or template directory", "fixture/template source of test input", "fixture"
    if leaf in LOCKS:
        return "F", "dependency lockfile", "generated dependency resolver output", "lock"
    if p in GENERATED:
        return "F", "documented derived index/view", "generated; see root AGENTS.md", "generated"
    # Exactly the three artifacts authored by this census command. Do not
    # exempt arbitrary audit documentation or user-written adjacent files.
    if re.fullmatch(
        r"docs/audits/source-census-\d{4}-\d{2}-\d{2}/(?:readme\.md|manifest\.csv|summary\.json)",
        p,
    ):
        return ("F", "pinned census generator output",
                "derived by tools/source_census.py", "generated")
    if ext in ASSETS:
        return "F", "static/media/compiled extension", "asset, not authored source text", "asset"
    if b"\0" in data:
        return "F", "NUL byte", "binary tracked blob", "binary"
    try:
        data.decode("utf-8")
    except UnicodeDecodeError:
        return "F", "invalid UTF-8", "non-text for UTF-8 source budget", "non-utf8"
    # Owner-local Rust test modules also use directories such as named_pipe_tests/.
    # Match directory components only, so similarly named production files stay B.
    if ("/tests/" in "/" + p or "/test/" in "/" + p
            or "/fuzz_targets/" in "/" + p
            or any(part.endswith("_tests") for part in parts[:-1])
            or leaf.startswith("test_")
            or leaf.endswith(("_test.py", ".test.rs", ".spec.rs", "tests.rs",
                              ".test.ts", ".spec.ts", ".test.tsx", ".spec.tsx",
                              ".test.js", ".spec.js", "_test.sh"))):
        return "C", "test/scenario/support by path or suffix", "", "utf-8"
    if re.fullmatch(r"crates/[^/]+/src/(lib|main)\.rs", p) or re.fullmatch(
            r"packages/(?:@[^/]+/)?[^/]+/src/index\.(ts|tsx|js)", p):
        return "A", "crate/package composition root", "", "utf-8"
    if (p.startswith(("tools/", ".github/workflows/", ".github/actions/", ".githooks/"))
            or leaf in ("justfile", "makefile", "dockerfile")
            or ext in (".ps1", ".sh", ".bat", ".cmd", ".toml", ".json", ".jsonc", ".yaml", ".yml", ".tsv")
            or leaf in (".editorconfig", ".gitattributes", ".gitignore", ".gitleaksignore", "codeowners")
            or p.startswith(".config/")):
        return "D", "authored tooling, CI or configuration", "", "utf-8"
    if ext in (".md", ".mdx", ".rst", ".adoc", ".txt") or leaf.startswith("license"):
        return "E", "authored docs/spec/text", "", "utf-8"
    if ext in CODE or leaf.endswith((".mts", ".cts")):
        return "B", "authored production/demo source", "", "utf-8"
    return "F", "not a supported source/document extension", "other tracked artifact", "other"


def collect(entries, blobs):
    rows = []
    for e in entries:
        data = blobs.get(e["blob_sha"]) if e["kind"] == "blob" else None
        cat, why, exclusion, enc = classify(e["path"], e["mode"], data)
        loc = physical_lines(data) if data is not None and enc == "utf-8" else None
        marked, tail, fraction = False, None, None
        if cat in ("A", "B") and e["path"].endswith(".rs") and data is not None:
            m = INLINE.search(data)
            marked = m is not None
            if marked and loc:
                tail = loc - physical_lines(data[:m.start()])
                fraction = round(tail / loc, 4)
        first_mod, mod_fraction = None, None
        if cat in ("A", "B") and e["path"].endswith(".rs") and data is not None and loc:
            start = TEST_MODULE.search(data)
            if start:
                first_mod = 1 + physical_lines(data[:start.start()])
                mod_fraction = round((loc - first_mod + 1) / loc, 4)
        rows.append(dict(path=e["path"], blob_sha=e["blob_sha"], mode=e["mode"],
                         category=cat, classification_reason=why, exclusion_reason=exclusion,
                         bytes=len(data) if data is not None else None,
                         physical_lines=loc, encoding=enc, inline_test_flag=marked,
                         inline_test_tail_lines=tail, inline_test_tail_ratio=fraction,
                         first_test_module_line=first_mod, module_tail_ratio=mod_fraction))
    return rows


def percentile(values, p):
    if not values:
        return 0.0
    xs = sorted(values)
    index = (len(xs) - 1) * p / 100
    a, b = int(index), min(int(index) + 1, len(xs) - 1)
    return round(xs[a] + (xs[b] - xs[a]) * (index - a), 1)


def policy_impact(rows, proposal, name):
    output = []
    for category, (review_loc, review_b, hard_loc, hard_b) in proposal.items():
        selected = [r for r in rows if r["category"] == category]
        entry = lambda r: category == "C" and (
            r["path"].endswith("/tests/mod.rs") or r["path"].endswith("/tests.rs"))
        revl = sum((r["physical_lines"] or 0) > (100 if entry(r) else review_loc)
                   for r in selected) if review_loc is not None else 0
        revb = sum((r["bytes"] or 0) > (4096 if entry(r) else review_b) for r in selected)
        hl = sum(hard_loc is not None and (r["physical_lines"] or 0) > hard_loc for r in selected)
        hb = sum(hard_b is not None and (r["bytes"] or 0) > hard_b for r in selected)
        union = sum((hard_loc is not None and (r["physical_lines"] or 0) > hard_loc)
                    or (hard_b is not None and (r["bytes"] or 0) > hard_b) for r in selected)
        output.append(dict(proposal=name, category=category, files=len(selected),
                           review_over_lines=revl, review_over_bytes=revb,
                           hard_over_lines=hl, hard_over_bytes=hb,
                           union_existing_grandfathered=union))
    return output


def independently_validate(rows, blobs):
    eligible = [r for r in rows if r["physical_lines"] is not None]
    biggest = sorted(eligible, key=lambda r: (-r["physical_lines"], r["path"]))[:15]
    picks = {r["path"] for r in biggest}
    picks.update(r["path"] for r in eligible[::max(1, len(eligible) // 30)])
    indexed = {r["path"]: r for r in eligible}
    for path in sorted(picks):
        r = indexed[path]
        independent = len(blobs[r["blob_sha"]].splitlines(keepends=True))
        if independent != r["physical_lines"]:
            raise AssertionError("physical LOC disagreement: " + path)
    return dict(method="bytes.splitlines(keepends=True) vs explicit newline arithmetic",
                samples=len(picks), mismatches=0)


def summarize(rows, pinned, validation, date, changes):
    dist = {}
    for c in "ABCDEF":
        members = [r for r in rows if r["category"] == c]
        values = [r["physical_lines"] for r in members if r["physical_lines"] is not None]
        dist[c] = dict(files=len(members), counted_text_files=len(values),
                       total_lines=sum(values), total_blob_bytes=sum(r["bytes"] or 0 for r in members),
                       **{f"p{n}_lines": percentile(values, n) for n in (50, 75, 90, 95, 99)})
    top = sorted([r for r in rows if r["category"] in "ABCD" and r["physical_lines"] is not None],
                 key=lambda r: (-r["physical_lines"], r["path"]))[:25]
    inline = sorted([r for r in rows if r["inline_test_flag"]],
                    key=lambda r: (-(r["inline_test_tail_lines"] or 0), r["path"]))[:25]
    recent = sorted([r for r in rows if r["category"] in "ABCD" and r["physical_lines"] is not None],
                    key=lambda r: (-changes.get(r["path"], 0), -r["physical_lines"], r["path"]))[:20]
    return dict(schema="keld.source-census/v1", audit_date=date, pinned_commit=pinned,
                tracked_manifest_entries=len(rows), accounted_rows=len(rows),
                category_counts=dict(sorted(Counter(r["category"] for r in rows).items())),
                distributions=dist,
                proposed_impacts=policy_impact(rows, CANDIDATE, "candidate") +
                                 policy_impact(rows, ROOMIER, "roomier"),
                top_authored_code=[{k: r[k] for k in
                                   ("path", "category", "physical_lines", "bytes",
                                    "inline_test_flag", "inline_test_tail_ratio",
                                    "first_test_module_line", "module_tail_ratio")} for r in top],
                recent_change_ranking=[
                    dict(path=r["path"], category=r["category"], physical_lines=r["physical_lines"],
                         commit_touches=changes.get(r["path"], 0)) for r in recent],
                history_since="2026-09-15",                top_inline_test_proxies=[{k: r[k] for k in
                                         ("path", "inline_test_tail_lines",
                                          "inline_test_tail_ratio")} for r in inline],
                independent_validation=validation,
                methodology={
                    "population": "every tracked path in pinned git ls-tree -r -z, including F exclusions",
                    "bytes": "raw Git blob bytes, not working tree disk encoding",
                    "physical_lines": "CRLF one break; CR/LF each one break; nonempty final unterminated line counted",
                    "percentiles": "linear interpolation, index (n-1)*p/100",
                    "inline_tests": "low-confidence tail proxy: physical lines from first anchored #[cfg(test)] to EOF; not AST or exact test code",
                    "classification": "deterministic path/mode/UTF-8 rules in tools/source_census.py",
                    "source_of_truth": "pinned Git blob IDs; excludes untracked user files",
                },
                limitations=["File size is not architectural failure or proof of coupling.",
                             "Inline-test tail percentages are NOT exact test share.",
                             "Agent navigation/read-set and semantic ownership require manual evidence.",
                             "Test discovery or native behavior not exercised by static census.",
                             "Proposed hard-stop counts are grandfathering estimates; no gates approved."])


def markdown(s):
    out = [f"# KELD source census — {s['audit_date']}", "",
           "Snapshot of pinned Git objects, not the live working tree. Physical lines include blank lines.",
           f"Audited commit: {s['pinned_commit']}.",
           f"Tracked manifest reconciliation: **{s['accounted_rows']}/{s['tracked_manifest_entries']}**.",
           "A: crate/package entrypoints; B: authored implementation; C: tests;",
           "D: tooling/CI/config; E: authored documentation; F: explicit exclusions.", "",
           "## Population and distributions", "",
           "| Class | Files | Text files | Physical lines | Blob bytes | p50 | p75 | p90 | p95 | p99 |",
           "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|"]
    for c, d in s["distributions"].items():
        out.append(f"| {c} | {d['files']} | {d['counted_text_files']} | {d['total_lines']} | "
                   f"{d['total_blob_bytes']} | " + " | ".join(
                       str(d[f'p{p}_lines']) for p in (50, 75, 90, 95, 99)) + " |")
    out += ["", "## Hypothetical category-specific policy impact (NOT adopted)", "",
            "Hard counts include existing files that would require ratchet/grandfathering; "
            "LOC and bytes counted separately, union deduplicated. Strict greater-than.", "",
            "| Proposal | Class | Files | Review LOC | Review bytes | Hard LOC | Hard bytes | Legacy union |",
            "|---|---|---:|---:|---:|---:|---:|---:|"]
    for i in s["proposed_impacts"]:
        out.append("| {proposal} | {category} | {files} | {review_over_lines} | "
                   "{review_over_bytes} | {hard_over_lines} | {hard_over_bytes} | "
                   "{union_existing_grandfathered} |".format(**i))
    out += ["", "## Largest code modules (not automatic refactor candidates)", "",
            "| Path | Class | LOC | Bytes | Test cfg marker | Cfg-tail proxy | Test-module-tail proxy |",
            "|---|---|---:|---:|---|---:|---:|"]
    for r in s["top_authored_code"][:20]:
        out.append(f"| {r['path']} | {r['category']} | {r['physical_lines']} | {r['bytes']} | "
                   f"{'yes' if r['inline_test_flag'] else 'no'} | "
                   f"{r['inline_test_tail_ratio'] if r['inline_test_tail_ratio'] is not None else 'n/a'} | "
                   f"{r['module_tail_ratio'] if r['module_tail_ratio'] is not None else 'n/a'} |")
    out += ["", "## Recently changed code (change-frequency proxy, not ownership proof)", "",
            "Count of commits touching each *current path* since 2026-09-15 on the pinned history. "
            "Renames are not followed. A frequently edited file is not automatically architecturally bad.",
            "| Path | Class | LOC | Commits touching path |",
            "|---|---|---:|---:|"]
    for r in s["recent_change_ranking"][:15]:
        out.append(f"| {r['path']} | {r['category']} | {r['physical_lines']} | "
                   f"{r['commit_touches']} |")
    out += ["", "## Verification and scope limits", "",
            f"- Independently spot-checked {s['independent_validation']['samples']} blob line counts; "
            f"mismatches: {s['independent_validation']['mismatches']}.",
            "- Source of truth: git ls-tree -r -z and git cat-file --batch; every path has a "
            "recorded category and F exclusions explain why a file is out of scope.",
            "- Inline-test tail is a LOW-CONFIDENCE proxy: everything after the first "
            "line-anchored Rust cfg(test) marker is counted; it is NOT a semantic test code percentage.",
            "- Another LOW-CONFIDENCE measure starts at the first source line of a tests-like module; "
            "neither proxy is an AST-aware count, and production items may follow test-only fragments.",
            "- Category C test entry modules use existing review triggers (>100 LOC or 4 KiB), "
            "other test modules >400 LOC or 16 KiB; review is not rejection.",
            "- Do not impose file limits until their design implications and exceptions are independently reviewed.",
            "- Candidate priority for next stage: inspect ownership and test discovery of test-heavy modules, "
            "compare agents' before/after read-set, check active PRs, then choose cohesive families.",
            "- Commit-touch counts are a bounded change-frequency proxy, not evidence of "
            "coupling or ownership; the census does not measure live updater behavior, "
            "compiled test identities or platform compatibility.",
            "",
            "## Reproduce", "",
            "From a checkout of KELD:",
            "",
            "    python tools/test_source_census.py",
            "    python tools/source_census.py --repo . --commit " + s["pinned_commit"] +
            " --output docs/audits/source-census-" + s["audit_date"] + " --date " + s["audit_date"],
            "",
            "Machine-readable evidence: manifest.csv (one row per tracked path) and summary.json.",
            "Counts include source comments/blank lines, not executable statements; file classification is "
            "deterministic, but architectural judgments remain human-reviewed.", ""]
    return "\n".join(out)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--commit", default="HEAD")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--date", default="2026-10-15")
    args = parser.parse_args()
    repo = args.repo.resolve()
    pinned = git(repo, "rev-parse", args.commit + "^{commit}").decode().strip()
    entries = parse_tree(git(repo, "ls-tree", "-r", "-z", "--full-tree", pinned))
    blobs = read_blobs(repo, entries)
    rows = collect(entries, blobs)
    if len(rows) != len(entries):
        raise AssertionError("not all tracked entries were accounted for")
    validation = independently_validate(rows, blobs)
    # Commit-path touches in a pinned history interval; renames are NOT followed.
    history = git(repo, "log", "-z", "--format=", "--name-only", "--since=2026-09-15", pinned)
    changes = change_counts(history)
    summary = summarize(rows, pinned, validation, args.date, changes)
    dest = args.output if args.output.is_absolute() else repo / args.output
    dest = dest.resolve()
    if not dest.is_relative_to(repo):
        raise ValueError("output must be beneath repository")
    dest.mkdir(parents=True, exist_ok=True)
    with (dest / "manifest.csv").open("w", encoding="utf-8", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=FIELDS, lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
    (dest / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n",
                                       encoding="utf-8")
    (dest / "README.md").write_text(markdown(summary), encoding="utf-8")
    print(json.dumps(dict(commit=pinned, tracked=len(rows),
                          categories=summary["category_counts"],
                          independent_checks=validation["samples"], output=str(dest))))


if __name__ == "__main__":
    main()

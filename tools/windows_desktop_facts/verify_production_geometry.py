#!/usr/bin/env python3
"""Source-bound Rust geometry regression. Retains all raw controls, never edits product."""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

EXPECTED_FRAGMENT_SHA = "6801ee1096cfb537b9ba30d47e738f8dac4e5ef19bfbb36e15589bbc31012dd6"
EXPECTED_FIXTURE_SHA = "4bed36bc789db0e50bf2e74449609aa09ed86ceaaba9be564d4085f1f4b27d5a"
EXPECTED_FULL_SOURCE_SHA = "aee0b6aa5aaba105eab061fd873513f132d25629d5cf11aba3c58643cfa1e7cd"
START = b"    // BEGIN VERBATIM WINDOWS LOGICAL\n"
END = b"// END VERBATIM WINDOWS LOGICAL"

def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

def source_fragment(data: bytes) -> bytes:
    assert data.count(START) == 1 and data.count(END) == 1, "source framing changed"
    return data.split(START, 1)[1].split(END, 1)[0]

def run(command: list[str], log: Path) -> subprocess.CompletedProcess:
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    log.write_bytes(result.stdout)
    return result

def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence-dir", required=True, type=Path)
    parser.add_argument("--windows-source", required=True, type=Path,
                        help="Immutable byte copy of live Windows display.rs; bind full SHA")
    parser.add_argument("--fixture", type=Path,
                        default=Path(__file__).with_name("production_geometry_fragment.rs"))
    args = parser.parse_args()
    evidence = args.evidence_dir.resolve()
    evidence.parent.mkdir(parents=True, exist_ok=True)
    # Exclusive admission: never reuse, overwrite or merge two campaigns' receipts.
    evidence.mkdir(exist_ok=False)
    source = args.fixture
    content = source.read_bytes()
    fragment = source_fragment(content)
    if sha(fragment) != EXPECTED_FRAGMENT_SHA:
        raise SystemExit("REFUSED: source-bound geometry fragment does not match verified Windows production")
    if sha(content) != EXPECTED_FIXTURE_SHA:
        raise SystemExit("REFUSED: pinned geometry test oracle/scaffold source SHA changed")
    actual = args.windows_source.read_bytes()
    if sha(actual) != EXPECTED_FULL_SOURCE_SHA:
        raise SystemExit("REFUSED: full Windows display.rs source SHA changed")
    marker = b"pub(super) fn logical("
    # This exact pinned source ends with logical(); future source growth must be
    # re-reviewed and rebound rather than silently accepted by a substring match.
    if actual.count(marker) != 1 or not actual.endswith(fragment):
        raise SystemExit("REFUSED: live Windows function differs from compiled fragment")

    variants = [
        ("healthy", content, None, 4),
        ("ceil_to_floor", content.replace(
            b"let n = if upper { q.ceil() } else { q.floor() };",
            b"let n = if upper { q.floor() } else { q.floor() };"),
            "tests::fractional_edges_enclose_the_complete_physical_rectangle", 1),
        ("remove_positive_width", content.replace(
            b"        .filter(|w| *w > 0)\n", b""),
            "tests::zero_and_reversed_dimensions_refuse_instead_of_publishing_a_display", 1),
        ("restored", content, None, 4),
    ]
    manifest = {"source_sha256": sha(content), "fragment_sha256": sha(fragment),
                 "windows_source_sha256_verified": sha(actual),
                "variants": []}
    for name, body, selection, count in variants:
        if name not in ("healthy", "restored") and body == content:
            raise SystemExit(f"REFUSED: missing mutation {name}")
        path = evidence / f"{name}.rs"
        path.write_bytes(body)
        binary = evidence / f"{name}.bin"
        compilation = run(["rustc", "--edition=2024", "-D", "warnings", "--test",
                           str(path), "-o", str(binary)], evidence / f"{name}.build.log")
        if compilation.returncode != 0:
            raise SystemExit(f"REFUSED: {name} compilation failed, not a semantic assertion: {compilation.returncode}")
        cmd = [str(binary)]
        if selection:
            cmd += [selection, "--exact", "--nocapture"]
        result = run(cmd, evidence / f"{name}.test.log")
        log = (evidence / f"{name}.test.log").read_text()
        if name in ("healthy", "restored"):
            qualified = (result.returncode == 0 and
                         f"{count} passed; 0 failed" in log)
        else:
            qualified = (result.returncode != 0 and
                         "0 passed; 1 failed" in log and
                         "assertion `left == right` failed" in log and
                         selection in log)
        manifest["variants"].append({
            "name": name, "source_sha256": sha(body), "compile_exit": compilation.returncode,
            "test_exit": result.returncode, "qualified": qualified,
            "test_log_sha256": sha((evidence/f"{name}.test.log").read_bytes())
        })
        if not qualified:
            (evidence / "manifest.json").write_text(json.dumps(manifest, indent=2)+"\n")
            raise SystemExit(f"REFUSED: {name} did not produce its intended test result")
    manifest["overall"] = "SCOPED_PASS"
    (evidence / "manifest.json").write_text(json.dumps(manifest, indent=2)+"\n")
    print(json.dumps(manifest, indent=2))

if __name__ == "__main__":
    main()

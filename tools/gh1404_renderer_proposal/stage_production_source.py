#!/usr/bin/env python3
"""Create a source-verified overlay for a NEW draw.io production source build.

The overlay contains real replacement mxgraph JavaScript at its original
application paths, plus a unified patch and SHA-256 input/output manifest.
It never writes to the input tree or an installed/signed application.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parent
SOURCES = (
    ("src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js", "make_patch.py"),
    ("src/main/webapp/mxgraph/src/shape/mxText.js", "make_bbox_patch.py"),
)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True,
                        help="unmodified, pinned jgraph/drawio checkout")
    parser.add_argument("--output-root", type=Path, required=True,
                        help="new isolated directory for two replacement JS files and patch")
    args = parser.parse_args()

    source_root = args.source_root.resolve(strict=True)
    output_root = args.output_root.resolve()
    if output_root == source_root or source_root in output_root.parents:
        parser.error("output must be outside the original source tree")
    if output_root.exists():
        parser.error("output already exists; use a new isolated directory")
    if any(path.suffix == ".app" for path in (source_root, *source_root.parents)):
        parser.error("refusing installed application input; use a source checkout")

    prepared = []
    full_patch = bytearray()
    for relative_path, generator in SOURCES:
        original_path = source_root / relative_path
        if original_path.is_symlink() or not original_path.is_file():
            parser.error(f"missing real source file: {relative_path}")
        if not original_path.resolve().is_relative_to(source_root):
            parser.error(f"source file escapes the checkout: {relative_path}")
        original = original_path.read_bytes()
        generated = []
        for as_patch in (False, True):
            argv = [sys.executable, str(ROOT / generator)]
            if as_patch:
                argv.append("--patch")
            result = subprocess.run(argv, input=original, capture_output=True, check=False)
            if result.returncode:
                parser.error(f"{generator}: {result.stderr.decode('utf-8', 'replace')}")
            generated.append(result.stdout)
        candidate, diff = generated
        if not diff or candidate == original:
            parser.error(f"patch did not change {relative_path}")
        prepared.append((relative_path, original, candidate))
        full_patch.extend(diff)

    # Mutate only after every input/anchor was independently verified.
    output_root.mkdir(parents=True, exist_ok=False)
    manifest = {
        "upstream": "jgraph/drawio@24b76c2cbd55d88e354042e8d329a2e4708972bc",
        "scope": "replacement application source; full app/provider/build qualification pending",
        "files": [],
    }
    for relative_path, original, candidate in prepared:
        destination = output_root / relative_path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(candidate)
        manifest["files"].append({
            "path": relative_path,
            "original_sha256": sha256(original),
            "candidate_sha256": sha256(candidate),
        })

    patch = bytes(full_patch)
    (output_root / "gh1404-g1-production.patch").write_bytes(patch)
    manifest["patch_sha256"] = sha256(patch)
    (output_root / "source-manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()

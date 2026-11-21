"""Read-only audit of immutable W1 feasibility fixtures; no product authority."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from fixture_census import census, read_fixture


def verify_sources(root: Path, manifest: dict) -> list[dict]:
    """Check recorded SHA256, Git blob identity, and length before any test uses bytes."""
    source_root = root / manifest["source"]["root"]
    checked = []
    for item in manifest["files"]:
        data = read_fixture(source_root, item["path"])
        digest = hashlib.sha256(data).hexdigest()
        git_blob = hashlib.sha1(
            b"blob " + str(len(data)).encode("ascii") + b"\0" + data
        ).hexdigest()
        if (len(data), digest, git_blob) != (
            item["bytes"], item["sha256"], item["git_blob"]
        ):
            raise ValueError(f"source identity mismatch: {item['path']}")
        checked.append({"path": item["path"], "bytes": len(data), "sha256": digest})
    return checked


def verify_review_anchors(root: Path, manifest: dict) -> None:
    """Bind hand-reviewed registration observations to the exact recorded main file."""
    main = read_fixture(
        root / manifest["source"]["root"], "src/main/electron.js"
    ).decode("utf-8").splitlines()
    for item in manifest["hand_reviewed_main_registrations"]:
        for line in item["lines"]:
            text = main[line - 1]
            double = f'ipcMain.{item["kind"]}("{item["channel"]}"'
            single = f"ipcMain.{item['kind']}('{item['channel']}'"
            if double not in text and single not in text:
                raise ValueError(f"review anchor mismatch: {item['channel']}:{line}")
    site = manifest["main_window_site"]
    if "new BrowserWindow(options)" not in main[site["line"] - 1]:
        raise ValueError("unresolved original main-window anchor moved")


def report(root: Path) -> dict:
    manifest = json.loads((root / "source_manifest.json").read_text(encoding="utf-8"))
    checked = verify_sources(root, manifest)
    verify_review_anchors(root, manifest)
    fixture = json.loads((root / "census_fixture.json").read_text(encoding="utf-8"))
    digest = census(root / "census_project", fixture["inputs"], fixture["declared_preloads"])
    return {
        "scope": manifest["evidence_scope"],
        "source_commit": manifest["source"]["commit"],
        "verified_files": checked,
        "reviewed_site_count": len(manifest["hand_reviewed_preload_sites"]),
        "reviewed_sites_are_classifier_output": False,
        "candidate_literals_only": manifest["candidate_literals_only"],
        "enabled_grants": manifest["enabled_grants"],
        "original_main_window": manifest["main_window_site"],
        "production_main_digest": manifest["main_code_digest"],
        "trust_root": manifest["trust_root"],
        "experimental_census": digest,
        "product_AC39_AC44": "NOT RUN: keld migrate/build and authoritative census unavailable",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent)
    args = parser.parse_args()
    try:
        result = report(args.root)
    except (OSError, ValueError, KeyError) as error:
        print(json.dumps({"status": "FAIL", "error": str(error)}, sort_keys=True))
        return 1
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

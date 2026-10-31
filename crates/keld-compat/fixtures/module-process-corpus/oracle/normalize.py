#!/usr/bin/env python3
"""Normalize the pinned Electron oracle transcript without publishing local paths."""

import argparse
import hashlib
import json
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("raw", type=Path, help="raw stdout from the Electron fixture")
    parser.add_argument("output", type=Path, help="normalized JSON destination")
    parser.add_argument("--expected-raw-sha256", help="optional SHA-256 of the exact raw bytes")
    args = parser.parse_args()
    raw = args.raw.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    if args.expected_raw_sha256 is not None and digest != args.expected_raw_sha256:
        raise SystemExit("raw stdout SHA-256 does not match the expected receipt")
    data = json.loads(raw)
    if set(data) != {"electron", "earlyType", "userData", "argv", "userArgs", "facts"}:
        raise SystemExit("unexpected oracle output fields; refusing unsafe normalization")
    if not isinstance(data["userData"], str):
        raise SystemExit("userData must be a string")
    argv = data["argv"]
    if not isinstance(argv, list) or len(argv) != 4 or not all(isinstance(v, str) for v in argv):
        raise SystemExit("oracle argv must contain executable, entry, and two arguments")
    if data["electron"] != "44.4.5" or data["earlyType"] != "browser":
        raise SystemExit("unexpected Electron version or early process type")
    if argv[2:] != ["gh569-alpha", "gh569-beta"] or data["userArgs"] != argv[2:]:
        raise SystemExit("unexpected supplied user arguments")
    facts = data["facts"]
    if set(facts) != {"defaultApp", "mas", "windowsStore", "sandboxed", "contextIsolated", "resourcesPath"}:
        raise SystemExit("unexpected fact fields; refusing unsafe normalization")
    if facts["defaultApp"] != {"kind": "boolean", "value": True}:
        raise SystemExit("unexpected defaultApp fact")
    for key in ("mas", "windowsStore", "sandboxed", "contextIsolated"):
        if facts[key] != {"kind": "undefined"}:
            raise SystemExit(f"unexpected {key} fact")
    resources = facts["resourcesPath"]
    if set(resources) != {"kind", "value"} or resources["kind"] != "string" or not isinstance(resources["value"], str):
        raise SystemExit("resourcesPath must be a tagged string")
    del data["userData"]
    argv[:2] = ["<electron-executable>", "<fixture-directory>"]
    resources["value"] = "<resources-directory>"
    args.output.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    print(f"raw_sha256={digest}")
    print(f"normalized_sha256={hashlib.sha256(args.output.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    main()

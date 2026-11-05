"""Prepare exact-byte native preload-order experiments. Never launches a process.

The fixture adapter is not @keld/electron, Route D, an approved trust root, a
classifier, or a permission writer. The pinned preload is inserted unchanged
after checking the SHA-256 of the same in-memory bytes.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

PRELOAD_SHA256 = "1091fd21ef727a52ef89bde814aa662eed4a0c3ce32beb858a068a96b285bf98"
ROOT = Path(__file__).resolve().parent


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def prepare(destination: Path, variant: str, existing: str | None = None) -> dict:
    if variant not in {"document-created", "dom-content-loaded"}:
        raise ValueError("Unknown timing variant")
    if existing not in {None, "none", "simple", "auto"}:
        raise ValueError("Unknown fixture configuration")
    source_path = ROOT / "source_binding/vendor/drawio/src/main/electron-preload.js"
    source = source_path.read_bytes()
    if digest(source) != PRELOAD_SHA256:
        raise ValueError("Pinned preload bytes differ; no registration bundle produced")
    template = (ROOT / "preload_adapter.js").read_bytes()
    marker = b"/* INSERT_AUTHENTIC_PRELOAD_BYTES */"
    options_marker = b'"__W1_FIXTURE_OPTIONS__"'
    if template.count(marker) != 1 or template.count(options_marker) != 1:
        raise ValueError("Expected exactly one literal marker of each type")
    options = json.dumps(
        {"sourceDigest": PRELOAD_SHA256, "existingConfiguration": existing},
        separators=(",", ":"),
    ).encode("utf-8")
    bundle = template.replace(options_marker, options).replace(marker, source)
    destination.mkdir(parents=True, exist_ok=False)
    bundle_path = destination / "preload-bundle.js"
    bundle_path.write_bytes(bundle)
    page = destination / "index.html"
    page.write_bytes((ROOT / "first_inline.html").read_bytes())
    config = {
        "mode": "order", "variant": variant, "url": page.as_uri(),
        "bundlePath": str(bundle_path), "bundleSha256": digest(bundle),
        "outDir": str(destination / "native"), "timeoutMs": 15000, "visible": False,
    }
    config_path = destination / "config.json"
    config_path.write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")
    receipt = {
        "schema": "keld.w1-prepared-fixture/v1",
        "scope": "native timing experiment; test-only facade adapter",
        "upstreamCommit": "2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e",
        "preloadSha256": digest(source), "preloadBytes": len(source),
        "adapterSha256": digest(template), "bundleSha256": digest(bundle),
        "bundleBytes": len(bundle), "htmlSha256": digest(page.read_bytes()),
        "variant": variant, "expectedConfiguration": existing or "auto",
        "config": str(config_path),
        "productionTrustRoot": "unapproved; immutable test-input pin only",
        "productionBridge": "not exercised", "applicationMain": "not executed",
    }
    (destination / "prepared.json").write_text(
        json.dumps(receipt, indent=2) + "\n", encoding="utf-8"
    )
    return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--variant", choices=["document-created", "dom-content-loaded"], required=True)
    parser.add_argument("--existing", choices=["none", "simple", "auto"])
    args = parser.parse_args()
    print(json.dumps(prepare(args.output.resolve(), args.variant, args.existing), indent=2))

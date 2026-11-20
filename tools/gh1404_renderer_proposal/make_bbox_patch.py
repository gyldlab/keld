#!/usr/bin/env python3
"""Patch the pinned draw.io mxText HTML foreignObject bounding-box consumer.

Reads the original mxText.js on stdin and writes the corrected production
JavaScript, or a unified source patch with --patch. Refuses a different pin.
"""

from __future__ import annotations

import argparse
import difflib
import hashlib
import sys


UPSTREAM_SHA256 = "a6c0f2c748403a74b11405c864a8278bfcdad216f80ea5b290c1e917d13f9772"
SOURCE_PATH = "src/main/webapp/mxgraph/src/shape/mxText.js"
FUNCTION = "mxText.prototype.updateBoundingBox = function()"
ANCHOR = "ow = node.offsetWidth * this.scale;\t"
REPLACEMENT = """// Wrapped foreignObject labels have a fixed CSS wrapping
					// width from mxSvgCanvas2D.createCss. WebKit may report
					// the outer SVG/foreignObject viewport instead of this
					// box's intended width. Do not feed that measurement
					// back into the SVG minimum size and page translation.
					ow = node.offsetWidth * this.scale;
					if (this.wrap && isFinite(this.bounds.width) &&
						this.bounds.width >= 0)
					{
						var maxWidth = this.bounds.width +
							this.textWidthPadding * this.scale;
						if (isFinite(maxWidth))
						{
							ow = Math.min(ow, maxWidth);
						}
					}"""


def apply_patch(original_bytes: bytes) -> str:
    observed = hashlib.sha256(original_bytes).hexdigest()
    if observed != UPSTREAM_SHA256:
        raise ValueError(
            f"unexpected mxText source digest: {observed}; expected {UPSTREAM_SHA256}"
        )
    original = original_bytes.decode("utf-8")
    start = original.find(FUNCTION)
    if start < 0 or original.find(FUNCTION, start + len(FUNCTION)) >= 0:
        raise ValueError("expected one mxText.updateBoundingBox definition")
    end = original.find("\n};", start)
    if end < 0:
        raise ValueError("mxText.updateBoundingBox end not found")
    owner = original[start:end]
    if owner.count(ANCHOR) != 1:
        raise ValueError("foreignObject DOM width read changed from source pin")
    return original[:start] + owner.replace(ANCHOR, REPLACEMENT, 1) + original[end:]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--patch", action="store_true", help="emit unified source diff")
    args = parser.parse_args()
    original_bytes = sys.stdin.buffer.read()
    try:
        candidate = apply_patch(original_bytes)
    except ValueError as exc:
        parser.error(str(exc))

    if args.patch:
        sys.stdout.writelines(
            difflib.unified_diff(
                original_bytes.decode("utf-8").splitlines(keepends=True),
                candidate.splitlines(keepends=True),
                fromfile="a/" + SOURCE_PATH,
                tofile="b/" + SOURCE_PATH,
            )
        )
    else:
        sys.stdout.write(candidate)


if __name__ == "__main__":
    main()

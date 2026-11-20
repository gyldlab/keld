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
TITLE_ANCHOR = """			if (node.firstChild != null && node.firstChild.firstChild != null &&
				node.firstChild.firstChild.nodeName == 'foreignObject' &&
				node.firstChild.firstChild.firstChild != null &&
				node.firstChild.firstChild.firstChild.firstChild != null)
			{
				// Uses second inner DIV for font metrics
				node = node.firstChild.firstChild.firstChild.firstChild;"""
TITLE_REPLACEMENT = """			// addForeignObject may add <title> before its HTML viewport.
			// Use the same title-skip rule as updateTextNodes, while non-HTML
			// SVG text still takes the original getBBox path.
			var textGroup = node.firstChild;
			var fo = (textGroup != null) ? textGroup.firstChild : null;
			if (fo != null && fo.nodeName == 'title')
			{
				fo = fo.nextSibling;
			}
			if (fo != null && fo.nodeName == 'foreignObject' &&
				fo.firstChild != null && fo.firstChild.firstChild != null)
			{
				// Uses second inner DIV for font metrics
				node = fo.firstChild.firstChild;"""
REPLACEMENT = """// Only positive-width, horizontal wrapped foreignObject labels
					// have a fixed CSS width in mxSvgCanvas2D.createCss.
					// For those labels, WebKit may measure the outer SVG
					// viewport instead. Do not feed that width into the
					// graph bounds and page translation. Vertical text wraps
					// by height; zero-width text keeps its natural width.
					ow = node.offsetWidth * this.scale;
					if (this.wrap && isFinite(this.bounds.width) &&
						this.bounds.width > 0)
					{
						var dir = this.getActualTextDirection();
						if (dir == null || dir.substring(0, 9) != 'vertical-')
						{
							var maxWidth = this.bounds.width +
								this.textWidthPadding * this.scale;
							if (isFinite(maxWidth))
							{
								ow = Math.min(ow, maxWidth);
							}
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
    if owner.count(TITLE_ANCHOR) != 1:
        raise ValueError("foreignObject/title detection changed from source pin")
    if owner.count(ANCHOR) != 1:
        raise ValueError("foreignObject DOM width read changed from source pin")
    return (original[:start] +
            owner.replace(TITLE_ANCHOR, TITLE_REPLACEMENT, 1)
            .replace(ANCHOR, REPLACEMENT, 1) +
            original[end:])


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

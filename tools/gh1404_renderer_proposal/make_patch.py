#!/usr/bin/env python3
"""Inert candidate renderer diff against one pinned upstream draw.io source.

The candidate is never written into an installed app or KELD production path.
Use stdout with --source or --patch; verify the source digest first.
"""

from __future__ import annotations

import argparse
import difflib
import hashlib
import sys


UPSTREAM_SHA256 = "0daef9812e3b8c530bc9556791ffaea53fe9a57091a86534bd909ca31e4485b3"
SOURCE_PATH = "src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js"
ANCHOR = """mxSvgCanvas2D.prototype.plainText = function(x, y, w, h, str, align, valign, wrap, overflow, clip, rotation, dir, textElement, node, blockTextHeight, htmlContentWidth)
{
	rotation = (rotation != null) ? rotation : 0;

	var s = this.state;
"""
REPLACEMENT = """mxSvgCanvas2D.prototype.plainText = function(x, y, w, h, str, align, valign, wrap, overflow, clip, rotation, dir, textElement, node, blockTextHeight, htmlContentWidth)
{
	rotation = (rotation != null) ? rotation : 0;

	var s = this.state;
	var values = [x, y, w, h, rotation, s.scale, s.dx, s.dy, s.rotation,
		s.fontSize, s.alpha, this.textOffset];
	if (blockTextHeight != null) values.push(blockTextHeight);
	if (htmlContentWidth != null) values.push(htmlContentWidth);

	// Reject geometry that cannot be represented as finite SVG coordinates.
	// Numeric strings remain accepted, including exponent notation.
	var valid = Number(s.scale) > 0;
	for (var i = 0; valid && i < values.length; i++)
	{
		var value = values[i];
		valid = (typeof value == 'number' ||
			(typeof value == 'string' && value.trim() != '')) &&
			isFinite(Number(value));
	}

	if (valid)
	{
		var sx = Number(s.scale);
		var xx = Number(x), yy = Number(y);
		var ww = Number(w), hh = Number(h);
		valid = isFinite(xx * sx) && isFinite(yy * sx) &&
			isFinite(ww * sx) && isFinite(hh * sx) &&
			isFinite(xx + ww) && isFinite(yy + hh) &&
			isFinite(yy + Number(s.fontSize)) &&
			isFinite(Number(s.rotation) + Number(rotation));
	}

	if (!valid)
	{
		// Reused node can remain attached after a failed geometry update.
		// Keep renderer-owned descendants for a later valid update.
		if (node != null)
		{
			if (node.mxRendererParkedSvgText == null)
			{
				var parked = node.ownerDocument.createDocumentFragment();
				while (node.firstChild != null) parked.appendChild(node.firstChild);
				node.mxRendererParkedSvgText = parked;
			}
			node.removeAttribute('transform');
			node.removeAttribute('clip-path');
			node.removeAttribute('opacity');
		}
		return false;
	}

	if (node != null && node.mxRendererParkedSvgText != null)
	{
		while (node.mxRendererParkedSvgText.firstChild != null)
			node.appendChild(node.mxRendererParkedSvgText.firstChild);
		delete node.mxRendererParkedSvgText;
	}
	x = Number(x); y = Number(y); w = Number(w); h = Number(h);
	rotation = Number(rotation);
"""

# Reused nodes must not preserve an old invalid or stale transform/opacity
# after a valid zero-rotation/full-alpha update.
TRANSFORM_ANCHOR = """	if (tr.length > 0)
	{
		node.setAttribute('transform', tr);
	}

	if (s.alpha < 1)
	{
		node.setAttribute('opacity', s.alpha);
	}

	var lines = str.split('\\n');"""
TRANSFORM_REPLACEMENT = """	if (tr.length > 0)
	{
		node.setAttribute('transform', tr);
	}
	else
	{
		node.removeAttribute('transform');
	}

	if (s.alpha < 1)
	{
		node.setAttribute('opacity', s.alpha);
	}
	else
	{
		node.removeAttribute('opacity');
	}

	var lines = str.split('\\n');"""


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--patch", action="store_true", help="emit unified diff")
    args = parser.parse_args()
    original_bytes = sys.stdin.buffer.read()
    observed = hashlib.sha256(original_bytes).hexdigest()
    if observed != UPSTREAM_SHA256:
        parser.error(f"unexpected source digest: {observed}; expected {UPSTREAM_SHA256}")
    original = original_bytes.decode("utf-8")
    if original.count(ANCHOR) != 1:
        parser.error("plainText entry does not match the pinned source")
    candidate = original.replace(ANCHOR, REPLACEMENT, 1)
    # Constrain the valid-update change to plainText, not other SVG call sites.
    begin = candidate.index("mxSvgCanvas2D.prototype.plainText = function(")
    end = candidate.index("\n};", begin)
    owner = candidate[begin:end]
    if owner.count(TRANSFORM_ANCHOR) != 1:
        parser.error("plainText transform/opacity region does not match the pin")
    candidate = (candidate[:begin] +
                 owner.replace(TRANSFORM_ANCHOR, TRANSFORM_REPLACEMENT, 1) +
                 candidate[end:])
    if args.patch:
        sys.stdout.writelines(difflib.unified_diff(
            original.splitlines(keepends=True),
            candidate.splitlines(keepends=True),
            fromfile="a/" + SOURCE_PATH,
            tofile="b/" + SOURCE_PATH,
        ))
    else:
        sys.stdout.write(candidate)


if __name__ == "__main__":
    main()

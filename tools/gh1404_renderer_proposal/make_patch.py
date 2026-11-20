#!/usr/bin/env python3
"""Inert candidate renderer diff against one pinned upstream draw.io source.

The candidate is never written into an installed app or KELD production path.
Stdout emits candidate JavaScript by default, or a diff with --patch.
Both forms verify the upstream source digest first.
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
REPLACEMENT = """mxSvgCanvas2D.areValidRendererNumbers = function(values)
{
	for (var i = 0; i < values.length; i++)
	{
		var value = values[i];
		if (!(typeof value == 'number' ||
			(typeof value == 'string' && value.trim() != '')) ||
			!isFinite(Number(value)))
		{
			return false;
		}
	}

	return true;
};

mxSvgCanvas2D.prototype.plainText = function(x, y, w, h, str, align, valign, wrap, overflow, clip, rotation, dir, textElement, node, blockTextHeight, htmlContentWidth)
{
	rotation = (rotation != null) ? rotation : 0;

	var s = this.state;
	var values = [x, y, w, h, rotation, s.scale, s.dx, s.dy, s.rotation,
		s.fontSize, s.alpha, this.textOffset];
	if (blockTextHeight != null) values.push(blockTextHeight);
	if (htmlContentWidth != null) values.push(htmlContentWidth);

	// Reject geometry that cannot be represented as finite SVG coordinates.
	// Numeric strings remain accepted, including exponent notation.
	var valid = Number(s.scale) > 0 &&
		mxSvgCanvas2D.areValidRendererNumbers(values);

	if (valid)
	{
		x = Number(x); y = Number(y); w = Number(w); h = Number(h);
		rotation = Number(rotation);
		valid = isFinite(x + w) && isFinite(y + h) &&
			isFinite(y + Number(s.fontSize)) &&
			isFinite(Number(s.rotation) + Number(rotation));
	}

	var size = Number(s.fontSize);
	var tr = s.transform || '';
	var lines, lh, textHeight, cy;
	if (valid)
	{
		// Calculate the actual baseline before modifying an SVG node.
		// Reuse this layout below so the gate checks what is rendered.
		lines = str.split('\\n');
		var rawLineHeight = size * mxConstants.LINE_HEIGHT;
		lh = Math.round(rawLineHeight);
		textHeight = (blockTextHeight != null) ? blockTextHeight :
			size + (lines.length - 1) * lh;
		cy = (blockTextHeight != null) ? y : y + size - 1;

		if (valign == mxConstants.ALIGN_MIDDLE)
		{
			if (overflow == 'fill')
			{
				cy -= h / 2;
			}
			else
			{
				var dy = ((this.matchHtmlAlignment && clip && h > 0) ?
					Math.min(textHeight, h) : textHeight) / 2;
				cy -= dy;
			}
		}
		else if (valign == mxConstants.ALIGN_BOTTOM)
		{
			if (overflow == 'fill')
			{
				cy -= h;
			}
			else
			{
				var dy = (this.matchHtmlAlignment && clip && h > 0) ?
					Math.min(textHeight, h) : textHeight;
				cy -= dy + 1;
			}
		}

		var scale = Number(s.scale);
		var offset = Number(this.textOffset);
		var lastLineY = cy + (lines.length - 1) * lh;
		valid = isFinite(rawLineHeight) && isFinite(lh) &&
			isFinite(textHeight) && isFinite(cy) && isFinite(lastLineY) &&
			isFinite(size * scale) && isFinite(x * scale) &&
			isFinite(y * scale) && isFinite(w * scale) &&
			isFinite(h * scale) && isFinite(x * scale + offset) &&
			isFinite(cy * scale + offset) &&
			isFinite(lastLineY * scale + offset);

		if (valid && clip && w > 0 && h > 0)
		{
			var clipX = x, clipY = y;
			if (align == mxConstants.ALIGN_CENTER) clipX -= w / 2;
			else if (align == mxConstants.ALIGN_RIGHT) clipX -= w;
			if (overflow != 'fill')
			{
				if (valign == mxConstants.ALIGN_MIDDLE) clipY -= h / 2;
				else if (valign == mxConstants.ALIGN_BOTTOM) clipY -= h;
			}
			valid = isFinite(clipX * scale - 2) &&
				isFinite(clipY * scale - 2) &&
				isFinite(w * scale + 4) && isFinite(h * scale + 4);
		}

		if (valid && blockTextHeight != null && clip &&
			(align == mxConstants.ALIGN_CENTER ||
				align == mxConstants.ALIGN_RIGHT) &&
			htmlContentWidth != null && htmlContentWidth > w)
		{
			var shift = (Number(htmlContentWidth) - w) /
				((align == mxConstants.ALIGN_CENTER) ? 2 : 1);
			valid = isFinite(shift) && isFinite((x + shift) * scale + offset);
		}
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
"""

HTML_ANCHOR = """mxSvgCanvas2D.prototype.updateTextNodes = function(x, y, w, h, align, valign, wrap, overflow, clip, rotation, dir, g)
{
	var s = this.state.scale;
"""
HTML_REPLACEMENT = """mxSvgCanvas2D.prototype.updateTextNodes = function(x, y, w, h, align, valign, wrap, overflow, clip, rotation, dir, g)
{
	rotation = (rotation != null) ? rotation : 0;
	var values = [x, y, w, h, rotation, this.state.scale, this.state.dx,
		this.state.dy, this.state.rotation, this.state.alpha, this.foOffset,
		this.foreignObjectPadding, this.state.fontSize];
	var valid = Number(this.state.scale) > 0 &&
		mxSvgCanvas2D.areValidRendererNumbers(values);
	var s = Number(this.state.scale);

	if (valid)
	{
		x = Number(x); y = Number(y); w = Number(w); h = Number(h);
		rotation = Number(rotation);
		var anchor = mxUtils.getAlignmentAsPoint(align, valign);
		var width = w + Number(this.foreignObjectPadding);
		var left = x + Number(this.state.dx);
		var top = y + Number(this.state.dy);
		var angle = ((this.rotateHtml) ? Number(this.state.rotation) : 0) + rotation;
		// Preserve the old subpixel-zoom refusal independently of FO sizing.
		var reciprocalZoom = Math.ceil(100 / Math.min(1, s));
		// Match getTextCss: either line-height mode can overflow after
		// its individual numeric inputs and scaled font-size pass.
		var lineHeight = (mxConstants.ABSOLUTE_LINE_HEIGHT) ?
			Number(this.state.fontSize) * mxConstants.LINE_HEIGHT :
			mxConstants.LINE_HEIGHT * Number(this.lineHeightCorrection);
		valid = isFinite(width) && isFinite(left) && isFinite(top) &&
			isFinite(angle) && isFinite(reciprocalZoom) && isFinite(lineHeight) &&
			isFinite(left + width) && isFinite(left - width) &&
			isFinite(top + h) && isFinite(top - h) &&
			isFinite(Math.round(left + anchor.x * width)) &&
			isFinite(Math.round(top + anchor.y * h)) &&
			isFinite(left * s) && isFinite(top * s) &&
			isFinite(width * s) && isFinite(h * s) &&
			isFinite(Number(this.state.fontSize) * s);

		// Background and border padding becomes CSS pixel geometry.
		if (valid && this.state.labelPadding != null)
		{
			var padding = this.state.labelPadding;
			valid = mxSvgCanvas2D.areValidRendererNumbers(
				[padding.top, padding.right, padding.bottom, padding.left]);
		}
	}

	if (!valid)
	{
		// Hide reused rich-label descendants without destroying the DOM
		// needed for a later valid geometry or zoom update.
		if (!g.mxRendererHiddenForInvalidGeometry)
		{
			g.mxRendererPreviousDisplay = g.getAttribute('display');
			g.mxRendererHiddenForInvalidGeometry = true;
		}
		g.setAttribute('display', 'none');
		g.removeAttribute('transform');
		g.removeAttribute('opacity');
		return false;
	}

	if (g.mxRendererHiddenForInvalidGeometry)
	{
		if (g.mxRendererPreviousDisplay == null)
			g.removeAttribute('display');
		else
			g.setAttribute('display', g.mxRendererPreviousDisplay);
		delete g.mxRendererPreviousDisplay;
		delete g.mxRendererHiddenForInvalidGeometry;
	}
"""

# Original layout is moved before any SVG mutation to validate its derived
# floating-point values once; subsequent painting reuses the same variables.
LAYOUT_ANCHOR = """	var lines = str.split('\\n');
	var lh = Math.round(size * mxConstants.LINE_HEIGHT);
	var textHeight = (blockTextHeight != null) ? blockTextHeight :
		size + (lines.length - 1) * lh;

	// For block mode, y offsets on child elements already include baseline
	var cy = (blockTextHeight != null) ? y : y + size - 1;

	if (valign == mxConstants.ALIGN_MIDDLE)
	{
		if (overflow == 'fill')
		{
			cy -= h / 2;
		}
		else
		{
			var dy = ((this.matchHtmlAlignment && clip && h > 0) ? Math.min(textHeight, h) : textHeight) / 2;
			cy -= dy;
		}
	}
	else if (valign == mxConstants.ALIGN_BOTTOM)
	{
		if (overflow == 'fill')
		{
			cy -= h;
		}
		else
		{
			var dy = (this.matchHtmlAlignment && clip && h > 0) ?
				Math.min(textHeight, h) : textHeight;
			cy -= dy + 1;
		}
	}
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
"""
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
"""


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--patch", action="store_true", help="emit unified diff")
    args = parser.parse_args()
    original_bytes = sys.stdin.buffer.read()
    observed = hashlib.sha256(original_bytes).hexdigest()
    if observed != UPSTREAM_SHA256:
        parser.error(f"unexpected source digest: {observed}; expected {UPSTREAM_SHA256}")
    original = original_bytes.decode("utf-8")
    if original.count(ANCHOR) != 1 or original.count(HTML_ANCHOR) != 1:
        parser.error("text renderer entry does not match the pinned source")
    candidate = original.replace(HTML_ANCHOR, HTML_REPLACEMENT, 1)
    begin_html = candidate.index("mxSvgCanvas2D.prototype.updateTextNodes = function(")
    end_html = candidate.index("\n};", begin_html)
    html_owner = candidate[begin_html:end_html]
    substitutions = (
        ("w + this.foreignObjectPadding", "w + Number(this.foreignObjectPadding)"),
        ("x += this.state.dx;", "x += Number(this.state.dx);"),
        ("y += this.state.dy;", "y += Number(this.state.dy);"),
        ("var r = ((this.rotateHtml) ? this.state.rotation : 0) + ((rotation != null) ? rotation : 0);",
         "var r = ((this.rotateHtml) ? Number(this.state.rotation) : 0) + rotation;"),
        ("""// Workaround for clipping in Webkit with scrolling and zoom
		fo.setAttribute('width', Math.ceil(1 / Math.min(1, s) * 100) + '%');
		fo.setAttribute('height', Math.ceil(1 / Math.min(1, s) * 100) + '%');
		var yp = Math.round(y + dy);""",
         """// For wrapped horizontal labels, use a bounded pixel viewport in
		// the label's SVG coordinates. Percentage extents depend on the
		// outer SVG size, which itself depends on measured label bounds.
		// Keep the legacy overflow viewport for unwrapped/vertical labels.
		var yp = Math.round(y + dy);
		if (wrap && w > 0 && !vertical)
		{
			fo.setAttribute('width', Math.max(1, Math.ceil(
				Math.max(0, x + dx) + Math.max(0, w + Number(this.foreignObjectPadding)))));
			fo.setAttribute('height', Math.max(1, Math.ceil(
				Math.max(0, yp) + Math.max(0, h))));
		}
		else
		{
			fo.setAttribute('width', Math.ceil(1 / Math.min(1, s) * 100) + '%');
			fo.setAttribute('height', Math.ceil(1 / Math.min(1, s) * 100) + '%');
		}"""),
    )
    for before, after in substitutions:
        if html_owner.count(before) != 1:
            parser.error(f"HTML text renderer region changed: {before}")
        html_owner = html_owner.replace(before, after, 1)
    candidate = candidate[:begin_html] + html_owner + candidate[end_html:]
    candidate = candidate.replace(ANCHOR, REPLACEMENT, 1)
    # Constrain the valid-update change to plainText, not other SVG call sites.
    begin = candidate.index("mxSvgCanvas2D.prototype.plainText = function(")
    end = candidate.index("\n};", begin)
    owner = candidate[begin:end]
    if owner.count(LAYOUT_ANCHOR) != 1:
        parser.error("plainText baseline layout region does not match the pin")
    if owner.count(TRANSFORM_ANCHOR) != 1:
        parser.error("plainText transform/opacity region does not match the pin")
    candidate = (candidate[:begin] +
                 owner.replace(TRANSFORM_ANCHOR, TRANSFORM_REPLACEMENT, 1)
                 .replace(LAYOUT_ANCHOR, "", 1)
                 .replace("\tvar size = s.fontSize;\n\tvar tr = s.transform || '';\n", "", 1) +
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

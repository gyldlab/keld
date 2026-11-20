#!/usr/bin/env node
// Source-level regression for the real mxText.updateBoundingBox consumer.
// The page translation check is calculated from the pinned Graph.getPageLayout.
// No test here is a claim of installed application or accessibility behavior.
"use strict";

const assert = require("node:assert/strict");
const { createHash } = require("node:crypto");
const { readFileSync } = require("node:fs");

const originalSha =
  "a6c0f2c748403a74b11405c864a8278bfcdad216f80ea5b290c1e917d13f9772";
const path = process.argv[2];
const baseline = process.argv.includes("--baseline");
assert(path, "Usage: node bounds_probe.cjs mxText.js [--baseline]");
const source = readFileSync(path, "utf8");
if (baseline) {
  assert.equal(
    createHash("sha256").update(source).digest("hex"),
    originalSha,
    "baseline must be the actual immutable draw.io mxText source",
  );
}

const marker = "mxText.prototype.updateBoundingBox = function()";
const begin = source.indexOf(marker);
const end = source.indexOf("\n};", begin);
assert(begin >= 0 && end > begin, "pinned real updateBoundingBox missing");
assert.equal(
  source.indexOf(marker, begin + marker.length),
  -1,
  "duplicate function",
);
const directionMarker = "mxText.prototype.getActualTextDirection = function()";
const directionBegin = source.indexOf(directionMarker);
const directionEnd = source.indexOf("\n};", directionBegin);
assert(
  directionBegin >= 0 && directionEnd > directionBegin,
  "pinned real getActualTextDirection missing",
);

class Rect {
  constructor(x, y, width, height) {
    Object.assign(this, { x, y, width, height });
  }
  clone() {
    return new Rect(this.x, this.y, this.width, this.height);
  }
  static fromRectangle(rect) {
    return rect.clone();
  }
}
class Pt {
  constructor(x, y) {
    Object.assign(this, { x, y });
  }
}

function Text() {}
const constants = {
  STYLE_LABEL_POSITION: "labelPosition",
  STYLE_VERTICAL_LABEL_POSITION: "verticalLabelPosition",
  ALIGN_CENTER: "center",
  ALIGN_MIDDLE: "middle",
  TEXT_DIRECTION_AUTO: "auto",
  TEXT_DIRECTION_LTR: "ltr",
  TEXT_DIRECTION_RTL: "rtl",
  TEXT_DIRECTION_VERTICAL_LR: "vertical-lr",
  TEXT_DIRECTION_VERTICAL_RL: "vertical-rl",
};
const [updateBoundingBox, getActualTextDirection] = new Function(
  "mxText",
  "mxRectangle",
  "mxPoint",
  "mxConstants",
  "mxUtils",
  source.slice(directionBegin, directionEnd + 3) +
    "\n" +
    source.slice(begin, end + 3) +
    "\nreturn [mxText.prototype.updateBoundingBox, mxText.prototype.getActualTextDirection];",
)(Text, Rect, Pt, constants, {
  getValue: (style, key, otherwise) => style?.[key] ?? otherwise,
  trim: (value) => value.trim(),
  getBoundingBox: (rect, rotation, center) => {
    // Deterministic 90-degree coordinate oracle for the caller's supplied
    // rectangle; this test checks rotation routing and ownership.
    assert.equal(rotation, 90);
    assert.deepEqual([center.x, center.y], [0, 0]);
    return new Rect(-rect.height / 2, -rect.width / 2, rect.height, rect.width);
  },
});

function makeText(options = {}) {
  // Draw.io's real DOM path:
  // <svg><g text.node><g><foreignObject><div><div box><div text>.
  const box = {
    offsetWidth: options.domWidth ?? 211,
    offsetHeight: options.domHeight ?? 14,
  };
  const foreignObject = {
    nodeName: "foreignObject",
    firstChild: { firstChild: box },
  };
  const contentNode = options.nonForeignObject
    ? { nodeName: "text" }
    : foreignObject;
  const firstNode = options.title
    ? { nodeName: "title", nextSibling: contentNode }
    : contentNode;
  let svgBBoxReads = 0;
  const node = {
    ownerSVGElement: {},
    firstChild: { firstChild: firstNode },
    getBBox: () => {
      svgBBoxReads++;
      return options.fallbackBBox ?? new Rect(31777, -345, 18614, 10);
    },
  };
  const text = {
    node,
    bounds: new Rect(100, 110, options.width ?? 320, options.height ?? 80),
    margin: { x: -0.5, y: -0.5 },
    scale: options.scale ?? 1,
    wrap: options.wrap ?? true,
    overflow: options.overflow ?? "",
    clipped: false,
    ignoreStringSize: false,
    style: {},
    textWidthPadding: 3,
    getTextRotation: () => options.rotation ?? 0,
    value: options.value ?? "GH1404-TEXT",
    textDirection: options.direction,
    getActualTextDirection,
    getAutoDirection: () => options.autoDirection ?? "ltr",
  };
  return { text, box, svgBBoxReads: () => svgBBoxReads };
}

// These formulas are pinned in Graph.prototype.getPageLayout and
// Graph.prototype.getDefaultTranslate, independent of mxText.
function pageAndTranslate(bbox) {
  const pageWidth = 1169;
  const pageX = Math.floor(Math.ceil(bbox.x) / pageWidth);
  return { pageX, translateX: 500 - pageX * pageWidth };
}

const cases = [
  [
    "normal wrapped HTML keeps its intrinsic size",
    () => {
      const { text } = makeText();
      updateBoundingBox.call(text);
      assert.deepEqual(
        [
          text.boundingBox.x,
          text.boundingBox.y,
          text.boundingBox.width,
          text.boundingBox.height,
        ],
        [-5.5, 103, 211, 14],
      );
    },
  ],
  [
    "inflated WebKit DOM width cannot grow graph/page bounds",
    () => {
      const { text } = makeText({ domWidth: 18614, domHeight: 10 });
      updateBoundingBox.call(text);
      assert.equal(text.boundingBox.width, 323);
      assert.equal(text.boundingBox.x, -61.5);
      assert.deepEqual(pageAndTranslate(text.boundingBox), {
        pageX: -1,
        translateX: 1669,
      });
    },
  ],
  [
    "wrapped label bounds remain stable across changing SVG viewport widths",
    () => {
      const { text, box } = makeText({ domWidth: 322 });
      const observed = [];
      for (const width of [322, 1000, 18614, 100000, 322]) {
        box.offsetWidth = width;
        updateBoundingBox.call(text);
        observed.push(pageAndTranslate(text.boundingBox));
        assert(text.boundingBox.width <= 323);
      }
      assert(observed.every((entry) => entry.translateX === 1669));
    },
  ],
  [
    "titled wrapped HTML skips title before foreignObject and bounds oversized width",
    () => {
      const { text, svgBBoxReads } = makeText({
        title: true,
        domWidth: 18614,
        domHeight: 10,
      });
      updateBoundingBox.call(text);
      assert.equal(
        svgBBoxReads(),
        0,
        "titled HTML must not route to SVG getBBox",
      );
      assert.equal(text.boundingBox.width, 323);
      assert.equal(text.boundingBox.x, -61.5);
    },
  ],
  [
    "titled wrapped HTML retains normal intrinsic text and title ownership",
    () => {
      const { text, svgBBoxReads } = makeText({ title: true, domWidth: 211 });
      const title = text.node.firstChild.firstChild;
      updateBoundingBox.call(text);
      assert.equal(svgBBoxReads(), 0);
      assert.equal(text.node.firstChild.firstChild, title);
      assert.equal(title.nodeName, "title");
      assert.equal(text.boundingBox.width, 211);
      assert.equal(text.boundingBox.x, -5.5);
    },
  ],
  [
    "titled non-HTML SVG text retains original getBBox path",
    () => {
      const observed = new Rect(15, 19, 61, 12);
      const { text, svgBBoxReads } = makeText({
        title: true,
        nonForeignObject: true,
        fallbackBBox: observed,
      });
      updateBoundingBox.call(text);
      assert.equal(svgBBoxReads(), 1);
      assert.deepEqual(text.boundingBox, observed);
      assert.equal(text.boundingBox.x, 15);
    },
  ],
  [
    "titled HTML with 90-degree text rotation keeps measured and rotated bounds",
    () => {
      const { text, svgBBoxReads } = makeText({
        title: true,
        rotation: 90,
        domWidth: 211,
      });
      updateBoundingBox.call(text);
      assert.equal(svgBBoxReads(), 0);
      assert.deepEqual(text.boundingBox, new Rect(93, 4.5, 14, 211));
      assert.deepEqual(text.unrotatedBoundingBox, new Rect(-5.5, 103, 211, 14));
    },
  ],
  [
    "unwrapped overflow retains native measured width",
    () => {
      const { text } = makeText({ domWidth: 18614, wrap: false });
      updateBoundingBox.call(text);
      assert.equal(text.boundingBox.width, 18614);
    },
  ],
  [
    "zero-width horizontal wrapping retains the entire measured text width",
    () => {
      const { text } = makeText({ width: 0, domWidth: 200 });
      updateBoundingBox.call(text);
      assert.equal(text.boundingBox.width, 200);
      assert.equal(text.boundingBox.x, 0);
    },
  ],
  [
    "vertical-rl wrapping uses height, so the measured text width survives",
    () => {
      const { text } = makeText({
        width: 20,
        domWidth: 200,
        direction: "vertical-rl",
      });
      assert.equal(text.getActualTextDirection(), "vertical-rl");
      updateBoundingBox.call(text);
      assert.equal(text.boundingBox.width, 200);
      assert.equal(text.boundingBox.x, 0);
    },
  ],
  [
    "vertical-lr wrapping also retains the measured text width",
    () => {
      const { text } = makeText({
        width: 20,
        domWidth: 200,
        direction: "vertical-lr",
      });
      assert.equal(text.getActualTextDirection(), "vertical-lr");
      updateBoundingBox.call(text);
      assert.equal(text.boundingBox.width, 200);
      assert.equal(text.boundingBox.x, 0);
    },
  ],
  [
    "width overflow still uses the authored box",
    () => {
      const { text } = makeText({ domWidth: 18614, overflow: "width" });
      updateBoundingBox.call(text);
      assert.equal(text.boundingBox.width, 320);
    },
  ],
  [
    "reused label restores fresh intrinsic width after oversized measurement",
    () => {
      const { text, box } = makeText({ domWidth: 18614 });
      updateBoundingBox.call(text);
      assert.equal(text.boundingBox.width, 323);
      box.offsetWidth = 211;
      updateBoundingBox.call(text);
      assert.equal(text.boundingBox.width, 211);
      assert.equal(text.boundingBox.x, -5.5);
    },
  ],
];

let failureCount = 0;
let validFailureCount = 0;
const normal = new Set([
  "normal wrapped HTML keeps its intrinsic size",
  "unwrapped overflow retains native measured width",
  "zero-width horizontal wrapping retains the entire measured text width",
  "vertical-rl wrapping uses height, so the measured text width survives",
  "vertical-lr wrapping also retains the measured text width",
  "width overflow still uses the authored box",
  "titled non-HTML SVG text retains original getBBox path",
]);
for (const [name, run] of cases) {
  try {
    run();
    console.log("PASS", name);
  } catch (error) {
    failureCount++;
    if (normal.has(name)) validFailureCount++;
    console.log("FAIL", name, error.message);
  }
}
if (baseline) {
  assert.equal(validFailureCount, 0, "original unaffected contracts regressed");
  assert(
    failureCount > 0,
    "original source should fail the inflated-size regressions",
  );
} else {
  assert.equal(
    failureCount,
    0,
    "corrected source failed a bounding-box regression",
  );
}
console.log(
  `${baseline ? "BASELINE_RED" : "CANDIDATE_GREEN"}: ${cases.length - failureCount}/${cases.length} passed`,
);

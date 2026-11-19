#!/usr/bin/env node
// Source-level geometry oracle for the pinned draw.io mxSvgCanvas2D plainText.
// This intentionally models SVG DOM structure, not native WebKit acceptance.
'use strict';

const assert = require('node:assert/strict');
const { createHash } = require('node:crypto');
const { readFileSync } = require('node:fs');

class SvgNode {
  constructor(nodeName, ownerDocument) {
    this.nodeName = nodeName;
    this.ownerDocument = ownerDocument;
    this.childNodes = [];
    this.parentNode = null;
    this.attributes = new Map();
    this._text = '';
  }
  appendChild(child) {
    if (child.nodeName === '#document-fragment') {
      while (child.firstChild) this.appendChild(child.firstChild);
      return child;
    }
    if (child.parentNode) child.parentNode.removeChild(child);
    child.parentNode = this;
    this.childNodes.push(child);
    return child;
  }
  removeChild(child) {
    const index = this.childNodes.indexOf(child);
    assert.notEqual(index, -1);
    this.childNodes.splice(index, 1);
    child.parentNode = null;
    return child;
  }
  get firstChild() { return this.childNodes[0] || null; }
  get nextSibling() {
    if (!this.parentNode) return null;
    const siblings = this.parentNode.childNodes;
    return siblings[siblings.indexOf(this) + 1] || null;
  }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
  removeAttribute(name) { this.attributes.delete(name); }
  set textContent(value) { this._text = String(value); this.childNodes = []; }
  get textContent() {
    return this._text + this.childNodes.map((child) => child.textContent).join('');
  }
  contains(other) {
    return this === other || this.childNodes.some((child) => child.contains(other));
  }
}

class SvgDocument {
  createElement(name) { return new SvgNode(name, this); }
  createDocumentFragment() { return new SvgNode('#document-fragment', this); }
}

function compilePlainText(source) {
  const marker = 'mxSvgCanvas2D.prototype.plainText = function(';
  const start = source.indexOf(marker);
  const end = source.indexOf('\n};', start);
  assert(start >= 0 && end > start, 'pinned plainText definition missing');
  assert.equal(source.indexOf(marker, start + marker.length), -1,
    'multiple plainText definitions');
  const compile = new Function('mxSvgCanvas2D', 'mxConstants', 'mxUtils',
    source.slice(start, end + 3) + '\nreturn mxSvgCanvas2D.prototype.plainText;');
  function Canvas() {}
  return compile(Canvas, {
    ALIGN_CENTER: 'center', ALIGN_RIGHT: 'right',
    ALIGN_MIDDLE: 'middle', ALIGN_BOTTOM: 'bottom',
    DEFAULT_FONTSIZE: 11, LINE_HEIGHT: 1.2,
  }, {
    trim: (value) => value.trim(),
    write: (node, value) => { node.textContent = value; },
  });
}

function canvasWith(plainText) {
  const doc = new SvgDocument();
  const svg = doc.createElement('svg');
  const canvas = {
    root: svg,
    state: {
      fontSize: 16, scale: 1, rotation: 0, dx: 0, dy: 0, alpha: 1,
    },
    textOffset: 0, pointerEvents: true, styleEnabled: false,
    matchHtmlAlignment: true,
    addTitle: (element) => element,
    createElement: (name) => doc.createElement(name),
    updateFont: () => {},
    addTextBackground: () => {},
    // Upstream mxSvgCanvas2D.format rounds using toFixed, including large floats.
    format: (value) => parseFloat(parseFloat(value).toFixed(2)),
    createClip: (...values) => {
      canvas.clipArguments = values;
      return values;
    },
    setClip: (node, values) => { node.clipArguments = values; },
    plainText,
  };
  return { canvas, svg, doc };
}

function draw(canvas, text, rotation = 0, node = null, coords = [120, 120, 150, 50]) {
  return canvas.plainText(...coords, text, 'center', 'middle',
    false, '', false, rotation, null, null, node);
}

function collect(root, name) {
  return (root.nodeName === name ? [root] : []).concat(
    root.childNodes.flatMap((child) => collect(child, name)));
}

const file = process.argv[2];
assert(file, 'Usage: node render_probe.cjs <original-or-proposed-mxSvgCanvas2D.js> [--baseline]');
const source = readFileSync(file, 'utf8');
const baseline = process.argv.includes('--baseline');
if (baseline) {
  assert.equal(createHash('sha256').update(source).digest('hex'),
    '0daef9812e3b8c530bc9556791ffaea53fe9a57091a86534bd909ca31e4485b3',
    'baseline source must match the pinned upstream input');
}
const plainText = compilePlainText(source);
const cases = [
  ['ordinary numeric coordinates render unchanged', () => {
    const { canvas, svg } = canvasWith(plainText);
    draw(canvas, 'EXACT-LABEL', 30);
    const text = collect(svg, 'text');
    assert.equal(text.length, 1);
    assert.equal(text[0].textContent, 'EXACT-LABEL');
    assert.equal(text[0].getAttribute('x'), '120');
    assert.equal(text[0].getAttribute('y'), '127');
    assert.equal(svg.firstChild.getAttribute('transform'), 'rotate(30,120,120)');
  }],
  ['valid numeric strings use numeric geometry', () => {
    const { canvas, svg } = canvasWith(plainText);
    draw(canvas, 'NUMERIC-STRING', '30', null, ['120', '120', '150', '50']);
    assert.equal(collect(svg, 'text')[0].getAttribute('y'), '127');
    assert.equal(svg.firstChild.getAttribute('transform'), 'rotate(30,120,120)');
  }],
  ['valid negative positions and zero rotation render', () => {
    const { canvas, svg } = canvasWith(plainText);
    draw(canvas, 'NEGATIVE', 0, null, [-10, -20, 150, 50]);
    assert.equal(collect(svg, 'text')[0].textContent, 'NEGATIVE');
    assert.equal(svg.firstChild.getAttribute('transform'), null);
  }],
  ['null rotation uses the original zero-rotation default', () => {
    const { canvas, svg } = canvasWith(plainText);
    draw(canvas, 'DEFAULT-ROTATION', null);
    assert.equal(collect(svg, 'text')[0].textContent, 'DEFAULT-ROTATION');
    assert.equal(svg.firstChild.getAttribute('transform'), null);
  }],
  ['large finite font and native SVG center position remain accepted', () => {
    const { canvas, svg } = canvasWith(plainText);
    canvas.state.scale = 2;
    canvas.state.fontSize = 1e307;
    draw(canvas, 'LARGE-FINITE', 0, null, [0, 0, 0, 0]);
    const text = collect(svg, 'text');
    assert.equal(text.length, 1);
    assert.equal(text[0].textContent, 'LARGE-FINITE');
    assert.equal(svg.firstChild.getAttribute('font-size'), '2e+307px');
    assert(Number.isFinite(Number(text[0].getAttribute('y'))));
  }],
  ['ordinary multiline text retains each line and its baseline', () => {
    const { canvas, svg } = canvasWith(plainText);
    canvas.state.fontSize = 12;
    draw(canvas, 'FIRST\nSECOND');
    const text = collect(svg, 'text');
    assert.deepEqual(text.map((node) => node.textContent), ['FIRST', 'SECOND']);
    assert.deepEqual(text.map((node) => node.getAttribute('y')), ['118', '132']);
  }],
  ['valid clip coordinates retain original finite clipping geometry', () => {
    const { canvas, svg } = canvasWith(plainText);
    canvas.plainText(120, 120, 150, 50, 'CLIPPED', 'right',
      'middle', false, '', true, 0, null);
    assert.equal(collect(svg, 'text')[0].textContent, 'CLIPPED');
    assert.deepEqual(canvas.clipArguments, [-32, 93, 154, 54]);
  }],
  ['font size multiplied by scale cannot emit Infinitypx', () => {
    const { canvas, svg } = canvasWith(plainText);
    canvas.state.scale = 2;
    canvas.state.fontSize = 1e308;
    const returned = draw(canvas, 'OVERFLOW-FONT', 0, null, [0, 0, 0, 0]);
    assert.equal(collect(svg, 'text').length, 0,
      `invalid native font-size=${svg.firstChild?.getAttribute('font-size')}`);
    assert.equal(returned, false);
    assert.equal(svg.childNodes.length, 0);
  }],
  ['centered text y times scale cannot emit Infinity', () => {
    const { canvas, svg } = canvasWith(plainText);
    canvas.state.scale = 2;
    canvas.state.fontSize = 4e307;
    const returned = draw(canvas, 'OVERFLOW-CENTER', 0, null,
      [0, 8e307, 0, 0]);
    assert.equal(collect(svg, 'text').length, 0,
      `invalid centered y=${collect(svg, 'text')[0]?.getAttribute('y')}`);
    assert.equal(returned, false);
  }],
  ['multiline line-height arithmetic cannot emit NaN or Infinity', () => {
    const { canvas, svg } = canvasWith(plainText);
    canvas.state.fontSize = 1.6e308;
    const returned = draw(canvas, 'FIRST\nSECOND', 0, null,
      [0, 0, 0, 0]);
    assert.equal(collect(svg, 'text').length, 0,
      `invalid multiline y=${collect(svg, 'text')[0]?.getAttribute('y')}`);
    assert.equal(returned, false);
  }],
  ['second text-line position cannot overflow after a finite first line', () => {
    const { canvas, svg } = canvasWith(plainText);
    canvas.state.scale = 2;
    canvas.state.fontSize = 3e307;
    const returned = draw(canvas, 'FIRST\nSECOND', 0, null,
      [0, 8e307, 0, 0]);
    assert.equal(collect(svg, 'text').length, 0,
      'second-line y=' + collect(svg, 'text')[1]?.getAttribute('y'));
    assert.equal(returned, false);
  }],
  ['finite positions cannot overflow when adding renderer text offset', () => {
    const { canvas, svg } = canvasWith(plainText);
    canvas.textOffset = 1e308;
    const returned = draw(canvas, 'OVERFLOW-OFFSET', 0, null,
      [1e308, 0, 0, 0]);
    assert.equal(collect(svg, 'text').length, 0,
      `invalid offset x=${collect(svg, 'text')[0]?.getAttribute('x')}`);
    assert.equal(returned, false);
  }],
  ['clip anchor subtraction cannot overflow after a finite input check', () => {
    const { canvas, svg } = canvasWith(plainText);
    canvas.state.scale = 2;
    const returned = canvas.plainText(-8e307, 0, 8e307, 3,
      'OVERFLOW-CLIP', 'right', 'top', false, '', true, 0, null);
    assert.equal(canvas.clipArguments, undefined,
      `invalid clip coordinates=${JSON.stringify(canvas.clipArguments)}`);
    assert.equal(collect(svg, 'text').length, 0);
    assert.equal(returned, false);
  }],
  ['converted rich label retains its native SVG child', () => {
    const { canvas, svg, doc } = canvasWith(plainText);
    const richText = doc.createElement('text');
    richText.textContent = 'RICH-LABEL';
    canvas.plainText(120, 120, 150, 50, '', 'center', 'middle',
      false, '', false, 30, null, richText);
    assert.equal(collect(svg, 'text')[0].textContent, 'RICH-LABEL');
    assert.equal(svg.firstChild.getAttribute('transform'), 'rotate(30,120,120)');
  }],
  ['malformed rotation has no new rendered label or injected transform', () => {
    const { canvas, svg } = canvasWith(plainText);
    const result = draw(canvas, 'BAD-FRESH', '0) scale(4) rotate(0');
    assert.equal(result, false);
    assert.equal(collect(svg, 'text').length, 0);
    assert.equal(svg.childNodes.length, 0);
  }],
  ['invalid reused geometry hides old text and recovers without hurting a sibling', () => {
    const { canvas, svg, doc } = canvasWith(plainText);
    draw(canvas, 'STILL-VISIBLE', 30);
    const group = svg.firstChild;
    const neighbor = doc.createElement('text');
    neighbor.textContent = 'NEIGHBOR-KEEP';
    svg.appendChild(neighbor);
    draw(canvas, '', '0) scale(4) rotate(0', group);
    assert.equal(collect(group, 'text').length, 0);
    assert.equal(group.getAttribute('transform'), null);
    assert.equal(svg.textContent.includes('STILL-VISIBLE'), false);
    assert.equal(neighbor.textContent, 'NEIGHBOR-KEEP');
    draw(canvas, '', '0) scale(4) rotate(0', group);
    assert.equal(collect(group, 'text').length, 0);
    draw(canvas, '', 0, group);
    assert.equal(collect(group, 'text')[0].textContent, 'STILL-VISIBLE');
    assert.equal(group.getAttribute('transform'), null);
    assert.equal(svg.contains(neighbor), true);
  }],
  ['invalid converted-rich SVG child recovers without losing its sibling', () => {
    const { canvas, svg, doc } = canvasWith(plainText);
    const richText = doc.createElement('text');
    richText.textContent = 'RICH-CHILD';
    canvas.plainText(120, 120, 150, 50, '', 'center', 'middle',
      false, '', false, 30, null, richText);
    const group = svg.firstChild;
    const sibling = doc.createElement('text');
    sibling.textContent = 'UNRELATED-SIBLING';
    svg.appendChild(sibling);
    draw(canvas, '', '0) scale(4) rotate(0', group);
    assert.equal(collect(group, 'text').length, 0);
    assert.equal(collect(svg, 'text')[0].textContent, 'UNRELATED-SIBLING');
    draw(canvas, '', 0, group);
    assert.equal(collect(group, 'text')[0].textContent, 'RICH-CHILD');
    assert.equal(collect(svg, 'text').length, 2);
    assert.equal(svg.contains(sibling), true);
  }],
  ['invalid then full repaint of a new value never leaks the old label', () => {
    const { canvas, svg, doc } = canvasWith(plainText);
    draw(canvas, 'OLD-LABEL', 30);
    const originalGroup = svg.firstChild;
    const sibling = doc.createElement('text');
    sibling.textContent = 'SIBLING';
    svg.appendChild(sibling);
    // In the actual mxText caller, updateText/plainText reuse is only a
    // geometry update. A changed cell value runs mxShape.redraw: clear the
    // owned shape children first, then create new SVG text.
    draw(canvas, '', '0) scale(4) rotate(0', originalGroup);
    assert.equal(collect(originalGroup, 'text').length, 0);
    svg.removeChild(originalGroup);
    draw(canvas, 'NEW-LABEL', 0);
    assert.deepEqual(collect(svg, 'text').map((n) => n.textContent).sort(),
      ['NEW-LABEL', 'SIBLING']);
    const newGroup = svg.childNodes.find((n) => n.nodeName === 'g');
    svg.removeChild(newGroup);
    draw(canvas, 'REPLACED-LABEL', 30);
    assert.deepEqual(collect(svg, 'text').map((n) => n.textContent).sort(),
      ['REPLACED-LABEL', 'SIBLING']);
  }],
  ['nonfinite composite geometry and zero scale refuse', () => {
    const { canvas, svg } = canvasWith(plainText);
    draw(canvas, 'HUGE', 0, null, [1e308, 120, 1e308, 50]);
    assert.equal(collect(svg, 'text').length, 0);
    const second = canvasWith(plainText);
    second.canvas.state.scale = 0;
    draw(second.canvas, 'ZERO-SCALE');
    assert.equal(collect(second.svg, 'text').length, 0);
  }],
];

let failures = 0;
let validFailures = 0;
const baselineMustPass = new Set([
  'ordinary numeric coordinates render unchanged',
  'valid negative positions and zero rotation render',
  'null rotation uses the original zero-rotation default',
  'large finite font and native SVG center position remain accepted',
  'ordinary multiline text retains each line and its baseline',
  'valid clip coordinates retain original finite clipping geometry',
  'converted rich label retains its native SVG child',
]);
for (const [name, test] of cases) {
  try {
    test();
    console.log('PASS', name);
  } catch (error) {
    failures++;
    // Baseline deliberately retains the historical numeric-string defect.
    // Only independently unchanged valid-input behavior must pass upstream.
    if (baselineMustPass.has(name)) {
      validFailures++;
    }
    console.log('FAIL', name, error.message);
  }
}
// Falsifier: the original implementation must exhibit a defect on the exact
// malformed/overflow inputs; the proposed implementation must clear all tests.
if (baseline) {
  assert.equal(validFailures, 0, 'original valid numeric/rich-label cases regressed');
  assert(failures > 0, 'unfixed upstream unexpectedly passes all defect probes');
} else {
  assert.equal(failures, 0, 'renderer proposal fails a behavioral regression');
}
console.log(`${baseline ? 'BASELINE_RED' : 'CANDIDATE_GREEN'}: ${cases.length - failures}/${cases.length} passed`);

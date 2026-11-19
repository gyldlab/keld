# GH-1404 Draw.io renderer: isolated source proposal

**Status: candidate only — production integration is blocked.** This directory
contains a deterministic proposal for a source file in the separate draw.io
application. Neither its bytes nor its tests are an installed-app claim. Do
not patch the installed V3 application, substitute a provider input, change the
protected source graph, or infer G0–G12 acceptance from these tests.

## Identity and observed problem

- Original webapp: `jgraph/drawio` commit
  `24b76c2cbd55d88e354042e8d329a2e4708972bc`.
- Original source: `src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js`;
  SHA-256 `0daef9812e3b8c530bc9556791ffaea53fe9a57091a86534bd909ca31e4485b3`;
  Git blob `8b2ccd5cd1f37fa480a48ce1aea5a4972a515552`.
- Installed V3 observation, `gh1404-macos-v3-loss-aware-closeout-20261120/BLOCKERS.md`:
  two original labels invisible; a retry reported AX bounds
  `(31777,-345,18614,10)` outside a `(360,85,1200,800)` window. The native
  output does not itself isolate the error to this function.
- Prior V7 `make-guard-v2.py` and `stale-svg-falsifier.js` survived in
  `.keld-work/scratch/gh1404-renderer-worker1-20261120`; their referenced
  `compiled-v7` source and runner are missing. Their logs show missing
  files. Historical assertions cannot be promoted to current production proof.

## Code path and independent test atoms

| Owner / boundary | Source and observable | Limit |
| --- | --- | --- |
| Graph import | `mxCodec.decode` through `Editor.setGraphXml`; the graph owns cell value and geometry | No malformed imported file has been proven to cause the observed off-screen label |
| Label model → canvas | `mxText.paint` calls `mxSvgCanvas2D.text/updateText` | Ordinary numeric positions and converted SVG text remain usable |
| Native SVG text | `mxSvgCanvas2D.plainText` uses coordinates, rotation, text node and SVG attributes | Invalid numeric/combined geometry must produce no live text or forged transform; valid reuse restores the original child |
| HTML/foreignObject text | `mxSvgCanvas2D.updateTextNodes` and `createCss` | **Outside this candidate.** Full HTML and foreignObject behavior remains unverified |
| Save and reopen | `mxGraphModel.setValue` → `Editor.getGraphXml` → `EditorUi.getFileData/saveData` | The V3 `-EDIT-OK` XML result followed unreliable automated keyboard input; it does not establish a model/serializer defect |

The source-level test uses a small fake SVG tree to check the native SVG
`plainText` contract. It cannot prove WebKit, application import, HTML
layout, an editor keystroke, saved file integrity, or installed G1/G3/G4.
The `NEW-LABEL` test models a full shape clear/repaint. Its route was
independently checked in the pinned `mxText.redraw` and
`mxShape.redraw/clear` sources, but those caller functions are not
executed by this test. Direct in-place `plainText` reuse is a same-value
geometry update and cannot be used to change the label text.
The test also exercises native SVG text converted from HTML in a single
node, but it is not a full rich-label/foreignObject compatibility test.

## Proposed behavior and explicit open decisions

The inert `make_patch.py` reads only the exact upstream source SHA and emits
its candidate source or unified patch. It keeps finite numeric and numeric
string inputs (including negative coordinates) and guards non-finite inputs,
zero/negative scale, non-finite combined positions and malformed rotations.
When an invalid update targets an existing SVG group, **all** of its children
(including any title/background or custom children) are held in a detached
document fragment and its stale transform/clip/opacity is removed. The next
valid update restores the children. Upstream's `mxText` paint/update caller
passes groups it owns, but the public canvas method also accepts an explicit
node; that wider ownership case needs approval and independent verification.
A valid
zero-rotation/full-alpha update also clears stale transform/opacity.

Approval decisions still owned by the user/product/source owner:

1. **Source-input authority.** Existing GH-499/GH-1404 approval preserves the
   original renderer and 3,339 webapp blob identities. Approve an explicit
   renderer amendment and independent new source/provider/graph hash
   qualification before any compiled app consumes altered source.
2. **Malformed geometry behavior.** Confirm that hiding only invalid
   entire reused SVG group, while allowing restoration on a
   healthy update, is the desired policy. Custom descendants within a passed
   group would also be hidden. This changes behavior for
   non-numeric objects, blank numeric strings and invalid geometry; the
   existing product contract does not authorize that rejection policy.
3. **Remaining renderer coverage.** Approve and qualify the complete HTML
   `foreignObject`/SVG block/wrap/update path separately. This proposal
   covers `plainText` only and must not be labelled a complete G1 fix.
4. **Installed oracle.** Verify one real editable label before typing;
   record exact `GH1404-EDIT-OK` in the editor and disk XML; reopen and
   compare; use separate disposable files for Cancel/Discard. The native
   device/GUI owner performs these checks after a qualified separate build.

No public KELD API, wire, permission, dependency, framework Rust or
process-lifecycle boundary changes are proposed. A product renderer source
change and source graph requalification are still review gates.

## Reproduce source-level result

Use the pinned upstream source and inspect the deterministic diff:

```sh
gh api 'repos/jgraph/drawio/contents/src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js?ref=24b76c2cbd55d88e354042e8d329a2e4708972bc' --jq '.content' |
  base64 --decode |
  python3 tools/gh1404_renderer_proposal/make_patch.py --patch
```

Run the original regression with `--baseline` and the proposed behavior
without it. Both require the same pinned SHA-256 source input:

```sh
gh api 'repos/jgraph/drawio/contents/src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js?ref=24b76c2cbd55d88e354042e8d329a2e4708972bc' --jq '.content' |
  base64 --decode |
  node tools/gh1404_renderer_proposal/render_probe.cjs /dev/stdin --baseline

gh api 'repos/jgraph/drawio/contents/src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js?ref=24b76c2cbd55d88e354042e8d329a2e4708972bc' --jq '.content' |
  base64 --decode |
  python3 tools/gh1404_renderer_proposal/make_patch.py |
  node tools/gh1404_renderer_proposal/render_probe.cjs /dev/stdin
```

Baseline is required to fail at least one defect case while preserving
the ordinary numeric/rich-label cases. Candidate must pass every case.
The guard itself is the single-fault control: remove it, and the malformed
fresh and reused SVG cases must become failures again.

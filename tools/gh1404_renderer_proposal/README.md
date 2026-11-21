# GH-1404 Draw.io renderer: isolated source proposal

**Status: real production-source patch prepared; qualified app integration is blocked.**
The historical 33-case evidence below describes the earlier proposal at
`b8da958c`. The current two-file G1 source patch and fresh regression receipts
are in [G1 production-source handoff](#g1-production-source-handoff).
Neither its bytes nor its tests are an installed-app claim. Do
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
- The read-only original acceptance fixture
  `.keld-work/reports/gh1404-w6-acceptance-20261119/fixtures/baseline.drawio`
  has **ordinary finite geometry**: two `html=1` cells at
  `(100,110,320,80)` and `(100,260,320,80)`. The installed V3 renderer
  retained the pristine upstream `mxSvgCanvas2D.js` input. Those cells take
  the `foreignObject` branch because no `convertToSvg=1` is selected.
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
| HTML/foreignObject text | `mxSvgCanvas2D.updateTextNodes` and `createCss` | Source-level finite numeric/derived CSS geometry, rotation, zoom, existing DOM and wrapping tested; native WebKit layout and actual imported original cells remain unverified |
| Save and reopen | `mxGraphModel.setValue` → `Editor.getGraphXml` → `EditorUi.getFileData/saveData` | The V3 `-EDIT-OK` XML result followed unreliable automated keyboard input; it does not establish a model/serializer defect |

The source-level test runs the actual pinned `plainText`, `updateTextNodes`
and `createCss` function bodies against a small fake SVG/HTML tree. It does
not run full `mxSvgCanvas2D.text` dispatch, `addForeignObject`, `mxText.paint`,
CSS layout, native WebKit, app import, an editor keystroke, disk serialization,
or installed G1/G3/G4. Assertions about DOM attributes and label presence
cannot establish pixel visibility or accessibility bounds.
The `NEW-LABEL` test models a full shape clear/repaint. Its route was
independently checked in the pinned `mxText.redraw` and
`mxShape.redraw/clear` sources, but those caller functions are not
executed by this test. Direct in-place `plainText` reuse is a same-value
geometry update and cannot be used to change the label text.
The test exercises a converted SVG rich-text child and a separate
`foreignObject` layout path. It does not run actual HTML-to-SVG conversion
or prove complete rich-text/export parity.

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
A valid zero-rotation/full-alpha update also clears stale transform/opacity.
The same generated patch guards the `foreignObject` layout before its
`createCss` callback: it requires finite coordinates, scaled dimensions,
derived offsets/rotation and zoom percentages. Numeric strings are converted
before arithmetic so rotation `'20' + '30'` cannot silently become `'2030'`.
An invalid reused HTML group is hidden with `display=none`, keeping its
descendants intact for a later valid update; the exact previous display
attribute, including an intentionally hidden label, is restored. These are
proposed error semantics and remain subject to owner approval.

### Follow-up: actual finite-input overflow in the first candidate

The initial source-only commit `10e279957ce30078e42deab66eebcd726a223bd4`
checked that individual numbers were finite but did not check *all computed*
native SVG outputs. On the pinned upstream source with its first candidate
guard, `x=y=w=h=0`, `scale=2`, `fontSize=1e308` emitted a live
`font-size="Infinitypx"`. A separate centered example emitted
`y="Infinity"`, real two-line text emitted non-finite Y values, and
finite individual inputs could overflow after adding `textOffset` or
computing a clip anchor. The first candidate passes **13 of the 19** source
tests, with six derived-overflow regressions failing. Original pinned
upstream passes **7/19**, and the earlier hardened inert candidate passes
**19/19**, on that earlier test revision.

The revision calculates line height, text height, and initial center/bottom
baseline before touching the SVG node; the renderer reuses those same
computed values for actual drawing. It also checks the scaled font size,
every line's bounded endpoint (including the last line), rendered offsets,
clip rectangle and the block-overflow shift where the content width is
already known. This prevents the tested non-finite attributes. Ordinary
multiline text, large but finite fonts, numeric strings, original clipping
and existing rich native SVG children still pass.

The guard does not compute browser-derived `getBBox()` width in
`adjustBlockTextOverflow`; full HTML layout, converted/wrapped SVG exports,
text backgrounds and all other export variants remain unproved.

### 🔶 MEDIUM · correctness · issue (blocking): HTML line-height can overflow after finite-input checks

- **Problem:** On source-proposal commit `49d898d5a2238fe613b3b772aaa31c5c000d3c9f`,
  the HTML guard accepts finite values that make `getTextCss` emit
  `line-height: Infinity` or `Infinitypx`. The invalid text stays in a
  visible `foreignObject` group. This overflow exists in the pinned upstream
  calculation; the proposed new guard claimed broader derived CSS protection
  than it established.
- **📎 Evidence (reproduced):** The real pinned `mxSvgCanvas2D.getTextCss`
  computes `LINE_HEIGHT * lineHeightCorrection` in relative mode and
  `fontSize * LINE_HEIGHT` in absolute mode. With an original-cell layout
  `html=1`, `x=100`, `y=110`, `w=320`, `h=80`,
  `lineHeightCorrection=Number.MAX_VALUE` emits `Infinity` while
  `fontSize=Number.MAX_VALUE`, `scale=0.5` emits `Infinitypx`, even
  though `fontSize * scale` is finite. The prior committed generator fails
  these two exact source probes.
- **🛠 Change:** The generated `updateTextNodes` guard computes the line
  height selected by `mxConstants.ABSOLUTE_LINE_HEIGHT` and refuses non-finite
  results before `getTextCss` or SVG/HTML node updates. It retains the
  existing invalid-group hiding and valid-update restoration contract.
- **🧪 Check (reproduced):** Actual pinned `getTextCss` yields `1.2` for
  normal relative `lineHeightCorrection=1` and `19.2px` for normal
  absolute `fontSize=16`. The former candidate fails the two MAX cases.
  The corrected candidate passes both normal controls and both overflow
  rejection/recovery controls. Removing only `isFinite(lineHeight)` makes
  those two overflow checks fail again. This is a source-level check, not a
  native WebKit visual or installed G1 result.

The installed G1 labels' finite off-screen AX bounds
`(31777,-345,18614,10)` are a **different unresolved geometry problem**.
The actual two original `html=1` cells have ordinary coordinates. Potential
failure boundaries include WKWebView `foreignObject` percentage sizing and
CSS coordinate mapping, root transforms, `EditorUi.getPagePadding`,
`extendCanvas` and `resetScrollbars`. No one of these is yet established
as the cause. **The source guard is not a verified fix for installed G1.**
The necessary independent native falsifier compares `html=1` and `html=0`
with identical bounded geometry and records SVG/FO element bounds,
`getBoundingClientRect()`, computed CSS, graph scale/translation, native
screen visibility and AX coordinates before and after Reset View. That test
requires an approved, separately qualified source/graph build; do not
mutate the installed V3.
G3's `-EDIT-OK` save followed unverified keyboard input; it is not proof
that the serializer changes the cell value incorrectly.

### Source-only verification receipt (20 November 2026)

The source input, prior first candidate and revised two-path candidate are
reproducible using the commands below; no local installed app was used.
The prior preserved failure logs retain actual `Infinitypx`, `Infinity`
and `NaN` SVG attributes observed from the pinned function. Those logs
refer to the earlier 19-case run and must not be used as 33-case evidence.

| Input | Source-level oracle | Shell exit |
| --- | --- | --- |
| Upstream pinned Draw.io (current tests) | **13/33** passed; 20 defect checks fail, ordinary SVG and HTML, including real line-height controls, pass | 0 (`--baseline` expects RED) |
| First candidate at `10e279957` (earlier tests) | **13/19** passed; six derived-overflow tests failed | 1 |
| Hardened native SVG-only at `2bc8336c8` (earlier tests) | **19/19** passed, native path only | 0 |
| SVG + HTML proposal at `49d898d5` (current tests) | **31/33** passed; both real CSS line-height MAX cases fail | 1 |
| Corrected inert SVG + HTML proposal (current tests) | **33/33** passed; includes real `getTextCss` normal and overflow controls for both CSS branches | 0 |

- Original `mxSvgCanvas2D.js` SHA-256:
  `0daef9812e3b8c530bc9556791ffaea53fe9a57091a86534bd909ca31e4485b3`.
- Corrected unified source patch SHA-256:
  `77d0e7fec237cb4ea92c368829013c6082ab406c2cf175f02e24b8523aae490d`.
- Corrected candidate JavaScript SHA-256:
  `d6857eb3f3d371c07dc64352e00af099917691bd8710ea702ff06e23f483bfb7`.
- `evidence/upstream-baseline-red.log` SHA-256:
  `efa0e733b9e6fa61a38f485950f436fe4fb29d87c8381767f65f732e4436e19f`.
- `evidence/first-candidate-red.log` SHA-256:
  `f432796a37ebfa3f5877bc4bc04c7eb4b1cec3f2328dc97b9aa1be662a6e7157`.
- `evidence/hardened-candidate-green.log` SHA-256:
  `421597548cfde528cd0176ad58b7fd1ff6429883068199b5518946fb027509e4`.

The revised patch passed `git apply --check --whitespace=error`, was
applied and reversed against an independent temporary copy, and recovered
the original bytes exactly. Its source input is SHA-pinned; the earlier
one-byte source negative control caused deterministic digest refusal.
The generated JavaScript passed `node --check`. Disabling the HTML
invalid-geometry guard in-memory makes **five current regressions fail**:
malformed rotation, stale reused label, preservation of prior display,
derived coordinate overflow and overflowing zoom percentage.
Removing only the computed `isFinite(lineHeight)` check makes both MAX
CSS cases fail, with the normal relative and absolute controls still passing.
No installed-WebKit acceptance is implied.

Approval decisions still owned by the user/product/source owner:

1. **Source-input authority.** Existing GH-499/GH-1404 approval preserves the
   original renderer and 3,339 webapp blob identities. Approve an explicit
   renderer amendment and independent new source/provider/graph hash
   qualification before any compiled app consumes altered source.
2. **Malformed geometry behavior.** Confirm native SVG child parking and
   temporary whole-group hiding on invalid HTML geometry with exact display
   restoration. Custom descendants in the passed SVG group are also parked,
   and unsupported non-numeric/blank inputs now reject. This policy is not
   authorized by the existing source contract.
3. **Finite G1 and export parity.** Independently diagnose actual original
   HTML labels' off-screen finite bounds with DOM/CSS/graph/AX observations;
   qualify the complete `foreignObject`, HTML-to-SVG, wrap, export and
   text-background path. The current proposal is insufficient to mark G1
   fixed or the entire rich renderer compatible.
4. **Installed edit/save/reopen oracle.** Verify one real editable label before typing;
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
Each path has independent defect cases. Disabling only the HTML invalid-input
guard makes five current HTML regressions fail while the other paths remain
unchanged. Existing native SVG invalid-geometry and stale-node tests also
fail on the pinned original source.

## G1 production-source handoff

The original G1 acceptance cells are ordinary wrapped `html=1` labels at
`(100,110,320,80)` and `(100,260,320,80)`. The assigned V3 exposed both
labels at out-of-window AX bounds `(31777,-345,18614,10)`. The source
trace found an independent feedback route:

1. `mxSvgCanvas2D.updateTextNodes` assigns viewport-relative percentages to
   the `foreignObject` dimensions. A wrapped label lives in an SVG whose
   `min-width` changes with graph bounds.
2. `mxText.updateBoundingBox` feeds the HTML node's measured
   `offsetWidth` directly into the graph's text bounding box, even when the
   label has a fixed wrapping width.
3. `mxGraphView.validate` includes that text box in `graphBounds`;
   `mxGraph.sizeDidChange` and `EditorUi.initCanvas` use these bounds for
   SVG minimum size, page layout and view translation.

The new production patch uses bounded pixel `foreignObject` dimensions for
**wrapped horizontal labels with positive width**, derived from their own SVG
coordinates. Unwrapped and vertical labels retain the original percentage
overflow viewport. It caps only the same positive-width horizontal wrapped
HTML DOM measurements entering graph bounds, at the authored label width plus
the original text-width padding. A zero-width horizontal label and a vertical
label retain their natural measured width: `mxSvgCanvas2D.createCss` gates
horizontal wrapping on positive width, but vertical wrapping on height.

`mxSvgCanvas2D.addForeignObject` can insert a `<title>` before the
`<foreignObject>`. Both `mxSvgCanvas2D.updateText` dispatch and
`mxText.updateBoundingBox` now skip this optional first title and follow the
same HTML path as an untitled label. Native SVG nodes still use the existing
`plainText`/`getBBox` path, and the title remains attached. Focused source
regressions preserve real text measurement below the cap, non-wrapped
overflow, `overflow=width`, rich children, 90-degree rotation, invalid-number
rejection and stale-text recovery.

The upstream application sources are pinned at
`jgraph/drawio@24b76c2cbd55d88e354042e8d329a2e4708972bc`.
The two source files and accepted hashes are:

| Upstream application path | Pinned SHA-256 | Current candidate SHA-256 |
| --- | --- | --- |
| `src/main/webapp/mxgraph/src/util/mxSvgCanvas2D.js` | `0daef9812e3b8c530bc9556791ffaea53fe9a57091a86534bd909ca31e4485b3` | `838862ae4d8f8af0a0dc1d4acb1f9fa7613afbab23419e8cbacf7cb61c0328b3` |
| `src/main/webapp/mxgraph/src/shape/mxText.js` | `a6c0f2c748403a74b11405c864a8278bfcdad216f80ea5b290c1e917d13f9772` | `c67a67cc2d0ae435baee216ead31c672236cff79eec38cd84de8d8bf6138658e` |

The combined `gh1404-g1-production.patch` SHA-256 is
`d31486a5d7d1cbcfaec2aedd9309bb3d44e1b0efc06d11fb915164a98eb457e5`.
This is an application-source patch with exact original paths and a strictly
verified separate source overlay. It can be handed to the owner of a **new**
qualified Draw.io application build. It must not replace the signed assigned V3
resource tree or bypass source-graph qualification.

```sh
python3 tools/gh1404_renderer_proposal/stage_production_source.py \
  --source-root /path/to/pristine/drawio-source \
  --output-root /path/to/new/gh1404-g1-source-overlay
git -C /path/to/pristine/drawio-source apply --check --whitespace=error \
  /path/to/new/gh1404-g1-source-overlay/gh1404-g1-production.patch
GH1404_PINNED_SOURCE_ROOT=/path/to/pristine/drawio-source \
  python3 -B tools/gh1404_renderer_proposal/test_stage_production_source.py
```

The overlay contains the two full corrected JavaScript files at their original
application paths, the combined source patch and an input/output hash manifest.
The tool refuses unexpected upstream digests, an already-existing output, and
an output path inside any `.app` bundle, including a symlink alias to one.
It pins the existing destination ancestor through POSIX no-follow directory
handles **before** the four source generators run. Missing intermediate parents
and the final output directory are created relative to those handles; all
subdirectories and files use directory-relative no-follow/exclusive operations.
An output parent renamed and replaced by a symlink during source verification
causes a refusal before output creation. A symlink substituted immediately
after output or source-directory creation is also refused. If the pathname
changes after opening the output directory, writes stay on the original
directory handle and the final output is marked unqualified.
Before printing success, the tool reopens every advertised nested directory
and output file using no-follow directory handles. It requires each name to
retain its captured device/inode identity and every file to match its original
length and SHA-256. A name rebound after source writing therefore refuses the
overlay even when all writes remained within the original directory handles.
Verification opens output files nonblocking and rejects non-regular inodes,
including a FIFO swapped into the advertised JavaScript path, without waiting
for a pipe writer. Directory-path FIFO replacements are refused by the
directory-only open.
The source manifest is written only after the two candidate JavaScript files
and patch pass their first exact-inode/content verification and the advertised
output-root path still names the pinned descriptor. A real subprocess
`SIGKILL` before that verification leaves an incomplete overlay with **no**
manifest and no success stdout; a new attempt must use a different output
path. This is producer-crash staging evidence, **not** a KELD Host-death or
installed-app recovery test. A manifest alone cannot authorize consumption:
the separate source/build owner still needs the producer's successful exit,
fresh output identity verification and final approved build-input custody.
These checks are point-in-time evidence: a separate process running under the
**same UID** with write permission can still mutate paths after verification,
or physically move an already-open directory inode into an application bundle.
The latter could put staged bytes inside the bundle before the CLI refuses;
the staging tool does not provide containment against that actor. Only run it
in an isolated task-owned staging folder, and require new source/build-owner
verification at the consumption boundary. It does not modify its input
checkout, compile the renderer or install an app.
It does not carry the original application's `drawio/LICENSE` or other graph
resources: the separately qualified original application build must preserve
those source inputs and record their hashes before any installed test.

### Additional G1 source and crash custody (21 November 2026)

The previous source-stage parent edc40a48ea8d12da02af90469ea8ac2f3d96fdd4
accepted a foreign source-path substitution between its pathname validation and
the subsequent read. It also created the final manifest name before the JSON
file had been fully written. Independent review reproduced both exposures.

The new source-stage correction opens the two original files using retained
read-only, no-follow, nonblocking file descriptors beneath a pinned no-follow
source root. Each leaf must be a single-link, regular file on the same root
filesystem, must match its hard-coded pristine upstream SHA-256, and must retain
the original device, inode, size, mode, link count and timestamps. The generator
uses only buffered bytes read through those handles. Source-root identity,
intermediate directory identities, original file names and contents are
reverified before and after publishing output. Hardlinks, swapped symlinks,
changed originals and source-directory or source-root replacement refuse.

For a completed output, the tool fsyncs staged source files and directories,
writes the full manifest to a new random temporary name using an exclusive
no-follow open, fsyncs that complete file, and atomically links the finished
inode into the final manifest name without replacement under the pinned
directory handle. The directory is fsynced before and after removing the
temporary link. A real SIGKILL during the manifest write leaves only a partial
temporary file; the final manifest name does not exist. A preexisting final
symlink cannot be overwritten or followed. An aborted directory cannot be
reused as a verified stage even if some source files were written.

Six adversarial staging cases were added to the original 18, including
hardlink rejection, a source swap immediately before opening the source file,
source file and source-root rebinding after generation, real SIGKILL during
temporary manifest writing, and refusal of a competing final manifest name.
All six demonstrated exact RED on the parent; the corrected real pinned
source suite passes 24/24. The real staged outputs remain byte-for-byte the
same: SVG probe 40/40 and bounds probe 13/13, with unchanged candidate source
and patch SHA-256 values. No installed application or signed release is used.

These output files do NOT update the normal draw.io compiled application:
the pinned upstream Ant app target compiles prebuilt mxgraph/mxClient.js into
client.min.js and app.min.js, rather than compiling the two raw G1 source
files. Original mxGraph precompile reconstruction, full-source/legal inputs,
source-owner approval, signed build, and installed G0-G12 acceptance are
separate mandatory gates. Same-UID late modification remains a consumption
boundary hazard even after an atomic manifest was fully published.

### Current focused evidence (20 November 2026)

| Test oracle | Pinned original | Patched original-source functions |
| --- | --- | --- |
| `render_probe.cjs` (SVG/HTML geometry, wrap, clip, zoom, title-first update dispatch, rotation, reused DOM and malformed inputs) | RED: 16/40 | GREEN: 40/40 |
| `bounds_probe.cjs` (real `mxText.updateBoundingBox` and `getActualTextDirection`, title-first HTML, oversized width, page layout, zero-width, vertical direction, 90-degree rotation and native SVG fallback) | RED: 7/13 | GREEN: 13/13 |
| Fault controls | Old wrapped-bounds cap fails zero-width and vertical controls; original title-first detection falls back to oversized `getBBox` | All five independent in-memory mutations fail their matching source tests: restore zero-width clamp, remove vertical exclusion, remove bounding-box title skip, invert width clamp, or remove `updateText` title dispatch |
| Source staging | Modified one byte in `mxText.js` gives deterministic SHA-256 refusal and no output directory | Combined patch passes `git apply --check --whitespace=error`; both generated JS files pass `node --check` |

An independent real `WKWebView` 900-by-600 fixture checked six SVG
minimum-size updates with original percentage and fixed-pixel `foreignObject`
extents. Both were stable and the label was visible. Thus **percentage sizing
alone has not been proven to cause the installed G1 failure**; these focused
checks prove bounded renderer output and bounded graph layout, not native
original-app label visibility. The required full original application build,
renderer/HTML export parity and installed G0–G12 acceptance remain open.

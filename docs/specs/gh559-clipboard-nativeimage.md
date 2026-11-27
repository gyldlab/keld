# Spec: Guarded clipboard promises and synchronous PNG NativeImage values
Status: draft
Linear: GH-559 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Implement the pinned Electron clipboard promise shapes and the synchronous PNG NativeImage subset used by draw.io. Clipboard
effects pass the existing Host guard; NativeImage conversion produces a validated local value with a pixel/dimension oracle.
Each clipboard batch is one OS commit.

Non-goals: No synchronous Electron 43 clipboard compatibility, clipboard.read/has/clear/selection, arbitrary MIME formats, file references,
raw OS formats, bookmarks, general NativeImage member family or new renderer authority. Original-app activation remains
scheduled by its existing product owner.

## 2. Spec refs

- [Owning issue](https://github.com/gyldlab/keld/issues/559)
- [Architecture 03 system literals](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/03-security.md)
- [Architecture 05 native ownership](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md)
- [Existing blocking transport](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh527-worker-owned-blocking-call-transport.md)
- [Clipboard contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/clipboard.md)
- [ClipboardItem payload contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/clipboard-item.md)
- [NativeImage contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/native-image.md)
- [NativeImage implementation](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/common/api/electron_api_native_image.cc)
- [Actual draw.io clipboard caller](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js)

Verified source and limits: The pinned draw.io clipboardAction awaits writeText/readText and constructs one ClipboardItem containing a PNG Blob and an HTML
string. NativeImage creation and toPNG occur synchronously before clipboard.write. Electron's pinned docs explicitly return
promises for clipboard methods and accept string/Blob/promise item payloads. Its CreateFromDataURL parses the MIME and routes
PNG through an image decoder; raw acceptance of corrupt bytes is not equivalent. Main exports neither clipboard nor nativeImage.

All proposed API, schema, ownership and permission changes below are future contracts.
This documentation PR changes no runtime boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given separate clipboard.read and clipboard.write grants, when writeText resolves and readText follows, then a
Unicode/empty-string fixture round-trips through the actual OS pasteboard and both methods return promises.

Independent negative control: Return a synchronous string or a facade-local cached last write; promise-shape and external-writer read controls fail
independently.

### AC2

Given only clipboard.write, when readText is called, then a typed KELD-GUARD denial occurs with zero native read; writeText
still works. The read-only inverse rejects writes before native commit.

Independent negative control: Use one combined clipboard grant or check permission after native access; the independent native access/commit counter fails.

### AC3

Given one ClipboardItem with image/png Blob plus text/html string, when write resolves, then a native reader sees both
representations from that same complete write.

Independent negative control: Commit PNG and HTML separately; a controlled native observer between commits must expose partial content and fail.

### AC4

Given a later invalid representation, unsupported MIME, rejected payload promise or over-budget batch, when write is attempted,
then no part of that batch changes a previously seeded clipboard.

Independent negative control: Write the first valid representation before validating the remaining batch; sentinel clipboard contents change and fail.

### AC5

Given declared string, Blob and resolving-promise payloads for the admitted MIME set, when ClipboardItem.types/getType and
clipboard.write consume them, then keys and bytes match the reference shape; missing getType rejects. Awaiting payloads happens
before Host native admission.

Independent negative control: Stringify a Blob or promise instead of obtaining bytes, or discard text/html strings; byte/content assertions fail.

### AC6

Given a valid PNG data URL with known dimensions, transparency and pixels, when createFromDataURL and toPNG are called, then
both return synchronously and a separately decoded output has the reference dimensions and pixels.

Independent negative control: Return the original encoded bytes without decoding/validation; the corrupt-PNG control in AC7 fails independently of a valid
round-trip.

### AC7

Given invalid base64, wrong/unsupported MIME, truncated or corrupt PNG, when the admitted invalid-image policy applies, then
isEmpty is true and toPNG returns a zero-length Buffer. Capacity refusal is a distinct typed error, not a fake empty image.

Independent negative control: Return nonempty input bytes for an invalid PNG or treat max-plus-one resource refusal as valid empty content; exact outcome
assertions fail.

### AC8

Given two toPNG calls and a caller mutation of the first returned Buffer, when the second result is decoded, then the original
image value remains unchanged and separate NativeImage objects do not alias mutable output storage.

Independent negative control: Return the owned internal buffer directly; corrupt the first returned byte and observe the next output fail.

### AC9

Given encoded-size, decoded-dimension, pixel-count and aggregate-retention boundaries, when exact maximum and maximum-plus-one
inputs are admitted, then checked size arithmetic and reservations refuse overflow before excess allocation or OS clipboard
effects.

Independent negative control: Bound only compressed bytes; a small decompression-bomb fixture must cross the independent decoded-allocation census and fail.

### AC10

Given role death or link failure before or after native commit, when settlement occurs, then known pre-admission refusal remains
no-effect, post-commit uncertainty remains committed/unknown, stale completion never targets a successor, and the next valid
operation can proceed after actual retirement.

Independent negative control: Automatically retry a rejected possibly committed write or release retained native work solely on promise rejection;
duplicate-effect/resource assertions fail.

### AC11

Given the unmodified pinned draw.io copy-image action in an approved fixture, when it runs, then the actual OS clipboard
contains pixel-equivalent image and corresponding HTML, and the renderer's awaited result settles through its original IPC
route.

Independent negative control: Replace the caller's nativeImage output with an empty buffer while retaining a success reply; native image/HTML pairing fails.

## 4. Design

### 4.1 API and exact supported data

Expose writeText(text): Promise<void>, readText(): Promise<string>, write(items): Promise<void>, ClipboardItem,
createFromDataURL(png): NativeImage, isEmpty(): boolean and toPNG(): Buffer. The proposed first ClipboardItem MIME set is
exactly text/plain, text/html and image/png. Bind allowed item/representation counts and property validation under T1. Reject
text/uri-list, electron/web custom formats and any other MIME before native commit; their names can confer file or raw-format
behavior and cannot be treated as inert text.

### 4.2 Host authority and atomic effect

Use the existing generated guarded-call/role dispatcher and the single keld-guard system-list owner. clipboard.read and
clipboard.write are distinct architecture literals; their presence in the architecture does not prove a current broker exists.
Validate the authenticated role and immutable policy before native access. Resolve and validate the complete item batch, reserve
its aggregate storage, and submit one native pasteboard commit on its owning thread. A read is a live native observation; no
change-count mirror substitutes for it.

### 4.3 NativeImage selected proposal

Select validated decode-and-encode as the proposed PNG contract, using the existing pinned platform image facilities under
native-owner review and GH527 for the synchronous Host round trip. Preserve a Bun-owned private value after completion; toPNG
returns an owned copy and later unsupported mutators stay typed unsupported. Do not keep a Host handle alive merely to serve a
local toPNG read. Byte identity, ancillary chunks and codec implementation are not the acceptance oracle; pixel/dimension
equivalence and invalid-image behavior are. Alpha/color-space conversion must be fixed by the reference fixture, with a declared
divergence if necessary.

### 4.4 Alternatives and compatibility

Reject blind Bun-side byte pass-through because valid PNG syntax/decoding and corrupt-image behavior remain unproved. A
separately qualified synchronous decoder could replace the proposed Host seam only through the same public/dependency review and
retained oracle. The issue's historical Promise-only Bun.Image observation is version-specific; an asynchronous facility alone
cannot supply a synchronous constructor. Never park on a promise with a second JS pump or new Worker transport. Electron 43 sync
clipboard calls receive an explicit migration diagnostic; no hidden quirk.

### 4.5 Bounds and failure independence

Freeze encoded input/output bytes, UTF-8 text, item count, representation count, decoded width/height/pixels, copy buffers,
native temporaries and per-role/global aggregate limits before approval. Existing frame maxima are transport ceilings, not safe
image-memory budgets. Perform checked multiplication and reservation before decode/allocation; unwind every reservation exactly
once. Capture caller values under the chosen reference semantics. Payload promises can reject or remain pending: bound retained
caller work and admit no native operation until resolution/validation; do not claim to bound arbitrary app-owned objects.

### 4.6 Lifecycle and effects

A queued unstarted native operation may be canceled under the existing owner contract. Once the OS commit occurs, later timeout,
response failure or role retirement does not undo it. Return the exact known/unknown outcome; no automatic replay, rollback
claim or content logging. Fence callback destination by authenticated generation and observe native completion/quiescence
independently of promise settlement. The current role's later read still requires clipboard.read.

### 4.7 Privacy and platforms

Mac first, real pasteboard access only. The issue explicitly leaves programmatic-read privacy alerts unknown; obtain an Apple
primary/API receipt and actual version-qualified fixture before fixing prompt/failure semantics. A Keld grant does not suppress
OS privacy decisions. Windows/Linux, selection clipboard and platform custom formats remain unsupported until separate
contracts. Use disposable seeded clipboard fixtures with explicit restoration in future native acceptance; do not access the
user's clipboard in this drafting session.

### 4.8 Migration

Move canonical request/result schema, guard dispatcher, native owner, API client, Electron facade and mapped conformance cells
together. Preserve draw.io's original ClipboardItem/Blob/string shape instead of adding an application-specific IPC shortcut.
GH545 payload budgets and GH527 blocking/retirement remain predecessor owners; this draft allocates no channel ID, wire tag,
decoder dependency or numeric error.

## 5. Boundaries

Future code belongs to the existing native broker/guard/Core routing and API/Electron compatibility owners. Host UI handles stay
with the qualified native thread. This publication edits only this spec plus issue/PR metadata; no native code, schema, global
settings, application files or other owners' unpublished changes.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Approve the MIME/value table, exact invalid-image and color/alpha oracle, native decoder, privacy-read receipt, aggregate limits
and current GH527/GH545 dependency composition; add source-backed pending conformance entries.

### T2

- [ ] Implement one guarded atomic clipboard batch and live readText with complete payload resolution, native observations and
deny/partial-write controls.

### T3

- [ ] Implement the synchronous validated NativeImage value through the approved shared seam, with copy ownership, corrupt-input and
decompression-budget controls.

### T4

- [ ] Run actual macOS clipboard/native-image/cancellation fixtures and original draw.io copy-image; flip only proved cells and retain
all OS/native gaps.

## 7. Test plan

AC1–AC5 distinguish API shape, independent OS-reader bytes and permission effects. AC6–AC9 use checked-in PNG fixtures and a
separately selected decoder/oracle, including ancillary chunks, alpha, malformed data and compressed-small/decoded-large inputs.
AC10 injects barriers before admission, before commit and after commit; promise rejection alone proves no cleanup. AC11 runs the
real caller and renderer await. Use exact OS/runtime/source pins and restore fixture-owned clipboard contents when the future
real test ends.

Every acceptance criterion has an independently observable outcome and named negative control.
Record source commit, pinned reference, exact platform/runtime and raw receipt for every claimed result.
Documentation validation covers immutable source/readback, ten sections, controls and file limits.
Runtime, native and original-app tests are planned, not executed by this drafting session.
GitHub documentation CI cannot establish platform acceptance. Local `just ci`, hooks and machine
closeout were not run: the user restricted this work to GitHub. Official upstream GitHub sources
were read within that restriction; no Context7 query or external-machine evidence is claimed.

## 8. Review gates triggered

Future public API, permission model, wire protocol and native unsafe review if new sanctioned FFI is required. Decoder/library
additions trigger the dependency gate; none is silently selected by this document. Existing raw renderer clipboard access stays
denied.

Status remains draft. Implementation requires the section 10 decisions and approval of the exact contract.
Draft publication does not approve an upstream dependency or change a parked implementation schedule.

## 9. Perf impact

Measure encoded bytes, native decoded/copy peak, retained NativeImage backing, write/read latency and synchronous conversion
park duration separately. No performance improvement is claimed and frame-size constants do not waive aggregate memory limits.

## 10. Open questions (block implementation)

1. Approve the precise ClipboardItem payload/validation subset and resource budgets, including unresolved/rejected promise
retention.

2. Qualify the synchronous PNG decoder and reference pixel/alpha/color/invalid-input policy; bind native versus Bun value
ownership.

3. Obtain the macOS privacy primary receipt and actual read-prompt/failure disposition.

4. Approve current guard/channel/schema integration and committed-versus-unknown effect settlement before implementation.

Independent review must refute or fix concrete findings on the published revision. Platform gaps
remain explicit; no unexecuted acceptance test is reported as passed.

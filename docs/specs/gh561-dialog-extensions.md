# Spec: Synchronous file dialogs and native error and trust-dialog extensions
Status: draft
Linear: GH-561 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Extend the existing GH478 dialog broker with synchronous open/save result shapes, directory creation controls, startup error
display and a separately authorized certificate-trust dialog. Preserve the current guard, parent identity, cancellation and
native-custody owners.

Non-goals: Do not reimplement GH478 asynchronous dialogs, GH564 message-box sync behavior or GH527 transport. No Strict user-intent path
grants, Mac App Store bookmarks, arbitrary filesystem writes, automatic certificate trust, new modal pump or
application-specific code. The first qualified platform is macOS.

## 2. Spec refs

- [Owning issue](https://github.com/gyldlab/keld/issues/561)
- [Existing broker specification](https://github.com/gyldlab/keld/blob/1063610e163cd97093084a5c1dc47b87939c04a3/docs/specs/gh478-guarded-macos-dialog-broker.md)
- [Active scoped native owner](https://github.com/gyldlab/keld/issues/478#issuecomment-6681875758)
- [Shared blocking transport](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh527-worker-owned-blocking-call-transport.md)
- [Architecture 03](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/03-security.md)
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md)
- [Pinned dialog contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/dialog.md)
- [Pinned facade argument handling](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/dialog.ts)
- [Pinned native sync-dialog binding](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_dialog.cc)
- [Pinned macOS Save cancellation](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/ui/file_dialog_mac.mm)
- [Pinned macOS certificate completion](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/ui/certificate_trust_mac.mm)
- [Zettlr directory caller](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/windows/dialog/ask-directory.ts)
- [Zettlr save caller](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/windows/dialog/save-dialog.ts)

Verified source and limits: Electron's pinned Open cancel result is undefined. For sync Save, docs say empty string, but the same pin's macOS implementation
returns an empty optional on Cancel and its native binding returns no JS value: source implies undefined. This session did not
run that oracle. Do not import the async {canceled,filePath} shape or claim that this docs/source conflict is settled. The macOS
certificate promise resolves in the dialog-did-end callback, while the docs say shown. GH478 already admits openDirectory in its
scoped proposal but leaves createDirectory unsupported; Zettlr uses both through the async API. Existing GH478 native work
remains separately owned.

All proposed API, schema, ownership and permission changes below are future contracts.
This documentation PR changes no runtime boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given an admitted current legacy-profile caller, when showOpenDialogSync selects one or multiple paths, then it returns string[]
synchronously; actual native Cancel returns undefined.

Independent negative control: Return a Promise or {canceled,filePaths}; the immediate return-shape test fails without awaiting it.

### AC2

Given the source-faithful macOS proposal and its separately verified pinned-runtime oracle, when showSaveDialogSync selects a
target, then it returns its path synchronously; native Cancel returns undefined. An approval choosing the documented
empty-string contract must instead record an explicit divergence and amend this criterion. Selection alone does not
create/truncate the target.

Independent negative control: Return an async-shaped object or the unselected alternate cancellation value; exact-value assertions fail. Independently
creating/truncating the selected sentinel before an app write fails the filesystem check.

### AC3

Given openDirectory plus createDirectory on macOS, when the native panel creates and selects a new fixture directory, then the
result is that actual directory and the same option contract works through async and sync entry points.

Independent negative control: Silently discard createDirectory or implement an arbitrary Host mkdir outside the panel; native button/action and resulting path
observations fail.

### AC4

Given openFile plus openDirectory on macOS, when each kind is selected in separate runs and jointly where multiselection is
admitted, then both remain selectable and ordered results match the native selection.

Independent negative control: Copy Windows/Linux directory-precedence semantics into Mac; the ordinary-file arm fails.

### AC5

Given denied dialog authority, wrong role, stale/recycled parent or unsupported Strict path-return mode, when a request reaches
the Host, then it receives a typed refusal before any panel or capability creation; a valid call still works.

Independent negative control: Use a stale public WindowId without its owner generation or bypass the final shared guard; native allocation/parent identity
fails.

### AC6

Given a sync call parked in a real native panel, when it times out, is canceled or its role retires, then GH478's actual panel
retirement/quiescence and GH527 settlement both occur under their separately approved bounds; later callbacks cannot complete a
successor.

Independent negative control: Settle only the caller exception and leave the panel visible; independent native handle/healthy-follow-up controls fail.

### AC7

Given the approved pre-Ready error-display route and a current eligible caller, when showErrorBox(title,content) runs, then a
real modal error is displayed with the supplied values and no application quit, fabricated Ready or startup-policy bypass.

Independent negative control: Refuse solely because public Ready is false, or return void without native presentation; the early-startup fixture fails. A
separate unauthorized caller must still cause zero panel.

### AC8

Given certificate trust is not expressly authorized by the selected contract, when showCertificateTrustDialog is called, then it
is typed unsupported/denied before UI or trust-store mutation. A general dialog grant alone cannot grant certificate
installation.

Independent negative control: Route it as an ordinary message dialog and return successful completion; certificate-specific admission/no-effect assertions
fail.

### AC9

Given the approved macOS certificate-display profile and its pinned-runtime oracle, when an actual native trust dialog is
presented and the user declines, then Promise<void> remains pending until the qualified did-end completion and no certificate
becomes trusted.

Independent negative control: Resolve when merely queued/shown, or treat native dismissal as trust consent; the pre-dismissal promise barrier and trust-store
observations fail independently.

### AC10

Given the unmodified pinned Zettlr async directory/save helpers and separate sync oracle fixtures, when their supported option
sets run, then real selection/cancel values propagate through the original callers; unsupported cross-platform options receive
an explicit disposition.

Independent negative control: Change the shared broker only for the new sync facade and leave async createDirectory refused; the actual Zettlr directory
action fails.

## 4. Design

### 4.1 One native operation owner

GH478 owns dialog nonce, guarded channel, role/window validation, admission capacity, UI command, original/cancel correlations
and native retirement. GH527 owns the one blocking-call transport. Add thin synchronous result adapters over the same Open/Save
operations; do not add synchronous native duplicates or route through sendSync. Returned paths remain data, not grant tokens.

### 4.2 Exact shapes and options

Open success is string[] and Cancel undefined. For Save, the source-backed macOS proposal is string | undefined:
file_dialog_mac.mm returns {} on Cancel, and electron_api_dialog.cc calls args->Return only for a present path. Pinned dialog.md
instead says empty string. Preserve both references; T1 must run the actual pinned build and bind the selected
contract/divergence before implementation approval. Never normalize the conflict silently. Preserve omission versus explicit
undefined, defaults, filters and array order through the facade oracle. Add createDirectory through the shared parser/native
property, preserving already-admitted openDirectory and combined openFile behavior. Selection is not permission to
create/truncate a save file.

### 4.3 Sync return and cancellation

Use callBlocking only where GH527 is qualified; source facts apply before return, user callbacks after the parked continuation.
Timeout/abandon is not native cancellation. Consume GH478's existing Cancel-on-new-correlation/tombstone/physical-retirement
design and its separately approved bounds. Original reply, Cancel acknowledgement, native quiescence and healthy-next-call are
four distinct observables; none substitutes for another. No second UI loop, sleep synchronization or retry of possibly executed
work.

### 4.4 Early showErrorBox

Electron documents availability before app Ready. Existing normal broker readiness alone cannot satisfy this. The
Core/GH446/native owners must approve one early authenticated route using captured startup policy and the same native
error-display owner. State the earliest admissible boot phase, no-window behavior, return/blocking semantics and cleanup under
startup failure before enabling it. No unauthenticated emergency channel or global guard bypass. Preserve any existing lifecycle
do-not-quit handler contract; this API reports an error, it does not decide process termination.

### 4.5 Certificate trust is a separate authority

A trust dialog can offer certificate import/trust and is materially different from ordinary message display. Preserve typed
unsupported until the security/native owners approve the exact consent, certificate validation, allowed store/scope and request
admission, including whether a new capability is needed. Do not allocate a permission here or silently reuse dialog. On future
qualification, the Host invokes native UI; app-supplied certificate data never directly changes trust. Resolve only at the
source-qualified milestone and do not equate showing/closing a dialog with successful trust installation.

### 4.6 OS and source distinctions

Mac first. Electron labels createDirectory as macOS; Windows/Linux mixed-file/directory behavior differs. The certificate
method's macOS implementation resolves its promise in panelDidEnd, after native dismissal; this is distinct from the docs' shown
wording. Prefer that source-faithful completion proposal and qualify it with the actual pinned oracle. Windows ignores
message/window options and needs its own receipt. showErrorBox on Linux before Ready is documented stderr-only, a separate lane.
No unsupported platform returns a successful Cancel or empty path as a placeholder.

### 4.7 Migration, memory and reuse

Extend the existing generated option/request families only through their owners; retain current tag/field compatibility and
error allocation rules. Existing request/reply/path/active-operation bounds apply; new certificate bytes and option fields need
explicit aggregate caps before approval. Preserve immutable user input through presentation, capture parent/role at admission,
and release callback/handle custody exactly once. Migrate async/sync facade consumers and conformance cells with the shared
option change.

## 5. Boundaries

Future edits belong to GH478 native/Core/API/Electron owners with separate coordinated implementation claims; GH527, GH531 and
GH446 retain transport, parent and startup authority. Publication touches only this spec and issue/PR metadata. Existing
PR1306/1308 and #478 claim6681875758, private candidates and native validation remain untouched.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Freeze per-method shapes, option/platform table, early error route, certificate-specific security/milestone decision and current
GH478/GH527 dependencies; add pending source-backed tests.

### T2

- [ ] Add sync Open/Save adapters and shared createDirectory native option in one reviewed vertical slice, with separate cancellation
values and unchanged async consumers.

### T3

- [ ] Integrate only approved early ErrorBox and certificate-dialog subsets with existing native lifetime/guard owners; unsupported
operations retain typed refusal.

### T4

- [ ] Run real Mac selection/cancel/modal-retirement and healthy-follow-up fixtures plus original Zettlr helpers; bind exact source/OS
evidence before conformance cells pass.

## 7. Test plan

AC1–AC4 compare immediate values and real panel behavior against the pinned reference. AC5 observes zero native effects on
denied admission. AC6 uses deterministic before/after presentation barriers, actual handle census and a subsequent real call.
AC7 covers the actual pre-Ready phase separately from ordinary post-Ready panels. AC8–AC9 keep display, consent and trust-store
effect independent; use disposable test certificates and stores in future qualification. AC10 runs original async callers to
catch split option implementations.

Every acceptance criterion has an independently observable outcome and named negative control.
Record source commit, pinned reference, exact platform/runtime and raw receipt for every claimed result.
Documentation validation covers immutable source/readback, ten sections, controls and file limits.
Runtime, native and original-app tests are planned, not executed by this drafting session.
GitHub documentation CI cannot establish platform acceptance. Local `just ci`, hooks and machine
closeout were not run: the user restricted this work to GitHub. Official upstream GitHub sources
were read within that restriction; no Context7 query or external-machine evidence is claimed.

## 8. Review gates triggered

Future public API, permission model and wire protocol; GH478/GH527/GH446 owner alignment. Certificate authority requires
explicit security disposition. Any newly needed native FFI needs an exact nearest-owner unsafe allowance and independent review.
No dependency is introduced.

Status remains draft. Implementation requires the section 10 decisions and approval of the exact contract.
Draft publication does not approve an upstream dependency or change a parked implementation schedule.

## 9. Perf impact

Measure synchronous park duration, native operation/cancel latency, aggregate request/reply retention and actual handle
retirement. Reuse existing capacity and deadlines; neither this draft nor a typed caller timeout authorizes widening them.

## 10. Open questions (block implementation)

1. Resolve the pinned docs/source conflict for sync Save cancellation with an actual oracle, then approve the selected exact
value/divergence and current GH478/GH527 profile composition.

2. Approve shared createDirectory options and platform-specific compatibility/error behavior with native observations.

3. Approve the earliest safe authenticated ErrorBox boot phase and its modal return/cleanup contract.

4. Select certificate permission/store/consent policy and byte limits, and qualify the source-backed did-end promise milestone
against the conflicting shown wording, or retain the method unsupported.

Independent review must refute or fix concrete findings on the published revision. Platform gaps
remain explicit; no unexecuted acceptance test is reported as passed.

# Spec: Hidden-window presentation and qualified readiness
Status: draft
Linear: GH-535 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Support macOS hidden creation and an honestly qualified ready-to-show event while preserving Electron's
paintWhenInitiallyHidden:false exception and native visibility truth.

Non-goals: Windows/Linux hidden painting, performance first-paint claims and unqualified transparent-window behavior are separate.

## 2. Spec refs

- [Owning issue GH-535](https://github.com/gyldlab/keld/issues/535) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Window facade](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/browser-window.ts).
- [Managed native windows](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wkwebview/managed_windows.rs).
- [Engine rules](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/AGENTS.md).
- [Electron ready-to-show contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/browser-window.md).
- [Hidden paint decision owner](https://github.com/gyldlab/keld/issues/394).
- [Implementation owner](https://github.com/gyldlab/keld/issues/535).

Verified baseline: Pinned Electron docs say ready-to-show never fires when paintWhenInitiallyHidden is false. The issue's
unconditional hidden-window criterion therefore needs this explicit exception. Keld's engine rules distinguish navigation-finished
from first paint; neither a load callback nor a requested visibility boolean proves compositing.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given show:false, when the native window is built and navigates, then the window server observes it as hidden until an explicit
show.

Independent negative control: Order the window front during construction and then hide it afterward.

### AC2

Given paintWhenInitiallyHidden:false, when a hidden page finishes loading, then ready-to-show is not emitted.

Independent negative control: Emit readiness unconditionally from load-finished.

### AC3

Given the approved hidden-paint source and default hidden painting, when its first valid readiness observation arrives, then
ready-to-show is emitted at most once for that window lifetime.

Independent negative control: Reset the once flag on each navigation.

### AC4

Given a Zettlr-style listener that calls show from ready-to-show, when the qualified event fires, then native visibility changes
through the existing UI owner and is observed, not assumed.

Independent negative control: Update the mirror immediately without applying native show.

### AC5

Given a stale, duplicate or foreign-document reporter event, when it arrives, then it cannot emit readiness for a new
window/document or mint navigation authority.

Independent negative control: Accept a renderer-supplied window or generation field as authoritative.

### AC6

Given a selected load-finished fallback, when evidence is published, then the cell explicitly records its timing divergence and
does not claim first composited frame parity.

Independent negative control: Label a load-finished or double-rAF proxy as measured composited paint.

## 4. Design

### 4.1 Separate readiness from visibility

Hidden construction must avoid ordering the native window front at any intermediate stage. show() delegates to the existing UI
window owner. Native observation drives the mirror. ready-to-show is one readiness notification, not a command to show and not a
navigation-generation mint.

### 4.2 Qualified source gate

GH394 selects the supported mechanism: a constrained reporter or a documented navigation-finished divergence. A double
requestAnimationFrame callback demonstrates rendering opportunities, not a composited frame or physical display. Do not claim
parity or implement an unconditional fake event while that choice remains unqualified.

### 4.3 Conditional lifetime

Capture show and paintWhenInitiallyHidden at construction. The once flag belongs to the host-owned window identity and survives
navigation. It is not a process-global or JS-object-local counter. paintWhenInitiallyHidden:false disables the readiness event.
Caller-initiated show before readiness must not be prohibited; the event need not precede an explicit early show.

### 4.4 Trust and cleanup

A reporter is bound by the existing host-created source/window/document record; payload fields cannot self-authorize. Validate
current lifetime before dispatch. Remove callbacks/scripts with their existing owner on destruction; stale events are harmless. No
bridge capability or user-grant policy is widened to manufacture the event.

### 4.5 Migration and evidence

Add only the chosen source and its consumer together. Keep runtime cells unknown or divergent until native hidden-window evidence
exists. Record the correction to the older unconditional issue text in the implementing handoff; do not silently rewrite
historical observation.

### 4.6 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future implementation spans BrowserWindow capture, existing native visibility owner and the qualified readiness reporter. Do not
alter performance metric definitions or claim other engines share macOS hidden-paint behavior.

This publication changes only `docs/specs/gh535-hidden-window-readiness.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Resolve GH394 with exact source/OS qualification and approve the conditional readiness contract.

### T2

- [ ] Implement hidden native construction and observed show/hide through the existing window owner, with a no-flash negative
control.

### T3

- [ ] Wire the approved readiness source, once-per-window lifetime and false-option exception with stale-source tests.

### T4

- [ ] Run native window-server and original Zettlr-style startup fixtures; publish parity/divergence rows separately from paint
metrics.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-535;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Observe native visibility continuously across construction, not just after a test pause. Use explicit source callbacks and
destruction barriers; cover early explicit show, repeated navigation and false-option suppression. Device/OS-qualified evidence is
mandatory for a hidden-paint claim.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API and GH394 source decision; permission/wire review only for a concrete changed reporter boundary. No implicit approval
to add a trusted bridge capability.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

Any injected reporter may affect startup. Measure it in a separately registered hidden-window workload; keep navigation completion
and compositor/paint claims distinct.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Select and qualify the GH394 source and its exact divergence wording.
2. Approve callback lifetime and authenticated source binding for that mechanism.
3. Confirm native hidden creation causes no temporary visible window before assigning a supported row.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.


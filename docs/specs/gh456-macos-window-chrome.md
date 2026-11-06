# Spec: macOS native window chrome and capability options
Status: draft
Linear: GH-456 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Apply the corpus-used macOS chrome and capability options through the existing UI-owned window builder, and report actual native
state rather than accepted-but-ignored options.

Non-goals: Windows/Linux chrome, transparent windows, TouchBar, JumpList and Thumbar remain separate. Vibrancy/visualEffectState
retain explicit divergence rows until qualified.

## 2. Spec refs

- [Owning issue GH-456](https://github.com/gyldlab/keld/issues/456) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Current window facade](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/browser-window.ts).
- [Native managed windows](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wkwebview/managed_windows.rs).
- [Engine snapshots](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/engine.rs).
- [Electron constructor options](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/structures/base-window-options.md).
- [Chrome owner issue](https://github.com/gyldlab/keld/issues/456).

Verified baseline: Main's facade currently diagnoses backgroundColor/icon as accepted without application. The engine already has
UI-owned WindowBuilt and native state observations. This pack proposes the missing chrome contract; the older issue's statement
that no backend has any chrome option is not used as a current inventory.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given titleBarStyle hiddenInset and a trafficLightPosition, when a native window is built, then introspection reports full-size
content, hidden title visibility and the requested close-button origin in the approved coordinate space.

Independent negative control: Populate readback from the constructor options while removing the native style change.

### AC2

Given backgroundColor, when the window is constructed, then both native window and webview backgrounds are applied before the
first visible frame and read back from their actual owners.

Independent negative control: Apply color only after navigation finish or only to a JS mirror.

### AC3

Given each supported boolean capability set false, when the window is built and exercised, then native style/behavior and its
reported capability agree.

Independent negative control: Ignore closable:false; a native user-close capability probe must expose the mismatch.

### AC4

Given alwaysOnTop and its setter, when the requested level changes, then a UI-owned observation confirms the native level before
the mirror reports success.

Independent negative control: Optimistically update the mirror before the native action can fail.

### AC5

Given a frameless window and invalid/conflicting titlebar options, when validation runs, then the approved precedence or typed
refusal is deterministic and no partially configured window escapes.

Independent negative control: Allow option application order to silently decide conflicting requests.

### AC6

Given the platform matrix, when evidence is published, then each option has its own cited row; transparent remains unsupported and
unqualified Windows/Linux overlays remain unknown.

Independent negative control: Mark titleBarOverlay pass from a macOS screenshot or a constructor echo.

## 4. Design

### 4.1 Capture once, apply on UI owner

The facade validates and captures immutable constructor options before create. Core forwards only the validated schema to the
existing registry/window command owner. NSWindow/WKWebView mutations remain on the UI thread. A construction failure produces the
existing failed-build outcome and releases partial native resources.

### 4.2 Concrete mapping proposal

Use public native style/full-size-content/title visibility/background/window-level/capability controls, with availability pinned
in the implementing source. hidden and hiddenInset are separate observable layouts; frame:false does not imply transparency.
trafficLightPosition uses a documented logical-point content coordinate convention and requires scale/inset roundtrip fixtures.

### 4.3 Coupled capability behavior

resizable, minimizable, maximizable, fullscreenable and closable must map to independently observed native behaviors, not just a
shared style mask assertion. skipTaskbar on macOS requires an explicitly qualified per-window behavior or a documented divergence;
do not silently change application-wide activation policy for one window.

### 4.4 Trust and reuse

Reuse GH531 window identity and build/observation lifecycle. No new window registry or generic native command escape hatch.
Constructor options are untrusted input; reject invalid numbers, colors and unsupported combinations before native construction.
Security preferences remain with their existing strict validation owner.

### 4.5 Migration

Add generated option fields through the existing create schema, together with backend application and readback. Move the current
backgroundColor diagnostic to implemented status only after native evidence. Preserve explicit diagnostics for unsupported
options; do not silently claim an option was applied.

### 4.6 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future changes stay in BrowserWindow option validation, the existing create schema and managed macOS window builder/observations.
Do not rewrite security preferences or alter app-wide policy to fake one window option.

This publication changes only `docs/specs/gh456-macos-window-chrome.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Create per-option pending conformance rows from pinned Electron options and approve conflict/coordinate/skipTaskbar
behavior.

### T2

- [ ] Implement one vertical constructor slice for titlebar/frame/color with native readback and partial-build cleanup controls.

### T3

- [ ] Implement capability and window-level slices through the same owner with setter failure and stale-window controls.

### T4

- [ ] Run real native introspection and original Zettlr/draw.io window fixtures; update only individually proved option rows.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-456;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Use native introspection plus a real interaction where a style flag alone is insufficient. Cover 1x/2x scale,
hidden/hiddenInset/frameless combinations and cleanup on each failed build stage. Screenshots supplement but do not replace native
property and lifetime receipts.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API and generated create-schema review; unsafe review if new sanctioned Objective-C calls are required. No new permission
grant is proposed.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

Constructor-only option capture and UI application; measure first-visible-frame effects without equating nav-finished with paint.
No unmeasured improvement claim.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Approve exact option precedence and coordinate conventions.
2. Qualify per-window skipTaskbar and every public API/OS availability before assigning pass.
3. Approve create-schema fields with the current owner; Windows/Linux remain outside this contract.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.


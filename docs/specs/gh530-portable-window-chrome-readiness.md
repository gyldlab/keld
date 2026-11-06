# Spec: Windows and Linux window chrome and hidden readiness
Status: draft
Linear: GH-530 (#517) · Owner: @0monish (exact contract approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; documentation only, do not merge.

## 1. Goal & non-goals

Specify the residual Windows/Linux window appearance and hidden-readiness contract without duplicating the existing native
create/close adapters. Every accepted option must have an observable native/caller result or an explicit
unsupported/divergence outcome.

No new close registry, Windows/Linux create implementation, GTK4 rewrite, native pass from macOS evidence, arbitrary
host-drawn caption system or first-paint benchmark claim. GH1125 and GH1188 keep lifecycle ownership; GH395 and GH394 keep
chrome tiering and hidden-source decisions.

## 2. Spec refs

- [Owning issue GH-530](https://github.com/gyldlab/keld/issues/530); current public owner decisions remain authoritative.
- [Linux adapter scope and residual boundary](https://github.com/gyldlab/keld/issues/1125).
- [Windows window adapter](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh1188-windows-window-adapter.md).
- [Active Windows managed-window integration](https://github.com/gyldlab/keld/pull/1403).
- [Chrome decision and source gaps](https://github.com/gyldlab/keld/issues/395#issuecomment-6022075165).
- [Hidden-paint experiment owner](https://github.com/gyldlab/keld/issues/394).
- [Shared hidden-readiness correction](https://github.com/gyldlab/keld/blob/d110221dae48df373ea7a57368c3b09a08aedf6d/docs/specs/gh535-hidden-window-readiness.md).
- [Current WebView2 implementation](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/webview2/mod.rs).
- [Current GTK3/Wry implementation](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/webkitgtk/mod.rs).
- [Electron window options](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/structures/base-window-options.md).
- [Electron readiness and overlay method](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/browser-window.md).
- [Real Zettlr chrome consumer](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/windows/set-window-chrome.ts).
- [Real Zettlr theme update](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/appearance/index.ts).

Verified source baseline: Zettlr's pinned window helper chooses frame:false, hidden title bar and overlay
color/symbolColor/height on Windows and non-native Linux appearance; its appearance provider later calls setTitleBarOverlay.
Electron says ready-to-show never fires with paintWhenInitiallyHidden:false. The Linux backend is currently GTK3 through Wry;
a GTK4 destination is not current implementation. The old GH395 packet's SDK/runtime/frameless claims still need current
native qualification before tiering.

The proposed contract below is future work. This publication changes no runtime boundary and grants no implementation or
predecessor approval.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given the approved per-engine option table, when frame/titleBarStyle/titleBarOverlay are requested, then each supported
combination produces its specified native state and unsupported combinations fail honestly before partial success.

Independent negative control: Accept every option into a JS mirror while retaining an unchanged decorated native window.

### AC2

Given a qualified overlay and the original Zettlr theme update, when color/symbolColor/height change, then visible controls
and exposed geometry change through the existing native owner without silently ignoring a field.

Independent negative control: Update only stored option values or only background color while the symbol/geometry observer
remains unchanged.

### AC3

Given native caption, resize and drag controls, when actual pointer/keyboard operations occur, then they target the correct
Host window and preserve existing close veto, focus, maximize/restore and accessibility behavior.

Independent negative control: Add page hit regions that swallow a close-veto route or report drag success without moving the
real window.

### AC4

Given show:false, when the native window and engine controller are created, then the window server/compositor never observes
a visible flash or activation before an explicit show.

Independent negative control: Create visible, paint once and hide; an observer armed before creation must detect the
intermediate exposure.

### AC5

Given paintWhenInitiallyHidden:false, when loading completes, then ready-to-show never fires for that window lifetime.

Independent negative control: Emit readiness unconditionally from native load-finished.

### AC6

Given default hidden painting and the owner-selected source, when a valid hidden readiness observation occurs, then at most
one ready-to-show event is emitted per Host window lifetime and the actual Zettlr-style show listener reveals that window.

Independent negative control: Reset the readiness latch on navigation, fake a callback in the page, or change only the mirror
without native visibility.

### AC7

Given a reporter event from a retired or foreign document, when received, then it cannot emit readiness, select another
window or mint a new navigation identity.

Independent negative control: Trust page-supplied window/generation fields instead of the existing native source binding.

### AC8

Given a selected load-finished or rendering-opportunity fallback, when evidence is published, then the exact timing
divergence is recorded; no composited-frame or benchmark first-paint PASS is inferred.

Independent negative control: Label double requestAnimationFrame or load-finished as a measured composited frame.

### AC9

Given Windows, Linux X11 and Linux Wayland lanes, when support is declared, then each has its own actual native/compositor
input and visibility receipts; an unavailable lane remains unqualified.

Independent negative control: Copy a hosted X11 or macOS result into the Wayland/Windows cell.

## 4. Design

### 4.1 Residual scope and per-concern handoff

GH1125 and GH1188 already own Linux/Windows creation, close veto, registry and cleanup. This draft supplies only residual
appearance/hidden-readiness requirements and is not another lifecycle adapter. Before implementation, GH530's promised
per-concern child trackers must be assigned or existing GH395/GH394 children explicitly adopted. Their decisions cannot be
replaced by this document.

### 4.2 Honest per-engine option table

Freeze frame, hidden title style, overlay enablement, color, symbolColor, height, drag/no-drag regions and runtime changes
separately. Evaluate each combination against the exact SDK/runtime/backend. A platform capability that supports drag does
not prove caption-button or Window Controls Overlay API/CSS parity. If only a subset is possible, approve a typed divergence
with exact supported fields. Do not advertise Tier 1 from a frameless native window alone.

### 4.3 Reuse native owner and controls

Extend the existing native UI-thread adapter and Host-minted window identity. Retain close-veto and observed-state routing.
Any host-owned caption implementation requires its own reviewed accessibility, hit-test, scale and input contract; it is not
selected just because an engine API is absent. A page-provided rectangle is untrusted geometry and must not become native
window identity or arbitrary privileged input. Existing permanent Electron facade remains the API owner.

### 4.4 Hidden window and engine visibility are different

Capture show and paintWhenInitiallyHidden once at construction. The native parent can be hidden while a controller has its
own visibility state; never assume that setting the controller false is equivalent to allowing rendering in a hidden parent.
Qualify the selected Windows controller/parent combination and Linux realized/unmapped behavior on real systems. Keep the
existing no-flash, focus and native handle-release rules during failed construction.

### 4.5 Readiness source and one lifetime latch

GH394 selects a qualified mechanism or an explicit timing divergence for each engine. Reuse the existing trusted
native/document seam; do not create a page-authenticated paint reporter. Double requestAnimationFrame is a
rendering-opportunity observation, not a composited-frame measurement. paintWhenInitiallyHidden:false suppresses
ready-to-show. The one-shot latch belongs to Host window lifetime, survives navigation and is destroyed with the window. An
app may call show before the event; no artificial wait is introduced.

### 4.6 Runtime changes and retirement

Serialize accepted chrome mutations on the UI owner against window destruction. An unavailable or failed native mutation must
not publish a false successful mirror value. Retain callback/registration ownership until real release, and ignore a stale
theme update for a retired window. Recompute approved overlay geometry on actual size/scale changes; native, page CSS and
app-observed geometry must agree under the frozen mapping.

### 4.7 Evidence and version scope

Record exact Windows SDK/runtime, Linux GTK/WebKitGTK/Wry and display protocol, scale/theme, window options and actual caller
bundle. GH395's historical primary receipts are leads for requalification, not current runtime minimums. Separate real
input/visibility results from source models and benchmark instruments. No platform implementation or underlying native
experiment ran in this session.

## 5. Boundaries

Future work belongs in the existing Windows/Linux native adapters, shared captured options/facade rows and GH395/GH394
approved sources. Do not edit macOS owners, shared close state, profile policy or scored first-paint definitions. No new
native dependency or unsafe scope is authorized by this draft.

This publication changes only `docs/specs/gh530-portable-window-chrome-readiness.md` and related grouped-PR/issue handoffs.
Shared architecture, policy, generated contracts, implementations and other owners' documents are not edited.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Resolve residual per-concern tracker ownership, current primary SDK/runtime semantics and GH395/GH394 mechanism/tier choices; freeze a per-engine option/readiness table with pending cells.

### T2

- [ ] Implement a single approved chrome option slice through the existing native adapter and actual Zettlr caller, including native input/geometry and unsupported-combination controls.

### T3

- [ ] Implement hidden creation and the selected one-shot readiness source without flashes or fabricated paint, preserving the paintWhenInitiallyHidden:false arm and all lifecycle hooks.

### T4

- [ ] Qualify Windows, X11 and Wayland independently across scale/theme/input and close/navigation/recovery, then publish supported/divergent/unqualified rows with exact receipts.

## 7. Test plan

AC1/2 compare original caller options and method results with independent native and page geometry observers. AC3 uses real
pointer/keyboard action, a vetoing close listener and visible state changes. AC4 arms the window-server observer before
construction. AC5/6 independently vary hidden-paint policy and explicit show while preserving the same page. AC7 holds an old
reporter across navigation/destruction. AC8 distinguishes source classes. AC9 requires actual compositor/OS evidence; no fake
signal or cross-platform transfer is accepted.

Each acceptance criterion has its own observable and negative control above. Documentation verification covers all ten
sections, source-versus-proposal wording, file limits and exact GitHub readback. GitHub CI belongs to the exact published
head. Native/runtime/conformance/OS outcomes remain unrun until the implementing owner executes them; green documentation
checks do not prove those outcomes.

The user restricted this session to GitHub. Local `just ci`, hooks and machine session-closeout were not run. Official
immutable upstream GitHub sources are used where relevant; Context7 was not used under that source restriction. No
external-machine or native receipt is fabricated.

## 8. Review gates triggered

Public window API and native UI/appearance review. Wire/permission/unsafe/dependency gates are conditional on the selected
implementation and must be declared before changes; existing allowances are not expanded here.

Draft publication is not exact-content approval. Pending predecessor approvals and native qualification remain separate
gates.

## 9. Perf impact

No steady-state polling is selected. Measure script injection and mutation costs only for the approved mechanism;
hidden-readiness evidence is not a first-paint benchmark result. Bound reporter and callback state by existing native window
lifetimes.

## 10. Open questions (block implementation)

1. GH395 must approve the actual Windows/Linux chrome tier and exact field/geometry support after current native
documentation and device evidence.

2. GH394 must approve each engine's hidden source or declared timing divergence; preserve the unconditional
paintWhenInitiallyHidden:false exception.

3. Assign/adopt GH530's per-concern children before implementation and preserve active GH1125/GH1188 claims.

4. Qualify real Windows and Linux X11/Wayland behavior, including controls/accessibility and no-flash creation, without
inferring results from macOS.

Independent review must cover the final published revision. Every remaining owner/native gate stays explicit; this document
is not a completed feature.


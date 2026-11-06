# Spec: macOS application reopen and activate

- Status: draft
- Tracker: GH-521
- Owner: @0monish (specification coordination)
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Deliver one app.activate event for each admitted macOS reopen. Bind its boolean to the authoritative Host window registry. Match first-launch behavior only after a pinned Electron trace.
Exclude window creation by the framework, focus events, did-become-active, Windows/Linux emission and the draw.io open/edit/save acceptance run.

## 2. Spec refs

- FACT: [Architecture 04](../architecture/04-electron-compat.md) sections 3–4 govern the Electron facade and compatibility evidence. [Architecture 02](../architecture/02-ipc.md) governs Host-to-app events.
- FACT: [GH-531](gh531-window-registry-close-state-machine.md) governs window identity. [#449](https://github.com/gyldlab/keld/issues/449) owns registry state. #446 owns Host Ready. #508 owns generated event constants.
- FACT: Electron v44.4.5 [documents activate][docs] with an event and a boolean. It lists first launch as a possible trigger. Retrieved 2026-11-06.
- FACT: The [Electron delegate][delegate] passes the AppKit flag through “Activate” and returns the original value. Its didBecomeActive hook has a different target.
- FACT: [Browser::Activate][browser] notifies observers. [App::OnActivate][emit] emits “activate”. Neither function computes window visibility.
- FACT: The workspace selects tao 0.35.3 at Cargo.lock line 2797. Its [AppKit reopen callback][taodel] invokes AppState::reopen and returns the native flag. [AppState][taostate] sends Event::Reopen. Retrieved 2026-11-06.
- FACT: KELD's macOS UI loop receives tao events in `crates/keld-wv/src/wkwebview/mod.rs:422`. The Core registry already controls window readiness and retirement in `crates/keld-core/src/app_session/window_registry.rs:275`.
- FACT: The [pinned draw.io handler][draw] creates a window when its application array is empty. Its handler does not compute KELD's event argument.

[docs]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/app.md#L144-L154
[delegate]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/mac/electron_application_delegate.mm#L102-L132
[browser]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/browser.cc#L231-L254
[emit]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_app.cc#L612-L626
[taodel]: https://github.com/tauri-apps/tao/blob/5a14e624c81b7a799728129417e9218be25f17d9/src/platform_impl/macos/app_delegate.rs#L206-L216
[taostate]: https://github.com/tauri-apps/tao/blob/5a14e624c81b7a799728129417e9218be25f17d9/src/platform_impl/macos/app_state.rs#L313-L318
[draw]: https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L2126-L2140

## 3. Acceptance criteria

### AC-1: Zero windows and host-originated close

- **Given:** An admitted app stays alive after its last window is closed natively. Its stale facade deliberately still lists the closed window.
- **When:** The operator clicks the running app's Dock icon once.
- **Then:** The admitted link receives one activate event with false. The test app can create one replacement in its listener.
- **Negative control:**
  - **Single fault:** Compute the bit from the stale facade list length.
  - **At-risk precondition:** The Host has committed destruction before the reopen, but the stale list still has one entry.
  - **Expected RED:** The observer receives true after the last accepted window closed.

### AC-2: Real visibility differs from live-window count

- **Given:** The reference fixture can independently show, hide or minimize the sole managed window.
- **When:** Run one genuine reopen for zero, one visible, hidden-only, minimized-only and mixed visible/hidden states on macOS.
- **Then:** Zero yields false. One visible yields true. Check other states against the recorded AppKit/Electron values under B-1.
- **Negative control:**
  - **Single fault:** Treat every extant window record as visible.
  - **At-risk precondition:** The oracle has a state with at least one extant window and a false native flag.
  - **Expected RED:** The process remains live, but the event boolean disagrees with the reference cell.

### AC-3: Preserve event ordering and origin

- **Given:** A live app has an activate listener, a normal focus listener and a native reopen observer.
- **When:** Alternate focus without reopen, then issue two distinct reopens with unchanged visibility.
- **Then:** Pure focus produces zero activate events. Each delivered native reopen produces exactly one activate on its original link in accepted order.
- **Negative control:**
  - **Single fault:** Translate didBecomeActive into activate as well as translating reopen.
  - **At-risk precondition:** A refocus trace changes foreground status without a reopen callback.
  - **Expected RED:** An activate event has no matching native reopen witness.

### AC-4: Cold launch matches the reference

- **Given:** The pinned recorder starts before the packaged reference app runs any module and the app registers listeners at entry.
- **When:** Launch from Finder and the Dock in fresh independent runs. Separately launch the already running app.
- **Then:** Match observed event count, booleans and relative order for will-finish-launching, ready, whenReady continuation, first window and activate per launch path.
- **Negative control:**
  - **Single fault:** Unconditionally inject a synthetic activate immediately before ready.
  - **At-risk precondition:** The reference sample contains no activate at that position.
  - **Expected RED:** Event count or sequence differs. A duplicate window is an additional failure where the fixture creates one.

### AC-5: Freeze the accepted fact and revoke stale delivery

- **Given:** A valid reopen has an associated Host session, app-link generation and a visibility snapshot at the same ordering point.
- **When:** Next, hide or destroy the window before facade delivery. Separately terminate and replace the primary before delivery.
- **Then:** A still-live original link receives the captured boolean. A successor link receives no event from the retired link.
- **Negative control:**
  - **Single fault:** Recompute visibility at delayed JavaScript delivery.
  - **At-risk precondition:** Native visibility changes after the accepted reopen and before listener dispatch.
  - **Expected RED:** The delivered boolean contradicts the accepted event snapshot.

## 4. Design

### D-1: Consume one native source

Reuse the workspace-pinned tao callback. It already owns the Objective-C selector and the synchronous AppKit return value.
The macOS UI loop consumes Event::Reopen once. Preserve the native flag for differential evidence. Keep the AppKit return path independent of Bun listeners or their results.
Do not subclass NSApplicationDelegate, install a second observer, synthesize reopen from activation/focus, or wait on Bun in the native callback.

### D-2: Make registry visibility an event-time fact

Core remains the only authority for KELD managed-window membership and visibility. Record native visibility changes against the Host-issued identity and incarnation.
At the ordered reopen, take the boolean snapshot through that owner. Compare it with the native flag for the same observation in the reference cells.
Do not overwrite the registry snapshot with a later AppKit value. A live object, a hidden object and an AppKit auxiliary panel differ.
Cover all managed windows, multiple visible windows, last visible closing with a hidden peer, minimized-only windows and an auxiliary panel with no managed visible window.
The primitive may be a scan under the owning registry or its existing maintained count. Never add a second writable visibility counter in Bun or in the platform adapter.
An assignment to show/hide expresses intent. Apply only observed visibility facts under the registry owner's ordering rules.
If tao delivers the native flag after another visibility mutation, retain both observed orderings. Do not assert equivalence without a paired trace.
If the native flag and registry rule diverge in a supported case, capture the case under B-1. Hold that row until #449/#521 approve a compatible treatment.
UNKNOWN: The native boolean for each auxiliary/minimized/hidden scenario and its exact relation to managed windows on the tested macOS build.

### D-3: Transport and lifecycle

Use one generated Host-to-app event through #508. The proposed payload contains the frozen boolean. The trusted Host session and link route provide source identity.
Do not mint a new principal, grant, registry, public method or free-form channel. Use the existing facade event object's construction and listener arguments.
Dispatch once to the current observer set. Do not cache an activate for listeners registered afterward. Reentrant window creation updates later registry state only.
Do not coalesce genuine successive reopens because they share a boolean. Apply the existing bounded event-link rules on transport failure. Drop retired-link work without cross-generation replay.
Keep #446's Ready and startup owner. Cold-start callbacks arriving before an admitted link require the recorded order and an owner-approved bounded policy.
Do not add an unbounded pre-ready queue or infer a cold activate from ready. If the required ordering cannot be represented, block that launch cell.
On unsupported OS lanes preserve the existing portable listener surface when applicable. No native activate source or platform parity is implied.

## 5. Boundaries

Future implementation touches the macOS tao event arm, the existing Host registry/dispatcher, the registered event owner and the Electron app facade.
Coordinate visibility updates with #449 and the window show/hide/minimize owner #534. Use the root and nearest crate instructions before editing those paths.
Keep macOS package launch, Ready, single-instance delivery, direct window creation and generic focus semantics with their current owners.

## 6. Tasks

### T-1: Commit and run the Electron reference cells

- **Blocked by:** none
- After exact-spec approval, add conformance entries at the pinned Electron version. Run the macOS recorder with clean process and app identities.
- Address AC-1 through AC-4. Record unknown for an unavailable reference lane. Keep the first-launch cell unknown until the exact trace exists.

### T-2: Deliver an ordered reopen with verified window facts

- **Blocked by:** T-1, B-1
- Integrate tao, registry and generated Host event as one vertical slice. Implement the agreed warm activation contract with AC-1, AC-2, AC-3 and AC-5.

### T-3: Qualify startup as a separate lane

- **Blocked by:** T-1, T-2, B-2
- Implement only the startup disposition specified by the recorded trace. Verify AC-4 and both event/visibility orders in AC-5.
- Publish a macOS-only result with exact executable, OS build, input mechanism, source revision and all observed attempts.

## 7. Test plan & evidence

Use a real macOS app bundle, a Dock interaction and an independent observer. Record native callback count/flag, window incarnation, registry visibility, Host Ready and facade listener arguments.
Use a marker per input action. Wait for each event source to settle on documented checkpoints. A fixed sleep is never a proof of ordering.
Run a warm Dock click, a Finder reopen, ordinary focus and cold Finder/Dock launches separately. Bind all observations to actual app process incarnations.
Keep the original program alive with zero windows. Confirm one visible, minimized, hidden, active/inactive, auxiliary panel and mixed-window states before each input.
Use a deterministic test to reorder a committed window state before vs. after a reopen. Deliver two genuine reopens with the same boolean and assert both arrive.
Run a second control with event delivery paused over primary restart. Verify the successor does not receive the original link's event.
A blocked cold-start test must report the first missing witness. Record the first window count before and after whenReady, including accidental double creation.

### E-1: Reopen source and ownership trace

- **Run:** Read the pinned Electron delegate, Browser observer, Electron app emitter and tao 0.35.3 callback. Inspect the KELD event loop and registry.
- **n:** One Electron source release and one workspace-pinned tao version.
- **Environment:** GitHub source only, retrieved 2026-11-06.
- **Limits:** Source proves the reopen path. It cannot establish first-launch callback order or hidden/minimized AppKit results on a device.

## 8. Review gates triggered

Implementation affects the generated wire event and the Electron-facing event contract. Require wire and public API review.
The reused safe tao event needs no new native unsafe grant. Review any actual additional unsafe operation with the relevant owner before implementation.
No permission expansion or dependency addition is proposed.

## 9. Perf impact

No idle polling. Take one registry snapshot per actual reopen. Record peak delivery during a bounded reopen burst before claiming a performance effect.

## 10. Open questions, blockers & unknowns

### B-1: Window visibility compatibility

- **Status:** open
- **Owner:** #449 window registry and #521 macOS conformance.
- **Impact:** Warm reopen qualification requires an authoritative event-time snapshot matching native semantics for each claimed case.
- **Check:** Capture native flag, window membership and verified visibility in zero, visible, hidden, minimized and auxiliary-panel cases.

### B-2: First-launch reference

- **Status:** open
- **Owner:** #521 working with the X01-T2 Electron recorder and #446 startup owner.
- **Impact:** Cold activation and relation to ready remain unqualified. Warm reopen work can proceed when B-1 clears.
- **Check:** Produce the v44.4.5 Finder and Dock traces with input provenance, listener registration checkpoint and first-window counts.

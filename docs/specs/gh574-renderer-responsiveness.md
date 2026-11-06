# Spec: Renderer responsiveness transitions

- Status: draft
- Tracker: GH-574
- Owner: @0monish (specification coordination)
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Deliver supported renderer-hang transitions to the affected BrowserWindow and webContents. Bound state to the native binding. Exclude page recovery, primary-process watchdogs and renderer termination ownership.

## 2. Spec refs

- FACT: [Architecture 05](../architecture/05-webview-and-native.md) governs backend handles. [Architecture 02](../architecture/02-ipc.md) governs Host events. [Architecture 04](../architecture/04-electron-compat.md) governs compatibility and score reporting.
- FACT: #449 owns window identity. #459 owns the webContents facade and macOS delegate. #508 owns generated wire constants. [#573][source-1] owns pinned conformance and the destroy oracle.
- FACT: `crates/keld-wv/src/webview2/mod.rs:1598` installs the environment BrowserProcessExited observer. Its separate release obligation survives this proposal.
- FACT: `crates/keld-wv/AGENTS.md:5` restricts native operations. The verifier must inspect the actual platform bindings and exact allowed paths before implementation.
- FACT: Electron v44.4.5 specifies [BrowserWindow events][source-2] and [webContents events][source-3]. Its responsive event follows a previously unresponsive web page. Retrieved 2026-11-06.
- FACT: WebKitGTK defines [“is-web-process-responsive”][source-4] as a readable property since 2.34. Retrieved 2026-11-06.
- FACT: Microsoft documents that [hang notifications repeat][source-5]. The [Win32 reference][source-6] specifies “STILL_ACTIVE” (259). Retrieved 2026-11-06.
- FACT: The reviewed public [WKWebView][source-7] and [WKNavigationDelegate][source-8] declarations were inspected on 2026-11-06.
- INFERRED: These WK declarations provide no qualified renderer-hang/recovery observation for this design. Retain a documented limitation on macOS.

[source-1]: https://github.com/gyldlab/keld/issues/573
[source-2]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/browser-window.md#L243-L249
[source-3]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/web-contents.md#L495-L501
[source-4]: https://github.com/WebKit/WebKit/blob/febeda30215e0030c6279cc8a6fdbddd81ec8d31/Source/WebKit/UIProcess/API/glib/WebKitWebView.cpp#L1807-L1820
[source-5]: https://github.com/MicrosoftDocs/edge-developer/blob/8f4be4dcd43226dff0ee1131be8b504a29bf8ef7/microsoft-edge/webview2/concepts/process-related-events.md#L242-L254
[source-6]: https://github.com/MicrosoftDocs/webview2-win32-reference/blob/ec5ef41865aaeeea937b7d0b9225d099f3921bac/1-0-4314-35/icorewebview2processfailedeventargs2.md#L46-L55
[source-7]: https://github.com/WebKit/WebKit/blob/febeda30215e0030c6279cc8a6fdbddd81ec8d31/Source/WebKit/UIProcess/API/Cocoa/WKWebView.h
[source-8]: https://github.com/WebKit/WebKit/blob/febeda30215e0030c6279cc8a6fdbddd81ec8d31/Source/WebKit/UIProcess/API/Cocoa/WKNavigationDelegate.h

## 3. Acceptance criteria

### AC-1: Deliver a Linux hang and its true recovery

- **Given:** Two views are attached and initially responsive. Only A’s renderer enters a controlled stall.
- **When:** Observe A’s GTK property become false, then true after the external test controller releases the stall.
- **Then:** Each A surface receives unresponsive once, then responsive once. B receives zero when its renderer remains responsive.
- **Negative control:**
  - **Single fault:** Disconnect the property subscription while preserving the initial state read.
  - **At-risk precondition:** A’s real property changes in both directions during the fixture.
  - **Expected RED:** The expected transition sequence is absent.

### AC-2: Suppress duplicate and unmatched edges

- **Given:** An attached renderer has a known current property level.
- **When:** Feed true, false, false, true, true through the native result adapter.
- **Then:** Publish exactly one down edge and one up edge. A healthy baseline or duplicate notification emits no recovery.
- **Negative control:**
  - **Single fault:** Publish responsive for every true sample.
  - **At-risk precondition:** Initial healthy and final repeated healthy samples both reach the adapter.
  - **Expected RED:** A responsive event appears without a preceding delivered unresponsive.

### AC-3: Keep Windows recovery unclaimed

- **Given:** A WebView2 main-frame renderer raises ProcessFailed with kind RenderProcessUnresponsive.
- **When:** The same live binding receives repeated hang notifications, followed by silence.
- **Then:** Both target surfaces emit one unresponsive. Emit zero responsive and zero render-process-gone. Impose no elapsed-time threshold for recovery.
- **Negative control:**
  - **Single fault:** Treat the absence of another notification as recovery after a timer.
  - **At-risk precondition:** Silence occurs during a still-stalled renderer or a suspended notification pump.
  - **Expected RED:** A recovery appears without a trustworthy positive witness.

### AC-4: Revoke old bindings

- **Given:** A has reported a hang, a callback is queued, and the window is destroyed or replaced.
- **When:** Deliver the old callback after a successor binding exists.
- **Then:** The successor starts with a fresh baseline. It receives no recovery or hang from the retired binding.
- **Negative control:**
  - **Single fault:** Index state only by the reused numeric window ID.
  - **At-risk precondition:** The successor shares the numeric ID while the queued old callback is delivered.
  - **Expected RED:** The successor receives the prior state or a stale transition.

### AC-5: Preserve portable listener registration

- **Given:** A macOS host runs a live BrowserWindow and webContents facade.
- **When:** Register and remove listeners for both event names on each surface.
- **Then:** Registration and removal succeed. Report emission as unsupported on the inspected WK lane.
- **Negative control:**
  - **Single fault:** Throw during registration because the native signal is unavailable.
  - **At-risk precondition:** A consumer installs its unresponsive listener during setup.
  - **Expected RED:** The consumer receives a registration exception.

## 4. Design

### D-1: Use an engine observation as the only transition source

Linux: connect notify::is-web-process-responsive at view attachment. Read the property through its approved binding on the UI thread. Connection plus initial sample must exclude a lost transition.
An initial true value establishes the healthy baseline silently. If already false at attachment, publish one observed unresponsive level after the facade can receive it. Publish the next true only to that binding.
Retain one bounded state per active native view. Repeated property notifications with the same value are idempotent. A view that remains healthy never emits responsive.
Windows: reuse one per-view ProcessFailed handler with #488. Select RENDER_PROCESS_UNRESPONSIVE by kind. The value 259 is irrelevant to transition classification.
Emit the first hang for that binding and coalesce repeats. Microsoft’s cited contract provides no positive recovery event on this route. Silence, navigation success, JS callbacks and timers cannot establish recovery.
The Windows lane can report at most one hang per qualified native binding without a proven reset. A later independent hang on that same binding may go unreported. Record this partial coverage.
Only a qualified new native binding starts a fresh episode without a recovery event. A document navigation or reload alone does not qualify a replacement. A future positive recovery signal requires separate qualification.
WKWebView: register normal facade listeners. Expose platform support in the existing scoreboard. Do not poll script execution or use private framework delegates.
When a process exits during a hang, retire the state silently. #488 owns the terminal event. Never emit an optimistic recovery while replacing a failed renderer.

### D-2: Preserve identity and cancellation

The UI owner retains native subscription tokens. Detach on its owning thread before rebinding or destruction. Release any partial registrations on attach failure.
Bind incoming observations to the current native view incarnation and authenticated Host/app link. Reject old queued callbacks after revocation or primary restart. A child cannot publish a native fact.
Publish one bounded Host fact through #508. The facade obtains the paired BrowserWindow and webContents from #449/#459. Dispatch both from the same accepted fact.
A listener may destroy the window on unresponsive. Freeze the affected pair before dispatch. Never deliver recovery to its successor or translate listener effects into a second hang.
Use one owned state transition for paired dispatch. A failed send follows the existing link failure semantics. Never recover state by replay to another generation.
No new watchdog, heartbeat, broad capability, recovery promise, native process handle transfer or unbounded queue.

## 5. Boundaries

Documentation only in this change. After approval, keld-wv owns native attachment and teardown. Core owns the authoritative registry. The generated #508 source owns channel and payload allocation.
The Electron facade owns public event names and argument shapes. Use the existing conformance runner, OS matrix and approved observer owner.
Keep principal minting, native handle custody, primary supervision, shared generator policy and OS-specific unsafe allowlists with their current owners. Secure exact-operation approval before adding native unsafe code.
Use a closed, bounded Host fact carrying the existing binding identity and the diagnostic variant. Negotiate the owner-approved wire revision before sending the new variant.
Capture the current app-link generation at observation. Drop a fact after revocation. No new ambient grants or persisted state. Register callbacks and freeze configuration before the first qualifying action.
Compatibility fallback is the corresponding documented partial row. Observable absence never proves that the native engine executed a supported behavior.

## 6. Tasks

### T-1: Freeze oracle and observer identities

- **Blocked by:** none
- Record #573 reference results and two paired event surfaces. Declare macOS partial support and the Windows recovery gap.

### T-2: Implement one real Linux edge-pair

- **Blocked by:** T-1, B-1
- Deliver the GTK property change end to end using existing handles. Run AC-1, AC-2 and AC-4 with real Linux evidence.

### T-3: Qualify Windows hang and macOS registration

- **Blocked by:** T-1, B-1
- Connect Windows through the existing shared failure adapter. Run AC-2 through AC-5 where applicable. Keep Windows responsive explicitly uncovered.

## 7. Test plan & evidence

Drive a disposable renderer with a bounded blocking task and an external release mechanism. Keep the GTK UI loop and controller running. For each backend record real source changes, per-target deliveries, duplicate events, window identity and teardown.
Replay deterministic duplicates and reversed delivery orders against the shared adapter. Hold a stale callback across destruction. On Windows leave an active renderer stalled across repeated notifications, then release it. Its silence is insufficient evidence for responsive.
Test a listener that closes its own window. Check for a stale recovery on a newly created window. Report mocked/cooperative controls separately from OS observation. Confirm registration without claiming macOS emission.

### E-1: Pinned contract review

- **Run:** Inspect the cited Electron v44.4.5 and engine declarations, the current WebView2 environment observer and #573's conformance contract.
- **n:** Three backend contracts. Zero real-engine runs during this documentation pass.
- **Environment:** Immutable GitHub source and documentation revisions listed in section 2.
- **Limits:** Static signatures and documented semantics cannot qualify OS event ordering or product behavior.

## 8. Review gates triggered

The implementation triggers public API and wire protocol review. Native unsafe operations need exact path/operation review where applicable. No dependency addition or permission widening is proposed.
Require independent review of pair identity, platform fidelity, callback removal and reentrant listener behavior. A green documentation check does not approve the contract.

## 9. Perf impact

Keep constant state per attached view with no steady-state polling. Use existing bounded Host dispatch. Measure attach, detach and event bursts before making performance claims.

## 10. Open questions, blockers & unknowns

### B-1: Existing owner contracts

- **Status:** open
- **Owner:** #573, #449, #459 and #508 maintainers, plus each native backend owner.
- **Impact:** #573 conformance, #449/#459 paired facade objects and #508 generated event ownership. Shared Windows ProcessFailed registration requires agreement with #488.
- **Check:** Attach accepted versions, mapped red cases and actual native subscription ownership to the implementation claim. Reject any parallel event registry.

### U-1: Native acceptance

UNKNOWN: Native tests remain unrun across all three OS lanes in this draft. A future Microsoft API may supply an attributable positive recovery signal. Record API, SDK, runtime version and native evidence before enabling any Windows up-edge.

# Spec: Renderer process termination

- Status: draft
- Tracker: GH-488
- Owner: @0monish (specification coordination)
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Report disappearance of the renderer for the affected webContents and its application. Leave the Host window alive. Exclude renderer hangs, child-process-gone, Bun recovery and automatic page reload.

## 2. Spec refs

- FACT: [Architecture 05](../architecture/05-webview-and-native.md) governs backend handles. [Architecture 02](../architecture/02-ipc.md) governs Host events. [Architecture 04](../architecture/04-electron-compat.md) governs compatibility and score reporting.
- FACT: #449 owns window identity. #459 owns the webContents facade and macOS delegate. #508 owns generated wire constants. [#573][source-1] owns pinned conformance and the destroy oracle.
- FACT: `crates/keld-wv/src/webview2/mod.rs:1598` installs the environment BrowserProcessExited observer. Its separate release obligation survives this proposal.
- FACT: `crates/keld-wv/AGENTS.md:5` restricts native operations. The verifier must inspect the actual platform bindings and exact allowed paths before implementation.
- FACT: Electron v44.4.5 specifies [webContents][source-2] and [app][source-3] events. Its [details][source-4] contain a reason and integer exitCode. Retrieved 2026-11-06.
- FACT: WKNavigationDelegate gives only the affected view in [webViewWebContentProcessDidTerminate:][source-5]. The signature has no exit code or termination cause. Retrieved 2026-11-06.
- FACT: [GLib signal][source-6] reports web-process-terminated. Its [reason enum][source-7] distinguishes crash, memory limit and requested termination. Retrieved 2026-11-06.
- FACT: Microsoft distinguishes [main-frame and subframe failures][source-8] and documents the [reason enum][source-9] and [exit code accessor][source-10]. Retrieved 2026-11-06.

[source-1]: https://github.com/gyldlab/keld/issues/573
[source-2]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/web-contents.md#L485-L493
[source-3]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/app.md#L404-L413
[source-4]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/structures/render-process-gone-details.md
[source-5]: https://github.com/WebKit/WebKit/blob/febeda30215e0030c6279cc8a6fdbddd81ec8d31/Source/WebKit/UIProcess/API/Cocoa/WKNavigationDelegate.h#L164-L167
[source-6]: https://github.com/WebKit/WebKit/blob/febeda30215e0030c6279cc8a6fdbddd81ec8d31/Source/WebKit/UIProcess/API/glib/WebKitWebView.cpp#L2621-L2637
[source-7]: https://github.com/WebKit/WebKit/blob/febeda30215e0030c6279cc8a6fdbddd81ec8d31/Source/WebKit/UIProcess/API/glib/WebKitWebView.h.in#L228-L239
[source-8]: https://github.com/MicrosoftDocs/edge-developer/blob/8f4be4dcd43226dff0ee1131be8b504a29bf8ef7/microsoft-edge/webview2/concepts/process-related-events.md#L153-L180
[source-9]: https://github.com/MicrosoftDocs/webview2-win32-reference/blob/ec5ef41865aaeeea937b7d0b9225d099f3921bac/1-0-4314-35/webview2-idl.md#L650-L667
[source-10]: https://github.com/MicrosoftDocs/webview2-win32-reference/blob/ec5ef41865aaeeea937b7d0b9225d099f3921bac/1-0-4314-35/icorewebview2processfailedeventargs2.md#L46-L55

## 3. Acceptance criteria

### AC-1: Route one fact to the two surfaces

- **Given:** Two live BrowserWindows A and B have independent renderers and active observation.
- **When:** Terminate only A’s web-content process.
- **Then:** Observe one webContents event on A and one app event carrying the identical A webContents. B receives zero. A’s window stays alive.
- **Negative control:**
  - **Single fault:** Resolve the facade target from the focused window.
  - **At-risk precondition:** B has focus throughout A’s termination.
  - **Expected RED:** The event arrives on B or the app listener receives B.

### AC-2: Preserve native reason fidelity

- **Given:** A real termination has been attributed to the current native view.
- **When:** Decode the platform-specific reason and construct details for each listener.
- **Then:** Use the approved mapping below. Omit an unavailable property from the details object. The receipt records the platform limitation.
- **Negative control:**
  - **Single fault:** Substitute exitCode zero when no engine code exists.
  - **At-risk precondition:** The WK callback contains no exit-code parameter.
  - **Expected RED:** The receipt and property-presence assertion reject the fabricated zero.

### AC-3: Exclude other failures

- **Given:** The environment observer and the shared per-view ProcessFailed handler are both live.
- **When:** Deliver main-renderer exit, subframe exit, browser exit and renderer unresponsiveness in separate inputs.
- **Then:** Only main-renderer exit reaches this facade. Browser release still consumes its own signal. The hang input remains available to #574.
- **Negative control:**
  - **Single fault:** Route every WebView2 ProcessFailed to renderer termination.
  - **At-risk precondition:** The input queue includes a genuine main-frame hang with exitCode 259.
  - **Expected RED:** A forbidden termination event appears and the hang-only case fails.

### AC-4: Order revocation and idempotence

- **Given:** A Host-owned native binding carries an incarnation and one admitted app link.
- **When:** Queue a termination, destroy its window and reuse the visible window number with a new binding.
- **Then:** Drop a queued callback after binding revocation. Preserve an accepted earlier fact only for its original facade identity.
- **Negative control:**
  - **Single fault:** Resolve a queued termination by numeric ID only.
  - **At-risk precondition:** The new binding is live when the old callback is consumed.
  - **Expected RED:** The new webContents receives the retired renderer’s event.

### AC-5: Honor destroy and primary-loss oracles

- **Given:** The #573 destroy transcript exists and a separate case has a healthy renderer.
- **When:** Destroy a window through the facade in the first case. Terminate the Bun primary alone in the second.
- **Then:** Match the recorded destroy-time behavior. Primary death produces zero synthetic renderer-gone events and no replay on a successor link.
- **Negative control:**
  - **Single fault:** Convert every primary disconnect into renderer loss.
  - **At-risk precondition:** The Host still owns the healthy renderer during disconnect.
  - **Expected RED:** The surviving observer records a made-up termination.

## 4. Design

### D-1: Classify at the native owner

Proposed Windows mapping for the main-frame RENDER_PROCESS_EXITED kind only:
- Map CRASHED, OUT_OF_MEMORY, TERMINATED, LAUNCH_FAILED and INTEGRITY_FAILURE to crashed, oom, killed, launch-failed and integrity-failure respectively.
- Map NORMAL_EXIT or ABNORMAL_EXIT only if the reported exit code supports the pinned definition. A contradictory pair is a diagnostic gap.
- Retain a successfully read native exit code including a genuine zero. Omit it when the interface or getter is unavailable. Treat the WebView2 value as telemetry.
- For launch-failed, expose the code only after a native test establishes the Electron launch-error interpretation. Otherwise omit it.
- Unknown, inconsistent and future reason values produce an absent reason and a partial-compatibility row. Never infer crashes from a generic unexpected failure.

On WebKitGTK map CRASHED to crashed and TERMINATED_BY_API to killed. These are proposed compatibility mappings.
Map EXCEEDED_MEMORY_LIMIT to an absent reason plus a diagnostic of memory-limit termination. A quota kill alone does not prove physical memory exhaustion.
On WKWebView omit reason and exitCode. On GTK omit exitCode. Expose a documented KELD detail type with optional fields. Keep the pinned Electron type and divergence visible.
Exclude frame-only failures, utility/GPU/browser loss and ordinary navigation. One callback per affected WebView may be correct for a shared renderer. Never broadcast across the registry.

### D-2: Bind native notification to the current view

Use #459’s navigation delegate and the existing native view owners. Subscribe before eligible navigation. Keep one Windows ProcessFailed token per view across this pack and #574.
Fail attachment explicitly if registration fails. Drop partially registered handlers in reverse order. Remove callbacks on the UI thread before destroying or rebinding the native object.
Bind opaque view incarnation, Host/app-link generation and native attachment to each observation. Do not accept page-sent diagnostics.
Use the current Host registry and one ordered event. The facade maps that record to the two surfaces. Snapshot the matched object before listener code can destroy it.
Do not promise relative callback order beyond the recorded Electron oracle. Preserve both required deliveries under listener reentry using the agreed dispatch policy.
Coalesce duplicate terminal callbacks for the same attributed loss. A new observation becomes eligible after an authoritative new renderer/view binding. An ordinary navigation alone cannot mint a new renderer.
If the backend cannot distinguish a new renderer from a delayed old callback, leave that repeated-loss cell unqualified. Avoid time-window deduplication.
Use the app link’s bounded delivery/fault policy. No application-controlled IPC publisher, unbounded retry, cross-generation replay, automatic process launch or PID lookup.

## 5. Boundaries

Documentation only in this change. After approval, keld-wv owns native attachment and teardown. Core owns the authoritative registry. The generated #508 source owns channel and payload allocation.
The Electron facade owns public event names and argument shapes. Use the existing conformance runner, OS matrix and approved observer owner.
Keep principal minting, native handle custody, primary supervision, shared generator policy and OS-specific unsafe allowlists with their current owners. Secure exact-operation approval before adding native unsafe code.
Use a closed, bounded Host fact carrying the existing binding identity and the diagnostic variant plus optional reason and signed engine code. Negotiate the owner-approved wire revision before sending the new variant.
Capture the current app-link generation at observation. Drop a fact after revocation. No new ambient grants or persisted state. Register callbacks and freeze configuration before the first qualifying action.
Compatibility fallback is the corresponding documented partial row. Observable absence never proves that the native engine executed a supported behavior.

## 6. Tasks

### T-1: Record the reference and the capability table

- **Blocked by:** none
- Under #573, record both event signatures and deliberate destroy. Verify the backend attachment and reason/code availability before approving the implementation path.

### T-2: Deliver macOS termination vertically

- **Blocked by:** T-1, B-1
- Implement the existing delegate-to-registry-to-facade route with its negative twins. Execute AC-1, AC-2, AC-4 and AC-5 on macOS.

### T-3: Qualify Windows and Linux independently

- **Blocked by:** T-1, B-1
- Add the two backend adapters through their existing owners. Run AC-1 through AC-5 for each qualified backend. Keep failing cells visible.

## 7. Test plan & evidence

Exercise crash with A focused and backgrounded. Include two controls backed by a shared process and an unrelated control. Record each impacted binding exactly once. Inject a duplicated callback and late delivery after retirement in the deterministic layer.
For the macOS fixture terminate the actual WebContent process, record one event on each surface and call isDestroyed(). For Windows verify one attached ProcessFailed handler alongside the existing environment release barrier. For Linux record each available native cause and the missing exit code.
Use a disposable process and event barriers. Use a bounded observation deadline only as a test kill switch. A mock can prove mapping, never OS event availability.

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
- **Impact:** Exact #573 destroy oracle, #449/#459 native registry/delegate, and #508 generated event ownership.
- **Check:** Attach accepted versions, mapped red cases and actual native subscription ownership to the implementation claim. Reject any parallel event registry.

### U-1: Native acceptance

UNKNOWN: Native tests remain unrun across all three OS lanes in this draft. Exact destruction order and platform launch-error fidelity await executable oracles. Draft approval must decide whether consumers accept omitted optional detail fields.

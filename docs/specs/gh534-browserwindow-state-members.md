# Spec: macOS BrowserWindow state, mutations and owner lookups

- Status: draft
- Tracker: GH-534
- Owner: @0monish
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

An admitted app can change a live macOS window's geometry, title, visibility, focus and minimization. Its Electron-shaped getters and static lookups synchronously reflect the authoritative applied window facts. Confirm and attribute observable state to the correct native window across two windows, reentrant listeners and destruction.

Scope is the member set in [#534](https://github.com/gyldlab/keld/issues/534). Parent/modal windows, DevTools focus equivalence, Windows and Linux do not acquire new conformance coverage. Initial hidden paint and ready-to-show remain with #535.

## 2. Spec refs

The [approved registry contract][registry] owns identities, ordering and lifetime. Read [the current facade][facade], [wire-shaped state][codec], [app mirror
access][appts] and [architecture 02](../architecture/02-ipc.md) before adding fields. [#533](https://github.com/gyldlab/keld/issues/533) owns the reference cells.
[#449](https://github.com/gyldlab/keld/issues/449) owns the registry and [#459](https://github.com/gyldlab/keld/issues/459) owns the WebContents identity.

Electron 44.4.5 [BrowserWindow][bwdoc], [BaseWindow][basedoc] and the [JS wrapper][upstream] define the reference. The historical issue language predates the pinned main facade. Main already has getSize, getPosition, existing move/resize handling, getFocusedWindow and a private WebContents wrapper. Reuse these paths.

FACT: The current app mirror type declares geometry, focus, maximization and full-screen state. `packages/@keld/api/src/window-codec.ts:31`.

FACT: Upstream documents show as “Shows and gives focus to the window.” [BaseWindow][basedoc]. Sources retrieved 2026-11-06.

## 3. Acceptance criteria

### AC-1: Record setter and listener observations

- **Given:** The #533 reference fixture owns two macOS windows under Electron 44.4.5.
- **When:** Read the applicable getter immediately after each setter and inside its event callback.
- **Then:** The record binds exact values and order to their observed checkpoints.
- **Negative control:**
  - **Single fault:** Delay the immediate read until callback dispatch.
  - **At-risk precondition:** Keep an observable transition outstanding when the setter returns.
  - **Expected RED:** The checkpoint order reveals the mislabeled observation.

### AC-2: Apply visibility, focus and minimization facts first

- **Given:** Two owned windows are live. Each has state-reading listeners.
- **When:** Exercise show, hide, focus, blur, minimize and restore across both windows.
- **Then:** Listener reads match the native state for the correct target. Duplicate observations do not duplicate transitions.
- **Negative control:**
  - **Single fault:** Emit restore before applying the minimized-state update.
  - **At-risk precondition:** Minimize one window successfully before calling restore.
  - **Expected RED:** The restore listener still sees isMinimized true.

### AC-3: Preserve partial geometry and isolate return values

- **Given:** The native window has known outer bounds in logical points.
- **When:** Issue two disjoint partial changes, read bounds, mutate the returned rectangle, then reread.
- **Then:** The Host's actual rectangle includes both changes. A caller cannot mutate the mirror via the returned value.
- **Negative control:**
  - **Single fault:** Merge the second command using a stale whole rectangle.
  - **At-risk precondition:** Write width then x while the width observation is in flight.
  - **Expected RED:** The native width reverts in the second operation.

### AC-4: Observe the native title

- **Given:** The native window and web page have different titles.
- **When:** Set empty and Unicode titles. Exercise the approved encoded-byte boundary.
- **Then:** The native oracle and getter agree. Invalid input produces no native mutation.
- **Negative control:**
  - **Single fault:** Read the document title instead.
  - **At-risk precondition:** Keep native and document titles different while the window remains live.
  - **Expected RED:** The getter returns the page's title.

### AC-5: Resolve only a live canonical owner

- **Given:** A captured canonical WebContents and an unrelated window exist.
- **When:** Look up the live owner by ID and WebContents, destroy it, and retry with the captured handle.
- **Then:** Live lookups return the identical wrapper. Destroyed and unknown owners cannot resolve to another live window.
- **Negative control:**
  - **Single fault:** Retain the reverse mapping after the tombstone.
  - **At-risk precondition:** Keep the original WebContents object reachable after native destruction.
  - **Expected RED:** The old window is returned in the destroyed lookup.

### AC-6: Keep reads local

- **Given:** The admitted mirror is seeded and the real link's frame counter is quiet.
- **When:** Run 1,000 rounds of the five state getters and two successful lookups.
- **Then:** The attributable outbound frame delta is zero.
- **Negative control:**
  - **Single fault:** Add a host call to isMinimized.
  - **At-risk precondition:** Use a live admitted window with an already populated snapshot.
  - **Expected RED:** The counter detects at least one getter-originated frame.

### AC-7: Reject stale mutation and observation

- **Given:** A new window or role owns a valid live generation.
- **When:** Hold an old command or callback and release it after replacement.
- **Then:** The new state, callbacks and lookup mappings remain bound to the new generation.
- **Negative control:**
  - **Single fault:** Compare only numeric window IDs on callback apply.
  - **At-risk precondition:** Use the same public numeric ID in the fault fixture with two distinct incarnations.
  - **Expected RED:** The target mirror accepts the stale observation.

### AC-8: Separate invalid input from unknown effect

- **Given:** A native mutation counter and an observable reply fault are available.
- **When:** Submit an invalid request. Separately drop the reply after a valid command executes.
- **Then:** Invalid input has no effect. A reply failure cannot trigger an automatic duplicate command.
- **Negative control:**
  - **Single fault:** Retry on missing reply.
  - **At-risk precondition:** Confirm native application before withholding the response.
  - **Expected RED:** The native mutation counter increments twice.

## 4. Design

**Current wire and ownership.** Main's WindowState carries bounds, focus, maximization and full-screen. Visibility, minimization and title need authenticated state at initial seed/replay and live update. The Host registry is the sole window identity mint. The macOS UI thread performs native mutations and observes actual state. The existing app mirror is the sole facade read source.

| Members | Command / observation | Consumer rule |
|---|---|---|
| show, hide, focus, blur, minimize, restore | Validate live WindowRef on the existing command path. Observe the native transition, including focus transfer between windows. | Return void per the reference. Apply facts before emitting matching events. show also requests focus. |
| getBounds, setBounds, setSize, setPosition | Use the existing outer-frame logical points and actual OS-reported rectangle. | Preserve caller-supplied field presence. Return fresh values. Native constraints can adjust a requested rectangle. |
| setTitle, getTitle | Admit one bounded UTF-8 title on the approved window path. Seed and update from host-confirmed state. | Distinguish native title from the page title. Do not expose arbitrary platform handles. |
| isVisible, isFocused, isMinimized | Read distinct native facts. | A hidden or minimized window cannot be classified from absence of focus alone. |
| fromId, fromWebContents | Reuse the host-minted ref, existing canonical wrapper set and #459's webContents identity. | Compare role and incarnation, and confer no privileges through JS object properties. |

For a partial setBounds, Electron's JS wrapper merges supplied fields with current bounds. A KELD command must preserve the equivalent behavior under two rapid
writes. Avoid overwriting a prior accepted width with an earlier mirror's stale width when changing only x. Use an ordered field mask or an equivalently verified
synchronization point. Full-row replacement from a stale local snapshot fails the back-to-back case.

An immediate getter after a setter is a separate contract from a getter inside a later OS event. Capture both in Electron. If matching the former requires an
acknowledged mutation barrier, reuse the established owner and correlate the result with the applied fact. Keep getter reads local. Do not hide a difference by
waiting for an event in the parity test or optimistically storing a requested value. Record a bounded divergence if the native observation cannot match.

Native voluntary and externally initiated updates share the same fact path. Use existing per-window delta handling and preserve behavior when a move listener
synchronously destroys the object. For `animate`, record both default and true on macOS with the pinned binary. Keep timed animation outside the supported subset
unless that path is implemented and observed. Reject an unsupported request before partial application.

The current WebContents release path uses a private WeakMap. Reuse private identity binding from #459 for reverse lookup. Never accept an object because its public id, constructor name, property or script-supplied method resembles the real wrapper. A previously captured valid WebContents remains a valid input to the destroyed-lookup test after accessing win.webContents itself becomes illegal.

**Limits and compatibility.** Use one bounded title definition in the Rust wire owner with shared vectors for the TS codec and exact/over-limit multibyte tests. Do
not copy process-private pointers or truncate a title silently. Bind snapshot identity and each command to the host-authenticated role. Recovery replays current Host
state and rebuilds facade wrappers only through the approved replay path.

## 5. Boundaries

Use the current Host registry and window command router, macOS UI-thread backend, canonical Rust/TS codec contract, app mirror and Electron facade. Coordinate extensions with #531, #449 and #459. Preserve the close state machine, protocol version discipline, queued operation semantics and generated-policy ownership. No page-to-native or second JS-to-host channel.

## 6. Tasks

### T-1: Freeze the recorded public and wire contract

- **Blocked by:** none

Reconcile #533 reference rows against main. Pin immediate versus eventual semantics, partial bounds, animation, wrong-input behavior, destroyed lookup and the distinct native/document title. Freeze the minimal extension and bounded title encoding with the wire owner.

### T-2: Deliver window state transitions

- **Blocked by:** T-1, B-1

Deliver stateful show/hide/focus/blur/minimize/restore end to end. Exercise replay, two-window focus, callbacks and stale-role refusal. Add missing fields and event variants only in the owning codec revision.

### T-3: Deliver geometry and native title

- **Blocked by:** T-2

Deliver geometry and title vertical slices with native-change paths, partial-write race controls and zero-traffic getter checks. Classify any remaining animation divergence with its observed result.

### T-4: Qualify live lookups and recovery

- **Blocked by:** T-3

Bind lookups to canonical owned live wrappers, run destruction/recovery controls, and qualify each named #533 cell independently on macOS.


## 7. Test plan & evidence

| Criteria | Independent oracle | Deliberate fault |
|---|---|---|
| 1–2 | Electron binary transcript and a separate native window-state observer. Compare ordered observations for each target. | Emit restore before applying its new minimized bit, or emit a focus event for the wrong window. |
| 3 | Request a Y below the platform's allowed top boundary. Compare actual native rect, callback read and later getter. Run two immediate disjoint-field writes. | Mirror the requested rect, or overwrite the first command's width with a stale snapshot. |
| 4 | Keep page title and actual native title different. Include empty and multibyte strings around the byte cap. | Read document.title or measure JS code units instead of UTF-8 bytes. |
| 5, 7 | Capture WebContents before destruction, keep a second window live, and observe fromId and fromWebContents from a reentrant closed listener. Exercise an old-incarnation fact using the same public numeric ID in a fault fixture. | Resolve solely by id, retain a tombstoned reverse mapping, or accept a lookalike. |
| 6 | Count frames on the real app link in a quiescent window after a controlled seed barrier. Repeat with an injected getter CALL as a positive detection control. | Add one host call to isMinimized while preserving returned values. |
| 8 | Observe typed outcome and count state mutations for an invalid request; separately drop the reply after a valid command has executed. | Cast an invalid rectangle at the OS boundary, or retry a command whose prior effect is unknown. |

Record macOS/SDK, screen configuration, scale, native window roles, app/host revisions, Electron executable and ordered events. Reconcile Electron's documented menu-bar clamp against the actual display layout. Timeouts bound failure; sleeps cannot establish order or final state. No native run has occurred for this document.

### E-1: Compare shipped mirror and reference interfaces

- **Run:** [KELD codec][codec], [facade][facade] and [Electron wrapper][upstream]
- **n:** Four current WindowState fields and the named GH-534 member group
- **Environment:** Pinned GitHub sources on 2026-11-06
- **Limits:** This checks code and contract presence only. It cannot prove macOS event or geometry behavior.

## 8. Review gates triggered

Public API. The added state/commands affect the wire protocol and require its owner and independent exact-diff review. Unsafe or new dependencies require separate gate coverage if the eventual macOS binding needs them. Existing window ownership and permissions remain the authority.

## 9. Perf impact

No measured delta. Count actual bytes/frames for seed, replay and mutation, 1,000 read calls, and retained reverse-map entries after repeated create/destroy. A full title embedded in every geometry update requires justification against the bounded transport budget.

## 10. Open questions, blockers & unknowns

### B-1: Approve the bounded registry extension

- **Status:** open
- **Owner:** The #533, #449 and #459 owners
- **Impact:** New state encoding, immediate reads and reverse lookups require these owners' artifacts.
- **Check:** Attach the reference timing trace, approved codec version and canonical WebContents association.

### Q-1: Settle native title and ordering limits

Exact approval must settle title cap, seed and update encoding/version, and the ordered mutation barrier from T1's immediate-read evidence. Native animation, constraints and event order stay open until measured. Only the individually exercised supported members qualify for pass.

[registry]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh531-window-registry-close-state-machine.md
[facade]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/browser-window.ts
[codec]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/api/src/window-codec.ts
[appts]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/api/src/app.ts
[bwdoc]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/browser-window.md
[basedoc]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/base-window.md
[upstream]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/browser-window.ts

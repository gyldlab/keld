# Spec: ipcMain registration without delivery

- Status: draft
- Tracker: GH-466
- Owner: @0monish
- Updated: 2026-11-20
- Pin: 15621a3b55c0840cd494c798519cb6a24179d66e

## 1. Goal & non-goals

Expose the `ipcMain` registration surface needed by the unchanged XML app. Reuse the
registration design in [PR #1365 at `6a30a421`](https://github.com/gyldlab/keld/pull/1365),
reconciled to the selected SDK and its canonical `@keld/kipc` provider. Named and default
Electron aliases expose the same singleton in source and packed artifacts.

Registration authorizes no channel and delivers no Host message. This slice does not
mint sender identities, deliver channel-7 DATA, implement replies, dispatch invokes,
change the guard, or claim native Windows or application acceptance. GH-466 and GH-547
retain delivery and invoke obligations. `returnValue` and `ports` remain with GH-548
and GH-468.

## 2. Spec refs

Architecture 04 §3 owns the Electron facade; architecture 02 §5 and GH-540 own
routing; the existing Guard owns authorization. The [GH-466 delegated registration
decision](https://github.com/gyldlab/keld/issues/466#issuecomment-6548000565) selects
EventEmitter registration and a separate invoke table as an admitted stage. Electron
semantics are pinned to v44.4.5 commit `694f45852a0f1726cd23bfd379854de489cccb65`.
The original PR remains draft; this document does not claim approval by its author or
resolution of that PR's reviews.

## 3. Acceptance criteria

### AC-1: Registration singleton and alias

- **Given:** The selected source and each installed ESM and CJS packed Electron alias.
- **When:** A caller imports named and default `ipcMain` without a Host link.
- **Then:** Both names refer to one `node:events` EventEmitter with Electron's no-op
  `error` listener, and an instrumented link-attempt counter remains zero.
- **Negative control:**
  - **Single fault:** Remove `ipcMain` from one public alias.
  - **At-risk precondition:** The packed alias is independently installed and imported.
  - **Expected RED:** The alias singleton assertion fails.

### AC-2: Listener semantics

- **Given:** Two identical listeners and separate once listeners on one channel.
- **When:** `off` removes one duplicate, `removeListener` removes one once registration
  by its original function before invocation, and the other once listener is invoked
  reentrantly by the application using EventEmitter's own `emit`.
- **Then:** Exactly one duplicate remains; the removed once listener never runs and
  the reentrant once listener runs once. `addListener` preserves insertion order;
  channel-specific and global removal retain EventEmitter semantics.
- **Negative control:**
  - **Single fault:** Delay removal of the once wrapper until after the callback.
  - **At-risk precondition:** The callback emits the same channel recursively.
  - **Expected RED:** The invocation count exceeds one.

### AC-3: Separate invoke registration

- **Given:** One listener and one invoke handler registered on the same channel.
- **When:** The caller removes either and tries a duplicate or invalid handler.
- **Then:** Removing either leaves the other intact. An occupied channel rejects a
  second handler with an `Error` beginning `KELD-COMPAT-098: Attempted to register a
  second handler for '<channel>'` before checking its type. An invalid free handler
  rejects with a `TypeError` beginning `KELD-COMPAT-099: Expected handler to be a
  function, but found type`. `handleOnce` registers a wrapper even for an invalid
  handler and removes it before invocation; tests use an observable invocation seam
  that does not pretend Keld already dispatches invokes. Both error codes are in the
  existing registry with actionable guidance.
- **Negative control:**
  - **Single fault:** Share the invoke table with the listener registry.
  - **At-risk precondition:** Both entries use the same channel.
  - **Expected RED:** The independent-removal assertions fail.

The type-before-duplicate and remove-once-after-invocation faults are separately
tested with an occupied invalid handler and a reentrant wrapper invocation.

### AC-4: No Host delivery or new authority

- **Given:** Draw.io-shaped and library registrations without a Host link.
- **When:** They register and remove listeners and handlers.
- **Then:** The 26 app registrations cover 24 channels: `app-load-finished` has three
  listeners and the other 23 have one. Five removals leave those five channels empty and
  21 app listeners. `electron-store-get-data` and `__ELECTRON_LOG__` each have one
  listener. No callback runs and no Host link opens. Delivery cells remain pending under
  their existing owning issues.
- **Negative control:**
  - **Single fault:** Start a Host link from registration.
  - **At-risk precondition:** The fixture has no Host link configured.
  - **Expected RED:** The no-link observer fails.

### AC-5: Public types and canonical package graph

- **Given:** TypeScript consumers and installed Electron, API and KIPC packages.
- **When:** They import the registration interfaces and run the packed alias probe.
- **Then:** Export `IpcMain`, `IpcMainListener`, `IpcMainInvokeHandler`, `WebFrameMain`,
  `IpcMainEvent` and `IpcMainInvokeEvent` as types. `IpcMainEvent` has `type: "frame"`,
  host-derived opaque `processId` and `frameId`, `sender: WebContents`, nullable
  `senderFrame`, and `reply`. `WebFrameMain` has readonly `url` and `origin` strings.
  The invoke event has the same members except `reply`.
  Event types have no value export, listener payloads are `unknown[]`, and `returnValue`
  and `ports` remain absent. Registration imports `@keld/kipc`; the artifact has no
  second WorkerLink.
- **Negative control:**
  - **Single fault:** Use a relative import of the KIPC transport source.
  - **At-risk precondition:** The complete Electron package uses its normal packer.
  - **Expected RED:** The artifact graph check finds the inlined duplicate transport.

### AC-6: Conformance evidence

- **Given:** The pinned registration corpus and its upstream documentation.
- **When:** The corpus registry and mapped Bun tests run.
- **Then:** The seven registration cells pass with byte-exact citations from pinned
  `docs/api/ipc-main.md`. Four delivery cells remain fail: `ipc-main.on` and
  `ipc-main.once` owned by GH-466, and `ipc-main.handle.invoked` and
  `ipc-main.handle-once` owned by GH-547. Windows results are reported separately from
  the original macOS corpus scope.
- **Negative control:**
  - **Single fault:** Mark a delivery cell passing without a delivery witness.
  - **At-risk precondition:** Its mapped test only observes registration.
  - **Expected RED:** Corpus evidence review rejects the unsupported verdict.

## 4. Design

No boundary change: Bun owns listener registration; Host and Guard retain routing and
authorization. Reuse Electron's `EventEmitter` and Keld's `stateMap` from the canonical
`@keld/kipc` package for the independent invoke table. The absent facade has no
compatibility fallback. A second listener table or relative transport import would
duplicate an existing owner. Event interfaces declare type shape only; no event value is
minted here. Later delivery must capture and revalidate original Host-issued window and
document identity. Registration does not substitute for that contract.

## 5. Boundaries

Implement in `packages/@keld/electron`, the GH-466 compat corpus and existing error
registry. Do not change KIPC framing, grants, process ownership, dependency pins,
native adapters, or the channel-7 delivery owner.

## 6. Tasks

### T-1: Finalize selected registration contract

- **Blocked by:** none

Review this exact draft and record the delegated decision, distinguishing it from
formal third-party PR approval.

### T-2: Add registration conformance before implementation

- **Blocked by:** T-1

Port the pinned snapshot, corpus entries and mapped tests; retain delivery fail verdicts.
Demonstrate the selected-base missing-export failure.

### T-3: Implement and verify packed aliases

- **Blocked by:** T-2

Port the reviewed implementation to the canonical KIPC import, preserve selected
Electron members, update artifact tests, and run package and registry checks.

## 7. Test plan & evidence

Map AC-1–5 to Electron registration, no-link observer, type and installed packed-alias
tests. Map AC-6 to `cargo test -p keld-compat --test corpus_registry` and its manifest.
Run the error registry check and Electron suite. Report exact revisions and negative
controls; selected Windows application and live channel-7 acceptance remain separate.

### E-1: Selected-base registration and package probe

- **Run:** Bun tests and the existing packed Electron artifact test.
- **n:** Each named/default alias and all supported packed aliases.
- **Environment:** Windows, Bun 1.4.4, exact branch revision recorded in the handoff.
- **Limits:** Registration and package graph only; no native or renderer delivery.

## 8. Review gates triggered

Public API: yes; named independent exact-diff review required. Unsafe, permission model,
dependency addition and wire protocol: none. Formal third-party review is not implied.

## 9. Perf impact

No performance claim. Registration is cold; no architecture 01 §5 budget changes are
asserted.

## 10. Open questions, blockers & unknowns

### B-1: Actual message delivery remains separate

- **Status:** open
- **Owner:** GH-466 delivery and Windows channel-7 integration owners
- **Impact:** Registered handlers cannot yet receive real renderer messages from Keld.
- **Check:** Matched SDK and Host deliver an authorized message to the original live
  document with stale-document controls.

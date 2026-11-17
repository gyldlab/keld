# Delivery and evidence

Part of: [index](index.md)
Owns: sections 5–7, 9 and 10

## 5. Boundaries

When (and only when) approved:
- `keld-wv` macOS UI backend and existing `AppWindowEvent`/UI-thread
  observation owner: bounded real OS snapshots and controlled teardown;
  no raw AppKit objects leave the UI owner.
- `keld-core` GH531 window/subscription producer and current primary role
  writer: authoritatively serializes the desktop fact before the accepted
  `Subscribed` REPLY and fences old generations.
- `keld-ipc` **GH531-owned** window event enum/codec/golden vectors:
  one append-only EVENT variant after explicit amendment; not a new ID.
- `packages/@keld/api` **GH531-owned** existing mirror primitive and
  authenticated role adapter: one decode/applier and one latest snapshot.
- `packages/@keld/electron/src/screen.ts`, `native-theme.ts` and `index.ts`:
  synchronous `screen` and `nativeTheme` read-only facade, with no Electron
  runtime dependency.
- GH477's existing `electron-native-v1` mapped tests must invert their four
  corresponding cells in the implementation change, with real Mac proof.

**Must not edit** any of these under *this spec-only PR*: all production
Rust/TS, GH449/#1150/#1153 source worktrees or live GH446 readiness code,
GH527 transport, fixed GH531 spec without approved amendment, generated
channel table/allocations, KELD_KEL143_* handshakes, Host20/Guardian10/IPC
clocks, root Cargo/workflow/input policy or Windows/Linux production.
Any shared file requires the winning incumbent owner's claim plus
independent exact-diff review and canonical generated-input rebinding.

## 6. Tasks

### T-1: Independent GH481 architecture/security review

- **Blocked by:** B-1, B-2
- Approve this exact public-read/mirror design and the narrow GH531 global desktop fact/before-Subscribed amendment. Do not treat this as a unilateral change to GH531.

### T-2: Consume the GH477 expected-fail synchronous conformance cells

- **Blocked by:** B-5
- Reuse the existing pinned 44.4.5 conformance cases; do not add a new parser or duplicate source. Preserve all other cells.

### T-3: Native macOS display/appearance observer and conversion

- **Blocked by:** B-3
- Implement in the existing UI owner with retained bounds, light/dark, negative offset, changing displays and physical callback teardown tests.

### T-4: GH531 owner wire event and initial replay amendment

- **Blocked by:** B-1, B-4
- Append one wire event variant and extend current Subscribe replay/ack envelope with a first desktop snapshot, using one host-controlled epoch and bounded payload. Add hostile-frame/failure and old-code golden-vector tests.

### T-5: GH531 singleton mirror and Ready barrier

- **Blocked by:** B-1, B-2
- Consume the new fact in one state applier for the current primary/admitted role only, before its public Ready barrier. Test recovery/reordered Ready and deferred user listeners.

### T-6: Thin Electron screen/nativeTheme synchronous getters

- **Blocked by:** B-1, B-2, B-3, B-4
- Add zero-frame local reads with no Electron runtime dependency, atomic current snapshot and explicit pre-Ready/unsupported behavior.

### T-7: Real paired Electron/KELD Mac evidence and release review

- **Blocked by:** B-1, B-2, B-3, B-4, B-5
- Prove primary/non-primary monitor, workArea DIP and light/dark; verify event/reader/role lifecycle controls; flip only the four owned GH477 cells; complete full required CI and independent review.

T-3 can be developed as an isolated native observation test after T-1.
T-4 and T-5 share one GH449/GH531 owner and cannot race its active branch.
GH446 final implementation integration is a dependent review, not a license for a second Ready/Subscribe policy.

## 7. Test plan & evidence

### Test D1: authenticated primary subscription

- **Positive:** An authenticated primary Subscribe receives one desktop EVENT. An app-bound or unsubscribed role receives no desktop or window fact.
- **Negative control:** Broadcast the same frame to every authenticated link. A nonprimary role sees a fact and the test fails.

### Test D2: initial snapshot and public Ready ordering

- **Positive:** Test both Ready-first and Ready-last orderings. Ready-first is raw Ready, Subscribe(7), then valid native Init snapshot(1). Ready-last is Subscribe(7), valid native Init snapshot(1), then raw Ready.
- **Positive:** Before the first sample, retain one bounded pending correlation only: no subscriber bit, EVENT, Subscribed(7) or public Ready. After the sample, require DesktopSnapshot(1), Created replay and Subscribed(7) in that order; public Ready follows application of the complete snapshot/replay and acceptance of that same-correlation reply.
- **Terminal controls:** A sample-first ordering is accepted. No sample or malformed first sample at completed Init, whether Subscribe arrived before or after, produces one same-correlation typed ERR, zero replay and sticky rejected Ready. A missed 15-second Init uses GH446 KELD-CORE-037. Role death uses the GH527 terminal path. Unauthorized or second Subscribe cannot consume the pending slot.
- **Negative control:** Mutate pending admission to the original immediate ERR on a pre-Init missing sample. Both healthy Ready-first and Ready-last orderings fail. Removing terminal settlement hangs whenReady; admitting a second correlation or partial replay creates a ghost subscription.

### Test D3: real primary and all-displays shape

- **Positive:** A real Mac snapshot returns the Display array and primary id, finite DIP rectangles/scales and exactly one primary.
- **Negative control:** Accept a duplicate id, nonfinite scale or zero displays. The exact boundary test fails.

### Test D4: actual OS work areas

- **Positive:** Paired pinned Electron 44.4.5 and KELD results agree for physical primary and negative-offset secondary workArea.
- **Negative control:** Leave AppKit's bottom-left y-coordinate unchanged. The paired comparison fails.

### Test D5: effective appearance

- **Positive:** Paired light and dark runs on the actual Mac return the same shouldUseDarkColors boolean.
- **Negative control:** Hardcode false. The dark-mode run fails.

### Test D6: zero IPC on local reads

- **Positive:** Count actual WorkerLink and Host frames before and after 1,000 calls each to all-displays, primary, workArea and dark-mode getters. The count remains zero.
- **Negative control:** Replace a local getter with a per-read CALL. The real frame count increases.

### Test D7: ordered updates and parked callers

- **Positive:** Force observer epoch e+1 before Subscribe capture and immediately after capture. Require initial e+1, or replay e followed by Subscribed and live e+1, in order. A parked GH527 reply sees the applied fact before its listener.
- **Negative control:** Let the native observer write outside the Core writer or enqueue e+1 before replay e. Monotonic order, Ready or parked-listener tests fail.

### Test D8: lifetime and recovery

- **Positive:** After Bun crash, the same app's successor receives the current snapshot and surviving windows. Duplicate same-epoch/same-value delivery is idempotent; contradictory same-epoch data is refused; retired links are fenced.
- **Negative control:** Apply an old epoch to the successor or classify a legitimate e+1 Subscribe race as terminal stale state. The test fails.

### Test D9: bounded representation

- **Positive:** Test 1..32 displays, 32 KiB payload, malformed/truncated data, epoch exhaustion and invalid scale.
- **Negative control:** Accept a 33rd display or a snapshot with no primary. The typed boundary test fails.

### Test D10: default-deny role admission

- **Positive:** An absent, forged or app-bound role receives zero frames and no new permission.
- **Negative control:** Add screen facts to a public unguarded role. The refusal test fails.

### Test D11: explicit unsupported boundaries

- **Positive:** Pre-Ready getters and unsupported APIs return stable typed errors; no synthetic values are produced.
- **Negative control:** Return false or an empty array as an unqualified default. The observer test fails.

### Test D12: conformance flip only with native proof

- **Positive:** Real native proof and the GH566 four-cell pending-to-pass flip occur in the same implementing diff.
- **Negative control:** Flip a corpus cell based only on a TypeScript mock or without platform evidence. The corpus/product-acceptance gate fails.

Actual Mac tests need native display access, two physical layouts and both light/dark modes.
A headless CoreGraphics/TCC denial is an environment failure.
It is not evidence of an empty display or correct conversion.
Each OS fixture records the Electron binary/source pin and KELD commit.
It records monitor identity/layout, appearance epoch and raw/native rectangles.
It records normalized rectangles, event order and false/positive controls.
Use actual OS/UI callbacks and compare state after an observed condition.
Never sleep to synchronize tests.
Record unavailable physical configurations as unknown.
Do not weaken tests to make them pass.

## 9. Perf impact

No performance gain is claimed before measurement. Target **zero kipc
frames per synchronous read** is an externally verified correctness
condition. The cold native observer, one bounded snapshot serialization
and host subscription replay may affect Startup and retained memory,
so report exact frame counts/bytes, snapshot capacity, peak bounded
copy budget, allocation count, first public-Ready latency and Host
shutdown cleanup in the real Mac fixture. Compare with architecture 01
§5; a >5% regression needs the existing written waiver process. No
latency workaround may widen Host/Guardian/IPC deadlines or GH527 ring
budgets.

## 10. Open questions, blockers & unknowns

### Q-1: GH531 global snapshot event owner

Explicitly approve the global `WindowEvent::DesktopSnapshot` appended variant and initial snapshot
   before window-entity replay/`Subscribed`; currently GH531 describes
   per-window event ownership. Its owner must sign the narrow exception
   and test the old discriminants and subscribed-only recipient rule.
### Q-2: GH446 facade barrier composition

Approve/reject B's **one bounded pending authenticated Subscribe(c)** before first native sample, and add
   the first **applied** desktop snapshot to its existing `raw Ready +
   accepted Subscribe + full replay + same-correlation Subscribed` gate.
   Explicitly bind (a) first sample finalized no later than native Init,
   (b) the bounded path to UI-loop entry for an early Subscribe and a finite
   caller reply deadline covering the entire 15-second Init window plus
   replay/transport, and (c) typed terminal settlement for no sample,
   malformed sample, Init timeout and role death. If sampling may legitimately
   follow Init, instead decide the *additional* finite observation deadline.
   The GH531 owner must separately approve this pending/ordering amendment;
   no second Ready source or delayed-entry assumption is permitted.
### Q-3: Snapshot limits, transaction and epoch

Approve/revise proposed max **32** displays, max **32 KiB** encoded bytes, monotonic Core-writer
   epoch and last-sent deduplication. Confirm one atomic Subscribe capture/
   replay/Subscribed batch and the two deterministic interleavings; a
   legitimate OS update must not become a stale-epoch terminal error.
   These values/policies are **not authorized** by this draft alone.
### Q-4: macOS coordinate/effective-appearance selection

Choose the exact pinned OS display/appearance API and conversion only after a same-
   machine Electron 44.4.5 paired native experiment; raw NSScreen
   coordinates and a convenience appearance flag do not prove parity.
### Q-5: Pre-Ready/terminal getter and native-snapshot failure

Obtain the GH531/GH446 and error-registry owners' approval for *definitive Init-time*
   correlated lifecycle CallError ERR `DesktopSnapshotUnavailable` (no numeric
   code allocated), atomic zero-replay settlement, sticky public-ready
   failure and typed unavailable getters. An early healthy Subscribe is
   **pending**, not erroneous; absent/malformed final sample is terminal,
   never an empty headless display or light-mode false. Waiters must settle.

### B-1: GH531 global desktop fact amendment

- **Status:** open
- **Owner:** GH531 window-mirror contract owner
- **Impact:** The global DesktopSnapshot event and its single-applier/replay semantics are not approved by the existing per-window event contract.
- **Check:** Approve the append-only event discriminant, authenticated primary-only recipient rule and one ordered writer/state applier.

### B-2: GH446 readiness and pending Subscribe composition

- **Status:** open
- **Owner:** GH446 facade readiness owner and GH531 subscription owner
- **Impact:** A healthy early Subscribe cannot be admitted until the first native sample without an approved pending-correlation and finite caller-deadline contract.
- **Check:** Approve both Ready-first/Ready-last orderings, the 15-second Init terminal, the full caller deadline and sticky Ready failure semantics.

### B-3: Paired native macOS display and appearance evidence

- **Status:** open
- **Owner:** GH481 macOS UI/native implementation owner
- **Impact:** DIP workArea, primary identity, secondary negative offset, dock/menu insets and effective light/dark parity remain unqualified.
- **Check:** Compare the same physical Mac/layout/configuration against pinned Electron 44.4.5, including second display and both appearance modes.

### B-4: Snapshot bounds, epoch and typed terminal error

- **Status:** open
- **Owner:** GH531/Core wire owner and KELD error-registry owner
- **Impact:** The proposed 32-display/32-KiB bounds, writer transaction, epoch/deduplication and DesktopSnapshotUnavailable mapping are not approved.
- **Check:** Approve limits and ordering, validate hostile frames/golden vectors and allocate the canonical typed error before implementation.

### B-5: GH477/GH566 conformance and product acceptance

- **Status:** open
- **Owner:** GH477 compatibility owner and GH566 corpus-manifest owner
- **Impact:** Four expected-fail cells cannot become passing compatibility evidence without real native paired results and zero-frame mirror proof.
- **Check:** Flip only the four owned cells in the implementing diff after native macOS acceptance and full required CI.

This spec is **DRAFT** and approval is a separate tracked decision. It
neither merges/edits the active GH449/GH446 branches, implements any
native module, qualifies real macOS product behavior, closes #481 nor
flips the four pending GH477 conformance cells by itself.

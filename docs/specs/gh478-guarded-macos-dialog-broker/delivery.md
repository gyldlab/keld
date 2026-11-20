# Delivery and evidence

Part of: [index](index.md)
Owns: sections 5–7, 9 and 10

## 5. Boundaries

Implement only after approval, in the existing owners:

- `crates/keld-guard/src/lib.rs` (one system-list evaluator and constant,
  existing dispatch contract), `crates/keld-ipc/src/channel_table.rs`,
  `channel_allocations.txt` and **generated** TS constant through the one
  approved generator, with source/golden-vector negative controls.
- `crates/keld-native/src/dialog.rs` plus allowed crate-local tests,
  module export, UI-affinity/native handle custody.
- `crates/keld-core/src/app_session.rs` is the **existing protected primary
  reader** (`read_primary_frames`); it must win an explicit GH824/#834 and
  GH449 owner-coordinated claim before the single existing
  `dispatch_privileged`/guarded-CALL seam is extended to route the
  canonical generated dialog channel. Tests may live under
  `crates/keld-core/src/app_session/`, but that directory alone does not
  connect a handler. The same router must bind the live role, verified
  policy, WindowRef, nonce, current generation and Cancel tombstones;
  no second primary reader, fallback dispatcher or bypass of `keld-guard`.
- `crates/keld-wv/src/engine.rs` and **macOS-only** event-loop adapter to
  deliver guarded UI requests and owned cleanup to current native windows.
- `packages/@keld/api/src/dialog.ts` and **one existing app singleton**;
  `packages/@keld/electron/src/dialog.ts` thin async compatibility facade;
  `packages/@keld/electron/src/index.ts` exports the facade.
- `crates/keld-compat/fixtures/native-corpus` and mapped #477 tests as
  approved by its source/test owner. The already published first #477
  synchronous red baseline remains GH564-owned and stays fail for #478.

Must not touch: `crates/keld-core/src/app_session.rs` or GH449 active writer's
regions without a separately won claim/owner coordination; GH824 runtime
shutdown, `keld-ipc` transport Worker implementation, GuardedPrimary and
Guardian timeouts, `#834` owner regions, Windows/Linux native production,
root workspace dependency pins, `docs/specs/gh527*` or `docs/specs/gh531*`
without a new expressly approved amendment. Shared `tools/ci-inputs.json`
needs its existing single-writer/branch-specific review and canonical rebind.
No existing transport change is implied by the proposed same-link
`DialogRequestV1::Cancel`: if tests show GH527 cannot carry it through
the owned WorkerLink and live modal Host, stop for that owner's explicit
coordinated change and independent wire/lifecycle review.

## 6. Tasks

### T-1: Approve exact-content GH478 spec and resolve open questions below

- **Blocked by:** B-1, B-2, B-3, B-4, B-5, B-6, B-7, B-8

No code or channel allocation before this gate.

### T-2: Complete *only* missing async #477 expected-fail cells using

- **Blocked by:** B-8

the existing GH566 registry. No fabricated pass, no second parser.

### T-3: One exact `app.system:["dialog"]` guard admission with

- **Blocked by:** B-2, B-3, B-6, B-7

negative grant/principal/duplicate/wildcard controls, and one new
append-only channel table entry using current maximum id + 1.
Obtain the protected `app_session.rs` single-router owner claim.

### T-4: Real macOS asynchronous **message** panel behind the guard,

- **Blocked by:** B-1, B-2, B-3, B-4, B-7, B-8

actual visible-parent and no/hidden-parent mapping; compile/fault tests,
OS panel witness and exact result. This supplies GH564's shared broker.

### T-5: Real Mac open/save panels, selected/cancelled shapes, exact

- **Blocked by:** B-2, B-4, B-5, B-6

OS-selected absolute paths and no-FsBroker-grant property; alias/symlink rows
labelled observed, unknown or divergence.

### T-6: Generation-bound native cancellation/reap, deadline, stale

- **Blocked by:** B-1, B-3, B-4, B-7

callback and healthy next operation, exercised on an actual Mac;
source-exact caller-timeout/cancel tombstone and concurrent-call
refusal with separate positive and negative native witnesses.

### T-7: Thin `@keld/api` and Electron async facades, red-cell flips

- **Blocked by:** B-1, B-2, B-3, B-4, B-6, B-7, B-8

in implementing PR, strict/public-API, guard and exact-head CI gates;
prove optional present-`undefined` source inputs and keep public
compatibility score honest.

### T-8: GH564 independently consumes T-4's shared broker and GH527's

- **Blocked by:** B-1, B-3

existing `callBlocking` for synchronous clicked-index behavior,
after its **own approved spec**, never in this branch.

T-4/T-5 may be independently tested after the shared T-3 gate; T-6 validates their
composition, not just their individual wrappers. No incomplete T-4-only
implementation may be labelled a complete GH478 acceptance.

## 7. Test plan & evidence

| Criterion | Test owner and observed positive | Independent falsifier (one fault) |
|---|---|---|
| AC-1 | Guard/Core: verified permission vs zero NSPanel construction on denied call | Remove guard evaluation: denied call shows native UI and test fails |
| AC-3 | Real Mac: select save target, then Cancel; file unchanged until app writes; `securityScopedBookmarks:true` denied before UI | Create/truncate target in supported dialog mode or silently accept bookmarks: sentinel/denial control fails |
| AC-4 | Real Mac: distinct button index 0/1/2, checkbox off/on; async Promise value shape | Always return defaultId: a non-default click fails |
| AC-5 | Real Mac: shown parent attached sheet, hidden/absent fallback, stale/forged window refusal | Accept recycled WindowId with old generation: UI attaches and fails |
| AC-6 | Subprocess + real panel: kill role and inject late callback, next call succeeds | Drop callback owner without closing panel: native census detects leak |
| AC-7 | Actual event-loop delivery while modal active; host Quit remains bounded | Deliver UI cleanup only in default mode: watchdog/control fails |
| AC-8 | Path from panel; guarded FS read denied with no grant; zero grant snapshots change | Treat selected path as FS scope: denied read succeeds and fails |
| AC-9 | Boundaries for 0/max/max+1, malformed/unknown enum, oversized reply | Remove selected-path ceiling: max+1 is accepted and fails |
| AC-10 | Actual non-macOS typed unsupported, strict vs explicit legacy disclosure | Return fabricated Cancel on unsupported platform: cell fails |
| AC-11 | GH566 v44.4.5 expected-fail cells actually run; flip only against native proof | Remove mapped test from execution: manifest admission fails |

### Test: Open selection and cancellation (AC-2)

- **Positive:** Real Mac: select one file, then Cancel; observe exact filePaths and zero scopes. With both `openFile` and `openDirectory`, separately select an ordinary file
and a directory; with `multiSelections` also select both in one result. Observe both native selection flags and exact selected paths.
- **Negative control:** Return last selected path on Cancel, or force directories-only when both flags are present: ordinary-file selection/result fails; a files-only mutant fails the
directory case.

### Test: Source-exact option admission (AC-12)

- **Positive:** Run pinned unmodified draw.io Open/Save/Message argument objects, including present-`undefined` and absent, native UI result/cancel/0-1-2 clicks; show each
known option is admitted
- **Negative control:** Treat all present-`undefined` as unsupported or remove `noLink` allowlist: source-exact valid call fails before panel; accept unknown `bogus:undefined`: pre-UI
denial control fails

### Test: Caller timeout and reentrancy custody (AC-13)

#### Transport and original-correlation release

- **Positive:** Deterministic real WorkerLink/Core fixture: deliver `CALL(c_orig)`, induce `KELD-IPC-006` local timeout/Worker abandon, then `Cancel(c_cancel)` with distinct
ACK/`REPLY(c_cancel)` **and** terminal `ERR(c_orig)` (or genuine prior committed `REPLY(c_orig)`); verify original abandoned-id release exactly once, zero
retained abandoned entries and next call success after **256+1 = 257** successful timeouts/cancels on one healthy link; dead-link cleanup has no fabricated wire
reply
- **Negative control:** ACK `c_cancel` but omit `ERR(c_orig)`: original abandoned entries accumulate and 257th cleanup closes link with `KELD-IPC-027`; double-terminal `c_orig` or
pretend Cancel ACK retires original: independent correlation/once-only assertions fail

#### Native physical cancellation and custody

- **Positive:** Real Mac: delay Host-to-UI admission until after Bun `callBlocking` expiry, same-link Cancel and Host tombstone, actual physical panel dismissal or suppressed
late presentation, native handle count zero within proposed bound, stale click fenced and healthy next call; separate original/Cancel correlation captures and
denied cross-role/generation Cancel; second concurrent async call denied without a second panel
- **Negative control:** Remove Host Cancel relay or tombstone, suppress modal-mode physical cleanup or permit stale callback: late panel/leaked handle crosses bound despite transport
ACK/ERR; remove `DialogBusy`: two native panels appear

The upstream October 24 Electron oracle ran 75 pinned observations, including
an *async-positive* reentrancy control. It is **input evidence only**. The
native implementation must have **new** real KELD host logs and actual OS
click/sheet/cleanup observations; mocks test state logic but never replace
macOS acceptance. Tests await observed native events, not sleeps; a bounded
external watchdog is only a kill switch. No CI skip, retry or softened assertion
may count as a pass. Cross-OS lanes run applicable pure tests and retain
unknown native implementation status.

## 9. Perf impact

No performance improvement is claimed. New native dialogs are cold user
interactions. During implementation measure request/frame bytes, outstanding
native handles/queued request count, OS panel lifetime, cancellation tail and
retained memory while a caller is parked; compare against architecture 01 §5
and GH527 outbound/inbound caps. Credit extension is conditional on observed
retained traffic (GH528), not automatically introduced. Never raise existing
Host/Guardian/IPC deadlines to compensate for UI blocking.

## 10. Open questions, blockers & unknowns

### Q-1: Human-interaction and caller-abort clocks

Approve or change the
   distinct Host **600,000 ms** limit and proposed **10 s from live-link
   caller timeout / 5 s from Host-observed role/link loss** physical
   cancellation limits. Validate the same guarded-channel nonce/Cancel
   mechanism with GH527/Core owners on *delayed Host admission* and real
   Cocoa modal callbacks. GH527's local Worker `abandon` is not a Host
   cancellation; if the relay cannot meet the bounds, leave this
   contract **BLOCKED**, with no spec approval or sync compatibility pass.
   Existing Host20/Guardian/KIPC/cleanup service clocks are unchanged.
### Q-2: Permission grammar for exact system literals

Confirm the proposed
   `{"app":{"system":["dialog"]}}` syntax, canonical denial at
   `/app/system`, duplicate/unknown literal handling, and the `keld-guard`
   method ownership. This is intentionally distinct from current
   `evaluate("group.action", path)` behavior.
### Q-3: Error-code allocation

Assign stable new dialog-specific codes through
   the current error-code registry *at implementation time*. The typed
   categories here are normative; no numeric codes are reserved in advance.
### Q-4: macOS native modal fallback and objc2 safety

Choose the concrete
   parentless/hidden-parent panel API after one compiling/native-dispatch
   prototype proves cancellation and UI command delivery. If a new unsafe
   operation is necessary, obtain a narrowly scoped allowlist amendment and
   source/FFI review **before** implementation.
### Q-5: Selection, alias and symlink limits

The first-proof file-open/save
   tests must observe alias resolution and POSIX symlink behavior before
   declaring Electron equivalence. The pinned Electron 44.4.5 documentation
   provides `noResolveAliases` and Mac App Store bookmark options. The first
   proof **refuses** unimplemented option variants; adding them requires
   separate native evidence. Unsupported variants stay explicit unknown/
   recorded divergence, not silent path normalization or a forged bookmark.
### Q-6: Source-exact admissibility and architecture ownership

Approve
   the first-proof typed option allowlist/bounds (including present
   `undefined`, `title`, macOS-inert `noLink`), one-per-role panel policy,
   proposed Host-wide capacity 8,
   and same-PR `docs/architecture/05-webview-and-native.md` path-only
   reconciliation with the FS/architecture owner. A separate strict
   selection-derived grant still belongs to F06-T15. Record the
   `app_session.rs` primary-router winning owner claim before touching it.

Approval authorizes the bounded **macOS** design and test tasks only; it does
not imply the unrelated #564 synchronous consumer, #449 window registry,
GH528 cross-platform transport, real KELD product acceptance or a PR merge
is complete.

### B-1: Same-link caller-timeout cancellation and original correlation

- **Status:** open
- **Owner:** GH527 WorkerLink, GH478 Host/Core and GH564 consumer owners
- **Impact:** AC-13 cannot pass. A local Worker abandon or Cancel ACK alone cannot prove original-correlation retirement or physical panel release.
- **Check:** Prove delayed Host admission, nonce/role-bound Cancel, exactly one original terminal, 257 successful timeout/cancel cycles, real AppKit dismissal and zero retained native handles.

### B-2: Exact system-literal permission model

- **Status:** open
- **Owner:** keld-guard permission-model owner
- **Impact:** No dialog request may be admitted. The proposed app.system grammar is not an existing evaluator.
- **Check:** Approve exact manifest syntax, canonical /app/system denial, duplicate/unknown literal handling and failure-first guard tests.

### B-3: Stable dialog error-code allocation

- **Status:** open
- **Owner:** KELD error-registry owner
- **Impact:** Typed native, expiry and parent errors cannot ship with guessed or reused codes.
- **Check:** Allocate each required code and fix through the current registry at implementation time, then verify generated documentation and mapping tests.

### B-4: Native AppKit presentation, cancellation and unsafe boundary

- **Status:** open
- **Owner:** keld-native and keld-wv macOS owners
- **Impact:** No real panel, modal-mode cancellation or physical cleanup acceptance is available.
- **Check:** Compile the selected parentless/hidden-parent prototype, approve any exact unsafe selector, and pass real UI-thread presentation, cancellation, callback-quiescence and healthy-follow-up tests.

### B-5: Path selection, alias and symlink semantics

- **Status:** open
- **Owner:** GH566 and GH477 conformance owners
- **Impact:** Electron path parity remains unknown or divergent. No alias/symlink behavior may be claimed from source inspection alone.
- **Check:** Run real macOS open/save fixtures for alias resolution, POSIX symlink behavior, selected path bytes and immediate guarded-FS denial without an independent grant.

### B-6: Source-exact options and path-only architecture decision

- **Status:** open
- **Owner:** GH478 owner and filesystem/architecture owner
- **Impact:** The typed option allowlist, one-per-role policy, Host capacity 8 and no-implicit-FsBroker rule remain proposals. No architecture approval is inferred.
- **Check:** Approve the exact draw.io option set, present-undefined handling, macOS-inert noLink, path-only result and same-PR architecture reconciliation.

### B-7: Protected primary-router and window-owner integration

- **Status:** open
- **Owner:** GH824/#834 primary-reader owner and GH449 window-registry owner
- **Impact:** No production app_session.rs route, window custody or final Host dispatch may be added by an unclaimed writer.
- **Check:** Record the winning owner claim for the single existing primary reader and Host-minted WindowRef path; prove one dispatcher and no second reader.

### B-8: First-proof conformance registry and evidence

- **Status:** open
- **Owner:** GH566 corpus-manifest owner and GH477 test owner
- **Impact:** No compatibility cell can flip to pass without the real source-exact native result and registered failing-first case.
- **Check:** Register and execute every owned async expected-fail cell, retain real Mac receipts and update only the implementing PR's cells after native proof.

# Dialog boundaries, delivery, evidence and decisions

Part of: [index](index.md)
Owns: Sections 5?7 and 9?10

## 5. Boundaries

Implement only after approval, in the existing owners:

- `crates/keld-guard/src/lib.rs` (one system-list evaluator and constant,
  existing dispatch contract), `crates/keld-ipc/src/channel_table.rs`,
  `channel_allocations.txt` and **generated** TS constant through the one
  approved generator, with source/golden-vector negative controls.
- `crates/keld-native/src/dialog.rs` plus allowed crate-local tests,
  module export, canonical request/reply DTOs, UI-affinity/native handle custody.
  Extend the existing `packages/@keld/kipc/scripts/echo-codegen.ts` owner for
  `packages/@keld/api/src/dialog.generated.ts` and drift/golden-vector checks;
  do not create another generator or hand-edit the generated codec.
- `crates/keld-core/src/app_session.rs` is the **existing protected primary
  reader** (`read_primary_frames`); it must win a fresh path-level GH478 claim
  coordinated with current GH824/GH449/GH1073 overlapping writers before the single existing
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
  `packages/@keld/electron/src/browser-window.ts` owns the package-private
  genuine-brand extraction of its existing #ref plus reuse of #state liveness;
  `packages/@keld/electron/src/index.ts` exports only the public dialog facade,
  never that internal reader. Coordinate BrowserWindow's active owner first.
- `crates/keld-compat/fixtures/native-corpus` and mapped #477 tests as
  approved by its source/test owner. The already published first #477
  synchronous red baseline remains GH564-owned and stays fail for #478.

Must not touch: `crates/keld-core/src/app_session.rs` or GH449 active writer's
regions without a separately won claim/owner coordination; GH824 runtime
shutdown, `keld-ipc` transport Worker implementation, GuardedPrimary and
Guardian timeouts, current overlapping Core owner regions, Windows/Linux native production,
root workspace dependency pins, `docs/specs/gh527*` or `docs/specs/gh531*`
without a new expressly approved amendment. Shared `tools/ci-inputs.json`
needs its existing single-writer/branch-specific review and canonical rebind.
No existing transport change is implied by the proposed same-link
`DialogRequestV1::Cancel`: if tests show GH527 cannot carry it through
the owned WorkerLink and live modal Host, stop for that owner's explicit
coordinated change and independent wire/lifecycle review.

## 6. Tasks: Ordered vertical slices

Read these dependencies together with the shared first production vertical below. The same vertical must ship its owner components together.

### T-1: Review the exact contract

- **Blocked by:** none

Approve exact-content GH478 spec and resolve open questions below.
No code or channel allocation before this gate.

### T-2: Complete the owned async conformance cases

- **Blocked by:** T-1

Complete *only* missing async #477 expected-fail cells using
the existing GH566 registry. No fabricated pass, no second parser.

### T-3: Admit guarded calls through the existing router

- **Blocked by:** T-1

One exact `app.system:["dialog"]` guard admission with
negative grant/principal/duplicate/wildcard controls, and one new
append-only channel table entry using current maximum id + 1.
Obtain the protected `app_session.rs` single-router owner claim.

### T-4: Complete the guarded asynchronous Message vertical

- **Blocked by:** T-3

Complete asynchronous **message** vertical behind the guard:
supplied-parent sheets and deferred absent-parent native operation,
hidden supplied-parent upstream/Keld oracle and cleanup evidence,
same-link timeout/Cancel custody, original terminal correlation, physical
close/quiescence, and healthy next call. No partial UI export before
cleanup exists. This supplies GH499's qualified async dirty-close route
and the native broker that GH564 may independently consume.

### T-5: Qualify real macOS Open and Save panels

- **Blocked by:** T-3

Real Mac open/save panels, selected/cancelled shapes, exact
OS-selected absolute paths and no-FsBroker-grant property; alias/symlink rows
labelled observed, unknown or divergence.

### T-6: Qualify native custody and cancellation

- **Blocked by:** T-3

Generation-bound native cancellation/reap, deadline, stale
callback and healthy next operation, exercised on an actual Mac;
source-exact caller-timeout/cancel tombstone and concurrent-call
refusal with separate positive and negative native witnesses.

### T-7: Qualify the thin async facades

- **Blocked by:** T-1

Thin `@keld/api` and Electron async facades, red-cell flips
in implementing PR, strict/public-API, guard and exact-head CI gates;
prove optional present-`undefined` source inputs and keep public
compatibility score honest.

### T-8: Qualify each downstream route under its own spec

- **Blocked by:** T-4

For unchanged synchronous app source, GH564 independently
consumes T-4's broker and existing callBlocking under its own approved
spec. For the approved GH499 adapted route, its owner instead qualifies
exact async patch bytes and native dirty-close reentrancy. Neither
route is inferred passed from the other; no additional source hunk.

The first production vertical contains the message portions of T-2/T-3/T-4/T-6/T-7
together: guard, canonical channel/codec, Core admission/custody, real native
message operation/cleanup and thin async facade with meaningful tests. It must
not land a public dialog export or a routed production stub awaiting later
cleanup. Open/Save is the next complete vertical and retains its filter/path
obligations; the remaining public methods are not invented placeholders.

T-4/T-5 may be independently tested after the shared T-3 gate; T-6 validates their
composition, not just their individual wrappers. No incomplete T-4-only
implementation may be labelled a complete GH478 acceptance.

## 7. Test plan & evidence: Failure-first controls

| Criterion | Test owner and observed positive | Independent falsifier (one fault) |
|---|---|---|
| A1a | Existing boot preflight: malformed/duplicate-object policy or mismatched digest retains its typed startup failure and zero new app child/listener/window/panel | Move verification after app startup: independent resource counters become nonzero |
| A1b | Valid verified app-role CALL: exact grant versus absent/wrong/duplicate-array/unknown/wildcard literals and zero native constructors on denial | Remove guard evaluation: denied CALL creates UI |
| A1c | Bootstrap/session rejects untrusted/stale/wrong role; separately evaluator rejects non-AppProcess before grants | Reuse app grants for another principal or admit untrusted link: identity owner check fails |
| AC-3 | Real Mac: select save target, then Cancel; file unchanged until app writes; `securityScopedBookmarks:true` denied before UI | Create/truncate target in supported dialog mode or silently accept bookmarks: sentinel/denial control fails |
| AC-4 | Real Mac: distinct button index 0/1/2, checkbox off/on; async Promise value shape | Always return defaultId: a non-default click fails |
| AC-6 | Subprocess + real panel: kill role and inject late callback, next call succeeds | Drop callback owner without closing panel: native census detects leak |
| AC-7 | Actual event-loop delivery while modal active; host Quit remains bounded | Deliver UI cleanup only in default mode: watchdog/control fails |
| AC-8 | Path from panel; guarded FS read denied with no grant; zero grant snapshots change | Treat selected path as FS scope: denied read succeeds and fails |
| AC-9 | Boundaries for 0/max/max+1, malformed/unknown enum, oversized reply | Remove selected-path ceiling: max+1 is accepted and fails |
| AC-10 | Actual non-macOS typed unsupported, strict vs explicit legacy disclosure | Return fabricated Cancel on unsupported platform: cell fails |
| AC-11 | GH566 v44.4.5 expected-fail cells actually run; flip only against native proof | Remove mapped test from execution: manifest admission fails |
| AC-12 | Run pinned unmodified draw.io Open/Save/Message argument objects, including present-`undefined` and absent, native UI result/cancel/0-1-2 clicks; show each known option is admitted | Treat all present-`undefined` as unsupported or remove `noLink` allowlist: source-exact valid call fails before panel; accept unknown `bogus:undefined`: pre-UI denial control fails |
| AC-13 cleanup admission | One active dialog and original correlation remain pending while another same-channel Cancel reaches actual Core decode; no second native panel | Reuse one-panel capacity as a pre-payload rejection of every CALL: Cancel is refused and cleanup fails |
| AC-12 macOS filters | Four actual source groups; first group initially, VSDX and All Files selectable with corresponding allowed types | Union groups accepts VSDX under first-group restriction; first-only implementation cannot change to VSDX/All Files |

- **AC-2**
  - Test: Real Mac: select one file, then Cancel; observe exact filePaths and zero scopes. With
    both `openFile` and `openDirectory`, separately select an ordinary file and a directory;
    with `multiSelections` also select both in one result. Observe both native selection flags
    and exact selected paths.
  - Independent falsifier: Return last selected path on Cancel, or force directories-only when
    both flags are present: ordinary-file selection/result fails; a files-only mutant fails the
    directory case.
- **AC-5**
  - Test: Supplied parent uses async sheet API; absent uses detached operation; run separate
    real Electron/Keld hidden-parent oracle without assuming sync fallback. Genuine live wrapper
    passes package-private brand/#state reader; stale/forged/wrong/proxy/revoked objects fail
    before send; Host rechecks owner/ref at UI and reply
  - Independent falsifier: Public id/getter/instanceof substitution admits forgery or executes a
    callback; bypass Host incarnation/role check attaches/replies to wrong owner;
    hidden→detached fallback mutation violates the selected source branch
- **AC-13 transport/correlation**
  - Test: Deterministic real WorkerLink/Core fixture: deliver `CALL(c_orig)`, induce
    `KELD-IPC-006` local timeout/Worker abandon, then `Cancel(c_cancel)` with distinct
    ACK/`REPLY(c_cancel)` **and** terminal `ERR(c_orig)` (or genuine prior committed
    `REPLY(c_orig)`); verify original abandoned-id release exactly once, zero retained abandoned
    entries and next call success after **256+1 = 257** successful timeouts/cancels on one
    healthy link; dead-link cleanup has no fabricated wire reply
  - Independent falsifier: ACK `c_cancel` but omit `ERR(c_orig)`: original abandoned entries
    accumulate and 257th cleanup closes link with `KELD-IPC-027`; double-terminal `c_orig` or
    pretend Cancel ACK retires original: independent correlation/once-only assertions fail
- **AC-13 native/owner**
  - Test: Real Mac: delay Host-to-UI admission until after Bun `callBlocking` expiry, same-link
    Cancel and Host tombstone, actual physical panel dismissal or suppressed late presentation,
    native handle count zero within proposed bound, stale click fenced and healthy next call;
    separate original/Cancel correlation captures and denied cross-role/generation Cancel;
    second concurrent async call denied without a second panel
  - Independent falsifier: Remove Host Cancel relay or tombstone, suppress modal-mode physical
    cleanup or permit stale callback: late panel/leaked handle crosses bound despite transport
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

## 10. Open questions, blockers & unknowns: Owner decisions and remaining gates

The repository owner accepts the exact v2 design under the user-delegated
decision, recorded at [GH478 comment 6476354568](https://github.com/gyldlab/keld/issues/478#issuecomment-6476354568). The choices
below are fixed for this spec; implementation, generated channel allocation,
named owner claims/reviews and native acceptance remain separate gates. This
design approval does not claim native/Host/product acceptance has passed.

1. **Clocks:** 600,000 ms absolute Host interaction deadline from original
   admission; physical suppression/cleanup within 10 s of live-link caller
   timeout, or 5 s of Host-observed role/link retirement. Existing Host20,
   guardian, FS and IPC clocks remain unchanged. The bounds are acceptance
   requirements, not measured by the primitive probe. Failed bounded cleanup
   invokes the existing fatal/custody owner, never fabricated user Cancel.
2. **Permission:** exact `app.system:["dialog"]` in the guard-owned evaluator,
   pointer `/app/system`, principal-first default deny, duplicate/unknown/
   wildcard refusal. Every wire Cancel still uses dispatch_privileged and
   matching existing custody. Immutable generation revocation/death is cleaned
   up by Core lifecycle, without a wire permission exception.
3. **Wire/errors:** typed Open/Save/Message/Cancel and CancelDisposition on
   one append-only guarded channel, original and Cancel correlations distinct.
   At inspected main a866a7c5, channel allocations are echo1/fs2/lifecycle3/
   auth4/window5, so6 is the next candidate. NATIVE009–016 have no current
   registered/source use; proposed categories are InvalidDialogRequest,
   Unsupported, InvalidParent, ParentRetired, Busy, Expired, Cancelled and
   NativeFailure in that order. These are not reservations: the designated
   schema/error writer refreshes the actual allocation tables immediately
   before recording them, preserves every existing ID, and regenerates through
   the canonical owner. Role retirement keeps existing IPC023 semantics.
4. **Native:** callback-based file panels/sheets and deferred-outside-Tao
   absent-parent NSAlert as §4.3. A supplied hidden parent remains on the
   pinned async sheet branch; actual upstream/Keld hidden behavior is an
   explicit unrun acceptance gate, not a Keld safety divergence. One actual Keld WV experiment on macOS26.5.1
   with verified main-equivalent F2 source observed visible/modal alert,
   real AppWindowCommand::Quit, service abort, response-1001, invisible/
   nonmodal state and normal drained exit. Direct-in-service entry blocked
   delivery; removing only native abort left the modal hung. This qualifies
   the existing-loop scheduling primitive only, not Host DIALOG routing.
   Exact Native/WV unsafe operations and any feature/dependency delta retain
   their independent owner gates; the private probe grants no production allow.
5. **Paths:** data only, no FS scope/handle/bookmark/file-write effect.
   Default alias resolution is native; unsupported noResolveAliases and true
   bookmarks reject before UI. Symlink/alias selections require separate real
   fixtures before parity claims. Approve the accompanying architecture05
   reconciliation, while strict intent-derived grants remain F06-T15-owned.
6. **Source/public contract:** source-exact Open/Save options and actual Mac
   selectable filter groups; async Message also admits detail and omitted
   buttons. Parentless Message remains supported, including the source's boot
   error form. Adopt GH499's already-approved macOS-only async dirty-close
   direction as the minimal adapted-MVP route; exact patch bytes and real
   repeated-close/two-window/save-failure/late-result tests still belong to
   GH499. The original source needs GH564 if that patch is omitted. Whole-app
   correctness is not inferred from an isolated caller or a configured update
   suppression field: GH452 and the inert GH491 adapter must supply their own
   actual environment/no-side-effect proof.

**Passed versus open evidence.** Pinned Electron44.4.5 file_dialog_mac.mm
explicitly initializes the first filter group and installs a popup accessory
for multiple groups; selectFormat replaces allowed types with the selected
group, including its `*` behavior. It does not union all groups. The actual
original Open renderer passes four groups. This is primary-source proof;
native Keld filter switching is not yet executed.

Current WorkerLink.call has a pending map and supports simultaneous async
correlations; the single blocking reply slot restricts callBlocking nesting,
not all CALLs. A real Bun1.4.4 WorkerLink probe with a mock peer observed
same-channel original1/cancel2 while original1 was pending, separately routed
both replies, then original3 timeout IPC006/cancel4/late original terminal and
healthy5. It used one existing link and no production channel allocation.
This proves producer feasibility only. Actual Core DIALOG capacity, custody,
guard, original-terminal and257-cycle native recovery remain acceptance work.
There is no existing LifecycleLink.asyncRequest API: add only its package-private
dialog method following callAuth/callFs on the same WorkerLink; no replacement
transport or generic new request framework.

**Ownership before first edit.** GH478's latest spec-only claim expired
2026-10-27T22:30Z; GH449 F2's Core claim expired Oct28 11:30Z; GH824's latest
bounded shared-Cargo claim was released. These facts do not grant a new writer.
GH1073 retains a live claim until Oct29 02:30Z on named Windows/Core tests and
GH527 spec rows. Open PR1282 and1018 touch app_session.rs; PR1150 touches window
registry/router. PR834 is merged. Root must designate one actual GH478 Core
writer and the shared channel/schema/error/dependency writer, post/read back
the path-level claim, and reconcile those exact overlapping diffs before code.
No protected file is deemed free merely because an older claim expired.

Approval authorizes the bounded macOS test-first implementation sequence, not
GH564 sync completion, GH499 exact patch/native success, strict containment,
full app scoring, release, or merge. A product operation remains unknown until
its own acceptance passes. Failure of the chosen same-link/native composition
stops that vertical for a named owner correction; it does not authorize a
second principal/link, event pump, queue or broad permission.

### B-1: Review this exact structural rendition

- **Status:** open
- **Owner:** GH-478 specification owner and independent reviewer
- **Impact:** The structural rendition still needs exact review. The recorded decisions retain their original scope.
- **Check:** Compare each relocated clause and each explicit field to the pinned current monolith and its approval receipts.

### B-2: Protected implementation ownership and allocation

- **Status:** open
- **Owner:** GH-478 Core writer and the designated channel, schema, error and dependency owners
- **Impact:** Existing path-level claim and allocation gates govern any protected implementation changes.
- **Check:** Reconcile current overlapping diffs. Record the owner claims and refresh each live allocation table before assigning IDs.

### B-3: Exact implementation and real macOS acceptance

- **Status:** open
- **Owner:** GH-478 Host, Guard, WorkerLink, native and conformance owners
- **Impact:** GH-478 acceptance remains conditional on the source and real macOS proof in the test plan.
- **Check:** Qualify the complete native operations, delayed admission, original-correlation release and healthy follow-up on the intended implementation head.

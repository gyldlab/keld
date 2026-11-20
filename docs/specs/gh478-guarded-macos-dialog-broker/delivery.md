# Spec: GH-478: delivery

Part of: [index](index.md)
Owns: sections 5, 6, 7, 9 and 10, boundaries, tasks, evidence, perf and blockers

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

## 6. Tasks

### T-1: approve the exact-content spec

Approve the exact-content GH478 spec and resolve the open questions. No code or channel
allocation starts before this gate.

- **Blocked by:** none

### T-2: register the missing async conformance cells

Complete only the missing async #477 expected-fail cells using the existing GH566
registry. No fabricated pass and no second parser.

- **Blocked by:** T-1

### T-3: guard admission and channel allocation

Add one exact `app.system:["dialog"]` guard admission with negative grant, principal,
duplicate and wildcard controls. Add one append-only channel table entry using the
current maximum id + 1. Obtain the protected `app_session.rs` single-router owner claim.

- **Blocked by:** T-1

### T-4: asynchronous message vertical

Complete the asynchronous **message** vertical behind the guard: supplied-parent sheets
and deferred absent-parent native operation; hidden supplied-parent upstream/Keld oracle
and cleanup evidence; same-link timeout/Cancel custody; original terminal correlation;
physical close and quiescence; and a healthy next call. No partial UI export lands before
cleanup exists. This supplies GH499's qualified async dirty-close route and the native
broker that GH564 may independently consume.

- **Blocked by:** T-2, T-3

### T-5: real Mac open and save panels

Land real Mac open/save panels, selected and cancelled shapes, exact OS-selected absolute
paths and the no-FsBroker-grant property. Label alias and symlink rows observed, unknown
or divergence.

- **Blocked by:** T-3

### T-6: generation-bound cleanup

Land generation-bound native cancellation and reap, deadline, stale callback and healthy
next operation, exercised on an actual Mac. Cover source-exact caller-timeout/cancel
tombstone and concurrent-call refusal with separate positive and negative native
witnesses.

- **Blocked by:** T-4, T-5

### T-7: thin async facades

Land thin `@keld/api` and Electron async facades, red-cell flips in the implementing PR,
and strict/public-API, guard and exact-head CI gates. Prove optional
present-`undefined` source inputs and keep the public compatibility score honest.

- **Blocked by:** T-4, T-6

### T-8: independent consumer qualification

For unchanged synchronous app source, GH564 independently consumes T-4's broker and
existing callBlocking under its own approved spec. For the approved GH499 adapted route,
its owner instead qualifies exact async patch bytes and native dirty-close reentrancy.
Neither route is inferred passed from the other; no additional source hunk.

- **Blocked by:** T-4

The first production vertical contains the message portions of T-2, T-3, T-4, T-6 and T-7
together: guard, canonical channel/codec, Core admission/custody, real native message
operation/cleanup and a thin async facade with meaningful tests. It must not land a public
dialog export or a routed production stub awaiting later cleanup. Open/Save is the next
complete vertical and retains its filter/path obligations; the remaining public methods
are not invented placeholders.

T-4 and T-5 may be independently tested after the shared T-3 gate. T-6 validates their
composition, not just their individual wrappers. No incomplete T-4-only implementation
may be labelled a complete GH478 acceptance.

## 7. Test plan & evidence

Every entry names one AC, its observed positive and one independent falsifier. A mock or
unit test never replaces the real macOS witness.

### Boot preflight (AC-1)

- **Observed positive:** a malformed, duplicate-object or mismatched-digest policy
  retains its typed startup failure and creates zero app children, listeners, windows or
  panels.
- **Independent falsifier:** move verification after app startup. Independent resource
  counters become nonzero.

### Exact grant (AC-2)

- **Observed positive:** a valid verified app-role CALL admits the exact grant, and
  absent, wrong, duplicate-array, unknown and wildcard literals create zero native
  constructors.
- **Independent falsifier:** remove guard evaluation. A denied CALL creates UI.

### Caller identity (AC-3)

- **Observed positive:** bootstrap/session rejects an untrusted, stale or wrong role; the
  evaluator separately rejects a non-AppProcess principal before grant lookup.
- **Independent falsifier:** reuse app grants for another principal or admit an untrusted
  link. The identity owner check fails.

### Open dialog (AC-4)

- **Observed positive:** on a real Mac, select one file, then Cancel; observe exact
  `filePaths`, zero scopes and both native selection flags. With both `openFile` and
  `openDirectory`, select an ordinary file and a directory separately. With
  `multiSelections`, select both in one result.
- **Independent falsifier:** return the last selected path on Cancel, or force
  directories-only when both flags are present. Ordinary-file selection fails, or the
  files-only mutant fails the directory case.

### Save dialog (AC-5)

- **Observed positive:** on a real Mac, select a save target, then Cancel; the file stays
  unchanged until the app writes. `securityScopedBookmarks:true` is denied before UI.
- **Independent falsifier:** create or truncate the target in supported mode, or silently
  accept bookmarks. The sentinel or denial control fails.

### Message dialog (AC-6)

- **Observed positive:** on a real Mac, choose distinct button indexes 0, 1 and 2 with the
  checkbox off and on; observe the async Promise value shape.
- **Independent falsifier:** always return `defaultId`. A non-default click fails.

### Parent identity (AC-7)

- **Observed positive:** a supplied parent uses the async sheet API and an absent parent
  uses a detached operation. Run a separate real Electron/Keld hidden-parent oracle
  without assuming the sync fallback. A genuine live wrapper passes the package-private
  brand and `#state` reader; stale, forged, wrong, proxy and revoked objects fail before
  send. The Host rechecks owner and ref at UI and reply.
- **Independent falsifier:** a public id, getter or `instanceof` substitution admits a
  forgery or executes a callback. A bypass of the Host incarnation/role check attaches or
  replies to the wrong owner. A hidden-to-detached fallback violates the selected source
  branch.

### Native custody (AC-8)

- **Observed positive:** a subprocess and real panel kill the role and inject a late
  callback; the next call succeeds.
- **Independent falsifier:** drop the callback owner without closing the panel. The native
  census detects the leak.

### UI-thread delivery (AC-9)

- **Observed positive:** actual event-loop delivery occurs while the modal is active, and
  host Quit remains bounded.
- **Independent falsifier:** deliver UI cleanup only in default mode. The watchdog or
  control fails.

### Path-only authority (AC-10)

- **Observed positive:** a path from the panel is returned, a guarded FS read is denied
  with no grant, and zero grant snapshots change.
- **Independent falsifier:** treat the selected path as an FS scope. The denied read
  succeeds and fails.

### Bounded inputs (AC-11)

- **Observed positive:** 0, max and max+1 boundaries, malformed or unknown enums and an
  oversized reply are handled as specified.
- **Independent falsifier:** remove the selected-path ceiling. Max+1 is accepted and
  fails.

### Non-macOS and strict (AC-12)

- **Observed positive:** an actual non-macOS call is typed unsupported, and strict versus
  explicit legacy disclosure is recorded.
- **Independent falsifier:** return a fabricated Cancel on an unsupported platform. The
  cell fails.

### First-proof conformance (AC-13)

- **Observed positive:** GH566 v44.4.5 expected-fail cells actually run, and only native
  proof flips them.
- **Independent falsifier:** remove the mapped test from execution. Manifest admission
  fails.

### Source-exact options (AC-14)

- **Observed positive:** run the pinned unmodified draw.io Open, Save and Message argument
  objects, including present-`undefined` and absent values, with native UI
  result/cancel/0-1-2 clicks. Show that each known option is admitted. Four actual source
  filter groups start with the first group, and VSDX and All Files are selectable with
  the corresponding allowed types.
- **Independent falsifier:** treat all present-`undefined` values as unsupported, remove
  the `noLink` allowlist, or accept unknown `bogus:undefined`. A valid call fails before
  the panel, or the pre-UI denial control passes. A union of groups accepts VSDX under
  the first-group restriction, or a first-group-only implementation cannot change to
  VSDX or All Files.

### Timeout and reentrancy (AC-15)

- **Observed positive, transport:** a deterministic real WorkerLink/Core fixture delivers
  `CALL(c_orig)`, induces `KELD-IPC-006` local timeout or Worker abandon, then delivers
  `Cancel(c_cancel)` with a distinct ACK/`REPLY(c_cancel)` and terminal `ERR(c_orig)`, or
  a genuine prior committed `REPLY(c_orig)`. Verify the abandoned-id release exactly
  once, zero retained abandoned entries, and next-call success after **256+1 = 257**
  successful timeouts or cancels on one healthy link. Dead-link cleanup has no fabricated
  wire reply.
- **Observed positive, admission:** one active dialog and original correlation remain
  pending while another same-channel Cancel reaches actual Core decode, with no second
  native panel.
- **Observed positive, native/owner:** on a real Mac, delay Host-to-UI admission until
  after Bun `callBlocking` expiry. Deliver the same-link Cancel and Host tombstone, then
  observe actual physical dismissal or suppressed late presentation, zero native handles
  within the bound, a fenced stale click and a healthy next call. Separately capture
  original and Cancel correlations, deny a cross-role or cross-generation Cancel, and
  deny a second concurrent async call without a second panel.
- **Independent falsifier:** ACK `c_cancel` but omit `ERR(c_orig)`. Original abandoned
  entries accumulate and the 257th cleanup closes the link with `KELD-IPC-027`. A
  double-terminal `c_orig`, or an ACK treated as retiring the original, fails the
  independent correlation assertions. Reusing one-panel capacity as a pre-payload
  rejection refuses Cancel and cleanup fails. Removing the Host Cancel relay or tombstone,
  suppressing modal-mode cleanup, permitting a stale callback, or removing `DialogBusy`
  lets a late panel or leaked handle cross the bound or creates two native panels.

### E-1: pinned upstream Electron oracle

- **Run:** the October 24 evaluated Electron 44.4.5 macOS oracle, recorded at
  [KEL-237](https://linear.app/gyldlab-keld/document/electron-4445-macos-drawio-closequit-oracle-24-oct-2026-1718a506d0c7)
  and [#442](https://github.com/gyldlab/keld/issues/442#issuecomment-6381736061).
- **n:** 75 pinned observations, including an async-positive reentrancy control.
- **Environment:** macOS, Electron 44.4.5, at the pinned upstream commit.
- **Limits:** input evidence only. It is not a KELD host run and does not prove a panel,
  cleanup or compatibility score.

## 9. Perf impact

No performance improvement is claimed. New native dialogs are cold user
interactions. During implementation measure request/frame bytes, outstanding
native handles/queued request count, OS panel lifetime, cancellation tail and
retained memory while a caller is parked; compare against architecture 01 §5
and GH527 outbound/inbound caps. Credit extension is conditional on observed
retained traffic (GH528), not automatically introduced. Never raise existing
Host/Guardian/IPC deadlines to compensate for UI blocking.

## 10. Open questions, blockers & unknowns: owner-delegated decisions

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

### B-1: same-link native cleanup proof

- **Status:** open
- **Owner:** GH478 and GH564 transport/Core owners
- **Impact:** the timeout/Cancel vertical cannot claim native cleanup until a
  proved same-link relay exists. GH478 and GH564 stay blocked.
- **Check:** the real WorkerLink/Core fixture delivers `Cancel(c_cancel)` while
  the original is pending, the Host retires the panel within the bound, and the
  257th cycle does not hit `KELD-IPC-027`.

### U-1: hidden supplied-parent async behavior

The pinned Electron and Keld native oracle have not observed a hidden supplied
parent. The async sheet branch is the selected source contract. Record actual
appearance and completion before a parity claim.

# GH540 native document binding: scoped implementation decision

Status: approved for bounded local MVP integration under direct owner delegation; native acceptance remains unproved.
Tracker: GH540 / GH467 · Owner: @0monish · Updated: 2026-11-02

The prime assigned this amendment before implementation under the user's standing
technical delegation. The approved four-field DTO deliberately omits a URL: the
existing WindowSource already owns the native committed URL. The local regression
was observed before implementation; it is not an authentication or macOS receipt.

## 1. Goal and non-goals

A registered native document must be addressable from main before that renderer
sends application DATA. Use the existing Host registry and authenticated app link.
No new principal, registry, channel, permission, page message or data reservation
is introduced. This change alone does not prove original application startup IPC.

## 2. Spec refs

The governing GH540 bounded compat contract is
`docs/specs/gh540-compat-ipc.md` at repository commit
`3d72e492916259d89b1bf4723329e602ea6e1691`, sections 4.4, 4.6 and 4.7.
Architecture 02 owns framing and the canonical codec. GH552's approved
[registered-route decision](gh552-local-exact-channel-dispatch-d3.md#11-inbound-only-route-availability--delegated-clarification)
owns native Registered, current subscription and final Guard. GH531 owns
WindowRef and window events. GH542 owns native document mint and revocation.

## 3. Acceptance criteria

1. With Created, matching NavigationCommitted and authenticated native Registered,
   a fresh window has its exact view/generation target without RendererMessage.
2. A binding fact alone invokes no application listener and releases no DATA credit.
3. A missing commit, wrong WindowRef half, future generation or ambiguous current
   view cannot select a target. Duplicate metadata cannot reset message sequences.
4. Retired A cannot be revived, address B, or revoke B. Destroy and link loss
   invalidate captured targets. A successful observation never skips final Guard.
5. Host writes binding metadata after the matching window facts on the same writer.
   Subscription replay writes metadata before its correlated Subscribed response;
   native route availability still starts only after that response succeeds.
6. An unregistered, pre-Ready, foreign-owner or retired route produces no binding.
   Role loss keeps the existing requirement for a new explicit load and registration.

## 4. Design and decision-bearing atoms

- Identity: the existing Core Registry selects WindowRef and native lease. The
  observation uses their exact values. A page or Bun selector cannot mint them.
- Authentication: the current primary caller fence and existing subscribed link
  own the writer. A source unit test proves only mirror state; actual Host/native
  authentication needs the separate macOS receipt.
- Lifecycle: the native lease remains the single liveness owner. Existing
  DocumentRetired and window/link teardown revoke observations. A captured main
  send retains its original view/navigation and cannot retarget to a successor.
- Ordering: one canonical writer serializes window facts before binding facts.
  Registered-before-commit waits for the Core commit; Registered-after-commit
  publishes from the existing current Entry. No early-page work is buffered.
- Authorization: DocumentBound carries no topic, payload, nonce or grant. Existing
  final Host document/role/Guard admission and physical release remain mandatory.
- Representation: append `DocumentBound { webview: u32, navigation: u32,
  window_id: u32, window_incarnation: u64 }` at postcard event tag 2. All identities
  are nonzero. The complete event is at most 26 bytes under the existing frame cap.
  Independent vector for view 1, navigation 2, window 7/258: `02 01 02 07 82 02`.
  Existing event tags and payloads do not change. Rust owns the DTO; the existing
  echo-codegen owner generates its TypeScript codec. No handwritten parser.

The API updates its existing bounded native-view map. WindowSource supplies the
matching committed generation; RendererMessage keeps its separate native URL
equality check. A binding event has no renderer message sequence or consumer phase.
No handle, process, crash-domain or principal ownership changes. No global KIPC
frame/HELLO version bump is required for this append-only payload amendment under
`crates/keld-ipc/AGENTS.md`; paired Host/kipc/API requalification remains required.

## 5. Boundaries

Implement only the canonical compat DTO and generator, Core registered-route
producer and its existing window commit call site, and API compat-source/link
tests. Native GUI, Host packaging and native preload changes have separate owners.

## 6. Tasks

- [x] Record independent-vector fresh-window and stale/forged regressions before code.
- [x] Implement canonical DTO/generator and single-map consumption.
- [x] Publish from the existing Core registered-route owner at ordered cuts.
- [x] Run focused source, codec, link, IPC and Core subscription regressions.
- [ ] Resolve the separately owned full-Core lint blockers and retain the window
      shutdown suite failure plus its passing isolated recheck.
- [ ] Requalify paired Host/kipc/API packages and actual macOS delivery.

## 7. Test plan

Run API tests against the unfixed codec, then the canonical codec before the API
consumer correction, and finally the corrected consumer. Pin tag/field bytes,
identity bounds, truncation, unknown tags and trailing-byte rejection in both
languages. Exercise Source callbacks, sequence replay, A/B revocation and window
destruction. Core tests cover real socket subscription and negative routes; those
component tests do not qualify native registration. The native owner must observe
a cold window's bound target and final guarded delivery with no prior renderer
DATA, then stale/denied controls and actual original-app close/quit.

Source receipts are retained in `.keld-work/worker5-document-bound-20261102/`.
The fresh-window regression failed both before the codec amendment and with only
the codec amended, then passed with the consumer fix. Source and codec tests
passed 11/11; actual WorkerLink tests passed 3/3, including a send directly from
the Ready callback, stale-A/current-B ordering and final Host refusal. Removing
the renderer URL equality check made its negative test fail; the check is restored.
The full API suite passed 245/245 and Electron 380/380. Codec/codegen passed 24/24,
the three package typechecks and codegen freshness check passed, IPC tests passed
3/3, and Core compat tests passed 14/14. IPC Clippy and changed Rust formatting
passed. These fixture and component results do not prove native authentication.

The window-router suite passed 43/44: its failed-quit-reply case observed
`guardian shutdown` instead of `lifecycle Quit reply`. The isolated exact case
then passed 1/1; the suite failure is retained rather than treated as erased.
Full Core Clippy was blocked by diagnostics in the separately owned native menu
and WebView files. A supplementary no-dependencies Core check reported only
`menu_router.rs` diagnostics outside this change. Native/build owners must resolve
those gates and qualify the combined source. No full-workspace gate or macOS
acceptance is claimed here.

## 8. Review gates triggered

Public payload/API and wire review: yes. Unsafe, new permission and dependencies:
none. Independent final-diff review and actual source/test receipts are required.

A separate reviewer inspected the supplied API/wire production delta and Core
producer excerpts and found no concrete defect in that bounded logic review.
This review did not independently retrieve complete files, execute tests, or
inspect native behavior. It confirmed the shown lock order, binding-before-ready
publication, exact identity fields and metadata-only consumer handling under the
stated existing caller/callee contracts. Full paired/native qualification remains
outstanding; the review does not replace it.

## 9. Performance impact

No performance claim. One small bounded observation is written at registration,
commit or subscription replay through existing ownership and writer locks.

## 10. Open questions

No remaining source-design approval question under this delegation. Bootstrap
rendererReq/mainResp and real macOS close/quit remain separate acceptance work.

## 11. Post-bind call sequencing with the existing role witness

Approved scoped amendment, 2026-11-02, under the same direct owner delegation.
The prime owns the native bridge and exclusive Cargo/native acceptance; worker-6
owns this Core producer/validator amendment and its focused identity regressions.
This section extends bounded local post-bind sequencing to effectful calls; it
does not amend the zero-queue pre-bind rule or increase Core DATA capacity.

The original save callback in the V7 artifact calls `watchFile`, optional draft
deletion, then `saveAndClose-result` without awaiting the first two operations.
The current one-effect native gate can refuse the final result after the actual
save succeeds. One additional pending call cannot cover this general burst.
Rewriting that application or selecting whichever Bun role exists when a copied
call is eventually drained would violate the existing ownership contract.

The existing native FIFO may retain at most 64 copied call records and 524,288
accounted source/credential bytes, including active retained call custody.
Admission still requires the actual registered app function, exact native lease,
source and stored app credential. Local listener registration remains independent
of Bun availability. An effectful record additionally captures the current live
`RendererRoleWitness` from the actual Core `RouteState` observation. A missing or
stopped witness refuses before retaining that effect. Retaining a copied call
does not enter its original function, create original request/watch bookkeeping,
reserve a second DATA lane, or authorize the eventual effect.

The witness wraps the existing `PrimaryCallerContext.reader_stop` Arc. Core
publishes that exact Arc on both ready and retired native `RouteState` outcomes;
it creates no role id, counter, principal, JavaScript field or wire token. The
native queue captures that same witness with the call's original native lease
and source. Stale A readiness or retirement must not overwrite current B state.

When its existing Host lane is free, the first effect starts the existing Host
preflight immediately, even if an earlier local Call completion remains pending.
Later effects wait in the bounded FIFO and retain their original role witness.
Before delayed preflight and original entry, native rechecks the exact live
role, document and source. Each Preflight carries the captured witness. Core
checks live pointer equality against the authenticated caller while holding its
existing transition/current fence, before Registry lookup or any preflight
reservation. Missing, foreign or stopped witnesses receive the existing refused
Preflight outcome. The ledger still holds one preflight and one DATA record per
direction; final role, document, source and exact-channel Guard checks remain.

Navigation, destroy, accepted Quit and role retirement cancel unstarted effects
without retargeting them. Cancellation does not release active WK evaluation
custody or original JavaScript bookkeeping. The existing physical completion and
canonical release owners retain those charges until their own real receipts.
No original application/preload rewrite, synthetic response, successor replay,
permission widening, new transport or new dependency is permitted.

The decision-bearing tests are separate:

- Core identity: the exact live reader succeeds; equal numeric identity with a
  different live Arc, a stopped original Arc, and a missing witness refuse. An
  old witness cannot authorize its successor. Removing the predicate must make
  the negative controls fail. These are state tests, not Runtime authentication.
- Native sequencing: hold real Call completion and send release independently;
  issue the genuine save callback burst and observe each original call once in
  order only after its own accepted preflight, including the final close result.
- Lifecycle and bounds: test 64/65 records and 524,288/524,289 accounted bytes,
  old-role A versus successor B, delayed A ready/retired outcomes, document
  replacement, destroy and accepted Quit. Retained active custody must remain
  charged until actual completion and old queued work must never reach B.
- Product: a genuine dirty Save-and-Close must both persist the edited bytes and
  complete the original correlated close. Disk write alone is not a pass.

The Core regression source is authored before the producer/validator change.
Execution and deliberate predicate-removal receipts remain pending the prime's
exclusive Cargo slot; no RED, GREEN or native acceptance is claimed by this note.
Review gates: native Rust API/security review; no new unsafe, permission, wire
format or dependency. No handle, crash-domain or principal ownership change and
no performance claim.

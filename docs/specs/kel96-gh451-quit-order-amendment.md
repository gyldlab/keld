# Spec: KEL-96 conditional quit order amendment

- Status: draft
- Tracker: KEL-96
- Owner: GYLDLAB (approval pending)
- Updated: 2026-11-26
- Pin: 2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a

## 1. Goal & non-goals

Amend KEL-96 section 4.5 for the qualified GH-451 conditional Quit only.
Keep the application alive after a veto. Commit terminal shutdown only after
the existing owner proves physical release and settles admitted work.

This is a proposed scoped amendment. The approved KEL-96 text and decision
digest remain unchanged. Until joint approval and qualified implementation,
the existing section 4.5 governs all live paths.

Non-goals: new Exit API, wire allocations, permission grants, native strategy
selection, deadline changes, Windows or Linux parity, signing or release.

## 2. Spec refs

- FACT: [Approved KEL-96 section 4.5](https://github.com/gyldlab/keld/blob/2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a/docs/specs/kel96-no-flag-host-boot.md#L452-L480)
  owns the correlated reply, retirement and cleanup order. Its blob is
  `179353fa4a2fcbd8ec294d85ca6e804afe14db40`.
- FACT: [GH-451 conditional contract at historical #1432 revision](https://github.com/gyldlab/keld/blob/ce9ba6e9d2d2fe46954201f58f251754602b5c06/docs/specs/gh451-cancelable-two-phase-quit.md)
  selects conditional behavior. GH-451 Q-2 and GH-451 Q-8 are separate gates.
- FACT: [Historical proposed IPC contract](https://github.com/gyldlab/keld/blob/ce9ba6e9d2d2fe46954201f58f251754602b5c06/docs/engineering/gh451-conditional-quit-ipc-amendment.md)
  defines the receipt consumption and transport proof proposed for review.
- FACT: #1432 `docs/specs/gh523-host-authorized-app-exit.md:100@c779aa2ac08270911b86cdbe3ebcb4e5f6dbcf2a`
  proposes one terminal winner and a shared filesystem drain.
- FACT: GH-531 `docs/specs/gh531-window-registry-close-state-machine.md:389@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`
  owns physical Destroyed, LastWindowClosed and the window registry.
- FACT: `docs/architecture/06-runtime-and-tooling.md:27` retains Host lifecycle
  ownership. `docs/architecture/05-webview-and-native.md:302` retains registry ownership.
  Architecture 02 section 4 and architecture 03 section 1 retain wire and authority boundaries.

The proposed amendment changes the conditional admission order, not these owners.
GH-451 and GH-523 remain drafts. This document does not approve them.

## 3. Acceptance criteria

### AC-1: Reversible suppression and veto

- **Given:** A qualified current original starts Quit and a window vetoes.
- **When:** The existing facade applies the correlated ReplyApplied Veto for the exact current window, document,
  generation and close sequence.
- **Then:** Public last-window suppression resets. The Host retains ordered
  window facts. The selected sweep finishes without terminal reservation.
  After a matched pre-Prepare veto, a fresh qualified attempt needs no
  conditional receipt if no conditional record exists.
  Any retry with a recorded conditional effect needs confirmed noncommit.
  Require exact receipt consumption and independent no-late proof before that
  retry. The current role must remain qualified and no terminal winner may exist.
- **Negative control:**
  - **Single fault:** Keep suppression set after the matched veto.
  - **At-risk precondition:** Save finishes and the last window is destroyed.
  - **Expected RED:** The public last-window event or fresh Quit is missing.

Hold or lose the exact Veto acknowledgement separately. An optimistic listener veto or public handoff settlement
cannot substitute for the matching Host receipt. Test later valid receipt and exact-role retirement without stale reset.

Test a retry after confirmed noncommit with a retained conditional receipt.
Withhold receipt consumption and no-late proof in separate runs on the live role.
Remove only the corresponding retry guard in each run.
The test goes RED if a fresh conditional effect starts before both proofs hold.
The matched pre-Prepare veto with no conditional record remains a valid control.

### AC-2: Prepare and Create share the existing owner

- **Given:** Physical release and an empty authoritative registry at Prepare.
- **When:** The owner admits Create, CreateWithPreload or adoption after Prepare.
- **Then:** Admission permanently invalidates the token, even if the operation later fails and the census returns to zero.
  Known invalidation suppresses will-quit. An unobserved invalidation still prevents Commit.
- **Negative control:**
  - **Single fault:** Derive token validity from the final zero census alone.
  - **At-risk precondition:** An admitted Create, CreateWithPreload or adoption fails before Commit.
  - **Expected RED:** The invalidated token reserves terminal shutdown.

### AC-3: Owed filesystem replies precede reservation

- **Given:** An admitted FS call owes its terminal reply and lease release.
- **When:** Conditional Commit pauses further reader admission.
- **Then:** The session stays Running through actual Reply or Err and lease
  settlement. The owner rechecks and reserves once after those obligations.
- **Negative control:**
  - **Single fault:** Reserve before the admitted worker writes its reply.
  - **At-risk precondition:** The writer requires Running and the transition lock.
  - **Expected RED:** The reply is missing, the lease remains, or reservation is early.

### AC-4: Drain failure retains exact custody

- **Given:** A conditional drain still owes a terminal reply or lease, or its actual terminal writer fails.
- **When:** The owner observes an indeterminate viable drain or an actual failed writer.
- **Then:** Preserve the original error, attempt and owed-work custody. Do not reserve during a viable indeterminate drain.
  Resume ordinary admission only if all these conditions hold:
  - The owner confirms the current identity and proves noncommit.
  - All owed replies and leases are settled.
  - No terminal winner exists.
  Status uses only the original ordered reader when available. An actual failed writer follows existing exact-generation
  retirement. Do not require Status or Consume on the dead link. Neither case publishes false Accepted or retryable Retained.
  A successfully delivered typed FS Err is a settled reply, not a failed write.
- **Negative control:**
  - **Single fault:** Convert a viable indeterminate drain to retryable Retained.
  - **At-risk precondition:** An admitted reply or lease remains unresolved on the viable link.
  - **Expected RED:** The owner reports retryable Retained before settlement.

Test the actual failed writer with a separate negative control.
Omit only exact-generation retirement after an actual write failure.
Force the terminal writer to fail, then release its lease.
The test goes RED if the failed generation remains current before fixture cleanup.
Check that the failed generation admits no later work.

Run distinct viable-held-drain and actual-failed-writer cases, plus successful Reply and successfully delivered typed Err
controls. An in-flight count of zero alone cannot establish write success or a live role.
In both failure cases, also check the original error and attempt custody.
Check that new work cannot start without proved settlement on a viable link.

### AC-5: One committed terminal decision

- **Given:** An eligible authorized terminal contender races Quit Commit.
- **When:** Both satisfy the same owed FS obligations.
- **Then:** The first final reservation fixes the cause and code.
  One correlated accepted decision and one cleanup tail result.
  A queued same-reader contender is not admitted during the pause.
- **Negative control:**
  - **Single fault:** Let the later contender replace the committed cause.
  - **At-risk precondition:** The first contender already reserved shutdown.
  - **Expected RED:** The recorded cause changes or a second cleanup starts.

Use a validated Exit fixture for the second contender until GH-523 is approved.

### AC-6: Reply and postcommit failure are distinct

- **Given:** Conditional Commit has reserved terminal shutdown.
- **When:** A correlated reply is lost or the existing cleanup owner fails.
- **Then:** The committed decision stays fixed. The owner retains the actual
  error. No rollback, second effect or clean-exit claim follows.
  The Host attempts the correlated reply before link close if its exact writer
  and existing acceptance prerequisites remain available.
- **Negative control:**
  - **Single fault:** Treat failed cleanup as successful exit.
  - **At-risk precondition:** A held owned cleanup operation returns failure.
  - **Expected RED:** Success appears without the required cleanup receipt.

### AC-7: Legacy order stays unchanged

- **Given:** A legacy or unqualified session uses the existing Quit path.
- **When:** The Host admits the legacy Quit.
- **Then:** Existing section 4.5 behavior and bytes remain unchanged.
  Conditional requests never fall back to this path.
- **Negative control:**
  - **Single fault:** Route a legacy Quit through conditional Prepare.
  - **At-risk precondition:** A legacy fixture requests Quit with a live window.
  - **Expected RED:** The previously accepted legacy operation is refused.

## 4. Design

### D-1: Scoped order for section 4.5

For the qualified GH-451 path only, apply the following steps instead of the
unconditional admission sequence in existing section 4.5 steps 1–4.

1. Keep GH-531 Destroyed and LastWindowClosed facts ordered on the existing
   app link. The facade alone suppresses the public event during active Quit.
   Matched veto resets suppression. It does not retire the current role.
   Outside an active attempt, retain the existing listener policy. For a
   qualified original with no listener, default Quit enters the same
   conditional coordinator as an explicit Quit.
2. Keep the existing lifecycle coordinator Running. Prepare checks the
   authenticated current Ready and subscribed original and its registry under its
   existing admission order. Opening and Closing count as live.
   Prepare grants reversible authority only. The existing owner mints the token.
   Admission of a later Create, CreateWithPreload or adoption permanently invalidates the token.
   A later failure does not restore the token.
3. Cancel revokes the exact preparation without terminal reservation.
   Commit pauses further reader dispatch. Drain all admitted FS terminal
   replies and leases while Running. Do not hold required locks during drain.
   Prove that no other admission path can add owed work behind the pause.
4. Recheck the current role, token, registry and drain at the final existing
   serialized cut. Reserve the terminal decision once. A successful terminal
   reservation closes successor and new work admission. Independent failure
   retirement still uses the existing owner.
5. Use the existing post-reservation tail once. Attempt the correlated acceptance
   before link closure when the exact writer and existing acceptance prerequisites
   permit it. A failure before reply does not undo the reservation. Revoke the
   exact endpoint, token and generation.
   Reap the owned child and finish native cleanup through existing owners.
   Preserve section 4.5 step 5 and its KEL-75 portability caveat.

Apply the same drain and final reservation rule to any independently approved
GH-523 Exit contender. The first reservation fixes cause, code and decision.
A competing committed winner is not a live-link Retained result.
Do not call the full legacy Quit again after reservation.

### D-2: Failure and receipt custody

Distinguish the following three cases under the existing owners:

- Caller acknowledgement timeout on a viable link: keep the unresolved
  conditional attempt. Ordinary permitted work keeps its existing owner.
  Use ordered same-link reconciliation. Timeout alone proves no disposition.
- Indeterminate drain on a viable link: do not admit new work or reserve.
  Status can run only when the original reader can service ordered control
  without bypassing the outstanding obligations. Do not start a second reader.
  Resume only on proven noncommit, successful owed reply/lease settlement,
  current live generation and no competing terminal winner.
- Actual terminal writer failure: use the existing exact-generation retirement.
  The current writer marks the failed write before lease release. Retain the
  primary error, attempt/correlation and recovery custody. Do not pretend the
  failed link is live or require Status/Consume after its retirement.

A zero in-flight count alone does not prove successful terminal writes or a
current live generation. Check actual write failure and identity at the cut.
Preserve unresolved work and exact errors. Do not invent a timeout or exit code.

Keep committed decisions immutable after reservation. A lost reply does not
prove noncommit. Reconcile only the original attempt using the negotiated
GH-451 contract. Reclaim live-link receipt identities only with exact consumption
and proven no-late delivery. Definitive old-link retirement ends its local
reservations, not the historical known winner or unknown custody. Never require
post-EOF Status or Host exit as a condition for sending a terminal reply.
Do not delay the existing terminal tail for a receipt-consumption confirmation.
Keep a known committed decision and exact primary failure if Guardian preparation,
echo retirement, correlated reply, local fence or cleanup fails. Do not synthesize
acceptance or repeat the tail. Never transfer an old identity to a successor.

### D-3: Owner and compatibility boundaries

Reuse the current lifecycle, reader, FS, registry and supervisor owners.
The facade owns public Promise settlement and suppression. A handoff
acknowledgement does not prove physical release or terminal acceptance.
Native handles stay with the existing UI owner. Role retirement revokes
old authority without proving noncommit. No persisted state is proposed.

Types and channels: no new public Rust type, capability, manifest field or
wire number in this amendment. GH-451 Q-3 owns negotiation and exact schema
review. GH-523 owns Exit validation and status mapping.
The legacy path remains byte-identical. Do not use it as conditional fallback.
Reject a second shutdown coordinator and irreversible reserve before drain.

Platform: macOS selected original only after qualification. Other paths
retain the approved contract. GH-451 Q-8 and GH-443 own the native experiment.
AppKit success and actual Host exit are separate observations.

## 5. Boundaries

Implement only after applicable approvals, in the existing
`crates/keld-core/src/app_session.rs` and its registry, reader and FS owners.
Use existing `packages/@keld/api/src/` and `packages/@keld/electron/src/` seams.
GH-451 and GH-523 own their separate public and generated contract changes.

Do not change permissions, trust policy, supervisor identity rules, protected
CI, deadline constants, or the approved legacy KEL-96 file and digest.
Do not allocate wire values or enable native termination from this amendment.

## 6. Tasks

### T-1: Record conditional and legacy conformance fixtures

- **Blocked by:** none

Record the proposed outcomes as pending. Add isolated tests for AC-1 through
AC-7 using existing owners and nonshipping fixtures. Do not change shipping
behavior. Preserve the legacy baseline.

### T-2: Adopt the scoped reversible lifecycle amendment

- **Blocked by:** T-1, B-1, B-2, B-3

Implement conditional Prepare, Cancel, shared drain and one reservation in the
existing owner. Run AC-1 through AC-7 against real authenticated fixture paths.
Keep native enablement and the independent Exit API outside this task.
Run the approved GH-451 Q-3 transport tests before enabling conditional calls.

### T-3: Qualify the native terminal path

- **Blocked by:** T-2, B-4

Consume the independently qualified GH-451 Q-7, GH-451 Q-8 and GH-443 decisions.
Prove actual cleanup and process exit. Do not treat the native experiment as
a prerequisite for the reversible design decision.

## 7. Test plan & evidence

| Criteria | Planned witness |
|---|---|
| AC-1 | Exact facade veto and Save retry, plus retained receipt with each missing proof |
| AC-2 | Admit Create, CreateWithPreload and adoption separately, then check invalidation even if the operation later fails |
| AC-3 | Authenticated FS Reply and Err, held lease, pause and reserve counters |
| AC-4 | Independent viable-drain and failed-writer mutations, exact error custody, successful Reply/Err controls |
| AC-5 | Both eligible reservation orders, queued reader contender and one tail |
| AC-6 | Lost reply, held cleanup failure and committed decision readback |
| AC-7 | Existing legacy Quit bytes, live-window behavior and old-peer refusal |

Use explicit barriers and the existing test owners. Do not use sleeps to
establish order. Native acceptance uses an independent process witness.

### E-1: Draft-only readback

- **Run:** Exact Git source readback of the approved KEL-96 and revised #1432.
- **n:** Two pinned revisions, three governing specification documents.
- **Environment:** macOS read-only repository inspection at the header pin and #1432.
- **Limits:** No builds, runtime or native qualification. No approved amendment.

## 8. Review gates triggered

Public API, wire protocol and lifecycle/FS owner review apply to integration.
Permission model and dependency changes: none. Unsafe changes: none proposed.
Independent human review and joint exact-content approval remain required.
Do not infer approval from the existing KEL-96 digest or the GH-451 council vote.

## 9. Perf impact

No measured impact. Conditional preparation adds a bounded registry check.
Measure lock hold, retained receipt storage and UI dispatch with the existing
fixtures. Do not claim a faster shutdown from a model.

## 10. Open questions, blockers & unknowns

### B-1: Joint adoption of section 4.5 amendment

- **Status:** open
- **Owner:** KEL-96 lifecycle owner with GH-451 and GH-531 owners.
- **Impact:** T-2 cannot change shipping lifecycle behavior.
- **Check:** Review and approve this exact amendment blob. Record its precedence
  only for the qualified conditional path. Keep the original approved text intact.

### B-2: Shared terminal and drain custody

- **Status:** open
- **Owner:** KEL-96, GH-523 and FS owners.
- **Impact:** No conditional reservation without agreed owed-work and failure custody.
- **Check:** Adopt the shared rule without requiring completed Exit API or CLI work.
  Approve exact viable-drain, failed-writer and postreservation failure custody.
  Keep any applicable bounded failure policy under the existing owner.

### B-3: Negotiation and receipt reclamation

- **Status:** open
- **Owner:** GH-451 API and IPC owners.
- **Impact:** No shipping conditional calls or receipt reclamation.
- **Check:** Approve GH-451 Q-3 schema, exact consumption proof and retirement policy.

### B-4: Native mechanism and failure qualification

- **Status:** open
- **Owner:** GH-443, KEL-96 and native cleanup owners.
- **Impact:** No delayed AppKit enablement or native-success claim.
- **Check:** Qualify a disposable experiment under GH-451 Q-7 and GH-451 Q-8. Prove modal
  progress, prerequisite cleanup, bounded failure and post-reply actual exit.

Canonical adoption: publish this standalone amendment as
`docs/specs/kel96-gh451-quit-order-amendment.md` on the owning draft PR.
Link the exact approved amendment from the KEL-96 tracker and consuming GH-451
contract. Record exact joint approval, approved component blob identities and precedence together before implementation.
Historical reciprocal links identify reviewed proposals, not approval of a later revision. Do not create a cyclic requirement
for each commit to cite the other's future commit.
Do not extend the sealed oversized KEL-96 document, change the ratchet, or
create a parallel normative lifecycle spec. If architecture wording changes,
review that change with the owning amendment. No approval is asserted here.

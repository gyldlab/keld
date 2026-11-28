# Spec: KEL-96 conditional quit order amendment

- Status: draft
- Tracker: KEL-96
- Owner: GYLDLAB (approval pending)
- Updated: 2026-11-27
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
- FACT: Historical #1432 separates the
  [GH-451 Q-2 lifecycle amendment](https://github.com/gyldlab/keld/blob/ce9ba6e9d2d2fe46954201f58f251754602b5c06/docs/specs/gh451-cancelable-two-phase-quit.md#L415-L417)
  from [GH-451 Q-8 native qualification](https://github.com/gyldlab/keld/blob/ce9ba6e9d2d2fe46954201f58f251754602b5c06/docs/specs/gh451-cancelable-two-phase-quit.md#L444-L448).
- FACT: The [historical #1432 IPC proposal](https://github.com/gyldlab/keld/blob/ce9ba6e9d2d2fe46954201f58f251754602b5c06/docs/engineering/gh451-conditional-quit-ipc-amendment.md#L159-L186)
  defines the proposed receipt consumption and transport proof.
- FACT: #1432 `docs/specs/gh523-host-authorized-app-exit.md:100@c779aa2ac08270911b86cdbe3ebcb4e5f6dbcf2a`
  proposes one terminal winner and a shared filesystem drain.
- FACT: GH-531 `docs/specs/gh531-window-registry-close-state-machine.md:389@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`
  owns physical Destroyed, LastWindowClosed and the window registry.
- FACT: `docs/architecture/06-runtime-and-tooling.md:27` retains Host lifecycle
  ownership. `docs/architecture/05-webview-and-native.md:302` retains registry ownership.
  `docs/architecture/02-ipc.md:397@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a` and `docs/architecture/03-security.md:8@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a` retain wire and authority boundaries.

FACT: GH-451 and GH-523 remain drafts. This amendment changes only the proposed conditional admission order. It does not approve those drafts.

## 3. Acceptance criteria

### AC-1: Reversible suppression and veto

- **Given:** A qualified current original starts Quit and a window vetoes.
- **When:** The existing facade applies the correlated ReplyApplied Veto for the exact current window, document,
  generation and close sequence.
- **Then:** Public last-window suppression resets. The Host retains ordered
  window facts. The vetoed Quit attempt finishes without terminal reservation.
  An independently eligible Exit can still commit under the shared owner.
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

Test a veto after Prepare with one queued retry. Observe exact successful Cancel. Hold each receipt proof separately.
Omit only the corresponding retry guard. The test goes RED on early before-quit or Prepare.
Lose role qualification during final receipt consumption. Reject the queued intent without before-quit or Prepare.
Omit only the final qualification check. The test goes RED on a new callback or effect.
The settled case permits one fresh attempt on the same qualified role.

For a local pre-send refusal after valid Prepare, run the B-3 acceptance check.
Apply it separately to Cancel and Commit. A free local request slot alone
must not permit replay or a fresh conditional attempt.

### AC-2: Prepare and Create share the existing owner

- **Given:** Physical release and an empty authoritative registry at Prepare.
- **When:** The current owner admits Create or CreateWithPreload after Prepare.
- **Then:** Admission permanently invalidates the token, even if the operation
  later fails and the census returns to zero.
  Known invalidation suppresses will-quit. An unobserved invalidation still prevents Commit.
- **Negative control:**
  - **Single fault:** Derive token validity from the final zero census alone.
  - **At-risk precondition:** An admitted Create or CreateWithPreload fails before Commit.
  - **Expected RED:** The invalidated token reserves terminal shutdown.

FACT: GH-531 `docs/specs/gh531-window-registry-close-state-machine.md:924@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a` defines the five registry states.
FACT: GH-531 `docs/specs/gh531-window-registry-close-state-machine.md:945@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a` makes adoption claim an existing Open survivor.
Before Prepare, test Opening, Open, ClosePending, and Closing separately.
Each live state must refuse Prepare. For Closing, deliver Released. Observe Destroyed before Prepare becomes eligible.
Remove only the live-state census guard in one owner fixture.
The test goes RED if Prepare grants a token in any live state.
After registry census becomes empty, withhold only the physical-release observation in a nonshipping owner fixture.
Remove only the destruction-complete check. The test goes RED if Prepare succeeds without that observation.
Verify a genuine successor adopts its Open survivor without a new build.
Reject the retired role's token after adoption and after the registry later becomes empty.
Test the final generation check in a nonshipping owner fixture. Keep every other final-cut predicate valid. A matching generation must pass. Remove only the generation check. The stale generation must then reserve and make the test RED.
After terminal reservation, attempt Create, CreateWithPreload, and successor provisioning separately. Refuse each before native build or authority minting.
Remove only the terminal admission fence. The test goes RED if any post-reservation admission starts.

### AC-3: Owed filesystem replies precede reservation

- **Given:** An admitted FS call owes its terminal reply and lease release.
- **When:** Conditional Commit pauses further reader admission.
- **Then:** The session stays Running through actual Reply or Err and lease
  settlement. The owner rechecks and reserves once after those obligations.
- **Negative control:**
  - **Single fault:** Reserve before the admitted worker writes its reply.
  - **At-risk precondition:** The writer requires Running and the transition lock.
  - **Expected RED:** The reply is missing, the lease remains, or reservation is early.

Test lease settlement independently of reply delivery. Reuse the
[held-lease component witness](https://github.com/gyldlab/keld/blob/d8e8859afbc0ac603da45d9739052548c7bccfd2/crates/keld-core/src/app_session/terminal_quit_core_real_fs_candidate.rs#L208-L232)
and the [authenticated reader witness](https://github.com/gyldlab/keld/blob/d8e8859afbc0ac603da45d9739052548c7bccfd2/crates/keld-core/src/app_session/tests/conditional_quit_dispatch.rs#L498-L517).
After successful reply delivery, hold the lease and verify no reservation.
Release the lease and verify one reservation. In the existing nonshipping
owner fixture, omit only the lease-settlement predicate and retain the
reply-success predicate. The test goes RED if reservation precedes lease release.

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

Test D-2's failed-write accounting independently in the existing nonshipping
writer and lease fixture. Observe the recorded write failure before lease
release and verify that release does not count it as successful settlement.
In a separate run, defer only the failure record until after lease release. Keep exact-generation retirement enabled.
The test goes RED if it observes lease release before the failure record, even when retirement still completes.

Run distinct viable-held-drain and actual-failed-writer cases. Include successful Reply and successfully delivered typed Err controls. An in-flight count of zero alone cannot establish write success or a live role.
In both failure cases, also check the original error and attempt custody.
Check that new work cannot start without proved settlement on a viable link.
In both invalidation checks hold an unrelated FS reply and lease. Before Draining, refuse without pausing, draining or reserving.
After Draining begins, hold the same obligations and verify no early Retained, resume or reservation.
Observe the authoritative invalidation without admitting work behind the pause.
After actual reply and lease settlement, verify eligible resumption under D-2.

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

Check the exact winning code in both eligible reservation orders. Reuse the
[model winner tests](https://github.com/gyldlab/keld/blob/d8e8859afbc0ac603da45d9739052548c7bccfd2/crates/keld-core/src/app_session/terminal_quit_candidate.rs#L595-L665)
and the [separate code and identity check](https://github.com/gyldlab/keld/blob/d8e8859afbc0ac603da45d9739052548c7bccfd2/crates/keld-core/src/app_session/terminal_quit_candidate.rs#L846-L887).
Independently change only the recorded winning code in the nonshipping owner
fixture. The test goes RED for a different code even with the original cause
and exactly one cleanup tail. This follows
[GH-523 AC-7](https://github.com/gyldlab/keld/blob/6a8607e52f9c0e8a8d4b757ffb047d97f1cf12f3/docs/specs/gh523-host-authorized-app-exit.md#L100-L113).

Use a validated Exit fixture for the second contender until GH-523 is approved.
Run separate race checks for each owed FS obligation.
Hold an admitted reply in one check. In the other, deliver the reply but hold its lease.
Verify that neither contender reserves while the selected obligation is outstanding.
After settlement, verify each eligible reservation order independently in both checks.

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

Separately from failed cleanup, withhold exact acceptance after final reservation.
Remove only winner retention on delivery failure. The test goes RED on winner erasure or a second terminal effect.
The intact control preserves cause and code and runs one tail. Missing acknowledgement is not proof of noncommit.
With the exact writer and prerequisites available, use the [planned IPC Accepted-before-EOF check](https://github.com/gyldlab/keld/blob/6a8607e52f9c0e8a8d4b757ffb047d97f1cf12f3/docs/engineering/gh451-conditional-quit-ipc-amendment.md#L321-L347).
Withhold terminal Consume and Status. Observe one existing cleanup tail complete without either confirmation.
Add only a success-path wait for either confirmation. The bounded owner-level tail oracle must go RED.
Hold terminal retirement after consumption and no-late proof. Omit only the retirement requirement. The test goes RED on early release. Prove exact retirement without Consume in a separate control.
For each D-2 failure, inject only that failure after conditional reservation. Run the same fixture without the fault. At the owning seam, check unchanged cause, code, and the exact primary error. Verify no fabricated Accepted, false clean exit, or second tail.
- Guardian preparation: reuse the [Core owner failure fixture](https://github.com/gyldlab/keld/blob/070a20525896c4603fe153d9707d8b02c4953b48/crates/keld-core/src/app_session/tests/terminal_cleanup_and_windows_shutdown.rs#L1-L53) with conditional reservation. Observe fatal attribution and preserved error.
- Echo retirement: reuse the [Core Echo error observer](https://github.com/gyldlab/keld/blob/e3a45811fa8ffa82a07bcd5e22e8dfb68cc7a0c4/crates/keld-core/src/app_session/auth_router_tests.rs#L851-L909). Induce only Echo retirement failure on the conditional path and observe the exact retirement error.
- Reply and cleanup: use the controls above. Local fence: reuse the [API fence fixture](https://github.com/gyldlab/keld/blob/070a20525896c4603fe153d9707d8b02c4953b48/packages/@keld/api/fixtures/gh451-retained-link-draft/candidate.test.ts#L675-L744) for direct and recovered Accepted with the original error. Check custody and one tail in conditional integration.
Existing fixtures are component evidence only. T-2 owns the conditional checks. Native mechanism and cleanup qualification remain under T-3 and B-4.

### AC-7: Legacy order stays unchanged

- **Given:** An actual legacy or unselected session uses the existing Quit path.
- **When:** The Host admits the legacy Quit.
- **Then:** Existing section 4.5 behavior and bytes remain unchanged.
  Conditional requests never fall back to this path.
- **Negative control:**
  - **Single fault:** Route a legacy Quit through conditional Prepare.
  - **At-risk precondition:** A legacy fixture requests Quit with a live window.
  - **Expected RED:** The previously accepted legacy operation is refused.

Use the four dispositions in the [GH-451 IPC negotiation proposal](https://github.com/gyldlab/keld/blob/6a8607e52f9c0e8a8d4b757ffb047d97f1cf12f3/docs/engineering/gh451-conditional-quit-ipc-amendment.md#L45-L82):

| Peer state | Quit disposition |
|---|---|
| Actual legacy peer | Keep existing Quit behavior and bytes. Send no conditional frames. |
| Negotiation-capable but unselected | Keep existing Quit behavior. Do not send Bind or enter the conditional choreography. |
| Positively selected but not bound | Conditional Quit is unavailable, including with zero windows. Do not send a legacy Quit on refusal, loss or mismatch. Follow the existing protocol-error policy where applicable. |
| Bound to the current qualified original | Use the conditional coordinator. Do not downgrade to legacy. |

Check each row separately. Test the opposite direction on a qualified session.
Use distinct refused and unresolved conditional attempts.
Retain each original disposition. Observe no legacy Quit frame, terminal reservation or cleanup tail.
Mutate only conditional-to-legacy fallback. The test goes RED if any of those legacy effects occur.

Hold Bind unresolved on a positively selected role with one window.
Destroy the last window with no last-window listener. Verify D-1's event order.
Observe the unavailable disposition and normal Destroy return, without either Quit path.
Observe no conditional close sweep, Prepare or terminal reservation.
In separate controls, test the actual legacy, unselected and qualified bound roles.
Remove only the selected-but-unbound refusal. The test goes RED on either Quit path.
Complete the same authenticated Bind and its release proof after Destroy returns.
Observe no replay of the refused intent: no before-quit, close sweep, Prepare, legacy Quit, reservation or cleanup.
On the same still-current qualified bound original, create and destroy a fresh sole window with no listener.
Observe exactly one new coordinator entry in D-1's event order before this Destroy returns.

## 4. Design

### D-1: Scoped order for section 4.5

For the qualified GH-451 path only, replace existing section 4.5 steps 1–4
with the following steps.
Step 1 also defines the default-intent refusal before binding.

1. Keep GH-531 Destroyed and LastWindowClosed facts ordered on the existing
   app link. The facade alone suppresses the public event during active Quit.
   Matched veto resets suppression. It does not retire the current role.
   Outside an active attempt, retain the existing listener policy. For a
   qualified original with no listener, default Quit enters the same
   conditional coordinator as an explicit Quit.
   Before automatic intent, preserve the [existing facade eligibility check](https://github.com/gyldlab/keld/blob/d8e8859afbc0ac603da45d9739052548c7bccfd2/packages/@keld/electron/src/app.ts#L234-L263).
   If the current snapshot already shows a live replacement, do not enter
   automatic Quit. The later authoritative checks remain necessary.
   Do not apply this default-intent restriction to explicit Quit.
   B-1 must record precedence over the default-Quit write-before-return clause of
   [GH-531 criterion 32](https://github.com/gyldlab/keld/blob/2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a/docs/specs/gh531-window-registry-close-state-machine.md#L389-L406).
   This applies to qualified bound and selected-but-unbound default intent.
   Preserve the Destroy reply and physical facts. Emit `closed` before
   `window-all-closed` when applicable. For the qualified bound path, enter
   the reversible coordinator after those events and before `destroy()` returns.
   Neither a legacy Quit write nor terminal Commit is required before return. Prepare and the
   final cut still check the authoritative registry. If a replacement window
   is Opening, Open, ClosePending, or Closing at Prepare, refuse preparation. If Create is admitted
   after Prepare, the old token cannot reserve even if that Create later fails.
   For a positively selected role without Bound, preserve the same Destroy and event order.
   Apply the unavailable disposition from AC-7 before `destroy()` returns.
   Send no legacy Quit and enter no conditional choreography.
   Do not delay Destroy return for Bind. Keep the existing protocol-error policy.
   This exception does not change legacy or unselected behavior.
   Test qualified and legacy destroy ordering separately, including an
   admitted replacement window. The legacy write-before-return rule stays.
   Without a listener on the qualified bound path, observe coordinator entry once.
   For an already-known live replacement, observe no automatic entry,
   before-quit callback or replacement close request.
   With a registered listener, verify the existing listener policy instead.
2. Keep the existing lifecycle coordinator Running. Prepare checks the
   authenticated current Ready and subscribed original and its registry under its
   existing admission order. Opening, Open, ClosePending, and Closing count as live.
   Prepare grants reversible authority only. The existing owner mints the token.
   Admission of a later Create or CreateWithPreload permanently invalidates the token.
   A later failure does not restore the token.
   GH-531 adoption needs an existing Open survivor. Such a survivor prevents
   Prepare. Generation retirement revokes the old token before successor adoption.
3. Cancel revokes the exact preparation without terminal reservation.
   If the owner finds the token invalidated before Draining, refuse Commit without
   pausing reader dispatch or draining unrelated FS work. Otherwise, Commit pauses
   further reader dispatch. Drain all admitted FS terminal replies and leases while
   Running, even if invalidation is learned after Draining begins.
   Apply D-2 before resume or retryable Retained. Do not hold required locks during drain.
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

Keep committed decisions immutable after reservation. A lost reply does not prove noncommit. Reconcile only the original attempt using the negotiated GH-451 contract.
Reclaim nonterminal effect identities only after exact consumption and proven no-late delivery.
Keep known terminal effect identities until definitive old-link retirement, per #1432 `docs/engineering/gh451-conditional-quit-ipc-amendment.md:280@6a8607e52f9c0e8a8d4b757ffb047d97f1cf12f3`.
Retirement ends local reservations, not historical winner or unknown custody.
Never require post-EOF Status or Host exit before the terminal reply. Do not delay the existing terminal tail for a receipt-consumption confirmation.
Keep the committed decision and exact primary failure if Guardian preparation,
Echo retirement, correlated reply, local fence or cleanup fails. Do not synthesize
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
CI, deadlines, or the approved legacy KEL-96 file and digest.
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
Before shipping, close GH-451 Q-1, GH-451 Q-3, and GH-451 Q-4. Run the approved GH-451 Q-3 transport tests.

### T-3: Qualify the native terminal path

- **Blocked by:** T-2, B-4

Consume the independently qualified GH-451 Q-7, GH-451 Q-8 and GH-443 decisions.
Prove actual cleanup and process exit. Do not treat the native experiment as
a prerequisite for the reversible design decision.

## 7. Test plan & evidence

| Criteria | Planned witness |
|---|---|
| AC-1 | Exact facade veto, Save retry, post-Prepare Cancel and retained receipt with each missing proof, and B-3 surviving-Prepare recovery |
| AC-2 | Failed Create/CreateWithPreload, four live-state refusals, Released/Destroyed gate, successor adoption, generation and post-reservation admission fences |
| AC-3 | Authenticated FS Reply and Err, held lease and independent lease-only control |
| AC-4 | Independent viable-drain, failed-writer retirement and failure-accounting controls, exact error custody and successful Reply/Err |
| AC-5 | Separate held-reply and held-lease races, both reservation orders in each, code-only control, queued reader contender and one tail |
| AC-6 | Conditional Accepted before EOF, independent progress with no confirmation wait, and separate D-2 failure witnesses and controls |
| AC-7 | Four peer dispositions and destroy orders, held Bind refusal, legacy Quit bytes, listener policy, replacement window and independent no-fallback controls |

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
- **Check:** Approve this exact amendment blob and close GH-451 Q-4 with the GH-531 owner.
  Record precedence for qualified conditional default intent and the GH-531 criterion 32 exception.
  Apply it to qualified bound and positively selected but unbound default intent.
  Preserve legacy and unselected write-before-return behavior.
  Keep the approved KEL-96 and GH-531 text intact.

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
- **Check:** Close GH-451 Q-1 and GH-451 Q-3 with API and IPC owners.
  Approve exact error mapping, schema, consumption proof, retirement policy, and surviving-Prepare recovery.
  A proved local pre-send refusal proves only that the refused Cancel or Commit did not execute.
  Keep the original preparation in custody until the approved contract settles it.
  Do not replay the refused call or start a fresh attempt from pre-send proof alone.
  Do not synthesize a receipt or require automatic retry.

  After valid Prepare, force local pre-send refusal of Cancel and Commit in separate checks.
  Free the occupied request slot in each check without settling the preparation.
  Observe original preparation custody at the owner and refusal of replay and fresh attempts.
  Remove only the unresolved-attempt guard in each check.
  The check fails if replay or a fresh attempt starts before settlement.
  In the positive control, settle the original preparation under the approved contract.
  A fresh attempt is then eligible on the same qualified role if no terminal winner exists.

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

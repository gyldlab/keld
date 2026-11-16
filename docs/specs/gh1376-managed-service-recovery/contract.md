# Required behavior and source references

Part of: [index](index.md)
Owns: Source references and falsifiable acceptance conditions.

## 2. Spec refs

FACT: The following owners govern this proposal at the header commit.

- `docs/architecture/02-ipc.md`: IPC identity, contracts and wire review.
- `docs/architecture/03-security.md`: canonical authorization and profile boundaries.
- `docs/architecture/06-runtime-and-tooling.md`: Supervisor and platform custody.
- `docs/specs/kel75-principalized-bun-child-roles.md:244`: role lifetime and shutdown.

FACT: #1379 selects the service ownership question. #1384 owns recovery behavior.
The [existing review handoff](https://github.com/gyldlab/keld/issues/1376#issuecomment-6902783193) records the current deficiencies.
This specification narrows that handoff to one service/recovery seam.
It leaves PTY implementation, browser composition and package updates with their existing owners.

FACT: The required OpenCode source commit is `4423f42836aaaf2933bc7b94b35aca4e5bcce7c3`.
External source references below use that commit. Retrieval date: 2026-11-16.

## 3. Acceptance criteria

### AC-1: Activation has one accountable owner

- **Given:** Two authorized clients request the same account and canonical physical data root.
  Exercise both one installation and two distinct installed components with separate OS registrations.
- **When:** Both request activation before either observes a service endpoint.
- **Then:** Exactly one service generation obtains management admission. Its writer admission separately satisfies AC-7.
  Compatible clients discover that generation after readiness succeeds.
  A losing request returns a typed conflict or a compatible authenticated service reference. Incompatible artifacts cannot be silently adopted.
- **Negative control:**
  - **Single fault:** Key exclusive management admission by installation instead of account and physical data root.
  - **At-risk precondition:** Both requests pass authentication, policy and artifact validation.
    The two installations have distinct valid OS registrations for the same root.
    Observe management admission at its owner before downstream writer arbitration.
  - **Expected RED:** The management witness records two admitted management generations, even if writer arbitration later refuses one.

### AC-2: Connection authority does not administer

- **Given:** A client can connect and use a healthy disposable service.
- **When:** The client requests management without the corresponding operation and service-mode grant.
- **Then:** The owner refuses before changing state. The error identifies the missing grant class without exposing credentials.
  The client's permitted use operation still succeeds.
  Parameterize Activate, Adopt, Start, Stop, Replace, Update and Recover with valid disposable targets.
  Include grants valid for a different service mode. Local management grants cannot authorize an external or remote target implicitly.
- **Negative control:**
  - **Single fault:** For each operation, independently disable only its operation-and-mode management check.
  - **At-risk precondition:** The target and arguments are valid. The client lacks that operation's applicable grant.
    Observe the owning authorization decision before unrelated compatibility, custody or write checks.
  - **Expected RED:** The connection-only client obtains management admission for the tested operation or wrong mode.

### AC-3: Recovery waits for predecessor custody

- **Given:** A predecessor owns a session claim and a live tool descendant.
- **When:** Its endpoint disappears and a successor requests recovery admission.
- **Then:** Recovery remains blocked until the custody owner proves termination or complete enforceable fencing.
  A typed refusal identifies the unresolved custody. Endpoint loss and elapsed time do not satisfy the proof.
- **Negative control:**
  - **Single fault:** Replace the custody check with an endpoint-health check.
  - **At-risk precondition:** The successor has valid management authority. The predecessor's effect witness remains live.
    Observe recovery admission before downstream writer and operation checks.
  - **Expected RED:** The runtime admits successor recovery while the predecessor's conflicting effect witness remains live.

### AC-4: Desktop exit does not stop independent work

- **Given:** The installed service owns authorized background work with an independent progress witness.
- **When:** A window closes, desktop Quit commits, or the desktop crashes.
- **Then:** The service continues the work. Its generation and resource owner remain unchanged.
  Authorized service Stop blocks new admission and begins bounded cleanup.
  A vetoed desktop Quit performs no committed shutdown action.
- **Negative control:**
  - **Single fault:** Route accepted desktop Quit to service Stop.
  - **At-risk precondition:** No other shutdown condition exists. Background work remains authorized.
  - **Expected RED:** The witness stops before an authorized service Stop.

### AC-5: Retired bindings cannot deliver into successors

- **Given:** A request from one client binding completes after that binding retires.
- **When:** Its result reaches a replacement binding that uses the same application-visible resource name.
- **Then:** Delivery refuses the stale generation. The replacement receives no stale state or credential.
  An authorized outcome query can still retrieve a previously committed result.
- **Negative control:**
  - **Single fault:** Remove the receiving-binding generation comparison.
  - **At-risk precondition:** The result otherwise passes identity and schema validation.
  - **Expected RED:** The replacement's observer receives the predecessor's result.

### AC-6: Uncertain effects are not silently repeated

- **Given:** An independent sink records an irreversible effect. The service loses control before committing completion.
- **When:** A successor recovers the claim or a client retries the operation.
- **Then:** Recovery exposes an indeterminate outcome and a reconciliation action.
  It repeats the effect only after authoritative deduplication or deliberate authorization for a new attempt.
  A recorded successful outcome remains successful after client loss.
  Hold automatic effectful continuation of that execution, including continuation with a fresh tool-call ID.
  Authorized inspection and unrelated permitted work still succeed. Test both cancellation/completion orderings.
- **Negative control:**
  - **Single fault:** Automatically classify the indeterminate operation as replayable.
  - **At-risk precondition:** The sink has no deduplication. Recovery reaches the unfinished claim.
  - **Expected RED:** The sink records a second effect without reconciliation or new authorization.

### AC-7: Replacement cannot revive retired write authority

- **Given:** A predecessor still holds a database handle when replacement begins.
- **When:** The successor requests write admission and the predecessor attempts another write.
- **Then:** The owner admits at most one writer generation under the approved state contract.
  If effective fencing is unavailable, successor admission waits for verified predecessor termination.
  A stale binary cannot regain admission through saved registration or credentials.
- **Negative control:**
  - **Single fault:** Treat registration replacement alone as write-authority revocation.
  - **At-risk precondition:** The predecessor can reach its open database handle. Both writes target valid records.
  - **Expected RED:** A retired generation commits a write after successor admission.

### AC-8: Cleanup has an observable terminal result

- **Given:** Service Stop has valid authority and owns live children, streams and pending outcomes.
- **When:** Cleanup exceeds the approved drain or termination deadline.
- **Then:** The owner reports unresolved resources and blocks conflicting replacement.
  It preserves known outcomes and retains cleanup responsibility.
  A timeout does not become a success or proof of absence.
- **Negative control:**
  - **Single fault:** Mark cleanup complete when its deadline expires.
  - **At-risk precondition:** A retained process-identity witness proves one owned descendant remains alive.
    Observe the cleanup owner's terminal result before any independent replacement gate.
  - **Expected RED:** The cleanup owner reports completion while the unresolved resource remains live.

### AC-9: One dispatch claim binds the exact operation

- **Given:** Two authorized clients observe the same Prepared operation before either effect starts.
- **When:** Both request dispatch, or one changes the operation's inputs.
- **Then:** Only one atomic claim can release the non-replayable effect.
  The claim compares state, input digest, and current owner and writer admission.
  Changed inputs return InputConflict. The losing unchanged request returns the existing outcome or Pending.
  Also race first admission before either row exists. One operation is created and the loser receives a typed result.
  A storage uniqueness exception cannot escape as the protocol result.
  Unrelated permitted work still succeeds.
- **Negative control:**
  - **Single fault:** Remove the expected-state comparison from dispatch admission.
  - **At-risk precondition:** Both callers pass guard and writer admission before either effect starts.
  - **Expected RED:** The independent sink records two releases for the same operation.

### AC-10: Saturation preserves control progress and output integrity

- **Given:** Valid Use requests exhaust call or byte capacity while a terminal remains live.
- **When:** An authorized client requests Stop or revocation.
- **Then:** Reserved control capacity remains available. New excess calls return NotAdmitted(Capacity).
  Accepted outcomes remain queryable. Terminal lifetime does not permanently consume an RPC slot.
  Backpressure preserves the required output and replay contract without silent truncation.
  Test zero, maximum and maximum-plus-one limits independently for count and bytes.
- **Negative control:**
  - **Single fault:** Route Stop through ordinary execution permits instead of its independent permit.
  - **At-risk precondition:** Use traffic occupies every ordinary execution permit. Stop otherwise passes authorization and control-queue admission.
  - **Expected RED:** Stop cannot make progress within its observation deadline.

### AC-11: Compaction and restore cannot revive expired execution

- **Given:** Completed retry records surround an unresolved operation. An independent sink retains effect history.
- **When:** Records are compacted, old references retry, or an approved older snapshot is restored.
- **Then:** Unknown and expired references cannot dispatch. Unresolved outcomes remain available for reconciliation.
  Rejection floors cannot pass unresolved holes. Restore retires replay domains before automatic effects.
  New deliberate admissions remain possible within capacity and current authorization.
- **Negative control:**
  - **Single fault:** Treat an expired retry as a new authorized admission.
  - **At-risk precondition:** A completed operation has a compacted record and the sink does not deduplicate.
    Current caller authorization is valid. Observe outcome admission before any independent effect-authorization gate.
  - **Expected RED:** The state owner admits the expired reference as new work without deliberate new-attempt authorization.

### AC-12: Stop persists across OS activation and registration changes

- **Given:** A service has stopped through authorized management, or its OS registration is user-disabled.
- **When:** Discovery or an OS trigger activates a root without a new authorized Start.
- **Then:** No effect becomes admitted. Registration refusal returns a typed state without detached fallback.
  Graceful Stop first blocks admission and reconciles custody. Unregister cannot substitute for that sequence.
  Forced OS termination follows abrupt-loss recovery. Native termination does not prove remote-effect completion.
- **Negative control:**
  - **Single fault:** Ignore the persisted stopped state at root admission.
  - **At-risk precondition:** The OS starts a valid root after Stop. No new Start grant exists.
  - **Expected RED:** An independent progress witness observes newly admitted work.

### AC-13: Recovery cannot erase a committed success

- **Given:** Recovery reads DispatchIntent and pauses before its outcome transition.
- **When:** An authorized outcome owner commits success before recovery resumes.
- **Then:** The recovery transition compares the current state and version atomically.
  It preserves the committed receipt and returns success to an authorized query.
  Test both transition orders. An unresolved operation still becomes Indeterminate when no success exists.
- **Negative control:**
  - **Single fault:** Remove the expected-state/version comparison from the recovery transition.
  - **At-risk precondition:** Recovery read DispatchIntent before the outcome owner committed success. Both use valid writer admission.
  - **Expected RED:** Recovery replaces success with Indeterminate or removes the committed receipt.

### AC-14: Revocation and dispatch have an observable order

- **Given:** A valid operation is Prepared after authorization. Dispatch pauses before its effect-owner admission point.
- **When:** Revocation commits before dispatch resumes, or dispatch admission commits before revocation.
- **Then:** The first ordering refuses release. The second preserves possible effects and reports the actual outcome.
  Neither ordering invents rollback. A separate permitted operation still succeeds.
- **Negative control:**
  - **Single fault:** Remove the effect owner's current grant-revision comparison.
  - **At-risk precondition:** All other admission checks pass. Revocation committed before the held dispatch resumes.
  - **Expected RED:** The effect owner admits the revoked operation at its dispatch boundary.

### AC-15: A fresh operation identity cannot bypass an unresolved execution

- **Given:** An execution has an unresolved irreversible effect at an independent sink without deduplication.
- **When:** Restart, a subagent completion or a background wake proposes the same effect with a fresh operation reference.
- **Then:** The execution's effectful continuation remains held. Inspection and an unrelated execution remain available.
  Deliberate authorized new admission must link the unresolved attempt. A fresh identifier alone is insufficient.
- **Negative control:**
  - **Single fault:** Bypass the execution-wide unresolved-outcome hold for a fresh operation reference.
  - **At-risk precondition:** The new operation has valid inputs, grants and dispatch admission. No separate policy blocks it.
  - **Expected RED:** The sink records a second effect without reconciliation or deliberate new admission.

### AC-16: Disabled registration cannot cause an unmanaged fallback

- **Given:** The OS registration is disabled or its required user approval is absent.
- **When:** An otherwise authorized client requests activation.
- **Then:** Activation reports the exact registration state. No alternate unmanaged root or new effect appears.
  The approved registration case still reaches the normal non-effectful root-admission path.
- **Negative control:**
  - **Single fault:** On registration refusal, use a detached launch instead of returning the refusal.
  - **At-risk precondition:** The candidate executable is valid and launchable. An independent process witness observes root creation.
  - **Expected RED:** The activation owner creates an unmanaged root despite the registration refusal.

### AC-17: Graceful removal cannot unregister before custody reconciliation

- **Given:** Removal owns a valid grant, a live descendant and a retained unresolved remote outcome.
- **When:** The package owner requests graceful Stop followed by unregister.
- **Then:** Admission closes before drain. Unregister waits for the runtime's declared custody result and retained outcome state.
  An unresolved descendant blocks graceful completion. Forced OS termination reports abrupt loss, not successful graceful cleanup.
  Remote uncertainty does not disappear merely because local custody ended.
- **Negative control:**
  - **Single fault:** Invoke unregister before the runtime returns its custody result.
  - **At-risk precondition:** The held descendant remains live and the OS registration permits unregister.
  - **Expected RED:** The ordered owner trace records unregister before runtime custody reconciliation.

### AC-18: Restored state cannot authorize automatic replay from an older snapshot

- **Given:** A snapshot predates a released effect. The independent sink retains that effect after the snapshot is restored.
- **When:** The restored root requests automatic recovery admission.
- **Then:** The state owner retires old replay domains and requires reconciliation before effectful continuation.
  Missing snapshot records do not establish non-execution. Authorized inspection remains available.
- **Negative control:**
  - **Single fault:** Skip restore reconciliation at state-owner recovery admission.
  - **At-risk precondition:** Artifact, custody and authentication checks pass. Observe state admission before downstream execution checks.
  - **Expected RED:** The state owner admits automatic effectful recovery from the unreconciled restored snapshot.

### AC-19: Outcome access and cancellation require their own grants

- **Given:** A client can Connect and Use but lacks QueryOutcome and Cancel grants for a valid target.
- **When:** The client requests the committed receipt or cancels that target's active operation.
- **Then:** Each action refuses before disclosure or mutation. The permitted Use operation still succeeds.
  An appropriately authorized client can query and cancel under the outcome contract.
- **Negative control:**
  - **Single fault:** Independently disable only QueryOutcome or only Cancel authorization in each control run.
  - **At-risk precondition:** The target and request are valid. All unrelated policy checks pass.
  - **Expected RED:** The unauthorized client receives the receipt or admits cancellation at its owning boundary.

### AC-20: Saved registration cannot revive retired artifact admission

- **Given:** A successor is admitted. A retired artifact retains old registration and credential material.
- **When:** The retired artifact requests activation or writer admission with that saved material.
- **Then:** The applicable owner refuses the retired artifact-generation admission before resource release.
  A separately approved compatible rollback may obtain fresh admission. Older bytes alone are not categorically forbidden.
- **Negative control:**
  - **Single fault:** Replace current artifact-generation admission with acceptance of saved registration material.
  - **At-risk precondition:** The old artifact remains executable. Observe admission before independent writer or process cleanup checks.
  - **Expected RED:** The owner restores retired admission without fresh compatibility and management authorization.

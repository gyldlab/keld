# Outcome storage and recovery bindings

Part of: [index](index.md)
Owns: Proposed Q-3 state contract and the specification edit boundaries.

## 4. Design: Outcome bindings

### D-10: Exact outcome and storage contract

PROPOSED: This part extends D-7 in the existing core state owner after approval.
It does not create another database, journal owner or execution scheduler.
Q-3 remains open for the cross-owner bindings below. B-1 and B-3 remain open.

#### Atomic model

| Atom | Owner and input/output boundary | First independent falsifier |
| --- | --- | --- |
| Identity | Core state owner, upstream identity to persistent reference | Retrying a compacted request creates another effect. |
| First admission | Database transaction, issued reference to one operation | Concurrent callers expose a uniqueness error or create different operations. |
| Dispatch | State/effect owners, Prepared to one release token | Two callers release the same attempt. |
| Outcome | State owner, authenticated receipt to committed result | Recovery overwrites a success committed after its read. |
| Continuation | Execution owner, unresolved effects to resume refusal | A fresh tool ID bypasses an execution hold. |
| Conflict | State owner, conflicting terminal evidence to a durable execution hold | A core restart forgets a conflict while runtime continuity survives. |
| Output | SessionBlob/state owner, exact bytes to retained retrieval | Normal Job cleanup invalidates a committed receipt's retained output. |
| Durability | Database/VFS owner, commit to acknowledgment | Success is acknowledged before the required commit finishes. |
| Capacity | State owner, byte allowance to admission | New admissions consume an existing operation's completion allowance. |
| Restore | State/update owners, snapshot to recovery admission | A missing old record authorizes automatic replay. |

Identity binds dispatch, retention and queries. Custody gates writer admission.
Dispatch release determines uncertainty. Continuity evidence gates recovery after owner loss.
Byte accounting cannot prove physical durability or external-effect completion.

#### Source and reuse

FACT: Upstream bootstrap selects WAL and synchronous=NORMAL.
Source: [database.ts](https://github.com/anomalyco/opencode/blob/4423f42836aaaf2933bc7b94b35aca4e5bcce7c3/packages/core/src/database/database.ts#L39-L44).
FACT: The transaction adapter supports configurable BEGIN behavior and rollback after failed commit.
Source: [transaction adapter](https://github.com/anomalyco/opencode/blob/4423f42836aaaf2933bc7b94b35aca4e5bcce7c3/packages/core/src/database/drizzle/effect-sqlite/session.ts#L140-L224).
FACT: Session claims use time_suspended. A claim is not an effect receipt.
Source: [SessionStore](https://github.com/anomalyco/opencode/blob/4423f42836aaaf2933bc7b94b35aca4e5bcce7c3/packages/core/src/session/store.ts#L207-L260).
FACT: Restart describes at-least-once effects and resumes sessions and background subagents.
Source: [restart.ts](https://github.com/anomalyco/opencode/blob/4423f42836aaaf2933bc7b94b35aca4e5bcce7c3/packages/core/src/session/execution/restart.ts#L47-L62).
FACT: Job.completeBackground deletes the durable Job row and its finished in-memory entry.
Source: [job.ts](https://github.com/anomalyco/opencode/blob/4423f42836aaaf2933bc7b94b35aca4e5bcce7c3/packages/core/src/job.ts#L539-L547).

PROPOSED: Extend SessionStore, SessionExecution, SessionRestart, Job and the existing Database migration path.
Keep their event-commit hook. Commit a described outcome and its event in the same transaction.
The desktop storage database is not this authority. Generated schema files remain generated.

#### Persisted representation

PROPOSED: Add these tables to the existing core database.
These are proposed schema fields, not existing upstream fields or allocated wire IDs.

| Table | Exact fields and constraints |
| --- | --- |
| effect_meta | singleton=1 primary key; schema_version=1; ledger_epoch BLOB(16); admitted_writer BLOB(16); restore_hold boolean; logical_bytes and reserved_bytes nonnegative integers. |
| effect_domain | domain BLOB(16) primary key; ledger_epoch BLOB(16); execution_id BLOB(16); next_sequence positive integer; rejection_floor nonnegative integer; closed boolean. |
| effect_binding | primary key(execution_id, upstream_key); unique(domain, sequence); execution_id BLOB(16); upstream_key BLOB(32); domain BLOB(16); sequence positive integer; access_binding BLOB; retired boolean. |
| effect_operation | primary key(domain, sequence); execution_id BLOB(16); upstream_key BLOB(32); input_digest BLOB(32); state; version positive; claimant BLOB(16) nullable; grant_revision integer; dispatch_token BLOB(16) nullable unique; receipt BLOB nullable; attempt_of nullable reference; completed_at_ms nullable integer. |
| effect_conflict | primary key(domain, sequence), referencing effect_operation; execution_id BLOB(16); version positive; resolved_version nonnegative; first_outcome BLOB; latest_digest BLOB(32); reconciliation BLOB nullable. |
| effect_dependency | primary key(dependent_execution, source_execution, source_boundary); both executions BLOB(16); source_boundary BLOB(32); closed causal-kind enum; graph_revision nonnegative. |
| effect_hold | execution_id BLOB(16) primary key; desired_closed boolean; closure_revision, dependency_revision and registry_revision nonnegative; closure_reserved_bytes nonnegative; closure evidence at most 8 KiB. |

Use CHECK constraints for fixed blob lengths, state values and receipt length.
Constrain resolved_version to at most version. A conflict is active when those versions differ.
Constrain first_outcome to 16 KiB and reconciliation to 8 KiB. Reserve 32 KiB for each conflict record and metadata.
Constrain access_binding to 4 KiB. Q-2 owns its canonical encoding and authorization meaning.
That binding retains stable actor, policy reference, service mode, target, operation and admitted execution lifetime.
Preserve service-owned versus foreground lifetime across restart. An attachment loss cannot manufacture or revoke service-owned admission.
Current Guard policy still governs recovery, queries, cancellation and each effect release.
Sequences and versions use positive signed 64-bit integers. Refuse exhaustion before addition overflows.
Only the state owner mints domains and sequences. A domain belongs to one durable execution identity.
The runtime mints writer identity. The database compares that admitted value without minting another authority.
Add a durable execution identity to the existing claim. Preserve it across restart, steering and background wake.
Bind that claim to its retained access binding. Recovery cannot reconstruct authority from a caller's execution identifier.
Child work inherits the affected execution group. A new generated call ID does not create a new group.
Automatic fork and import continuation also retains its source execution and copied-history boundary in effect_dependency.
Keep the graph acyclic. Preserve dependencies after source conversation deletion or outcome-detail compaction.
Before enabling a dependent execution, install the current graph revision at its actual effect owners.
A later uncertainty or conflict closes affected dependents through D-9. A copied settled projection is not effect-certainty evidence.
Subagent completion events retain their originating execution, even when delivery wakes a different current execution in the parent session.
Join that source dependency before automatic wake or release. Session ID alone cannot supply the source execution.

Bind schedule-check and prompt-run origins separately to the admitted schedule identity and ordinal.
Retries preserve those bindings. Later automatic checks and runs depend on unresolved prior schedule work unless approved independence applies.
The initial schedule check needs a durable core-owned creation admission before any effect, even before the final schedule row exists.
Retry schedule creation through that same admission. Minting another schedule ID cannot establish that the initial check had no effect.
Explicit independent work uses deliberate current admission. Automatic continuation cannot select that exception.
Charge persisted dependencies and hold evidence to the existing logical budget. D-9 bounds their live mirrors and owner enrollment.
Reserve one 16 KiB closure allowance for each admitted execution before any effect or causal dependency can require its closure.
This allowance covers the complete effect_hold record, its evidence and bounded revisions. Enforce that complete-record limit before writing.
Materialize effect_hold during execution admission with desired_closed=false, closure_revision=0 and empty closure evidence.
Persist closure_reserved_bytes as 16 KiB minus the hold record's actual accounted bytes.
Commit that row and the matching effect_meta charges atomically. Reopening the core reconstructs the allowance from this row.
The first closure advances closure_revision from zero. False desired_closed cannot certify current runtime enrollment or permission.
Dependency admission reserves the dependent execution's allowance before enabling that dependency.
All operations in one execution share its allowance. Do not charge or release it once per operation.

FACT: Forks clear parent_id and retain fork_session_id while copying selected history.
Source: [projector.ts](https://github.com/anomalyco/opencode/blob/4423f42836aaaf2933bc7b94b35aca4e5bcce7c3/packages/core/src/session/projector.ts#L154-L205).
FACT: Schedule creation evaluates its initial check before inserting the schedule row.
Source: [schedule.ts](https://github.com/anomalyco/opencode/blob/4423f42836aaaf2933bc7b94b35aca4e5bcce7c3/packages/core/src/schedule.ts#L723-L789).
INFERRED: Child-only inheritance and existing schedule rows do not cover these causal entry points.
Bind upstream_key to the existing message ID, call identity and effect ordinal through Q-2's canonical encoding.
Create the mapping, Prepared row and receipt and conflict allowances atomically before effects become possible.
PrepareNew also establishes or verifies the execution's closure allowance in that transaction.
Never admit an operation that can require an unreserved hold record.
effect_binding preserves identity across domains and detail compaction. Repeated issuance returns its original reference, including Expired.
PrepareNew validates an actual eligible durable upstream object. Raw caller identifiers cannot create that object.
Never delete bindings during ordinary detail compaction or domain retirement. Charge every retained binding to the byte budget.
Remove a binding only with deliberate source deletion after its execution has no unresolved effects or active conflict holds.
A missing source refuses admission. An existing retired binding cannot mint another reference.
An external caller cannot establish issuance with its own nonce.
Do not cascade-delete outcome rows through session, message or Job deletion.
Keep a protected minimal upstream binding until its domain retires.
Refuse destructive deletion while unresolved effects or active conflict holds need that binding.

#### Owner interface and closed outcome

PROPOSED: Extend the existing state service with these operations.
Q-2 owns transport encoding, authorization, digest construction and public error-code allocation.

```typescript
type OperationRef = { readonly domain: Bytes16; readonly sequence: bigint }
type OutcomeState = "Prepared" | "DispatchIntent" | "Indeterminate"
  | "Succeeded" | "FailedNoEffect" | "FailedWithEffects"
  | "CancelledNoEffect" | "CancelledWithEffects"
type TerminalState = Exclude<OutcomeState, "Prepared" | "DispatchIntent" | "Indeterminate">
type StoredOutput = { readonly kind: "None" }
  | { readonly kind: "Inline"; readonly bytes: Uint8Array; readonly digest: Bytes32 }
  | { readonly kind: "Retained"; readonly digest: Bytes32; readonly bytes: bigint; readonly chunks: number }
  | { readonly kind: "Unavailable"; readonly digest: Bytes32 | null;
      readonly reason: "RetentionUnqualified" | "SourceLost" }
type OutcomeReceipt = {
  readonly kind: "NoRelease" | "VerifiedCompletion" | "VerifiedPartial"
  readonly evidenceDigest: Bytes32
  readonly output: StoredOutput
}
type StoredOutcome = {
  readonly operation: OperationRef; readonly version: bigint
  readonly state: OutcomeState; readonly receipt: OutcomeReceipt | null
}
type VerifiedOutcome = {
  readonly state: TerminalState
  readonly receipt: OutcomeReceipt
}
```

Bytes16 and Bytes32 mean exact byte arrays. Sequence and version use the signed 64-bit limits above.
VerifiedOutcome is an owner-private validated value. The shown fields do not authorize public construction.
Validation binds the operation, input digest, attempt token where present, terminal state and receipt to the authenticated effect owner.
The core state owner supplies pre-claim cancellation evidence. Callers cannot assert a receipt or terminal state.
Receipt output requires current query and output authorization against the retained binding.
The canonical receipt envelope is at most 16 KiB. The dispatch token remains internal.
The complete encoded VerifiedOutcome also fits 16 KiB. Pending and uncertain states can retain a null receipt.
Inline output contains at most 8 KiB of exact bytes. Check both limits before committing the receipt.
None means the operation produces no output. It cannot represent discarded or oversized output.
Unavailable states the missing retention guarantee explicitly. A Job identifier or hash cannot substitute for retained bytes.
Retained resolves an ordered manifest bound to this OperationRef through the existing SessionBlob owner.
The receipt stores only the bounded descriptor. Output chunks do not enlarge the 16 KiB envelope.

| Terminal state | Required receipt and classification |
| --- | --- |
| Succeeded | VerifiedCompletion proves the requested successful effect result. The output field states its separate retention status. |
| FailedNoEffect | NoRelease proves no effect release occurred and the failed attempt cannot later release. |
| CancelledNoEffect | NoRelease proves cancellation prevented release and the attempt cannot later release. |
| FailedWithEffects | VerifiedPartial or VerifiedCompletion proves the actual failed outcome and settled effects. |
| CancelledWithEffects | VerifiedPartial or VerifiedCompletion proves the actual cancelled outcome and settled effects. |

Receipt kind alone never selects terminal state. Finish consumes both fields of VerifiedOutcome and rejects an invalid combination.
VerifiedPartial is terminal only when no admitted effect remains capable of further completion.
Pending cancellation, remote uncertainty and an unauthenticated tool error cannot satisfy that condition.
Keep those attempts DispatchIntent under a live owner, or Indeterminate after loss of authoritative completion evidence.
An authoritative later receipt can resolve Indeterminate. It cannot silently overwrite a different committed terminal outcome.

| State-service operation | Exact semantic result |
| --- | --- |
| prepareNew(upstream, digest, writer) | Original or newly prepared StoredOutcome; typed InputConflict, Expired, Held, Capacity or Unavailable. |
| retry(ref, digest, writer) | Existing StoredOutcome; or typed InputConflict, Expired, Held or Unavailable. No new row or effect. |
| queryOutcome(ref, queryAuthority) | StoredOutcome or typed Expired or Unavailable. Current authorization failure returns the existing Guard error. No writes. |
| claim(ref, digest, expectedVersion, writer, grantRevision) | One internal Bytes16 dispatch token; existing StoredOutcome; or typed refusal. |
| cancel(ref, expectedVersion, cancelAuthority, writer) | Q-2 Prevented, Requested or AlreadyTerminal with the preserved outcome; typed Expired or Unavailable. |
| finish(token, verifiedOutcome, expectedVersion, writer) | Committed StoredOutcome; OutcomeConflict with the preserved record; or typed Expired or Unavailable. |
| recover(token, expectedVersion, writer) | Current StoredOutcome after conditional recovery; or typed Expired, Held or Unavailable. |
| reconcileConflict(ref, expectedConflictVersion, verifiedReconciliation, writer) | Recorded reconciliation and current outcome; typed ConflictChanged, Held or Unavailable. No automatic effect release. |

Reuse the existing Effect service and database transaction adapter.
Unavailable after a possibly committed change requires a query. It does not mean that no effect was admitted.
queryAuthority and cancelAuthority are private current-Guard results bound to the stored operation and its original access binding.
The state owner supplies writer admission internally. Public callers supply neither a writer identity nor an authority object.
QueryOutcome needs no input digest, dispatch token or writer admission. It does not release an effect or clear a hold.
Stop, restore_hold, ContinuityUnproven and execution holds do not deny otherwise authorized reads of available state.
Authorize before returning outcome details, existence or inline output. Apply the retained operation's current output access policy.
Use a read-only connection during denied write admission. Report Unavailable if the store itself cannot be safely read.

#### Transaction and release order

PROPOSED: Use behavior=immediate for prepareNew, claim, cancel, finish, recovery, conflict reconciliation and compaction.
Compare admitted_writer inside each write transaction. Refuse stale writers before mutation.
Runtime custody must exclude native-trusted direct writers. A cooperative row comparison is not physical fencing.
Use a five-second database busy deadline. Return typed Unavailable on pre-admission exhaustion.
Do not automatically retry effectful work after storage errors or uncertain commit results.

1. PrepareNew checks the durable upstream object, active domain, rejection floor, execution holds and every reserved byte allowance.
2. Mint or reuse its persistent reference. Create its mapping, Prepared row and reservations atomically.
3. Use unique insert-or-read. Compare immutable bindings when the row already exists.
4. Matching retries return the existing state. Changed input returns InputConflict. Unknown and retired references return Expired.
5. Reserve the actual effect-owner record. Claim then compares Prepared and expected version as one eligibility predicate.
   The winning transaction records DispatchIntent with the bound fresh dispatch_token.
6. Claim compares the digest, execution holds and current owner/writer admission. Only one changed row returns the token.
7. Commit DispatchIntent before presenting the token to the existing effect owner.
8. The effect owner consumes the token once under Q-2's revocation-ordered release gate.
9. Finish validates VerifiedOutcome and conditionally commits its receipt and outcome before acknowledging the corresponding durability class.

Do not hold a SQLite transaction or effect-owner mutex during external work.
An interruption between intent commit and release stays uncertain without authoritative proof that release did not occur.
Lost commit acknowledgments require a query with the original reference. They cannot create another attempt.

Cancel compares Prepared, expectedVersion and a null dispatch token within the same transaction as claim.
When that comparison wins, cancel records CancelledNoEffect and core-authenticated NoRelease evidence, then increments version.
No releasable token is issued. Retire any unused non-authorizing reservation.
The losing claim reloads the cancelled outcome and cannot return a release token.
When claim wins, cancel returns Requested and invokes the existing effect owner's interruption path.
Requested preserves possible effects. A terminal race returns AlreadyTerminal without changing the stored outcome.
An expected-version mismatch reloads state. It never authorizes a new claim or an invented no-effect receipt.

#### Bounded output retention

PROPOSED: Commit complete eligible small output as Inline. Commit larger qualified output through SessionBlob as Retained.
Its existing source owner retains the bytes until the selected persistence path acknowledges every required byte and reference.
Job.completeBackground can then retire its source without invalidating the retained result.
Inline uses the existing outcome reservation. Retained uses the reviewed blob credits and ordered manifest below.
Never truncate output to fit the envelope. Missing, unqualified or quota-refused retention uses the explicit Unavailable variant.
Retain a known successful effect outcome even if output retention failed. Do not misreport the effect as unexecuted.
An Unavailable output does not acknowledge durable retention of the result bytes.
The existing output owner may still provide authorized live output, but that availability is not a durable receipt guarantee.
The selected large-output binding still needs Q-3 approval and actual AC-29 evidence. An ephemeral pointer cannot qualify it.

#### Outcome transitions and holds

| Current state | Permitted transition and evidence |
| --- | --- |
| Prepared | DispatchIntent after the unique claim; CancelledNoEffect after cancellation before any release claim. |
| DispatchIntent | A verified known outcome, or Indeterminate after owner loss. |
| Indeterminate | A verified known outcome, or a separately authorized linked attempt. Preserve the old row. |
| Known terminal outcome | Same-result idempotent query/receipt only. A conflicting receipt returns OutcomeConflict and holds continuation. |

Every state change compares state and version in its write transaction.
Recovery changes only the exact DispatchIntent version it inspected. If another transition won, reload and return that result.
Recovery and cancellation cannot replace Succeeded or erase its receipt.
Cancellation after possible release is not proof of no effect. Preserve partial effects or uncertainty.
A tool error, failed output encoding or missing destination record does not alone prove FailedNoEffect.
No-effect reconciliation must prove that the old attempt cannot still complete later.

When a verified receipt conflicts with a terminal outcome, preserve the original operation row and receipt.
Begin D-9 closure before persisting that hold. A partial acknowledgement remains Pending/Unresolved, including after conflict persistence.
In the same transaction, create or update effect_conflict and commit the conflict event.
The first conflict stores its complete VerifiedOutcome. Later conflicts preserve that evidence and advance version with the latest authenticated digest.
This bounded record is not a complete history of every repeated receipt.
An active effect_conflict holds its execution across core restart, live handoff, compaction and domain retirement.
Reserve its full 32 KiB allowance during PrepareNew. Keep that reservation until operation detail can be safely retired.
If recording the conflict fails, return Unavailable and close that execution's release gate before reporting the storage failure.
That fail-closed admission remains owned through handoff. If it cannot be transferred, the successor remains ContinuityUnproven.

Only reconcileConflict can advance resolved_version to the expected current conflict version.
Its private verifiedReconciliation binds the operation, preserved outcome, first conflict, latest digest, version and current management authority.
The effect owner must establish the authoritative terminal outcome and that the old attempt cannot complete later.
Store the reconciliation evidence with resolved_version in one transaction. A concurrent conflict invalidates the expected-version comparison.
Reconciliation never overwrites the preserved terminal outcome or clears another operation's conflict.
If evidence cannot validate the preserved outcome, keep the conflict active and expose the discrepancy for separate owner resolution.
Deliberate linked-attempt authorization does not clear a conflict hold.

Before automatic continuation, check the durable execution group for Indeterminate, retired-owner DispatchIntent and active effect_conflict records.
Also check restore_hold and ContinuityUnproven. Prepared alone does not establish an uncertain effect.
Include tool retries, SessionExecution.resume/wake, subagent completion, scheduled work and startup recovery.
All effectful entry points use the same hold. Unknown tool behavior defaults to effectful.
Q-2 serializes release with acknowledged execution closure, including causal dependents and newly enrolled effect owners.
An earlier Prepared or claim result cannot bypass a closure acknowledged before the release-admission point.
Stored hold intent does not certify complete closure when a capable owner remains unreachable.
Live pending work retains its current owner. A pending response alone does not imply owner loss.
Inspection and unrelated authorized executions remain available.
Deliberate new-attempt authorization names the unresolved reference and acknowledges possible duplicate effects.
It permits only the exact linked attempt under that grant. It does not clear the entire execution hold.
Preserve the old outcome and authorization record. Broader continuation needs separately scoped authorization or reconciliation.

#### Durability and storage limits

PROPOSED: Configure WAL, synchronous=FULL and foreign_keys=ON on each authoritative writing connection.
Read back the settings before write admission. Refuse unsupported managed storage profiles.
Use the existing database connection construction. An adapter setting cannot fix another connection's policy.
Distinguish admission recorded, process-crash committed outcome, and qualified power-durable outcome acknowledgments.
Expose only the first two until native storage evidence qualifies the third.
FACT: SQLite WAL/FULL adds synchronization "after each transaction commit".
Source: [SQLite synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous). Retrieved 2026-11-16.
FACT: A write can fail with SQLITE_FULL or SQLITE_IOERR.
Source: [SQLite transactions](https://www.sqlite.org/lang_transaction.html). Retrieved 2026-11-16, "database or disk full".

| Initial prototype limit | Required accounting |
| --- | --- |
| 128 active domains | Count domains with live or unresolved work. Refuse extra domains at the limit. |
| 1024 unresolved operations | Count Prepared, DispatchIntent and Indeterminate. Refuse new effects at the limit. |
| 16 KiB outcome envelope | Reserve the full envelope during PrepareNew, including up to 8 KiB inline output, evidence and metadata. |
| 32 KiB conflict allowance | Reserve one bounded conflict record per operation before admission. Retain the allowance through terminal detail retirement. |
| 16 KiB closure allowance | Reserve one complete hold record per execution. PrepareNew verifies its retained reservation alongside receipt and conflict allowances. |
| 64 MiB logical outcome budget | Charge actual records, bindings, domains and all receipt, conflict and closure reservations atomically. |
| 30-day detailed retention | Use completed_at_ms and floor rules. Never expire unresolved outcomes or active conflicts. |
| 256-row compaction batch | Commit a checked floor and its row deletions together. Yield between batches. |

These are prototype limits, not measured shipping limits or total database-size guarantees.
Completion and closure can consume their existing allowances when new-admission capacity is exhausted.
Convert reserved closure bytes to stored bytes atomically. Updates reuse the same bounded hold record and remaining allowance.
Adjust closure_reserved_bytes and effect_meta in the same transaction as the hold write.
The stored hold bytes plus closure_reserved_bytes must remain 16 KiB until safe allowance retirement.
Ordinary admissions cannot spend closure reserves. Closure does not require a fresh ordinary admission or unused ordinary capacity.
Retain the allowance while any live execution, retained outcome, conflict or causal dependency can require that execution's closure.
Release it only after execution retirement and removal of those closure obligations. Detail compaction alone cannot release it.
Run AC-36 with saturated ordinary capacity and multiple affected executions, including dependents without their own operations.
Count actual stored byte lengths. Do not charge compressed estimates.
SQLite pages, indexes, WAL and filesystem overhead require separate physical-storage measurements and native fault injection.
Logical reservation does not reserve physical WAL space against other applications.
If an outcome commit fails, report Indeterminate and invoke D-9 closure for that execution and its causal dependents.
Retain Pending/Unresolved for missing owner acknowledgements. A local health flag cannot certify that remote releases stopped.
Retain the durable intent. Do not acknowledge success or delete evidence to recover space.
Queries, reconciliation and cleanup remain available where their own storage operations succeed.

#### Compaction, restart and restore

PROPOSED: An active domain admits only owner-issued references above its rejection_floor.
Retry never creates a missing row. Fresh admission binds a newly issued upstream operation.
Advance the floor only over a contiguous prefix of terminal operations eligible for detail retirement without active conflicts.
Do not pass an unresolved hole. Stop compaction when an unresolved hole prevents safe progress.
Update the floor and delete covered operation detail in one transaction. Preserve effect_binding and the domain floor.
Delete eligible inline receipts and resolved conflict detail in that transaction. Do not retain a dangling output reference.
Set bindings retired when their domain closes. Preserve those bindings until the source-deletion rule permits removal.
A closed or unknown domain refuses new dispatch. Retain required unresolved and queryable records.
Retiring a domain cannot authorize its old references in another domain.

On owner loss, classify prior DispatchIntent rows before scheduling automatic effectful continuation.
Automatic recovery also needs runtime-proved custody and applicable failure-class continuity.
Fresh activation after loss of all retained owner state returns ContinuityUnproven until an approved witness or reconciliation exists.
Do not infer continuity from a PID, elapsed time, database epoch or successful lock acquisition.
This hold may require reconciliation after root crash or reboot. Full required continuity remains a qualification obligation.
Controlled restore first closes admission and proves writer custody.
An authenticated restore hold must survive replacement of the restored database.
Bind that hold to the existing state/update owner. Do not create an adapter recovery journal.
Use one owner-only .keld-control/ barrier in the retained physical service root, outside the selected database/snapshot payload.
Every installation sharing that root checks the same barrier before writable bootstrap.
Bind it to the restore attempt, phase, physical root, generation and retained pre-restore control revision.
Persist the barrier after exclusion and before payload replacement. Retain quiesced prior state for reconciliation.
Restore and validate the payload. Retire old replay domains and record Stopped beyond the retained control revision.
Clear the barrier last, after the scoped reconciliation commits. An interrupted phase keeps admission closed.
Rotate replay domains after restore. Hold pre-existing effectful continuations for reconciliation.
An older snapshot's absent records prove nothing about external effects.
Unannounced replacement by a native administrator is outside the stated storage threat model.
Binary rollback preserves the live database and refusal floors. Incompatible older binaries cannot obtain writer admission.

#### Required owner bindings before Q-3 closes

UNKNOWN: Q-1 has no qualified boot/lineage witness after loss of all retained owner state.
The selected safe behavior is a recovery hold. Native proof must establish any less restrictive behavior.
PROPOSED: Put service_control and service_attempt in the existing OpenCode Database.Service at its admitted configured disk file.
The managed profile rejects memory and verifies WAL, synchronous=FULL and foreign keys before migrations.
Keep ordinary standalone defaults unchanged. A durability profile cannot supply writer custody or authentication.
service_control retains desired_state, control_revision and removal_hold. Its key binds the admitted physical service target.
service_attempt retains an owner-issued identity, actor, target, mode, action, input digest and captured expected revision.
Prepare reserves the attempt without changing desired state. Apply uses the existing immediate transaction adapter.
Commit the revision comparison, state change and immutable attempt disposition in one transaction.
An accepted retry returns its original disposition after later Stop. A stale Start cannot refresh its captured revision.
Stop and removal require their durable ordering before acknowledgement, descriptor retirement and unregister.
An uncertain commit queries the same attempt. A stale expected revision cannot rebase itself or repeat the action.
The existing package transaction still owns Update and Replace. Link its identity without duplicating its journal.
Bound attempt count, encoded bytes and retention before exposing this control API. Retired references never mint a fresh attempt.

PROPOSED: Extend SessionBlob's content-addressed BlobTable with outcome-owned references and an ordered per-operation chunk manifest.
Source: [SessionBlob](https://github.com/anomalyco/opencode/blob/4423f42836aaaf2933bc7b94b35aca4e5bcce7c3/packages/core/src/session/blob.ts).
Each manifest entry binds operation, ordinal, blob hash and exact byte length. The descriptor retains total bytes and whole-output digest.
Retain those references independently of Job and session deletion. Every blob deletion and GC path must honor both owner classes.
Do not widen ordinary media reads to accept outcome-only references. Outcome reads require the operation-bound current grant.
Managed retention cannot depend on the experimental.media_blob presentation flag.
Publish Retained only after all bytes, references and the manifest commit. Bound chunk sizes, readers and aggregate storage credits.
Exact limits and the complete GC census require the existing output owner's review before Q-3 closes.
Quota or missing bytes must return an explicit result. Silent truncation cannot qualify a required full-output journey.
This extends the existing blob owner. It creates no separate spool or database.

UNKNOWN: Protected restore publication, initial layout migration and native durability still require state/update-owner approval.
The existing Windows activation journal does not already implement this restore transaction.
The barrier cannot establish continuity after an unannounced whole-root administrator rollback.
These selected control, output and restore bindings require exact owner review before specification approval.
UNKNOWN: Durable schedule-creation admission and cross-execution dependency hooks need the existing core owners' exact binding review.
Initial checks, fork/import continuation and delayed subagent completion cannot bypass that prerequisite.
FACT: KEL-53 Slice A requires dataMigration:none and provides no migration hook.
Source: `docs/specs/kel53-full-package-activation.md:606@413c128e5375df3ffa241809db651bf7fd27e498`.
This schema change requires a coordinated migration/compatibility amendment before shipped activation.
An old binary that ignores outcome tables must not obtain managed write admission.
Actual core instrumentation, full effect coverage, native durability and product journeys remain unqualified.

#### Required discriminating checks

PROPOSED: Run AC-27 through a core restart while qualified runtime continuity survives.
The original terminal receipt must remain queryable. The conflict must hold only its affected execution.
Keep only effect_conflict in memory for the mutation. After restart, inspect its required authenticated evidence at the state owner.
Missing evidence must fail before independent effect_hold or runtime release gates can mask that loss.
AC-34 separately checks closure. Do not require a sink release to prove this storage mutation failed.
Run AC-28 against the actual cancel and claim operations in both transaction orders.
Cancellation first must produce no token or sink effect. Claim first must preserve possible effects and permit an actual later outcome.
Run AC-29 with inline output through real Job.completeBackground cleanup and core reopen before detail expiry.
Verify exact bytes and digest under current authorization. Remove only the inline copy to fail that retrieval.
Repeat with retained multi-chunk output after Job and session cleanup. Verify order, full bytes and the whole-output digest.
Remove only outcome retention, then mutate GC separately. Observe candidate deletion before independent foreign keys can mask it.
A blob retained solely by the outcome must remain unavailable through ordinary media reads.
Exercise Unavailable output separately. It must not satisfy a full-output retention assertion.
Query known outcomes during Stop, each hold and denied writer admission. Denied current query grants must expose no outcome details.
Test every terminal-state/receipt-kind pair. Reject mismatches while preserving the valid outcomes in the matrix.
Race conflict reconciliation against a later conflicting receipt. Only the unchanged expected conflict version may clear that hold.

## 5. Boundaries

This PR changes only this draft specification.
Implementation candidates are the canonical runtime owner and the OpenCode managed-service adapter.
The existing core recovery entry point and state owner require coordinated changes after approval.

Must not touch runtime source, public contracts, guard policy, KEL-76 implementation, credentials or unrelated owner branches.
Do not replace canonical architecture text before the proposed contract is approved.

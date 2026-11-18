# Integration, approval, and qualification

Part of: [index](index.md)
Owns: The combined contract, task dependencies, approval record, and evidence limits.

## 4. Design: Integration

### D-80: Atomic owners and dependency edges

The following atoms decide the design. A dependent atom cannot inherit another atom's evidence.

| Atom | Owner and boundary | Input to output | Failure and first falsifier |
|---|---|---|---|
| Identity | Pack codec and KEL-135 | Signed bytes to typed expected identity | One mutated byte changes interpretation or bypasses refusal |
| Authentication | Guard and existing signature owners | Retained image, chain, signatures to verified observations | A wrong valid signer passes one exact policy |
| Freshness | Standard metadata client | Root and role graph to exact ready revision | Equal-version replacement or expired role changes authority |
| Persistence | Existing KEL-53 native writer | Verified records to durable current generation | One crash cut invents first use or restores old authority |
| Admission | Guard worker and KEL-270 | Actual worker, retained objects, revision to consumed permit | Worker or object replacement reaches a protected creation |
| Lifecycle | Existing transaction and KEL-135 | Complete before/after views to commit or finite rollback | Candidate and predecessor use one browser store concurrently |
| Recovery | Existing verifier and transaction | Independent directive to one exact replacement | Missing U1 becomes an automatic authorization fallback |
| Evidence | Independent reviewer and acceptance owner | Exact source plus observations to qualified support cell | A source inspection is reported as a native pass |

Identity bytes feed authentication. Authentication and freshness independently bind the same candidate.
Admission consumes both results and retained native custody under the required lease order.
Lifecycle cannot select an artifact outside that complete binding.
Recovery replaces authority only through its independent, bounded operation.
Evidence qualifies each edge separately and then the joined behavior.

### D-81: Reuse and migration

Reuse the existing Pack PE parser, Guard Authenticode verifier, KEL-53 writer, and KEL-135 physical-store lease.
The standard TUF client remains the metadata protocol owner.
The selected client needs the source-bound corrections in D-20. A storage adapter alone is insufficient.
No second implementation of TUF, Authenticode, extraction, or activation is selected.

The identity v2 codec is additive. Existing v1 bytes and their decoder remain unchanged.
An authenticated legacy bridge remains a separate conditional support cell.
The provenance change uses a new schema and lifecycle domain. Old observations cannot authorize the new domain.
No performance improvement is claimed. Native cost remains an implementation measurement.

### D-82: Approval and implementation order

Complete the four owner contracts and the combined acceptance mapping before requesting sign-off.
Run isolated independent reviews. Give each reviewer one named lens and the exact source snapshot.
Independently refute each finding. Fix each surviving design defect and recheck the changed boundary.
Record unresolved implementation evidence as a release gate, with an owner and first completion check.
An unresolved design choice or contradiction prevents approval readiness.

The human approval records the exact source commit and the accepted support boundaries.
Record the approval on the specification PR or its KEL-270 decision thread.
A direct conversation approval can be recorded with its exact wording and version.
Then change the status to approved and start only tasks whose dependencies pass.
Do not treat a draft PR, a review pass, or an instruction to continue drafting as human approval.

Design approval does not require execution of unimplemented production cases.
Implementation qualification requires the product tests and independent code review.
Merge and release remain separate decisions under their existing owners and evidence gates.

### D-83: Supported cells and authority boundaries

| Cell | Design disposition | Required execution before a claim |
|---|---|---|
| Fresh v2, Windows x64 direct per-user | Defined | Commissioned signatures, full admission, crash and native cases |
| Different apps sharing one repository ID | Outside this profile | Each app needs its own commissioned repository; enrolled-ID reuse refuses |
| v2 planned key or identity transition | Changed policy conditional on D-84/D-90; positive cell currently unqualified | Fresh metadata, U1/U2 proofs, joined browser/floor/LKG and D-87 cases |
| v2 loss-only manual recovery | Defined under D-61 prerequisites | Independent credential custody, exact directive, native ceremony and recovery cuts |
| v1 to v2 | Conditional on B-2 | Original-signer bridge and joined retained-data qualification |
| Known managed installation | Refuse direct mutation | Zero-network refusal and zero direct-writer authority |
| Machine-seamless or managed takeover | Outside this feature | Existing deployment-owner work; no implication from direct tests |
| Suspected system compromise | No in-place safety claim | Clean-system operator recovery and explicit data disposition |
| Lost independent recovery authority | No in-place replacement | Separately authenticated clean installation and retained original evidence |

The release record must name each supported cell. A passing fresh install cannot qualify migration or recovery.
Do not ship a broad recovery claim while the independent authority or joined evidence is missing.


### D-84: Planned policy changes have a distinct repository owner

D-7's planned transition, not D-28's manual recovery capability, authorizes
an installed policy A to destination policy B change. The existing KEL-53
transaction owner coordinates the metadata adapter and application writer.
There is no second writer, generic policy setter or fresh-enrollment fallback.
The adapter accepts only a private, noncloneable `VerifiedPlannedPolicyTransition`.
Its constructor requires the admitted source, exact D-7 document, U1 and
applicable U2 signatures, fresh source-policy metadata authorization and the
fully verified destination catalogue, policy cohort and candidate.
A candidate's own signed policy is insufficient.

The MVP supports policy-changing transitions only when the complete
protected repository-owner census proves this is the sole installed channel
and no other channel has current, LKG, previous-LKG, pending admission or
retained policy authority in this repository. Prove that condition under the
repository lease before preparation and recheck before commit-authorized.
Unknown, unreadable or incomplete ownership evidence refuses the transition;
absence from a caller-supplied list is not proof. Use the independently versioned
D-90 completeness proof from the existing owner and authenticated discovery, not a new
per-channel store or an environment override. An existing sibling channel
blocks before journal/pointer publication or candidate execution.
Same-policy multiple-channel operation remains unchanged. Cross-channel
policy migration is unsupported until separately specified and approved;
this design does not claim a post-switch catch-up path for a stranded sibling.

Preserve repository ID, app ID, target, metadata profile, enrollment identity,
recovery epoch, current accepted root bytes and all historical role, application
and UTC floors. Planned policy change does not rebootstrap a repository.
The destination bootstrap-root fields remain equal to the source policy's
commissioning fields; actual root rotation uses D-24, not this operation.
Verify the destination catalogue under its authorized catalogue key and the
source-authenticated TUF target bytes. Source authorization and destination
catalogue verification are independent predicates, not interchangeable trust.
Changed origins cannot fetch a substitute during commit. Future refreshes use
B's origins with the preserved root and floors, never B as a fresh trust store.
If the destination cannot preserve these invariants, refuse this transition.
A broader root replacement uses only its separately authorized recovery cell.

The metadata adapter prepares a successor at source sequence plus one using
checked arithmetic. It copies the complete ready role graph and rootRefresh
null from the admitted source generation, changes only `policySha256`, sequence
and previous-generation digest, and preserves epoch and UTC floor exactly.
The destination graph's catalogue target must authenticate the exact admitted
candidate under both the source TUF graph and destination catalogue policy.
No root or role acceptance occurs during this policy-only publication.
The adapter binds both generation digests and both policy digests into the
existing transition journal's complete before/after state.

### D-85: Durable planned-policy fencing

Extend the existing versioned KEL-53 transition case, not the manual-recovery
schema. Its repository record is `transitions/<attemptId>.json`, with the
D-64 random attempt ID and a 65,536-byte cap. Canonical ordered fields are
`schema,attemptId,repositoryId,appId,ownerSid,sourceGenerationSha256`,
`destinationGenerationSha256,sourcePolicySha256,destinationPolicySha256`,
`beforeProvenance,afterProvenance,transition,signatureU1,signatureU2`,
`metadataAdmission,sourceSequence,destinationSequence,recoveryEpoch,phase`.
Schema is `keld.windows.planned-policy-transition/v1`.
Digest fields use h32; identity and integer fields use D-23/D-64 domains.
The six provenance/transition/signature/admission values use D-9 evidence
references; signatureU2 is null exactly when D-7 requires one signature.
All other fields are required. No unknown field, rewritten plan or path input
is accepted. D-9's complete policy evidence sets remain required and bounded.
Phases are `prepared`, `candidate`, `commit-authorized`,
`metadata-committed`, `complete`, `aborted`, `complete-cleaned`, or `aborted-cleaned`.
Only a terminal outcome may advance to its matching cleaned phase; no reverse
or cross-outcome transition is allowed. The immutable plan normalizes phase to
`prepared`, so neither terminal nor cleaned state alters its digest.

Before this record, finish D-24 ordinary residue and committed-history capacity
retirement, then seal the exact source and successor under the metadata lease.
Retain source generation and both policy cohorts until every dependent view
releases them. A live reference blocks retirement; never lower a floor for space.
Publish, flush and reread the repository prepared record first. Then publish,
flush and reread the application prepared journal bound to the same attempt,
immutable prepared-plan digest and both provenance digests. The plan digest
is over the canonical repository record with phase fixed to `prepared`; retain
those exact bytes so later phase updates cannot change the journal binding. Only then publish and
reread D-68's `planned-transition` pointer selecting the fixed repository record.
The prepared app journal is an inert journal-only write, not execution, current
selection or authority change; no such effect or successor generation precedes
the pointer. This distinction also governs D-68's initial no-effect condition.
Ordinary application launch fences on its prepared journal before the pointer.
A missing or different application journal refuses advancement.
All metadata open, refresh, admission and ordinary launch selection must check
this repository pointer. An unrelated channel cannot mutate through its own
installation lease while this repository transition is pending.

Hold metadata, installation and physical-profile leases in the existing order.
Only the recorded owner may provide the attempt-bound candidate after-view.
Ordinary launch remains blocked, including after process restart when leases
have been released. The pointer is a fence, not an authorization by itself.
Every store-open and pre-mutation census checks the protected transitions
directory and matching application journal ownership, even without a pointer.
An unreferenced prepared record or journal must be reconciled before mutation:
prove exact source state, no candidate, successor or execution/current effect and
abort, or halt for owner disposition. Never infer no effect from one missing
file. A repository-only prepared record from the first publication cut permits
only this no-effect abort, never forward execution. An application-only record
or mismatched pair is corruption, not an adoption or deletion opportunity.

### D-86: Commit and rollback cuts for planned policy changes

Process restart or loss of the live admitted worker before commit-authorized
permits only verified source rollback/abort, never reconstruction of a permit
from persisted ReadyRevision or admission evidence. After terminal cleanup, a
new acquisition requires a fresh authenticated attempt and new live authority.
Before candidate execution on the still-live original worker, revalidate original authority, live expiry,
monotonic deadline, source generation, complete custody and every D-3 predicate.
Candidate failure before `commit-authorized` retires the candidate through the
existing process/browser barriers, restores the complete source application
view, and retains any raised application floor. Metadata remains A.
Only then may the owner durably record `aborted` and remove its matching pointer.
The journal and source evidence remain until terminal readback and pointer absence.

After accepted health and completed retirement/browser-release barriers,
recheck the exact source and all authorization/expiry predicates under the leases.
Durably publish and reread `commit-authorized` before any B generation write.
This freezes the authorized complete transaction; it grants no different target,
new graph or reusable policy authority. Subsequent expiry cannot replace inputs.
Publish the exact successor through D-24's retained-parent flush/readback and
single current-record replacement. Then record `metadata-committed`.
Keep the repository fence while publishing and rereading complete B application
provenance through the existing activation journal. Only complete coherent B
readback permits `complete`, then matching pointer removal.

After `commit-authorized`, do not restore metadata A or invent an epoch change.
Complete only the recorded B transaction. If candidate/application execution
subsequently fails, the exact finite D-8 LKG binding may run under the B
recovery view; it cannot restore general A/U1 release authority. Preserve the
physical browser store and application floor. If no compatible bound LKG exists,
refuse admission before `prepared`. A post-commit failure without a provable
coherent view remains fenced for explicit owner recovery, not ordinary launch.

| Durable cut | Only permitted restart action |
|---|---|
| Repository prepared, application journal absent | Prove source/no-effect state and abort; never resume forward. |
| Both prepared, pointer absent | Reconcile both records; prove no effects and abort. |
| Prepared/candidate, metadata A after restart | Retire candidate and perform only verified source rollback/abort; no persisted permit resurrection. |
| Commit-authorized, metadata A | Complete the exact sealed B generation and application transaction, without new network inputs. |
| Commit-authorized, metadata B | Verify the exact successor; advance phase without a second sequence or policy switch. |
| Metadata-committed, old/partial application view | Keep the repository fence and complete exact B provenance or its bound LKG view. |
| Complete, pointer present | Reverify coherent B state, then remove only the matching pointer. |
| Aborted, pointer present | Prove source metadata and restored source application coherent; retire candidate first, then remove pointer. |
| Corrupt, foreign, missing required journal or unexpected generation | Halt with fence/evidence retained; never select an older current record. |

Terminal publication order is durable application terminal journal first, then
matching repository terminal record, then exact pointer removal and durable
absence readback. For the repository-only prepared crash cut, independently prove no effects and
write/read back a same-attempt inert terminal no-effect application journal
bound to the immutable plan before the repository terminal record. This is
cleanup evidence only, never an authorization or synthesized live permit.
Before terminal publication, independently verify coherent
physical state, terminal application journal and candidate retirement. Reuse
D-68's one bounded completed diagnostic slot for the authenticated repository
terminal record, not an unbounded tombstone collection. No new mutation or
attempt may intervene before the matching cleaned phase is durably recorded there.
After pointer absence, retire only the exact matching terminal app journal.
Its absence is accepted only under that authenticated repository terminal record,
coherent final-state comparison and no-intervening-mutation gate. Flush/read back
the matching `complete-cleaned` or `aborted-cleaned` phase before reopening
mutation. Restart repeats these steps
idempotently; unknown state preserves the fence. Retain the repository terminal
record until normal exact-slot replacement, and apply D-88's audit-only rule.
Absence of both records never by itself proves recovery or restores authority.

A partial destination-generation file cannot be overwritten, adopted or deleted
by generic refresh. Resume only exact complete bytes through the existing writer;
otherwise retain the fence for named KEL-53 operator disposition. This version
makes no automatic liveness claim for corrupt planned-transition publication.
Ordinary D-24 cleanup cannot run through this pending pointer.
If an unexpected sibling using A is discovered after commit, refuse its
ordinary acquisition and preserve evidence for owner disposition. Do not
promise automatic catch-up, restore repository A, initialize another store,
or silently rewrite that channel's provenance. Such discovery violates the
pre-switch support predicate; it is not a qualified normal transition state.

### D-87: Required joined falsifiers

Extend AC-3/AC-4/AC-26 with real A-to-B publisher-policy and origin-change
positives, preserving the same accepted root, role floors, epoch and browser
store. All cases remain NOT_RUN until native implementation exists.
Cut at journal/pointer publication, candidate start/retirement, commit-authorized,
successor write/flush/current replacement, provenance publication, complete and
pointer removal. Repeat process death and physical power loss separately.
Independently observe repository policy/generation, provenance, launch authority,
current/LKG, profile lease and application floor at each restart.

Mutate one boundary at a time: omit repository fencing; switch policy before
commit-authorized; accept an unbound successor; use manual recovery capability;
reset a floor/epoch; fetch replacement bytes during completion; allow another
channel through a pending pointer; restore general U1 authority after commit;
or remove the pointer before coherent application readback. Each must fail at
its own predicate with all unrelated signatures valid. Preserve positive,
intended RED and restored-pass receipts for the exact implementation.
A broad refusal or a synthetic schema check is not joined native evidence.

Add two installed channels sharing policy A: an otherwise valid A-to-B
transition MUST refuse before prepared/pointer/candidate effects. Repeat after
restart, with another-channel pending ownership, and with an unreadable census
entry. Omitting only the sibling predicate must make the intended refusal
assertion fail. Retain a genuine sole-channel A-to-B positive and same-policy
multi-channel positive; refusal alone does not prove supported behavior.

### D-88: Metadata retention and authority-use clock

These rules are normative parts of D-24 and D-26, not optional qualification.

Installed current/LKG/previous-LKG provenance retains its own verified package,
manifest, policy and rollback binding; a historical metadata digest is audit-only.
Completed admission/metadata receipts retain audit-only generation digests once
the live operation is terminal; they cannot renew a ReadyRevision or pin role blobs.
Never delete the independently retained installation objects or receipt evidence.
A live admitted attempt or ReadyRevision lease using that generation does pin it;
release only after terminal attempt cleanup and every borrower has relinquished it.
Pending journal/pointer ownership remains a hold until its owning crash-resumable
protocol proves terminal completion and release. Unknown ownership refuses retirement.

After every wait and at the final effect check, observe fresh OS UTC and
checked monotonic elapsed time since the captured refresh start. Refuse if either
fresh UTC or fixed refresh UTC plus elapsed is at or after any signed role expiry.
Overflow, unrepresentable addition or invalid monotonic elapsed refuses. The fixed
protocol timestamp remains unchanged; the admission deadline is a separate bound.

### D-89: MVP boundary and future multi-channel evolution

The MVP includes the necessary already-specified admission, same-policy
coexistence, lifecycle, rollback and recovery behaviors for its supported cells.
D-84 through D-87 supply the missing policy-changing transition contract only
for a proven sole eligible installed channel. They do not introduce a general
multi-channel migration coordinator. An existing sibling or uncertain census
refuses before effects; deleting sibling evidence or weakening its authority is
not an MVP path. D-90 owns census and source-reader eligibility.
Same-policy coexistence remains supported under existing rules, not permission
to enroll a new channel through an outstanding transition fence.

Prove concurrency at the repository owner: race a second-channel admission
against the initial census, pre-commit recheck and pointer publication. A prior
admission, including an outstanding admitted permit, must cause transition
refusal; an established transition fence must prevent new admission. Both must
never proceed. Include retained channel roots absent from candidate discovery;
the complete census cannot be replaced by a caller list or blob-only inventory.
These MVP negative/positive cases are NOT_RUN until actual qualification.

FUTURE DIRECTION, NON-NORMATIVE AND NOT IMPLEMENTED: Multi-channel policy
changes would require one atomic shared
repository-trust transition and separately owned per-channel application
activations. Shared trust commit cannot silently rewrite an inactive channel's
application provenance or declare its activation complete. A future approved
contract must define channel enrollment, catch-up, independent activation failure,
crash recovery and terminal ownership without a split-authority interval.

That future contract also needs an explicit versioned Host/Guard compatibility
bridge: authenticated supported schema/protocol versions, permitted mixed-version
states, bounded upgrade order and fail-closed rejection of incompatible peers.
No implicit compatibility, broad grant, reused stale permit or recovery-key
exception may substitute for that bridge. Revocation must invalidate affected
live and pending authority; rollback must not lower repository/application/UTC
floors, resurrect revoked authority or turn an artifact-specific LKG exception
into general update authority. Preserve strict default-deny and exact provenance.

One channel's U1 does not authorize changes to other channels. Existing exact
Host-policy pins remain mandatory until a separately approved compatibility
bridge exists. Future revocation design must distinguish ordinary offline run,
already-admitted operation and fresh acquisition authority; it cannot infer
permission to kill or invalidate an installed app from metadata expiry alone.

These future mechanisms require separate specification and review before use.
Their implementation or qualification is not an MVP admission, B-80 or release
gate for the narrower supported cells. Existing MVP security, independent review,
trusted CI and native acceptance gates remain mandatory and are not waived.
No current checkbox, passing definition test or future design paragraph claims
multi-channel policy migration or Host/Guard bridging is implemented today.

### D-90: Authoritative membership and source-reader eligibility

A discovery list or directory scan alone cannot prove sole-channel ownership.
The planned-policy cell requires an independently reviewed, versioned completeness
proof from the existing native repository owner. It must bind the supported
installation topology and all possible supported v1/v2 root namespaces for this
app/repository/SID, retained-parent enumeration, pinned provenance, pending
journals and already admitted permits. Managed or other artifact roots must have
explicit coverage or authenticated exclusion; an unsupported, unreadable or
incomplete topology refuses before preparation. Absence from candidate discovery
or a blob-reference census is not completeness. All enrollment, relocation and
retirement writers must serialize with the same repository lease and fence.
No new registry schema or retroactive completeness declaration is introduced here.

This revision specifies the required proof boundary, not a qualified producer.
Until an exact producer/version and its coverage, crash/race and old-reader
controls are independently qualified, the policy-changing positive cell remains
unavailable: refuse it, do not label it implemented or accepted for the MVP.
This is necessary safety evidence for that supported single-channel cell, not a
requirement to implement the future multi-channel coordinator. Existing qualified
same-policy behavior can proceed without claiming this transition is qualified.

Before preparation, prove that every source Host/Guard/launcher capable of using
this repository implements the pointer, prepared-journal and membership fences.
An exact binary digest alone is not that capability proof. Use explicitly
qualified source versions and real launch/activation tests. Incompatible older
readers make the policy-changing cell unsupported, even for a sole channel.
Seal new launch admission and retire/drain every in-flight admitted launch or
operation through existing process/browser barriers before commit-authorized.
No old process may survive as an unaccounted writer or launch authority. This is
an MVP fail-closed eligibility condition, not the future mixed-version bridge.

D-65 manual recovery applies the same source-reader eligibility, admission seal and drain, not planned-transition signing authority.
Before preparation, the existing owner must prove complete coverage of readers of the shared repository and qualify each source
Host/Guard/launcher for the D-68 pending pointer and applicable prepared-journal and membership fences. Unknown coverage or
any unsupported or unfenced reader refuses before preparation. A digest or successful directive check is not that proof.
Under the repository and installation leases, seal new ordinary admission and drain or retire every old-generation admitted launch
and operation through the existing process/browser barriers before switching protected current. Verify no unaccounted old writer
or launch authority remains. Hold the ordinary admission seal until D-68 releases it; only the matching recovery owner may
advance its independently authorized candidate/health or exact rollback under D-65. On pre-switch restart,
re-establish eligibility, seal and drain under those leases; neither a journal nor a pointer proves an earlier drain still holds.
If proof or drain fails, do not switch trust. Follow D-65/D-66 for pending recovery or abort; never clear the fence as a shortcut.

D-8's artifact-specific LKG exception remains limited to its exact verified
package, manifest, policy and binding. Neither metadata expiry nor withdrawal
alone authorizes killing an already installed offline application. At recovery
selection enforce the current applicable revocation and exact binding policy;
if that binding itself is revoked or cannot be verified, refuse the LKG view.
A permitted exact LKG view never restores revoked publisher/U1 authority for
fresh acquisition. Future broader revocation semantics remain D-89 work.

Qualify missing/forged/incomplete coverage proof, unknown legacy roots, concurrent
pending admission, unsupported old readers and in-flight old launch races.
Each must refuse before unsafe effects. Preserve genuine qualified sole-channel
transition and same-policy coexistence positives. Cases are NOT_RUN, not claims
that a completeness producer or compatibility bridge already exists.

# Recovery and independent replacement authority

Part of: [index](index.md)
Owns: D-RECOVERY, initial-admission interruption, and manual key-loss recovery.

## 4. Design: Recovery

### D-60: Separate four recovery operations

PROPOSED: Use the following closed operation table.

| Operation | Authority | State effect |
|---|---|---|
| Ordinary interrupted refresh | Existing authenticated repository and protected store | Resume its recorded root/role phase, or refuse |
| Ordinary application rollback | Existing KEL-53 journal and exact LKG binding | Restore only that artifact, without lowering the floor |
| Planned identity or U1 transition | Fresh TUF authorization and the identity part's required signatures | Apply the complete joined transition |
| Manual loss-only recovery | Independently authenticated recovery Host, pinned recovery policy, explicit user confirmation | Replace only the directive-bound trust and update key |

A missing signature cannot select another operation.
Normal refresh cannot call manual recovery. Neither Bun nor a webview can request recovery authority.
The existing process-role decoder owns the dedicated recovery role.
Its implementation requires the public API, permission, wire, and unsafe review gates.

### D-61: Commission independent authority before distribution

PROPOSED: `RepositoryPolicy` contains a required `recoveryPolicySha256` field.
The signed Host's existing repository-policy digest authenticates that field.
The immutable installed provenance retains authenticated references to the repository and recovery policies.
The existing protected repository evidence store retains their exact bytes under the KEL-53 writer.
Use D-9 evidence references and retention rules. Verify each digest before parsing.
Normal TUF refresh cannot change this recovery policy.
A planned policy transition needs the identity part's complete authority.

`RecoveryPolicy` is canonical JSON using the identity part's exact encoding rules.
Its maximum length is 16,384 bytes. Its ordered fields are:

| Field | Closed value |
|---|---|
| `schema` | `keld.windows.recovery-policy/v1` |
| `repositoryId` | Exact 32-byte repository ID, lowercase hexadecimal |
| `recoveryPublisherPolicySha256` | SHA-256 of one exact `PublisherPolicy` |
| `recoveryIssuerPolicySha256` | SHA-256 of that publisher's exact `IssuerPolicy` |
| `recoveryOrigin` | One canonical HTTPS origin, under the identity part's URL rules |
| `method` | `manual-authenticode-bootstrap-v1` |
| `incidentClasses` | Ordered array containing only `loss-only` |
| `maximumDirectiveLifetimeSeconds` | Integer `86400` |
| `inPlaceManagedRecovery` | JSON `false` |

The recovery publisher authority reuses the existing Authenticode verification owner, not the application's signing credential.
Commission a distinct recovery-only `PublisherPolicy.profileOid` before distribution.
The protected installed `RecoveryPolicy` pins both recovery publisher and issuer policy digests.
The recovery profile OID MUST differ from the installed application profile OID.
The recovery verifier MUST match the verified primary signer's full recovery-only
profile OID and issuer policy, and MUST refuse a signer leaf containing the ordinary
application profile OID. Its issuer comparison applies to the same verified primary
signer. An OID prefix, subject, or generic Public Trust/code-signing EKU is
not a substitute for the commissioned recovery-only full OID.
Microsoft Artifact Signing rotates short-lived leaf certificates daily.
Therefore observed leaf SPKI and thumbprint are artifact evidence, not
installed durable signing-authority pins. An otherwise valid later leaf
from the same commissioned recovery profile MUST NOT be rejected merely
because its SPKI changed. Recreating the profile changes its OID and
requires separately approved authority transition; never fall back
to ordinary application signing.
The pinned recovery publisher's `issuerPolicySha256` MUST equal `RecoveryPolicy.recoveryIssuerPolicySha256`.
Normal Authenticode success or application-publisher success alone MUST NOT authorize recovery.
An ordinary application signer MUST fail recovery authorization even if its Windows chain and application profile are valid.
The same recovered application publisher and issuer policies remain required in the source and destination.
The recovery Host instead uses the separately pinned recovery-only signer policy; it carries the application's destination identity.
Recovery signing custody remains separate from TUF, catalogue, application signing, and installed U1 credentials.
The release/security owner MUST record that separation before commissioning. A distinct label alone is insufficient.
The installed recovery policy digest remains constant throughout a loss-only operation.
Loss-only recovery cannot change application publisher scope, identity scheme, discovery, install mode, or root policy.
The source must use the v2 identity scheme. Legacy recovery remains conditional on B-2.
No new custom signature algorithm, emergency signing envelope, or generic broker is introduced.

The recovery origin supplies instructions and authenticated recovery artifacts.
TLS alone does not authorize a replacement. A downloaded digest from the affected TUF repository grants no recovery authority.
The user obtains the recovery Host through the separately commissioned recovery origin.
Windows validates the Host against the separately installed recovery-only
publisher/profile and issuer policies and the full existing chain, timestamp,
revocation, and one-primary/zero-secondary signature requirements.
Do not read replacement recovery-signing policy from the directive, candidate, or newly self-signed root.

Loss-only recovery compares source and destination `RepositoryPolicy` using this closed matrix.
Compare exact canonical decoded fields against the protected installed source policy before any trust change.

| RepositoryPolicy field | Loss-only rule |
|---|---|
| `schema`, `repositoryId`, `appId`, `target`, `metadataProfile` | Exact source/destination equality |
| `metadataBaseUrl`, `targetsBaseUrl` | Exact equality; no origin or path substitution |
| `catalogueKeys` | Exact equality, including key IDs, public keys, and order |
| `publisherPolicySha256`, `discoveryPolicySha256`, `recoveryPolicySha256` | Exact equality to protected installed values |
| `bootstrapRootSha256` | Must change to the directive-bound, independently authorized, verified replacement root |
| `bootstrapRootVersion` | Must equal the new verified root version and strictly exceed the currently accepted root version recorded by protected metadata, without overflow |

The destination repository-policy digest MAY differ only because the two bootstrap-root fields change.
The directive's `source.rootSha256` MUST match the actual currently accepted
root in the protected current metadata generation. That root may differ from
the source policy's original commissioning `bootstrapRootSha256` after
ordinary authenticated root rotations. Do not confuse the bootstrap pin
with the current root or compare a proposed new version only against the
original commissioning floor. The source policy digest MUST agree with
protected current metadata's policy reference, installed provenance and
retained authenticated policy evidence. The new independently authorized
root version must strictly exceed the *current* root version; this
conservative loss-only profile does not permit a version reset. If the
available genuine independent replacement cannot meet that condition,
refuse this in-place profile rather than lowering a protected floor.
Verify its complete canonical bytes against the directive and the destination Host and catalogue.
Retain the installed recovery policy and all of its recovery signer pins throughout the operation.
No other field change inherits authority from the directive or from a newly self-signed root.
Broader policy rotation requires a separately approved planned transition, not the loss-only path.

If the recovery signing authority is also lost, this in-place method is unavailable.
Do not lower a threshold, replace a policy from caller input, or invent a fresh identity.
Retain the installation and data. Clean-system reinstallation remains a separate operator procedure.

### D-62: Authenticate a dedicated replacement verifier

PROPOSED: The dedicated recovery Host contains one Authenticode-covered PE section named `.keldrec`.
The section name contains exactly eight ASCII bytes.
Reuse the existing PE container's alignment, overlap, bounds, and covered-byte rules.
Place `.keldrec` before `.keldea2`. The expected-identity section remains the final PE section.
Its payload is one canonical JSON `RecoveryDirective`, between 1 and 65,536 bytes.
Reject multiple recovery sections, trailing nonpadding bytes, unknown fields, and unsupported versions.

The same fully verified primary signer authenticates `.keldea2` and `.keldrec`.
The directive contains no digest of its containing Host. That would create a signing cycle.
The verifier records the final signed Host digest in its local authorization record.
The ordinary initial-install and application roles cannot consume `.keldrec` as permission to recover.
The Guard returns a distinct private `RecoveryImage`. It cannot convert into an admitted catalogue Host.
Its `.keldea2` binds the destination lane, repository policy, and U2. The directive binds the exact source and destination.
The recovery Host and candidate Host can have different hashes. The catalogue must match the separately verified candidate Host.
The recovery Host's signer satisfies the installed recovery-only profile and
issuer policy; its verified leaf MUST NOT contain the ordinary app profile EKU.
It need not satisfy the ordinary application's signer profile.
The destination candidate Host MUST satisfy the unchanged installed application publisher and issuer policies.
The recovery-only signer MUST NOT qualify as a normal Host or catalogue signer.

The directive has these ordered fields:

| Field | Closed value |
|---|---|
| `schema` | `keld.windows.recovery-directive/v1` |
| `operation` | `replace-trust-and-key` |
| `incidentClass` | `loss-only` |
| `repositoryId`, `appId`, `channel`, `target` | Exact established lane and repository |
| `source` | `rootSha256,repositoryPolicySha256,recoveryEpoch,updatePublicKeyEd25519,publisherScope` |
| `destination` | `bootstrapRootSha256,repositoryPolicySha256,recoveryEpoch,updatePublicKeyEd25519,catalogueSha256` |
| `rollback` | The identity part's complete exact `ArtifactBinding` |
| `browserStore` | The identity part's exact `retain-source` binding |
| `issuedAtUnixSeconds`, `expiresAtUnixSeconds` | Unsigned integers in the identity part's `n53` domain |

All digest and key fields use the identity part's exact representations.
Both recovery epochs fit checked u64. Destination epoch equals source epoch plus one.
Epoch zero is valid. Encode epochs as canonical unsigned decimal integers without exponent or leading-zero aliases.
Reject overflow. The remaining integer fields use their named field domains.
Issue time cannot exceed the effective trusted UTC. Expiry must exceed that UTC.
The positive interval cannot exceed the recovery policy's lifetime cap.
The destination repository ID remains unchanged. The destination Host expectation and catalogue must match every repeated field.
The source fields must match protected readback. The local journal additionally binds the exact source generation and provenance digests.
The new root must validate its own threshold and the fully refreshed new metadata graph.
That self-signature checks internal consistency. The independently verified directive supplies replacement authority.

### D-63: Require an explicit, bounded user ceremony

PROPOSED: The recovery Host displays the publisher, application, operation, source state, and destination release.
The recovery Host states that automatic verification could not continue because a signing key was lost.
The user confirms this exact displayed operation through the trusted native recovery UI.
There is no unattended, command-line-confirm, environment-variable, Bun, or webview confirmation path.
Cancellation, timeout, or UI failure grants no authority.
Use a 120-second monotonic confirmation deadline. Do not extend that deadline through retries.

After confirmation, reacquire metadata, installation, and physical-profile leases in that order.
Revalidate the worker, retained sources, source generation, provenance, floors, and directive expiry.
A changed input invalidates confirmation. Display and confirm a newly verified operation before trying again.
The private `VerifiedRecoveryAuthorization` is noncloneable and bound to that worker and attempt.
It exposes no raw path, key override, or reusable policy grant.
The recovery owner consumes it once to construct `VerifiedMetadataRecovery` for the metadata owner.
That narrow capability binds the exact old generation, old epoch, next epoch, replacement root, policy, and catalogue.
The client adapter constructs `VerifiedRecoveryGraph` only after it verifies the complete replacement metadata graph.
Graph verification alone grants no replacement authority.

The user confirmation approves an operation already authenticated by the independent policy.
Confirmation cannot make an unsigned or wrongly signed recovery artifact acceptable.
This operational confirmation is separate from approval of this implementation specification.

### D-64: Freeze the initial admission record

PROPOSED: KEL-53 stores the record under the protected repository ledger, outside the installation tree.
The filename is `admissions/<attemptId>.json`. The attempt ID is 32 random bytes encoded as 64 lowercase hexadecimal characters.
Use the existing OS entropy owner. A caller cannot choose the attempt ID.
The record is canonical JSON with a 32,768-byte cap and these ordered fields:

| Field | Meaning |
|---|---|
| `schema` | `keld.windows.initial-admission/v1` |
| `attemptId`, `repositoryId`, `ownerSid` | Exact attempt, repository, and admitted user |
| `metadataSequence`, `metadataGenerationSha256`, `recoveryEpoch` | Exact admitted revision |
| `hostSha256`, `catalogueSha256`, `repositoryPolicySha256`, `discoveryPolicySha256` | Exact authenticated inputs |
| `manifestSha256`, `packageContentBlake3`, `packageTransportBlake3` | Exact complete package selection |
| `destinationIdentity`, `destinationRootIdentity` | Native root derivation and retained ancestor identity |
| `sourceIdentity` | Native retained archive volume/file identity |
| `phase` | `admitted`, `bootstrap-owned`, `provenance-published`, or `complete` |

`destinationIdentity` uses the complete v3 `WireIdentity` from D-8.
`destinationRootIdentity` uses D-48's destination binding. `sourceIdentity` uses D-48's object observation.
These observations contain no caller-selected path. Matching recycled file identifiers never grants recovery authority.
Sequence is a positive u64. Epoch is a nonnegative u64. Digest fields use `h32`; BLAKE3 fields use existing lowercase codecs.
`ownerSid` is the canonical SID string captured by D-42, at most 184 ASCII bytes.
The one-shot permit becomes consumed before the admission-record write.
Publish and reread `admitted` while holding the metadata admission lease.
Before that record, publish and reread D-9's complete seven-reference policy set.
It includes the separately commissioned recovery publisher and issuer policies.
Retain those objects while admission or committed provenance references them. Never search arbitrary attempts to reconstruct missing policy bytes.
Publish D-68's pending pointer before installation creation.
Only then create installation parents, scaffold, and KEL-53's exclusive bootstrap lock.
Installation parents here exclude only D-44's explicitly authorized shared acquisition ancestors.
Recheck destination ownership under that lock. Persist `bootstrap-owned` before releasing the metadata lease.
The existing initializer owns subsequent provenance, baseline, and current publication.
Advance the admission phase only after the corresponding native durable operation succeeds.

A restart requires the same authenticated inputs, effective user, admitted root, and exact package.
Before `bootstrap-owned`, recovery reacquires a fresh metadata admission for the same exact target.
Withdrawal or changed bytes refuses. Do not create a substitute installation.
After `bootstrap-owned`, the journal owns commit or refusal using the recorded admitted inputs.
Metadata expiry cannot substitute new bytes midway through that accepted transaction.
An absent or invalid record beside an incomplete scaffold refuses normal initialization.
Only the admitted recovery operation can reconcile that scaffold. It cannot recursively remove unrelated content.
If creation succeeds before its object receipt becomes durable, stop on restart. Do not infer ownership from an empty directory.
An authorized operator must reconcile that exact orphan. Automatic recovery cannot delete or adopt it.
After each successful component creation, advance `destinationRootIdentity` to that verified component and its remaining derived suffix.
Publish and reread that observation before creating its child. `destinationIdentity` retains the complete intended destination.

### D-65: Join manual recovery to one transaction

PROPOSED: Prepare the new metadata graph privately before changing established state.
The same bounded client verifies it from the directive-bound replacement root.
Download and authenticate the exact candidate and finite rollback binding.
The recovery directive replaces planned-transition authority. It does not require an unavailable U1 signature.
Do not run candidate code during preparation.
Require a compatible rollback artifact before admitting any change.
An old Host that cannot consume the new identity/recovery view does not satisfy that requirement.

The KEL-53 recovery journal records the complete before/after state.
Use the bounded immutable evidence references from D-9. Do not inline complete provenance, policy, or metadata objects.
Its canonical record has a 65,536-byte cap and these ordered fields:

| Field | Meaning |
|---|---|
| `schema` | `keld.windows.manual-recovery/v1` |
| `attemptId`, `ownerSid`, `repositoryId` | Exact local attempt |
| `recoveryHostSha256`, `recoveryPolicySha256`, `directiveSha256` | Independent authorization evidence |
| `sourceGenerationSha256`, `sourceProvenanceSha256`, `sourceEpoch` | Protected before-state |
| `destinationGenerationSha256`, `destinationProvenanceSha256`, `destinationEpoch` | Complete proposed after-state |
| `applicationFloor`, `utcFloorMillis`, `browserBindingSha256`, `rollbackBindingSha256` | Invariants and finite LKG authority |
| `phase` | `prepared`, `aborted`, `trust-committed`, `activation-bound`, or `complete` |

Persist `prepared` before either store changes.
Persist the record at `recoveries/<attemptId>.json`. Publish D-68's pending pointer before changing established trust or application state.
The metadata owner's private `rebootstrap` method consumes the narrow authorization and complete verified replacement graph.
It compares the exact source generation and epoch under the lease.
It advances the epoch once and retains at least the previous UTC floor.
Its next generation is `ready`, with sequence equal to the old sequence plus one.
Publish every referenced blob and the complete generation before replacing the single current pointer.
The prepared journal names that exact destination generation. No network request occurs during trust commit or activation binding.
The application floor is outside this operation and cannot be reset.
Persist `trust-committed` after protected readback of the new metadata generation.
Bind the existing KEL-53 activation journal to the new provenance and exact candidate.
Persist `activation-bound` before releasing either transaction lease.
Use the existing exact-health, candidate cleanup, commit, and rollback state machine.
Persist `complete` only after that state machine reaches its accepted terminal state.

The old update key grants no general release-selection authority after trust commit.
The finite LKG binding permits only the exact recorded rollback artifact.
Failed candidate health never reverses metadata recovery or lowers either floor.
The physical browser store and its lease identity remain unchanged throughout recovery.

An expired `prepared` attempt does not gain new authorization from its old directive.
Use abort-and-retry, never in-place replacement of the recorded directive or its journal bindings.
Only the KEL-53 recovery owner may abort under the existing repository and installation leases.
First verify the protected journal, pending pointer, original signature evidence, and exact installed source generation.
Check the old epoch, provenance, policy pins, and unchanged application current/LKG and browser bindings.
Check that the metadata current pointer still selects the old generation and no trust or activation effect occurred.
An unreferenced prepared successor is not committed trust; retain it until
safe bounded cleanup while the matching pending pointer still guards the repository.
Any ambiguous or changed state MUST refuse abortion and preserve all evidence for diagnosis.
Persist and reread terminal `aborted` before any cleanup. Under the same
metadata, installation and physical-profile leases, first publish, flush
and read back the D-23 mode-`recovery` retirement intent. Bind its exact
attempt ID, protected old/current and destination generation, full list
of exclusively owned blobs, and matching pending pointer to the aborted
journal; no object deletion may precede this durable record. Then retire the *exact*
unreferenced prepared successor generation at
`generations/<oldSequence+1>.json` and exclusively owned preparation blobs.
Verify its recorded digest, protection and ownership and prove it was
never selected by current, LKG, previous LKG or another protected owner.
The aborted journal remains as bounded diagnostic evidence; its destination
digest is historical, not live authority to keep uncommitted successor bytes.
The immutable next-sequence filename MUST be absent before a new attempt
can prepare different bytes at that same required sequence.
Retirement is idempotent only under the matching authentic pre-deletion
D-23 intent: a missing intent-listed blob or retired successor can be
counted as completed only while the protected old state still matches.
Missing unlisted or authoritative old-state files, a changed reference,
or a mismatched/absent intent MUST refuse before any further deletion.
After a crash following durable `aborted`, the matching pending pointer
permits only finishing that same retirement and pointer cleanup.
Reread the unchanged old metadata current, provenance, epoch, application
state, and the verified absence of the prepared successor, exclusively
owned blobs, and the retired D-23 intent before removing the pointer.
Remove and reread the pointer under the same lease. An interrupted removal resumes this terminal cleanup only.
If any readback or removal fails, keep the operation blocked; never infer success from a missing response.
Only after verified pointer absence MAY a new random attempt and independently signed directive enter `prepared`.
The new attempt requires a fresh native confirmation and full source, destination, policy, epoch, and expiry verification.
Matching desired destination state is permitted; the expired directive itself never authorizes the new attempt.
Reject a stale directive or a changed replacement without changing the established trust.

### D-66: Define every recovery cut

| Last durable state | Next permitted action |
|---|---|
| No journal | Established state remains authoritative. Discard only unreferenced owned preparation objects. |
| `prepared`, old metadata current, directive valid | Reverify the original authorization and all source/policy bindings, then commit or refuse. |
| `prepared`, old metadata current, directive expired | Verify no trust or application effect, durably mark `aborted`, retire only authenticated unreferenced preparation, verify old state and clear matching pointer, then require a new attempt. |
| `prepared`, new metadata current | Match the recorded destination generation. Advance the journal without repeating the epoch change. |
| `aborted`, old metadata current | Verify no trust/application effect, idempotently finish the exact D-23 retirement-intent cleanup, verify intent removal, then clear the pending pointer; never resume trust commit. |
| `trust-committed` | Complete the exact activation binding. Refuse all unrelated selection until that binding exists. |
| `activation-bound` | Resume only the corresponding KEL-53 activation phase. |
| `complete` | Use the coherent new view or its finite LKG view. |
| Unknown, corrupt, or contradictory state | Halt. Preserve state and evidence. Never choose an older record to appear healthy. |

Recovery authenticates persisted directive and verifier evidence before continuing.
Ordinary startup performs read-only diagnosis. It cannot mint a recovery authorization or create a replacement journal.
After trust commit, existing durable authorization controls completion. Expiry does not authorize replacement inputs.
Before trust commit, an expired directive MUST NOT resume trust commit.
Follow the D-65 abort path before accepting a new directive and confirmation in a fresh attempt.
If metadata current is new, finish only the committed exact attempt under the recorded durable authorization.
No normal update may run while a manual-recovery journal is pending.
All persisted edges need process-termination and separately named physical-power-loss qualification.

### D-67: State the limits of complete recovery

PROPOSED: Loss-only recovery assumes a trustworthy OS and intact protected before-state.
The verifier cannot prove that the OS is uncompromised. Operator incident classification supplies this prerequisite.
Known or reported compromise refuses the loss-only ceremony. A successful signature check does not clear that classification.
It also requires the independently commissioned recovery authority to remain available.
These are explicit prerequisites, not facts inferred from successful signature verification.
Suspected compromise uses a clean system and independently acquired current verifier.
Old executables, services, browser extensions, and unverified state cannot be copied into that trusted installation.
Preserve original data for forensic or user-data recovery. Do not claim automatic safe browser-data import from a compromised system.

The legacy v1 bridge remains conditional on its own admissible signer and compatibility evidence.
Absence of that evidence blocks the legacy cell. It does not block independent fresh-v2 qualification.
No result in fresh-v2 qualification can mark manual recovery, legacy migration, or compromise recovery complete.
Recovery does not silently change another channel's installed provenance.
An incompatible other-channel policy remains refused until that channel completes its own authenticated transition.

### D-68: Locate and bound one pending repository operation

PROPOSED: The existing KEL-53 writer owns `pending-operation.json` at the protected repository root.
Its closed canonical JSON fields are `schema,repositoryId,appId,ownerSid,kind,attemptId`.
The schema is `keld.repository-pending-operation/v1`. Its maximum size is 2,048 bytes.
Identity fields use the exact established anchor values. The attempt ID uses D-64's random lowercase hexadecimal encoding.
The kind is exactly `initial-admission` or `manual-recovery`.
It selects exactly `admissions/<attemptId>.json` or `recoveries/<attemptId>.json`. No pointer contains a caller path.

Allow one pending operation per repository. Publish and reread its complete journal before publishing the pointer.
Publish and reread the pointer before the first installation effect or manual trust switch.
An existing different pointer refuses another operation. A matching pointer permits only the recorded attempt's recovery or terminal abort cleanup.
Every ordinary metadata open and admission checks this fixed pointer under the repository lease.
Pending state blocks unrelated refresh, admission, update, and launch selection. Diagnosis may read the recorded state.
The initial recovery path may refresh the same exact target before `bootstrap-owned`, as D-64 requires.
It must update the admission revision before discarding any superseded evidence. Manual recovery follows D-66 instead.

Keep every generation and object needed by the pending operation. Refuse a new mutation if the retention cap cannot hold them.
Clear the pointer only after protected readback proves `complete` or `aborted` and coherent current application state.
For `aborted`, the old metadata generation, epoch, provenance, and application state MUST all remain unchanged,
and no abandoned prepared successor at the required next sequence may remain.
Keep current/LKG evidence after pointer removal. Do not garbage-collect reachable policy objects.
An unreferenced prepared journal cannot have caused installation or trust effects under the required order.
Clean only such owned preparation records under the lease before creating another journal. Refuse ambiguous orphan state.
Keep at most one pending journal and one completed journal for diagnosis per repository.
If cleanup fails, refuse the next attempt. Do not grow the store through repeated attempts.
Add process-termination cuts before and after pointer publication, after terminal readback, and during pointer removal.
An intact pointer with a missing, corrupt, or inconsistent journal requires recovery. It cannot become an absent operation.

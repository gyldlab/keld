# Recovery and independent replacement authority

Part of: [index](index.md)
Owns: D-RECOVERY, initial-admission interruption, and manual key-loss recovery.

## 2. Spec refs: Recovery

FACT: KEL-53 owns installation state, version floors, current selection, and LKG recovery.
KEL-270 owns the common Windows transaction. KEL-135 owns browser identity and its physical store lease.
These owners remain unchanged. This draft adds no second installer or updater.

FACT: TUF 1.0.37 requires out-of-band root replacement after root-threshold compromise.
Primary source: <https://theupdateframework.github.io/specification/v1.0.37/#root>.
Retrieved 2026-11-15. Exact relevant words: "the Root keys should be updated out-of-band".
TUF does not authenticate a user's first downloaded verifier.

FACT: The existing installation owner has different metadata and application recovery obligations.
Source: `docs/specs/kel53-full-package-activation.md:218@7ff41173d28d58ae61ccbb3382d055c8ef9c9c97`.
The identity and metadata parts define the proposed bytes consumed below.
No operation below is implemented or qualified by this document.

## 3. Acceptance criteria: Recovery

### AC-60: Initial admission has one recoverable owner

- **Given:** A fresh destination and a valid, leased metadata revision.
- **When:** The admitting process terminates at each persisted boundary.
- **Then:** The next admitted worker completes the exact attempt or refuses without selecting unrelated bytes.
- **Negative control:**
  - **Single fault:** Create the installation scaffold before the admission record becomes durable.
  - **At-risk precondition:** Termination occurs immediately after the first scaffold creation.
  - **Expected RED:** The next worker observes an installation effect without an authenticated attempt record.

### AC-61: A missing key cannot authorize its own replacement

- **Given:** An established installation whose update key is unavailable.
- **When:** A caller supplies a normal transition without its required U1 signature.
- **Then:** The operation refuses. It does not infer recovery intent or delete trust state.
- **Negative control:**
  - **Single fault:** Route missing-signature errors to recovery.
  - **At-risk precondition:** All other metadata and candidate checks pass.
  - **Expected RED:** A caller replaces the update key without independent recovery authorization.

### AC-62: Recovery authority is independent and exact

- **Given:** A commissioned recovery policy and an independently authenticated recovery Host.
- **When:** One source, destination, operation, issuer, profile, epoch, expiry, or consent binding differs.
- **Then:** Recovery refuses before mutation of established metadata or installation state.
- **Negative control:**
  - **Single fault:** Omit the source update-key comparison.
  - **At-risk precondition:** A valid directive names another installation key under the same publisher.
  - **Expected RED:** The wrong installation receives a replacement key.

### AC-63: Recovery cannot lower application or time floors

- **Given:** An established floor, UTC floor, browser binding, current artifact, and LKG artifact.
- **When:** Manual recovery replaces metadata authority or the installed update key.
- **Then:** Both floors remain at least their prior values. The physical browser store remains unchanged.
- **Negative control:**
  - **Single fault:** Reuse first-install initialization for an established recovery.
  - **At-risk precondition:** The installation floor exceeds the bootstrap package version.
  - **Expected RED:** A lower package becomes eligible or a new browser store replaces existing data.

### AC-64: A failed recovery candidate preserves finite rollback authority

- **Given:** A valid independent directive that binds a compatible LKG artifact.
- **When:** The new candidate fails health after the new trust state commits.
- **Then:** The common transaction selects only the bound LKG. New releases still require the new update key.
- **Negative control:**
  - **Single fault:** Convert the LKG key exception into a general update-verification key.
  - **At-risk precondition:** Another old-key-signed release exceeds the floor but is absent from the directive.
  - **Expected RED:** The updater admits that unrelated release.

### AC-65: Recovery cannot claim safety on a compromised system

- **Given:** Suspected root-threshold compromise, suspect native code, or untrusted local state.
- **When:** A user requests in-place recovery.
- **Then:** The recovery Host refuses in-place mutation and directs the user to clean-system recovery.
- **Negative control:**
  - **Single fault:** Treat a valid replacement signature as proof that the operating system is clean.
  - **At-risk precondition:** The incident class requires clean-system recovery.
  - **Expected RED:** The application claims an in-place secure recovery completed.

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

The recovery publisher authority uses the existing verified Authenticode publisher owner.
It is independent of TUF signing credentials, catalogue credentials, and installed U1 credentials.
Independent means separate credential custody and incident access, not merely another public-key identifier.
The release/security owner records that separation before commissioning.
This loss-only cell requires the same application publisher and issuer policies in the source, recovery Host, and destination.
Both recovery policy digests must equal the installed application policy digests.
It cannot change publisher scope, identity scheme, discovery policy, install mode, or root policy.
The source must use the v2 identity scheme. Legacy recovery remains conditional on B-2.
No new custom signature algorithm, emergency signing envelope, or generic broker is introduced.

The recovery origin supplies instructions and authenticated recovery artifacts.
TLS alone does not authorize a replacement. A downloaded digest from the affected TUF repository grants no recovery authority.
The user obtains the recovery Host through the separately commissioned recovery origin.
Windows validates the Host against the installed recovery publisher and issuer policies.

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
The recovery Host's signer satisfies the unchanged application publisher policy and the installed recovery policy.

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
Before that record, publish and reread the complete five-policy set at D-9's derivable content-addressed paths.
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
| `phase` | `prepared`, `trust-committed`, `activation-bound`, or `complete` |

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

### D-66: Define every recovery cut

| Last durable state | Next permitted action |
|---|---|
| No journal | Established state remains authoritative. Discard only unreferenced owned preparation objects. |
| `prepared`, old metadata current | Reverify the same authorization and source state, then complete trust commit or refuse. |
| `prepared`, new metadata current | Match the recorded destination generation. Advance the journal without repeating the epoch change. |
| `trust-committed` | Complete the exact activation binding. Refuse all unrelated selection until that binding exists. |
| `activation-bound` | Resume only the corresponding KEL-53 activation phase. |
| `complete` | Use the coherent new view or its finite LKG view. |
| Unknown, corrupt, or contradictory state | Halt. Preserve state and evidence. Never choose an older record to appear healthy. |

Recovery authenticates persisted directive and verifier evidence before continuing.
Ordinary startup performs read-only diagnosis. It cannot mint a recovery authorization or create a replacement journal.
After trust commit, existing durable authorization controls completion. Expiry does not authorize replacement inputs.
Before trust commit, an expired directive requires a newly authenticated directive and fresh user confirmation.
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
An existing different pointer refuses another operation. A matching pointer permits only the recorded attempt's recovery path.
Every ordinary metadata open and admission checks this fixed pointer under the repository lease.
Pending state blocks unrelated refresh, admission, update, and launch selection. Diagnosis may read the recorded state.
The initial recovery path may refresh the same exact target before `bootstrap-owned`, as D-64 requires.
It must update the admission revision before discarding any superseded evidence. Manual recovery follows D-66 instead.

Keep every generation and object needed by the pending operation. Refuse a new mutation if the retention cap cannot hold them.
Clear the pointer only after protected readback proves the recorded terminal state and coherent current application state.
Keep current/LKG evidence after pointer removal. Do not garbage-collect reachable policy objects.
An unreferenced prepared journal cannot have caused installation or trust effects under the required order.
Clean only such owned preparation records under the lease before creating another journal. Refuse ambiguous orphan state.
Keep at most one pending journal and one completed journal for diagnosis per repository.
If cleanup fails, refuse the next attempt. Do not grow the store through repeated attempts.
Add process-termination cuts before and after pointer publication, after terminal readback, and during pointer removal.
An intact pointer with a missing, corrupt, or inconsistent journal requires recovery. It cannot become an absent operation.

## 5. Boundaries: Recovery

Implement in the existing Guard verifier, Host role decoder, KEL-53 records, and KEL-270 transaction owners.
Reuse KEL-135 physical-store ownership and identity codecs.
Do not change sandbox grants, KIPC authority, machine-seamless support, or managed deployment ownership.
Do not add a second recovery service, installer, or general-purpose broker.

## 6. Tasks: Recovery

### T-60: Implement durable initial-admission recovery

- **Blocked by:** T-80, T-21, T-40, T-41

Implement D-64 through the existing writer. Run AC-60 with every publication cut and its isolated mutant.
Include D-68 pointer lookup, exact-attempt exclusion, bounded retention, and every pointer cut.

### T-61: Implement independent manual recovery authorization

- **Blocked by:** T-80, T-1, T-40, T-81

Implement the exact policy, dedicated verifier role, directive, and native confirmation.
Run AC-61, AC-62, and AC-65. Reject unattended recovery and incomplete authority commissioning.

### T-62: Join and qualify the common recovery transaction

- **Blocked by:** T-61, T-60, T-23, T-2

Implement D-65 and D-66 without another writer. Run AC-63 and AC-64 with joined browser/LKG/floor observation.
Qualify loss-only recovery separately from fresh-v2 and legacy cells.

## 7. Test plan & evidence: Recovery

### E-60: Primary-source recovery boundary

- **Run:** TUF 1.0.37 sections 1.4, 2.1.1, 5.2, and 6.1, retrieved 2026-11-15.
- **n:** Four normative sections.
- **Environment:** Primary-source review against the product and research pins cited above.
- **Limits:** The source explains trust replacement. It does not approve this new recovery contract or prove native behavior.

### E-61: Recovery implementation qualification

- **Run:** NOT_RUN. Execute AC-60 through AC-65 after implementation and authority commissioning.
- **n:** Zero executed native cases.
- **Environment:** Required Windows x64 direct per-user installation with genuine independent recovery signer evidence.
- **Limits:** No implementation, process-crash, physical-power-loss, compromise-cleanup, or real browser migration pass is claimed.

## 9. Perf impact: Recovery

Manual recovery is cold and user initiated. It starts no persistent service.
One request and confirmation retain the metadata part's deadlines and byte bounds.
UNKNOWN: Native latency and filesystem durability cost require measurement during qualification.

## 10. Open questions, blockers & unknowns: Recovery

### B-60: Independent recovery authority is not commissioned

- **Status:** open
- **Owner:** Existing release/security owner.
- **Impact:** No manual recovery artifact may ship or grant authority.
- **Check:** Genuine policy, issuer/profile, separated credential custody, recovery origin, and signed directive receipts exist.

### B-61: Native recovery and joined migration remain unqualified

- **Status:** open
- **Owner:** KEL-270 with KEL-135 and independent Windows/security reviewers.
- **Impact:** Initial and manual recovery cannot be advertised as production ready.
- **Check:** Exact-source AC-60 through AC-65 and every applicable joined A19 support cell pass.

### B-62: Compromised-system and unavailable-recovery-authority cells require operator recovery

- **Status:** open
- **Owner:** Release/security and the affected system operator.
- **Impact:** In-place automated continuity is unavailable in those cells.
- **Check:** Clean-system recovery is independently executed with current authentic installation and an explicit data-restoration disposition.

No design approval or release approval is recorded by this draft.

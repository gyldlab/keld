# Protected metadata and client qualification

Part of: [index](index.md)
Owns: D-METADATA, the metadata store, protocol profile, and M-CLIENT qualification.

## 4. Design: Metadata

### D-20: Select one client with an explicit correction contract

PROPOSED: Use `tough =0.24.0` as the patch baseline. The crate archive SHA-256 is `35b378d98765c2ae9cdc3e9963ea7e670da8cdd9ee39611b8d722083c7f1ac11`. The source commit is `98d8eb8b2ce63515d9b4981c938ef6453c5b5771`. Pin the reviewed patch and resolved dependency graph before product integration. Do not select a moving branch.

FACT: S1 provides the top-level verification workflow. Its default datastore uses ordinary path writes. It deletes the old root before replacement. Malformed cached metadata can bypass comparisons.
See S1 [datastore lines 47–122](https://github.com/awslabs/tough/blob/98d8eb8b2ce63515d9b4981c938ef6453c5b5771/tough/src/datastore.rs#L47-L122).
The corresponding workflow ranges are [813–850](https://github.com/awslabs/tough/blob/98d8eb8b2ce63515d9b4981c938ef6453c5b5771/tough/src/lib.rs#L813-L850),
[929–968](https://github.com/awslabs/tough/blob/98d8eb8b2ce63515d9b4981c938ef6453c5b5771/tough/src/lib.rs#L929-L968),
and [1087–1162](https://github.com/awslabs/tough/blob/98d8eb8b2ce63515d9b4981c938ef6453c5b5771/tough/src/lib.rs#L1087-L1162).

FACT: The prior storage-and-clock-only proposal is insufficient. The following client differences need exact disposition:

| Boundary | S1 evidence | Required patch or profile rule |
|---|---|---|
| Expiry equality | `lib.rs:665-669` uses `start <= expires` | Use strict `start < expires` for every applicable role. |
| Equal timestamp version | `lib.rs:929-979` permits equality and then stores replacement metadata | Return an explicit unchanged outcome. Discard the replacement. |
| Historical floors after threshold changes | `lib.rs:934,1096` skips old comparisons when old signatures fail the new root policy | Compare intact historical floors independently of old-envelope verification against the new threshold. |
| Root arithmetic | `lib.rs:741-745` adds the limit and next version without checked arithmetic | Count transitions separately. Check every next-version increment. |
| Root transport error | `lib.rs:752-765` converts an initial fetch error into end-of-chain | Only an explicit absent-root result ends catch-up. Other failures refuse. |
| Snapshot cap | `lib.rs:1023-1039` trusts signed length instead of the configured cap | Enforce the independent snapshot cap in the transport. |
| Targets cap | `lib.rs:1222-1244` similarly uses signed length | Enforce the independent targets cap in the transport. |
| Target authority | `lib.rs:448-458` exposes bytes before checksum completion | Buffer bounded bytes privately. Require successful EOF before parsing. |

The expiry rule follows S2 sections 5.3.10, 5.4.4, 5.5.6, and 5.6.5. The equal-version rule follows S2 section 5.4.3.1. The root transport rule is the stricter KELD availability policy. Do not describe every profile difference as an upstream security vulnerability. These source findings are not executed exploit evidence.

FACT: The historical-floor conditions affect timestamp, snapshot, and referenced targets comparisons. A threshold-only change can invalidate an old envelope under the new threshold without rotating its keys. D-24 does not reset floors for that change.

Authenticate incoming metadata against the new root. Read historical floors from intact protected records that were accepted previously. Compare those floors even when the old envelope does not meet the new threshold. Only the durable D-24 key-set reset can clear those ordinary-refresh floors. Corrupt historical records refuse.

Run AC-28 for timestamp version, timestamp-to-snapshot version, snapshot version, and snapshot-to-targets references. Run each case before and after restart. Use unchanged keys, a threshold change from one to two, stored version 100, replay version 1, and valid version 101. Restore the upstream conditional as the isolated failing mutant.

### D-21: Freeze the metadata profile

PROPOSED: Profile version 1 supports only root, timestamp, snapshot, and top-level targets. Require consistent snapshots. Refuse delegated metadata before its first fetch. Require SHA-256 and exact lengths for snapshot, targets, and catalogue descriptions. Retain upstream canonical signing and threshold verification.
Do not introduce installer-side substitutes for missing protocol checks.

| Item | Bound |
|---|---:|
| Root | 65,536 bytes |
| Timestamp | 16,384 bytes |
| Snapshot | 262,144 bytes |
| Targets | 1,048,576 bytes |
| Catalogue | 65,536 bytes |
| Catalogue signature | exactly 64 bytes |
| Downloaded metadata and catalogue | 73,400,320 bytes |
| Root-update setting | 1,024 |
| Total refresh and admission duration | 120 seconds |
| One request | 15 seconds within the remaining total |
| Automatic retries, redirects, delegations | zero |

Retain the upstream root-limit edge for this version. At 1,024 accepted transitions, refuse before another root lookup. At 1,023 transitions, an explicit missing successor can complete catch-up. This conservative rule is intentional. Do not claim that 1,024 transitions complete successfully.

Count every response body against the aggregate cap, including discarded metadata. Use identity content encoding. Refuse other encodings. Bound a response before parsing or allocation. Stop the request at its deadline. Cancel and discard the client after a transport or store error.

Use only the exact HTTPS bases from the authenticated repository policy. Allow no user information, query, fragment, redirect, or alternate origin. The identity part owns URL and target-path bytes. The selected path is `catalogues/<app>/<channel>/windows-x64/current.json`. Construct its components from verified Host policy.
A missing current target means withdrawal. An archived target cannot replace it.

Release publication uses maximum lifetimes of 365 days, 30 days, 7 days, and 24 hours. These values apply to root, targets, snapshot, and timestamp, respectively. FACT: TUF metadata has an expiry value but no mandatory issue-time value. Therefore, client checks cannot prove the original lifetime.
The client caps remaining lifetime against its fixed refresh time. The release signer must separately enforce the publication lifetime.

### D-22: Use a stable repository identity

PROPOSED: `RepositoryId` contains 32 opaque commissioned bytes. It does not contain a channel, key, SPKI, URL, or policy digest. The signed Host expectation binds this ID and exact repository-policy digest. The identity part owns the repository-policy codec.

Profile 1 assigns exactly one app ID to each repository ID. Cross-app repository sharing is unsupported. Require exact equality between Host appId, RepositoryPolicy appId, and enrollment-anchor appId. A mismatch refuses before metadata loading. Channels, origins, signing keys, and SPKIs still share that app's existing repository floors.

The Windows part owns these OS-derived paths:

- Base: `LocalAppData\\GYLDLAB\\Keld\\acquisition-v1`.
- Anchor: `enrollments\\<lowerhex(repository_id)>.anchor` below the base.
- Store: `repositories\\<lowerhex(repository_id)>\\` below the base.

No caller supplies the base. The admitted SID and retained parents bind each operation. A successor policy preserves this store. Do not weaken its root or role floors. A changed policy requires the authenticated transition specified by the identity owner.

FACT: KEL-270 excludes complete rollback by the owning user or native malware. A user-controlled marker cannot defeat an actor who can replace both marker and store. This design does not claim that stronger threat protection. Bun roles and webviews remain outside the write authority.

### D-23: Freeze store records and their ownership

PROPOSED: KEL-53 owns one metadata store. The store uses immutable blobs, immutable generations, and one replaceable current record. The store holds its exclusive lease during mutation. No asynchronous callback can retain a write authority after the lease ends.

All local records use ASCII JSON with fixed field order. Use UTF-8 without BOM, whitespace, or a final newline. Use decimal unsigned integers without leading zeros. Use lowercase 64-character SHA-256 strings. Use exact JSON null for absent references. Reject duplicate, unknown, missing, reordered, or noncanonical fields.
Require `encode(decode(bytes)) == bytes`. Local record digests detect corruption. ACLs and retained handles provide protection.

Exact record definitions:

| Record | Ordered fields |
|---|---|
| Anchor | `schema,repositoryId,appId,ownerSid,enrollmentState,initialGenerationSha256` |
| Current | `schema,repositoryId,sequence,generationSha256` |
| Generation | `schema,repositoryId,sequence,recoveryEpoch,previousGenerationSha256,policySha256,utcFloorMillis,phase,root,timestamp,snapshot,targets,rootRefresh` |
| Role reference | `sha256,length,version` |
| Root refresh | `timestampKeysSha256,snapshotKeysSha256` |

Schema values are `keld.metadata-anchor/v1`, `keld.metadata-current/v1`, and `keld.metadata-generation/v1`. Anchor states are `pending` and `established`. Generation phases are `root-refresh`, `roles`, and `ready`. Only `root-refresh` has a non-null `rootRefresh`. The root reference is always present.
Role references can be null only before acceptance or after an authorized reset. Ready requires all four references.


Sequence is a positive u64. Its increment must succeed with checked arithmetic. Recovery epoch is a u64. Initial enrollment sets it to zero. Normal refresh preserves it. Only the private recovery operation advances it by exactly one. Version is a positive u64. Length is bounded by the referenced role's profile cap. UTC uses nonnegative Unix milliseconds
within the clock implementation's supported range. SID uses the native owner's canonical string.
Maximum anchor/current/generation lengths are 2,048/512/4,096 bytes.
Anchor appId uses the exact app-ID grammar and bytes from the identity part. Normal refresh and in-place recovery preserve this value.

Blob filenames are `blobs/<sha256>.json`. Generation filenames are `generations/<sequence>.json`. The current filename is `current.json`. Retirement-intent filenames are `retirements/<successorSequence>.json`, owned by
the same protected KEL-53 writer. A retirement intent is a separate bounded,
immutable-before-completion cleanup authority, never caller-supplied.
Its canonical ASCII JSON record has a 65,536-byte cap. Ordered fields:
`schema`, `repositoryId`, `ownerSid`, `oldSequence`,
`oldGenerationSha256`, `orphanSequence`, `orphanGenerationSha256`,
`mode`, `attemptId`, `exclusiveBlobs`.
`schema` is `keld.metadata-retirement/v1`; `mode` is `ordinary` or `recovery`.
`attemptId` is null only for ordinary mode. `orphanSequence` MUST be the
checked successor of `oldSequence`. `exclusiveBlobs` is a sorted,
duplicate-free, bounded list of canonical `{sha256,length}` references to
preparation blobs proved unreachable from every other protected owner.
Empty is valid. The intent pins the exact authenticated old/current state,
expected orphan digest and complete exclusive blob inventory. An existing
intent is never overwritten with different bytes. Ordinary retirement requires
the complete orphan and its references before intent publication.
Recovery retirement may include a not-yet-written successor or blob only when
D-65's protected prewrite plan and terminal aborted journal name it.
An absent planned object never authorizes deletion of another object.
For recovery mode, D-65 also binds the exact intent SHA-256 in the
protected aborted journal before deletion. A matching durable completion
digest MUST precede intent removal. Keep both journal digests through
verified pending-pointer absence.

Use only these validated components with retained-parent operations. Existing content-addressed objects require exact-byte and protection readback. Never overwrite an existing blob or generation with different bytes.

Role blobs preserve the exact downloaded bytes. Do not reconstruct signed role bytes from parsed fields. Before client loading, verify every reference, bound, digest, parse result, and role identity. A malformed established role cannot become an absent cache entry.

During root-refresh, quarantine downstream role references. Do not require their signatures to match a newly rotated root. Finish the recorded reset before loading those roles as trusted metadata.

### D-24: Freeze durable transitions

PROPOSED: Enrollment starts only with authenticated fresh-state evidence from the Windows owner. Publish the pending anchor before the first generation. Publish a generation with the independently authenticated bootstrap root. Publish current, then replace the anchor with its established form. Any partial enrollment refuses normal acquisition.
Only the recovery owner can resolve its recorded phase.

Each accepted mutation publishes one immutable generation:

1. Write all new blobs through retained parents.
2. Flush each new blob and verify its retained readback.
3. Write the next generation with its previous-generation digest.
4. Flush the generation and verify its retained readback.
5. Replace current through the existing same-parent primitive.
6. Flush and verify current through retained readback.
7. Return the new sequence only after all required native durability checks pass.

A failed operation returns no authority. Destroy the associated metadata client. Do not continue with its in-memory state.

On restart, the current record determines the accepted generation. A valid unreferenced next generation is not automatically committed. A corrupt current record cannot select an older generation. A missing established current record requires recovery. An interrupted publish can resolve to the old or new complete current record.
Native qualification must prove each claimed crash and power-loss behavior. Source inspection alone does not prove durable replacement.

Before **any** ordinary metadata mutation, the owner MUST inspect the
fixed D-68 pending-operation pointer under the repository lease. A
matching pointer delegates only to its recorded owner; ordinary refresh
MUST refuse. A protected `current` record at sequence N is authoritative.
An existing `generations/<N+1>.json` cannot be overwritten with different
bytes and cannot be silently adopted as committed state.

With no pending pointer, the metadata owner MAY retire only the exact unreferenced
ordinary-publication successor at `generations/<N+1>.json`. Under the repository
lease, first authenticate the old protected current and established enrollment,
repository ID, owner SID, previous-generation digest, successor sequence,
recovery epoch, canonical orphan bytes and protection. Verify the complete
referenced-blob set and prove every proposed exclusive blob is unreachable
from current/LKG/previous-LKG and all other protected owners. No ordinary
cleanup is permitted while a D-68 pending pointer exists.

Before deleting any blob or generation, the writer MUST durably publish, flush
and read back the D-23 retirement intent for N+1 with mode `ordinary`, exact
old-state and orphan digests, and the complete sorted exclusive-blob list.
The same protected intent MUST be present on resume. If it already exists,
all fields and the old protected state MUST match exactly; a mismatch halts.
While that intent exists, no independent mutation may reuse N+1.

Under the retained-parent writer, delete only the intent-listed exclusive
blobs whose remaining bytes, owner and unreachability are verified. A listed
blob already absent on restart counts as previously retired ONLY when the
matching immutable pre-deletion intent remains authentic and the entire
protected old/current, LKG and ownership evidence is unchanged. Delete the
exact authenticated orphan generation, then verify its absence, the absence
of every listed exclusive blob and the unchanged established old state.
An orphan already absent may count as retired ONLY under that same
durable, previously verified intent. Missing unlisted or authoritative
objects, changed protections, foreign bytes or reachable references MUST
halt with the intent preserved; they never authorize another deletion.

Only after complete readback may the writer durably retire the intent itself
and verify its path absent. A subsequent authenticated mutation MAY create
new bytes at N+1 only after that verified absence. A crash at any deletion,
generation-removal or intent-removal edge resumes this exact finite
operation if its protected evidence is still coherent; otherwise it halts.
An absent orphan without a matching durable intent cannot authorize blob
deletions. The KEL-53 operator recovery owner investigates ambiguous state.
There is no caller-provided cleanup flag or empty-store fallback.

When `current` already names N+1, the successor is committed and MUST
NOT be retired. Manual recovery with a pending pointer uses only
D-65's explicit terminal-`aborted` path, not ordinary orphan cleanup.
Before D-65 publishes a fresh manual-recovery journal or D-68 pointer,
the same lease-protected owner MUST finish any pending ordinary N+1
retirement by these D-24 rules. Confirm both the ordinary intent
and N+1 absent before sealing a new recovery prewrite inventory.
If source identity, generation epoch or ownership is ambiguous,
refuse recovery without creating its journal or pending pointer.

For D-65 manual recovery only, a verified expired `prepared` journal,
matching D-68 pending pointer, protected old `current` and terminal
readback-verified `aborted` phase are required. The protected journal
MUST predate all preparation writes and bind the complete expected exclusive
blob list. Verify each present planned object by digest, protection and
retained owner. An absent planned object may not have been written.
Refuse unlisted attempt-owned preparation objects, not unrelated
authenticated shared objects. Before any deletion, the recovery owner MUST
durably publish and read back a D-23 intent at
`retirements/<oldSequence+1>.json` with mode `recovery`, the exact
`attemptId`, journal-bound expected successor digest and all planned
exclusively owned preparation blobs. It MUST prove that no present object
is reachable from current/LKG/previous-LKG or another protected owner.
Continue deletion only with the authenticated intent and pending pointer.
Delete only present exact intent-listed objects. An absent listed object needs
no deletion. Unlisted preparation, changed protected state or ambiguous
ownership MUST halt without deletion. Verify the successor path and all listed
blobs absent, the old current,
provenance, epoch, application and UTC floors unchanged. Before removing
the intent, durably record and reread the matching completion digest in
the aborted recovery journal. Then remove and reread the intent as absent.
If restart finds the pointer, no intent and both journal digests null,
cleanup has not started. Verify the complete protected prewrite plan,
old state, attempt and every present exact unreferenced planned object.
Refuse unlisted attempt-owned preparation objects, changed candidate
bytes and foreign ownership. Leave unrelated shared blobs untouched.
Publish and reread the D-23 intent before any deletion.
If both journal digests are non-null, require equality and matching old
state and attempt. Verify the expected successor and all journal-planned
exclusive blobs absent before D-68 pointer-only removal.
Mixed or mismatched journal digests MUST halt without deletion or pointer
removal. Keep the aborted journal through verified pointer absence.
A new attempt may use the immutable next-sequence filename only then.
When `current` already names the committed successor, abort is forbidden.

Before root catch-up, persist `phase=root-refresh`. Record fingerprints of the timestamp and snapshot key sets in the starting trusted root. Use canonical sorted key IDs and their canonical public-key bytes. Persist each accepted N-to-N+1 root before the next fetch. Keep the starting fingerprints across every successor and process interruption.

Fingerprint encoding is SHA-256 over the following concatenation:

1. ASCII `KELD-TUF-ROLE-KEYS-v1`, then one zero byte.
2. The distinct key count as unsigned 32-bit little-endian.
3. Each key, sorted by its lowercase 64-character TUF key ID.
4. For each key: its 64 ASCII ID bytes, canonical-key length as u32 little-endian, then canonical-key bytes.

Canonical-key bytes use the TUF canonical JSON encoding. Require correct TUF key IDs before this step. Do not include the role threshold in this fingerprint. Threshold changes still affect signature verification. Overflow, duplicate IDs, or absent key definitions refuse.

At the final valid root, compare the final role-key sets with those recorded fingerprints. When either set differs, clear both timestamp and snapshot references in one generation. Clear the targets reference with them. The targets version floor lives in snapshot metadata. This reset does not change the application floor or UTC floor.
Then enter `phase=roles`. Do not use targets while the root-refresh phase remains incomplete.

Persist accepted timestamp, snapshot, and targets in protocol order. Do not clear old downstream references merely because a newer timestamp arrived. Those references preserve comparisons until a replacement passes verification. Enter ready only after one complete matching role graph passes.

Keep previous accepted metadata during failed refreshes. Do not use that metadata as a silent failure fallback. A new admission needs a successful online outcome and current expiry checks.

### D-25: Make unchanged timestamp results explicit

PROPOSED: Timestamp loading returns `Advanced`, `Unchanged`, or a typed failure. An equal timestamp version returns Unchanged after authentication. Discard the replacement bytes. Keep the stored timestamp. Do not update its expiry, snapshot description, or signature envelope from the replacement.

Then continue snapshot and targets verification against the retained timestamp. Use matching cached objects or fetch their exact authenticated versions and hashes. Recheck their signatures, rollback rules, and expiry against this refresh's fixed time. This can complete a previous interrupted refresh without trusting changed same-version metadata.

FACT: S6 follows this sequence in [refresh lines 174–177](https://github.com/theupdateframework/python-tuf/blob/e897eec93e37843884d0f37e37d40aa45393e1bb/tuf/ngclient/updater.py#L174-L177).
Its [timestamp loader](https://github.com/theupdateframework/python-tuf/blob/e897eec93e37843884d0f37e37d40aa45393e1bb/tuf/ngclient/updater.py#L427-L467) discards equality.
The outer refresh still loads snapshot and targets.
The [trusted-set comparison](https://github.com/theupdateframework/python-tuf/blob/e897eec93e37843884d0f37e37d40aa45393e1bb/tuf/ngclient/_internal/trusted_metadata_set.py#L247-L278) uses the same pin.

A successful unchanged timestamp is an online outcome. A failed fetch cannot use it. An expired retained timestamp refuses. A signed same-version replacement with a new expiry cannot extend authority. A signed same-version replacement with another snapshot description cannot select another graph.

### D-26: Capture the clock and bind authority at use

PROPOSED: Capture OS UTC and a monotonic start before the first request. Persist the UTC observation before network work. Refuse a value below the retained UTC floor. Use the fixed UTC value for the protocol refresh. Use strict expiry comparison without grace.

The deployment assumes a trustworthy OS clock within five minutes. A fresh client cannot prove absolute clock correctness from signed metadata. A valid signature cannot repair an incorrect clock. Root, catalogue, and key-loss recovery cannot reduce the UTC floor.

After a future clock value poisons the floor, correct the OS clock and wait until it reaches that floor. An operator can instead perform a clean-system reinstall with preserved evidence and user data. That action is outside in-place recovery. It does not authorize deleting or lowering this store's floor.

Before the first installation effect, acquire leases in this order:

1. Metadata admission.
2. Installation bootstrap or update.
3. Browser profile.

After every wait, check the monotonic deadline and all role expiry values. Check the store sequence and selected catalogue again under the metadata lease. Keep that lease across comparison and the first installation effect. The native owner consumes the one-shot permit on the admitted worker.

A fresh install has no existing bootstrap lock. Consume the private one-shot permit when entering the effectful operation. Hold the metadata lease. Next, persist and read back the admission record outside the installation tree. Then create the scaffold and exclusive bootstrap lock. Recheck ownership.
Keep the metadata lease until the durable record and exclusive bootstrap ownership exist. The recovery part owns the admission record. No installation-tree creation precedes its durable readback.

A new refresh can replace expired acquisition evidence before publication. It must independently select the same catalogue and package tuple. Withdrawal refuses. Changed bytes require selection and package verification to restart. Do not extend the old deadline. Do not fetch metadata during an admitted commit or rollback.

### D-27: Freeze the store boundary

PROPOSED: The following names are owner-local design types. They are not existing public APIs. The patch must retain the standard client as protocol owner.

```rust
trait MetadataStore {
    fn open(&mut self, context: &AdmittedRepository)
        -> Result<StoreSnapshot, MetadataError>;
    fn initialize(&mut self, fresh: FreshRepository, root: VerifiedBootstrapRoot)
        -> Result<StoreSnapshot, MetadataError>;
    fn rebootstrap(&mut self, recovery: VerifiedMetadataRecovery, graph: VerifiedRecoveryGraph)
        -> Result<ReadyRevision, MetadataError>;
    fn begin_refresh(&mut self, at: RefreshTime)
        -> Result<RefreshState, MetadataError>;
    fn accept_root(&mut self, state: RefreshState, root: VerifiedNextRoot)
        -> Result<RefreshState, MetadataError>;
    fn finish_roots(&mut self, state: RefreshState)
        -> Result<RoleRefreshState, MetadataError>;
    fn accept_role(&mut self, state: RoleRefreshState, role: VerifiedRole)
        -> Result<RoleRefreshState, MetadataError>;
    fn finish_refresh(&mut self, state: RoleRefreshState)
        -> Result<ReadyRevision, MetadataError>;
    fn admit_revision(&mut self, revision: ReadyRevision, now: AdmissionTime)
        -> Result<MetadataAdmissionLease<'_>, MetadataError>;
}
```

Ordinary metadata verification types have private constructors inside the pinned client adapter. VerifiedMetadataRecovery has a different owner. The independent recovery owner consumes VerifiedRecoveryAuthorization once to construct that narrow capability. The private client adapter verifies the replacement graph. Callers cannot manufacture either authority from a boolean.
VerifiedRecoveryGraph contains the complete verified root, timestamp, snapshot, and targets graph. It also binds the authorized catalogue target and bytes. The private adapter enforces the existing profile and complete-stream checks. Graph verification alone cannot authorize replacement of established trust.
Store state types are non-cloneable. ReadyRevision binds repository, enrollment digest, recovery epoch, generation, role digests, target, expiry, and clock deadline. It also binds the current policy digest. The metadata lease borrows the store.
The native owner derives the final consumed installation permit. The store never accepts an installation path.

Errors distinguish corrupt state, incomplete enrollment, rollback, expiry, changed revision, unavailable target, resource limit, and failed durability. Each error states the required corrective action. Retrying a deterministic refusal is not a corrective action.

### D-28: Preserve application recovery and scope

PROPOSED: Metadata code cannot write provenance, current application, LKG, profile data, or application floor. Catalogue authority cannot change the installed update key. The identity migration and recovery parts own those transitions. Root-quorum loss halts ordinary refresh. Only the independent recovery authority can authorize a replacement.
No empty-store option, command-line override, or generic reset API exists.

The recovery owner alone constructs VerifiedMetadataRecovery. It binds the old generation digest, old epoch, next epoch, new root, and exact replacement policy. It also binds the complete authorized catalogue graph. The store compares these values under its lease with VerifiedRecoveryGraph and current protected state.

Rebootstrap requires next sequence equal to old sequence plus one. It requires next recovery epoch equal to old epoch plus one. Both additions use checked arithmetic. The replacement UTC floor cannot decrease. Enrollment identity remains unchanged. A stale authorization, mismatched graph, overflow, or lower UTC value refuses.

Prepare and durably read back all replacement blobs before switching trust. Prepare one ready generation with the exact replacement graph. Atomically publish that generation through one current-record replacement. Return ReadyRevision only after its durable readback succeeds. Do not publish a root-only intermediate generation or fetch metadata after the switch.

D-65 owns the recovery journal and completion across stores. A pending D-65 operation blocks ordinary admission, even when metadata is ready. Normal refresh cannot obtain the recovery capability. A failed recovery operation cannot fall back to an empty store. RepositoryPolicy includes the identity owner's required recoveryPolicySha256 field.

The fixed D-68 pending pointer lets this repository owner detect incomplete operations without accepting an installation path.
Check that pointer under the lease before ordinary open, refresh, or admission. Only the recorded recovery operation can proceed.

# Protected metadata and client qualification

Part of: [index](index.md)
Owns: D-METADATA, the metadata store, protocol profile, and M-CLIENT qualification.

## 2. Spec refs: Metadata

FACT: The source baseline is product commit `7ff41173d28d58ae61ccbb3382d055c8ef9c9c97`. KEL-53 owns installation mutation and application recovery. KEL-270 owns its Windows transaction. The metadata store does not acquire a second installation writer.

FACT: This part refines note 269 at research commit `022d0c5640319c912e0fd34817622d836bf207bd`. The previous proposal did not close the storage or protocol qualification gates. The decision IDs below define the proposed implementation contract. They do not claim implemented behavior.

Primary source aliases:

- S1: [tough 0.24.0 source](https://github.com/awslabs/tough/tree/98d8eb8b2ce63515d9b4981c938ef6453c5b5771).
- S2: [TUF 1.0.37](https://github.com/theupdateframework/specification/blob/d4cb70582ba507b9265672792c0fc8a5f390b851/tuf-spec.md).
- S3: [KEL-53 specification](https://github.com/gyldlab/keld/blob/7ff41173d28d58ae61ccbb3382d055c8ef9c9c97/docs/specs/kel53-full-package-activation.md).
- S4: [updater invariants](https://github.com/gyldlab/keld/blob/7ff41173d28d58ae61ccbb3382d055c8ef9c9c97/crates/keld-update/AGENTS.md).
- S5: [original metadata proposal](https://github.com/0monish/keld-research/blob/022d0c5640319c912e0fd34817622d836bf207bd/inputs/external/keld-2026-11-15-pr1531-execution/metadata-spec.md).
- S6: [Python TUF reference workflow](https://github.com/theupdateframework/python-tuf/blob/e897eec93e37843884d0f37e37d40aa45393e1bb/tuf/ngclient/updater.py).

## 3. Acceptance criteria: Metadata

### AC-20: Only a coherent, current metadata revision can authorize installation

- **Given:** An enrolled repository, a valid worker, and an authenticated current catalogue.
- **When:** The worker requests installation authority.
- **Then:** The store returns a private admission revision under its lease. Expired, replaced, incomplete, or corrupt state refuses.
- **Negative control:**
  - **Single fault:** Remove the current-generation comparison.
  - **At-risk precondition:** A second refresh withdraws the target before the first worker obtains its lease.
  - **Expected RED:** The first worker creates an installation object. The test must detect that object.

### AC-21: Root progress and authorized counter reset survive interruption

- **Given:** Valid root successors and poisoned timestamp or snapshot counters.
- **When:** An authorized role-key rotation occurs, with interruption at each durable store cut.
- **Then:** Recovery retains every committed root. Recovery finishes the recorded reset or refuses. Application state remains unchanged.
- **Negative control:**
  - **Single fault:** Reconstruct the reset comparison from the recovered new root.
  - **At-risk precondition:** The process dies after root replacement and before downstream-role invalidation.
  - **Expected RED:** Valid replacement timestamp metadata cannot recover the poisoned counter.

### AC-22: Established state cannot become first use through partial loss

- **Given:** An established enrollment anchor and a committed metadata generation.
- **When:** A required record disappears, changes, or fails protection checks.
- **Then:** Normal acquisition refuses with a state-recovery error. It does not initialize a new store.
- **Negative control:**
  - **Single fault:** Treat a missing current record as an empty cache.
  - **At-risk precondition:** The anchor remains valid, but the current record is absent.
  - **Expected RED:** A lower bootstrap root or timestamp becomes eligible.

### AC-23: Metadata limits apply before allocation and after signed lengths

- **Given:** Authenticated metadata with a length above a profile bound.
- **When:** The adapter fetches that object.
- **Then:** The adapter refuses before the excessive allocation. No target authority exists.
- **Negative control:**
  - **Single fault:** Use the signed length without the independent object cap.
  - **At-risk precondition:** Snapshot metadata signs a targets length of 1,048,577 bytes.
  - **Expected RED:** The transport accepts that object or exceeds the cap.

### AC-24: Client boundary behavior matches the approved TUF profile

- **Given:** Authenticated fixtures for root, timestamp, snapshot, and targets.
- **When:** Expiry equality, an equal timestamp version, or root-counter overflow occurs.
- **Then:** Expiry equality refuses. Equal timestamps cannot replace trusted metadata. Arithmetic cannot wrap or panic.
- **Negative control:**
  - **Single fault:** Restore the upstream expiry comparison.
  - **At-risk precondition:** The fixed refresh time equals the timestamp expiry.
  - **Expected RED:** The metadata client accepts the expired boundary.

### AC-25: Incomplete target streams produce no authority

- **Given:** A valid metadata graph and a correctly authenticated target description.
- **When:** The stream yields a prefix, then ends with a digest or transport error.
- **Then:** No catalogue parser, installation call, or published evidence consumes that prefix.
- **Negative control:**
  - **Single fault:** Emit catalogue evidence before successful EOF.
  - **At-risk precondition:** The target contains valid JSON followed by a failing final digest.
  - **Expected RED:** The installation owner receives usable authority.

### AC-26: Repository changes cannot reset its trust state

- **Given:** One repository ID with an established root and role floors.
- **When:** Channel, origin, signing key, SPKI, or policy version changes.
- **Then:** The adapter opens the same repository ledger. It requires an authenticated policy successor where applicable.
- **Negative control:**
  - **Single fault:** Include the current policy digest in the ledger namespace.
  - **At-risk precondition:** A genuine successor Host carries a new repository policy.
  - **Expected RED:** The successor starts with bootstrap floors instead of the existing floors.

### AC-27: Metadata recovery cannot lower the application floor

- **Given:** Application version 1, a failed version 2, and a retained floor of 2.
- **When:** Metadata keys rotate and their counters reset.
- **Then:** Journal recovery can run version 1. A new version-2 update refuses. Version 3 remains eligible.
- **Negative control:**
  - **Single fault:** Reset application floor storage with metadata role storage.
  - **At-risk precondition:** The application floor and metadata state both exist.
  - **Expected RED:** The updater admits version 2 as a new candidate.

### AC-28: Threshold-only changes preserve historical metadata floors

- **Given:** Protected metadata at version 100, accepted under a one-signature threshold, and an authorized root with the same keys.
- **When:** That root raises the threshold to two signatures and the client receives a correctly authenticated replay at version 1.
- **Then:** The client refuses the replay. A valid version 101 remains eligible. Restart preserves each historical floor.
- **Negative control:**
  - **Single fault:** Restore the upstream conditional that skips old floors after failed verification against the new threshold.
  - **At-risk precondition:** The stored version 100 has one valid signature. Both incoming candidates meet the new two-signature threshold.
  - **Expected RED:** Version 1 passes the affected timestamp or snapshot comparison, or lowers a referenced targets version.

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

Blob filenames are `blobs/<sha256>.json`. Generation filenames are `generations/<sequence>.json`. The current filename is `current.json`. Use only these validated components with retained-parent operations. Existing content-addressed objects require exact-byte and protection readback. Never overwrite an existing blob or generation with different bytes.

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

## 5. Boundaries: Metadata

Implement the client port and protocol corrections against S1. Add the reviewed dependency through the workspace owner only when consumed. Implement the protected backend within `crates/keld-update/src/windows_baseline/`. Extend `crates/keld-update/src/windows_fs.rs` only through its existing unsafe owner.
Reuse its retained-parent publication and readback primitives. Any new replaceable record kind needs the explicit updater unsafe review.

Do not alter Host admission until the joined native contract passes. Do not alter package-manager ownership, update signatures, application floor, or browser profile policy. Do not add keys, production origins, or production authorization from test fixtures.

## 6. Tasks: Metadata

### T-20: Qualify and correct the pinned client

- **Blocked by:** T-80

- T-80 includes the approved specification and design-level dependency review.
- Compare the crate archive with the pinned source and checksum.
- Record exact features, transitive lockfile, license, advisories, and supported targets.
- Implement D-20 with discriminating regressions in the client.
- Run AC-28's historical-floor matrix, including its upstream-conditional mutant.
- Map applicable S2 clauses to exact functions and tests.
- Run upstream tests and Rust 1.99.1 builds.
- Publish the exact patch and independent review.

### T-21: Implement the durable store

- **Blocked by:** T-20, T-40

- Implement D-22 through D-24 with one KEL-53 writer.
- Run AC-21, AC-22, and AC-26 at every persistent cut.
- Repeat AC-28 after restart. Reject a Host or repository policy with a different app ID from the anchor.
- Exercise actual Windows subprocess termination and storage errors.
- Qualify power loss separately.

### T-22: Implement bounds and clock handling

- **Blocked by:** T-20

- Implement D-21, D-25, and D-26.
- Run AC-23 and AC-24.
- Test exact limits, limit-plus-one, cancellation, stalls, rollback, and expiry equality.

### T-23: Join metadata to native admission

- **Blocked by:** T-21, T-22, T-1, T-41

- Run AC-20, AC-25, and AC-27.
- Prove zero installation effects after changed revision or expired authority.
- Preserve one consumed permit and the existing transaction.

### T-24: Qualify the supported recovery cells

- **Blocked by:** T-23, T-2, T-62, T-81

- Ordinary rotation and counter-poisoning tests can run before this task.
- Qualify full supported recovery only after T-62 completes its manual recovery implementation.
- Run the joined identity/browser/LKG migration case and each supported manual recovery cell.
- Keep root-quorum-loss and installed-key-loss cells separately visible.
- Do not infer those passes from ordinary v2 qualification.

## 7. Test plan & evidence: Metadata

### E-20: Source comparison

- **Run:** Re-fetched S1 and S2 through the GitHub connector on 2026-11-15.
- **n:** One selected client revision and one normative revision.
- **Environment:** Read-only source inspection.
- **Limits:** No patched client, protocol test, or native execution occurred.

### E-21: Current-documentation receipt

- **Run:** Context7 resolved `/awslabs/tough` and queried persistence, reset, and equal-timestamp behavior.
- **n:** One narrow query, retrieved 2026-11-15.
- **Environment:** Discovery plus confirmation against S1 and S2.
- **Limits:** Context7 examples use develop. They did not decide the contract.

### E-22: Registry and build boundary

- **Run:** Historical native `cargo info tough@0.24.0 --registry crates-io`, session 16199.
- **n:** One package lookup, documented in S5's publication receipt.
- **Environment:** DESKTOP-3FMM79E, prior execution turn.
- **Limits:** This was not a build. This turn has no Rust compiler in its local Linux PATH.

Every AC above remains NOT_RUN. Definition review and golden-record checks do not change that status. A patch must make each negative control fail at its intended predicate. A later restored pass must use the same exact source.

## 9. Perf impact: Metadata

No performance improvement is claimed. The metadata profile bounds each object, refresh, request, and retained state operation. Reuse immutable blobs to avoid copying every role on each root transition. Garbage collection cannot delete the current generation, its referenced blobs, or pending recovery evidence.
Keep at most three generation files: current, its immediate predecessor, and one prepared successor. Retain every blob referenced by those files. A root-reset intent stays in current until completion. Delete only unreferenced objects after successful readback, under the store lease.
If cleanup fails, refuse the next mutation before another generation is prepared. Never delete the enrollment anchor. Never restore an older current record to satisfy storage bounds. A prepared successor cannot coexist with another successor. Measure native I/O and cold-start overhead before claiming acceptable latency.

## 10. Open questions, blockers & unknowns: Metadata

### B-20: Pinned client qualification remains unexecuted

- **Status:** open
- **Owner:** KEL-53 dependency owner.
- **Impact:** Production metadata acquisition cannot use the candidate.
- **Check:** T-20 passes on its exact reviewed source and supported compiler/targets.

### B-21: Native durability requires execution

- **Status:** open
- **Owner:** KEL-270 Windows transaction owner.
- **Impact:** Store code cannot claim crash or power-loss safety.
- **Check:** T-21 publishes the real Windows cut matrix and retained-state observations.

### B-22: Production authority and trusted clock remain uncommissioned

- **Status:** open
- **Owner:** Release/security owner.
- **Impact:** No production repository, profile, bootstrap root, or clock trust claim is available.
- **Check:** The authority contract has authentic provider, policy, delivery, and deployment receipts.

### U-20: Complete applicable-clause and transitive review

UNKNOWN: This inspection does not establish full TUF 1.0.37 conformance or all dependency risks. T-20 must finish the clause map and dependency review. A required missing rule blocks client qualification.


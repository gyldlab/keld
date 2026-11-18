# Implementation boundaries and tasks

Part of: [index](index.md)
Owns: Change boundaries, implementation tasks, and their dependency graph.

## 5. Boundaries: Identity

Implement codecs in `crates/keld-pack/src/expected_identity.rs` and its existing test module.
Reuse `crates/keld-pack/src/host_identity.rs` for section selection and container validation.
Implement policy observation in the existing Guard Authenticode owner after its exact unsafe grant passes review.
Implement provenance and transitions in `crates/keld-update/src/records.rs` and existing Windows transaction owners.
Update KEL-135, KEL-254, KEL-53, and KEL-270 owning specifications in the same approved change set.
Do not open Host initial-install authority in the codec slice.
Do not modify v1 bytes, the manifest signature format, strict-principal policy, production keys, or release permissions.

## 5. Boundaries: Metadata

Implement the client port and protocol corrections against S1. Add the reviewed dependency through the workspace owner only when consumed. Implement the protected backend within `crates/keld-update/src/windows_baseline/`. Extend `crates/keld-update/src/windows_fs.rs` only through its existing unsafe owner.
Reuse its retained-parent publication and readback primitives. Any new replaceable record kind needs the explicit updater unsafe review.

Do not alter Host admission until the joined native contract passes. Do not alter package-manager ownership, update signatures, application floor, or browser profile policy. Do not add keys, production origins, or production authorization from test fixtures.

## 5. Boundaries: Windows admission

Owner-authorized rollback of every local record remains outside the claimed boundary. A process-kill result does not prove physical power-loss durability. A filtered administrator token can fail the conservative initial token profile.
Each support claim requires its own native cell. Guard's current unsafe grant does not authorize every new query in D-42. Implementation requires exact owner-file updates and independent unsafe review before the additional queries ship.
Offline preflight can refuse a valid image when the local certificate chain is unavailable. It cannot fall back to network before discovery. Native zero-network qualification remains required.

## 5. Boundaries: Recovery

Implement in the existing Guard verifier, Host role decoder, KEL-53 records, and KEL-270 transaction owners.
Reuse KEL-135 physical-store ownership and identity codecs.
Do not change sandbox grants, KIPC authority, machine-seamless support, or managed deployment ownership.
Do not add a second recovery service, installer, or general-purpose broker.

## 5. Boundaries: Integration

This change writes the new specification directory and its separate evidence directory.
It does not edit runtime code, protected root policy, dependencies, production keys, or existing specifications.
After approval, T-82 reconciles the exact affected owner documents and unsafe operations before code uses them.
The original PR #1531 remains outside this documentation branch's write scope.

## 6. Tasks: Identity

### T-1: Implement the additive identity codec

- **Blocked by:** T-80, T-82

Add the v2 byte codec and exact vectors. Retain existing v1 vectors unchanged.
Run AC-1 and every malformed enum, OID, size, and cross-version case.

### T-2: Integrate one admitted identity transition

- **Blocked by:** T-1, T-23, T-42, T-81

Implement D-3 through D-9 and integration D-84 through D-87 with the existing owners.
Include D-89's MVP concurrency/census cases, not its non-gating future mechanisms.
Run AC-2 through AC-4, including the exact planned-policy repository fence and cross-store cuts.
Keep the legacy support cell disabled until its bridge and joined recovery evidence pass.

## 6. Tasks: Metadata

### T-20: Qualify and correct the pinned client

- **Blocked by:** T-80, T-82

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

## 6. Tasks: Windows admission

### T-40: Implement the admitted worker
- **Blocked by:** T-80, T-82

Extend Guard's existing query owner and captured-SID descriptor contract. Run AC-40 on each claimed token cell.

### T-41: Implement source and acquisition custody
- **Blocked by:** T-40, T-20

Extend the existing source and metadata owners. Run AC-41, AC-42, and AC-43 against actual Windows objects.

### T-42: Integrate one-shot admission
- **Blocked by:** T-41, T-1, T-60, T-23

Join the approved codecs, metadata generation, native custody, and recovery journal. Run AC-44 and AC-45 with controlled subprocess boundaries.

### T-43: Qualify installed behavior
- **Blocked by:** T-42, T-81

Use commissioned production policy and signed artifacts. Run the full applicable acceptance matrix and independent source review. Do not enable the Host entry before its authority and qualification gates pass.

| Existing owner path | Concrete implementation change after specification approval |
|---|---|
| `crates/keld-guard/src/windows_machine/uac_token.rs` | Extend query owner and bounded parser for admitted actual-worker context |
| `crates/keld-guard/src/windows_authenticode.rs` | Extend the single verified-signature identity owner for D-WIRE policy |
| `crates/keld-pack/src/expected_identity.rs` | Add versioned codec and signed policy digests from D-WIRE |
| `crates/keld-update/src/windows_extraction.rs` | Retain admitted archive sources and verified ancestry without pathname reopen |
| `crates/keld-update/src/windows_fs.rs` | Native handle operations and exact same-volume protected publication |
| `crates/keld-update/src/windows_baseline/initialize.rs` | Consume one-shot admission before any initialization effect |
| `crates/keld-update/src/windows_baseline/initialize/publication.rs` | Accept the retained validated TAR through publication |
| Existing KEL-53 metadata integration owner | Enrollment, generation, metadata lease, and retained readback |
| `crates/keld-host/src/windows_initial_install.rs` | Thin commissioned-policy adapter after upstream gates pass |

The current Host refuses unconditionally. The new adapter cannot replace that refusal before authority commissioning and its required checks. The first code slice can strengthen token and retained-source ownership while keeping the Host closed.
That slice still needs approved specification scope and its unsafe/public-API/security review gates. No native tests, physical power-loss tests, production signatures, release approval, or merge occurred in this work.

## 6. Tasks: Recovery

### T-60: Implement durable initial-admission recovery

- **Blocked by:** T-80, T-21, T-40, T-41

Implement D-64 through the existing writer. Run AC-60 with every publication cut and its isolated mutant.
Include D-68 pointer lookup, exact-attempt exclusion, bounded retention, and every pointer cut.

### T-61: Implement independent manual recovery authorization

- **Blocked by:** T-80, T-1, T-40, T-81

Implement the exact policy, dedicated verifier role, directive, and native confirmation.
Run AC-61, AC-62, AC-65, AC-66, and AC-68.
Reject ordinary application signers, unattended recovery, unauthorized policy changes, and incomplete commissioning.

### T-62: Join and qualify the common recovery transaction

- **Blocked by:** T-61, T-60, T-23, T-2

Implement D-65 and D-66 without another writer. Run AC-63, AC-64, and AC-67.
Prove successor and new exclusive-blob paths absent before sealing the plan.
Exclude existing shared blobs. Crash after `prepared`, terminal `aborted`, and pending-pointer removal.
Require idempotent retirement of exact or partial unreferenced planned
successor/blob objects only when the sealed plan, regular non-reparse type,
expected owner SID, KEL-53 DACL, retained handle and complete protected-owner
unreachability proof agree. Delete through the same handle; never adopt partial
bytes. Foreign, wrong-owner/DACL, unlisted, reparse, directory or referenced
objects MUST halt. A new attempt MUST be able to write different independently
authorized bytes at the required old-sequence-plus-one path.
Observe the metadata epoch and current/LKG state independently before a fresh attempt.
Qualify loss-only recovery separately from fresh-v2 and legacy cells.

## 6. Tasks: Integration

### T-80: Approve the complete exact specification

- **Blocked by:** none

Finish the draft, isolated review, finding refutation, definition checks, and repository structure check.
Publish the exact commit and recommendation. Obtain the explicit human sign-off required by AC-80.
Record all remaining implementation gates without declaring them passed.

### T-81: Commission the actual release and recovery authority

- **Blocked by:** T-80

The release/security owner records actual profile, issuer, repository, catalogue, and independent recovery policy evidence.
Commission and pin the durable recovery-only profile OID and issuer policy,
not the daily-rotating leaf SPKI. Verify separate signing permissions and custody,
application-signer and dual-profile refusal, changed-leaf-SPKI continuity
within the same recovery profile, and recovery-origin control.
Use no synthetic fixture as commissioning evidence.
Supply two genuine changed-SPKI artifacts for the unchanged commissioned publisher profile.
This task needs the existing release owner's operational authority. No draft approval manufactures those credentials.

### T-82: Reconcile governing contracts and precise native grants

- **Blocked by:** T-80

Update affected KEL-53, KEL-135, KEL-254, KEL-270, and architecture contracts in their owning changes.
Name each new token, file, or signature operation in the nearest existing unsafe owner.
Obtain independent unsafe, permission, public API, dependency, and wire gate evidence for the exact applicable changes.
Do not add a blanket allowance or a parallel policy owner.

### T-83: Execute the complete joined acceptance matrix

- **Blocked by:** T-24, T-43, T-62, T-81

Run every applicable case in `acceptance-plan.json` and every added AC in this specification.
Keep source, artifact, device, mutant, restored pass, and exact observed state in each receipt.
Run required format, clippy, workspace, CI, and independent exact-head code reviews.
Release only the cells whose authority and native evidence pass. Keep unsupported cells explicit.

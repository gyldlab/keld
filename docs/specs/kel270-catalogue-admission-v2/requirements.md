# Sources and acceptance requirements

Part of: [index](index.md)
Owns: Source facts and acceptance criteria for every owner contract.

## 2. Spec refs: Identity

FACT: Product pin P is `7ff41173d28d58ae61ccbb3382d055c8ef9c9c97`. PR #1531 still named P during this source check.
FACT: `crates/keld-pack/src/expected_identity.rs:16@7ff41173d28d58ae61ccbb3382d055c8ef9c9c97` owns the existing payload.
FACT: `docs/specs/kel135-persistent-profile-identity.md:330@7ff41173d28d58ae61ccbb3382d055c8ef9c9c97` defines SPKI-based Windows identity.
FACT: `crates/keld-update/src/records.rs:232@7ff41173d28d58ae61ccbb3382d055c8ef9c9c97` defines provenance v2.
FACT: `crates/keld-update/src/records.rs:345@7ff41173d28d58ae61ccbb3382d055c8ef9c9c97` requires a separately versioned lifecycle contract for a changed projection.
FACT: `crates/keld-host/src/windows_initial_install.rs:53@7ff41173d28d58ae61ccbb3382d055c8ef9c9c97` refuses initial installation.
This draft changes none of those approved contracts until the complete specification receives approval.

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

## 2. Spec refs: Windows admission

FACT: The Host refuses before opening the request. Source: `crates/keld-host/src/windows_initial_install.rs:53@7ff41173d28d58ae61ccbb3382d055c8ef9c9c97` at the product pin.
FACT: Current per-user initialization checks only the existing non-SYSTEM token rule. Source: `crates/keld-update/src/windows_baseline/initialize.rs:224@7ff41173d28d58ae61ccbb3382d055c8ef9c9c97`.
FACT: Publication reopens the TAR locator before validation. Source: `crates/keld-update/src/windows_baseline/initialize/publication.rs:15@7ff41173d28d58ae61ccbb3382d055c8ef9c9c97`.
FACT: Source opens share only reads and refuse unsupported file kinds. Source: `crates/keld-update/src/windows_extraction.rs:668@7ff41173d28d58ae61ccbb3382d055c8ef9c9c97` and `:854`.
FACT: Guard owns the pinned-handle verifier and matching VERIFY/CLOSE sequence. Source: `crates/keld-guard/src/windows_authenticode.rs:204@7ff41173d28d58ae61ccbb3382d055c8ef9c9c97`. FACT: The token owner bounds query allocations to one MiB. Source: `crates/keld-guard/src/windows_machine/uac_token.rs:35@7ff41173d28d58ae61ccbb3382d055c8ef9c9c97` and `:320`.

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

## 2. Spec refs: Integration

FACT: The repository requires human sign-off before implementation of a new feature specification.
Source: `docs/agents/spec-template.md:7` and `docs/agents/workflow.md:96` at the index pin.
FACT: KEL-53 owns application activation and rollback. KEL-270 owns the common Windows transaction.
FACT: KEL-135 owns persistent browser identity. KEL-254 owns signed Host expectations.
The parts cite those owners at the inspected product source.

FACT: Research note 269 contains the prior acceptance matrix and execution proposal.
Its immutable research commit is `022d0c5640319c912e0fd34817622d836bf207bd`.
`acceptance-plan.json` preserves its 19 groups and 110 planned case entries.
Every product case remains NOT_RUN. Definition checks do not replace those cases.

## 3. Acceptance criteria: Identity

### AC-1: One version has one interpretation

- **Given:** A valid v1 fixture and a valid v2 fixture.
- **When:** Each decoder receives both fixtures.
- **Then:** The matching decoder accepts. The other decoder refuses. Existing v1 bytes remain identical.
- **Negative control:**
  - **Single fault:** Change only the v2 domain or one enum to an unsupported value.
  - **At-risk precondition:** Preserve all field lengths and other syntax.
  - **Expected RED:** The owning codec refuses before it constructs an identity.

### AC-2: A signing-key change preserves the intended browser store

- **Given:** Two genuine Public Trust signatures have different SPKIs and the same admitted profile and app.
- **When:** The second artifact starts after complete first-artifact shutdown.
- **Then:** The same browser store retains independently seeded state. Different profiles cannot access that state.
- **Negative control:**
  - **Single fault:** Restore SPKI-only scope derivation in an isolated mutant.
  - **At-risk precondition:** Both Authenticode verifications and profile checks succeed.
  - **Expected RED:** The continuity oracle detects the changed store.

### AC-3: Planned transition requires every independent authorization

- **Given:** Fresh metadata authorizes one exact transition between admitted U1 and candidate U2.
- **When:** The existing writer validates the transition.
- **Then:** Metadata, U1, and U2 signatures agree before the writer publishes a transition journal.
- **Negative control:**
  - **Single fault:** Remove only the U1 signature.
  - **At-risk precondition:** Keep fresh metadata, U2 proof, catalogue, package, and OS admission valid.
  - **Expected RED:** The transition owner refuses without protected installation mutation.

Also materialize an authentic U1-different-from-U2 positive transition
with two distinct seven-reference policy cohorts. Include both
provenance records, one transition, both signature objects, and metadata
admission. All 20 required objects MUST fit and remain authenticated.
Add a 25th distinct evidence object as an isolated negative control.
The evidence owner MUST refuse before a consuming journal becomes effectful.

### AC-4: Joined migration and recovery preserve one complete state

- **Given:** An admitted predecessor has browser data and a last-known-good artifact.
- **When:** An authorized identity transition fails after each named durable cut.
- **Then:** Recovery yields the complete old state, complete new state, or explicit pending recovery. Browser data and the application floor remain intact. Mixed identity fields never permit ordinary launch.
- **Negative control:**
  - **Single fault:** Give the new logical identity a different lease for the retained physical store.
  - **At-risk precondition:** Run old and new launch attempts while the transition owns the installation lease.
  - **Expected RED:** The oracle detects concurrent ownership of one store.

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

Also terminate ordinary metadata publication after durable next-generation
write but before current-pointer replacement. The next valid refresh MUST
either authenticate and retire the exact orphan, or refuse and retain it
for named operator recovery. A different valid successor MUST not be
written over the orphan. After verified retirement, a different
authenticated successor MAY use the same N+1 name without lowering
metadata, application, or UTC floors.
Terminate at the durable D-23 retirement-intent cut, after deletion of an
exclusive blob, after removal of the orphan generation, and after intent
removal. Only an authenticated pre-deletion intent may treat an absent
listed blob as retired. A missing unlisted or still-reachable object MUST
refuse without deleting another object.

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

- **Equal-version controls:** Serve a valid two-signature v100 after the same-key threshold increase. The stored one-signature v100 remains floor-only. Refuse before snapshot selection or catalogue authority, with either identical or changed signed content.
- **Current-authority control:** Both stored and fetched v100 satisfy the current threshold. Identical canonical signed content permits `Unchanged`. Different snapshot descriptions or expiry values refuse. Different signature ordering alone does not refuse.
- **Authority negative control:** Remove only D-25's stored-envelope authentication. Keep incoming signatures valid and signed content identical. The below-threshold stored envelope must not authorize the retained graph.
- **Content negative control:** Remove only D-25's signed-content equality check. Keep both envelopes valid under the current threshold. A changed same-version snapshot description must fail the refusal assertion.
- Repeat these controls after restart. Retain replay-v1 refusal and valid advancing-v101 eligibility. Product execution remains NOT_RUN until the patched client exists.

## 3. Acceptance criteria: Windows admission

### AC-40: Actual-worker admission
- **Given:** A valid signed installation and a live Windows worker.
- **When:** The worker requests installation admission.
- **Then:** Only D-42's exact token profile can reach protected creation.
- **Negative control:**
  - **Single fault:** Remove the worker impersonation check.
  - **At-risk precondition:** The worker impersonates while all signatures and source inputs remain valid.
  - **Expected RED:** The OS-visible creation oracle observes the forbidden effect.

### AC-41: Managed preflight
- **Given:** Valid managed provenance at a required discovery root.
- **When:** A valid Direct candidate requests initial installation.
- **Then:** Admission refuses before every network request, namespace creation, and direct-writer acquisition.
- **Negative control:**
  - **Single fault:** Skip managed-owner precedence.
  - **At-risk precondition:** Current signatures, identities, and native protection remain valid.
  - **Expected RED:** The independent request counter, creation census, or writer-handle observer becomes nonzero.
The census starts at retained LocalAppData, above GYLDLAB, for clean-user and existing-tree fixtures.
Managed refusal creates no shared ancestor, acquisition object, or installation object.

### AC-42: Source object custody
- **Given:** A verified retained canonical TAR and a valid destination.
- **When:** Another process substitutes its diagnostic pathname.
- **Then:** Publication consumes only the original retained object or refuses before publication.
- **Negative control:**
  - **Single fault:** Reopen the pathname after verification.
  - **At-risk precondition:** Synchronize replacement after validation and before content copying.
  - **Expected RED:** The copied file digest or consumed file identity differs from the admitted object.

### AC-43: Established acquisition state
- **Given:** An established enrollment anchor and a current generation.
- **When:** The current generation becomes missing or malformed.
- **Then:** Normal refresh and installation refuse without reseeding from the embedded root.
- **Negative control:**
  - **Single fault:** Treat missing established generation as first use.
  - **At-risk precondition:** Preserve the valid enrollment anchor and independent installation reference.
  - **Expected RED:** The reset-generation or protected-write oracle observes unauthorized bootstrap.

### AC-44: First-effect authorization
- **Given:** Valid inputs and a blocked metadata lease acquisition.
- **When:** The generation changes or the 120-second admission deadline expires during the wait.
- **Then:** No installation-specific parent, scaffold, lock, or stage is created.
D-44 acquisition provisioning does not authorize those objects.
- **Negative control:**
  - **Single fault:** Omit the post-wait generation or deadline recheck in its separate control.
  - **At-risk precondition:** Keep every unrelated signature, path, and ownership check valid.
  - **Expected RED:** A native object census detects the first unauthorized creation.

### AC-45: Exact commit classification
- **Given:** A valid admitted baseline transaction.
- **When:** A child process terminates at each publication boundary, including after provenance publication.
- **Then:** Restart reports incomplete, committed, or recovery-required state from protected readback.
- **Negative control:**
  - **Single fault:** Return success before final protected readback.
  - **At-risk precondition:** Interrupt after a partial or uncertain publication.
  - **Expected RED:** A success receipt disagrees with independent protected-record inspection.

### AC-46: Offline preflight cannot retrieve network data
- **Given:** A valid signed fixture, controlled certificate endpoints, and a managed installation.
- **When:** The native adapter performs preflight and local ownership discovery.
- **Then:** Neither the process nor delegated Windows certificate retrieval emits a network request. No namespace object or writer authority exists.
Start the creation census at retained LocalAppData, above GYLDLAB, including clean-user fixtures.
- **Negative control:**
  - **Single fault:** Remove `WTD_CACHE_ONLY_URL_RETRIEVAL` from preflight.
  - **At-risk precondition:** The signed fixture requires a locally absent intermediate available through its controlled AIA endpoint.
  - **Expected RED:** Independent endpoint or OS telemetry observes retrieval. Restored preflight refuses offline without retrieval.

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
- The verifier also refuses an ordinary application signer before a recovery authorization exists.
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

### AC-66: Recovery signer authorization is a separate cryptographic role

- **Given:** An independently commissioned recovery signer and a valid normal application signer.
- **When:** Each signs a recovery-role image with valid Authenticode and otherwise valid directive inputs.
- **Then:** Only the signer matching the protected recovery-only profile
  and issuer policies and all native signature checks authorizes recovery.
- Two authentic renewed certificate leaves with different SPKIs but the same
  independently commissioned recovery profile OID MUST both be eligible.
- A leaf with both application and recovery profile EKUs MUST refuse.
- An otherwise correctly signed ordinary application candidate containing
  the independently commissioned recovery profile OID MUST also refuse;
  the symmetric role predicate is required before granting launch or catalogue
  authority.
- The application-signed image MUST fail at recovery signer authorization, before trust or application mutation.
- **Negative control:**
  - **Single fault:** Replace the recovery-only signer predicate with ordinary application publisher verification.
  - **At-risk precondition:** Keep both Windows signature chains, directives, destination, and user consent valid.
  - **Expected RED:** The application-signed recovery image wrongly obtains recovery authorization.

### AC-67: An expired prepared recovery aborts durably before retry

- **Given:** A `prepared` journal, sealed prewrite inventory and pending pointer, with old metadata and application state still current.
- Before that journal, any valid orphan N+1 from interrupted ordinary
  refresh MUST be retired using D-24's protected ordinary intent
  while no pending pointer exists. The repository lease MUST cover
  retirement, verified absence, plan sealing and pointer publication.
- An ambiguous ordinary orphan or mismatched intent MUST block recovery
  before any new recovery journal or pending pointer is published.
- **When:** The process crashes, the directive expires, and the recovery owner restarts.
- **Then:** It refuses expired trust commit and readback-verifies terminal `aborted`.
  It retires only present exact planned preparation objects before pointer removal.
  It preserves established current/LKG.
- Terminate before the first preparation blob, during blob publication,
  before successor generation publication, and before current replacement.
  Each cut with only absent or exact authenticated exclusive objects
  MUST allow bounded abort without deleting shared or unrelated objects.
  The complete protected plan MUST predate every preparation write.
- An incomplete, corrupt or foreign canonical object MUST halt automated
  cleanup, preserve old current, journal and pointer, and require
  KEL-53 operator disposition. No deletion or orphan adoption is allowed.
- Before deleting any preparation object, the owner MUST read back a D-23
  retirement intent binding the aborted attempt, expected successor digest
  and complete sealed exclusive-blob inventory. Planned objects MAY be
  absent because they were never written or were already retired.
  Unlisted, altered, foreign or live-referenced objects MUST refuse deletion.
- Before deletion, the aborted journal MUST durably bind the exact intent
  digest. After complete retirement readback, it MUST durably bind the same
  completion digest before removing the intent.
- With intent absent and both journal digests null, restart MUST verify
  the protected prewrite plan, old state, pointer and all present planned
  objects. It MUST publish a new D-23 intent before any deletion.
- With intent absent and both journal digests non-null, restart MUST
  verify equality, unchanged old state, matching attempt and absence
  of the successor and all planned exclusive blobs.
  It MAY remove only that pointer. Mixed or mismatched digests MUST refuse.
- A distinct newly authorized replacement MUST be able to reuse the
  immutable next-sequence filename after safe retirement. An abandoned
  successor file MUST NOT silently grant trust or block the new attempt.
- A new signed directive and fresh consent MAY start a new attempt only after verified pointer absence.
- Recheck old generation, provenance, epoch, protected policies, destination, current/LKG, and browser binding.
- An altered replacement or ambiguous first attempt MUST refuse without replacing established trust.
- **Negative control:**
  - **Single fault:** Resume the expired `prepared` journal as though its original directive remained valid.
  - **At-risk precondition:** Keep source, replacement graph, and all unrelated authorization checks valid.
  - **Expected RED:** The stale attempt commits trust without fresh authority or leaves conflicting pending state.
  - **Additional independent fault:** Remove or change only the completion digest after intent removal.
  - **Additional condition:** The aborted journal, old state and pointer remain valid; the intent is absent.
  - **Additional RED:** Restart removes the pointer or admits a new attempt without matching completion evidence.
  - **Additional independent fault:** Remove the sealed prewrite inventory before the first preparation write.
  - **Additional condition:** The pointer remains and only some planned objects exist when the directive expires.
  - **Additional RED:** The owner deletes a shared/unlisted object or commits a partial successor.
  - **Additional independent fault:** Refuse initial-intent creation whenever both journal digests remain null.
  - **Additional condition:** The aborted journal and matching pointer are valid; all present preparation objects match the plan.
  - **Additional RED:** The next authorized attempt remains blocked despite valid bounded cleanup.
  - **Additional independent fault:** Truncate one canonical planned blob
    during its first write while all other protected evidence remains valid.
  - **Additional RED:** Cleanup deletes or adopts the damaged blob, removes
    the pointer or changes established current without operator disposition.
  - **Additional independent fault:** Leave an authentic uncommitted
    ordinary-refresh N+1, normal epoch and valid D-24 retirement evidence
    before starting recovery. Without predecessor cleanup, recovery's
    immutable N+1 conflicts with different epoch and remains blocked.
  - **Additional condition:** Keep old current valid, no pending pointer
    and exact ordinary orphan ownership. Also test a foreign or corrupt
    ordinary orphan independently.
  - **Additional RED:** Recovery creates its journal/pointer over the
    ordinary orphan, or deletes a foreign one. Correct behavior retires
    only the authentic ordinary orphan first, or refuses before recovery.

### AC-68: Loss-only policy changes are limited to bootstrap root replacement

- **Given:** An established installation with an unavailable source update-key signer U1 and unchanged protected recovery-signer policy.
- A valid independent recovery directive binds the replacement metadata root and destination update key U2.
- **When:** A signed recovery-only Host, the valid directive, and fresh native consent authorize the loss-only operation.
- The owner compares canonical source and destination repository policy before trust commit.
- **Then:** The transaction commits only the directive-bound root and U2 without any signature from unavailable U1.
- Only `bootstrapRootSha256` and increasing `bootstrapRootVersion` MAY change in RepositoryPolicy.
- All other repository-policy fields and the installed recovery policy MUST remain exact.
- Bind the source directive root to the *currently accepted protected metadata root*,
  not blindly to the original bootstrap root. The independently authorized
  replacement root version must exceed that current verified version.
- Repeat with one isolated change each to recovery policy, recovery signer, catalogue keys, URLs, and discovery.
- Each unauthorized change MUST refuse before established trust changes.
- A normal planned U1-to-U2 transition without the old U1 signature MUST refuse under AC-61.
- A loss-only request without independent recovery authorization MUST refuse before established trust changes.
- **Negative control:**
  - **Single fault:** Compare only repository identity and replacement root, omitting the closed field matrix.
  - **At-risk precondition:** Independently authenticate the directive and keep all other policy fields valid.
  - **Expected RED:** An unrelated policy substitution enters the newly trusted repository view.

## 3. Acceptance criteria: Integration

### AC-80: One approval names one complete version

- **Given:** A complete specification with an immutable commit and reviewed findings.
- **When:** The owner approves implementation.
- **Then:** The approval record names that exact commit and scope. Later contract changes require a new recorded disposition.
- **Negative control:**
  - **Single fault:** Substitute another specification commit after approval.
  - **At-risk precondition:** The substituted commit changes one normative identity or recovery field.
  - **Expected RED:** The implementation gate refuses the unapproved version.

### AC-81: Evidence has one explicit claim

- **Given:** Definition checks pass but native implementation has not run.
- **When:** A reviewer evaluates production readiness.
- **Then:** Native and release gates remain open. A definition pass cannot mark a product case passed.
- **Negative control:**
  - **Single fault:** Mark one native case passed using only a JSON fixture check.
  - **At-risk precondition:** The native case requires actual Windows handles or signed artifacts.
  - **Expected RED:** The evidence review rejects the unsupported qualification.

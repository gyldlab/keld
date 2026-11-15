# Integration, approval, and qualification

Part of: [index](index.md)
Owns: The combined contract, task dependencies, approval record, and evidence limits.

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
| v2 planned key or identity transition | Defined | Fresh metadata, required U1/U2 proofs, joined browser/floor/LKG case |
| v2 loss-only manual recovery | Defined under D-61 prerequisites | Independent credential custody, exact directive, native ceremony and recovery cuts |
| v1 to v2 | Conditional on B-2 | Original-signer bridge and joined retained-data qualification |
| Known managed installation | Refuse direct mutation | Zero-network refusal and zero direct-writer authority |
| Machine-seamless or managed takeover | Outside this feature | Existing deployment-owner work; no implication from direct tests |
| Suspected system compromise | No in-place safety claim | Clean-system operator recovery and explicit data disposition |
| Lost independent recovery authority | No in-place replacement | Separately authenticated clean installation and retained original evidence |

The release record must name each supported cell. A passing fresh install cannot qualify migration or recovery.
Do not ship a broad recovery claim while the independent authority or joined evidence is missing.

## 5. Boundaries: Integration

This change writes the new specification directory and its separate evidence directory.
It does not edit runtime code, protected root policy, dependencies, production keys, or existing specifications.
After approval, T-82 reconciles the exact affected owner documents and unsafe operations before code uses them.
The original PR #1531 remains outside this documentation branch's write scope.

## 6. Tasks: Integration

### T-80: Approve the complete exact specification

- **Blocked by:** none

Finish the draft, isolated review, finding refutation, definition checks, and repository structure check.
Publish the exact commit and recommendation. Obtain the explicit human sign-off required by AC-80.
Record all remaining implementation gates without declaring them passed.

### T-81: Commission the actual release and recovery authority

- **Blocked by:** T-80

The release/security owner records actual profile, issuer, repository, catalogue, and independent recovery policy evidence.
Verify separated credential custody and recovery-origin control. Use no synthetic fixture as commissioning evidence.
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

## 7. Test plan & evidence: Integration

The fixture and review files are in `docs/engineering/evidence/kel270-catalogue-admission-v2/`.

### E-80: Definition and structure checks

- **Run:** `python docs/engineering/evidence/kel270-catalogue-admission-v2/verify_identity_vectors.py` and the canonical `just spec-structure` in the owning repository.
- **n:** Exact counts and outputs appear in the publication receipt.
- **Environment:** Definition fixtures and the isolated documentation worktree at the recorded commit.
- **Limits:** No product codec, patched client, native signature, crash, power-loss, or release qualification follows from these checks.

### E-81: Independent specification review

- **Run:** Isolated review of the exact snapshot, one lens per reviewer, with independent finding refutation.
- **n:** The review receipt records completed lenses and findings. Pending reviews grant no evidence.
- **Environment:** Exact file hashes and source pins in `review-receipt.json`.
- **Limits:** Design review does not prove implementation, native behavior, or human approval.

### E-82: Original acceptance coverage

- **Run:** NOT_RUN. The preserved matrix defines cases, mutants, and independent oracles for later implementation.
- **n:** 19 groups and 110 planned case entries.
- **Environment:** Original research source plus this specification's exact owner contracts.
- **Limits:** These are planned cases. Count is not coverage evidence or a test pass.

| Group | Owner contract | Implementation tasks |
|---|---|---|
| A01 | Acquisition and independent authentication | T-20, T-22, T-41, T-42 |
| A02 | Literal catalogue signature | T-1, T-22 |
| A03 | Closed catalogue parser | T-1 |
| A04 | Metadata and catalogue role separation | T-20, T-1 |
| A05 | Complete identity and package binding | T-1, T-2, T-41, T-42 |
| A06 | Metadata bounds, time, and admission | T-21, T-22, T-23 |
| A07 | Root progression | T-20, T-21 |
| A08 | Root recovery and durable state | T-20, T-21, T-24, T-62 |
| A09 | Installed update-key authority | T-2, T-22, T-61 |
| A10 | Stable publisher and changed SPKI | T-1, T-2, T-81 |
| A11 | Actual worker token | T-40 |
| A12 | Owner discovery and admitted root | T-40, T-41, T-42 |
| A13 | Retained source and object replacement | T-41, T-42 |
| A14 | Single initializer and concurrency | T-42, T-60 |
| A15 | Commit and interruption | T-42, T-60 |
| A16 | Floor, finite LKG, and recovery | T-2, T-21, T-62 |
| A17 | Deployment boundary | T-42, T-81 |
| A18 | Installed signed behavior | T-2, T-43, T-81 |
| A19 | Joined identity, browser, floor, and LKG | T-2, T-24, T-62 |

Each added AC requires its own discriminating test. A prior research case cannot silently substitute a different oracle.
Use actual OS observations for OS claims. Keep physical power loss separate from process termination.

## 9. Perf impact: Integration

The specification itself changes no runtime behavior.
The implementation adds bounded cold acquisition and recovery work.
Measure native overhead against the same signed baseline before making a performance claim.

## 10. Open questions, blockers & unknowns: Integration

### B-80: Exact-version human sign-off is pending

- **Status:** open
- **Owner:** Monish Khandelwal, specification sign-off.
- **Impact:** Feature implementation cannot begin from this draft.
- **Check:** A recorded approval names the exact complete specification commit.

### B-81: Independent combined review

- **Status:** closed
- **Owner:** Isolated reviewers and the coordinating author.
- **Impact:** The three R3 design lenses reported no remaining concrete draft defect. Implementation gates remain open.
- **Check:** `review-receipt.json` binds the exact R3 snapshot, independent finding refutations, corrections, and final lens dispositions.

No unresolved design choice may be hidden as a release gate.
Product and native unknowns remain in the owning parts until their stated execution checks pass.

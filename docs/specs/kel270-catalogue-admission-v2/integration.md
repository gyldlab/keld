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
| v2 planned key or identity transition | Defined | Fresh metadata, required U1/U2 proofs, joined browser/floor/LKG case |
| v2 loss-only manual recovery | Defined under D-61 prerequisites | Independent credential custody, exact directive, native ceremony and recovery cuts |
| v1 to v2 | Conditional on B-2 | Original-signer bridge and joined retained-data qualification |
| Known managed installation | Refuse direct mutation | Zero-network refusal and zero direct-writer authority |
| Machine-seamless or managed takeover | Outside this feature | Existing deployment-owner work; no implication from direct tests |
| Suspected system compromise | No in-place safety claim | Clean-system operator recovery and explicit data disposition |
| Lost independent recovery authority | No in-place replacement | Separately authenticated clean installation and retained original evidence |

The release record must name each supported cell. A passing fresh install cannot qualify migration or recovery.
Do not ship a broad recovery claim while the independent authority or joined evidence is missing.

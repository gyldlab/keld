# Learning lifecycle lint extension

Status: isolated proposed follow-up to the stable PR1320 migration. No product boundary change.

The existing reader omitted status from metadata-only output. It accepted stale records without dated evidence.
The retained failure-first control reproduces both behaviors at2ed1bdff.

The follow-up exposes status, verification and retirement markers without reading bodies or peer metadata.
Retirement needs a marking date, concise reason and immutable evidence locator. Supersession needs reciprocal,
acyclic replacement links. A later-stale replacement keeps backward history. Imported records need new proof
before verified use; their original source and migration quote stay intact. Neither age nor a later filename
proves invalidity. These are local Keld rules, not an adoption of a provenance ontology.

The lifecycle owner is routed from .agents/index.md. The directory AGENTS links that owner.
Root and the lookup index stay link-only. No always-chain or per-record cap increases.
All168 original records remain byte-identical and reconstruct the unchanged original source hash.

Verification:27 filesystem tests pass (13 existing plus14 lifecycle). Actual168-record check passes.
Context/inventory, atomic protocol, llms/corpus and format gates pass. Missing-route control fails and restores.
In disposable copies, disabling metadata enforcement makes its regression fail; disabling graph enforcement
makes the cycle regression fail. Both restored controls pass. Actual metadata-only tests reject peer/body reads.
The checker validates shape, dates, locators and graph structure. It cannot establish semantic truth,
replacement scope, cited-test success or full STE dictionary compliance.

The worker first used an invalid fixture precondition for one old ledger test; that failed attempt is retained.
The fixture then supplied valid verification evidence so the intended ledger oracle remained observable.
No assertions or source preservation predicates were weakened.

The implementation was hash-transferred from the original task tree into a new stacked worktree.
Only owned pending files were restored in the old tree. PR1320 remains at2ed1bdff and clean.
The lifecycle test module is admitted through the existing agent-context-test recipe using unittest discovery.

The combined6af implementation received independent review and refutation. Confirmed corrections
are recorded in [the correction report](learning-review-corrections.md). Exact corrected-head review is pending.
Reader fingerprints remain proposed until their independent source review; freshness/full CI is not claimed.
Full native workspace acceptance, default-runtime memory and actual Drawio acceptance remain separate.
No merge, ready flag, release or production change is authorized by this report.

Rollback: retain the branch/evidence and revert only this follow-up. Original migration and record identities remain.

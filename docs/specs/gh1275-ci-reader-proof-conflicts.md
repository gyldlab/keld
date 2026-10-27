# Spec: CI reader provenance without shared-hash PR conflicts

Status: draft
Linear: GH-1275 (GitHub tracker of record under #517) · Owner: GYLDLAB · Updated: 2026-10-27

## 1. Goal & non-goals

Remove Git conflicts caused only by disjoint pull requests rebinding one shared reader-set SHA-256 in tools/ci-inputs.json, while keeping stale-reader detection across commits, the Rust filename census, reviewed external-input edges, and all KEL-81 CI routing behavior intact.

Non-goals: no application/runtime/permission/wire/API changes; no weakening of Linux GUI, three-OS, Bun, CodeQL, gitleaks, cargo-deny, generated docs or CI required checks; no automated approval of a changed dependency scope; no changes to the separate KEL-169 trusted-provenance project.

## 2. Spec refs

Existing approved KEL-81: docs/specs/kel81-ci-change-routing.md. Current digest contract: docs/engineering/decisions.md and tools/ci_inputs.py. CI rules: .agents/ci.md, .agents/testing.md, docs/agents/workflow.md and docs/agents/spec-template.md. Architecture 01 and 06 describe tooling context; no product boundary change. Tracker: https://github.com/gyldlab/keld/issues/1275, linked from the completed KEL-81 issue.

## 3. Acceptance criteria (binary; each has a negative control)

1. Existing source: when any registered reader changes without approved provenance, freshness verification rejects it and no later PR may use the old exclusions.
2. Across-merge drift: when reader A expands an external read and changes in an earlier commit without reattestation, a later PR editing that input cannot silently skip the consumer. The original failure-first Keld regression must pass.
3. New and removed readers: a new watched path requires a per-path source proof; deleting, renaming, or deregistering an old watched reader requires an explicit deletion tombstone. Otherwise both check and classify fail closed.
4. Rust membership: tracked/untracked additions or removals under crates, missing regular files and symlink substitutions cannot retain old census approval without checked, path-local membership evidence.
5. Scope: deleting or changing a complete consumer, consumer identity/output set, reader set, reader pattern, package edge, documentation-only narrowing, or known-input rule without verified scope review rejects before selection. Newly added or renamed consumers must preserve unique output ownership. A source hash never creates an input edge.
6. Proof corruption: duplicate/unknown fields, invalid types, wrong path, stale, orphan, replayed, unexpected tombstone, malformed or missing file fail before any narrow selector is published.
7. Provenance: an unavailable **PR comparison base** continues to select all affected lanes as KEL-81 requires. Separately, an unavailable immutable **proof baseline** must not authorize narrowing; the approved design must prove a typed refusal or a genuinely safe full-gate fallback. Re-pinning to an attacker-controlled source tree is forbidden.
8. Routing equivalence: identical reviewed code selects exactly the same complete gate/package/OS vector for every original required case, including GitHub range, push, local worktree, path-only, unknown, ignored and untracked cases. Any narrowing requires separate approval.
9. Merge benefit: independent changes to two distinct readers that formerly rewrote the same broad digest now merge cleanly with both sources and proofs retained; genuine overlapping source changes may still conflict.
10. Verification: fail-first regressions, CI router tests, hygiene, workflow YAML validation, exact root just ci, independent CI/security review and final-tip hosted CI required (including real platform gates) pass before merging an implementation.

## 4. Design

**No Keld product boundary change.** The single existing CI input owner remains tools/ci_inputs.py and the existing scheduler remains tools/ci_changes.sh. Reject introducing a second selector or a bypassable validation wrapper.

**First-principles trade-off.** The legacy shared hash is a durable source-state fingerprint but a hot Git conflict point. Pure runtime hashing has no durable approved past state: the existing reader-widening regression PASSES on legacy and FAILS on the naive hybrid. A seven-way digest split still conflicts inside the common tools group. A one-file-per-reader-plus-crate-census pilot needed 173 initial files (~728 KB).

**Candidate:** bind a frozen, externally reviewed Git tree/commit rooted in the trusted main history as the source baseline, not a PR-writable moving ref or a candidate-selected hash. The baseline identity itself must have authenticated admission; otherwise this candidate is NO-GO. Only readers modified from that baseline acquire a path-local proof file keyed by the exact normalized relative path. A proof records that path plus LF-normalized SHA-256; a removed historical reader instead gets a tombstone. For Rust census changes, add a path-local membership record or deletion tombstone. For policy modifications, bind the exact changed semantic node (consumer, reader set, known inputs, package mapping) with a distinct reviewed record. Shared data policy remains canonical; no ordinary PR updates the broad aggregate SHA in JSON.

**Validation:** one mandatory, schema-versioned validator runs for EVERY classifier and check path before any output. It also binds the whole consumer and reader-set inventory, checks unique output names, and refuses full-object removal or shadowing without the separately reviewed transition. It confirms full baseline membership including *missing* old readers, proof identity, exact schema, regular file/mode status, no untracked certification, and policy-node consistency. The old digest algorithm is initially reconstructed from reviewed source proofs to compare routing. A failure must never turn into a successful skipped test. The immutable base must be present in the checkout or fail closed. Windows CRLF handling must match current approved normalization.

**Trust limitation:** SHA or a self-authored proof is NOT evidence of human/independent semantic approval. A PR author can synthesize a matching proof after omitting a newly introduced source-to-input edge; therefore a current-tip independent review receipt must bind the source, declared edges, baseline and approval identity. Without a trusted source of this receipt, migration is NO-GO and existing committed SHA rules remain. New cross-tree reads still require explicit scope review. An authenticated automatic bot or reviewer signature is not in this draft and would require separately approved KEL-169-compatible trust. No workflow secret or pull_request_target privilege change is authorized.

**Migration:** shadow the legacy selector over the same merge candidate, compare exact output and post-merge stale-reader cases, and keep legacy as the only authority until all gates pass. Switch once on an approved PR, remove temporary dual routing, and update docs/engineering/decisions.md. If the extra records, unverified scope, lost Git objects or CI overhead erase the advantage, keep legacy and manual rebind.

## 5. Boundaries

Draft: docs/specs/gh1275-ci-reader-proof-conflicts.md only.

Approved future implementation may edit tools/ci_inputs.py, tools/ci-inputs.json, tools/test_ci_local.py, tools/ci_changes_test.sh, per-path proof files and the routing decision record. Edit tools/ci_changes.sh, justfile or CI workflow only if a passing negative control requires it, with independent CI review. Do not touch product crates, Cargo manifests, other agents' branches or shared PR #1075/#1253.

## 6. Tasks (one concern per slice)

- [ ] T1. Obtain a human approval on this draft and verify current owner/claim and conflicting PRs.
- [ ] T2. Implement one exact proof admission boundary and persistent base/path binding without global digest edits.
- [ ] T3. Add independent red/green source, membership, policy, malformed-proof, missing-baseline, symlink/ignored/untracked and across-merge tests.
- [ ] T4. Prove full router differential parity and a two-branch conflict reduction on exact frozen inputs.
- [ ] T5. Complete local/full hosted gates and independent final-tip review; migrate only on GO.

## 7. Test plan

Reuse tools/test_ci_local.py InputContractTests, ProductionConsumerTests and FreshnessGateTests, plus tools/ci_changes_test.sh. The naive hash-free candidate must fail the historical reader-widening oracle; approved alternative must pass it. Add independent negative controls for historical reader deletion, new source membership and ghost/tracked-but-missing source, orphan/malformed approval, malicious baseline repin, an apparently valid self-authored proof with a missing consumer edge, complete consumer/reader-set deletion or rename, duplicate/shadowed outputs, unauthorized policy narrowing, unavailable proof/comparison bases, and GitHub/local execution-mode differences. Do not substitute an implementation-derived hash for an independent test oracle. Run full local and hosted exact-head checks on macOS, Linux and Windows; mocks only prove parser state, not hosted behavior.

## 8. Review gates triggered

unsafe: none; public API: none; permission model: none; dependency addition: none; wire protocol: none. CI routing, required-check applicability, reviewer trust and GitHub Actions provenance require independent CI-owner/security review. No new bot privileges.

## 9. Perf impact

No product runtime impact. Isolated Mac trial: 23/23 PATH-ONLY output vectors equal to a correctly rebound legacy router; seven illustrative measurements showed legacy ~63 ms median and the optimized sparse script ~122–206 ms depending on defensive checks. These are NOT hosted CI performance measurements. Benchmark cold and warm Git object reads, missing-history behavior, fallback job cost, and full CI end-to-end time before adoption; prefer the simpler v1 if the complexity is not justified.

## 10. Open questions

- Approve replacing the existing committed aggregate hashes with immutable-base plus sparse path proofs only after a trusted reviewer/baseline anchor is specified and proven? Without that anchor, retain the old design.
- Is reducing independent-reader Git conflicts enough for phase one, or must proof creation also be automated? Automatically approving new reads is unacceptable without a trusted reviewer gate.
- Can the immutable source tree be available reliably in all required checkouts? How will a future baseline rotation be reviewed without mass rewriting?
- Can the final integrated candidate pass full-mode and hosted checks, especially removal/tampering of old readers and policy scopes, without losing KEL-81 protection?

**Current release/adoption decision: NO-GO.** Isolated experiments are evidence for review, not approval to change production CI.

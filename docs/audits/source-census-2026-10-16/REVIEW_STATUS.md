# KELD architecture review queue — pinned 16 October 2026

**Status: census complete, semantic architecture review NOT complete.** This is a traceable
inventory and ownership *candidate* map for the whole-codebase program
[#824](https://github.com/gyldlab/keld/issues/824). The queue is not permission to merge.

## Snapshot and reconciliation

- Source tree commit: `16eaccbbc7f5d95f8daad6b432baa7d033426b65` (existing
  draft [#838](https://github.com/gyldlab/keld/pull/838), before these audit files were added).
- `manifest.csv` and `summary.json` record **1,114 of 1,114 tracked paths** from
  the snapshot; categories A entrypoints **15**, B authored implementation **177**,
  C authored test/support **361**, D tools/CI/config **92**, E authored docs/spec
  **115**, F explicit exclusion candidates **354**.
- Compared to the pinned 15 October manifest (1,014 paths):
  **100 new paths**, **17 changed blobs**, **997 identical paths**, **0 removed paths**.
  There are no removed paths to map to renamed paths for this exact pair of manifests;
  subsequent snapshots MUST check for renames/deletions before carrying decisions.
- The newly created audit outputs are *not* self-included in the pinned 1,114-path
  source snapshot. Re-pin/reconcile on every next stacked head, including the
  final integration tree, rather than silently declaring the later tree covered.

## Review-state ledger (not percentage-of-refactor)

| Evidence state | Paths | Meaning |
|---|---:|---|
| `pending_file_semantic_review` | **756** | Authored path is inventoried; no completed per-file architecture judgment. |
| `local_impl_tests_pass_independent_semantic_review_pending` | **4** | The hello/static-auth slice has local regression evidence under draft #838, but independent review and hosted integration are NOT complete. |
| `classified_exclusion_provenance_signoff_pending` | **354** | Class F has an explicit exclusion reason, but reviewers still need to verify provenance and confirm it is valid to exclude. |
| **All paths accounted, none certified complete** | **1,114** | A census is not an architectural signoff. |

`review-ledger.csv` has one row per pinned Git path, including Git blob SHA,
census class and reason, proposed component and owner, previous-tree blob/delta,
triage *hint* (not severity), review state, disposition, reviewer, and evidence.
Every `owner_verified` field deliberately starts as `no`: filename-based
routing cannot establish ownership, coupling, threat models, or correctness.
`component-queue.csv` aggregates the provisional ownership groups
(**304 candidates**) solely to order future human/agent component review.
A component group can contain many files and need not become a new folder.

## Review workflow and gates

1. Review related files in component groups. Confirm the real owner, data/control
   dependencies, public API, cross-platform conditional compilation, security
   authority, failure ordering, lifetimes and negative-contract tests. Record the
   reviewer, exact blob SHA, scoped evidence, and either a justified no-change
   decision or a concrete owned refactor finding.
2. When a path changes, invalidate any earlier decision unless the reviewer
   proves the new blob equivalent. Add/remove/rename reconciliation must use
   both path identity and blob identity, never copy a prior `pass` by filename.
3. Class F requires provenance and exclusion justification (fixtures, generated
   artifacts, locks, images, etc.); do not use exclusion to hide authored tests
   or policy. Existing classification alone is not signoff.
4. For scoped code changes, preserve contracts and fully qualified test IDs
   where required, run native macOS/Windows/Linux evidence, warning-denied
   Clippy, format, import guard, source-census router inputs, security tests
   and exact-head hosted required CI. Seek independent authority/security
   review on material boundaries.
5. Do not auto-close #824 until every applicable path has a reviewed
   disposition, remaining findings are fixed or owner-approved exceptions,
   conflicting stack heads are reconciled and integrated tests are green.
   **No merges, retargets or auto-merges without explicit owner authorization.**

## Current material stack blockers (not silently resolved)

- Existing [#833](https://github.com/gyldlab/keld/pull/833) now contains the
  fail-safe Guardian shutdown security fix at `58d649df6f4599f66c5580b8eaef1f5a7e75ea29`.
  Its exact-head CI had **22/22 passing jobs** on the last verified check,
  but independent permission/authority review is still required.
- Existing [#834](https://github.com/gyldlab/keld/pull/834) was based on
  `7c50cb6d...`, the *older* #833 head. Its Ubuntu `keld-core`
  app-session test-scope compilation failed with 53 errors; fix in #834
  and reconcile with new #833 head without weakening tests.
- New draft [#838](https://github.com/gyldlab/keld/pull/838) is based on #834 at
  `e0c85764...`; it inherits that pre-existing Linux failure until the
  parent is corrected. The parser extraction does not fix it. Reconcile its
  ancestry only after #834's branch owner has made the parent correction.
- Independent component review beyond `hello.rs` is pending. High read-set
  candidates include `app_session.rs`, `windows_job.rs`, the runtime,
  IPC/bootstrap, updater, and WebView2 modules; **file length alone does not
  establish an architecture defect**.

## Repeatability

The source census is deterministic on the selected Git object:

```sh
python3 -B tools/source_census.py \
  --repo . \
  --commit 16eaccbbc7f5d95f8daad6b432baa7d033426b65 \
  --output docs/audits/source-census-2026-10-16 \
  --date 2026-10-16
python3 -B tools/test_source_census.py
```

Do **not** regenerate into this directory after reviewers fill in decisions
without first preserving and reconciling `review-ledger.csv`: the source
census recreates its own `manifest.csv`, `summary.json`, and `README.md`,
but the review decisions are separately maintained. The next snapshot
must update ledger rows on added, removed, renamed, or changed Git objects.

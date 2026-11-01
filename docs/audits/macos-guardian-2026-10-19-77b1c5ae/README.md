# macOS Guardian #824 — supplemental exact-blob source review

**Partial, docs-only review at draft stack #952; not merged, not a complete architecture audit, and not a declaration of containment or release readiness.** This supplement preserves the historical [#949 77-file inventory](../macos-guardian-2026-10-18-ea728589/README.md) without editing its three files. The two reviews describe different revisions.

| Role | Exact commit |
| --- | --- |
| Historical 77-file inventory, #949 | `899e287ea487f38ec1cd347f7c9db8f1927348ef` |
| Current reviewed production-tree basis, #952 | `77b1c5aef9202a6cc0d84195fc1f4c32e1eb1cba` |
| Source-review date | 2026-10-19 |

The historical 77 paths are retained in `ledger_77.csv`, including their original #949 blobs, comparison to its then-pinned `main`, and eight **narrow security-PR** `verified-scoped` records. New columns give each path's exact #952 blob and **separate** source-review state; narrow fix verification is **not** silently promoted to a completed file review. `review_evidence.csv` contains independently inspectable responsibility, actual production callers, security invariant, explicit native selector/helper consumer, source location, custody/negative oracle, and reasoned disposition for each of the seven *freshly* reviewed production files and five *reused* exact-blob test audits. Native tests were **not rerun** by this docs-only review; selectors name existing source code, not a new passing run.

## Scope reconciliation: 77 + 1, never silently 77 == 78

The #952 Git tree has **78** tracked Guardian-scope paths: 18 production files and 60 test files. The historical #949 set has 77 paths: 18 production and 59 test files. The extra #952 path, `crates/keld-runtime/src/macos_guardian/authenticated_runner/tests.rs`, is recorded as **pending** in `scope_delta.csv` and in `summary.json`; it is not silently misrepresented as a historical #949 row. Three existing paths changed blobs relative to #949: `authenticated_runner.rs`, `process_group.rs` and `tests/process_group_failsafe.rs`. Their older narrow verification cannot authenticate their changed #952 bytes. The new reviewer did **not** claim those changed files or the new file as fully reviewed.

| Review classification of historical 77 paths | Files |
| --- | ---: |
| Fresh full-file source review at identical #949/#952 blob (seven production files) | 7 |
| Reused prior exact-blob full-file test review (five files; not rescanned) | 5 |
| Historical scoped security verification only, unchanged blob, **not** full-file signoff | 4 |
| Changed blob at #952; needs fresh review | 3 |
| Pending source-level review, no current full-file signoff | 58 |
| **Historical paths** | **77** |
| Additional #952-only file, pending | 1 |

The 12 reviewed rows carry a **source-review disposition**, not an execution/CI acceptance result. Existing native test selectors distinguish direct process/pipe oracles from synthetic reader/state or mock response fixtures. The seven production responsibilities and evidence are in `review_evidence.csv`.

## Fresh architectural findings and boundaries

- **KGR1 host retention and pre-exec permission are separate gates.** `bootstrap.rs` validates the authenticated one-use KGR1 record and the Guardian's *real direct OS child* before `HostGuardian` custody and host-retained `A` acknowledgment. `registration_ack.rs` accepts only this exact one-byte acknowledgment within its own native monotonic deadline. `spawn_admission.rs`'s fork-safe child-side pipe gate denies exec on missing/invalid registration or Guardian death. The static review found no source-proven widening of numeric PID signal authority in these newly reviewed modules.
- **KGC1 wire/state and cancellation are separate.** `generation_wire.rs` owns fixed 404-byte framing, payload bound/padding, per-record clock, and the caller's *first* bootstrap deadline. `host_control.rs` serializes requests and validates exact response transition/attempt/fields. The **successor** `PREPARE` response is deliberately a native wait without the ordinary five-second reply deadline; only a complete framed response, attributed shutdown cancellation, or host-liveness failure can end that wait. It is **not** evidence of a universal bounded five-second shutdown. `recovery_state.rs` denies a successor until the matching earlier generation is retired, CLEARED and explicitly released. `error.rs` aggregates failures, but when multiple errors occur it creates a new lifecycle error containing their rendered text; code should not infer original structured error variants survive this aggregation.
- **Native evidence vs source coverage.** Named regression tests exist for bounded KGR1/KGC1 first records, the late buffered KGR1 `A` refusal, pre-exec denied registration without Bun execution, whole-group native revocation, and successor denial. This review did not execute them, mutate faults, or establish signed-app/GUI acceptance.
- **Open external custody contract** is unchanged: the original process group cannot contain a `setsid`/detached descendant; simultaneous host and Guardian SIGKILL requires an independent OS custodian; a verified PID/PGID birth is not a kernel-pinned process-group handle. A clean scoped code review is not a containment guarantee.
- **Known fixture evidence limits** (reused review, not repeated): `tests/guarded_primary_shutdown.rs` has one explicitly `#[ignore]` staged-host Q/EOF test. The two active selectors prove narrower synthetic/real-child scenarios; the ignored scenario cannot be counted as automatically run. `tests.rs` registers modules but tests nothing itself. `tests/fatal_cleanup_custody.rs` and `tests/host_guardian_failed_custody.rs` deliberately use altered kernel births to prove **retention/refusal**, not successful cleanup of a forged identity.
- **Other-owned integration paths:** five `tests/macos_host_death_guardian*` files are explicitly reserved for the parallel prime review. Their historical ledger entries are **pending** here; this batch claims neither those files nor the public-FS root-cause track.

## Evidence, hygiene, and handoff

`ledger_77.csv` and `scope_delta.csv` are generated from exact `git ls-tree` blobs; `review_evidence.csv` anchors every reviewed row to the same object. `summary.json` lists counts and byte-identity checks. Run the local consistency verification described by the published source-count checker only on this branch; use `python3 docs/audits/verify.py` and `python3 docs/audits/test_verify.py` for the public audit policy. No source, test, parent snapshot, CI router, generated docs, or baseline branch changed. All pull requests remain **draft**; **do not merge**.

**Remaining:** source-review the other 65 historical rows to full file level and the new #952 file (or reconcile other workers' independently evidenced reviews by exact blob); separately validate live OS integrations/host process failure. Do not interpret `reviewed` in this manifest as complete #824 closure.

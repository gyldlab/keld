# KELD #824 — ten additional Guardian exact-blob source reviews

**Partial historical evidence only. No OS-custody certification, product release acceptance, or merge permission.**

This docs-only child is based on **draft #967** at `c89db4a37de966578f16405c45075c79ffeee897`. It retains the immutable historical 77-path inventory from **draft #949** `899e287ea487f38ec1cd347f7c9db8f1927348ef`, the conservative source ledger from **#956/#960**, and #967's offline structural verifier. All ten new records refer to the previously pinned **#952 source tip** `77b1c5aef9202a6cc0d84195fc1f4c32e1eb1cba`, not to HEAD, later stacked patches, or main. No existing audits, production code, native tests, other branches, or CI workflows are modified.

## Evidence advanced

`review_append.csv` contains exactly **ten distinct source-level records** with each file's original #949 and #952 Git blob, responsibility and actual caller, security invariant, native selector or consumer, fixture/negative-oracle evidence, finding, disposition and source lines. Seven unchanged historical files receive a narrowly **limited-source-reviewed** status in the new derived `ledger_77.csv`; three files changed between #949 and #952 have a limited source-change overlay but explicitly retain **changed-since-949-pending** in the 77-file historical accounting. These changes must not be treated as globally merged or fully accepted.

The unchanged source files are `macos_guardian.rs`, `supervised_ack.rs`, `tests/rejected_registration_deadline.rs`, `tests/pre_registration_host_death.rs`, `tests/guardian_crash_pre_registration.rs`, `tests/terminal_liveness.rs`, and `tests/worker_outcome.rs`. The changed overlays are `authenticated_runner.rs`, `process_group.rs`, and `tests/process_group_failsafe.rs`. Their source-level analysis reuses previously completed exact-blob review evidence and verifies pinned Git identity; it does not repeat full native suites or treat generated descriptions as semantic proof.

The ten reviews establish source-specific *test and architecture* candidate areas rather than claiming demonstrated new shipping defects:

- `rejected_registration_deadline.rs:272-299`: its bounded watchdog calls unconditional `owner.join()` after timeout rescue, so a failed native rescue could leave the test hung. A separate genuine-owned-Child, process-isolated native negative control is needed before an implementation claim.
- `pre_registration_host_death.rs`: accepted event reading on the pinned #952 source could extend the deadline. This has a separate failure-first native test and correction on **draft #971**; #971 is **not** inherited by this docs-only ledger.
- `authenticated_runner.rs:109-137`: the `reject_spawn` refusal branch still calls unbounded `Child.wait()` after `terminate_attested_group` may fail. #952 bounds the separately executed `run_live_group` refused-group cut, **not** this path. A real original-Child failure-first native oracle is still required; no unconditional OS hang claim is made.
- `process_group.rs:493-545`: separate rejected-registration reap calls `Child.wait()` even after `Child.kill()` may fail at :507 or :528. This too requires a real owned Child plus controlled native kill-denial oracle before a bounded shipping correction; ownership on terminal error must remain explicit.
- The #950/#952 source overlays cover a native post-reap deadline and an original direct `Child` cleanup refusal respectively. Universal kernel-pinned group authority, detached descendants, and simultaneous Host+Guardian death are **not** established. The three changed historical file rows are deliberately not upgraded to fully reviewed by their overlay records.

This reduces the historical **pending-source-review row count** from **49 to 42**, with seven files now documented as *limited* reviews, not certified fully complete. Three changed historical rows still require source-version reconciliation, and the extra #952-only `authenticated_runner/tests.rs` remains pending. These counts apply only to this particular conservative historical snapshot and must not be added to parallel draft #955's broad 77-row ledger without exact-blob reconciliation.

## Verification and remaining limitations

Run from this child branch's repository root:

```sh
python3 -B docs/audits/macos-guardian-2026-10-19-a2952a0f-followon/verify_followon.py
python3 -B docs/audits/macos-guardian-2026-10-19-c89db4a3-tenfile/verify_tenfile.py
python3 -B docs/audits/macos-guardian-2026-10-19-c89db4a3-tenfile/test_verify_tenfile.py
python3 -B docs/audits/verify.py
python3 -B tools/ci_inputs.py --check
just hygiene
git diff --check
```

The verifier is offline, uses Python's standard library, requires the pinned Git objects locally, asserts the exact 77 one-to-one inherited entries, all ten exact blobs and cited source-line ranges (including named critical callsite anchors), supported status transitions, record joins, counts, and no false product/merge claims. The test module runs **29 checks (one positive plus 28 negative mutations)**, including deleting native-EPERM, attested group cleanup, KGR1 `reject_spawn`, forged KGC1 and host SIGKILL source citations. Citation anchors are a structural check only: they cannot certify that source-level prose, conclusions, or native behaviors are semantically correct. **Structural verification is not independent semantic source review.** These new scripts were run directly on the local Mac; the existing hosted CI `audit-docs-test` recipe does **not** invoke these private follow-on scripts. Accordingly, green hosted CI by itself does not establish the ten-file structural verifier passed. No native tests have been rerun solely for these documentation changes. The end-to-end #824 architecture/security program remains unfinished and every related PR remains **DRAFT / UNMERGED**.

# KELD #824 — seven more historically pinned Guardian source reviews

**Conservative, source-reviewed-LIMITED documentation. It does not change any production behavior, re-run native tests, certify process custody, approve a merge, or complete the architecture review.**

This isolated docs-only child builds directly on **draft #984** at exact head `7cf35e99dd02f4aa58158eb0b40f9a4ea82e0224`. Source receipts remain pinned to historical **#949** `899e287ea487f38ec1cd347f7c9db8f1927348ef` and original **#952** `77b1c5aef9202a6cc0d84195fc1f4c32e1eb1cba`, not to an arbitrarily later branch. All seven reviewed paths have the **same exact Git blob** in both snapshots and were previously `pending-source-review` in #984.

## Exact reviewed scope

Each `review_append.csv` record identifies its immutable two-commit source blob, actual callers and responsibility, security invariant, named unit/native selectors, fixture or independent negative oracle, concrete source lines, finding and residual limitations. The seven files are all under `crates/keld-runtime/src/macos_guardian/tests/`:

- `control_acknowledgement.rs` — actual Bun KQA + Unix control EOF attribution in both orders, and genuine delayed PREPARED reply
- `generation_control_wire.rs` — exact KGC1 bytes, split-reader and expired final-byte/first-record clock; *synthetic Read implementations*, not a native socket guarantee
- `generic_pre_registration_crash.rs` — real separate Guardian, original birth-attested crash, preexec kernel Bun-image witness
- `generic_pre_registration_crash/cleanup_authority.rs` — separate real `/bin/cat` group sentinel survives wrong original Guardian birth
- `initial_generation_adversaries.rs` — authenticated zero/partial/10ms drip first KGC1 and expired queued PREPARE; real Guardian role and separate in-process worker
- `registration_record_adversaries.rs` — real one-use private-link bad magic, wrong PID and missing bytes; typed native Guardian cleanup
- `supervised_bootstrap.rs` — registration authority stripping, revocation, live native STOP/CONT, exact Q/KQA attribution, status-zero terminal reporting

The derived `ledger_77.csv` preserves every inherited #984 field and all **77** historical paths. Only these seven previously pending files become **source-reviewed-limited-batch7**, reducing the historical `pending-source-review` count **26 → 19**. The three changed-since-#949 source paths (`authenticated_runner.rs`, `process_group.rs`, `tests/process_group_failsafe.rs`) still require their old/current version reconciliation. One additional #952-only `authenticated_runner/tests.rs` remains outside the original 77-file inventory and pending. Existing #949/#955/#956/#960/#967/#976/#981/#984 evidence is not rewritten or indiscriminately promoted.

## Source-level findings and limits

The review does not establish a new exploitable product defect. Three native **test-only** failure-mode gaps remain candidates for separately falsifiable engineering, plus one earlier candidate that source verification withdrew (first bullet):

- **Withdrawn, not a gap:** `generic_pre_registration_crash.rs:124,147-150`. `killed_at` is taken at :124, before the original-birth re-check and the attested SIGKILL, and right after the dual-ESRCH loop (:135-146) the test asserts `killed_at.elapsed() < 3s` at :147-150. A leader or group absence first seen more than 3 s after the kill therefore fails the test, so a late absence cannot be called timely. This post-loop bound is stricter than the in-loop `deadline` taken at :134. An earlier draft of this ledger claimed the opposite; it was wrong and no falsifier task is needed for it.
- `initial_generation_adversaries.rs:123-126` unconditionally calls `Child.wait()` after a bounded first-KGC1 refusal and close of the owned liveness writer. A non-waking child on a deliberately broken code path could hang the test. A genuine independently owned Child, retained birth and bounded completion receipt would falsify the test-only watchdog gap.
- `supervised_bootstrap.rs:17-33` invokes external `/bin/ps` in a bounded-loop observer, but one `Command::output()` itself has no wall-clock fence. The source also uses unconditional `worker.join()` after some bounded result waits. This is a limited test-hang candidate, not proven failure of the production supervisor. Existing SIGCONT tests refer to the direct test-owned child, not a kernel-pinned process-generation handle.
- `control_acknowledgement.rs:223-231` closes its actual Unix peer before joining a control reader, but the join itself is not a bounded native OS rescue. The reader contract and its Darwin shutdown wake-up reliability remain separate from the source-level Q/EOF tests.

Some tests use synthetic readers or in-process Guardians; a passing selector is not equivalent to native signed/installed macOS GUI acceptance. None proves simultaneous Host+Guardian SIGKILL containment, detached `setsid` descendants, absolute kernel-pinned PID/PGID authority, or a full cross-platform integrated stack. Existing public-FS Host exit/IPC reader intermittent failures are unresolved independently of this docs-only branch.

## Structural verification and provenance

Run from the child's repository root with original pinned Git objects available:

```sh
python3 -B docs/audits/macos-guardian-2026-10-19-2c488b9a-next8/verify_next8.py
python3 -B docs/audits/macos-guardian-2026-10-19-54ac81e5-sevenfile/verify_seven.py
python3 -B docs/audits/macos-guardian-2026-10-19-54ac81e5-sevenfile/test_verify_seven.py
python3 -B docs/audits/verify.py
python3 -B docs/audits/test_verify.py
python3 -B tools/ci_inputs.py --check
just hygiene
git diff --check
```

The new standard-library verifier checks unchanged parent bytes, 77 one-to-one historical rows, 14 exact Git blob identities, conservative statuses and counts, manually named source-line anchors, receipt joins and explicit **non-completion** flags. Its suite has **29 tests: one positive and 28 independently failing mutations** that invalidate source citations, old-parent evidence, scoped status promotions, and unsupported merge/security assertions. This verifies structure, not truth of every semantic sentence, lifetime, or operating-system race; the finite source-line anchors are not exhaustive.

The new private follow-on scripts are invoked **locally only** and are not called by the standard hosted public audit registry test. Hosted green alone therefore does not certify their execution. No Guardian native suite was rerun merely to publish this documentation. Independent **exact-SHA adversarial read-only review** remains required. All related PRs stay **OPEN / DRAFT / UNMERGED**.

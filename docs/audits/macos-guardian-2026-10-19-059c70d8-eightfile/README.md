# KELD #824 — eight further pinned Guardian fixture source reviews

**Partial source-level evidence only. No production security acceptance, native test execution, kernel-backed process custody, or merge authorization.**

This docs-only branch is a direct child of **draft #976** at `059c70d88c1841c58bc85907bdf9424fc29adb99`. It preserves that parent's historical 77-path ledger and review records byte-for-byte. Its source references use the original **#949** snapshot `899e287ea487f38ec1cd347f7c9db8f1927348ef` and **#952** tree `77b1c5aef9202a6cc0d84195fc1f4c32e1eb1cba`; these eight paths have identical blobs at both revisions.

## Exact scope

`review_append.csv` contains eight scoped reviews of macOS Guardian native fixture and custody code. Each has its exact Git blob, inbound/outbound roles, security invariant, named native test selector or consumer, independent native witness or negative oracle, source ranges, review result, and remaining limitations. The eight paths are `retained_ack.rs`, `retained_ack/dual_death.rs`, `retained_ack/escaped_descendant.rs`, `retained_ack/escaped_descendant/cleanup_tests.rs`, `retained_ack/observers.rs`, `guardian_crash_pre_registration/cleanup_authority.rs`, `pre_registration_host_death/cleanup_authority.rs`, and `runtime_race_validation/ownership_races.rs`, all under `crates/keld-runtime/src/macos_guardian/tests/`.

The reconciled `ledger_77.csv` retains every parent column and inventory identity. Exactly eight former `pending-source-review` records become **source-reviewed-limited-batch8**. The historical pending row count decreases **42 → 34**. Three changed-since-#949 files still need version-specific semantic reconciliation. The additional #952-only `authenticated_runner/tests.rs` remains outside the 77-file snapshot and pending. These are ledger-accounting facts, not whole-feature acceptance.

## Specific source findings and unfinished guarantees

The original `retained_ack.rs:48-76` test-only held-ACK gate accepted an `R` byte after its original clock. `retained_ack/observers.rs:35-74` similarly accepted a late LF-framed native event. Both have separate failure-first real Unix-socket regressions and corrections in **draft #979** (stacked above #953); #979 is **not inherited** by this documentation child. Other test-only time-classification candidates in `observers.rs` remain open: `read_marker:95-111` can accept a newly available marker after the caller's original deadline, and `group_gone:77-93` can classify group absence observed after a deadline as timely. Independently owned, native late-event and post-deadline cleanup witnesses are needed before claiming corrections.

The native dual-death test is **negative containment evidence**: killing both Host and Guardian after Bun exec leaves the independently enrolled Bun group alive before test-only rescue. The real detached descendant also survives ordinary original-group reaping, proving that `setsid` escape is not prevented by basic Guardian group cleanup. These passing tests must not be reported as product containment success. Other source reviews demonstrate narrower original-birth cleanup safeguards and genuine native outsider controls, not a kernel-pinned PID/PGID guarantee. The `ownership_races` fixture has a potentially delayed panic-time join, but the observed source uses an existing finite 5-second gate; no unbounded deadlock is proven by that evidence alone.

The user-requested architecture still needs a real external custody model for simultaneous Host/Guardian death and detached descendants; original PID/group reuse and full installed/signed macOS GUI acceptance remain unresolved. No new production code or tests are changed on this docs branch. All related PRs remain **draft and unmerged**.

## Reproducible structural checks

Run from this branch's repository root with pinned Git objects available locally:

```sh
python3 -B docs/audits/macos-guardian-2026-10-19-c89db4a3-tenfile/verify_tenfile.py
python3 -B docs/audits/macos-guardian-2026-10-19-059c70d8-eightfile/verify_eightfile.py
python3 -B docs/audits/macos-guardian-2026-10-19-059c70d8-eightfile/test_verify_eightfile.py
python3 -B docs/audits/verify.py
python3 -B tools/ci_inputs.py --check
just hygiene
git diff --check
```

The new verifier checks immutable inherited parent bytes, the 77 one-to-one historical rows, eight exact unchanged Git blobs, source-line citations anchored at selected native custody/clock evidence, status transitions, joined dispositions, and honest scope counts. The negative suite mutates these structural claims to prove specific refusals. It cannot determine whether every English source-review claim is true or whether an OS race is safe. These follow-on scripts run **locally when invoked**; the standard hosted public-audit registry job does **not** execute them. Hosted CI green is not proof of independent semantic review. Independent exact-SHA source review remains a separate required step. **DRAFT / DO NOT MERGE.**

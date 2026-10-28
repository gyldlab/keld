# KELD #824 — eight more exact-blob Guardian native fixture source reviews

**Historical, source-reviewed-LIMITED evidence only. No shipping code change, native test rerun, process/OS-custody certification, product security acceptance, independent exact-head review, or merge permission.**

This second eight-file source batch is stacked **directly on DRAFT #981** at immutable Git commit `ca0e62cc0af8c972e83b1548d88f79c2e1af98be`. The original historical snapshot is **#949** `899e287ea487f38ec1cd347f7c9db8f1927348ef`, and the source tip is **#952** `77b1c5aef9202a6cc0d84195fc1f4c32e1eb1cba`. Each of the eight new paths is unchanged between these historical revisions. All six #981 parent source documents and all its original 77 ledger columns remain immutable.

## Records and scope

`review_append.csv` contains one manually source-reviewed record per path with both exact Git blob identities, responsibility/callers, security invariant, native selectors and independent failure oracle, actual source-line ranges, source finding, and explicitly limited disposition. The new files, all rooted at `crates/keld-runtime/src/macos_guardian/tests/`, are:

- `generation_host.rs`
- `generation_host/clear_authority/cleanup_authority.rs`
- `generation_host/invalid_spawn_after_revoke.rs`
- `bootstrap_shutdown_race.rs`
- `bootstrap_shutdown_race/denial_order.rs`
- `bootstrap_shutdown_race/factory_failure.rs`
- `bootstrap_shutdown_race/post_s_spawn.rs`
- `active_generation_shutdown.rs`

`ledger_77.csv` copies every inherited #981 historical path/field without changing it. Only these eight previously `pending-source-review` rows receive a distinct `source-reviewed-limited-batch8b` receipt. Historical pending falls **34 → 26**. The three paths changed between #949 and #952 (**authenticated_runner.rs**, **process_group.rs**, **tests/process_group_failsafe.rs**) retain `changed-since-949-pending` status. The additional #952-only `authenticated_runner/tests.rs` is not in the 77-file historical inventory and remains pending. The earlier #976/#981 reviews are not repeated or promoted. No inference from #955's broader ledger is applied.

**Important evidence distinction:** `generation_host.rs` and `invalid_spawn_after_revoke.rs` use an *in-process* host KGC1 worker with actual native direct Child groups and a synthetic Guardian parent; they do not independently exercise a separate Guardian OS process. The bootstrap-race and active-generation native integration tests do spawn separate authenticated Guardian roles. The latter's same-process-group descendant witness is not a detached session/custodian test.

## Source-proven open cuts and validation limits

The records name bounded, independently falsifiable cases without claiming that a new shipping defect has been reproduced:

- `bootstrap_shutdown_race.rs:143-188`: the test-only `receive_held_milestone` accepts an already-connected peer or LF completing after the original test observation deadline. Its successful accept/LF paths are not postchecked. Native UnixStream with a held late LF and an original clock is the smallest independent failure-first oracle.
- `active_generation_shutdown.rs:100-116,153-165`: a valid Bun/descendant file marker can return after the file-observation deadline; genuine group-and-descendant ESRCH can finish after a separate group-absence observation deadline. These are *test-oracle* clocks, not a claim about the host's shipping S/KSA deadline.
- `generation_host/invalid_spawn_after_revoke.rs:84-115`: a blocked generation worker can strand an unconditional `join()` with the Guardian test peer retained open if the expected typed denial regresses. SIG0, getpgid and `try_wait` are point-in-time, not fresh native echo. Falsify with a separate retained real Child, bounded worker completion receipt, and fresh cat nonce; rescue using original captured birth.
- `generation_host/clear_authority/cleanup_authority.rs:106-114`: a native second member's unconditional wait can delay a leader-only-kill mutant by the original `/bin/sleep 30` lifetime. A bounded `try_wait`, unique member and original-birth rescue would make the negative fail on time.

An adjacent **production-source candidate outside this batch** is `crates/keld-runtime/src/macos_guardian/host_liveness.rs:284-310`: on pre-generation S/KSA, `HostLiveness::Dead` at line 287 can return success without postchecking the original 5-second host-EOF deadline. A real OS pipe with a held postread EOF/deadline fence is still required; no production fix or native reproduction is implied by this documentation.

The bootstrap tests generally check *Bun JavaScript marker absence*, not an independent kernel process image showing that Bun never briefly executed. The post-S registration test observes the pre-REGISTERED test fence, not a literal independent wire-tap capturing absent REGISTERED. No original-owner test proves kernel-pinned group handles, detached `setsid` descendants, simultaneous Host+Guardian SIGKILL, or signed/installed complete GUI acceptance.

## Exact local verification

Run from this branch's repository root, with all pinned Git history available:

```sh
python3 -B docs/audits/macos-guardian-2026-10-19-059c70d8-eightfile/verify_eightfile.py
python3 -B docs/audits/macos-guardian-2026-10-19-2c488b9a-next8/verify_next8.py
python3 -B docs/audits/macos-guardian-2026-10-19-2c488b9a-next8/test_verify_next8.py
python3 -B docs/audits/verify.py
python3 -B docs/audits/test_verify.py
python3 -B tools/ci_inputs.py --check
just audit-docs-test
just fmt-check
just hygiene
git diff --check
```

The new validator checks immutable parent documents, exact source blobs, ordered finite line citations with manually selected native/deadline anchors, inherited 77-row identity, conservative status transitions, summary provenance and explicit pending security claims. Negative mutations are designed to make **each named missing native line** fail structurally without relying on another row/summary failure. These checks are *not* proof that every cited source sentence is correct, that every possible citation shrink is caught, or that any OS race was executed. The privately scoped follow-on Python verifiers are **local-only** when called directly; the hosted public `audit-docs-test` recipe invokes only the public audit registry test, not these private scripts. A hosted-green check therefore cannot certify their execution.

**No boundary change. DRAFT. Do not merge. Independent exact-head read-only semantic review is still required.**

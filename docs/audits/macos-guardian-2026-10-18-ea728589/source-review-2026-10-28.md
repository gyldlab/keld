# Guardian source reconciliation — 28 October 2026

## Pins, scope and non-acceptance

This is a new dated **static-source** supplement and erratum, not a rerun of the historical audit.
All original CSV/JSON bytes and their historical hashes remain unchanged. In the historical
`source-review-summary.json`, `current_stacked_tip` / `current_tip_paths` mean the snapshot
at the original report date, **not** live #953. Historical `reviewed-no-change` is not renewed
security or native acceptance; the explicit qualifications below take precedence when reading it.

| Immutable source | Four-root path census |
|---|---:|
| Original #949 `ea728589d0143df134c0f21fe5cf335240018d6a` | 77 |
| Historical #953 `46b91848db10f81156cebe0c6c34dec90082eaba` | 79 |
| #953 observed 2026-10-28 `8dd43926297479851c555596aafa012ee992c7e5` | 86 |
| #955 observed 2026-10-28 `e0ababf1f07217a44199e7e3c0e69ceba6ea929e` | 86 |

Scope is exactly `crates/keld-runtime/src/macos_guardian.rs`, its sibling
`macos_guardian/` subtree, `crates/keld-runtime/tests/macos_host_death_guardian.rs`
and its sibling `macos_host_death_guardian/` subtree. The last two trees are identical
within this scope: 18 production and 68 test paths. Relative to historical #953:
**50 unchanged, 29 modified, 7 added, 0 removed** (8 modified production; 21 modified
and 7 added tests). Relative to #949: 48 unchanged, 29 modified, 9 added, 0 removed.
The historical 79-path / 70-unchanged / 7-modified / 2-added comparison remains valid
for its original baseline; it is not today's comparison.

[source-delta-2026-10-28.csv](source-delta-2026-10-28.csv) accounts for all 86 paths,
full immutable blob identities, both baseline deltas, and qualified current dispositions.
Every one of the 36 modified/added file bodies received bounded static inspection; ranges
and findings below are not native test receipts. Unchanged blobs retain historical limitations,
not automatic present-day acceptance. No new production defect was established by this pass.
No whole-repository, current-main, merged-stack, installed-product, kernel-custody or cross-OS
acceptance follows. No product/test source, deadline, protocol or authority changed here.

## Errata and outstanding acceptance

1. **Final-newline deadline oracle (unresolved):** `tests/bootstrap_shutdown_race.rs`
   `receive_held_milestone` accepts the newline before its final deadline test (current
   lines 176 versus 187–190). The ordering exists in historical source too. A terminator
   delivered after expiry can be accepted when its prefix has already accumulated.
   `tests/test_support.rs:153–194` has analogous ordering. This specifically overrides the
   old `reviewed-no-change` implication for that timing obligation; it is not evidence of
   product admission failure. Existing #953/Guardian test owners must add a deterministic
   after-deadline terminator control, show RED before the oracle fix and GREEN afterward,
   preserving the original deadline. **No such native or mutation execution was run here.**
   See [requested correction](https://github.com/gyldlab/keld/pull/955#discussion_r4393859635).
2. **Custody remains open:** exact retained Child identity is not a universal bounded
   kill/wait/join guarantee. Failed-kill waits and worker joins, numeric PID/PGID observation
   races, simultaneous host/Guardian death and detached descendants retain #834/#824 owner
   dependencies. No unsafe timeout increase or detachment is proposed as a remedy.
3. **Partial-timeout lower bound is qualified, not a runtime finding:**
   `delivery_assertions.rs:113` observes elapsed time after `delivery_fixture.rs:212–237`
   collects outcomes and joins owners. It proves time to assertion, not necessarily time
   of framing decision. A hypothetical early correctly typed timeout plus delayed cleanup
   is not excluded by this assertion alone. Existing `generation_control_wire.rs:41–106`
   covers split reads and expired final bytes; production `generation_wire.rs:190–227`
   retains the framing clock. Do not claim framing clocks are globally untested or the
   runtime is defective. `terminal_at` also follows revocation and is not a sufficient fix.
   Existing Guardian testing owner must first distinguish total-return from decision-time
   obligation; only for the latter, exercise cancellation-specific early TimedOut with
   deliberately held cleanup and an independently observed decision. This remains a
   proposed falsifier, not an executed mutation. KEL-143's in-tree approval references
   were inspected, but their original Linear receipt was not independently fetched.
4. **Enforcement boundary:** the CSV/Git census and byte-preservation checks for this
   supplement are manual structural evidence, **not canonical CI enforcement**. Existing
   `docs/audits/verify.py` / `test_verify.py` remain unchanged. The request that a future
   source/path mutation fail the canonical census gate is still an integration dependency
   for that verifier's designated writer, not completed by publishing this prose/CSV.
   See [current-source request](https://github.com/gyldlab/keld/pull/955#discussion_r4393819644).

## Production changed-body review

Paths below are under `crates/keld-runtime/src/macos_guardian/`; exact blobs and full-body
read ranges are in the CSV. Falsifiers are pending evidence, not executed checks.

### authenticated_runner.rs
Blob `4528d69455abd04cc27a88f7dd58a3e6d06d6b87`; entire file 1–256. Registration 44–108 retains local kernel witness before permitting exec. Live cleanup 162–200 preserves terminal group errors and limits exact-owned Child reap using original pre-revocation deadline. Helper 232–252 never derives kill authority from reported raw PID. Historical #952 live-path unbounded wait is addressed at source level. Rejection waits 128–150 are not covered by that live-path bound; do not generalize to all cleanup. Native denied-signal/changed-birth/direct-child-reap evidence remains necessary.

### generation_host.rs
Blob `354f25434571b967a106b2685071a9293851c6ac`; entire file 1–661. First record checks before/after read 101–135. `spawned_attempt` 187–204 blocks duplicate SPAWNED from minting fresh signal authority after host fail-safe clears the slot. Registration holds recovery lock through actual REGISTERED 211–308. Post-S refusal first retains attested group and preserves failed cleanup custody 233–264. CLEAR spends authority before fallible absence proof, then drops enrollment/ACKs/advances recovery only after success 336–411. Typed refusal is not emitted on cleanup failure. Falsifiers: duplicate SPAWNED after spent authority, S-before-SPAWNED, held zombie CLEAR, late first complete record.

### guarded_lease.rs
Blob `b7e419583478ca6a1bae7d10905ba8e1725c05d1`; entire file 1–432. Prepared-stage flag 35–45 prevents local post-PREPARED failures being classified as first-PREPARE denial. Reporter-specific EOF marker 122–168; failed registration marks exec withheld 257–266. Revocation 317–425 uses exact witness/Child; failed reap prevents CLEAR; retired-refusal flag only follows successful local retirement. Important review-enforced assumption: factory must install no earlier `pre_exec` hook (71–76); otherwise “unexecuted direct child is entire group” is not established. No runtime hook inspection exists. Falsifiers: local factory failure plus S stays fatal; refused registration plus failed Child retirement cannot set successful-retirement flag; pre-existing hooks require separate consumer audit.

### guarded_primary.rs
Blob `0e31b86f3e7b1960ce3f43e9fbe4ef3b4641a45c`; entire file 1–726. NativeSignalAuthority 66–173 spends one host signal attempt on both success/failure; later calls probe only. Generation mutex serializes fail-current with CLEAR 320–331. Denial lock held through physical S write 553–607. Worker reconciliation 614–666 requires actual ACK, empty registered slot, and exact private typed outcome; post-SPAWNED refusal needs KSA, not KQA. Retained historical limitation: waits after failed kill 478–492/504–505 and unconditional worker join 510/669–678 remain potentially unbounded. Preserve #834/#824 cross-owner custody blocker; not a new main regression.

### host_liveness.rs
Blob `299c644deeaa61f835545e84bdc43b0f62cb715b`; entire file 1–448. Q/S mutual attribution 212–265; failed initial-preparation reconciliation accepts actual S, rejects Q, and does not treat bare EOF as orderly 347–365. Attributed stopped-Q path waits on retained descriptor 406–420. Pre-generation EOF 323–325 returns without creating any generation; this is not proof of acknowledged orderly shutdown despite runner’s broad adjacent comment. Falsifiers: duplicate/conflicting Q/S, bare EOF after failed startup, ACK failure, retained writer after early KSA.

### process_group.rs
Blob `927e10feea299b60c02cb0c100f93b110195d007`; entire file 1–798. Parent/birth validation 231–293. Per-call one-signal decision 313–383. Shared absence owner 438–494 requires dual ESRCH and guards both before observation and after completed observation. Exact EPERM classifier 559–635 preserves denial plus absence reason. Own-parent helper 661–716 performs reap before reconciling ambiguous/denied outcomes. Do not claim every successful signal proves absence: `Decision::Done(Ok)` passes through; host CLEAR supplies independent absence proof. Known numeric observation race explicitly remains 295–306/418–420. Rejected-bootstrap cleanup still has post-kill unbounded waits 735–748/756–769; source-level bound is not universal.

### supervised_lease.rs
Blob `a85f4cd6a11a2146ea5c3ce7e15aa37077a4c22a`; entire file 1–202. Single-use, attempt-one preparation 47–59; local child witness before registration 85–90. Revocation 157–200 retains original resource cleanup error, uses own-child retirement when available, and rejects discovered group without native witness. No newly shown raw-PGID fallback or successor authority. Falsifiers: failed resource revoke, no witness, denied signal with live original Child, persistent descendant after leader reap.

### supervision_runner.rs
Blob `5434707bd47d3b11dca138627ca5d0df14d29691`; entire file 1–507, including clearly test-only legacy runner 133–243. Production startup 314–384 validates liveness before authentication and only reconciles first-PREPARE EOF before genuine PREPARED. Outcome reconciliation 434–492 additionally requires successful local retired-refusal fact plus S attribution, not Q/bare EOF/general failure. Falsifiers: post-PREPARED factory/admission failure with S stays fatal; failed refusal cleanup stays fatal; Q cannot authorize refused-spawn orderly success.


## test-core-review



- `guarded_lease/tests.rs:144–244` requires socket cessation AND a successful nonempty native process-state census with no non-zombie member. `native_activity.rs:10–133` supplies live closed-socket / injected EINVAL / ENOTCONN negative controls and killed-group positive controls; failed or empty census fails closed. This distinguishes execution cessation from whole-group ESRCH. Census itself uses blocking `Command::output` (tests.rs:148–153); this is not a hard end-to-end observer deadline.
- Legacy ACK fixtures (`tests.rs:268–352`) execute test-only late `child_spawned` and test-only birth-validation fallback: governing `guarded_lease.rs` blob `b7e419583478ca6a1bae7d10905ba8e1725c05d1`:275–287,384–397. They do not prove shipping SpawnAdmission transfer. Real `revoke_with_child` calls are separately exercised by `own_child_reap.rs:80–249,379–421`, with actual native EPERM, exact retained Child, post-reap ESRCH and no concurrent helper reap.
- `own_child_reap.rs:251–327` adds stronger joiner effect oracle: eventual SIGTERM status, not only transient `try_wait()==None`, detects a second product SIGKILL. `423–466` independently observes signal-entry monotonic time to constrain reap budget ordering. No mutation run was performed; comments saying a mutant must turn red describe intended sensitivity only.
- `process_group_failsafe.rs:116–263,460–588` distinguishes live group, held zombie group, actual absence, and unrelated error classes; receipt-triggered reaping plus persistently-held negative control prevents a positive-only early-reap inference. `269–377` and `shared_absence.rs:55–174` exercise independently controlled before/after deadline decisions with real kernel dual-ESRCH observations.
- `exiting_member_race.rs:80–106,133–255,291–388` positions real member exit at signal seam and keeps actual group/state observations. The generic no-second-signal helper at 339–350 uses hook count plus momentary liveness and then test SIGKILL; stronger SIGTERM effect control exists separately at 177–213 and own_child_reap 251–327. Do not describe every case as independently effect-sensitive to arbitrary off-seam signal mutants.
- `bootstrap_shutdown_race/post_s_spawn.rs:120–232,252–289` observes genuine flushed S/Q, actual decoded parent/birth-validated SPAWNED, no registration-fence entry, no Bun JS marker, native group absence before fallback, and exact Guardian reap. Without-S and accepted-Quit controls preserve terminal refusal instead of treating any ACK as orderly S. Governing generation_host.rs:219–246 emits the SPAWNED receipt after actual validation and before denied recovery decision. No native execution receipt is established here.
- `guarded_primary_shutdown.rs:186–262` is explicitly ignored staged Host acceptance requiring external fixture receipt; not a routine passing test. It checks retained Guardian live across permit-held and Q-stopped phases before EOF. 264–417 are narrower zero-exit/worker-failure tests using native stand-ins, not packaged Host acceptance.

## Preserved oracle / failure-path qualifications (existing owners)

1. Known `bootstrap_shutdown_race.rs:164–197` newline branch (176) returns before final deadline check (187–190). Preserve as test-observer timing gap, NOT shipping bootstrap admission fault. `test_support.rs:153–194` contains analogous final-newline ordering. No duplicate runtime finding or issue created.
2. `test_support.rs:32–46` takes JoinHandle by value and panics if unfinished at deadline, thereby detaching it rather than proving cancellation/join. Its two callers are covered in test-cleanup-review.md. Outer socket drop / exact Child rescue can enable eventual shutdown but is not a worker-completion receipt.
3. `NativeChildOwner::Drop` at test_support.rs:83–94 and `OwnedLeader::Drop` at own_child_reap.rs:32–38 retain exact Child but kill then unconditional wait, with no independent ineffective-kill bound. Likewise NativeGroup Drop tests.rs:247–257 and native reaper joins must not be advertised as universal bounded failure recovery. Existing #824 custody owners remain responsible; no hanging execution was reproduced and no unsafe timeout/detach fix proposed.
4. `shared_absence.rs:214–223` uses blocking move-receipt read_line; source is not a complete fixture-deadline proof. This is fixture qualification, not product escaped-leader fault. Its product assertion independently requires leader still live in destination, original group ESRCH, refusal and untouched destination (224–249).


## test-delivery-review



- Prepared2 cuts use actual production host provisioning, real Q/KQA bytes, canonical relay delivery, independent no-effect/output/endpoint witnesses, and literal wire-field assertions distinguishing normal Revoke/Clear from EOF cleanup. See `delivery_cut_oracles.rs:119–174`; governing unchanged `private_record_relay.rs:15–126` (blob `b0d676d8dfe24a21ff8e48ff292d508a2469dd3e`) and `delivery_assertions.rs:11–104`. Shared production decoding means this is not an independent codec qualification.
- ACK/EOF ordering has an owned real Bun ACK peer, actual EOF, both orderings, idempotent successful shutdown, denial-without-ACK negative, partial EOF negative, and independently measured zero-byte withholding (`control_acknowledgement.rs:14–256`). It is explicitly not a staged real Guardian proof.
- Recovery tests distinguish initial arm from retired Clear, deny rearming, exact unclaimed-lease revocation and endpoint closure. Registered PID 42 fixture is explicitly inert, not signal authority.
- Terminal-liveness test independently observes descriptor nonblocking flag and terminal-read count, while always releasing fence/writer before join (`terminal_liveness.rs:28–45,123–171`).
- Worker tests retain actual Child and capture retirement before rescue, covering premature successful worker exit, zero Guardian exit with registered group, and externally consumed Child status (`worker_outcome.rs:83–112,172–193,228–281`).
- Integration readiness attests live parent chain and original births; cleanup requires PID/UID/start/PGID equality, including retained same-group member after leader reap. Negative controls require fresh sentinel echo for altered birth, missing authority and outsider member, paired with successful exact-authority cleanup (`readiness.rs:56–102,125–158`; `support.rs:110–234`; `support/tests.rs:68–262`).


## test-cleanup-review



### CLEAR oracle independence

`clear_authority.rs:160–211,266–373` holds the actual unreaped Bun leader through the production budget, rejects every KGC1 byte, requires a typed native absence timeout, and checks retained original birth. This closes the source-level weakness of relying solely on its 150ms negative window (`127–158`). Independent helper negative control (`232–263`) and native reap/success control (`375–481`) complement it. Comments describing 200/250ms mutants are proposed sensitivity claims, not observed mutant receipts.

The Bun fixture retains direct Child custody before birth observation and retains birth before releasing its pre-exec gate (`65–114`). Failure rescue uses that original birth, not descendant-marker PID (`28–44`).

### Single-attempt authority

`fatal_cleanup_custody.rs:18–101,103–218` constructs real test-owned groups with deliberately altered enrollment birth, covers Guardian exit, failed shutdown and worker-failure cleanup, checks retained original enrollment and terminal failure. Its live-now checks do not alone prove absence of an asynchronously delivered signal; the later retry tests have stronger final-status controls.

`retry_authority.rs:102–190` and `delivered_signal.rs:30–151` use a later live member/joiner and final SIGTERM exit status, rather than only a live-now PID probe, to detect forbidden repeat SIGKILL. They cover refused, delivered and denied attempts plus first-attempt positive cleanup. Budget controls are `retry_authority.rs:192–243` and `delivered_signal.rs:212–236`; direct enum assignment in the latter is a unit state/deadline control, not a native transition receipt.

The denial cut invokes a scoped hook to kill the exact retained leader and observe its zombie group (`delivered_signal.rs:74–114`); governing hook custody is thread-local and RAII-cleared in `process_group.rs:139–175`. No external production hook exists in this reviewed source path.

### State persistence across real CLEAR

`failed_clear_authority.rs:80–138` checks the shared generation retains original birth and Delivered authority after typed CLEAR failure, then challenges subsequent fail-safe with a late joiner. Governing production `generation_host.rs:336–411` persists spent authority before fallible absence proof; `guarded_primary.rs:66–173` separates Armed, Delivered and ProbeOnly budgets.

### Duplicate SPAWNED

`duplicate_spawned.rs:45–151` exercises both armed-recovery and denied-recovery routes after deliberately releasing a spent slot; it requires transition rejection, absent new custody and surviving second Child. Governing deduplication is separate `spawned_attempt` state, not just slot occupancy (`generation_host.rs:53–63,187–204`). Its `try_wait` observation is weaker alone than a final status/activity proof; exact transition rejection and empty slot are additional state assertions, not replacements for executed mutation sensitivity.

### Generation-host callback/error attribution

`tests/generation_host.rs:200–377` covers accepted/requested/unattributed preparation interruption and unmarked I/O errors with a real initial child, existing preparer trait fence, terminal outcome, crash ledger and output checks. Release is sent and sender dropped before assertions (`237–250,330–342`), avoiding an assertion holding its own gate closed. These remain source-designed tests; terminal event followed by synchronous `wait_for_outcome` does not establish all callback custody failure paths.

### Crash cleanup authority

`cleanup_authority.rs:97–173` tests no authority, genuine original birth and altered birth. Outsider survival is active `/bin/cat` challenge/response (`29–67`), not PID existence alone. Exact direct Child RAII avoids signaling after reap (`16–27`). Positive cleanup still includes synchronous `wait`; this does not establish ineffective-kill boundedness. Governing fixture Drop is `tests/guardian_crash_pre_registration.rs:62–83`: kills/polls/waits original Guardian, then applies only saved birth authority.

## Existing #824 qualification: bounded assertion is not completed worker custody

`tests/test_support.rs:32–46` takes `JoinHandle` by value and panics at its deadline. Timeout therefore drops/detaches the still-running worker rather than proving cancellation/join completion. Affects `failed_clear_authority.rs:91–98` and `duplicate_spawned.rs:105–111`.

Outer unwind retains and drops the guardian peer socket and original `NativeChildOwner`s, which may permit cooperative worker exit. But `start_generation_host_fixture` captures host socket and cloned state Arcs (`tests/generation_host.rs:43–62`); no timeout-path join or completion acknowledgment verifies their release. No executed leak/hang or runtime defect is claimed. Keep this with existing #824 native/custody owners, not a duplicate issue.

Other synchronous joins/waits remain qualified, notably `clear_authority.rs:344–347,478–480`, `tests/generation_host.rs:177–180,268,359`, and original-child fallback `tests/test_support.rs:83–94`. NativeChildOwner preserves exact Child custody but ignores kill result and synchronously waits; no ineffective-kill experiment was run.

The known bootstrap_shutdown_race newline-before-deadline oracle issue is not reclassified or duplicated here.


## Reproduction and remaining gates

Enumerate the four roots with `git ls-tree -r -z <full commit> -- <roots>` and compare
path sets and full Git blob IDs with the CSV, independently for all four pinned commits.
Compare the original three `source-review-*` CSV/JSON files byte-for-byte against the
pre-correction #955 commit; parse JSON/CSV and reject duplicates, missing paths or blob drift.
A changed recorded blob or inserted path must fail this manual comparison. These operations
verify provenance/census, not source semantics or kernel behavior.

Run the repository's existing `python3 -B docs/audits/verify.py`,
`python3 -B docs/audits/test_verify.py`, `python3 -B tools/ci_inputs.py --check`
and `git diff --check`; results belong in the exact candidate publication receipt.
No new verifier/framework, CI workflow, reader-manifest edit or native test is part of this
correction. Native macOS falsifiers, full local format/Clippy/workspace tests/tooling gates,
exact final-head hosted checks, independent source review and owner acceptance remain
separate requirements. Keep both review requests open until their remaining evidence and
integration dependencies are satisfied. GH #824 remains incomplete and do-not-merge.

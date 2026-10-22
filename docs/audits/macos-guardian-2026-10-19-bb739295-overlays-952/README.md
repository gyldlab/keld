# KELD #824 — four LIMITED #952 Guardian source-version overlays

**Pinned source review only.** This isolated audit child follows **DRAFT #992**
at exact commit \`bb739295d4dda0ef83acc8c38b521594a49a29c5\`.
It adds no production Rust, native fixture, permission, process-signal, protocol
or test-behavior change.

The source was inspected at two immutable Git revisions:

- Historical #949: \`899e287ea487f38ec1cd347f7c9db8f1927348ef\`
- Historical #952: \`77b1c5aef9202a6cc0d84195fc1f4c32e1eb1cba\`

The parent #992 had three \`changed-since-949-pending\` historical entries
and one separately identified #952-only module. This report resolves **only
the absence of a LIMITED source-version review of those four #952 blobs**.
It does not promote the original historical rows, certify the latest KELD
code, or complete the macOS Guardian security program.

## Exact source and responsibility

All four complete #952 files were read, together with their actual direct
callers at #952. \`overlay_evidence.json\` records the source diff sizes, full
line partition, 163 independently pinned decision-bearing line anchors,
10 caller anchors, named native oracle selectors, owner/boundary and open gaps.

| #952 source relative to \`macos_guardian/\` | #949 → #952 Git blob (prefix) | Source scope and direct owner |
| --- | --- | --- |
| \`authenticated_runner.rs\` | \`f00e4260\` → \`8a3a1368\` | 233 lines. Legacy single-group Guardian takes authenticated registration and host EOF, checks local original-child birth, revokes capabilities and owns one real \`Child\`. Direct public caller: \`macos_guardian.rs:167-185\`. New native test calls the private \`run_live_group\`. |
| \`process_group.rs\` | \`ab6e23fb\` → \`213047a5\` | 570 lines. Shared Darwin kernel group inspection, PID/start-time identity, attested signal and post-reap EPERM reconciliation. Production callers include \`guarded_lease.rs:307-324\`, \`generation_host.rs:347-365\`, legacy runner rejection and \`bootstrap.rs:326-335\`. |
| \`tests/process_group_failsafe.rs\` | \`5a1002b0\` → \`a1d3bc87\` | 590 lines. Test-owned real native leader/member groups, zombie receipts, wrong-birth denial and late-absence clock. Root test module: \`tests.rs:23-30\`; production \`process_group.rs:480-489\` exposes only its test-only probe/clock seam. |
| \`authenticated_runner/tests.rs\` | absent → \`3fc77f68\` | 122 lines. Additional #952-only native \`/bin/sleep\` / EOF / \`waitpid(ECHILD)\` test. Included through \`authenticated_runner.rs:231-233\` under \`#[cfg(test)]\`. It is not a member of the original 77 paths. |

## Invariants and real native negative oracles

The 952 legacy runner validates host-liveness input before spawn, strips the
untrusted registration environment, creates a private process group, and
attests the Guardian's **direct native child** before the authenticated
registration is accepted (\`authenticated_runner.rs:23-106\`). On host EOF,
resource revocation precedes a birth-attested group signal. A failed group
signal preserves the typed group error while trying \`Child::kill\` and a
deadline-bounded \`Child::try_wait\` on the **retained direct Child** only
(\`:139-229\`). Its native test
\`denied_attested_kgr1_group_signal_never_blocks_on_live_direct_child_wait\`
(\`authenticated_runner/tests.rs:39-122\`) uses a real isolated
\`/bin/sleep\`, closed Unix pipe, altered-birth rejection, a 400ms result
receipt, pre-rescue ESRCH and \`waitpid\` returning ECHILD. The oracle
distinguishes an exited/reaped direct child from a live or zombie one before
any test-owned true-birth rescue.

The shared process-group code reads \`PROC_PIDTBSDINFO\` and checks direct
parent, original real UID and leader start time before authorizing a
signal (\`process_group.rs:165-245\`). When the original leader is gone,
an EPERM group probe is ambiguous and cannot grant signal authority. The
\`generation_host.rs:347-365\` caller requires **two read-only ESRCH observations
inside the original deadline** before acknowledging a CLEAR. The new #952
\`reconcile_denied_signal_after_reap\` seam checks the deadline both before
and after a real read-only native probe (\`process_group.rs:410-489\`).

The \`tests/process_group_failsafe.rs\` fixture distinguishes these cuts:

- \`guardian_signal_eperm_reconciles_only_after_native_group_absence\`
  (\`:112-197\`): two actual same-group native children, independent
  held-zombie reaper, native EPERM receipt, then dual ESRCH after reap.
- \`expired_post_reap_reconciliation_cannot_accept_late_native_absence\`
  (\`:268-377\`): real leader kill/reap and dual ESRCH; independently
  injected before/after clock values make deletion of the post-probe
  deadline guard falsifiable. **The controlled clock is not proof of a
  real slow syscall crossing that deadline.**
- \`stale_group_birth_cannot_signal_a_live_unrelated_native_group\`
  (\`:379-413\`): actual foreign group's survival is sampled before
  the test-owned true-birth rescue.
- \`reaped_leader_with_temporarily_unreaped_dead_member_clears_without_false_host_fault\`
  (\`:459-524\`) and
  \`reaped_leader_with_persistently_unreaped_member_must_not_claim_clear\`
  (\`:527-590\`): actual zombie-only EPERM, respectively a receipt-driven
  native recovery and fail-closed persistent ambiguity.

These are **source-evidenced selectors and oracles**, not tests rerun for
this documentation PR. They establish neither exhaustive scheduler coverage
nor real native acceptance on the current tip.

## Source-level blockers and custody limitations

\`authenticated_runner.rs:126-135\` still calls \`Child::wait\` without
an absolute deadline after rejected spawn; the preceding
\`terminate_attested_group\` may fail. Separately,
\`process_group.rs:493-544\` calls unbounded \`Child::wait\` in both
late/failed Guardian bootstrap inspection branches, even after
\`Child::kill\` returns an error. These are **source-proven conditional
test/production cleanup-liveness hazards**, with no new native negative
reproduction in this docs-only PR. They require their own owner and
failure-first native fix if pursued.

\`authenticated_runner/tests.rs:26-35\` and
\`tests/process_group_failsafe.rs:28-36\` also retain unconditional
test-owned \`Child::wait\` after ignored kill errors. The new runner test
has a bounded **result sample**, but \`worker.join\` after rescue
(\`authenticated_runner/tests.rs:91-100\`) is not itself bounded when
the rescued native process fails to exit. It is incorrect to promote
those tests into a guarantee that all teardown paths are bounded.

Darwin process group IDs remain numeric, reused authority is refused
when the old leader's birth changes, and \`getpgid\`/zero-signal
observations are time-limited **point-in-time witnesses**. The code
does not retain a kernel-pinned process-group handle across
observation and \`killpg\`. Successful direct-child kill/reap or dual
ESRCH does not establish containment of descendants which moved to
another session or group. Neither simultaneous host-and-Guardian
SIGKILL, launchd custody, signed/installed GUI acceptance nor
latest-head process-group identity has been proven here.

## Historical census remains immutable

\`ledger_77.csv\` is an **exact-byte copy** of #992's final19 ledger.
It has **77 rows and 30 original columns**; all 19 LIMITED batch19
entries and every other inherited status/disposition remain identical.
The three historical version-change rows remain visibly
\`changed-since-949-pending\` because the #949-to-#952 change is documented
in a **separate four-row evidence collection**, with no automatic
historical promotion. The #952-only additional file is not inserted
into the 77. Thus zero historical \`pending-source-review\` labels and
three changed-source historical labels are both true without implying
complete semantic or current-source review.

## Structural evidence and validation

Run from this child repository root with original #949, #952 and #992
Git objects available:

\`\`\`sh
python3 -B docs/audits/macos-guardian-2026-10-19-86933528-final19/verify_final19.py
python3 -B docs/audits/macos-guardian-2026-10-19-bb739295-overlays-952/verify_overlays.py
python3 -B docs/audits/macos-guardian-2026-10-19-bb739295-overlays-952/test_verify_overlays.py
python3 -B docs/audits/verify.py
python3 -B docs/audits/test_verify.py
python3 -B tools/ci_inputs.py --check
just hygiene
just fmt-check
git diff --check
\`\`\`

\`verify_overlays.py\` compares unchanged parent evidence and inherited
ledger **against original #992 Git bytes**; checks the exact 3 changed
blobs and old-to-new line diffs plus the #952-only file's absence
at #949; checks complete 1,515 lines partitioned into 29 source
sections and 163 specific native/security lines; verifies 10 caller
anchors and seven oracle references from immutable #952 source;
and pins conservative findings and explicit false claims. Its
\`test_verify_overlays.py\` suite has **38 tests**, including independent
single-anchor removal for every named line, and single-section, caller,
oracle, original-blob, ledger/status, and semantic-overclaim mutations.
These are Python evidence checks, **not mutants executed against
Rust production code** and not a native runtime rerun.

These nested local verifier scripts are **not automatically invoked**
by the hosted public audit registry or standard CI policy. CI success
does not certify their execution. Named independent adversarial
review of this child's exact final commit is pending. All dependent
PRs remain **OPEN / DRAFT / UNMERGED**. Current latest HEAD and the
larger #824 architecture/security program remain outstanding.

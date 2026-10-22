# #824 macOS Guardian — scoped source-review progress, not release acceptance

This is the **second review layer** for the exact 77-source-path Git tree pinned
by [draft #949](https://github.com/gyldlab/keld/pull/949). The original
`ledger.csv` remains the immutable inventory of
`899e287ea487f38ec1cd347f7c9db8f1927348ef` (its code parent is
`ea728589d0143df134c0f21fe5cf335240018d6a`). The new
`source-review-progress.csv` records *per-file* responsibility, inbound/outbound
callers, the relevant security or test ownership invariant, actual Rust native
test selectors (or an explicit helper-only consumer), source-level finding and
disposition. Every row carries the pinned exact Git blob, not the live working
tree or a guessed change count. All 77 original file bodies were inspected in
coherent Guardian lifecycle, registration, control, process-group, supervision,
native-test and integration groups; **a file's initial inventory did not count
as this review**.

The review is scoped to the stated file-level contracts. A source-level
`reviewed-no-change` disposition does **not** prove every OS failure schedule,
whole-repository architecture acceptance, independent kernel custody, or
packaged GUI acceptance. Test-only helpers without `#[test]` have their actual
consuming test module recorded instead of an invented test selector. A
documented `#[ignore]` staged native test is not counted as routine passing
native proof. No reviewer independently reran tests merely by reading source.

## Findings, fixes and remaining native falsifiers

| Concern | Historical finding at pinned #949 | Evidence-backed disposition |
| --- | --- | --- |
| KGC1 post-reap EPERM after original deadline | `process_group.rs` could accept dual-`ESRCH` observations after expiry | [Draft #950](https://github.com/gyldlab/keld/pull/950): checked before and after the read-only probes. A real original group and independent pre/post-guard deletion mutants returned RED. No timeout increase or new signal |
| KGR1 fatal group signal refusal | `authenticated_runner.rs::run_live_group` blocked unboundedly waiting on a still-live direct Child, delaying a terminal error | [Draft #952](https://github.com/gyldlab/keld/pull/952): one same-budget exact-owned `Child` kill/reap fallback after group error, retaining original terminal error; it cannot guarantee whole-group descendant absence if the group-wide signal failed |
| Native integration fixture cleanup | `macos_host_death_guardian/support.rs` used raw group and leaker numbers reported in test events, potentially signaling a different kernel birth; its accepted stream could block forever on a partial event | Separately stacked [draft #953](https://github.com/gyldlab/keld/pull/953) at `46b91848db10f81156cebe0c6c34dec90082eaba`, directly above #952. Failure-first four native tests RED on old source; original kernel-birth and parent validation, read-only checked original deadline and test-owned native sentinel oracles GREEN. Does not add shipping signal authority |
| Generation host fixture panic | `tests/generation_host.rs` spawned live `/usr/bin/tail` and ran multiple fallible protocol assertions before manual kill/wait | Scoped [draft #953](https://github.com/gyldlab/keld/pull/953) uses existing `NativeChildOwner` RAII from spawn and preserves the original compiled selector; native focused 12/12 and full 479/479 passed with five preexisting skips on its final local source |
| Host KGC1 shutdown blocked OS calls | `guarded_primary.rs::shutdown_once` can block in `Child::wait` after a failed kill, and later `JoinHandle::join` of a live worker; Core can block joining Guardian owner | **BLOCKED ON CROSS-OWNER CUSTODY DESIGN**. Merely returning `Err` early would drop the last Child/worker witness in separately owned `keld-core::app_session` (#834). Requires a coherent retained error-owner/native custodian, independent denied-kill and held-worker negative tests; not safe as a one-file timeout patch |
| KGR1 pre-owner and post-ACK constructor failure | Source paths close writer, retain typed error and deny Bun pre-exec without completed host-retention ACK | Specific branch audits found no demonstrated executing-Bun orphan. Separate already-exited constructor and actual post-ACK EPIPE fault-injected native selectors remain possible coverage work, **not** proven production bypasses |
| Legacy `HostGuardian` post-terminal Drop and ECHILD | After writer is consumed, Drop does not retry a retained `GroupIdentity`; externally reaped `Child` may expose a stale PID concern | Native error and birth-retention risks documented. No source-proven safe last-owner recovery after permanent kernel signal denial or reproduced recycled-PID victim. Do not mint numeric fallback authority from this observation |
| First pre-exec PID-receipt final deadline | `spawn_admission.rs::read_child_pid` checks deadline before poll but lacks final successful-read after-clock check | Lower-severity availability/timing issue requiring an independently controlled native late-byte oracle; no shown Bun execution without host ACK |
| Renderer-driven public FS quit | [#948 CI attempt 1](https://github.com/gyldlab/keld/actions/runs/39144935715/attempts/1) reached correct second-launch UI/FS outcome but real host remained alive at exit deadline | [Same-SHA attempt 2](https://github.com/gyldlab/keld/actions/runs/39144935715/attempts/2) succeeded. Root cause remains unproven. Add stage-specific `/quit` receipt, Bun quit handler, KQA and host lifecycle witnesses before assigning a code fix; no deadline increase or disabled test |

## Native protection *not* implemented by this stack

The independent tests
`retained_ack::dual_death::simultaneous_host_guardian_sigkill_after_bun_exec_requires_external_os_custody`
and
`retained_ack::escaped_descendant::detached_bun_descendant_outlives_ordinary_guardian_group_reap`
are **negative** product-acceptance witnesses: both prove actual Bun survivors
*before* narrowly attested **test-owned** rescue. Their successful test result
must never be read as a successful shipping containment guarantee.

Production still has no installed, kernel-pinned external process-group
custodian for simultaneous host/Guardian death, detached new-session descendants
or the interval between numeric PID/PGID observations. Signed/packaged
launchd/XPC custody and GUI acceptance need independent platform evidence.
Core's `GuardianOwner` and other platform modules have their own owners;
the Guardian ledger does not authorize modifying #834, #941 or Windows/Linux
production code.

## Publication and merge boundaries

- The historical original 77-path table stays pinned; follow-up draft source
  edits and newly added native tests are separately pinned in `source-review-tip-953.csv` and `source-review-summary.json` at exact stacked #953 `46b91848`. This yields 79 paths: 70 unchanged historical reviewed blobs, seven modified files requiring patch-specific verification and two newly added test files.
- This is a **partial architecture evidence ledger** on stacked unmerged
  branches, **not** the formal, landed public technical-audit publication
  governed by `.agents/docs.md` and `docs/audits/README.md`.
- Generated `llms-full.txt` remains owned by separate draft
  [#938](https://github.com/gyldlab/keld/pull/938); the stale macOS Guardian
  docs contract was corrected and regenerated in its own direct child
  [draft #951](https://github.com/gyldlab/keld/pull/951), not here.
- No code in this ledger has merged. No architecture PR may merge without
  completing the owner-reviewed #824 whole-repository source ledger, native
  integration/acceptance gates and explicit authorization.

## Exact source-delta evidence after #949

The separate `source-review-tip-953.csv` compares all **79** tracked Guardian paths at #953 `46b91848db10f81156cebe0c6c34dec90082eaba` against the 77 at #949 `899e287ea487f38ec1cd347f7c9db8f1927348ef`: **70 unchanged exact Git blobs, 7 modified, 2 added test files**. The two additions are `authenticated_runner/tests.rs` (#952) and `macos_host_death_guardian/support/tests.rs` (#953). All changed paths remain explicitly linked to their source-owning draft PRs. A changed file does **not** inherit its historical review merely because its path matches; native tests and final-head independent code review/CI are distinct recorded gates. At #953 local Mac full core+runtime Nextest 479/479 passed with 5 skipped and strict Clippy/fmt/source census/hygiene passed; hosted/independent review outcomes belong to exact PR heads, not to the historic #949 review.

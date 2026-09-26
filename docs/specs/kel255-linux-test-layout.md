# Spec: Linux no-flag test ownership pilot
Status: approved
Linear: KEL-255 (KEL-252 Linux slice) · Owner: Keld maintainers · Updated: 2026-09-25

The repository owner approved this Linux implementation and delegated remaining
choices on 2026-09-25 after reviewing the concrete KEL-252 proposal and the requested
amendments. Approval is recorded on KEL-255; it does not confer another agent's claim.
This implements the Linux slice of [KEL-252 PR #288](https://github.com/gyldlab/keld/pull/288)
at proposal commit `cf60141f938d24df093e7b55488ed5f77ed9d33e`.

## 1. Goal & non-goals

Separate product scenarios, fixture creation, and independent Linux observations so
one contract can be understood without unrelated native acceptance machinery. Preserve
shipping behavior, assertions, negative controls, fixture bytes, resource lifetime,
runner identity/scheduling, and observable helper execution. No production changes,
new dependency, runner replacement, cross-platform shared harness or macOS/Windows
source migration. File-size thresholds prompt review, never omission of a regression.

## 2. Spec refs

KEL-96 §§3–4 and 7 governs the existing no-flag shipping observations;
architecture 01 §§1–4 and architecture 06 §§1–2 retain product ownership.
The host AGENTS file owns native shipping acceptance. `.agents/testing.md` retains
proof requirements; the new routed `.agents/test-layout.md` owns placement/migration.
KEL-245 separately repairs the managed scratch prerequisite; its commits are not a
product change or a substitute for before/after Linux acceptance.

## 3. Acceptance criteria (binary, each becomes a test)

1. Native default and `profile-test-hooks` Cargo/nextest listings retain all nine
   baseline entries: eight scenarios plus `keld_dev_linux_helper`. Scenario names may
   gain a documented module prefix bijectively; helper stays at root. No new targets,
   ignored cases or duplicates. Preserve `binary(no_flag_linux)` one-thread scheduling.
2. Every exact helper caller executes its live HELLO/renderer/READY/ECHO handshake.
   A deliberately wrong selector fails that scenario despite a zero-test child exit.
3. Each mechanical family compiles and passes its affected native scenarios before
   the next extraction. Preserve independent mode/inode/bytes/error/procfs/control
   oracles and 20-second deadlines, then pass the complete Linux suite and `just ci`.
4. Scenario modules show launch/action/observation/quit/cleanup/relaunch. Project
   fixtures contain setup; OS/wire observations have narrow named owners. No catch-all
   harness or wildcard import/re-export maze. Product cleanup is observed before any
   fallback cleanup. Existing temporary-directory and local resource scopes remain.
5. The dedicated routed policy fits recorded budgets, passes instruction and negative
   route checks, and leaves root AGENTS unchanged. Required native selectors/binaries
   receive only host-specific additional guidance. Size triggers: entry >100 lines or
   >4 KiB; scenario/support >400 lines or >16 KiB. Retained exceptions name owner,
   reason, issue and review/removal condition.
6. Report actual file/import closures for staging, generation restart and renderer
   fixture changes. No invented token, speed or agent-success benchmark.
7. Publish actual-result Mac Mini and Windows prompts after the pilot, naming exact
   branch/commit/PR, approved design, evidence gaps and dependencies. Execution order:
   Linux, macOS, Windows, then shared-mechanism review. No unrun OS claims.

## 4. Design

No production boundary change. Reuse the existing Linux observers, production API
calls and substantial `tests/fixtures/t1b_harness.ts` program; do not duplicate that
cross-platform fixture or extract tiny configuration/HTML literals for appearance.
The baseline is `14aeb4783b8ce70d55cfa7dcba98522d91c9e8d0`: Linux 968 lines/38,544 bytes.
The pre-move inventory and native listings were retained before editing.

| Responsibility | Destination under tests/ | Existing items |
|---|---|---|
| Registration/helper | no_flag_linux.rs | cfg/lints, declarations, root keld_dev_linux_helper |
| Staging/admission scenarios | no_flag/linux/staging.rs | owner/private/new inode/bytes; stock create sidecar/missing link; invalid boot/lease |
| Ordered lifecycle/relaunch | no_flag/linux/lifecycle.rs | owns-window/two calls/quit/relaunch; run_product_cycle; ProductEvidence |
| Generation recovery | no_flag/linux/recovery.rs | same-renderer recovery; accept_generation |
| CLI/host lifetime | no_flag/linux/dev_lifecycle.rs | delegated ownership/stage; CLI death; host death/tree/relaunch |
| Setup/resource ownership | no_flag/linux/support/project.rs | StageFixture and ProductFixture with impls; prepare_keld_dev_helper; DARK_BG; PRODUCT_TITLE |
| Native observation/control | no_flag/linux/support/process.rs | ProcessIdentity; StrictGeneration; kill declaration; direct host/generation/descendant census; process_stat/executable; identity gone; sigkill_identity; child status/output |
| Control transcript | no_flag/linux/support/control.rs | accept_control_or_host_failure; read_control_line; assert_nonzero_descendant; expect_ready_and_echoes |
| Renderer observation | no_flag/linux/support/renderer.rs | serve_renderer_beacon |
| Stage observation | no_flag/linux/support/stage.rs | sidecar/import check; dev_stage_count; wait_for_dev_stage_count |
| Shared Linux configuration | no_flag/linux/support/mod.rs | declarations; PRODUCT_DEADLINE |

Dependency direction is scenario to narrow support; lifecycle's cycle helper remains
an explicit scenario reused by the host-death follow-up. Use bounded ancestor
visibility and explicit imports. Keep the malformed boot byte string unchanged.
Fixture-integrity checks remain distinct from observations of staged production
output; fixture builders must not decide whether production behavior passed.
No new abstraction replaces handle ownership, crash ownership or principal minting.

Registration, navigation, observations, resource lifetime, instruction routing and
provenance are separate atoms. Module paths explicitly couple navigation to discovery;
cleanup explicitly depends on observing absence before releasing harness resources.
Native evidence is independent of source-size/structural evidence. Every unknown stays
named until tested. Permissions, wire changes, capabilities and runtime seams: none.
Compatibility fallback: retain existing root helper and integration target; mechanical
moves can be reverted independently of the prerequisite runner repair.

## 5. Boundaries

Linux test root/modules; routed layout owner, testing consumer, router/budget manifest;
minimal host AGENTS; this spec and focused migration controls/evidence. Do not edit
macOS/Windows test roots, shared TS fixtures, production crates, Cargo/nextest or CI
selection. Parent PR #288 policy overlap must be reconciled without duplicate owners.

## 6. Tasks (ordered reviewable commits)

- [ ] T1: Repair KEL-245 in its own PR; establish a passing native Linux baseline.
- [ ] T2: Extract staging, then lifecycle/recovery/CLI families, preserving discovery.
- [ ] T3: Introduce dedicated routed policy and smallest behavioral prevention checks.
- [ ] T4: Validate exact final diff, independent review and PR workflow; publish handoffs.

## 7. Test plan

AC1: Cargo/libtest and nextest JSON inventory and show-config in both feature modes.
AC2: actual three shipping helper callers plus wrong-selector negative control.
AC3–4: native affected tests after each move; assertion/fixture/resource-scope comparison;
final all-nine native run; format, warning-denied workspace clippy, full workspace
nextest and exact `just ci`. Negative controls must fail the named contract, not build.
AC5: agent-context, atomic-protocol, llms-test/check, instruction review and route-loss
control, before/after bytes and pinned-token counts; actual client prompt traces where
available, never infer native hook activation. AC6: measured file/read closure. AC7:
re-fetch published state and handoff dependencies. Keep Linux session details with logs.

## 8. Review gates triggered

Test-only unsafe observation move: independent review of unchanged Linux signal/identity
proof. Public API, permission model, dependency addition and wire protocol: none.
Independent instruction review required. No CI routing change is planned.

## 9. Perf impact

No production or test-speed improvement claimed. Keep one integration executable;
measure source/read locality separately from runtime/build cost.

## 10. Open questions

None requiring a new user decision. Native acceptance, final review and cross-machine
qualification remain evidence obligations, not presumed results.

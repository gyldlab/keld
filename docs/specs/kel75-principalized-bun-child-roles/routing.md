# KEL75 virtual port routing, boundaries and delivery

Part of: [index](index.md)
Owns: sections 4–7, 9–10

## 4. Design: routing and Electron facade
### Routed virtual ports

The host owns `VirtualPort` pairs. A port capability is bound to one principal
generation and a route target; a receiver must be an already authenticated role or a
live webview/navigation generation selected by host policy. The host performs every
transfer, validates its target and chargeable quota, then forwards bounded frames over
existing links. There is no direct renderer-to-role transport.

The generic contract is FIFO per port generation, one-shot ownership transfer,
host-visible close/disconnect, bounded queues/credits and generation invalidation. The
compatibility facade maps Electron's `MessageChannelMain`, `MessagePortMain`,
`webContents.postMessage` and `utilityProcess.postMessage` into that contract. Exact
Electron event/error shape is established by pinned-oracle conformance fixtures rather
than assumed from this generic transport prose.

### Electron facade boundary

`utilityProcess.fork` is an `@keld/electron` request to the host for an app-bound or
window-bound declared role. It is not a generic child-process escape hatch. The facade
may expose its own OS PID only as a diagnostic after host spawn; no Keld authorization,
reap or routing decision uses it. Requested `env`, working directory, executable flags,
network session and unsigned-library options are translated to reviewed Keld policy or
rejected with a documented compatibility result. `allowLoadingUnsignedLibraries` can
never silently weaken a strict profile.

## 5. Boundaries

- Implement in later slices: `keld-runtime`, `keld-core`, `keld-ipc`, `keld-guard`,
  `keld-compat`, `@keld/api`, `@keld/electron`, and generated schema packages.
- The original specification-only review is confined to the listed architecture documents
  and this file. KEL-75 implementation tasks may touch only their named crate/package
  boundaries; unrelated frame, manifest, native-service, CI, or package-specific changes
  require their owning issue and review gate. T1a is the documented exception: it added
  the reusable `keld-ipc` bootstrap listener, migrated the CLI echo consumer, and used
  the already workspace-pinned `getrandom` dependency. It did not change a wire version,
  add an async runtime, or add a package-specific branch.

## 6. Tasks

### T-1: Completed T1a shared bootstrap listener

- **Blocked by:** none

- [x] T1a: Extract a generic Unix bootstrap listener in `keld-ipc`, used by the live
  CLI echo server. It mints an owner-only endpoint/token, continues accepting after an
  invalid `HELLO`, and unlinks on drop. This proves one shared bootstrap primitive, not
  a role coordinator.
- [x] T1b: Reuse KEL-70's `Supervisor` for one Unix host-owned `primary` role
  coordinator through its internal prepared-child/lease seam. It provisions fresh
  identity before every spawn, binds only a successful `HELLO`, revokes before successor
  provisioning, reports redacted bootstrap rejection, and proves the black-box restart
  flow. No Windows named pipe/DACL, ports, extra roles, sandbox or shared memory.
- [x] T2: Add one `app-bound` role and host-owned lifecycle registry; prove independent
  crash/restart isolation and stale-generation rejection.
- [x] T3: Add a bounded host-owned virtual-port pair between two authenticated roles;
  prove transfer, close and cross-principal negative cases before Electron facade code.
- [x] T4a: Freeze the `window-bound` owner-generation state model, close/restart race
  ordering, opaque authority revocation, PID-independent host-death evidence contract and exact
  renderer-continuity beacon. The portable executable trace oracle is contract proof,
  not a shipping implementation or real-webview pass.
- [x] T8: Extend the one T1b generation owner to a Windows `primary`; it
  originally landed over the KEL-96-D10 unprivileged loopback interim. Reuse one cross-platform
  `keld-ipc::BootstrapListener`, expose the authenticated bound generation to its
  future host router, and prove real Bun g1→g2 rotation, stale authority rejection,
  revoke-before-successor, handle-owned shutdown and a clean next cycle. T8 is an
  independently promoted predecessor and does not wait for T5–T7.
  KEL-101/T3 later migrated that shared listener and its consumers to the
  current-user-DACL named pipe without changing this generation-owner contract.
- [ ] T4: Add `window-bound` role lifecycle and real host-window-close integration.
  Consume T4a without adding a second restart loop or treating the event queue as the
  authority ledger. Real macOS, Windows and Linux acceptance remains here (or in an
  explicitly approved successor product node), not in T4a.
- [ ] T5: Add the `utilityProcess`/`MessageChannelMain` compatibility facade through
  pinned Electron conformance entries, one operation family at a time.
- [ ] T6: Integrate KEL-78's approved per-OS sandbox admission and addon-worker proof;
  only then permit a strict-profile release claim.
- [ ] T7: Consider a role-private bulk mapping only after an attributed Keld end-to-end
  benchmark and hostile handle-inheritance proof; inline bounded `RAW` remains required.

## 7. Test plan & evidence

| Acceptance | Future fixture | Independent oracle |
|---|---|---|
| T1b subset of 1–3 | real Bun subprocess + owner-only test control socket + hostile raw kipc client | ordered `Provisioned(g1) → Spawned(g1) → LinkBound(g1) → Revoked(g1) → Provisioned(g2) → Spawned(g2) → LinkBound(g2)`; host-only redacted `KELD-IPC-007` rejection; stale g1 locator fails; legitimate g2 client binds after foreign g1-token input |
| T8 Windows subset of 1–2 | real Bun subprocess + loopback test-control socket + shared `BootstrapListener` + authenticated stream echo worker | See T8 Windows subset of 1–2 detail immediately below |
| 4 / T4a | portable trace model plus direct-child subprocess-isolation helper | exact owner-window tombstone; close at successor prepare/spawn/bind/ready boundaries; every opaque authority class revoked; stale/PID-only/duplicate cleanup rejected; correlated app-bound and other-window replies; admitted-work drain; app shutdown cleans every role; next helper cycle succeeds |
| 4 / T4 | per-backend host integration fixture with two real temporary window owners and hostile child processes | exact identity-qualified close delivery; bound role and descendants gone; app-bound and other-window positive call; clean relaunch |
| 5 / T3 | virtual-port contract fixture | ordered received sequence, exactly-once disconnect, and no foreign delivery |
| 6 | platform-specific hostile sandbox fixture | real OS-visible fs/net/handle attempts; untested OS stays unverified |
| 7 | pinned Electron differential fixture | Electron's documented behavior and checked-in oracle result |
| 8 / T4a | portable trace model | changed/missing/replayed document nonce or missing post-`Ready(g2)` beacon is rejected |
| 8 / T4 | per-backend document-local nonce plus port-0 beacon server | same window/navigation/document nonce before crash and after successor readiness, strictly increasing beacon sequence |

**T8 Windows subset of 1–2 (full table-cell detail):**
first silent peer times out without consuming the generation; distinct g1/g2 generation, endpoint
and token; real echo on both; exact `Revoked(g1)` before `Provisioned(g2)`; exact stale g1 port
can be rebound; g1 token rejected once on g2 without consuming it; shutdown wait succeeds, the
same fixture's instance-bound control connection closes, and a clean next coordinator cycle
completes


Fixtures use temporary owner-only endpoints, bind ports to `0`, wait for explicit
readiness records, and use timeouts only as kill switches. Crash/lifetime assertions run
the risky child out of process. T1 does not claim renderer continuity: current
host/webview code has no persistent window registry plus renderer acknowledgement oracle.
T4a proves only that the portable contract accepts the complete trace and rejects
counterexamples. T4 must prove the document nonce and post-restart beacon on each real
backend it claims. The test author performs a temporary negative control by reusing a
token, binding before `HELLO`, closing after one invalid `HELLO`, removing generation
revocation, dropping the close tombstone or accepting a changed/missing document nonce,
and records the named failing test.

Acceptance classification for T4a is fixed:

- `CI-only`: the trace state model, exact counterexamples and direct subprocess
  hostile-shutdown/next-cycle isolation in
  `crates/keld-runtime/tests/window_bound_contract.rs`;
- `not-applicable`: native close-event delivery, real renderer continuity and platform
  process-tree reaping. These remain future T4 product evidence; KEL-78 owns strict
  sandbox admission, and KEL-97/runtime-04 must re-check its own shipping/OS entry gate;
- no real-OS criterion is owned or passed by T4a.

Acceptance classification for T8 is separate:

- `CI-only`: shared generation-owner ordering, cross-platform compilation and
  existing Unix bootstrap/role regression coverage;
- `real OS/device`: Windows 11 with pinned Bun 1.4.0 — both authenticated
  generations complete an echo after a first silent peer, stale g1 authority fails,
  revoke precedes g2, shutdown wait succeeds, that Bun instance's control connection
  closes and a fresh same-fixture cycle succeeds;
- `not-applicable`: KEL-96/T4 no-flag host/window behavior, KEL-101 named-pipe/DACL,
  privileged dispatch, strict OS containment and Windows abnormal-host-death cleanup.

The T4a assertions are named so review can repeat the negative control instead of
accepting a prose claim:

| Deleted or inverted rule | Test that must fail |
|---|---|
| remove fresh successor generation | `restart_rotates_generation_after_full_authority_revocation` |
| reset the generation counter by recreating a coordinator | `coordinator_recreation_cannot_reset_role_generation` |
| provision before locator/link/dispatch/grant/port/mapping/pending-call revocation | `restart_rejects_successor_before_full_authority_revocation` |
| revoke before portable natural-exit reap | `natural_exit_is_observed_and_reaped_before_revocation` |
| infer ready before spawn/authenticated bind or after owner close | `ready_requires_spawn_authenticated_link_and_live_owner` |
| publish ready for a post-bind/pre-ready failure or recover before revocation | `post_bind_pre_ready_failure_revokes_before_recovery` |
| reap an admission failure before revocation or restart that terminal failure | `admission_failure_revokes_before_reap_and_is_terminal` |
| reap a post-bind protocol failure before revocation or restart that terminal failure | `protocol_failure_revokes_before_terminate_and_is_terminal` |
| accept a retired generation | `stale_generation_is_rejected_after_rotation` |
| broaden a close to app-bound/other-window roles | `closing_one_window_revokes_only_its_bound_role` |
| omit the close tombstone at prepare/spawn/bind/ready | `close_wins_at_every_successor_boundary` |
| revoke before quiesce, drain before route/grant invalidation, close before reap, or reap before link/locator revocation | `window_close_revokes_routes_before_drain_and_link_before_reap` |
| admit new work after quiesce or finish drain with admitted work outstanding | `drain_completes_admitted_work_and_rejects_new_work_after_quiesce` |
| accept a changed/missing/replayed nonce or no post-restart beacon | `continuity_requires_same_document_nonce_and_post_restart_beacon` |
| accept PID-only reap or leave host-death authority live | `host_death_requires_full_revocation_and_handle_bound_reap` |
| stop an app session with a live authority/handle or schedule a successor after quiesce | `application_shutdown_revokes_and_reaps_every_role_before_stop` |
| stop cleanup after the first failure or discard the aggregated terminal failure | `application_shutdown_continues_after_cleanup_failure` |
| run hostile shutdown in the test runner or break relaunch | `hostile_shutdown_is_subprocess_isolated_and_next_cycle_succeeds` |

T8's real-Windows negative controls temporarily remove revoke-before-successor,
reuse the retired endpoint or token, stop accepting after a foreign `HELLO`, and
disable supervisor shutdown before the clean next cycle. Each mutation must make
`windows_primary_restart_rotates_authenticated_generation_and_rejects_stale_authority`
fail and must be restored before review.

The completed node publishes `keld.execution-artifact/v1` with
`node_id=role-lifecycle-contract`, `issue_id=KEL-75`,
`approved_task_id=KEL-75/T4a`, `status=passed`, an ancestor-of-current-main
`head_sha`, the exact prompt source/body digest, CI-only T4a acceptance rows and
explicit `not-applicable` product-OS rows. That artifact records contract completion;
it cannot mark T4 implemented, satisfy a KEL-102 approval, or substitute for
KEL-97/runtime-04's own entry gate and real-OS observations.

The completed T8 node publishes a separate `keld.execution-artifact/v1` with
`node_id=windows-primary-generation`, `issue_id=KEL-75`,
`acceptance.id=KEL-75/T8`, `status=passed`, an ancestor-of-current-main
`head_sha`, the real Windows/Bun observation and explicit not-applicable rows for
KEL-96/T4, KEL-101, privileged dispatch, containment and host-death cleanup.

## 9. Perf impact

T1 adds a cold bootstrap listener and is not a hot-path optimization. T1–T5 measure only after semantic correctness:
role spawn/restart latency, p99 bounded routed-port latency, queued-byte maxima, CPU and
RSS. Shared memory is not a baseline or an acceptance condition; see the committed
P13 new-run evidence and `docs/research/campaigns/vscode/reports/48-p13-new-run-audit.md`.

## 10. Open questions, blockers & unknowns

- KEL-78 must select and prove the platform lifecycle/reaping mechanism used to ensure
  host death cannot orphan role descendants on each supported OS.
- KEL-74 must freeze the versioned compatibility-record format that stores the Electron
  oracle revision, fixture artifact hash and operation-level result.
- KEL-101 must separately approve and prove the named-pipe/current-user-DACL
  transport before privileged Windows dispatch can replace T8's explicitly
  unprivileged loopback predecessor.

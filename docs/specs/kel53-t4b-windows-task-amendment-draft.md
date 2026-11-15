# Draft amendment proposal: KEL-53 T4b Windows task coordinator

Status: **Draft, not approved; no production implementation is authorized by this file.**
Linear: KEL-270 / KEL-53 · Owner: KEL-53 maintainers · Updated: 2026-09-28

This proposal amends the approved `docs/specs/kel53-full-package-activation.md` T4b
Windows runtime authority and handoff. It does not alter the approved spec in place.
An owner must approve the exact amendment and its new content hash before implementation.

## 1. Goal & non-goals

Evaluate an installer-provisioned, fixed, on-demand Windows SYSTEM task as the wake-up
for a narrowly scoped activation coordinator, while ordinary application code remains
non-SYSTEM. Starting the task is not authorization to update: no protected state may
change until the coordinator authenticates the exact installation, host and fresh
attempt, obtains the one writer lease, and proves the required process/reader handoff.
The existing monotonic floor, journal order, 30-second health rule, rollback target and
fail-closed recovery contract remain unchanged.

Non-goals: a resident/general-purpose broker or service; caller-controlled task
arguments; a broad Users write/owner/WRITE_DAC grant; running the application as SYSTEM;
new app permissions, KIPC frames, manifest fields or dependencies; macOS/Linux activation
before KEL-137; delta updates; or using this feasibility probe as product acceptance.

## 2. Spec refs

- `docs/specs/kel53-full-package-activation.md`: T4b, health identity, single-writer
  transition, recovery and approved inherited-handle contract.
- `docs/architecture/03-security.md`: host-minted principal, default-deny and role
  isolation.
- `docs/architecture/06-runtime-and-tooling.md` §4: update ownership, direct/managed
  admission and current activation flow.
- KEL-135 owns verified publisher/app identity and profile identity; KEL-53 retains
  installation provenance and active-selection ownership. KEL-254 owns installed-boot
  verification and role reads; KEL-96 owns no-flag host admission. Their boundaries
  must be reconciled before this amendment is approved.

This changes a runtime authority entry point, handle ownership and crash ownership.
Architecture 06 and the activation spec MUST be amended together in the approval PR.

## 3. Acceptance criteria (binary, each becomes a test)

1. From trusted installation, one fixed SYSTEM task is provisioned with a protected
   executable/configuration/action, no triggers or caller substitutions, and
   `IgnoreNew`. Admin/SYSTEM retain full control; the intended ordinary SID receives
   only the approved read/execute task rights. A standard user can start it; a
   configuration-change attempt is explicitly denied. Re-registration, action changes,
   deletion, DACL changes, arbitrary arguments and image substitution are denied.
2. A task start with no authenticated host request is a harmless wake-up: zero protected
   journal/pointer/floor writes and no candidate launch.
3. A same-user, same-session wrong host, wrong installation, hostile role, substituted
   image, stale attempt, replay or arbitrary local pipe client is refused before a
   journal write. Only a fresh host-minted capability bound to exact installation,
   artifact, coordinator image and attempt admits one update.
4. The coordinator receives and validates the initiating host's actual Windows token
   and process identity. SID/session alone, task invocation, PID/path alone, command
   line, environment or feed/config input never authorizes an attempt.
5. One protected update lease serializes manifest selection through recovery. Existing
   immutable ancestry pins remain held; mutable record/pointer readers yield only by a
   specified race-free handoff. A competing writer, retained conflicting reader or
   unknown lease owner causes refusal with no partial publication.
6. The coordinator launches only the exact journaled candidate with the authenticated
   initiating user's intended token/session and role restrictions. Candidate health is
   attempt-bound, proves the exact artifact booted and reached Ready, and requires 30
   monotonic seconds without unexpected generation exit. A transferred event or generic
   marker alone cannot commit.
7. Coordinator loss, user logoff, task re-run, candidate descendants and stale/replayed
   requests obey the single-owner process-family and recovery rules. Recovery waits for
   authenticated family death; live/unknown state refuses. Every persisted crash cut
   preserves the trust floor and either resumes the exact attempt, completes the
   journaled rollback, or halts.
8. The successful path and all invalid/replay/concurrency paths have exact-source native
   Windows evidence. Unsupported platform/channel cells retain typed fail-closed
   behavior.

## 4. Design

**Facts and current proof.** On Windows 11 Home Single Language, 64-bit, version
10.0.26200/build 26200, an administrator-provisioned task with a fixed action and user
DACL `0x1200A9` (read+execute) could be started by the verified non-elevated user.
`schtasks /Change /Disable` returned exit 1 / `ERROR: Access is denied`; `schtasks /Run`
returned exit 0 / SUCCESS. The worker then ran as SYSTEM/session 0, captured the normal
user's pipe token, outlived the initiating process, launched one candidate in session
13 at medium integrity with the user's profile/TEMP, and explicitly duplicated a
limited event handle to that candidate. It signaled the event and exited 0. The helper
verified cleanup; standard-session lookup then returned `FILE_NOT_FOUND`. The fixed
action was protected against the exercised ordinary-user operations. An execute-only
`0x20` ACE was insufficient for this `schtasks` run path; its `/Change` result was
ambiguous “task does not exist,” so it is not counted as a modification-denial proof.
These tests prove the exercised OS mechanics only; they do not prove an exact Keld
host, installation, attempt, writer lease, health or recovery.

Evidence provenance: standard-user rights and pipe transcript is work-run evidence
`run-6dbd4d028d9947b58e97ddf4865e6aef`; successful result receipt is
`C:\ProgramData\KeldKel270Probe-aa1b0d08db26446097f621ba75a8354a\result.json`;
the operator printed `KELD_PROBE_PASS` for that root. Reviewed source hashes:
`install-readexec-probe.ps1` `031D736A7C8D02AA96396EB4240D3C095673600278405B2059D5A47D19794C5F`,
`task-readexec-check.ps1` `F28D8C725E94FD578F8485B47B04363A2C675F5B823D6E3F62715B2CEE6CE239`,
`Native.cs` `9A1D10A152E94D00434BC2535EEDDC5F73C1FC73F09874C0B582613938F21B11`,
and `probe.ps1` `2E117F2DFE25AFB9B515099CCC32DFF49F90649433564FDD54947A68B36D3BB2`.
The prior worker-local pass with a null image field remains a failed overall run and
does not close this evidence row.

**Decision.** Continue evaluating the fixed task as the smallest demonstrated wake-up;
there is no evidence-based reason in these results to switch to a service. A service
would require a separately qualified SCM lifecycle/control path and would not solve the
missing host/attempt authentication. This is a recommendation for amendment, not
approval to ship.

**Proposed ownership.** The trusted installer owns task creation and image/config
protection. Task Scheduler owns the SYSTEM launch. A short-lived coordinator owns only
the authenticated activation attempt and exits after health/rollback. The host remains
the only principal that can mint an update attempt or health identity. The task run
right is only a wake-up; it never carries or establishes update authority.

**Authentication/channel gate (blocking design input).** The approved spec assumes
trusted-spawn inherited handles; Task Scheduler does not provide that relationship.
The production cross-task request and candidate-health transfer mechanism is not chosen
by the feasibility harness. Before implementation, extend/reuse the existing
`keld-ipc` Windows named-pipe owner and guard-owned token/profile policy rather than add
a parallel parser/transport. Specify how the coordinator authenticates the host process,
installation and one-use attempt without trusting SID/session, PID/path alone, argv,
environment or a generic pipe frame. Specify how the exact private health endpoint is
transferred to the candidate without inherited cross-session handles. If those contracts
cannot be met by existing owners, keep automatic activation refused.

**Lifecycle and writer gate (blocking design input).** Define the exact writer lease and
how retained baseline/current readers release mutable pointer/journal records without a
replacement race. Distinguish task-instance `IgnoreNew` from the installation-wide
mutation lease. Bind coordinator and candidate to the existing exact process-family
owner; a new task invocation must not recover or relaunch while a prior family may live.
The coordinator must not write any updater record until authentication and the lease
both pass.

**Reuse/fallback.** Reuse the existing strict manifest/verifier, guard-owned ACL
profiles, `keld-ipc` transport owner, runtime candidate/job lifecycle and approved T4b
publication order; extend the existing strict local-record machinery for the specified
journal (there is no implemented T4b activation-journal codec to reuse). The signed
fixed task/coordinator is Windows-only. Do not add a second transport, signer/parser,
user-writeable settings path or general service. Until all acceptance rows pass,
standard-user activation returns the current typed unsupported/refusal result and leaves
installed state unchanged.

## 5. Boundaries

Implement only after approval and ownership refresh:

- `keld-update`: authenticated activation attempt, writer lease, journal/recovery and
  update result state.
- Existing host/runtime owners: mint/validate attempt and health identity; exact
  candidate/session/process-family lifecycle.
- Existing guard/IPC owners: task/user trust profile, Windows peer-token/host
  authentication and named-pipe semantics.
- Trusted installer owner (KEL-266 predecessor or explicitly designated installer):
  signed coordinator/task provisioning and removal.

Must not touch renderer permissions, manifest/KIPC wire, package-manager mutation,
macOS/Linux activation, or another crate's internal policy without a separate owner
decision. The task DACL and external test helpers do not themselves define production
authorization.

## 6. Tasks (ordered, no implementation before approval)

- [ ] T1 — KEL-53 owner approves this exact amendment; KEL-135/KEL-254/KEL-96 record
  host identity, installed-boot and role-read ownership.
- [ ] T2 — prove same-user wrong-host/wrong-install/hostile-role requests cannot obtain
  an admitted attempt or mutate protected state; add the exact-host positive control.
- [ ] T3 — specify and test one-use attempt authentication, replay and competing requests
  before adding updater writes.
- [ ] T4 — qualify writer lease plus retained-reader handoff and process-family death.
- [ ] T5 — integrate exact candidate health endpoint, same-session launch and 30-second
  health contract while preserving current journal publication order.
- [ ] T6 — real Windows crash cuts, corruption/replay substitutions, subsequent valid
  attempt, hosted gates and independent security review.

## 7. Test plan

| Criterion | Test / first falsifier |
|---|---|
| Task ACL | Read task DACL as admin; standard-user `/Change /Disable` denied; `/Run` succeeds; action/config mutation and deletion denied |
| No authority from wake-up | Bare/repeated task start leaves all protected bytes and candidate count unchanged |
| Host/install/attempt authentication | Same SID/session wrong host/image/install/attempt/replay rejected before write; exact host positive |
| Exclusive writer and readers | Retained-pin and competing-writer subprocesses show zero pointer/journal changes until exactly one lease/handoff |
| Candidate/lifecycle | Exact token/session/profile; explicit endpoint transfer; early exit, user switch/logoff and live descendant refuse health/recovery |
| Journal/health | Terminate at every durable boundary; exact 30-second Ready commits only exact attempt; otherwise exact rollback, floor never decreases |
| Role/evidence | Actual Keld role tokens, exact source hashes, task XML/DACL, OS/build, token facts, command transcript, receipts and cleanup retained |

Timing uses monotonic deadlines and bounded subprocesses. Mocks, the PowerShell probe,
or a task `Run` success cannot substitute for exact Keld host/attempt and crash-cut proof.

## 8. Review gates triggered

`unsafe`: conditional on the final native implementation; exact-diff independent review.
`public API` and internal dependency-edge change: conditional on the selected
cross-crate capability surface. `permission model`: yes. `dependency addition`: none
currently proposed. `wire protocol`: none currently proposed; any authenticated IPC
contract change must update its owning versioned protocol. These gates are not closed
until the authentication transport and implementation diff are selected.

## 9. Performance impact

No performance claim. The task is on-demand; measure update start latency only after the
correctness/authority contract is approved. No budget change is justified by this probe.

## 10. Open questions / approval gates

1. What existing host-owned capability proves the caller is the exact Keld host and
   binds installation + artifact + attempt across an independently launched SYSTEM
   task? If none exists, which owning abstraction receives the smallest extension?
2. How do mutable record pins yield to one writer without opening an ancestry or
   replacement race?
3. Which trusted installer owns task image/config provisioning, update, revocation and
   removal across upgrade/uninstall?
4. How do KEL-135/KEL-254/KEL-96 consume the same authenticated installation/host
   identity before boot admission and deny hostile roles?
5. Owner approval of the exact revised T4b / Architecture 06 boundary is required.
6. How does a fresh coordinator prove the prior host/candidate process family is gone
   after the original process handles and owner disappear?
7. For every transferred endpoint, how is it authenticated, consumed once, closed or
   revoked, and prevented from becoming a stale capability?
8. How do bare/repeated task wake-ups terminate without accumulating privileged workers
   or interfering with one admitted attempt? `IgnoreNew` only blocks simultaneous task
   instances and is not the update lease.

Until these are answered and the exact amendment is approved, this document is not an
implementation contract and the production updater remains fail-closed.

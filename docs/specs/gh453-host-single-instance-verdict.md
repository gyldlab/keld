# Spec: Host-owned single-instance verdict
- Status: draft
- Tracker: GH-453
- Owner: @0monish (draft owner; approval pending)
- Updated: 2026-11-21
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

The original draft was dated 2026-11-06. GH-453 is the tracker of record under #517.
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Provide synchronous requestSingleInstanceLock and hasSingleInstanceLock from a host-held cooperative lock and an immutable initial
election fact, while still launching the losing Bun role for version/help/export paths. Current lease state after release is a
separate host-owned fact.

Non-goals: GH524 owns second-instance delivery, additionalData transport and releaseSingleInstanceLock. Protocol registration,
profile leasing and Windows/Linux lock qualification are separate.

## 2. Spec refs

- [Owning issue GH-453](https://github.com/gyldlab/keld/issues/453) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Current app facade](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/app.ts).
- [Boot contract owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/app_session/boot_contract.rs).
- [Session facts draft](https://github.com/gyldlab/keld/blob/c2dd2ff53767adf20dc0e91d276310095fe31acf/docs/specs/gh452-app-session-facts.md).
- [Private-lock owner correction](https://github.com/gyldlab/keld/issues/453#issuecomment-6564005869).
- [Reported kernel-lock observations](https://github.com/gyldlab/keld/issues/453#issuecomment-6560426753).
- [Electron lock contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/app.md).

Historical baseline at the pinned source: the facade had no single-instance member. Public comments report local std::fs::File::try_lock
observations and an approved shared-ancestor/private-service correction. This session did not execute that fixture or inspect
unpublished local code.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria

### AC-1: A declared winner reads the synchronous verdict at either call time
- **Given:** An app declares one stable lock identity and no other process holds it.
- **When:** Its host starts and the facade calls before or after Ready.
- **Then:** `requestSingleInstanceLock()` returns true synchronously, and `hasSingleInstanceLock()` returns true.
- **Negative control:**
  - **Single fault:** Replace the winner's captured host verdict with false.
  - **At-risk precondition:** A declared app starts as the sole lock contender.
  - **Expected RED:** Both facade calls disagree with the host's successful election.

### AC-2: A losing host still runs its selected Bun role
- **Given:** Another host holds the same declared app lock.
- **When:** A second host starts and its selected app requests `--version`.
- **Then:** The second election is false, while its Bun role prints the version and exits successfully.
- **Negative control:**
  - **Single fault:** Exit the losing host before Bun spawn.
  - **At-risk precondition:** The selected losing app has a reachable `--version` path.
  - **Expected RED:** No app-produced version output appears for the second process.

### AC-3: An undeclared app receives the typed migration error
- **Given:** The compiled boot declaration has no single-instance entry.
- **When:** The app calls `requestSingleInstanceLock()`.
- **Then:** A registered typed compatibility error names the missing declaration and migration fix.
- **Negative control:**
  - **Single fault:** Return false silently for an undeclared app.
  - **At-risk precondition:** The app has no compiled lock declaration.
  - **Expected RED:** The typed-error and fix-it assertion fails.

### AC-4: A safe shared ancestor is accepted without mutation
- **Given:** The same-user Keld ancestor is stable, mode 0755, and has no group/other write grant.
- **When:** The host admits a new private service directory.
- **Then:** The ancestor stays unchanged. Private directories stay 0700. The single-link lease stays 0600.
- **Negative control:**
  - **Single fault:** Require mode 0700 on every ancestor.
  - **At-risk precondition:** A valid same-user shared ancestor uses mode 0755.
  - **Expected RED:** The valid shared-ancestor admission is refused.

### AC-5: Unsafe namespace entries are refused before role spawn
- **Given:** A symlink, replaced identity, multiply linked lease, or wrong private permission is present.
- **When:** The host checks namespace admission.
- **Then:** Admission refuses before role spawn and preserves the existing path without chmod, unlink, or truncate.
- **Negative control:**
  - **Single fault:** Follow a symlink during the admission walk.
  - **At-risk precondition:** A symlink appears in the checked namespace path.
  - **Expected RED:** The refusal or stable-identity assertion fails before Bun spawn.

### AC-6: The kernel lock distinguishes contention from owner death
- **Given:** A live holder owns a lease and later terminates normally or abruptly.
- **When:** Another process retries acquisition using an explicit process-I/O barrier.
- **Then:** It observes contention while the holder lives and acquisition after the kernel releases the lock.
- **Negative control:**
  - **Single fault:** Treat pathname existence as the held-lock verdict.
  - **At-risk precondition:** The original holder crashes without deleting the lease path.
  - **Expected RED:** The replacement cannot acquire after the kernel releases the descriptor.

### AC-7: The pre-spawn election divergence is recorded
- **Given:** A declared app delays its first compatibility call.
- **When:** A second process contends before that call occurs.
- **Then:** The first host already holds the lock, and the documented divergence identifies pre-spawn admission.
- **Negative control:**
  - **Single fault:** Defer host lock acquisition until the facade method runs.
  - **At-risk precondition:** The declared first app delays its first lock method call.
  - **Expected RED:** A second process acquires the lock while the declared first host is alive.

### AC-8: Contention cannot be hidden behind a constant facade result
- **Given:** The first declared host has already acquired the same stable lock.
- **When:** The second host reads its initial verdict before or after Ready.
- **Then:** The second `requestSingleInstanceLock()` returns false synchronously.
- **Negative control:**
  - **Single fault:** Replace the host-issued verdict with a facade-local constant true.
  - **At-risk precondition:** Both processes declare the same lock identity.
  - **Expected RED:** The second process reports a false winner.

### AC-9: A writable shared ancestor is rejected
- **Given:** A same-user shared ancestor grants group or other users write access.
- **When:** A host attempts new private service-directory admission.
- **Then:** Admission refuses and does not grant custody to the unsafe shared path.
- **Negative control:**
  - **Single fault:** Accept a writable shared ancestor as safe.
  - **At-risk precondition:** A shared ancestor has a group/other write grant.
  - **Expected RED:** The expected refusal changes to successful admission.

### AC-10: A rejected incumbent lease remains intact
- **Given:** The existing lease has a multiple-link, identity or private-permission defect.
- **When:** The host rejects admission.
- **Then:** The host does not chmod, unlink, truncate, repair or replace the incumbent lease.
- **Negative control:**
  - **Single fault:** Unlink an incompatible incumbent lease during refusal.
  - **At-risk precondition:** A second process presents a defective existing lease.
  - **Expected RED:** The before/after inode and path-preservation checks detect mutation.

## 4. Design

### 4.1 Identity and custody

Reuse the repository's existing instance-lock owner and approved std::fs::File::try_lock proposal; do not add a lock daemon,
second election service or PID-file protocol. Identity is compiled application identity plus the approved per-user namespace, not
argv or a renderer string. The original Host holds the descriptor for the complete elected lifetime.

### 4.2 Stable namespace

Preserve the public owner correction: a shared ancestor is same-user owned, nofollow/kind/identity stable and not group/other
writable; private InstanceLocks/v1/application-id components are 0700 and the lease is 0600, single-link and close-on-exec.
Perform the owning implementation's pre/post identity checks. Never unlink a held lease: a replacement inode would create a second
election domain.

### 4.3 Verdict delivery

Capture the initial election verdict once before Bun spawn and deliver it through GH452. Before any release, request/query can
synchronously read that initial state. GH524's future release requires an explicit joint extension for host-confirmed current
lease state; hasSingleInstanceLock must never report the original true verdict after confirmed release. Reacquisition cannot be
inferred from the captured initial verdict. Contention is false, while permission/type/identity failures are typed startup errors.
The losing host still starts the selected app.

### 4.4 Trust limitation

This is cooperative same-user instance election, not protection against a hostile same-user process that can replace its
namespace. Keep that limitation explicit; do not promote election to a filesystem grant or imply Strict-profile isolation. Lock
custody must not leak through child inheritance.

### 4.5 Schema and migration

The compiled boot-schema owner must allocate the declaration against the current schema; the old issue's schema-v1 wording is
historical. No-flag boot must not parse keld.config.ts. Migrate declaration generation and session facts with their existing
owners; the compatibility facade remains permanent. additionalData is only validated/stored until GH524 admits delivery.

### 4.6 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future implementation stays in the existing Core instance-lock/boot owner, GH452 facts and Electron app facade. Preserve profile
leases, Strict defaults and shared ancestors. No lock-path cleanup or local worktree edits in this documentation batch.

This publication changes only `docs/specs/gh453-host-single-instance-verdict.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks

### T-1: Pin and approve the declaration and session-fact contracts
- **Blocked by:** B-1
- [ ] Pin the compiled declaration and session-fact schema. Record pending cells and the pre-spawn divergence.

### T-2: Implement host lock admission with independent process controls
- **Blocked by:** T-1, B-2
- [ ] Implement stable lock admission and retained descriptor custody.
- [ ] Run real two-process contention, holder-death and filesystem substitution negatives.

### T-3: Expose the synchronous host verdict through the owned facade
- **Blocked by:** T-1, T-2, B-1
- [ ] Thread the captured verdict through GH-452 session facts.
- [ ] Expose both synchronous facade methods and the undeclared typed error.

### T-4: Qualify the selected original application on macOS
- **Blocked by:** T-3
- [ ] Exercise original-app version, help, export and GUI loser behavior.
- [ ] Qualify macOS; record Windows and Linux as unverified.

## 7. Test plan & evidence

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-453;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Synchronize lock holders and contenders through process I/O, not sleeps. Record process lifetime, dev/inode identity and exact
permission refusal. Reproduce the reported kernel fixture independently; a mocked bool or single-process test cannot establish
election.

Documentation validation covers immutable source readback, all ten sections, independent controls, proposal wording and file limits.
Runtime, native and original-app tests remain planned, without new execution evidence from this specification migration.
The original publishing session was GitHub-only and reported no local `just ci` or native run.
Documentation checks in the migration cannot qualify any OS behavior.

### E-1: Draft-source and structure evidence only
- **Run:** `git diff --check` and the trusted `spec-structure check` against a separate merge-source verification checkout.
- **n:** 1 GH-453 draft specification with 7 original criteria and 3 separated negative-control cases.
- **Environment:** macOS source worktree at PR #1432 `bba88b22f58b0de6763028a9f6a8ce62bc854a57`.
- **Limits:** Document structure and change hygiene only. No host election, kernel-lock or original-app acceptance is proved.

## 8. Review gates triggered

Public API and wire/compiled-schema owner review. Permission-boundary review for namespace admission; no new capability is
granted.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

One nonblocking kernel acquisition per host admission and synchronous reads thereafter. Measure only under a pinned fixture; no
startup improvement or hostile-process safety claim.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions, blockers & unknowns

### Q-1: Which compiled declaration and immutable session-fact schema can be admitted?
Obtain the actual KEL-96 and GH-452 owner decisions before implementing T-1 or T-3.
The historic schema-v1 wording alone does not approve the present schema.

### Q-2: Does the current private-lock code implement the approved namespace correction?
Compare the current owning source and actual two-process fixture with the published private-lock decision before T-2.

### Q-3: What must qualify on other platforms?
Keep Windows/Linux and hostile same-user replacement outside the macOS acceptance claim until separately specified.

### Q-4: How may GH-524 release and reacquisition update the current lock fact?
Joint approval must define host-confirmed release, the current-state mirror, synchronous methods and post-release reacquisition.
Until that extension qualifies, release/reacquire stay typed unsupported. A stale initial success must never be returned.

### B-1: Compiled declaration and session-fact owner approvals are open
- **Status:** open
- **Owner:** @0monish (GH-453 draft owner), with the KEL-96 and GH-452 contract owners.
- **Impact:** T-1 and T-3 cannot ship an unapproved declaration or facts contract.
- **Check:** Read the approved, pinned KEL-96 and GH-452 decision and record its accepted schema and cells.

### B-2: Current private-lock implementation and OS proof are unverified here
- **Status:** open
- **Owner:** @0monish (GH-453 tracker), coordinating the existing native lock owner.
- **Impact:** T-2 cannot claim stable custody, refusal or kernel release without current code and two-process evidence.
- **Check:** Compare the owning source with the public correction and run the synchronized contention/release and substitution cases.

### B-3: GH-524 release contract needs a separate joint amendment
- **Status:** open
- **Owner:** @0monish (GH-524 and GH-453 contract coordination).
- **Impact:** Post-release `hasSingleInstanceLock()` and reacquisition remain unsupported; initial verdict alone cannot answer them.
- **Check:** Read the jointly approved GH-524/current-state contract and its native negative controls.

### U-1: Windows/Linux election and hostile same-user replacement remain outside this proof
Obtain platform-specific lock identity, kernel lifetime and namespace-substitution acceptance from their owners before claiming support.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

# Spec: Host-authorized application exit and shell status
- Status: draft
- Tracker: GH-523
- Owner: @0monish (draft owner; approval pending)
- Updated: 2026-11-26
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

The original draft was dated 2026-11-06. GH-523 is the tracker of record under #517.
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Make app.exit(code) an intentional host-owned terminal action. The selected code reaches keld dev's invoking shell and no
successor role is created.

Non-goals: Cancelable app.quit, updater exit, relaunch and crash recovery policy changes beyond intentional exit classification
are separate.

## 2. Spec refs

- [Owning issue GH-523](https://github.com/gyldlab/keld/issues/523) and its latest public owner decisions.
- [Architecture
05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md):
native ownership, engine boundary and capability separation.
- [Current app
facade](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/app.ts).
- [Lifecycle
owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/lifecycle.rs).
- [Runtime
architecture](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/06-runtime-and-tooling.md).
- [Electron ExitWithCode
source](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/browser.cc).
- [Owner issue and CLI amendment](https://github.com/gyldlab/keld/issues/523).

Historical baseline at the pinned source: the app facade does not expose `app.exit`. Electron's pinned source skips cancelable quit callbacks,
destroys windows after loop readiness and has an earlier direct-exit branch. Current Keld CLI status behavior must be read in the
implementation task; the issue's older baseline is not proof of today's CLI.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria

### AC-1: An admitted ready-app exit is terminal and non-vetoable
- **Given:** A ready app has live windows and an authenticated main role.
- **When:** The host admits `app.exit(7)`.
- **Then:** No `before-quit` or `will-quit` fires. The host destroys windows. `quit` carries 7. The host exits 7 without a successor role.
- **Negative control:**
  - **Single fault:** Implement the app call using Bun `process.exit`.
  - **At-risk precondition:** The host supervisor would otherwise recover unexpected Bun role death.
  - **Expected RED:** A recovery generation appears, violating the terminal no-successor assertion.

### AC-2: The parent shell receives the chosen host code
- **Given:** A parent process launches the application under `keld dev`.
- **When:** The host intentionally exits with code 7.
- **Then:** The invoking parent sees exit status 7.
- **Negative control:**
  - **Single fault:** Map every nonzero host code to the CLI generic failure code.
  - **At-risk precondition:** The authenticated host has committed intentional exit code 7.
  - **Expected RED:** The parent observes the generic CLI failure status instead of 7.

### AC-3: An early exit uses the approved direct terminal branch
- **Given:** An app's host loop has not reached readiness.
- **When:** The host admits an authenticated early exit.
- **Then:** The approved early terminal route exits directly without synthesizing `quit` or Ready.
- **Negative control:**
  - **Single fault:** Reuse the after-ready event sequence before the loop starts.
  - **At-risk precondition:** An early exit is admitted while the native loop is not ready.
  - **Expected RED:** A `quit` or Ready event appears, or early termination cannot complete.

### AC-4: A stale or unauthenticated role cannot exit the host
- **Given:** A role's identity is stale or its request is unauthenticated.
- **When:** That role requests application exit.
- **Then:** No terminal decision commits. If the link is live, the host returns a typed refusal to the stale or unauthenticated caller without granting exit authority.
- **Negative control:**
  - **Single fault:** Authorize exit using caller-supplied role identity.
  - **At-risk precondition:** A stale or forged role supplies a privileged identity.
  - **Expected RED:** The host commits an unauthorized terminal decision.

### AC-5: Duplicate requests and role death cannot restart committed exit
- **Given:** The host has already latched an intentional exit and its validated code.
- **When:** A duplicate request arrives or the exiting role dies.
- **Then:** Cleanup stays idempotent and supervisor recovery never creates another role generation.
- **Negative control:**
  - **Single fault:** Clear the intentional-exit classification before child teardown.
  - **At-risk precondition:** A role dies after terminal exit is committed but before cleanup finishes.
  - **Expected RED:** The supervisor starts a recovery generation or repeats terminal cleanup.

### AC-6: Invalid codes cannot cause terminal side effects
- **Given:** An exit argument violates the approved signed integer contract.
- **When:** The app calls `exit` with that argument.
- **Then:** Validation rejects the call before the host commits terminal state or destroys windows.
- **Negative control:**
  - **Single fault:** Coerce `NaN` into an accepted exit code.
  - **At-risk precondition:** The app submits `NaN` or a fractional/out-of-range value.
  - **Expected RED:** The host accepts an invalid code and begins terminal teardown.

### AC-7: Exit and Quit commit only one terminal decision

- **Given:** An authenticated Exit(7) races a reversible GH-451 Quit preparation or its terminal Commit.
- **When:** The Host lifecycle owner orders the two admission operations.
- **Then:** Exit first invalidates the preparation and commits Exit(7); Quit first retains its committed cause and code.
  The later operation cannot change the winner, issue a second terminal receipt, emit a second quit event or repeat cleanup.
- **Negative control:**
  - **Single fault:** Allow Exit to overwrite a committed Quit code.
  - **At-risk precondition:** Quit committed before authenticated Exit(7) reached terminal admission.
  - **Expected RED:** The terminal receipt or actual process status changes from the Quit winner to 7.

Run the reverse admission order independently. Mutate acceptance of an invalidated Quit token after Exit wins; require RED if
that Commit succeeds or repeats terminal cleanup.

## 4. Design

### 4.1 Single terminal owner

The facade requests exit over the existing authenticated Main call path. The lifecycle owner atomically latches terminal reason
and validated code before teardown can kill a child. Supervisor recovery consumes that same reason, not a separate facade flag.
Ordinary crashes retain their existing classification.

GH-451 Prepare is reversible and does not own the terminal latch. Use one ordered Host admission boundary for Exit and the
GH-451 Commit. A validated, authorized Exit admitted first invalidates the active Quit preparation in that same decision;
subsequent Commit/Cancel for that token cannot change the winner. If Quit commits first, later Exit receives a typed
already-terminal refusal while a response path remains available. Do not invent a second successful terminal receipt.
Duplicate requests for the winning decision may report that same outcome, never a new code or terminal action. The committed
cause and code remain authoritative for native cleanup, supervisor classification and the actual process status. This proposed
cross-feature ordering requires joint GH-451/GH-523 and KEL-96 owner approval; it is not a current runtime contract.

### 4.2 API proposal

Proposed signature is exit(exitCode: number = 0): void with an integer accepted by the host's signed exit-code contract. POSIX
shell truncation and Windows behavior need explicit conformance rows; code 7 is the portable acceptance fixture. Reject
nonfinite/fractional/out-of-range input instead of allowing accidental coercion.

### 4.3 Ordering and failure

After readiness: terminal latch, non-vetoable native destruction, quit event if the current app link can receive it, ordered host
cleanup, process status. Link loss cannot cancel an already committed intentional exit. Before readiness the host uses the
approved early branch. If delivery outcome is unknown, do not replay exit into a successor generation.

### 4.4 Wire and CLI migration

Use the existing lifecycle request family and one generated schema allocation. KEL-96 terminal-code wording and the CLI
child-status mapping must change together under owner review. This draft does not select a numeric wire tag or treat a docs change
as that amendment. No process.exit shim is an acceptable adapter.

### 4.5 Runtime boundary

Host owns OS window destruction and process-family cleanup; Bun only expresses intent. Retirement invalidates callback/response
custody. No filesystem, URL or window permission is created by exit, and no second supervisor is introduced.

### 4.6 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future implementation spans Electron app, the existing authenticated API/lifecycle contract, Core supervisor and keld dev status
propagation. Do not change crash retry budgets, updater semantics or unrelated CLI errors.

This publication changes only `docs/specs/gh523-host-authorized-app-exit.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks

### T-1: Approve the exact terminal-code and conformance contracts
- **Blocked by:** B-1, B-2
- [ ] Approve the KEL-96, supervisor and CLI terminal-code amendment.
- [ ] Record pinned source-backed conformance cells and the ready/early branch split.

### T-2: Commit one authenticated intentional-exit decision
- **Blocked by:** T-1
- [ ] Implement validated host exit admission and one atomic terminal latch.
- [ ] Test duplicate requests, stale identities and terminal-vs-crash classification.
- [ ] Test both GH-451 Commit/Exit admission orders with a reversible preparation and one terminal receipt, event and status.

### T-3: Connect native exit paths and parent status propagation
- **Blocked by:** T-2, B-2
- [ ] Connect ready and early native termination branches.
- [ ] Propagate the selected code through the owned CLI contract in the same vertical slice.

### T-4: Qualify host status and original application exit paths
- **Blocked by:** T-3, B-3
- [ ] Run separate ready, early, invalid-code and role-death parent-process fixtures.
- [ ] Exercise original app exit sites and preserve normal crash-recovery positive controls.

## 7. Test plan & evidence

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-523;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Use a parent process that records actual child exit status and role-generation events. Tests must distinguish a real host exit
from a mock response or forced harness kill. Assert cleanup order without arbitrary sleep and keep a normal crash-recovery
positive control. Use explicit barriers to order Exit admission against Quit Commit, then check one terminal decision,
receipt, event and actual process status in both orders; also verify rejected invalid Exit leaves the reversible Quit unchanged.

Documentation validation covers immutable source readback, all ten sections, independent controls, proposal wording and file limits.
Runtime, native and original-app tests remain planned, without new execution evidence from this specification migration.
The original publishing session was GitHub-only and reported no local `just ci` or native run.
Documentation checks in the migration cannot qualify any OS behavior.

### E-1: Draft-source and structure evidence only
- **Run:** `git diff --check` and the trusted `spec-structure check` against a separate merge-source verification checkout.
- **n:** 1 GH-523 draft specification with 6 original acceptance criteria.
- **Environment:** macOS source worktree at PR #1432 `bba88b22f58b0de6763028a9f6a8ce62bc854a57`.
- **Limits:** Document structure and change hygiene only. No native exit, event ordering or CLI status acceptance is proved.

## 8. Review gates triggered

Public API, wire protocol and lifecycle/CLI owner approval; architecture amendment required before implementation.
Joint GH-451/GH-523 approval must cover the shared terminal winner, receipt and status rule in section 4.1.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

Terminal-only path; no intended steady-state/startup cost. Bound any final event/cleanup wait by the existing ordered terminal
policy; do not introduce a new independent timeout.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions, blockers & unknowns

### Q-1: What signed code range and per-platform shell normalization can be approved?
Record valid host integer input, POSIX shell truncation and Windows semantics. Code 7 remains the portable fixture.

### Q-2: How must ready and early exit behave if the app link is already dead?
Approve the two event paths and host failure disposition. Do not replay an uncertain exit into a successor role.

### Q-3: Which generated exit Call allocation and CLI contract wording are approved?
Use the existing lifecycle family. Allocate one generated schema entry and approve the exact CLI status rule before implementation.

### B-1: Lifecycle and recovery-ownership amendment is not approved by this draft
- **Status:** open
- **Owner:** @0monish (GH-523 draft owner), coordinating KEL-75 and KEL-96 lifecycle owners.
- **Impact:** T-1 cannot authorize new exit semantics or relabel unexpected role death without a named owner decision.
- **Check:** Read the approved, pinned supervisor and KEL-96 intentional-exit amendment, with both early and ready cases.

### B-2: Code range, generated schema and CLI status mapping require approval
- **Status:** open
- **Owner:** @0monish (GH-523 tracker), coordinating the existing CLI and generated-contract owners.
- **Impact:** T-1 and T-3 cannot deliver a new exit Call or claim parent status propagation.
- **Check:** Verify a pinned owner decision for the code range, generated allocation and revised `keld dev` status wording.

### B-3: Platform and original-app terminal acceptance has not run for this draft
- **Status:** open
- **Owner:** @0monish (GH-523 tracker), with the macOS native and original-app owners.
- **Impact:** T-4 cannot claim actual ordered exit, process status or no recovery from a documentation result.
- **Check:** Run the real parent-process early/ready fixtures and original-app sites on the selected pinned macOS build.

### U-1: Current CLI implementation and other-platform exit normalization need a fresh pin
Inspect current code and source-backed OS fixtures before adopting the historic KEL-96 and CLI assumptions.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

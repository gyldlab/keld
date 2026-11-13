# Spec: Host-authorized application exit and shell status
Status: draft
Linear: GH-523 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
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

Verified baseline: The main app facade does not expose app.exit. Electron's pinned source skips cancelable quit callbacks,
destroys windows after loop readiness and has an earlier direct-exit branch. Current Keld CLI status behavior must be read in the
implementation task; the issue's older baseline is not proof of today's CLI.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given a ready app with windows, when app.exit(7) is admitted, then no before-quit or will-quit fires, windows are destroyed, quit
carries 7 and the host exits 7 without a successor generation.

Independent negative control: Implement the call using Bun process.exit; the recovery-generation assertion must fail.

### AC2

Given keld dev launched by a parent process, when the host intentionally exits 7, then the parent observes status 7.

Independent negative control: Map every nonzero host code to the CLI's generic failure code.

### AC3

Given an app before the host loop is ready, when an admitted exit occurs, then it takes the explicitly approved early terminal
route without synthesizing quit or Ready.

Independent negative control: Reuse the after-ready event sequence before the loop is running.

### AC4

Given a stale or unauthenticated role, when it requests exit, then no terminal decision is committed and a typed refusal is
returned if the link is live.

Independent negative control: Authorize by caller-supplied role or window identity.

### AC5

Given a terminal exit is already committed, when duplicate delivery or role death occurs, then cleanup remains idempotent and the
supervisor never treats that death as recoverable.

Independent negative control: Clear the intentional-exit classification before child teardown.

### AC6

Given an invalid code under the approved integer contract, when called, then validation fails before terminal side effects.

Independent negative control: Coerce NaN or a fractional value to a successful terminal code.

## 4. Design

### 4.1 Single terminal owner

The facade requests exit over the existing authenticated Main call path. The lifecycle owner atomically latches terminal reason
and validated code before teardown can kill a child. Supervisor recovery consumes that same reason, not a separate facade flag.
Ordinary crashes retain their existing classification.

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

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Approve the KEL-96/supervisor/CLI terminal-code contract and record pending source-backed exit cells.

### T2

- [ ] Implement validated intentional-exit admission and one atomic terminal latch with duplicate/stale request controls.

### T3

- [ ] Connect ready and early native terminal paths plus CLI propagation in the same vertical slice.

### T4

- [ ] Run separate ready/early/invalid/child-death parent-process fixtures and original app exit sites; retain crash recovery
controls.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-523;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Use a parent process that records actual child exit status and role-generation events. Tests must distinguish a real host exit
from a mock response or forced harness kill. Assert cleanup order without arbitrary sleep and keep a normal crash-recovery
positive control.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API, wire protocol and lifecycle/CLI owner approval; architecture amendment required before implementation.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

Terminal-only path; no intended steady-state/startup cost. Bound any final event/cleanup wait by the existing ordered terminal
policy; do not introduce a new independent timeout.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Approve exact code range and per-platform shell normalization.
2. Approve ready/early event delivery and failure disposition where the app link has already died.
3. Approve the single schema allocation and CLI contract wording.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.


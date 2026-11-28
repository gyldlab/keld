# Spec: Named-role child-process-gone events
Status: draft
Linear: GH-578 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Emit one truthful child-process-gone event for qualified crash/kill-class named-role termination, using supervisor facts and
preserving clean exit and primary/renderer separation.

Non-goals: Bun-primary, renderer and hidden engine helper events are excluded. utilityProcess exit and recovery policy stay with
their existing owners. Implementation remains parked.

## 2. Spec refs

- [Owning issue GH-578](https://github.com/gyldlab/keld/issues/578) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Runtime architecture](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/06-runtime-and-tooling.md).
- [App event facade](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/app.ts).
- [Lifecycle owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/lifecycle.rs).
- [Electron child-process-gone contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/app.md).
- [Named-role event issue](https://github.com/gyldlab/keld/issues/578).

Verified baseline: The issue requires real terminating-signal/classification facts and forbids primary or renderer emission. Its
old claim that the signal owner is missing is historical; implementation must identify today's supervisor record before allocating
another observer.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given an admitted named role terminated with a recorded SIGKILL, when the owning supervisor records its terminal result, then
exactly one event names that role and the approved killed reason.

Independent negative control: Infer the reason from socket closure before wait status is known.

### AC2

Given a named role exits with status zero, when observed, then no child-process-gone is emitted and the existing utility-process
exit path remains responsible.

Independent negative control: Emit gone for every child exit.

### AC3

Given the Bun primary or renderer dies, when existing owners handle it, then this facade emits no named-role Utility row.

Independent negative control: Classify every supervised or engine process as a Utility role.

### AC4

Given duplicate observation or a role-name reuse in a new generation, when records arrive, then deduplication uses the original
lifetime and does not suppress or misattribute the new role.

Independent negative control: Deduplicate forever by role name or PID alone.

### AC5

Given a signal/exit cause cannot be determined, when evidence is produced, then the event remains unsupported or explicitly
unknown under the approved mapping, without fabricated crashed/killed detail.

Independent negative control: Map every nonzero code to crashed.

### AC6

Given intentional host shutdown, when it kills a named role, then the explicitly approved shutdown classification applies and no
event policy is inferred from SIGKILL alone.

Independent negative control: Treat the supervisor's intentional teardown as an unexplained application crash.

## 4. Design

### 4.1 Existing observer only

The current supervisor/process owner already owns reap and terminal classification. Consume its stable role/lifetime facts; do not
add a second waitpid/handle observer or replace cleanup. Proposed event contains existing role identity, Utility type and a reason
derived from qualified cause.

### 4.2 Truth before delivery

Loss of a link is not proof of a signal. Deliver only after the terminal record identifies the class. Unknown outcomes stay
unknown/unsupported and must not be converted into an apparently precise crash reason. Separate intended supervisor shutdown from
unexpected death in the approved mapping.

### 4.3 Generation and event lifetime

Use one terminal event latch attached to the actual role lifetime. Retire it with that record; reuse of a role name or PID starts
a new lifetime. The current app event owner receives only valid in-session facts. No automatic event replay into a successor app
generation.

### 4.4 Migration

Extend the existing generated lifecycle event contract only after its owner approves the shape. Keep named-role utilityProcess
exit and the primary recovery owner intact. macOS/Linux signal evidence does not qualify Windows exit-code causes.

### 4.5 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future changes stay in supervisor-fact projection, existing lifecycle event schema and app facade. No new signal handler, reap
loop, process grant or native recovery behavior.

This publication changes only `docs/specs/gh578-named-role-exit-events.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Identify and pin the current terminal-record owner; approve reason mapping including intentional shutdown and Windows
limitations.

### T2

- [ ] Implement projection/deduplication through the existing generated app event route with lifetime controls.

### T3

- [ ] Run named-role signal, clean exit, duplicate, primary, renderer and shutdown fixtures; publish only actually observed cause
classes.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-578;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Use real child wait/exit receipts and distinguish signal death from normal nonzero exit. Synchronize observation and role-name
reuse. A fabricated terminal enum or killed test process without its owner receipt cannot establish event truth.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API, wire protocol and supervisor/lifecycle owner approval.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

One event and bounded latch per terminal named-role lifetime; no polling or independent process census.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Approve current supervisor cause representation and exact event projection.
2. Approve intentional-shutdown and unknown-cause behavior before enabling the event.
3. Keep Windows cause qualification and parked scheduling explicit.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.


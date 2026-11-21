# Spec: Window close across a parked or recovered app role
Status: draft
Linear: GH-529 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Keep one pending close safe while Bun is parked in a blocking call, then retire it exactly once if that role is lost while
preserving the same native window for qualified recovery.

Non-goals: Forced-close user interface, preempting an OS call and modal-sheet recovery remain separate. This contract does not
make a wedged native call finish.

## 2. Spec refs

- [Owning issue GH-529](https://github.com/gyldlab/keld/issues/529) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Window close state machine](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh531-window-registry-close-state-machine.md).
- [Worker-owned transport](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh527-worker-owned-blocking-call-transport.md).
- [Recovery owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel143-macos-generation-filesystem-recovery.md).
- [Registry implementation](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/app_session/window_registry.rs).
- [Close recovery owner issue](https://github.com/gyldlab/keld/issues/529).

Verified baseline: The issue's old KEL-143 Backlog/no-artifact wording is stale: an approved recovery spec exists on main. That
spec still distinguishes caller rejection from actual native retirement and says a wedged synchronous operation can stall
recovery. Its product acceptance is not established by this draft.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given Bun is parked, when the native close button is used, then the same window remains open and its single close event is
delivered in existing transport sequence after unpark.

Independent negative control: Drop the close event while the facade thread is parked.

### AC2

Given one unanswered close request, when the user repeats native close, then the request is not duplicated and one bounded
diagnostic is emitted for that pending attempt.

Independent negative control: Emit a diagnostic or enqueue a new request on every repeated click.

### AC3

Given recoverable role loss with a pending close, when retirement runs, then that request has one terminal cancellation
disposition, the same native window survives and the successor receives no old CloseRequested.

Independent negative control: Replay the request into the new role or destroy the window on recoverable loss.

### AC4

Given an old reply arrives after retirement, when the new role is authenticated, then the old reply cannot close or veto a new
request.

Independent negative control: Compare only window identity and omit role/request correlation.

### AC5

Given native work has not returned, when the caller deadline expires, then its local request terminates once but recovery does not
claim native quiescence or provision a successor early.

Independent negative control: Use local timeout as proof the old native call and handles are gone.

### AC6

Given fatal session termination rather than recoverable loss, when cleanup runs, then the existing terminal owner releases the
window and request once without applying the recoverable keep-window rule.

Independent negative control: Keep windows alive after the session has selected fatal termination.

## 4. Design

### 4.1 Reuse three existing owners

GH531 owns the pending close state and native window identity. GH527's worker owns the single authenticated link and ordered
backlog while the app thread is parked. KEL-143 owns retirement and successor admission. This child connects their dispositions;
it introduces no second pump, worker, registry or deadline.

### 4.2 Pending record

Retain the current close correlation together with its authenticated role generation and host window identity. Only one is pending
per window. Repeated native requests are suppressed against that record, with one diagnostic bit scoped to its lifetime. A valid
reply or selected terminal cancellation consumes the record exactly once.

### 4.3 Park and failure

Being parked is not failure and does not authorize native destruction. Keep the close in the existing bounded transport/admission
capacity; overflow must use the existing typed backpressure disposition rather than dropping or allocating unbounded memory. Do
not replay a stale request after the generation boundary.

### 4.4 Recovery seam

On recoverable loss, revoke old dispatch and terminalize the old close locally before fresh role admission. Preserve native
window/document ownership. Native call completion and actual old-broker destruction remain KEL-143 prerequisites, even if the
caller has already received rejection. The successor may receive a newly initiated user close, not the retired one.

### 4.5 Migration

Extend the existing registry terminal-reason handling and tests in place. Consume transport/lifecycle events through their current
schema owners. If no new wire field is required, keep wire bytes unchanged; any required addition must be jointly reviewed, not
silently allocated here.

### 4.6 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future implementation stays in Core window_registry, existing link-worker delivery and existing recovery integration. Do not
shorten timeout policy, add force-close semantics or bypass native retirement.

This publication changes only `docs/specs/gh529-close-across-park-recovery.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Reconcile the approved GH531/GH527/KEL143 current contracts and add pending-close disposition tests.

### T2

- [ ] Implement ordered close delivery across park plus repeated-request diagnostic suppression in the existing owners.

### T3

- [ ] Implement generation retirement/stale-reply refusal while retaining the same window, with fatal-versus-recoverable controls.

### T4

- [ ] Run real blocking-call and recovery fixtures, recording window identity, link generation, pending outcome and actual native
retirement.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-529;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Use explicit park/unpark/native-return barriers. A local cancellation receipt and native completion are separate assertions.
Exercise close-before-park, during-park, after-unpark and retirement races, then one new close after successor admission. Reuse
existing deadline fixtures instead of inventing timing sleeps.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Lifecycle/transport owner review; wire gate only if the concrete integration changes the generated contract. No new grant or
authority is proposed.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

At most one pending record plus one diagnostic bit per registered window, within existing transport limits. Measure bounded
backlog pressure; no claim that blocked OS work becomes preemptible.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Approve the exact terminal disposition mapping against current GH531/GH527 records.
2. Confirm the qualified KEL143 implementation and same-window acceptance source before enabling this integration.
3. Resolve any required schema amendment with its existing owner; do not assume a new field is necessary.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.


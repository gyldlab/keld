# Spec: Instance, file and URL handoff ordering
Status: draft
Linear: GH-524 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Deliver bounded second-instance and native open-file/open-url inputs through the existing app lifecycle while treating all
supplied paths, arguments and data as untrusted.

Non-goals: GH453 owns acquisition and verdict. Association registration and packaged URL/document declarations remain separate.
This draft does not promise durable exactly-once delivery across a host crash.

## 2. Spec refs

- [Owning issue GH-524](https://github.com/gyldlab/keld/issues/524) and its latest public owner decisions.
- [Architecture
05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md):
native ownership, engine boundary and capability separation.
- [App event
owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/app.ts).
- [Boot
owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/app_session/boot_contract.rs).
- [Native
ownership](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md).
- [Electron app events](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/app.md).
- [Handoff owner issue](https://github.com/gyldlab/keld/issues/524).

Verified baseline: The public issue requests after-ready second-instance delivery and pre-ready file/URL queueing. The current
facade uses a shared app event owner; this draft extends that owner rather than making a new event bus. Historical 16 MiB
transport wording must be reconciled with the current encoded envelope and aggregate queue budget.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given a losing declared instance during primary startup, when its lock-request call admits a handoff, then the primary emits one
second-instance event only after ready, with the admitted argv/cwd/additionalData.

Independent negative control: Flush the queue before Ready or dispatch the same admitted record twice.

### AC2

Given an open-file intent before readiness, when the app registers its listener inside will-finish-launching, then that listener
receives the intent before ready.

Independent negative control: Flush the file queue before will-finish-launching listeners finish registering.

### AC3

Given a path in argv or additionalData with no existing grant, when app code tries to read it, then the guard refuses.

Independent negative control: Convert every delivered pathname into a host read grant.

### AC4

Given an over-budget handoff or a full queue, when admission occurs, then the loser keeps its false lock verdict and the primary
emits no partial event; the refusal is diagnosed.

Independent negative control: Account only one field or enqueue a prefix before validating the full message.

### AC5

Given duplicate delivery within one live generation, when the same admitted record is seen twice, then the facade dispatches once
and duplicate state is reclaimed with that generation.

Independent negative control: Keep unbounded duplicate IDs or clear them before dispatch completes.

### AC6

Given a role generation retires with queued records, when a successor connects, then the selected no-replay contract retires old
records and reports their disposition without granting authority.

Independent negative control: Replay old untrusted user-intent grants into a successor.

### AC7

Given releaseSingleInstanceLock on the current holder, when the host confirms release, then hasSingleInstanceLock becomes false
and a later contender can acquire; nonholders cannot release someone else's lease.

Independent negative control: Mutate only a facade bool while the host retains the lock.

## 4. Design

### 4.1 One admitted queue owner

Host owns OS input capture, authentication and bounded queue admission. Reuse the existing app link/event applier and Ready owner.
The facade connects on the approved first-touch path without faking Ready at import. Separate queues/order barriers are logical
states in that owner, not separate transports.

### 4.2 Proposed bounds

For review, cap queued records at 64 and aggregate encoded payload at 64 KiB per primary generation, with each record also subject
to the existing transport/frame and codec limits. Admit all fields atomically before enqueue. These are proposed product limits,
not measured safe capacities; owner approval and pressure tests are required.

### 4.3 Trust and grant lifetime

argv, cwd, URLs and additionalData remain data. A verified native user-intent source may request the existing one-shot grant flow;
merely receiving a pathname never mints a grant. Identity, scope, expiry and consumption are determined by the existing guard
owner. Sanitize diagnostics so raw arguments or local paths are not logged by default.

### 4.4 Delivery guarantee

Exactly once is scoped to an admitted record in a surviving authenticated host/role generation. Mark dispatched through the single
event owner, not a second JS cache. Crash/reconnect ambiguity has an explicit dropped/unknown disposition and no automatic replay.
Durable delivery would require a separate reviewed protocol; it is not implied by this API.

### 4.5 Release and schema

Release is an authenticated host operation on the existing lock custody, not unlinking a pathname. Update the session mirror only
from host confirmation. Extend the owning generated lifecycle schema, reuse current framing, and preserve GH453's false-verdict
behavior for losing instances even when notification admission fails.

### 4.6 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future implementation spans existing Core instance/native intent admission, app lifecycle delivery and facade. Do not add a grant
shortcut, new IPC transport, arbitrary URL launcher or protocol-registration implementation.

This publication changes only `docs/specs/gh524-instance-file-url-handoff.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Approve queue budgets, ordering and generation-scoped delivery wording; record source-backed pending conformance rows.

### T2

- [ ] Add host admission and existing-link delivery with aggregate budgets, deduplication and retirement in one slice.

### T3

- [ ] Wire will-finish-launching/Ready ordering and authenticated release into the existing app/facts owner.

### T4

- [ ] Run two-process startup races, Finder-style file/URL fixtures, oversize/duplicate/stale controls and unauthorized read
negatives.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-524;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Drive explicit barriers for primary boot, listener registration and contender admission. Record host admission ID and role
generation without logging raw sensitive payload. Native intent provenance requires an actual OS-source fixture; injecting a
facade event proves only the consumer.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Permission model, wire protocol and public API; GH453/GH446 owner alignment.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

Bound queue memory by both bytes and item count, including bookkeeping. Measure worst-case drain latency at the approved caps; no
latency or durable-delivery claim is made.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Approve the proposed 64-record/64-KiB limits or replace them with receipted bounds.
2. Approve explicit dropped/unknown behavior across role or host loss and the user's retry surface.
3. Approve native user-intent provenance, lifecycle schema and mirror release ordering.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.


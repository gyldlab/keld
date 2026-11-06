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
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [App event owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/app.ts).
- [Boot owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/app_session/boot_contract.rs).
- [Native ownership](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md).
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

Given separate native open-file and open-url intents before readiness, when the app registers each listener inside
will-finish-launching, then each intended listener receives its corresponding payload after registration and before ready.

Independent negative control: Flush either queue before will-finish-launching callbacks finish; run file and URL cases separately
through actual native-source admission.

### AC3

Given a path in argv or additionalData with no existing grant, when app code tries to read it, then the guard refuses.

Independent negative control: Convert every delivered pathname into a host read grant.

### AC4

Given an over-budget handoff or a full queue, when admission occurs, then the loser keeps its false lock verdict and the primary
emits no partial event; the refusal is diagnosed.

Independent negative control: Account only one field or enqueue a prefix before validating the full message.

### AC5

Given more than 64 sequential admitted-and-drained records on one authenticated ingress lifetime, when its first record is
replayed, then duplicate suppression still prevents redispatch while retained bookkeeping stays bounded under the approved
high-water contract.

Independent negative control: Forget old dispatched IDs without a watermark or keep a generation-long unbounded ID set; delivery
and high-water census assertions fail independently.

### AC6

Given a G1 queue with a unique payload nonce and no user-intent grant, when G1 retires and G2 connects, then G2 receives zero
callbacks carrying that nonce and the owner records a dropped/unknown G1 disposition.

Independent negative control: Omit only queue retirement and deliver the old data into G2. A separate grant-lifetime test checks
authority; absence of a grant cannot mask replayed callback data.

### AC7

Given the current holder, when a qualified release operation completes, then the host has released custody, hasSingleInstanceLock
is false and a separate contender acquires. A subsequent request cannot return stale true from the initial election fact; it
follows the separately approved reacquisition contract or typed unsupported behavior.

Independent negative control: Mutate only a facade bool, keep the native lock, or keep returning the initial true fact after
release; test each independently.

### AC8

Given a wrong-app peer, stale elected-holder endpoint or foreign/unqualified sender, when it submits a handoff, then it is
rejected before queue insertion and the losing instance's lock verdict remains false.

Independent negative control: Treat knowledge of the primary app-link address or a payload app ID as peer/holder authentication. A
qualified sender positive must still enqueue through the same ingress.

## 4. Design

### 4.1 One admitted queue owner

Host owns OS input capture and bounded queue admission. Reuse the primary's existing app-link/event applier only for elected Host
to its own Bun delivery. It is not a losing-Host to elected-Host service. Main has no qualified instance ingress. Before
implementation, the single-instance/IPC owners must approve an exact composition using existing primitives: lock-bound endpoint
custody, app/peer/holder identity, bounded admission acknowledgement and retirement. Wrong-app/stale-holder/foreign-peer inputs
are refused before enqueue. No new transport is silently selected here.

### 4.2 Proposed bounds

For review, cap queued records at 64 and aggregate encoded payload at 64 KiB per primary generation, with each record also subject
to the existing transport/frame and codec limits. Admit all fields atomically before enqueue. These are proposed product limits,
not measured safe capacities; owner approval and pressure tests are required.

### 4.3 Trust and grant lifetime

argv, cwd, URLs and additionalData remain data. A verified native user-intent source may request the existing one-shot grant flow;
merely receiving a pathname never mints a grant. Identity, scope, expiry and consumption are determined by the existing guard
owner. Sanitize diagnostics so raw arguments or local paths are not logged by default.

### 4.4 Delivery guarantee

Exactly once is scoped to an admitted record in a surviving authenticated ingress/host/role lifetime. Proposed bounded duplicate
protocol: an ordered sequence and one high-water value per bounded live ingress identity, owned with that identity. A sequence at
or below the watermark is a duplicate; gaps/overflow follow the approved refusal policy. Reclaim a watermark only after that
ingress identity can no longer authenticate, so old packets cannot become new. Approve the live-peer cap and sequence
width/retirement with the ingress owner; no unbounded ID set or eviction that admits old replay. Crash ambiguity stays
dropped/unknown without automatic replay.

### 4.5 Release and schema

Release is an authenticated operation on existing Host lock custody, never pathname unlink. It requires a joint GH453/GH452
amendment separating immutable initial election from current lease state. Electron's release/query behavior is synchronous:
preserve it through an approved existing transport/state barrier, or keep release unsupported until qualified; never return
completion while still reporting stale custody. Define reacquisition after release explicitly. Extend only the owning generated
schema, and preserve the losing instance's false verdict even when handoff admission fails.

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

- [ ] Approve the inter-Host ingress/identity/acknowledgement/retirement composition, bounded ordered duplicate protocol, queue
budgets and lifecycle ordering; record source-backed pending rows.

### T2

- [ ] Add host admission and existing-link delivery with aggregate budgets, deduplication and retirement in one slice.

### T3

- [ ] Wire will-finish-launching/Ready ordering for both native file and URL sources, and the jointly approved synchronous
release/current-state/reacquisition contract into existing app/facts owners.

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
4. Approve the missing losing-Host to elected-Host ingress using existing IPC primitives, including
wrong-app/foreign-peer/stale-holder authentication and acknowledgement semantics.
5. Approve bounded ingress identity/high-water retention and pressure/replay behavior; 64 queued records does not bound
generation-long duplicate history by itself.
6. Jointly approve GH453 initial/current lock-state distinction, synchronous release completion and post-release reacquisition
without stale success.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.


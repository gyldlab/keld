# Spec: Instance, file and URL handoff ordering
- Status: draft
- Tracker: GH-524
- Owner: @0monish (approval pending)
- Updated: 2026-11-21
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack (#517); docs only, do not merge.

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

## 3. Acceptance criteria

### AC-1: Deliver one second-instance event after Ready

- **Given:** A losing declared instance during primary startup.
- **When:** Its lock-request call admits a handoff.
- **Then:** The primary emits one second-instance event only after Ready with the admitted argv, cwd and additionalData.
- **Negative control:** Flush the admitted queue before Ready.
- **Single fault:** Remove the Ready barrier for one admitted record.
- **At-risk precondition:** The primary admits a handoff but has not reached Ready.
- **Expected RED:** The second-instance listener fires before Ready.

The duplicate-dispatch negative also remains required. Replaying the same admitted record must not emit a second callback.

### AC-2: Deliver native file and URL inputs after listener registration

- **Given:** Separate native open-file and open-url intents arrive before readiness.
- **When:** The app registers each listener inside will-finish-launching.
- **Then:** Each listener receives its corresponding payload after registration and before Ready.
- **Negative control:** Flush the file queue before will-finish-launching callbacks finish.
- **Single fault:** Remove the file listener-registration barrier.
- **At-risk precondition:** A real native file intent is queued while its listener is being registered.
- **Expected RED:** The file callback is lost or arrives before its listener is registered.

Run a separate URL negative with the same listener-order requirement through actual native-source admission.

### AC-3: Keep received paths outside the grant authority

- **Given:** A path in argv or additionalData has no existing grant.
- **When:** App code attempts to read that path.
- **Then:** The guard refuses the read.
- **Negative control:** Convert a delivered pathname into a host read grant.
- **Single fault:** Grant read authority on receipt of one untrusted pathname.
- **At-risk precondition:** The handoff supplies a pathname with no permitted file-read capability.
- **Expected RED:** The guard admits the unauthorized read.

### AC-4: Refuse an oversized or full-queue handoff atomically

- **Given:** A handoff exceeds the approved budget or the queue is full.
- **When:** The losing instance requests admission.
- **Then:** Its lock verdict stays false, the primary emits no partial event, and the refusal is diagnosed.
- **Negative control:** Account for only one payload field when checking the budget.
- **Single fault:** Exclude additionalData from the aggregate admission calculation.
- **At-risk precondition:** The complete encoded record exceeds the cap while one field fits.
- **Expected RED:** Admission exceeds the cap or exposes a partial event.

Also mutate prefix enqueue before full validation. The atomic-admission assertion must fail independently.

### AC-5: Retain bounded duplicate protection after long ingress activity

- **Given:** One authenticated ingress has admitted and drained more than 64 sequential records.
- **When:** Its first record is replayed during the same ingress lifetime.
- **Then:** Duplicate suppression prevents redispatch, and high-water bookkeeping stays within its approved bound.
- **Negative control:** Forget old dispatched IDs without a retained watermark.
- **Single fault:** Evict the ingress watermark while that ingress can still authenticate.
- **At-risk precondition:** The first record has aged beyond the 64-record queue, but the peer remains valid.
- **Expected RED:** The first record redispatches instead of being rejected as a duplicate.

An independent census must also reject a generation-long unbounded ID set, even if duplicate delivery stays suppressed.

### AC-6: Retire queued data with its generation

- **Given:** A G1 queue holds a unique payload nonce without a user-intent grant.
- **When:** G1 retires and G2 connects.
- **Then:** G2 receives zero callbacks with that nonce, and the owner records a dropped/unknown G1 disposition.
- **Negative control:** Omit only queue retirement and deliver old data into G2.
- **Single fault:** Keep one G1 queue entry eligible for delivery after G1 retires.
- **At-risk precondition:** G1 has a pending nonce at the point G2 becomes current.
- **Expected RED:** A G2 listener receives the G1 nonce.

Test grant lifetime separately. A missing read grant must not hide a replayed data callback.

### AC-7: Release actual lock custody and report current state

- **Given:** The current holder owns the native single-instance lock.
- **When:** A qualified release operation completes.
- **Then:** The Host has released custody, hasSingleInstanceLock is false, and a separate contender acquires.
- **Negative control:** Change only the facade lock boolean while native custody remains held.
- **Single fault:** Clear the facade boolean without releasing the native lock.
- **At-risk precondition:** A contender tries to acquire after the release call reports completion.
- **Expected RED:** The contender cannot acquire despite the reported release.

A subsequent request cannot return stale true from the initial election fact. It follows the separately approved reacquisition
contract or typed unsupported behavior. Test the stale initial-true return and the retained native lock independently.

### AC-8: Authenticate cross-Host handoff before queue insertion

- **Given:** A wrong-app peer, stale elected-holder endpoint or foreign/unqualified sender submits a handoff.
- **When:** The elected Host authenticates the submission.
- **Then:** It rejects the request before queue insertion, and the losing instance's lock verdict stays false.
- **Negative control:** Treat a payload app ID or knowledge of the primary app-link address as authentication.
- **Single fault:** Authorize the wrong-app sender using only its claimed app ID.
- **At-risk precondition:** An unqualified sender knows the primary address and a plausible app ID.
- **Expected RED:** An unauthorized record reaches the queue or the intended event listener.

A qualified sender must still enqueue through that same ingress in an independent positive test.

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

## 6. Tasks

### T-1: Set ingress, queue and lifecycle contracts

- **Blocked by:** none

- [ ] Approve the inter-Host ingress/identity/acknowledgement/retirement composition, bounded ordered duplicate protocol, queue
budgets and lifecycle ordering; record source-backed pending rows.

### T-2: Implement bounded host admission and delivery

- **Blocked by:** T-1

- [ ] Add host admission and existing-link delivery with aggregate budgets, deduplication and retirement in one slice.

### T-3: Implement ordered native inputs and release semantics

- **Blocked by:** T-1, T-2

- [ ] Wire will-finish-launching/Ready ordering for both native file and URL sources, and the jointly approved synchronous
release/current-state/reacquisition contract into existing app/facts owners.

### T-4: Qualify native and hostile-input acceptance

- **Blocked by:** T-2, T-3

- [ ] Run two-process startup races, Finder-style file/URL fixtures, oversize/duplicate/stale controls and unauthorized read
negatives.

## 7. Test plan & evidence

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

### E-1: Source baseline inspection

- **Run:** `git show 0eb3ce203187f35c5276a4237189c906a5220d5a:packages/@keld/electron/src/app.ts` and inspect app event ownership.
- **n:** 1 pinned source revision.
- **Environment:** Git repository with source pin `0eb3ce203187f35c5276a4237189c906a5220d5a`; documentation review only.
- **Limits:** Source inspection does not qualify native input, inter-Host ingress, grant isolation or exact-once behavior.

### E-2: Planned GH-524 runtime and OS acceptance

- **Run:** Not executed; run the separate AC-1 through AC-8 positive and negative fixtures during implementation.
- **n:** 0 native acceptance runs.
- **Environment:** Exact implementing revision and qualified native host/device must be recorded at execution.
- **Limits:** No acceptance result is claimed for second-instance, file/URL ordering, release, replay or authenticated ingress.

## 8. Review gates triggered

Permission model, wire protocol and public API; GH453/GH446 owner alignment.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

Bound queue memory by both bytes and item count, including bookkeeping. Measure worst-case drain latency at the approved caps; no
latency or durable-delivery claim is made.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions, blockers & unknowns

### Q-1: Queue limits

Approve the proposed 64-record/64-KiB limits or replace them with receipted bounds.

### Q-2: Role-loss disposition

Approve explicit dropped/unknown behavior across role or host loss and the user's retry surface.

### Q-3: Native intent and release ordering

Approve native user-intent provenance, lifecycle schema and mirror release ordering.

### Q-4: Cross-Host ingress

Approve the missing losing-Host to elected-Host ingress using existing IPC primitives, including
wrong-app/foreign-peer/stale-holder authentication and acknowledgement semantics.

### Q-5: High-water retention

Approve bounded ingress identity/high-water retention and pressure/replay behavior; 64 queued records does not bound
generation-long duplicate history by itself.

### Q-6: Lock state and reacquisition

Jointly approve GH453 initial/current lock-state distinction, synchronous release completion and post-release reacquisition
without stale success.

### U-1: Real native acceptance remains unverified

The implementing commit and qualified OS device have no recorded acceptance result in this draft. E-2 remains unrun.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

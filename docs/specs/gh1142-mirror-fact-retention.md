# Spec: retain the applied mirror fact until its ring record is dispatched
Status: draft
Linear: GH-1142 (#517), child of GH-449 · Owner: @amishabenramani · Updated: 2026-10-24

This is an exact-content amendment proposal, not implementation or approval.
The approved GH-531 and GH-527 contracts remain operative until exact-content
owner approval; this draft alone authorizes no mirror storage or production consumer.

## 1. Goal & non-goals

Preserve the generic mirror's terminal-on-source-failure contract without decoding
one event twice. The proposed helper retains the exact successfully applied fact
until the existing ring dispatcher delivers that record. This explicitly corrects
the current claim that all undispatched-listener memory stays inside the ring.

Non-goals: a production window consumer, BrowserWindow, channel allocation,
subscription/readiness changes, transport callbacks/hooks/public exports, new
dependencies, native qualification, or a performance/memory-improvement claim.

## 2. Spec refs

- [GH-531 §4.d, approved carrier bb43e474](https://github.com/gyldlab/keld/blob/bb43e474577297f0792740676994aa153e5df1dd/docs/specs/gh531-window-registry-close-state-machine.md#4d-mirror-primitive),
  file SHA-256 `dd2c08bd70e859cebabbc93cb7c491d3afcb8bcc890e333ea013adec7456975e`;
  [its exact approval](https://github.com/gyldlab/keld/pull/657#issuecomment-6371083658).
- [GH-527 §4.6 and §9 at main 6f67aa74](https://github.com/gyldlab/keld/blob/6f67aa749ed2da0f8a4aa9134e9a038f9781c56d/docs/specs/gh527-worker-owned-blocking-call-transport.md),
  file SHA-256 `f666eb5d640748cb3fc441ff89b4a40cd17c9ea72a010d6660074331ae505305`.
- Architecture 01 §5 (RSS/performance budgets) and architecture 02 §2/§7
  (payload/error ownership, one link and terminal failure) govern this change.
- [Tracker, claim and refreshed base](https://github.com/gyldlab/keld/issues/1142#issuecomment-6374615982):
  base `6f67aa749ed2da0f8a4aa9134e9a038f9781c56d`; #1072 changed Windows
  teardown/test bounds, not this mirror contract. Probe source: [GH-449 head 4a56aa07](https://github.com/gyldlab/keld/blob/4a56aa078354d419314f08c5c36a94b979d3e62f/packages/@keld/kipc/src/transport.ts#L2346).

## 3. Acceptance criteria (binary, each becomes a test)

1. One admitted event causes one decode and one successful apply; dispatch receives
   the exact applied object/value, after the synchronous continuation, in issue order.
2. A second-record decode or apply failure enqueues no fact for that record. If it
   wins the existing first-terminal-recorder transition, the close cause is
   `KELD-IPC-005`; parked and still-unanswered pending calls report `KELD-IPC-022`
   naming that cause. Earlier retained facts drain; the bad and later facts do not.
3. The helper removes its pending reference before calling user dispatch. A throwing
   dispatch is isolated by the existing transport; the next fact and healthy call work.
4. Pending plus in-flight helper-held facts never exceed matching retained event
   records. Consumed slots retain no references; after the existing earlier-record
   drain, no helper-held values remain at terminal finalization. Any additional
   residual-clearing path needs a reachable trace that demonstrates its necessity.
5. Before any production source attaches, that source has a reviewed maximum decoded
   shape, retained-heap budget and alias/lifetime accounting. A record-count assertion
   or the ring byte limit alone does not satisfy this criterion.
6. Independent review binds the final proposal and later implementation diff to the
   owning amendments below. No window/native/public-readiness pass is inferred.
7. A previously recorded terminal cause keeps priority over a later source failure.
   A pending call with an earlier retained answer receives that answer during drain;
   only still-unanswered calls reject. Neither correction changes these transport rules.

## 4. Design

### Decision atoms before selecting the correction

| Atom / owner and boundary | Failure and first falsifier | Evidence state |
| --- | --- | --- |
| Authority / GH-531 and GH-527: approved prose → permitted retention | Treat this draft as approval; check exact-content record | **blocker:** approval absent |
| Frame ownership / WorkerLink: authenticated records → admission/order/release | Independent admission, queue capacity or release; compare A/R/W and transcript | **passed:** unchanged source and real-link probes |
| Fact lifetime / mirror: successful apply → same value → consumption | Decode twice, retain a failed apply, or remove after user throw; identity/count probes | **passed:** bounded prototype behavior/count only |
| Heap / each source: bytes → decoded object graph and aliases | Infer heap bytes from record count; require source shape and budget proof | **unknown:** generic byte bound; **blocker** before production attachment |
| Source failure / state applier: decode/apply throw → 005 cause / 022 callers | Source failure leaves a healthy followup; second-record fault probe | **passed:** counterexample and retained-fact prototype |
| User failure / ring dispatcher: callback throw → isolated error and continued drain | Dispatch throw strands value/record; throw-then-healthy-call probe | **passed:** unchanged owner and prototype |

The fact-count atom depends explicitly on retained-record lifetime, not an independent
producer or capacity. Heap shape and approval are independent of the behavior proof.
No native handle, crash owner or principal minting changes: **No boundary change** in
that architectural sense. This is an explicit additional heap-retention contract.
WorkerLink keeps the socket, shared ring, two cursors and all frame admission/release.
The API helper owns only derived fact references; authenticated identity, authorization,
OS containment and revocation owners stay unchanged. No OS grant or manifest changes.

### Exact proposed owning-text amendments

**GH-531 §4.d:** insert after the `MirrorSource`/`attachMirror` signatures:

> For each admitted event, the helper calls `decode` exactly once in the state-applier
> phase and calls `apply` with that value. Only after apply succeeds does it retain
> that exact value for the same record. The ordinary event-dispatch callback removes
> the retained entry before calling `source.dispatch` with the applied value; it does
> not decode again. Object identity is preserved; primitives, including undefined,
> are valid values. The helper neither admits frames nor schedules listeners.
>
> This permits physical heap storage outside the ring for derived facts. Pending and
> in-flight helper-held facts correspond one-for-one to still-retained same-channel
> event records; their aggregate count is no greater than the matching retained event
> count. Storage metadata and consumed slots do not grow independently of this bound.
> After the transport's earlier-record drain, the helper holds no values at terminal
> finalization. Ordinary consumption may establish this; any additional residual
> clearing needs a reachable lifetime proof. The helper does not retain released payload copies or add an
> independent frame/byte backlog. Each production source needs reviewed maximum shape,
> heap budget and alias/lifetime accounting before attachment; count is not a byte cap.

Replace §4.d rule 4 with:

> **Terminal on source failure.** Decode and apply run only in the framework state
> phase. When a source failure wins the existing first-terminal-recorder transition,
> its close cause is normalized to `KELD-IPC-005`; an internal codec `KELD-IPC-003`
> does not escape as that cause. The existing transport ends delivery at the failing
> record and drains earlier records, including earlier answers to pending calls.
> Parked and still-unanswered pending callers report `KELD-IPC-022` naming the
> recorded cause. A prior terminal cause and other transport wake codes keep their
> existing priority. No failed fact is retained or dispatched. User dispatch exceptions remain
> isolated listener failures; they do not become source failures or retain the
> consumed entry. A mirror that missed a source fact cannot continue serving state.

**GH-527 §4.6:** replace its final paragraph, beginning “The ring is therefore also
the listener FIFO”, with:

> The ring remains the sole inbound-frame admission, issue-order and release owner
> and the listener schedule. GH-531's mirror may retain exactly one successfully
> decoded/applied fact per corresponding retained event until dispatch, removing its
> entry before user callbacks and retaining no helper-held values at terminal
> finalization after earlier-record delivery. This is an explicit physical heap-retention exception,
> not a second frame queue or independent backlog. Its count follows retained records;
> its bytes require source-specific shape, heap-budget and alias/lifetime proof before
> production attachment. An applier is synchronous and calls no user code. Source
> decode/apply failure that wins the existing first-terminal-recorder transition has
> cause `KELD-IPC-005`. Earlier retained answers still settle their pending calls;
> parked or still-unanswered pending callers report the existing recorded terminal
> code/cause. A prior terminal cause keeps priority. GH-531 §4.d owns the
> derived facts; this section keeps the two-cursor order, listener isolation and release.

**GH-527 §9:** replace its “queue and copy” bullet with:

> queue and copy: the ring is the only inbound-frame queue, bounded in bytes and
> records; the reply slot holds at most one reply. GH-531 permits additional physical
> heap retention of derived decoded facts until their existing record is dispatched.
> Their count/lifetime follows retained records; source-specific maximum shape,
> heap-budget and alias accounting is required before each production attachment.
> The ring's byte bound does not bound arbitrary decoded heap graphs.

### Reuse, alternatives and execution seam

The existing `setStateApplier`, `onEvent`, `onEnd` and terminal dispatcher suffice.
The API remains `attachMirror<Fact>(link: WorkerLink, source: MirrorSource<Fact>): void`;
no transport method, wire byte, protocol version, persisted state or capability changes.
There is no platform-specific algorithm; current probes ran on Linux only.

Repeat-decode is rejected by the real second-invocation failure counterexample.
A silent purity/determinism precondition is absent from the approved generic interface
and would remove a covered failure. WeakMap/payload identity fails because the owner
supplies separate arrays/buffers in the two phases; equal duplicate records are distinct.
Throwing 005 during dispatch remains isolated; plain `close()` lacks the required
cause/delivery-stop semantics. A wider transport failure/callback API adds an owner/API
change unnecessary for the behavior demonstrated by the retained-value prototype.
Compatibility fallback: leave the production mirror absent pending approval and proof;
the existing transport and codec-only work continue unchanged.

## 5. Boundaries

This proposal edits only `docs/specs/gh1142-mirror-fact-retention.md`.
After approval, the same GH-449 implementation PR updates GH-531 §4.d and GH-527
§4.6/§9 with provenance and adds the internal API helper plus owning real-link tests.
It changes no shared wire/table, public package export, native adapter, GH-446/profile
contract, guard, dependency, generator or agent instruction. No standalone partial
feature lands. Caller/source qualification remains with each production consumer.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1 / GH-1142: independently review and publish this exact draft; obtain the one
  scoped approval in §10. Existing approved owners stay operative meanwhile.
- [ ] T2 / held GH-449 T1: after approval, amend both owners with provenance and
  implement/test the helper using the existing Rust-host/Bun-role integration owner.
  Require each source's heap qualification before its production attachment.
  Keep the full registry/subscription/constructor/native completion gates open.

## 7. Test plan

AC1–4 use `crates/keld-ipc/tests/worker_link.rs` with the existing Bun role fixture:
exact applied-object identity, one decode per record, parked/unparked order, duplicate
registration refusal, second-record decode/apply faults, user throw, terminal zero retention,
null/undefined facts and reentrant callback lifetime. First negative controls are
repeat decode, enqueue-before-success, dequeue-after-dispatch and retaining consumed
slots. Removing a clear of an already-empty collection is not a negative control.
An additional residual-cleanup path needs a reachable failure trace before its test
can prove release. AC7 uses prior-terminal and earlier-retained-answer real-link
controls; replacing the first cause or rejecting the earlier answer must fail.
AC5 uses source-specific maximum/maximum-plus-one shapes and alias accounting with an
independent budget; scaling the implementation bound alone must not preserve the oracle.
AC6 is exact-diff independent review. Wait on frames/fences, not sleeps; the real
BootstrapListener/WorkerLink is the owner. No mock, prototype or native-unrun lane passes.

Evidence: independent reviewer `/root/review_window_codec`, source head `4a56aa07`,
report `continue-window-mirror-design-refutation.json`; phase receipt SHA-256
`41798be7f577d1fb64bfb18621d56dfe892970aec81c779a48f910ade1064c2c`, retained-value
receipt `9810b71f5908082384412091b76e0ee683a90079ec6792d8333164f8a9a22279`.
Those probes establish behavior/count, not heap-byte proof, approval or native behavior.

## 8. Review gates triggered

unsafe: none; public API: independent architecture review of the generic contract and
memory guarantee (no new app export); permission model: none; dependency addition: none;
wire protocol: none. Source-failure/lifetime review covers the exact implementation diff.

## 9. Perf impact

Additional decoded heap retention can affect architecture 01 §5 RSS and dispatch/park
work. No speed, latency or generic byte bound is claimed. Census: source and retained
records; work: decode once/apply/dispatch; queue/copy: existing ring plus derived facts;
clock/statistic: none claimed; artifact: attributed prototypes above. Source budgets and
an attributed registered benchmark are required before any measured performance claim.

## 10. Open questions

Approve this exact scoped memory/ownership correction to GH-531 §4.d and GH-527
§4.6/§9, retaining source-specific heap qualification before production attachment?
Recommendation: accept the correction after exact-diff review. Acceptance permits the
bounded helper work, not an unqualified consumer or native release; deferral leaves the
mirror absent while codec-only development and existing transport work continue.

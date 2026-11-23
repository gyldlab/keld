# Spec: Bounded outbound retention on the existing Bun role link

- Status: draft
- Tracker: GH-623
- Owner: Monish Khandelwal (@0monish, proposed approval owner)
- Updated: 2026-11-23
- Pin: 269d51528137faf574cfd3548586ee3ada1acfda

## 1. Goal & non-goals

Bound transport-owned outbound bytes and operation records before their first retention.
Preserve the existing role link, accepted-frame order, and failure distinctions.
Define early refusal separately from a failed write with unknown effects.
Production implementation requires approval and closure of B-1 and B-2. Activation also requires B-3.

FACT: The payload limit is 16 MiB, while WriteQueue retains payloads in a Promise chain.
See `packages/@keld/kipc/src/transport.ts:222` and `packages/@keld/kipc/src/transport.ts:1310`.
FACT: Public FS and Auth adapters encode before their Ready wait. Their retention precedes WorkerLink.
See `packages/@keld/api/src/fs.ts:13`, `packages/@keld/api/src/auth.ts:101`, and `packages/@keld/api/src/app.ts:361`.

Non-goals:
- No second socket, replacement transport, shared-memory payload plane, or new Host wire message.
- No frame-limit increase or silent replacement of existing timeout constants.
- No native Host-capacity, decoder, receive-profile, role-custody, or watch-ordering implementation.
- No silent drops, automatic retry, coalesced application messages, or unbounded admission waiters.
- No change to Host execution authority, profile admission, or replay policy.
- No total-process memory or performance claim from transport counters.

## 2. Spec refs

FACT: GH527 owns the Worker, link, outbound order, typed failures, and wake contract.
See `docs/specs/gh527-worker-owned-blocking-call-transport.md:409` and `docs/specs/gh527-worker-owned-blocking-call-transport.md:503`.
FACT: Architecture 02 owns framing and the control plane. See `docs/architecture/02-ipc.md:1`.
FACT: The IPC-005 capacity contract currently concerns guarded Host admission.
See `docs/engineering/keld-error-codes.md:60`. B-2 must approve any local extension.
FACT: One private MessagePort carries CALL, EVENT, answer, abandon, and close messages.
See `packages/@keld/kipc/src/transport.ts:2456` and `packages/@keld/kipc/src/transport.ts:2911`.
The ledger stays with that owner. GH527 remains the owner of shared ordering and termination.

## 3. Acceptance criteria

B-1 selects capacities and queue/retirement clocks. B-2 freezes public failure and shared-input behavior.
B-3 gates activation, not the qualification work in T-3.

### AC-1: Refuse before retaining more work

- **Given:** A valid live link whose byte or record capacity is exhausted.
- **When:** A producer requests another CALL or EVENT.
- **Then:** Return the B-2 refusal before cloning, pending-state creation, or posting.
- **Negative control:**
  - **Single fault:** Bypass EVENT admission.
  - **At-risk precondition:** The peer stops reading after HELLO. Offered work exceeds the frozen capacity.
  - **Expected RED:** An independent retained-byte or record census exceeds its ceiling.

### AC-2: Count backing storage and zero-length records

- **Given:** Small views into large buffers, pooled buffers, empty payloads, and maximum payloads.
- **When:** Each enters the outbound path.
- **Then:** Retain admitted compact snapshots and bounded metadata only. Empty payloads consume a record.
- **Negative control:**
  - **Single fault:** Post a small view while charging only its visible length.
  - **At-risk precondition:** The qualified runtime retains or clones the larger backing buffer.
  - **Expected RED:** The independent backing census exceeds the charge.

### AC-3: Include each producer in one budget

- **Given:** A partly full ledger and valid work from each D-2 producer.
- **When:** Each producer attempts to exceed its partition.
- **Then:** Reserve before retention or follow the specified terminal path. Ordinary data cannot borrow protected capacity.
- **Negative control:**
  - **Single fault:** Bypass reservation for Worker-generated PING replies.
  - **At-risk precondition:** The peer sends valid PINGs while refusing outbound bytes.
  - **Expected RED:** Control work exceeds its partition without the specified terminal refusal.

### AC-4: Preserve ordered, immutable frame contents

- **Given:** A direct WriteQueue caller admits numbered frames with independent payload digests.
- **When:** The first write pauses, the caller mutates a later buffer, and the peer forces fragmented writes.
- **Then:** Receive each admitted snapshot once, in send order, without interleaving. Run a separate WorkerLink mailbox case.
- **Negative control:**
  - **Single fault:** Retain the direct caller's mutable view instead of its snapshot.
  - **At-risk precondition:** No Worker or MessagePort clone occurs before the caller mutation.
  - **Expected RED:** The peer digest differs from the admitted digest.

### AC-5: Release payload credit only after retention ends

- **Given:** A frame remains queued or partly written.
- **When:** Its caller times out before write completion.
- **Then:** Keep byte credit until the charged backing retires or physical link quiescence releases it.
- **Negative control:**
  - **Single fault:** Release byte credit on caller timeout.
  - **At-risk precondition:** The writer retains the payload while another producer fills the apparently free capacity.
  - **Expected RED:** Independent retained buffers exceed the byte ceiling.

### AC-6: Keep pending CALL records bounded

- **Given:** A peer reads without replying, then replies while main-thread delivery is paused.
- **When:** Producers continue issuing asynchronous CALLs.
- **Then:** Bound distinct CALL tickets across caller, Worker, abandonment, and retained-delivery states. Do not double-count one ticket across states.
- **Negative control:**
  - **Single fault:** Release CALL credit when its request finishes writing.
  - **At-risk precondition:** The peer drains requests promptly and never answers.
  - **Expected RED:** Pending CALLs exceed their ceiling while outbound byte use remains small.

### AC-7: Release each reservation once

- **Given:** A reservation encounters failure, reply, timeout, close, and retirement in both competing event orders.
- **When:** Its callbacks and retained messages settle.
- **Then:** Release each credit once. Prevent negative counts and release into a successor ledger.
- **Negative control:**
  - **Single fault:** Remove the release-state compare-and-exchange.
  - **At-risk precondition:** Two terminal paths release the same reservation.
  - **Expected RED:** Credit underflows or admission exceeds independently observed free capacity.

### AC-8: Preserve progress while the main thread is parked

- **Given:** A blocking CALL parks main. Worker retirement can release credit required by a following operation.
- **When:** The Worker drains that ticket and the peer waits for the required operation before replying.
- **Then:** Worker release needs no main callback. The required operation progresses and the call respects its original deadline.
- **Negative control:**
  - **Single fault:** Make credit release depend on a main-thread callback.
  - **At-risk precondition:** Drain the exact partition needed next. Occupy other reserves so their credit cannot hide the fault.
  - **Expected RED:** Required progress fails despite completed drain and available physical capacity.

### AC-9: Distinguish early refusal from partial-write damage

- **Given:** Separate full-budget refusals and failures after a partial header or payload write.
- **When:** The operation fails.
- **Then:** Early refusal emits zero bytes and preserves the link. Partial-write failure poisons it and preserves unknown-effect semantics.
- **Negative control:**
  - **Single fault:** Treat partial-write failure as early refusal.
  - **At-risk precondition:** The peer records a frame prefix before failure.
  - **Expected RED:** Later bytes use the damaged stream or the error falsely proves nonexecution.

### AC-10: Keep terminal control capacity separate from ordinary data

- **Given:** Data capacity is full while control capacity remains available.
- **When:** Submit another ordinary EVENT, then separately request terminal close.
- **Then:** Refuse the EVENT without control credit. Close uses bounded reserved capacity without another payload queue.
- **Negative control:**
  - **Single fault:** Allow ordinary EVENTs to spend control credit.
  - **At-risk precondition:** The valid EVENT fits the free control bytes and records, and reaches actual admission.
  - **Expected RED:** Independent admission and retention observations detect its forbidden control charge.

### AC-11: Keep Host handler admission bounded

- **Given:** One active handler fills a one-handler cap. Answer tickets, answer bytes, and handler-input storage remain available.
- **When:** Another valid CALL reaches its installed handler.
- **Then:** Do not invoke it. Follow the approved terminal path without an unbounded Promise chain.
- **Negative control:**
  - **Single fault:** Omit handler-record reservation while preserving answer and input admission.
  - **At-risk precondition:** Hold one handler behind a barrier. Fund two maximum answers and inputs, and pace CALLs below ring overflow.
  - **Expected RED:** Active handlers exceed the configured handler bound.

### AC-12: Preserve capacity across failure and a healthy successor

- **Given:** Separate postMessage-throw, pre-write-refusal, stalled-write, Worker-death, and retirement cases.
- **When:** The owner observes each physical terminal boundary.
- **Then:** Seal retired admission. Give the permitted successor a fresh ledger. Observe actual exit separately from termination request.
- **Negative control:**
  - **Single fault:** Accept a release with a previous generation's ticket.
  - **At-risk precondition:** The successor occupies the same local slot index.
  - **Expected RED:** Old work frees successor credit or permits excessive admission.

### AC-13: Reserve answer bytes before Host handler effects

- **Given:** A valid Host CALL has record capacity but lacks maximum-answer bytes. A separate positive handler holds both reservations.
- **When:** The new CALL reaches admission, or the admitted handler answers while data capacity is full.
- **Then:** Refuse before new handler effects. Deliver the positive answer with its existing ticket.
- **Negative control:**
  - **Single fault:** Omit only maximum-answer reservation before invocation.
  - **At-risk precondition:** Role, frame, handler, record, and input checks pass. Answer capacity alone is exhausted.
  - **Expected RED:** An independent effect occurs without reserved answer capacity. Later answer refusal cannot hide it.

### AC-14: Retire permanent CALL tombstone saturation

- **Given:** The smallest legal CALL capacity, occupied competing partitions, and a reading peer that never replies.
- **When:** Timeouts reach the B-1 threshold or fill eligible records while heartbeats continue.
- **Then:** Retire within the approved bound without new CALLs or main callbacks. Keep correlations until actual retirement.
  Late replies before terminal commitment restore admission. Test both reply/retirement orders and a healthy successor CALL separately.
- **Negative control:**
  - **Single fault:** Disable saturation retirement while preserving local refusal.
  - **At-risk precondition:** Tombstones fill eligible capacity. Abandon and close rights are prepaid, and no further CALL is submitted.
  - **Expected RED:** The bound expires with the link OPEN and CALL admission permanently unavailable.

### AC-15: Admit before encoding and Ready retention

- **Given:** HELLO succeeds but Ready is withheld. Public FS/Auth operations can exhaust their record or encoding allowance.
- **When:** Exceed either allowance with valid operations. Separately submit oversized encoder input with available operation and scratch capacity.
- **Then:** Refuse excess operations before encoding or Ready retention. Refuse oversized encoding before allocation. Preserve direct whenReady behavior.
- **Negative control:**
  - **Single fault:** Move operation admission after Ready. In a separate run, remove only encoder size preflight.
  - **At-risk precondition:** Withhold Ready for the first arm. Keep other capacity checks open for the oversized-encoding arm.
  - **Expected RED:** Retained storage or waiters exceed their bound, or oversized encoding allocates before refusal, respectively.

### AC-16: Arbitrate real main and Worker credit updates

- **Given:** Main reserves while the Worker returns credit on the same shared counter. Independent live tickets identify actual usage.
- **When:** Barrier those updates, then fill the remaining capacity. Separately force partial reservation failure and check rollback.
- **Then:** Credit equals live-ticket charges, admission respects capacity, and failed reservations leave no partial charge.
- **Negative control:**
  - **Single fault:** Replace the shared atomic update with a load/store pair.
  - **At-risk precondition:** Both agents read the old value before either stores. The Worker stores last, losing the reservation update.
  - **Expected RED:** Credit disagrees with live tickets and subsequent admissions exceed actual capacity.

### AC-17: Enforce the approved shared-input boundary

- **Given:** B-2 has approved the proposed shared-backed-input refusal. A valid payload view uses a SharedArrayBuffer.
- **When:** Another agent mutates that storage while admission is paused.
- **Then:** Reject before dispatch or asynchronous retention. An unsynchronized copy does not satisfy a coherent-snapshot claim.
- **Negative control:**
  - **Single fault:** Remove shared-backing rejection while preserving ordinary compaction.
  - **At-risk precondition:** Frame and capacity checks pass. The shared view reaches the input boundary.
  - **Expected RED:** Shared-backed input enters the prohibited path instead of receiving the B-2 refusal.

### AC-18: Bound admission-to-service residence

- **Given:** Accepted work has a B-1 queue deadline. Each preceding active write completes within its existing I/O deadline.
- **When:** A queued tail exceeds its original residence deadline before its first write.
- **Then:** Apply the B-2 terminal outcome within its bound. Preserve FIFO and unknown effects of earlier work.
- **Negative control:**
  - **Single fault:** Remove queue-age expiry while keeping active-write and caller timeouts.
  - **At-risk precondition:** Slow progress keeps every head healthy while tail residence exceeds the queue bound.
  - **Expected RED:** The tail remains retained beyond its bound without the required terminal settlement.

## 4. Design

### D-1: Atoms and their independence

| Atom and owner | Boundary | Failure and falsifier |
| --- | --- | --- |
| Census, transport | Objects to bytes and records | Missing producer/backing. AC-2, AC-3, AC-15. |
| Admission, ledger | Input to reservation | Retention or effect before charge. AC-1, AC-11, AC-13. |
| Work/order, writer | Snapshot to stream | Mutation or interleaving. AC-4, AC-17. |
| Clock, deadline owner | Original clock to retirement | Early credit release or unlimited residence. AC-5, AC-18. |
| Lifetime, ticket | Ownership to release | Early, double, stale, or lost release. AC-6, AC-7, AC-12, AC-16. |
| Progress, Worker | Credit to wake/close | Parked-main dependence or immortal saturation. AC-8, AC-10, AC-14. |
| Failure, IPC owner | Dispatch facts to error | Unknown effect reported as nonexecution. AC-9. |
| Statistic/artifact, test owner | Independent observations to claim | Ledger counter substitutes for memory evidence. B-3. |

Admission does not prove release. Write completion does not prove execution. Timeout does not prove nonexecution.
Byte limits do not bound timers or handlers. Each lifetime needs its own record and observable.

### D-2: Complete producer census

FACT: These paths currently produce or retain outbound-related work at the Pin.

| Producer | Current source | Target admission and lifetime |
| --- | --- | --- |
| Public FS encoding | `packages/@keld/api/src/fs.ts:13` | Reserve before generated encoding and snapshot storage. |
| Public Auth encoding | `packages/@keld/api/src/auth.ts:101` | Reserve before generated encoding and snapshot storage. |
| Operation Ready wait | `packages/@keld/api/src/app.ts:361` | Keep the same ticket through Ready. No second queue. |
| Blocking CALL | `packages/@keld/kipc/src/transport.ts:2739` | Reserve before correlation publication and parking. |
| Async CALL | `packages/@keld/kipc/src/transport.ts:2767` | Reserve before timer, Promise record, and posting. |
| Application EVENT | `packages/@keld/kipc/src/transport.ts:2807` | Synchronous reservation or local refusal. |
| Host CALL answer | `packages/@keld/kipc/src/transport.ts:3290` | Reserve handler input/count and maximum answer before effects. |
| Worker HELLO | `packages/@keld/kipc/src/transport.ts:3532` | Bounded bootstrap allocation under the same owner. |
| Worker PING | `packages/@keld/kipc/src/transport.ts:3594` | Protected control admission, existing writer order. |
| Abandon and close | `packages/@keld/kipc/src/transport.ts:2456` | Prepaid metadata, one abandon per CALL and one close per link. |
| Direct writer | `packages/@keld/kipc/src/transport.ts:1321` | Reuse the ledger primitive under each existing socket owner. |

Worker frames still enter WriteQueue. Main messages still use the existing private FIFO.
Do not add another MessagePort, ordering owner, or capacity-waiter queue.

### D-3: One pre-retention ledger

Proposed private state, conditional on B-1 and B-2:
~~~ts
type OutboundClass = "data" | "answer" | "control";
interface OutboundTicket {
  readonly epoch: number;
  readonly slot: number;
  readonly incarnation: number;
}
~~~
Limits freeze before admission. B-1 defines byte, frame, CALL, handler, answer, control, input, and metadata capacities separately.
Reject nonintegers, unsafe arithmetic, impossible partitions, and insufficient bootstrap/maximum-frame allowances before Worker creation.
Use a fixed ticket table in the existing shared control owner. Reserve all required units or roll back completely.
Include encoder scratch and operation Ready retention before WorkerLink. Carry the ticket rather than reserving twice.
Preserve direct whenReady semantics. Its application subscriptions do not become an alternate operation queue.
Regenerate codecs through their existing owner if budgeted encoding changes. Do not hand-edit generated output or duplicate parsing.
Shared updates must arbitrate main reservations against Worker releases. Promise concurrency on one JS agent is insufficient evidence.
Validate epoch and incarnation on every release. Define exhaustion before arithmetic or incarnation wrap permits unsafe reuse.
Worker release updates shared credit directly, without a parked-main acknowledgment. Internal tickets do not change Host wire format.

### D-4: Bytes, copies, and records

Count backing allocations and temporary overlap. Snapshot ordinary input after admission and before asynchronous retention.
Do not retain a large backing through a tiny view. Charge admitted snapshots, mailbox clones, and serialized envelopes together.
The charge covers worst simultaneous retention. Qualify pooled buffers and every distinct copy on the selected Bun build.
Caller buffers remain usable. Transfer-based optimization requires B-3 evidence and an approved public contract.
Proposed B-2 rule: reject shared-backed input before dispatch. Ordinary copying cannot prove a coherent concurrent SAB snapshot.
A future synchronized shared-input protocol needs separate approval. AC-17 must change with that approved contract before activation.

Release bytes after every charged reference retires. Keep CALL credit until caller settlement, Worker retirement, and retained delivery finish.
Count distinct CALL tickets across those states. Timeout does not release the Worker tombstone or withdraw queued wire work.
Prepay one abandon message per CALL. Reserve one terminal right without requiring fresh admission during saturation.
B-1 must couple all legal partitions with bounded tombstone retirement. Never free correlations early to restore apparent capacity.

Before handler invocation, reserve its record, retained input, and maximum answer storage. Input and answer allowances are separate.
If any reservation fails, follow the approved terminal path before effects. Already admitted answers use their existing tickets.
Ordinary traffic cannot spend answer/control reserves. Reserved admission does not provide priority over accepted FIFO work.

### D-5: Failure, cancellation, and ordering

Proposed B-2 extension: one local AdmissionCapacity constructor uses KELD-IPC-005. It does not reuse the guarded Host constructor.
Proposed message: "outbound transport capacity exhausted before dispatch".
Proposed fix: "Wait for admitted work to retire, then submit new work. This operation was not dispatched. Do not replay earlier unanswered calls."
B-2 also reviews additive `reason: "capacity"` and `phase: "local-admission"` fields, plus shared-input refusal code/message/fix.
sendEvent and callBlocking throw before dispatch. call rejects without a pending record. Early refusal emits zero bytes.
Recoverable saturation preserves a healthy link. Permanent tombstone saturation follows B-1 retirement and keeps earlier effects unknown.
Code alone never authorizes replay or reconnect. A Host-issued 024 and a caller-observed 022 remain distinct.
See `docs/specs/gh527-worker-owned-blocking-call-transport.md:523`.

Capture queue age at the first operation reservation. Preserve existing caller-clock semantics unless B-2 approves their amendment.
B-1 selects queue, saturation, and close bounds. Keep existing active I/O clocks and GH527 ordered-close obligations.
Neither successful short writes nor renewed polls reset an original clock. B-2 freezes queue-expiry errors and their terminal behavior.
Partial-write failure poisons the queue. Keep existing IPC-001/006 and terminal-cause propagation where their contracts apply.
Cancellation records a terminal decision. Release follows actual ownership retirement. Close seals admission before retiring queued work.
A termination request is not physical quiescence. Observe Worker exit and retained storage separately.
A permitted successor receives a fresh ledger. Old callbacks cannot change successor capacity.

### D-6: Reuse and external semantics

Reuse the existing writer, Worker, shared control block, schemas, and channel contracts. No replacement transport is required.
Larger queues and high-water callbacks cannot bound prior posting. A second queue duplicates retention and ordering ownership.
Do not change sendEvent into a Promise-returning API. Do not hide overload in indefinite blocking, silent drops, or retries.
FACT: Bun documents manual socket backpressure: "manage backpressure manually with the drain handler."
Source: https://bun.sh/docs/runtime/networking/tcp. Retrieved 2026-11-05 for historical E-2.
FACT: Bun describes message serialization using "HTML Structured Clone Algorithm."
Source: https://bun.sh/docs/runtime/workers. Retrieved 2026-11-05 for historical E-2.
Those documents do not establish pinned-runtime copy overlap or cleanup. E-3 supplies narrower executed observations.

## 5. Boundaries

Future work uses the existing transport owner and its actual public adapters:
- `packages/@keld/kipc/src/transport.ts`, its tests, generation, stamping, and installed-package contracts.
- Existing FS/Auth adapters, Ready integration, and schema-generator inputs needed for admission before encoding.
- IPC error declarations and shared GH527 contracts only after B-2 approval.

This PR changes draft specification, architecture clarification, and proposal evidence only. It activates no runtime behavior.
Native Host queues, decoder limits, receive-size compatibility, and retired-reader reaping remain with their existing owners.
The broader evidence records those findings without expanding GH623 implementation authority.
Do not edit native Host capacity, role custody, producer-ordering fixes, or active Windows transport work.
Do not add a dependency, duplicate schema, Host message, GRANT implementation, or remote cancellation protocol.

## 6. Tasks

### T-1: Select capacities and freeze error semantics

- **Blocked by:** none
- Inventory every D-2 caller and retained object at the selected revision. Measure ordinary and stalled-peer demand.
- Close B-1 and B-2 with transport/error owners. Freeze units, partitions, copy charges, clocks, and public failure fixtures.

### T-2: Add failure-first admission and lifetime fixtures

- **Blocked by:** T-1, B-1, B-2
- Register AC-1 through AC-18 before implementation. Use independent peer transcripts and retained-object observations.
- Prove baseline failures where feasible. Keep expected baseline failure distinct from positive boundedness acceptance.

### T-3: Implement disabled integration and qualify the actual runtime

- **Blocked by:** T-2
- Integrate the ledger at each producer and writer seam. Run all ACs and single-fault mutants on the pinned runtime matrix.
- Measure copies, retained records, memory, teardown, and successor behavior separately. This work produces B-3 evidence.

### T-4: Activate the reviewed contract and reconcile consumers

- **Blocked by:** T-3, B-3
- Reconcile callers with explicit refusal and no automatic retry. Preserve generated/stamped ownership and installed-package tests.
- Publish exact-head acceptance and independent review before activation.

## 7. Test plan & evidence

| Family | Criteria | Independent oracle |
| --- | --- | --- |
| Stalled peers and producer partitions | AC-1, AC-3, AC-10 | Peer bytes, admission, retained backing and records |
| Input ownership | AC-2, AC-4, AC-17 | Backing capacities, input rejection, peer digests |
| CALL lifetime and teardown races | AC-5, AC-6, AC-7, AC-9, AC-12, AC-14 | Both retirement orders, exact errors, physical exit, successor |
| Parked main and shared updates | AC-8, AC-16 | Real main/Worker schedule, live tickets, required progress |
| Handler effects | AC-11, AC-13 | Input/answer allowances, active handlers, independent effect count |
| Public adapters and queue residence | AC-15, AC-18 | Pre-Ready allocations/waiters, original clock, terminal settlement |

Test zero, exact capacity, and capacity-plus-one independently for bytes and records. Keep other gates open for each mutant.
Use barriers after HELLO and at actual consumer boundaries. Do not synchronize with sleeps. Isolate hostile teardown in children.
Observe both race orders, physical cleanup, and a healthy follow-up. Instrumentation must not supply its own resource oracle.
Use logical concurrent producers for load/order and actual main/Worker execution for atomicity. Check encoder allocations before refusal separately.

### E-1: Historical transport source inspection

- **Run:** Read producer, writer, pending, shared-control, and error paths through GitHub at `0eb3ce203187f35c5276a4237189c906a5220d5a`.
- **n:** One source revision, eight producer/retention categories, zero product runs.
- **Environment:** Cloud documentation workspace, 2026-11-05.
- **Limits:** Historical source inspection establishes no runtime memory or throughput result. D-2 is the refreshed census.

### E-2: Historical external documentation

- **Run:** Resolve Context7 /oven-sh/bun, query socket/Worker semantics, and confirm the official D-6 pages.
- **n:** Two documentation pages, zero pinned Bun executions.
- **Environment:** Retrieved 2026-11-05 for the original `0eb3ce203187f35c5276a4237189c906a5220d5a` source study.
- **Limits:** Documentation does not establish selected-runtime copy costs or termination quiescence.

### E-3: Executed baseline probes and proposal evidence

- **Run:** Commands, fixtures, hashes, receipts, and limitations: [GH623 proposal evidence](../engineering/gh623-proposal-evidence/README.md).
- **n:** 2,048 retained writer payloads, 512 native EVENTs, 512 unresolved handlers, two Rust witnesses, and 54 existing tests across corrected runs.
- **Environment:** Pin, macOS 26.5.1 arm64, Bun 1.4.4 revision38afee6c8, Rust/Cargo 1.99.1, 2026-11-23.
- **Limits:** Baseline defects, not bounded-fix acceptance. No throughput/RSS ceiling, Windows/Linux qualification, physical UI run, or new-ledger mutation pass.
FACT: The writer witness retained 134,217,728 logical payload bytes. A one-byte view cloned an 8 MiB backing in Bun.
FACT: Shared input remained shared, and direct queued mutation changed wire bytes. E-3 binds these observations.
The linked README owns the complete structured evidence, derivative manifest, raw-result limitations, and 24 cross-owner regression designs.
Its numeric qualification profile is illustrative. It does not close B-1 or replace the ACs in this specification.

## 8. Review gates triggered

Public API: refusal, shared-input behavior, and queue expiry require exact-content approval.
Permission model: no new authority proposed.
Wire protocol: no Host wire change. GH527 reviews internal ordering, ticket lifetime, and termination.
Dependency addition: none proposed. Unsafe: none proposed.
B-2 owns code allocation/reuse. This draft cannot approve itself.

## 9. Perf impact

The target is bounded retention. No speed improvement is claimed.
Measure logical bytes, backing capacity, operation records, and process memory separately. Count kernel buffers in their own domain.
Compare equivalent workloads before and after. Include open-loop overload, mixed payloads, slow consumers, and recovery.
Retain architecture 01 budgets and regression review. Lower queue counts do not prove lower resident memory.

## 10. Open questions, blockers & unknowns

### B-1: Capacity and copy-accounting selection

- **Status:** open
- **Owner:** Monish Khandelwal with the GH527 transport owner
- **Impact:** Blocks production limits, shared layout, clocks, and implementation after T-1.
- **Check:** Publish measured demand, exact byte/record/input ceilings, protected partitions, conservative copy charges, and queue/retirement bounds.
  Prove every legal capacity/terminal configuration, including the smallest CALL ceiling and occupied competing partitions.
  Prepay abandon/close rights. Keep existing abandonment termination reachable, or approve bounded saturation retirement without early release. Require AC-14.

### B-2: Local refusal and shared-contract amendment

- **Status:** open
- **Owner:** GH1101 error owner with GH527 transport owner
- **Impact:** Blocks new refusal behavior, shared-input restrictions, queue-expiry behavior, and claims that IPC-005 already permits them.
- **Check:** Approve exact code/message/fix, no-effect distinction, optional typed fields, caller behavior, and canonical contract amendments.

### B-3: Native runtime qualification

- **Status:** open
- **Owner:** GH623 implementation owner and operators of each claimed OS
- **Impact:** Blocks activation and broad memory/compatibility claims. It does not block qualification work in T-3.
- **Check:** Publish exact-version AC/mutant runs, independent retained-storage evidence, physical teardown, and a healthy successor for each claimed platform.

### U-1: Remaining Bun retention and termination behavior

UNKNOWN: Exact simultaneous mailbox-copy overlap, pooled-buffer retention, and post-termination reclamation on the selected build.
E-3 narrows ordinary subview and shared-alias behavior only. T-3 must qualify the remaining lifetimes on the actual runtime.

### U-2: Existing caller handling of synchronous refusal

UNKNOWN: Every installed consumer's response to the proposed refusals and queue expiry.
T-1 enumerates callers. T-4 preserves application outcomes or leaves the affected consumer unqualified.

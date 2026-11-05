# Spec: Bounded outbound retention on the existing Bun role link

- Status: draft
- Tracker: GH-623
- Owner: Monish Khandelwal (@0monish, proposed approval owner)
- Updated: 2026-11-05
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Bound transport-owned outbound bytes and live operation records before retention.
Preserve the existing single role link, frame ordering, and error semantics.
Define early refusal separately from a failed write with unknown effects.
Runtime implementation requires approval of this draft and closure of its named design gates.

FACT: The current frame payload limit is 16 MiB.
It is a per-frame limit, not an aggregate queue limit.
See `packages/@keld/kipc/src/transport.ts:222`.
The current writer retains payloads in a Promise chain.
See `packages/@keld/kipc/src/transport.ts:1182`.

Non-goals:

- No second socket, replacement transport, or shared-memory payload plane.
- No increase to frame limits, timeout constants, or Host admission capacity.
- No fix for application watch/unwatch ordering or application request floods.
- No silent event drops, unbounded waiter queue, automatic retry, or coalesced application messages.
- No change to Host execution authority, profile admission, or replay policy.
- No total-process memory claim from a logical transport counter.

## 2. Spec refs

FACT: GH527 owns one Worker, one link, one ordered outbound path, and typed terminal failures.
See `docs/specs/gh527-worker-owned-blocking-call-transport.md:409`.
Its typed error and wake contracts remain authoritative.
See `docs/specs/gh527-worker-owned-blocking-call-transport.md:503`.
Architecture 02 owns framing and the control plane.
See `docs/architecture/02-ipc.md:1`.

FACT: Current KELD-IPC-005 capacity wording concerns Host admission.
See `docs/engineering/keld-error-codes.md:60`.
A local early-refusal use needs the explicit error-owner amendment in B-2.
This draft allocates no new code and does not silently broaden an existing code.

FACT: WorkerLink uses one private MessagePort for CALL, EVENT, answers, abandonment, and close.
See `packages/@keld/kipc/src/transport.ts:2328` and `packages/@keld/kipc/src/transport.ts:2783`.
The new ledger belongs inside that existing transport owner.

## 3. Acceptance criteria

B-1 selects capacities before production implementation.
B-2 freezes error/API behavior.
B-3 gates runtime qualification, not the work needed to produce that qualification.

### AC-1: Refuse before retaining more work

- **Given:** A valid live link whose declared byte or record capacity is exhausted.
- **When:** A producer requests another CALL or EVENT.
- **Then:** It receives the B-2 local refusal before payload cloning, pending-state creation, or MessagePort enqueue.
- **Negative control:**
  - **Single fault:** Bypass admission for EVENT.
  - **At-risk precondition:** The peer stops reading after HELLO and the producer exceeds the frozen capacity.
  - **Expected RED:** The independent retained-byte or record census exceeds its declared ceiling.

### AC-2: Count backing storage and zero-length records

- **Given:** Small views into large buffers, pooled buffers, empty payloads, and maximum-size payloads.
- **When:** Each enters the outbound path.
- **Then:** Transport retains only admitted compact snapshots and bounded metadata. Empty payloads consume a record.
- **Negative control:**
  - **Single fault:** Post a small view directly while charging only its visible length.
  - **At-risk precondition:** That view retains or clones a larger backing buffer in the qualified runtime.
  - **Expected RED:** The independent backing-storage census exceeds the recorded charge.

### AC-3: Include each producer in one budget

- **Given:** A partially full ledger and valid work from each producer listed in D-2.
- **When:** Each producer attempts to cross its partition's capacity.
- **Then:** Every path reserves before retention or terminates through its specified fail-closed path.
- **Negative control:**
  - **Single fault:** Bypass the ledger for Worker-generated PING replies.
  - **At-risk precondition:** The peer sends valid PINGs while refusing outbound bytes.
  - **Expected RED:** PING work exceeds the declared control partition without the expected terminal refusal.

### AC-4: Preserve ordered, immutable frame contents

- **Given:** A direct WriteQueue caller admits frames with unique sequence numbers and payload digests.
- **When:** The first write pauses, the caller mutates a later admitted buffer, and the peer forces fragmented writes.
- **Then:** The peer receives each admitted snapshot once, in existing send order, without interleaved bytes.
  Run a separate WorkerLink mailbox case for the same ordering contract.
- **Negative control:**
  - **Single fault:** Keep the direct writer caller's mutable view instead of its admitted snapshot.
  - **At-risk precondition:** The later frame reaches no Worker or MessagePort clone before the caller mutates it.
  - **Expected RED:** The independent peer digest differs from the admitted digest.

### AC-5: Release payload credit only after retention ends

- **Given:** A frame remains queued or partly written.
- **When:** A caller times out before the write finishes.
- **Then:** Payload credit remains charged until the actual retained buffers are released or the link quiesces.
- **Negative control:**
  - **Single fault:** Release byte credit when the caller timeout fires.
  - **At-risk precondition:** The writer still retains the payload and a new producer fills the apparently free capacity.
  - **Expected RED:** The retained-buffer census exceeds the ledger ceiling.

### AC-6: Keep pending CALL records bounded

- **Given:** A peer reads requests but withholds replies, then returns replies while main-thread delivery is paused.
- **When:** The producer continues issuing async CALLs.
- **Then:** Pending callers, Worker entries, abandonment records, and retained terminal deliveries stay within the declared record ceiling.
- **Negative control:**
  - **Single fault:** Release a CALL record when its request finishes writing.
  - **At-risk precondition:** The peer drains requests quickly and never answers them.
  - **Expected RED:** Pending operation records exceed the ceiling while outbound byte use stays small.

### AC-7: Release each reservation once

- **Given:** One reservation encounters write failure, late reply, timeout, close, or generation retirement in both event orders.
- **When:** All owned callbacks and retained messages settle.
- **Then:** Each credit releases once. Counts never become negative. Retired tickets cannot release successor credit.
- **Negative control:**
  - **Single fault:** Remove the ticket-state compare-and-exchange from release.
  - **At-risk precondition:** Two terminal paths race to release the same reservation.
  - **Expected RED:** A count underflows or a new admission exceeds the real available capacity.

### AC-8: Preserve progress while the main thread is parked

- **Given:** A blocking CALL parks main while the Worker can still drain admitted outbound data.
- **When:** Writes finish and the Host sends the matching reply.
- **Then:** Worker-owned byte release requires no main-thread acknowledgment. The call returns under the existing deadline and wake contract.
- **Negative control:**
  - **Single fault:** Make byte-credit release depend on a main-thread message callback.
  - **At-risk precondition:** Later required control output needs the credit released by the drained data.
  - **Expected RED:** The control/progress oracle stalls or refuses despite completed writes and available physical capacity.

### AC-9: Distinguish early refusal from partial-write damage

- **Given:** Separate cases for full-budget refusal and a socket failure after a partial header or payload write.
- **When:** The operation fails.
- **Then:** Early refusal writes zero bytes and preserves a healthy link. Partial writes poison the writer and preserve unknown-effect semantics.
- **Negative control:**
  - **Single fault:** Treat a partial-write failure as an early local refusal.
  - **At-risk precondition:** The peer records part of the original frame before the injected failure.
  - **Expected RED:** A later frame is written on the damaged stream or the error claims proven nonexecution.

### AC-10: Keep terminal control capacity separate from ordinary data

- **Given:** Normal traffic fills its data partition while the declared control allowance remains available.
- **When:** Submit another ordinary EVENT, then separately request the actual terminal close.
- **Then:** Refuse the EVENT without consuming control credit. The close completes through bounded reserved capacity and creates no extra queue.
- **Negative control:**
  - **Single fault:** Let ordinary EVENTs consume the reserved control partition.
  - **At-risk precondition:** Only data credit is exhausted. The valid EVENT's complete byte/record charge fits the free control allowance and reaches the real admission owner.
  - **Expected RED:** The EVENT is admitted against control credit. Independent admission and retained-object observations detect the forbidden charge.

### AC-11: Keep Host handler admission bounded

- **Given:** Host handler records are exhausted, answer-byte capacity is available, and a valid CALL targets an installed handler.
- **When:** The request reaches handler admission.
- **Then:** The handler is not invoked. The link follows the approved terminal path and retains no unbounded Promise chain.
- **Negative control:**
  - **Single fault:** Omit the handler-record reservation before invocation while preserving answer-byte admission.
  - **At-risk precondition:** Existing handlers wait behind a barrier and the next valid CALL reaches record admission with answer bytes available.
  - **Expected RED:** Active handler records exceed the declared bound.

### AC-12: Preserve capacity across failure and a healthy successor

- **Given:** Separate runs cover postMessage throw, pre-write refusal, stalled writes, Worker death, and link retirement.
- **When:** The owner observes each actual terminal boundary.
- **Then:** No new work enters a retired ledger. A new role generation uses a new ledger after the existing lifecycle permits it.
- **Negative control:**
  - **Single fault:** Accept a release carrying the previous generation's ticket.
  - **At-risk precondition:** The successor has an occupied slot with the same local index.
  - **Expected RED:** Old work frees successor capacity or the successor exceeds its real bound.

### AC-13: Reserve answer bytes before Host handler effects

- **Given:** A valid Host CALL has record capacity but cannot reserve its maximum answer bytes. A separate positive case already holds both reservations.
- **When:** The new CALL reaches handler admission, or the positive handler answers while normal traffic fills its data partition.
- **Then:** Refuse the new CALL before handler effects. The admitted positive answer reaches the peer using its existing ticket.
- **Negative control:**
  - **Single fault:** Omit only the maximum-answer byte reservation before granting handler invocation.
  - **At-risk precondition:** The new CALL has valid role, framing, handler and record capacity. Its answer-byte allowance is exhausted.
  - **Expected RED:** The independently observed handler effect occurs without a reserved answer ceiling. Later answer refusal cannot hide that effect.

## 4. Design

### D-1: Atoms and their independence

| Atom and owner | Boundary | Failure and falsifier |
| --- | --- | --- |
| Census, transport owner | Retained objects to bytes and records | Omitted backing store or producer. AC-2, AC-3. |
| Admission, transport ledger | Valid input to reserved ticket | Enqueue or handler effect before its required charge. AC-1, AC-11, AC-13. |
| Work and order, existing writer | Ticket and snapshot to ordered stream | Mutable payload or frame interleave. AC-4. |
| Clock, current deadline owner | Original deadline to terminal decision | Timeout frees still-retained memory. AC-5. |
| Lifetime, ticket state | Queue/pending completion to one release | Double release or early CALL release. AC-6, AC-7. |
| Progress, existing Worker | Reserved control capacity to wake/close | Main-thread-dependent release. AC-8, AC-10. |
| Failure meaning, IPC error owner | Dispatch facts to typed failure | Partial write reported as no effect. AC-9. |
| Statistic and artifact, test owner | Independent census and raw run to claim | Self-reported counters presented as memory proof. B-3. |

Admission does not prove memory release.
Write completion does not prove application execution.
A caller timeout does not prove that the peer did nothing.
A byte bound does not bound pending timers or handler Promises.
Each of those facts needs a separate record or observable.

### D-2: Complete producer census

FACT: These source paths currently produce or retain outbound-related work.

| Producer | Current source | Target admission and lifetime |
| --- | --- | --- |
| Blocking CALL | `packages/@keld/kipc/src/transport.ts:2610` | Reserve before correlation publication and parking. |
| Async CALL | `packages/@keld/kipc/src/transport.ts:2640` | Reserve before timer, Promise record, and enqueue. |
| Application EVENT | `packages/@keld/kipc/src/transport.ts:2679` | Synchronous reserve or synchronous local refusal. |
| Host CALL answer | `packages/@keld/kipc/src/transport.ts:3162` | Reserve handler record and answer ceiling before invoking handler. |
| Worker HELLO | `packages/@keld/kipc/src/transport.ts:3404` | Bounded bootstrap allocation within the same lifetime owner. |
| Worker PING | `packages/@keld/kipc/src/transport.ts:3466` | Bounded control partition and ordinary writer order. |
| Abandon and close | `packages/@keld/kipc/src/transport.ts:2328` | Bounded metadata, one abandon per CALL and one close per link. |
| Low-level writer callers | `packages/@keld/kipc/src/transport.ts:1193` | Reuse the same ledger implementation for each socket owner. |

Worker-created wire frames still enter its existing WriteQueue.
All main-to-Worker messages still use the current private FIFO.
Do not add a second MessagePort or a second ordering owner.

### D-3: One pre-retention ledger

Proposed private transport types, conditional on B-1 and B-2:

~~~ts
type OutboundClass = "data" | "answer" | "control";
interface OutboundLimits {
  readonly retainedBytes: number;
  readonly records: number;
  readonly answerBytes: number;
  readonly controlRecords: number;
}
interface OutboundTicket {
  readonly slot: number;
  readonly incarnation: number;
}
~~~

These names illustrate the internal contract. They add no public constructor.
The limits freeze when the link opens.
Reject nonintegers, unsafe arithmetic, impossible partitions, and undersized bootstrap reserves before spawning the Worker.

Use a fixed-capacity ticket table inside the existing shared control owner.
Separate byte and record credits within that one ledger.
Reserve both before retaining a payload, callback, timer, or mailbox message.
If partial reservation fails, roll it back before returning.
No producer waits in an unbounded queue for credit.

A slot state and incarnation identify each reservation.
Only the current transition can release that reservation.
The implementation must prove arithmetic bounds and exhaustion behavior before enabling reuse.
A naked slot index never authorizes release.

Worker release updates shared credit directly.
It must not require delivery of an acknowledgment to the parked main thread.
The ledger's internal additions do not change the Host wire format.

### D-4: Bytes, copies, and records

Count transport-owned backing allocations, not only visible payload lengths.
Make an exact-length immutable snapshot after admission and before asynchronous retention.
Do not retain an application's large backing buffer through a small view.

Declare and account for every transport copy:
the admitted snapshot, any mailbox clone, and the contiguous serialized frame.
Temporary overlap during copy counts too.
The charge must cover the chosen implementation's worst simultaneous retained copies.
A view, alias, or pooled buffer must not bypass this accounting.

No transfer-based optimization is assumed.
A proposed transfer may ship only after B-3 qualifies the pinned Bun semantics.
Caller buffers remain usable under the existing public contract.

Byte credit may release after the owning transport drops every relevant payload reference.
CALL record credit remains until both sides retire their operation state.
The caller must settle, and the Worker must retire its pending or abandonment entry.
A reply retained for later main delivery still owns a record.
Caller timeout alone does not release a Worker tombstone.

For a Host CALL, reserve its record before invoking application code.
Reserve the approved maximum answer size before granting that invocation.
If no reservation fits, terminate through the existing typed path before invoking the handler.
Never invoke a side effect, discover answer exhaustion, and silently discard the result.

B-1 selects total ceilings and partitions.
One legal maximum-size frame must fit the approved ordinary-data configuration.
Control reserves cannot admit arbitrary application data.
All partitions count toward the same declared total.

### D-5: Failure, cancellation, and ordering

Proposed early-refusal mapping: reuse KELD-IPC-005 with an owner-approved local diagnostic.
B-2 must approve that extension before any implementation depends on it.
Do not reuse the Host admission-capacity constructor without its documented dispatch preconditions.

For sendEvent and callBlocking, local refusal throws before dispatch.
For call, local refusal rejects without creating a pending transport record.
The failed request writes no bytes and leaves a healthy link usable.
Code alone never authorizes automatic replay.

Reuse existing I/O and caller deadlines.
Do not reset a caller's original deadline while it waits in a bounded writer.
A write failure after possible byte emission poisons the queue.
Keep existing KELD-IPC-001/006 and terminal cause propagation where their contracts apply.

Cancellation records one terminal decision.
Reservation release follows actual ownership retirement, not that decision alone.
Close seals admission first.
Queued work follows the existing GH527 ordered-close contract.
A lost Worker may leave resources awaiting physical teardown.
Report that state instead of publishing fabricated zero retention.

A successor role gets a fresh ledger through existing lifecycle ownership.
Old callbacks never decrement the successor's counters.
No new reconnect path is introduced.

### D-6: Alternatives and current external semantics

Reuse the existing Writer, Worker, shared control block, and channel contracts.
A larger queue does not fix unbounded retention.
A high-water callback alone cannot bound messages already posted.
A main-thread acknowledgment alone cannot support parked progress.
Changing sendEvent into a Promise-returning API would expand the public contract without need.

FACT: Bun documents manual handling of socket backpressure.
Its TCP documentation says "manage backpressure manually with the drain handler."
Source: https://bun.sh/docs/runtime/networking/tcp
Retrieved 2026-11-05. Current documentation is not a pinned-runtime measurement.

FACT: Bun describes message serialization using "HTML Structured Clone Algorithm."
Source: https://bun.sh/docs/runtime/workers
Retrieved 2026-11-05.
This establishes why mailbox retention belongs in the census.
It does not establish a precise copy count or memory ceiling for the required Bun version.

## 5. Boundaries

Future implementation stays within the existing transport owner and its real callers:

- `packages/@keld/kipc/src/transport.ts` and its contract/Worker tests.
- Existing transport generation, stamping, and package tests when their inputs change.
- IPC error-owner declarations only under B-2's approved amendment.
- Shared GH527 architecture text only through its owner-approved amendment.

This PR adds only this specification.
Do not edit producer ordering fixes, native Host capacity, role custody, or active Windows transport branches.
Do not add a schema, dependency, or Host wire message for local accounting.

## 6. Tasks

### T-1: Select capacities and freeze error semantics

- **Blocked by:** none
- Inventory every D-2 caller and retained object at the selected current revision.
- Measure ordinary and stalled-peer demand without enabling a new production path.
- Close B-1 and B-2 with the transport and error owners.
- Record exact numeric ceilings, units, copy accounting, and public failure fixtures.

### T-2: Add failure-first admission and lifetime fixtures

- **Blocked by:** T-1, B-1, B-2
- Register AC-1 through AC-13 before the implementation change.
- Use an independent peer transcript and retained-object census.
- Prove the baseline crosses the intended bound in the stalled-peer case.
- Keep expected failure explicit. Do not classify the future feature as passing.

### T-3: Implement disabled integration and qualify the actual runtime

- **Blocked by:** T-2
- Integrate the ledger at every existing producer and writer seam.
- Run all ACs and their one-fault mutants on the pinned Bun/runtime matrix.
- Measure copies, retained records, process memory, and teardown separately.
- This work produces the evidence needed to close B-3.

### T-4: Activate the reviewed contract and reconcile consumers

- **Blocked by:** T-3, B-3
- Update existing callers for explicit early refusal without automatic retry.
- Preserve ordering, typed errors, generated/stamped transport ownership, and installed-package tests.
- Publish the exact-head acceptance and independent review before activation.

## 7. Test plan & evidence

| Family | Criteria | Independent oracle |
| --- | --- | --- |
| Stalled peer and all producers | AC-1, AC-3, AC-10, AC-11, AC-13 | Peer bytes, actual admission, handler effects/count and separate retained-object census |
| Sliced, pooled, mutable, empty, maximum buffers | AC-2, AC-4 | Backing-store sizes and peer digest |
| Deadline, reply, close, poison, and retirement cuts | AC-5, AC-6, AC-7, AC-9, AC-12 | Distinct caller/Worker retirement events and terminal errors |
| Blocking caller with an active Worker | AC-8 | Barrier-controlled progress and original call deadline |

Test zero, exact capacity, and capacity-plus-one for bytes and records independently.
Hold a real peer after HELLO instead of delaying it with sleep.
Use explicit read/write and callback barriers.
Run hostile teardown in an isolated child.
Observe Worker exit and retained-resource release separately from a close request.
Measure both event orders at each race.
A wrapper counter cannot serve as its own bounded-memory oracle.

### E-1: Current transport source inspection

- **Run:** Read current transport producer, WriteQueue, pending-map, shared-control, and error paths through GitHub.
- **n:** One pinned main revision, eight producer/retention categories, zero product runs.
- **Environment:** Cloud documentation workspace on 2026-11-05.
- **Limits:** Source identifies current ownership and retention paths. It establishes no runtime memory or throughput result.

### E-2: Current external documentation

- **Run:** Resolve Context7 /oven-sh/bun, query socket/Worker semantics, then confirm the official Bun pages in D-6.
- **n:** Two official documentation pages, zero pinned Bun execution runs.
- **Environment:** Documentation retrieved 2026-11-05.
- **Limits:** Runtime clone behavior, termination quiescence, and concrete retained-memory costs remain subject to B-3.

## 8. Review gates triggered

Public API: changed failure behavior requires exact-content approval.
Permission model: no new authority proposed.
Wire protocol: no Host wire change proposed. GH527 still reviews internal ordering and termination.
Dependency addition: none proposed.
Unsafe: none proposed.
B-2 owns code allocation or reuse. This draft cannot self-approve that gate.

## 9. Perf impact

The target is bounded retained work, not an unmeasured speed improvement.
Report logical bytes, operation records, and process memory separately.
Process memory includes allocator and runtime overhead beyond the ledger.
Measure throughput and latency with the same workload before and after.
Retain the existing architecture 01 budgets and regression-review rule.
A lower queue count does not alone prove lower resident memory.

## 10. Open questions, blockers & unknowns

### B-1: Capacity and copy-accounting selection

- **Status:** open
- **Owner:** Monish Khandelwal with the GH527 transport owner
- **Impact:** Blocks production limit values, shared-layout approval, and implementation after T-1.
- **Check:** Publish measured demand, exact byte/record ceilings, answer/control partitions, and a conservative copy-accounting rule.

### B-2: Local refusal and shared-contract amendment

- **Status:** open
- **Owner:** GH1101 error owner with GH527 transport owner
- **Impact:** Blocks implementation of new refusal behavior and any claim that IPC-005 already covers this local case.
- **Check:** Approve exact code/message/fix, no-effect distinction, caller behavior, and the corresponding canonical contract amendment.

### B-3: Native runtime qualification

- **Status:** open
- **Owner:** GH623 implementation owner and the operators of each claimed OS
- **Impact:** Blocks activation and any Bun/OS memory or compatibility claim. It does not block qualification work in T-3.
- **Check:** Publish exact-version runs for all ACs, mutants, retained storage, teardown, and a healthy successor on each claimed platform.

### U-1: Concrete Bun retention and termination behavior

UNKNOWN: Exact mailbox copy overlap, pooled-buffer behavior, and post-termination retention for the selected Bun build.
T-3 must measure these on the actual runtime.
Node compatibility or current documentation cannot substitute for that receipt.

### U-2: Existing caller handling of synchronous refusal

UNKNOWN: Every installed consumer's response to the proposed sendEvent refusal.
T-1 must enumerate actual callers.
T-4 must preserve application outcomes or keep the affected consumer unqualified.


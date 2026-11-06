# Spec: Attributed measurement of the actual one-way IPC facade
Status: draft
Linear: GH-417 (#517) · Owner: @0monish (exact-content approval pending) · Updated: 2026-11-06
Source baseline: 0eb3ce203187f35c5276a4237189c906a5220d5a
Publication: future evidence contract; existing parked measurement condition remains.

## 1. Goal & non-goals

Define the smallest reproducible experiment that can attribute one-way facade cost to actual work, copies/queues and engine
delivery after that path exists. Measure webContents.send fan-out and ipcRenderer.send separately, preserving their accepted
ordering, payload and authorization semantics. The output is a per-hop evidence table, not an optimization chosen in advance.

Non-goals: implement batching, pooling, shared memory or a second queue; add a benchmark service; change payload/credit
limits; reinterpret a byte-length observation as a physical memory copy; turn wire-only round-trip evidence into one-way
application latency. This specification does not activate the parked measurement campaign.

## 2. Spec refs

- [GH417 decision packet and measurement hold](https://github.com/gyldlab/keld/issues/417#issuecomment-6022095682).
- [GH431 performance claim prerequisites](https://github.com/gyldlab/keld/issues/431).
- [GH507 existing compatibility-overhead owner](https://github.com/gyldlab/keld/issues/507).
- [Architecture 02 transport ownership and hot path](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/02-ipc.md).
- [Existing addressed event specification](https://github.com/gyldlab/keld/blob/c4e5f3973807a7435cbf10fa9ec1234f5703f274/docs/specs/gh543-addressed-renderer-events.md).
- [Existing measured payload contract](https://github.com/gyldlab/keld/blob/c4e5f3973807a7435cbf10fa9ec1234f5703f274/docs/specs/gh545-measured-renderer-payloads.md).
- [Existing bounded outbound owner](https://github.com/gyldlab/keld/blob/819c347f9e0e000af2f438311b63f31c76077973/docs/specs/gh623-bounded-outbound-budget.md).
- [Public metric runner and output ownership](https://github.com/gyldlab/keld-benches/blob/a3d56a33b4f4a876bb1e5830dbada8113abf583c/HARNESS-CONTRACT.md).
- [Current metric registry](https://github.com/gyldlab/keld-benches/blob/a3d56a33b4f4a876bb1e5830dbada8113abf583c/schema/metrics.v1.json).

The issue's allocation and 4-KiB/one-pending statements describe its October source snapshot. This draft does not assert that
snapshot as today's measured limit or qualify an unmerged replacement. Reconcile the actual selected facade/transport head,
current admission contracts and workload first. Existing event/payload/budget drafts remain drafts, not executed behavior.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1 — Real-path prerequisite

Given a proposed experiment, it identifies the actual one-way path, callers/handlers and accepted payload/order/authorization
contract at the exact tested revision. A constant-result shim, direct internal call or standalone echo cannot pass as an
application facade path. If the facade is absent or an owner prerequisite is missing, the campaign remains parked.

### AC2 — Workload census

Given the pinned original-app workflow, retain observed message directions, unique channels, recipient counts and actual
encoded sizes, including the largest valid one-way payload. A guessed ceiling or source string length cannot substitute for
encoded length. Route an oversized/reordered/dropped real message to its correctness owner; it is not a performance waiver.

### AC3 — Causal delivery and ordering

Given identifiable messages and recipient windows, the observer matches each intended actual delivery and preserves the
existing per-source/recipient ordering contract. Missing, duplicate, foreign or reordered deliveries fail correctness before
any performance summary. A send-return timestamp without a receiver observation measures only admission and is labeled so.

### AC4 — Stage and allocation attribution

Given the source/hop ledger, serialization, transport buffering, native bridge and engine delivery costs each name their
owner and observable. Counting allocations requires the appropriate runtime/native instrument and retained configuration.
A source scan, buffer length, estimated capacity or host-only allocator cannot become a claim about Bun or engine allocation.

### AC5 — Queue bounds and slow recipients

Given a slow or non-reading recipient, observe admitted/retained work, refusal and healthy-recipient progress under the
existing budget owner. A hidden unbounded facade queue or a dropped message presented as throughput success fails. Retained
bytes, total allocated bytes and process memory are distinct metrics; do not derive one from another without an owner proof.

### AC6 — Clock scope

Given sender and receiver observations, use the existing clock contract. A one-way interval is valid only with a proved common
monotonic clock domain or an approved calibration/error bound. Otherwise report local-stage duration or separately registered
round trip, not fabricated one-way latency. Clock uncertainty larger than the claimed effect defeats the comparison.

### AC7 — Valid samples and observer effects

Given the complete run, record attempts, warmup/cache policy, independent sessions, instrumentation state and invalidations.
Use the existing registered sample/statistic policy; do not treat 100,000 iterations within one process as 100,000 independent
sessions. Uninstrumented/instrumented control runs establish whether tracing changes the behavior being compared.

### AC8 — Candidate equivalence before improvement

Given an experimental batching or pooling candidate separately approved by its owner, run identical semantic controls against
baseline and candidate: values, errors, authorization, ordering, close/restart and slow-recipient behavior. A faster candidate
that loses or reorders a message fails before any gain is claimed. No candidate is selected by language or mechanism alone.

### AC9 — Artifact and raw evidence

Given a reported result, exact fixture, app/facade/transport/native source, runtime images, engine/OS and observer modules bind
the raw data and summary. Swap a binary/module, omit a raw session or attach another OS receipt and eligibility fails. Use the
existing schema/result validator; an unregistered metric cannot be published as a registered performance result.

### AC10 — Decision boundary

Given an attributable measured stage and valid comparable end-to-end result, only the existing owner may open an optimization
slice with the named unmet requirement and correctness fallback. No dominant cause or no reliable difference means retain
the simpler path. A library echo, source line count or isolated instrumented microbenchmark cannot alone justify facade
replacement or a public compatibility performance sentence.

## 4. Design

### 4.1 Owner ledger

No runtime boundary change. KEL80 owns transport copy/queue admission, KEL90 the metric interface, KEL129 census/budget rules,
GH507 the compatibility-overhead lane and GH465/GH543 the actual facade delivery path. GH545/GH623 retain payload and retained
budget ownership. This document is a consumer experiment definition; it introduces no second transport/allocator policy.

| Independent atom | Inputs and observable | First falsifier |
|---|---|---|
| Census | Exact app/role/window/recipient identities and complete workload | Remove one intended recipient without changing the denominator |
| Work | Real message flow and runtime-specific allocation observations | Replace the actual call with an echo or count Rust-only allocations as Bun allocations |
| Queue/copy | Owner instrumentation for stage, queue retention and explicit copy events | Treat a Data/buffer length as a demonstrated physical copy |
| Clock | Specified same-domain/corrected timestamps or explicitly local duration | Subtract unrelated process/OS clocks without calibration |
| Statistic | Raw valid/invalid attempts and independent sessions | Omit slow failures or inflate sample independence |
| Artifact | Source/live-image/observer identity and retained raw files | Reuse a receipt after changing a runtime image |

Hidden coupling is recorded as an edge: instrumentation may change work/clock; workload sizes affect queues; admission refusal
changes delivered count. Record that coupling rather than comparing unlike denominators. A pure source ledger can describe
known copies, but cannot estimate their measured cost without the corresponding instrument.

### 4.2 Smallest experiment

First refresh the real facade and largest one-way workload demand. Reuse existing OS-qualified benches fixture and runner
slots. Select one direction, one OS/engine, a narrow size/recipient set grounded in that demand and the existing registered
metric contract. Measure baseline first. Separate Bun, Rust and engine stages in output; do not collapse them into one number.
Use a finite, declared run bound and unique per-message values. The historical 100,000-send suggestion is diagnostic scope,
not a newly mandated publication sample threshold.

If one-way end-to-end timing lacks an accepted common clock, report local source/receiver stage measurements and an explicitly
separate round-trip metric. Do not solve the missing clock by a new production timestamp protocol. Native platform semantics
and instrumentation require current primary documentation and an actual OS receipt before that lane is claimed.

### 4.3 Reuse and candidate handling

Read the existing writer/reader and guard/dispatch code before proposing instrumentation. Extend the owning runner's hook if
needed; a new metric goes into its one registry. No framework fixture lives at the repository root or in Keld docs. A research
controller may produce diagnostic raw evidence only under the benches contract; it is not a parallel scored harness.

A batching comparison is optional and stays out until separately approved. Preserve the existing immediate error boundary,
receiver identity and causal order. Never hide refusal/lost work by increasing a measurement queue. Accept a more complex
candidate only after semantic equivalence and a reliable attributed end-to-end gain; otherwise retain baseline.

### 4.4 Runtime seam and migration unit

Runtime seam: none in this draft. The future observation unit includes original callers, guard/transport handlers, native
engine callback, recipient observers and raw-result consumer. Config, source, admission/budget and instrumentation settings
are captured before each independent session. No OS grant, wire field, principal or callback lifetime is changed. A diagnostic
hook is owned by the existing runner and must not become an alternate shipping delivery path.

## 5. Boundaries

This publication writes only this spec. Future diagnostic fixtures belong in gyldlab/keld-benches under the actual OS and
existing runner. Any instrumentation in Keld requires the owning transport/facade/native claim and applicable review gates.
Do not implement pooled buffers, add a queue, change payload/credit policy, alter a frozen metric schema or touch a shared
baseline/seal as part of this documentation task.

## 6. Tasks (ordered, each independently reviewable)

- [ ] T1 Reconcile the parked predicate, selected real facade revision, workload size/recipient census and existing metric/clock owner; record a specific blocker if any is absent.
- [ ] T2 Add only needed diagnostic fixture/hooks through existing owners and prove missing-delivery, wrong-recipient, hidden-queue and wrong-identity controls.
- [ ] T3 Run the bounded baseline experiment on the actual OS/engine, retaining all sessions, clocks, observer effects, queue/copy evidence and uncertainty.
- [ ] T4 Independently recompute and review the attribution; publish only qualified metrics, then propose a bounded optimization only if the evidence identifies a worthwhile owned stage.

## 7. Test plan

AC1/2/3 use the actual source-pinned app/facade and delivered-message census. AC4/5 use independently configured runtime/native
instruments and slow/no-consumer controls. AC6 tests clock-domain assumptions before calculating an interval. AC7 retains
all raw independent sessions and instrument controls. AC8 runs semantic regression controls before any candidate comparison.
AC9/10 use existing schema/registry/publication admission and reviewer recomputation with wrong-source/omitted-session negatives.

Synchronize on observed send/admission/delivery/retirement, not sleeps. Timed-out operations remain failed/unknown attempts;
they are not replayed as completed work. No measurement, native test or optimization ran during this documentation task.
Ten-section/limit/source checks, independent review and exact GitHub readback verify only the draft. Local product
fmt/Clippy/workspace tests, hooks and machine closeout are unrun.

## 8. Review gates triggered

None for production behavior in this draft. Future runtime/native instruments and any optimization retain their actual
public API, permission, dependency, unsafe and wire gates when triggered. Metric/clock/statistic admission stays with its
existing owner. This publication does not approve a new field, helper, runtime or campaign.

## 9. Perf impact

No number or improvement is claimed. The purpose is to establish attribution and uncertainty before design. Architecture
budgets and the existing regression rule remain unchanged; observer overhead is reported rather than assumed negligible.

## 10. Open questions

1. The actual one-way facade, admitted payload/budget contracts and GH431 prerequisites must qualify before execution; the historical measurement hold is preserved.
2. Existing metric owners must choose/approve the exact per-direction metric and clock interpretation before publication. Missing common-clock evidence cannot be guessed.
3. Native owners must select the available runtime/engine allocation instrument and retain its primary-doc receipt and perturbation control.
4. Exact-content approval remains pending. Any later candidate optimization requires its own evidence-backed owner decision.

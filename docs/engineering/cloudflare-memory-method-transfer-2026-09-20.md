# Cloudflare memory methods: a Keld investigation proposal

Date: 2026-09-20. Status: **research proposal; no implementation or new performance results**.
Source-inspected Keld revision: `f7865b9fc2e548e5385ea2d28c1216ad60f17b93`.

## Decision and scope

Investigate aggregate admission and long-session resource lifetimes before attempting
representation optimizations. The first candidates are the TypeScript outgoing queue,
virtual-port registry lifetime, and read-only message storage. All proposed numeric
budgets, changes, and performance benefits still require evidence and owner approval.

This is an engineering method-transfer note, not a release certification, replacement
architecture, or full technical audit. The public audit registry remains
[docs/audits/README.md](../audits/README.md). No historical audit is modified.
Current product capability remains owned by
[product-status.md](product-status.md); measurements remain owned by the
[budget scoreboard](budget-scoreboard.md) and public
[keld-benches](https://github.com/gyldlab/keld-benches).

The code observations below apply only to the pinned revision and inspected boundaries.
They are not proofs of a product leak, reachable denial of service, measured regression,
or whole-application memory contribution. Refetch current source and active work before
implementation. A higher-level bound or a shorter lifetime can change practical impact.

## Primary-source ledger

| Source | Scope and supported use |
| --- | --- |
| [CF-MATH: Saving another 100TB of RAM with math (and Rust)](https://blog.cloudflare.com/saving-100-tb-of-ram-with-math/) | Cloudflare, 2026-09-18. Data representation, parameter justification, configuration multiplicity, finite-model checks and rollout methods; not Keld performance evidence. |
| [CF-DNS: How we saved 100 terabytes of memory by optimizing 1.1.1.1's DNS cache](https://blog.cloudflare.com/dns-cache-memory-optimization-1111/) | Cloudflare, 2026-08-27. Immutable storage, consolidation, context-dependent elision, enum cost, encoded storage and construction reuse; not a desktop workload or allocator guarantee. |
| [K-TS: transport.ts at the inspected revision](https://github.com/gyldlab/keld/blob/f7865b9fc2e548e5385ea2d28c1216ad60f17b93/packages/%40keld/kipc/src/transport.ts) | `FrameReader`, `DirectedReader`, `DrainSignal`, `WriteQueue`, `writeOneFrame`, `withIoDeadline`, socket lifecycle handlers. |
| [K-PORT: virtual_port.rs at the inspected revision](https://github.com/gyldlab/keld/blob/f7865b9fc2e548e5385ea2d28c1216ad60f17b93/crates/keld-runtime/src/virtual_port.rs) | Inline limits, immutable payload API, pair/endpoint state, transfer, close, receive, revocation and generations. |
| [Rust Vec guarantees](https://doc.rust-lang.org/std/vec/struct.Vec.html) | Capacity and owned storage; distinguish metadata from allocation and physical residency. |
| [Rust VecDeque](https://doc.rust-lang.org/std/collections/struct.VecDeque.html) | Clearing elements and retaining container capacity are different operations. |
| [Rust type layout](https://doc.rust-lang.org/reference/type-layout.html) | Alignment, padding and representation constraints; actual target layout must be measured. |
| [Rust Option representation](https://doc.rust-lang.org/std/option/index.html#representation) | Documented niche/null-pointer optimizations; not every enum needs a separate extra tag. |

Source-intake date: 2026-09-20. A later same-session attempt to refresh both article
bodies returned a web-cache miss; the official Cloudflare index confirmed the titles
and dates. The earlier intake is retained, not described as a newly successful body
fetch. The Keld observations were independently re-read from the pinned public files.
Current standard-library documentation does not establish availability of every API
on Keld's pinned toolchain; implementation must verify that toolchain. No dependency
or allocator is selected here.

## What transfers, and what does not

CF-MATH illustrates two distinct levers: reduce storage per entry and reduce how many
entries are needed. Alignment made a nominally smaller field insufficient on its own;
parameter reduction required an explicit distribution model, checks against the actual
finite hash behavior, and controlled transition. Its consistent-hash variability model
is not an IPC queue bound, a tail-latency guarantee, or a desktop workload distribution.
Its fleet saving does not predict a Keld percentage.

CF-DNS treats cache entries as long-lived immutable data rather than permanent mutable
construction objects. It combines exact-sized storage, related-list consolidation,
omitting reconstructible values, reducing rare-variant overhead, encoded bodies with
structured metadata, and reusable construction storage. Those choices are workload and
lifetime dependent. Its synthetic allocation accounting and production memory
observations are different evidence layers. The useful transfer is the measurement and
ownership method, not its DNS record mixture, integer widths, allocator choice, cache
implementation, or reported fleet result.

Keld does not need a DNS cache, consistent-hashing ring, Pingora dependency, generic
resource-governor framework, new shared-memory path, or runtime replacement to use these
lessons. Correct admission and reclamation can matter even when idle-memory savings are
small. Optional representation experiments must not delay the current TS/Bun product
path or the approved bounded echo-codegen implementation.

## Pinned observations and unknowns

### Outgoing admission and time

**Source fact, K-TS:** `WriteQueue.writeFrame` rejects an oversized individual payload,
serializes writes through a promise chain, and poisons subsequent writes following a
serialized failure. The inspected class has no aggregate pending-byte or pending-call
admission limit. Closures retain their payload references. `writeOneFrame` constructs a
contiguous header-plus-payload frame and starts its monotonic I/O deadline when that
write begins.

**Inference:** a producer backlog can retain storage and accumulate queue residence
before the active write deadline starts. **Unknown:** higher-layer producer limits,
actual workload frequency and full-product contribution. Preserve admission rejection
without poison, partial-write poisoning, ordering and monotonic deadlines.

### Aggregate resources and terminal state

**Source fact, K-PORT:** defaults are 64 queued messages per endpoint and 4,096 inline
payload bytes. The send path checks admission before copying. `create_pair` validates
principals but does not impose a total-pair admission limit inside this registry.
Pair and revoked-generation records are retained by the inspected implementation;
revocation scans pairs and clears affected queues. Plain close changes lifecycle flags;
receive can still access queued messages subject to its ownership/revocation checks.

**Conditional arithmetic, not a measurement:** two fully occupied default endpoints
hold `2 * 64 * 4096 = 524288` payload bytes. One hundred such pairs hold 50 MiB of
payload, excluding metadata. This is not idle memory or eager preallocation.

**Unknown:** intended post-close drain/discard semantics, registry lifetime, callers'
aggregate bounds and the practical size of retained history. Reclamation must preserve
fresh generations, transfer-once behavior, original-owner checks and disconnect
observation. `initial_owner` is not redundant with `owner`: transfer and revocation use
the distinction. Removing it because the values often match would be unsound reasoning.

### Representation and receive storage

**Source fact, K-PORT:** `PortMessage` owns a `Vec<u8>`, is constructed from a slice,
and exposes a read-only `as_bytes` API. This is a concrete candidate for an exact-sized
owned-storage experiment. It is not evidence that this construction has excess capacity.

**Source fact, K-TS:** `FrameReader` uses owned chunks and cursors instead of repeatedly
concatenating the whole buffered prefix. It releases fully consumed chunk references.
The ownership copy deliberately avoids assuming that callback storage remains safe to
retain. Complete payload and outgoing-frame construction also copy bytes.

Two distinct accounting questions survive inspection:

* `push` makes its owned chunk copy before the later post-processing byte/fragment
  checks. Actual callback size bounds and legal coalesced traffic determine the relevant
  transient envelope. A post-processing cap alone is not a demonstrated peak-allocation
  bound. No production failure has been reproduced.
* `bufferedBytes()` explicitly reports unread bytes. A partly consumed head can retain
  its complete backing allocation until fully consumed. That counter is not mislabeled
  in the source, but using it as total retained-memory evidence would be incorrect.

### Auxiliary references and cleanup

**Source fact, K-TS:** `DirectedReader` limits parked frame count; actual retained bytes
must be derived from the admitted policies and payloads, not an unrelated maximum.
`DrainSignal` retains callbacks until fired. A deadline race does not itself unregister
the waiter. However, the socket close, error and connect-error handlers already call
`drain.fire()`. **Unknown:** timeout-to-teardown lifetime and practical retention.
Treat this as ownership tracing, not an established leak. A count bound, byte bound,
fairness property and end-to-end latency bound are four different obligations.

## Twenty-four method-to-action dispositions

All action rows are proposals. "Investigate" justifies a test, not a predetermined fix.
Owners identify existing subsystem responsibilities, not new abstractions or teams.

| ID | Application and disposition | Owner and first falsifier/completion check |
| --- | --- | --- |
| M01 | Investigate total pending bytes and calls, including active storage. | Transport: trace every producer; bounded local slow-consumer fixture settles all accepted/rejected calls within the declared resource contract. |
| M02 | Separate queue residence, active I/O and end-to-end time. | Transport: independent timing observables; no wall-clock regression or implicit unbounded wait. |
| M03 | Compose endpoint, pair, principal and registry limits. | Role/transport owners: one checked, unit-labeled resource ledger; local bounds cannot stand in for global bounds. |
| M04 | Separate payload destruction, container capacity, history and physical residency. | Role/lifecycle owners: constant-live-work churn stabilizes the declared logical state; OS memory is observed separately. |
| M05 | Reclaim only after semantic obligations end. | Role owner: stale identities remain invalid, disconnect is observed correctly, and terminal cleanup cannot revive a capability. |
| M06 | Experiment with immutable exact-sized message storage. | Transport/role owners: actual target layout, occupancy, allocations, clone/drop and latency improve; no assumed Vec spare-capacity saving. |
| M07 | Consider consolidated immutable arrays only for shared access/lifetime. | Transport owner: identify a real repeated structure; offsets preserve order and use checked bounds. Otherwise defer. |
| M08 | Omit data only when reconstruction has a trusted surviving context. | Role owner: transfer/restart cannot substitute identity; preserve `initial_owner` and authentication semantics. |
| M09 | Profile variant frequency and weighted enum storage. | Budget/measurement owners: include inline size, rare heap allocations, size-class rounding and access cost; no hot enum established here. |
| M10 | Consider encoded bodies plus structured metadata only after attribution. | Transport/schema owners: exact values, bytes, validation and lifetime remain equivalent; no change to current echo-codegen scope. |
| M11 | Bound reusable scratch and its high-water mark. | Transport/measurement owners: outlier-then-small recovery, reentrancy and aggregate worker retention are measured. |
| M12 | Count retained backing allocations alongside copied bytes. | Transport/measurement owners: fewer copies must not hide longer retention or unsafe borrowed lifetime. |
| M13 | Measure actual target layout rather than adding field widths. | Budget owner: toolchain/target size and alignment plus enclosing storage; no blanket packed or unsafe changes. |
| M14 | Inventory multiplicity across roles, windows and generations. | Role/lifecycle owners: measured counts/lifetimes, no duplicated mutable authorization state or unproved engine sharing. |
| M15 | Budget waiters, parked frames, cancellation and diagnostics. | Transport/lifecycle owners: all terminal paths release references; overload does not move to an unbounded promise or log queue. |
| M16 | Fit parameters to declared useful work and quality. | Role/transport/budget owners: explicit memory, fairness and tail requirements under legal bursts; averages are not hard bounds. |
| M17 | Challenge model assumptions independently. | Evidence/test owners: finite-width, skew, saturation and boundary cases can falsify the model; no transplant of hash-ring math. |
| M18 | Measure isolated changes and their interaction. | Measurement owner: unchanged baseline, single-change arms and combined arm with matched correct work and uncertainty. |
| M19 | Treat synthetic distributions as hypotheses. | Measurement/evidence owners: sanitized desktop length/occupancy/variant data plus sensitivity arms; DNS traffic is not a proxy. |
| M20 | Account for temporary old/new coexistence. | Role/lifecycle owners: bounded transition and rollback where versioned state actually exists; no invented migration framework for ephemeral objects. |
| M21 | Prove the oracle detects its named incorrect behavior. | Test owner: bounded local negative controls fail independent assertions while valid work still progresses; tests stay colocated. |
| M22 | Separate mechanism, outcome and public claim. | Research/evidence/budget owners: source inspection and smaller type layout never become unrun full-product benchmark results. |
| M23 | Investigate pre-check receive-copy peaks. | Transport owner: model actual callback bounds and construction overlap without rejecting legal coalesced traffic or removing safe ownership. |
| M24 | Distinguish unread length from retained backing size. | Transport/measurement owners: account once for full retained buffers, partial heads and output copies; label each counter honestly. |

## Resource model and decision order

Use one ownership ledger rather than summing overlapping counters:

`retained storage = inline/slot storage + owned backing allocations + auxiliary state + required history + simultaneous construction overlap`.

Spare capacity is part of the corresponding backing allocation, not another allocation
to count twice. Record lower-bound requested bytes, measured allocator overhead and
OS-resident counters as separate layers. Attribute kernel/engine buffering separately
where observable. Mark inaccessible counters unknown rather than zero.

For a fixed workload and chosen memory denominator, let `f` be the fraction attributable
to a candidate and `r` the removable fraction. Its direct whole-product improvement is
`f * r`, before secondary effects. Use this arithmetic to reject complexity that cannot
materially move the endpoint; it is not a guarantee about OS residency or latency.

Order the work: establish ownership and existing bounds; close a demonstrated
boundedness/lifecycle gap; attribute remaining costs; run one representation experiment;
then evaluate combined full-product effects. Do not shrink constants blindly to make
memory charts look better. Admission behavior, valid throughput and foreground/control
progress are part of the contract, not optional side metrics.

## Four bounded experiments, all currently unrun

### E1: outgoing admission and receive accounting

Trace producers, payload mutation/ownership, active storage, queue closures, parked
frames and waiters. Compare the current baseline with the smallest justified admission
change on equal legal work. Record pending bytes/calls, retained backing, transient
peaks, queue age, active-write time, completion/rejection/cancellation counts and
end-to-end latency. Include normal, burst, slow-consumer, recovery and close paths.
Use independent ceilings and a resource-capped local incorrect variant to prove the
oracle; do not load-test production or interpret rejection of all work as success.

### E2: lifecycle churn

Hold live work constant while varying the number of completed window/role/pair
lifecycles. Separately observe payload destruction, queue capacity, terminal records,
revoked identities, callback/timer/waiter ownership, disconnect and revocation work.
Include drain-after-close and generation loss according to the established contract.
A flat average or a resident-memory high-water mark alone cannot decide logical leakage.
Cleanup must not remove correctness-required state before its obligation ends.

### E3: representation and copies

Start with `PortMessage` rather than an invented global cache. Compare current storage
with exact-sized owned storage using real occupancy and the pinned target/toolchain.
Record layout, requested/retained heap, allocation count, clone/drop, CPU and latency.
Only add scratch, consolidated-array, enum or encoded-body arms after a real consumer
is profiled. Include empty, ordinary, rare/large and outlier-then-small cases. Account
for temporary dual representations, slice-retained backing and allocator rounding.
One change at a time first; retain a combined arm to detect interactions.

### E4: full-product effect

Reuse the current benchmark runner, metric registry and artifact schema rather than
introducing a parallel version. Preserve existing results and their exact admitted
scope. Rust-library transport, actual Bun-host exchange, renderer round trip and full
application are different layers; existing evidence at one layer does not establish
these new memory experiments at another.

Measure the required host, Bun, webview and helper process population, with shared-page
accounting explicitly named. Report idle, load, burst recovery and teardown separately.
Record immutable source/fixture SHAs, actual runtime/engine/allocator, OS/architecture,
build flags, cache/power conditions, repetitions, raw samples and uncertainty. Separate
instrumented attribution from scored timing and quantify its overhead. Compare equal
successful useful work; disclose rejections, failures and fidelity changes. No platform
inherits another platform's result. Publish only executed fixtures and raw receipts in
OS-qualified keld-benches homes.

## Rejected shortcuts and acceptance boundary

Do not introduce blanket `repr(packed)`, shrink identities, remove generation history
without proof, preallocate maximum frames for every owner, create unbounded interning,
share mutable authority state, bypass validation with cached encoded bytes, or remove
the receive ownership copy based on an undocumented lifetime assumption. Any future
permission-decision cache needs complete authenticated identity/policy/generation keys
and invalidation proof under its existing owner; this note proposes no such cache.

A smaller struct is not automatically smaller total heap. A box can add indirection or
allocation overhead; eligible Option representations can avoid extra tags. A bounded
queue is not automatically fair. Fewer allocations are not automatically faster. A
smaller idle number is not proof of stable long-session behavior. Vendor fleet results
are not Keld results.

No numeric budget, API, wire, permission, dependency, unsafe code or implementation
approval is created by this note. Applicable specifications, independent review and
normal validation gates remain required. The first useful deliverable is the producer
and resource-lifetime ledger plus the smallest independently falsifiable regression,
not a framework rewrite or a superiority claim.

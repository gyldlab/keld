# Spec: Bounded renderer request liveness and fair admission
Status: draft
Linear: GH-1097 (#517) · Owner: @0monish (exact contract approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; documentation only, do not merge.

## 1. Goal & non-goals

Give each admitted renderer invoke one bounded Host decision, preserve truthful dispatch-proof errors, and prevent one hung
Bun handler from retaining every window's admission indefinitely. A timeout never proves rollback, cancellation or safe
replay.

No new error-code split, renderer-origin policy, arbitrary retry, automatic replay, general Bun outbound deadline redesign or
competing renderer-credit owner. GH546 owns capacity policy; GH1095 owns outcome classification. A blocked renderer event
loop cannot be promised immediate JavaScript callback execution.

## 2. Spec refs

- [Owning issue GH-1097](https://github.com/gyldlab/keld/issues/1097); current public owner decisions remain authoritative.
- [Current dispatch and pending Echo owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/app_session.rs).
- [Current native/page request state](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wkwebview/macos_bridge.rs).
- [Request classification draft r2](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh1095-request-error-codes.md).
- [Existing recovery ownership](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel143-macos-generation-filesystem-recovery.md).
- [Renderer bridge contract](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel142-macos-renderer-bridge-api.md).
- [Runtime supervisor](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/src/lib.rs).
- [Renderer credit-window owner](https://github.com/gyldlab/keld/issues/546).
- [Restart-ready notification owner](https://github.com/gyldlab/keld/issues/1098).

Verified source baseline: At the source baseline, start_renderer_dispatch waits in recv_timeout polling until reply, stop or
disconnection; the timeout is a stop-poll interval, not a reply deadline. PrimaryRouterHandle holds one pending_echo for the
session. BridgeState and the page facade also each hold one pending request. GH1095 r2 is still explicitly draft; its
proposed WV-012/WV-013 meanings are not inferred to be shipping from the presence of that document.

The proposed contract below is future work. This publication changes no runtime boundary and grants no implementation or
predecessor approval.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given a runnable live document and a Bun handler held after entry, when its Host deadline expires, then exactly one
unknown-outcome terminal reaches the page within the approved deadline plus qualified native-delivery bound.

Independent negative control: Disable the deadline while keeping the same held handler and observer; the original pending
call must remain visible and fail the bound.

### AC2

Given one hung call from window A and an admitted request from B, when the fair dispatcher runs, then B obtains service or a
truthful bounded pre-dispatch refusal under the frozen policy; A cannot indefinitely own the dispatcher.

Independent negative control: Keep today's blocking receive loop or let A reinsert ahead of all other documents; B's
independent admission/terminal census must fail.

### AC3

Given the Host attempted a CALL write, when timeout, navigation or link loss wins the terminal race, then the result is
GH1095's unknown class and the handler execution count for that logical action is at most one across generations.

Independent negative control: Record the dispatch-attempt bit only after a fully successful write, classify a partial-write
failure as definitely-not-dispatched, or replay the request on a successor.

### AC4

Given a request expires while still queued before any write attempt, when refused, then its classification reflects that
proof and no Bun handler ran.

Independent negative control: Dispatch a deadline-expired queue item after sending its pre-dispatch refusal; the independent
Bun handler census must detect the forbidden execution.

### AC5

Given reply and deadline are released in either order, when they race, then one terminal latch wins and the other cannot
overwrite it or release a reservation twice.

Independent negative control: Use separate unsynchronized timeout/reply completion paths; assert duplicate settlements or
negative capacity.

### AC6

Given a late reply after retirement, a reused page request number, navigation or a new Bun generation, when delivery is
attempted, then no result reaches the wrong document or newer request.

Independent negative control: Match only page-local request id or numeric PID and omit Host generation/correlation/document
lifetime.

### AC7

Given a timeout whose handler may still be running, when caller state is retired, then physical in-flight capacity remains
accounted until a reply or confirmed generation retirement; repeated timeouts cannot grow retained work without bound.

Independent negative control: Release all capacity on Promise rejection while the native/Bun work remains live, then repeat
until the independent retained-work census exceeds the approved bound.

### AC8

Given the selected terminal-retirement policy, when a stuck generation cannot release bounded work, then the existing
supervisor/recovery owner performs bounded retirement or a documented terminal failure; no ad hoc signal or successor replay
is used.

Independent negative control: Clear correlation state and start new work without native/link retirement evidence, or kill a
PID outside the retained owner.

### AC9

Given a temporarily non-runnable page event loop, when the Host deadline passes, then Host terminal timing and delayed page
observation are reported separately; no fabricated page-settled timestamp is accepted.

Independent negative control: Use the Host timeout timestamp as if the JavaScript Promise callback had actually run.

### AC10

Given each qualified macOS hosted arm, when published, then at least ten real runs print handler count, terminal count,
native document identity and capacity/retirement observations, with separate negative mutations.

Independent negative control: Publish a model-only or mock-host result as native liveness qualification.

## 4. Design

### 4.1 Atomic ownership model

Core's existing app-session owner owns request admission, Host monotonic deadlines, dispatch-attempt proof and physical work
reservations. keld-wv owns current native document identity and page delivery; the supervisor owns generation retirement. A
caller-terminal latch and an execution-reservation lifetime are distinct records connected by one Host request identity.
Their observable clocks and releases cannot substitute for each other.

### 4.2 Deadline policy proposal

Use one monotonic Host clock, starting at admission before waiting for a dispatch slot. Proposed default total budget is 30
seconds, captured at session creation, with a shorter trusted test policy for deterministic fixtures. This number is a policy
proposal, not measured evidence or an approved user-config surface. Renderer-supplied durations cannot extend it. T1 must
freeze the value, clock/suspension behavior and finite native-delivery allowance before implementation. The total budget
includes queue residence, CALL write and reply wait. A blocking write whose transport bound can exceed the remaining request
budget is not admissible; integrate the owning transport's deadline/progress mechanism rather than putting a timer only after
write completion.

### 4.3 Fair admission reuses GH546

Keep at most the currently approved pending count per document; any increase belongs to GH546. Replace the single blocking
receive loop with one owner that services ready request/reply/deadline events and a finite queue, using round-robin eligible
documents and FIFO within a document. The shared GH546 owner must approve queue/work capacities, admission cost and overflow
behavior; this document does not create a second credit ledger. A queued request has its own admission deadline, so time
behind another window counts against its bound.

### 4.4 Proof-specific terminals

A request denied as invalid/busy/expired before any Host write uses GH1095's applicable not-sent class; absence of an
admitted generation has its distinct definitely-not-dispatched class. Once any CALL write is attempted, timeout is unknown
even if the write later fails or the response is absent. Proposed deadline detail uses the existing unknown error carrier,
not a new literal code. GH1095 r2 exact approval and implementation are dependencies, not silently granted here.

### 4.5 Late results and retained work

Timeout settles the caller once but does not cancel an operation or refund its physical-work reservation. Retain the bounded
correlation/generation record until an actual reply or the generation's confirmed retirement makes late delivery impossible.
Refuse new work at the shared bound instead of accumulating orphaned callbacks. Do not use a timer to evict an identity that
can still authenticate. Correlation wrap/reuse must preserve the existing owner rule; page request-number reuse cannot
identify an old execution.

### 4.6 Recovery choice is explicit

Proposed fallback on an expired in-flight call is to stop new dispatch to that generation and request retirement through its
existing Host/supervisor owner, then allow its already approved successor policy. Timeout itself is not a crash signal or
cleanup receipt. All affected requests retain their own dispatch-proof outcomes; none is replayed. T1 must approve whether
this policy or an alternative with equally bounded retained work is used, plus the real retirement bound. Until then the safe
fallback remains refusal, not falsely qualified liveness.

### 4.7 Delivery and lifecycle

A live runnable native/page event loop is a prerequisite for observing JavaScript settlement within a delivery bound. Host
deadline enforcement remains observable separately when UI/page execution is blocked. Navigation/destruction retires only
that document's delivery rights and never converts unknown execution into no-effect. Session stop and generation replacement
settle eligible callers once and follow existing quiescence ordering. The restart-ready event is a later signal, never proof
that a timed-out operation failed.

### 4.8 Migration unit and compatibility

Change the shared request owner, dispatcher, native pending state, page rejection shape and tests together. No new private
decoder or replay queue. Keep the public invoke signature unchanged; remove the old indefinite blocking branch only after the
new terminal/capacity controls pass. Existing runtime outbound KELD-IPC-006 policy remains separate.

## 5. Boundaries

Future changes are limited to the current Core renderer dispatch/PrimaryRouter owner, native bridge request lifecycle and
their generated/public outcome consumers. GH546 owns credit policy; GH1095 owns codes; KEL-143/runtime owns retirement. No
renderer-minted identity, arbitrary process kill, broader permission or wire cancellation message is introduced.

This publication changes only `docs/specs/gh1097-renderer-request-liveness.md` and related grouped-PR/issue handoffs. Shared
architecture, policy, generated contracts, implementations and other owners' documents are not edited.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Resolve GH546 tracker/authority and freeze the joint finite capacity/fairness contract, deadline policy, native-delivery assumptions and generation-retirement choice; preserve GH1095 r2 exact approval.

### T2

- [ ] Add independent current-tree failure fixtures, then implement one Host request terminal owner with deadline-versus-reply races, dispatch-proof classification and bounded correlation retention.

### T3

- [ ] Integrate fair dispatch and existing generation retirement with independent caller/physical-work accounting, navigation and healthy follow-up controls.

### T4

- [ ] Run ten-or-more real hosted macOS repetitions per arm, publish raw censuses and negative mutations, and qualify any later engine separately.

## 7. Test plan

AC1/5 use a trusted controllable Host clock and explicit entered/reply/release barriers, plus a real elapsed hosted bound.
AC2 holds A while B proceeds or receives its specifically documented bounded refusal; all-B-busy forever cannot pass. AC3/4
count actual handler entry and irreversible sentinel effects independently of error text. AC6 releases old replies into a
newly created document with reused page ids. AC7/8 observe both live work and retirement, not just pending Promise count. AC9
deliberately blocks page execution. AC10 carries real native receipts; no hosted tests ran here.

Each acceptance criterion has its own observable and negative control above. Documentation verification covers all ten
sections, source-versus-proposal wording, file limits and exact GitHub readback. GitHub CI belongs to the exact published
head. Native/runtime/conformance/OS outcomes remain unrun until the implementing owner executes them; green documentation
checks do not prove those outcomes.

The user restricted this session to GitHub. Local `just ci`, hooks and machine session-closeout were not run. Official
immutable upstream GitHub sources are used where relevant; Context7 was not used under that source restriction. No
external-machine or native receipt is fabricated.

## 8. Review gates triggered

Public API/outcome semantics, runtime/lifecycle and shared credit-owner architecture review. Wire/permission changes are none
unless the owners select a different transport or retirement primitive, which requires an explicit amended gate before
implementation.

Draft publication is not exact-content approval. Pending predecessor approvals and native qualification remain separate
gates.

## 9. Perf impact

Bound admission queue, active calls and retired-but-live reservations using the single GH546 policy. Use an event-driven
deadline structure owned by the current dispatcher, not a thread/timer per request. Measure total retained work and fairness
latency under a hung handler; no measured guarantee is asserted.

## 10. Open questions (block implementation)

1. Resolve GH546's authority/tracker conflict and approve the single shared finite credit/fairness policy before
queue/concurrency changes.

2. Approve the proposed 30-second Host budget, captured configuration, suspension rule and real native/page delivery bound.

3. Approve bounded generation retirement after an in-flight timeout, or another equally bounded physical-work policy, with
the existing supervisor owner.

4. Obtain exact GH1095 r2 approval/implementation and native hosted evidence; the current source still has the earlier WV-011
behavior.

Independent review must cover the final published revision. Every remaining owner/native gate stays explicit; this document
is not a completed feature.


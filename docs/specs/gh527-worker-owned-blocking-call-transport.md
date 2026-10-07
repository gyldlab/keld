# Spec: worker-owned single-link blocking host CALL transport for Bun roles
Status: approved
Linear: GH-527 (#517) · Owner: @0monish · Updated: 2026-10-07

## 1. Goal & non-goals

A Bun role's main thread cannot block today without stalling its own link. The
`@keld/kipc` client is an event-loop `Bun.connect` client on the main thread
(`packages/@keld/kipc/src/transport.ts`), so a main thread parked in a blocking wait
stops reading. On macOS the host writer then fills the 8 KiB AF_UNIX send space and
fails `KELD-IPC-006` five seconds later (#418 arm A). Electron main-process code such
as `dialog.showMessageBoxSync` needs a blocking host call that returns a real host
value with no observable Promise.

This spec adopts **Design B**, which PANEL-P1 (#418) selected (resolution comment
6035712580; evidence on `research/electron-compat-map` at `46e59078`, under
`wayfinder/electron-compat/probes/panel-p1/`). A transport Worker owns the role
generation's one authenticated link for the whole session. It drains the link into a
bounded, ordered `SharedArrayBuffer` ring and wakes the parked main thread with
`Atomics.notify`. The observable outcome: a synchronous host CALL from the Bun main
thread returns the host's real REPLY, or throws a registered typed error. Host frames
that arrive during the park are kept in issue order and delivered after wake under the
wake-time rule in §4.6. The link never stalls while the main thread is parked.

Non-goals:

- the `showMessageBoxSync` facade and its re-entrancy behaviour (F06-T7, PANEL-D22);
- renderer `sendSync` (F04-A5, F04-T15), a different atom on the webview link;
- the `@keld/api` mirror primitive itself (F02-T2, #449). This spec only defines the
  ordering hook that the mirror relies on;
- any round-trip-time threshold. No performance number is a pass criterion (§9);
- principal minting, grants and the guard. The role's principal is unchanged;
- a second link, a second principal or a second endpoint per role generation;
- Linux and Windows qualification evidence. The contract is engine-neutral, the first
  proof is macOS, and T5 owns the other two operating systems.

## 2. Spec refs

- `docs/architecture/02-ipc.md` §1 (principal identity is link metadata; one link per
  role generation; KEL-75 role-instance contract), §2 (frame kinds, `ERR` payload,
  receiver semantics, backpressure), §7 (failure and lifecycle semantics). **This spec
  changes handle ownership.** The client end of the role's link moves from the Bun main
  thread to a transport Worker. The same PR amends arch 02 (listed in §4.12).
- `docs/architecture/01-overview.md` §1 item 2 (the app-process family; each child is
  a distinct host-minted principal with its own kipc link) and §5 (budgets).
- `docs/architecture/06-runtime-and-tooling.md`: Bun speaks kipc through the one
  TypeScript transport source. The Linux strict profile binds exactly `main.ts` and
  `kipc-transport.ts`. Not amended: ownership of the TypeScript source does not move.
- `docs/specs/kel136-generated-ts-app-link-transport.md`: one TypeScript transport
  owner (`FrameReader`, `WriteQueue`, `DirectedReader`). This spec extends that owner
  and does not add a second one.
- `docs/specs/kel133-kipc-receiver-semantics.md`: one validator, one canonical corpus
  (`crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv`) and one digest. This
  spec adds corpus rows (§4.9).
- `docs/specs/kel75-principalized-bun-child-roles.md`: role generation, `RevokeAll`
  and crash paths. Unchanged; Worker death maps onto its natural-crash row.
- `docs/specs/kel139-macos-product-spine.md` AC6: quit order (accept and attribute,
  reply, quiesce and drain, revoke and close, terminate and reap).
- Evidence: #418 (PANEL-P1 resolution), #419 (PANEL-P2 resolution, re-entrancy row
  E3), #449 (F02-T2 mirror staleness contract). Implementation: #528 (F04-T18).

## 3. Acceptance criteria (binary, each becomes a test)

The "arm-B load" is #418's load. Main issues one blocking CALL. The host then writes a
10,000-EVENT burst and 1,000 more EVENTs at 100 per second (each a 64-byte frame), and
only then the REPLY. Main is therefore parked for about 10 s. Each criterion names its
negative control: the one mutation that MUST make the test fail.

1. **Link drains during a park.** Given a role parked in `callBlocking` under arm-B
   load against the real `keld_ipc::link::write_frame` writer, when the host writes
   every frame, then no write returns `KELD-IPC-006` and every frame is accepted.
   *Negative control:* moving link draining back onto the main thread makes the
   test fail with `KELD-IPC-006` after the platform send space fills (8,192 bytes on
   macOS). This test lands failing first in #528 with its expected status asserted.
2. **Synchronous result.** Given a parked `callBlocking`, when the host REPLY arrives,
   then the call returns the decoded host bytes synchronously, the returned value is
   not a thenable, and no listener has run yet. *Negative control:* removing the
   Worker's claim and publish (no compare-and-exchange on `BLOCKING` and no
   `REPLY_READY = 1`) makes the call end with `KELD-IPC-006` at its deadline. Removing
   only `Atomics.notify` is not a control for this design: main re-checks
   `REPLY_READY` after every heartbeat-sized slice (§4.5), so the reply still returns,
   up to one `WORKER_HEARTBEAT_INTERVAL_MS` later, and no criterion times that.
3. **Ordered replay.** Given arm-B load, when main wakes, then every EVENT written
   during the park reaches its listener after wake: count equal to the written count,
   sequence strictly increasing by one, zero gaps, zero duplicates. *Negative
   control:* dropping one record in the replay path makes the count and sequence
   assertion fail.
4. **Wake-time rule (§4.6).** Given a state fact for a channel with a registered
   applier, written during the park, when `callBlocking` returns, then a synchronous
   getter backed by that applier already shows the fact. That channel's listener runs
   only after the caller's synchronous continuation has finished, in issue order. A
   main-thread timer armed before the park records zero callbacks between park and
   return. *Negative control:* dispatching listeners inside the wake drain (before
   return) makes the listener-after-resume assertion fail (#419 E3).
5. **Link close: no fabricated reply.** Given a parked call, when the host closes the
   link without writing an `ERR`, then `callBlocking` throws `KELD-IPC-022`, returns
   no value, and records that precede the close are still delivered in order.
   *Negative control:* returning a default value (or a synthetic "closed" REPLY) on
   close makes the test fail.
6. **Generation retire.** Given a parked call on a channel whose reply waiter declares
   `ERR` (lifecycle, or a guarded channel), when the host retires the role generation
   while the link is writable, then the host answers the call with an `ERR` whose
   `CallError.code` is `KELD-IPC-023`, then closes the link, and `callBlocking` throws
   `KELD-IPC-023`. Given a parked call on the echo channel, then the host writes no
   `ERR` for it (KEL-133 keeps echo REPLY-only, §4.9) and the call throws
   `KELD-IPC-022` at the close; a host-written `ERR` on the echo channel closes the
   link with `KELD-IPC-005`, so that call also throws `KELD-IPC-022`. *Negative
   control:* a host that closes without the `ERR` on the lifecycle call makes the test
   observe `KELD-IPC-022`; a Worker that selects `reply_waiter` for an echo call admits
   the echo `ERR` and returns `KELD-IPC-023`, so the echo case fails. A client that maps
   every EOF to `KELD-IPC-023` fails criterion 5.
7. **Quit.** (a) Given a parked blocking `Quit`, when the host accepts it, then the
   call returns the host's real `LifecycleResponse::Quit` bytes and the link closes
   afterwards. (b) Given another blocking call still pending when the host's Quit
   drain ends, then that call throws `KELD-IPC-024`. *Negative control:* a host that
   closes without writing the Quit REPLY makes (a) throw `KELD-IPC-022`, so a
   client-synthesized Quit success fails (a).
8. **Worker death or wedge wakes immediately.** Given a parked call with a 30 s
   deadline, in each of three arms, then `callBlocking` throws `KELD-IPC-025`, not
   `KELD-IPC-006`, and the host observes link loss and takes KEL-75's natural-crash
   path for that generation. (a) *Orderly exit:* the Worker exits through its exit
   handler (a test-only uncaught error). A test counter shows main's liveness branch
   never ran: the exit handler's `STATE` compare-and-exchange woke it. *Control:*
   removing that compare-and-exchange leaves the wake to the liveness branch, so the
   counter is 1 and the arm fails. (b) *Abrupt termination:* before main parks, the
   test posts a test-only message that makes the Worker call `process.exit` N ms
   after it reads `BLOCKING` as 1. `self.close()` is not an arm-(b) trigger: §4.5
   classes it as an orderly exit, whose handler runs. Main parks, then observes
   `KELD-IPC-025`. A test counter MUST show the Worker's exit handler did not run, so
   the wake comes from the liveness branch; a termination path that runs the exit
   handler does not satisfy (b). The message handler exists only in the test build;
   the production Worker has no such message. (c) *Wedge:* a test-only hook stops the
   Worker's event loop. *Control for (b) and (c):* removing the liveness check
   (§4.5) makes the call fail with `KELD-IPC-006` at its deadline, so the code
   assertion fails (#418 risk 1).
9. **No false liveness failure.** Given arm-B load for 5 runs, then no run reports
   `KELD-IPC-025`. *Negative control:* a Worker that stops advancing its heartbeat
   during the load makes the run report `KELD-IPC-025`.
10. **Bounded ring overflow.** Given a ring at its byte bound, or at its record bound,
    with no credit lane, when one more frame arrives, then the Worker stops reading,
    closes the link, and `callBlocking` throws `KELD-IPC-026`. Every record retained
    before the overflow is delivered in order, and the host observes link loss.
    *Negative control:* replacing fail-closed with drop-oldest or drop-newest makes the
    order and count assertion fail.
11. **Second link refused.** Given an authenticated role link, when the role calls
    `WorkerLink.open` a second time in the same realm, then the call throws
    `KELD-IPC-005` before any connect. When any thread or process connects again with
    the same `KELD_APP_LINK`, the OS refuses the connect (the locator was consumed at
    authentication), and the first link keeps working. *Negative control:* keeping the
    locator live after authentication makes the second connect succeed, so the test
    fails.
12. **Correlation match and one in-flight blocking call.** Given a pending blocking
    call with correlation id `c`, a REPLY for a different live id never satisfies it.
    A REPLY for an id that is neither pending nor abandoned closes the link with
    `KELD-IPC-005`. A late REPLY for an abandoned id is discarded and never returned.
    The one reachable path to a second blocking call is a state applier that calls
    `callBlocking` during the step-1 drain (§4.6), since no other code runs on a parked
    main. That inner call throws `KELD-IPC-005` before any write, although the Worker
    has already cleared `BLOCKING` at its claim, and the reply slot, `REPLY_AT` and
    `REPLY_LEN` are unchanged. The applier then throws, so the link closes and the
    outer call throws `KELD-IPC-022` (§4.4).
    *Negative control:* matching the "next REPLY" instead of the correlation id
    makes the abandoned-id case return the wrong bytes. *Second negative control:*
    an in-flight check that reads `BLOCKING` instead of main's flag lets the inner
    call write its CALL during the drain, so the no-write assertion fails.
13. **Deadline on every call.** Given `callBlocking` without a finite positive
    deadline no greater than `MAX_BLOCKING_CALL_DEADLINE_MS`, then it throws
    `KELD-IPC-005` before any frame is written. Given a host that never replies, then
    the call throws `KELD-IPC-006` at its deadline, the link stays up, and a following
    call succeeds. *Negative control:* an infinite default deadline makes the
    invalid-deadline case fail.
14. **No fabricated reply on any wake path.** Given each wake path (close, retire,
    Quit drain, Worker death, Worker wedge, overflow, deadline), then `callBlocking`
    throws the code listed in §4.4 and never returns a value. *Negative control:* any
    one path returning a default value fails the sweep. The sweep includes a close
    caused by a malformed inbound frame (`KELD-IPC-005`), for which the parked call
    throws `KELD-IPC-022` (§4.5), and a race in which main's liveness failure and the
    Worker's close both try to end the link: exactly one code is recorded, and it never
    changes afterwards.
15. **Receiver corpus.** Given the canonical TSV with the §4.9 rows, then Rust and Bun
    both replay it and print one digest. *Negative control:* changing the TypeScript
    `replyWaiter` rule without changing the TSV fails the Bun corpus test.
16. **Registered codes.** Given the six codes in §4.4, then each has a registry
    heading with crate, message and fix lines, and each is emitted from a scanned
    tree. *Negative control:* deleting one heading fails `error_registry`.
17. **Optional credit lane (T4 only).** Given protocol version 3 with credit enabled,
    under arm-B load and a ring smaller than the burst, then the host producer
    suspends at zero credit, no write returns `KELD-IPC-006`, the parked call returns
    its REPLY, and every EVENT arrives in order after wake. A malformed `GRANT` closes
    the link with `KELD-IPC-005`. A version-2 peer and a version-3 peer fail at
    `HELLO` with `KELD-IPC-002`, with no session. *Negative control:* enabling credit
    without the version bump makes the mixed-version test reach a session, so it
    fails.
18. **Reply space cannot be taken by EVENTs.** Given a parked call and a ring whose free
    bytes are smaller than the REPLY envelope (no credit lane), and separately, with
    credit enabled (T4), every credited channel at zero credit and the uncredited share
    full, when the host writes a REPLY whose payload is exactly `replyBytes`, then
    `callBlocking` returns those bytes, no `KELD-IPC-026` occurs, and every EVENT is
    delivered in order after wake. A blocking REPLY of `replyBytes + 1` payload bytes
    throws `KELD-IPC-026`. When a test hook stalls the Worker between its claim and
    `REPLY_READY` past the deadline, and releases it as soon as main's deadline
    compare-and-exchange has failed (signalled through a test word), the call returns
    the reply, never also throws `KELD-IPC-006`, and returns before the criterion-26
    watchdog reaches its limit. *Negative control:* storing the blocking reply as a ring record
    makes the first case throw `KELD-IPC-026`.
19. **No stranded ring record.** Given an unparked role, when the Worker appends a
    second EVENT while the dispatch task for the first is running (a test listener
    waits, through a test-only hook, until `W_RECS` has passed the second EVENT) and
    the host sends nothing more, then both EVENTs reach their listeners in order. A
    test hook runs each time a dispatch task returns without requesting another task,
    and asserts `R_RECS == W_RECS` there. *Negative control:* posting a `kick` only
    when the ring goes from empty to non-empty, with no re-check before idle, makes the
    first task return with the second EVENT appended and no task requested, so the
    hook's assertion fails on that return. No step depends on timing: the listener
    hook orders the append before the task returns. Given a first EVENT whose listener
    throws, then the second EVENT's listener still runs, the idle hook still sees
    `R_RECS == W_RECS`, the link stays up, and the thrown error is reported once, as
    an uncaught error in a later task (§4.6). *Second negative control:* letting the
    throw leave the batch before the cursors advance and `KICK` is reset leaves
    `KICK = 1` with the second EVENT undelivered, so the assertions fail.
20. **Outbound path.** Given a test hook that holds the Worker's message handling until
    `BLOCKING` is nonzero, when main sends EVENT `e1`, then an asynchronous CALL `c1`,
    then a blocking CALL `c2` (so all three are queued before the Worker handles any),
    then the host reads `e1`, `c1`, `c2` in that order, and `W_BYTES` and `W_RECS`
    advance for no outbound frame. *Negative control:* a Worker that writes a queued
    blocking CALL ahead of the frames queued before it makes the host read `c2` first,
    every run, so the order assertion fails.
21. **Per-frame inbound policy.** Given a role whose `receive` table declares EVENT
    channel `e` and the `echoReceiver` CALL receiver, when the host writes a REPLY whose
    correlation id is not in the Worker's pending-CALL map, then the link closes with
    `KELD-IPC-005`, `W_RECS` does not advance for that frame, and no listener, waiter
    or applier sees it. The same holds for an EVENT on an undeclared channel and an
    EVENT with a nonzero correlation id on `e`. A REPLY for a pending id, an EVENT on
    `e` and a host-originated echo CALL (KEL-142) are each admitted. A `PING` on any
    channel is echoed with its channel and correlation id and does not advance
    `W_RECS`. A `receive` table that repeats a channel, including one named only as an
    `alsoChannel`, makes `open` throw `KELD-IPC-005`. A REPLY whose id equals
    `BLOCKING` but whose channel differs from the pending CALL's closes the link with
    `KELD-IPC-005`, `REPLY_READY` stays 0 and the call throws `KELD-IPC-022`: the Worker
    validates against the pending map before it claims. *Negative control:*
    admitting every REPLY or `ERR` with a nonzero correlation id, without the map
    lookup, appends the unsolicited REPLY to the ring, so the `W_RECS` assertion fails.
22. **Bounded abandoned set.** Given a host that never replies, when
    `MAX_ABANDONED_CALLS` calls expire, then each throws `KELD-IPC-006` and the link
    stays up. When one more call expires, that call still throws `KELD-IPC-006` (main
    decides its deadline before the Worker sees the `abandon`), then the link closes,
    every other pending call and every later call throw `KELD-IPC-027`, and the host
    observes link loss. A late REPLY for any of
    the retained abandoned ids is discarded without closing the link. *Negative
    control:* an unbounded abandoned set leaves the link up after the extra expiry, so
    the following call does not throw `KELD-IPC-027` and the test fails.
23. **GRANT window (T4).** Given credit enabled, when the Worker's first `GRANT` on a
    credited channel declares `(frames, bytes)`, and a later `GRANT` would raise the
    host's outstanding credit on that channel above either value, then the host closes
    the link with `KELD-IPC-005` and writes no further EVENT. A `GRANT` that keeps
    outstanding credit within the window is admitted. *Negative control:* a host that
    checks each `GRANT` only against the total granted so far, with no declared window,
    admits the over-grant, so the test fails.
24. **No GRANT before `HELLO` (T4).** Given a Worker test hook that writes a `GRANT`
    before its `HELLO`, then the host rejects it under `server-pre-auth-hello` with
    `KELD-IPC-005` and no session exists. Given the real Worker, then its first `GRANT`
    is written only after it has validated the host's `HELLO` reply. *Negative
    control:* adding `GRANT` to the host's pre-authentication policy lets the hooked
    Worker reach a session, so the test fails.
25. **Power-of-two ring.** Given `ringBytes` of 3 MiB, then `WorkerLink.open` throws
    `KELD-IPC-005` before the Worker spawns; given 4 MiB, it opens. Given a test hook
    that starts every byte counter at `2^32 - 64`, then records that straddle the
    counter wrap are delivered intact and in order. In the same run a blocking reply
    arrives after the wrap, so its `REPLY_AT` is past `2^32` while facts before it are
    not: every fact before the reply is applied before return, and none after it. *Negative control:* removing the
    power-of-two check makes the 3 MiB `open` succeed, so the test fails. With that
    check removed, the wrap case also reads a record at the wrong position, because
    `2^32` is not a multiple of 3 MiB.
26. **No unbounded parked wait after a claim.** Given a parked call, when a test hook
    makes the Worker claim the reply and then skip the publish while its heartbeat
    timer keeps running, and separately throw inside the claim step, then
    `callBlocking` throws `KELD-IPC-025`, `STATE` records 25 and the link closes. A
    watchdog test thread counts `HEARTBEAT` advances after main's deadline
    compare-and-exchange fails, and the call MUST throw before that count reaches
    `2 * WORKER_LIVENESS_WINDOW_MS / WORKER_HEARTBEAT_INTERVAL_MS`. *Negative
    control:* removing the post-claim bound (§4.5) leaves main parked while the
    heartbeat advances, so the watchdog reaches its limit and the test fails. In the
    throw arm, with a 30 s deadline, the call throws `KELD-IPC-025` within
    `WORKER_LIVENESS_WINDOW_MS` of the throw, and a test counter shows main's
    post-claim branch never ran. *Second negative control:* removing the Worker's own
    25 record on a throw (§4.5 step 2) leaves the wake to the post-claim bound after
    the deadline, so the timing and counter assertions fail.
27. **Asynchronous replies and host CALLs are dispatched on main (§4.6).** Given two
    pending `call()`s, `c1` on the lifecycle channel (its reply waiter admits `ERR`,
    §4.7; an `ERR` on the echo channel closes the link, criterion 6) and `c2` on the
    echo channel, and a host that writes an EVENT `e`, then a REPLY for `c2`, then an
    `ERR` for `c1`, then `e`'s listener runs before either
    Promise settles, `c2` resolves with its REPLY bytes, and `c1` rejects with the
    `ERR`'s `CallError.code`. A late REPLY for a `call()` that already rejected with
    `KELD-IPC-006` leaves that outcome unchanged and the link up. Given a call handler
    set for the echo channel and a host echo CALL written while main is parked, then
    the handler has run 0 times when `callBlocking` returns and once afterwards, and the
    host reads exactly one frame for that CALL, carrying the CALL's channel and
    correlation id and the handler's kind and payload. Given an `echoReceiver` with no
    call handler set, the same host CALL closes the link with `KELD-IPC-005`, and every
    pending call throws or rejects with `KELD-IPC-022`. *Negative control:* a dispatcher
    that settles the oldest pending Promise instead of the one `header.corr` names
    resolves `c1` with `c2`'s bytes, so the test fails. *Second negative control:*
    starting call handlers in the step-1 wake drain (§4.6) makes the handler count 1
    when `callBlocking` returns, so the parked case fails. *Third negative control:*
    answering a CALL that has no handler with an `ERR`, or discarding it, leaves the
    link up, so the `KELD-IPC-005` assertion fails.
28. **Uncredited share overflow (T4).** Given credit enabled, every credited channel
    holding unused credit, a parked call, and the uncredited share (`ringBytes / 4`
    bytes or `ringRecords / 4` records, §4.8) full of retained frames, when the host
    writes one more uncredited frame (a lifecycle EVENT or an asynchronous REPLY), then
    the Worker stops reading, records 26, and closes the link; the parked call throws
    `KELD-IPC-026`, every retained record is delivered in order, and the host observes
    link loss. *Negative control:* checking an uncredited frame against the whole
    ring's free space instead of its share admits the frame into credited space, so
    the test fails.
29. **Credit shares are never empty (T4).** Given credit enabled with 2 credited
    channels, `ringRecords = 3` makes `WorkerLink.open` throw `KELD-IPC-005` before
    the Worker spawns (the uncredited share would be 0 records), and `ringRecords = 8`
    opens with shares of 2, 3 and 3 records. *Negative control:* removing the
    empty-share check makes the `ringRecords = 3` open succeed, so the test fails.

## 4. Design

### 4.1 First-principles and reuse decision

Atomic decomposition (root `AGENTS.md`). Each atom has its own evidence. FACT marks
something a probe or the repository shows. The PANEL-P1 probe is a scratch prototype
that links the real `keld-ipc` writer and the real `@keld/kipc` framing, on macOS
arm64, Bun 1.4.2. Its results are FACT for that prototype and platform only. INFERENCE
marks a design consequence that this spec's tests must confirm.

| # | Atom | Owner and boundary | Failure mode | Observable contract | Evidence |
|---|---|---|---|---|---|
| A1 | Link handle ownership | Transport Worker in the role process; owns the socket from connect to close | main thread blocks the link reader | criterion 1 | FACT: arm B 5/5, 704,043 B written, no `KELD-IPC-006`; arm A 5/5 stalled at 8,192 B |
| A2 | Identity and authentication | Host mints the endpoint and token (KEL-75); Worker sends `HELLO` | second link or second principal | criterion 11 | FACT: arm B second connect refused `ENOENT` 5/5; arch 02 §2 consumed locator |
| A3 | Authorization | Unchanged: host binds the link to the role principal and the guard decides per CALL | none added | no new capability | FACT: no grant or guard change in this spec |
| A4 | OS containment | Worker runs inside the role's process and strict profile; no new mount | a third staged file would widen the Linux strict mount | T3 Linux test: Worker entry is the staged transport file | FACT: arch 06 binds exactly two files. INFERENCE: a self-entry Worker needs no new mount |
| A5 | Blocking wait and wake | main parks on one `Int32Array` word; Worker claims, publishes and notifies | lost wake | criterion 2 | FACT: arm B main woke once per call. The prototype's NC2 (no notify, `KELD-IPC-006` at deadline 2/2) does not carry over: its main waited out the whole remaining deadline in one `Atomics.wait` (`client/arm-b-main.ts:144` at `46e59078`), while this design waits in heartbeat slices. INFERENCE until criterion 2's claim-and-publish control passes |
| A6 | Ordered retention | one ring, written only by the Worker, freed only by main | drop, duplicate, reorder, or unbounded growth | criteria 3 and 10 | FACT: 11,000/11,000 in order 5/5; NC3 gap detected 2/2; 64 KiB ring fails closed 3/3 |
| A7 | Wake-time ordering | main-thread drain on wake | re-entrant user code while parked or before return | criterion 4 | FACT (#419 E3): Electron 44.4.5 runs no main-process JS during `showMessageBoxSync` and delivers queued work in order afterwards. INFERENCE: the two-cursor ring reproduces this |
| A8 | Lifecycle and revocation wake | host writes `ERR`, then closes; Worker publishes the close | hang or fabricated value | criteria 5 to 7 | FACT: E1 retire 5/5 typed wake; E2 real Quit reply 5/5 |
| A9 | Worker liveness | heartbeat word written by the Worker, checked by parked main | wake only at the call deadline | criteria 8 and 9 | FACT: worker death woke main only at its 3 s deadline, 2,503–2,504 ms after death, 2/2. The heartbeat is INFERENCE until criterion 8 passes |
| A10 | Deadline | caller-supplied, enforced by main | unbounded park | criterion 13 | FACT: NC2 deadline expiry 2/2 |
| A11 | Optional credit | `GRANT` from the Worker to the host producer | writer stall, or an unbounded host producer queue | criterion 17 | FACT: arm B+C 3/3 passed, with 9,976 EVENTs deferred at the host producer. The payload is INFERENCE (#418 risk 3) |
| A12 | Evidence provenance | PANEL-P1 artifact `s0-link-drain.json` at `46e59078` | prototype mistaken for product proof | #528 tests replace it | FACT: scratch prototype; per-frame allocation; counters valid only below 2^31; macOS only |

Hidden coupling promoted to explicit edges:

- A6 to A7: the ring is also the listener FIFO, so ring space is freed only when
  listeners are dispatched (§4.6);
- A8 to A6: an `ERR` written before close must reach main before the close. An
  asynchronous `ERR` and the close are in arrival order in the ring. A blocking `ERR`
  fills the reply slot before the Worker publishes the close, and main checks the slot
  before `STATE` (§4.5), so the earlier frame wins;
- A9 to A8: Worker death is also link loss at the host, so KEL-75's crash path runs.
  The heartbeat only decides how the parked caller learns of it.

Reuse:

- kept: `FrameReader`, `WriteQueue`, `DrainSignal`, `connectKipcSocket`,
  `validateReceivedHeader`, `decodeCallError` and `errorFromErrFrame` from
  `transport.ts`. The KEL-133 validator, `write_call_error`, `CallError`, the consumed
  bootstrap locator, `FrameKind::Grant = 9` and KEL-75's crash and revoke paths;
- reused codes: `KELD-IPC-005` (session-contract violations) and `KELD-IPC-006` (an
  expired deadline, as in KEL-133 criterion 8);
- not reused: the prototype's `KELD-IPC-001` and `KELD-IPC-004` wake codes.
  `KELD-IPC-001` does not tell the caller that no reply was received and the call's
  host effect is unknown. `KELD-IPC-004` means a frame above `MAX_FRAME_LEN`. Each
  needs different fix guidance, so §4.4 registers new codes;
- `DirectedReader` parking (`MAX_PARKED_FRAMES = 8`) is superseded by the ring for
  Worker-owned links and removed when its last consumer migrates (T3). Two inbound
  ordering owners would be a DRY defect.

Named unmet requirement that justifies the change: the main-thread transport cannot
deliver a REPLY to a parked main thread (arm A and arm C, §4.11). Compatibility
fallback: the main-thread client path remains until T3 migrates the scaffold and
`@keld/electron`, then it is removed. No permanent fallback is required. Performance:
no claim is made (§9).

### 4.2 Ownership, process and crash facts

- One role generation has one link and one principal. The host mints both (KEL-75).
  The Worker is a thread inside the role process, not a principal. The host binds the
  accepted link to the role principal exactly as today.
- `WorkerLink.open` spawns the transport Worker before any connect. The Worker parses
  `KELD_APP_LINK`, connects, sends `HELLO` and owns the socket until close. The main
  thread never calls `connectKipcSocket` on a Worker-owned link.
- Every main-thread frame passes through the Worker: blocking calls, asynchronous
  calls, outbound EVENTs and every inbound frame. There is no side channel. The two
  directions use different paths. Outbound, main posts each frame to the Worker with
  `postMessage`, in send order, and the Worker writes it with its one `WriteQueue`.
  The ring does not bound or order outbound frames: they keep KEL-136's per-frame
  `MAX_FRAME_LEN` bound and write order. Inbound, the Worker appends each frame to the
  ring, except the blocking reply, which goes to the reply slot (§4.3).
- The Worker entry is the staged transport file itself. No third file is staged or
  mounted. The entry branch is selected only by the marker that `WorkerLink.open`
  passes in `workerData`. "Not the main thread" is not enough: an application Worker
  that imports the transport MUST NOT start a link.
- Crash domains. Worker death leaves the role process alive but without a link. The
  transport converts it into terminal link loss: every later call throws
  `KELD-IPC-025` (or the code recorded first, §4.5), and the socket closes when the Worker dies. The host owns the crash
  decision through KEL-75's natural-crash path. No new crash owner is added.
- The Worker runs no application code. It imports only the transport module, and no
  user listener or applier runs on it.

### 4.3 Shared memory layout

One `SharedArrayBuffer` is allocated at `WorkerLink.open`. It is never resized and
there is no allocation per frame in the ring path. It holds a 64-byte control block of
`Int32` words, then the blocking-reply slot (`replyBytes`), then the ring.

```text
word  name        writer   meaning
0     SEQ         Worker   bumped + Atomics.notify on: reply slot filled, close, overflow, Worker exit
1     STATE       both     0 OPEN, else the KELD-IPC number that ended the link: 22, 25, 26
                           or 27. Set only by compareExchange(STATE, 0, code); the first wins
2     reserved    -        zero (the closing code lives in STATE, so it is set atomically)
3     W_BYTES     Worker   monotonic byte write counter (u32, modular)
4     A_BYTES     main     apply cursor: records before it have had their state fact applied
5     R_BYTES     main     dispatch cursor: records before it are dispatched; frees ring space
6     W_RECS      Worker   monotonic record counter (u32, modular)
7     R_RECS      main     records released by the dispatch cursor
8     HEARTBEAT   Worker   bumped every WORKER_HEARTBEAT_INTERVAL_MS and after every frame
9     BLOCKING    both     correlation id of the waiting blocking call, 0 when none; the
                           Worker claims a reply by CAS id->0, main abandons by CAS id->0
10    REPLY_READY both     Worker stores 1 after filling the reply slot; main stores 0 after copying
11    REPLY_KIND  Worker   2 REPLY or 3 ERR
12    REPLY_LEN   Worker   payload bytes in the reply slot, at most replyBytes
13    REPLY_AT    Worker   W_BYTES when the reply was claimed: the reply's place in issue order
14    KICK        both     1 while a dispatch task is posted or running, else 0 (§4.6)
15    reserved    -        zero
```

A ring record is the received 16-byte frame header followed by its payload, written
contiguously with wrap-around. Counters use modular `u32` arithmetic. `ringBytes` MUST
be a power of two, so a byte's ring position is `counter & (ringBytes - 1)` and stays
continuous when a counter wraps at 2^32. The ring byte capacity is at most 2^30, so
`(W - R) >>> 0` is never ambiguous. This removes the
prototype's 2^31 per-session limit. The Worker stores the bytes, then publishes
`W_BYTES` and `W_RECS` with `Atomics.store`. Main reads them with `Atomics.load`.

Bounds, fixed at open:

```ts
export const DEFAULT_RING_BYTES = 1 << 20;        // 1 MiB
export const DEFAULT_RING_RECORDS = 16_384;
export const DEFAULT_REPLY_BYTES = 1 << 16;       // 64 KiB blocking-reply slot
export const MAX_RING_BYTES = 1 << 30;
export const MAX_BLOCKING_CALL_DEADLINE_MS = 24 * 60 * 60 * 1000;
export const WORKER_HEARTBEAT_INTERVAL_MS = 100;
export const WORKER_LIVENESS_WINDOW_MS = 1_000;
export const MAX_ABANDONED_CALLS = 256;
```

The blocking call's REPLY or `ERR` never enters the ring. The Worker copies its
payload into the reply slot, which only that one call can use, so EVENTs, credit and
asynchronous replies can never take its space. A blocking reply whose payload is at
most `replyBytes` is therefore always delivered; a larger one is an overflow (§4.7).
`replyBytes` is an integer from 4,096 to `MAX_FRAME_LEN`. `ringBytes` is a power of
two from 65,536 to `MAX_RING_BYTES`, and `ringRecords` a positive integer. Any other
value makes `WorkerLink.open` throw `KELD-IPC-005` before the Worker spawns
(criterion 25). A frame whose envelope
is larger than the ring's byte capacity can never be retained, so it is an overflow
(§4.7). A role that needs larger inline frames opens with a larger
`ringBytes`, up to `MAX_RING_BYTES`. Bulk payloads stay on the bulk plane (arch 02 §3).
INFERENCE: the defaults hold #418's 704,000-byte, 11,000-record park with headroom.
The real event rate during a modal is UNKNOWN (#418 risk 2), so the defaults are open
question 1.

### 4.4 Typed errors

Reserved numbers. `KELD-IPC-018` to `KELD-IPC-021` are taken by #607 (merged; their
registry headings are on `main`), so this spec reserves `KELD-IPC-022` to
`KELD-IPC-027`. The registry rule (`docs/engineering/keld-error-codes.md`) fails a
heading that no scanned tree emits. The headings therefore land in #528 together with the code that emits them, and T1
adds `packages/@keld/kipc/src` to `SCAN_REL` in `crates/keld-cli/tests/error_registry.rs`.
That test today sees TypeScript codes only when Rust happens to emit the same string.
If #607 or another change takes these numbers first, #528 takes the next free numbers
and updates this table in the same PR.

| Code | Emitter | When | Message | Fix |
|---|---|---|---|---|
| `KELD-IPC-022` | `@keld/kipc` | the link reached EOF or an I/O error, or was closed for a `KELD-IPC-005` session-contract violation, while a call was pending, and no host `ERR` arrived for it | link closed before the host replied; no reply was received and the call's host effect is unknown | Treat the call as not answered. The role's link is gone: check the host log for the close cause, and do not retry on this link, because it cannot reconnect. |
| `KELD-IPC-023` | `keld-ipc` (host writes it with `write_call_error`) | the host retired this role generation before the call's handler finished | role generation retired before this call completed | The role instance is being replaced or stopped. Do not retry here; the successor generation reissues the work after its own `Ready`. |
| `KELD-IPC-024` | `keld-ipc` (host writes it with `write_call_error`) | the host accepted `Quit` and its drain ended with this call still pending | session ended by an accepted Quit before this call completed | The application is quitting. Do not issue new work; finish only the shutdown path. |
| `KELD-IPC-025` | `@keld/kipc` | the transport Worker exited, or its heartbeat stopped for `WORKER_LIVENESS_WINDOW_MS`, while the role is open | transport Worker dead or unresponsive; the role's link is lost | The role has no link and cannot reconnect. Report the crash. The host restarts the role per its policy; check the role log for the Worker's last error. |
| `KELD-IPC-026` | `@keld/kipc` | a frame did not fit the ring's byte or record bound (with the credit lane, its share of them, §4.7), or a blocking REPLY or `ERR` payload was larger than `replyBytes` | parked ring or reply slot full; the link was closed rather than drop a frame | Raise `ringBytes`, `ringRecords` or `replyBytes` at `WorkerLink.open`, reduce the host event rate toward this role, or enable the credit lane (T4). Retained events were delivered in order. |
| `KELD-IPC-027` | `@keld/kipc` | a call was pending, or was issued, after the Worker processed an `abandon` that exceeded `MAX_ABANDONED_CALLS` (the expiring call itself throws `KELD-IPC-006`) | too many unanswered calls; the link was closed rather than track another abandoned id | The host is not answering this role's calls. Check the host log for the stalled handler; raise call deadlines only if the host is slow rather than stuck. The role's link is gone and cannot reconnect. |

Reused codes: `KELD-IPC-005` for a second `WorkerLink.open`, invalid open bounds or
an invalid `receive` table (§4.5), a second in-flight
blocking call, an invalid deadline, an unsolicited correlation id, a malformed `GRANT`,
an applier that throws, or a host CALL whose call handler is missing or fails (§4.6).
A `KELD-IPC-005` raised by an API call before any write is thrown to that caller. A
`KELD-IPC-005` that closes the link (an inbound frame the Worker rejects, a reused
correlation id, a throwing applier, a failed call handler) records `STATE = 22`, so
the parked call and every pending call throw `KELD-IPC-022`, whose detail names the
`KELD-IPC-005` cause; the cause is also written to the role log. `KELD-IPC-006` for an
expired call deadline. Only the
deadline leaves the link up, until the abandoned set is full (`KELD-IPC-027`, §4.5). A
late REPLY for a retained abandoned id is discarded.

### 4.5 Blocking call and Worker liveness

```ts
export interface WorkerLinkOptions {
  link: string;                     // KELD_APP_LINK text; parsed only in the Worker
  receive: WorkerReceiveTable;      // role-supplied inbound table (§4.7)
  ringBytes?: number;               // default DEFAULT_RING_BYTES
  ringRecords?: number;             // default DEFAULT_RING_RECORDS
  replyBytes?: number;              // default DEFAULT_REPLY_BYTES
}

/** Role-supplied inbound frames, beyond replies to its own calls; narrows, never grants. */
export interface WorkerReceiveTable {
  /** Channels on which the host may send EVENT frames (correlation id 0). */
  readonly eventChannels: readonly number[];
  /** Host-originated CALL receivers, named by KEL-133 constructor, at most one per channel. */
  readonly callReceivers: readonly WorkerCallReceiver[];
}

/** Names a KEL-133 constructor; the Worker builds the policy, never the caller. */
export type WorkerCallReceiver =
  | { readonly policy: "echoReceiver" }                        // KEL-142 host Echo CALL
  | { readonly policy: "privilegedCallReceiver"; readonly channel: number };

/** How main answers one host CALL: REPLY bytes, or an ERR's encoded CallError. */
export interface WorkerCallReply {
  readonly kind: typeof FrameKind.Reply | typeof FrameKind.Err;
  readonly payload: Uint8Array;
}

export class WorkerLink {
  /** Spawns the transport Worker, which connects and completes HELLO. At most once per realm. */
  static open(options: WorkerLinkOptions): Promise<WorkerLink>;
  /** Parks the calling thread; returns the host REPLY bytes or throws a KeldCallError. */
  callBlocking(channel: number, payload: Uint8Array, deadlineMs: number): Uint8Array;
  /** Same correlation and ring path, Promise-shaped, for non-blocking callers. */
  call(channel: number, payload: Uint8Array, deadlineMs: number): Promise<Uint8Array>;
  sendEvent(channel: number, payload: Uint8Array): void;
  /** Framework-only synchronous state applier; at most one per channel; never user code. */
  setStateApplier(channel: number, applier: (payload: Uint8Array) => void): void;
  /** Framework-only answer to host CALLs on a `receive.callReceivers` channel; at most one per channel. */
  setCallHandler(channel: number, handler: (payload: Uint8Array) => Promise<WorkerCallReply>): void;
  onEvent(channel: number, listener: (payload: Uint8Array) => void): () => void;
  close(): void;
}
```

`callBlocking`:

1. It validates the deadline: finite, greater than 0 and at most
   `MAX_BLOCKING_CALL_DEADLINE_MS`. Otherwise it throws `KELD-IPC-005`.
2. No blocking call may be in flight; otherwise it throws `KELD-IPC-005`. "In
   flight" is a main-only flag, never `BLOCKING`, which the Worker clears at its
   claim. Main sets the flag here and clears it only after step 4, in every outcome including a throw from the drain or the copy (a throw closes the link, so later calls throw the recorded code, never 005), so it covers the
   step-1 drain (§4.6) and the copy out of the reply slot; the slot stays owned by
   the outer call until then. Main
   allocates the correlation id (the rule is under "Pending-CALL map" below). Main
   stores the id in `BLOCKING` and posts the CALL to the Worker
   with `postMessage`, marked blocking. FACT: `postMessage` from a parked main reaches
   the Worker (#418).
   The Worker first selects and validates every REPLY or `ERR` against the
   pending-CALL map (§4.7), so a frame with an unknown id or the wrong channel closes
   the link and is never claimed. Only an admitted REPLY or `ERR` whose id equals
   `BLOCKING` is claimed, with
   `Atomics.compareExchange(ctrl, BLOCKING, id, 0)`. On success it stores `REPLY_AT =
   W_BYTES`, copies the payload into the reply slot, sets `REPLY_KIND`, `REPLY_LEN` and
   `REPLY_READY = 1`, then bumps `SEQ` and notifies. Claim, copy and publish run in one
   synchronous Worker step, and the Worker bumps `HEARTBEAT` only after it, so a
   claimed reply is either published or caught by the liveness check within
   `WORKER_LIVENESS_WINDOW_MS`. Any fault in that step (a thrown error, a skipped
   publish) makes the Worker record 25 in `STATE`. If the claim fails, the call was
   abandoned, and the frame is discarded like any late reply. The slot is
   empty whenever a claim can succeed, because main empties it before the call returns
   and only one blocking call is in flight.
3. Main loops on `Atomics.wait(ctrl, SEQ, seen, slice)`, where `slice` is the smaller
   of the remaining deadline and `WORKER_HEARTBEAT_INTERVAL_MS`. After every return it
   checks, in this order:
   - `REPLY_READY = 1`: apply facts up to `REPLY_AT` (§4.6), copy `REPLY_LEN` bytes out
     of the slot, store `REPLY_READY = 0`, then return the REPLY bytes, or throw
     `errorFromErrFrame` for an `ERR`;
   - `STATE` other than 0: throw the code it records (22, 25, 26 or 27);
   - `HEARTBEAT` unchanged for `WORKER_LIVENESS_WINDOW_MS`:
     `Atomics.compareExchange(ctrl, STATE, 0, 25)`, call `worker.terminate()` once main
     resumes, and throw the code `STATE` then holds (25, or the code the Worker
     recorded first);
   - the deadline passed: `Atomics.compareExchange(ctrl, BLOCKING, id, 0)`. On success,
     post `abandon(id)` to the Worker and throw `KELD-IPC-006`. On failure the Worker
     has already claimed the reply, so keep waiting for `REPLY_READY` under the same
     liveness check, for at most `WORKER_LIVENESS_WINDOW_MS` more. If it is still 0
     then, record 25 with `Atomics.compareExchange(ctrl, STATE, 0, 25)`, terminate the
     Worker once main resumes, and throw the code `STATE` holds. This post-claim bound
     does not depend on the heartbeat, so a parked wait never exceeds the deadline plus
     one liveness window (criterion 26). The one compare-and-exchange on `BLOCKING`
     decides between reply and deadline, so a call never returns after it has thrown.
4. In every outcome, `BLOCKING` is 0 and the reply slot is empty before the call
   returns or throws.

Link termination. Every side that ends the link does so with one
`Atomics.compareExchange(ctrl, STATE, 0, code)`, then bumps `SEQ` and notifies. Only
the first succeeds, so the recorded code never changes and the Worker and main cannot
disagree about it. The Worker records 22 (EOF, I/O error or a link-closing
`KELD-IPC-005`), 25 (its own orderly exit, or a fault after a claim), 26 (overflow) or 27 (abandoned cap); main
records 25 (liveness failure) or 22 (a throwing applier or a failed call handler).
23 and 24 are never recorded: they arrive as host `ERR` payloads.

Pending-CALL map. The Worker is its one owner, because it writes every CALL. It adds
an entry `(corr, channel, blocking)` when it writes the CALL, and removes it when the
REPLY or `ERR` is claimed, appended or discarded. On `abandon(id)` (a blocking or
asynchronous deadline), the entry stays, marked abandoned, until its late reply
arrives and is discarded; an `abandon(id)` for an id no longer in the map is ignored.
At most `MAX_ABANDONED_CALLS` entries may be abandoned. The
abandon that would exceed that cap makes the link terminal: the Worker records 27,
ends the socket, and every other pending call and every later call throws
`KELD-IPC-027`. The call whose `abandon` crossed the cap has already thrown
`KELD-IPC-006`: the Worker is the one owner of the abandoned count, so main never
counts abandons itself. Correlation ids, the
one rule: main allocates every id, blocking or asynchronous, from one `u32` counter
that skips 0 and the ids of main's own unresolved calls. Abandoned ids are tracked only
in this map, not by main. A CALL whose id is still in the map (possible only after the `u32` counter
wraps) closes the link with `KELD-IPC-005` before it is written. An asynchronous
reply that the Worker appended before it processed `abandon(id)` is discarded by main,
which no longer waits on that id.

Worker liveness. This is the mechanism #418 risk 1 requires:

- *Orderly exit* (an uncaught error, `self.close()`, or the transport closing): the
  Worker's exit handler records 25 (or 22 when the transport closed first) through
  the `STATE` compare-and-exchange. The parked caller wakes at once.
- *Abrupt death or a wedge*: the Worker bumps `HEARTBEAT` on a
  `WORKER_HEARTBEAT_INTERVAL_MS` timer and after each frame it handles. A parked main
  wakes at least once per heartbeat interval and throws `KELD-IPC-025` once the
  heartbeat has not moved for `WORKER_LIVENESS_WINDOW_MS`. The wake is bounded by the
  liveness window and does not depend on the call deadline.
- *Not parked*: main also listens for the Worker's `error` and `close` events. On
  either, main runs `Atomics.compareExchange(ctrl, STATE, 0, 25)`, which keeps any
  code recorded first, and the link is terminal. Pending asynchronous calls then
  settle by the §4.6 rule: retained records are delivered first, and each unresolved
  `call()` rejects with the code `STATE` holds.
- *Host side*: Worker death closes the socket. FACT: the host saw `KELD-IPC-001`
  broken pipe at once in both runs. The host handles it as role link loss (arch 02
  §7; KEL-75 natural crash). UNKNOWN: whether `terminate()` closes the socket of a
  Worker wedged in a synchronous loop (open question 2).

Rejected liveness options: waiting for the Worker `close` event (FACT: it cannot run
on a parked main; the probe woke only at its deadline), and host-only detection (the
host has no path to a parked main except the dead link).

### 4.6 Wake-time rule (mirror facts before return, listeners after resume)

No application JavaScript runs on the main thread while it is parked. Main is inside
`Atomics.wait`, and the transport Worker runs only transport code. Other application
Workers are separate threads and keep running; this rule makes no claim about them. This reproduces #419 E3: Electron 44.4.5 runs
no main-process JavaScript during `showMessageBoxSync`, then delivers queued work in
order and loses none. Issue order is the order in which the host wrote frames to the
link; the ring keeps arrival order, which is the same order on a stream.

The ring has two main-side cursors over one record sequence:

1. On wake, before `callBlocking` returns, main walks from `A_BYTES` up to
   `REPLY_AT`, the reply's place in issue order (§4.5). For each EVENT record it runs
   the channel's registered state applier, if one exists, then advances `A_BYTES`. It
   runs no listener and calls no user code. It copies the reply payload out of the
   slot. It does not apply facts that arrived after the reply: the caller sees host
   state as of the reply.
2. It requests one dispatch task (step 3). The task is an ordinary event-loop task,
   never a microtask, so it runs only after the caller's synchronous continuation. It
   walks from `R_BYTES` up to the `W_BYTES` it loaded when it started. For a record
   at or after `A_BYTES` it first runs the applier and advances `A_BYTES` past it. It
   then dispatches the record's listeners (a throwing listener is handled as below)
   and advances `R_BYTES` and `R_RECS`, which
   frees ring space. Invariant: `A_BYTES` is never behind `R_BYTES`, that is
   `((A_BYTES - R_BYTES) >>> 0) <= ((W_BYTES - R_BYTES) >>> 0)`. So the step-1 drain,
   which starts at `A_BYTES`, never re-applies a fact and never reads freed bytes.
3. One dispatcher, tracked by `KICK`. To request a task, a side sets `KICK` from 0 to
   1 with `Atomics.compareExchange`, and only the side that succeeds schedules it: the
   Worker posts one `kick` message after it publishes `W_BYTES` and `W_RECS`, and
   main schedules a `setImmediate` task. When a task has walked its batch, it stores
   `KICK = 0` and then loads `W_BYTES` again. If records remain, it requests another
   task the same way. All of these are sequentially consistent `Atomics` operations,
   so either the re-check sees a record that the Worker appended during the task, or
   the Worker's compare-and-exchange sees `KICK = 0` and posts a kick. A record is
   never left in the ring with no task requested, and each task yields to the event
   loop between batches. Parked and unparked delivery use the one dispatch cursor, so
   listener order is issue order whatever mix of the two occurs.

Dispatch by record kind. In step 2, after the applier step, the dispatch task handles
each record by its kind (criterion 27):

- EVENT: the channel's `onEvent` listeners run.
- Asynchronous REPLY or `ERR`: main looks up `header.corr` among its own unresolved
  `call()`s, the set the correlation counter skips (§4.5). A match is removed, and its
  Promise resolves with the REPLY payload or rejects with `errorFromErrFrame` for an
  `ERR`. With no match, main already rejected that call at its deadline, and the
  record is discarded. The correlation id selects the Promise, never arrival order.
  A `call()` deadline is a main-thread timer: on expiry main removes the entry,
  rejects with `KELD-IPC-006` and posts `abandon(id)`, as `callBlocking` does.
- Host CALL: main runs the handler that `setCallHandler` set for `header.channel` with
  the payload. When its Promise fulfils, main posts the returned kind and payload to
  the Worker. The Worker writes them with the CALL's channel and correlation id through
  its `WriteQueue`, like any outbound frame (§4.2). The record is released when the
  handler starts, so a slow handler holds no ring space. A missing handler, a rejected
  Promise, a kind other than REPLY or `ERR`, or a payload above `MAX_FRAME_LEN` makes
  the link terminal with `KELD-IPC-005`, recorded as 22 (§4.4). `setCallHandler`
  throws `KELD-IPC-005` for a channel that `receive.callReceivers` does not name, or a
  second handler on one channel. The answer rule stays with its owner: `@keld/api`
  adapts its existing module-private `resolveEchoCall` rule
  (`packages/@keld/api/src/echo-call.ts`), which already turns a missing or failing
  application handler into an `ERR` with `KELD-API-001`. In T3 it takes the payload
  instead of a `DecodedFrame`, drops its `validateReceivedHeader` call because the
  Worker has already validated the CALL (§4.7), and returns `WorkerCallReply`.
  `WorkerCallReply` replaces `EchoCallResult`, so the reply shape has one owner. The
  transport adds no error code.

No call handler starts and no `call()` Promise settles while main is parked, because
main does both only in the dispatch task. Once `STATE` is not 0 and the task has
delivered every retained record, each unresolved `call()` rejects with the code
`STATE` records.

A throwing `onEvent` listener is application code, not a transport fault, so it does
not end the link. The dispatcher catches the throw, finishes that record's other
listeners, advances `R_BYTES` and `R_RECS` past the record, and continues the batch.
After the batch's `KICK = 0` store and re-check (step 3), it rethrows the first caught
error in a new `setImmediate` task, so the error reaches the role's uncaught-error
handling and no record or `KICK` state is left behind (criterion 19).

The ring is therefore also the listener FIFO. No second queue exists, and memory held
for undispatched listeners stays inside the ring bound. An applier MUST be synchronous
and MUST NOT call user code. A throwing applier makes the link terminal with
`KELD-IPC-005`: a mirror that cannot apply a fact must not serve stale state. #449
(F02-T2) owns the mirror; its zero-round-trip getters rely on step 1, and its sequence
test lands with #528.

### 4.7 Bounded ordered ring and overflow

- The Worker validates each inbound frame before it appends it. One `ReceivePolicy`
  cannot cover a role's inbound mix, because it carries one correlation rule and at
  most two channels (`crates/keld-ipc/src/receive.rs`). So the Worker selects one
  existing-shape policy per frame, from the frame's kind and channel and the
  pending-CALL map (§4.5), then runs the unchanged KEL-133 validator under it. This is
  the shape of keld-ipc's host-side `validate_primary_app_header`: trusted state picks
  the policy, and wire bytes never select their own.

  | Inbound kind | Selected policy | Selector | No match |
  |---|---|---|---|
  | REPLY, `ERR` | for a CALL on the echo channel, the existing `echo_reply_waiter(corr)` (REPLY only, KEL-133 row 4, `receive.rs:194-199`); for any other channel, `reply-waiter:<channel>:<corr>`: the existing private `ReceivePolicy::reply_waiter` (REPLY and `ERR`, exactly `corr`), made public; `lifecycle_reply_waiter` already uses it | the pending-CALL map entry for `header.corr`, which supplies the CALL's channel | abandoned entry: discard and remove it, no append; no entry: `KELD-IPC-005` |
  | EVENT | `event-receiver:<channel>`: `lifecycle_event_receiver` with the channel as a parameter; `lifecycle_event_receiver()` becomes its channel-3 call, so one constructor remains | `header.channel` is in `receive.eventChannels` | `KELD-IPC-005` |
  | CALL | built only from `RECEIVE_POLICIES.echoReceiver` or `privilegedCallReceiver(channel)`, as `receive.callReceivers` names it; both pin the correlation rule to nonzero | `header.channel` | `KELD-IPC-005` |
  | `PING` | `lifecycle-event-receiver`, whose `allowPing` admits `PING` on any channel and correlation id (`receive.rs:428`), as `@keld/api` `link.ts` does today | kind only | flags or payload invalid: `KELD-IPC-005` |
  | any other kind | none | - | `KELD-IPC-005` |

  An admitted `PING` is never appended to the ring and reaches no user code. The Worker
  echoes it through its `WriteQueue` with the same channel and correlation id and an
  empty payload, as `@keld/api` and the host's lifecycle and primary readers do. The
  echo cannot loop: production hosts only echo `PING` and never originate one, and
  `WorkerLink` originates none.

  Channel arguments follow #613 (`docs/specs/gh508-kipc-channel-table.md`, criterion
  10): the public Rust constructors this spec adds or exposes (`reply_waiter`,
  `event_receiver`) take `&'static ChannelEntry` and return
  `Result<ReceivePolicy, IpcError>`, like #613's `privileged_call_receiver`.
  `event_receiver` refuses an entry whose class carries no host EVENTs, with
  `KELD-IPC-005`. When #613 is approved, #528 lands after it; if #613 changes that
  type first, #528 follows it. On the TypeScript side, channel values are #613's
  generated constants, never literals. `lifecycle_reply_waiter(corr)` and
  `lifecycle_event_receiver()` keep their current public signatures; both delegate to
  the constructors above with the lifecycle channel.

  *Fallback if #613 is not approved:* this spec stands on its own. `reply_waiter` and
  `event_receiver` take today's `ChannelId` and still return
  `Result<ReceivePolicy, IpcError>`, and `WorkerLink` uses the current hand-held
  channel constants: echo = 1 (`ECHO_CHANNEL`) and lifecycle = 3
  (`LIFECYCLE_CHANNEL`), both in `keld-ipc` and in `@keld/kipc` `transport.ts`, and
  fs = 2. The fs channel has no named constant today (`ChannelId(2)` appears only in
  `keld-ipc` tests), so it reaches `WorkerLink` only as the role's
  `privilegedCallReceiver(channel)` argument, and this spec adds no constant for it.
  Lifecycle is the only channel with host EVENTs today, so under the fallback
  `event_receiver` refuses every other channel with `KELD-IPC-005`. The now-public `reply_waiter` MUST carry a doc comment stating its kinds,
  its correlation rule and the KEL-133 row it serves.

  A frame with no selected policy is validated against a policy that admits no kind,
  so the validator's own first check produces the `KELD-IPC-005` and no new detail
  string exists. A rejected frame closes the link before any append, so it never
  reaches a waiter, applier or listener. The `receive` table is supplied by the role
  (its framework code), not declared by the host. It only narrows what this Worker
  admits and grants nothing. The host still enforces its own policy on every frame
  the Worker writes: its KEL-133 receivers (`validate_primary_app_header` and the
  privileged receivers) admit or close, the guard decides each CALL, and the host
  alone decides which frames it sends. A role that declares too little closes its own
  link with `KELD-IPC-005`; one that declares more gains nothing. `WorkerLink.open`
  builds each CALL policy
  from its named constructor and throws `KELD-IPC-005` when any channel appears twice
  across `eventChannels`, the built policies' `channel` and their `alsoChannel`, or
  when a channel is 0. No caller-built `ReceivePolicy` object is accepted.
- Before appending, the Worker checks both bounds of the frame's share: free bytes for
  the frame envelope, and a free record slot. With no credit lane the share is the
  whole ring. With the credit lane (§4.8), an uncredited frame's share is the reserved
  uncredited share, and a credited EVENT's share is its channel's share; a frame never
  takes space from another share (criterion 28). If either check fails, the Worker
  stops reading, records 26 in `STATE` (§4.5), and ends the socket. The frame is never dropped silently and nothing already retained is
  discarded. Main delivers the retained records in order, and the pending blocking
  call throws `KELD-IPC-026`. FACT: a 64 KiB ring failed closed 3/3, and the first
  1,024 EVENTs replayed in order.
- Asynchronous REPLY and `ERR`, lifecycle frames and EVENTs share the ring, so one
  ordered path serves every inbound frame except the blocking reply. The blocking
  reply is kept in issue order by `REPLY_AT` and takes no ring space. A blocking
  REPLY or `ERR` whose payload exceeds `replyBytes` fails closed the same way, with
  26 recorded.

### 4.8 Optional ring backpressure through `GRANT` credit (T4)

Credit is not a transport alternative. It is an optional way to keep the ring from
overflowing by pausing the host *producer* instead of failing closed. FACT: arm B+C
passed 3/3; the bound moved to the host producer, which deferred 9,976 EVENTs.

- Direction: Worker to host, `kind = GRANT (9)`, `flags = 0`, `corr = 0`, `channel` =
  the credited EVENT channel.
- Payload v0, postcard:

  ```rust
  /// Additive credit for one EVENT channel, granted by the frame consumer.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  pub struct GrantCredit {
      /// Frames the producer may additionally send on `header.channel`.
      pub frames: u32,
      /// Envelope bytes (header + payload) it may additionally send.
      pub bytes: u32,
  }
  ```

- Credited channels are fixed by the host-declared channel table at open. The Worker
  reserves `ringBytes / 4` and `ringRecords / 4` for uncredited frames (asynchronous
  REPLY and `ERR`, host CALLs, lifecycle). Credit never suspends an uncredited frame,
  so one that does not fit this share fails closed with 26 (§4.7, criterion 28). The
  blocking reply needs no share, because it uses the reply slot (§4.3). The Worker
  splits the rest equally among the credited channels. All shares use integer
  division: the uncredited share is `floor(ringBytes / 4)` bytes and
  `floor(ringRecords / 4)` records, and each of `n` credited channels gets
  `floor((ringBytes - floor(ringBytes / 4)) / n)` bytes and
  `floor((ringRecords - floor(ringRecords / 4)) / n)` records; any remainder is
  unused. With the credit lane enabled, `WorkerLink.open` throws `KELD-IPC-005` before
  the Worker spawns when any share would be 0 records or smaller than one
  `16 + 1`-byte envelope (criterion 29). The first
  `GRANT` per channel declares that channel's window: both fields MUST be nonzero, and
  the host records them as the channel's maximum outstanding credit. The Worker writes
  it only after it has validated the host's `HELLO` reply. It grants freed capacity
  back to the owning channel as `R_BYTES` advances, never above the window.
- The host producer for a credited channel sends an EVENT only when its remaining
  credit covers one frame and the envelope bytes. Otherwise it suspends, and the link
  writer never blocks on it. A suspended producer MUST coalesce or bound its own
  backlog and report its own typed failure. That host-side budget belongs to KEL-80
  ("one authoritative budget"), not to this transport.
- The host closes the link with `KELD-IPC-005` for a `GRANT` with trailing bytes,
  both fields zero, a first `GRANT` with either field zero, a later `GRANT` that would
  raise outstanding frames or bytes above the declared window, or an uncredited
  channel. A `GRANT` before the role's `HELLO` is rejected with `KELD-IPC-005` by the
  existing `server-pre-auth-hello` policy, which admits only `HELLO`.
- Version handling: today no `ReceivePolicy` admits `GRANT`, so a version-2 peer
  rejects it with `KELD-IPC-005` and tears the link down. `HELLO` carries no
  capability field. The credit lane therefore ships only with `PROTOCOL_VERSION = 3`,
  a global bump of both peers, the hello scaffold copy and the corpus, as
  `crates/keld-ipc/AGENTS.md` requires for a shared endpoint. Version 2 and version 3
  fail at the header version check (`KELD-IPC-002`) before any session exists. T1 to
  T3 stay at version 2.

### 4.9 Wire and protocol changes

- Frame layout, kinds and flags: unchanged. Version 2 holds for T1 to T3. T4 bumps to
  version 3 for `GRANT` only.
- Receiver semantics: new corpus rows in the one TSV, with one digest update:
  - `reply-waiter:<channel>:<corr>`, for the existing `reply_waiter` made public
    (§4.7). It admits `REPLY` or `ERR` on the declared channel with exactly that
    correlation id, as `lifecycle-reply-waiter` already does for channel 3. No
    duplicate policy is added. Its `@keld/kipc` mirror is `replyWaiter(channel, corr)`,
    which `lifecycleReplyWaiter` then calls. The echo waiter keeps admitting `REPLY`
    only: KEL-133's table is not amended, and the Worker selects `echo_reply_waiter`
    for echo calls;
  - `event-receiver:<channel>`, the per-channel EVENT policy that the §4.7 table
    selects, with its `@keld/kipc` mirror `eventReceiver(channel)`, which
    `RECEIVE_POLICIES.lifecycleEventReceiver` then equals for channel 3. The selection
    itself is Worker code, tested by criterion 21; the corpus
    covers only the validator under each selected policy;
  - T4 adds `host-grant-receiver` rows, including over-window and pre-`HELLO` cases.
- `ERR` payloads: no change. `CallError` carries the new codes. A payload value is
  public-API review, not a version bump (`crates/keld-ipc/AGENTS.md`).
- Host behaviour: on retire or Quit, the host app-link router answers every call still
  pending after its drain step with `write_call_error` (`KELD-IPC-023` or
  `KELD-IPC-024`), then closes. It does so only on channels whose reply waiter declares
  `ERR`. A pending echo call gets no `ERR`, keeps KEL-133's REPLY-only rule, and
  observes `KELD-IPC-022` at the close (criterion 6). A handler that finishes during the drain sends its
  real REPLY. A real reply that comes before the close always wins.

### 4.10 Platform notes and runtime seam

- macOS: the first proof. FACT: the send space is 8,192 bytes
  (`net.local.stream.sendspace`). The arm-B evidence is macOS arm64 only.
- Linux: the strict profile mounts exactly `/code/main.ts` and
  `/code/kipc-transport.ts`. The self-entry Worker MUST load from the latter. UNKNOWN:
  Bun's Worker resolution under the strict remap, and whether thread creation is
  admitted (INFERENCE: Bun already runs threads). Qualified in T5.
- Windows: `Bun.connect({ unix: "\\\\.\\pipe\\..." })` from a Worker is UNKNOWN.
  Qualified in T5.
- Runtime seam: before this change, the main thread owns the socket, the reader and
  the writer. After it, the Worker owns them and main owns the correlation counter,
  the cursors and the deadline. OS grants: none added. Crash domain: §4.2. Handle
  lifetime: the socket's lifetime is the Worker's lifetime. Value, error and order
  semantics: §4.4 to §4.6. Configuration is captured once, at `WorkerLink.open`.
- Capabilities and manifest (arch 03): none.

### 4.11 Rejected alternatives

- **Arm A, socket on the main thread (current).** FACT, 5/5: the host accepted exactly
  8,192 B (128 frames), then `write_frame` returned `KELD-IPC-006` after blocking
  5,000–5,001 ms. The client saw 0 bytes during the park and no REPLY was possible.
- **Arm C, `GRANT` credit alone as the transport, socket on main.** FACT, 5/5: no
  `KELD-IPC-006`, but the producer was suspended 12,002–12,006 ms, and the REPLY
  arrived only after wake (10,007–10,021 ms). Credit cannot deliver a REPLY to a
  parked reader. It is kept only as §4.8.
- **A second link per role (for example, one owned by a Worker beside the main-thread
  link).** It mints a second principal or a second binding for one role generation.
  That breaks KEL-75's one-link-one-generation binding and arch 02 §1, and principal
  minting is architecture. FACT: Design B never needs one, and the second connect was
  refused `ENOENT` 5/5. A draft that opens a second link to avoid the stall is
  rejected at the wire-protocol review gate.
- **Inverse ownership (app main module in a Worker, transport on main).** It was a
  fallback only if arm B failed, and arm B passed. INFERENCE (#418 packet): app code
  would see `isMainThread = false` and Worker-specific process semantics, which are
  compatibility hazards for corpus dependencies.
- **Waking only at the call deadline on Worker death.** FACT: this is what the
  prototype did, and it is #418 risk 1. Rejected in §4.5.

### 4.12 Architecture 02 sentences changed in this PR

- §1: one new paragraph, "Destination Bun-side link owner (GH-527, approved spec)", after the
  KEL-75 role-instance contract. It states Worker ownership of the client end, that
  every main-thread frame passes through the Worker, that outbound frames reach the
  Worker by `postMessage` while inbound frames arrive through its bounded ordered ring
  (the blocking reply through its reply slot), that a second link is refused, and that
  Worker death is role link loss. It also notes
  that v0 consumers still own the socket on the main thread.
- §2 "Backpressure", v0 sentence: two appended sentences. GH-527 gives `GRANT` its
  first payload: optional credit that the Worker sends to the host, bounding the
  host's EVENTs to the role by free ring space. It ships only behind a
  protocol-version bump.
- §7 "App-role crash": two appended sentences. Transport Worker death is that role's
  link loss, and the parked caller wakes with `KELD-IPC-025`, not at its deadline,
  unless another code (22, 25, 26 or 27) was recorded first, in which case that
  first recorded code stands (§4.5).

### 4.13 Migration unit

- Callers: the hello scaffold `AppLinkSession` (`crates/keld-cli/templates/hello/src/kipc.ts`)
  and the `@keld/api` app-link owner `LifecycleLink` (`packages/@keld/api/src/link.ts`,
  `connect` at line 111) move to `WorkerLink` in T3. `@keld/electron/src/link.ts` only
  re-exports that owner and needs no change.
- Handlers: the host router changes only for §4.9 retire and Quit `ERR`s.
- Generated contracts: none (`keld gen` is not built). Persisted state: none.
- Temporary adapter: the main-thread `connectKipcSocket`, `FrameReader` and
  `DirectedReader` client path, owned by `@keld/kipc`. It is removed in T3 once both
  consumers are on `WorkerLink`. The removal check: no production `connectKipcSocket`
  call outside the Worker entry.
- Permanent compatibility facade: none.

## 5. Boundaries

Implement in:

- `packages/@keld/kipc/src/transport.ts` and its tests (`WorkerLink`, the Worker
  self-entry, the ring, liveness, wake codes);
- `crates/keld-ipc/src/call_error.rs` (constructors for `KELD-IPC-023` and
  `KELD-IPC-024`), `crates/keld-ipc/src/receive.rs` and
  `crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv` (§4.9 rows);
- `crates/keld-core/src/app_session.rs` and `crates/keld-core/src/lifecycle.rs` (the
  pending-call `ERR` on retire and Quit drain);
- `crates/keld-cli/tests/error_registry.rs` (`SCAN_REL`) and
  `docs/engineering/keld-error-codes.md`;
- T3: `crates/keld-cli/templates/hello/src/kipc.ts`, `packages/@keld/api/src/link.ts`
  (`LifecycleLink`, the real link owner; `@keld/electron` re-exports it), and the arch
  02, arch 06 and product-status current-state text;
- T4 only: `crates/keld-ipc/src/lib.rs` (`PROTOCOL_VERSION`), `frame.rs` and a
  `GrantCredit` codec.

Must not touch: `keld-guard`, principal minting and the KEL-75 role registry, the
workspace `Cargo.toml`, the KEL-53 attempt and lifecycle protocols, renderer
`sendSync`, the `showMessageBoxSync` facade, and the `@keld/api` mirror implementation
(F02-T2). The `@keld/api` app-link owner in `link.ts` is in scope for T3; the mirror is
not.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1 — #528 client: criterion-1 failing-first test, then `WorkerLink` with
  self-entry Worker, ring, two cursors, `callBlocking`, liveness, and codes
  `KELD-IPC-022`, `KELD-IPC-025`, `KELD-IPC-026` and `KELD-IPC-027` with registry
  headings and the `SCAN_REL` extension. Corpus rows. Criteria 1 to 5, 8 to 16, 18
  to 22 and 25 to 27 against
  the real `keld-ipc` writer on macOS (the credit case of 18 lands with T4).
- [ ] T2 — #528 host: `KELD-IPC-023` and `KELD-IPC-024` constructors and registry
  headings; the router answers pending calls on retire and Quit drain; host test that
  Worker death takes KEL-75's natural-crash path. Criteria 6, 7 and the host half of 8.
- [ ] T3 — migrate the hello scaffold and the `@keld/api` `LifecycleLink` (which
  `@keld/electron` re-exports) to `WorkerLink`; Linux
  strict self-entry mount proof; remove `DirectedReader` and the main-thread client
  path; update the arch 02, arch 06 and product-status current state.
- [ ] T4 — conditional: only when a consumer shows the ring bound is insufficient.
  `GrantCredit`, the version-3 bump, criteria 17, 23, 24, 28 and 29, and the credit case of 18.
- [ ] T5 — Linux and Windows qualification of criteria 1 to 14 and 18 to 20 on real
  hosts.

## 7. Test plan

| Criterion | Test |
|---|---|
| 1 | Bun `worker-link.test.ts` against a Rust fixture host that uses `keld_ipc::link::write_frame` with `APP_LINK_IO_DEADLINE`; failing first in #528 |
| 2, 3, 5, 10, 12–14 | the same harness, one case per wake path; counts and sequence asserted exactly |
| 4 | the same harness with a test applier and listener recording a global step log; the expected log is `applier*, return, continuation, listener*` |
| 6, 7 | `keld-core` router tests (retire, Quit drain) plus a Bun end-to-end case |
| 8, 9 | per arm: an exit-handler error with a liveness-branch counter, a test-thread terminate, a wedge hook; five arm-B runs counting `KELD-IPC-025` |
| 11 | in-realm second `open`; second connect to the consumed locator |
| 15 | Rust `receiver_corpus.rs` and Bun `corpus.test.ts` on the one TSV |
| 16 | `cargo nextest run -p keld-cli -- error_registry` |
| 17 | T4 version-2 and version-3 mixed `HELLO` and credit runs |
| 18 | the criterion-1 harness with a small ring, a full-size reply and a claim-stall hook; the credit case in T4 |
| 19 | the same harness with a listener hook that holds the dispatch task until `W_RECS` advances; an idle-transition assertion hook |
| 20 | the same harness with a hook that holds Worker message handling until `BLOCKING` is nonzero, recording the host's read order and the ring counters |
| 21 | the same harness with a host that writes each listed frame; a Bun table test of the §4.7 selection |
| 22 | the same harness with a host that never replies, `MAX_ABANDONED_CALLS + 1` expiries, then late replies |
| 23, 24 | T4 Rust host tests with a scripted Worker peer for over-window and pre-`HELLO` `GRANT`s |
| 25 | Bun `open` cases for 3 MiB and 4 MiB rings, plus a counter-start hook at `2^32 - 64` with a blocking reply after the wrap |
| 26 | claim-then-skip-publish and claim-step-throw hooks with a heartbeat-counting watchdog thread |
| 27 | the criterion-1 harness with a host that answers two `call()`s out of order and writes an echo CALL during a park, recording a step log and the host's read frames |
| 28 | T4 harness with credit enabled, a small ring and a host that fills the uncredited share with lifecycle EVENTs |
| 29 | T4 Bun `open` cases with credit enabled and two credited channels, `ringRecords` 3 and 8 |

Anti-flake: no sleep is used for synchronization. The host's 100 EVENT/s pacing is load
generation only. Every assertion is a code, a count or a step log, never a duration. The
liveness test passes the code check whatever the wake latency, because a missing wake
surfaces as `KELD-IPC-006`. Unix socket paths stay under 104 bytes. Each platform-only
path is marked in T5.

## 8. Review gates triggered

unsafe: none. **public API**: the new `@keld/kipc` exports (`WorkerLink`,
`WorkerReceiveTable`, `WorkerCallReceiver`, `WorkerCallReply`, `setCallHandler`, `replyWaiter(channel, corr)`,
`eventReceiver(channel)`, the `replyBytes` option, the constants); `keld-ipc`'s
`ReceivePolicy::reply_waiter` made public with a doc comment, and its new
`ReceivePolicy::event_receiver`, both taking #613's `&'static ChannelEntry` (today's
`ChannelId` under the §4.7 fallback) and returning `Result<ReceivePolicy, IpcError>`,
with `lifecycle_reply_waiter` and `lifecycle_event_receiver` unchanged; and the new `CallError` codes.
permission model: none (no capability, manifest or mount change). dependency addition: none. **wire protocol**: new receiver
corpus rows, the host `ERR` on retire and Quit, the Worker as the link endpoint, and
(T4) the `GRANT` payload with the version-3 bump. Review rejects any draft that opens a
second link per role.

## 9. Perf impact

No performance number is a pass criterion, and this spec states no round-trip figure.
Budgets in architecture 01 §5 that could move: idle RSS (one extra Worker thread, a
1 MiB ring and a 64 KiB reply slot per role), cold start (Worker spawn before `HELLO`)
and the kipc small-message round trip (one extra thread hop). Bench to run: the architecture 01
§5.1 harness for those rows once it lands. Decomposition:

- census: one role, one link, arm-B load;
- work: one copy from socket chunk to ring (or to the reply slot), and one copy out
  for the reply;
- queue and copy: the ring is the only inbound queue, bounded in bytes and records;
  the reply slot holds at most one reply;
- clock: none is asserted;
- statistic: none is claimed;
- artifact: `s0-link-drain.json` (#418) is prototype evidence only. Any later figure
  appears only under a registered metric id.

## 10. Open questions

Review notes (wire and public-API gate, recorded, not blocking): `setStateApplier` is
public on `WorkerLink`, so "framework-only" is a usage rule that the type does not
enforce; #449 should keep the `WorkerLink` handle out of application code. Outbound
frames are not bounded beyond `MAX_FRAME_LEN` per frame: the `postMessage` queue and
the `WriteQueue` promise chain are as unbounded as today's main-thread `WriteQueue`.

1. Confirm the default bounds (`1 MiB` ring, `16,384` records, `64 KiB` reply slot). The modal event rate
   is UNKNOWN (#418 risk 2). Recommendation: keep the defaults, and let F02-T2 or
   F06-T7 measure the real rate before any change.
2. After `KELD-IPC-025`, if `terminate()` does not close a wedged Worker's socket,
   should the transport also end the role process so the host's crash path runs?
   Recommendation: yes. Exit the role process right after surfacing the error, since a
   role without a link cannot recover in place.
3. Confirm `MAX_ABANDONED_CALLS = 256`. Recommendation: keep it; a host that leaves
   that many calls unanswered is stuck, and criterion 22 proves the typed failure.
4. Confirm the liveness constants (100 ms heartbeat, 1 s window). Recommendation:
   keep them. Criterion 9 falsifies the window if it is too tight under load.

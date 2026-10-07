# Spec: worker-owned single-link blocking host CALL transport for Bun roles
Status: draft
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
   Worker's `Atomics.notify` makes the call end with `KELD-IPC-006` at its deadline.
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
6. **Generation retire.** Given a parked call, when the host retires the role
   generation while the link is writable, then the host answers the call with an `ERR`
   whose `CallError.code` is `KELD-IPC-023`, then closes the link, and `callBlocking`
   throws `KELD-IPC-023`. *Negative control:* a host that closes without the `ERR`
   makes the test observe `KELD-IPC-022`. A client that maps every EOF to
   `KELD-IPC-023` fails criterion 5.
7. **Quit.** (a) Given a parked blocking `Quit`, when the host accepts it, then the
   call returns the host's real `LifecycleResponse::Quit` bytes and the link closes
   afterwards. (b) Given another blocking call still pending when the host's Quit
   drain ends, then that call throws `KELD-IPC-024`. *Negative control:* a host that
   closes without writing the Quit REPLY makes (a) throw `KELD-IPC-022`, so a
   client-synthesized Quit success fails (a).
8. **Worker death or wedge wakes immediately.** Given a parked call with a 30 s
   deadline, when the transport Worker is terminated, or stops running its event loop
   through a test-only hook, then `callBlocking` throws `KELD-IPC-025`, not
   `KELD-IPC-006`. The host observes link loss and takes KEL-75's natural-crash path
   for that generation. *Negative control:* removing the liveness check (§4.5) makes
   the call fail with `KELD-IPC-006` at its deadline, so the code assertion fails
   (#418 risk 1).
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
    A second `callBlocking` while one is in flight throws `KELD-IPC-005` before any
    write. *Negative control:* matching the "next REPLY" instead of the correlation id
    makes the abandoned-id case return the wrong bytes.
13. **Deadline on every call.** Given `callBlocking` without a finite positive
    deadline no greater than `MAX_BLOCKING_CALL_DEADLINE_MS`, then it throws
    `KELD-IPC-005` before any frame is written. Given a host that never replies, then
    the call throws `KELD-IPC-006` at its deadline, the link stays up, and a following
    call succeeds. *Negative control:* an infinite default deadline makes the
    invalid-deadline case fail.
14. **No fabricated reply on any wake path.** Given each wake path (close, retire,
    Quit drain, Worker death, Worker wedge, overflow, deadline), then `callBlocking`
    throws the code listed in §4.4 and never returns a value. *Negative control:* any
    one path returning a default value fails the sweep.
15. **Receiver corpus.** Given the canonical TSV with the §4.9 rows, then Rust and Bun
    both replay it and print one digest. *Negative control:* changing the TypeScript
    blocking-reply-waiter rule without changing the TSV fails the Bun corpus test.
16. **Registered codes.** Given the five codes in §4.4, then each has a registry
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
| A5 | Blocking wait and wake | main parks on one `Int32Array` word; Worker notifies | lost wake | criterion 2 | FACT: NC2 (no notify) gives `KELD-IPC-006` at deadline 2/2; arm B main woke once per call |
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
- A8 to A6: an `ERR` written before close must reach main before the close. Both are
  ring records in arrival order, so the earlier frame wins;
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
  calls, outbound EVENTs and every inbound frame. There is no side channel.
- The Worker entry is the staged transport file itself. No third file is staged or
  mounted. The entry branch is selected only by the marker that `WorkerLink.open`
  passes in `workerData`. "Not the main thread" is not enough: an application Worker
  that imports the transport MUST NOT start a link.
- Crash domains. Worker death leaves the role process alive but without a link. The
  transport converts it into terminal link loss: every later call throws
  `KELD-IPC-025`, and the socket closes when the Worker dies. The host owns the crash
  decision through KEL-75's natural-crash path. No new crash owner is added.
- The Worker runs no application code. It imports only the transport module, and no
  user listener or applier runs on it.

### 4.3 Shared memory layout

One `SharedArrayBuffer` is allocated at `WorkerLink.open`. It is never resized and
there is no allocation per frame in the ring path. It holds a 64-byte control block of
`Int32` words, then the ring.

```text
word  name        writer   meaning
0     SEQ         Worker   bumped + Atomics.notify on: blocking reply/err appended, close, overflow, Worker exit
1     STATE       both     0 OPEN, 1 CLOSED, 2 OVERFLOW, 3 WORKER_DEAD (main may store 3 on liveness failure)
2     CLOSE_CODE  Worker   0, or the KELD-IPC number that ended the link (22, 26)
3     W_BYTES     Worker   monotonic byte write counter (u32, modular)
4     A_BYTES     main     apply cursor: records before it have had their state fact applied
5     R_BYTES     main     dispatch cursor: records before it are dispatched; frees ring space
6     W_RECS      Worker   monotonic record counter (u32, modular)
7     R_RECS      main     records released by the dispatch cursor
8     HEARTBEAT   Worker   bumped every WORKER_HEARTBEAT_INTERVAL_MS and after every frame
9     BLOCKING    main     correlation id of the in-flight blocking call, 0 when none
10-15 reserved    -        zero
```

A ring record is the received 16-byte frame header followed by its payload, written
contiguously with wrap-around. Counters use modular `u32` arithmetic. The ring byte
capacity is at most 2^30, so `(W - R) >>> 0` is never ambiguous. This removes the
prototype's 2^31 per-session limit. The Worker stores the bytes, then publishes
`W_BYTES` and `W_RECS` with `Atomics.store`. Main reads them with `Atomics.load`.

Bounds, fixed at open:

```ts
export const DEFAULT_RING_BYTES = 1 << 20;        // 1 MiB
export const DEFAULT_RING_RECORDS = 16_384;
export const MAX_RING_BYTES = 1 << 30;
export const MAX_BLOCKING_CALL_DEADLINE_MS = 24 * 60 * 60 * 1000;
export const WORKER_HEARTBEAT_INTERVAL_MS = 100;
export const WORKER_LIVENESS_WINDOW_MS = 1_000;
```

A frame whose envelope is larger than the ring's byte capacity can never be retained,
so it is an overflow (§4.7). A role that needs larger inline frames opens with a larger
`ringBytes`, up to `MAX_RING_BYTES`. Bulk payloads stay on the bulk plane (arch 02 §3).
INFERENCE: the defaults hold #418's 704,000-byte, 11,000-record park with headroom.
The real event rate during a modal is UNKNOWN (#418 risk 2), so the defaults are open
question 1.

### 4.4 Typed errors

Reserved numbers. `KELD-IPC-018` to `KELD-IPC-021` are taken by open PR #607, so this
spec reserves `KELD-IPC-022` to `KELD-IPC-026`. The registry rule
(`docs/engineering/keld-error-codes.md`) fails a heading that no scanned tree emits.
The headings therefore land in #528 together with the code that emits them, and T1
adds `packages/@keld/kipc/src` to `SCAN_REL` in `crates/keld-cli/tests/error_registry.rs`.
That test today sees TypeScript codes only when Rust happens to emit the same string.
If #607 or another change takes these numbers first, #528 takes the next free numbers
and updates this table in the same PR.

| Code | Emitter | When | Message | Fix |
|---|---|---|---|---|
| `KELD-IPC-022` | `@keld/kipc` | the link reached EOF or an I/O error while a call was pending, and no host `ERR` arrived for it | link closed before the host replied; no reply was received and the call's host effect is unknown | Treat the call as not answered. The role's link is gone: check the host log for the close cause, and do not retry on this link, because it cannot reconnect. |
| `KELD-IPC-023` | `keld-ipc` (host writes it with `write_call_error`) | the host retired this role generation before the call's handler finished | role generation retired before this call completed | The role instance is being replaced or stopped. Do not retry here; the successor generation reissues the work after its own `Ready`. |
| `KELD-IPC-024` | `keld-ipc` (host writes it with `write_call_error`) | the host accepted `Quit` and its drain ended with this call still pending | session ended by an accepted Quit before this call completed | The application is quitting. Do not issue new work; finish only the shutdown path. |
| `KELD-IPC-025` | `@keld/kipc` | the transport Worker exited, or its heartbeat stopped for `WORKER_LIVENESS_WINDOW_MS`, while the role is open | transport Worker dead or unresponsive; the role's link is lost | The role has no link and cannot reconnect. Report the crash. The host restarts the role per its policy; check the role log for the Worker's last error. |
| `KELD-IPC-026` | `@keld/kipc` | a frame did not fit the ring's byte or record bound and no credit lane was active | parked event ring full; the link was closed rather than drop an event | Raise `ringBytes` or `ringRecords` at `WorkerLink.open`, reduce the host event rate toward this role, or enable the credit lane (T4). Retained events were delivered in order. |

Reused codes: `KELD-IPC-005` for a second `WorkerLink.open`, a second in-flight
blocking call, an invalid deadline, an unsolicited correlation id, a malformed `GRANT`
or an applier that throws. `KELD-IPC-006` for an expired call deadline. Only the
deadline leaves the link up. A late REPLY for the abandoned id is discarded.

### 4.5 Blocking call and Worker liveness

```ts
export interface WorkerLinkOptions {
  link: string;                     // KELD_APP_LINK text; parsed only in the Worker
  receive: ReceivePolicy;           // host-declared inbound policy (KEL-133)
  ringBytes?: number;               // default DEFAULT_RING_BYTES
  ringRecords?: number;             // default DEFAULT_RING_RECORDS
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
  onEvent(channel: number, listener: (payload: Uint8Array) => void): () => void;
  close(): void;
}
```

`callBlocking`:

1. It validates the deadline: finite, greater than 0 and at most
   `MAX_BLOCKING_CALL_DEADLINE_MS`. Otherwise it throws `KELD-IPC-005`.
2. `BLOCKING` MUST be 0; otherwise it throws `KELD-IPC-005`. Main allocates the
   correlation id from its one counter, which skips 0 and every pending or abandoned
   id. Main stores the id in `BLOCKING` and posts the CALL to the Worker with
   `postMessage`. FACT: `postMessage` from a parked main reaches the Worker (#418).
3. Main loops on `Atomics.wait(ctrl, SEQ, seen, slice)`, where `slice` is the smaller
   of the remaining deadline and `WORKER_HEARTBEAT_INTERVAL_MS`. After every return it
   checks, in this order:
   - a REPLY or `ERR` record for `BLOCKING` in the ring: apply facts (§4.6), then
     return the REPLY bytes, or throw `errorFromErrFrame` for an `ERR`;
   - `STATE` other than OPEN: throw `KELD-IPC-022`, `KELD-IPC-026` or `KELD-IPC-025`
     as recorded;
   - `HEARTBEAT` unchanged for `WORKER_LIVENESS_WINDOW_MS`: store `WORKER_DEAD`, call
     `worker.terminate()` once main resumes, and throw `KELD-IPC-025`;
   - the deadline passed: move the id to the abandoned set and throw `KELD-IPC-006`.
4. In every outcome, `BLOCKING` is reset to 0 before the call returns or throws.

Worker liveness. This is the mechanism #418 risk 1 requires:

- *Orderly exit* (an uncaught error, `self.close()`, or the transport closing): the
  Worker's exit handler stores `WORKER_DEAD` (or `CLOSED`), bumps `SEQ` and notifies.
  The parked caller wakes at once.
- *Abrupt death or a wedge*: the Worker bumps `HEARTBEAT` on a
  `WORKER_HEARTBEAT_INTERVAL_MS` timer and after each frame it handles. A parked main
  wakes at least once per heartbeat interval and throws `KELD-IPC-025` once the
  heartbeat has not moved for `WORKER_LIVENESS_WINDOW_MS`. The wake is bounded by the
  liveness window and does not depend on the call deadline.
- *Not parked*: main also listens for the Worker's `error` and `close` events. Every
  pending asynchronous call rejects with `KELD-IPC-025`, and the link is terminal.
- *Host side*: Worker death closes the socket. FACT: the host saw `KELD-IPC-001`
  broken pipe at once in both runs. The host handles it as role link loss (arch 02
  §7; KEL-75 natural crash). UNKNOWN: whether `terminate()` closes the socket of a
  Worker wedged in a synchronous loop (open question 2).

Rejected liveness options: waiting for the Worker `close` event (FACT: it cannot run
on a parked main; the probe woke only at its deadline), and host-only detection (the
host has no path to a parked main except the dead link).

### 4.6 Wake-time rule (mirror facts before return, listeners after resume)

No application JavaScript runs while main is parked. Main is inside `Atomics.wait`,
and the Worker runs only transport code. This reproduces #419 E3: Electron 44.4.5 runs
no main-process JavaScript during `showMessageBoxSync`, then delivers queued work in
order and loses none. Issue order is the order in which the host wrote frames to the
link; the ring keeps arrival order, which is the same order on a stream.

The ring has two main-side cursors over one record sequence:

1. On wake, before `callBlocking` returns, main walks from `A_BYTES` up to and
   including the blocking reply record. For each EVENT record it runs the channel's
   registered state applier, if one exists, then advances `A_BYTES`. It runs no
   listener and calls no user code. It copies the reply payload out. It does not
   apply facts that arrived after the reply: the caller sees host state as of the
   reply.
2. It schedules one ordinary event-loop task with `setImmediate`, not a microtask.
   That task walks from `R_BYTES`. It runs the applier for any record not yet applied
   (records at or after `A_BYTES`), dispatches the record's listeners, then advances
   `R_BYTES` and `R_RECS`, which frees ring space. Already-returned reply records are
   skipped.
3. Not parked: the Worker posts one `kick` message whenever the ring becomes
   non-empty, and the kick task performs step 2. Both paths use the one dispatch
   cursor, so listener order is issue order whatever mix of parked and unparked
   delivery occurs.

The ring is therefore also the listener FIFO. No second queue exists, and memory held
for undispatched listeners stays inside the ring bound. An applier MUST be synchronous
and MUST NOT call user code. A throwing applier makes the link terminal with
`KELD-IPC-005`: a mirror that cannot apply a fact must not serve stale state. #449
(F02-T2) owns the mirror; its zero-round-trip getters rely on step 1, and its sequence
test lands with #528.

### 4.7 Bounded ordered ring and overflow

- The Worker validates each inbound frame with the KEL-133 validator under the
  host-declared `receive` policy before appending it. An undeclared kind, channel or
  correlation closes the link with `KELD-IPC-005`.
- Before appending, the Worker checks both bounds: free bytes for the frame envelope,
  and a free record slot. If either fails and no credit lane is active, the Worker
  stops reading, stores `CLOSE_CODE = 26` and `STATE = OVERFLOW`, notifies, and ends
  the socket. The frame is never dropped silently and nothing already retained is
  discarded. Main delivers the retained records in order, and the pending blocking
  call throws `KELD-IPC-026`. FACT: a 64 KiB ring failed closed 3/3, and the first
  1,024 EVENTs replayed in order.
- REPLY, `ERR` and lifecycle frames share the ring with EVENTs, so one ordered path
  serves every inbound frame.

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
  reserves `ringBytes / 4` and `ringRecords / 4` for uncredited frames (REPLY, `ERR`,
  lifecycle). It splits the rest equally among the credited channels in its first
  `GRANT` per channel, sent right after `HELLO`. It grants freed capacity back to the
  owning channel as `R_BYTES` advances.
- The host producer for a credited channel sends an EVENT only when its remaining
  credit covers one frame and the envelope bytes. Otherwise it suspends, and the link
  writer never blocks on it. A suspended producer MUST coalesce or bound its own
  backlog and report its own typed failure. That host-side budget belongs to KEL-80
  ("one authoritative budget"), not to this transport.
- A `GRANT` with trailing bytes, both fields zero, a total over the declared window,
  or an uncredited channel closes the link with `KELD-IPC-005`.
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
  - `blocking-reply-waiter:<channel>:<corr>`, which admits `REPLY` or `ERR` on the
    declared channel with exactly that correlation id. Today the echo waiter admits
    `REPLY` only, while the lifecycle waiter already admits both;
  - `worker-inbound:<role policy>`, the Worker's admission policy for every inbound
    kind before a ring append;
  - T4 adds `host-grant-receiver` rows.
- `ERR` payloads: no change. `CallError` carries the new codes. A payload value is
  public-API review, not a version bump (`crates/keld-ipc/AGENTS.md`). A client built
  before this spec that receives an `ERR` on the echo channel fails closed with
  `KELD-IPC-005`. That can happen only at retire or Quit, when the link is closing
  anyway.
- Host behaviour: on retire or Quit, the host app-link router answers every call still
  pending after its drain step with `write_call_error` (`KELD-IPC-023` or
  `KELD-IPC-024`), then closes. A handler that finishes during the drain sends its
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

- §1: one new paragraph, "Destination Bun-side link owner (GH-527, draft)", after the
  KEL-75 role-instance contract. It states Worker ownership of the client end, that
  every main-thread frame passes through the Worker and its bounded ordered ring, that
  a second link is refused, and that Worker death is role link loss. It also notes
  that v0 consumers still own the socket on the main thread.
- §2 "Backpressure", v0 sentence: one appended sentence. GH-527 gives `GRANT` its
  first payload (optional app-to-host ring credit) and ships it only behind a
  protocol-version bump.
- §7 "App-role crash": one appended sentence. Transport Worker death is that role's
  link loss, and the parked caller wakes with `KELD-IPC-025`, not at its deadline.

### 4.13 Migration unit

- Callers: the hello scaffold `AppLinkSession` (`crates/keld-cli/templates/hello/src/kipc.ts`)
  and `@keld/electron` `link.ts` move to `WorkerLink` in T3.
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
- T3: `crates/keld-cli/templates/hello/src/kipc.ts`, `packages/@keld/electron/src/link.ts`,
  and the arch 02, arch 06 and product-status current-state text;
- T4 only: `crates/keld-ipc/src/lib.rs` (`PROTOCOL_VERSION`), `frame.rs` and a
  `GrantCredit` codec.

Must not touch: `keld-guard`, principal minting and the KEL-75 role registry, the
workspace `Cargo.toml`, the KEL-53 attempt and lifecycle protocols, renderer
`sendSync`, the `showMessageBoxSync` facade, and the `@keld/api` mirror implementation.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1 — #528 client: criterion-1 failing-first test, then `WorkerLink` with
  self-entry Worker, ring, two cursors, `callBlocking`, liveness, and codes
  `KELD-IPC-022`, `KELD-IPC-025` and `KELD-IPC-026` with registry headings and the
  `SCAN_REL` extension. Corpus rows. Criteria 1 to 5 and 8 to 16 against the real
  `keld-ipc` writer on macOS.
- [ ] T2 — #528 host: `KELD-IPC-023` and `KELD-IPC-024` constructors and registry
  headings; the router answers pending calls on retire and Quit drain; host test that
  Worker death takes KEL-75's natural-crash path. Criteria 6, 7 and the host half of 8.
- [ ] T3 — migrate the hello scaffold and `@keld/electron` to `WorkerLink`; Linux
  strict self-entry mount proof; remove `DirectedReader` and the main-thread client
  path; update the arch 02, arch 06 and product-status current state.
- [ ] T4 — conditional: only when a consumer shows the ring bound is insufficient.
  `GrantCredit`, the version-3 bump and criterion 17.
- [ ] T5 — Linux and Windows qualification of criteria 1 to 14 on real hosts.

## 7. Test plan

| Criterion | Test |
|---|---|
| 1 | Bun `worker-link.test.ts` against a Rust fixture host that uses `keld_ipc::link::write_frame` with `APP_LINK_IO_DEADLINE`; failing first in #528 |
| 2, 3, 5, 10, 12–14 | the same harness, one case per wake path; counts and sequence asserted exactly |
| 4 | the same harness with a test applier and listener recording a global step log; the expected log is `applier*, return, continuation, listener*` |
| 6, 7 | `keld-core` router tests (retire, Quit drain) plus a Bun end-to-end case |
| 8, 9 | Worker terminate and a wedge hook; five arm-B runs counting `KELD-IPC-025` |
| 11 | in-realm second `open`; second connect to the consumed locator |
| 15 | Rust `receiver_corpus.rs` and Bun `corpus.test.ts` on the one TSV |
| 16 | `cargo nextest run -p keld-cli -- error_registry` |
| 17 | T4 version-2 and version-3 mixed `HELLO` and credit runs |

Anti-flake: no sleep is used for synchronization. The host's 100 EVENT/s pacing is load
generation only. Every assertion is a code, a count or a step log, never a duration. The
liveness test passes the code check whatever the wake latency, because a missing wake
surfaces as `KELD-IPC-006`. Unix socket paths stay under 104 bytes. Each platform-only
path is marked in T5.

## 8. Review gates triggered

unsafe: none. **public API**: the new `@keld/kipc` exports (`WorkerLink`, the
constants) and the new `CallError` codes. permission model: none (no capability,
manifest or mount change). dependency addition: none. **wire protocol**: new receiver
corpus rows, the host `ERR` on retire and Quit, the Worker as the link endpoint, and
(T4) the `GRANT` payload with the version-3 bump. Review rejects any draft that opens a
second link per role.

## 9. Perf impact

No performance number is a pass criterion, and this spec states no round-trip figure.
Budgets in architecture 01 §5 that could move: idle RSS (one extra Worker thread plus a
1 MiB ring per role), cold start (Worker spawn before `HELLO`) and the kipc
small-message round trip (one extra thread hop). Bench to run: the architecture 01
§5.1 harness for those rows once it lands. Decomposition:

- census: one role, one link, arm-B load;
- work: one copy from socket chunk to ring, and one copy out for the reply;
- queue and copy: the ring is the only queue, bounded in bytes and records;
- clock: none is asserted;
- statistic: none is claimed;
- artifact: `s0-link-drain.json` (#418) is prototype evidence only. Any later figure
  appears only under a registered metric id.

## 10. Open questions

1. Confirm the default ring bounds (`1 MiB`, `16,384` records). The modal event rate
   is UNKNOWN (#418 risk 2). Recommendation: keep the defaults, and let F02-T2 or
   F06-T7 measure the real rate before any change.
2. After `KELD-IPC-025`, if `terminate()` does not close a wedged Worker's socket,
   should the transport also end the role process so the host's crash path runs?
   Recommendation: yes. Exit the role process right after surfacing the error, since a
   role without a link cannot recover in place.
3. Confirm the liveness constants (100 ms heartbeat, 1 s window). Recommendation:
   keep them. Criterion 9 falsifies the window if it is too tight under load.

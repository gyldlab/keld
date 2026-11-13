# Design 1: First principles, reuse and rejected alternatives

Part of: [index](index.md)
Owns: Design 1: First principles, reuse and rejected alternatives

## 4. Design: First principles, reuse and rejected alternatives

Facts (each one is an atom from the GH-1095 claim, verified at `516644b0`):

- **F1. The dispatch point.** The renderer CALL's dispatch point is the
  instant `write_frame` returns `Ok` inside `begin_echo_call`
  (`crates/keld-core/src/app_session.rs:5853@516644b0e1c086baa08ff8e8b52667f4bc97144a`).
  - `write_frame` writes the header with `write_all`, then the payload with
    `write_all`, then flushes (`crates/keld-ipc/src/link.rs:835@516644b0e1c086baa08ff8e8b52667f4bc97144a`).
  - The host can observe this point: `KELD_KEL142_KIPC_CALL` is printed only
    after it (`app_session.rs:4245-4252`).
- **F2. Pre-write refusals.** The refusals in `begin_echo_call` all return
  before the slot is armed and before `write_frame` is invoked
  (`app_session.rs:5812-5840`). They are:
  - quiescing;
  - generation lock poisoned;
  - no admitted generation;
  - pending lock poisoned;
  - slot busy.
- **F3. What a failed write does not tell the host.** `write_all` reports `Err`
  without reporting how many bytes reached the kernel.
  - Bun's `FrameReader` dispatches only complete frames
    (`packages/@keld/kipc/src/transport.ts:664-665@516644b0e1c086baa08ff8e8b52667f4bc97144a`).
  - So "a failed write is never dispatched" holds only if no later writer
    completes the truncated frame on that stream.
  - A census of every writer to that stream has not been done. Until fault
    injection and that census exist, a failed write is **Unknown**. This is the
    #1081 final review's caveat on the failed-write case.
- **F4. Nothing is observable after the dispatch point.** Once the write has
  returned, nothing the host can see separates these cases:
  - Bun never read the frame;
  - Bun read it but did not run it;
  - the handler ran;
  - the handler ran and replied.

  A successor generation is a new principal with no memory of its predecessor,
  so it cannot attest either. Only a live receiver can say "not processed"
  (RFC 9113 §8.7: a server "MUST NOT indicate that a stream has not been
  processed unless it can guarantee that fact").
- **F5. The no-handler check runs before any app code.**
  `dispatchApplicationCall` throws "Echo channel has no registered handler"
  before it calls `decodeRequest` or the handler
  (`packages/@keld/api/src/channels.ts:95-97@516644b0e1c086baa08ff8e8b52667f4bc97144a`). `resolveEchoCall` then maps
  every throw to one `KELD-API-001` ERR (`packages/@keld/api/src/echo-call.ts:30-37@516644b0e1c086baa08ff8e8b52667f4bc97144a`).
- **F6. The link opens lazily.** It opens only on the first `app.whenReady()`,
  `app.quit()`, `fs` or `auth` call. `ensureLink` is at
  `packages/@keld/api/src/app.ts:74-97@516644b0e1c086baa08ff8e8b52667f4bc97144a`, and its only callers are at `108`
  (quit), `122` (whenReady), `164` (fs) and `171` (auth). `channels.handle` is synchronous and never touches the
  link. A handler registered before the first `whenReady()` therefore exists
  before any CALL can arrive.
- **F7. The page facade cannot see dispatch.**
  - On `pagehide` it rejects its pending call
    (`crates/keld-wv/src/wkwebview/macos_bridge.rs:63-68@516644b0e1c086baa08ff8e8b52667f4bc97144a`).
  - It has no signal that tells it whether the isolated world queued the call
    before bind or the host already wrote it.
- **F8. Bun outbound codes are shared between Not sent and Unknown.**
  - `WorkerLink.call` throws the recorded terminal code from `#throwIfTerminal`
    before it posts to the Worker (`transport.ts:2011-2017`, `2177-2180`).
  - It rejects a still-pending call with the same code in `#finalize`
    (`transport.ts:2506-2514`).

- **F9. The host forwards Bun's ERR code verbatim.** The reader decodes the
  Echo ERR's `CallError` (`app_session.rs:7766-7770`), and the dispatch thread
  copies `error.code` into the renderer outcome (`:4272-4278`). Today, Bun can
  therefore make the page see any code, including `KELD-WV-012`, for a call
  that Bun received.
- **F10. A native result reaches whichever document is current.**
  - `reply_immediate_error` (`macos_bridge.rs:801-822`) and the dispatch outcome
    path evaluate `__keldBridgeNativeResult` in the WebView's current isolated
    world.
  - That function (`:137-147`) clears `inflight` and relays the result without
    checking its request id, and every document starts request ids at 1.
  - So a refusal of a stale invoke from a predecessor document, handled after
    the successor binds, can settle the successor's pending call.

Reuse:

- The codes travel in fields that already exist:
  - `CallError.code` inside a KIPC ERR;
  - the wv-link result `code`;
  - the page `Error` message prefix.
- Settlement reuses the slots that already exist:
  - the page facade `pending`;
  - isolated-world `inflight`;
  - native `BridgeState.pending`, matched on navigation and request;
  - host `pending_echo`, matched on correlation and attempt
    (`app_session.rs:5885-5918`).
- Nothing is rewritten.

Rejected alternatives:

- **A single "retryable" flag on WV-011.** It still hides three different
  policies: no generation, busy and quitting.
- **WV-012 for every pre-dispatch failure, including a failed write.** F3: this
  is unproven.
- **A host-side dedup store or result cache.** The host sees the dispatch point,
  not the commit of the effect. A cache would fabricate results for Unknown
  calls and would carry a predecessor generation's results to a new principal.
- **"Last dispatched id" in the restart notification ([#1098](https://github.com/gyldlab/keld/issues/1098)).** "Dispatched" is not
  "processed". The owner has also decided that the notification carries no ids
  or counters.
- **A Keld status-query API.** Only the app's durable store knows whether the
  effect happened.
- **Reclassifying `KELD-IPC-022` for the `#throwIfTerminal` case.** [historical §4.5](design-5.md).

Compatibility fallback: none required. The codes are additive. [historical §4.3](design-3.md) states what
still returns the old codes, and the safe default ([historical §4.4](design-4.md): an unrecognized code
means Unknown) keeps old callers correct.

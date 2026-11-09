# Spec: request error codes split by dispatch proof (KEL-142 §7 revision)

Status: draft — **revision r2, pending fresh owner approval.** The owner approved
the content at `d1801f97`. r2 changes only the two items in "Revision r2" below.
Linear: GH-1095 (#517; tracker of record; linked from KEL-143) · Revises: KEL-142 §7 ·
Owner: repository owner · Updated: 2026-10-23

Code citations are at `origin/main` `516644b0`. Evidence for the current
behaviour comes from the never-merge experiment PR #1082 (issue #1081). This
draft authorizes no production change. Implementation starts only after the
exact content is approved.

## Revision r2 (2026-10-23, docs only; needs a new owner approval)

The implementation review of PR #1105 found two defects in the approved
content. Owner decision 3 authorized drafting this revision.

1. **§4.7's premise was unreachable.** A host `navigate()` cannot run while
   the app runs. `run_app_until_quit` holds `&mut self` and moves the views
   and bridges into the event loop (`wkwebview/mod.rs:302`, `:391` on #1105),
   and `AppWindowCommand` has no navigate command. So "a bridge reset without a
   commit" cannot happen today. r2 revises criterion 7, §4.7 and AT-S arm (d)
   to match. Exactly-once now applies to every navigation that can actually
   occur, and the uncommitted host-navigation case is deferred to its future
   owner (#459).
2. **Criteria 10 and 13 conflicted.** The host prefixing rule
   (`app_session.rs:5789` on #1105) changes the message bytes when a Bun
   message starts with its code but not with `"<code>: "`. r2 picks one
   precise prefix rule with one owner (§4.9). It states exactly which messages
   keep their bytes and which gain the prefix.

## 1. Goal & non-goals

A rejected `window.keld.invoke` call or Bun application call has to tell the
app one thing: whether it is safe to send the request again. Today it cannot.

- `KELD-WV-011` is registered as "rejected before application dispatch". The
  host also returns it after the CALL frame was written, when the app link ends
  or the page navigates while a call is pending.
- `KELD-API-001` covers both "no handler is registered", where no app code ran,
  and "the handler threw", where the handler may have run.

The app therefore cannot tell "never sent" from "may have run".

This revision splits the codes by what Keld can prove:

| Code | Class | Meaning |
|---|---|---|
| `KELD-WV-011` (narrowed) | Not sent, not retryable as is | the renderer request was refused before the host attempted any write: invalid, busy, stale, undeclared, or the app is quitting |
| `KELD-WV-012` (new) | **Definitely not dispatched** | no admitted application generation existed, so the host attempted no write |
| `KELD-WV-013` (new) | **Outcome unknown** | the host attempted the CALL write, or the page navigated while the call was pending; the handler may have run |
| `KELD-API-002` (new) | **Not executed** (asserted by `@keld/api`) | the call reached Bun, but no handler was registered, so no app code ran |
| `KELD-API-001` (narrowed) | Outcome unknown | the call reached the application stage and failed: decode, handler throw, encode, or oversize reply |

Observable outcome: each renderer invoke settles exactly once with one of these
codes or a reply. No layer of Keld ever replays a request. The rules for an
app resend are in §4.4:

- only WV-012 or API-002 permits one *automatic* resend of an action that is
  not idempotent;
- WV-011 is never resent automatically, whatever its detail says;
- WV-013 and API-001 may be resent only under §4.4 (a) or (b).

The app makes every resend; Keld makes none.

Non-goals:

- navigation policy, the trusted-page predicate, and which documents may hold
  the bridge (separate work);
- the restart notification's design, delivery and ordering ([#1098](https://github.com/gyldlab/keld/issues/1098);
  once it ships, this spec consumes it as the WV-012 restart trigger, §4.4);
- the secure app origin (#1090, KEL-79);
- a host-side dedup store, a "last dispatched id", or a Keld status-query API
  (all rejected, §4.1);
- a reply deadline for a hung Bun (§4.7; liveness, not classification);
- Windows or Linux renderer bridges (none exist; §4.6);
- new KIPC codes for Bun-side outbound calls (§4.5).

## 2. Spec refs

- `docs/specs/kel142-macos-renderer-bridge-api.md` §4.1 (no automatic replay),
  §4.2 (terminal outcomes), §6 (ordering, one terminal), §7 (errors). This
  revision replaces §7 and refines §4.2's "missing handler" bullet. Every other
  KEL-142 rule is unchanged.
- `docs/specs/gh527-worker-owned-blocking-call-transport.md` §4.4 (`WorkerLink`
  terminal codes) and §4.6 (`@keld/api` ERR answers). Consumed. Per owner
  decision Q3 (§10), T0 corrects only the three passages where it says that a
  call pending at a Quit-close "neither ran" (§4.5).
- `docs/architecture/02-ipc.md`: KIPC frames and the `CallError` payload. Not
  changed.
- `docs/architecture/07-agent-experience.md` §2 and
  `docs/engineering/keld-error-codes.md`: registry rules (a heading lands only
  with its first emitter).
- KEL-143 acceptance: "prove recovery never silently replays an operation whose
  outcome is ambiguous".

The design deviates from no architecture section.

## 3. Acceptance criteria (binary, each becomes a test; §7 maps them)

1. **WV-012 only without a write.** The renderer call arrives while the router
   has no admitted generation (`current` is `None`). This covers both app
   startup, before the first generation is admitted, and the gap after a crash.
   The host returns
   `KELD-WV-012`. `write_frame` is never invoked for that call, and the handler
   census for that call stays 0 in every generation.
2. **Failed or partial write is Unknown.** `write_frame` fails after k bytes,
   for any k including 0. The host returns `KELD-WV-013`, never `KELD-WV-012`.
   It clears the slot, retires the generation and writes no further byte to that
   stream.
3. **After the dispatch point is Unknown.** The CALL write completed, and then
   the link ends, the reader fails, the generation is revoked, or a lifecycle
   write fails. The host returns `KELD-WV-013` in every case, whether or not the
   handler ran.
4. **Navigation while pending is Unknown.** The page facade has a pending
   invoke when `pagehide` fires. The promise rejects with `KELD-WV-013`.
5. **No handler is API-002.** The Echo CALL reaches `@keld/api` while no
   handler is registered. Bun answers one ERR `KELD-API-002` and runs no app
   code. A handler that throws, a decode failure, an encode failure and an
   oversize reply answer `KELD-API-001`, never `KELD-API-002`.
6. **WV-011 is narrowed and still pre-write.** Every remaining `KELD-WV-011`
   emitter returns before `begin_echo_call` invokes `write_frame`. These are:
   - page facade validation;
   - the isolated-world busy check;
   - native admission (S3b);
   - the undeclared channel check;
   - quiescing;
   - lock poisoning;
   - the session-wide busy slot.

   Hostile envelopes rejected at S3a get no terminal and are never dispatched.
7. **Exactly one terminal, no replay.** Each facade-formed invoke whose
   document is alive when the outcome is decided gets exactly one terminal.
   A hostile envelope that skips the facade can get none (§4.2, S3a). A later
   reply or retirement for the same call is discarded. No Keld layer re-sends a
   CALL frame.
   - The guarantee covers every navigation that can occur today: a
     page-initiated main-frame navigation, which resets the bridge only when it
     commits (§4.7). In that case the old document settles once with
     `KELD-WV-013` on `pagehide`, and the successor never settles the old
     call.
   - A host-initiated navigation while the app runs does not exist today
     (§4.7). When #459 adds one, it MUST keep this criterion: a reset whose
     load never commits settles the live document's call once with
     `KELD-WV-013`.
8. **One resend after WV-012 or API-002 runs once.** The page resends once,
   as a new invoke, and the handler census for the logical action is exactly 1.
   The trigger depends on the code:
   - after WV-012, at startup or after a restart: until restart readiness
     exists ([#1098](https://github.com/gyldlab/keld/issues/1098)), the trigger is a later user action or a reliable
     app signal. A generation becoming ready is not, by itself, an app-visible
     trigger;
   - after API-002, the trigger is the handler being registered, which may
     happen in the same generation (late registration).
9. **Bun outbound codes are unchanged and documented as Unknown.**
   - `WorkerLink.call` returns `KELD-IPC-022` both when the link had already
     ended and when the call was still pending at close. Neither case claims
     "not sent".
   - The receiver-side refusals `KELD-IPC-024` and `KELD-IPC-005` (capacity)
     keep "not executed".
10. **Existing error shape is kept.**
    - A page rejection is still an `Error`. Its message keeps today's bytes
      whenever the detail already starts with `"<code>: "`. That covers every
      message Keld itself emits: facade refusals, host refusals and
      retirements, the fixed `KELD-API-001` text, and `@keld/kipc`
      `codedError` text.
    - A Bun-supplied detail that does not start with `"<code>: "` gains that
      prefix under the single rule in §4.9. This is the only byte change, and
      it is deliberate: it is what makes criterion 13 hold.
    - The only other addition is the `code` property of criterion 13.
    - A Bun ERR still carries the code in `CallError.code`.
    - The wv-link result still carries it in `code`. The only addition is the
      private `document` field of criterion 12, which never reaches the page.
11. **The host never trusts a Bun-supplied `KELD-WV-*` code.** Bun answers an
    Echo CALL with an ERR whose `CallError.code` starts with `KELD-WV-`. The
    host forwards it to the renderer as `KELD-WV-013`.
    - A Bun ERR with any other code (`KELD-API-*`, `KELD-IPC-*`) is forwarded
      unchanged.
    - Only the host can mint `KELD-WV-011` and `KELD-WV-012` for a renderer
      call.
12. **A result settles only the document and request that sent it.**
    - Each native→isolated result carries the document nonce of the request it
      answers. The nonce stays inside keld-wv end to end (§4.7). The structs
      that cross into keld-core (`RendererBridgeRequest`,
      `RendererBridgeOutcome`) carry no nonce.
    - The isolated world settles a result only if both of these hold:
      `document` equals its current nonce, and `request` equals its in-flight
      request id. It drops any other result without changing `inflight`.
    - A stale invoke from a predecessor document therefore never settles the
      successor's pending call.
13. **The page can read the code without parsing the message.** Every
    `window.keld.invoke` rejection is an `Error` with an own, enumerable,
    string `code` property. Its `message` always starts with
    `code + ": "` under the §4.9 prefix rule.

## 4. Design

### 4.1 First principles, reuse and rejected alternatives

Facts (each one is an atom from the GH-1095 claim, verified at `516644b0`):

- **F1. The dispatch point.** The renderer CALL's dispatch point is the
  instant `write_frame` returns `Ok` inside `begin_echo_call`
  (`crates/keld-core/src/app_session.rs:5853`).
  - `write_frame` writes the header with `write_all`, then the payload with
    `write_all`, then flushes (`crates/keld-ipc/src/link.rs:835`).
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
    (`packages/@keld/kipc/src/transport.ts:664-665`).
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
  (`packages/@keld/api/src/channels.ts:95-97`). `resolveEchoCall` then maps
  every throw to one `KELD-API-001` ERR (`packages/@keld/api/src/echo-call.ts:30-37`).
- **F6. The link opens lazily.** It opens only on the first `app.whenReady()`,
  `app.quit()`, `fs` or `auth` call. `ensureLink` is at
  `packages/@keld/api/src/app.ts:74-97`, and its only callers are at `108`
  (quit), `122` (whenReady), `164` (fs) and `171` (auth). `channels.handle` is synchronous and never touches the
  link. A handler registered before the first `whenReady()` therefore exists
  before any CALL can arrive.
- **F7. The page facade cannot see dispatch.**
  - On `pagehide` it rejects its pending call
    (`crates/keld-wv/src/wkwebview/macos_bridge.rs:63-68`).
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
- **Reclassifying `KELD-IPC-022` for the `#throwIfTerminal` case.** §4.5.

Compatibility fallback: none required. The codes are additive. §4.3 states what
still returns the old codes, and the safe default (§4.4: an unrecognized code
means Unknown) keeps old callers correct.

### 4.2 Failure stages and codes (renderer → host → Bun, macOS)

The stages run in request order. "DP" is the dispatch point (F1).

| # | Stage | Owner / site at `516644b0` | Today | After this revision |
|---|---|---|---|---|
| S1 | Page facade validation: `opts`, channel, payload type, size > 4096, page `pending` | `macos_bridge.rs:70-79` | WV-011 | **WV-011** (unchanged) |
| S2 | Isolated-world busy (`queued` / `inflight`) | `macos_bridge.rs:157-166` | WV-011 | **WV-011** |
| S3a | Native: wrong content controller/world, subframe, no or wrong WebView, envelope decode failure (only a hostile envelope that skips the facade reaches these) | `macos_bridge.rs:699-745` (`report_detail_rejection`, then `return`) | no terminal | unchanged: **no terminal**, zero dispatch |
| S3b | Native admission: stale or forged nonce, busy; host dispatch queue unavailable | `admit_invoke` `macos_bridge.rs:393-431`; `:776-795`; `reply_immediate_error` `:801-822` | WV-011 | **WV-011** |
| S4 | Host dispatch: undeclared channel | `app_session.rs:4213-4226` | WV-011 | **WV-011** |
| S5 | `begin_echo_call`: session quiescing | `app_session.rs:5813-5819` | WV-011 "unavailable or already pending" | **WV-011**, detail "application is quitting" |
| S6 | `begin_echo_call`: a lock is poisoned | `:5820-5823`, `:5831-5834` | WV-011 (same text) | **WV-011**, detail "host call state is unavailable" |
| S7 | `begin_echo_call`: session-wide slot busy | `:5835-5840` | WV-011 (same text) | **WV-011**, detail "another application call is already pending" |
| S8 | `begin_echo_call`: no admitted generation (`current` is `None`) | `:5824-5829` | WV-011 (same text) | **WV-012** |
| S9 | `write_frame` returns `Err` (any byte count, k ≥ 0) | `:5853-5876` | WV-011 (same text) | **WV-013** |
| DP | `write_frame` returns `Ok` | `:5878` | (none) | (none) |
| S10 | After DP: link EOF/reset, reader error, `Revoke` retirement, failed lifecycle or filesystem write, accepted Quit, profile handover or lease loss, router shutdown, reply channel disconnected | every retirement that answers the pending slot, e.g. `:6343`, `:6436`, `:6558`, `:6719`, `:7002`, `:7044`, `:7396`, `:7718`; all mapped generically at `:4278-4285` | WV-011 "application link ended before the renderer reply" | **WV-013** |
| S10b | After DP: Bun answers ERR with a `KELD-WV-*` code | forwarded at `:4272-4278` | that code, verbatim | **WV-013** (host rewrite, criterion 11) |
| S11 | Bun: no handler registered at dispatch time (never registered, unsubscribed, or registered after this call arrived) | `channels.ts:95-97` | API-001 | **API-002** |
| S12 | Bun: `decodeRequest` throws | `channels.ts:99` | API-001 | **API-001** (Q2, verified in §10) |
| S13 | Bun: handler throws or rejects | `channels.ts:100` | API-001 | **API-001** |
| S14 | Bun: `encodeResponse` throws, or reply > `MAX_FRAME_LEN` | `channels.ts:101`, `echo-call.ts:26-28` | API-001 | **API-001** (the effect is committed) |
| S15 | Reply in flight when Bun dies | host reader vs `Revoke` race | reply or WV-011 | reply or **WV-013**, never WV-012 |
| S16 | Page `pagehide` while an invoke is pending | `macos_bridge.rs:63-68` | WV-011 "renderer document navigated before reply" | **WV-013** |
| S17 | Bun alive but never answers | dispatch thread waits with no deadline (`:4253-4266`) | no terminal | unchanged (§4.7) |

How each crash point maps to a code:

| When it happens | Code |
|---|---|
| Crash before dispatch (generation already revoked) | S8 → WV-012 |
| Crash during the write | S9 → WV-013 |
| Crash after the write | S10 → WV-013 |
| Connection loss | S9 or S10 → WV-013 |
| Restart window | S8 → WV-012 |
| Navigation | S3b (stale nonce, before dispatch) → WV-011; S16 → WV-013 |
| Timeouts | none on the renderer path; S17 is out of scope |

The S5–S9 split requires one implementation change. `begin_echo_call` must
return a typed refusal that says which stage it failed at, and the dispatch
thread must map that stage to a code. It MUST NOT infer the stage from a
`HostAppError` detail string. Only the S8 variant may become WV-012.

Promotion rule for S9 (owner decision Q4, §10). Failed and partial writes stay
WV-013. A failure may move to WV-012 only when all six of these preconditions
hold:

1. deterministic partial-write fault injection;
2. byte-boundary coverage (at least k ∈ {0, 1, HEADER_LEN−1, HEADER_LEN,
   HEADER_LEN+len−1});
3. a complete analysis of who owns and writes to the stream;
4. proof that a failed write can never later become a complete frame;
5. independent negative controls;
6. a separately reviewed spec revision.

Until all six hold, S9 is WV-013.

Trust rule for codes from Bun. Every Bun ERR arrives after DP, so it can never
prove "not sent".

- The host rewrites any `KELD-WV-*` code from Bun to `KELD-WV-013` (S10b). This
  keeps WV-011 and WV-012 host-minted only.
- `KELD-API-002` is passed through. It is asserted by the app's own Bun process:
  Keld guarantees only `@keld/api`'s emission site (S11). App code that builds
  its own ERR frames can claim API-002 with the same trust as its handler. No
  host-side proof is possible, and none is claimed.

### 4.3 Backward compatibility

- **Still `KELD-WV-011`:** S1, S2, S3b and S4–S7. Every one of these returns
  before the host attempts a write. The registry meaning "rejected before
  application dispatch" becomes true again of every outcome the renderer can
  see.
  - `WvError::RendererBridge` (Rust side) also uses `KELD-WV-011` for
    bridge-install and reply-encode failures (`macos_bridge.rs:511-618`, `:843`).
    Those are construction or host faults, not call outcomes, and this
    revision leaves them unchanged.
- **No longer `KELD-WV-011`:**
  - S8 moves to WV-012;
  - S9, S10 and S16 move to WV-013.
- **Still `KELD-API-001`:**
  - S12–S14;
  - every registration-time error that `channels.handle` throws synchronously
    to app code: reserved lifecycle channel, undeclared descriptor, a handler
    that is not a function, and a duplicate handler. These errors are not call
    outcomes.
- **No longer `KELD-API-001`:** S11 moves to API-002.
- **Shape is unchanged** (§3 criterion 10). Detail strings are diagnostic, not
  part of the contract; only the code prefix is. No rule in this spec, the
  KEL-142 note or the registry branches on a detail string.
- **Effect on existing callers:**
  - A caller that treats every rejection as "handle the error" sees no
    difference.
  - A caller that matched WV-011 as "safe to resend" was wrong for S9, S10 and
    S16. After this revision, those cases carry WV-013, which such a caller does
    not match, so it stops resending them. The change makes callers safer, not
    less safe.
  - `@keld/electron` has no `ipcRenderer`/`ipcMain` today, so no Electron
    caller exists.
- **No dual emission and no adapter.** Each call gets exactly one code.
- **Migration unit.** Each implementing task updates, in the same PR, the
  in-repo assertions on the old strings:
  - `crates/keld-host/tests/no_flag/macos/public_fs.rs:755-768`;
  - `crates/keld-host/tests/no_flag/macos/public_fs/outcomes.rs:88`;
  - `crates/keld-core/src/app_session/tests/native_renderer_main_entry.rs:162-167`;
  - `packages/@keld/api/src/echo-call.test.ts`;
  - `packages/@keld/api/src/channels.test.ts`;
  - the `macos_bridge.rs` unit tests.

  The `KELD-WV-011` registry `fix` and the `WvError::RendererBridge` `Display`
  text stop recommending a document reload. The `Display` pin at
  `crates/keld-wv/src/error.rs:196` changes with it.
- **Interim mismatch (blocker owned by T3).** T0 already removes the reload
  advice from the registry `fix`. `error.rs:109-110` still says "Reload the
  current main-frame document" until T3 lands, because changing it is
  production code.

### 4.4 Retry rules (normative for apps; Keld itself never resends)

**Default:** a code the app does not recognize means Outcome Unknown.

| Code | Rule |
|---|---|
| WV-011 | **Never resend automatically**, whatever the detail. A new invoke made from a later user action is a new request, not a resend. The detail helps a developer diagnose the refusal; code MUST NOT branch on it. Until the §4.7 document-scoped result lands (T3), WV-011 proves "never sent" only for the document that issued the call (F10). |
| WV-012 | **One resend** after the first (startup) or successor Bun generation is ready (see the rules below). |
| WV-013 | **Never resend automatically**, unless it is made safe in one of the two ways below. |
| API-002 | **No app code ran.** Fix the registration order so the error cannot recur, then the page MAY resend once. |
| API-001 | Same as WV-013: the call may have run. |

WV-012 resend rules:

- The resend is a new `invoke`, with a new local request id and a new host
  correlation. It is never a replay of the old frame.
- WV-012 ships independently of the restart notification ([#1098](https://github.com/gyldlab/keld/issues/1098);
  owner decision Q1, §10). Keld emits it only when non-dispatch is proven (S8).
- No blind retries, no timer or backoff retry loops, and no Keld replay.
- Until restart readiness exists ([#1098](https://github.com/gyldlab/keld/issues/1098)), the one resend is allowed
  only after a later user action or a reliable app signal. This applies at startup as well as
  after a restart.
- Once it ships, the trusted restart notification ([#1098](https://github.com/gyldlab/keld/issues/1098)) becomes the
  restart trigger.
- Keld does not claim automatic recovery: a WV-012 call is recovered only by
  the app's own resend.
- A second WV-012, or an API-002, on the resend is surfaced to the user and is
  not resent again. The limit is at most one automatic resend per logical
  action.

WV-013 can be made safe to resend in one of two ways:

- **(a) An app-supplied idempotency key.** The page puts the key (for example
  `crypto.randomUUID()` per user action) in its own payload and keeps it across
  the resend. The Bun handler records `(key, result)` in the same durable
  transaction as the effect, returns the stored result for a repeated key, and
  passes the key on to external APIs.
- **(b) An app-defined status check.** The app's own handler reads that durable
  record and confirms the effect did not happen.

An in-memory map dies with the crashed generation and makes neither safe. Keld
ships no key store, dedup helper or status API (§4.1). Keld never reads, stores
or forwards the key.

API-002 guidance: register every `channels.handle` handler synchronously at
module top level, **before** the first `app.whenReady()` and before any `fs` or
`auth` call. F6 proves this ordering closes the window.

```ts
import { app, channels, echoChannel } from "@keld/api";

channels.handle(echoChannel, handleEcho); // first: no link exists yet
await app.whenReady();                    // then: opens the link
```

A handler registered inside `whenReady().then(...)` leaves a window after every
start and restart in which calls get API-002.

### 4.5 Bun-side outbound calls (role → host)

No new code is reserved or emitted; this section only records each code's class.

| Code | Class | Note |
|---|---|---|
| `KELD-IPC-004`, `KELD-IPC-005` local (deadline, frame bound, blocking call in flight) | Not sent, deterministic | thrown before any post |
| `KELD-IPC-022`, `025`, `026`, `027` | **Unknown** | the same code is thrown before posting (`#throwIfTerminal`) and for a call pending at close (F8); callers MUST NOT infer "not sent" from it |
| `KELD-IPC-023` | Unknown | already registered so |
| `KELD-IPC-006` | Unknown | the host may still run the call |
| `KELD-IPC-024` | Not executed | receiver-side: the host ran nothing behind an accepted Quit |
| `KELD-IPC-005` capacity | Not executed | receiver-side admission refusal |

A Bun crash kills the Bun caller, so splitting `KELD-IPC-022` would help only
with Worker death and host retirement while Bun is alive. That is not a Phase 2
need (YAGNI).

GH-527 said that a call still pending at a Quit-close "is treated as
`KELD-IPC-024` (neither ran)". That is unproven for a call the role wrote before
its `Quit`. Owner decision Q3 (§10) applies:

- T0 corrects the three GH-527 passages that make this claim. A role-observed
  `KELD-IPC-022` at a Quit-close is **Unknown** unless AT-B3 proves otherwise.
- The independently proven non-execution case is kept: `KELD-IPC-024`. The
  host writes it for a CALL it read behind the accepted `Quit` and ran none of
  it. This is a host-side proof, tested by `guarded_router_and_quit.rs`.
- The GH-527 falsifier "a caller or contract that must tell `KELD-IPC-024`
  apart from `KELD-IPC-022`" is now met, and T0 records it as met in GH-527.
  T0 does not change the close-on-REPLY design. Its cost is now explicit: a
  call the host answered `KELD-IPC-024` is observed by the role as
  `KELD-IPC-022`, which is Unknown. Whether to deliver that `024` is a decision
  for GH-527's owner.
- GH-531's O11 row ("the facade treats the two alike") gets a matching interim
  note.
- The matching comment at `transport.ts:2571-2572` is production source, so
  T0 leaves it unchanged. Correcting it is a recorded T4 follow-up.

### 4.6 Platform notes

- **macOS:** the only renderer bridge (`#[cfg(target_os = "macos")]` on
  `begin_echo_call`, `pending_echo` and `start_renderer_dispatch`). Every
  renderer criterion is macOS-only.
- **Windows and Linux:** no renderer bridge. The `@keld/api` (S11–S14) and
  `@keld/kipc` (§4.5) rules apply on all three OSes.
- **A future Windows named-pipe Echo path:** it MUST re-prove F1 and F3
  (overlapped `WriteFile` can complete despite a cancel) before it emits WV-012.

### 4.7 Exactly-once settlement and no replay

- Three layers hold at most one pending entry and empty it **before** they
  deliver a terminal. A second terminal finds the slot empty and is dropped.
  The three layers are:
  - page facade `pending`, matched on request id (`macos_bridge.rs:49-53`);
  - native `BridgeState.pending`, matched on `(navigation, request)`
    (`:425-436`);
  - host `pending_echo`, matched on `(attempt, correlation)`.
- **Today**, the isolated world's `inflight` flag is not a settlement guard.
  `__keldBridgeNativeResult` (`:137-147`) clears it and relays every result
  without checking the request id. Duplicates within one document are dropped
  by the native layer and the facade, but a result can cross documents (F10).
- **Settlement fix (T3), document-scoped results:**
  - The isolated world records the in-flight request id, not a boolean.
  - Every native→isolated result carries `document`. The field is a new
    `document: Option<String>` on the private `PageOutcome` struct
    (`macos_bridge.rs:653`), serialized into the JSON passed to
    `__keldBridgeNativeResult`. It is filled in one of three ways:
    - **Immediate refusals** (`reply_immediate_error`, `:801-822`) copy it from
      the `document` field of the envelope being refused. An S3b stale nonce
      therefore carries the old nonce, and the successor drops it.
    - **Dispatch outcomes.** `PendingRequest` (keld-wv `BridgeState`) gains
      `document: String`, set by `admit_invoke` (`:393-431`) to the nonce the
      request was admitted under. `BridgeState::settle` (`:425`) returns that
      nonce (`Option<String>`, not `bool`) when it removes the matching
      `(navigation, request)` entry. `deliver` (`:633`) writes it into
      `PageOutcome.document`.
    - The host round trip is unchanged. `RendererBridgeRequest` and
      `RendererBridgeOutcome` still identify the call by `(webview,
      navigation, request)`, and keld-core never sees the nonce.
  - `__keldBridgeNativeResult` settles only when `document` equals the current
    `documentNonce` and `request` equals the in-flight id. Otherwise it drops
    the result and leaves `inflight` unchanged.
  - The relay to the page world never includes `document`.
  - The nonce already lives in the isolated world, so echoing it back exposes
    nothing new. The page world cannot read it, which keeps KEL-142 §5.1.
- **The resulting gap:** a stale invoke whose document is gone gets no
  terminal. That is correct, because its own `pagehide` already settled it
  with WV-013 (S16).
- **Navigation and bridge reset (r2).** A navigation can reach the bridge
  in two ways:
  - **Page-initiated main-frame navigation.** This is the only kind that can
    occur while the app runs.
    - In wry 0.56.1, `Started` fires only when a navigation commits (wry
      `src/wkwebview/navigation.rs:17-25`, `did_commit_navigation`). So the
      bridge resets only on commit, and a navigation the page cancels never
      resets it.
    - On commit, the old document has already rejected its pending call with
      WV-013 on `pagehide` (S16).
    - Any later result for that call is dropped: either by native `settle`,
      because the entry was cleared, or by the successor's `document` check.
    - Exactly once holds.
    - Main has no navigation-policy handler. #460 owns that policy, and it MUST
      keep this property.
  - **Host-initiated navigation (`WebEngine::navigate`).** It resets the bridge
    eagerly, before `load_html`/`load_url` (`wkwebview/mod.rs:687-690`). But
    it is **unreachable while the app runs**:
    - `run_app_until_quit` holds `&mut self` and moves the views and bridges
      into the event loop (`wkwebview/mod.rs:302`, `:391`);
    - `AppWindowCommand` has no navigate command.

    A reset without a commit therefore cannot occur today, and nothing in this
    spec requires a delivery for it.
  - **Future owner:** #459 (the webContents facade with host `loadURL`). It
    adds the first running-app host navigation. When it does, it MUST settle a
    call cleared by a reset whose load never commits: exactly once,
    `KELD-WV-013`, to the original document, scoped by `document`. It MUST
    also add AT-S arm (d) with that oracle and a committing-load control. It
    MAY reuse a reset-delivery helper, or it MAY reset only on commit; either
    way it MUST NOT leave a live document's call pending.
  - **Code that cannot be reached today.** A reset-delivery path in keld-wv
    cannot be exercised end to end until #459 exists. T3 MUST NOT claim
    end-to-end coverage for it. It may keep the path only behind a unit test
    that calls `navigation_started` directly, and it must say so.
  - `destroy()` needs nothing, because the document dies with the view.
- The dispatch thread sends exactly one outcome per renderer request.
- A reply that arrives after a retirement is refused by the correlation and
  attempt check, and the reader that would deliver it is already inert.
- Each stage's code is assigned at exactly one site.
- **No replay:**
  - no layer keeps the payload after the write;
  - no layer re-sends a CALL frame;
  - a successor generation never receives a predecessor's call;
  - Keld's only re-delivery is the app's own new invoke under §4.4.
- **Exception (existing behaviour, kept):** at session shutdown, the dispatch
  thread can stop without an outcome while the document is being destroyed.
  "Exactly once" therefore applies to documents still alive when the outcome is
  decided.
- **Hung Bun:** a call to a live Bun that never answers has no terminal (S17).
  This is a liveness defect, outside this classification spec (Q5, §10; #1097).

### 4.8 Remaining design items

- **New or changed types:**
  - a private `begin_echo_call` refusal enum in keld-core, with variants
    `Quitting`, `StateUnavailable`, `Busy`, `NoGeneration` and
    `WriteFailed(HostAppError)`;
  - in keld-wv (all private):
    - `PendingRequest.document: String`;
    - `PageOutcome.document: Option<String>`;
    - `BridgeState::settle` returns `Option<String>`;
    - `BridgeState::navigation_started` returns `Option<PendingRequest>`;
  - no public Rust type changes. `RendererBridgeRequest` and
    `RendererBridgeOutcome` are unchanged.
- **TypeScript:** `dispatchApplicationCall` throws a typed no-handler error,
  and `resolveEchoCall` maps it to `KELD-API-002`. Every other throw still maps
  to `KELD-API-001`.
- **Capabilities and manifest:** none.
- **Wire and protocol:**
  - KIPC: no frame, field or version change. New *values* travel in existing
    string fields, and the host now rewrites one value class (S10b).
  - `keld.wv-link/v1`: the private native→isolated result gains one field,
    `document` (§4.7). The isolated→native envelope is unchanged. See §8.
- **Runtime seam:** none. The execution owner, crash domain and ordering are
  unchanged.
- **Persisted state:** none.

### 4.9 Structured error representation (audit and the scoped fix)

The question for each surface: can an app read a machine-readable `error.code`
without parsing the message string? The audit is at `516644b0`.

| Surface | Code readable without parsing? | Evidence |
|---|---|---|
| WebView JS API (`window.keld.invoke` rejection) | **No.** The page gets an `Error` whose `message` is `"<code>: <detail>"` and that has no `code` property | `fail` `macos_bridge.rs:46`, result rejection `:57-59`, `pagehide` `:67` |
| Page-world relay (`result-v1` `postMessage`, internal) | Yes, but internal: `code` and `detail` fields, observable by page scripts, not a contract | `__keldBridgeNativeResult` `:137-147`; isolated-world busy result `:159-165` |
| Native renderer bridge | Yes, inside Rust: `PageOutcome.code: Option<String>`; `RendererBridgeOutcome::Error.code: String` | `macos_bridge.rs:653-661`, `:234-259` |
| Host | Yes, inside Rust: renderer outcomes carry `code: String`; `HostAppError::code()` | `app_session.rs:4219`, `4235`, `4276`, `4283`; `app_session/error.rs:88` |
| IPC `CallError` (wire) | **Yes:** `code: String` beside `message` | `crates/keld-ipc/src/call_error.rs:59-65` |
| `@keld/kipc` | **Mixed.** ERR replies and link-terminal codes reject with an own enumerable `code` (`codedError`, `linkError`; `isCallError`). Local validation and decode failures use `kipcError`, a plain `Error` with the code only in `message`, so `isCallError` is false for them | `transport.ts:1254-1262`, `1388-1390`, `1118-1135` (`errorFromErrFrame`), `1067-1074`; `kipcError` `227-229`, used at `1052`, `1059`. A malformed ERR, empty or not a `CallError`, also becomes a plain `kipcError` with no `code` (`errorFromErrFrame`, `:1126`, `:1132`). On the `WorkerLink` path the same malformation ends the link instead (`:2311`) |
| `@keld/api` | **Mixed.** Host ERRs (fs, auth) reach the app as `KeldCallError` with `code`, re-exported with `isCallError`. Local failures are plain `Error`s with the code in `message`: an unset link (`KELD-IPC-007`), `apiError` for `channels.handle` registration, and the fs response-variant mismatch | `index.ts` exports; `app.ts:76-82`; `channels.ts:28-30`; `fs.ts:16`, `26` |
| Rust error types | **Mixed.** `code()` exists on `HostAppError`, the keld-guard refusal types and `keld-ipc` admission. `WvError` and `IpcError` expose the code only through `Display` | `app_session/error.rs:88`; `keld-guard/src/lib.rs:347`, `507`; `keld-ipc/src/admission.rs:63`; `keld-wv/src/error.rs:55-110`; `keld-ipc/src/lib.rs:114-138` |
| CLI/MCP `KeldErrorObject` | **Yes:** `code`, `message`, `cause`, `fix`, `docs` fields | `crates/keld-cli/src/error_object.rs:11-22` |

**Finding.** The surface this spec revises, the `window.keld.invoke`
rejection, is the one where an app must parse `message` to tell
`KELD-WV-012` (resend allowed) from `KELD-WV-013` (resend forbidden). That
parsing is exactly the decision §4.4 depends on, so a public improvement is
justified here.

**Design: an additive `code` property (T3, keld-wv only).**

- **Prefix rule (r2, one rule, one owner).** For a rejection with code `c`
  and detail `d`:

  ```text
  message = d               if d starts with c + ": "
          = c + ": " + d    otherwise
  ```

  - The rule is idempotent. It is applied **once**, by the page facade's
    coded-error helper, which is the rule's single owner.
  - The host forwards a Bun ERR's `CallError.message` as the detail
    **unchanged**. It MUST NOT prefix it, so the prefix logic at
    `app_session.rs:5789` on #1105 is removed.
  - The host's own refusals and retirements send a bare detail, and the
    facade prefixes it.
  - Today the facade's test is `detail.startsWith(code)`. r2 tightens it to
    `startsWith(code + ": ")`. Every message Keld emits already has that form,
    so their bytes are unchanged (criterion 10).
  - A Bun-supplied detail like `"KELD-API-001 failed"` (code but no `": "`)
    becomes `"KELD-API-001: KELD-API-001 failed"`. That is the only byte
    change.
- The page facade gets one coded-error helper. It is the only place in
  `PAGE_FACADE_SCRIPT` that constructs an `Error`. It builds the `message` by
  the prefix rule:
  - `fail(detail)` gives `"KELD-WV-011: " + detail`, unchanged;
  - a result goes through the rule;
  - `pagehide` gives its existing text under its §4.2 code.
- The helper then defines an own `code` property with the same attributes
  `@keld/kipc` `codedError` uses (`enumerable`, `writable` and `configurable`
  all true). A page rejection and a Bun `KeldCallError` therefore share one
  shape, `Error & { code: string }`.
- The value is the result's `code`, or `KELD-WV-011` for local facade
  refusals. That is the same code as the message prefix.
- No `detail` property is added, because the detail is not contract (§4.3).
- **Spec-level type.** The rejection type below is recorded for the future
  generated web client. No TypeScript declaration ships today; no package
  declares `window.keld`.

  ```ts
  interface KeldInvokeError extends Error { readonly code: string }
  ```
- **Backward compatibility.**
  - `message`, `name`, `instanceof Error` and `String(err)` are unchanged.
  - `code` is enumerable, so `JSON.stringify(err)`, `Object.keys(err)` and
    object spread (`{ ...err }`) now include it. This is intended: `code` is a
    stable public field, enumerable exactly like `@keld/kipc` `codedError`'s.
    These are the only visible differences.
- **Scope.**
  - The property is set at the facade's single constructor, so it covers every
    invoke rejection.
  - The values this spec defines are `KELD-WV-011/012/013` and
    `KELD-API-001/002`.
  - Any other code passed through from Bun (after the S10b rewrite) appears
    verbatim, and its meaning belongs to its owning spec.
- **Tests:** AT-EC (§7).

**Hand-off: the wider migration (out of scope for #1096).** The rest of this
belongs to the separate contract-architecture program. Its issue is being
created by another worker; no number exists yet. It covers:

- giving `kipcError` local failures and `@keld/api` local failures a `code`
  property, so `isCallError` is true for them;
- adding `code()` to `WvError` and `IpcError`;
- one error-object shape across JS and Rust, aligned with `KeldErrorObject`'s
  `fix` and `docs`;
- a diagnostic detail for `KELD-API-001` causes (Q2);
- a generated `window.keld` TypeScript declaration.

## 5. Boundaries

- **Implement in:**
  - `crates/keld-core/src/app_session.rs` (`begin_echo_call` refusal type,
    renderer dispatch mapping) and its tests;
  - `crates/keld-wv/src/wkwebview/macos_bridge.rs` (`pagehide` code,
    document-scoped result) and `crates/keld-wv/src/error.rs` (`fix` text);
  - `packages/@keld/api/src/{channels,echo-call}.ts` and their tests;
  - `docs/engineering/keld-error-codes.md`;
  - the KEL-142 spec §7;
  - the host macOS acceptance tests named in §7.
- **Must not touch:**
  - KIPC frame layout, `CallError` schema, the isolated→native
    `keld.wv-link/v1` envelope;
  - `@keld/kipc` transport behaviour;
  - guard, permissions, workspace `Cargo.toml`;
  - navigation policy;
  - restart-notification code ([#1098](https://github.com/gyldlab/keld/issues/1098)).

## 6. Tasks (each ≈ one PR; ordered)

- [ ] T0: docs only. It includes:
  - this spec;
  - the KEL-142 §7 current-behaviour correction;
  - the interim `KELD-WV-011` registry text;
  - the GH-527 Q3 correction, and the matching GH-531 O11 interim note.
- [ ] T1: test-only characterisation. It pins today's codes at S8–S16 and
  S10b with negative controls, writing the AT-12a/12d/13a/13b/13c arms against
  today's strings. It also pins the cross-document settlement today (AT-S arm
  c). Arm (d) is deferred to #459 (r2).
- [ ] T2: keld-core. It includes:
  - the typed refusal;
  - the S8 → WV-012, S9/S10 → WV-013 and S10b rewrite;
  - the WV-012/WV-013 registry headings;
  - T1's assertions flipped to the target codes.
- [ ] T3: keld-wv. It includes:
  - `pagehide` → WV-013;
  - the document-scoped result (§4.7), which flips T1's AT-S arm c. The
    uncommitted host-navigation settlement belongs to #459 (r2);
  - the `WvError::RendererBridge` fix text;
  - the narrowed WV-011 registry text;
  - the `code` property on every invoke rejection (§4.9, AT-EC).
- [ ] T4: `@keld/api` and `@keld/kipc`. It includes:
  - S11 → API-002, its registry heading, and the unit and hosted API-002 tests;
  - the follow-up fix to the `transport.ts:2571-2572` comment, matching Q3 and
    the AT-B3 verdict;
  - AT-API2a's decode-failure arm (S12), which no current test covers.
- [ ] T5: developer guidance for §4.4: the idempotency pattern and registering
  before `whenReady`. The X12-style example app comes only when a consumer
  needs it.

T2–T4 are independent once T1 lands. WV-012 does not wait for the restart
notification (Q1, [#1098](https://github.com/gyldlab/keld/issues/1098)).

## 7. Test plan

Shared oracles:

- **Handler census:** every test handler POSTs `{gen, pid, nonce, phase}` to the
  test observer. The phases are `entered`, `effect-committed` and `returning`.
- **Host markers:** `RENDERER_ADMIT`, and `KELD_KEL142_KIPC_CALL` (DP).
- **Terminal count:** the number of page settlements per nonce.
- **"Census 0 forever":** re-checked after the successor generation serves three
  more calls.

Every hosted arm prints its raw census with `--success-output immediate`.

| ID | Criterion | Stage | Platform | N | Observable | Negative control / mutation |
|---|---|---|---|---|---|---|
| AT-12a | 1, 7 | S8 (G2 held before HELLO) | hosted macOS | 10 × arms {handler before `whenReady`, handler inside `whenReady`} | page code `KELD-WV-012`; `KIPC_CALL` = admits − 1; census 0 for G1 and G2 forever; 1 terminal | G2 released before the invoke → reply, census 1 (the oracle can count) |
| AT-12b | 1 | S8 | macOS unit (keld-core) | deterministic | WV-012 variant returned; the counting writer saw **0 write calls** | admitted generation → 1 write call and no WV-012; **own mutation:** move the `current is None` refusal after `write_frame` → the counting writer sees 1 call and AT-12b fails; cross-check: map `WriteFailed` to WV-012 → AT-13a fails |
| AT-12d | 1, 8 | S8 at startup (G1 held before HELLO, page already loaded) | hosted macOS | 10 | page invokes before the first admission → `KELD-WV-012`, census 0, `KIPC_CALL` = admits − 1. The test then releases G1 and waits for the host Ready marker; this is a test precondition only, not a page-visible trigger. Then it drives a **user action**: an OS-visible pointer click on the fixture's resend button, as in KEL-142 AC3. The page resends once → reply, census 1 | (i) G1 released before the first invoke → reply, census 1, so the startup gap is load-bearing; (ii) the same click driven while G1 is still held → WV-012 again, surfaced, and no second resend (at most one resend) |
| AT-12c | 8 | S8 then resend after a restart | hosted macOS | 10 | after a WV-012 in the restart gap, the test waits for G2's host Ready marker (test precondition only), then drives an OS-visible click on the resend button → one resend → reply; census for the logical action = 1 | the same click driven before G2 is admitted → WV-012 again, surfaced, and no second resend |
| AT-13a | 2 | S9, k ∈ {0, 1, HEADER_LEN−1, HEADER_LEN, HEADER_LEN+len−1} | macOS unit (keld-core; extends `failed_primary_writes_retire_the_generation`) | deterministic per k | WV-013; slot cleared; generation retired; 0 bytes written to the stream after the failure | k = full length → DP reached, no S9; characterization: an injected completing write after failure makes the Bun `FrameReader` dispatch (proves why S9 cannot be WV-012 yet) |
| AT-13b | 3, 7 | S10 | hosted macOS | 10 per arm | arms: (i) SIGSTOP Bun → invoke → SIGKILL: census 0; (ii) kill at `entered`: census 1; (iii) kill after `effect-committed`: effect present. Every arm: `KELD-WV-013`, 1 terminal, G2 census for the nonce 0 forever | arm (ii) census 1 under the same code proves WV-013 cannot mean "not sent"; mutation: S10 → WV-012 fails arm (ii) |
| AT-13c | 4 | S16 | hosted macOS | 10 | handler held at `entered`; page `location.reload()`; the `pagehide` rejection beacon reports `KELD-WV-013`; census 1; no settlement in the successor document | navigate while queued before bind → census 0 with the same WV-013 (the facade cannot tell) |
| AT-13d | 7 | S15 | hosted macOS | 50 | kill Bun on the `returning` POST; outcome ∈ {reply, WV-013}; never WV-012; 1 terminal; the success:Unknown ratio is reported, not asserted | **kill disabled**, same harness → 50/50 reply and 0 WV-013 (the Unknown outcomes come from the kill, not from the harness); mutation: S15 retirement → WV-012 fails |
| AT-API2a | 5 | S11–S14 | `bun test` in `@keld/api`, macOS + Linux + Windows CI | deterministic | missing → API-002, census 0; late (call, then register, then call) → API-002 then reply, census 1; unsubscribed → API-002; handler throws after effect → API-001, census 1; decode failure → API-001, census 0; oversize → API-001, census 1 | mutation: move the no-handler check after `handler(request)` → the census for the API-002 case becomes 1 and the test fails; a throwing handler never yields API-002 |
| AT-API2b | 5, 8 | S11 across restart | hosted macOS | 10 × arms {inside `whenReady`, before `whenReady`} | inside: at least one post-restart API-002 with G2 census 0, then one resend → census 1; before: 0 API-002 across all N | the inside arm reproduces the window (round-1 E8 hit it 10/10 under API-001) |
| AT-11 | 6 | S1, S2, S3b, S4–S7 (and S3a: no terminal) | macOS unit (keld-wv + keld-core) | deterministic | each reason → WV-011 and 0 write calls; each S3a reason → no terminal and no dispatch | mutation: emit WV-011 from S9 → fails |
| AT-B1 | 9 | `#throwIfTerminal` | `@keld/kipc` unit, 3 OSes | deterministic | link already ended → `call()` rejects IPC-022; host fixture read 0 frames | remove `#throwIfTerminal` → frame posted (the check is the proof, but the code stays Unknown) |
| AT-B2 | 9 | pending at close | `@keld/kipc` unit, 3 OSes | deterministic | pending, then host close → IPC-022; host read 1 frame; same code as AT-B1 | host writes the REPLY before closing → the call resolves with the reply, not IPC-022 (the close produces 022, not the harness) |
| AT-B3 | 9 | written before Quit, pending at close | keld-core unit | deterministic | host census for that CALL recorded; only a census of 0 in every arm could reverse Q3's Unknown | written after Quit → IPC-024, census 0 (existing `guarded_router_and_quit.rs`) |
| AT-S | 7, 12 | settlement | macOS unit (keld-wv, keld-core) + hosted macOS | deterministic (unit); 10 (hosted) | (a) late reply after retirement refused; (b) facade result after `pagehide` ignored, and native result for a stale `(navigation, request)` dropped; (c) a stale-document invoke refused after the successor binds, while the successor has a pending call with the same request id → the successor's call settles only from its own result; (d, r2) handler held at `entered`, then a **page-initiated** main-frame navigation that commits (`location.href` to a fixture page) → the original document settles exactly once with `KELD-WV-013` (from `pagehide`, counted by a beacon carrying the original document's nonce); census 1; the successor records 0 settlements; the later host outcome is dropped. The uncommitted host-`navigate()` arm moves to #459. T1 pins today's behaviour (c may cross); T3 flips c to the target | (a) remove the corr/attempt check → second settlement observed; (c) remove the `document` check in `__keldBridgeNativeResult` → the successor settles with the stale WV-011; (d) the same harness with no navigation → the call settles once with its real reply, census 1 (the WV-013 comes from the navigation); mutation: make the successor accept a result without the `document` check → the successor records a settlement and the arm fails |
| AT-SP | 11 | S10b | macOS unit (keld-core; fake Bun peer on `socketpair`) | deterministic | Bun ERR `code = "KELD-WV-012"` (and `"KELD-WV-011"`) → renderer outcome `KELD-WV-013`; the frame was written (`KIPC_CALL` emitted) | Bun ERR `code = "KELD-API-002"` → forwarded unchanged (the rewrite is scoped to `KELD-WV-*`); mutation: remove the rewrite → the page sees `KELD-WV-012` and the test fails |
| AT-EC | 10, 13 | every invoke rejection | macOS keld-wv unit + hosted macOS | deterministic (unit); 10 per code (hosted) | the hosted page reports `{code: err.code, message: err.message}` for WV-011 (`opts` given), WV-012 (AT-12a), WV-013 (AT-13b) and API-002 (AT-API2b): `err.code` is a string equal to the message prefix; `message` is byte-equal to today's format; `err instanceof Error`; `Object.keys(err)` includes `"code"` (intended, §4.9). Prefix rule (r2), keld-core and keld-wv units: the host forwards `CallError.message` unchanged; a Bun ERR message `KELD-API-001: x` → page message unchanged; `KELD-API-001 failed` → `KELD-API-001: KELD-API-001 failed`; a detail that starts with a *different* code is prefixed and never re-coded: a `KELD-WV-013` rejection whose detail is `KELD-API-001: x` → message `KELD-WV-013: KELD-API-001: x` with `err.code === "KELD-WV-013"`, and a Bun ERR `code = KELD-API-002` with message `KELD-API-001: x` → `KELD-API-002: KELD-API-001: x` with `err.code === "KELD-API-002"` (`code` always comes from the code field, never from the detail); a host refusal with a bare detail → prefixed once (the rule is idempotent). Unit: `PAGE_FACADE_SCRIPT` builds every rejection through its one coded-error helper (exactly one `new Error(` in the script) | the observer run against a plain `new Error("KELD-WV-011: x")` reports `code` undefined (the oracle detects absence); mutation: drop the `code` definition in the helper → every hosted arm fails |

Anti-flake:

- No sleeps. Holds and releases use the existing test-only observer gates.
- Kills are driven from census POSTs, not timers.
- Hosted arms use the PR #1082 fixtures, promoted into landing tests by T1.

## 8. Review gates triggered

| Gate | Applies | Reason |
|---|---|---|
| Public API | **Yes** | three new page- and app-visible codes; narrowed meanings for `KELD-WV-011` and `KELD-API-001`; normative retry guidance; an additive `code` property on `window.keld.invoke` rejections (§4.9); KEL-142 §7 revised |
| Wire protocol | **Yes** (values, plus one private field; no version bump) | KIPC: `KELD-API-002` is a new value crossing the Bun→host link in `CallError.code`, and the host now rewrites `KELD-WV-*` values arriving there (S10b). The frame and the `CallError` schema are unchanged, and receivers treat an unknown code as Unknown, so no version bump. `keld.wv-link/v1`: the native→isolated result gains `document` (§4.7). Both ends ship in one keld-wv build, and the isolated→native envelope and its decoder are unchanged, so no version bump; the reviewer confirms this |
| Permission model | No | no grant, capability or guard path |
| Dependency addition | No | none |
| Unsafe | No | no FFI change |

## 9. Perf impact

None. `begin_echo_call` returns an enum instead of an error built from strings,
on a cold refusal path. The hot reply path is unchanged. No budget in
architecture 01 §5 can move.

## 10. Owner decisions (2026-10-23)

The owner answered Q1–Q5 on 2026-10-23, and the coordinator relayed the
answers on PR #1096. These are directions, not approval of the exact content.
This spec stays `Status: draft` until that approval.

1. **Q1, WV-012 independent of the restart notification ([#1098](https://github.com/gyldlab/keld/issues/1098)): Option A.**
   - Ship WV-012 without waiting for the notification, and emit it only when
     non-dispatch is proven (S8).
   - No blind retries, no timer retry loops, no Keld replay.
   - Until restart readiness exists ([#1098](https://github.com/gyldlab/keld/issues/1098)), one resend is allowed
     only after a later user action or a reliable app signal (§4.4).
   - The restart notification and restart readiness are tracked in [#1098](https://github.com/gyldlab/keld/issues/1098).
   - Keld does not claim automatic recovery.
2. **Q2, decode failure: provisional Option A, verified.** Decode failures
   stay on `KELD-API-001`. The evidence at `516644b0`:
   - **Emission.** `dispatchApplicationCall` runs `decodeRequest` at
     `channels.ts:99`. That is after the no-handler check (`:95-97`) and
     before the handler (`:100`), so the handler census is 0 by construction.
   - **What the decoders throw.** They throw plain errors:
     - `kipcError("KELD-IPC-003", …)` at `transport.ts:1052` and `1059`;
     - the truncated-varint error from `decodeVarint` (`:1039-1041`);
     - `apiError("trailing bytes…")` at `channels.ts:36`.
   - **What reaches the page.** `resolveEchoCall`'s bare `catch`
     (`echo-call.ts:30-37`) turns each of these into the fixed ERR
     `KELD-API-001: application channel call failed`. The link survives,
     because the call handler never rejects, so `#failLink` is not reached.
   - **Why a malformed request reaches Bun.** The host forwards the renderer
     bytes after checking only length and byte range, not the Echo schema
     (`macos_bridge.rs` S3b; `begin_echo_call`).
   - **Test coverage.** No current test covers the decode case.
     `echo-call.test.ts` covers missing, throwing and oversize; AT-API2a adds
     decode.
   - **Verdict.** The evidence does not contradict `KELD-API-001`. It is a
     safe superset: it forbids only a resend that would fail identically.
     - A separate "invalid request" code would give no correctness gain. The
       resend that "not executed" would permit sends the same bytes and fails
       the same way. The only valid recovery is changing the encoder, which is
       a code change, not a retry.
     - The developer-experience gain is real but small, and it does not need
       a new code: the lost cause (`KELD-IPC-003 …`) can be carried as a
       diagnostic. That is recorded in the §4.9 hand-off.
     - No code is added.
3. **Q3, the GH-527 `KELD-IPC-022` claim: Option A.**
   - Corrected to Unknown unless AT-B3 proves otherwise (§4.5).
   - `KELD-IPC-024` keeps its host-side non-execution proof.
   - The `transport.ts` comment is a T4 follow-up.
4. **Q4, failed and partial writes: Option A.** They stay WV-013. The six
   promotion preconditions are recorded in §4.2.
5. **Q5, hung Bun: Option A.** Tracked in its own issue,
   [#1097](https://github.com/gyldlab/keld/issues/1097) ("liveness: hung Bun
   leaves renderer requests pending and blocks other windows"). It covers:
   - missing reply deadlines;
   - permanently pending requests;
   - session-wide busy-slot blocking;
   - fairness and isolation between windows;
   - cancellation and recovery;
   - duplicate-execution prevention.

   It is not implemented here.

Cross-document settlement, formerly Open question 6, is in scope: see F10,
§4.7, criterion 12 and AT-S arm c.

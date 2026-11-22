# Design 2: Failure stages and codes (renderer → host → Bun, macOS)

Part of: [index](index.md)
Owns: Design 2: Failure stages and codes (renderer → host → Bun, macOS)

## 4. Design: Failure stages and codes (renderer → host → Bun, macOS)

The stages run in request order. "DP" is the dispatch point (F1).


- #: S1
- Stage: Page facade validation: `opts`, channel, payload type, size > 4096, page `pending`
- Owner / site at `516644b0`: `macos_bridge.rs:70-79`
- Today: WV-011
- After this revision: **WV-011** (unchanged)

- #: S2
- Stage: Isolated-world busy (`queued` / `inflight`)
- Owner / site at `516644b0`: `macos_bridge.rs:157-166`
- Today: WV-011
- After this revision: **WV-011**

- #: S3a
- Stage: Native: wrong content controller/world, subframe, no or wrong WebView, envelope decode failure (only a hostile envelope that skips the facade reaches these)
- Owner / site at `516644b0`: `macos_bridge.rs:699-745` (`report_detail_rejection`, then `return`)
- Today: no terminal
- After this revision: unchanged: **no terminal**, zero dispatch

- #: S3b
- Stage: Native admission: stale or forged nonce, busy; host dispatch queue unavailable
- Owner / site at `516644b0`: `admit_invoke` `macos_bridge.rs:393-431`; `:776-795`; `reply_immediate_error` `:801-822`
- Today: WV-011
- After this revision: **WV-011**

- #: S4
- Stage: Host dispatch: undeclared channel
- Owner / site at `516644b0`: `app_session.rs:4213-4226`
- Today: WV-011
- After this revision: **WV-011**

- #: S5
- Stage: `begin_echo_call`: session quiescing
- Owner / site at `516644b0`: `app_session.rs:5813-5819`
- Today: WV-011 "unavailable or already pending"
- After this revision: **WV-011**, detail "application is quitting"

- #: S6
- Stage: `begin_echo_call`: a lock is poisoned
- Owner / site at `516644b0`: `:5820-5823`, `:5831-5834`
- Today: WV-011 (same text)
- After this revision: **WV-011**, detail "host call state is unavailable"

- #: S7
- Stage: `begin_echo_call`: session-wide slot busy
- Owner / site at `516644b0`: `:5835-5840`
- Today: WV-011 (same text)
- After this revision: **WV-011**, detail "another application call is already pending"

- #: S8
- Stage: `begin_echo_call`: no admitted generation (`current` is `None`)
- Owner / site at `516644b0`: `:5824-5829`
- Today: WV-011 (same text)
- After this revision: **WV-012**

- #: S9
- Stage: `write_frame` returns `Err` (any byte count, k ≥ 0)
- Owner / site at `516644b0`: `:5853-5876`
- Today: WV-011 (same text)
- After this revision: **WV-013**

- #: DP
- Stage: `write_frame` returns `Ok`
- Owner / site at `516644b0`: `:5878`
- Today: (none)
- After this revision: (none)

- #: S10
- Stage: After DP: link EOF/reset, reader error, `Revoke` retirement, failed lifecycle or filesystem write, accepted Quit, profile handover or lease loss, router shutdown, reply
  channel disconnected
- Owner / site at `516644b0`: every retirement that answers the pending slot, e.g. `:6343`, `:6436`, `:6558`, `:6719`, `:7002`, `:7044`, `:7396`, `:7718`; all mapped generically at
  `:4278-4285`
- Today: WV-011 "application link ended before the renderer reply"
- After this revision: **WV-013**

- #: S10b
- Stage: After DP: Bun answers ERR with a `KELD-WV-*` code
- Owner / site at `516644b0`: forwarded at `:4272-4278`
- Today: that code, verbatim
- After this revision: **WV-013** (host rewrite, criterion 11)

- #: S11
- Stage: Bun: no handler registered at dispatch time (never registered, unsubscribed, or registered after this call arrived)
- Owner / site at `516644b0`: `channels.ts:95-97`
- Today: API-001
- After this revision: **API-002**

- #: S12
- Stage: Bun: `decodeRequest` throws
- Owner / site at `516644b0`: `channels.ts:99`
- Today: API-001
- After this revision: **API-001** (Q-2, verified in §10)

- #: S13
- Stage: Bun: handler throws or rejects
- Owner / site at `516644b0`: `channels.ts:100`
- Today: API-001
- After this revision: **API-001**

- #: S14
- Stage: Bun: `encodeResponse` throws, or reply > `MAX_FRAME_LEN`
- Owner / site at `516644b0`: `channels.ts:101`, `echo-call.ts:26-28`
- Today: API-001
- After this revision: **API-001** (the effect is committed)

- #: S15
- Stage: Reply in flight when Bun dies
- Owner / site at `516644b0`: host reader vs `Revoke` race
- Today: reply or WV-011
- After this revision: reply or **WV-013**, never WV-012

- #: S16
- Stage: Page `pagehide` while an invoke is pending
- Owner / site at `516644b0`: `macos_bridge.rs:63-68`
- Today: WV-011 "renderer document navigated before reply"
- After this revision: **WV-013**

- #: S17
- Stage: Bun alive but never answers
- Owner / site at `516644b0`: dispatch thread waits with no deadline (`:4253-4266`)
- Today: no terminal
- After this revision: unchanged ([historical §4.7](design-7.md))

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

Promotion rule for S9 (owner decision Q-4, §10). Failed and partial writes stay
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

# Owner decisions and outstanding approval

Part of: [index](index.md)
Owns: Owner decisions and outstanding approval

## 10. Open questions, blockers & unknowns

### B-1: Exact r3 content approval

- **Status:** open
- **Owner:** repository owner
- **Impact:** This draft does not authorize production implementation.
- **Check:** Obtain explicit approval of the exact migrated r3 content.

The owner answered Q-1–Q-5 on 2026-10-23, and the coordinator relayed the
answers on PR #1096. These are directions, not approval of the exact content.
This spec stays `Status: draft` until that approval.

### Q-1: WV-012 independent of the restart notification ([#1098](https://github.com/gyldlab/keld/issues/1098)): Option A.


   - Ship WV-012 without waiting for the notification, and emit it only when
     non-dispatch is proven (S8).
   - No blind retries, no timer retry loops, no Keld replay.
   - Until restart readiness exists ([#1098](https://github.com/gyldlab/keld/issues/1098)), one resend is allowed
     only after a later user action or a reliable app signal ([historical §4.4](design-4.md)).
   - The restart notification and restart readiness are tracked in [#1098](https://github.com/gyldlab/keld/issues/1098).
   - Keld does not claim automatic recovery.
### Q-2: decode failure: provisional Option A, verified.

 Decode failures
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
       diagnostic. That is recorded in the [historical §4.9](design-9.md) hand-off.
     - No code is added.
### Q-3: the GH-527 `KELD-IPC-022` claim: Option A.


   - Corrected to Unknown unless AT-B3 proves otherwise ([historical §4.5](design-5.md)).
   - `KELD-IPC-024` keeps its host-side non-execution proof.
   - The `transport.ts` comment is a T-5 follow-up.
### Q-4: failed and partial writes: Option A.

 They stay WV-013. The six
   promotion preconditions are recorded in [historical §4.2](design-2.md).
### Q-5: hung Bun: Option A.

 Tracked in its own issue,
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
[historical §4.7](design-7.md), criterion 12 and AT-S arm c.

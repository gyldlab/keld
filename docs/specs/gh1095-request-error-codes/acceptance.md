# Dispatch and retry acceptance

Part of: [index](index.md)
Owns: Dispatch and retry acceptance

## 3. Acceptance criteria

### AC-1: WV-012 only without a write

- **Given:** A renderer request and no admitted generation.
- **When:** Invoke at startup or the post-crash admission gap.
- **Then:** The renderer call arrives while the router
   has no admitted generation (`current` is `None`). This covers both app
   startup, before the first generation is admitted, and the gap after a crash.
   The host returns
   `KELD-WV-012`. `write_frame` is never invoked for that call, and the handler
   census for that call stays 0 in every generation.
- **Negative control:**
  - **Single fault:** Move the no-generation refusal after the write.
  - **At-risk precondition:** A counting writer observes admission without a generation.
  - **Expected RED:** AT-12b records a write instead of zero.

### AC-2: Failed or partial write is Unknown

- **Given:** An admitted call and a writer that fails after a selected byte count.
- **When:** Fail the write at each AT-13a cut.
- **Then:** `write_frame` fails after k bytes,
   for any k including 0. The host returns `KELD-WV-013`, never `KELD-WV-012`.
   It clears the slot, retires the generation and writes no further byte to that
   stream.
- **Negative control:**
  - **Single fault:** Classify a failed write as WV-012.
  - **At-risk precondition:** The write failure occurs at S9.
  - **Expected RED:** The expected WV-013 assertion fails.

### AC-3: After the dispatch point is Unknown

- **Given:** A completed CALL write with a pending outcome.
- **When:** End the link, reader, generation or lifecycle write at S10.
- **Then:** The CALL write completed, and then
   the link ends, the reader fails, the generation is revoked, or a lifecycle
   write fails. The host returns `KELD-WV-013` in every case, whether or not the
   handler ran.
- **Negative control:**
  - **Single fault:** Classify S10 as WV-012.
  - **At-risk precondition:** Hold a handler after entry before retiring its generation.
  - **Expected RED:** AT-13b sees the wrong code after a census of one.

### AC-4: Navigation while pending is Unknown

- **Given:** A page with a pending invoke.
- **When:** Deliver pagehide during navigation.
- **Then:** The page facade has a pending
   invoke when `pagehide` fires. The promise rejects with `KELD-WV-013`.
- **Negative control:**
  - **Single fault:** Leave the pagehide outcome unclassified as WV-013.
  - **At-risk precondition:** The original document has a pending promise.
  - **Expected RED:** AT-13c misses its original-document WV-013 beacon.

### AC-5: No handler is API-002

- **Given:** An Echo CALL reaches the API registry.
- **When:** Exercise missing and existing handlers and the failure classes below.
- **Then:** The Echo CALL reaches `@keld/api` while no
   handler is registered. Bun answers one ERR `KELD-API-002` and runs no app
   code. A handler that throws, a decode failure, an encode failure and an
   oversize reply answer `KELD-API-001`, never `KELD-API-002`.
- **Negative control:**
  - **Single fault:** Move the no-handler check after handler invocation.
  - **At-risk precondition:** AT-API2a records handler entry independently.
  - **Expected RED:** API-002 has a nonzero handler census.

### AC-6: WV-011 is narrowed and still pre-write

- **Given:** A facade refusal or hostile envelope at the listed admission stages.
- **When:** Exercise each S1–S7 refusal and S3a rejection.
- **Then:** Every remaining `KELD-WV-011`
   emitter returns before `begin_echo_call` invokes `write_frame`. These are:
   - page facade validation;
   - the isolated-world busy check;
   - native admission (S3b);
   - the undeclared channel check;
   - quiescing;
   - lock poisoning;
   - the session-wide busy slot.

   Hostile envelopes rejected at S3a get no terminal and are never dispatched.
- **Negative control:**
  - **Single fault:** Emit WV-011 after a failed write.
  - **At-risk precondition:** AT-11 and AT-13a distinguish admission from S9.
  - **Expected RED:** The stage-specific code/write assertion fails.

### AC-7: Exactly one terminal, no replay

- **Given:** A facade-formed invoke and its original document.
- **When:** Race terminal outcome with retirement or a committed page navigation.
- **Then:** Each facade-formed invoke whose
   document is alive when the outcome is decided gets exactly one terminal.
   A hostile envelope that skips the facade can get none ([historical §4.2](design-2.md), S3a). A later
   reply or retirement for the same call is discarded. No Keld layer re-sends a
   CALL frame.
   - The guarantee covers every navigation that can occur today: a
     page-initiated main-frame navigation, which resets the bridge only when it
     commits ([historical §4.7](design-7.md)). In that case the old document settles once with
     `KELD-WV-013` on `pagehide`, and the successor never settles the old
     call.
   - A host-initiated navigation while the app runs does not exist today
     ([historical §4.7](design-7.md)). When #459 adds one, it MUST keep this criterion: a reset whose
     load never commits settles the live document's call once with
     `KELD-WV-013`.
- **Negative control:**
  - **Single fault:** Remove the correlation or document match.
  - **At-risk precondition:** AT-S holds a stale result while the successor owns a pending request.
  - **Expected RED:** A second or successor settlement appears.

### AC-8: One resend after WV-012 or API-002 runs once

- **Given:** WV-012 proved no host write, or API-002 proved no application handler execution.
- **When:** Perform one new invoke after the specified user/app signal or handler registration.
- **Then:** The page resends once,
   as a new invoke, and the handler census for the logical action is exactly 1.
   The trigger depends on the code:
   - after WV-012, at startup or after a restart: until restart readiness
     exists ([#1098](https://github.com/gyldlab/keld/issues/1098)), the trigger is a later user action or a reliable
     app signal. A generation becoming ready is not, by itself, an app-visible
     trigger;
   - after API-002, the trigger is the handler being registered, which may
     happen in the same generation (late registration).
- **Negative control:**
  - **Single fault:** Resend again after the permitted resend is also refused.
  - **At-risk precondition:** AT-12c and AT-12d keep admission blocked for the click control.
  - **Expected RED:** The at-most-one-resend observation fails.

### AC-9: Bun outbound codes are unchanged and documented as Unknown

- **Given:** A Bun outbound call before or after link termination.
- **When:** Run the distinct AT-B1, AT-B2 and AT-B3 schedules.
- **Then:** - `WorkerLink.call` returns `KELD-IPC-022` both when the link had already
     ended and when the call was still pending at close. Neither case claims
     "not sent".
   - The receiver-side refusals `KELD-IPC-024` and `KELD-IPC-005` (capacity)
     keep "not executed".
- **Negative control:**
  - **Single fault:** Remove the pre-call terminal guard.
  - **At-risk precondition:** AT-B1 starts with the link already ended.
  - **Expected RED:** The Host records a frame instead of zero.

### AC-10: Existing error shape is kept

- **Given:** A coded rejection with an already-prefixed or bare/different-code detail.
- **When:** Inspect page and wire error fields in AT-EC.
- **Then:** - A page rejection is still an `Error`. Its message keeps today's bytes
      whenever the detail already starts with `"<code>: "`. That covers every
      message Keld itself emits: facade refusals, host refusals and
      retirements, the fixed `KELD-API-001` text, and `@keld/kipc`
      `codedError` text.
    - A Bun-supplied detail that does not start with `"<code>: "` gains that
      prefix under the single rule in [historical §4.9](design-9.md). This is the only byte change, and
      it is deliberate: it is what makes criterion 13 hold.
    - The only other addition is the `code` property of criterion 13.
    - A Bun ERR still carries the code in `CallError.code`.
    - The wv-link result still carries it in `code`. The only addition is the
      private `document` field of criterion 12, which never reaches the page.
- **Negative control:**
  - **Single fault:** Prefix an already canonical message again.
  - **At-risk precondition:** The input already starts with its own code and colon-space.
  - **Expected RED:** The exact message-byte assertion fails.

### AC-11: The host never trusts a Bun-supplied `KELD-WV-*` code

- **Given:** A dispatched Echo call and a Bun-supplied error code.
- **When:** Return reserved WV codes and an API control.
- **Then:** Bun answers an
    Echo CALL with an ERR whose `CallError.code` starts with `KELD-WV-`. The
    host forwards it to the renderer as `KELD-WV-013`.
    - A Bun ERR with any other code (`KELD-API-*`, `KELD-IPC-*`) is forwarded
      unchanged.
    - Only the host can mint `KELD-WV-011` and `KELD-WV-012` for a renderer
      call.
- **Negative control:**
  - **Single fault:** Remove the reserved-WV rewrite.
  - **At-risk precondition:** AT-SP supplies WV-012 after a CALL was written.
  - **Expected RED:** The page receives WV-012 rather than WV-013.

### AC-12: A result settles only the document and request that sent it

- **Given:** An old document result and a successor request with the same request ID.
- **When:** Deliver the stale result after the successor binds.
- **Then:** - Each native→isolated result carries the document nonce of the request it
      answers. The nonce stays inside keld-wv end to end ([historical §4.7](design-7.md)). The structs
      that cross into keld-core (`RendererBridgeRequest`,
      `RendererBridgeOutcome`) carry no nonce.
    - The isolated world settles a result only if both of these hold:
      `document` equals its current nonce, and `request` equals its in-flight
      request id. It drops any other result without changing `inflight`.
    - A stale invoke from a predecessor document therefore never settles the
      successor's pending call.
- **Negative control:**
  - **Single fault:** Remove the document check.
  - **At-risk precondition:** AT-S keeps the successor request pending.
  - **Expected RED:** The successor settles from the old result.

### AC-13: The page can read the code without parsing the message

- **Given:** A page-facing invoke rejection.
- **When:** Inspect the error and its own enumerable code property.
- **Then:** Every
    `window.keld.invoke` rejection is an `Error` with an own, enumerable,
    string `code` property. Its `message` always starts with
    `code + ": "` under the [historical §4.9](design-9.md) prefix rule.
- **Negative control:**
  - **Single fault:** Drop the code definition in the facade helper.
  - **At-risk precondition:** AT-EC observes the actual rejected Error.
  - **Expected RED:** The code/property assertion fails.

### AC-14: An Unknown outcome never becomes retryable through a negative status snapshot alone (GH-1132)

- **Given:** An Unknown outcome with a deferred effect still able to commit.
- **When:** Observe absent status, challenge retry, then release the earlier effect.
- **Then:** For `KELD-WV-013` and `KELD-API-001`, a
    status check that reports "not committed yet" does not authorize a
    non-idempotent resend while the earlier effect could still commit. The app
    must either ensure both attempts share atomic durable duplicate
    suppression, or prove that the original attempt is permanently unable to
    commit. AT-RS (§7) exercises the API-001 late-commit arm in portable Bun.
    The WV-013 arm is **pending hosted macOS WebView acceptance**, owned by
    GH-1095 T-2 (native host harness) with T-4 (keld-wv renderer bridge). No
    portable Bun result is evidence of a page receiving WV-013.
- **Negative control:**
  - **Single fault:** Replace atomic suppression or the finality fence with an absent-status guard.
  - **At-risk precondition:** AT-RS begins the second effect before releasing the first.
  - **Expected RED:** Two committed effects violate the at-most-once assertion.

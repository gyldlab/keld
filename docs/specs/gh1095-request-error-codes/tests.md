# Test oracles and evidence limits

Part of: [index](index.md)
Owns: Test oracles and evidence limits

## 7. Test plan & evidence

Shared oracles:

- **Handler census:** every test handler POSTs `{gen, pid, nonce, phase}` to the
  test observer. The phases are `entered`, `effect-committed` and `returning`.
- **Host markers:** `RENDERER_ADMIT`, and `KELD_KEL142_KIPC_CALL` (DP).
- **Terminal count:** the number of page settlements per nonce.
- **"Census 0 forever":** re-checked after the successor generation serves three
  more calls.

Every hosted arm prints its raw census with `--success-output immediate`.


### E-1: AT-12a

- **Run:** Planned AT-12a scenario at S8 (G2 held before HELLO); original criterion 1, 7.
- **n:** 10 × arms {handler before `whenReady`, handler inside `whenReady`}
- **Environment:** hosted macOS
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: page code `KELD-WV-012`; `KIPC_CALL` = admits − 1; census 0 for G1 and G2 forever; 1 terminal

Negative control / mutation: G2 released before the invoke → reply, census 1 (the oracle can count)

### E-2: AT-12b

- **Run:** Planned AT-12b scenario at S8; original criterion 1.
- **n:** deterministic
- **Environment:** macOS unit (keld-core)
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: WV-012 variant returned; the counting writer saw **0 write calls**

Negative control / mutation: admitted generation → 1 write call and no WV-012; **own mutation:** move the `current is None` refusal after `write_frame` → the counting writer sees 1
call and AT-12b fails; cross-check: map `WriteFailed` to WV-012 → AT-13a fails

### E-3: AT-12d

- **Run:** Planned AT-12d scenario at S8 at startup (G1 held before HELLO, page already loaded); original criterion 1, 8.
- **n:** 10
- **Environment:** hosted macOS
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: page invokes before the first admission → `KELD-WV-012`, census 0, `KIPC_CALL` = admits − 1. The test then releases G1 and waits for the host Ready marker; this is a
test precondition only, not a page-visible trigger. Then it drives a **user action**: an OS-visible pointer click on the fixture's resend button, as in KEL-142 AC3. The page
resends once → reply, census 1

Negative control / mutation: (i) G1 released before the first invoke → reply, census 1, so the startup gap is load-bearing; (ii) the same click driven while G1 is still held →
WV-012 again, surfaced, and no second resend (at most one resend)

### E-4: AT-12c

- **Run:** Planned AT-12c scenario at S8 then resend after a restart; original criterion 8.
- **n:** 10
- **Environment:** hosted macOS
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: after a WV-012 in the restart gap, the test waits for G2's host Ready marker (test precondition only), then drives an OS-visible click on the resend button → one resend
→ reply; census for the logical action = 1

Negative control / mutation: the same click driven before G2 is admitted → WV-012 again, surfaced, and no second resend

### E-5: AT-13a

- **Run:** Planned AT-13a scenario at S9, k ∈ {0, 1, HEADER_LEN−1, HEADER_LEN, HEADER_LEN+len−1}; original criterion 2.
- **n:** deterministic per k
- **Environment:** macOS unit (keld-core; extends `failed_primary_writes_retire_the_generation`)
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: WV-013; slot cleared; generation retired; 0 bytes written to the stream after the failure

Negative control / mutation: k = full length → DP reached, no S9; characterization: an injected completing write after failure makes the Bun `FrameReader` dispatch (proves why S9
cannot be WV-012 yet)

### E-6: AT-13b

- **Run:** Planned AT-13b scenario at S10; original criterion 3, 7.
- **n:** 10 per arm
- **Environment:** hosted macOS
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: arms: (i) SIGSTOP Bun → invoke → SIGKILL: census 0; (ii) kill at `entered`: census 1; (iii) kill after `effect-committed`: effect present. Every arm: `KELD-WV-013`, 1
terminal, G2 census for the nonce 0 forever

Negative control / mutation: arm (ii) census 1 under the same code proves WV-013 cannot mean "not sent"; mutation: S10 → WV-012 fails arm (ii)

### E-7: AT-13c

- **Run:** Planned AT-13c scenario at S16; original criterion 4.
- **n:** 10
- **Environment:** hosted macOS
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: handler held at `entered`; page `location.reload()`; the `pagehide` rejection beacon reports `KELD-WV-013`; census 1; no settlement in the successor document

Negative control / mutation: navigate while queued before bind → census 0 with the same WV-013 (the facade cannot tell)

### E-8: AT-13d

- **Run:** Planned AT-13d scenario at S15; original criterion 7.
- **n:** 50
- **Environment:** hosted macOS
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: kill Bun on the `returning` POST; outcome ∈ {reply, WV-013}; never WV-012; 1 terminal; the success:Unknown ratio is reported, not asserted

Negative control / mutation: **kill disabled**, same harness → 50/50 reply and 0 WV-013 (the Unknown outcomes come from the kill, not from the harness); mutation: S15 retirement →
WV-012 fails

### E-9: AT-API2a

- **Run:** Planned AT-API2a scenario at S11–S14; original criterion 5.
- **n:** deterministic
- **Environment:** `bun test` in `@keld/api`, macOS + Linux + Windows CI
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: missing → API-002, census 0; late (call, then register, then call) → API-002 then reply, census 1; unsubscribed → API-002; handler throws after effect → API-001, census
1; decode failure → API-001, census 0; oversize → API-001, census 1

Negative control / mutation: mutation: move the no-handler check after `handler(request)` → the census for the API-002 case becomes 1 and the test fails; a throwing handler never
yields API-002

### E-10: AT-API2b

- **Run:** Planned AT-API2b scenario at S11 across restart; original criterion 5, 8.
- **n:** 10 × arms {inside `whenReady`, before `whenReady`}
- **Environment:** hosted macOS
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: inside: at least one post-restart API-002 with G2 census 0, then one resend → census 1; before: 0 API-002 across all N

Negative control / mutation: the inside arm reproduces the window (round-1 E8 hit it 10/10 under API-001)

### E-11: AT-11

- **Run:** Planned AT-11 scenario at S1, S2, S3b, S4–S7 (and S3a: no terminal); original criterion 6.
- **n:** deterministic
- **Environment:** macOS unit (keld-wv + keld-core)
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: each reason → WV-011 and 0 write calls; each S3a reason → no terminal and no dispatch

Negative control / mutation: mutation: emit WV-011 from S9 → fails

### E-12: AT-RS (GH-1132)

- **Run:** Planned AT-RS (GH-1132) scenario at Unknown outcome with an unfinished effect; original criterion 14; [historical §4.4](design-4.md).
- **n:** portable: one API-001 failure plus controls; WV-013 and durability arms remain pending
- **Environment:** portable Bun app-level fixture (real `resolveEchoCall`; **API-001 only**); **WV-013 pending hosted macOS** under GH-1095 T-2 native host harness + T-4 keld-wv;
  durable-store integration pending its own owner
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: (a) handler schedules a deferred effect then throws; Bun answers API-001, status query reads "not committed yet", attempted status-only resend is **refused** (or uses
the same durable idempotency claim); when the first deferred effect is released, committed-effect count ≤ 1. (b) **PENDING hosted Mac, not run by Bun:** page receives WV-013 while
first handler is held before commit; status is absent; a retry is likewise forbidden without proof. Positive: two attempts with the same durable claim result in one effect and one
logical result, even if the first finishes late. A running-model reproduction of the old unsafe rule may observe 2 commits, which is evidence of the vulnerability, **not** a
passing safe-retry test.

Negative control / mutation: Mutation/negative control: replace the fence/atomic claim with a plain status=absent guard, then release the original after the second begins → 2
committed effects, so the at-most-once assertion fails. Control with no resend → 1 effect; a returned API-001 by itself never cancels spawned work. The portable test proves neither
WV-013 page delivery nor persistence across process death. GH-1095 T-2/T-4 must implement and register the hosted WV-013 arm with its own real WebView/page/error receipt and
first-handler-late-commit negative control; the durable-store owner must separately integrate a persisted-at-most-once test.

Additional planned AT-RS control (not an executed receipt): the original
handler commits one effect and then returns API-001, with no work left able
to commit. Without atomic deduplication, a retry guard must refuse a resend
because the effect already exists; final effect count remains 1. Single
fault: drop the no-past-commit check while retaining the future-execution
fence, then resend the same non-idempotent logical action. The second effect
makes the at-most-once assertion fail (2 effects). Positive finality control:
establish no committed effect and an enforceable fence preventing every
original attempt from committing, then one new attempt produces 1 effect.
These app-level proof obligations do not claim WebView or process-death
acceptance and do not replace the existing late-commit controls.

### E-13: AT-B1

- **Run:** Planned AT-B1 scenario at `#throwIfTerminal`; original criterion 9.
- **n:** deterministic
- **Environment:** `@keld/kipc` unit, 3 OSes
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: link already ended → `call()` rejects IPC-022; host fixture read 0 frames

Negative control / mutation: remove `#throwIfTerminal` → frame posted (the check is the proof, but the code stays Unknown)

### E-14: AT-B2

- **Run:** Planned AT-B2 scenario at pending at close; original criterion 9.
- **n:** deterministic
- **Environment:** `@keld/kipc` unit, 3 OSes
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: pending, then host close → IPC-022; host read 1 frame; same code as AT-B1

Negative control / mutation: host writes the REPLY before closing → the call resolves with the reply, not IPC-022 (the close produces 022, not the harness)

### E-15: AT-B3

- **Run:** Planned AT-B3 scenario at written before Quit, pending at close; original criterion 9.
- **n:** deterministic
- **Environment:** keld-core unit
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: host census for that CALL recorded; only a census of 0 in every arm could reverse Q-3's Unknown

Negative control / mutation: written after Quit → IPC-024, census 0 (existing `guarded_router_and_quit.rs`)

### E-16: AT-S

- **Run:** Planned AT-S scenario at settlement; original criterion 7, 12.
- **n:** deterministic (unit); 10 (hosted)
- **Environment:** macOS unit (keld-wv, keld-core) + hosted macOS
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: (a) late reply after retirement refused; (b) facade result after `pagehide` ignored, and native result for a stale `(navigation, request)` dropped; (c) a stale-document
invoke refused after the successor binds, while the successor has a pending call with the same request id → the successor's call settles only from its own result; (d, r2) handler
held at `entered`, then a **page-initiated** main-frame navigation that commits (`location.href` to a fixture page) → the original document settles exactly once with `KELD-WV-013`
(from `pagehide`, counted by a beacon carrying the original document's nonce); census 1; the successor records 0 settlements; the later host outcome is dropped. The uncommitted
host-`navigate()` arm moves to #459. T-2 pins today's behaviour (c may cross); T-4 flips c to the target

Negative control / mutation: (a) remove the corr/attempt check → second settlement observed; (c) remove the `document` check in `__keldBridgeNativeResult` → the successor settles
with the stale WV-011; (d) the same harness with no navigation → the call settles once with its real reply, census 1 (the WV-013 comes from the navigation); mutation: make the
successor accept a result without the `document` check → the successor records a settlement and the arm fails

### E-17: AT-SP

- **Run:** Planned AT-SP scenario at S10b; original criterion 11.
- **n:** deterministic
- **Environment:** macOS unit (keld-core; fake Bun peer on `socketpair`)
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: Bun ERR `code = "KELD-WV-012"` (and `"KELD-WV-011"`) → renderer outcome `KELD-WV-013`; the frame was written (`KIPC_CALL` emitted)

Negative control / mutation: Bun ERR `code = "KELD-API-002"` → forwarded unchanged (the rewrite is scoped to `KELD-WV-*`); mutation: remove the rewrite → the page sees
`KELD-WV-012` and the test fails

### E-18: AT-EC

- **Run:** Planned AT-EC scenario at every invoke rejection; original criterion 10, 13.
- **n:** deterministic (unit); 10 per code (hosted)
- **Environment:** macOS keld-wv unit + hosted macOS
- **Limits:** This is an acceptance plan, not a new executed receipt.

Observable: the hosted page reports `{code: err.code, message: err.message}` for WV-011 (`opts` given), WV-012 (AT-12a), WV-013 (AT-13b) and API-002 (AT-API2b): `err.code` is a
string equal to the message prefix; `message` is byte-equal to today's format; `err instanceof Error`; `Object.keys(err)` includes `"code"` (intended,
[historical §4.9](design-9.md)). Prefix rule (r2), keld-core and keld-wv units: the host forwards `CallError.message` unchanged; a Bun ERR message `KELD-API-001: x` → page message
unchanged; `KELD-API-001 failed` → `KELD-API-001: KELD-API-001 failed`; a detail that starts with a *different* code is prefixed and never re-coded: a `KELD-WV-013` rejection whose
detail is `KELD-API-001: x` → message `KELD-WV-013: KELD-API-001: x` with `err.code === "KELD-WV-013"`, and a Bun ERR `code = KELD-API-002` with message `KELD-API-001: x` →
`KELD-API-002: KELD-API-001: x` with `err.code === "KELD-API-002"` (`code` always comes from the code field, never from the detail); a host refusal with a bare detail → prefixed
once (the rule is idempotent). Unit: `PAGE_FACADE_SCRIPT` builds every rejection through its one coded-error helper (exactly one `new Error(` in the script)

Negative control / mutation: the observer run against a plain `new Error("KELD-WV-011: x")` reports `code` undefined (the oracle detects absence); mutation: drop the `code`
definition in the helper → every hosted arm fails

Anti-flake:

- No sleeps. Holds and releases use the existing test-only observer gates.
- Kills are driven from census POSTs, not timers.
- Hosted arms use the PR #1082 fixtures, promoted into landing tests by T-2.

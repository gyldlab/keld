# Spec: Message types, payload ownership and request outcomes
Status: draft
Linear: GH-1102 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; documentation only, do not merge.

## 1. Goal & non-goals

Make each kipc frame kind, payload owner and caller outcome discoverable and checked without changing
a single wire byte. Distinguish an application error, a session fault and uncertain dispatch so
applications cannot mistake a diagnostic code for permission or a safe retry instruction.

Non-goals: new frame kinds, channel exchange, a bulk plane, credit-lane implementation, payload
fields, GH1095's code split, GH1097's deadlines, schema tooling selection or a central runtime
registry. Existing authentication, guard and lifetime owners remain unchanged.

## 2. Spec refs

- [Owning issue GH1102](https://github.com/gyldlab/keld/issues/1102); [census GH1100](https://github.com/gyldlab/keld/issues/1100).
- [Architecture 02 control protocol and current/target distinction](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/02-ipc.md).
- [KIPC version and error rules](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-ipc/AGENTS.md).
- [Eleven frame discriminants and header](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-ipc/src/frame.rs).
- [Protocol constants](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-ipc/src/lib.rs).
- [Canonical channel owners/classes](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-ipc/src/channel_table.rs).
- [Exact allowed receive policies](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-ipc/src/receive.rs).
- [CallError encoding](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-ipc/src/call_error.rs).
- [Canonical TypeScript transport and waiters](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/kipc/src/transport.ts).
- [Existing generator and payload source paths](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/kipc/scripts/echo-codegen.ts).
- [Current source parity test](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/kipc/src/transport.test.ts).
- [Wire-format consumer catalog](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/onboarding/04-wire-formats-and-contracts.md).
- [GH1095 r2 request classes, pending fresh approval](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh1095-request-error-codes.md).
- [Worker transport and terminal owner GH527](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh527-worker-owned-blocking-call-transport.md).
- [Reply deadline owner GH1097](https://github.com/gyldlab/keld/issues/1097).
- [Error meanings owner GH1101](https://github.com/gyldlab/keld/issues/1101).
- [Lifecycle taxonomy owner GH1103](https://github.com/gyldlab/keld/issues/1103).

Verified baseline: FrameKind defines 11 values; PROTOCOL_VERSION is 2 and the header is 16 bytes.
The TypeScript source-parity test checks Ping among the frame kinds, not all 11 discriminants.
Channel constants/receive classes are generated from the Rust table; FrameKind, CallError codec
and WorkerLink terminal STATE mappings retain hand-written copies.

Architecture 02 explicitly says Grant has no live sender/receiver and GH527 credit would require a
protocol bump. The presence of a decodable enum discriminant does not make its operation live.
Current GH1095 r2 on main still says draft; its merged PR is not fresh exact-content approval.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given the Rust frame enum and all TypeScript copies, when the catalog/check runs, then all 11
values match the literal contract and each has direction, channel class, settlement and live
status. No reserved-but-not-live kind is advertised as implemented.

Independent negative control: mutate any one Rust discriminant, including one other than Ping,
or classify Grant live based only on its enum declaration; that individual control must fail.

### AC2

Given each actual receive-policy row, when frame kind/channel/correlation/flags/phase is varied,
then only that row's declared combinations pass. A generically valid frame is not sufficient.

Independent negative control: allow ERR in the strict echo reply waiter, allow EVENT on a
GuardedCall channel, accept PING before HELLO, or allow a wrong correlation to settle a call.

### AC3

Given the GH1100 copy census, when every channel payload is followed to its consumers, then each
schema has exactly one authoring owner and each copy has an existing or proposed named parity
check. Generated declarations are never hand-edited to make the comparison pass.

Independent negative control: add an unlisted hand-written window or lifecycle discriminant copy
or change a Rust field while retaining stale TypeScript bytes.

### AC4

Given independent literal header and CallError vectors, when the pre-change and proposed tooling
produce/consume them, then every byte and result remains identical and PROTOCOL_VERSION stays 2.

Independent negative control: swap CallError field order, change a varint boundary, accept trailing
bytes, or change an existing header value while updating both implementations together. Literal
fixtures outside the generator must still detect the coordinated drift.

### AC5

Given each outcome class in section 4.4 for Bun-to-host and renderer-to-Bun calls, when the class
is induced, then one caller settlement occurs with the owning code/terminal disposition. A local
timeout or link failure does not prove an effect did not happen.

Independent negative control: retry an outcome-unknown write automatically, report not-dispatched
after a partial write, or let both deadline and late response settle the same request.

### AC6

Given authenticated routing, when the session token, allocated channel, host-bound document,
generation or pending correlation is wrong, then the existing owner refuses before wrong-route
dispatch. Peer error text and code never select a destination or authorize a capability.

Independent negative control: route by the peer's code prefix or trust a payload generation
instead of the authenticated host binding; observe zero wrong-principal handler entries.

### AC7

Given the single version rule in section 4.5, when its documentation migration lands, then the
other three existing rule locations link to its canonical owner with no contradictory duplicate.

Independent negative control: leave one source saying payload-field edits require a frame-version
bump, or change an ordinary shared-endpoint HELLO/frame while claiming no bump is needed.

### AC8

Given a WorkerLink terminal STATE value, when terminal publication and public rejection occur,
then its registered code, first-terminal precedence and ordered retained-event behavior match
GH527. A transport number is not reinterpreted as an application outcome or process exit code.

Independent negative control: change STATE_OVERFLOW without its registered mapping, overwrite an
earlier terminal, or drop an already-retained ERR when EOF follows it.

### AC9

Given a current main implementation and an unapproved predecessor draft, when the catalog is
published, then current behavior and proposed outcome classes occupy distinct evidence rows.
This issue cannot close with pending GH1095 or GH1097 semantics silently marked implemented.

Independent negative control: infer WV-012/013 or API-002 shipping behavior from a proposal's code
table while the actual path still emits its older class.

### AC10

Given a malformed ERR, when the bare helper and the real WorkerLink path run, then the helper
fallback and link-terminal disposition remain distinct and preserve their typed-code owner.

Independent negative control: replace WorkerLink's decode-and-fail-link path with the helper's
recoverable-looking error. A valid subsequent request must still be refused after terminality.

## 4. Design

### 4.1 Frame catalog and receiver authority

FrameKind owns numeric discriminants; ReceivePolicy owns legal receiving combinations; the
channel table owns allocation and class. No second dispatcher or generic permissive reader.
The following is the frozen v2 census for review, not permission to introduce a sender.

| Kind | Value | Permitted direction/class at this baseline | Call terminal and live status |
| --- | --- | --- | --- |
| Hello | 0 | Client-to-server pre-auth and server-to-client HELLO reply; reserved handshake channel, zero correlation, raw 32-byte token under the exact HELLO policies | Authentication only, not a Call result; live. |
| Call | 1 | App-to-host on declared HostCall, HostCallWithEvents or GuardedCall rows; host-to-app Echo only through the app-declared echoReceiver | Expects a matching Reply or declared Err; session/deadline termination can instead settle locally; live. |
| Reply | 2 | Opposite an admitted Call, same pending channel/correlation and that waiter's policy | One successful response; duplicate/foreign replies never settle another call; live. |
| Err | 3 | Opposite an admitted Call only on an ERR-declaring waiter; strict echo waiter is Reply-only | One structured application failure when valid; malformed bytes are a protocol fault; live on declared paths. |
| Event | 4 | Host-to-app on HostCallWithEvents and its actual event-receiver/subscription policy | No call result; ordered notification, not authority; live. |
| StreamOpen | 5 | No live receive-policy row | Reserved schema value, not implemented. |
| StreamChunk | 6 | No live receive-policy row | Reserved schema value, not implemented. |
| StreamClose | 7 | No live receive-policy row | Reserved schema value, not implemented. |
| Cancel | 8 | No live receive-policy row | Reserved schema value; no cancellation/no-effect promise. |
| Grant | 9 | No live receive-policy row; GH527 owns future worker-to-host event credit | Reserved schema value; optional credit requires its separate protocol revision. |
| Ping | 10 | Only authenticated policies with allow_ping, empty payload and zero flags as validated by the existing owner | Liveness traffic, never a Call terminal; live. |

A header decoder recognizing a kind does not establish admission. Keep exact direction/phase,
channel, flags, size and correlation checks in receive.rs and the generated/current TS policies.
Strict echo and primary reverse-echo waiters are different rows; do not widen one to match the other.
ChannelId construction remains unforgeable outside its owner. A channel id selects routing,
never principal identity, authorization or the existence of a grant.

### 4.2 Payload ownership and copies

| Family | Authoring owner | Consumer/copy and required proof |
| --- | --- | --- |
| Echo request/response | keld-ipc echo.rs | Generated echo declarations plus actual Rust/TS postcard byte tests. |
| Filesystem request/response | keld-native src/fs.rs | Existing generated fs declarations and real codecs; golden bytes and malformed boundaries. |
| Authentication request/response | keld-native src/auth/mod.rs | Existing generated auth declarations; preserve redacted token-denial semantics. |
| Lifecycle request/response/event | keld-ipc lifecycle.rs | Hand-written transport/API consumers; complete discriminant, field and ordering parity through GH1103. |
| Window request/response/event | keld-ipc window.rs | Existing API window codec and source; exact discriminant/field/lifetime vectors, no duplicate registry. |
| CallError | keld-ipc call_error.rs | Two-field postcard schema plus TS encodeCallError/decodeCallError; GH1101 owns logical meaning and redaction. |
| Frame constants/kinds | keld-ipc lib.rs/frame.rs | TS transport copy and embedded template; all constants/discriminants checked against independent literals. |
| Link-terminal state | GH527 and canonical TS transport | STATE and TERMINAL_CODES mapping; public terminal error and retained-frame ordering tests. |

These paths identify owners; channel numbers and allocation rows remain in channel_table.rs,
not copied here. GH1100 must discover additional consumers beyond this starting set. Full exact
file/copy inventory is an approval prerequisite, not an assumed result of this document.

The private renderer/native envelope and its host-stamped source identity remain with KEL142,
GH542 and GH1095. Do not confuse that boundary with ordinary kipc framing or infer permissions
from an opaque renderer payload. Lifecycle and process exits remain GH1103 terminology.

### 4.3 Settlement and failure ownership

A successful authenticated link may carry one Reply or declared Err for one pending Call.
The receiver validates before a waiter consumes it. A terminal transport failure can leave no
peer response; the caller still settles at most once through its existing terminal owner.
Exactly-once local settlement is not exactly-once execution, rollback or effect certainty.

Preserve CallError's two strings and the single write_call_error path for privileged errors.
Ordinary valid CallError answers one call without inherently killing a link. Lifecycle-selected
generation retirement or Quit can intentionally send an ERR then close; the closure is the
separate lifecycle decision. IpcError denotes transport/session failure, not a new business error.

The TypeScript errorFromErrFrame helper alone is not the live WorkerLink behavior:
WorkerLink validates the payload first, terminalizes malformed input, and returns the selected
terminal error. GH1101 owns structured error properties; neither specification merges those paths.

### 4.4 Caller outcome table

| Class | Bun-to-host observation | Renderer-to-Bun observation and owning rule |
| --- | --- | --- |
| Reply | Valid matched bytes decoded by the channel's response owner | Current bridge delivers its matched result; payload success proves only that API's declared effect. |
| Registered application error | Valid CallError on an ERR-declaring waiter; use that code's meaning | Current bridge's registered/sanitized error; public code shape and prefix follow approved GH1095 revision, not this table. |
| Session fault | IpcError, malformed frame/ERR, authentication or receive-policy failure terminalizes the relevant link | Bridge/lifecycle owner settles or retires its affected requests; no inferred not-dispatched result. |
| Link terminal | IPC-022/025/026/027 mean link close, Worker loss, overflow or abandoned-call saturation under GH527 | Bridge translates only through its owned terminal path; numbers do not become renderer retry policy. |
| Definitely not dispatched | Requires the actual channel's proof that no handler/write admission happened, not any generic error | Proposed WV-012 and API-002 classifications belong to GH1095; exact r2 approval and implementation remain pending. |
| Outcome unknown | A failed/partial write or later link loss may follow execution; preserve the owning contract | Proposed WV-013 and current broader WV-011 behavior must be recorded separately under GH1095. |
| Reply deadline | Existing caller-deadline contract; late response cannot settle twice, and host work may continue | GH1097 owns the future renderer deadline, local terminal action and retained-work/recovery proof. |

Codes in this table are references to existing owner meanings, not new allocations.
No automatic retry follows from a bare code-prefix or class label. Adopt exactly the approved
GH1095 resend/idempotency/status-check rules when that version is authorized, including their
restrictions during retirement/shutdown. A timeout cannot grant fresh authority or replay work.

### 4.5 One version rule

Proposed canonical published owner: architecture 02 section 2. After approval, crate AGENTS,
onboarding section 14 and CallError's wire note link to it instead of restating variants.

The canonical rule preserves the current contract: frame layout, kinds, flags or ordinary shared
HELLO/handshake changes require PROTOCOL_VERSION bump and wire review. Payload-field changes need
their public-API/schema review, not an automatic frame-version bump. A separately versioned
protocol needs its own disjoint pre-handshake endpoint, owner spec and cross-protocol refusal tests;
a shared endpoint does not escape the global version gate.

This governance migration changes none of those bytes or rules and keeps version 2. Incompatible
payload pairs remain a deployment/consumer compatibility concern even when HELLO version matches.
A no-version-bump statement is not evidence that arbitrary payload edits are interoperable.

### 4.6 Reuse and migration

Extend the existing source-parity and independent-vector checks; do not copy a parser or centralize
domain schemas merely to obtain a single file. GH1104 chooses generation versus targeted checks
after measurement. Each migration moves its real Rust/TS consumers and artifacts together with
unchanged behavior. Generated output is regenerated, never patched by hand.

Keep authentication, guard dispatch, window/document generations and pending correlation state in
their current owners. No additional process, queue, allocator, capability or runtime registry.
Cold checks may read sources; runtime traffic keeps the existing compiled constants/codecs.

## 5. Boundaries

Future edits are limited to canonical contract documentation and the existing parity/vector
checks after owner approval. Any later emitter or parser implementation needs a separate scoped
issue and gate. Do not edit reserved kinds into live status, redefine GH1095/GH1097 outcomes,
change channel allocation, add payload fields or alter another owner's active implementation.

This PR adds only this specification and related metadata; no wire or runtime change.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Reconcile GH1100's actual copy/consumer census and current predecessor approvals; freeze
the frame/receive/outcome matrix as current versus proposed evidence.

### T2

- [ ] Extend existing parity tests to all frame constants/discriminants and terminal mappings,
with one-at-a-time source mutations and independent literal vectors.

### T3

- [ ] Add unchanged CallError and per-channel payload vector proof through actual Rust/TS
consumers, including malformed, trailing, wrong-channel and wrong-correlation cases.

### T4

- [ ] Consolidate version-rule links under architecture 02 without changing its semantics;
regenerate included documentation with the current generator.

### T5

- [ ] Exercise caller outcomes and authenticated-routing controls; retain GH1095/GH1097 cells
pending until their exact approved implementation, then publish reviewable evidence.

## 7. Test plan

AC1–AC4 are source/byte contracts. Mutate every Rust discriminant individually and confirm the
existing TS check fails; also mutate both producer and consumer together so independent literals
still catch drift. Test 0, exact limits, limit+1, truncated varints, invalid UTF-8 and trailing bytes.

AC5–AC6 exercise real transport requests and wrong-identity refusals, counting handler entry,
write attempt and caller settlement independently. AC8/AC10 inject ERR followed by EOF and Worker
failure schedules with explicit barriers, keeping first-terminal precedence. AC7 checks owner
links and version negative controls; AC9 rejects evidence that treats a draft as approval.

Planned commands include existing Rust IPC/CLI tests and canonical Bun transport/codegen checks.
No local tests, native execution, runtime mutations or machine closeout ran in this GitHub-only
documentation session. Readback and hosted docs gates do not establish implementation acceptance.

## 8. Review gates triggered

Future public API and wire-protocol review verify unchanged contracts/bytes. Unsafe, permission
model and dependency addition: none. Any discovered need for a byte change leaves this scope and
requires its owning approved revision; it cannot be hidden in a governance/tooling migration.

## 9. Perf impact

No runtime effect from this document. Future checks run in cold tooling. Measure their CI/build
time under GH1104; preserve no runtime registry lookup, new allocation or async hot-path machinery.
No throughput, latency or memory improvement is claimed.

## 10. Open questions (block implementation)

1. Approve GH1100's complete consumer/copy census and the exact receive/outcome matrix.
2. Obtain the exact current GH1095 and GH1097 decisions; until then their proposed cells stay pending.
3. Approve architecture 02 as the one version-rule owner and the unchanged-byte migration evidence.
4. Select enforcement implementation under GH1104 after its measurements; no tool is preselected.

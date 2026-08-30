# Spec: kipc receiver semantics and absolute admission deadlines
Status: approved
Linear: KEL-133 · Owner: GYLDLAB · Updated: 2026-08-30

## 1. Goal & non-goals

KEL-133 makes one `keld-ipc` receiver abstraction the source of truth for v0
frame/session semantic validation and absolute transport, frame, and call deadlines.
Every authenticated structured receiver rejects reserved or ambiguous frames before a
handler, guard, or broker can observe them. Rust and TypeScript consumers replay one
canonical hostile-vector corpus. This contract preserves every currently valid v0 byte
sequence and makes already-reserved invalid combinations fail explicitly.

Non-goals:

- no frame-layout, `FrameKind`, flag-definition, handshake, or protocol-version change;
- no new channel table, shared memory, credit window, generic bridge API, or async runtime;
- no principal minting, permission evaluation, handler policy, or filesystem resource
  decision;
- no KEL-130 retained-handle, path-resolution, special-file, or bounded filesystem-I/O
  implementation;
- no KEL-102/T3, KEL-136 transport, or KEL-97 shipping-link implementation;
- no claim that a documentation-only T0 task changes live product behavior.

## 2. Spec refs

- `docs/architecture/02-ipc.md` §2: v0 frame layout, HELLO, session, correlation, and
  persistent reader contract.
- `docs/architecture/02-ipc.md` §4: validation at trust changes.
- `docs/architecture/02-ipc.md` §6: callback/state-machine hot path and bounded queues.
- `docs/architecture/02-ipc.md` §7: I/O failures, existing frame-wide persistent-reader
  clock, and the ordinary-reader byte-trickle gap.
- `docs/architecture/03-security.md` §2: host mediation and guard-before-handler trust
  boundary.
- `docs/architecture/06-runtime-and-tooling.md` §1: supervised Bun process boundary.
- `docs/onboarding/04-wire-formats-and-contracts.md` §14: public payload changes versus
  protocol-version changes.
- `docs/specs/kel101-windows-named-pipe-dacl.md` §4: pre-authentication error taxonomy
  consumed by transport adapters.
- `docs/specs/kel102-host-guard-enforcement.md` acceptance criteria 6–7: the future
  privileged filesystem receiver consumes this contract before guard dispatch.

This spec resolves the joint KEL-130/KEL-133 decision packet approved in Linear comment
`7deffd67-a1cc-4813-94ac-4d131caca2eb`, digest
`f81a1a180192a87e531519c9c0f6d0c16d7de76577849317de765e9fed89a5b5`.
It does not deviate from the architecture. The implementation PR updates architecture
02's current-state text only as far as the shared validator and clocks become live.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given any decoded v2 header and a named live session class, when the receiver admits
   it, then the existing envelope cap first returns `IpcError::PayloadTooLarge`
   (`KELD-IPC-004`) for `len > MAX_FRAME_LEN`; otherwise one `keld-ipc` semantic
   validator returns a typed validated frame or `IpcError::Protocol`
   (`KELD-IPC-005`) before payload allocation/decode or handler dispatch.
2. Given a structured post-HELLO `CALL` with correlation `0`, `FLAG_RAW`, any unknown
   flag bit, or the wrong channel, when a privileged receiver reads it, then the link
   closes with host-visible `KELD-IPC-005`, the peer receives no handler reply, and the
   handler-effect counter remains zero. Given valid header semantics but malformed or
   trailing postcard payload bytes, the same no-effect rule holds and the existing
   codec classification remains `KELD-IPC-003`.
3. Given a valid structured `CALL` with flags `0`, its session's declared nonzero
   channel, a nonzero correlation, and an exactly decodable payload, when admitted, then
   the handler receives it once and its correlated `REPLY` or `ERR` remains valid.
4. Given a HELLO candidate, when admission validates it, then `kind=HELLO`, flags,
   channel, and correlation are zero and payload length is exactly 32 before token
   comparison. A semantic mismatch is `KELD-IPC-005`; an exactly shaped but foreign
   token remains `KELD-IPC-007`. Neither result discloses the token or link string.
5. Given a reply-side session awaiting correlation `N`, when it receives a frame, then
   only the declared `REPLY`/`ERR` kind, flags `0`, declared channel, and correlation
   `N` can satisfy that waiter. An `EVENT` uses flags `0`, its declared channel, and
   correlation `0`. An unexpected semantic frame is `KELD-IPC-005`, not an unrelated
   waiter completion.
6. Given an absolute generation/admission deadline `D`, when peers connect, disconnect,
   retry, or drip header/token bytes, then every accept and read consumes the same `D`.
   A 100 ms fixture cannot remain in admission for approximately 800 ms by performing
   eight shorter operations; expiry is terminal for that generation.
7. Given a frame whose first byte arrives at `T`, when the remaining header or payload
   stalls, then the whole frame expires at the earlier of the session/call deadline and
   `T + APP_LINK_IO_DEADLINE`, returning `KELD-IPC-006`. Per-receive timeout renewal
   cannot extend either absolute clock.
8. Given an idle persistent session with no frame byte received, when short reader polls
   expire, then it remains live until shutdown or its separately declared session/call
   deadline. Idle polling is not a started-frame timeout and cannot busy-spin.
9. Given `KELD-IPC-001`, `002`, `004`, `005`, `006`, or `007` during admission or a
   session, when the receiver records the result, then the host-visible class/code,
   peer-visible close or `ERR`, session continuation/closure, and retry eligibility
   match §4. No failure is swallowed, relabeled as authentication, or retried after a
   partially consumed frame.
10. Given the canonical KEL-133 vector corpus, when Rust `keld-ipc`,
    `@keld/electron`, the generated hello scaffold, and later KEL-136 consumers run it,
    then they agree on header fields, accept/reject result, error code, and
    continuation. The corpus has one owner and one digest; consumers must not copy its
    semantic table.
11. Given all pre-KEL-133 positive v0 vectors, when the shared validator is inserted,
    then their encoded bytes and results remain unchanged. `PROTOCOL_VERSION` remains
    `2`; changing any previously accepted valid bytes, header shape, kind, flag
    definition, or handshake fails compatibility review and requires a separately
    approved protocol decision.
12. Given a raw-byte fuzz input, when header/session decode runs, then it terminates
    within the harness bound without panic, unbounded allocation, handler effect, or
    credential disclosure. Every retained fuzz failure is minimized and also becomes a
    deterministic semantic regression.

## 4. Design

### First principles and reuse decision

This change crosses the IPC trust boundary but does not change handle, crash, or
principal ownership. It centralizes rules that are currently split across
`FrameHeader::decode`, HELLO helpers, echo/lifecycle loops, and TypeScript consumers.

| Atom | Owner and boundary | Input → output | Failure and direct observable | Independence |
|---|---|---|---|---|
| Header integrity | existing `FrameHeader::decode` | 16 bytes → syntactically decoded header | bad magic/version/kind → existing `002` class | no session policy |
| Envelope bound | existing `MAX_FRAME_LEN` check | syntactic header → bounded declared payload length | declared envelope over maximum → existing `004` before allocation | no session policy or payload decode |
| Session semantics | new shared `keld-ipc` validator | decoded header + declared session policy → validated frame | reserved kind/flags/channel/correlation/length → `005`, zero handler effects | token authentication and payload codec remain separate stages |
| Authentication | existing HELLO/token owner | exactly shaped HELLO + host token → admitted link | foreign token → `007`, redacted | cannot select principal or handler |
| Payload shape | declared channel codec | validated structured frame + payload → typed request | malformed/trailing payload → deterministic protocol rejection before handler | resource authorization is later |
| Absolute clocks | shared receiver/admission state | one host-minted `Instant` deadline + partial reads/retries → frame or `006` | byte trickle or retry cannot renew time | filesystem-operation completion starts only after admission |
| Authorization | `keld-guard`/privileged dispatch | admitted typed request + trusted principal/snapshot → allow/deny | registered guard error and no denied side effect | does not reinterpret wire validity |
| Filesystem work | KEL-130 | allowed typed operation → bounded OS result | native typed failure | no socket/frame deadline |
| Evidence | KEL-133 corpus | exact bytes/policy/result rows → Rust/TS/fuzz assertions | digest/result disagreement fails | adapters consume rather than own semantics |

Existing facilities evaluated:

- `FrameHeader::decode` remains the syntax owner; extending it with session rules is
  rejected because a header is not valid or invalid without direction and session
  class.
- `read_frame_interruptible` already owns a correct started-frame wall clock and is
  extended/reused; a second reader or async runtime is rejected.
- `BootstrapListener` already owns host-minted generation deadlines and bad-peer
  reaccept; adapters must carry its absolute deadline rather than mint per-peer clocks.
- lifecycle, echo, guard-dispatch, and TypeScript local checks are consumers to migrate,
  not parallel policy owners.
- `IpcError` and registered `KELD-IPC-*` codes remain the only transport/session error
  taxonomy.

No rewrite is justified. T1 extends the owning abstractions and deletes duplicated
receiver rules after each consumer is covered. Compatibility fallback is not required:
the newly rejected combinations were already reserved/invalid, while every positive
legacy vector must remain byte-identical. No performance claim is made; the hot-path
acceptance is no new steady-state allocation and the existing IPC budgets remain the
regression guard.

### Session-policy model

T1 may refine names, but it must preserve this ownership shape:

```rust
/// Static semantic contract selected by the host for one receiver state.
pub struct ReceivePolicy {
    pub direction: Direction,
    pub phase: SessionPhase,
    pub channel: ChannelId,
    pub payload: PayloadMode,
    pub expected_corr: ExpectedCorrelation,
}

/// Header whose reserved fields are valid for the selected policy.
pub struct ValidatedFrameHeader(FrameHeader);

pub fn validate_received_header(
    policy: &ReceivePolicy,
    header: FrameHeader,
) -> Result<ValidatedFrameHeader, IpcError>;

/// One monotonic clock carried across accept, retries, header and payload reads.
pub struct AbsoluteDeadline(Instant);
```

`ReceivePolicy` is host-selected trusted state. No frame chooses its policy, session
class, principal, or payload codec. `ValidatedFrameHeader` is the only header type a
privileged dispatch adapter accepts. Construction remains private to `keld-ipc` except
for documented read-only accessors needed by consumers. The receiver applies the
existing `MAX_FRAME_LEN` check before constructing this type, so an oversized envelope
remains `KELD-IPC-004`; policy-specific exact-length mismatches remain
`KELD-IPC-005`.

The validator is a synchronous allocation-free decision over fixed-size values. It
does not decode channel payloads, call the guard, write replies, close sockets, or own
handler lifecycle. The receiver owns those transitions and records the exact outcome.

### v0 semantic table

These are the live structured cases T1 must cover. A declared raw/bulk policy is future
work; `FLAG_RAW` is invalid for every row below.

| Receiver state | Allowed kind | Flags | Channel | Correlation | Payload rule | Failure action |
|---|---|---:|---:|---:|---|---|
| server before auth | `HELLO` | `0` | `0` | `0` | exactly 32 token bytes | `005` for shape; `007` only for foreign exact-shape token; close/reaccept while generation deadline remains |
| client awaiting server HELLO | `HELLO` | `0` | `0` | `0` | exact host-token bytes | `005`/`007`; close |
| host echo receiver | `CALL` | `0` | `ECHO_CHANNEL=1` | nonzero | exact `EchoRequest`, no trailing bytes | `005`; close; zero echo handler effect |
| echo caller waiter | `REPLY` or declared `ERR` | `0` | `1` | exact outstanding id | exact response/error codec | `005`; close; waiter fails |
| host lifecycle receiver | `CALL` | `0` | `LIFECYCLE_CHANNEL=3` | nonzero | exact `LifecycleRequest` | `005`; close; zero lifecycle effect |
| app lifecycle event receiver | `EVENT` | `0` | `3` | `0` | exact `LifecycleEvent` | `005`; close |
| app lifecycle reply waiter | `REPLY` or `ERR` | `0` | `3` | exact outstanding id | exact response or `CallError` | `005`; close; waiter fails |
| privileged FS receiver (future T3 consumer) | `CALL` | `0` | host-declared FS channel | nonzero | exact request, no trailing bytes | `005`; close; zero guard/broker effect |

`PING`, stream, cancel, grant, event, reply, and error combinations not declared by the
selected policy are invalid even though their kind byte is syntactically known. Future
live session classes extend the table through the same owner and review; handlers must
not accept a kind merely because `FrameKind::from_u8` recognizes it.

Unknown flag bits are every bit outside the selected policy's allowed mask. For all
structured v0 rows the mask is zero. This ruling does not redefine `FLAG_RAW`; it makes
its existing structured-session prohibition explicit.

### Deadline model

The host mints absolute monotonic deadlines. Relative durations are converted once at
the owning boundary with checked arithmetic; overflow is `KELD-IPC-006`, never
blocking forever.

- Generation/admission deadline: created with the bootstrap generation and carried
  across listener accept, bad-peer cleanup/reaccept, header, payload, and HELLO token
  verification. It is terminal when expired.
- Started-frame deadline: created when the first byte of a frame is observed and is
  the earliest of `first_byte + APP_LINK_IO_DEADLINE`, the generation/admission
  deadline during pre-authentication, and any enclosing session/call deadline.
- Idle persistent-reader polls: observe shutdown and do not create a started-frame
  deadline before a byte arrives.
- Call deadline: where a session declares one, the same absolute instant follows the
  outbound call and matching reply wait. Drain or partial-read progress cannot renew it.
- Filesystem completion: KEL-130 starts its own bounded OS-operation clock only after a
  valid frame, payload, principal, and guard allow reach the broker. It cannot extend or
  replace a KEL-133 clock.

Every blocking OS wait is capped by the remaining duration to the relevant absolute
deadline. A timeout is not a retry after partial consumption. Transport adapters may
use shorter polls for cancellation, but each poll recomputes remaining time from the
same absolute instant.

### Failure and continuation table

| Condition | Host-visible result | Peer-visible result | Link/retry rule |
|---|---|---|---|
| EOF/non-timeout I/O | `KELD-IPC-001` | close/local I/O failure | close; pre-auth may reaccept if generation deadline remains |
| bad magic/version/kind | `KELD-IPC-002` | close | close; pre-auth may reaccept |
| structured payload codec/trailing bytes | `KELD-IPC-003` | close, or correlated `ERR` only where an approved live session explicitly permits request errors | never invoke handler |
| payload envelope over maximum | `KELD-IPC-004` | close | close; pre-auth may reaccept |
| invalid kind/flags/channel/correlation/declared length | `KELD-IPC-005` | close | never invoke handler; pre-auth may reaccept |
| absolute transport/frame/call expiry | `KELD-IPC-006` | close/local timeout | partial frame is terminal for the link; generation expiry is terminal for admission |
| exact-shape foreign HELLO token | `KELD-IPC-007` (redacted) | close/local I/O failure | pre-auth may reaccept while generation deadline remains |
| valid privileged request denied by guard | registered `KELD-GUARD-*` `CallError` | correlated `ERR` | session remains live; zero broker side effect |

The receiver records only bounded semantic metadata: phase, kind, flags mask class,
channel, zero/nonzero or matched correlation class, declared length, code, and terminal
decision. It never logs payload bytes, token, full endpoint, or `KELD_APP_LINK`.

### Canonical vector corpus

T1 adds one test-only, line-oriented corpus at
`crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv`. Its format is closed and
versioned in the first row. Each later row contains:

```text
id  policy  header_hex  payload_hex  expected_code  link_action  handler_effects
```

Tabs are separators; lowercase hex is canonical; `-` represents an empty value. The
fixture contains no secret or OS path. Rust parses it with `std` in tests; Bun consumers
load the same repository file. No runtime JSON/parser dependency is added. The fixture's
SHA-256 is printed by both test suites and recorded in the T1 artifact. KEL-136 and
future generated packages consume this file or its owning generated successor; they do
not copy rows or constants.

The first corpus covers positive legacy HELLO/echo/lifecycle rows plus bad
kind/flags/channel/correlation, zero/max/max-plus-one payload envelope, truncated header,
split header/payload, trailing payload, wrong token length, foreign token, and deadline
traces. Timing traces contain ordered actions and an absolute virtual/fixture clock;
real socket integration independently proves the same outcome without sleeps.

### Capabilities and manifest

None. The validator runs before permission evaluation and cannot grant authority.

### Wire/protocol changes

No wire-format change and no protocol-version bump. This is an explicit wire-behavior
review gate because receivers newly make reserved invalid combinations fail at one
shared boundary. Positive legacy vectors prove the accepted v0 byte set is unchanged.

### Platform notes

- Unix domain sockets and Windows loopback TCP use the same semantic validator and
  absolute-clock state. Transport-specific error acquisition stays in the adapter.
- Windows clone shutdown does not wake a blocking local read; bounded reader polls must
  recompute remaining absolute time and observe cancellation.
- Unix `SO_RCVTIMEO` may surface expiry as `WouldBlock`; Windows may surface
  `TimedOut`. Both map to the same clock result after remaining-time evaluation.
- KEL-101 named pipes later consume this contract; DACL and overlapped-I/O ownership
  remain KEL-101.

## 5. Boundaries

Implement T1 in:

- `crates/keld-ipc/src/frame.rs` for syntax only and a new or existing receiver module
  for semantic policy/validated header/absolute deadline;
- `crates/keld-ipc/src/link.rs` and `bootstrap.rs` only to thread the shared absolute
  clock through existing readers/admission;
- current echo, lifecycle, and guarded receiver adapters only to consume the shared
  validator and delete their duplicate checks;
- `packages/@keld/electron` and the generated hello scaffold only as vector consumers
  and to enforce matching reply/event semantics;
- one canonical fixture and fuzz target owned by `keld-ipc`;
- architecture 02 current-state text and generated documentation after behavior lands.

Must not touch:

- `keld-guard` policy or manifest schema;
- KEL-130 filesystem handles/path/I/O implementation;
- principal minting, role registry, sandboxing, renderer bridge, shared memory,
  packaging, or update code;
- workspace dependencies unless a separately approved dependency review proves a
  std-only implementation insufficient;
- protocol layout/kinds/flag definitions/handshake/version without a new approved spec.

## 6. Tasks (each approximately one PR; ordered)

- [x] **T0 — contract freeze:** approve this spec, the exact owner partition, v0
  semantic table, deadline model, vector format, failure actions, and review gates. No
  product code or OS acceptance.
- [ ] **T1a — deterministic validator and corpus:** land failure-first Rust tests,
  shared allocation-free semantic validator, canonical TSV corpus, exact positive
  compatibility vectors, and raw-byte fuzz target/regressions. No consumer may retain a
  second semantic table.
- [ ] **T1b — absolute admission/frame/call clocks:** extend the existing reader and
  bootstrap owners so one monotonic deadline survives byte drip and bad-peer retries;
  prove with real sockets, child processes where needed, and no sleeps.
- [ ] **T1c — consumer convergence:** migrate echo, lifecycle, guarded receiver,
  `@keld/electron`, and hello scaffold to the shared contract; run the same corpus in
  Rust and Bun; delete duplicated checks; prove zero handler effects for hostile
  authenticated frames.
- [ ] **T1d — artifact and architecture update:** run full gates/fuzz replay, obtain
  wire-behavior and any triggered public-API review, update architecture 02's live
  claims, and publish one passed `keld.execution-artifact/v1` for `KEL-133/T1` whose
  landed head includes T1a–T1c.

T1a–T1d may use multiple reviewable commits but produce one atomic implementation PR
and one landed artifact. A partial merge cannot unlock KEL-130, KEL-102/T3, or KEL-97.

## 7. Test plan

| Acceptance | Test and independent oracle |
|---|---|
| 1–3 | Unit table over exact headers plus an integration receiver effect counter. Mutating header validation to unconditional success fails zero-correlation, RAW, unknown-flag, and wrong-channel cases. A separate malformed/trailing-payload mutation bypasses or weakens the postcard decode boundary and must fail exact `KELD-IPC-003` plus zero-handler-effect assertions; payload-codec proof is not attributed to the header validator. |
| 4 | Existing real HELLO link plus raw clients for reserved fields, 0/31/32/33-byte payloads and foreign token. Host observer code and absence of any reply/token bytes are the oracle. Collapsing shape failure into `007` fails. |
| 5 | Reply/event waiter fixtures send wrong kind/channel/correlation and assert the intended waiter never completes before the typed session rejection. Removing exact-correlation matching fails. |
| 6 | Real listener with a 100 ms generation deadline and a byte-drip child; elapsed monotonic bound, terminal state, joined child, removed locator, and next fresh-generation success are asserted. Resetting the deadline on accept/read/retry must make the named test exceed its bound and fail. |
| 7–8 | Existing interruptible-reader tests are generalized: idle polls remain live; partial header and payload expire from first byte. Resetting the started-frame clock per read fails while the idle case remains independent. |
| 9 | One table row per `001/002/003/004/005/006/007` asserts host code, peer result, close/reaccept decision, redaction, and next allowed operation. Mapping all pre-auth errors to `007` fails the non-authentication rows. |
| 10–11 | Rust and Bun load the same TSV path, print the same fixture digest, and compare exact results. Positive legacy frame bytes are golden oracles. Copying/changing one consumer rule without changing the corpus fails that consumer. |
| 12 | `cargo-fuzz` raw header/session target with retained corpus; every discovered input becomes a fast deterministic test. Removing max-length or kind validation must be caught by deterministic and fuzz replay tests. |

Boundary cases: zero and nonzero correlation; flags `0`, `FLAG_RAW`, each other single
bit, and all bits; channel `0`, declared, and wrong; payload `0`, exact, maximum,
maximum plus one; truncated/split header and payload; EOF; idle; byte drip; shutdown;
wrong token; retry before generation expiry; expiry; and a clean next session.

Anti-flake rules: bind port `0`, use owner-created temporary directories, await explicit
listener/byte/close/join conditions, and use timeouts only as kill switches. Drip and
crash actors run in child processes and report status/signal/cleanup. No test sleeps to
synchronize. Platform code is claimed only on the real platform where it ran.

T0 documentary negative controls use a checked query over this spec: removing the sole
owner, zero-correlation/RAW/unknown-flags ruling, absolute-deadline non-renewal,
`KELD-IPC-005/006`, canonical TSV owner, or no-version-bump condition must make the
query fail. These checks validate contract completeness only, not T1 behavior.

## 8. Review gates triggered

- unsafe: none for T0; forbidden for the framing implementation under current crate
  rules;
- public API: T1 triggers this only if the shared receiver types/accessors become public
  outside `keld-ipc`; prefer crate-private consumers where possible;
- permission model: none;
- dependency addition: none expected; any addition requires separate approval;
- wire protocol: **wire-behavior review required** for the reserved-invalid rejection
  boundary; no layout/version change is approved.

## 9. Perf impact

T0: none. T1 adds a fixed-value validation branch set and deadline comparisons on the
kipc hot path. It must add no steady-state allocation, queue, runtime, or payload copy.
Run the existing small-call RTT benchmark if available at implementation time and
report it without claiming improvement. A regression greater than 5% requires a written
waiver and attributed benchmark per architecture 01 §5.

## 10. Open questions

None. T1 keeps the validator implementation owned by `keld-ipc` and uses the narrowest
consumer API that existing crate boundaries permit. Any exported type or accessor is a
public-API review gate on the T1 candidate, not an unresolved T0 design question.

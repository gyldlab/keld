# GH-451 Q3 — proposed IPC amendment for joint owner review

Review-only proposal; no numeric allocation, implementation authorization or
production edits. Remote refs observed 2026-11-26: main
`2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`, #1581
`db74c84673d912c720320470e04f84468e760589`, #1432
`c779aa2ac08270911b86cdbe3ebcb4e5f6dbcf2a`. GH451-side Q3 decision:
#1432 §4.3 lines 273–295 and Q3 lines 417–420. W2 local fixture
`615080a967f8d750bf7cf0f6ac58a478f39193cc` is model evidence only.

## Existing allocations and skew

| Family | Current main | Exact #1581 and inspected expanded stack |
| --- | --- | --- |
| Lifecycle request | Quit 0, Subscribe 1 | Quit 0, Subscribe 1, Init 2 |
| Lifecycle response | Quit 0, Subscribed 1, SubscribeRefused 2 | Quit 0, Subscribed 1, SubscribeRefused 2, Initialized 3 |
| Lifecycle event | Ready 0, LastWindowClosed 1 | Ready 0, LastWindowClosed 1, QuitRequested 2 |
| Channel allocations | echo 1, fs 2, lifecycle 3, auth 4, window 5 | same + dialog 6, compat-control 7, menu 8 |
| Closed launch facts | inspect approved upstream before change | schemas 1–5, 6, 7; Windows AC06 #1530/#1531 adds 8 |

Source: `crates/keld-ipc/src/lifecycle.rs` lines 24–87, 238–336
(#1581), `channel_allocations.txt`, `channel_table.rs`; generated
`packages/@keld/kipc/src/runtime-process.generated.ts` lines 11–37.
Subscribe refusal reasons remain UnknownChannel=0, NotSubscribable=1,
GuardedNotSupported=2, RoleNotAdmitted=3. Channel zero is handshake only.
#1581 PROTOCOL_VERSION=2 (`transport.ts:611`).

Compared exact live PR heads: #1447 c70dfef71c, #1507 b5884a140f,
#1546 698537a35b, #1548 dd42e19cb8, #1576 75999112bf,
#1582 c15dcbfda1 and #1602 c421dc83ac have the #1581 lifecycle/ledger;
#1530 b821757660 and #1531 1a8325d426 have the same wire entries plus
WindowsOriginalDevelopmentHostLaunchFacts schema 8.
#1477 a1aeb455fa has the shorter main lifecycle family and launch facts
through 7. These are inspected heads, not a proof that all pending work
is compatible. Live open PR listing returned 200 entries; wider
enumeration hit the connector's retained terminal-session limit (64).

Do not allocate from apparent free positions in main: Init request 2,
Initialized response 3 and QuitRequested event 2 already exist in the
pending stack. GH523 (#1432, gh523-host-authorized-app-exit.md §4 and Q3)
requests a generated lifecycle allocation, expressly without a numeric
tag. It competes for the next request/reply extensions, but has not
reserved a numeric value. GH563 already owns scoped event 2. Do not
reuse schema 8 for negotiation or interpret schemas 5/7/8 as feature
support. GH508 §4.4: append only at merge, no advance reservations;
reconcile all concurrent allocations with joint owners and refresh refs
before actual generation. Keep channel 3, all existing tags and the
16-byte header/ordinary HELLO unchanged. Payload-only extension may
retain protocol 2 after wire/public-API review; changing ordinary HELLO
or header instead mandates a global version bump under IPC AGENTS.md.

## Amendment text: immutable selection and authenticated binding

Use the existing trusted Host bootstrap and one authenticated Main link.
Propose a separate closed parse-time selector
`__KELD_CONDITIONAL_QUIT_V1__` (name proposed, not allocated wire data).
Default absent/false. Host sets true only for the exact verified
Host-selected preload/runtime graph that implements this contract and
retains the selection in the corresponding generation admission owner.
The role's captured immutable selector is solely a compatibility hint;
Host independently validates its own selected graph, authenticated
Main identity, current generation, running state, native original
selection and Ready plus accepted window/compat subscriptions.

Use the current Init/Initialized path unchanged: Init is first
post-HELLO CALL at correlation 1 with bounded runtime facts, and its
ACK is not a capability grant or native Ready. After this admitted
Init and genuine Ready plus replay/subscription barrier, a positively
selected role issues the private Bind operation below on the existing
lifecycle channel. Host returns Bound only from that exact current
generation; store the result privately for the lifetime of that link.
Do not gate legacy public Ready or user entry on a new speculative
handshake. Conditional choreography becomes available only after Bound.
No old Host/new role (selector absent), new Host/old role (graph not
selected), or unselected role sends Bind, conditional frames or starts
the new conditional window-close choreography. Mismatch in a positively selected pair stays unavailable or follows the
existing terminal protocol-error policy. Bind uses the single control
slot before any effect. Lost or mismatched Bind stays unavailable; do
not blindly rebind, reconnect or downgrade to forceful Quit. A late
exact Bound may activate only after qualified control-slot proof.
A selected but unbound role refuses even a zero-window conditional Quit.
Never use mutable env, package versions, process.versions, schema 8,
Ready, Subscribe success alone or an unknown-tag probe to infer support.
No new host event is required; preserve scoped QuitRequested exactly.

## Amendment text: one conditional payload family

Names below describe closed field shapes, **not** chosen discriminants
or existing code. Append one ConditionalQuitV1 request envelope and one
ConditionalQuitV1 response envelope to the jointly reviewed existing
lifecycle enums, leaving existing variants byte-identical. No second
channel, principal selector, socket, HELLO change or permission grant.

Request family (all are ordinary correlated CALLs):
- Bind (unit): only trusted selected peers use it after the barriers.
- Prepare (unit): original correlation comes from the actual header.
- Cancel { token: nonzero u64 }, Commit { token: nonzero u64 }.
- Status { original: EffectIdentity } (read-only).
- Consume { key: ReceiptKey } (acknowledges observed record consumption,
  never reexecutes or grants authority).

EffectIdentity = { operation: Prepare|Cancel|Commit, correlation:
nonzero u32, token: optional nonzero u64 }. Token absent for Prepare,
present for Cancel/Commit. All fields must agree with the original
admitted request; the current authenticated link/role/generation are
derived from Host state, never supplied as authority by the caller.
ReceiptKey = { original: EffectIdentity, record_serial: nonzero u64 }.
Host alone mints checked nonreused serials scoped to its exact link/
generation. A successful Prepare may use that record serial as the
opaque attempt token; subsequent records get distinct serials. Refuse
overflow instead of recycling. This prevents a delayed consumed-record
ack from matching a new operation that reused a transport number.

Response family:
- Bound, Unavailable (Bind only).
- Recorded { key: ReceiptKey, disposition: Disposition } (direct
  effect response or ordered Status; outer header correlation identifies
  the current CALL; key identifies the original effect).
- Indeterminate { original: EffectIdentity } (Status unable to prove
  the exact disposition, not a nonexecution receipt).
- Consumed { key: ReceiptKey } (exact Consume confirmation).
- ConsumeUnconfirmed { key: ReceiptKey } (no release proof).
- NotAdmitted { original: EffectIdentity, reason: AdmissionRefusal }
  (definitive exact pre-effect rejection, no new record or token).
Disposition is a closed sum: Prepared { token: nonzero u64 },
Retained { reason: RetainedReason }, Cancelled, Accepted, Invalidated,
CompetingTerminal. Select the following closed semantic vocabulary
for the GH451-side proposal; numeric values remain unallocated.

RetainedReason: RegistryNotEmpty (Prepare only, authoritative live,
Opening or Closing) and PreparationInvalidated (Commit only, exact
permanently invalidated preparation). Retained requires proved
noncommit, no surviving preparation, successful owed-work settlement,
current usable link and no independent terminal winner. Choose the
narrow two-reason rule over treating generic nonadmission as Retained.
AdmissionRefusal: Unavailable, ReceiptPending, WrongPhase. These are
pre-effect refusals only when the existing owner proves the exact
new request never executed. They cannot erase a surviving preparation,
overwrite an unresolved record, or by themselves authorize a retry.
If the owner cannot prove the refusal, keep the new request unknown.
The actual typed error mapping and response encoding require IPC/API
owner adoption; do not assign error codes in this draft.

| Request | Legal dispositions (for exact current identity) |
| --- | --- |
| Bind | Bound, Unavailable |
| Prepare | Recorded Prepared or Retained RegistryNotEmpty; CompetingTerminal if separately committed; definitive NotAdmitted |
| Cancel | Recorded Cancelled (including an invalidated token only with proved exact noncommit/no remaining preparation); CompetingTerminal; definitive NotAdmitted |
| Commit | Recorded Accepted or Retained PreparationInvalidated; CompetingTerminal; definitive NotAdmitted |
| Status | Recorded for the original operation, including Invalidated for a now-revoked Prepared; Indeterminate |
| Consume | Consumed, ConsumeUnconfirmed for exact key |

Generic ERR, stale identity, failed/indeterminate drain, role loss or
wrong-operation/malformed response never maps to Retained. A missing
record is not nonexecution. CompetingTerminal is not this attempt's
acceptance or a live-link claim. GH451 Q5 owns exact other-winner
cause/code; preserve that evidence in the lifecycle owner.
No arbitrary string, length-prefixed list or caller-chosen authority.
Proposed conditional body cap 64 bytes (closed postcard fields above);
enforce on the decoded conditional envelope after existing bounded
frame admission, reject trailing/noncanonical/overflow/zero forms.
Do not claim a header-only cap that could distinguish variants before
reading their payload. Codegen must derive shapes/order from Rust and
reject unsupported or conflicting allocation changes.

## Amendment text: receipt ownership, consumption and retirement

Keep one active attempt plus one effect record on the existing current
Host generation (including the last consumed tombstone until replacement
or retirement). Serialize all conditional controls in the same reader.
Record exact admitted operation and disposition under existing
shutdown/current/registry ordering, before emitting the reply. Never
overwrite an unresolved record on a rejected later request. Every next
effect, including Cancel/Commit or queued fresh retry, requires the
previous nonterminal receipt to be consumed and the independent
Worker/main no-late barrier complete. The client sends no new effect
while either slot remains occupied. A defensive Host rejection cannot
release the prior slot or grant another token. Status
can answer only after the original is decided, including invalidation
since a historical Prepare or an independent terminal winner; absent
or mismatched record is Indeterminate unless nonexecution is itself
authoritatively recorded. If an accepted terminal tail prevents
Status, no reconnect or substitute authority: preserve Unknown.

WorkerLink's existing allocator reserves the effect correlation before
posting the write. Include it in both blocking and async allocation
exclusions, regardless of Promise timeout/abandon. One separate control
slot handles Status or Consume with distinct correlation, and is not
reused while it could deliver late. Preserve GH527's ordinary pending,
abandon, ring and MAX_ABANDONED_CALLS (256) behavior; no unbounded
side map, counter reset or bypass of IPC027. A synchronous send failure
after reservation is unresolved unless the owner proves nonexecution.

For a nonterminal Recorded result, private API records the exact
receipt and facade applies its disposition once (Prepared authorization
may be captured but delay will-quit until its receipt is consumable;
recheck known invalidation before callback). The owner sends Consume
with the exact record key only after consuming that result. Under
current-generation/reader ownership Host validates the key and marks
the same record consumed, without changing attempt validity, terminal
cause or replaying effects. Repeat Consume of that exact consumed
record is idempotent while the tombstone survives. Host may replace a
consumed tombstone only at the next legal ordered effect admission or
definitive retirement. A mismatched/stale Consume cannot retire a record
or mint a token. No retention time or pressure eviction.

Consumed must be written through the existing ordered writer after the
original operation can never execute or write another reply and all
preceding control replies have completed. Host ordering alone is not
the no-late proof: the transport Worker must validate the exact
correlated Consumed response using the generated decoder, account for
all earlier frames (including abandoned original/status records), and
publish an ordered quiescence watermark/receipt on its existing ring
or ordered control path. Main drains/applies/discards records through
that watermark before clearing the original and control reservations.
The Worker is the only owner of the pending map; main must not infer
quiescence merely because its own Promise settled or disappeared.
D2 public first qualified close handoff may already fulfill independently
of private receipt consumption. Pending typed error rejects once;
later error is reported through exact attempt custody. Will-veto local
settlement does not authorize a new effect. Launch the single queued
explicit retry only after proved Cancel/nonexecution, consumed/no-late
barrier, current live role and no terminal winner. Unknown rejects the
queued request; subsequent recovery needs new explicit intent.
Only release an identity once both API receipt consumption and that
ordered independent Host/Worker/main proof hold. Existing control
FIFO and ring ordering must be demonstrated, not assumed across
separate ports. This is the smallest explicit retirement barrier;
do not add a parallel transport queue. No general public caller can
mint quiescence proof.

If Status times out, keep its control identity until a real terminal
frame is processed/discarded or a qualified no-late witness is obtained;
no fresh Status while it occupies the slot. If Consume times out,
retain both original and control; a later exact valid Consumed may
finish private custody/retirement without resurrecting an expired
public Promise. Reissue only the idempotent Consume, never the original
effect, after independent proof that the preceding control cannot
arrive late. If such proof is unavailable, remain blocked on conditional
effects until approved lifecycle retirement. This bounds memory, not
wall-clock nonprogress. Q7/lifecycle owner must separately approve
any finite nonprogress retirement trigger and disposition; no timer
is proposed here. Ordinary permitted work retains its existing owner
and restrictions; no claim of live retention after a competing terminal.

Accepted is immediately known irreversible reservation, even if local
fencing throws. Fence the original current link once; do not wait for
Consume/Status before existing terminal tail or cleanup. Known
commit/other-winner evidence belongs to lifecycle owner across link
retirement. Retire link-local reservations only on definitive link
retirement; closed-link identities are never transferred to successor.
Unknown after retirement remains unknown (not noncommit). No replay.

## Implementation owner map (exact #1581 paths)

| Owner/path | Required delta after joint approval |
| --- | --- |
| IPC `crates/keld-ipc/src/lifecycle.rs`, `codec.rs`, `receive.rs`, `link.rs`; existing `channel_table.rs`/allocations | Extend only the payload owner, strict bounded decoding, preserve baseline channel/HELLO/Init. Standalone LifecycleSession in Core refuses new feature; no alternate authority path. |
| KIPC `packages/@keld/kipc/scripts/echo-codegen.ts` (shape whitelist ~259, lifecycle renderer ~2090), generated `src/runtime-process.generated.ts` or one generated lifecycle module under same owner | Generate closed codecs, no handwritten copies; preserve source order, package/scaffold checks and old golden bytes. |
| Bootstrap `crates/keld-host/src/main.rs:653–689`, `crates/keld-core/src/app_session.rs:2508–2533,2759`, `crates/keld-runtime/src/role.rs:1463–1505`, existing staged provider verification and `packages/@keld/api/src/runtime-preload.ts`, `installed-runtime-entry.mjs`, `app.ts:62–81` | Qualify exact new graph, propagate immutable Host selection into admitted generation, capture trusted parse-time hint before app code. No runtime facts/launch-facts authority change. Preserve non-selected launches. |
| Core `app_session.rs:6749,6859,7296,7844,9510–9543,9667`; `app_session/window_router.rs`, `window_registry.rs`, `menu_router.rs:367` | Existing current-generation/lifecycle record, conditional dispatcher, ordered replies/retirement, irreversible Create invalidation, reversible FS drain, shared Q5 terminal winner and existing post-reserve tail. Preserve existing menu intent and Guard. |
| Transport `packages/@keld/kipc/src/transport.ts:3194,3215,3388,3686,3766,3966,4105–4115,4275–4317` | Private pre-write retained reservation, same shared allocator, exact control/result binding, worker pending-map and ring/watermark no-late proof. Keep ordinary abandon and legacy quitAndCloseLink behavior. |
| API `packages/@keld/api/src/link.ts:200–375,557`, `app.ts:372–414,512–526`; W3 facade owner | One captured LifecycleLink, binding after qualified Init/Ready/replay, explicit record consumption and typed recovery; retain normal calls, fence only known Accepted, converge facade/private attempt. |

## Minimum real proof inventory (unrun)

1. Rust lifecycle byte tests + KIPC codegen/runtime-process vectors:
   all existing tags, Init corr1, event2, Subscribe reasons; new bounded
   golden request/reply, exact types, truncation/trailing/zero/overflow,
   malformed and wrong operation. GH508 channel-table/receiver corpus
   and release/scaffold artifact freshness. No new channel.
2. Real old/new bootstrap peers with windows: old Host + new role,
   new Host + old/unsupported role, unselected/wrong generation,
   forged environment/package/schema claim, genuine selected graph:
   inspect actual frames, zero new conditional frames or close choreography
   before Bind; existing Ready order and legacy bytes unchanged.
   Extend `runtime-preload.test.ts`, `runtime-packed-startup.test.ts`,
   `ready-publication.test.ts`, launch-facts/bootstrap tests.
3. Existing Rust `keld-ipc/tests/worker_link.rs`,
   `packages/@keld/kipc/src/worker-link.test.ts`,
   `transport.test.ts` and `test/worker-link-role.ts`: real duplex
   socket, separately hold original/Status/Consume ACK, ring dispatch
   and worker abandon; force wrap/capacity with ordinary/blocking calls.
   Record exact corr/channel before each write, one effect, no alias,
   no release on timeout, late reply discard, stale receipt rejected,
   ordered Consumed + worker/ring proof required. Test IPC027 unchanged.
4. Extend existing Core authenticated fixture tests
   `app_session/tests/router_lifecycle.rs`,
   `worker_link_e2e.rs`, `guarded_router_and_quit.rs`,
   `window_router/authenticated_tests.rs`: observe Host decisions,
   same-link permitted Call/event and retry after Retained/Cancel;
   lose each ACK after exact cut, recover by Status only; absent record
   while original could still execute stays unknown; Create/Exit races,
   malformed/mismatch/retired record, one reserve/cause/tail and owed FS
   replies. Hold reply before callback; no stale Prepared authorization.
5. API/facade actual link tests `link-quit-release.test.ts`,
   `original-quit-generation.test.ts`, existing W3 composed fixture
   and then genuine authenticated link: explicit receipt consumption,
   duplicate Consume, loss/late ACK, no fallback, exact callback once,
   known Accepted despite fence exception, no successor fence.
   Native signed-app and cleanup qualification remains separate Q7.

Negative controls: remove selector, reuse reserved original, release
on timeout/receipt alone, accept stale key, bypass ordered writer or
ring drain, allow a new effect while record unresolved, replay Commit,
erase known terminal cause on fence failure. Each must fail its own
wire/Host/counter oracle. No full build or test run performed by this
read-only design review.

Joint owners still must approve proposed selector, envelope/field
schema, exact Retained reason enum, receipt confirmation mechanism and
retirement contract; only then reconcile and allocate numeric values.
GH523/KEL96/FS Q5/Q6 and Q7 are separate approvals and execution proof.

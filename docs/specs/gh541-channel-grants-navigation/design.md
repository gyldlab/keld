# GH541 design

Part of: [index](index.md)
Owns: §4, including §§4.1–4.9

## 4. Design

### 4.1 Decision-bearing atoms

An atom is one independently falsifiable boundary. A pass in one row does not prove another.

| Atom | Owner / input → output | Failure mode | Independent observable and coupling |
|---|---|---|---|
| Identity | keld-wv native callback + committed document → opaque admission stamp | Page supplies window or principal; old world rebinds | Actual native sender/frame/nonce mismatch yields zero host forwarding. Couples to lifecycle, never to channel spelling. |
| Authentication | Existing host app-link/role registry → accepted Bun session | Old/foreign link accepted after recovery | Real connection generation and ingress records. Separate from navigation generation. |
| Authorization | keld-guard immutable snapshot + trusted window selector + channel → decision | Wildcard/prefix/global-union/app fallback | Exact positive/negative decisions plus distinct ingress observer. Identity cannot be replaced by a grant. |
| Effect boundary | keld-ipc dispatch + core forwarding closure → app-link handoff | Denial reported after bytes already sent | Zero/one app-link writes and handler-entry counts. Transport/parser success is not authorization. |
| Lifecycle/revocation | Existing keld-wv state + native event → live/dead admission | Queued stale request; old reply enters new document | Deterministic race barriers, current document markers, real native callbacks. Must order against final handoff. |
| OS containment | Existing KEL-78/profile admission → native authority boundary | Broker result mistaken for Bun confinement | Separate real OS proof. This review supplies no new containment result. |
| Evidence provenance | Exact source + command + raw outcome → scoped claim | Model/mocked pass presented as native proof | Git blob hashes, captured outputs, mutant identity, and explicit unrun rows. |

### 4.2 Reuse and qualification scope

The approved host-session, guard-dispatch and renderer-bridge contracts are the starting points for this extension. This specification defines the proposed behavior and the evidence its implementing owners must supply. It does not claim that #552 or #542 is implemented, or that the new channel and native lifecycle boundaries are qualified.

Reuse the verified manifest loader, unique-key JSONC parser, immutable host snapshot, guard evaluator, opaque `ScopePermit`, existing dispatcher, existing per-WebView state, and existing authenticated app-link owner. Do not add a second registry/counter in core, a second transport, per-handler policy parsing, or a fallback grant.

The required changes are extensions of those owners. No language rewrite or performance speedup is proposed. Unsupported paths fail closed. Compatibility fallback to broad Electron authority is not permitted.

### 4.3 Exact-literal policy grammar

Illustrative policy only; these names are not a claim about draw.io's extracted channel census:

```jsonc
{
  "$keldWindowChannelGrants": 1,
  "windows": {
    "main": { "channels": ["el:rendererReq", "el:saveFile"] },
    "about": { "channels": [] }
  }
}
```

**Windows Legacy P01 decision.** Adopt the exact marker for the deliberately prepared Windows Legacy first-proof artifact.
This decision qualifies only that artifact. It does not introduce a per-app allowlist in the pure parser.
The stage adds no live renderer route. Other artifacts receive no enrollment, migration or delivery authority from this decision.
Preserve existing parse and admission outcomes for the ordinary and strict-profile entry paths.
P01 does not establish compatibility for other stored manifests, profiles or platforms.
GH499 retains automatic migration ownership.

Record the path and SHA-256 of the final emitted manifest that the Windows Host actually verifies.
Review its exact window keys, literal grants and CSP. Deliberately adopt the integer marker in the authored input.
Use a fresh verified snapshot for each paired probe. In the second input remove only the marker.
Recompute dependent integrity metadata for each input. Both must reach the same Guard decision owner.
At the pure policy boundary, supply the same WebView-shaped query and Host-selected window borrow to both variants.
The approved literal is eligible only with the marker. GH542 separately proves real document authentication.
Its neighboring literal stays denied. The stripped copy denies both. CSP selection is identical.
Capture tagged bytes and frozen GUARD003 denials independently of any wire or Bun-delivery qualification.

Rules for this slice:

- Only JSON integer `1` in the top-level `$keldWindowChannelGrants` field opts into exact channel grants.
- An absent or unsupported marker grants zero incoming channels. Legacy profile selection alone grants none.
- Preserve the independent window response-policy owner's validation and CSP selection for every marker value.
- The selected artifact retains its existing window structure and CSP checks. V0 scalar metadata is historical evidence only.
- Preserve the existing unique-key decoder and whole-input size limit for every marker value.
- With opt-in, `windows`, each window value, and `channels` have the object/object/array-of-strings shapes shown. Omitted fields are empty authority; explicitly wrong types, including `null`, are policy errors.
- Each grant is a decoded, well-formed Unicode string whose UTF-8 bytes begin with ASCII `el:` and have a nonempty suffix. It contains no U+002A `*`, NUL, C0/C1 control, or DEL. Surrogate halves are rejected by the string/wire validation boundary; they are not replaced or normalized into a granted value.
- Match the validated decoded UTF-8 string exactly. Do not trim, case-fold, normalize Unicode, percent-decode, interpret regular expressions, apply path `/**`, or perform prefix/suffix matching. Punctuation other than the explicitly forbidden characters is literal, never a pattern language.
- Reject `el:*`, `el:**`, `el:rendererReq*`, and their JSON-escaped forms after decoding. An opted-in mixed valid/wildcard list rejects the complete policy. This is stricter than loading an inert wildcard.
- Duplicate decoded object keys remain invalid under the existing parser. Repeated identical strings in an array add no authority; preserve deterministic first-match behavior and let migration deduplicate them.
- The existing 64 KiB total manifest limit remains. #540 owns message channel-length and envelope budgets; no larger unbounded channel allocation is implied here.
- The Electron adapter maps an application channel `c` to the canonical name by exactly one `"el:" + c` construction. No repeated-prefix stripping, alias, URL decoding, or case conversion is allowed. keld-ipc receives a generic channel string and has no Electron or codec knowledge.

The host binds a configured window key to a WebView at creation from trusted configuration. The binding stays with that WebView lifetime. A
renderer cannot supply, rename, or select it. A numeric WebView id is not a manifest key. Remote-content admission must still satisfy the
owning navigation/content policy; a permitted cross-document navigation is not permission to expose the application bridge to arbitrary
remote content. If that policy is absent, fail closed. Architecture 03's remote-window `channels: []` rule remains in force.

**First-proof eligibility gate:** before a bind can use a nonempty incoming channel list, the host must establish that the committed
main-frame document belongs to the approved application-document set through its boot/resource containment and native navigation evidence. A
page-supplied URL, a `file:` prefix, same URL text, or a fresh nonce alone is insufficient. Repeat that check for each replacement document.
This review does not claim the gate exists. The proposed narrow owner is **#542**, extending its document-bind admission scope to consume
the existing host-validated application-resource boundary and prove N11. This proposed scope addition requires exact approval in #541 and
the separate #542 spec; no new web.navigate grammar is authorized. Full navigation policy remains [#460 / KEL-79](https://github.com/gyldlab/keld/issues/460).
Without the qualified restricted bind gate or the qualified general policy, channel enablement is NO-GO. A functional application
demonstration alone does not qualify this security boundary.

### 4.4 Query types and dispatch ownership

The logical guard query distinguishes `NativeCapability(operation, resource)` from `RendererChannel(channel, host-bound-window)`. This is a required semantic distinction. #552's separately approved public API selects the narrow Rust representation; this amendment does not fabricate an already-existing constructor or treat a raw `Principal` value as proof of host provenance.

The window grant selector is an immutable borrow/handle resolved from the existing host snapshot and WebView binding. It is not a renderer string, mutable global union, or cached Allow. Existing native capability queries preserve their principal rules. In particular, a grant for `el:fs.read` authorizes only an application message named `fs.read`; it cannot enter the native filesystem broker.

Normative order for renderer-originated channel traffic:

1. Enforce bounded envelope syntax and native sender/frame/content-world checks.
2. Resolve the original document's host-owned stamp and trusted window binding; verify live state. Capture the currently admitted host-owned Bun/app-link generation as part of this work's dispatch identity. If no usable generation exists, refuse admission with the applicable no-effect outcome; do not leave old work eligible to attach to a future link.
3. Validate the generic channel name and retain opaque payload bytes within #540's bounds.
4. At the final app-link handoff, order the live-stamp check against revocation, require the captured Bun/app-link generation to equal the still-current authenticated link, and invoke `dispatch_privileged` on the typed channel query. Never retarget already-admitted work to a successor Bun generation.
5. Only its Allow closure may forward. No listener, Bun frame, native handler, or application-value decoder is reached on a denial.

The final check and handoff form one ordering boundary with navigation/destruction. If revocation wins, no app-link write occurs. If
dispatch already won, the operation may have reached Bun; navigation cannot promise rollback. Drop obsolete delivery, retain the correct
terminal outcome, and never replay automatically. A check before queueing alone is insufficient. Every queue whose consumer can forward
after revocation must retain a revocable host stamp and observe the same owner. This is lifetime validation, not a duplicate permission
matcher.

The final handoff must be bounded and nonblocking, with a defined linearization point: the single instant at which dispatch or revocation
wins. #540 and #542 must freeze this ordering across their existing transport and navigation owners; any later transport queue preserves the
captured identities and that ordering. Do not hold a lock needed by native navigation/destruction over blocking app-link I/O, a Bun wait,
drain, or handler completion. KEL-142's UI-thread progress requirement remains mandatory. A blocked-transport negative test must still
observe native revocation progress.

The diagnostic MCP path may query the shared pure evaluator using a hypothetical host-selected window key; it cannot produce a live admission, export a `ScopePermit`, or call an effect closure. A repository-wide sole-authority-bearing-evaluator claim requires accepted KEL-102/T4 evidence for the designated media integration. This amendment grants no exception to that requirement.

### 4.5 Stable denial contract

For an otherwise admissible request from a valid, current document, with a usable matching app link, whose valid channel is absent from its window list:

```text
Variant: ChannelForbidden { channel }
Code: KELD-GUARD003
Kind: channel_forbidden
Fix: Add `{channel}` to this principal's channels list in keld.permissions.jsonc.
Display: KELD-GUARD003: channel `{channel}` is not granted to this principal. {fix}
```

Preserve the existing `CallError { code, message }` mapping through `From<&DenyReason>`. Do not replace the denial with WV-011 or a bare string. The valid literal in the fix is the canonical `el:` name, not a guessed native capability.

#552's requirement to identify the exact manifest entry is reconciled without silently changing this frozen sentence: host diagnostics and
MCP explain include the trusted JSON pointer `/windows/<escaped-key>/channels`, using JSON Pointer escaping for `~` and `/`. That diagnostic
field is subject to #552's public API gate. It does not add a field to the frozen CallError wire format. If an exact window pointer is also
required in the wire text, approve that as a separate text/API change; do not call it preservation.

Identity/lifetime failures are not grant misses and must not recommend adding authority. Reuse the approved bridge error taxonomy; coordinate missing categories and terminal outcomes with #540 and #1096. No numeric error allocation is made by this amendment.

Error precedence follows §4.4: bounded syntax/native identity, document and admitted-link lifetime, then channel authorization at final
handoff. If an earlier identity or availability check fails, return its refusal even when the channel would also be ungranted; do not
continue solely to choose a grant error. The GUARD003 propagation test keeps these earlier prerequisites healthy. A usable link never makes
an ungranted request admissible.

### 4.6 Directional permission decision matrix

| Principal/state and operation | Required outcome | Before Bun? |
|---|---|---|
| Invalid/unverified manifest | Existing boot denial; no child/listener/window startup | Yes |
| Missing/stale/destroyed native identity, bad nonce, wrong frame/world, generation mismatch | Identity/lifecycle refusal; no grant lookup fallback | Yes |
| Live window and usable matching app link; no `windows`, no window key/list, or empty list | GUARD003; exact stable fix | Yes |
| Live window and usable matching app link; valid literal only in another window | GUARD003 | Yes |
| Live window; valid exact literal in its own list | One guarded forwarding operation, subject to current app link and bounded credit | Guard precedes handoff |
| Syntactically valid prefix/suffix/case/Unicode variant, with current identity/link | GUARD003 unless separately declared | Yes |
| Wildcard/malformed declaration | Entire manifest rejected at load | Yes |
| Page forges AppProcess, window key, generation, role, or native target | Header rejected or ignored as nonauthority at the public relay; never used for principal selection | Yes for any unauthorized effect |
| Channel grant plus app native capability | Still no direct native/web-media permission for the renderer | Yes |
| Old queued request after revocation wins | Refuse without app-link write; settle/retire accounting once | Yes |
| Correct grant but Bun unavailable | Typed transport/lifecycle refusal, not ChannelForbidden; no replay | Depends on whether a handoff occurred; preserve no-effect versus unknown distinction |
| Authenticated Bun `event.reply` | Original captured sender document/frame; reject retired target | Outbound target validation applies |
| Authenticated Bun `webContents.send` | Current host-resolved target at send admission; bind queued event to that target | Outbound target validation applies |

The incoming grant list authorizes renderer-to-Bun calls/sends. It is not a requirement that a reply channel also have an `ipcMain`
registration. Outbound send/reply still requires authenticated Bun source, host-owned destination, generation validity, and any sending-role
policy. This directional clarification is proposed for exact #541 approval; it is consistent with #398's renderer-originated scope and
#543's delivery contract. It is not a grant of arbitrary broadcast authority.

An `ipcMain.on/once/handle` registration is not runtime permission. `once` is consumed only by an admitted message; a denied frame never
reaches the listener. `senderFrame` is a host-authenticated representation, never a page-selected object. Electron v44.4.5 documents that it
can become null after navigation/destruction and that `reply` returns to the originating frame; #466 owns the compatible facade.

### 4.7 Single navigation-generation contract

Owner: the existing per-WebView host state in keld-wv. Core consumes an opaque admission stamp and liveness accessor; it does not maintain a counter or replacement registry. The guard principal is constructed from the admitted stamp only after its currentness is validated. The renderer never mints a generation.

Use checked `u32` for the navigation generation everywhere that carries this authority. `0` is the unbound initial state. Each revocation
event advances via `checked_add(1)`. Overflow enters permanent `Exhausted` before any new bind, request, or delivery. Do not cast a `u64`
counter to `u32`. Initial first commit advances from 0 to 1 and enables one document binding. Any initialization-specific binding sequence
must be proven against real WebKit before authority is exposed; readiness labels do not substitute for commit identity.

State terms:

- `Unbound`: WebView exists, no qualified committed document can send.
- `Live(g, nonce)`: current qualified document, with host-created nonce and generation.
- `AwaitingCommit(g, expected_native_attempt)`: host full-document load has revoked prior authority; neither old-world bind nor invoke is accepted. The expected engine navigation/load identity is host-owned correlation metadata, not a second principal or generation mint.
- `FailedClosed(g)`: the expected host load failed or was cancelled; its attempt is retired and a late commit for it cannot reopen admission. A new explicit trusted load is required unless a separate rebind protocol is approved.
- `Destroyed` and `Exhausted`: permanent closed states for that lifetime.

Generation is a revocation epoch, not a document count. A successful host full-document load has two distinct increments: one before engine
load and one at commit. Each is checked independently. A duplicate of an already processed native event is an idempotent no-op: it preserves
the current generation and live or closed state. Native event identity/deduplication belongs to #542, not URL equality. Only a commit
correlated to the current expected native attempt can reopen AwaitingCommit. Stale metadata callbacks for superseded, cancelled, or failed
attempts cannot modify current authority. An actual replacement-document commit with uncorrelated or unqualified identity revokes the prior
document and remains closed. #542 must use native correlation or prove an equivalent serialized native event contract; URL equality,
callback timing, and a page claim are insufficient. If the scaffold exposes too little identity, extend the existing native owner or fail
qualification. An unrecognized replacement commit cannot leave the old document authorized.

| Event | Generation change | Document/pending disposition |
|---|---:|---|
| Initial qualified cross-document commit | 0 → 1 | Enable one host-authenticated document bind |
| Renderer begins provisional navigation | 0 | Existing live document remains valid until commit |
| Renderer navigation cancelled/fails without replacement commit | 0 | Same nonce, generation, and pending work remain usable |
| Renderer cross-document commit, including renderer reload/same-URL replacement | +1 checked | Revoke old nonce/queued admission/document-owned renderer subscriptions; retire pending work; bind new qualified document |
| Hash, pushState, replaceState, same-document history | 0 | Preserve identity and pending delivery |
| Host full-document load/reload accepted | +1 checked before engine invocation | Enter AwaitingCommit; revoke old nonce and reject rebind/invoke from old document |
| That host load commits with matching current native attempt | +1 checked again | Enable new qualified document bind; no reuse of old pending identity |
| Duplicate of an already processed native event | 0 | Idempotent no-op; preserve the current live or closed state and binding |
| Stale metadata callback for a retired/superseded/failed native attempt | 0 | Cannot reopen, replace, close, or otherwise modify current authority |
| Actual replacement-document commit with uncorrelated or unqualified identity | +1 checked revocation; no admission | Revoke the prior document and remain closed; no new binding until a qualified trusted transition |
| That host load fails or is cancelled | 0 further increments | Enter FailedClosed; retire expected attempt; no rollback to old nonce/generation. Recovery requires an explicit trusted load/commit or separately approved rebind protocol |
| Proven host same-document operation | 0 | Preserve document. Classify through a trusted backend contract, not a page claim or URL string |
| Unclassified host navigation | No authority-changing guess | Hold/refuse that command until #459/#542 can classify it without violating same-document preservation; do not ship a claim of supported ambiguous loadURL behavior |
| Increment at u32::MAX | No wrapped result | Exhausted, permanent refusal; retirement once |
| WebView destruction | No new usable generation | Destroyed; queued work and callbacks revoked; lifetime cannot be resurrected by id/address reuse |
| Bun loss/recovery with surviving WebView | 0 | Preserve document nonce; retire old Bun session/correlations, no replay, fresh accepted app link for future calls |
| Back-forward cache restores an old document | Determined by real engine document transition | Never reuse a retired nonce. Remains unqualified until a native restore/rebind test proves the contract; do not infer from same URL |

WindowGeneration, navigation generation, and Bun role generation are distinct. Window recreation cannot reset authority for an old WebView stamp; Bun restart cannot serve as a navigation event. Compare the complete host-owned tuple needed by the route: host session/WebView lifetime, navigation generation, document binding, target frame, and app-link generation/correlation where applicable.

### 4.8 Replies, delayed evaluation, and recovery

Keep request identity through every asynchronous step. A reply accepted on the host before navigation can still wait for WebKit JavaScript
execution. Therefore a host-side equality check alone is insufficient as a general delivery proof. Delivery must address the captured native
document/frame and the isolated receiver must reject a completion whose host document binding is not its own. Native pointer, WebView id, or
local request number alone is insufficient to identify the captured document.

Do not send the secret document nonce into page-world events or accept it from page input. The isolated/native control path may carry its bound token as approved by #540/#1096. Page promises and DOM messages are attacker-controlled observations and cannot be the only oracle for absence of a privileged effect.

A cancelled renderer provisional navigation keeps valid pending work. A replacement commit invalidates old delivery. A failed host load
keeps the early revocation. Pending work that might have reached Bun is not reissued merely because its renderer disappeared. Exactly-once
terminal accounting means one reservation release and one outcome for the request; it does not mean rollback or exactly-once external side
effects.

During Bun recovery, old app-link callbacks cannot authorize a new link or inherit new correlation entries. Previously admitted but unsent
queue entries also retain the old captured Bun generation and are retired, rather than forwarded to the successor. Only newly admitted
requests can capture the successor link. No renderer message chooses the runtime generation. Reuse KEL-143's accepted recovery owner. Its
surviving document need not reload to send a new permitted message once the new app link is authenticated.

### 4.9 Migration source and architecture example

`keld migrate` enumerates main-side string-literal `ipcMain.on`, `once`, and `handle` registrations. It records the source location and
exact decoded string, emits the canonical `el:` literal, and refuses wildcard strings. Dynamic/computed names are unenumerable; the report
must expose them and block claims that all required traffic is covered. Preload/page names, network input, and runtime renderer requests are
not grant sources.

#499 owns the hand-authored first draw.io artifact and additionally intersects enumerated registrations with the observed PANEL-P3 trace. An observation can narrow an eligible registration list; it cannot create a grant for a name with no trusted main-side literal source. Conditional registrations need their scope recorded rather than global cross-window union.

The architecture example becomes `["el:rendererReq", "el:saveFile"]` for its illustrative main window, with empty grants for about. The schema rule becomes: for schema-declared channels, the declared capability set is a subset of caller grants. Opaque Electron channels use exact incoming message grants, while every native operation requested by Bun still passes its native capability guard.

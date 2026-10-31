# Spec: exact channel grants and host-owned navigation identity

Status: approved contract, unmerged — explicit repository-owner delegation; separate implementation gates remain

Tracker: [GH #541](https://github.com/gyldlab/keld/issues/541), F04-T7. Reference: KEL-102. Implementing owners: [#552](https://github.com/gyldlab/keld/issues/552), F04-T8, and [#542](https://github.com/gyldlab/keld/issues/542), F04-T9. Contract owner: GYLDLAB. Reviewed: 2026-10-23.

Source pin: `442f3ebcef777f204cec7073932dc06edfab3a2e` (immutable review baseline). The approved technical commit is `7e0c1b89c4767f2e6625cd2bfc4d36d86b30fc26`, based on `ef491ebd67ae794e004977a6d6578cdbb7235d61`. At approval time, main's delta from the review pin comprised two KEL-254 Windows files, the Tokio lock entry and its CI-input digest; no reviewed Keld source or proposed documentation target had changed. Original KEL-102 and KEL-142 approval records remain scoped to their original content. New exact-content decision: KEL-102 comment `8fa29b34-bdb6-470f-9f97-9cdf0aa1d0eb`; decision SHA-256 `2da2c23a53848d618f08612a1106d53283b7488601be7fd3eaa5ed95038df5af`. The digest covers the decision JSON line, excluding its newline, and its five-file manifest binds the approved technical content. This follow-up records provenance and tracker metadata only. Independent-human approval is not claimed. No implementation or merge is authorized by this document.

Integration-base refresh: `f19c8847787aaf0e9a14e07c0af008fc5a4c2a1f` subsequently merged #1096's documentation. This branch preserves that owner's KEL-142 §7 correction and the generated error-registry text. The #541 contract requirements and original decision artifact are unchanged; #1105's implementation remains unmerged. Historical references below to #1096 being unmerged describe the approval-time snapshot.

## 1. Goal & non-goals

Enable the minimum Electron renderer-to-main IPC needed for the macOS draw.io proof while preserving host-enforced default deny. IPC means inter-process communication: messages cross from an untrusted renderer through the Rust host to the supervised Bun application process. A channel grant permits one named message route. It does not grant filesystem, camera, shell, or other native authority.

Observable result: a current, host-authenticated main-frame document may forward an application message only when its host-bound window has that exact channel literal. Every miss fails before an app-link frame is written. Every retained request and delivery remains bound to its original live document and Bun session. Removing a guard or lifetime check must make an independently observed test fail.

Non-goals:

- Implementing #552 or #542 under this contract approval alone; each retains its own implementation-spec gate.
- Changing the KIPC frame format, allocating v2 wire fields, selecting endpoint codecs, or defining credit constants. #540 owns those contracts; #465/#543/#544 implement them.
- General window/web grants, media permission enablement, role grants, a new core WebView registry, or a second permission evaluator.
- Claiming Electron navigation equivalence without the recorded oracle owned by #551. Keld tests alone do not establish it.
- Claiming strict OS containment from a channel check. KEL-78 and the approved execution profile own operating-system confinement. The first draw.io proof's explicit legacy profile does not become strict because this amendment passes.
- General subframe IPC, arbitrary remote-content authority, automatic replay, live permission reload, native module enablement, or a fallback to ungated Echo for Electron traffic.

## 2. Spec refs and approval provenance

| Reference | Verified relationship |
|---|---|
| [KEL-102 pinned approved spec](https://github.com/gyldlab/keld/blob/442f3ebcef777f204cec7073932dc06edfab3a2e/docs/specs/kel102-host-guard-enforcement.md) | Immutable verified policy, trusted principal selection, shared dispatch, and separate OS containment. Original approval `b330c2ec-2193-4aa6-b324-67187cdda4d5`; its digest is not approval of this amendment. |
| [KEL-142 pinned approved spec](https://github.com/gyldlab/keld/blob/442f3ebcef777f204cec7073932dc06edfab3a2e/docs/specs/kel142-macos-renderer-bridge-api.md) | Existing main-frame isolated-world bridge and bounded Echo invoke. Original approval `53510bc6-66e0-441c-8992-010b6a8719e6`, approved content `2e8da1b7ba19783eceb671093620419664537415`. |
| [#398 decision](https://github.com/gyldlab/keld/issues/398#issuecomment-6022113039) | Adopted KEL-102 amendment, exact `el:<channel>` literals, existing GUARD003, and separate exact-content approval. |
| [#400 resolution](https://github.com/gyldlab/keld/issues/400#issuecomment-6022099642) and [packet](https://github.com/gyldlab/keld/issues/400#issuecomment-6022069479) | Reuse the existing per-WebView mint; renderer commit rotation, earlier host-load invalidation, same-document preservation, checked u32. |
| [#541 brief](https://github.com/gyldlab/keld/issues/541#issuecomment-6026146702) | Owns this contract and the architecture 03 example correction. |
| [#552 brief](https://github.com/gyldlab/keld/issues/552#issuecomment-6026147923) | Owns generic channel evaluation, before-Bun enforcement, zero-allocation allow, and explain parity. |
| [#542 brief](https://github.com/gyldlab/keld/issues/542#issuecomment-6026149009) | Owns the unified navigation mint and real macOS tests. |
| [Architecture 03](https://github.com/gyldlab/keld/blob/442f3ebcef777f204cec7073932dc06edfab3a2e/docs/architecture/03-security.md) | Authority model, immutable grants, default-deny remote windows, and native capability checks. |
| [#1096 exact r2 approval](https://github.com/gyldlab/keld/pull/1096#issuecomment-6364741222) | Approved error/document-result amendment at `60280baa8cbce4b58755679a42b54bd57df66299`; its documentation merged through `f19c8847787aaf0e9a14e07c0af008fc5a4c2a1f`. Implementation #1105 remains unmerged. Preserve its owner; landed documentation does not establish implemented behavior. |

KEL-102/T3 is already landed at `66ccbbc68c3dd44be55878bb0c14e9788e25a4b1` through #357, terminal comment `cde25f5e-db85-4d22-9349-553469fecea0`. Its unchecked task in the source spec is documentation drift. KEL-142 is Done and landed through #351. Neither completion authorizes the new channel schema.

### 2.1 Explicit amendments to earlier wording

1. Remove only the exact Electron channel-grant slice from KEL-102's window/channel non-goals after this content is approved. General window/web, role, and native capability grants remain outside the slice. Keep the old approval JSON and digest unchanged as historical provenance.
2. Qualify KEL-102 D5 and the core routing paragraph: the new registered **renderer-to-Bun forwarding adapter in keld-core** may call the existing `dispatch_privileged` around forwarding. Core still does not call `evaluate`, match grants, or mint permits. Native brokers and media retain their existing designated ownership. This narrow new caller of the shared dispatcher is an explicit proposed ownership change, not an inference from the old approval.
3. Qualify KEL-142 §5.2 item 6, §6, and AC5: renderer provisional navigation alone does not revoke the current document; cross-document commit does. Host-initiated full-document load/reload revokes earlier. Same-document navigation never rotates. A failed host load does not restore old authority.
4. Preserve KEL-143's recovery rule: Bun recovery does not rotate navigation identity or replace the surviving document nonce. App-link/Bun generation is a separate lifetime axis.
5. Preserve the approved but unmerged #1096 error owner. Its `WV-012`, `WV-013`, `API-002`, and document-scoped-result behavior must be integrated under that approval; this amendment allocates no competing numeric errors.

## 3. Acceptance criteria

Each criterion names an observable, not an implementation assertion. Section 7 maps them to tests.

1. A valid policy with exact channel strings loads once from the verified host-selected snapshot. Missing `windows`, a missing window, missing `channels`, or `[]` grants nothing.
2. A malformed windows/channel structure, non-string member, decoded wildcard, invalid channel spelling, or duplicate decoded object key rejects the whole policy with `KELD-GUARD005` before application resources. There is no filtering, partial policy, or fallback.
3. Given an authentic current document bound to window A and a usable matching app link, an unlisted channel yields the existing `ChannelForbidden` contract and zero app-link writes and zero Bun handler entries. A listed exact literal in the same fixture produces one observed forwarding operation.
4. The same literal in window B, `/app` scopes, another role, a renderer field, or a dynamically registered handler cannot grant window A authority.
5. Prefix, suffix, case, whitespace, percent-encoding, and Unicode-normalization variants do not match unless independently listed as valid distinct literals. JSON escape variants of the same decoded string are the same literal.
6. Every authority-bearing channel path calls `dispatch_privileged`; its callback is the sole place that can forward the message. `evaluate` remains inside that helper for authoritative decisions. Diagnostic explain is the named non-dispatch exception. Repository-wide sole-caller completion additionally requires the accepted KEL-102/T4 media-integration evidence; this channel amendment does not waive that gate.
7. The warmed exact-allow policy/dispatcher path performs zero heap allocations, measured with a counting allocator around the actual path. Channel serialization and transport allocations are measured separately, never hidden inside the result.
8. MCP (Model Context Protocol) permission explain uses the same pure decision logic and returns the same channel decision. Its hypothetical window selection cannot mint a live principal or dispatch permission.
9. One host-owned navigation source supplies both admitted request stamps and `Principal::Webview { id, generation }`. Their values must be equal. No renderer identity, pointer conversion, URL, token text, numeric manifest key guess, or separate counter supplies authority.
10. A native cross-document commit invalidates the old nonce and pending identity. A late old request cannot cross the final dispatch boundary if revocation won the ordering race. Old replies/events cannot settle or enter the new document. Final handoff is bounded and nonblocking; a stalled transport cannot block native revocation on the UI thread.
11. Hash, `pushState`, `replaceState`, and same-document history navigation leave navigation generation and valid pending work unchanged, observed on real macOS WKWebView.
12. A cancelled renderer-initiated provisional navigation preserves the old document's nonce, generation, and ability to send. A committed replacement, including a reload to the same URL, does not.
13. A trusted host full-document load/reload invalidates before invoking the engine. An old document cannot rebind during the uncommitted interval. A failed/cancelled load does not roll authority back. Native-attempt correlation rejects obsolete transitions; a duplicate event is an idempotent no-op. The live running-app host-load seam belongs to #459.
14. Navigation arithmetic is checked `u32`. An attempted increment above `u32::MAX` permanently refuses that WebView's dispatch/delivery; no wrap, saturation, truncation, reset, or rebind escapes the terminal state.
15. Destroying a WebView revokes queued admission, pending delivery, document-owned renderer subscriptions, and callbacks exactly once. Reusing a native address or numeric id cannot resurrect the old lifetime. Global Bun `ipcMain` registrations belonging to the still-running application are not document-owned subscriptions.
16. Wrong WebView, wrong native frame/content world, bad nonce, unknown authority header, stale navigation, or mismatched app-link identity fails closed before forwarding.
17. Bun recovery revokes old app-link correlation/work exactly once, preserves the surviving document identity, creates fresh authenticated app-link authority, and never automatically replays an uncertain operation.
18. The host treats application values as opaque payload bytes. Payload contents cannot select a guard principal, grant, native handler, window, correlation, or codec. The bounded outer envelope supplies the validated channel and routing metadata.
19. `event.reply` retains the original sender document/frame identity. `webContents.send` resolves the current target document at its own send admission. Neither targets by numeric window id alone, broadcasts, or falls back to a newer document.
20. Main-side string-literal registrations are the only grant-enumeration source. Dynamic registrations are reported unenumerable and wildcard emission is refused. #499's first-proof artifact further intersects those registrations with the observed PANEL-P3 trace.
21. The architecture 03 example contains only enumerated literal channel grants. The capability-set-subset rule is explicitly about schema-declared channels, not opaque Electron payloads.
22. Corresponding navigation conformance rows 10–12 remain `UNKNOWN` for Electron behavior until #551's recorded oracle comparison; a passing Keld-side test cannot promote them.

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
  "windows": {
    "main": { "channels": ["el:rendererReq", "el:saveFile"] },
    "about": { "channels": [] }
  }
}
```

Rules for this slice:

- `windows`, each window value, and `channels` have the object/object/array-of-strings shapes shown. Omitted fields are empty authority; explicitly wrong types, including `null`, are policy errors.
- Each grant is a decoded, well-formed Unicode string whose UTF-8 bytes begin with ASCII `el:` and have a nonempty suffix. It contains no U+002A `*`, NUL, C0/C1 control, or DEL. Surrogate halves are rejected by the string/wire validation boundary; they are not replaced or normalized into a granted value.
- Match the validated decoded UTF-8 string exactly. Do not trim, case-fold, normalize Unicode, percent-decode, interpret regular expressions, apply path `/**`, or perform prefix/suffix matching. Punctuation other than the explicitly forbidden characters is literal, never a pattern language.
- Reject `el:*`, `el:**`, `el:rendererReq*`, and their JSON-escaped forms after decoding. A mixed valid/wildcard list rejects the complete policy. This is stricter than loading an inert wildcard.
- Duplicate decoded object keys remain invalid under the existing parser. Repeated identical strings in an array add no authority; preserve deterministic first-match behavior and let migration deduplicate them.
- The existing 64 KiB total manifest limit remains. #540 owns message channel-length and envelope budgets; no larger unbounded channel allocation is implied here.
- The Electron adapter maps an application channel `c` to the canonical name by exactly one `"el:" + c` construction. No repeated-prefix stripping, alias, URL decoding, or case conversion is allowed. keld-ipc receives a generic channel string and has no Electron or codec knowledge.

The host binds a configured window key to a WebView at creation from trusted configuration. The binding stays with that WebView lifetime. A renderer cannot supply, rename, or select it. A numeric WebView id is not a manifest key. Remote-content admission must still satisfy the owning navigation/content policy; a permitted cross-document navigation is not permission to expose the application bridge to arbitrary remote content. If that policy is absent, fail closed. Architecture 03's remote-window `channels: []` rule remains in force.

**First-proof eligibility gate:** before a bind can use a nonempty incoming channel list, the host must establish that the committed main-frame document belongs to the approved application-document set through its boot/resource containment and native navigation evidence. A page-supplied URL, a `file:` prefix, same URL text, or a fresh nonce alone is insufficient. Repeat that check for each replacement document. This review does not claim the gate exists. The proposed narrow owner is **#542**, extending its document-bind admission scope to consume the existing host-validated application-resource boundary and prove N11. This proposed scope addition requires exact approval in #541 and the separate #542 spec; no new web.navigate grammar is authorized. Full navigation policy remains [#460 / KEL-79](https://github.com/gyldlab/keld/issues/460). Without the qualified restricted bind gate or the qualified general policy, channel enablement is NO-GO. A functional application demonstration alone does not qualify this security boundary.

### 4.4 Query types and dispatch ownership

The logical guard query distinguishes `NativeCapability(operation, resource)` from `RendererChannel(channel, host-bound-window)`. This is a required semantic distinction. #552's separately approved public API selects the narrow Rust representation; this amendment does not fabricate an already-existing constructor or treat a raw `Principal` value as proof of host provenance.

The window grant selector is an immutable borrow/handle resolved from the existing host snapshot and WebView binding. It is not a renderer string, mutable global union, or cached Allow. Existing native capability queries preserve their principal rules. In particular, a grant for `el:fs.read` authorizes only an application message named `fs.read`; it cannot enter the native filesystem broker.

Normative order for renderer-originated channel traffic:

1. Enforce bounded envelope syntax and native sender/frame/content-world checks.
2. Resolve the original document's host-owned stamp and trusted window binding; verify live state. Capture the currently admitted host-owned Bun/app-link generation as part of this work's dispatch identity. If no usable generation exists, refuse admission with the applicable no-effect outcome; do not leave old work eligible to attach to a future link.
3. Validate the generic channel name and retain opaque payload bytes within #540's bounds.
4. At the final app-link handoff, order the live-stamp check against revocation, require the captured Bun/app-link generation to equal the still-current authenticated link, and invoke `dispatch_privileged` on the typed channel query. Never retarget already-admitted work to a successor Bun generation.
5. Only its Allow closure may forward. No listener, Bun frame, native handler, or application-value decoder is reached on a denial.

The final check and handoff form one ordering boundary with navigation/destruction. If revocation wins, no app-link write occurs. If dispatch already won, the operation may have reached Bun; navigation cannot promise rollback. Drop obsolete delivery, retain the correct terminal outcome, and never replay automatically. A check before queueing alone is insufficient. Every queue whose consumer can forward after revocation must retain a revocable host stamp and observe the same owner. This is lifetime validation, not a duplicate permission matcher.

The final handoff must be bounded and nonblocking, with a defined linearization point: the single instant at which dispatch or revocation wins. #540 and #542 must freeze this ordering across their existing transport and navigation owners; any later transport queue preserves the captured identities and that ordering. Do not hold a lock needed by native navigation/destruction over blocking app-link I/O, a Bun wait, drain, or handler completion. KEL-142's UI-thread progress requirement remains mandatory. A blocked-transport negative test must still observe native revocation progress.

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

#552's requirement to identify the exact manifest entry is reconciled without silently changing this frozen sentence: host diagnostics and MCP explain include the trusted JSON pointer `/windows/<escaped-key>/channels`, using JSON Pointer escaping for `~` and `/`. That diagnostic field is subject to #552's public API gate. It does not add a field to the frozen CallError wire format. If an exact window pointer is also required in the wire text, approve that as a separate text/API change; do not call it preservation.

Identity/lifetime failures are not grant misses and must not recommend adding authority. Reuse the approved bridge error taxonomy; coordinate missing categories and terminal outcomes with #540 and #1096. No numeric error allocation is made by this amendment.

Error precedence follows §4.4: bounded syntax/native identity, document and admitted-link lifetime, then channel authorization at final handoff. If an earlier identity or availability check fails, return its refusal even when the channel would also be ungranted; do not continue solely to choose a grant error. The GUARD003 propagation test keeps these earlier prerequisites healthy. A usable link never makes an ungranted request admissible.

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

The incoming grant list authorizes renderer-to-Bun calls/sends. It is not a requirement that a reply channel also have an `ipcMain` registration. Outbound send/reply still requires authenticated Bun source, host-owned destination, generation validity, and any sending-role policy. This directional clarification is proposed for exact #541 approval; it is consistent with #398's renderer-originated scope and #543's delivery contract. It is not a grant of arbitrary broadcast authority.

An `ipcMain.on/once/handle` registration is not runtime permission. `once` is consumed only by an admitted message; a denied frame never reaches the listener. `senderFrame` is a host-authenticated representation, never a page-selected object. Electron v44.4.5 documents that it can become null after navigation/destruction and that `reply` returns to the originating frame; #466 owns the compatible facade.

### 4.7 Single navigation-generation contract

Owner: the existing per-WebView host state in keld-wv. Core consumes an opaque admission stamp and liveness accessor; it does not maintain a counter or replacement registry. The guard principal is constructed from the admitted stamp only after its currentness is validated. The renderer never mints a generation.

Use checked `u32` for the navigation generation everywhere that carries this authority. `0` is the unbound initial state. Each revocation event advances via `checked_add(1)`. Overflow enters permanent `Exhausted` before any new bind, request, or delivery. Do not cast a `u64` counter to `u32`. Initial first commit advances from 0 to 1 and enables one document binding. Any initialization-specific binding sequence must be proven against real WebKit before authority is exposed; readiness labels do not substitute for commit identity.

State terms:

- `Unbound`: WebView exists, no qualified committed document can send.
- `Live(g, nonce)`: current qualified document, with host-created nonce and generation.
- `AwaitingCommit(g, expected_native_attempt)`: host full-document load has revoked prior authority; neither old-world bind nor invoke is accepted. The expected engine navigation/load identity is host-owned correlation metadata, not a second principal or generation mint.
- `FailedClosed(g)`: the expected host load failed or was cancelled; its attempt is retired and a late commit for it cannot reopen admission. A new explicit trusted load is required unless a separate rebind protocol is approved.
- `Destroyed` and `Exhausted`: permanent closed states for that lifetime.

Generation is a revocation epoch, not a document count. A successful host full-document load has two distinct increments: one before engine load and one at commit. Each is checked independently. A duplicate of an already processed native event is an idempotent no-op: it preserves the current generation and live or closed state. Native event identity/deduplication belongs to #542, not URL equality. Only a commit correlated to the current expected native attempt can reopen AwaitingCommit. Stale metadata callbacks for superseded, cancelled, or failed attempts cannot modify current authority. An actual replacement-document commit with uncorrelated or unqualified identity revokes the prior document and remains closed. #542 must use native correlation or prove an equivalent serialized native event contract; URL equality, callback timing, and a page claim are insufficient. If the scaffold exposes too little identity, extend the existing native owner or fail qualification. An unrecognized replacement commit cannot leave the old document authorized.

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

Keep request identity through every asynchronous step. A reply accepted on the host before navigation can still wait for WebKit JavaScript execution. Therefore a host-side equality check alone is insufficient as a general delivery proof. Delivery must address the captured native document/frame and the isolated receiver must reject a completion whose host document binding is not its own. Native pointer, WebView id, or local request number alone is insufficient to identify the captured document.

Do not send the secret document nonce into page-world events or accept it from page input. The isolated/native control path may carry its bound token as approved by #540/#1096. Page promises and DOM messages are attacker-controlled observations and cannot be the only oracle for absence of a privileged effect.

A cancelled renderer provisional navigation keeps valid pending work. A replacement commit invalidates old delivery. A failed host load keeps the early revocation. Pending work that might have reached Bun is not reissued merely because its renderer disappeared. Exactly-once terminal accounting means one reservation release and one outcome for the request; it does not mean rollback or exactly-once external side effects.

During Bun recovery, old app-link callbacks cannot authorize a new link or inherit new correlation entries. Previously admitted but unsent queue entries also retain the old captured Bun generation and are retired, rather than forwarded to the successor. Only newly admitted requests can capture the successor link. No renderer message chooses the runtime generation. Reuse KEL-143's accepted recovery owner. Its surviving document need not reload to send a new permitted message once the new app link is authenticated.

### 4.9 Migration source and architecture example

`keld migrate` enumerates main-side string-literal `ipcMain.on`, `once`, and `handle` registrations. It records the source location and exact decoded string, emits the canonical `el:` literal, and refuses wildcard strings. Dynamic/computed names are unenumerable; the report must expose them and block claims that all required traffic is covered. Preload/page names, network input, and runtime renderer requests are not grant sources.

#499 owns the hand-authored first draw.io artifact and additionally intersects enumerated registrations with the observed PANEL-P3 trace. An observation can narrow an eligible registration list; it cannot create a grant for a name with no trusted main-side literal source. Conditional registrations need their scope recorded rather than global cross-window union.

The architecture example becomes `["el:rendererReq", "el:saveFile"]` for its illustrative main window, with empty grants for about. The schema rule becomes: for schema-declared channels, the declared capability set is a subset of caller grants. Opaque Electron channels use exact incoming message grants, while every native operation requested by Bun still passes its native capability guard.

## 5. Boundaries and integration ownership

| Owner | Allowed responsibility | Explicit boundary |
|---|---|---|
| #541 | Companion amendment; scoped KEL-102/KEL-142 wording; architecture 03 correction; approval packet | No product implementation; old approval digest preserved |
| #552 / keld-guard, keld-ipc, diagnostic CLI, narrow keld-core forwarding adapter | One parser/evaluator extension, typed query/context, exact renderer-to-Bun `dispatch_privileged` composition in `keld-core/src/app_session.rs` or its owned extraction, zero-write integration oracle, explain parity, allocation test | Core-path addition is proposed scope for exact #541/#552 approval; generic names only in IPC; no Electron codec; no second counter |
| #542 / keld-wv | Checked navigation state, host stamp, attempt correlation, lifecycle validation/accessor, restricted application-document bind eligibility and native tests N01–N12 | Restricted eligibility is proposed scope for exact #541/#542 approval; consume host resource-containment identity, no new web.navigate schema, core registry, or Bun generation mint |
| #540 | wv-link v2 channel representation, typed errors, bytes/credits and opaque payload rules | Must consume the grant and generation contract; cannot infer approval of permissions |
| #465/#543/#544 | Send, outbound event delivery, endpoint codec implementation | Bounded queue/revocation and original target identity |
| #466 | ipcMain/once/reply/senderFrame/webContents facade | Cannot introduce facade-only authorization or direct Bun ingress |
| #459 | Running-app first-proof `loadURL(file://)` seam and #1096's cleared pending-call handoff | Consumes #542; does not acquire the separately owned reload/stop/rejection facade vocabulary |
| #538 | Later full webContents vocabulary including reload, stop, and loadURL rejection | Consumes the shared navigation contract; full surface is not invented as a first-proof blocker |
| #460 / KEL-79 | Guard-first navigation policy and deny-only application hooks | Owns general navigation grants; #542 owns the proposed narrow first-proof binding restriction if this remains deferred |
| KEL-102/T4 | Complete and qualify the designated shared-dispatch media integration | Media remains denied outside its separately approved scope, including any initial-generation-0 path; channel grants cannot enable capture |
| KEL-143 | Bun generation recovery and surviving document behavior | Navigation counter is unchanged by Bun restart |
| #499 | First-proof migration artifact | Main literal registrations intersect observed trace |

Keep shared wire/schema changes and other owners' branches within their recorded gates. Refresh tracker claims and main before implementation. The [#541 specification claim](https://github.com/gyldlab/keld/issues/541#issuecomment-6370773598) records this draft's coordination; it does not authorize product implementation or merging.

## 6. Tasks and dependencies

### One new KEL-102 task unit

**T7 — exact window channel grants and shared navigation identity**: consume the exact-content-approved #541 amendment; implement via #552 and #542 with their separately approved implementation specs; coordinate the transport owner; prove the acceptance matrix before enabling Electron traffic. T7 does not mark media T4, role T5, containment, or Electron oracle work complete.

### Execution order

1. Preserve the recorded #541 exact-content decision, including the explicit clarifications in §10 and paired architecture/spec edits. Complete final documentation/review gates and the still-open #541 merge acceptance; merge intent remains do-not-merge.
2. In parallel, finish #464's failing-first conformance corpus and #540's protocol specification. Their drafting has no open native dependency on #541. Freeze their common identity/channel interface before integration.
3. After #541, approve and implement #552, explicitly including its narrow core forwarding adapter and P02/N02/N09b composition tests. After #541 and #464, approve and implement #542, explicitly including native attempt correlation and the restricted bind gate N11. Policy unit work may proceed independently, but the composed #552 zero-write claim waits for the trusted #542 stamp/liveness seam and the #540 transport contract. Require the accepted T4 shared-dispatch evidence before claiming #552's repository-wide acceptance criterion.
4. Integrate #465/#543/#544 and #466 only after their recorded predecessors. Consume approved #1096/#1105 changes with their owner if available; do not allocate substitute errors.
5. Run the required real macOS boundary/race tests and exact allow/deny app-link observer. Maintain UNKNOWN Electron navigation rows until #551's later recorded oracle.

### Verified native dependency map

| Issue | Native predecessors at review time | State relevant to this review |
|---|---|---|
| #541 | none | Open; [specification claim recorded](https://github.com/gyldlab/keld/issues/541#issuecomment-6370773598); implementation gates remain |
| #540 | none | Independent spec work may proceed |
| #464 | #532, #566 | Both predecessors closed; #464 itself open |
| #552 | #541 | Blocked on amendment |
| #542 | #541, #464 | Both open |
| #465 | #464, #540 | Both open |
| #543 | #464, #540, #542 | All open |
| #544 | #464, #540 | Both open |
| #466 | #464, #465, #543, #540, #552, #542, #544, #473, #597 | All open except #597 |
| #459 | #458, #449, #567, #542 | All open; contractual #542 edge added and read back on 2026-10-23 |
| #499 | #497, #586, #452, #453, #491, #420, #541 | All open except #420; contractual #541 edge added and read back on 2026-10-23 |
| #551 | #494, #464, #495 | Later recorded-oracle milestone; do not turn it into an invented first-proof blocker |

Under the owner-delegated decision, the two contractual dependencies were added to native tracking and read back without removing existing predecessors. The owning issues receive explicit handoffs; no implementation claim or ownership transfer is made. Unassigned issues do not imply permission to take over active related PRs. #1095/#1096/#1105 retain their existing ownership.

## 7. Test acceptance and independent controls

These are required implementation acceptance tests, not assertions that a failure
has been reproduced or that the proposed boundary is implemented. This specification
supplies no new native qualification. Each owner must record the exact revision,
platform, command and observed outcome before claiming its criterion complete.

Each expected-red test must be observed failing before the corresponding product
change when its real harness is available. A negative control is a deliberate single
fault that the test must detect. Preserve paired valid controls. A source inspection,
a state model, or a page-only observer cannot prove a native effect boundary.

| ID / criteria | Contract under test | Independent required oracle | Single-fault negative control |
|---|---|---|---|
| P01 / 1–2 | Strict policy parsing | Malformed/wildcard/escaped-star inputs fail with GUARD005 before real application resources; paired valid policy loads. | Remove decoded-string validation. |
| P02 / 3 | Guard-before-Bun forwarding | Pin GUARD003 and its complete text; independently observe zero app-link writes and zero handler entries on denial, one each for the exact permitted control. | Bypass dispatch_privileged. |
| P03 / 4 | Per-window grant isolation | A host-authenticated window lacking a literal cannot use another window's grant; independently observe no ingress. | Use a global grant union or renderer-selected window key. |
| P04 / 5 | Exact channel equality | Distinct prefix, suffix, case and Unicode spellings are denied unless separately granted. | Replace equality with prefix matching or normalization. |
| P05 / 4,16 | Host-owned renderer identity | Authority fields supplied by a page cannot change the native principal or permit an unauthorized effect. | Derive authority from a renderer header. |
| P06 / 4,6 | Channel/native capability separation | Channel permission cannot enter a native adapter or permit capture; paired permitted app operation still succeeds. | Route a native capability query through the channel-grant branch. |
| P07 / 6 | Single authoritative evaluation boundary | Review authority-bearing callsites and prove real denial through the shared dispatcher; preserve the diagnostic-only exception. | Add a direct-authority evaluator caller. |
| P08 / 7 | Allocation-free allowed dispatch | A counting allocator around the actual warmed policy/dispatcher path observes zero allocations. | Clone a channel or window string on the allowed path. |
| P09 / 8 | Diagnostic parity without authority | Explain and authoritative evaluation yield the same decision; diagnostic output cannot produce live admission or invoke an effect callback. | Use an independent matcher or export a usable live permit. |
| P10 / 1,20 | Immutable session policy | A policy-file edit does not change decisions in an existing session; a fresh authorized session uses the new snapshot. | Reparse policy per request. |
| N01 / 9 | Single navigation source | Capture equality between the trusted admitted-request stamp and guard-principal generation. | Rotate only one of those values. |
| N02 / 10 | Revocation orders queued admission | Use deterministic barriers to prove that revocation winning before handoff prevents Bun ingress, with a permitted control. | Omit the final currentness comparison. |
| N03 / 10,19 | Document-scoped completion | A completion whose captured document differs from the receiving document has no callback or application effect; a matching completion succeeds. | Remove the document-binding comparison. |
| N04 / 11 | Same-document preservation | Real WKWebView same-document events preserve the stamp, nonce and valid pending result. | Rotate on a same-document event. |
| N05 / 12 | Cancelled renderer-navigation preservation | A real cancelled provisional navigation preserves the current document identity and next valid send. | Rotate at renderer provisional start. |
| N06 / 13 | Host-load admission closure | The native harness proves that AwaitingCommit admits neither bind nor invoke from a retired document; a qualified successor is admitted. | Remove the AwaitingCommit refusal. |
| N06b / 13 | Native navigation-attempt correlation | Only the expected native attempt can satisfy a pending transition; mismatched and retired attempts cannot admit a document. | Omit the expected-native-attempt comparison. |
| N06c / 13 | Native event identity and duplicate handling | A duplicate event cannot rotate a healthy binding twice; an unrecognized replacement still closes authority. | Treat every callback as a new commit. |
| N07 / 14 | Generation exhaustion | At the maximum u32 generation, the next invalidation produces permanent refusal and no dispatch or delivery. | Use wrapping/saturating arithmetic or unchecked narrowing. |
| N08 / 15 | WebView lifetime revocation | A native subprocess proves no post-destruction effects or callback authority; unrelated live views remain healthy. | Omit the destroyed-state or lifetime comparison. |
| N09 / 17 | Bun recovery isolation | Old and new links have distinct authority; no replay occurs, navigation identity remains stable, and a fresh request succeeds. | Replay pending work or rotate navigation on Bun restart. |
| N09b / 17 | Queued work remains bound to its admitted Bun session | Controlled recovery proves that old admitted work cannot enter the successor link; a newly admitted request can. | Select the current link at dequeue without comparing the captured generation. |
| N10 / 10,19 | Back-forward-cache document binding | Real restore and rebind evidence proves that a retired document binding is never reused as current authority. | Reuse a retired cached nonce. |
| N11 / 4,9,16 | Qualified application-document admission | The real native binding gate accepts approved application documents and refuses documents outside the approved set. | Admit a document from its WebView id alone. |
| N12 / 10 | Bounded final handoff and revocation progress | Independently stalled transport cannot prevent native navigation/destruction and revocation within the harness deadline. | Hold a revocation-critical lock across blocking transport or handler waits. |
| W01 / 16,18 | Bounded public-relay envelope | The production relay and native harness accept no wider effect-bearing request than the reviewed facade contract. | Remove an admitted-source, channel, byte or request bound. |
| W02 / 18 | Queue and credit lifetime bounds | A held consumer cannot exceed message/byte bounds; every reservation is retired exactly once across lifetime changes. | Remove a bound or release credit twice. |
| W03 / 18 | Payload snapshot integrity | Mutating caller-owned bytes after admission cannot change the admitted payload observed independently at the receiving boundary. | Omit the required synchronous payload copy. |
| E01 / 3 | Stable denial propagation | An independent code/message/target observer proves the exact ChannelForbidden contract survives delivery. | Substitute a generic error for GUARD003. |
| M01 / 20–21 | Restricted migration grant source | Compare emitted literals with main-side literal registrations and the required trace intersection; no wildcard or renderer-derived grant is emitted. | Use preload/page names or an unrestricted observation union. |
| C01 / 22 | Conformance provenance | The corpus validator refuses an Electron PASS without the required pinned recorded oracle. | Promote UNKNOWN without the oracle artifact. |

### Required evidence and qualification

The implementation owners must supply actual guard/parser tests, the zero-write
app-link observer, allocation measurements, real macOS navigation/lifetime tests,
and independent negative-control outcomes. No row above is qualified by this draft.
A test plan or a written but unexecuted test is not a passing artifact. Electron
conformance remains subject to #464/#551's recorded-oracle requirements.

Reuse the owning crates' existing parser, guard-dispatch, allocation, wv-link and
native renderer test harnesses. Keep waits tied to observable events; timeouts are
kill switches, not synchronization. Use isolated subprocesses for destruction and
recovery. Retain exact result provenance in the authorized evidence channel.

Handle any suspected or reproduced security defect under [SECURITY.md](../../SECURITY.md).
A public implementation report must not disclose an unfixed or uncertain defect,
private report content, or reproduction details ahead of coordinated disclosure.
Existing public [#1096](https://github.com/gyldlab/keld/pull/1096) and
[#1105](https://github.com/gyldlab/keld/pull/1105) retain their recorded contract and
implementation ownership; this specification does not duplicate that work.

## 8. Review gates triggered

Permission model and public API: **required** for #541/#552/#542. Wire protocol: **required at #540's v2 boundary** and any change to completion/envelope fields; this draft cannot waive it. Unsafe: required if #542 changes native frame/delegate or WebKit lifetime operations; no unsafe code is changed in this evidence package. Dependency addition: none proposed.

Documentation: the companion patch changes architecture 03, an included source. In a complete checkout, regenerate with the repository's current generator, then run `just llms-test` and `just llms-check`. Do not hand-edit `llms-full.txt`. Rust format, warning-denied clippy, applicable full tests and required CI remain future implementation gates. None are claimed passed here.

The delegated decision records independent isolated permission/public-API and navigation/lifetime reviews, each with no blocking specification defect, plus independent refutation of the initial-construction allegation. Review bytes were bound to the approved technical commit. Final provenance changes require exact-final-diff review. No CodeRabbit review, full CI or native qualification is claimed by this contract approval; unavailable tooling does not waive those gates.

## 9. Perf impact

No measured performance gain is claimed. The policy allow path remains allocation-free after cold parsing/indexing. The trusted window lookup and revocation check use existing host ownership. Reuse the current counting-allocator harness and benchmark the actual permitted/denied end-to-end route after implementation. Queue/codec allocation and native callback latency are separate measurements. Do not justify a second policy cache or registry with an unmeasured performance assumption.

## 10. Exact-content approval selections and remaining gates

The recorded owner-delegated decision approves these nine selections together with the exact technical content manifest:

1. Decoded exact `el:` literals; reject U+002A and controls; strict windows shape; no wildcard/normalization authority.
2. Incoming channel grants only; authenticated outbound reply/send remains destination/lifetime-gated.
3. One existing keld-wv mint, checked u32, +1 renderer commit and +2 successful host-load-plus-commit; no same-document/cancelled-renderer increment; terminal exhaustion.
4. AwaitingCommit closes old-world rebind; failed host load never rolls back authority; ambiguous host same-document classification remains unshipped until #459/#542 qualifies it.
5. Final pre-Bun revocation ordering and original-document delivery validation; no replay and no promise of rollback after handoff.
6. The narrow new core forwarding call to the existing dispatcher; typed distinction between channel/native queries; implementation API representation remains under #552's separate gate.
7. Exact ChannelForbidden text preserved, exact manifest pointer supplied in trusted diagnostics under the public API gate, no accidental CallError wire extension.
8. KEL-142 wording reconciliation, KEL-143 recovery separation, the KEL-102/T4 media-integration gate, and coordination with approved/unmerged #1096/#1105.
9. Assign #542 the narrow first-proof trusted application-document binding gate and native attempt correlation, and #552 the core forwarding adapter plus composed zero-write test. These are explicit proposed scope clarifications to approve; full #460 policy and #538 surface ownership stay separate. Fresh navigation identity alone does not prove content eligibility.

Approval artifact: KEL-102 comment `8fa29b34-bdb6-470f-9f97-9cdf0aa1d0eb`, decision SHA-256 `2da2c23a53848d618f08612a1106d53283b7488601be7fd3eaa5ed95038df5af`, binds technical commit `7e0c1b89c4767f2e6625cd2bfc4d36d86b30fc26` and all five file blobs/hashes. Its permitted metadata follow-up does not change requirements or waive gates. Open implementation artifacts: #552 and #542 approved specifications, their failing-first executed tests, final source/CI gates, and real macOS evidence. These missing artifacts prevent implementation/security qualification. #541 remains open until its actual merge acceptance; no merge is authorized.

### Assessment

**GO for the approved contract and parallel #540/#464 specification/test-corpus work. NO-GO for enabling arbitrary Electron IPC, claiming #552/#542 complete, or claiming the MVP security boundary qualified.** This documentation change supplies the approved contract and acceptance plan. It supplies no implementation or native qualification, and each implementing owner must satisfy its separate specification and evidence gates.

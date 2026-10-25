# Spec: exact-literal WebView channel grants and single guarded Host dispatch (GH552)

Status: draft
Linear: GH-552 (GitHub tracker under #517; KEL-102 is the reference, not a second claim location) · Owner: GYLDLAB · Updated: 2026-10-25

**Approval boundary:** Specification-only proposal for [GH552](https://github.com/gyldlab/keld/issues/552), under the [time-bounded issue claim](https://github.com/gyldlab/keld/issues/552#issuecomment-6391210001). This file has **no production implementation** and no implied permission to merge. An independent permission-model **and** public-API review, requested from Amisha, must establish the exact approved content and change this document to **Status: approved** before any GH552 source implementation. Neither approval of GH540/GH541 documentation nor a green documentation check satisfies GH552's own feature-spec gate.

**Source baseline:** remote main `de768dee8c3a9ba512ee862fe1c409a41fac5a1f` (2026-10-25 review). It has a v0 AppProcess-only guard, one shared `dispatch_privileged` entry, v1 numeric WebView Bind/Invoke, and an Echo-only renderer-to-core route. This proposal does **not** claim those features already implement channel grants, a checked-u32 navigation mint, or v2 Send.

## 1. Goal & non-goals

Allow **only** a Host-authenticated, current main-frame document in a Host-bound window to send one application topic whose decoded **exact** `el:<topic>` literal appears in that window's immutable `windows.<window>.channels` list. Missing authority produces `ChannelForbidden / KELD-GUARD003` **before any Bun app-link frame or application handler effect**. Keep the one approved Rust Guard policy and `dispatch_privileged` as the sole authority-bearing dispatch boundary; a bounded, typed, original-document-aware core adapter may use it only after GH542 authenticates identity and GH540/465 establishes the canonical one-link v2 carriage. An allowed, warmed channel-policy/dispatcher decision performs **zero heap allocations** independently of transport serialization.

The first independent implementation candidate is deliberately narrower than the full goal: strict policy parsing, pure typed query/evaluation and counted no-effect closures can be developed without WebKit **after this spec is approved**. Pure tests cannot close the real Host/Bun zero-write or lifecycle acceptance.

Non-goals:

- No unapproved source changes in this PR; no WebKit patch, shared navigation counter, policy bypass or simultaneous compatibility implementation.
- No generic `windows` capabilities, native FS/media permission for a WebView, role/window registry replacement, OS confinement claim, live policy reload, wildcard channel language or trusted page-supplied authority.
- No second authorization evaluator, channel decoder, IPC link, credit ledger, transport reader, façade-local guard or new numeric KIPC channel id.
- No interpretation of app endpoint value bytes by `keld-core`, `keld-ipc` or `keld-guard`. No change to GH544 codec, GH1095 error/settlement, GH449 subscriptions, GH657 readiness, or GH1171/GH1181.
- No assertion of Electron parity or draw.io product acceptance based on a pure guard test, native fixture-only check, or current green documentation CI.

## 2. Spec refs

| Contract / owner | Exact consumed rule, approval status and boundary |
| --- | --- |
| [GH541 grant/navigation amendment PR #1123](https://github.com/gyldlab/keld/pull/1123), [approved owner record](https://github.com/gyldlab/keld/issues/541#issuecomment-6373759995) | Owner-delegated exact-content technical approval at approved head `7e0c1b89c4767f2e6625cd2bfc4d36d86b30fc26`, decision `2da2c23a53848d618f08612a1106d53283b7488601be7fd3eaa5ed95038df5af`, for `docs/specs/gh541-channel-grants-navigation.md` blob `05a4348a3af4ec0856dcab601e2f719d59295143`. Later draft, unmerged head `1596db78639adeda8d99d74245fd697bb4f5090e` contains blob `cccb85a1fed9650292b905b793cb585345074d2c`, which requires refreshed exact-content approval before consumption. The approved contract's §§4.3–4.6 and P01–P10 are the dependency baseline; historical GH541 approval is **not** GH552 or GH542 implementation approval. |
| [GH540 bounded IPC contract PR #1122](https://github.com/gyldlab/keld/pull/1122), [delegated approval](https://github.com/gyldlab/keld/issues/540#issuecomment-6371793024) | Owner-delegated approved spec at `3d72e492916259d89b1bf4723329e602ea6e1691`, still DRAFT/unmerged. Owns a single family-neutral `compat-control` entry, v2 `send/consumed`, opaque endpoint bytes, DATA/CONTROL reservations, authenticated WorkerLink and terminal release. Does not grant channel authority. |
| [KEL-102 original approved guard spec](kel102-host-guard-enforcement.md), [architecture 03](../architecture/03-security.md) | Host-selected immutable policy snapshot, trusted principals, native capability scopes and shared guard-before-handler dispatch. Existing `/app` permission never becomes a WebView's authority; GH541's architecture/KEL-102 amendments remain owned by #541. |
| [KEL-142 original renderer bridge](kel142-macos-renderer-bridge-api.md), [GH542](https://github.com/gyldlab/keld/issues/542) | One Host-owned checked `u32` navigation generation for current document and Guard principal; actual native qualified application-document binding, host-load attempt correlation, lifecycle/recovery and its own separately approved implementation spec belong to GH542. GH542's proposed app/trusted separate credentials remain unqualified; GH552 does not invent them. |
| [GH465](https://github.com/gyldlab/keld/issues/465), [GH543](https://github.com/gyldlab/keld/issues/543), [GH466](https://github.com/gyldlab/keld/issues/466), [GH544](https://github.com/gyldlab/keld/issues/544), [GH449](https://github.com/gyldlab/keld/issues/449) | Canonical v2 native ingress, reply/event delivery, Bun façade, endpoint codec and subscription ownership remain distinct. GH552 receives a validated generic channel and opaque bytes after their admission; it does not implement their protocols. |
| [GH552 source-evidence handoff](https://github.com/gyldlab/keld/issues/552#issuecomment-6391174207), [architecture 02](../architecture/02-ipc.md) | Current source ignores `windows` grants and denies WebView principals; current MCP explain rejects channel queries. The proper diagnostic-only selector is a separate public-API review choice, not an already implemented feature. |
| [GH464](https://github.com/gyldlab/keld/issues/464), [GH532 evidence rules](gh532-first-proof-evidence-rules.md), [GH566 manifest owner](gh566-corpus-manifest-owner.md) | Failing-first cited conformance cells remain pending; missing native evidence cannot be scored PASS. GH552 pure logic has its own permission tests and must not promote unqualified Electron rows. |

If upstream #541 merges before this spec approval, refresh these pins and compare its merged content; **do not** rewrite historical approval identity to imply new approval. The GH552 PR changes no architecture file or another owner's technical specification; an exact cross-spec discrepancy is a blocker to resolve through the owning issue before implementation.

## 3. Acceptance criteria (binary, each becomes a test)

**AC1 — strict immutable policy admission (GH541 P01/P10).** Given one host-loaded JSONC policy within the existing **64 KiB** total input cap, `windows` must be an object; every named window must be an object; `channels`, when present, must be an array of strings. Missing `windows`, missing window, missing `channels` and `[]` convey **zero** incoming channel authority. An explicit `null`, wrong type, non-string member, decoded wildcard/control, malformed Unicode or duplicate decoded object key rejects the **entire** manifest with existing `KELD-GUARD005`, before a window/Bun listener starts. Preserve existing `app` grants and currently tolerated unrelated top-level metadata. The Host captures the verified snapshot **once** per session; a same-session file edit never widens authority.

**AC2 — exact decoded literal grammar (P04).** A channel grant is a valid Unicode string whose UTF-8 bytes start with literal ASCII `el:`, followed by a nonempty suffix, with no U+002A `*`, NUL, C0/C1 controls or DEL. Surrogates fail at decoding rather than being replaced. No trimming, case-folding, NFC/NFD normalization, percent-decoding, path `/**`, regex or prefix/suffix matching. Escaped JSON letters decode into the same exact string; escaped `*` still invalidates **the whole policy**. Duplicate strings in one list add no authority; first exact match is deterministic. Any `el:rendererReqX`, `EL:rendererReq`, `el:rendererreq`, `el:rendererReq%20` or composed/decomposed variant is denied unless independently valid and listed.

**AC3 — only Host-bound window selection (P03/P05).** The Host resolves one immutable configured window key at WebView creation and associates it with an authenticated native view/document. The channel query consumes only a **borrowed Host-resolved grant selection** from the verified snapshot, not a page-provided window string, URL, WebView id, role, principal, nonce or header. A grant for window A cannot be satisfied by matching window B or a global union; reusing a numeric WebView id after destruction cannot resurrect the old selection.

**AC4 — typed channel/native capability separation (P06).** A named channel query `RendererChannel` and a native capability query `NativeCapability` have separate variants and admission paths in the single policy evaluator and dispatcher. A grant `el:fs.read` authorizes **only** the Bun application message named `fs.read`; it cannot admit a native filesystem broker or media operation. Existing AppProcess native `/app/fs/*` requests and their scope rules retain their behavior and callsites.

**AC5 — denied current request (P02/E01).** Given an otherwise **authentic current original document**, correct window binding, valid bounded v2 request and usable matching Bun role, an absent `el:rendererReq` grant produces exactly the existing `DenyReason::ChannelForbidden { channel: "el:rendererReq" }`, code **KELD-GUARD003**, kind `channel_forbidden`, fix text: "Add `{channel}` to this principal's channels list in keld.permissions.jsonc.", and the unchanged `CallError { code, message }` mapping. The final guarded effect closure never runs; independent observers report **zero** app-link frame writes and **zero** Bun listener entries. A subsequent current, explicitly granted control forwards exactly once. An identity or unavailable-link refusal **precedes** a channel-grant miss: do not reclassify such failure as GUARD003.

**AC6 — borrowed, warmed Allow (P07/P08).** The only production authority-bearing evaluator call is inside the shared `dispatch_privileged`; the **Allow closure** is the only site from which an authorized app-link forwarding operation may initiate. Its `ScopePermit` is callback-scoped and cannot escape, be cached across document changes or be constructed from a renderer value. Measured after manifest parsing and policy indexing, the exact allowed policy/dispatcher decision path performs **zero heap allocations** under a counting allocator. Native envelope parsing, diagnostic string construction and transport serialization are measured separately and do not excuse allocations in the warmed path. Repository-wide sole-evaluator completion still depends on KEL-102/T4 media integration.

**AC7 — diagnostic-only explain parity (P09; separate public-API gate).** The CLI/MCP `keld_permissions_explain` accepts a separately approved, explicitly *hypothetical* window selector for a **read-only** renderer-channel query. It resolves that key against a parsed snapshot to call the **same pure evaluator** as dispatch and returns the same allow/deny decision, channel spelling and trusted escaped JSON pointer `/windows/<key>/channels` (escape `~` as `~0`, `/` as `~1`). It may construct a *suggested* patch but **never** mint a live `Principal::Webview`, bind a WebView, return a `ScopePermit`, invoke an effect callback or write the manifest. Existing v0 app-only explain callers remain compatible; incomplete/miscombined channel/window arguments continue to fail closed. No added pointer or field appears in the frozen wire-level GUARD003 text. Approval of this CLI input/output shape is independently required before implementing it.

**AC8 — final live identity and ordering (P02/N01/N02/N09b).** A validated renderer request captures GH542's one checked-`u32` current document stamp, native-qualified application-document eligibility and trusted Host-bound window key, plus the then-admitted authenticated Bun/app-link generation. At the **final nonblocking pre-Bun handoff**, the same owner orders revocation against admission, rechecks the full captured identity and calls the one `dispatch_privileged` channel variant. If navigation/destroy or role retirement wins first, **zero** Bun writes, no retarget to a new role/document, one terminal accounting outcome and no automatic replay; a new current/granted call remains healthy. If dispatch won first, outcome can be unknown and no rollback/exactly-once external side effect is promised. No lock needed by native UI revocation may be held across blocking I/O. GH542/GH540 must qualify these real effects; passing this document does not qualify them.

**AC9 — unchanged outbound and native authority.** Window channel grants authorize *incoming* renderer→Bun messages only. Authenticated `mainResp`/event.reply retains original document/frame, while `webContents.send` captures its *current* target at send admission through GH543/GH466; these routes never inherit window incoming grants, broadcast, or permit native FS/media. GH540's one `compat-control` v2 channel, credits, body caps, canonical parser, subscribe/retirement and Host-opaque value bytes remain mandatory.

**AC10 — composed positive/negative product proof.** Once the dependencies are implemented, on real authorized macOS and a verified application document, observe `el:rendererReq` denied with no app-link write/Bun entry and then one permitted request with one actual Bun handler entry. Separately pause revocation just before final handoff (zero write when revoke wins), pause an admitted old role across Bun restart (zero successor effect), and prove a fresh granted call succeeds. Failing-first tests, exact source/OS/fixture receipt and independent single-fault RED mutations are required. Pure policy/explain tests alone cannot establish AC5/AC8/AC10 or native security; no conformance cell flips from a docs-only result.

## 4. Design

### 4.1 First-principles atoms, reuse and failure isolation

| Atom / sole owner | Trusted input → output | One fault and independent falsifier |
| --- | --- | --- |
| Policy admission, GH552 `keld-guard` | One already verified, immutable JSONC snapshot → closed window/channel table and existing app scopes | Treat a decoded wildcard or null as ignorable; invalid manifest incorrectly loads instead of GUARD005 |
| Authority identity, GH542 `keld-wv` | Actual native view/controller/frame/content-world + committed eligible document → one current checked generation + app/trusted realm credentials | Bind from just URL/id or reuse stale credential; forbidden frame/Host effects become visible |
| Window binding, GH552 Host consumer | Verified creation config + current WebView identity → borrowed immutable current-window selection | Accept page-supplied window or global grant union; window B's grant authorizes A |
| Channel decision, GH552 `keld-guard` | Host-bound WebView principal + exact generic topic + borrowed selection → typed Allow permit or GUARD003 | Replace equality with prefix, or let `el:fs.read` enter the native capability branch |
| Final effect, GH552 `keld-ipc` + narrow `keld-core` adapter | Live original-document + Bun role stamp + pure Allow → one bounded app-link handoff | Skip final guard/currentness; independent unauthorized frame and Bun handler counter goes RED |
| Carriage/settlement, GH540 and GH465/543 | Canonical bounded v2 envelope + existing WorkerLink, one credit ledger → admitted or terminal event | Reintroduce a second parser/ledger or release before native completion; quota and old-role oracle fail |
| Explain only, GH552 `keld-cli` | Hypothetical diagnostic window/channel → same pure decision and pointer, **not** authority | CLI opens live permit or uses independent match; no-effect/parity oracle fails |
| OS containment, KEL-78 | Existing execution profile → independent confinement effect | Treat a passed channel guard as proof of sandboxing; native OS/profile control disproves claim |
| Evidence provenance, GH552 + GH541/GH464 owners | Exact commits/pins + native/transport observations + mutant → scoped result | Stub/synthetic page PASS substituted for app-link effect; receipt mismatch/negative control fails |

The existing unique-key JSONC loader, `ScopePermit`, evaluator, `dispatch_privileged`, canonical kipc receiver and existing native mint **win over any rewrite**. No second policy parser, global window registry, JS-side permission cache, protocol, socket, or helper-only authority shall be introduced. No measured performance gain justifies a rewrite; the target here is semantic correctness and unchanged warmed zero-allocation allowance.

### 4.2 Policy grammar and snapshot capture

The admitted policy fragment is (illustrative, not a claim that draw.io needs every listed topic):

~~~json
{
  "windows": {
    "editor": { "channels": ["el:rendererReq"] },
    "about": { "channels": [] }
  }
}
~~~

The current `PermissionsManifest` in `crates/keld-guard/src/lib.rs:620-628` contains only `app` and intentionally tolerates unrelated top-level metadata. Extend only the `windows` subtree with strict structural validation **after** the existing unique-key JSONC decode; do not apply serde `deny_unknown_fields` to the whole manifest. Unknown unrelated legacy top-level keys remain accepted. Duplicate decoded object keys are already rejected by the shared decoder. Wrong-shaped or wildcard content anywhere within the `windows` policy invalidates the entire snapshot; there is no fail-open filtering of invalid entries. An absent `channels` key means an empty incoming list; an explicit `channels:null` is malformed. A missing window is empty authority, not a fallback to `app` or another window.

Cold snapshot validation, string allocation and indexing are allowed within existing 64 KiB budget. The host receives only an immutable verified snapshot at session creation; a later file change cannot silently reparse it or update decisions. Each channel is the already validated decoded UTF-8 literal; compared **byte for byte**. GH540 caps *message* channel name length at 256 UTF-8 bytes; no guard-side relaxed wire limit or ad-hoc double-prefix stripping may override it.

### 4.3 Proposed typed query and one authoritative dispatcher

**Proposed design sketch, subject to independent permission/public API review (not implemented):**

~~~rust
// Semantic types, not yet public Rust declarations:
enum GuardQuery<'a> {
    NativeCapability { operation: &'a str, resource: &'a str },
    RendererChannel {
        channel: &'a str,
        current_window: BorrowedHostWindowGrants<'a>,
    },
}

// The existing keld_ipc::guard_dispatch::dispatch_privileged is retained as
// the SINGLE authoritative entry around an effect closure. Adapt its
// internal query shape once, preserving existing native caller semantics.
// Do not add an independently authorizing dispatch_privileged_channel.
fn dispatch_privileged<T>(
    manifest: &PermissionsManifest,
    principal: Principal, // native Host-derived, never decoded from page
    query: GuardQuery<'_>,
    handler: impl FnOnce(&ScopePermit) -> T,
) -> Result<T, DenyReason>;
~~~

The borrowed `BorrowedHostWindowGrants` denotes a selection made from the immutable session snapshot and **existing trusted WebView-to-window association**, not an object that page metadata may deserialize, rename or choose. No public type naming alone authenticates a value: the real core adapter must obtain the borrowed selection from its native Host binding and verify GH542 currentness at handoff. A caller presenting only a numeric `Principal::Webview` or page URL is insufficient. Existing `Principal::AppProcess` path/host grants retain their native-specific, `/**`-aware matcher; the new `RendererChannel` variant instead requires a WebView principal plus borrowed Host-selected window and checks only exact `el:` literals. A channel permit cannot authorize native media or filesystem work.

There is only **one** Guard evaluator for effect-bearing decisions. The proposed signature may require adapting existing `keld-native` filesystem callsites: any such source edit must be coordinated with the native owner and keep its current golden tests; it is **not** authorized by this DRAFT. If a compatibility shim is needed, it must convert old native calls to the same enum/single dispatcher, be reviewed as one atomic public API change and have an explicit removal criterion. It must not become a second effect-bearing policy entrypoint.

On a current, valid channel miss, return exactly the existing `ChannelForbidden` variant/code/kind/fix text from GH541 §4.5. Do not put `/windows/editor/channels` into the frozen wire `CallError` message by a silent change; the optional pointer is a trusted diagnostics/MCP field. Path validation for native `fs.*` stays only on `NativeCapability`; renderer named topics are not filesystem paths. Guard selection must not allocate on the warmed allowed route.

### 4.4 Final handoff, generation and error ordering

A renderer's arbitrary data are not proof of identity. The native `keld-wv` owner validates content world, frame, WebView, qualified committed application document and the single GH542 checked-u32 navigation generation (same stamp that supplies `Principal::Webview.generation`). It captures an authenticated currently admitted Bun role/link generation **at admission**, not at dequeue. GH552's narrow `keld-core` adapter is the *only* new consumer allowed to call the shared dispatcher around a generic renderer-channel forwarding operation. It must:

1. Refuse malformed v2 input at the existing GH465 canonical decoder and require verified native identity/current document; a page-supplied target/window/generation is neither trusted nor reparsed by Guard.
2. Borrow the immutable Host-current window grant selection. If native identity, eligibility, link availability or generation is stale, return the owning typed no-effect/outcome-unknown category **before** selecting an absent-grant error.
3. Bound, charge and copy the opaque payload/metadata through GH540's existing owners. At final nonblocking handoff linearize native revocation versus forwarding, compare the captured original document **and** authenticated Bun role against current authority, then call `dispatch_privileged` exactly once.
4. Only the Allow closure can begin the original app-link write. On a grant denial the handler does not run; no Bun frame, listener entry or native capability effect exists. On revocation or role replacement first, never retarget old work to successor. If already dispatched, do not invent rollback; retain correct ambiguous outcome and release accounting when quiescent.

Never block the UI/navigation owner under a lock while waiting on Bun, Writer queue, callback or socket I/O. `event.reply` and `webContents.send` remain GH543/GH466 authenticated outbound delivery, not extra incoming `windows` permissions. GH542 must separately qualify the restricted application-document admission and native attempt correlation (GH541 N11/N06b), and GH540/465 must establish one v2 decoder/link, subscribed event route and credit retirement before GH552 can claim end-to-end AC5/AC8/AC10.

### 4.5 Diagnostic-only MCP API proposal (separate public-API gate)

Current `crates/keld-cli/src/mcp/permissions.rs` requires `principal="app"` (MCP012 otherwise), has an optional channel field that always yields MCP014, and exposes **no window selector**. Do not silently reinterpret its existing `capability/path` request or permit live renderer authority.

**Proposed additive request**, keeping the old app query intact:

~~~json
{
  "manifest_path": "/path/to/keld.permissions.jsonc",
  "operation": {
    "principal": "webview",
    "channel": "el:rendererReq",
    "hypothetical_window_key": "editor"
  }
}
~~~

This is explicitly an **offline hypothetical diagnostic**: the caller may select the named window *to ask what a Host-selected window would receive*, but that selection **never** authorizes an actual WebView. It does not require a running Host, mint a native principal, select a live Bun link, return a `ScopePermit`, or use a second string matcher. It reads the supplied policy via the existing pure parser, performs a non-authoritative lookup of `hypothetical_window_key` in the parsed snapshot, and passes a diagnostic-only window view to the same pure policy evaluator before reporting `decision: allow|deny`; on denial supplies the same GUARD003 code/message/fix and a **separate trusted diagnostic** `json_pointer` equal to `/windows/editor/channels` (JSON Pointer escaped). The diagnostic path does not construct `BorrowedHostWindowGrants`, `Principal::Webview`, or `ScopePermit`. A suggested patch is advisory only and does not change the file.

Backward compatibility and ambiguity rules: requests without a `channel` keep the v0 app-capability route; `principal=webview` lacking a hypothetical window or channel, `principal=app` plus a hypothetical window, or a channel with conflicting native `capability/args` use typed MCP validation failure, never a made-up Allow; preserve existing MCP012/MCP014 behavior for legacy unsupported shapes until an explicitly approved migration test passes. The independent reviewer must approve these exact input and output fields (including whether `hypothetical_window_key` is the final name), failure precedence and snapshots before implementing `keld-cli/src/mcp/permissions.rs` or `keld-cli/tests/doctor_mcp.rs`. No diagnostic result can cross into the authoritative dispatch closure.

### 4.6 Lifecycle, platform, migration and current-code limitations

The source boundary is already visible without new runtime work:

- `crates/keld-guard/src/lib.rs:620-628` ignores `windows`; `:784-813` rejects WebView principals with GUARD006; `ChannelForbidden / GUARD003` text is registered but not emitted by an authorized channel path.
- `crates/keld-ipc/src/guard_dispatch.rs:48-70` owns the one sanctioned effect closure and its existing native FS-specific inputs. This must remain the authority boundary for any narrow renderer channel.
- `crates/keld-wv/src/wv_link.rs:131-197` parses only v1 Bind/Invoke. `crates/keld-wv/src/wkwebview/macos_bridge.rs:323,357-389` has a wrapping u64 counter and one nonce; the checked-u32 native document authority belongs to GH542 and its own approved spec. No second local counter or unchecked cast is allowed here.
- At the exact merged baseline, `crates/keld-core/src/app_session.rs` has an Echo-only renderer forwarding route; it does not yet know a real generic named renderer-to-Bun operation. Its future GH552 adapter must be coordinated with its owner's current head/other workers.
- The existing single supervised WorkerLink, bounded ring and primary role authentication are mandatory; GH540 owns compatibility credits and authenticated delivery, not GH552.
- macOS is the only required real-native first-proof lane. Engine-neutral pure tests may execute on macOS/Linux/Windows; they do not claim WebView2 or WebKitGTK native compatibility. KEL-78 still owns OS sandbox/strict confinement.
- Migration: **none** for stored user data. Preserve existing v0 permission files' accepted unrelated metadata and native `/app` scopes; missing window grants default to deny. `keld migrate` grants-source logic belongs to GH499, not this implementation. Existing MCP callers keep v0 behavior. No temporary independent evaluator persists.

## 5. Boundaries

This **spec-only** PR writes exactly `docs/specs/gh552-exact-channel-grants-guard.md`; generated `llms.txt` / `llms-full.txt` only if the existing documentation generator actually selects this new document. No Rust/TS/manifest/architecture/CI file is modified in this DRAFT.

**After independent Status: approved only**, the named GH552 implementation may change these bounded ownership paths in separate PRs and only after refreshing current claims:

| Future GH552 ownership | Candidate path(s) | Dependency and boundary |
| --- | --- | --- |
| Pure policy parser and evaluation | `crates/keld-guard/src/lib.rs`, `crates/keld-guard/tests/manifest_parser.rs`, new focused guard tests under the same crate | After GH541 integration and GH552 spec public API approval. Preserve AppProcess, metadata and cold parser behaviors. |
| Single shared guarded query | `crates/keld-ipc/src/guard_dispatch.rs`, focused tests `crates/keld-ipc/tests/guard_dispatch.rs` | Narrow query API; native FS callsite compatibility requires independent current-owner review if signature changes. No new authorized dispatcher. |
| Diagnostic-only explain | `crates/keld-cli/src/mcp/permissions.rs`, `crates/keld-cli/tests/doctor_mcp.rs`, only their necessary tool-schema snapshots | Additive reviewed public API; no live privilege, no renderer-selected window. |
| Final Host forwarding adapter | Small GH552-owned extraction or narrow change in `crates/keld-core/src/app_session.rs`, focused composed native-effect tests | Wait on qualified GH542 current stamp, GH540/465 canonical v2 send and Bun-generation owner. Coordinate existing core/file writer first; must prove P02/N02/N09b. |

**Must not touch or borrow approvals from:** GH542 `keld-wv` navigation and WebKit script/handler source, GH540/465/543/544 wire/codec/credit tables, KEL-142 bridge, GH449 registry, GH1171/GH1181, native FS/media owner files, root Cargo/CI workflows or sibling agents' worktrees. Original KEL-102, GH541 and GH540 approval artifacts remain historically immutable. If any required architectural/public API amendment falls outside these paths, stop and route to its existing owner before implementation.

## 6. Tasks (each approximately one PR; ordered, no placeholders)

- [ ] **T0 — this GH552 documentation PR:** one 10-section draft; pin approved GH541 technical content and GH540 boundaries; run lightweight docs lint, source/fingerprint and placeholder checks; request Amisha's independent permission/public API review. **STOP at draft** until an explicit exact-content GH552 `Status: approved` decision. No product patch follows merely from this task's green CI.
- [ ] **T1 — independent pure policy slice:** on an approved GH552 spec and integrated GH541 contract, first land/run *failing-first* P01/P03/P04/P06 typed guard tests; extend only the existing `PermissionsManifest` parse and exact window lookup; preserve old `/app` behavior and unknown metadata; flip the guarded tests only on the actual implementing change. No WebKit dependency.
- [ ] **T2 — one authoritative dispatch API:** implement the independently accepted Rust `GuardQuery` shape inside one shared `dispatch_privileged`; adapt any native caller only with its owner's review. Prove failure-first allow/deny effect closure, callback-scope permit and **zero allocations** on the warmed Allow. Pure closure effects are not real Bun writes.
- [ ] **T3 — separate MCP diagnostics API:** following Amisha's API selection, extend only the read-only `keld_permissions_explain` request/response; preserve legacy error behavior, test hypothetical selector/escaped pointer, parity with pure evaluation and inability to reach a live native permit. No new authority.
- [ ] **T4 — qualified adapter and real enforcement:** only after GH542 native N11/currentness and GH540/465 bounded v2/WorkerLink acceptance, integrate the narrow core adapter. Prove GH541 P02/N02/N09b and GH552 AC5/AC8/AC10 on authorized real macOS with independent app-link writer/Bun listener counters and each single-fault mutant.
- [ ] **T5 — integration/review handoff:** before any enabling of generic Electron IPC, have security/permission, public API, shared-dispatch/KEL-102 T4 media, GH542 native, GH540 transport and conformance owners review their exact files/receipts. Preserve remaining GH464 Electron UNKNOWN/fail cells and known draw.io gaps. Merge requires separate user/owner authority, not this spec PR.

T1/T2 pure guard/unit work may be designed in parallel with GH542's spec, but **must not** claim the GH552 feature complete; T4 is blocked until qualified real native and transport prerequisites. Do not implement T1–T5 in this DRAFT branch.

## 7. Test plan

Every future executed case records exact source SHA, implementation PR, command, platform, raw effect/denial and **one** deliberately faulty mutation that the independent oracle rejects. Failing-first means the real unimplemented source shows the expected failure before its implementation, not a skipped/red CI lane. Tests use event/barrier synchronization; timeouts are kill switches, never success or proof of no effect.

| Test / GH541 mapping | Positive + denial observable | One-fault RED mutation |
| --- | --- | --- |
| `G552-P01-manifest` / AC1, P01 | Missing list is empty authority; correct policy loads; null/wrong shape/non-string, escaped `*`, U+0000, C0/C1/DEL, duplicate decoded keys and 64 KiB+1 reject whole policy GUARD005; valid sibling remains valid. | Remove decoded-string or nested-shape validation; hostile policy incorrectly loads. |
| `G552-P04-literal` / AC2, P04 | Exact `el:rendererReq` matches; `el:rendererReqX`, case/Unicode/percent variants deny unless independently granted. | Use prefix, case-fold or Unicode normalization; unauthorized variant becomes Allow. |
| `G552-P03-window` / AC3, P03/P05 | Only Host window A's listed literal admits A; B's grant cannot satisfy A; page-provided window/header ignored. | Use a global union or select key from page JSON; cross-window callback runs. |
| `G552-P06-native-isolation` / AC4, P06 | Renderer `el:fs.read` never enters native filesystem broker; native AppProcess `fs.read` keeps its unchanged allow/deny scope tests. | Route channel variant into native filesystem selector; independent marker file appears. |
| `G552-P02-effect` / AC5, P02/E01 | With live native current document and role, absent literal => full GUARD003 text and **0** writer frames/0 Bun handler entries; neighbor allowed => **1** of each. | Bypass guard/forward outside its Allow closure; independent forbidden Bun effect appears. |
| `G552-P08-allocator` / AC6, P08 | After cold parse/index, warm exact Allow and dispatcher query have exactly 0 measured heap allocations; deny allocation is measured separately. | Clone the channel or window key on every Allow; allocator observes >0. |
| `G552-P09-explain` / AC7, P09 | Hypothetical named window yields same pure result and escaped JSON pointer; writes neither manifest nor app link; existing app query unchanged. | Use alternate matcher, leak a live permit or let hypothetical key enter Host adapter; parity/no-effect fails. |
| `G552-N02-revoke` / AC8, N02/N12 | Pause queued work at final pre-Bun boundary; native revocation winner => 0 frames/handler entries; one fresh valid control forwards and native UI can progress despite blocked transport. | Omit final stamp check or hold revocation lock across blocked write; forbidden effect or UI deadline failure. |
| `G552-N09b-role-recovery` / AC8, N09/N09b | Old captured Bun role queued across restart cannot dispatch to successor; no replay/credit theft; fresh same-document admitted call works. | Select current link at dequeue without comparing captured role; successor gets old request. |
| `G552-N11-doc-admission` / AC8/AC10, GH542 N01/N11 | Original native qualified document/window/checked `u32` stamp equals guard principal; foreign document and unapproved `file:` claims yield no effects. | Mint identity from URL/WebView id or wrap generation; stale sender reaches Bun. |
| `G552-C14-two-requests` / AC9/AC10, GH540 E10 | Genuine unmodified draw.io `rendererReq/mainResp` from real main handler for two sequential distinct reqIds, page callback **and** independent native/transport release before second admission; denied control zero effects. | Return canned `false`, fake page callback or release credit before native drain; receipt/barrier RED. |

Do not combine all rows into one giant mock. GH552 owns the pure guard, dispatcher and diagnostic rows; GH542 owns real native navigation; GH540/465/543/466 own transport and application effects; GH464/567 own pinned conformance evidence. Run real native/failing-first tests only when these owners authorize their respective code. An allow with a synthetic callback is not AC5/AC10.

## 8. Review gates triggered

- **Permission model — mandatory, Amisha independent review.** Exact decoded policy syntax, whole-policy malformed rejection, immutable Host window borrow, native capability separation, default deny, no unauthorized Bun write, correct error precedence and no double evaluator.
- **Public API — mandatory, Amisha independent review.** Proposed Rust typed query/dispatcher callsite migration and additive **hypothetical** MCP selector/output; preserve legacy callers and frozen GUARD003/CallError wire texts. A reviewer must affirm a hypothetical diagnostic window is never a live Host binding.
- **Wire protocol — GH540/GH465 owner's separate gate if any wire/source change.** This spec only consumes approved v2 shapes; it creates no channel entry or serializer and cannot silently widen codec/reply text.
- **Unsafe/native — GH542/WebKit owner separate gate.** No unsafe or macOS native source in this document. Later real native identity and injected handlers need their own approval/device receipt.
- **Dependency changes — none.** No Cargo/Bun dependency or generated schema change in this PR.
- **KEL-102 shared dispatch integration:** the repository-wide sole-authoritative-evaluator claim remains gated on T4 media receipt; no bypass or policy-copy exception is approved.

The exact draft must be reviewed at its actual PR commit, not assumed approved because GH540 or GH541 specs carry approved labels. Amisha's review is requested, never fabricated.

## 9. Perf impact

No speed or memory improvement is claimed. The target is **zero heap allocations for warmed exact Allow** in the policy/dispatcher decision only, with a measurable counting-allocator oracle. Cold JSONC parsing/indexing may allocate within the existing 64 KiB snapshot cap; denial formatting may allocate. GH540 separately owns 4 KiB first-proof payload, 256-byte channel length, one DATA record per direction, reserved CONTROL capacity, checked per-hop retained byte/scratch bounds and terminal native release. Do not count transport allocations as guard allocations or add a second queue/ledger. Measure cold parse, warm Allow, denied path, final Host handoff and real native latency as **separate** attributable samples after implementation; record actual values before making any performance claim. No diagrams/bench threshold in this documentation-only PR.

## 10. Open questions (blocking exact-content approval selections)

1. **Public Rust query shape and callsite compatibility — Amisha / GH552 + existing native-dispatch owner:** approve the proposed `GuardQuery` enum with borrowed Host-selected grant handle and **one** `dispatch_privileged` authority-bearing entrypoint, or select a comparably narrow type with a proven single caller. State whether existing native AppProcess callsites may be mechanically adapted in GH552's future approved implementation PR or must have a separately claimed owner change. No `RendererChannel` native capability fallback.
2. **MCP public diagnostic input — Amisha / GH552:** approve the additive `hypothetical_window_key` selection and conditional `principal="webview"` + `channel` shape, diagnostic pointer and missing/invalid combinations, or supply the exact alternative before code. Confirm that hypothetical window selection can never enter a live effect path, returns no `ScopePermit`, and does **not** change the frozen GUARD003 wire text.
3. **Core adapter boundary and final order — GH552 / GH542 / GH540:** approve the narrow `keld-core` forwarder scope, source of its host-current window grant handle, native document stamp/liveness accessor, captured app-link generation and bounded nonblocking final guard/revocation ordering. Current WebKit wraps `u64`; GH542's checked-`u32` native implementation and qualified application-document N11 are separate unapproved source gates. Pure GH552 unit tests may proceed only after this spec approval; no composed Bun proof can proceed by borrowing GH541 spec approval.
4. **MCP/manifest release compatibility:** affirm unrelated unknown top-level metadata remains tolerated, strictness applies only inside `windows`; no new error codes or mutable file-based hot reload. Coordinate any generator/tool-schema or shared native caller diff before implementation, not within this spec PR.
5. **Upstream merge / independent source gates:** GH541's reviewed technical spec and GH540's approved wire spec remain DRAFT/unmerged; completion depends on their owners' merge authority and GH464/GH542/GH465 prerequisites. Recheck actual final blobs and claims before starting each future PR. An approved GH552 document does not override those dependencies.

**Next explicit action:** independent permission-model/public API reviewer selects these open decisions, records a decision on the exact GH552 PR head, and only then may the owner update **Status: approved** in a separately reviewed document change. **Stop here; no GH552 production code is authorized by this DRAFT.**

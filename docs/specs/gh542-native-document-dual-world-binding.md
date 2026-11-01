# Spec: native checked-navigation and dual-world document binding (GH-542)

Status: draft
Linear: GH-542 (GitHub tracker under #517; KEL-102 reference) · Owner: GYLDLAB / @0monish · Updated: 2026-10-25
Disposition: OPEN / DRAFT / DO NOT MERGE. This is a proposed native implementation contract, **not** production implementation or approval of #484 A1/A2. No new manifest grant, native endpoint, product evidence or permission is admitted by publication.
Baseline: `origin/main de768dee8c3a9ba512ee862fe1c409a41fac5a1f`. Scope is the #542 native document/navigation owner, with independent public/native/unsafe/security and exact-content approval outstanding.

## 1. Goal & non-goals

Give each live macOS `WKWebView` exactly one Host-minted, checked-`u32` **navigation generation** and one qualified current committed application-document identity. Bind two distinct native credentials to that **same** document generation: one exclusively for the KEL-142 trusted bridge content world, one for the separately decided #441/#484 app-preload content world. An admitted request must prove its current native world/controller/WebView/main-frame/role provenance, present a stored and matching credential for that particular world, carry the one native navigation stamp into the request, and create the same `Principal::Webview` generation at final Guard dispatch. An actual replacement document revokes both realms together; Bun recovery retires old app-link work without rewriting a surviving native document.

The first demonstrable result is a real macOS WKWebView with a pinned eligible application document, one admitted operation in each appropriately authorized realm, and **zero native/Host/Bun forbidden effects** after independently injected stale/cross-realm/page-origin faults. Early document-start script publication is allowed as inert setup; bridge authorization begins only after native qualification.

Non-goals: implementing code or changing current #541/#540/#441/#484/#552 sources in this PR; allowing page-world native handlers or treating `BroadcastChannel` as authenticated sender provenance; app-script/DOM/URL self-certification; a second Core generation mint, new renderer principal, new `el:*` grants, unbounded prebind queue, codec/credit owner, extra KIPC channel, shadow renderer transport, arbitrary remote/subframe IPC or general navigation policy (#460/KEL-79); claiming native success, CodeQL coverage, Windows/Linux, installation or draw.io product PASS from this document.

## 2. Spec refs and current-source facts

- **Approved design, unmerged:** [GH-541 #1123](https://github.com/gyldlab/keld/pull/1123), `docs/specs/gh541-channel-grants-navigation.md`, technical decision `7e0c1b89c4767f2e6625cd2bfc4d36d86b30fc26`; KEL-102 decision `8fa29b34-bdb6-470f-9f97-9cdf0aa1d0eb`. One Host/WebView checked-`u32` mint, initial 0 unbound, actual cross-document commit rotation, earlier trusted Host-load revocation, same-document/cancel preservation, Bun recovery as a separate axis, immutable current-resource eligibility, final Guard handoff.
- **Approved design, unmerged:** [GH-540 #1122](https://github.com/gyldlab/keld/pull/1122), exact head `3d72e492916259d89b1bf4723329e602ea6e1691`, `docs/specs/gh540-compat-ipc.md` §§4.5–4.9. One `keld.wv-link` v2 closed envelope (`bind`, `invoke`, `send`, `consumed` and corresponding native replies), one `compat-control` channel, existing delivery/credit owners and bounded retirement. The private v2 `document` field is a native-minted opaque binding value, not a new caller-selected generation. This spec requests #540 review of per-world bindings under that existing field; **no wire addition is approved here**.
- **Decided world separation:** [GH-441](https://github.com/gyldlab/keld/issues/441#issuecomment-6022098697) selects a distinct app `WKContentWorld` and its own world-scoped handler. KEL-142's trusted native handler and nonce must never be shared with or exposed to the app/page world.
- **Consumers:** [GH-484 #1155](https://github.com/gyldlab/keld/pull/1155) is still DRAFT/CHANGES_REQUESTED; its A1 page-to-app alternative via `BroadcastChannel` is **TEST ONLY**. Native app-handler provenance proves only the last app-world hop. It cannot attest to the earlier page sender. A2 (data-only page-native ingress) is an unresolved owner/security decision and this spec neither chooses nor enables it. #467 defines the separate contextBridge/facade owner. #552 owns literal grant checks and `dispatch_privileged`; #499 owns the pinned migrated application/source; #459 owns the actual host full-document load caller; #464 owns listener conformance.
- **Feasibility, not Keld proof:** [GH-542 native receipt](https://github.com/gyldlab/keld/issues/542#issuecomment-6383451853) and [GH-467 native receipt](https://github.com/gyldlab/keld/issues/467#issuecomment-6383445584): real macOS WKWebView 26.5.1 observed two isolated credentials under one **test-owned** checked epoch and 33 native cases with negative controls. The fixture did **not** prove Keld's production URL eligibility, actual Guard/Host/Bun forwarding, navigation-attempt races, Bun recovery, real window destruction or native resource bounds.
- **Current production source:** `crates/keld-wv/src/wkwebview/macos_bridge.rs` `BridgeState` still has private `navigation_generation: u64`, uses `wrapping_add(1).max(1)` on navigation-start, mints one `document_nonce` in `bind()` and rejects a second bind. It checks native message `world()`, `frameInfo().isMainFrame()`, `webView()` and controller pointer before `try_send`. The current `crates/keld-wv/src/wkwebview/mod.rs` invokes `navigation_started()` from existing navigation paths. Neither existing private value nor its diagnostic log is a qualified #542 principal or two-world admission fact. Guard webview dispatch still must consume a single real native stamp, not mint another counter.
- Existing governance: `docs/architecture/02-ipc.md` §§2,5, `docs/architecture/03-security.md` (default deny and WebView principals), `docs/specs/kel142-macos-renderer-bridge-api.md`, KEL-102 and `docs/agents/spec-template.md`. This draft proposes an implementation detail within their approved authority boundaries. If the #540 owner determines per-world `document` interpretation changes normative wire semantics, amend/approve #540 and the architecture **before** implementing; do not silently reinterpret approved bytes.

## 3. Acceptance criteria (binary, each becomes a test)

| ID | Input / action | Required observable; independent fault that must fail |
|---|---|---|
| N01 | Create one real WKWebView without committed eligible document | Native generation is `0`/Unbound, no credential or privilege and zero prebind queue; accept initial bind → RED |
| N02 | Native main-frame initial eligible commit, with pinned actual resource | Exactly one checked advance `0 → 1`, one qualified URL/source/document stamp; spoof a matching text URL with foreign resource → refusal |
| N03 | Install separate app and trusted script-message handlers | One document generation with **two different** independently random 128-bit-or-stronger world credentials held by native owner; shared token or shared trusted handler → RED |
| N04 | First valid `bind` after native qualified commit | Respond only to the originating registered main-frame world with that world's opaque `bound.document`; wrong view/controller/world/frame or repeated bind cannot mint an alternate authority |
| N05 | `invoke`, `send` or `consumed` from either realm | Admit only if **both** native stored credential exists and supplied nonempty `document` equals that world's stored value, the realm permits the kind, and current stamp/role match; missing stored+supplied token or swapped credential → zero forwarding |
| N06 | App handler receives a page-relayed message | Native can attest to the app-world **last hop only**; page-origin claim/`BroadcastChannel` label never supplies principal, grant, exposure policy or credit; forged page-origin receipt → zero privileged effect |
| N07 | Observe actual renderer cross-document **commit**, even same final URL | Revoke both realm credentials and prior deliveries/receipts; checked advance exactly once; late old call and stale bind completion fail |
| N08 | Hash change, `pushState`, `replaceState` or real same-document history | Preserve native stamp, both live realm credentials, current pending work and valid grant context; rotate-on-hash mutation → RED |
| N09 | Cancel/fail renderer-initiated provisional navigation without replacement commit | Preserve current document and ability to send; provisional-start invalidation → RED |
| N10 | Accept trusted Host full-document load/reload | Revoke before calling engine; advance checked epoch, enter AwaitingCommit with one native attempt correlation; even failed load never restores old bind |
| N11 | Host load commit/failure/cancel and A→B late A failure/cancel before or after B commits | Only the matching current attempt can commit with a second checked increment or fail/cancel into FailedClosed; duplicate commit is no-op; obsolete A callback releases only A retained work/charges exactly once, never B's lease, and preserves B AwaitingCommit or B Live with its bindings and eligible work; unknown callback without owned work releases nothing |
| N12 | Actual uncorrelated replacement commit or unauthorized redirect | Revoke previous document and stay unbound/failed-closed until independently qualified transition; same URL cannot bypass; a page-provided URL cannot authorize |
| N13 | Native `u32::MAX` plus one required advance | Enter terminal Exhausted; no saturation/wrap/truncation/reset/zero media shortcut, no realm rebind, no request or delivery |
| N14 | Destroy WebView, native controller or target document | Revoke both credentials and pending callbacks; no same-address/new-view reuse and no post-destruction effect |
| N15 | Bun/app-link role generation recovers while WebView survives | **Preserve** genuine native document generation and committed URL; revoke original app-link/correlation and all old role-bound work, reauthenticate successor before any new forwarding; no automatic replay or old receipt retargeting |
| N16 | Dispatch to currently authenticated main after native admission | One captured checked generation used identically in stamped bridge request and `Principal::Webview { id, generation }`; any independent Core counter → RED |
| N17 | Native dispatch races retirement or blocked app-link | Final nonblocking check/forward uses existing #552 Guard and #540 ledger; if revocation wins, zero Host/Bun writes; if prior dispatch won, retain Outcome Unknown without replay; never hold native lock across blocking IO |
| N18 | Normal sender with ungranted literal `el:topic` | Existing GUARD003/`ChannelForbidden` before any Bun write, including with valid realm credential; bypass Guard → RED |
| N19 | Prequalification first-inline app setup sends early `bind` or `send` | Deterministic local refusal/`KELD-WV-011`-class not-sent outcome, zero native queue and no Host/Bun effect; never silently drop, automatically wait, replay or grant from script visibility |
| N20 | Record limits or native delivery after revocation | No new per-realm allowance; use one #540 total credit/FIFO/quiescence owner with bounded records and stale delivery rejection; unlimited rebind queue or second world quota → RED |
| N21 | Classify supported platform/engine | Only native macOS WKWebView qualifies; Windows/Linux and BFCache/restore, remote and arbitrary subframe proof remain UNKNOWN until separate real-device oracle |
| N22 | Report product/security result | Keld-only model/probe/docs tests do not turn GH-551 Electron navigation rows 10–12 from UNKNOWN or establish GH-589 draw.io product PASS; source-only claim of native/CodeQL passing → reject |

## 4. Design

### 4.1 One owner per decision-bearing atom

| Atom / owner | Trust boundary and input → output | First falsifier |
|---|---|---|
| Navigation and document / **#542 keld-wv** | Actual native main-frame transition + approved resource identity → one current checked `u32` generation, native attempt, immutable committed URL and current document stamp | stale native commit, forged URL or wrapping counter enables a bind |
| Realm credential / **#542 keld-wv** | Qualified current document + native registered app/trusted world → separately stored opaque credentials in at most two live slots | swap world token or allow absent stored token |
| Role/link / **KEL-75/KEL-143 existing owner** | Authenticated live Bun app link → current role generation, retirement and correlation | retired G1 completion serves G2 |
| Grant and principal / **#541/#552 Guard/Core owner** | Captured native stamp + trusted window binding + literal grant → one `dispatch_privileged` decision, before forwarded effect | native token alone bypasses GUARD003 |
| Encoding/credits / **#540/#464 existing owner** | Closed v2 envelope and role-wide capacity → bounded ordered forwarding/retirement | second world gets independent allowance or control deadlocks behind DATA |
| Page/preload / **#441/#484/#467** | Untrusted page data → approved app-local method only if independently selected | app-world last hop counted as authenticated page-origin intent |
| Product proof / **#587/#589** | Real Host/Bun/OS evidence + qualified attempt census → truthful cell | artificial app log or screenshot minted as Host receipt |

No new principal, second navigation counter, world-specific generation, auth channel, global Native registry, timeout-renewal map or scored product corpus is introduced. The approved KEL-142 `BridgeState`/installed handler and native UI loop are the reuse seam. This **draft spec** changes no boundary; its proposed future native implementation strengthens the existing admission boundary and must pass independent security/public/unsafe review.

### 4.2 Native state and checked navigation transitions

Conceptual private state, **not** a committed API or second registry:

```rust
enum NativeDocumentPhase {
    Unbound { generation: u32 }, // initial 0, no authority
    AwaitingCommit { generation: u32, attempt: NativeAttempt },
    Live { generation: NonZeroU32, document: QualifiedDocument },
    FailedClosed { generation: u32 },
    Exhausted,
    Destroyed,
}
struct QualifiedDocument {
    // Existing Host-owned WebView lifetime and real committed main frame.
    webview: WebviewId,
    committed_url: NativeCommittedUrl,
    approved_resource: HostValidatedAppResource,
    native_commit: NativeCommitIdentity,
    // Two private credentials in this SAME native document generation.
    trusted: Option<RealmBinding>,
    app: Option<RealmBinding>,
}
struct RealmBinding {
    native_credential: Secret128Plus,
    world: NativeRegisteredWorld,
    handler: NativeRegisteredHandler,
    controller: NativeRegisteredController,
    main_frame: NativeMainFrameIdentity,
}
```

Types above name acceptance boundaries; new public Rust types or WK object wrappers need the owning native/API gate. A `NativeAttempt` is native load/commit correlation metadata, **not** another generation or a renderer-supplied request ID. A document also retains one Host-instance/WebView lifetime identity so recycled pointer values cannot resurrect stale state. A native URL is the effective **committed main-frame destination after redirects**, corroborated with the actual validated package/resource object; provisional `request.url`, DOM `location` and equality to a prior URL do not qualify it. If the engine cannot supply an independently correlated committed URL/frame, binding remains refused pending a native oracle.

| Native event | Checked epoch transition | Authority / attempt action |
|---|---|---|
| Create WebView | Unbound(0) | No realm slot; no queued bind; no Guard dispatch |
| First qualified native main-frame commit | 0→1 | Qualify exact committed URL and resource before issuing realm credentials |
| Renderer provisional start/cancel | +0 | Keep current committed document, credentials and pending work unless a replacement actually commits |
| Real renderer replacement commit (including reload to same URL) | +1 | Retire both credentials, native evaluations and old document delivery; qualify new document only under checked native identity |
| Same-document history/hash | +0 | Keep both realm bindings and outstanding work |
| Trusted Host full-document load command | +1 **before** engine call | Revoke both realms and old pending work; store one current expected native attempt, no implicit rollback |
| That current Host attempt commits | +1 | End AwaitingCommit; qualify actual resource and create one new doc/realm binding opportunity |
| Current Host attempt fails/cancels | +0 further | Only when matching the current AwaitingCommit attempt: retire that attempt into FailedClosed; old document remains revoked; explicit new trusted load required |
| Obsolete/unknown Host-attempt callback | +0 | Ignore late A failure/cancel or other mismatched callback for current-document state, but release only that callback's old attempt work/charges once if still retained (already released or unowned: no debit); before B commits keep AwaitingCommit(B), after B commits keep Live(B) with its realm bindings and eligible pending delivery; preserve B current resource lease, never retire B or change its epoch |
| Unknown actual replacement commit | +1 and closed | Revoke old document; never infer qualification from URL sameness |
| Bun role restart while WKWebView survives | +0 | Preserve native document and world-specific native binding identities; retire prior role/link/correlation, require newly authenticated role for fresh sends |
| Native WebView destroy | terminal | Zero realm slots; pending delivery quiescence; no rebind |
| `checked_add` would exceed `u32::MAX` | Exhausted forever | Refuse bind/dispatch/delivery for that WebView lifetime |

The native UI-loop must own event serialization/deduplication and check callback attempt identity before any failure/cancel transition. An obsolete callback may finalize **only its own bounded callback-held work and accounting** through the existing #540 owner; each charge is released at most once, and if earlier retirement already freed it, the callback performs no additional debit. This cleanup must never alter current B's reservation, credentials, state or pending delivery. Callback-owned work is reclaimed on completion without a new historical-attempt map or unbounded tombstone ledger. A second callback for the **same** observed native commit is idempotent. A stale A failure after B starts or commits is not an unknown actual replacement commit; a genuinely observed uncorrelated main-frame replacement remains a separate fail-closed event. A different committed document cannot be mistaken for a duplicate merely because its URL/text/pointer matches. The recorded committed-document URL/resource is immutable *for that document*; a hash or history-API change may change the visible URL without creating a replacement document or authorizing a new source. Successful application document eligibility is checked at every replacement, including redirects, frame changes and restored pages; BFCache is UNKNOWN until a real tested restore path exists. The `0` initial generation is unbound, never a media/renderer shortcut. The existing media-principal generation-0 residual remains under KEL-102; this spec does not silently fix/waive it or allow it for compat.

### 4.3 Two realm credentials under one stamped document

The native owner has exactly two accepted realm slots per current document: `TrustedK142` (existing isolated bridge world/handler) and `AppPreload` (separately installed app content world/handler under #484). Each slot has a **different** native 128-bit-or-stronger random credential, allocated only after commit/source eligibility and native `WKScriptMessage.world`/controller/view/main-frame provenance are checked. Allocation failure closes the bind attempt without a weak/fallback token. Native keeps the authoritative credential value; the script in **only its own** isolated world receives its corresponding private value. The untrusted page sees neither one. Tokens and renderer-supplied `document`/headers can never choose window, role, URL or generation.

Under the currently approved #540 v2 JSON shape, `bound.document` and subsequent `invoke/send/consumed.document` are **opaque private binding credentials**; the *single shared native document generation* stays internal and is attached by the native owner. The trusted and app `document` strings therefore differ while referencing the same Host native committed document. This is a proposed #540 owner clarification of the existing field, **not authority to add a token field or change v2 bytes**. If #540 judges this incompatible with its approved semantics, implementation remains blocked until the owner approves the minimal schema/document-binding amendment. There is no default to a shared nonce.

Native bind checks first: current eligible committed main frame; exact live `WKWebView`, `WKUserContentController`, world-scoped registered handler and native `WKFrameInfo`; expected role and allowed operation set; and an unoccupied slot for that realm/current document. Inbound private `bind` is initially tokenless **only because it is asking the Native owner to mint a new credential**; it can never skip the committed document and native-origin checks. All subsequent requests require simultaneously: `stored_credentials[realm] == Some(secret)`, `supplied_document != empty`, byte-equal supplied secret, matching immutable committed document/attempt, and current live route/role. There is **no** `Option == None` admission equality, empty/zero shortcut, shared token, fallback to another realm, or token accepted after retirement. Duplicate Bind cannot reset credits or replace a live slot.

Role provenance is separately authenticated by the existing Host app-link owner: native app-world provenance says only *which installed realm delivered the last message*. It does not assert a page sender or a Bun role. A `send` from AppPreload still needs the canonical exact `el:channel` Guard grant and the current authenticated primary app role; TrustedK142 operations retain KEL-142's strictly narrower operation/channel contract. A page message has no native-authenticated privileged role merely because an app handler later forwards its bytes.

### 4.4 Eligibility and the early-bind race

Initial scripts may be installed at `AtDocumentStart` in each designated world, before native WebKit has produced the confirmed committed app document. That makes APIs visible, not authorized. The native entry state is `Unbound` with **zero queued Bind/Invoke/Send/Consumed**, zero provisional credential, and no Host/Bun forwarding. An early Bind or legitimate early renderer request receives an explicit bounded local `KELD-WV-011`-class **never-forwarded-to-Host/Bun** refusal through its applicable existing error owner; receiving a rejected native handler callback is not evidence of an accepted app-link request. It is never silently dropped, replayed, held until commit, or reclassified as permitted because a page wrapper is visible.

After an independently verified current native commit and source/resource qualification, the existing registered isolated-world owner may trigger one **fresh**, explicitly correlated Bind attempt using its world-scoped native route; the earlier request is not stored or resurrected. Failure of callback/evaluation/bind/credential allocation is terminal to that attempt, with clear diagnostics and zero effect. Do not create an extra JS promise queue, new event transport or implicit restart loop. `bound.ready` or a route-state notification is informational and cannot itself prove eligible URL, authenticated Bun role, permission grant or consumed credit. New operations may start only after those owning gates pass.

The first-proof source admission is narrow: the verified stage/package app-resource identity and approved source closure from #499/KEL-102, native main-frame commit, and present policy-eligible window binding all match. A `file:` prefix, identical path spelling, DOM `location`, main-frame claim in JSON, redirect target text or current app name is insufficient. Arbitrary remote content remains no-channel until #460/KEL-79's general navigation policy is separately qualified.

### 4.5 Final dispatch, revocation and aggregate resources

Use the existing native bridge owner to snapshot `(Host/WebView lifetime, checked navigation generation, eligible native committed document, realm credential, real frame/world/controller, authenticated role/app-link generation, operation identity)`. Before **every** enqueue/forward, revalidate that stamp at the final nonblocking #552/`dispatch_privileged` boundary. The Guard principal is constructed from **that same** `WebviewId` and `NonZeroU32` generation; there is no alternate Core mint or inferred owner. Literal grant evaluation, source exposure/registration, authenticated app link and v2 direction/type checks all remain independent requirements. The final handoff and revocation have an explicit linearization point: if native revocation wins, zero app-link write; if a prior handoff wins, a dispatched effect may still finish, so represent uncertain completion rather than claim rollback or replay. Never hold a native navigation lock across blocking socket/Bun/JS/Guard work.

Revocation is effective when native state changes, before its later `DocumentRetired`/`route-state` control notification. Retire both realm slots together on real replacement commit, early Host load, WebView destruction and checked exhaustion. Revoke all old document delivery and native callback identities; stale evaluation completion releases only its old resource reservation and never delivers to a new document. On Bun recovery, retain the surviving **native** document stamp and native credentials per #541, but revoke old app-link generation/role-bound correlations, queued old work and old permissions association; the new role must authenticate and pass a **new** final Guard check. Never let prior role's credential possession bypass new link admission. If a distinct rebind-on-recovery mechanism is later proven necessary, seek its own exact owner amendment, without rotating navigation due solely to Bun restart.

Use exactly #540's existing closed `keld.wv-link` v2 envelope and aggregate per-role credits, not per-world credit windows. At most one DATA record per direction across the role, one per document, four bounded independent CONTROL records and one canonical FIFO/retirement path as specified by #540; all queued/retired/cancelled records count until their owner proves release. A repeated Bind never resets quota. Before-bind retention is zero. The native owner retains at most two active credential slots per WebView/document, one current native navigation attempt, and bounded existing evaluation/diagnostic slots; no historical secret map, infinite tombstones or hidden duplicate queue. Bound each credential to the existing 16-byte random source minimum, use bounded encoded JSON/UTF-8 and fixed diagnostics, and never print raw credential/payload. Every accepted and refused native handler callback is classified within existing #540 size/credit and #1095 terminal owners. CodeQL/source scanning can later examine code but **does not prove** native world or Host effects.

### 4.6 GH-484 A1 / BroadcastChannel provenance prohibition

The #484 A1 `BroadcastChannel` experiment is **test-only**, under current GH-484 AC17/no-page-native-endpoint review. A BroadcastChannel payload, `postMessage`, DOM `CustomEvent`, page-created IPC-looking envelope or caller-set world/frame field is untrusted application data. The app world's registered handler proves that **the last hop** entered the app world; it cannot prove who originated a previous page relay. Native #542 may authenticate app-world **handler and current committed document** for an actually allowed private app operation, never elevate arbitrary page-origin text into a privileged method. Method exposure and fixed wrapper admission remain #467/#484's separate owner, followed by #552 literal Guard.

No page-world script-message handler is added by this spec. A direct page-native endpoint remains prohibited under A1. The alternative A2 (untrusted page-native data carrier with same per-call Guard/source gates) requires its own explicit GH-484/KEL-142 and security approvals; this draft neither selects it nor weakens AC17. If A1 cannot establish a supported page→app route, report BLOCKED rather than substitute BroadcastChannel provenance or a hidden raw native handler.

### 4.7 Platform and trust limits

First native acceptance: actual **macOS WKWebView**, real process and managed test document on a disposable signed/source-identified test stage. No assertion that an isolated-world script is invulnerable to malicious same-world preload code; #484's adversarial preload tests must run in the final stack. macOS privacy/entitlement status is recorded as an observed condition. Linux WebKitGTK and Windows WebView2 have separate document/realm APIs and remain UNKNOWN. WindowGeneration, Bun role generation and navigation generation remain distinct; no cross-platform proof by shared Rust state-model unit tests. Hostile same-UID Legacy attestation is not asserted from a mutable local test file.

## 5. Boundaries

**This spec PR may change:** exactly `docs/specs/gh542-native-document-dual-world-binding.md` and generator-owned `llms.txt`/`llms-full.txt` only when canonical tooling actually selects them. It cannot modify production native files, Cargo, CI, permissions parser, KIPC wire, #1171/#1181 or their worktrees, and it cannot patch #484's A1 review.

**Future implementation ownership, subject to exact approved spec and nonoverlapping claim:**

- #542 `crates/keld-wv/src/wkwebview/macos_bridge.rs` and its existing `wkwebview/mod.rs` native callback/attempt seam, focused model/real-Mac tests. Native commit/resource eligibility must consume KEL-102 existing verified source and GH #541 single checked stamp.
- #552 under `keld-guard` and owning `keld-core/src/app_session.rs`: canonical literal Guard dispatch before Bun writes, current window/role validation and native stamp adapter. Do not claim a #542 write lock on these owners.
- #540/#464: existing `keld.wv-link` v2 decoder, single aggregate credits/retirement and typed carrier; #484/#467 own app preload and page method exposure, #499 owns draw.io source closure, #459 owns host-load caller. No duplicate implementation or transport is permitted under #542.
- #587/#589 own any eventual product receipt and score; GH #551 owns Electron navigation differential/oracle truth.

## 6. Tasks (ordered, one reviewable slice per PR)

- [ ] **T0 — this spec:** produce one ten-section GH #542 DRAFT contract, reconcile #541/#540/#441/real-Mac input, run docs/source gates and obtain independent public/native/unsafe/security approval plus exact repository-owner design approval. The spec remains `Status: draft` until that happens.
- [ ] **T1 — checked native mint/attempt:** replace the single WebView `u64` wrapping/private rotate seam with one checked `u32` and native commit/Host-load attempt correlation; pure RED-to-GREEN state tests for hash, cancel, early load, stale callback, duplicate, overflow, destroy.
- [ ] **T2 — qualified native committed document:** capture actual committed final main-frame URL and verified stage/package resource identity; refuse foreign/redirection/subframe/uncorrelated documents, with real-Mac negative controls and zero-route effects.
- [ ] **T3 — realm-scoped credentials and initial bind:** install/reuse distinct native app/trusted world handlers, issue two native-stored world credentials under one qualified document, zero pre-bind queue and explicit failure; host-test absent/swapped/cross-view and stale native delivery. #540 owner must affirm `bound.document` interpretation before enabling v2.
- [ ] **T4 — final Guard/credit composition:** with #552/#540 owners, pass identical navigation stamp into native request and Guard WebView principal; enforce final revocation/nonblocking #540 one-budget handoff and genuine `DocumentRetired`/`ReleaseConsumed` disposition, no forged page identity.
- [ ] **T5 — real native acceptance:** actually execute named macOS negative controls and positive allowed/denied Host+Guard+Bun effects, full G1/G2 recovery, untrusted page relay, source/URL commit races, same-document/cancel, high-water/ledger and independent mutation RED. Repeat KEL-142 AC2 and #484 adversarial app preload under actual stack. Do not publish product PASS until #587/#589 independently admit it.
- [ ] **T6 — other engines and policy:** defer Windows/Linux, BFCache and general remote navigation to their existing owners with distinct real-device proofs; leave UNKNOWN.

## 7. Test plan

| Test owner / IDs | Required real observable | Independently induced RED |
|---|---|---|
| Native state unit, **N01 N07–N13** | checked 0/1/MAX state, genuine native-attempt ordering, Host early revocation, duplicate/cancel/history preservation; A→B late A fail/cancel before B commit preserves AwaitingCommit(B), after B commit preserves Live(B) and both bindings; A charge releases exactly once across original/duplicate callbacks (already released: no additional debit), B resource lease remains charged and usable with successful B delivery; healthy B commits, a separate matching B failure/cancel while awaiting enters FailedClosed, and an uncorrelated actual replacement commit revokes Live(B); no rebind after Exhausted | replace `checked_add` with wrapping, rotate on hash, treat late A fail/cancel as matching B failure, suppress A-only release, release B instead of A, double-release A on repeat callback, retain unbounded old-attempt tombstones, or restore old bind after genuine matching B failure |
| Mac real-navigation, **N02 N07–N12** | real `WKNavigationDelegate`/main-frame commit and final URL, two real files with same URL-shaped string, redirect, canceled renderer provisional, real hash/`pushState`, Host full-document load; release late A fail/cancel before and after healthy B native commit, verify bounded A accounting reaches zero once while B resource lease, bind and delivery survive; in separate arms, matching B failure/cancel while awaiting and an actual uncorrelated replacement commit each fail closed | base admit on provisional URL or timing; late A fail/cancel changes B active stamp, A retained charges leak or double-release, B lease is erroneously released, or genuine matching B failure/unknown replacement leaves stale binding live |
| Mac two-world native, **N03–N05 N14** | two distinct credentials under the same current native generation; native handler `WKContentWorld`/frame/controller/WebView/role observations and one allowed effect | remove stored-token-present check, accept wrong supplied token, swap realm credentials, accept recycled view or subframe |
| App/page provenance, **N06 N19** | page posts forged relay / `BroadcastChannel` payload and direct native-looking message; zero privileged Host/Bun effects and explicit local early error; valid separately allowed app source control succeeds | treat app last hop as authenticated page source, install page-native handler under A1, silently queue before Bind |
| Real Bun recovery, **N15** | G1 pending/queued delivery retired, surviving document same checked generation, G2 authenticated after recovery, fresh allowed G2 call succeeds without replay | reuse old app-link generation, attribute late G1 reply to G2, reset document stamp |
| Core/Guard exact handoff, **N16–N18** | checked native generation equals Guard principal; denied literal channel or stale generation causes zero actual Bun handler entries/writes; allowed exact literal sends once | mint Core counter, remove final stamp check, skip `dispatch_privileged`, block UI navigation on full transport |
| Credit/native stress, **N20** | one #540 total per-role DATA/CONTROL ledger; rebind/retirement does not refill credit; no hidden retained-copy queues, native high-water recorded | give each world fresh quota or release sender credit before actual native consumption |
| Platform/reporting, **N21–N22** | unsupported OS outcomes UNKNOWN, no CodeQL/native/product success invented; v1 compatibility and approved Electron comparison separately recorded | publish native PASS or new product percentage solely from static docs/model fixture |

Each test uses a controlled disposable app/document fixture and an independent effect counter at the **actual Host/Bun/OS** owner where applicable. A single passing Rust model, signed test-only receipt, WKWebView log, screenshot or CodeQL report cannot replace that counter. Await real observed native/callback conditions rather than sleeps; timeouts only terminate a failed arm. Capture attempted native navigation/committed resource identities, frame/handler/controller provenance, event order, one-generation stamp, permission result, one/zero native writes, ledger release and cleanup, and check exact source/OS head. Keep failures and all attempts; do not cherry-pick later successes.

## 8. Review gates triggered

- **Public API/native:** YES for future typed current-document/role/handler accessor and `Principal::Webview` adapter; independent native/WebKit and KEL-102/KEL-142 architecture review needed. This document exports no API.
- **Permission model / security:** YES; actual committed app eligibility, two realm credentials, separate native KEL-142/#441 worlds, final GH-552 Guard grant and role ownership are a security boundary. Require independent named security reviewer on exact source with direct forbidden-effect negatives and existing `GUARD003` unchanged.
- **Unsafe:** YES for future macOS Objective-C WebKit callback/retained-handle and any new native unsafe. List exact unsafe operations, main-thread/ownership lifetime proofs and independent unsafe gate on *implementation*; none added by this docs PR.
- **Wire protocol:** **No new wire bytes or KIPC version authorized.** GH #540 owner must affirm per-world private `document` field semantics under the already approved v2 envelope/ledger; if incompatible, this is a **blocking GH-540 wire-review amendment**, not permission to extend v2 ad hoc.
- **Dependency addition:** none in spec; no new third-party runtime or crypto dependency assumed.
- **Exact approval:** request Amisha independent source/public/native/security review of the DRAFT specification, plus GH #540 and GH #541 owners for shared interfaces and GH #484 for unresolved A1. Owner-approved exact content is required before any production mutation. A green docs-only `CI required` is **not** CodeQL coverage or OS acceptance. **DO NOT MERGE.**

## 9. Perf impact

None in this docs-only PR. Future native first-proof hot-path work must measure per-message handler check/credential compare, native callback-to-Guard handoff, peak retained copies and native navigation progress against the current KEL-142 macOS renderer-bridge baseline (same pinned Host/Bun/OS/source workload). Respect #540's 1 DATA record per direction across role, 4 bounded CONTROL records, 262,144 owner-retained/scratch accounting and no new per-world quota. A changed >5% runtime metric requires existing performance review; no speed/constant-time or memory-footprint claim arises from a design table or 33-case feasibility fixture.

## 10. Open questions

1. **Approval gate, owner:** Human/repository-owner exact-content approval and independent Amisha native/security/public/unsafe review are pending. Until then `Status: draft` and no implementation.
2. **GH #540/credential mapping:** The approved v2 `bound.document` field is one opaque document value per bound isolated realm in this proposed interface. GH #540 must explicitly approve that per-world credential interpretation under one shared *native* checked generation, or provide a separate reviewed minimal v2 change. It must not silently become a shared cross-world secret or a new field.
3. **Native committed URL/attempt identity, GH #542/#459:** The actual WKWebView callback and verified package/resource interfaces must prove stable correlation through redirects, cancellations, same-document and failed loads. If the platform cannot make a callback/commit distinction with primary evidence, keep the native bind path BLOCKED rather than use URL equality or callback timing as an authority surrogate.
4. **GH #484 A1/A2:** The page→app transport selection is unresolved; `BroadcastChannel` A1 is test-only with untrusted originating sender. #542 cannot approve native page ingress or #484's currently rejected raw-handler alternative. No native permission is enabled by this draft.
5. **Real native/Host acceptance:** Existing test-owned 33-case WKWebView world-credential feasibility did not run production source, #552 Guard, actual Host/Bun credit or G1/G2 recovery. These named cross-owner gates remain required; macOS product pass, Windows/Linux support and installed distribution remain UNKNOWN.

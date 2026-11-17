# Design

Part of: [index](index.md)
Owns: section 4

## 4. Design

### 4.1 One owner per decision-bearing atom

### D-1: Navigation and document / **#542 keld-wv**
- **Boundary and input/output:** Actual native main-frame transition + approved resource identity → one current checked `u32` generation, native attempt,
  immutable committed URL and current document stamp
- **First falsifier:** stale native commit, forged URL or wrapping counter enables a bind

### D-2: Realm credential / **#542 keld-wv**
- **Boundary and input/output:** Qualified current document + native registered app/trusted world → separately stored opaque credentials in at most two live
  slots
- **First falsifier:** swap world token or allow absent stored token

### D-3: Role/link / **KEL-75/KEL-143 existing owner**
- **Boundary and input/output:** Authenticated live Bun app link → current role generation, retirement and correlation
- **First falsifier:** retired G1 completion serves G2

### D-4: Grant and principal / **#541/#552 Guard/Core owner**
- **Boundary and input/output:** Captured native stamp + trusted window binding + literal grant → one `dispatch_privileged` decision, before forwarded effect
- **First falsifier:** native token alone bypasses GUARD003

### D-5: Encoding/credits / **#540/#464 existing owner**
- **Boundary and input/output:** Closed v2 envelope and role-wide capacity → bounded ordered forwarding/retirement
- **First falsifier:** second world gets independent allowance or control deadlocks behind DATA

### D-6: Page/preload / **#441/#484/#467**
- **Boundary and input/output:** Untrusted page data → approved app-local method only if independently selected
- **First falsifier:** app-world last hop counted as authenticated page-origin intent

### D-7: Product proof / **#587/#589**
- **Boundary and input/output:** Real Host/Bun/OS evidence + qualified attempt census → truthful cell
- **First falsifier:** artificial app log or screenshot minted as Host receipt


No new principal, second navigation counter, world-specific generation, auth channel, global Native registry, timeout-renewal map or scored product corpus is
introduced. The approved KEL-142 `BridgeState`/installed handler and native UI loop are the reuse seam. This **draft spec** changes no boundary; its proposed
future native implementation strengthens the existing admission boundary and must pass independent security/public/unsafe review.

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

Types above name acceptance boundaries; new public Rust types or WK object wrappers need the owning native/API gate. A `NativeAttempt` is native load/commit
correlation metadata, **not** another generation or a renderer-supplied request ID. A document also retains one Host-instance/WebView lifetime identity so
recycled pointer values cannot resurrect stale state. A native URL is the effective **committed main-frame destination after redirects**, corroborated with the
actual validated package/resource object; provisional `request.url`, DOM `location` and equality to a prior URL do not qualify it. If the engine cannot supply
an independently correlated committed URL/frame, binding remains refused pending a native oracle.

- **Create WebView:** checked epoch transition `Unbound(0)`; No realm slot; no queued bind; no Guard dispatch
- **First qualified native main-frame commit:** checked epoch transition `0→1`; Qualify exact committed URL and resource before issuing realm credentials
- **Renderer provisional start/cancel:** checked epoch transition `+0`; Keep current committed document, credentials and pending work unless a replacement
  actually commits
- **Real renderer replacement commit (including reload to same URL):** checked epoch transition `+1`; Retire both credentials, native evaluations and old
  document delivery; qualify new document only under checked native identity
- **Same-document history/hash:** checked epoch transition `+0`; Keep both realm bindings and outstanding work
- **Trusted Host full-document load command:** checked epoch transition `+1 **before** engine call`; Revoke both realms and old pending work; store one current
  expected native attempt, no implicit rollback
- **That current Host attempt commits:** checked epoch transition `+1`; End AwaitingCommit; qualify actual resource and create one new doc/realm binding
  opportunity
- **Current Host attempt fails/cancels:** checked epoch transition `+0 further`; Only when matching the current AwaitingCommit attempt: retire that attempt into
  FailedClosed; old document remains revoked; explicit new trusted load required
- **Obsolete/unknown Host-attempt callback:** checked epoch transition `+0`; Ignore late A failure/cancel or other mismatched callback for current-document
  state, but release only that callback's old attempt work/charges once if still retained (already released or unowned: no debit); before B commits keep
  AwaitingCommit(B), after B commits keep Live(B) with its realm bindings and eligible pending delivery; preserve B current resource lease, never retire B or
  change its epoch
- **Unknown actual replacement commit:** checked epoch transition `+1 and closed`; Revoke old document; never infer qualification from URL sameness
- **Bun role restart while WKWebView survives:** checked epoch transition `+0`; Preserve native document and world-specific native binding identities; retire
  prior role/link/correlation, require newly authenticated role for fresh sends
- **Native WebView destroy:** checked epoch transition `terminal`; Zero realm slots; pending delivery quiescence; no rebind
- **`checked_add` would exceed `u32::MAX`:** checked epoch transition `Exhausted forever`; Refuse bind/dispatch/delivery for that WebView lifetime

The native UI-loop must own event serialization/deduplication and check callback attempt identity before any failure/cancel transition. An obsolete callback may
finalize **only its own bounded callback-held work and accounting** through the existing #540 owner; each charge is released at most once, and if earlier
retirement already freed it, the callback performs no additional debit. This cleanup must never alter current B's reservation, credentials, state or pending
delivery. Callback-owned work is reclaimed on completion without a new historical-attempt map or unbounded tombstone ledger. A second callback for the **same**
observed native commit is idempotent. A stale A failure after B starts or commits is not an unknown actual replacement commit; a genuinely observed uncorrelated
main-frame replacement remains a separate fail-closed event. A different committed document cannot be mistaken for a duplicate merely because its
URL/text/pointer matches. The recorded committed-document URL/resource is immutable *for that document*; a hash or history-API change may change the visible URL
without creating a replacement document or authorizing a new source. Successful application document eligibility is checked at every replacement, including
redirects, frame changes and restored pages; BFCache is UNKNOWN until a real tested restore path exists. The `0` initial generation is unbound, never a
media/renderer shortcut. The existing media-principal generation-0 residual remains under KEL-102; this spec does not silently fix/waive it or allow it for
compat.

### 4.3 Two realm credentials under one stamped document

**Proposed target only; not enabled by the current v2 Bind.** The eventual native owner
may hold at most two realm slots per qualified current document:
`TrustedK142` (the existing isolated bridge world) and `AppPreload` (the separately
approved #441/#484 app world). Each slot would have a different native-stored
128-bit-or-stronger random credential, allocated only after the exact committed
document and registered world are independently correlated. Each script would receive
only its own private credential. The page sees neither credential.

**Current wire fact:** GH540 draft #1122 defines `bind` as the tokenless
`{v:2,kind:"bind"}` request. `bound.document` is a response value, not proof of the
document that emitted the request. A queued Bind from A can be processed after B
commits at the same URL; current frame/world/controller/WebView checks do not provide
a request value that distinguishes A from B. Do not infer document identity from the
response nonce, URL equality, `isMainFrame`, WebView pointer or callback arrival time.

**Fail-closed rule:** GH542 MUST NOT allocate its new dual-world credential or emit
`bound.document` for the app-preload realm from the current tokenless Bind. The GH542
dual-world path remains disabled until the GH540 owner approves a document-correlated
Bind handshake and GH542 has a real native oracle. Existing KEL-142 behavior remains
under its separate approved contract; this draft does not change or requalify it.

**Possible future mechanism (proposal only):** a one-time native challenge may be
delivered only to the intended isolated world and echoed by its Bind. The challenge
would need to identify the exact WebView lifetime, native load attempt, committed
document and registered world, and become unusable after replacement or retirement.
This is an example for owner review, not a specified field or approved wire change.
GH540 owns the exact request bytes, encoding and ordering. `bound.document` may remain
the returned per-world credential only if GH540 explicitly approves that mapping.

The native message handler still verifies actual `WKScriptMessage.world()`,
`frameInfo().isMainFrame()`, `webView()` and registered controller identity. These
checks are necessary but not sufficient for document correlation. No realm credential
converts into a native filesystem/media permit. The two future credential slots remain
disjoint and callback-scoped; a missing/stale credential or unqualified sender produces
zero forwarding.

Role provenance is separately authenticated by the existing Host app-link owner:
native app-world provenance says only which installed realm delivered the last message.
It does not assert a page sender or a Bun role. A `send` from AppPreload still needs the
canonical exact `el:channel` Guard grant and current authenticated primary app role.
TrustedK142 operations retain KEL-142's narrower operation/channel contract. A page
message has no privileged role merely because an app handler later forwards its bytes.

### 4.4 Eligibility and the early-bind race

Initial scripts may be installed at `AtDocumentStart` in each designated world, before native WebKit has produced the confirmed committed app document. That
makes APIs visible, not authorized. The native entry state is `Unbound` with **zero queued Bind/Invoke/Send/Consumed**, zero provisional credential, and no
Host/Bun forwarding. An early Bind or legitimate early renderer request receives an explicit bounded local `KELD-WV-011`-class **never-forwarded-to-Host/Bun**
refusal through its applicable existing error owner; receiving a rejected native handler callback is not evidence of an accepted app-link request. It is never
silently dropped, replayed, held until commit, or reclassified as permitted because a page wrapper is visible.

After an independently verified current native commit and source/resource qualification, the native owner still MUST refuse the current tokenless v2 Bind for
the GH542 dual-world path. No fresh credential or `bound.document` is emitted until GH540 approves a document-correlated handshake and GH542 verifies that the
challenge matches the current native attempt and committed document. A pre-commit Bind is never stored or resurrected. Failure of
callback/evaluation/bind/credential allocation is terminal to that attempt, with clear diagnostics and zero effect. Do not create an extra JS promise queue, new
event transport or implicit restart loop. `bound.ready` or a route-state notification is informational and cannot itself prove eligible URL, authenticated Bun
role, permission grant or consumed credit. New operations may start only after those owning gates pass.

The first-proof source admission is narrow: the verified stage/package app-resource identity and approved source closure from #499/KEL-102, native main-frame
commit, and present policy-eligible window binding all match. A `file:` prefix, identical path spelling, DOM `location`, main-frame claim in JSON, redirect
target text or current app name is insufficient. Arbitrary remote content remains no-channel until #460/KEL-79's general navigation policy is separately
qualified.

### 4.5 Final dispatch, revocation and aggregate resources

Use the existing native bridge owner to snapshot `(Host/WebView lifetime, checked navigation generation, eligible native committed document, realm credential,
real frame/world/controller, authenticated role/app-link generation, operation identity)`. Before **every** enqueue/forward, revalidate that stamp at the final
nonblocking #552/`dispatch_privileged` boundary. The Guard principal is constructed from **that same** `WebviewId` and `NonZeroU32` generation; there is no
alternate Core mint or inferred owner. Literal grant evaluation, source exposure/registration, authenticated app link and v2 direction/type checks all remain
independent requirements. The final handoff and revocation have an explicit linearization point: if native revocation wins, zero app-link write; if a prior
handoff wins, a dispatched effect may still finish, so represent uncertain completion rather than claim rollback or replay. Never hold a native navigation lock
across blocking socket/Bun/JS/Guard work.

Revocation is effective when native state changes, before its later `DocumentRetired`/`route-state` control notification. Retire both realm slots together on
real replacement commit, early Host load, WebView destruction and checked exhaustion. Revoke all old document delivery and native callback identities; stale
evaluation completion releases only its old resource reservation and never delivers to a new document. On Bun recovery, retain the surviving **native** document
stamp and native credentials per #541, but revoke old app-link generation/role-bound correlations, queued old work and old permissions association; the new role
must authenticate and pass a **new** final Guard check. Never let prior role's credential possession bypass new link admission. If a distinct rebind-on-recovery
mechanism is later proven necessary, seek its own exact owner amendment, without rotating navigation due solely to Bun restart.

Use exactly #540's existing closed `keld.wv-link` v2 envelope and aggregate per-role credits, not per-world credit windows. The current tokenless `bind` is not
sufficient for GH542 dual-world admission. At most one DATA record per direction across the role, one per document, four bounded independent CONTROL records and
one canonical FIFO/retirement path as specified by #540; all queued/retired/cancelled records count until their owner proves release. A repeated Bind never
resets quota. Before-bind retention is zero. The native owner retains at most two active credential slots per WebView/document, one current native navigation
attempt, and bounded existing evaluation/diagnostic slots; no historical secret map, infinite tombstones or hidden duplicate queue. Bound each credential to the
existing 16-byte random source minimum, use bounded encoded JSON/UTF-8 and fixed diagnostics, and never print raw credential/payload. Every accepted and refused
native handler callback is classified within existing #540 size/credit and #1095 terminal owners. CodeQL/source scanning can later examine code but **does not
prove** native world or Host effects.

### 4.6 GH-484 A1 / BroadcastChannel provenance prohibition

The #484 A1 `BroadcastChannel` experiment is **test-only**, under current #1155 AC-17/no-page-native-endpoint review. A BroadcastChannel payload, `postMessage`,
DOM `CustomEvent`, page-created IPC-looking envelope or caller-set world/frame field is untrusted application data. The app world's registered handler proves
that **the last hop** entered the app world; it cannot prove who originated a previous page relay. Native #542 may authenticate app-world **handler and current
committed document** for an actually allowed private app operation, never elevate arbitrary page-origin text into a privileged method. Method exposure and fixed
wrapper admission remain #467/#484's separate owner, followed by #552 literal Guard.

No page-world script-message handler is added by this spec. A direct page-native endpoint remains prohibited under A1. The alternative A2 (untrusted page-native
data carrier with same per-call Guard/source gates) requires its own explicit GH-484/KEL-142 and security approvals; this draft neither selects it nor weakens
#1155 AC-17. If A1 cannot establish a supported page→app route, report BLOCKED rather than substitute BroadcastChannel provenance or a hidden raw native handler.

### 4.7 Platform and trust limits

First native acceptance: actual **macOS WKWebView**, real process and managed test document on a disposable signed/source-identified test stage. No assertion
that an isolated-world script is invulnerable to malicious same-world preload code; #484's adversarial preload tests must run in the final stack. macOS
privacy/entitlement status is recorded as an observed condition. Linux WebKitGTK and Windows WebView2 have separate document/realm APIs and remain UNKNOWN.
WindowGeneration, Bun role generation and navigation generation remain distinct; no cross-platform proof by shared Rust state-model unit tests. Hostile same-UID
Legacy attestation is not asserted from a mutable local test file.

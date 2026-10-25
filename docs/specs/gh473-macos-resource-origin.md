# Spec: macOS application-resource origin, bounded delivery and early CSP (GH473)

Status: draft
Linear: GH-473 (GitHub tracker under #517; KEL-79 reference) · Owner: GYLDLAB / @0monish · Updated: 2026-10-25

**Disposition: proposed, do not merge or implement.** This document makes the
existing resource decision reviewable. It does not approve changing GH473's
literal `file://` acceptance criterion or GH499's application main. The user's
2026-10-25 direction confirms native platform bindings as the destination and
parks the general Wry replacement/performance program; it is not silently
interpreted as approval of an application-origin or permission-model change.

Source baseline: `de768dee8c3a9ba512ee862fe1c409a41fac5a1f`.

## 1. Goal & non-goals

Reach the first genuine draw.io renderer request through a bounded macOS resource
path: original renderer/assets/preload bytes, actual language-resource XHR 2xx,
original initialization, original `requestSync('isFullscreen')`, and ultimately
the existing canonical Host/Guard/Bun route. The proposed resource-origin change
must be honest, separately approved, and limited to an explicitly admitted local
Legacy application. Strict behavior and the existing native security boundaries
do not weaken.

**Native destination:** `keld-wv` owns its platform contract. On macOS the engine is
Apple WKWebView, currently constructed through Wry and ultimately through direct
`objc2` bindings. Wry is an interim adapter, not a browser engine or an authority
owner. This slice may reuse its public custom-protocol response mechanism; it
must not leak Wry request/responder types into the manifest, app facade, canonical
IPC, source identity, or navigation-generation contract. General backend removal
and same-engine performance comparison are explicitly parked in
[GH1193](https://github.com/gyldlab/keld/issues/1193) and
[GH1194](https://github.com/gyldlab/keld/issues/1194), not predecessors of this MVP.

Non-goals: a Wry fork, WebKit fork/private preferences, broad file access, a
loopback server, XHR status shims, renderer/preload rewrites, fake sender URLs,
new IPC transport, a second manifest parser or document-generation mint,
unapproved edits to other owners' branches, persistent-origin migration,
arbitrary remote/subframe resource authority, service workers, media Range
support, or complete-app/edit/save/Electron-parity claims from a standalone probe.

## 2. Spec refs

- [Architecture 05 §1](../architecture/05-webview-and-native.md): own thin native
  bindings, current Wry scaffolding, UI-thread platform ownership.
- [Architecture 03 §§1–2](../architecture/03-security.md): Host-minted principals,
  one verified manifest, separate application and renderer authority. The current
  `web.csp` examples are **profile selectors** (`default`, `static-only`), not a
  raw-header grammar. Current runtime parsing does not implement these examples.
- [GH415 ADR](https://github.com/gyldlab/keld/issues/415#issuecomment-6395832965)
  and [independent replay](https://github.com/gyldlab/keld/issues/415#issuecomment-6396039842):
  mechanism evidence and its explicit limitations.
- [GH499 origin adaptation](https://github.com/gyldlab/keld/issues/499#issuecomment-6395844080):
  physical code paths stay separate from the proposed application resource URL.
- [GH542 draft #1192](https://github.com/gyldlab/keld/pull/1192): one checked native
  document generation, native load-attempt correlation, committed source
  qualification, revocation and distinct world credentials. This draft consumes
  that owner's proposed contract; it does not declare it approved or implemented.
- [GH552 draft #1191](https://github.com/gyldlab/keld/pull/1191), GH541/KEL-102:
  one policy loader, Host window binding and final exact-channel Guard. Channel
  parsing is not a CSP implementation. Shared-schema approval is required.
- [GH594](https://github.com/gyldlab/keld/issues/594) owns the separately planned
  closed web-capability vocabulary. Its camera/microphone-only rule must distinguish
  CSP policy metadata from capability names in the same `web` object. The narrow
  schema decision must not pull its whole next-milestone media/clipboard program
  into this MVP.
- GH484/GH467 own preload and callable exposure; GH540 owns the canonical
  decoder/transport/credits; GH470 owns the actual Electron 44.4.5 oracle;
  GH586/GH588/KEL-78 own explicit Legacy admission. No ownership is transferred.

### Evidence that decides the mechanism

| Observation | Scope and consequence |
|---|---|
| Direct Swift WKWebView file-load variants commit the original file URL but original `dia.txt` XHR fails with status 0 and empty body | A Wry defect is not established. Replacing Wry alone is not a demonstrated remedy. The exact installed WebKit denial branch remains untraced. |
| Direct custom handler: ordinary URLResponse gives exact bytes/status 0; HTTPURLResponse(200) gives exact bytes/status 200 | Response metadata is an independently isolated discriminator. Native filesystem read permission does not itself promise HTTP success semantics. |
| Original renderer and preload naturally reach `rendererReq/isFullscreen` at an honest custom URL | Standalone caller witness only: test facade/carrier, no actual Keld Guard/Bun or main reply. Original main sender check rejects that URL. |
| Public handler registration for file/http/https throws; custom registration succeeds | No supported file-subresource response interception was established. This is a bounded tested limitation, not a universal impossibility theorem. |
| Wry 0.56.1 constructs NSHTTPURLResponse from Rust status and headers | Reuse the existing adapter rather than introducing another native loader merely for status 200. |

The existing failed `loadSimulatedRequest` main-response experiment is a real
WKWebView API experiment, not a simulator. It does not intercept the unchanged
relative subresource request. No further broad loader matrix is a prerequisite;
reopen that direction only with a concrete supported mechanism and a falsifier.

## 3. Acceptance criteria (binary, each becomes a test)

| ID | Required observable | Independent negative control |
|---|---|---|
| R01 | Explicit approved Legacy/source adaptation is required; Strict and missing admission refuse this migration before the custom entry loads | Delete profile/adaptation admission; a previously refused launch incorrectly proceeds |
| R02 | Original pinned renderer/assets/preload yield actual `dia.txt` XHR 200–299, exact 95,821 bytes/hash, and a natural original caller; no injected initiating request | Plain response status 0 or deny `dia.txt`; original request must disappear while the observer remains alive |
| R03 | Native committed URL and sender URL are truthful and match the admitted custom entry; original file expectation remains separately classified as changed | Forge a file sender, foreign authority, or same text URL with a foreign source object; no admission |
| R04 | Retained-root reads serve only admitted source bytes; outside root, escaping symlink, malformed/encoded path, nonregular file and unauthorized method yield zero forbidden bytes | Remove one root/walker check; an independent outside marker becomes observable |
| R05 | Receiving WebView/lifetime and exact requested authority agree before any delivery, including classic script, image and no-CORS fetch | Remove only native authority matching; foreign resource bytes/effects become observable under the test's deliberately independent browser-policy control |
| R06 | One resolved, verified window CSP reaches the main response before parsing; A/B window policies remain independent | Alter only A's manifest policy; A's physical script effect changes and B does not. Runtime derivation from `app.net` must not affect A |
| R07 | CSP denies parser-inline script with an observer installed at document start; the admitted wasm route is independently exercised | Permit inline only in the mutation; effect changes 0→1. A late/missing notification does not excuse relaxing the effect oracle |
| R08 | Held response admitted for a current document yields no bytes after actual document revocation wins final delivery | Remove final live-document check; stale bytes become observable. A fixture counter is not actual GH542 acceptance |
| R09 | Initial resource loading works before commit without minting IPC authority; early unqualified bridge requests remain refused | Treat source bytes or pending native load as a committed document; premature Host/Bun effect fails |
| R10 | Cancellation/destruction releases request/body reservations exactly once; no response to stopped/reused native task or replacement document | Retain/double-release a charged response or reuse a stopped task; bounded-state/canary oracle fails |
| R11 | Bun restart alone preserves the surviving native document/resource identity while old role-bound IPC remains retired | Rotate native document on Bun restart, replay an old request, or retarget its reply; preservation/revocation test fails |
| R12 | Genuine original `el:rendererReq` passes canonical decoding and final Guard, enters actual Bun once, and replies only to its live original document | Missing grant, foreign source or revocation before final handoff produces zero Bun entries/effects; remove one final check and require RED |

R02 is already demonstrated only in the standalone fixture. R04–R08 have partial
synthetic native evidence, not a product pass. No row closes from a screenshot,
API availability, unit test, or process exit code alone. R05's request-type
qualification and R12's composed product effects are still open.

## 4. Design

### 4.1 Ownership and first-principles boundaries

| Owner | Input → output; independent failure |
|---|---|
| GH499/source-admission owner | Explicitly admitted source closure → retained source object; a path/hash supplied by the page is not package authentication |
| GH473 resource owner | Source object + bounded path + native view/load context → response bytes; delivery cannot create an IPC principal |
| KEL-102/GH552 shared policy owner | Single verified manifest snapshot + Host-bound window selection → resolved immutable window policy; no second decoder or borrowed AppProcess grants |
| GH542 native owner | Actual native load/commit/lifetime → pending load context and current committed document stamp; no GH473 navigation counter |
| GH484/GH467 | Approved page/preload method exposure; app-world last hop alone does not authenticate earlier page intent |
| GH540/GH552/Core | Canonical bounded request + current native/role stamp + exact grant → final Bun handoff; resource success grants nothing |

The Host owns root handles and immutable policy. File reads remain Host work, not
Bun code execution. The native UI executor serializes document state and response
delivery. Resource work owns its file descriptors/buffers until completion or
quiescence. WebKit owns rendering and its content-process crash domain. A Bun
crash is not a WebView document replacement. These facts remain identical when
the interim Wry call site is later replaced by direct native bindings.

### 4.2 Proposed origin and application migration

Use a Host-registered application resource scheme installed before first
navigation, with a separately minted unguessable authority for each admitted
WebView document/load scope. The exact scheme spelling must reconcile with
GH471's proposed host-owned scheme table before approval; no live table is claimed.
Do not copy the probe's `keld-mvp` spelling into a second registry. GH471's general
Bun `protocol.handle` producer and Zettlr matrix remain next-milestone work: this
MVP needs only one owned fixed built-in resource-scheme reservation.

The descriptor binds the source snapshot, Host/WebView lifetime, native load
context, admitted main entry and original query. The authority is a locator, not
a credential, and is never sufficient to authorize another receiving WebView.
Never compare generic custom `URL.origin == "null"` strings as authority.

GH499 makes one disclosed semantic adaptation with three narrow changes: keep
physical `codeDir`/`codeUrl` for file behavior; load the Host-admitted resource
entry with original query; validate the actual sender against that entry plus
native document eligibility. Renderer HTML, mxUtils, ElectronApp and preload
bytes stay unchanged. Do not call the full adapted desktop app unmodified.

Initial scope is an explicitly disposable local editor document. It makes no
persistent-storage continuity promise across replacement or launch. Preserve
the existing KEL-135 dev store choice; do not silently replace the production
profile policy. Importer/exporter windows, restored documents and persistent
origin migration require separate scoped approval; they are not faked as working.

### 4.3 Backend-neutral typed input, not a new framework

The following names describe proposed private integration types, not APIs that
already exist or authority to create parallel implementations:

```rust
struct AdmittedAppResources {
    source: RetainedVerifiedCodeRoot,
    entry: HostAdmittedResourceEntry,
    policy: ResolvedWindowPolicy,
    limits: ResourceLimits,
}

// GH542 owns this context, including the distinction between initial
// pending-load resource admission and a live committed IPC document.
enum ResourceDocumentContext {
    InitialLoad(NativeAttempt),
    Committed(CurrentDocumentStamp),
}
```

Resolve these names to owning existing abstractions in the implementation PR.
No general plugin interface, new crate, public backend-switch option or duplicate
state registry is needed. Wry request/responder objects remain confined to the
macOS adapter. The future direct adapter consumes the same policy/source/lifetime
contract; it does not redefine it.

### 4.4 Initial load, commit and revocation

A resource request for the initial main document necessarily precedes its native
commit. Requiring an already committed stamp for that request creates a bootstrap
deadlock. Instead consume GH542's **existing native load attempt**, which the Host
created for this exact view, entry and source. Permit only the admitted static
source closure for that attempt; it conveys no bridge credential or IPC grant.

At a qualified native main-frame commit, GH542 associates the admitted resource
object with its one checked document generation. It must atomically associate
pending resource work with that same committed document or retire it; merely
advancing the epoch must not invalidate the new document's own in-flight assets.
The exact transition is a required GH542 review point, not a second GH473 mint.

Admit reads against the captured context. Complete file work off the UI executor
through an approved bounded Host I/O owner, never touching WK objects there.
Return to the same UI executor that owns revocation; immediately before native
response delivery revalidate captured view/lifetime/context/authority, then
respond without yielding. Retire both new requests and already accepted deferred
responses when replacement/destruction wins. Do not hold a native lock across
blocking file, Bun, socket or JavaScript work.

Wry's task UUID/stopped-task checks remain necessary, but are not document
authority. A cancelled task releases reservations only after its retained read
or callback is quiescent. Already delivered bytes cannot be recalled. `no-store`
does not prove back/forward-cache revocation; the initial scope refuses restoration
until native acceptance exists. Bun-only recovery leaves the native resource
context intact while existing IPC owners retire role-bound work.

### 4.5 Root confinement, source authenticity and resource bounds

Consume the existing source-admission path and retained root. Reuse or extract
the reviewed descriptor-relative walker only with its owner; do not call
`FsBroker` under a fictitious AppProcess identity or add renderer filesystem grants.
Existing safe in-root link semantics must be preserved if promised; escaping
links, mount boundaries and aliases fail under the owning walker. Never reopen
an unchecked path after validation. Open leaf objects nonblocking before checking
regular-file type so a FIFO cannot block before rejection.

Parse delivered URL components once. Match exact scheme/authority/method and
reject unsupported credentials/ports, malformed escapes, encoded separators,
NUL, backslash, double-decoding ambiguity and escaping components. Browser URL
normalization may remove source-text dot segments before the handler; acceptance
must prove final path confinement rather than claim observation of original text.
Keep entry query semantics separate from filesystem lookup; no query selects a
different root, principal, policy or arbitrary file.

Limits are a reviewed Host input: maximum asset bytes, total retained bytes,
in-flight reads, queued requests, path/components and cancellation retention.
All reservations count until actually released. The existing probe's 16 MiB
single-file cap is **test evidence, not a production budget**. Before approval,
the implementation owner must census the pinned required source closure and
record concrete limits and the existing Host worker/queue used; no unbounded
worker per request or hidden retired queue is allowed. Synchronous OS read
cancellation is cooperative; no unsupported hard I/O deadline is promised.

A retained directory handle proves confinement, not immutable package bytes.
The admitted resource must remain tied to the authenticated/verified source
snapshot through the owner's same-object/byte-verification contract. Local
unpinned directory content or test SHA256 values alone do not authenticate a
production package or prove hostile same-UID containment in Legacy.

### 4.6 Response and CSP policy

Return actual status 200 and correct MIME/length for an admitted complete regular
asset. Use `Cache-Control: no-store`; no redirects, dynamic execution or general
network proxy. Fail closed with bounded diagnostics and zero forbidden content.
An error response may expose a fixed generic error body, never outside-file bytes.

The main response receives CSP **before parsing**. Reuse Wry 0.56.1's native
HTTP-response conversion now; direct WKURLSchemeHandler can replace this adapter
later. Do not insert a late meta policy or modify XHR status in JavaScript.

**One policy source:** the verified manifest selects `windows.<key>.web.csp`.
Preserve the existing selector grammar until its owner approves an amendment.
The shared policy owner resolves it once to a typed immutable policy; GH473 only
renders that policy for the admitted resource identity. There is no second raw
header field, fallback draw.io string, runtime `app.net` projection or page-selected
policy. GH473 must coordinate this extension with KEL-102/GH552 and GH594; the
channel-only draft does not implement it, and GH594's proposed camera/microphone
vocabulary otherwise conflicts with `csp`. The narrow shared-schema decision must
accept a valid CSP selector as policy metadata while still rejecting unknown
capability `web.hid` and malformed/unknown selectors. Neither the whole GH594
media implementation nor a second parser is needed to resolve this distinction.

For the offline first-request scope, propose an effective policy that permits
only admitted local scripts/connects/images/media/fonts, necessary data images/
fonts and inline styles, plus `wasm-unsafe-eval`; no remote origins, inline
scripts, object, frame or form destinations. This is a candidate resolved policy,
not a global redefinition of `default`. The generator/shared owner must approve
its representation. A bounded direct WKWebView test on 2026-10-25 changed only
the copied harness's response-policy constant: two native runs passed the
unchanged original-caller/XHR oracle under local-only sources. A control changing
only `script-src` to `'none'` stopped original app scripts while the original
preload observer stayed alive. Positive JSON SHA256:
`cbe9cfc6cd359fe143cbec57326e56000f601c21238473bcee7bfef9090a2258`;
summary SHA256:
`8a7b678b06edf35f8a208758a3d4e276c22b0d7ea7edd177c56e624f43742d74`.
This establishes candidate-effective-policy bootstrap feasibility only. It does
not qualify the absent manifest path, general network/OS confinement, the wasm
operation, literal-file identity or actual Keld Host/Bun. Original page meta
policy intersects the response policy and cannot widen it.

Distinct custom hosts do not replace native authority matching. The upstream
WebKit same-custom-scheme no-CORS exception is a reason to qualify native
script/image/fetch delivery, not proof that the installed engine is vulnerable.
Keep foreign frames/popups outside this first scope; WKURLSchemeTask alone is
not privileged IPC caller attestation.

### 4.7 Failure, migration and wire impact

Source/policy/admission failures refuse initial loading with existing typed
WebView/Guard error owners and useful fix guidance. Native task cancellation or
document retirement never triggers a hidden retry, fallback origin or automatic
request replay. Preserve the distinction between not-sent and already-dispatched
outcome uncertainty in the IPC owners.

No wire change or new KIPC channel is proposed. No public configuration key is
invented here. Runtime implementation affects the resource/lifetime/public API
and permission boundaries and requires exact review even though this draft only
adds prose. Persistent data migration is out of scope and is not implicit consent.

## 5. Boundaries

This draft changes only `docs/specs/gh473-macos-resource-origin.md`. No product
source, workspace dependency, generated manifest, existing spec, PR1171/1181,
other worktree, or canonical IPC file is edited.

After approval, the smallest resource implementation belongs in
`crates/keld-wv/src/{engine.rs,media.rs,wkwebview/mod.rs}` and one focused macOS
resource module, with the guarded builder's media-permission witness preserved.
GH542 alone owns `wkwebview/macos_bridge.rs` lifecycle integration. KEL-102/GH552
own shared manifest parsing and policy selection. The existing filesystem owner
reviews any walker extraction/nonblocking-open change. GH499 owns its main
adaptation; GH484/GH467 own the callable bridge. No source ownership is acquired
by naming these future paths.

## 6. Tasks (ordered vertical slices)

- [ ] T1: exact architecture/security approval of this origin/root/CSP contract;
  resolve §10 with the existing owners and amend GH473/GH499 acceptance explicitly.
- [ ] T2: one separately claimed resource slice consumes verified source/policy
  and GH542 load/document contexts, installs the guarded native response path,
  and passes original-renderer and independent resource controls.
- [ ] T3: existing preload/IPC owners compose the real original request through
  final Guard, Bun and same-document reply; run revoked/foreign no-effect controls
  and the separately pinned GH470 Electron oracle.

No full Wry removal, cross-platform port, benchmark campaign or feature expansion
is added to these tasks. Future tickets must not block T1–T3 merely by existing.

## 7. Test plan

R01/R03: profile/source admission tests plus real native committed URL and the
unchanged original sender predicate as a counterexample. R02: actual unchanged
pinned renderer, authentic XHR status/body hash/caller stack and original preload
send; controls deny the resource and remove the HTTP status independently.
R04/R05: real temporary nonsecret marker files and per-request-type native
delivery counters, healthy same-view counterweights and one-site physical
mutations. R06/R07: two windows, verified-policy variation, early CSP observation,
actual parser effects and a wasm compile witness. R08–R11: actual native
load/commit/cancel/destroy/recovery events with held response completion, exact
reservation accounting and healthy follow-up. R12: actual Keld/Guard/Bun product
run with an independent final-effect observer, not test-stub traffic.

Reuse retained probe receipts without rewriting them. The earlier boundary
verifier failure from a late CSP listener stays preserved; corrected observation
is not permission to loosen the oracle. No sleeps synchronize tests. Deadlines
are failure bounds, not assertions that blocked OS work was cancelled.

## 8. Review gates triggered

**Public API and permission model:** required for this proposed resource origin,
policy and ownership contract. **Unsafe:** required for future new native calls
or walker extraction, not present in this prose diff. **Dependency addition:**
none proposed; reuse pinned Wry/objc2. **Wire protocol:** none proposed; any later
change is a separate GH540 gate. Approval applies to exact reviewed content;
other draft specs or passing documentation checks are not implementation approval.

## 9. Perf impact

No measured performance claim. Both adapter routes use Apple's WebKit. Native
bindings give integration ownership, not an automatic rendering speedup. Resource
buffering, queue delay, copies and cancellation retention require bounded product
accounting; comparative Wry-versus-direct performance work is parked until after
the real draw.io Host/Bun milestone and matched security/functional conformance.

## 10. Open questions and approval record

1. **GH473/GH499 + architecture/security owner:** approve or reject the single
   disclosed custom-origin migration and the disposable initial-document scope.
   Native-destination direction alone does not settle this compatibility change.
2. **GH471 + GH473:** reconcile the fixed host scheme registration and admitted
   entry descriptor with the existing scheme table; do not create a second owner.
3. **KEL-102/GH552/GH594 shared policy owner + GH473:** approve the exact selector-to-
   resolved-CSP representation and offline candidate, without silently changing
   `default` or borrowing app-process network policy.
4. **GH542 + GH473:** approve the initial native-attempt-to-committed-document
   association for in-flight resource responses; preserve its one checked mint
   and do not invent precommit IPC authority.
5. **Source-admission/filesystem/Host worker owners + GH473:** name the concrete
   verified-byte source, retained walker, bounded I/O executor and corpus-derived
   resource limits. Test pins and an arbitrary mutable directory are insufficient.

These are bounded owner decisions, not reasons to repeat the whole investigation.
This draft stays unapproved until they have exact recorded dispositions. The
first observable product completion remains R02 **and** R12, not this document.

### Current-documentation receipt

- Applicability: public WKWebView scheme registration/response semantics and
  interim Wry adapter behavior.
- Context7: `/tauri-apps/wry`, queried 2026-10-25 for macOS custom response/status,
  native configuration and task lifetime. Returned development documentation;
  used for discovery only, not as pinned 0.56.1 authority.
- Official primary: [Wry tag wry-v0.56.1 response implementation](https://github.com/tauri-apps/wry/blob/wry-v0.56.1/src/wkwebview/class/url_scheme_handler.rs),
  [Apple scheme registration](https://developer.apple.com/documentation/webkit/wkwebviewconfiguration/seturlschemehandler(_:forurlscheme:)),
  and the exact WebKit/Apple/source receipts linked by the GH415 ADR/addendum.
- Supported claim: Wry already supplies native HTTP response status/headers;
  native scheme registration does not provide an accepted file override in the
  reviewed API/OS proof. Direct native tests isolate this from Wry.
- Fallback/blocker: installed Apple WebKit source/patch mapping and precise
  internal file-denial branch remain unknown. No private API, global access
  grant or universal impossibility claim fills that gap.

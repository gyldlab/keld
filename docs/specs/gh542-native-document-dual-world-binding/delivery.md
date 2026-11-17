# Delivery and evidence

Part of: [index](index.md)
Owns: sections 5, 6, 7, 9 and 10

## 5. Boundaries

**This spec PR may change:** only the GH542 specification directory `docs/specs/gh542-native-document-dual-world-binding/` and generator-owned
`llms.txt`/`llms-full.txt` only when canonical tooling actually selects them. It cannot modify production native files, Cargo, CI, permissions parser, KIPC
wire, #1171/#1181 or their worktrees, and it cannot patch #484's A1 review.

**Future implementation ownership, subject to exact approved spec and nonoverlapping claim:**

- #542 `crates/keld-wv/src/wkwebview/macos_bridge.rs` and its existing `wkwebview/mod.rs` native callback/attempt seam, focused model/real-Mac tests. Native
  commit/resource eligibility must consume KEL-102 existing verified source and GH #541 single checked stamp.
- #552 under `keld-guard` and owning `keld-core/src/app_session.rs`: canonical literal Guard dispatch before Bun writes, current window/role validation and
  native stamp adapter. Do not claim a #542 write lock on these owners.
- #540/#464: existing `keld.wv-link` v2 decoder, single aggregate credits/retirement and typed carrier; #484/#467 own app preload and page method exposure, #499
  owns draw.io source closure, #459 owns host-load caller. No duplicate implementation or transport is permitted under #542.
- #587/#589 own any eventual product receipt and score; GH #551 owns Electron navigation differential/oracle truth.

## 6. Tasks

Tasks are ordered vertical slices. This PR is specification-only. No production
implementation begins while the spec is draft or any blocker is open.

### T-1: Exact-content specification and owner decisions
- **Blocked by:** B-1, B-2, B-3, B-4, B-6
- Keep the complete ten-section contract in the current directory layout.
- Refresh all source pins, PR heads and links.
- Obtain exact-content repository-owner and independent public/native/unsafe/security
  approval. Keep `Status: draft` until that approval is recorded.
- Do not infer implementation approval from a green documentation check.

### T-2: Checked native mint and attempt correlation
- **Blocked by:** B-2, B-6
- Replace the single WebView `u64` wrapping/private rotate seam with one checked `u32`
  and native commit/Host-load attempt correlation.
- Add failing-first state tests for hash, cancel, early load, stale callback, duplicate,
  overflow and destroy.
- Preserve A-only late failure/cancel cleanup without changing B state or B's resource
  lease.

### T-3: Qualified native committed document
- **Blocked by:** B-2, B-6
- Capture the actual committed final main-frame URL and verified stage/package resource
  identity.
- Refuse foreign, redirected, subframe and uncorrelated documents.
- Add real-macOS negative controls and zero-route effects.

### T-4: GH540 document-correlated Bind and realm credentials
- **Blocked by:** B-1, B-2, B-6
- Do not enable the GH542 dual-world Bind path under the current tokenless v2 request.
- Wait for GH540's exact owner-approved document-correlated handshake and conformance
  bytes.
- Only then install/reuse distinct native app/trusted world handlers and issue two
  native-stored world credentials under one qualified document.
- Prove absent, stale, swapped, cross-view and same-URL A-to-B Bind refusals.

### T-5: Final Guard and aggregate-credit composition
- **Blocked by:** B-1, B-3, B-4, B-6
- With GH552/GH540 owners, pass the identical checked navigation stamp into the native
  request and Guard WebView principal.
- Enforce final revocation and nonblocking GH540 one-budget handoff.
- Prove `DocumentRetired`/`ReleaseConsumed` disposition and no forged page identity.

### T-6: Real native acceptance
- **Blocked by:** B-1, B-2, B-3, B-4, B-5, B-6
- Execute the named macOS negative controls and real Host/Guard/Bun positive and denied
  effects.
- Exercise full G1/G2 recovery, untrusted page relay, source/URL commit races,
  same-document/cancel, high-water/ledger and independent mutation RED.
- Repeat KEL-142 AC2 and GH484 adversarial app-preload checks under the actual stack.
- Do not publish product PASS until GH587/GH589 independently admit it.

### T-7: Other engines and policy
- **Blocked by:** B-5
- Defer Windows/Linux, BFCache and general remote navigation to their existing owners
  with distinct real-device proofs.
- Leave unsupported platform and restore cells UNKNOWN.


## 7. Test plan & evidence

### Native state unit, **N01 N07–N13**
- **Required real observable:** checked 0/1/MAX state, genuine native-attempt ordering, Host early revocation, duplicate/cancel/history preservation; A→B late A
  fail/cancel before B commit preserves AwaitingCommit(B), after B commit preserves Live(B) and both bindings; A charge releases exactly once across
  original/duplicate callbacks (already released: no additional debit), B resource lease remains charged and usable with successful B delivery; healthy B
  commits, a separate matching B failure/cancel while awaiting enters FailedClosed, and an uncorrelated actual replacement commit revokes Live(B); no rebind
  after Exhausted
- **Independent RED:** replace `checked_add` with wrapping, rotate on hash, treat late A fail/cancel as matching B failure, suppress A-only release, release B
  instead of A, double-release A on repeat callback, retain unbounded old-attempt tombstones, or restore old bind after genuine matching B failure

### Mac real-navigation, **N02 N07–N12**
- **Required real observable:** real `WKNavigationDelegate`/main-frame commit and final URL, two real files with same URL-shaped string, redirect, canceled
  renderer provisional, real hash/`pushState`, Host full-document load; release late A fail/cancel before and after healthy B native commit, verify bounded A
  accounting reaches zero once while B resource lease, bind and delivery survive; in separate arms, matching B failure/cancel while awaiting and an actual
  uncorrelated replacement commit each fail closed
- **Independent RED:** base admit on provisional URL or timing; late A fail/cancel changes B active stamp, A retained charges leak or double-release, B lease is
  erroneously released, or genuine matching B failure/unknown replacement leaves stale binding live

### Mac two-world native, **N03–N05 N14**
- **Required real observable:** two distinct credentials under the same current native generation; native handler `WKContentWorld`/frame/controller/WebView/role
  observations and one allowed effect
- **Independent RED:** remove stored-token-present check, accept wrong supplied token, swap realm credentials, accept recycled view or subframe

### App/page provenance, **N06 N19**
- **Required real observable:** page posts forged relay / `BroadcastChannel` payload and direct native-looking message; zero privileged Host/Bun effects and
  explicit local early error; valid separately allowed app source control succeeds
- **Independent RED:** treat app last hop as authenticated page source, install page-native handler under A1, silently queue before Bind

### Real Bun recovery, **N15**
- **Required real observable:** G1 pending/queued delivery retired, surviving document same checked generation, G2 authenticated after recovery, fresh allowed
  G2 call succeeds without replay
- **Independent RED:** reuse old app-link generation, attribute late G1 reply to G2, reset document stamp

### Core/Guard exact handoff, **N16–N18**
- **Required real observable:** checked native generation equals Guard principal; denied literal channel or stale generation causes zero actual Bun handler
  entries/writes; allowed exact literal sends once
- **Independent RED:** mint Core counter, remove final stamp check, skip `dispatch_privileged`, block UI navigation on full transport

### Credit/native stress, **N20**
- **Required real observable:** one #540 total per-role DATA/CONTROL ledger; rebind/retirement does not refill credit; no hidden retained-copy queues, native
  high-water recorded
- **Independent RED:** give each world fresh quota or release sender credit before actual native consumption

### Platform/reporting, **N21–N22**
- **Required real observable:** unsupported OS outcomes UNKNOWN, no CodeQL/native/product success invented; v1 compatibility and approved Electron comparison
  separately recorded
- **Independent RED:** publish native PASS or new product percentage solely from static docs/model fixture


Each test uses a controlled disposable app/document fixture and an independent effect counter at the **actual Host/Bun/OS** owner where applicable. A single
passing Rust model, signed test-only receipt, WKWebView log, screenshot or CodeQL report cannot replace that counter. Await real observed native/callback
conditions rather than sleeps; timeouts only terminate a failed arm. Capture attempted native navigation/committed resource identities, frame/handler/controller
provenance, event order, one-generation stamp, permission result, one/zero native writes, ledger release and cleanup, and check exact source/OS head. Keep
failures and all attempts; do not cherry-pick later successes.
### E-1: Historical test-owned WKWebView feasibility probe
- **Run:** [GH542 native receipt](https://github.com/gyldlab/keld/issues/542#issuecomment-6383451853)
- **n:** 33 native cases; two isolated credentials under one test-owned checked epoch.
- **Environment:** macOS WKWebView 26.5.1, test-owned fixture.
- **Limits:** This did not prove Keld production URL eligibility, actual Guard/Host/Bun
  forwarding, navigation-attempt races, Bun recovery, real window destruction or native
  resource bounds. It is feasibility evidence only, not GH542 product acceptance.


## 9. Perf impact

None in this docs-only PR. Future native first-proof hot-path work must measure per-message handler check/credential compare, native callback-to-Guard handoff,
peak retained copies and native navigation progress against the current KEL-142 macOS renderer-bridge baseline (same pinned Host/Bun/OS/source workload).
Respect #540's 1 DATA record per direction across role, 4 bounded CONTROL records, 262,144 owner-retained/scratch accounting and no new per-world quota. A
changed >5% runtime metric requires existing performance review; no speed/constant-time or memory-footprint claim arises from a design table or 33-case
feasibility fixture.

## 10. Open questions, blockers & unknowns

### Q-1: GH540 document-correlated Bind handshake
**Owner:** GH540 wire owner.
The current v2 request is `{v:2,kind:"bind"}` and carries no document correlation value.
The response `bound.document` cannot prove which committed document emitted that request.
GH540 must either approve an exact private document-correlated challenge/echo handshake
tied to WebView lifetime, native attempt, committed document and world, or explicitly
leave GH542 dual-world Bind disabled. The exact field names, bytes, delivery and ordering
belong to GH540. This draft does not invent them.

### Q-2: Per-world credential mapping
**Owner:** GH540 and GH542.
If the existing `bound.document` response is reused as a per-world credential, GH540 must
approve that exact interpretation under one shared native checked generation. It must
not become a shared cross-world secret, a caller-selected generation or a Bun role token.
The Bind request still needs independent document correlation.

### Q-3: Native committed URL and attempt identity
**Owner:** GH542 and GH459.
The actual WKWebView callback and verified package/resource interfaces must prove stable
correlation through redirects, cancellations, same-document navigation and failed loads.
If the platform cannot distinguish a queued A message from B's current committed
document, keep the GH542 Bind path disabled. Do not use URL equality or callback timing
as an authority surrogate.

### Q-4: GH484 A1/A2
**Owner:** GH484 and GH467.
The page-to-app transport selection is unresolved. A1 `BroadcastChannel` is test-only
with an untrusted originating sender. GH542 cannot approve native page ingress or the
currently rejected raw-handler alternative. No native permission is enabled by this
draft.

### Q-5: Real native and product acceptance
**Owner:** GH542 native, GH587 and GH589.
The existing test-owned 33-case WKWebView feasibility probe did not run production
source, GH552 Guard, actual Host/Bun credit or G1/G2 recovery. These named cross-owner
gates remain required. macOS product pass, Windows/Linux support and installed
distribution remain UNKNOWN.

### B-1: GH540 document-correlated Bind decision
- **Status:** open
- **Owner:** GH540 wire owner
- **Impact:** GH542 cannot allocate the new dual-world credential or enable app-preload
  binding from the current tokenless Bind.
- **Check:** GH540 approves exact document-correlated Bind bytes, challenge lifetime,
  per-world mapping and failure behavior on its current PR.

### B-2: Native committed-document and attempt proof
- **Status:** open
- **Owner:** GH542 native owner and GH459 Host-load owner
- **Impact:** A queued A Bind may be processed after B commits at the same URL; no GH542
  realm credential may be allocated until the native origin is correlated.
- **Check:** Real WKWebView evidence binds each Bind to the exact current WebView,
  native attempt, committed document and registered world, including stale A/B controls.

### B-3: Shared Guard and permission contract
- **Status:** open
- **Owner:** GH541 and GH552 owners
- **Impact:** Native document identity alone does not authorize a channel or native effect.
- **Check:** Exact current permission snapshot, literal grant and final
  `dispatch_privileged` decision are qualified with zero denied effects.

### B-4: App/page provenance
- **Status:** open
- **Owner:** GH484 and GH467 owners
- **Impact:** No page-origin privileged ingress or AppPreload permission is enabled.
- **Check:** Owner-approved A1/A2 contract and real hostile page/preload tests prove the
  selected provenance boundary.

### B-5: Real macOS Host/Guard/Bun acceptance
- **Status:** open
- **Owner:** GH542 native and GH587/GH589 product-acceptance owners
- **Impact:** No native or draw.io product PASS may be claimed from model, source or
  test-owned fixture evidence.
- **Check:** Real installed Host/Guard/Bun effects, denial counters, stale callback
  controls, recovery and cleanup pass on the exact source/OS head.

### B-6: Exact-content independent approval
- **Status:** open
- **Owner:** 0monish and Amisha
- **Impact:** The specification remains DRAFT; no production implementation is authorized.
- **Check:** Repository owner and independent public/native/unsafe/security reviewers
  approve the exact final specification content.

### U-1: Native committed-document identity availability
The current source checks `WKScriptMessage` world, frame, WebView and controller but
does not expose a committed-document identifier to Bind. Whether the supported WebKit
API can supply a stable, independently correlated identity at the message callback is
UNKNOWN. Resolve through current official platform documentation and a real macOS
callback experiment; until then the fail-closed B-1/B-2 behavior remains mandatory.

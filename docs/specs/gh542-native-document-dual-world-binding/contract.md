# Contract and acceptance

Part of: [index](index.md)
Owns: sections 2 and 3

## 2. Spec refs

- **Approved design, unmerged:** [GH-541 #1123](https://github.com/gyldlab/keld/pull/1123), `docs/specs/gh541-channel-grants-navigation.md`, technical decision
  `7e0c1b89c4767f2e6625cd2bfc4d36d86b30fc26`; KEL-102 decision `8fa29b34-bdb6-470f-9f97-9cdf0aa1d0eb`. One Host/WebView checked-`u32` mint, initial 0 unbound,
  actual cross-document commit rotation, earlier trusted Host-load revocation, same-document/cancel preservation, Bun recovery as a separate axis, immutable
  current-resource eligibility, final Guard handoff.
- **Draft design, not owner-approved:** [GH-540 #1122](https://github.com/gyldlab/keld/pull/1122), current exact head
  `ffd6d16811d684305abd3c17fdbf5c868965ede1`. One `keld.wv-link` v2 closed envelope (`bind`, `invoke`, `send`, `consumed` and corresponding native replies), one
  `compat-control` channel, existing delivery/credit owners and bounded retirement. The current v2 `bind` request is tokenless (`{v:2,kind:"bind"}`), while
  `bound.document` is returned only after Bind. See [GH540 PR #1122](https://github.com/gyldlab/keld/pull/1122) and `docs/specs/gh540-compat-ipc.md:342-345@ffd6d16811d684305abd3c17fdbf5c868965ede1`. The proposed
  per-world interpretation of `bound.document` is not approved. The response value cannot retroactively prove which committed document emitted a tokenless
  request. GH540 must approve a document-correlated Bind handshake before GH542 allocates a new realm credential; this draft adds no wire field or version.
- **Decided world separation:** [GH-441](https://github.com/gyldlab/keld/issues/441#issuecomment-6022098697) selects a distinct app `WKContentWorld` and its own
  world-scoped handler. KEL-142's trusted native handler and nonce must never be shared with or exposed to the app/page world.
- **Consumers:** [GH-484 #1155](https://github.com/gyldlab/keld/pull/1155) is OPEN/CHANGES_REQUESTED at head `895a42afc3a1a127bdd01305512c1de9f39db28d`; its A1
  page-to-app alternative via `BroadcastChannel` is **TEST ONLY**. Native app-handler provenance proves only the last app-world hop. It cannot attest to the
  earlier page sender. A2 (data-only page-native ingress) is an unresolved owner/security decision and this spec neither chooses nor enables it. [GH-467
  #1154](https://github.com/gyldlab/keld/pull/1154) is OPEN at head `5d73c949bb69ad788727a6f9ec2e63316cac696b` and defines the separate contextBridge/facade
  owner. [GH-552 #1191](https://github.com/gyldlab/keld/pull/1191) is at head `cabbb4944901c528dc876a54b0c97528cbbb26d1` and owns literal grant checks and
  `dispatch_privileged`; [GH-499 #1170](https://github.com/gyldlab/keld/pull/1170) is at head `74733512fe1b69a3e6a0b74580afd27bf7391990` and owns the pinned
  migrated application/source; #459 owns the actual host full-document load caller; #464 owns listener conformance.
- **Feasibility, not Keld proof:** [GH-542 native receipt](https://github.com/gyldlab/keld/issues/542#issuecomment-6383451853) and [GH-467 native
  receipt](https://github.com/gyldlab/keld/issues/467#issuecomment-6383445584): real macOS WKWebView 26.5.1 observed two isolated credentials under one
  **test-owned** checked epoch and 33 native cases with negative controls. The fixture did **not** prove Keld's production URL eligibility, actual
  Guard/Host/Bun forwarding, navigation-attempt races, Bun recovery, real window destruction or native resource bounds.
- **FACT — current production source at Pin `ec29b7ce2b155a8c98f96036cbb9c54488b5c302`:** `BridgeState` in
  `crates/keld-wv/src/wkwebview/macos_bridge.rs:323-389@ec29b7ce2b155a8c98f96036cbb9c54488b5c302` stores `navigation_generation: u64` and one `document_nonce`.
  `navigation_started()` uses `wrapping_add(1).max(1)` and clears the nonce; `bind()` checks destruction and WebView binding, sets generation 1 if zero, then
  mints a nonce. It receives no document/attempt correlation value.

**FACT — tokenless message path:** `WvLinkEnvelope::Bind` has no payload at `crates/keld-wv/src/wv_link.rs:40-42,163@ec29b7ce2b155a8c98f96036cbb9c54488b5c302`.
The native handler calls `BridgeState::bind()` and evaluates the returned nonce in the registered world at
`crates/keld-wv/src/wkwebview/macos_bridge.rs:695-762@ec29b7ce2b155a8c98f96036cbb9c54488b5c302`. The handler checks message world, main-frame status, WebView
and controller identity, but those checks do not identify which committed document emitted a queued tokenless Bind.

**FACT — navigation hooks:** current `navigation_started()` call sites include
`crates/keld-wv/src/wkwebview/mod.rs:851,1058@ec29b7ce2b155a8c98f96036cbb9c54488b5c302`. They do not establish a message-to-committed-document correlation at
Bind.

**INFERRED — security consequence, not a claimed exploit:** if A's tokenless Bind is queued and delivered after B commits at the same URL, `bind()` evaluates
current BridgeState and has no request value by which to prove that A emitted the message. GH542 therefore cannot allocate a new dual-world credential from that
message. The real native callback schedule remains an acceptance test, not a fact inferred from source inspection.
- Existing governance: `docs/architecture/02-ipc.md` §§2,5, `docs/architecture/03-security.md` (default deny and WebView principals),
  `docs/specs/kel142-macos-renderer-bridge-api/index.md`, KEL-102 and `docs/agents/spec-template.md`. This draft proposes an implementation detail within their
  approved authority boundaries. If the #540 owner determines per-world `document` interpretation changes normative wire semantics, amend/approve #540 and the
  architecture **before** implementing; do not silently reinterpret approved bytes.

## 3. Acceptance criteria

### AC-1: Checked native document and attempt lifecycle
- **Given:** One real macOS WKWebView with an initial unbound state and native Host load attempts.
- **When:** The engine starts, commits, cancels, fails or replaces a main-frame document, including a same-URL replacement.
- **Then:** One checked `u32` generation and one native attempt identity govern the transition; stale A failure/cancel can release only A-owned work and cannot
  change B's state, credentials, lease or delivery.
- **Negative control:**
  - **Single fault:** Replace checked advancement or attempt matching with wrapping/current-state-only handling.
  - **At-risk precondition:** A is superseded by B and an A callback arrives after B starts or commits.
  - **Expected RED:** B is revoked, rebound to A or loses its retained resource lease.

### AC-2: Fail-closed tokenless Bind
- **Given:** The current GH540 v2 tokenless `{v:2,kind:"bind"}` request and a possible same-URL A-to-B document replacement.
- **When:** A's queued Bind is delivered after B commits.
- **Then:** GH542 refuses the tokenless Bind, allocates no new dual-world credential, emits no `bound.document` for the GH542 app realm and remains unbound for
  that realm until GH540 approves a document-correlated handshake. The existing KEL-142 contract remains separate and is not requalified by this proposal.
- **Negative control:**
  - **Single fault:** Accept the tokenless Bind using only the current WebView/world/controller state.
  - **At-risk precondition:** A sends Bind, B commits at the same URL, then A's callback is processed.
  - **Expected RED:** A's stale Bind allocates or receives a credential under B's current state.

### AC-3: Per-world credentials after approved correlation
- **Given:** A GH540-owner-approved document-correlated handshake and a qualified current native document.
- **When:** The native owner binds the trusted KEL-142 and app-preload realms.
- **Then:** Each realm receives a different native-stored credential under the same checked document generation; the challenge is tied to the exact WebView
  lifetime, native attempt, committed document and registered world. No page-controlled value selects a realm or credential.
- **Negative control:**
  - **Single fault:** Reuse one credential across worlds or accept a missing/stale challenge.
  - **At-risk precondition:** Both realms are installed for the same current document.
  - **Expected RED:** A swapped, absent or stale challenge creates a live realm slot.

### AC-4: Final Guard, role and credit composition
- **Given:** A current qualified document, an owner-approved realm binding, a current authenticated Bun role and a literal channel grant.
- **When:** Invoke/send/consumed or a native dispatch races revocation and role retirement.
- **Then:** The same checked stamp reaches `Principal::Webview`; GH552 `dispatch_privileged` runs before effects; GH540's single role-wide ledger owns bounded
  delivery and retirement; revocation winning first produces zero Host/Bun writes, while a prior dispatch preserves honest outcome-unknown semantics without
  replay.
- **Negative control:**
  - **Single fault:** Bypass final stamp/Guard/role validation or create a second per-world credit budget.
  - **At-risk precondition:** A stale generation, ungranted channel or retired role reaches final handoff.
  - **Expected RED:** An independent Host/Bun effect counter observes a forbidden write or the ledger admits excess retained work.

### AC-5: Real native and product evidence boundary
- **Given:** The exact implementation and owner-approved upstream contracts on a disposable supported macOS system.
- **When:** The native negative controls and real Host/Guard/Bun positive/denied cases execute.
- **Then:** Only observed real effects qualify; model tests, source presence, screenshots and test-owned nonce fixtures do not establish product acceptance.
  Windows/Linux and BFCache remain UNKNOWN until their own real-device evidence.
- **Negative control:**
  - **Single fault:** Replace the real native/Host/Bun handler with a stub or canned success.
  - **At-risk precondition:** A report attempts to claim native or product PASS.
  - **Expected RED:** The independent native/Host/Bun effect oracle detects the missing real operation.

### N-01: Create one real WKWebView without committed eligible document
- **Required observable and independent fault:** Native generation is `0`/Unbound, no credential or privilege and zero prebind queue; accept initial bind → RED

### N-02: Native main-frame initial eligible commit, with pinned actual resource
- **Required observable and independent fault:** Exactly one checked advance `0 → 1`, one qualified URL/source/document stamp; spoof a matching text URL with
  foreign resource → refusal

### N-03: Install separate app and trusted script-message handlers
- **Required observable and independent fault:** One document generation with **two different** independently random 128-bit-or-stronger world credentials held
  by native owner; shared token or shared trusted handler → RED

### N-04: Current tokenless v2 Bind, including delayed A Bind after B commits at the same URL
- **Required observable and independent fault:** Refuse; allocate no GH542 dual-world credential and send no `bound.document` for the new app realm until GH540
  approves a document-correlated handshake; accepting A's stale Bind for B → RED

### N-05: `invoke`, `send` or `consumed` from either realm
- **Required observable and independent fault:** Admit only if **both** native stored credential exists and supplied nonempty `document` equals that world's
  stored value, the realm permits the kind, and current stamp/role match; missing stored+supplied token or swapped credential → zero forwarding

### N-06: App handler receives a page-relayed message
- **Required observable and independent fault:** Native can attest to the app-world **last hop only**; page-origin claim/`BroadcastChannel` label never supplies
  principal, grant, exposure policy or credit; forged page-origin receipt → zero privileged effect

### N-07: Observe actual renderer cross-document **commit**, even same final URL
- **Required observable and independent fault:** Revoke both realm credentials and prior deliveries/receipts; checked advance exactly once; late old call and
  stale bind completion fail

### N-08: Hash change, `pushState`, `replaceState` or real same-document history
- **Required observable and independent fault:** Preserve native stamp, both live realm credentials, current pending work and valid grant context;
  rotate-on-hash mutation → RED

### N-09: Cancel/fail renderer-initiated provisional navigation without replacement commit
- **Required observable and independent fault:** Preserve current document and ability to send; provisional-start invalidation → RED

### N-10: Accept trusted Host full-document load/reload
- **Required observable and independent fault:** Revoke before calling engine; advance checked epoch, enter AwaitingCommit with one native attempt correlation;
  even failed load never restores old bind

### N-11: Host load commit/failure/cancel and A→B late A failure/cancel before or after B commits
- **Required observable and independent fault:** Only the matching current attempt can commit with a second checked increment or fail/cancel into FailedClosed;
  duplicate commit is no-op; obsolete A callback releases only A retained work/charges exactly once, never B's lease, and preserves B AwaitingCommit or B Live
  with its bindings and eligible work; unknown callback without owned work releases nothing

### N-12: Actual uncorrelated replacement commit or unauthorized redirect
- **Required observable and independent fault:** Revoke previous document and stay unbound/failed-closed until independently qualified transition; same URL
  cannot bypass; a page-provided URL cannot authorize

### N-13: Native `u32::MAX` plus one required advance
- **Required observable and independent fault:** Enter terminal Exhausted; no saturation/wrap/truncation/reset/zero media shortcut, no realm rebind, no request
  or delivery

### N-14: Destroy WebView, native controller or target document
- **Required observable and independent fault:** Revoke both credentials and pending callbacks; no same-address/new-view reuse and no post-destruction effect

### N-15: Bun/app-link role generation recovers while WebView survives
- **Required observable and independent fault:** **Preserve** genuine native document generation and committed URL; revoke original app-link/correlation and all
  old role-bound work, reauthenticate successor before any new forwarding; no automatic replay or old receipt retargeting

### N-16: Dispatch to currently authenticated main after native admission
- **Required observable and independent fault:** One captured checked generation used identically in stamped bridge request and `Principal::Webview { id,
  generation }`; any independent Core counter → RED

### N-17: Native dispatch races retirement or blocked app-link
- **Required observable and independent fault:** Final nonblocking check/forward uses existing #552 Guard and #540 ledger; if revocation wins, zero Host/Bun
  writes; if prior dispatch won, retain Outcome Unknown without replay; never hold native lock across blocking IO

### N-18: Normal sender with ungranted literal `el:topic`
- **Required observable and independent fault:** Existing GUARD003/`ChannelForbidden` before any Bun write, including with valid realm credential; bypass Guard
  → RED

### N-19: Prequalification first-inline app setup sends early `bind` or `send`
- **Required observable and independent fault:** Deterministic local refusal/`KELD-WV-011`-class not-sent outcome, zero native queue and no Host/Bun effect;
  never silently drop, automatically wait, replay or grant from script visibility

### N-20: Record limits or native delivery after revocation
- **Required observable and independent fault:** No new per-realm allowance; use one #540 total credit/FIFO/quiescence owner with bounded records and stale
  delivery rejection; unlimited rebind queue or second world quota → RED

### N-21: Classify supported platform/engine
- **Required observable and independent fault:** Only native macOS WKWebView qualifies; Windows/Linux and BFCache/restore, remote and arbitrary subframe proof
  remain UNKNOWN until separate real-device oracle

### N-22: Report product/security result
- **Required observable and independent fault:** Keld-only model/probe/docs tests do not turn GH-551 Electron navigation rows 10–12 from UNKNOWN or establish
  GH-589 draw.io product PASS; source-only claim of native/CodeQL passing → reject

### N-23: A sends tokenless Bind, B commits at the same URL, and A's Bind callback is delivered afterward
- **Required observable and independent fault:** No GH542 realm credential or bound response is allocated; the app realm remains disabled until an
  owner-approved correlation handshake; remove the correlation refusal and stale A allocates B's slot → RED

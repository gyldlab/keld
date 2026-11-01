# Spec: macOS app preload runtime (GH-484 / F07-T1)

Status: draft
**Approval/production qualification: BLOCKED** by authenticated
first-inline callable publication, verified package-image authority
and #540/#542 dual-world binding.
GitHub: GH-484 · Owner: @0monish · Updated: 2026-10-25

**Approval scope:** This proposes an extension to KEL-142. The original KEL-142 approval does not approve an app-owned third world, native handler, preload source authority or renderer facade. No implementation, native acceptance or merge follows from this draft.

## 1. Goal & non-goals

Give macOS draw.io a secure, explicitly **first-proof-subset** Electron preload runtime. Execute its **unmodified** preload at document start in its own app-owned `WKContentWorld`, distinct from both the untrusted page and the existing KEL-142 trusted renderer-bridge world. Register a world-scoped native app handler on the **same per-view `WKUserContentController`** and consume the KEL-142 bridge's current frame/view checks plus #542's proposed **single** navigation-eligibility owner. Named Electron `el:<topic>` messages must consume **#540's approved v2 contract and #465's canonical decoder extension**, with the host grant contract from #541/#552. The [GH467 selected MVP Option A](https://github.com/gyldlab/keld/issues/467#issuecomment-6390880361) uses an **at-document-start, predeclared four-method page facade** (`request`, `registerMsgListener`, `sendMessage`, `listenOnce`) and copied/frozen `window.process.type` and real `.versions` facts. Its calls must resolve to those methods **actually registered by the authenticated unmodified app preload**, not a page-supplied function table. The **conditional** `.configuration` adaptive-colors result for a valid flag and previously nullish setting must also be observable by the **first inline page script**. Arbitrary dynamic Electron `contextBridge.exposeInMainWorld` graph publication is a **deferred, explicitly unqualified divergence**, not an Option A compatibility PASS. Test-only static publication timing has been observed; production callable binding, Host/Bun dispatch and security approval remain unproved.

Non-goals: #467 owns the bounded Option A page method/argument/callback facade, `ipcRenderer` event semantics, future separately reviewed full dynamic `contextBridge` graph and the complete draw.io two-request round trip. #473 owns page resources/CSP, not preload source admission. #567 owns F07 conformance cells and expected-fail policy; #540 owns the v2 wire/carriage spec, #465 the canonical decoder/send extension, #541/#542/#552 the grant and navigation-generation contracts, **#543 owns host-to-renderer event delivery**, and #464 owns the related IPC conformance cells. No new KIPC channel, second parser, nonce, grant store or ambient Node resolver in #484; no general dynamic graph parity, synchronous `sendSync`, bundled multiple preloads, WebView2/WebKitGTK implementation, production performance win, or changes to the KEL-142 `window.keld` public facade.

### 1.1 Selected A2 direction, precise security gate

The 2026-11-01 product choice is A2: one untrusted page-data-only ingress. Exact independent GH484/GH467/KEL142/architecture-05 source approval and real GH540/542/552 guard evidence remain required. Historical A1 alternatives and test-only F0 observations are retained as evidence; production approval is still BLOCKED. The detailed additive contract and negative controls are in §4.3A.

## 2. Spec refs

| Source | Contract or observed state |
| --- | --- |
| [Architecture 04 §3](../architecture/04-electron-compat.md#3-how-the-shim-works-layers) | A preload runs before page scripts; the renderer shim is distinct from native policy. |
| [Architecture 05 §2](../architecture/05-webview-and-native.md#2-renderer-bridge-contract-windowkeld) | The generic bridge and engine seams; add an app-world clarification only after owner coordination. |
| [Architecture 01 §5](../architecture/01-overview.md) and [03](../architecture/03-security.md) | Performance budgets and default-deny native authority. |
| [Approved KEL-142](kel142-macos-renderer-bridge-api.md) §§4–8, AC2/AC5/AC6 | Trusted isolated world, host-minted document nonce, bounded `keld.wv-link/v1` parser and real macOS isolation tests. |
| [PANEL-D21 #441](https://github.com/gyldlab/keld/issues/441) | Decided **A**: third app-owned world and separate world-scoped native handler on the same per-view controller; reject sharing KEL-142 trusted world. |
| [#484 technical handoff](https://github.com/gyldlab/keld/issues/484#issuecomment-6381705299) | Proposed contracts and falsifiers; **not** a spec approval or native test result. |
| [KELD native macOS bridge at `b4e9f4f2`](https://github.com/gyldlab/keld/blob/b4e9f4f2e0dc88ccb14407e4e839b50fabb6be50/crates/keld-wv/src/wkwebview/macos_bridge.rs), [`wv_link.rs`](https://github.com/gyldlab/keld/blob/b4e9f4f2e0dc88ccb14407e4e839b50fabb6be50/crates/keld-wv/src/wv_link.rs) | Existing `defaultClientWorld` + page facade scripts and one native handler at `AtDocumentStart`; native checks world/view/frame/controller. **Current `decode_wv_link` is v1 only: bind and numeric-u16 invoke, no named `el:` send/event.** Bind and `evaluate_in_world` are asynchronous; no app-world preload exists. |
| [#540 IPC v2 spec](https://github.com/gyldlab/keld/issues/540), [#465 canonical send extension](https://github.com/gyldlab/keld/issues/465), [#543 host-event implementation](https://github.com/gyldlab/keld/issues/543), [#464 IPC conformance](https://github.com/gyldlab/keld/issues/464), [#541 grant amendment](https://github.com/gyldlab/keld/issues/541), [#542 navigation mint](https://github.com/gyldlab/keld/issues/542) | Dependency contracts: v2 named send/event, bounded credit, one canonical parser, exact `el:` grants, and generation rotation at cross-document commit plus earlier host-initiated-load invalidation. #540's approved specification is **not** an implementation in current v1. World-scoped credential admission for *both* app/trusted handlers is unresolved under #540/#542. |
| [KELD boot walker](https://github.com/gyldlab/keld/blob/b4e9f4f2e0dc88ccb14407e4e839b50fabb6be50/crates/keld-core/src/app_session.rs#L1746-L1820) | Uses directory-relative `openat`, `O_NOFOLLOW`, regular-file and bounded read. It currently **drops** the root handle after boot and rejects symlinks, so is not yet a preload loader. |
| [Electron v44.4.5 `init.ts`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/sandboxed_renderer/init.ts), [`preload.ts`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/sandboxed_renderer/preload.ts) | Immutable sandboxed preload module map, wrapper and error wording, pinned to Electron commit `694f45852a0f1726cd23bfd379854de489cccb65`. |
| [draw.io preload](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron-preload.js) | Unmodified source at `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`: seeds `.configuration` **only** for a `--initial-adaptive-colors=` flag with value `auto`, `simple` or `none` when the existing `defaultAdaptiveColors` is nullish, catches malformed storage, registers IPC callbacks and exposes `electron` **and** `process`. |
| [Apple WKContentWorld](https://developer.apple.com/documentation/webkit/wkcontentworld), [world-scoped handler](https://developer.apple.com/documentation/webkit/wkusercontentcontroller/add(_:contentworld:name:)) | Isolated named world and handler API availability. These sources do **not** by themselves guarantee synchronous cross-world `contextBridge` publication before the first inline script. |
| [2026-10-24 physical macOS F0 baseline/negative control](https://github.com/gyldlab/keld/issues/484#issuecomment-6382021064) | macOS 26.5.1 ARM64 real WKWebView: first-inline page observed `typeof window.electron === 'undefined'` after app-world JS assignment, while a **static** page-world injection was visible. Mutating only the static bootstrap's world changed the page's first-inline method `function → undefined` with native handler still firing. **Partial isolation evidence only:** no dynamic `contextBridge` proxies, authentic draw.io preload, source-root admission or grant proof. |
| [2026-10-24 native F0 16/16 first-inline proof](https://github.com/gyldlab/keld/issues/484#issuecomment-6382401714) | Real macOS WKWebView + **unmodified pinned draw.io preload** with a **test-only** two-way synchronous publication handshake: first-inline page observed all four callable methods, copied process subset, frozen API and adaptive-colors seeding for the tested missing-preference/valid-flag fixtures, **16/16** across both app/page registration orders. An unguarded one-shot publication showed nondeterministic order; changing to `AtDocumentEnd` made first-inline API unavailable, and suppressing exposure gave **3/3 negative**. This proves local timing feasibility, **not** authentic `contextBridge` or Host authorization. |
| [2026-10-24 nested callback F0 6/6 with RED](https://github.com/gyldlab/keld/issues/467#issuecomment-6382468009) | Same immutable unmodified preload in real WKWebView, but **synthetic test-only IPC**: **6/6** positive runs exercised real preload `request` and `watchFile` with two nested callback deliveries and no serialized function; suppressing only nested callback-token revival caused **6/6** missing change callbacks, while top-level callbacks still worked. Direct forged DOM events against unexposed functions were refused in that fixture, but shared DOM events are **page-forgeable**; no genuine Bun effect, native credential, grant, teardown or release was proved. |
| [2026-10-24 dual-world native credential F0](https://github.com/gyldlab/keld/issues/467#issuecomment-6383445584) | Real WKWebView fixture with **one test-owned checked-u32 epoch** and separate app/trusted native credentials: 33 native receipts per run, 3/3 corrected baselines, independent credential-check and wrong-role-operation RED mutants. Its initial missing/missing optional credential equality was a fixture defect; the repair requires an existing stored binding and supplied credential. This supports only native-realm feasibility, **not** KELD #542 document authority, #552 guard, Bun link, source integrity or first-inline callable dispatch. |

**Current-documentation receipt (2026-10-24):** Applicability: pinned sandboxed Electron and Apple WKContentWorld API. Context7: `/electron/electron` resolved and queried for sandbox preload and contextBridge; results index evolving `main` docs and are discovery only. Official primary: immutable Electron commit above and Apple API documents. Supported claim: pinned nine-module wrapper, error behavior and world-scoped installation are available as design inputs. Supplemental **native test-only** F0 receipts above establish first-inline scheduling and local nested callback feasibility on this Mac, with no security authority. Fallback/blocker: approved production `contextBridge` publication, unforgeable method dispatch, native original-document bind and Host grant proof remain unproved. `origin/main` was `b4e9f4f2e0dc88ccb14407e4e839b50fabb6be50` on 2026-10-24, equal to the handoff pin.

### 2.1 Proposed A2 amendment to the KEL-142 boundary

The approved KEL-142 numeric-u16 window.keld.invoke, v1 decoder and one trusted non-page world/handler remain unchanged. For a **separately approved** macOS Legacy draw.io first proof, A2 adds one page-visible **untrusted-data-only** handler and one distinct app-preload-world handler on that same WKUserContentController. The page handler cannot mint/read a document, principal, token, native credential, KIPC endpoint, filesystem authority or Host grant. App and trusted worlds hold distinct native-only credentials under **one** GH542 checked committed-document owner. A direct raw page request naming a permitted registered original app method is processed exactly as a wrapper call, through matching current app-private registration, current native view/mainframe/document, app bind and bounded copy/callback checks. [GH467 §4.3A](gh467-contextbridge-ipcrenderer.md#43a-a2-local-original-function-copy-rules) owns local registration versus actual outbound/inbound prerequisites: no outbound send credit/grant is required merely to install a local listener; real sends and deliveries retain their own authorization. An unexposed or stale call admits no slot or forbidden work; a denied actual outbound topic has zero Bun effect. Shared DOM and user-provided ids cannot authenticate or bypass checks.

The page's exactly four fixed void methods and frozen process facts are inserted at document start, but real original preload **execution**, original app-private registrations and applicable .configuration seed must independently complete before first inline for product acceptance. Original KEL-142 AC2 numeric Echo, media/profile controls and Strict defaults remain regression-protected. **Historical KEL-142 approved status does not approve this proposed addition.** Root/independent native and security owner must first approve the exact A2 amendment and verify native hostile raw/wrapper/current-document/final-Guard RED/GREEN before enabling it.

**Proposed A2 acceptance falsifiers (not historical v1 AC evidence):** with current original registration and native document/app bind but no outbound grants or send credit, raw and wrapper registerMsgListener return undefined, retain one bounded document-local slot and write zero Bun bytes; requiring send authority makes the healthy control fail. Swap the native document or omit private registration and require zero slot growth; a wrong-target inbound mainResp must invoke zero callbacks under GH543, while a separately authorized/credited response to an actual original request invokes exactly once. Deny the actual el:rendererReq grant and require zero attempted Bun writes. Original internal ipcRenderer.on setup before commit remains inert; an early page-carrier registration retains no work and never replays. Run unchanged KEL-142 numeric Echo separately. Method policy remains owned by [GH467 §4.3A](gh467-contextbridge-ipcrenderer.md#43a-a2-local-original-function-copy-rules).

The approved KEL-142 specification and its approval-bound bytes remain unchanged. This separate GH484 draft owns the proposed additive A2 boundary; the historical KEL-142 approval does not authorize it. Exact A2 source/security approval and native acceptance are separate requirements.

## 3. Acceptance criteria (binary, each becomes a test)

| ID | Trigger | Required independent observable |
| --- | --- | --- |
| AC01 Realms | Construct one eligible macOS 11+ WKWebView | One controller contains three scoped native handlers: KEL-142 trusted and separate app handlers with native credentials, plus one page-data-only untrusted handler without any bind/token/principal. All three are main-frame scoped and native-view checked; no page-world trusted fallback below macOS 11. |
| AC02 **First-inline blocker** | Execute **unmodified** draw.io preload and the actual first inline page script across the adaptive-colors fixtures in §4.2 | Exactly four **predeclared** `window.electron` methods (`request`, `registerMsgListener`, `sendMessage`, `listenOnce`) are callable before first inline; page `window.process.type` and `.versions` are copied/frozen facts **from the actual preload/session**. Independently prove the original unmodified **app-world preload script executed, registered the four actual functions, and completed any applicable `.configuration` startup seed before first inline**; a visible static wrapper alone is insufficient. Subsequent effect-bearing calls reach only matching **current original app-world preload registrations** after native bind and Host grant; missing/mismatched registrations are refused, not replaced by canned page stubs. With valid `--initial-adaptive-colors=` mode (`auto`, `simple`, `none`) and absent/nullish existing field, first-inline `.configuration.defaultAdaptiveColors` equals that mode. Missing/invalid flag and explicit existing value leave storage unchanged; malformed JSON is caught and does not abort page loading. Observations are synchronous **before** first inline; delayed Promise, late-filled placeholder, forged DOM method identity or generic dynamic-contextBridge parity claim fails. |
| AC03 Exact `require` | Preload calls nine admitted specifiers, then `require('fs')` | Exactly §4.2 alias/module identities resolve; `require('fs')` synchronously throws `Error` with exact message `module not found: fs` and has no Node filesystem effect. |
| AC04 Electron module | Preload inspects `require('electron')` | Seven named renderer members: `contextBridge`, `crashReporter`, `ipcRenderer`, `nativeImage`, `sharedTexture`, `webFrame`, `webUtils`. Unimplemented operations throw registry-approved typed `KELD-COMPAT` unsupported errors, never silent no-ops. #467 owns functioning contextBridge and ipcRenderer APIs. |
| AC05 Error | Top-level admitted JS preload throws | `Unable to load preload script: <path>` plus error logged, and page finishes loading; effects before throw persist. A **file admission failure** rejects window creation before any preload executes. |
| AC06 Process | Preload reads `process` | `type='renderer'`, `sandboxed=true`, `contextIsolated=true`, own-property-free `env={}`; no inherited `KELD_APP_LINK`. `argv` includes exact `additionalArguments` in order; `versions` deep-equals canonical #452/#570 session facts (no locally invented fields). |
| AC07 Root mapping | App supplies computed absolute preload path | Host matches exactly **one trusted package-manifest relative entry** under its retained stage-root identity, distinct from page `file://` and CWD. Component-boundary and exact-entry checks reject unmapped/outside/traversal/root-swap paths before an attacker-selected file read. Unmodified draw.io absolute path must map without a source rewrite. |
| AC08 Source image | Matched manifest entry is admitted | Authenticate its expected SHA-256 **outside the same-UID-writable package tree**; bounded single-descriptor read (512 KiB/window, 8 MiB aggregate), verify every owned byte against that digest **before executing any preload**, then retain immutable Host-owned bytes. An absent independent trust anchor, changed/in-place-raced bytes, any symlink component, special file, +1-size or +1-capacity refuses. No self-derived digest or reopen-by-display-path. |
| AC09 Window/document lifecycle | Same window navigates; Bun crashes/restarts; window retires | Reuse same window's source snapshot but **rerun in new document with fresh JS callback state**. No forced reload/reset of surviving page on Bun crash, no source re-open on navigation; destroy releases handles/bytes/handler once. |
| AC10 Trusted isolation | Real draw.io preload + hostile app-world script | Original KEL-142 native AC2 passes unchanged. Hostile preload cannot read/wrap trusted handler/bind hook, capture nonce, endpoint or grants, re-export them or get forged native/Bun effect. |
| AC11 Identity | Wrong world, controller, view, subframe, app/trusted credential or old/destroyed document sends app message | Host checks native per-view identity and **one implemented #542 generation/eligibility owner** before dispatch. #540/#542 must approve **two distinct native-owned world credentials**; an initial world bind is authenticated by native controller/world/frame/document facts and mints one new credential, while a later effect-bearing send requires an existing stored binding **and** supplied matching credential (missing/missing denies). Never expose/reuse KEL-142's trusted nonce in app/page or mint a second generation. Revoke both at cross-document commit or earlier host load; preserve at same-document/cancelled renderer provisional navigation. Forged/duplicate/stale bindings have zero effects and bounded settlement. |
| AC12 Permission & parser | App-world v2 message names an `el:<topic>` with no or wrong exact-literal grant | **One canonical decoder**, extended by #465 after #540's approved v2 contract, handles named one-way send with bounded credit; #543 owns host-to-renderer event delivery and #464 its IPC conformance cases. #484's app handler consumes the approved decoder result through the existing app link/guard. Current v1 **cannot** accept a named channel. Default deny before effect, with no wildcard/prefix match; a correct exact grant permits only bounded, credited effects. No page-supplied principal, permission or KIPC endpoint. |
| AC13 A2 untrusted data only | Raw allowed method and fixed wrapper invoke the same current registered original app-world function | Native A2 page handler receives untrusted copied values only, never credential/principal/grant; both ingress paths require exact allowed method, actual private app registration, GH542 native view/main-frame/committed document, stored matching app credential and bounded document-owned copy/callback checks. Apply [GH467 §4.3A](gh467-contextbridge-ipcrenderer.md#43a-a2-local-original-function-copy-rules) to distinguish local registration, outbound effects and inbound delivery; an outbound grant or GH540 send credit is not a local-registration prerequisite. An allowed direct raw call must work when its applicable checks pass; unexposed, stale or unregistered calls have zero forbidden effects, and denied actual outbound topics have zero Bun writes. Falsifier: remove every outbound grant and send credit, then compare raw/wrapper registerMsgListener slot creation and undefined return; swap native document or remove private registration and require zero slot growth. Shared DOM and page-supplied metadata never prove identity. |
| AC14 Diagnostic | Measure app-native handler cost | One metric ID in existing `gyldlab/keld-benches` registry, required oracle/census/clock/statistic, class `diagnostic`, zero threshold and publication-ineligible. |
| AC15 Conformance first | Implementation is authorized by a subsequent approved spec | #567 F07-T4 expected-fail cases with runner assertions exist before change; flip **only** #484-owned world/preload/require/process entries after independent native proof. #467 owns the bounded app-specific facade/IPC/round-trip cells; all unimplemented **doc-cited** general dynamic `contextBridge` rows remain expected-fail or an independently approved recorded divergence, never UNKNOWN by default or PASS from the static wrapper. UNKNOWN applies only to genuinely unqualified engine-specific observations. |
| AC16 Native receipt | Claim Mac acceptance | Exact host/Electron/draw.io hashes, macOS/WebKit/Bun, view/frame redacted document identity, first-inline synchronous observation, native effect census, separately run single-fault RED controls, then GREEN on restored tree. |
| AC17 Narrow page ingress | Exact original admitted local Legacy A2 window | One page-visible untrusted-data handler is permitted after this amendment's independent approval; it has zero principal, bind, token, native document, grant, Node, filesystem, KIPC or app/trusted handler authority. Existing numeric KEL-142 trusted facade is unchanged; no implicit Strict or other-platform acceptance. Missing source/current app registration/document denies page-carrier admission; a missing actual outbound grant denies its Host/Bun effect under [GH467 §4.3A](gh467-contextbridge-ipcrenderer.md#43a-a2-local-original-function-copy-rules). |

**Error allocation is proposed, not approved:** `KELD-WV-011` stays KEL-142 native rejection; `KELD-WV-012` is already reserved by approved #540 for **no admitted Bun generation with zero attempted write**, and `KELD-WV-013` for outcome-unknown. Neither means a preload file-admission failure and neither can be reused. Registered `KELD-COMPAT-001..009` remain unchanged. The error-registry owner must allocate a **distinct noncolliding preload source-admission code**, with structured reason (`unmapped`, `outside`, `file-kind`, `oversize`, `root-unavailable`, `I/O`, `capacity`, `symlink`, `trust-anchor-missing`, `digest-mismatch`), before F1 implementation; the #467 facade owner independently allocates its graph/unsupported-member codes. Negative control: an untrusted manifest or changed source must remain distinguishable from absent Bun generation and cannot execute even partial preload effects.

## 4. Design

### 4.1 Atomic ownership and reuse

| Atom | Owner / input → output | Failure and falsifier |
| --- | --- | --- |
| World (W) | #484, per-view WKWebView config → third named world + app handler + main-frame script | Wrong-world app script can read trusted nonce/handler; swap app/trusted world to make native test RED. |
| Ordering (O) | #484 document-start static wrapper + app preload registry + #467 bounded method/callback path → exactly four callable page methods/frozen process facts before first inline | Delay wrapper until microtask → first-inline RED; omit actual matching app-private registration → zero real effects despite visible page methods. Static timing is test-only feasible; Host-qualified method binding remains blocked. |
| Runtime (R) | #484 exact Electron preload wrapper + #452/#570 process facts → app-only globals | Insert `fs` require, clone role env, hardcode versions (separate REDs). |
| Seed (A) | Unmodified draw.io `additionalArguments` and existing `.configuration` → optional first-inline setting | Invalid/missing flag overwrites storage; existing explicit preference is replaced; malformed JSON aborts preload/page. Separate REDs. |
| Source (S) | #484 retained stage root + externally authenticated digest → bounded immutable Host-owned preload image | Same-UID in-place writer, spoofed adjacent digest or stage/root substitution yields an unapproved executable byte; independently anchored digest and hostile-write oracle RED. |
| Document (D) | #542 proposed single native view/generation owner → current-document admission after its own approval/implementation | Old document call reaches new Bun handler, or same-document hash change spuriously rotates. |
| Bind (B) | #540's approved v2 contract + #542's proposed native owner + world identity → realm-scoped current-document admission, design pending | App handler accepts trusted nonce, page-forged credential or stale bind; test one mutation each before enabling v2. |
| Grant (G) | #540/#465 canonical v2 decoder + #541/#552 enumerated `el:` permissions + #484 handler → bounded guard-approved app call | V1 numeric invoke is misread as named send, or missing exact grant produces host effect. |
| Metric (M) | Existing keld-benches KEL-90 registry + #484 handler callback → diagnostic only | Change class to publishable or add threshold; registry gate must reject. |

These atoms have independent oracles: `require('fs')` refusal does not prove grants; copied process values do not prove world isolation; descriptor identity does not prove atomic contents under concurrent in-place writes. The host's OS/WebView handles stay on the AppKit main thread; no Bun call, disk read or pending async handler may block that thread.

Reuse the already owned KEL-142 **per-view `WKUserContentController`**, current `BridgeState` view checks and v1 `decode_wv_link`, then consume the **single canonical v2 decoder extension** under #540/#465 for named send/event. Consume #542's checked generation/eligibility owner when landed, one app-link writer/reader, and #541/#552 native guard. Add a new named app-world handler, **never** a second parser, nonce counter, grant registry, KIPC reader or privileged page relay. Until v2 and exact grants are implemented, named app messages **remain refused**; do not reinterpret a numeric v1 invoke as an `el:` channel. The current Wry scaffold does not expose a named-world selector through its ordinary user-script helper; use its existing sanctioned `objc2` macOS configuration seam or fail closed.

### 4.2 Sandboxed preload wrapper and object shapes

Execute the admitted file's **function body** with ordered parameter names `require`, `process`, `exports`, `module`, `Buffer`, `global`, `setImmediate`, `clearImmediate`. `module`/`exports` are per-execution local objects; `global` aliases app-world `globalThis`, never page or trusted globals. Do not expose global Node `require`, Node filesystem/process privileges, role tokens or `process._linkedBinding`.

| Allowed specifiers | Resolved object |
| --- | --- |
| `electron`, `electron/common`, `electron/renderer` | One restricted renderer Electron object |
| `events`, `node:events` | One bounded `events` shim |
| `timers`, `node:timers` | One bounded `timers` shim |
| `url`, `node:url` | One bounded `url` shim |

Every other specifier throws synchronously with `new Error("module not found: " + name)`; no fallback path or Node/relative resolution. The Electron object has precisely the seven names in AC04. Calls to unsupported members throw a registered typed compatibility error; the #467 owner delivers usable `contextBridge` and `ipcRenderer` surfaces (including IPC listener setup required by unmodified draw.io).

Preload `process` exposes only the needed first-proof subset: `type`, `sandboxed`, `contextIsolated`, `versions`, `argv`, `env`. `versions` is copied from canonical session facts; if facts are missing, fail preload initialization rather than synthesize. `argv` captures window `additionalArguments` at window creation. `env` must have zero own keys; never copy `KELD_APP_LINK` or other Bun role environment into this realm. Draw.io deliberately exports only `process.type` and `.versions` to the **page** via its `contextBridge`.

The [pinned unmodified draw.io preload](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron-preload.js) seeds `.configuration` only when **both** a recognized `--initial-adaptive-colors=` argument exists and its parsed object's `defaultAdaptiveColors == null`. It does not override a preference selected by the user. The seed operation is wrapped in `try/catch`; malformed JSON must not cancel page navigation. The native acceptance fixture varies these inputs independently:

| Argument / existing storage | Expected first-inline result |
| --- | --- |
| Valid `auto`, `simple` or `none`; key absent, or parsed JSON object with absent/`null` value | Write the chosen mode once and preserve unrelated object fields. |
| No `--initial-adaptive-colors=` argument, or an invalid value (for example `bogus`) | Do not read/overwrite storage for seeding; preserve original value exactly. |
| Valid argument, but valid JSON with non-nullish existing mode (including `false` or empty string) | Preserve that explicit value; do not write again. |
| Valid argument and malformed `.configuration` JSON | Catch/log the seed error, leave stored bytes unchanged and allow page **navigation** to finish; this does not prove the later editor app can parse malformed preferences. |

An admitted JS top-level exception logs Electron's exact prefix and error, then permits page load to finish. A source/path/read error fails **window construction** before execution. Any prior localStorage effect of a preload that later throws stays visible; there is no transactional rollback.

### 4.3 Ordering and the feasibility hold

1. **Before constructing the WKWebView or navigating:** capture an authorized snapshot and capture window config; install trusted KEL-142 and app-world handlers plus scripts on the one dedicated `WKUserContentController`. Scripts are `WKUserScriptInjectionTime::AtDocumentStart` and `forMainFrameOnly=true`.
2. **At navigation eligibility boundaries:** consume #541/#542's proposed single native generation owner: a host-initiated load invalidates the previous document **before commit**, a cross-document commit rotates the generation, a same-document change does **not** rotate, and canceled renderer-initiated navigation preserves the eligible document. The original trusted script requests a bind with a native receipt that can be asynchronous; first-inline eligibility is unproved. **Owner gate:** KEL-142 currently accepts one trusted nonce and rejects a repeated bind; #540/#542 must approve **two independently minted, non-reused per-world credentials under one document generation**, with explicit native handler/controller/realm registration and revoke/rebind ownership, or reapprove a changed v2 envelope. **Initial** app/trusted bind is authenticated by native world/frame/qualified document and mints only its own credential once; later effect-bearing messages require both a live stored binding **and** supplied matching credential. The app/page never gets the trusted nonce or mints generation; until an approved app-role bind exists, refuse app v2 dispatch.
3. **A2 selected:** install fixed four-function void page shape at WK document start and one untrusted-data-only page native handler. Run the authenticated **actual unmodified** original preload in separate app world at document start, prove four private registrations and conditional storage seed finished before first inline; static wrapper alone fails. Raw allowed method calls and wrapper calls run the same private registry/current GH542 document/app-bind/copy checks, with effect-specific authorization from [GH467 §4.3A](gh467-contextbridge-ipcrenderer.md#43a-a2-local-original-function-copy-rules); neither is privileged by its route. Original internal ipcRenderer.on setup is inert app-local initialization, not page-carrier admission or an outbound send. Zero queued pre-bind page-carrier work.
4. **Early page-carrier calls (not internal preload listener setup):** a callable page API may exist while host document bind is pending. Consume #467's selected **zero-queue early refusal**: public void method returns undefined, native rejects before reservation/dispatch, emits a bounded private typed diagnostic and retains **no** work for a future document or Bun role. If required draw.io startup traffic occurs before eligibility, application acceptance stays BLOCKED until earlier safe bind/order is proved or #540/#542 approve an amended policy. No silent queue or successor replay.
5. **After bind:** page-carrier calls pass view/world/frame/generation, original registration and bounded local-copy checks. Only actual outbound sends additionally pass the canonical parser/current Bun/credit/per-window grant gates; inbound delivery has independent GH543 checks as classified in [GH467 §4.3A](gh467-contextbridge-ipcrenderer.md#43a-a2-local-original-function-copy-rules). Navigation and destroy invalidate old requests before any new document can dispatch. Recovery cannot reset a surviving renderer.

**A2 direction chosen, exact-source security GO and product behavior still BLOCKED:** previous fixed-wrapper and DOM/BroadcastChannel fixtures only prove test feasibility. The A2 single page-native ingress may accept only untrusted data, never app or trusted credentials; raw allowed and wrapper calls require identical app registry, GH542 current committed stamp/app bind and bounded copy checks, with local/outbound/inbound classification owned by [GH467 §4.3A](gh467-contextbridge-ipcrenderer.md#43a-a2-local-original-function-copy-rules). Independently prove real preload/seed first inline, original two credited requests, missing-registration and forbidden raw-call mutants before qualifying product. No broad dynamic graph PASS.

**Native F0 chronology and limits (not AC02):** [direct app-world assignment](https://github.com/gyldlab/keld/issues/484#issuecomment-6382021064) was invisible to first inline; [the follow-up with the unmodified draw.io preload](https://github.com/gyldlab/keld/issues/484#issuecomment-6382401714) saw nondeterministic app/page script order under naive registration, then **16/16** correct first-inline callable API shapes with an order-independent test-only handshake and separate missing-exposure/`AtDocumentEnd` RED controls. [The 6/6 local proxy fixture with nested callback RED](https://github.com/gyldlab/keld/issues/467#issuecomment-6382468009) proved the *actual preload's* top-level and nested callback paths under simulated IPC. The common DOM/event carrier is forgeable by the page. A private app exposure table blocked an **unexposed** method in that fixture but did **not** authenticate exposed calls or prove original-document, Host/Bun, grant, teardown or release semantics. No production `contextBridge`, wire parser or principal is approved by these results.

### 4.3A. A2 page-data carrier and one private original-method registry (2026-11-01 decision, DRAFT)

The product owner selected **A2** for one local macOS Legacy original draw.io first proof. The single page-visible native handler accepts **untrusted data only**: a fixed method name and bounded owned argument/callback-slot data. It has **no** token, document nonce, principal, world credential, KIPC endpoint, native grant, source path authority or access to the app/trusted native handlers. The same per-view WKUserContentController carries this data-only page handler and distinct, credentialed **app-preload** and existing KEL-142 **trusted** handlers. One GH542 checked native WebView/main-frame/committed-document owner controls every app/trusted bind and every effect-bearing delivery; A2 adds no document mint.

Direct raw page calls naming any of the **same four permitted methods** are valid requests under A2. They are not categorically refused just because they bypassed the visible wrapper. Raw and wrapper requests pass the **identical** exact four-name allowlist, actual original current private app-world function-registration lookup, bounded mutable-copy/Function-slot rules, native checked-current-document/view/mainframe, stored matching app-world bind. [GH467 §4.3A](gh467-contextbridge-ipcrenderer.md#43a-a2-local-original-function-copy-rules) owns the method-effect classification: local registration consumes no outbound send credit/grant; actual outbound and inbound effects retain their separately owned role/credit/authorization gates. Page-supplied method name, proxy id, URL, DOM, claimed origin, realm or navigation metadata never prove that registration or authority. Missing registration, unexposed method, early/precommit/stale document, wrong view/world and, for actual outbound effects, denied exact el: grant each cause a bounded typed private refusal, **zero** forbidden Host/Bun writes and **zero** queued/replayed early work. A DOM or BroadcastChannel event is neither a Function handle nor an authorization channel. The app-world function table remains private, document-bound and created only by actual unmodified preload contextBridge.exposeInMainWorld calls.

**Outbound admission and retained bookkeeping:** [GH467 §4.3A](gh467-contextbridge-ipcrenderer.md#43a-a2-local-original-function-copy-rules) also owns preflight before original request/sendMessage entry and charging any original request/watch state that survives a later refusal. Known denied, unavailable or over-budget outbound calls must not enter the original Function; a later refusal cannot free local charges while original bookkeeping remains. The final Host guard and canonical transport credit remain mandatory and separately released. Local listener registration does not acquire those outbound prerequisites. GH467 BRG001e proves this distinction without rewriting the original preload or fabricating responses.

**Physical first-inline oracle:** inject a fixed four-void-method page facade and copied/frozen canonical process type/versions at WKUserScript AtDocumentStart. Independently prove the admitted **unmodified 3,201-byte source itself executed** in the app world, registered the four actual functions and completed its applicable .configuration adaptive-colors seed *before the real first inline page script*. A static wrapper plus delayed preload/seed is RED; an invented page seed or generic dynamic graph does not qualify. The original script uses require('electron') in its sandboxed preload, registers ipcRenderer mainResp/fileChanged listeners, and holds reqInfo/fileChangedListeners per document. Its internal ipcRenderer.on installations are inert bounded app-local setup and may precede native commit/bind; they mint no send or receive authority. This does not admit an early page-carrier call or any pre-bind native delivery. Falsifier: original internal listener setup succeeds before commit with zero Host/Bun work while an early page-carrier registration returns undefined, emits the private refusal, retains no slot/work and never replays. Preserve its complete source and all four void function bodies, including the nested watchFile msg.listener callback that fires repeatedly. A local GH467 callback slot is not serializable IPC data.

**Private window carrier:** current BrowserWindow sends only WindowRequest::Create and drops preload/argv. Append WindowRequest::CreateWithPreload at postcard **tag 6**, keeping tags 0..5 and all reply/fact variants unchanged, with width,height,x,y plus exact absolute selector and ordered additionalArguments. Require valid safe webPreferences and read all potentially throwing getters before irreversible Create. Bound selector to 2,048 UTF-8 bytes, arguments to 16 entries, 256 bytes each and 2,048 bytes total; reject if the actual serialized record exceeds existing inline frame capacity. A missing/invalid/unsafe selector or source capacity failure creates no native window. Rust's existing Copy derive needs a focused owner review for the new owned String/Vec fields; silent input dropping or a second queue is forbidden. Core binds one immutable selected source+configuration to the **existing build correlation**; UI receives only the existing Copy build identity, never a file descriptor or script payload.

**Source admission:** original absolute src/main/electron-preload.js selector must resolve to exactly one approved retained package-root relative entry, never page codeDir, CWD, symlink or prefix alias. Host authenticates the independently trusted expected SHA-256 outside the same-UID-writable tree, reads one nofollow regular descriptor with 512 KiB/window and 8 MiB/host retained accounting, compares every owned byte to expected digest **before any script executes**, and reuses that immutable image on later navigations. Original snapshot is 3,201 bytes, SHA-256 1091fd21ef727a52ef89bde814aa662eed4a0c3ce32beb858a068a96b285bf98. A neighboring editable graph or file self-hash is not an independent authority. No later path reopen, source rewrite or full stage rehash on the tao UI thread. App process has type renderer, sandboxed and contextIsolated true, versions from canonical GH452/GH570 Host facts, empty own-property env and exactly captured additionalArguments (empty when no flag; original optional adaptive-colors argument when actually supplied).

**Code/credit owner:** GH467 owns local descriptor-limited mutable argument copies and callback functions (depth 16, 256 keys/elements, 65,536 UTF-8 data bytes, 64 live proxies, 64 callback slots, 524,288 charged retained bytes). GH544 owns the **single** @keld/kipc/endpoint-value-codec encodeEndpointValue/decodeEndpointValue 4,096-byte Host/Bun endpoint wire format; it cannot encode JS Functions. GH540 owns one v2 parser/writer/credit; GH543 owns events; GH552 Host owns final exact el: grant. A function/proxy callback lives only in the original document's private app/page registries and cannot be promoted to a transport token. Preserve real listener order, once removal before callback, multi-fire watchFile until unregister/role-bound retirement/document end, and physical native/ring credit release per event.

**Approval/evidence:** This addendum supersedes the former Q1 A1-vs-A2 direction question, and proposes a narrowly amended AC17. It is a selected product direction, **not** an independently approved native/security implementation. KEL-142 v1 numeric Echo, permissions, existing media handler, GH473 source containment and Strict default retain their owners. Root and independent worker15 must approve exact A2 amendments before functional browser runtime, native handler, app effect or scoring. GH467/GH484 first-inline and hostile direct-permitted/unexposed/omitted-registry/denied-grant RED mutants plus two genuine Host/Bun credited requests are required. Generic dynamic contextBridge parity remains expected-fail/deferred.

### 4.4 Preload source authority and retained snapshot

Draw.io's main role computes an **absolute** `webPreferences.preload`
path. That path is a **selector, never filesystem authority**. The Host
must retain the previously admitted package/stage root and an immutable
trusted package-image manifest whose exact *relative* entry for the
preload carries a SHA-256 expected-byte digest. This expected digest
must be authenticated **outside any same-UID-writable package tree**
(for example by a separately verified signed package artifact or a
digest embedded in an independently trusted Host build). Computing
both the file hash and expected hash from mutable package files, or
trusting an adjacent editable JSON manifest, is insufficient. **No
such Host/package trust-anchor integration has been qualified for this
preload; source admission remains BLOCKED until its owner proves it.**

At window construction, map the absolute source to **exactly one**
manifest entry by checking complete path components against the
captured trusted staged-root identity. Reject any lexical prefix alias,
unmapped path, `.`/`..`, empty/invalid component, path changing under
normalization, alternate stage root, page `file://` root or ambient CWD.
The unmodified draw.io path must map as-is; a rewritten preload path
does not qualify. Traverse starting at the retained root descriptor
using directory-relative, no-follow operations. **First-proof policy:
reject symlinks at every component, including apparently in-root
ones.** This deliberately narrows the earlier proposal; if pinned
staged draw.io requires a symlink, leave AC07/AC08 BLOCKED pending
a separately reviewed, manifest-bound, race-safe alias design.
Do not reopen the validated entry using a display path.

**Proposed Rust signatures (illustrative only):**

```rust
struct RetainedPackageRoot; // admitted root fd, stage identity, trust anchor
struct TrustedPreloadEntry; // exact relative path + independently trusted digest
struct WindowPreloadSnapshot; // immutable checked bytes + manifest entry identity
fn prepare_preload(
    root: &RetainedPackageRoot,
    expected: &TrustedPreloadEntry,
    app_absolute_preload: &Path,
    host_total_bytes: usize,
) -> Result<WindowPreloadSnapshot, PreloadSourceError>;
```

Open **one** regular-file descriptor after safe component admission;
check its kind and bounded size, then reserve Host/window capacity
before copying. Away from the UI thread, read at most cap+1 bytes from
that descriptor and require EOF within the cap. Compute SHA-256 over
the **entire owned byte image**, then compare it with the separately
authenticated manifest digest **before any JS preload executes**.
On mismatch, read error, path race, no trusted digest, or exceeded
capacity, reject the window construction with a distinct registered
source-admission reason. No partial execution, silent truncation,
retry via ambient path or fallback to mutable source bytes. The
successful checked image is retained as immutable Host-owned memory
for the window, while the source descriptor may close; navigation
executes that exact snapshot in a fresh app-world JS context.

**Proposed initial limits for approval:** 512 KiB per window, 8 MiB combined per host; reserve budget before source acceptance and release exactly once on view destruction. Oversized files and budget exhaustion are typed construction refusals. Each later qualified navigation uses the **same source snapshot** but gets new JS variables and listeners. Bun recovery never rereads that source and never reloads an otherwise surviving page.

**Same-UID hostile writer contract:** an independent process may
rename/recreate path components, replace symlink aliases and write
**in place** through an already-open source descriptor while Host
copies. Retained fds, file ownership/permissions and pre/post
`fstat` do not establish coherent or authentic bytes. Only a
complete byte-for-byte match to an **independently trusted digest**
can become the executable snapshot; a mixed image fails that check
except for a cryptographic hash collision. A writer that restores
identical trusted bytes before the read does not change the executed
image. Metadata rechecks are optional diagnostics, never the
authenticity oracle. The trust anchor, verified manifest and actual
snapshot implementation remain **UNIMPLEMENTED / BLOCKED**. This
design claims neither atomic filesystem reads nor existing KELD
package-signature verification.

### 4.5 Native admission and lifecycle

The new `WKScriptMessage` handler verifies the exact per-view
controller, **actual source `WKContentWorld`**, main-frame status,
live WebView and **#542's implemented** checked document before
decoding or causing an effect. Proposed #542 custody is one checked
navigation generation with **two separately minted random 128-bit
credentials** held in distinct App and Trusted binding slots. Each
credential is delivered only to its matching isolated world.
**Initial bind** is admitted solely from the correct native controller,
main frame, content world and qualified document; it creates at most
one credential for that world. A later effect-bearing send/invoke
requires both an **existing live stored world binding** and a
present matching credential; missing plus missing is always a deny.
Neither app nor page sees the KEL-142 trusted nonce, and neither can
select a role by message field. Host full-document load/replacement,
destruction and exhaustion revoke both slots through the single
generation owner; same-document changes and cancelled renderer
provisional navigation do not rotate either. Native dispatch rejects
duplicate bind, role-operation mismatch, wrong controller/world/frame,
stale or swapped credentials, and unqualified resource documents.
The [33-receipt test-only native experiment](https://github.com/gyldlab/keld/issues/467#issuecomment-6383445584)
is a feasibility input, **not** this binding implementation or approval.
GH-540/#542 must freeze the precise v2 credential encoding and
single-owner lifetime in independently reviewed specs before use.

The app handler has its own world/name on the **same per-view
controller**. Current `keld.wv-link/v1` accepts only `bind` and
numeric-u16 `invoke`, so named Electron send/event must remain
disabled until #540's v2 schema, #465's **same** canonical decoder,
#543 outbound event delivery, #464 conformance, and #541/#552 exact
grants are implemented. Final forwarding includes the current native
document, captured Bun link identity, and the existing
`dispatch_privileged` allow closure. A page-selected URL, nonce,
role/window, DOM event or preload-provided manifest entry cannot
mint authority. Invalid or retired work uses #540's bounded
settlement; it cannot replay or target a successor. No second
parser, navigation counter, privileged page relay or transport.

An app preload has ordinary DOM APIs and can intentionally call public
`window.postMessage`; that act is not intrinsically a native
isolation breach. **Production `contextBridge` method identities,
callbacks, bind credentials, readiness receipts and app handler
traffic cannot use shared DOM events/descriptors as an authority or
transport.** The separate existing KEL-142 page facade relay keeps
its original owner but cannot authorize an Electron route. If the
runtime cannot securely predeclare and bind the **four** actual current
app functions before the first inline without a forgeable DOM carrier,
**do not enable Option A preload compatibility** until the named
GH467/GH484 security and platform owners approve a sound alternative.
General dynamic `contextBridge` parity remains outside this first proof.

### 4.6 One diagnostic owner

#484 records per-call **handler-local** cost from callback entry through validation/handoff, with a declared monotonic clock, case census (allowed/refused/malformed), statistic (count/median/p95) and source commit. The **single** metric ID lives in the existing `gyldlab/keld-benches` KEL-90 metric registry (candidate `macos.app_preload.handler_callback_ns`, uniqueness/naming awaiting registry-owner check), with class `diagnostic`, no threshold and no publishable acceptance score. X05-T1 is parked and may not duplicate this ID. This excludes Bun latency, queueing, file I/O and renderer JS exposure; do not infer comparative performance from the sample.

## 5. Boundaries

**This draft PR changes only** `docs/specs/gh484-macos-app-preload.md`. No implementation or registry mutation.

After human approval, #484 may propose macOS world/handler changes in `crates/keld-wv/src/wkwebview/*` with `keld-wv/AGENTS.md` sanctioned unsafe coverage, and a retained app package-source owner in the appropriate host/engine seam. The exact source owner must be coordinated with package boot. #567 owns conformance baseline edits; #467 owns JS facade and IPC. `gyldlab/keld-benches` owns the one metric registry ID separately.

Do not edit #467's facade, #541/#542/#552 grants/generation/wire contracts, #473 page resource loading, unrelated core session files or a shared architecture document under another claim. Architecture 05 §2 is generic and does not explicitly prohibit a third app realm; propose a reviewed cross-reference only if its owner agrees. KEL-142 original contract remains unchanged until the separately approved additive amendment is landed.

## 6. Tasks (each ≈ one PR; ordered; no placeholders)

- [ ] **F0 pre-approval feasibility (test fixture only):** physical Mac probes demonstrated **3/3** first-inline static wrappers with **2/2** no-wrapper controls, earlier **16/16** first-inline DOM-handshake shapes, **6/6** nested callback local effects and a separate 33-receipt native two-world credential fixture with independent RED mutants. Remaining **BLOCKED** gate is authenticated fixed-four page→app *actual-method* selection that does **not** use shared DOM/page-native authority, followed by real #540/#542/#552 Host/Bun proof. Keep all test-only evidence labelled; doc-cited general dynamic graph conformance stays expected-fail/recorded approved divergence, and unqualified engine-only observations stay UNKNOWN. If no secure fixed-wrapper composition exists, stop and seek GH467/GH484 owner review rather than use the insecure F0 carrier. GH441 placement remains unchanged.
- [ ] **S0 specification approval:** obtain independent architecture/security review, resolve critical feasibility and root-authority decisions Q1–Q3 with evidence, and seek **human sign-off** on this exact contract. Set `Status: approved` only after that sign-off. All later production tasks require S0.
- [ ] **F1 package source:** first approve a packaging-owned independent digest trust anchor and exact manifest path source. Implement retained admitted root, exact staged-absolute-to-relative mapping, **deny-all-symlink** first-proof traversal, bounded one-descriptor read and authenticated immutable snapshot before JS. Verify in-place same-UID racing writer, mutable-manifest substitution, root rename and caps with RED controls. If draw.io's staged source needs a symlink, block pending separately approved verified alias mapping.
- [ ] **F2 world/wrapper:** add one named app world/handler to the KEL-142 controller, exact nine-module wrapper, renderer process facts, **fixed page-world four-method document-start shape** and app-world real method registry, preserving native eligibility. Do not turn page-wrapper visibility into a bind/grant or claim generic dynamic contextBridge publication.
- [ ] **F3 isolation/grant acceptance (after #540-approved contract, #465/#543/#541/#542/#552 implementations and #464 conformance land):** consume the canonical v2 decode/credit, world-scoped bind and literal grant path; real Mac AC2 regression, hostile preload, two independent windows, navigation/teardown/recovery and per-window `el:` default-deny; independent RED fault for each guard.
- [ ] **F4 facade composition (owned by #467):** connect approved bounded Option A four-method facade to actual private preload registrations with copied arguments, nested callbacks and approved IPC; prove unmodified draw.io two-request scenario. General dynamic graph stays deferred/expected-fail. #567 flips only eligible F07-T4 cells after its own native proof, never arbitrary Electron parity.
- [ ] **F5 diagnostic:** register exactly one handler-cost metric under keld-benches' owner and capture diagnostic-only Mac samples. No threshold or performance claim.

These slices may split for review size without transferring authority between owners.

## 7. Test plan

**Production acceptance tests below remain planned and UNRUN at `Status: draft`.** The separately identified **test-only F0 macOS oracle** ran the 16/16 handshake, 6/6 nested callbacks and their described RED controls (sections 2 and 4.3); its synthetic IPC and forgeable DOM fail production authority requirements. For implementation acceptance, tests must observe WebKit/host/Bun independently of implementation self-report, execute each **one-fault temporary RED mutation** on a disposable branch/fixture, restore bytes exactly, then observe GREEN. Assertions wait on events, not sleeps; timeout is a kill switch.

| Cases / AC | Independent oracle | Individual RED mutation |
| --- | --- | --- |
| `f07.first_inline_complete` / 01–02 | Real WKWebView first inline script captures exactly four **predeclared** callable electron functions and copied/frozen process type/versions, plus **the real preload's applicable conditional `.configuration` state**. Independently observe real current app preload execution/registration and a native guarded call; static page shape alone is insufficient and full dynamic graph is unqualified. | Move fixed wrapper to microtask or `AtDocumentEnd` → first-inline RED; **delay only actual app-world preload execution past first inline while keeping wrapper visible** → conditional seed absent (RED); suppress only app-private method registration → wrapper visible but actual-method effect zero (RED). Restore clean fixture for GREEN. |
| `f07.adaptive_seed_valid_nullish` / 02 | Valid flag for each of `auto`, `simple` and `none`, absent or nullish key in valid configuration object: first inline sees exactly that mode and unchanged unrelated keys | Skip seed on nullish field; positive fixture fails. |
| `f07.adaptive_seed_no_invalid_flag` / 02 | No flag, then invalid flag value, with separate initial storage snapshots: each remains byte-identical and still loads | Treat any flag as accepted; invalid fixture detects overwritten data. |
| `f07.adaptive_seed_explicit` / 02 | Valid flag with explicit prior non-nullish value, including `false` or empty string: original bytes remain untouched | Replace user value unconditionally; sentinel changes. |
| `f07.adaptive_seed_malformed` / 02 | Valid flag with malformed `.configuration` JSON: startup navigation finishes, first-inline methods still exist, stored bytes remain unchanged and console records caught seed failure | Remove preload's catch: the preload wrapper may still permit navigation, but the subsequent `contextBridge` publication is skipped and the exact handled-seed-error oracle goes RED. |
| `f07.world_and_k142_isolation` / 01,10 | Existing KEL-142 real Mac AC2 + separate hostile app preload: trusted handler/nonce unobservable and zero forged native effects | Install app script in trusted world or remove world check. |
| `f07.require_nine` / 03–04 | Exact nine alias probes and `require('fs')` exact error; unsupported members throw typed code | Add `fs` resolver. |
| `f07.js_exception` / 05 | Observe exact console marker **and independently completed page load** | Propagate JS throw into navigation failure. |
| `f07.process_env_argv_versions` / 06 | Canonical session-facts values, fixed additionalArguments, `Object.keys(process.env).length===0`, no `KELD_APP_LINK` | Clone Bun role env; separately hardcode `versions` then vary canonical input. |
| `f07.root_path_and_symlinks` / 07–08 | Actual unmodified draw.io absolute path maps to **one** trusted staged relative entry. Directory descriptors and inode identity prove no out-of-root read; any symlink (including in-root), stage substitution, traversal and similar-prefix outside path deny before source read. | Reopen by display path, permit in-root symlink, use lexical-prefix-only matching or skip a directory `O_NOFOLLOW` check (separate REDs). |
| `f07.manifest_external_trust` / 07–08 | Prove the expected digest comes from an independently authenticated package/Host image outside the same-UID-writable file tree; tamper editable sibling manifest or change pinned expected digest and get typed refusal before JS executes. | Accept hash derived at the same open/read step or an adjacent attacker-writable manifest; independent trust-anchor observer must go RED. **BLOCKED until packager supplies this authority.** |
| `f07.source_kind_caps` / 08 | FIFO/device/dir fail without blocked UI; 512 KiB permitted, +1 denied; aggregate 8 MiB+1 denied | Skip file-kind check or cap (separate REDs). |
| `f07.inplace_concurrent_write` / 08 | Independent same-UID writer changes file bytes **in place** during descriptor read, including equal metadata before/after; mixed/changed owned bytes mismatch trusted SHA-256 and cause refusal **before script side effects**. The unchanged trusted image loads; no path reread. | Remove independent expected digest check or substitute `fstat` comparison; forbidden JS marker must appear and test go RED. **BLOCKED until trustworthy digest and real reader exist.** |
| `f07.snapshot_lifecycle` / 09 | Native snapshot hash stable across re-navigation, JS callback state resets; Bun crash leaves unsent typed page text unchanged | Reopen source on navigation or reload WebView on Bun restart. |
| `f07.frame_document_two_window` / 10–11 | Two real WKWebViews, wrong frame/view, stale navigation, canceled navigation and same-document change, view destruction; app handler effect count remains zero for denied cases and #542 stamp remains stable on same-document navigation | Remove checked #542 generation or rotate on `pushState`/hash change (separate runs). |
| `f07.world_scoped_bind` / 10–12 | One real #542 checked generation owns distinct 128-bit App/Trusted credentials and their native handler registrations. Absent stored+supplied token, swapped world tokens, wrong role operation, duplicate bind, stale navigation and page-forged DOM yield **zero actual Host/Bun writes** while valid app call is guarded; trusted credential remains invisible to app/page. | Remove **present stored bind** requirement, world/role validation or revocation (separate REDs). 33-receipt fixture is feasibility only. **BLOCKED pending #540/#542 binding approval and product proof.** |
| `f07.early_void_no_queue` / 02,11–13 | Before native app bind, every page-carrier method returns undefined with one private refusal and zero retained work/replay. After bind, local registerMsgListener/listenOnce may retain bounded document-local slots without Bun, outbound grant or send credit; actual outbound sends without an admitted Bun session still refuse without retained Host work. A fresh eligible effect succeeds after its own prerequisites. If the actual first required draw.io effect arrives early, C14 remains unqualified. | Buffer a pre-bind call, require Bun for post-bind local registration, or replay a refused outbound effect on a successor; independent local-slot and Bun-effect observers go RED. |
| `f07.grants_decoder` / 12 | Approved #540 v2 canonical decoder, #465 send and #543 host-event implementation, #464 IPC conformance and #541/#552 literal guard; no-grant/wrong-topic zero Bun effects; correct grant permits only bounded credited effect | Treat missing grant as allow, or let numeric v1 invoke count as named v2 send (separate runs). |
| f07.a2_raw_equivalence / AC13,17 | Real first-inline original preload registers four actual app functions, then permitted direct raw page and wrapper calls reach the same current private function under one GH542 stamp/app bind and the [GH467 §4.3A](gh467-contextbridge-ipcrenderer.md#43a-a2-local-original-function-copy-rules) method-effect rules. With outbound grants/credit absent, local registration creates one bounded document slot and returns undefined without Bun writes; actual request(rendererReq) denied its exact grant has zero Bun writes. Unexposed/missing registry/stale/wrong-world calls admit no slot or forbidden work. | Require an outbound grant/credit for local registration, reject all valid raw calls, skip private registration/native document checks, or omit the final actual-send guard in separate RED arms; local/raw control fails or a forbidden slot/effect appears. |
| `f07.metric_registry` / 14 | Unique keld-benches registry ID, diagnostic class and oracle/census/clock/statistic, no threshold | Make ID publishable or add threshold. |
| `f07.expected_fail_flip` / 15–16 | #567 runner proves expected-fail baseline before implementation and owned cells pass only after Mac native evidence | Pass a cell based only on source presence/sentinel. |

Real native receipt must include exact KELD/draw.io/Electron hashes, macOS/WebKit and Bun versions, redacted document/view/frame identity, native event ordering, guard/admission counts, mutation RED/GREEN evidence. No macOS test is implied by source inspection. Windows/Linux remain unqualified. On the low-disk shared machine, this draft's applicable validation is markdown/spec review and lightweight hygiene; complete Cargo fmt, warning-denied clippy, full nextest, docs freshness and hosted matrix remain **implementation PR** gates.

## 8. Review gates triggered

| Gate | Current draft | Implementation condition |
| --- | --- | --- |
| unsafe | **none in this docs diff** | Conditional YES for new native Objective-C paths; sanctioned per-crate allowlist and named independent unsafe evidence on exact code. |
| public API | **YES (proposed contract)** | `require`, process, preload error behavior and first-inline facade availability; #467 independently owns copy/proxy shape. |
| permission model | **YES** | Third-world handler must apply enumerated `el:` grants and #542 checked document lifecycle, with hostile-preload negative controls. |
| dependency addition | **none proposed** | Existing workspace-pinned objc2/WebKit and host primitives reused; a future new dependency reopens gate. |
| wire protocol | **YES (consume approved extension)** | `keld.wv-link/v1` is numeric-only. #540 owns v2 schema and carriage, #465 the canonical named-send decoder, #543 the host-event implementation and #464 conformance. #540/#542 must approve two world-scoped binding credentials under one checked navigation owner (or amend v2), and #484 must prove no duplicate parser/channel/document authority. |

An independent reviewer must assess the **final** spec diff; future code needs fresh named independent architecture/security evidence, not approval borrowed from old KEL-142 work.

## 9. Perf impact

Unmeasured. Third-world script injection, byte snapshotting and one additional native handler could affect startup CPU/I/O, memory per window and handler cost. Do not infer savings. Candidate metric: `macos.app_preload.handler_callback_ns` in the KEL-90-owned benches registry, monotonic clock, exact accepted/denied census, count/median/p95 and OS/commit provenance; **diagnostic only**, no target, no performance pass/fail and no public comparison. The **proposed 512 KiB/window and 8 MiB/host bounds** are resource safety limits, not measured performance. A true architecture 01 §5 regression claim requires an attributed end-to-end baseline on physical macOS.

## 10. Remaining decisions and BLOCKED qualification

**Status remains DRAFT/BLOCKED.** The source and runtime policies below are
proposed contracts for independent approval, **not** production capability.

1. **Q1 — A2 SELECTED / exact native-security GO PENDING:** The one permitted page-visible native ingress is untrusted request data only, independent of actual private registered app methods and two credentialed app/trusted worlds. Allowed raw and wrapper calls share identical method/doc/view/frame/app-bind/copy checks; [GH467 §4.3A](gh467-contextbridge-ipcrenderer.md#43a-a2-local-original-function-copy-rules) owns separate local registration and actual outbound/inbound effect prerequisites. Missing app registration, early/retired/wrong-world page-carrier calls, or a denied actual outbound literal get zero forbidden effects. AC13/AC17 and KEL142/architecture-05 need independent exact amendment review; historical F0 remains test-only. Real source/first-inline seed, genuine two credited requests and one-site RED/GREEN evidence are required. Dynamic graph remains deferred; no implicit Strict or other OS support.


2. **Q2 — absolute preload root, proposed mapping, BLOCKED (package boot/#484):** first produce an independently admitted stage-root/manifest authority, then prove the unmodified absolute draw.io path selects one exact relative entry with retained descriptor, no page `file://` root or CWD, and no path replacement or alias escape. A source rewrite is a failed proof.
3. **Q3 — same-UID source integrity, proposed immutable byte image, BLOCKED (package trust owner/#484):** authenticate the expected SHA-256 outside a same-UID-writable source tree, read one descriptor, compare *complete bytes* before JS, retain immutable owned snapshot. Deny **all symlinks** in this first slice. Independently race an in-place writer while holding the descriptor; absent manifest authority or digest mismatch must prevent any preload effect. `fstat` alone cannot close this gate.
4. **Q4 — snapshot budget, selected for draft:** 512 KiB per preload/window and 8 MiB retained across Host; one snapshot per window, fresh JS on navigation. Pinned upstream draw.io preload blob `44f2f22f159a8a845ff7e573dd10b816383703ec` reports **3,201 bytes** through GitHub's immutable blob API, within the proposed individual cap; the actual staged image, exact/max+1 and aggregate-accounting RED controls still require verification before approval.
5. **Q5 — errors and metric registry, BLOCKED:** allocate a new source-admission code distinct from reserved `KELD-WV-012/013`, a #467-owned graph/unsupported code and one diagnostic-only KEL-90 metric ID. Verify exact code uniqueness and typed reason with positive/negative cases before F1.
6. **Q6 — early bind, selected fail-closed:** #467 specifies **zero queued early page-carrier calls**, public void-return and internal typed refusal until #542 native bind; actual sends additionally require a live Bun generation. Original internal ipcRenderer.on setup and post-bind local registration follow [GH467 §4.3A](gh467-contextbridge-ipcrenderer.md#43a-a2-local-original-function-copy-rules); no automatic replay. Record real startup order. If draw.io's first required request arrives early, application acceptance stays BLOCKED; any alternate buffering design needs #540/#542 reapproval.
7. **Q7 — original KEL-142 architecture amendment, BLOCKED:** #484 and the architecture-05/KEL-142 owners must explicitly approve the third-world/second-handler addition without changing historical KEL-142 approval or native trusted facade.
8. **Q8 — product/native acceptance, BLOCKED:** independent real WKWebView/Host/Bun tests must meet AC02, AC07–AC13 and each one-fault mutation. No test-only fixture, digest-only handoff or mock qualifies product evidence.
9. **Q9 — v2 prerequisites, BLOCKED:** #540 specification still requires actual #465/#543 transport, #464 conformance and #541/#542/#552 grants/navigation implementation. Until that, F2 world/wrapper feasibility may be studied but named sends stay disabled.
10. **Q10 — two-world native custody, proposed, BLOCKED (#540/#542):** two distinct native 128-bit credentials under **one** checked-u32 document generation with actual stored-bind/present-token checks, independent role/handler identity and shared revocation. The 33-receipt fixture supports feasibility only; independent owner approval, real code and hostile swapped/replayed/missing tokens with zero Host/Bun writes are mandatory.

**Approval and non-claims:** Original KEL-142 Done does not grant this new app-world authority. Only the explicitly cited **test-only** native F0 executions exist; this document has no production acceptance, source-integrity oracle, authenticated full-API timing/dispatch proof, approval, renderer implementation, shipped draw.io compatibility or merge authority.

# Spec: macOS app preload runtime (GH-484 / F07-T1)

Status: draft
GitHub: GH-484 · Owner: @0monish · Updated: 2026-10-24

**Approval scope:** This proposes an extension to KEL-142. The original KEL-142 approval does not approve an app-owned third world, native handler, preload source authority or renderer facade. No implementation, native acceptance or merge follows from this draft.

## 1. Goal & non-goals

Give macOS draw.io a secure Electron-compatible preload runtime. Execute its **unmodified** preload at document start in its own app-owned `WKContentWorld`, distinct from both the untrusted page and the existing KEL-142 trusted renderer-bridge world. Register a world-scoped native app handler on the **same per-view `WKUserContentController`** and consume the KEL-142 bridge's current frame/view checks plus #542's proposed **single** navigation-eligibility owner. Named Electron `el:<topic>` messages must consume **#540's approved v2 contract and #465's canonical decoder extension**, with the host grant contract from #541/#552. The **complete** `contextBridge`-published `window.electron` and `window.process` APIs, as well as `.configuration` adaptive-colors seeding, must be observable by the **first inline page script**. This timing is an acceptance requirement, **not a proved WebKit capability**.

Non-goals: #467 owns `contextBridge` copy/freeze/function proxies, `ipcRenderer` event semantics and the complete draw.io two-request round trip. #473 owns page resources/CSP, not preload source admission. #567 owns conformance cells and expected-fail policy; #540 owns the v2 wire/carriage spec, #465 the canonical decoder/send extension, #541/#542/#552 the grant and navigation-generation contracts, and #464 the host-to-renderer event lane. No new KIPC channel, second parser, nonce, grant store or ambient Node resolver in #484; no synchronous `sendSync`, bundled multiple preloads, WebView2/WebKitGTK implementation, production performance win, or changes to the KEL-142 `window.keld` public facade.

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
| [#540 IPC v2 spec](https://github.com/gyldlab/keld/issues/540), [#465 canonical send extension](https://github.com/gyldlab/keld/issues/465), [#464 event delivery](https://github.com/gyldlab/keld/issues/464), [#541 grant amendment](https://github.com/gyldlab/keld/issues/541), [#542 navigation mint](https://github.com/gyldlab/keld/issues/542) | Proposed **dependency contracts**, open as of 2026-10-24: v2 named send/event, bounded credit, one canonical parser, exact `el:` grants, and generation rotation at cross-document commit plus earlier host-initiated-load invalidation. They are not existing v1 behavior. |
| [KELD boot walker](https://github.com/gyldlab/keld/blob/b4e9f4f2e0dc88ccb14407e4e839b50fabb6be50/crates/keld-core/src/app_session.rs#L1746-L1820) | Uses directory-relative `openat`, `O_NOFOLLOW`, regular-file and bounded read. It currently **drops** the root handle after boot and rejects symlinks, so is not yet a preload loader. |
| [Electron v44.4.5 `init.ts`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/sandboxed_renderer/init.ts), [`preload.ts`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/sandboxed_renderer/preload.ts) | Immutable sandboxed preload module map, wrapper and error wording, pinned to Electron commit `694f45852a0f1726cd23bfd379854de489cccb65`. |
| [draw.io preload](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron-preload.js) | Unmodified source at `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`: reads `additionalArguments`, seeds `.configuration`, registers IPC callbacks and exposes `electron` **and** `process`. |
| [Apple WKContentWorld](https://developer.apple.com/documentation/webkit/wkcontentworld), [world-scoped handler](https://developer.apple.com/documentation/webkit/wkusercontentcontroller/add(_:contentworld:name:)) | Isolated named world and handler API availability. These sources do **not** by themselves guarantee synchronous cross-world `contextBridge` publication before the first inline script. |

**Current-documentation receipt (2026-10-24):** Applicability: pinned sandboxed Electron and Apple WKContentWorld API. Context7: `/electron/electron` resolved and queried for sandbox preload and contextBridge; results index evolving `main` docs and are discovery only. Official primary: immutable Electron commit above and Apple API documents. Supported claim: pinned nine-module wrapper, error behavior and world-scoped installation are available as design inputs. Fallback/blocker: complete first-inline cross-world API publication remains unproved and needs an OS oracle. `origin/main` was `b4e9f4f2e0dc88ccb14407e4e839b50fabb6be50` on 2026-10-24, equal to the handoff pin.

## 3. Acceptance criteria (binary, each becomes a test)

| ID | Trigger | Required independent observable |
| --- | --- | --- |
| AC01 Realms | Construct one eligible macOS 11+ WKWebView before navigation | Three isolated worlds (page, app, trusted) and **two distinct world-scoped handlers** for app and trusted roles on **one per-view controller**. Main-frame only; below macOS 11 fails closed, never falls back to page world. |
| AC02 **First-inline blocker** | Execute **unmodified** draw.io preload and the actual first inline page script | `window.electron.request`, `.registerMsgListener`, `.sendMessage`, `.listenOnce` are callable; page `window.process.type` and `.versions` are populated; `.configuration.defaultAdaptiveColors` contains the admitted flag. All are ready **synchronously before first inline**; mere marker or delayed Promise success fails. |
| AC03 Exact `require` | Preload calls nine admitted specifiers, then `require('fs')` | Exactly §4.2 alias/module identities resolve; `require('fs')` synchronously throws `Error` with exact message `module not found: fs` and has no Node filesystem effect. |
| AC04 Electron module | Preload inspects `require('electron')` | Seven named renderer members: `contextBridge`, `crashReporter`, `ipcRenderer`, `nativeImage`, `sharedTexture`, `webFrame`, `webUtils`. Unimplemented operations throw registry-approved typed `KELD-COMPAT` unsupported errors, never silent no-ops. #467 owns functioning contextBridge and ipcRenderer APIs. |
| AC05 Error | Top-level admitted JS preload throws | `Unable to load preload script: <path>` plus error logged, and page finishes loading; effects before throw persist. A **file admission failure** rejects window creation before any preload executes. |
| AC06 Process | Preload reads `process` | `type='renderer'`, `sandboxed=true`, `contextIsolated=true`, own-property-free `env={}`; no inherited `KELD_APP_LINK`. `argv` includes exact `additionalArguments` in order; `versions` deep-equals canonical #452/#570 session facts (no locally invented fields). |
| AC07 Root mapping | App supplies computed absolute preload path | Host maps it to the **admitted package root**, not webapp document root or ambient CWD, and retains an authoritative root handle. Unmapped, absolute-outside, traversal and missing-root paths fail with a typed error **before any forbidden file read**. |
| AC08 Source read | Mapped file is admitted | Validated regular-file descriptor supplies one **bounded owned snapshot**; **proposed** max 512 KiB/window and 8 MiB aggregate host retention, both enforced before allocation. Proved in-root symlink succeeds; outside/broken/swap symlink, FIFO, device, directory, +1-size and aggregate overflow fail closed. Reopen-by-display-path is forbidden. |
| AC09 Window/document lifecycle | Same window navigates; Bun crashes/restarts; window retires | Reuse same window's source snapshot but **rerun in new document with fresh JS callback state**. No forced reload/reset of surviving page on Bun crash, no source re-open on navigation; destroy releases handles/bytes/handler once. |
| AC10 Trusted isolation | Real draw.io preload + hostile app-world script | Original KEL-142 native AC2 passes unchanged. Hostile preload cannot read/wrap trusted handler/bind hook, capture nonce, endpoint or grants, re-export them or get forged native/Bun effect. |
| AC11 Identity | Wrong world, controller, view, subframe or old/destroyed document sends app message | Host checks per-view native identity **and the one implemented #542 generation/eligibility owner** before app dispatch. Cross-document commit rotates the stamp; earlier host-initiated load invalidates, while same-document navigation and canceled renderer-initiated navigation do not rotate. No second mint. Each rejection has zero forbidden effect, bounded/settled request state. |
| AC12 Permission & parser | App-world v2 message names an `el:<topic>` with no or wrong exact-literal grant | **One canonical decoder**, extended by #465 only after #540's v2 contract approval, handles named one-way send and #464 event lanes with required credit; #484's app handler consumes that result through the existing app link/guard. Current v1 **cannot** accept a named channel. Default deny before effect, with no wildcard/prefix match; a correct exact grant permits only bounded, credited effects. No page-supplied principal, permission or KIPC endpoint. |
| AC13 No DOM carrier | App-native handler transmits internal requests/results | Native app transport generates **zero page-visible `window.postMessage` carrier** events; KEL-142's separately owned page-facade relay remains in place. Arbitrary app code could itself use public DOM APIs, which is outside this invariant. |
| AC14 Diagnostic | Measure app-native handler cost | One metric ID in existing `gyldlab/keld-benches` registry, required oracle/census/clock/statistic, class `diagnostic`, zero threshold and publication-ineligible. |
| AC15 Conformance first | Implementation is authorized by a subsequent approved spec | #567 F07-T4 expected-fail cases with runner assertions exist before change; flip **only** #484-owned world/preload/require/process entries after independent native proof. #467 owns contextBridge/IPC/round-trip cells. |
| AC16 Native receipt | Claim Mac acceptance | Exact host/Electron/draw.io hashes, macOS/WebKit/Bun, view/frame redacted document identity, first-inline synchronous observation, native effect census, separately run single-fault RED controls, then GREEN on restored tree. |
| AC17 No accidental capability | No preload/grant or unsupported engine | KEL-142 existing `window.keld` behavior unchanged; no ambient Node resolver, secondary wire channel, page-world native endpoint, or Windows/Linux acceptance. |

**Error allocation is proposed, not approved:** `KELD-WV-011` stays KEL-142 native rejection; registered `KELD-COMPAT-001..009` remain unchanged. Propose a **new registry-owned noncolliding code** (candidate `KELD-WV-012`) for preload source admission with structured reason (`unmapped`, `outside`, `file-kind`, `oversize`, `root-unavailable`, `I/O`, `capacity`). The #467 facade owner allocates its own noncolliding unsupported-member code. Never claim allocation based on this document.

## 4. Design

### 4.1 Atomic ownership and reuse

| Atom | Owner / input → output | Failure and falsifier |
| --- | --- | --- |
| World (W) | #484, per-view WKWebView config → third named world + app handler + main-frame script | Wrong-world app script can read trusted nonce/handler; swap app/trusted world to make native test RED. |
| Ordering (O) | #484 startup substrate + #467 copy/proxy implementation → complete page API before first inline | Delay **actual** exposure by microtask; first-inline test goes RED. **Current blocker.** |
| Runtime (R) | #484 exact Electron preload wrapper + #452/#570 process facts → app-only globals | Insert `fs` require, clone role env, hardcode versions (separate REDs). |
| Source (S) | #484 authoritative package-root handle → bounded per-window snapshot | Root swap/symlink escape or special file accepted; independent file descriptor and bytes oracle. |
| Document (D) | #542 proposed single native view/generation owner → current-document admission after its own approval/implementation | Old document call reaches new Bun handler, or same-document hash change spuriously rotates. |
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

An admitted JS top-level exception logs Electron's exact prefix and error, then permits page load to finish. A source/path/read error fails **window construction** before execution. Any prior localStorage effect of a preload that later throws stays visible; there is no transactional rollback.

### 4.3 Ordering and the feasibility hold

1. **Before constructing the WKWebView or navigating:** capture an authorized snapshot and capture window config; install trusted KEL-142 and app-world handlers plus scripts on the one dedicated `WKUserContentController`. Scripts are `WKUserScriptInjectionTime::AtDocumentStart` and `forMainFrameOnly=true`.
2. **At navigation eligibility boundaries:** consume #541/#542's proposed single native generation owner: a host-initiated load invalidates the previous nonce **before commit**, a cross-document commit rotates the generation, a same-document change does **not** rotate, and canceled renderer-initiated navigation preserves the eligible document. The timing and existence of a bind receipt **before first inline code remain unknown**. The original trusted script requests a bind, whose native receipt can be asynchronous; do not mint a new generation merely because a document-start script runs.
3. **Run preload in app world and publish complete page API:** `contextBridge.exposeInMainWorld` must finish dynamically copying/proxying the actual `electron` and `process` exports into page-owned values **before the first inline script**. Preinstalling a sentinel/empty object with later filling does not satisfy this; `evaluateJavaScript` callback completion after navigation alone does not prove it.
4. **Early calls:** a callable page API may exist while host document bind is pending. #467 must explicitly specify a **bounded** queue/rejection behavior that consumes #542 eligibility. It cannot mint JS grants/nonce, create an unbounded queue, or replay pending calls after document/Bun retirement.
5. **After bind:** app-world requests pass view/world/frame/generation checks, then parser and per-window grants. Navigation and destroy invalidate old requests before any new document can dispatch. Recovery cannot reset a surviving renderer.

**Proof gate before implementation:** run a **disposable real macOS WKWebView** with the unmodified pinned draw.io preload and an actual first inline page script synchronously checking callable API members, copied `window.process` and the adaptive-colors `.configuration` seed. Because #467's production facade is not yet present, this **pre-approval feasibility** fixture may provide a minimal *test-only* Electron object implementing the expected exposure mechanism; it proves native scheduling only, never #467's value/callback or authenticated IPC semantics. Include a separate native trace of ordering and a delayed-publication RED mutation. Repeat the same real-first-inline test with #467's actual API before acceptance. If no bounded, supported native mechanism can make these events causal, **stop and revise the approved architecture/acceptance with human review**; no marker-only or eventually-consistent substitute.

### 4.4 Preload source authority and retained snapshot

Draw.io's main role computes an **absolute** `webPreferences.preload` path. Identify the verified **package root** (distinct from `file://` page document root); map the absolute source through the admitted package/stage identity into a package-relative path. The mapping must work for unmodified draw.io and preserve enough provenance to detect an unmapped absolute path. A synthetic ambient CWD mapping, string-prefix check, or canonical path alone is not authority.

**Proposed Rust signatures (illustrative only):**

```rust
struct RetainedPackageRoot; // retained verified root fd, identity and lifetime
struct WindowPreloadSnapshot; // bounded owned bytes and display path
fn prepare_preload(
    root: &RetainedPackageRoot,
    app_absolute_preload: &Path,
    host_total_bytes: usize,
) -> Result<WindowPreloadSnapshot, PreloadSourceError>;
```

Resolve every component through retained directory descriptors; reject `..`, source escaping package root, missing paths, symlink escape, special files and stale-stage mappings. An **in-root symlink** can be allowed only when the exact resolved target is proved under the retained root by descriptor identity and race-safe traversal; if proof is unavailable, fail closed and record missing compatibility. Open **one** regular-file descriptor and read no more than cap+1 bytes **from that same descriptor**, with fixed bounds and retained lifetime. No reopen by ambient display path between validation and read; avoid blocking the UI thread on FIFO or stalled disk.

**Proposed initial limits for approval:** 512 KiB per window, 8 MiB combined per host; reserve budget before source acceptance and release exactly once on view destruction. Oversized files and budget exhaustion are typed construction refusals. Each later qualified navigation uses the **same source snapshot** but gets new JS variables and listeners. Bun recovery never rereads that source and never reloads an otherwise surviving page.

A held fd closes the *path-substitution* race but does **not** make an in-place concurrent write into one atomic byte image. Descriptor metadata rechecks are detection only; do not claim package authenticity, signer verification or coherent read under hostile concurrent writes until a verified immutable-image or independent integrity primitive is approved. Record this open security decision explicitly.

### 4.5 Native admission and lifecycle

The new `WKScriptMessage` handler verifies exact per-view content controller, app-world identity, `frameInfo.isMainFrame`, expected live WKWebView and **#542's implemented** checked document before decoding a message or causing an app effect. The handler has its own world/name. **Current `keld.wv-link/v1` accepts only `bind` and numeric-u16 `invoke`**, so named Electron send/event **cannot** be admitted by it. #540 is the sole selected **v2 spec** owner; #465 extends the **same** canonical decoder with named send and bounded credit, #464 adds host-to-renderer events, and #541/#552 own literal `el:<topic>` grant evaluation. #484 consumes those implementations and passes their v2 decoded messages through the one existing app link and guard (absence = deny; no wildcard). No separate parser, channel-ID reinterpretation, new principal minting, cross-world nonce handoff, public page bridge, KIPC registry or silent allow. A late response can retire its old request only once; it cannot settle a successor document's request of the same local number. No renderer-side automatic retry/replay.

An app preload has ordinary DOM APIs and can intentionally call public `window.postMessage`; that act is not intrinsically a native isolation breach. The stronger **native app transport** rule is that **none** of its handler traffic is carried via a page-observable DOM `postMessage` relay. Existing KEL-142 page facade already has a separately owned narrow DOM relay; do not silently remove it.

### 4.6 One diagnostic owner

#484 records per-call **handler-local** cost from callback entry through validation/handoff, with a declared monotonic clock, case census (allowed/refused/malformed), statistic (count/median/p95) and source commit. The **single** metric ID lives in the existing `gyldlab/keld-benches` KEL-90 metric registry (candidate `macos.app_preload.handler_callback_ns`, uniqueness/naming awaiting registry-owner check), with class `diagnostic`, no threshold and no publishable acceptance score. X05-T1 is parked and may not duplicate this ID. This excludes Bun latency, queueing, file I/O and renderer JS exposure; do not infer comparative performance from the sample.

## 5. Boundaries

**This draft PR changes only** `docs/specs/gh484-macos-app-preload.md`. No implementation or registry mutation.

After human approval, #484 may propose macOS world/handler changes in `crates/keld-wv/src/wkwebview/*` with `keld-wv/AGENTS.md` sanctioned unsafe coverage, and a retained app package-source owner in the appropriate host/engine seam. The exact source owner must be coordinated with package boot. #567 owns conformance baseline edits; #467 owns JS facade and IPC. `gyldlab/keld-benches` owns the one metric registry ID separately.

Do not edit #467's facade, #541/#542/#552 grants/generation/wire contracts, #473 page resource loading, unrelated core session files or a shared architecture document under another claim. Architecture 05 §2 is generic and does not explicitly prohibit a third app realm; propose a reviewed cross-reference only if its owner agrees. KEL-142 original contract remains unchanged until the separately approved additive amendment is landed.

## 6. Tasks (each ≈ one PR; ordered; no placeholders)

- [ ] **F0 pre-approval feasibility (test fixture only):** run a disposable native WKWebView oracle with pinned unmodified draw.io preload and a clearly labelled test-only Electron exposure shim (because #467 implementation is separately owned). Prove synchronously available complete API *shape* and localStorage before the first inline script, plus independently failing delayed-exposure/world-swap controls. This is neither a product implementation nor the eventual #467 behavioral proof. Stop if feasibility fails; route the architecture decision back to #441/#484.
- [ ] **S0 specification approval:** obtain independent architecture/security review, resolve critical feasibility and root-authority decisions Q1–Q3 with evidence, and seek **human sign-off** on this exact contract. Set `Status: approved` only after that sign-off. All later production tasks require S0.
- [ ] **F1 package source:** implement retained admitted package-root handle, exact stage/absolute-path mapping, proven in-root symlink resolution, bounded one-descriptor snapshot and race/fault tests; resolve in-place coherence/authenticity scope.
- [ ] **F2 world/wrapper:** add one named app world/handler to the KEL-142 controller, exact nine-module wrapper, renderer process facts and main-frame document-start execution, preserving native eligibility.
- [ ] **F3 isolation/grant acceptance (after #540 approval and relevant #465/#464/#541/#542/#552 owners land):** consume the canonical v2 decode/credit and literal grant path; real Mac AC2 regression, hostile preload, two independent windows, navigation/teardown/recovery and per-window `el:` default-deny; independent RED fault for each guard.
- [ ] **F4 facade composition (owned by #467):** provide cross-world `contextBridge`/IPC value/callback semantics; prove unmodified draw.io two-request scenario; #567 flips eligible F07-T4 cells only after its own expected-fail tests.
- [ ] **F5 diagnostic:** register exactly one handler-cost metric under keld-benches' owner and capture diagnostic-only Mac samples. No threshold or performance claim.

These slices may split for review size without transferring authority between owners.

## 7. Test plan

**All tests below are planned and UNRUN at `Status: draft`.** A test must observe WebKit/host/Bun independently of any implementation's self-report. Execute each **one-fault temporary RED mutation** on a disposable branch/fixture, restore bytes exactly, then observe GREEN. Assertions wait on events, not sleeps; timeout is a kill switch.

| Cases / AC | Independent oracle | Individual RED mutation |
| --- | --- | --- |
| `f07.first_inline_complete` / 01–02 | First inline script captures four callable electron functions, process type/versions, `.configuration` seeded **before** page startup, on real WKWebView | Move actual API publication to microtask or `AtDocumentEnd`; first-inline assertion fails even with marker present. |
| `f07.world_and_k142_isolation` / 01,10 | Existing KEL-142 real Mac AC2 + separate hostile app preload: trusted handler/nonce unobservable and zero forged native effects | Install app script in trusted world or remove world check. |
| `f07.require_nine` / 03–04 | Exact nine alias probes and `require('fs')` exact error; unsupported members throw typed code | Add `fs` resolver. |
| `f07.js_exception` / 05 | Observe exact console marker **and independently completed page load** | Propagate JS throw into navigation failure. |
| `f07.process_env_argv_versions` / 06 | Canonical session-facts values, fixed additionalArguments, `Object.keys(process.env).length===0`, no `KELD_APP_LINK` | Clone Bun role env; separately hardcode `versions` then vary canonical input. |
| `f07.root_path_and_symlinks` / 07–08 | Native descriptor/inode identity and no out-of-root open; prove approved in-root symlink, reject outside/dangling/swapped alias | Reopen validated path, or skip one containment guard (separate tests). |
| `f07.source_kind_caps` / 08 | FIFO/device/dir fail without blocked UI; 512 KiB permitted, +1 denied; aggregate 8 MiB+1 denied | Skip file-kind check or cap (separate REDs). |
| `f07.inplace_concurrent_write` / 08 | Independent writer changes file **in place** during read; requires explicit detected refusal or documented immutable-source guarantee | Remove integrity/immutable-source check; if no such check yet, **acceptance blocked**, no fabricated pass. |
| `f07.snapshot_lifecycle` / 09 | Native snapshot hash stable across re-navigation, JS callback state resets; Bun crash leaves unsent typed page text unchanged | Reopen source on navigation or reload WebView on Bun restart. |
| `f07.frame_document_two_window` / 10–11 | Two real WKWebViews, wrong frame/view, stale navigation, canceled navigation and same-document change, view destruction; app handler effect count remains zero for denied cases and #542 stamp remains stable on same-document navigation | Remove checked #542 generation or rotate on `pushState`/hash change (separate runs). |
| `f07.grants_decoder` / 12 | Approved #540 v2 canonical decoder and implemented #465/#464 send/event credit, #541/#552 literal guard; no-grant/wrong-topic zero Bun effects; correct grant only bounded credited effect | Treat missing grant as allow, or let numeric v1 invoke count as named v2 send (separate runs). |
| `f07.native_dom_silence` / 13 | Page `message` listener sees no new *app handler transport* carrier, with permitted ordinary app DOM calls distinguished | Route app-native traffic through `window.postMessage`. |
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
| wire protocol | **YES (consume approved extension)** | `keld.wv-link/v1` is numeric-only. #540 owns v2 schema and carriage, #465/#464 the single canonical decode/send/event extension; #484 must use that approved implementation and independently prove no duplicate parser/channel authority. |

An independent reviewer must assess the **final** spec diff; future code needs fresh named independent architecture/security evidence, not approval borrowed from old KEL-142 work.

## 9. Perf impact

Unmeasured. Third-world script injection, byte snapshotting and one additional native handler could affect startup CPU/I/O, memory per window and handler cost. Do not infer savings. Candidate metric: `macos.app_preload.handler_callback_ns` in the KEL-90-owned benches registry, monotonic clock, exact accepted/denied census, count/median/p95 and OS/commit provenance; **diagnostic only**, no target, no performance pass/fail and no public comparison. The **proposed 512 KiB/window and 8 MiB/host bounds** are resource safety limits, not measured performance. A true architecture 01 §5 regression claim requires an attributed end-to-end baseline on physical macOS.

## 10. Open questions

**Status remains draft until humans resolve the following with evidence.**

1. **Q1 critical: first-inline publication.** Can a supported, bounded WebKit mechanism publish the **complete dynamic** `contextBridge.exposeInMainWorld` API across app/page worlds before first inline script despite async native bind? **Recommendation:** real Mac F0 oracle with unmodified preload and delayed-publication RED; if it fails, revise approved architecture/criteria before building, never accept eventual marker-only proof.
2. **Q2 critical: runtime absolute path to admitted package root.** How is draw.io's computed absolute preload resolved through a signed/admitted staged package path, distinct from page `file://` root? **Recommendation:** retain package-relative mapping bound to admitted root identity; refuse until verifiable end to end.
3. **Q3 critical: source coherence, root and symlink authority.** Who owns a retained package-root fd (boot currently drops it), demonstrably safe in-root symlinks, and hostile **in-place modification**? **Recommendation:** reuse descriptor-relative walker with one opened snapshot; explicitly withhold atomic/authenticity claims until separately verified primitive is chosen.
4. **Q4 proposed resource/snapshot policy.** Approve 512 KiB per preload/window, 8 MiB aggregate, new JS state per navigation with bytes captured once per window. **Recommendation:** approve if pinned draw.io file fits and cap controls go RED; no silent fallback/limit increase.
5. **Q5 error and diagnostic registry.** Confirm noncolliding source-admission code under WV, #467-owned unsupported facade code and one KEL-90 diagnostic metric ID with no publication. **Recommendation:** allocate codes only through owners and record registry uniqueness before F1.
6. **Q6 initial native bind and early calls.** Define #467's bounded deferred/rejected call behavior when API exists before #542 bind receipt. **Recommendation:** callable synchronous API with one bounded native-eligible pending operation or typed refusal; no JS nonce/grant or unbounded queue.
7. **Q7 architecture 05 / KEL-142 amendment owner.** Does architecture 05 §2 require a cross-reference to third app world? **Recommendation:** seek owner review; preserve existing KEL-142 approved two-world contract and land additive scope only with independent approval.
8. **Q8 final native acceptance.** Can all AC02/AC10/AC11/AC12 negative controls be demonstrated with real WKWebView and exact restored-source evidence? **Recommendation:** no pass or product label until each relevant Mac native cell has real proof.
9. **Q9 v2 dependency order.** #540 is still a spec, and #465/#464 and #541/#542/#552 are separate owners; when do their approved decoder, credit, event and grant contracts land? **Recommendation:** F2 can provide an isolated world/wrapper without enabling named sends; F3 and #467's complete draw.io round trip remain blocked until the **one** approved v2 decoder and literal grants are available. Never claim the current v1 numeric-only envelope is already a named Electron transport.

**Approval and non-claims:** Original KEL-142 Done does not grant this new app-world authority. This document has no test execution, source-integrity oracle, native full-API timing proof, approval, renderer implementation, shipped draw.io compatibility or merge authority.

# Spec: Windows WebView2 preload compatibility through a Host-enforced declared facade (GH-1184 W1, route D)

Status: draft
Linear: GH-1184 (GitHub tracker under #517; parent program #1182 W1; renderer epic #483) ·
Owner: @amishabenramani (real Windows operator) · Updated: 2026-10-29

Code citations are at `origin/main` `457a9c3b37f71179f167bd1f8c706c461cc4c479`. Device
evidence is one runtime and one OS build: WebView2 Evergreen `156.0.4314.50` on Windows 11
`10.0.26300`, `webview2-com(-sys) 0.38.2` (SDK 1.0.3650.58). Evidence runs live in
keld-research PR [0monish/keld-research#34](https://github.com/0monish/keld-research/pull/34):
note `library/host-platforms/264-webview2-preload-isolation-route.md`, runs
`gh1184-webview2-preload-isolation-2026-10-29` (P1–P3) and
`gh1184-route-d-host-rules-2026-10-29` (E5). The route decision and its framework-level
correction are recorded on
[#1184](https://github.com/gyldlab/keld/issues/1184#issuecomment-6500027152). This draft
authorizes no production code, no permission and no public API. Implementation starts only
after an exact-content `Status: approved` decision and the review gates in §8.

This is a **framework mechanism for every Electron application on Windows**. No rule below
names or special-cases one application. The pinned draw.io Desktop preload
(`jgraph/drawio-desktop@2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`,
`src/main/electron-preload.js`) appears only as qualification evidence that the common
`contextBridge` pass-through pattern keeps no authority in preload code.

## 1. Goal & non-goals

An Electron application migrated with `keld migrate` runs its `contextIsolation: true`
preload against Keld's `contextBridge`/`ipcRenderer` facade inside a Windows WebView2
window, with the facade present before the first inline page script, while **every**
renderer-to-Host call is admitted by the Host alone: top-level `WebMessageReceived`,
browser-stamped `Source` origin on the window's allowlist, the current per-document epoch
nonce, and an exact literal channel grant that `keld migrate` generated into
`keld.permissions.jsonc`. Preload-side navigation protections become a manifest-declared,
Host-enforced navigation policy. A preload that gates authority in its own JavaScript is
detected statically by `keld migrate` and the application is reported unsupported for this
route; nothing degrades silently. Default-deny is unchanged.

Observable outcome: on a real Windows device, a migrated application's page sees its
preload-published API at first inline, a granted call produces exactly one Bun handler
entry, and every forged, stale, framed, opaque-origin or ungranted call produces zero
Host/Bun effects with a typed refusal.

Non-goals:

- Any JavaScript-world separation on Windows. Route B (CDP `Page.createIsolatedWorld`
  behind a page `debugger;` pause) is named only as the documented escalation for gating
  preloads; it is **out of scope** here and has no approved spec.
- macOS/Linux preload worlds (#484/#1155, KEL-142 own macOS); `<webview>` tags; a second
  bridge or a second transport beside the existing `window.keld` floor.
- The preload source-integrity trust root (#484 AC07/AC08 own it on every OS).
- Natural late IPC delivery across a renderer swap (E5 observed none; stays UNKNOWN).
- Arbitrary dynamic `contextBridge` graph parity, `sendSync`, `MessagePort` transfer,
  multiple preloads per window, Electron `webContents` navigation events.
- Conformance score changes (#464/#567 own cells; nothing here flips a row).

## 2. Spec refs

Governing architecture: [03-security](../architecture/03-security.md) §1 (webview principal
= host WebView id + navigation generation; guard before handler), §2 (`windows.<w>.channels`
manifest block), §4.3 (navigation allow-lists, `channels: []` for remote content);
[04-electron-compat](../architecture/04-electron-compat.md) §1 (`keld migrate` generates
`keld.permissions.jsonc`), §3 item 3 (`contextBridge` "mapped to real isolated-world exposure
where the engine supports it, **documented emulation otherwise**" — route D is that
emulation); [05-webview-and-native](../architecture/05-webview-and-native.md) §1 (Windows is
direct COM) and §2 (`window.keld` bridge contract, injected pre-load on every webview).

Deviation this spec records: architecture 04 §3 says preload files "run in the webview before
page scripts, same as Electron". On Windows under route D the preload runs **in the page's
main world**; it is not a security boundary. The approving PR MUST add that sentence to
architecture 04 §3 item 3 and to the 05 §1 Windows paragraph; the honesty ledger (03 §6)
gains "Windows preload isolation is not provided; authority is Host-enforced per call".

| Existing owner (reuse, no duplicate) | Exactly what this spec consumes | Decision |
| --- | --- | --- |
| [#1191 GH-552 exact channel grants](https://github.com/gyldlab/keld/pull/1191) | AC1 opt-in `"$keldWindowChannelGrants": 1`; AC2 exact decoded `el:<c>` literal (1–256 UTF-8 suffix bytes, no wildcard/case-fold/normalization); AC3 Host-bound window selection; AC4 `RendererChannelQuery`/`RendererChannelPermit` disjoint from native permits; AC5 `KELD-GUARD003` `ChannelForbidden` with unchanged fix text; AC6 zero-allocation warmed Allow inside the one `dispatch_privileged`; AC8 identity refusal precedes grant miss. | **Reused unchanged.** The Windows adapter only supplies `Principal::Webview` and the borrowed window grant selection. This spec invents no matcher, no second evaluator and no grant store. |
| [#1192 GH-542 checked navigation binding](https://github.com/gyldlab/keld/pull/1192) | §4.2 `NativeDocumentPhase` transitions: one checked `u32` generation per WebView, `0` unbound, +1 per real cross-document commit and per trusted Host load, +0 for same-document history and cancelled provisional navigation, `u32::MAX` → Exhausted, destroy terminal; N16 same generation in the stamped request and `Principal::Webview`; N21 "Windows … UNKNOWN until separate real-device oracle". | **Reused as the state model; this spec is the Windows oracle N21 asks for.** §4.5 maps WebView2 events onto those transitions. Divergence: Windows has **one** realm, so #1192 §4.3's two per-realm credentials do not exist here; the per-document epoch nonce is the only document credential and it is page-readable (§4.2). |
| [#1154 GH-467 contextBridge/ipcRenderer facade](https://github.com/gyldlab/keld/pull/1154) | §4.5 `IpcRendererFacade`/`ContextBridgeFacade` shapes; §4.3 graph bounds (depth 16, 256 keys, 64 KiB strings, 64 callable slots, 64 callback slots, 512 KiB retained); §4.7 fail-closed early-call policy (void return, zero queued native work, one private typed diagnostic); C09 emitter semantics. | **Reused as the one TypeScript facade; only the transport adapter is engine-specific.** Divergence: on Windows `exposeInMainWorld` is a same-realm define-and-freeze, so C02/C03 copy/proxy semantics do not apply (page and preload share object identity). Recorded as a scoreboard divergence under `keld-compat` rules, not hidden. |
| [#1190 GH-544 endpoint value codec](https://github.com/gyldlab/keld/pull/1190) | V1–V8: one TS codec, 4096-byte, depth/visit/argument bounds, synchronous refusal of functions/symbols before any send, Host-opaque bytes. | **Reused unchanged.** The Windows envelope carries the codec's bytes opaquely (§4.4); no Rust crate decodes them. |
| [#1155 GH-484 app preload runtime](https://github.com/gyldlab/keld/pull/1155) | §4.2 function-body wrapper (`require`, `process`, `exports`, `module`, `Buffer`, `global`, `setImmediate`, `clearImmediate`), admitted specifier table, `process` subset (`type`, `sandboxed`, `contextIsolated`, `versions`, `argv`, `env`); AC05 top-level throw logs `Unable to load preload script: <path>` and page load continues; AC07/AC08 source admission against an external trust root. | **Reused.** The wrapper and admission are OS-neutral; only the world it executes in differs. `process.contextIsolated` reports `true` for API compatibility while §4.2 documents that no isolation exists — flagged for owner decision in §10 Q3. |
| [KEL-142](kel142-macos-renderer-bridge-api.md) §4.1 | Public renderer floor `window.keld.invoke(channel: u16, payload: Uint8Array)`; frozen, non-writable, installed before page code; one pending invoke per WebView; `opts` unsupported. | **Reused as the Windows public surface.** §5 (two WK worlds, private nonce) is macOS-only; its page-forgeable-relay rule is adopted as the Windows design rule (the whole bridge is such a relay). |
| [GH-540 spec PR #1122](https://github.com/gyldlab/keld/pull/1122) `keld.wv-link` v2 | Bounded envelope kinds, DATA/CONTROL credits, terminal release. | **Consumed as the envelope grammar; carriage is WebMessage (§4.4).** Whether a WebMessage string carrier is an approved v2 carriage is #540's decision (§10 Q1). |
| `crates/keld-wv/src/webview2/mod.rs`, `guarded_navigation.rs` at `457a9c3b` | `create` ordering guard → devtools → bounds → navigate (`mod.rs:3521-3632`); `GuardInstalled` proof minted only by `install_guarded_media_permissions` (`guarded_navigation.rs:50-154`); deny-all `NewWindowRequested` (`:142-152`); `navigate_initial` requires the proof (`:161-172`); `NavigationStarting` observer reads only `NavigationId` (`mod.rs:2278-2297`); empty `AdditionalBrowserArguments` (`mod.rs:1432-1441`); devtools `cfg!(debug_assertions)` (`mod.rs:3583`). | **Extended, not duplicated.** New handlers register inside the same `GuardInstalled` mint so first navigation still requires them (§4.5). |
| `crates/keld-guard/src/lib.rs:136-146, 625-628` | `Principal::Webview { id, generation }`; `PermissionsManifest` currently ignores `windows`. | Consumed; the `windows` schema change is #1191/#541's. |

Microsoft documentation relied on (retrieved 2026-10-29): [`ICoreWebView2::AddScriptToExecuteOnDocumentCreated`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2#addscripttoexecuteondocumentcreated)
(runs in top-level and child frames; asynchronous registration),
[`ICoreWebView2::WebMessageReceived`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2#add_webmessagereceived)
(top-level document only; iframes use [`ICoreWebView2Frame2::WebMessageReceived`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2frame2#add_webmessagereceived)),
[`ICoreWebView2WebMessageReceivedEventArgs::Source`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2webmessagereceivedeventargs#get_source),
[`ICoreWebView2NavigationStartingEventArgs::Cancel`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2navigationstartingeventargs#put_cancel)
("Navigations to about schemes are cancellable, unless `msWebView2CancellableAboutNavigations`
feature flag is disabled. Cancellation of frame navigation to `srcdoc` is not supported"),
[`ICoreWebView2ContentLoadingEventArgs::IsErrorPage`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2contentloadingeventargs#get_iserrorpage),
[`ICoreWebView2::PostWebMessageAsString`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2#postwebmessageasstring),
[frames concept](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/frames),
[`ICoreWebView2Settings::AreHostObjectsAllowed`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2settings#put_arehostobjectsallowed).
All listed methods exist in the pinned `webview2-com-sys 0.38.2` bindings (`AddScriptToExecuteOnDocumentCreated`, `add_ContentLoading`, `add_WebMessageReceived`, `add_FrameNavigationStarting`, `PostWebMessageAsString`, `IsErrorPage`, `SetCancel`, `TryGetWebMessageAsString`, `SetAreHostObjectsAllowed`).

## 3. Acceptance criteria (binary, each becomes a test)

Every row has a single-fault negative control. "Zero effects" means independent counters on
the app-link writer, the Bun handler, and the Host navigation log all read 0.

| ID | Given / when | Pass observable | Negative control (one fault → RED) |
| --- | --- | --- | --- |
| AC01 First-inline bridge | A compat window is created and navigates to the application entry | The first inline page script observes `window.keld.invoke` and the preload-published API (frozen, non-writable, non-configurable) in the top-level document, and the `AddScriptToExecuteOnDocumentCreated` completion callback has fired **before** `Navigate` is called. | Call `Navigate` before the completion callback; the first-inline oracle must go RED at least once in 20 runs or the control is recorded inconclusive (P1 N3b did not discriminate; see §10 Q5). |
| AC02 Bridge is not a boundary | A hostile page poisons `JSON`, `Array.prototype`, `chrome.webview.postMessage` and redefines `window.electron` before calling an exposed method | The Host admits or refuses exactly as for an unpoisoned page; no refused call has a Host/Bun effect; the page gains **no** channel it lacked. The conformance report records the "preload runs in main world" divergence. | Remove the Host literal-grant check and keep the bridge intact: the poisoned page's ungranted channel reaches Bun → RED. |
| AC03 Top-level event only | A same-origin `srcdoc`/`about:blank` iframe and a same-process cross-origin iframe each call `chrome.webview.postMessage` with a well-formed envelope and the current epoch | Neither message arrives on `ICoreWebView2::WebMessageReceived`; frame messages arrive only on `ICoreWebView2Frame2::WebMessageReceived`, where the Host either rejects them (default) or routes them under an explicitly declared frame policy (none in this slice). Zero effects. | Register the admission handler on the frame event as well → frame call produces a Bun entry → RED. |
| AC04 Source origin | A message arrives from a document whose `Source` is `about:blank`, an opaque origin (error page, `data:`), or an origin not in the window's allowlist | Refused before epoch and grant checks with a typed `KELD-WV-011`-class refusal; zero effects. A control from the allowlisted origin is admitted. | Replace the origin check with a string-prefix match of the configured URL → an `about:blank#` or sibling-origin message is admitted → RED. |
| AC05 Epoch nonce | Document A sends after document B's `ContentLoading`; or any sender presents a nonce that is not byte-equal to the current one | Refused; zero effects. The epoch is posted to the new document at `ContentLoading` and the bridge queue drains only after it arrives; calls made before the epoch arrives are queued inside the bridge and never sent without it. | Post the epoch at `NavigationStarting` instead of `ContentLoading` → the epoch lands in the old document (E5: 24/24 wrong placement) and the new document's first call is refused → RED on the positive control. Separately, admit by `Source` only: a same-origin A→A2 reload replay is admitted → RED. |
| AC06 Literal grant | Current document, matching epoch, channel `c` without `el:c` in `windows.<w>.channels` | Exactly #1191 AC5: `KELD-GUARD003`, zero effects; a granted neighbor produces one Bun entry. Grants exist only because `keld migrate` wrote them; no `el:*`. | Prefix or case-fold match → `el:cX`/`EL:c` admitted → RED (reuse #1191 `G552-P04-literal`). |
| AC07 Navigation policy | The page triggers each top-level navigation trigger in E5 (`location`, `location.replace`, `navigation.navigate`, `window.open(…, '_self')`, link click, same-origin iframe `top.location`, popup `opener.location`, form GET, meta refresh) to `about:blank`, `about:blank#x`, `about:blank?x`, `ABOUT:BLANK` and to an off-allowlist `https:` origin | `NavigationStarting` fires, the Host sets `Cancel`, the application document survives (10/10 per trigger on the device) and the refusal is written to the on-deny audit log. Allowlisted navigations proceed. | Remove the case-insensitive scheme match → `ABOUT:BLANK` (Chromium rewrites to `about:blank#blocked`) replaces the document → RED. |
| AC08 Runtime flag untouched | Release and debug builds start the environment | `AdditionalBrowserArguments` stays empty; no `--disable-features` naming `msWebView2CancellableAboutNavigations` is ever passed; a source grep test and the environment-options unit test both assert it. | Add the disable flag → AC07's `about:blank` control replaces the document → RED. |
| AC09 Popups | The page calls `window.open` with `_blank`, a `target` link, or `opener`-less form | `NewWindowRequested` is handled (`SetHandled(TRUE)`, existing `guarded_navigation.rs:142-152`) or, when the window's policy declares it, a Host-owned window under the same policy is opened. No unaccounted popup exists. | Drop the handler → an unguarded popup appears outside principal accounting → RED. |
| AC10 `about:srcdoc` recovery | A top-level navigation to `about:srcdoc` is triggered | `Cancel` is set, the error page still commits (device: 10/10). At `ContentLoading` with `IsErrorPage == TRUE` and a non-allowlisted URL, the Host revokes the epoch **before** any message from the error page can be admitted and either re-navigates to the application entry or closes the window per the window's declared `onLoss` policy. A message from the error page is refused (opaque `Source`, AC04). | Skip the revocation on error page → a message carrying the stale nonce from the opaque document is admitted → RED. |
| AC11 Frames rule | A same-origin iframe calls `top.electron.<method>`; a cross-origin iframe tries the same | Same-origin: the call is indistinguishable from the top document and is admitted under the top document's grants (documented; Electron behaves the same). Cross-origin: `SecurityError`, zero effects. The window's CSP `frame-src`/`frame-ancestors` from `windows.<w>.web.csp` is the only control over framing, and the conformance report says so. | Claim frame isolation in the report while the same-origin iframe reaches Bun → the report/oracle mismatch check is RED. |
| AC12 Classifier gate | `keld migrate` runs over fixtures §4.8 F-A..F-F | F-A (allowlist on page input), F-D (payload sanitization before send), F-E (dynamic/indeterminate) are **flagged**; the report says "unsupported on Windows route D; escalation: route B (no approved spec)" and writes **no** channel grant for that window. F-B (page-chosen channel pass-through), F-C (fixed channel per method), F-F (inbound-only filtering) are **not** flagged and receive generated grants. | Remove the data-flow edge from exposed-function parameters to conditionals → F-A passes as pass-through → RED. Separately, treat any conditional as gating → F-C flagged → RED (false positive). |
| AC13 Generated grants | `keld migrate` sees `ipcMain.on('a')`, `ipcMain.handle('b')`, `ipcMain.on(dyn)` | `windows.<w>.channels` is exactly `["el:a", "el:b"]`, `"$keldWindowChannelGrants": 1` is written, `dyn` is reported as ungrantable (default deny), and the Host loads the file under #1191 AC1. | Emit `el:*` or infer channels from preload `send` strings → #1191 `G552-P01-manifest` rejects or the ungranted neighbor is admitted → RED. |
| AC14 Same-document stability | Hash change, `pushState`, `replaceState`, cancelled provisional navigation | Epoch nonce and generation unchanged; a pending granted call completes once. | Rotate on hash change → the pending call is refused → RED. |
| AC15 Hardening defaults | Compat window settings | `AreHostObjectsAllowed == FALSE`, `AreDevToolsEnabled == FALSE` in release (existing), `IsWebMessageEnabled == TRUE` only for windows that declare a channel list; a window with `channels: []` has `IsWebMessageEnabled == FALSE`. | Leave host objects enabled → a page-reachable host object exists → RED. |
| AC16 Receipt | A Windows acceptance claim is made | The receipt lists runtime `BrowserVersionString`, OS build, `457a9c3b`-relative implementation head, manifest digest, per-AC counters and each RED control's output. macOS/Linux/CI mocks cannot satisfy it. | A claim without the runtime string or with a mocked `WebMessageReceived` → receipt validator RED. |

## 4. Design

### 4.1 Atomic decomposition (decision input)

Security atoms are separated into identity, authentication, authorization, OS containment,
lifecycle/revocation and evidence provenance as root `AGENTS.md` requires.

| Atom | Owner / boundary | Inputs → outputs | Failure mode | Observable contract | Evidence |
| --- | --- | --- | --- | --- | --- |
| A1 Script registration | `keld-wv` Windows backend, before `navigate_initial` | bridge text + wrapped preload bytes → one `AddScriptToExecuteOnDocumentCreated` id, completion awaited on the STA by `wait_with_pump` | Script registered but first document races it; registration S_OK without effect | AC01: first-inline oracle after awaited completion | FACT: Microsoft documents asynchronous registration; P1 (E1): without `Page.enable` CDP scripts silently never ran — the WebView2 API path needs its own positive check (bridge `hello` handshake, §4.4). INFERRED: awaiting completion is sufficient on 156 (N3b did not falsify). |
| A2 Realm identity | WebView2; not controllable by Keld | document-created script → runs in the **main world of every frame** | Page poisons built-ins; frames get bridge text | AC02, AC03; `window === top` gate | FACT (E1/E2): script text present in same-process frames; cross-origin out-of-process frames get nothing. |
| A3 Event identity (frame) | WebView2 event routing | message from top document → `ICoreWebView2::WebMessageReceived`; from frame → `Frame2.WebMessageReceived` | Host registers the frame event by mistake | AC03 | FACT: Microsoft frames guide; E5 0 frame→top misrouting. |
| A4 Sender authentication (origin) | Browser-stamped `Source` | committed document URI → origin string | Opaque/`about:` Source; redirect to sibling origin | AC04 | FACT (S8 release notes 150/151: browser-authoritative origin stamped on the host pipe); FACT (E5): 0 mis-attribution under the Source rule in ~110k messages. UNKNOWN: Source behaviour under `document.open` and BFCache restore. |
| A5 Document authentication (epoch) | Host-minted 128-bit nonce, per committed document | `ContentLoading` → revoke old, mint, `PostWebMessageAsString` to new document; call carries nonce → byte-equal check | Posted to wrong document; same-origin A→A2 replay | AC05, AC14 | FACT (E5): post at `ContentLoading` reached the new document 24/24 per variant, 0 wrong-document; post at `NavigationStarting` reached the OLD document. FACT: cross-document ordering is **observed, not documented**. |
| A6 Authorization | `keld-guard` via `dispatch_privileged` (#1191) | `Principal::Webview`, borrowed window grants, exact `el:c` → Allow closure or `KELD-GUARD003` | Prefix/wildcard; second evaluator | AC06 | FACT: current `PermissionsManifest` ignores `windows` (`lib.rs:625-628`); the grant path is #1191's unmerged proposal — this spec cannot ship before it. |
| A7 Navigation authority | Host `NavigationStarting` handler | Uri → allowlist decision → `SetCancel(TRUE)` | `about:srcdoc` uncancellable; flag disabled | AC07, AC08, AC10 | FACT (E5, S9): 10/10 cancelled for every listed trigger; 10/10 `about:srcdoc` committed an error page despite Cancel. |
| A8 Popup authority | existing deny-all `NewWindowRequested` | request → handled | Unaccounted popup | AC09 | FACT: code at `guarded_navigation.rs:142-152`. |
| A9 OS containment | out of scope; WebView2 renderer sandbox and KEL-78 Bun profile are separate owners | — | Treating a passed guard as containment | none here | FACT: no change. |
| A10 Static classification | `keld migrate` (`keld-cli`, cold tooling) | preload AST → `pass-through` / `gating` / `indeterminate` + navigation-veto mapping | False negative admits a gating app | AC12, AC13 | FACT: `keld migrate` verb is reserved, not live (`crates/keld-cli/src/verb.rs:24,75`); the classifier is new work gated on KEL-17. |
| A11 Evidence provenance | this spec + keld-research#34 manifests | hashed per-run JSON → claims | Claiming a second runtime/OS | AC16 | FACT: one runtime, one OS build; every claim above is bounded to them. |

Independence: A5 does not depend on A4 (E5 tested each rule alone with 0 mis-attribution);
A6 is evaluated only after A3–A5 pass (#1191 AC8 ordering); A7 failures (srcdoc) are
contained by A4 (opaque Source). A1's silent-failure mode is promoted into its own
handshake check rather than inferred from A5.

### 4.2 Trust model: two rules instead of three worlds

Under Electron, `contextIsolation` makes the preload a boundary between page and Host.
WebView2 offers no supported isolated world (note 264 S1/S7: Microsoft, "we don't currently
have a way to execute scripts in a context other than the current page"). Route D therefore
replaces the three-way page/preload/Host criterion with two framework rules:

1. **No authority exists only in preload code.** Anything the preload may do, the page may
   do; the Host grants exactly the literal channels `keld migrate` generated, and nothing the
   bridge or preload holds can widen that. The epoch nonce is page-readable by construction;
   it authenticates the **document**, not the sender, and conveys no authority the document's
   origin does not already have.
2. **The Host authenticates frame, origin and document epoch, not the JavaScript world.**
   Admission = top-level event ∧ allowlisted `Source` origin ∧ current nonce ∧ literal grant.

Consequences recorded as divergences (scoreboard, `keld-compat` rules): `contextBridge`
copy/proxy semantics are not reproduced (same realm); `process.argv`
(`additionalArguments`) and the preload text are page-visible, so applications MUST NOT
place secrets in them (`keld migrate` warns on high-entropy arguments); a page can replace
`window.electron` for itself (self-harm only).

### 4.3 The bridge script

One function-wrapped script, no `eval`/`new Function`, registered once per WebView before
the first navigation. Sketch (TypeScript source of the shared `@keld/electron` renderer
facade, built once for all engines; only the transport adapter below is Windows-specific):

```ts
// Captured at document creation, before any page script can poison them.
const post = chrome.webview.postMessage.bind(chrome.webview);
const listen = chrome.webview.addEventListener.bind(chrome.webview);
const stringify = JSON.stringify, parse = JSON.parse, freeze = Object.freeze;

if (window !== window.top) return;              // frames get nothing (AC03/AC11)

let epoch: string | null = null;                // arrives from the Host at ContentLoading
const queue: string[] = [];                     // bounded by the #1154 §4.3 limits
listen("message", (e) => {
  const m = e.data;                             // string; Host posts only strings
  if (typeof m !== "string") return;
  const env = parse(m);
  if (env.v === 1 && env.kind === "epoch") { epoch = env.nonce; flush(); return; }
  deliver(env);                                 // reply / event → #1154 emitter
});
function sendEnvelope(body: object): void {
  const s = stringify({ v: 1, document: epoch, ...body });
  if (epoch === null) { if (queue.length >= 64) refuseLocal(); else queue.push(s); return; }
  post(s);
}
installKeldFloor(sendEnvelope);                 // KEL-142 §4.1 window.keld.invoke
installElectronFacade(sendEnvelope);            // #1154 ipcRenderer/contextBridge shapes
runWrappedPreload();                            // #1155 §4.2 wrapper; may throw → log, continue
```

Rules: the script text contains no secret (it is readable by the page and present in frames);
the page can synthesize a fake `message` event on `chrome.webview` and corrupt `epoch` — that
only breaks the page's own calls (the Host refuses the wrong nonce), so it is not an authority
path. `exposeInMainWorld(key, api)` defines `key` on `window` as a non-writable,
non-configurable, recursively frozen property (first definition wins; a second call for the
same key throws the #1154 typed error). The bridge sends a `hello` envelope immediately after
installation; the Host uses it as the positive registration check for A1 (§4.4).

### 4.4 Host admission pipeline

Carrier: `WebMessageReceived` strings (`TryGetWebMessageAsString`; JSON messages are rejected
to keep one parser). Envelope grammar reuses #540's `keld.wv-link` v2 kinds with
`document` = epoch nonce; payload is #1190 codec bytes encoded base64url (≤ 4096 decoded
bytes). Whether this carriage is an approved v2 carriage is §10 Q1.

```rust
// keld-wv/webview2, conceptual; runs on the STA inside the WebMessageReceived callback.
fn on_web_message(view: &ViewState, args: &ICoreWebView2WebMessageReceivedEventArgs) {
    // 1. top-level event only — this handler is registered on ICoreWebView2, never on Frame2.
    // 2. origin: browser-stamped Source, parsed; opaque / about: / off-allowlist → refuse.
    let origin = parse_origin(args.Source()?)?;              // Err → KELD-WV-011 class
    if !view.policy.origins.contains(&origin) { return refuse(Refusal::Origin); }
    // 3. epoch: byte-equal to the current document's nonce; same generation as the stamp.
    let env = Envelope::parse_bounded(args.TryGetWebMessageAsString()?)?; // 4 KiB + header
    if !view.epoch.matches(&env.document) { return refuse(Refusal::Epoch); }
    // 4. grant + forward: the ONE evaluator, Principal from the SAME generation (N16).
    let principal = Principal::Webview { id: view.id.0, generation: view.epoch.generation.get() };
    dispatch_privileged(&view.manifest, principal, RendererChannelQuery {
        channel: &env.channel,                 // adapter prefixes "el:" exactly once (#1191)
        current_window: view.window_grants.borrow(),
        on_allow: |_permit| view.app_link.forward_bounded(env.kind, env.payload),
    })?;                                        // Deny → KELD-GUARD003, zero writes
}
```

Order is fixed: identity refusals (origin, epoch) precede the grant miss (#1191 AC8). The
`hello` kind is admitted through steps 1–3 only and records "bridge registered for
generation g"; a window whose first committed application document produces no `hello`
within the existing initial-navigation deadline (`KELD-WV-005`) fails with a typed error
(code to be allocated by the error-registry owner; not `KELD-WV-012/013`, which #540 owns).
There is no fallback bridge and no silent degradation.

Host→renderer delivery (`mainResp`, `webContents.send`) uses `PostWebMessageAsString` to the
**original** document only: the Host stamps the target generation at send admission (#1192
N15, #543 owner) and drops the message if the current generation differs.

### 4.5 Per-document epoch lifecycle (WebView2 event mapping onto #1192 §4.2)

| WebView2 event | #1192 transition | Host action |
| --- | --- | --- |
| Controller created, scripts registered, completion awaited | Unbound(0) | No nonce; `WebMessageReceived` admits nothing but refuses with zero effects |
| `NavigationStarting` (top-level) | +0 | Policy decision only (§4.6). **Never** post the epoch here (E5: reaches the old document) |
| `ContentLoading`, `IsErrorPage == FALSE`, URL on allowlist | +1 (checked `u32`; `MAX` → Exhausted) | Revoke old nonce and all old pending deliveries; mint 128-bit nonce from the OS CSPRNG; `PostWebMessageAsString(epoch)`; start `hello` deadline |
| `ContentLoading`, `IsErrorPage == TRUE` or URL off allowlist (e.g. the `about:srcdoc` error page) | +1 then FailedClosed | Revoke; post **no** epoch; apply the window's declared `onLoss` (`reload-entry` default, or `close`) |
| `HistoryChanged`, `SourceChanged` with `IsNewDocument == FALSE`, hash/`pushState` | +0 | Keep nonce, generation and pending work (AC14) |
| Provisional navigation cancelled / `NavigationCompleted` failed without a new `ContentLoading` | +0 | Keep current document |
| Trusted Host full-document load (`reload-entry`) | +1 before the engine call | Revoke first, then `Navigate`; the next allowlisted `ContentLoading` advances again |
| Bun role restart while the WebView survives | +0 | Keep document and nonce; revoke old app-link correlations (#1192 N15) |
| Controller/window destroyed | terminal | Revoke; no reuse of the numeric id (#1191 AC3) |

Cross-document ordering ("no message from A after B's `ContentLoading`") is **observed** in
E5 (0 of ~110k) and not documented by Microsoft; the nonce check, not that ordering, is the
rule that refuses a late message.

### 4.6 Framework navigation policy (manifest-declared, Host-enforced)

Declared per window in `keld.permissions.jsonc`, beside the existing `web.csp` key (schema
name subject to the #541/#1191 manifest owner, §10 Q2):

```jsonc
{
  "$keldWindowChannelGrants": 1,
  "windows": {
    "main": {
      "channels": ["el:rendererReq"],                // generated by keld migrate (AC13)
      "web": {
        "csp": "default",
        "navigation": {
          "allow": ["file://<package-root>/", "https://app.example/"],  // exact origin or prefix
          "popups": "deny",                           // or "host-window" (same policy)
          "onLoss": "reload-entry"                    // or "close"
        }
      }
    }
  }
}
```

Enforcement in the top-level `NavigationStarting` handler (registered inside the
`GuardInstalled` mint so `navigate_initial` cannot run without it):

- Parse `Uri`. Lower-case the scheme before matching. Cancel when the scheme is `about`
  (covers `about:blank`, `about:blank#blocked`, `about:blank?x`, fragment forms,
  `ABOUT:BLANK`), `data`, `javascript`, `blob` outside the allowlist, or when the origin/prefix
  is not in `allow`. An unparsable `Uri` is cancelled. `IsUserInitiated` does not relax the
  decision (E5: identical with and without activation).
- `FrameNavigationStarting` is not cancelled by this policy (frames are governed by CSP,
  §4.7); a frame policy is a separate future owner.
- `AdditionalBrowserArguments` stays empty (`mod.rs:1432-1441`); no build may disable
  `msWebView2CancellableAboutNavigations` (AC08).
- `NewWindowRequested`: `SetHandled(TRUE)` (existing) when `popups: "deny"`; `"host-window"`
  opens a Host-owned window with the same `windows.<w>` policy and its own epoch.
- Known limit: top-level `about:srcdoc` commits an error page despite `Cancel` (E5 10/10;
  Microsoft documents non-cancellation only for frame srcdoc). Authority impact is nil (AC04
  rejects the opaque Source). Recovery is §4.5's `onLoss` row.

A preload's own veto (for example a `navigation` listener that calls `preventDefault()` on
`about:` destinations) is recognised by `keld migrate` and mapped to this policy; the
preload code may still run but is not relied on.

### 4.7 Frames rule

Same-origin iframes share the top document's realm reachability: `top.electron.*` and
`top.keld.invoke` are callable from them and arrive on the top-level event with the top
document's `Source` and nonce. They are **indistinguishable from the application** and are
admitted under its grants; this matches Electron and is stated in the conformance report
(AC11). Cross-origin frames cannot reach the top realm (`SecurityError`, E2) and their own
messages route to `Frame2.WebMessageReceived`, which this slice does not register. Framing
is therefore governed only by the window's CSP (`frame-src`, `frame-ancestors`) from
`windows.<w>.web.csp`; a window that needs stricter control declares it there.

### 4.8 `keld migrate` classifier

Cold tooling (`keld-cli`); no runtime decision depends on it except that a flagged window
receives **no** grants. Input: each `webPreferences.preload` path statically resolvable from
`new BrowserWindow(...)` sites, read as an ES/CommonJS AST (Bun's parser, no new dependency).

Definitions:

- **Exposure**: a function value reachable from `contextBridge.exposeInMainWorld(key, api)`
  or from an assignment to `window.<key>`/`globalThis.<key>`.
- **Sink**: `ipcRenderer.send/invoke/sendSync/postMessage/sendTo/sendToHost`.
- **Page input**: parameters of an exposure function and anything derived from them.
- **Gating**: a sink reachable from an exposure where, on some path, (a) a branch, `switch`,
  comparison, membership test (`Set.has`, `in`, array `includes`/`indexOf`), regex test, or
  `throw` **depends on page input**, or (b) a page-input value is transformed by a
  normalising call (`path.*`, `URL`, `decodeURI*`, `replace`, `trim`, `toLowerCase`) before it
  reaches the sink's channel or payload. These are authority decisions the Host cannot
  reproduce from a literal grant list.
- **Pass-through**: every sink reachable from exposures either has a literal channel or takes
  the channel/payload from page input without a page-input-dependent branch or transform.
- **Indeterminate**: dynamic `require`, `eval`, `new Function`, computed member access on
  `ipcRenderer`, or an unresolvable preload path. Treated as gating (fail closed).
- Inbound filtering (`ipcRenderer.on` listeners that drop or reshape Host→renderer events) is
  **not** gating: it reduces what the page sees and grants nothing.
- **Navigation veto detection**: a `navigation`/`beforeunload` listener whose handler calls
  `preventDefault()` under a scheme test maps to `web.navigation.allow` (the tested scheme is
  excluded); any other pattern is reported as "preload-side protection not reproducible" and
  flags the window.

Report lines (framework wording, no application names):
`✘ preload gates authority in JavaScript (<file>:<line>) — unsupported on Windows route D;
escalation: route B (no approved spec)`; `✔ preload is pass-through; generated
windows.<w>.channels from N ipcMain literals; M dynamic handler names left ungranted`.

Falsifier fixtures (checked into `keld-cli` tests; each is a minimal preload):

| Fixture | Content | Verdict |
| --- | --- | --- |
| F-A | `request: (msg) => { if (ALLOWED.has(msg.action)) ipcRenderer.send('req', msg); }` | gating |
| F-B | `sendMessage: (action, args) => ipcRenderer.send(action, args)` (page-chosen channel) | pass-through |
| F-C | `openFile: () => ipcRenderer.invoke('open-file')` (fixed channel per method) | pass-through |
| F-D | `save: (p, d) => ipcRenderer.send('save', path.normalize(p), d)` | gating |
| F-E | `const m = require(name); m.send(...)` or `eval` | indeterminate → flagged |
| F-F | `ipcRenderer.on('evt', (e, v) => { if (v.ok) cb(v); })` only | pass-through |

The pinned draw.io preload is F-B plus F-F plus a navigation veto on `/^about:/i`
(evidence: `electron-preload.js` lines 46–55, 87–123 at the pinned blob); the classifier
must return pass-through with an `about` veto mapping for it. Before the gate is relied on
for release it is run across the compat corpus (#496, #587) and its false-positive and
false-negative counts are recorded.

### 4.9 Rejected alternatives (recorded, not re-litigated)

| Route | Why rejected |
| --- | --- |
| A. CDP `Page.addScriptToEvaluateOnNewDocument` with a named `worldName` world + `Runtime.addBinding` | World is created with universal access: cross-origin XHR upload events fired in every such world and 0 times in the main world or a grant-free world (P3, E3) — more authority than an Electron preload; depends on CDP that Microsoft does not maintain (S2); `worldName` is experimental (S3). |
| B. `Page.createIsolatedWorld` with `grantUniveralAccess: false` behind a page `debugger;` pause + `Debugger.enable` | Works 36/36 (P3-A) but keeps the Debugger domain on for the page's life, pauses the renderer on the Host UI thread at every document start and every page `debugger;`, has two silent failures (`setSkipAllPauses`; a later same-name `worldName` script re-grants access), and relies on undocumented command handling during a pause. Kept **only** as the named escalation for gating preloads; no spec. |
| C. MV3 extension content script | Persists in the profile, no extension↔host channel without CDP (WebView2Feedback #3851), extension-origin authority (S6). |
| E. Real preload in the main world as the boundary | The page poisons the preload's built-ins and removes its veto; no Host rule compensates. |
| Status quo | No preload, no `ipcRenderer`, no Windows compatibility for `contextIsolation` applications; `keld migrate` cannot report support. |

### 4.10 Capabilities, wire, platform, seam, migration

- **Capabilities / manifest:** new `windows.<w>.web.navigation` block (§4.6); channel grants
  per #1191 AC1/AC2 (opt-in marker). No new capability names. Permission-model gate.
- **Wire/protocol:** no KIPC change. A new private Host↔bridge carriage (WebMessage string
  carrying `keld.wv-link` v2 envelopes with base64url codec bytes). Wire-protocol gate;
  #540 owner decision on carriage (§10 Q1).
- **Public API:** `window.keld.invoke` unchanged (KEL-142); `@keld/electron` renderer facade
  shapes from #1154 become reachable on Windows; `keld migrate` gains report lines and the
  classifier. Public-API gate.
- **Platform notes:** Windows only. macOS keeps #1155's three worlds and KEL-142's private
  nonce; Linux WebKitGTK is UNKNOWN. The same facade TypeScript builds for all; the
  transport adapter is per engine.
- **Runtime seam:** before — `create` registers media guard + popup denial, then navigates;
  after — `create` additionally registers `NavigationStarting` policy, `ContentLoading`
  epoch, `WebMessageReceived` admission and the document-created script **inside the
  `GuardInstalled` mint**, awaits script registration, then navigates. Execution owner stays
  the tao STA thread; crash domain unchanged (renderer crash ≠ Host); handle lifetime: COM
  handlers live for the WebView lifetime; config capture: manifest and navigation policy are
  snapshotted once at session creation (#1191 AC1) and never re-read.
- **Migration unit:** callers — `WebView2Engine::create`; new `WebviewSpec` fields
  (window key, navigation policy, admitted preload bytes) are a `WebEngine` trait change
  (crate rule: design review; a live backend implements it in the same PR). No temporary
  adapter; no persisted state beyond the manifest.
- **Compatibility fallback:** not required — there is no earlier Windows preload path.

## 5. Boundaries

- Implement in: `crates/keld-wv/src/webview2/mod.rs` and `guarded_navigation.rs` (new
  sibling module for epoch/admission is acceptable; one owner for the `GuardInstalled`
  mint); `crates/keld-wv/src/engine.rs` (`WebviewSpec` extension, trait review);
  `crates/keld-core/src/app_session.rs` (narrow forwarder that calls `dispatch_privileged`,
  shared with #1191 T4 — no second forwarder); `packages/@keld/electron` renderer facade +
  Windows transport adapter; `crates/keld-cli` (`migrate` classifier, report, generator);
  `docs/architecture/03,04,05` deviation sentences; `keld-compat` scoreboard divergence
  entries.
- Must not touch: `crates/keld-guard` policy grammar or evaluator (#1191/#541 own it);
  `crates/keld-ipc` wire framing; the KIPC channel table (#508); macOS `wkwebview/*`;
  workspace `Cargo.toml` dependencies (none needed; `webview2-com 0.38.2` already exposes
  every method used); `keld.config.ts` schema beyond what `keld migrate` already generates.

## 6. Tasks (each ≈ one PR; ordered)

- [ ] T0 — this draft; owner approval of exact content; §10 answers recorded on #1184.
- [ ] T1 — Navigation policy and popup handling in `keld-wv` (§4.6, AC07–AC10, AC08 source
  test, AC15 settings), manifest key parsed by the #1191 opt-in parser once it lands; Windows
  device tests with the E5 trigger matrix as the oracle. No bridge yet.
- [ ] T2 — Epoch lifecycle and `WebMessageReceived` admission steps 1–3 with the `hello`
  handshake and the first `AddScriptToExecuteOnDocumentCreated` registration of a
  **floor-only** bridge (`window.keld.invoke`); AC01, AC03–AC05, AC14, AC16 receipt.
- [ ] T3 — Grant and forward: the narrow `keld-core` forwarder calling `dispatch_privileged`
  with `RendererChannelQuery` (after #1191 T1/T2 merge), Host→renderer original-document
  delivery; AC02, AC06.
- [ ] T4 — `@keld/electron` renderer facade on the Windows transport: `ipcRenderer` subset,
  `exposeInMainWorld` define-and-freeze, #1155 wrapper execution, divergence entries;
  AC11 report wording.
- [ ] T5 — `keld migrate` classifier, grant generator and report (§4.8, AC12, AC13) with
  fixtures F-A..F-F; corpus run recorded.
- [ ] T6 — End-to-end Windows qualification of one corpus application with the receipt
  (AC16) and the full RED-control matrix; architecture deviation sentences land here if not
  earlier.

## 7. Test plan

| Test | AC | Shape and anti-flake |
| --- | --- | --- |
| `wv2_bridge_first_inline` (Windows, real WebView2, ignored-by-default device test like `media_acceptance`) | AC01 | Fixture page's first inline script records `typeof window.keld.invoke` and the exposed keys into `document.title`; the Host reads the title after `NavigationCompleted`. 20 runs; mutant navigates before completion. Uses the existing dedicated UI-thread harness; no sleeps, deadline = existing `KELD-WV-005`. |
| `wv2_poisoned_page_no_widening` | AC02 | Fixture poisons built-ins then calls granted/ungranted/forged envelopes; counters on forwarder and a fake Bun listener. Mutant removes the grant step. |
| `wv2_frame_messages_rejected` | AC03, AC11 | Fixture with same-origin srcdoc, about:blank and same-site cross-origin child (loopback HTTP on an ephemeral port bound by the test); assert zero top-level events for frame posts and `SecurityError` for cross-origin `top.*`. |
| `wv2_source_origin_rules` | AC04 | Messages from an allowlisted page, an `about:blank` document, a `data:` document and the srcdoc error page. |
| `wv2_epoch_rotation` | AC05, AC14 | A→B cross-origin, A→A2 same-origin reload, hash/pushState; replay recorded A envelopes after B's `ContentLoading`; mutant posts the epoch at `NavigationStarting`; second mutant admits by Source only. |
| `guard_channel_literal` (reuse #1191 `G552-P04-literal`, `G552-P02-effect`) | AC06 | Pure Rust; no engine. |
| `wv2_navigation_policy_matrix` | AC07, AC10 | E5's trigger × destination matrix, 5 runs each, with and without user activation (`SendInput` click on a fixture button); app-document-survival oracle = fixture heartbeat message; `about:srcdoc` run asserts `IsErrorPage` + revocation + `onLoss`. |
| `wv2_environment_args_empty` | AC08 | Unit test on `create_environment_for_profile_with_options` plus a source grep test for `msWebView2CancellableAboutNavigations`. |
| `wv2_popup_handled` | AC09 | Existing KEL-168 test extended for `host-window`. |
| `migrate_classifier_fixtures` | AC12 | Pure Bun/TS or Rust unit tests on F-A..F-F plus the pinned preload bytes; two mutants (drop data-flow edge; flag any conditional). |
| `migrate_grant_generation` | AC13 | Golden manifest output; mutant emits wildcard. |
| `wv2_settings_hardening` | AC15 | Read back `AreHostObjectsAllowed`, `AreDevToolsEnabled`, `IsWebMessageEnabled` per window policy. |
| Receipt validator | AC16 | `tools/` check that the Windows acceptance comment carries runtime string, OS build, head, manifest digest and per-control output. |

Anti-flake: every device test is gated behind the existing Windows feature flag and runs on
the dedicated STA harness; loopback servers bind port 0; no wall-clock sleeps — completion
is observed through WebView2 callbacks pumped by `wait_with_pump`; each RED control runs in
its own process so a mutant cannot leak state.

## 8. Review gates triggered

- **Permission model — mandatory, independent reviewer named in `.agents/coordination.md`:**
  per-call admission order, navigation policy semantics, generated grants, no second
  evaluator, no wildcard, `channels: []` → `IsWebMessageEnabled == FALSE`.
- **Public API — mandatory:** `WebEngine`/`WebviewSpec` extension; `@keld/electron` renderer
  facade on Windows; `keld migrate` report and manifest keys.
- **Wire protocol — #540 owner:** WebMessage carriage of `keld.wv-link` v2; base64url codec
  bytes; `hello`/`epoch` kinds.
- **unsafe:** new COM handler registrations in `keld-wv/webview2` (`add_NavigationStarting`
  with `SetCancel`, `add_ContentLoading`, `add_WebMessageReceived`, `PostWebMessageAsString`,
  `AddScriptToExecuteOnDocumentCreated` with awaited completion, `SetAreHostObjectsAllowed`)
  — each needs a local `// SAFETY:` proof and an issue-scoped update to
  `crates/keld-wv/AGENTS.md`'s Windows FFI list; independent unsafe gate on the exact diff.
- **Dependency addition:** none.

## 9. Perf impact

- Architecture 01 §5 first-paint budget may move: awaiting `AddScriptToExecuteOnDocumentCreated`
  before the first `Navigate` was measured at 96–109 ms in wry's **blocking** form
  (`docs/agents/learnings.md`, KEL-65). The asynchronous completion-handler form's cost on the
  critical path is UNKNOWN and MUST be measured with the KEL-64 double-rAF beacon before T2
  merges; >5 % regression needs the written waiver.
- Per-message Host cost: envelope parse (bounded, ≤ 4 KiB + header) allocates; the grant
  decision is zero-allocation on Allow (#1191 AC6, counting allocator). Report parse, decide
  and forward as separate attributed samples; no claim before measurement.
- `ContentLoading` work (nonce mint + one `PostWebMessageAsString`) is per navigation, not
  per message.

## 10. Open questions (each UNKNOWN names its deciding probe)

1. **Q1 (wire, #540 owner):** is a WebMessage string carrying `keld.wv-link` v2 envelopes an
   approved v2 carriage, and may `document` carry the Windows epoch nonce? Deciding action:
   #540 owner comment on #1184 before T2.
2. **Q2 (manifest schema, #541/#1191 owner):** key name and shape of
   `windows.<w>.web.navigation` (`allow`, `popups`, `onLoss`) and whether `allow` entries are
   exact origins only or prefixes. Deciding action: owner decision on #541 before T1.
3. **Q3 (compat semantics, #1154/#1155 owners):** should `process.contextIsolated` report
   `true` on Windows (API compatibility) when §4.2 provides no isolation, or `false` (truth)?
   Recommendation: `true` plus the documented divergence, because applications branch on it
   to pick the `contextBridge` path. Deciding action: owner choice recorded in the scoreboard
   entry.
4. **Q4 (issue acceptance, #1184 owner):** the issue body's criterion "prove three-way
   separation of page, app-preload and KELD private Host world" cannot be met by route D by
   construction; the owner must amend the issue's acceptance to the two rules in §4.2 before
   approval, or this spec is BLOCKED against its own tracker.
5. **Q5 (ordering proof) UNKNOWN:** whether awaiting the registration completion before
   `Navigate` is necessary on 156.0.4314.50 (P1 N3b did not falsify). Deciding probe: AC01's
   mutant, 20 runs, recorded in the T2 receipt; if it never fails, keep the await (cheap and
   documented) and record the control as inconclusive.
6. **Q6 (Source under edge navigations) UNKNOWN:** `Source` and `ContentLoading` behaviour
   under `document.open()`, BFCache restore and `location.reload()` of an error page.
   Deciding probe: extend the E5 harness with those three triggers before T2 merges.
7. **Q7 (natural late delivery) UNKNOWN:** a naturally delayed message from document A after
   B's `ContentLoading` was never observed (E5, 0 of ~110k). The nonce rule covers it by
   construction; no probe is required for approval, but T6's receipt must report the count.
8. **Q8 (main-world document-created scripts and page CSP) UNKNOWN:** whether a strict page
   CSP (`script-src 'none'`) blocks or alters the document-created bridge. Deciding probe: AC01
   fixture variant with that CSP header on the loopback server.
9. **Q9 (`keld migrate` availability):** the verb is reserved (`verb.rs:75`, KEL-17). T5
   depends on the KEL-17 owner accepting the classifier as the first live `migrate` slice or
   naming the interim command surface.

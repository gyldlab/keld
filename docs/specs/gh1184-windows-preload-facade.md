# Spec: Windows WebView2 preload compatibility through a Host-enforced declared facade (GH-1184 W1, route D)

Status: draft (revision r2 after isolated review of `69d0d9df`)
Linear: GH-1184 (GitHub tracker under #517; parent program #1182 W1; renderer epic #483) ·
Owner: @amishabenramani (real Windows operator) · Updated: 2026-10-29

Code citations are at `origin/main` `457a9c3b37f71179f167bd1f8c706c461cc4c479`. Device
evidence is one runtime and one OS build: WebView2 Evergreen `156.0.4314.50` on Windows 11
`10.0.26300`, `webview2-com(-sys) 0.38.2` (SDK 1.0.3650.58). Evidence runs live in
keld-research PR [0monish/keld-research#34](https://github.com/0monish/keld-research/pull/34):
note `library/host-platforms/264-webview2-preload-isolation-route.md`, runs
`gh1184-webview2-preload-isolation-2026-10-29` (P1–P3) and
`gh1184-route-d-host-rules-2026-10-29` (E5; `results.json`, per-trigger `p1-exec-*.json`,
probe host `src_main.rs`). E5 ran **only** `https://` virtual hosts mapped with
`DENY_CORS`. The `NavigateToString`/`file://` Source facts come from the owner's source
probe (`evidence/gh1184-source-probe/`, `SHA256SUMS`, `src/main.rs`, runs 1–3 identical,
same runtime and OS). The route decision and its
framework-level correction are recorded on
[#1184](https://github.com/gyldlab/keld/issues/1184#issuecomment-6500027152). This draft
authorizes no production code, no permission and no public API. Implementation starts only
after an exact-content `Status: approved` decision, the review gates in §8 and the blocker
list in §10 are cleared.

This is a **framework mechanism for every Electron application on Windows**. No rule below
names or special-cases one application. The pinned draw.io Desktop preload
(`jgraph/drawio-desktop@2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`,
`src/main/electron-preload.js`) appears only as qualification evidence, including where the
classifier in §4.8 flags it.

## 1. Goal & non-goals

An Electron application migrated with `keld migrate` runs its `contextIsolation: true`
preload against Keld's `contextBridge`/`ipcRenderer` facade inside a Windows WebView2
window, with the facade present before the first inline page script, while **every**
renderer-to-Host call is admitted by the Host alone: top-level `WebMessageReceived`,
browser-stamped `Source` equal to the qualified application document, the current
per-document epoch nonce, and an exact literal channel grant that `keld migrate` generated
into `keld.permissions.jsonc` for that window and that preload digest. Navigation is a
manifest-declared, Host-enforced policy separate from document eligibility. A preload that
gates authority in its own JavaScript is detected statically by `keld migrate` and the
window receives no grants; nothing degrades silently. Default-deny is unchanged.

Observable outcome: on a real Windows device, a migrated application's page sees its
preload-published API at first inline, a granted call produces exactly one Bun handler
entry, and every forged, stale, framed, ineligible-document or ungranted call produces zero
Host/Bun effects with a typed refusal.

Non-goals:

- Any JavaScript-world separation on Windows. Route B (CDP `Page.createIsolatedWorld`
  behind a page `debugger;` pause) is named only as the documented escalation for gating
  preloads; it is out of scope and has no approved spec.
- macOS/Linux preload worlds (#484/#1155, KEL-142 own macOS); `<webview>` tags; a second
  bridge or transport beside the `window.keld` floor; Host-owned popup windows.
- The Windows preload source-integrity trust root (see §10 B4: unowned).
- Natural late IPC delivery across a renderer swap (E5 observed none; UNKNOWN).
- Arbitrary dynamic `contextBridge` graph parity, `sendSync`, `MessagePort` transfer,
  multiple preloads per window, Electron `webContents` navigation events.
- Conformance score changes (#464/#567 own cells; nothing here flips a row).

## 2. Spec refs

Governing architecture: [03-security](../architecture/03-security.md) §1 (webview principal
= host WebView id + navigation generation; guard before handler), §2 (`windows.<w>.channels`
block; the **single vetted path/URL scope matcher**, 03 §2 "Path and URL scopes"), §4.3
(navigation allow-lists; remote-content windows get `channels: []`);
[04-electron-compat](../architecture/04-electron-compat.md) §1 (`keld migrate` generates
`keld.permissions.jsonc`), §3 item 3 (`contextBridge` "mapped to real isolated-world exposure
where the engine supports it, **documented emulation otherwise**" — route D is that
emulation); [05-webview-and-native](../architecture/05-webview-and-native.md) §1 (Windows is
direct COM) and §2 (`window.keld` bridge contract).

Deviations this spec records (all land in **T0**, the PR that approves this spec):

- Architecture 04 §3 says preload files "run in the webview before page scripts, same as
  Electron". On Windows under route D the preload runs **in the page's main world** and is
  not a security boundary. T0 adds that sentence to 04 §3 item 3 and 05 §1, and adds to the
  03 §6 honesty ledger: "Windows preload isolation is not provided; authority is
  Host-enforced per call; the page sees every Host→renderer post to its window".
- Architecture 03 §2's manifest example shows `"el:*"`; #1191 AC2 forbids wildcards. This
  spec follows #1191. Owner action: #541 (manifest owner) corrects the example; T0 links it.
- KEL-142 §5.1 forbids a page-callable relay that carries the document nonce. Route D's
  bridge **is** such a relay on Windows (§4.2). This is a **required KEL-142 amendment for
  Windows** needing KEL-142 owner sign-off before approval (§10 B1).

| Existing owner (reuse, no duplicate) | Exactly what this spec consumes | Decision |
| --- | --- | --- |
| [#1191 GH-552 exact channel grants](https://github.com/gyldlab/keld/pull/1191) (**open draft**) | AC1 opt-in `"$keldWindowChannelGrants": 1`; AC2 exact decoded `el:<c>` literal; AC3 Host-bound window selection; AC4 `RendererChannelQuery`/`RendererChannelPermit`; AC5 `KELD-GUARD003`; AC6 zero-allocation Allow inside the one `dispatch_privileged`; AC8 identity refusal precedes grant miss; T4 narrow `keld-core` forwarder. | Reused unchanged. Windows supplies `Principal::Webview` and the borrowed window grants; the forwarder is #1191 T4's, not a second one. |
| [#1192 GH-542 checked navigation binding](https://github.com/gyldlab/keld/pull/1192) (**open draft**) | §4.2 `NativeDocumentPhase` transitions and checked `u32` generation; §4.4 application-document eligibility (verified stage/package resource identity under #499/KEL-102, never URL text); N11 commit tied to an allowed attempt; N15 Bun restart keeps the document; N16 one generation in stamp and principal; N21 Windows UNKNOWN. | Reused as the state and eligibility model; this spec is the Windows oracle. Divergence: one realm on Windows, so §4.3's two realm credentials do not exist; the epoch nonce is page-readable and authenticates the document only. |
| [#1154 GH-467 facade](https://github.com/gyldlab/keld/pull/1154) (**open draft**) | §4.5 `IpcRendererFacade`/`ContextBridgeFacade`; §4.3 graph bounds (they are the only bounds; this spec adds none); §4.7 fail-closed early-call policy: void return, **zero queued work, no replay**, one private typed diagnostic; C09 emitter semantics. | Reused as the one TypeScript facade; only the transport adapter is engine-specific. Divergence: same-realm `exposeInMainWorld` (no copy/proxy). |
| [#1190 GH-544 codec](https://github.com/gyldlab/keld/pull/1190) (**open draft**) | V1–V8; 4096-byte bound; synchronous refusal of functions before send; Host-opaque bytes. | Reused unchanged; carried base64url inside the Windows envelope. |
| [#1155 GH-484 preload runtime](https://github.com/gyldlab/keld/pull/1155) (**open draft, macOS-only**) | §4.2 wrapper and `process` subset; AC05 throw handling; **AC08 SHA-256 source digest** verified outside the writable package tree. | Wrapper reused. The AC08 digest is reused as the key that binds classifier verdict and grants (§4.8). AC07/AC08's trust root is specified for macOS only; Windows has **no owner** (§10 B4). |
| [KEL-142](kel142-macos-renderer-bridge-api.md) §4.1 | `window.keld.invoke(channel: u16, payload)`; frozen; one pending invoke; routes exactly the declared Echo channel `1`. | Reused as the Windows public floor; admission in §4.4. §5.1 needs the amendment above. |
| [GH-540 spec PR #1122](https://github.com/gyldlab/keld/pull/1122) (**open draft**) `keld.wv-link` v2 | Envelope kinds, credits, terminal release. | Consumed as grammar; WebMessage carriage is new (§10 Q1). |
| `crates/keld-wv/src/webview2/mod.rs`, `guarded_navigation.rs` at `457a9c3b` | `create` ordering (`mod.rs:3521-3632`); `GuardInstalled` mint (`guarded_navigation.rs:50-154`); deny-all `NewWindowRequested` (`:142-152`); `navigate_initial` (`:161-172`); `NavigationStarting` observer (`mod.rs:2278-2297`); empty `AdditionalBrowserArguments` (`mod.rs:1432-1441`); devtools `cfg!(debug_assertions)` (`mod.rs:3583`); media principal with generation `0` (arch 03 §4.3). | Extended inside the same mint; one generation source feeds media and channel principals (§4.5). |
| `crates/keld-guard/src/lib.rs:136-146, 625-628, 784` | `Principal::Webview { id, generation }`; `PermissionsManifest` ignores `windows`; `evaluate` + the vetted scope matcher (`Scope::pattern`, `lib.rs:218`). | Consumed; `web.navigation.allow` entries use that matcher's grammar. |
| Today's app load path: `crates/keld-core/src/app_session.rs:3653` (`NavTarget::Html`) → `crates/keld-wv/src/webview2/mod.rs:3654` (`NavigateToString`) | A string-loaded document reports `Source = about:blank`, `location.origin = "null"` (source probe, 3/3). | **Must change for route D** (§4.6a): the application document MUST be loaded from a Host-identifiable URL. Which path — exact `file://` under the KEL-102-verified package root (W3 [#1185](https://github.com/gyldlab/keld/issues/1185) file-origin navigation/containment) or a Keld-owned virtual host — is decided by #1192/KEL-102 and #1185 (§10 B6). |

Microsoft documentation relied on (retrieved 2026-10-29):
[`AddScriptToExecuteOnDocumentCreated`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2#addscripttoexecuteondocumentcreated),
[`WebMessageReceived`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2#add_webmessagereceived) (top-level only; frames use [`ICoreWebView2Frame2`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2frame2#add_webmessagereceived)),
[`WebMessageReceivedEventArgs::Source`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2webmessagereceivedeventargs#get_source) ("the URI of the document that sent this web message"),
[`NavigationStartingEventArgs::Cancel`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2navigationstartingeventargs#put_cancel) (about schemes cancellable unless `msWebView2CancellableAboutNavigations` is disabled; frame srcdoc cancellation ignored),
[`IsRedirected`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2navigationstartingeventargs#get_isredirected),
[`ContentLoadingEventArgs::IsErrorPage`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2contentloadingeventargs#get_iserrorpage),
[`PostWebMessageAsString`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2#postwebmessageasstring),
[frames concept](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/frames).
All methods used exist in the pinned `webview2-com-sys 0.38.2` bindings.

## 3. Acceptance criteria (binary, each becomes a test)

Each row's negative control changes exactly one rule. "Zero effects" = independent counters on
the app-link writer, the Bun handler and the Host audit log read 0.

| ID | Given / when | Pass observable | Negative control (one fault → RED) |
| --- | --- | --- | --- |
| AC01 First-inline floor | A compat window navigates to the qualified application entry | The first inline page script observes `window.keld.invoke` and the preload's exposed keys, frozen, in the top document. | Install the facade from `DOMContentLoaded` via `ExecuteScript` instead of the document-created registration → first inline sees nothing → RED. (Whether awaiting the registration completion before `Navigate` is necessary is UNKNOWN, §10 Q5; it is not an AC.) |
| AC02 Bridge is not a boundary | A hostile page poisons `JSON`, `Array.prototype`, `chrome.webview.postMessage`, redefines the exposed key, then calls granted and ungranted channels | Host admits or refuses exactly as for a clean page; the page gains no channel it lacked. | Remove the Host literal-grant step, keep the bridge → ungranted channel reaches Bun → RED. |
| AC03 Top-level event only | A **same-origin, same-document-epoch** iframe (it reads the nonce from `top`) posts a well-formed envelope through its own `chrome.webview` | It arrives only on `ICoreWebView2Frame2::WebMessageReceived`, which this slice does not register; zero effects. A control posted via `top.keld.invoke` is admitted. | Register the admission handler on `Frame2` too → the iframe post reaches Bun → RED. |
| AC04 Eligibility before epoch | `ContentLoading` fires for (a) the qualified application document, (b) an allowlisted-navigation remote document, (c) an error page at an allowlisted URL (`IsErrorPage == TRUE`), (d) the `about:srcdoc` error page | Only (a) receives an epoch; (b)–(d) enter FailedClosed/`channels: []` with no epoch and every call from them refused. | Skip the `IsErrorPage` check → (c) receives an epoch → RED. Separately, qualify by URL-text equality instead of #1192 §4.4 resource identity → a spoofed same-text document receives an epoch → RED. |
| AC05 Source rule | A message arrives whose **full** `Source` URL (fragment stripped) differs from the qualified document's verified URL — including `about:srcdoc` (E5: the error page's `Source` **is** the string `about:srcdoc` and the bridge ran there, `p1-exec-b.json`, 5/5), `about:blank`, and another `file://` path under the same `file://` origin | Refused before epoch and grant checks; zero effects. | Compare by origin string only → a second `file://` document (every `file://` document shares that origin; source probe) or a sibling `https` path is admitted when it should not be → RED. |
| AC18 Host-identifiable load | The application is loaded with `NavigateToString` (today's path, `mod.rs:3654`) instead of the verified URL | `ContentLoading` reports `Source = about:blank` (source probe 3/3); the eligibility rule mints **no** epoch; the bridge's calls are refused not-ready forever and the window fails with the typed `hello`-deadline error. Loaded from the verified URL, the same application receives an epoch. | Qualify an `about:blank` document because the Host "knows" it just loaded the app string → an attacker-reached `about:blank` (E5 trigger a control) is indistinguishable and receives an epoch → RED. |
| AC06 Epoch nonce | Document A sends after B's `ContentLoading`; or a nonce not byte-equal to the current one | Refused; zero effects; the epoch is posted to the new document at `ContentLoading` (E5: 24/24 per variant). | Admit by `Source` + generation only → a same-origin A→A2 reload replay is admitted → RED. |
| AC07 Not-ready refusal | A page calls an exposed method or `window.keld.invoke` before the document's epoch arrived | Void APIs return `undefined`, `invoke` rejects with the typed not-ready error, **nothing is queued or replayed**; a call after the epoch succeeds. | Buffer and flush on epoch → the early call produces a late Bun entry → RED. |
| AC08 Renderer-side epoch check | The Host posts a reply/event stamped with epoch e₁ after document B (e₂) committed (host-replayed, as in E5 variant harness) | The bridge drops it (and drops anything received before its own epoch); B's listeners never fire. | Remove the bridge's stamp check → the stale post is delivered to B → RED. |
| AC09 Literal grant | Current document, matching epoch, channel `c` without `el:c` in `windows.<w>.channels` | Exactly #1191 AC5: `KELD-GUARD003`, zero effects; a granted neighbour produces one Bun entry. | Prefix/case-fold match → `el:cX`/`EL:c` admitted → RED (#1191 `G552-P04-literal`). |
| AC10 Navigation policy | Each E5 trigger (a, c, d, e1, f2, i, j, k, l, m, n) to an `about:` form and to an off-allowlist `https:` URL | `NavigationStarting` fires, `Cancel` is set, the application document survives (E5: 5/5 veto, 0/3 control, per trigger). Allowlisted navigations proceed. | Replace the scheme rule with an exact denylist `["about:blank"]` → triggers k (`about:blank?`), m (`about:blank#keld`) and n (`about:blank#blocked`) replace the document → RED. |
| AC11 Runtime flag untouched | Any build | `AdditionalBrowserArguments` stays empty; no `--disable-features` naming `msWebView2CancellableAboutNavigations`. | Add the flag → AC10's `about:blank` control replaces the document → RED. |
| AC12 Popups | `window.open` with `_blank`/`target`, `opener` forms | `NewWindowRequested` → `SetHandled(TRUE)` (existing `guarded_navigation.rs:142-152`); no popup exists (E5 f1/f2: handled, 0 windows). | Drop the handler → unaccounted popup → RED. |
| AC13 Generated grants | `keld migrate` sees `ipcMain.on('a')`, `ipcMain.handle('b')`, `ipcMain.on('c')`, `ipcMain.on(dyn)` and window W's preload exposes a fixed-channel method for `a` only (F-C) | `windows.W.channels == ["el:a"]`, with `"$keldWindowChannelGrants": 1` and `windows.W.preload.digest`; `b`, `c` and `dyn` are reported ungranted. For a pure pass-through preload (F-B) the result is `["el:a","el:b","el:c"]`. | Grant every `ipcMain` literal regardless of the preload's reachable sinks → `el:b` appears for W → RED (over-grant). |
| AC14 Digest binding | The admitted preload bytes' SHA-256 differs from `windows.W.preload.digest` | The Host treats W as `channels: []` and fails the window with a typed error naming the fix (`re-run keld migrate`); a matching digest loads the grants. | Ignore the digest → grants produced for another preload apply → RED. |
| AC15 Classifier gate | `keld migrate` over fixtures F-A..F-G (§4.8) | F-A, F-D, F-E, F-F flagged; F-G flagged (conservative: representation-only but unprovable); F-B, F-C not flagged. | Drop the page-input data-flow edge → F-A passes → RED. Separately, flag any conditional → F-C flagged → RED. Separately, let the analyzer "prove" F-G non-narrowing by pattern → F-G passes silently → RED (conservatism falsifier). |
| AC19 Attestation binding | Window W's preload digest d₁ carries a flag; the config holds (i) no attestation, (ii) an attestation bound to d₂ ≠ d₁, (iii) an attestation bound to d₁ | (i) and (ii): the Host refuses route D for W (`channels: []`, typed error naming the fix: review and attest digest d₁); (iii): W loads with exactly the P0-1 intersection grants — an attestation listing extra channels adds none. | Accept an attestation by file path or window key instead of digest → case (ii) loads → RED. Separately, let the attestation record carry channels → an extra `el:x` is granted → RED. |
| AC16 Same-document stability | Hash change, `pushState`, cancelled provisional navigation | Epoch and generation unchanged; a pending granted call completes once. | Rotate on hash change → pending call refused → RED. |
| AC17 Receipt | A Windows acceptance claim | Runtime `BrowserVersionString`, OS build, head, manifest digest, preload digest, per-AC counters, each RED control's output. | Missing runtime string or mocked `WebMessageReceived` → validator RED. |

## 4. Design

### 4.1 Atomic decomposition (decision input)

| Atom | Owner / boundary | Inputs → outputs | Failure mode | Observable | Evidence |
| --- | --- | --- | --- | --- | --- |
| A1 Script registration | `keld-wv` Windows backend, before `navigate_initial` | bridge + wrapped preload text → one document-created script id | Script absent from first document | AC01 | FACT: registration async (Microsoft). UNKNOWN: whether awaiting completion before `Navigate` is needed on 156 (P1 N3b did not falsify) — §10 Q5. |
| A2 Realm identity | WebView2; not controllable | document-created script runs in the main world of every top-level document and same-process frame | Page poisons built-ins; bridge text in frames and in error pages | AC02, AC03 | FACT (E1/E2; E5 `p1-exec-b`: bridge ran in the `about:srcdoc` error page). |
| A3 Event identity (frame) | WebView2 routing | frame post → `Frame2.WebMessageReceived` only | Host registers the frame event | AC03 | FACT: frames guide; E5 trigger q: same-origin srcdoc posts via `parent` arrive with the parent's `Source`. |
| A4 Document eligibility | #1192 §4.4 (KEL-102 verified resource identity) | committed main-frame document + `IsErrorPage` + allowed `NavigationId` → qualified / FailedClosed | URL-text spoof; error page at an app URL | AC04 | FACT: E5 qualified by URL only (probe), so resource-identity qualification is UNTESTED on Windows; FACT: `about:srcdoc` error page committed 5/5 with `Source=about:srcdoc`. |
| A5 Sender authentication (Source) | browser-stamped `Source` URL | compare **full URL** to the verified document URL | Opaque/`about:` documents; origin-only comparison; string-loaded documents | AC05, AC18 | FACT (E5, `https`): 0 mis-attribution in ~110k messages. FACT (source probe, 3/3): `NavigateToString` document → `Source = about:blank`, `location.origin = "null"`; `file://` document → `Source` = full file URL including path, `location.origin = "file://"`; the document-created script runs in both. Whether the full `file://` URL is **safe** as identity depends on KEL-102 path verification of that exact file under the package root (B6) — the stamp alone is not a trust root. |
| A6 Document epoch | Host-minted 128-bit nonce | `ContentLoading` → mint, post | Post at `NavigationStarting` reaches the old document | AC06, AC08 | FACT (E5): 24/24 correct at `ContentLoading`; `START_for_B_receivedBy_A` observed for a `NavigationStarting` post. Cross-document ordering observed, not documented. |
| A7 Authorization | `keld-guard` via `dispatch_privileged` in `keld-core` (#1191 T4) | principal + borrowed window grants + `el:c` → Allow / GUARD003 | Second evaluator; over-grant at generation time | AC09, AC13, AC14 | FACT: `windows` ignored today (`lib.rs:625-628`); #1191 is an open draft. |
| A8 Navigation authority | Host `NavigationStarting` | Uri → vetted-matcher decision → `SetCancel` | `about:srcdoc` and `document.open` unvetoable | AC10, AC11 | FACT (E5): b committed an error page 5/5 despite Cancel; g (`document.open`) replaced the document 5/5 with **no** `NavigationStarting`. |
| A9 Popups | existing deny-all | handled | — | AC12 | FACT: code. |
| A10 Static classification | `keld migrate` (`keld-cli`) | preload AST + digest → verdict + grants | False negative | AC15 | FACT: verb reserved (`verb.rs:24,75`). |
| A11 Evidence | this spec + keld-research#34 | hashed runs → claims | Second runtime/OS claimed | AC17 | FACT: one runtime, one OS, `https` only. |

Independence: A4 (eligibility) and A8 (navigation) are separate rules with separate
manifest keys; A5 and A6 were each tested alone in E5; A7 runs only after A3–A6 pass.

### 4.2 Trust model: two rules instead of three worlds

1. **No authority exists only in preload code.** The page can do whatever the preload can;
   the Host grants exactly the literals `keld migrate` generated for this window and this
   preload digest. The epoch nonce is page-readable; it authenticates the document, not the
   sender, and conveys no authority the document lacks.
2. **The Host authenticates frame, document and epoch, not the JavaScript world.**
   Admission = top-level event ∧ `Source` = qualified document ∧ current nonce ∧ literal grant.

Divergences (scoreboard entries land in T1): same-realm `exposeInMainWorld`; `process.argv`
and preload text are page-visible (no secrets in `additionalArguments`; `keld migrate`
warns); the page sees **every** Host→renderer post to its window (a preload that narrows
inbound events is gating, §4.8); a page can replace its own exposed key (self-harm).

### 4.3 The bridge script

One function-wrapped script, no `eval`, registered once per WebView before first navigation.
Shared `@keld/electron` renderer facade; only the transport adapter is Windows-specific.

```ts
const post = chrome.webview.postMessage.bind(chrome.webview);   // captured before page code
const listen = chrome.webview.addEventListener.bind(chrome.webview);
const stringify = JSON.stringify, parse = JSON.parse;
if (window !== window.top) return;                               // frames get nothing
let epoch: string | null = null;
listen("message", (e) => {
  if (typeof e.data !== "string") return;
  const env = parse(e.data);
  if (env.v !== 1) return;
  if (env.kind === "epoch") { if (epoch === null) epoch = env.nonce; return; }
  if (epoch === null || env.document !== epoch) return;          // AC08: drop stale / pre-epoch
  deliver(env);                                                   // #1154 emitter
});
function send(body: object): void {
  if (epoch === null) throw notReady();                           // AC07: #1154 §4.7, no queue
  post(stringify({ v: 1, document: epoch, ...body }));
}
post(stringify({ v: 1, kind: "hello", href: location.href }));   // authority-free, no epoch
installKeldFloor(send);                                           // KEL-142 §4.1
installElectronFacade(send);                                      // #1154 shapes
if (ENTRY_URIS.has(stripFragment(location.href))) runWrappedPreload(); // #1155 §4.2
```

Rules: the text holds no secret. `ENTRY_URIS` is the set of qualified application entry
URIs the Host embeds at registration from the same manifest snapshot it uses for A4; it is
a convenience gate so the app preload does not execute in allowlisted remote documents or
error pages (it has no authority either way; A4 is the rule). A page can forge a
`chrome.webview` `message` event and corrupt `epoch`; that only breaks its own calls. The
`hello` message is sent once per document before any epoch exists; it carries no channel
and requests nothing.

### 4.4 Host admission and forwarding

Carrier: `WebMessageReceived` strings; JSON-object messages are rejected (one parser).
Envelope: #540 `keld.wv-link` v2 kinds with `document` = epoch nonce; codec bytes base64url
(≤ 4096 decoded). Split of responsibilities per §5: **`keld-wv` authenticates identity
(steps 1–3) and hands a stamped, bounded message to `keld-core`'s forwarder, which is the
only caller of `dispatch_privileged` (#1191 T4).**

```rust
// keld-wv/webview2 (STA callback): identity only; no policy, no Bun write.
fn on_web_message(view: &ViewState, args: &ICoreWebView2WebMessageReceivedEventArgs) {
    // 1. top-level event: registered on ICoreWebView2 only.
    let source = strip_fragment(args.Source()?);
    let env = Envelope::parse_bounded(args.TryGetWebMessageAsString()?)?;   // 4 KiB + header
    if env.kind == Kind::Hello {
        // 2'. authority-free: accepted when Source equals the document the Host is loading;
        //     records "bridge present" for diagnostics; never waits for an epoch.
        return view.note_hello(source);
    }
    // 2. Source must equal the QUALIFIED document's FULL verified URL (A4 decided it; A5
    //    checks it). Never an origin: every file:// document shares one origin, and a
    //    NavigateToString document is `about:blank` (source probe).
    let Some(doc) = view.qualified_document() else { return refuse(Refusal::NoDocument) };
    if source != doc.verified_url { return refuse(Refusal::Source); }
    // 3. epoch byte-equal to the current nonce.
    if !doc.epoch.matches(&env.document) { return refuse(Refusal::Epoch); }
    // hand-off: stamped with (WebviewId, NonZeroU32 generation, digest-bound window key).
    view.forwarder.submit(Stamped { id: view.id, generation: doc.generation, window: view.window_key, env });
}
// keld-core app_session (#1191 T4 forwarder): the ONE evaluator call.
fn forward(stamped: Stamped) {
    let principal = Principal::Webview { id: stamped.id.0, generation: stamped.generation.get() };
    dispatch_privileged(manifest, principal, RendererChannelQuery {
        channel: &stamped.env.channel,                 // "el:" prefixed exactly once (#1191)
        current_window: grants_for(stamped.window),    // empty unless digest matched (AC14)
        on_allow: |_permit| app_link.forward_bounded(stamped.env),
    })
}
```

Refusal order: no-document / Source / epoch precede grant miss (#1191 AC8). A window whose
qualified document never produced a `hello` within the existing `KELD-WV-005` deadline fails
with a typed error (code allocated by the error-registry owner; not `KELD-WV-012/013`).

`window.keld.invoke(channel: u16, payload)` is admitted through steps 1–3 and then exactly
as KEL-142 §4.1 admits it on macOS: only the declared Echo channel `1`; it is **not** routed
through `el:` grants and no numeric channel grant grammar is introduced here. Widening it is
KEL-142/#508's decision (§10 Q7).

Host→renderer delivery (`mainResp`, `webContents.send`) uses `PostWebMessageAsString` with
`document` = the **target document's epoch** captured at send admission (#1192 N15, #543);
the Host drops it if the generation moved, and the bridge drops a mismatch (AC08).

### 4.5 Per-document lifecycle (WebView2 events onto #1192 §4.2)

One rule for revocation: **every top-level `ContentLoading` revokes the previous epoch. A
new epoch is minted iff `IsErrorPage == FALSE`, the `NavigationId` equals that of a
`NavigationStarting` (or its `IsRedirected` hop) the policy allowed, and the committed
document's full `Source` URL equals the Host-identifiable URL of the application document
verified under #1192 §4.4 / KEL-102 (exact file under the verified package root, or the
Keld-owned virtual-host URL). Otherwise the document is FailedClosed with `channels: []`.**
A document loaded with `NavigateToString` can never satisfy this rule (its `Source` is
`about:blank`), so route D **requires** the loading-path change in §4.6a.

| WebView2 event | #1192 transition | Host action |
| --- | --- | --- |
| Scripts registered, controller created | Unbound(0) | Nothing admitted; `hello` noted only |
| `NavigationStarting` (top-level, each redirect hop) | +0 | Policy (§4.6); record allowed `NavigationId`; never post an epoch here |
| `ContentLoading`, rule satisfied | +1 (checked; `MAX` → Exhausted) | Revoke; mint 128-bit nonce; post epoch; start `hello` deadline |
| `ContentLoading`, rule not satisfied (error page, remote allowlisted document, unknown `NavigationId`) | +1 then FailedClosed | Revoke; no epoch; apply `onLoss` |
| `document.open()`/`write` in the application document (E5 g: 5/5 replaced, no `NavigationStarting`, no `ContentLoading`) | +0 | **Host-unvetoable limit #2.** Same document, same epoch, same authority; the page blanked itself. Electron's `will-navigate` cannot see it either. No Host action; recorded in the report |
| Hash / `pushState` / cancelled provisional navigation | +0 | Keep nonce and work (AC16) |
| Trusted Host load (`onLoss: reload-entry`) | +1 before the engine call | Revoke, then `Navigate`; bounded to **3 automatic recoveries per window lifetime**, then close with a typed error |
| Bun role restart | +0 | Keep document and nonce; revoke old correlations |
| Destroy | terminal | Revoke; numeric id never reused |

One generation source (#1192 N16): the Windows backend supplies this checked generation to
**both** the media guard principal (today `0`, arch 03 §4.3) and channel queries; T2 changes
the media principal construction accordingly with the KEL-102 owner.

### 4.6 Navigation policy (manifest-declared, Host-enforced; separate from eligibility)

```jsonc
{
  "$keldWindowChannelGrants": 1,
  "windows": {
    "main": {
      "channels": ["el:rendererReq"],              // generated (AC13)
      "preload": {
        "digest": "sha256:…",                      // binds verdict + grants (AC14)
        "attestation": {                            // only when the classifier flagged sites (AC19)
          "digest": "sha256:…",                    // must equal preload.digest
          "sites": [{ "at": "preload.js:96-100", "why": "removes a function value that cannot cross IPC" }],
          "reviewedBy": "<permission-model reviewer>"
        }
      },
      "web": {
        "csp": "default",
        "navigation": {
          "allow": ["https://app.example/**"],      // arch 03 §2 vetted URL scopes; entry is implicit
          "onLoss": "reload-entry"                  // or "close"
        }
      }
    }
  }
}
```

- `allow` entries use the **existing vetted path/URL scope matcher** (arch 03 §2: `/**`
  suffix, authority must be named, literal separator). The application entry resource is
  always allowed. Anything else is cancelled in `NavigationStarting`, including every
  `about:`/`data:`/`javascript:`/`blob:` URI (the matcher cannot name those authorities, so
  they never match; no scheme list is hand-maintained). `IsUserInitiated` does not relax it.
- Allowed non-application documents (remote content) get **no epoch and `channels: []`**
  (arch 03 §4.3); navigation permission is not channel permission.
- `FrameNavigationStarting` is not cancelled; frames are governed by CSP (§4.7).
- `AdditionalBrowserArguments` stays empty (AC11). `NewWindowRequested` stays deny-all.
- Limits: top-level `about:srcdoc` commits an error page despite `Cancel` (E5 b, 5/5; the
  page's `Source` is `about:srcdoc`; rule §4.5 leaves it FailedClosed; `onLoss` recovers);
  `document.open()` replaces the document with no navigation event (E5 g, 5/5; §4.5).
- A preload's own veto (e.g. a `navigation` listener with `preventDefault()` on `about:`) is
  recognised by `keld migrate` as redundant with this policy and not relied on.

### 4.6a Loading-path requirement (not optional)

Today the Windows host loads the application renderer with `NavigateToString`
(`app_session.rs:3653` → `mod.rs:3654`). The source probe shows that such a document reports
`Source = about:blank` and `location.origin = "null"` — the same `Source` as an `about:blank`
a page or attacker reaches (E5 trigger a control). The Host therefore **cannot** tell the
application document from a replacement, and no epoch may be minted for it. Route D
**requires** the application document to be loaded from a Host-identifiable URL:

- either the exact `file://` URL of the entry file under the KEL-102-verified package root —
  the probe shows `Source` carries the full path, and the Source rule compares the **full
  URL**, never the `file://` origin, which every file document shares; whether that URL is
  safe as identity depends on the path verification (B6), not on the stamp;
- or a Keld-owned virtual host mapped to the verified package root (`DENY_CORS`), whose URL
  is likewise compared in full.

The choice is owned by W3 #1185 (file-origin navigation/containment) together with
#1192/KEL-102 (resource identity); T2 carries the `NavTarget::Html` → verified-URL change
for compat windows once that decision is recorded. Until then AC18 is the standing proof
that the current path is refused.

### 4.7 Frames rule

Same-origin iframes reach `top.<exposed key>` and `top.keld.invoke`; their calls arrive on the
top-level event with the top document's `Source` and nonce and are indistinguishable from the
application (admitted; Electron behaves the same; stated in the report). A same-origin
iframe's **own** `chrome.webview.postMessage` routes to `Frame2`, which is not registered
(AC03). Cross-origin frames cannot reach the top realm (`SecurityError`, E2). Framing is
governed only by `windows.<w>.web.csp` (`frame-src`, `frame-ancestors`).

### 4.8 `keld migrate`: window mapping, classifier, grants

**Window mapping.** Each statically resolvable `new BrowserWindow(opts)` site in the main
process becomes one `windows.<w>` entry: `w` = the `opts.title` literal slugified, else
`w<n>` in source order; the generated `keld.config.ts` window entry records the site
(`file:line`) and the resolved `webPreferences.preload` path; the `@keld/electron`
`BrowserWindow` facade passes `w` at creation, where the Host binds it immutably (#1191
AC3). Two sites sharing one preload are two windows with identical generated grants.

**Classifier.** Exposure = function reachable from `contextBridge.exposeInMainWorld` or an
assignment to `window.*`/`globalThis.*`. Outbound sink = `ipcRenderer.send/invoke/sendSync/
postMessage/sendTo/sendToHost`. Inbound sink = the page-facing callback invoked from an
`ipcRenderer.on/once` listener. Page input = exposure parameters and values derived from them.

> **Gating = authority narrowing.** For some page input the preload suppresses or refuses
> a sink call, restricts which channel is reachable, or restricts/validates argument values
> that a pass-through would have forwarded; dropping or filtering inbound Host→renderer
> messages also counts. A transform that only changes **representation** without narrowing
> what the page can cause — for example removing a value that cannot cross the IPC boundary
> anyway — is **not** gating by definition.

A static analyzer generally **cannot prove** that a transform is representation-only (it
would have to know the value domain and the boundary's rejection rules on every path).
That is why rule 2 exists:

> **Rule 2 — the classifier is conservative.** `keld migrate` flags **every**
> page-input-dependent branch or transform on a path to a sink that it cannot prove
> non-narrowing, plus every inbound filter. Indeterminate code (dynamic `require`, `eval`,
> computed member access on `ipcRenderer`, unresolvable preload path) is flagged.
> Unconditional additions (a counter `reqId`) are not page-input-dependent and are not
> flagged.

Classifier falsifiers: a refusing/validating preload MUST be flagged; a pure pass-through
MUST NOT be; a representation-only transform the analyzer cannot prove MUST be flagged
(conservative), never silently passed.

| Fixture | Content | Definition | Classifier |
| --- | --- | --- | --- |
| F-A | `request: (m) => { if (ALLOWED.has(m.action)) send('req', m); }` | narrowing (suppresses) | flagged |
| F-B | `sendMessage: (a, x) => send(a, x)` (page chooses the channel) | pass-through | not flagged |
| F-C | `openFile: () => invoke('open-file')` | pass-through | not flagged |
| F-D | `save: (p, d) => send('save', path.normalize(p), d)` | narrowing (validates values) | flagged |
| F-E | `require(name).send(...)` / `eval` | unknown | flagged (indeterminate) |
| F-F | `on('evt', (e, v) => { if (v.ok) cb(v); })` | narrowing (inbound filter) | flagged |
| F-G | `request: (m) => { m.reqId = n++; if (m.action == 'watchFile') { L[m.path] = m.listener; delete m.listener; } send('req', m); }` | representation-only (see below) | flagged (cannot prove) |

**Rule 3 — attestation.** A flagged preload runs on route D only with a **reviewed
attestation**: a record in the application's Keld config, bound to the preload source
digest (#1155 AC08 SHA-256), that names each flagged site (`file:line`) and states why it
does not narrow authority. It is reviewed under the permission-model gate. The Host refuses
route D for a window whose digest carries a flag and has no attestation for **that exact
digest** (AC19); an attestation bound to another digest is refused. An attestation **never
widens grants**: `channels(w)` stays exactly the P0-1 intersection below. Without an
attestation the application is reported unsupported for route D (route B is out of scope).

Qualification evidence, stated plainly: the pinned draw.io preload's `request` method
(`electron-preload.js` lines 89–103) is F-G. By the definition it is representation-only:
the branch moves the function-valued `msg.listener` out of the message before
`ipcRenderer.send`, and a function cannot cross the IPC boundary in either runtime — Electron
documents that `ipcRenderer.send` arguments are serialized with the Structured Clone
Algorithm and that "sending Functions, Promises, Symbols, WeakMaps, or WeakSets will throw
an exception" ([ipcRenderer.send](https://www.electronjs.org/docs/latest/api/ipc-renderer#ipcrenderersendchannel-args),
current docs; the pinned v44 wording is INFERRED identical), and #1190 V3 refuses a nested
Function synchronously before any send. Removing it therefore narrows nothing the page could
cause; the conservative classifier still flags it (it cannot prove the value is always a
function). Its `registerMsgListener`/`listenOnce` (lines 104–121) forward inbound payloads
unchanged. **So the pinned draw.io preload qualifies on route D only with an attestation**
for that digest covering line 96–100 — through the framework mechanism of rule 3, not an
application exception.

**Grants.** For window `w` with preload `P` (verdict pass-through):
`channels(w)` = literal channels of `ipcMain.on/handle` in the main process **∩** literal
channels of outbound sinks reachable from `P`'s exposures; if any reachable sink takes its
channel from page input (F-B), the intersection is the full `ipcMain` literal set. Dynamic
`ipcMain` names are reported ungranted. A flagged `P` **without** an attestation for its
digest yields `channels: []` and the report line `✘ preload has N unproven narrowing sites
(<file>:<line>, …) — unsupported on Windows route D without a reviewed attestation;
escalation: route B (no approved spec)`. A flagged `P` **with** a matching attestation
yields the same intersection as a pass-through `P` — never more. The verdict, flagged sites
and grants are written with `windows.<w>.preload.digest` = SHA-256 of `P`'s bytes (#1155
AC08 digest) and, when present, `windows.<w>.preload.attestation` (same digest, sites,
rationale, reviewer); the Host compares the admitted bytes' digest and refuses grants on
mismatch (AC14) or on a flag without a same-digest attestation (AC19). Before release
reliance, run across the compat corpus (#496, #587) and record false positives/negatives.

### 4.9 Rejected alternatives

| Route | Why rejected |
| --- | --- |
| A. CDP `Page.addScriptToEvaluateOnNewDocument` with a named `worldName` world + `Runtime.addBinding` | Universal access: cross-origin XHR upload events fired in every such world, 0 in the main world or a grant-free world (P3) — more than an Electron preload; CDP unmaintained by Microsoft (S2); `worldName` experimental (S3). |
| B. `Page.createIsolatedWorld` with `grantUniveralAccess: false` behind a page `debugger;` pause | Works 36/36 (P3-A) but Debugger stays on, renderer pauses on the Host UI thread per document and per page `debugger;`, two silent failures, undocumented pause semantics. Escalation only; no spec. |
| C. MV3 extension content script | Profile persistence, no host channel without CDP (#3851), extension-origin authority. |
| E. Real preload in the main world as the boundary | Page poisons it; no Host rule compensates. |
| Status quo | No `contextIsolation` preload support on Windows. |

### 4.10 Capabilities, wire, platform, seam, migration

- **Manifest:** `windows.<w>.web.navigation` and `windows.<w>.preload.digest` (schema owner
  #541/#1191, §10 Q2); channel grants per #1191. Permission-model gate.
- **Wire:** no KIPC change; new private WebMessage carriage of `keld.wv-link` v2 with
  `hello`/`epoch` kinds and base64url codec bytes. Wire gate; #540 (§10 Q1).
- **Public API:** `window.keld.invoke` unchanged; `@keld/electron` renderer facade reachable
  on Windows; `keld migrate` report/manifest keys; `WebEngine`/`WebviewSpec` extension
  (window key, navigation policy, admitted preload bytes + digest) — trait review, live
  backend in the same PR.
- **Platform:** Windows only; macOS keeps #1155/KEL-142; Linux UNKNOWN.
- **Runtime seam:** `create` additionally registers `NavigationStarting` policy,
  `ContentLoading` epoch, `WebMessageReceived` identity and the document-created script
  inside the `GuardInstalled` mint, then navigates. STA thread; renderer crash ≠ Host;
  handlers live for the WebView lifetime; manifest and policy snapshotted once (#1191 AC1).
- **Migration unit:** `WebView2Engine::create`, `WebviewSpec`, `app_session` forwarder,
  `@keld/electron` facade/transport, `keld migrate`. No temporary adapter; no persisted
  state beyond the manifest. Compatibility fallback: not required.

## 5. Boundaries

- Implement in: `crates/keld-wv/src/webview2/` (identity, epoch, navigation policy, script
  registration; one owner for the `GuardInstalled` mint); `crates/keld-wv/src/engine.rs`;
  `crates/keld-core/src/app_session.rs` (**the** forwarder, shared with #1191 T4; all
  `dispatch_privileged` calls live here); `packages/@keld/electron` (facade + Windows
  transport + `BrowserWindow` window key); `crates/keld-cli` (`migrate`); `docs/architecture/
  03,04,05` (T0 deviations); `keld-compat` conformance/scoreboard entries (T1).
- Must not touch: `crates/keld-guard` grammar/evaluator (#1191/#541); `crates/keld-ipc`
  framing; the KIPC channel table (#508); macOS `wkwebview/*`; workspace `Cargo.toml`;
  `keld.config.ts` schema beyond the generated window entries.

## 6. Tasks (each ≈ one PR; ordered; owner; blocking dependency)

- [ ] **T0 — this spec + architecture deviations** (owner: @amishabenramani). Blocks on:
  KEL-142 owner sign-off of the §5.1 Windows amendment (B1); tracker amendment (B2);
  owner answers to §10 Q1–Q3.
- [ ] **T1 — conformance entries first** (owner: `keld-compat` owner, #464/#567). Expected-fail
  cells and scoreboard divergence entries for route D (same-realm `exposeInMainWorld`,
  page-visible inbound posts, `about:srcdoc`/`document.open` limits). Blocks on T0.
- [ ] **T2 — loading path, navigation policy, eligibility and epoch in `keld-wv`** (owner:
  Windows W1 implementer). Replace `NavTarget::Html` for compat windows with the decided
  Host-identifiable URL (§4.6a); AC01, AC03–AC08, AC10–AC12, AC16–AC18; one generation source
  for media and channel principals (KEL-102 owner coordination). Blocks on: W3 #1185 +
  #1192/KEL-102 recording the loading-path decision (B6); #541 approving `web.navigation` and
  `preload.digest` keys (Q2); the Windows resource-identity oracle (B3); the Windows trust
  root (B4). Floor-only bridge (`window.keld.invoke`).
- [ ] **T3 — grant and forward** (owner: `keld-core`/#1191 T4 implementer). AC02, AC09, AC14.
  Blocks on #1191 T1–T4 merged and #540 carriage decision (Q1).
- [ ] **T4 — `@keld/electron` facade on the Windows transport** (owner: #1154 implementer).
  Wrapper execution gated on `ENTRY_URIS`; divergences wired to T1 entries. Blocks on T3.
- [ ] **T5 — `keld migrate` mapping, classifier, grants, digest, attestation** (owner:
  `keld-cli`/KEL-17 owner; Host-side refusal in T3's forwarder owner). AC13, AC15, AC19;
  fixtures F-A..F-G; corpus run. Blocks on KEL-17 admitting the first live `migrate` slice
  (Q9).
- [ ] **T6 — Windows qualification receipt** (owner: @amishabenramani). One corpus application
  whose preload is pass-through or carries a reviewed same-digest attestation (§4.8 rule 3),
  full RED matrix, AC17. Blocks on T2–T5 and the attestation review (permission gate).

## 7. Test plan

| Test | AC | Shape and anti-flake |
| --- | --- | --- |
| `wv2_first_inline_floor` (device, ignored-by-default, dedicated STA harness like `media_acceptance`) | AC01 | First inline writes `typeof window.keld.invoke` + exposed keys to `document.title`; Host reads after `NavigationCompleted`. Mutant installs via `ExecuteScript` at DOMContentLoaded. |
| `wv2_poisoned_page_no_widening` | AC02 | Poison then call granted/ungranted/forged; forwarder + fake Bun counters. |
| `wv2_same_origin_iframe_frame2` | AC03 | Same-origin srcdoc iframe reads `top` nonce, posts via own `chrome.webview`; assert 0 top-level events; mutant registers `Frame2`. |
| `wv2_eligibility_matrix` | AC04 | (a)–(d) documents on loopback `https` (port 0) with an error page served at an allowlisted path (HTTP 500 → `IsErrorPage`) and `about:srcdoc`; epoch-post counter per document; two mutants. |
| `wv2_source_rule` | AC05 | Messages from app document, sibling path, `about:blank`, `about:srcdoc` error page; mutant compares origin only. |
| `wv2_epoch_rotation` | AC06, AC16 | A→B cross-origin, A→A2 same-origin reload, hash/pushState; host-replayed A envelopes after B's `ContentLoading`; mutant admits by Source + generation. |
| `wv2_not_ready_refusal` | AC07 | Fixture calls before epoch; assert `undefined`/typed rejection, 0 queued, later success; mutant buffers. |
| `wv2_stale_post_dropped` | AC08 | Host test hook posts an e₁-stamped event after B commits; B's listener counter 0; mutant removes the bridge check. |
| `guard_channel_literal` (#1191 tests) | AC09 | Pure Rust. |
| `wv2_navigation_matrix` | AC10 | E5 triggers × destinations, 5 veto + 3 control runs each, with/without `SendInput` activation; heartbeat oracle; mutant uses exact denylist. |
| `wv2_environment_args_empty` + source grep | AC11 | Unit. |
| `wv2_popup_handled` | AC12 | Existing KEL-168 test. |
| `migrate_grant_generation` | AC13 | Golden manifest for F-B and F-C against a fixed `ipcMain` set; mutant grants all literals. |
| `wv2_digest_binding` | AC14 | Tamper one preload byte after migrate; assert `channels: []` + typed error; mutant ignores digest. |
| `migrate_classifier_fixtures` | AC15 | F-A..F-G + pinned preload bytes (expected: flagged); two mutants. |
| Receipt validator | AC17 | `tools/` check of the acceptance comment fields. |

Anti-flake: device tests behind the existing Windows feature flag on the dedicated STA
harness; loopback servers bind port 0; completion observed through callbacks pumped by
`wait_with_pump`, no sleeps; each mutant in its own process.

## 8. Review gates triggered

- **Permission model — mandatory, independent reviewer per `.agents/coordination.md`:**
  eligibility-before-epoch rule, Source rule, navigation matcher reuse, generated grants
  (intersection + digest binding), no second evaluator, no wildcard; **every preload
  attestation** (§4.8 rule 3) is reviewed under this gate per digest.
- **Public API — mandatory:** `WebEngine`/`WebviewSpec`; `@keld/electron` facade on Windows;
  `keld migrate` keys and report; KEL-142 §5.1 Windows amendment.
- **Wire protocol — #540 owner:** WebMessage carriage, `hello`/`epoch` kinds, base64url bytes,
  epoch-stamped Host→renderer envelopes.
- **unsafe:** new COM registrations in `keld-wv/webview2` (`add_NavigationStarting` +
  `SetCancel`, `add_ContentLoading`, `add_WebMessageReceived`, `PostWebMessageAsString`,
  `AddScriptToExecuteOnDocumentCreated`); local `// SAFETY:` proofs; issue-scoped update of
  `crates/keld-wv/AGENTS.md`'s Windows FFI list; independent unsafe gate on the final diff.
- **Dependency addition:** none.

## 9. Perf impact

- First paint (arch 01 §5): wry's **blocking** `AddScriptToExecuteOnDocumentCreated` cost
  96–109 ms (`docs/agents/learnings.md`, KEL-65); the asynchronous form's critical-path cost
  is UNKNOWN and MUST be measured with the KEL-64 beacon before T2 merges (>5 % → waiver).
- Per message: bounded envelope parse allocates; the grant decision is zero-allocation on
  Allow (#1191 AC6). Report parse / decide / forward as separate samples.
- Per navigation: nonce mint + one post; negligible, measured anyway.

## 10. Approval blockers and open questions

**Blockers (must clear before `Status: approved`):**

- **B1 KEL-142 §5.1 amendment (Windows):** the page-world bridge carries the document nonce;
  KEL-142 owner sign-off required.
- **B2 Tracker amendment (#1184 owner):** proposed text — *"Windows route D: replace AC2
  (three-way separation) with: (i) no authority exists only in preload code — enforced by
  the `keld migrate` classifier and digest-bound grants; (ii) the Host authenticates frame,
  qualified document and epoch on every call. The deterministic negative control 'move the
  app preload into the page execution world' is vacuous under route D (the preload already
  runs there); replace it with 'remove the Host literal-grant check' and 'remove the
  renderer-side epoch check' (AC02/AC08). AC1's 'external trust root' remains required and
  is currently unowned on Windows (B4)."*
- **B3 Windows application-document oracle:** #1192 §4.4 eligibility (verified resource
  identity, not URL text) has no Windows implementation; E5 qualified by URL. The #1192
  owner must name the Windows oracle or accept this spec's T2 as it.
- **B4 Windows preload trust root — unowned gap:** #1155 AC07/AC08 are macOS-only. Without
  it AC14's digest has no trust anchor. Owner to be assigned on #1182/#484.
- **B5 Every reused contract is an open draft:** #1191, #1192, #1154, #1190, #1155, #540
  (#1122). This spec cannot be approved as implementable before those it consumes are at
  least approved; list re-checked at approval time.
- **B6 Loading-path decision (W3 #1185 + #1192/KEL-102):** today's `NavigateToString` path
  yields `Source = about:blank` (source probe 3/3) and can never qualify (§4.6a). The owners
  must record which Host-identifiable URL compat windows load — exact `file://` under the
  verified package root, or a Keld-owned virtual host — and how KEL-102 verifies that exact
  path. Without it AC18 holds but AC04/AC05 have no positive case.

**Open questions:**

1. **Q1 (wire, #540):** WebMessage string carriage of `keld.wv-link` v2; `document` = epoch;
   `hello`/`epoch` kinds. Decide on #1184 before T2.
2. **Q2 (manifest, #541/#1191):** keys `windows.<w>.web.navigation` (`allow`, `onLoss`) and
   `windows.<w>.preload.digest`; `allow` entries as vetted URL scopes. Decide before T2.
3. **Q3 (compat, #1154/#1155):** `process.contextIsolated` on Windows: recommend `true` plus
   the documented divergence (applications branch on it). Decide in the T1 entry.
4. **Q4 (tracker):** B2 text acceptance.
5. **Q5 (ordering) UNKNOWN:** whether awaiting registration completion before `Navigate` is
   necessary on 156. Probe: 20 runs each way with the AC01 fixture; recorded in the T2 receipt;
   keep the await regardless.
6. **Q6 (`file://` Source) — resolved to FACT by the source probe (3/3, 156.0.4314.50):**
   `Source` is the full file URL including path; `location.origin` is `file://`; the
   document-created script runs. Still UNKNOWN and left for the T2 receipt: `Source` after a
   hash change on a `file://` document and `IsErrorPage` for a missing file. Whether the URL
   is safe as identity is B6 (KEL-102 path verification), not a Source question.
7. **Q7 (`window.keld.invoke` widening):** numeric channels beyond Echo `1` need KEL-142/#508
   grammar; none proposed here.
8. **Q8 — RESOLVED (owner-delegated decision, 2026-10-29):** gating is defined as authority
   narrowing; the static classifier is conservative and flags what it cannot prove; a
   flagged preload runs on route D only with a reviewed, digest-bound attestation that never
   widens grants (§4.8 rules 1–3, AC19). Applied to the qualification evidence: the pinned
   draw.io preload's `watchFile` branch is flagged by the classifier, is representation-only
   by the definition, and therefore qualifies only with an attestation for its digest. This
   is a framework mechanism; no application exception exists.
9. **Q9 (`keld migrate`):** verb reserved (`verb.rs:75`, KEL-17); T5 needs KEL-17's owner to
   admit the classifier as the first live slice.
10. **Q10 (natural late delivery) UNKNOWN:** never observed (E5, 0 of ~110k); the nonce rule
    covers it; T6 receipt reports the count.
11. **Q11 (page CSP vs document-created script) UNKNOWN:** probe with `script-src 'none'`
    on the AC01 fixture.

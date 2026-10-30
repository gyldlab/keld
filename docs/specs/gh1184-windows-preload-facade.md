# Spec: Windows WebView2 preload compatibility through a Host-enforced declared facade (GH-1184 W1, route D)

Status: draft (revision r11 applies the #540 wire owner's Q1 decision, [#1307 comment 6532691875](https://github.com/gyldlab/keld/pull/1307#issuecomment-6532691875):
unchanged CompatV2 over WebView2 WebMessage strings, `bound` answering a verified `bind`, a
page-readable `document` nonce that is a correlation tag only, and one send record per
direction across the role (§10 Q1, RESOLVED);
revision r10 answers 0monish's re-review of `9bc56726` — `KELD-GUARD003` keeps
#1191's frozen text, and a browser-process exit ends the session instead of reloading, through
W2's `AbnormalExit` as #1280 r16 D8 (`be9e9e56`) latches it; a review of the r10 draft found one P2,
that terminal assumed rather than cited, fixed before push, and a whole-spec defect-class sweep
(C1–C9) then corrected the remaining instances before push: W3's current rules at #1311
`1db53482` (B19's cancel of page-initiated `file:` navigations, Q22's delivery condition,
attribution by causal proof), negative controls that could not turn RED, task staging, one
handler per WebView2 event, and citations;
revision r9 answers 0monish's re-review of `e381bba2` — a sink grants only
the `ipcMain` registration kinds it dispatches to, and the bridge speaks only #540's CompatV2
grammar; an isolated review of the r9 draft found three P2s and seven P3s, fixed before push;
revision r8 answers 0monish's review of `0e0a909b` — main-IPC sinks only by
dispatch; revision r7 answers rpipaliya's reviews of `d7c9bdcb` — query comparison, window keys,
bounded hold exit, OCR gate — and the r5 round's open items: AC25 (f) outcomes, the hold's
navigation attribution, the `pushState` residual and the byte-exact query; an isolated review
of the r7 draft found four P1s, fixed before push; r6 after isolated reviews r1–r3 and pre-post round r4 of the #1184 B2
amendment, and rpipaliya's review of `043170ca`; r3 is `a765b4a5`, r4 is `043170ca` — rebased
from local `2ae5aedd`/`e5870822` onto `06718ac1` — and r5 is `27d81e66`/`53ec78e8`)
Linear: GH-1184 (GitHub tracker under #517; parent program #1182 W1; renderer epic #483) ·
Owner: @amishabenramani (real Windows operator) · Updated: 2026-10-30

Code citations are at `origin/main` `457a9c3b37f71179f167bd1f8c706c461cc4c479`. Device
evidence is one runtime and one OS build: WebView2 Evergreen `156.0.4314.50` on Windows 11
`10.0.26300`, `webview2-com(-sys) 0.38.2` (SDK 1.0.3650.58). Evidence runs live in
keld-research PR [0monish/keld-research#34](https://github.com/0monish/keld-research/pull/34):
note `library/host-platforms/264-webview2-preload-isolation-route.md`, runs
`gh1184-webview2-preload-isolation-2026-10-29` (P1–P3) and
`gh1184-route-d-host-rules-2026-10-29` (E5; `results.json`, per-trigger `p1-exec-*.json`,
probe host `src_main.rs`). E5 ran **only** `https://` virtual hosts mapped with
`DENY_CORS`, qualified documents by URL text, and its stale-document attribution statistic
had an **empty at-risk set** (§10 B7). The `NavigateToString`/`file://` Source facts come
from keld-research run `gh1184-message-source-2026-10-29`: `source-v1` (runs 1–3 identical,
same runtime and OS) and `source-v2` (same shape: `Source` under
`pushState`/`replaceState`/hash changes and an entry URL with a query), each with
`original-SHA256SUMS` and the probe source. The route-D device evidence and its
framework-level correction are recorded on
[#1184](https://github.com/gyldlab/keld/issues/1184#issuecomment-6500027152); route D itself
is the proposed decision of §10 B2. Electron
behaviour is read from source at `electron/electron` `v45.1.0` (commit `45d20ec0`) and
`v44.2.0` (commit `aa650d74`; the pinned draw.io declares `electron ^44.2.0`). This draft
authorizes no production code, no permission and no public API. Implementation starts only
after an exact-content `Status: approved` decision, the review gates in §8 and the blocker
list in §10 are cleared.

**Independent OCR review gate — pending.** The patched high-effort native OCR review at
`d7c9bdcb` (job `4a4273b4-3baf-467f-869f-d6ff91674a08`) finished `completed_with_warnings`:
round 2 failed because the host-agent relay timed out. Its two findings are recorded and
answered in r7 (window-key collisions, §4.8; bounded hold exit, §4.4 rule 2), but it is not
a warning-free complete review. Before exact-content approval, OCR is rerun on the
then-current head with every round successful and its findings are reconciled against the
review threads (§10 B8).

This is a **framework mechanism for every Electron application on Windows**. No rule below
names or special-cases one application. The pinned draw.io Desktop preload
(`jgraph/drawio-desktop@2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`,
`src/main/electron-preload.js`) appears only as qualification evidence, including where the
classifier in §4.8 flags it.

## 1. Goal & non-goals

An Electron application migrated with `keld migrate` runs its `contextIsolation: true`
preload against Keld's `contextBridge`/`ipcRenderer` facade inside a Windows WebView2
window, with the facade present before the first inline page script, while **every**
renderer-to-Host call is admitted by the Host alone: top-level `WebMessageReceived`, the
current per-document epoch nonce of a document whose identity was decided once at commit, a
browser-stamped `Source` that passes the per-call scheme/origin/root check (§4.4 step 3), and
an exact literal channel grant that `keld migrate` generated into `keld.permissions.jsonc` for
that window and that preload digest. Navigation is a manifest-declared, Host-enforced policy
separate from document eligibility. A preload that may narrow authority or hold
Host-delivered data in its own JavaScript is flagged statically by `keld migrate`; its window
receives no grants unless the developer attests, for that exact preload digest, that every
flagged site narrows nothing. Without an attestation nothing degrades silently, provided the
recorded verdict is current — it is bound to the preload digest and recomputed by release
builds (§4.8 rule 3); with an attestation, a wrong attestation silently loses the narrowing.
Default-deny is unchanged.

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
- Arbitrary dynamic `contextBridge` graph parity, `sendSync`, `MessagePort` transfer,
  multiple preloads per window, Electron `webContents` navigation events.
- Conformance score changes (#464/#567 own cells; nothing here flips a row).

## 2. Spec refs

Governing architecture: [03-security](../architecture/03-security.md) §1 (webview principal
= host WebView id + navigation generation; guard before handler), §2 (`windows.<w>.channels`
block; the vetted path/URL scope matcher for manifest scope patterns, 03 §2 "Path and URL
scopes", which T0 amends to name the `file:` URL owner beside it — see the deviations below), §4.3
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
  Host-enforced per call; the page sees every Host→renderer post to its window;
  `process.contextIsolated` is `false` there; the document nonce is page-readable there, a
  freshness/correlation tag, not a secret or authentication". The accepted divergences are listed once,
  in §4.2.
- Architecture 03 §2 says path and URL scopes "share one vetted-pattern matcher". That
  matcher (`keld_guard::path_in_scope`, `lib.rs:1276-1293`) is byte-exact, case-sensitive and
  non-decoding ("This is not URL normalization", `lib.rs:778`), and it is private behind
  `evaluate`, which refuses webview principals with `KELD-GUARD006` (`lib.rs:790-792`). It
  cannot decide `file:` URL membership without changing its lexical contract for every
  grant, and a byte matcher would refuse Chromium's percent-encoded or mixed-case canonical
  `Source` for a root with spaces or non-ASCII characters. T0 (or W3 #1185's T0, whichever
  lands first) amends 03 §2 to name **two** owners: the scope-pattern matcher for manifest
  scope patterns, including non-`file:` `web.navigation.allow`; and keld-guard's
  `file_url_root_relative` (W3 #1185 §4.4) for "`file:` URL → admitted-root membership +
  components", used by W3's resource serving, W1's per-call `Source` rule and W1's in-root
  `file:` navigation decisions.
- Architecture 03 §2's manifest example shows `"el:*"`; #1191 AC2 forbids wildcards. This
  spec follows #1191. Owner action: #541 (manifest owner) corrects the example; T0 links it.
- KEL-142 §5.1 forbids a page-callable relay that carries the document nonce. Route D's
  bridge **is** such a relay on Windows (§4.2). This is a **required KEL-142 amendment for
  Windows** needing KEL-142 owner sign-off before approval (§10 B1).
- #540 (#1122 `d9211a96` §4.2, `docs/specs/gh540-compat-ipc.md:163`) says the document nonce "stays private to native/isolated
  world". On Windows route D it does not: the bridge runs in the page's main world, so page
  code can read `bound.document`, inspect every raw Host post and send a raw v2 envelope. The
  #540 wire owner's decision (§10 Q1(c)) keeps the same 32-lowercase-hex field on route D as a
  freshness/correlation tag, not a secret and not authentication, and forfeits that sentence
  for route D only; it remains the rule for the isolated macOS path. The decision requires the
  sentence to be qualified in the #540 spec (a #1122 edit, its owner's) and in the
  security/compatibility ledger (T0's 03 §6 line above). It does not approve B1.

| Existing owner (reuse, no duplicate) | Exactly what this spec consumes | Decision |
| --- | --- | --- |
| [#1191 GH-552 exact channel grants](https://github.com/gyldlab/keld/pull/1191) (**open draft**) | AC1 opt-in `"$keldWindowChannelGrants": 1`; AC2 exact decoded `el:<c>` literal; AC3 Host-bound window selection; AC4 `RendererChannelQuery`/`RendererChannelPermit`; AC5 `KELD-GUARD003`; AC6 zero-allocation Allow inside the one `dispatch_privileged`; AC8 identity refusal precedes grant miss; T4 narrow `keld-core` forwarder. | Reused unchanged. Windows supplies `Principal::Webview` and the borrowed window grants; the forwarder is #1191 T4's, not a second one. |
| [#1192 GH-542 checked navigation binding](https://github.com/gyldlab/keld/pull/1192) (**open draft**) | §4.2 `NativeDocumentPhase` transitions and checked `u32` generation; §4.4 application-document eligibility (verified stage/package resource identity under #499/KEL-102, never URL text) and its zero-queue early `bind` ("zero queued Bind/Invoke/Send/Consumed", #1192 `cd30f997` §4.4, `docs/specs/gh542-native-document-dual-world-binding.md:133`); N11 commit tied to an allowed attempt; N15 Bun restart keeps the document; N16 one generation in stamp and principal; N21 Windows UNKNOWN. | Reused as the state and eligibility model; this spec is the Windows oracle. Divergence: one realm on Windows, so §4.3's two realm credentials do not exist; the epoch nonce is page-readable and is a correlation tag for the current document, not authentication (§4.2 rule 1, §10 Q1(c)). |
| [#1154 GH-467 facade](https://github.com/gyldlab/keld/pull/1154) (**open draft**) | §4.5 `IpcRendererFacade`/`ContextBridgeFacade`; §4.3 graph bounds (they are the only bounds; this spec adds none); §4.7 fail-closed early-call policy: void return, **zero queued work, no replay**, one typed diagnostic (Host-private in #1154; renderer-local on route D, divergence 11); C09 emitter semantics. | Reused as the one TypeScript facade; only the transport adapter is engine-specific. Divergence: same-realm `exposeInMainWorld` (no copy/proxy). |
| [#1190 GH-544 codec](https://github.com/gyldlab/keld/pull/1190) (**open draft**) | V1–V8; 4096-byte bound; synchronous refusal of functions before send; Host-opaque bytes. | Reused unchanged; the bytes travel as #540's `payload: JsonBytes` (a JSON array of byte values) inside the CompatV2 envelope, never base64url (§4.4). |
| [#1155 GH-484 preload runtime](https://github.com/gyldlab/keld/pull/1155) (**open draft, macOS-only**) | §4.2 wrapper and `process` subset; AC05 throw handling; **AC08 SHA-256 source digest** verified outside the writable package tree. | Wrapper reused. The AC08 digest is reused as the key that binds classifier verdict and grants (§4.8). AC07/AC08's trust root is specified for macOS only; Windows has **no owner** (§10 B4). |
| [KEL-142](kel142-macos-renderer-bridge-api.md) §4.1 | `window.keld.invoke(channel: u16, payload)`; frozen; one pending invoke; routes exactly the declared Echo channel `1`. | Reused as the Windows public floor; admission in §4.4. §5.1 needs the amendment above. |
| [GH-540 spec PR #1122](https://github.com/gyldlab/keld/pull/1122) (**open draft**) `keld.wv-link` v2 | CompatV2 `RendererToNativeV2`/`NativeToRendererV2` forms (`d9211a96` §4.5); fixed `bound.limits` ceilings, "not fresh spendable credit" (§4.5); reservations and terminal release (§4.8); `decode_wv_link` stays the one Rust decoder. | Consumed as the **only** grammar, through that one decoder in CompatV2 mode; no v1 path, no added kind. Windows-specific, set by the #540 wire owner's decision (§10 Q1, RESOLVED, [#1307 comment 6532691875](https://github.com/gyldlab/keld/pull/1307#issuecomment-6532691875)): WebMessage string carriage; `bound` answers a verified `bind` (the landed macOS v1 request→response order); the `document` nonce is page-readable on route D; `send_records: 1` kept. The Windows carriage/handshake vectors are appended to #1122 §7.1, not a parallel owner (§7). |
| `crates/keld-wv/src/webview2/mod.rs`, `guarded_navigation.rs` at `457a9c3b` | `create` ordering (`mod.rs:3521-3632`); `GuardInstalled` mint (`guarded_navigation.rs:50-154`); deny-all `NewWindowRequested` (`:142-152`); `navigate_initial` (`:161-172`); `NavigationStarting` observer (`mod.rs:2278-2297`); empty `AdditionalBrowserArguments` (`mod.rs:1432-1441`); devtools `cfg!(debug_assertions)` (`mod.rs:3583`); media principal with generation `0` (arch 03 §4.3). | Extended inside the same mint; one generation source feeds media and channel principals (§4.5). |
| `crates/keld-guard/src/lib.rs:136-146, 625-628, 784` | `Principal::Webview { id, generation }`; `PermissionsManifest` ignores `windows`; `evaluate` + the vetted scope matcher (`Scope::pattern`, `lib.rs:218`). | Consumed; non-`file:` `web.navigation.allow` entries use that matcher's grammar (`file:` allow forms are W3 rule A″'s, §4.6). That matcher (`path_in_scope`, `lib.rs:1276-1293`) is a byte comparison — no percent-decoding, no case folding (arch 03 §2; `lib.rs:778`) — and is reachable only through `evaluate`, which refuses webview principals (`KELD-GUARD006`); so it is **not** the `file:` owner (§2 deviation above). |
| `keld-guard` `file:` URL → root-relative components parser `file_url_root_relative` (W3 #1185 spec, PR #1311 at `1db53482` (r14, open draft), §4.4), built on the guard-owned Windows package grammar (`validate_windows_package_component`, `lib.rs:951`; ordinal case-insensitive comparison as in `validate_windows_package_paths`, `lib.rs:975`) and, for the Host-admitted root, `validate_fs_component` (`lib.rs:933-938`) | One percent-decode; empty authority only; `.`/`..` refused and interior empty segments collapsed (R6); drive and leading components equal to the admitted root under ordinal ignore-case (R7), the root spelled once from the retained handle's final long path; every remaining component through the package grammar and NFC (R8); a relative path over 100 bytes (R9) or a `.lnk` leaf (R10) refused; query and fragment cut before component matching; the raw query is returned for the allow match (byte-exact, §4.6 rule 3). | **The one owner** of the `file:` path-inside-root decision for W3 (resource serving), W1's per-call Source rule (§4.4 step 3) and rule A″'s `file:` navigation membership (§4.6). The same owner exports the **one component equality** (per-component NFC + `CompareStringOrdinal` ignore-case, the R7 class): W3's allow-form matching, Host-attempt attribution and every W1 comparison of components call it (exported as `keld_guard::components_equal`, W3 §4.6 rule 2 at `1db53482`). W1 adds no path matcher or equality of its own. Until W3's spec is approved this is a dependency (§10 B5, B6). |
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

Each negative control changes exactly one rule; a row that lists several runs them as
independent single-fault mutants, each in its own process. "Zero effects" = independent
counters on the app-link writer, the Bun handler and the Host audit log read 0.

Three standing conditions apply to every row (r10 sweep). (1) T2's bridge is floor-only (§6):
in T2 every call a row calls granted is the KEL-142 floor `invoke` on Echo channel `1`,
"reaches Bun" means one Echo handler entry, and the `el:` forms of the same cells run from
T3/T4 on. (2) W3 cancels every page-initiated top-level `file:` navigation at +0 until its Q27
passes (B19, #1311 `1db53482` §4.6 rule 2) and admits a Host `file:` attempt only in its
header-proven mode (Q22 (a), B18): a `file:` document in these rows is loaded by a Host
attempt (the application's `loadURL`) unless the row says otherwise, and a cell that needs a
page-initiated `file:` commit reads blocked, never pass, until W3 Q27. (3) Every Host test hook
named here (a replayed message, a stamped post, a reply latch's release signal, a held `bound`
post, a process termination) is a debug-only `profile-test-hooks` fixture (`keld-wv` `Cargo.toml` features;
`src/lib.rs:3-4` refuses that feature in release builds), the class W2 D16 uses; no new hook
class.

| ID | Given / when | Pass observable | Negative control (one fault → RED) |
| --- | --- | --- | --- |
| AC01 First-inline floor | A compat window navigates to the qualified application entry | The first inline page script observes `window.keld.invoke` and the preload's exposed keys, frozen, in the top document (T2 checks `window.keld.invoke`; the exposed keys need T4's wrapper and are checked there). The first-inline controls for equivalent spellings (`index%2Ehtml`, `INDEX.HTML`, a non-ASCII case variant), a non-entry in-root page, a missing in-root file and an in-root junction (AC25 (f)/(h)) and a remote document are AC25. | Install the facade from `DOMContentLoaded` via `ExecuteScript` instead of the document-created registration → first inline sees nothing → RED. (Whether awaiting the registration completion before `Navigate` is necessary is UNKNOWN, §10 Q5; it is not an AC.) |
| AC02 Bridge is not a boundary | A hostile page poisons `JSON`, `Array.prototype`, `chrome.webview.postMessage`, redefines the exposed key (once T4's facade exists), then calls granted and ungranted channels, each raw `send` posted after the previous one's `send-result`, so #540's one `send` record never refuses it first | Host admits or refuses exactly as for a clean page; the page gains no channel it lacked. | Remove the Host literal-grant step, keep the bridge → ungranted channel reaches Bun → RED (tracker negative control (c), AC22). |
| AC03 Top-level event only | An iframe whose `src` is **the verified application entry URL itself** (the same URL, inside the package root) receives the current nonce from the top document's page script by `window.postMessage` and posts a well-formed v2 `invoke` on Echo channel `1` through its **own** `chrome.webview` (reading `top` directly is INFERRED to throw `SecurityError` between `file:` documents, whose origins Chromium is INFERRED to make opaque; the receipt records it) | Every per-call rule except the event rule is satisfied; it arrives only on `ICoreWebView2Frame2::WebMessageReceived`, which this slice does not register; zero effects. A control: the top document posts the same envelope and it is admitted. | Register the admission handler on `Frame2` too → the iframe post reaches the Echo handler → RED (only the event rule stood between it and the effect). |
| AC04 Eligibility before epoch | `ContentLoading` fires for (a) the qualified application document (the `file:` entry, loaded by a Host attempt), (b) an allowlisted-navigation remote document, (c) an error page at an allowlisted URL (`IsErrorPage == TRUE`), (d) the `about:srcdoc` error page | Only (a) receives an epoch; (b)–(d) enter FailedClosed/`channels: []` with no epoch and every call from them refused. | Each mutant removes one term of the §4.5 mint predicate and is decided by the pure-Rust `wv2_epoch_mint_predicate` (§7), which feeds an otherwise-qualifying input: skip the `IsErrorPage` check → an error page with a status-200 record receives an epoch → RED. Separately, qualify by URL-text equality instead of #1192 §4.4 resource identity → a same-text input with no verified identity receives an epoch → RED. Separately, skip the `NavigationId` match → an input whose `NavigationId` no allowed `NavigationStarting` recorded receives an epoch → RED. On device no cell isolates one term (rule not exercised there; the device cells are defence in depth): for a `file:` document W3's served-record credit already requires an allowed, nonce-attributed `NavigationId`, `served.status == 200` and `!IsErrorPage` (#1311 `1db53482` §4.4), and (b)–(d) have no verified identity at all. |
| AC05 Source rule (per call) | A message carrying the **current, valid epoch** (stolen by a same-process script) arrives with a `Source` that (a) has a scheme other than the committed document's: `about:srcdoc` (the error page — E5 `p1-exec-b.json` 5/5: its `Source` **is** the string `about:srcdoc` and the bridge ran there), `about:blank`, `data:`, `blob:null/…` or `chrome-error://…`; (b) for an `https` application, a different tuple origin (`https://evil.example/…`); (c) for a `file:` application, an empty host but a path **outside** the verified package root (crafted `file:///…/outside/x.html` document that obtained the nonce); (d) for a `file:` application, a non-empty host: UNC `file://server/share/…` on device; `file://localhost/…` as a pure-Rust vector only (Chromium likely strips `localhost`, so it may be unreachable on device) | In the pure-Rust golden vectors against the one parser, each fed the current epoch on the top-level event, each case is refused by the Source rule alone; zero effects. Controls admitted: the committed document's in-root path; a mixed-case or percent-encoded spelling of an in-root path (`file:///D:/APP/INDEX.HTML`, `…/index%2Ehtml` — one identity class under the parser); and, for an admitted root `D:/Keld Test/Ünï/`, Chromium's canonical `file:///D:/Keld%20Test/%C3%9Cn%C3%AF/index.html`; on device each control is a document the Host loaded at that spelling, calling with its own current epoch. On device the negative cases cannot isolate the rule: a top-level document other than the qualified one has had the epoch revoked by its own `ContentLoading` (§4.5), so the `about:srcdoc` error page and an allowlisted remote document are refused earlier (`NoDocument`, defence in depth), and out-of-root, UNC and `about:blank` documents cannot commit under rule A″ and W3's serving; the cross-document race in which a foreign `Source` could carry the current epoch is §10 B7's. These controls test the per-call rule alone; which documents get the preload and an epoch is AC25. | Replace `file_url_root_relative` with a comparison of serialised origins (`"null" == "null"`), keeping the `scheme == file` half → cases (c) and (d) are admitted → RED (the scheme half still refuses (a)). Separately, replace it with a comparison of `Url::origin()` values → every `file:` control is refused, because `file:` origins are opaque and unique (`url` 2.5.8, the workspace's locked version: `origin.rs` L33, doc-test `lib.rs` L852–866) → RED. Separately, keep only the `scheme == file` half of the rule → case (c) is admitted → RED. Separately, compare components case-sensitively → the mixed-case control is refused → RED. Separately, decide membership with the arch 03 §2 byte matcher (`path_in_scope`) against the root's spelling → the percent-encoded non-ASCII control is refused → RED. Exactly one rule decides each case. |
| AC18 Host-identifiable load | The application is loaded with `NavigateToString` (today's path, `mod.rs:3654`) instead of the verified URL | `ContentLoading` reports `Source = about:blank` (source probe 3/3); the eligibility rule mints **no** epoch, and the bridge's `bind` from that document is refused with zero buffered work, zero nonce and zero authority (Host `bound`-post counter 0); every bridge call from that document is refused not-ready, and §4.5's "rule not satisfied" row FailedCloses it and applies `onLoss`, which this fixture sets to `close` (the window closes with a typed error; the startup deadline of §4.4 starts only when a document qualifies, so this fixture ends through `onLoss`, not that deadline). Loaded from the verified URL, the same application receives an epoch. | Qualify an `about:blank` document because the Host "knows" it just loaded the app string → the string-loaded document's `bind` is answered with a `bound` (Host `bound`-post counter 1) → RED. Why it matters: an attacker-reached `about:blank` (E5 trigger a control, measured with the veto off) would be indistinguishable from it; rule A″ cancels that navigation on this fixture, so it is not the observable. |
| AC06 Epoch nonce | A message from the previous document A, replayed by the Host test hook after same-origin document A2's `ContentLoading` (A2 is the application's `loadURL` of the entry, a Host attempt: W3 refuses a `file:` `reload()` until its Q22 (c) and cancels a page reload under B19); or a nonce not byte-equal to the current one | Refused; zero effects; the new document receives its own epoch only in the `bound` that answers its verified `bind` (AC29). E5's 24/24 per variant (12 trials × 2 documents) measured the earlier push at `ContentLoading`: feasibility only, not evidence for that order. Natural late delivery is a separate gate (§10 B7). | Admit by `Source` + generation only → the same-origin A→A2 replay is admitted → RED (tracker negative control (d), AC22). |
| AC07 Not-ready refusal | A page calls an exposed method (T4) or `window.keld.invoke` (T2) before the document's epoch arrived (the AC26 recorder confirms each counted call preceded `bound`'s arrival; the at-risk count must be above 0) | Void APIs return `undefined` and emit one typed local diagnostic (#1154 §4.7's policy, renderer-local on route D, divergence 11; the bridge's typed local error never reaches the page as a throw), `invoke` rejects with the typed not-ready error, **nothing is queued or replayed**; a call after the epoch succeeds. | Buffer and flush on epoch → the early call produces a late Bun entry → RED. |
| AC08 Renderer-side epoch check (**correctness guard, not a security control**) | A test hook posts a reply/event stamped with epoch e₁ through `PostWebMessageAsString` after document B (e₂) committed, bypassing §4.4 rule 1, which would otherwise drop it (AC21), as the E5 variant harness did | The bridge drops it (and drops anything received before its own epoch); B's facade listeners never fire. This keeps stale replies away from B's facade callbacks; it protects nothing from a hostile page, whose own raw `chrome.webview` `message` listener sees every post (AC21 is the security control). | Remove the bridge's stamp check → the stale post is delivered to B's facade listener → RED. |
| AC09 Literal grant | Current document, matching epoch, channel `c` without `el:c` in `windows.<w>.channels`, on a route-D window | Exactly #1191 AC5, unchanged for route D: `DenyReason::ChannelForbidden`, code `KELD-GUARD003`, kind `channel_forbidden`, fix text "Add `el:c` to this principal's channels list in keld.permissions.jsonc." and the unchanged `CallError { code, message }`, whose bytes equal `keld-guard`'s rendering for the same channel (#1191 `4f246a75` `docs/specs/gh552-exact-channel-grants-guard.md:48` (AC5) and `:140`; `keld-guard` `src/lib.rs:392-394`, asserted at `tests/acl.rs:562-574`, at `457a9c3b`); zero app-link writes and zero Bun entries; a granted neighbour produces one Bun entry. Separately (T5), `keld migrate`'s report for that window says its grants are generated and that changing them means re-running `keld migrate`, not hand-editing `windows.<w>.channels` (§4.8 rule 3). | Prefix/case-fold match → `el:cX`/`EL:c` admitted → RED (#1191 `G552-P04-literal`). Separately, put the route-D guidance (`re-run keld migrate`) into the `KELD-GUARD003` fix text → the `CallError` bytes differ from `keld-guard`'s → RED. Separately, drop the regeneration line from the migrate report → the report golden lacks it → RED. |
| AC10 Navigation policy (rule A″) | Each E5 trigger (a, c, d, e1, f2, i, j, k, l, m, n) to an `about:` form and to an off-allowlist `https:` URL; with an allowlist whose only `file:` form is the literal `index.html` with no query, a page navigation to another in-root `file:` document and a page link to `index.html?x=1`; with an allowlist whose only `file:` form is the literal `p.html?mode=a`, page links to `p.html?mode=A`, `p.html?` and `p.html` (empty and absent query differ); and, **not yet run on device**, top-level navigations to `data:`, `blob:` and `javascript:` URIs in lower and upper case | Every page-initiated top-level `NavigationStarting` whose target is off the window's allowlist under rule A″ (for `file:`, decided on the parser's components with keld-guard's one component equality, and on the query: a parser failure, or a target that matches no `file:` allow form, is off-allowlist) has `Cancel` set at generation +0, and the application document and its IPC survive (E5: 5/5 veto per arm, 0/3 control, per trigger, including uppercase `ABOUT:BLANK`). On-allowlist navigations proceed (+1 only at commit); a page-initiated `file:` target is off-allowlist under W3 B19 until W3 Q27, so its on-allowlist cells read blocked until then. For `javascript:`, whether WebView2 raises `NavigationStarting` at all is UNKNOWN; if it does not, the script runs in the current document with that document's authority only, and the T2 receipt records which. | Replace the allowlist with an exact denylist `["about:blank"]` → triggers k (`about:blank?`), m (`about:blank#keld`) and n (`about:blank#blocked`) replace the document → RED. Separately, treat in-root membership as sufficient (proceed whenever `file_url_root_relative` succeeds) → the in-root non-entry navigation proceeds under that literal-only allowlist → RED. Separately, cut the query before page-initiated matching → the entry-with-query link replaces the document → RED. Separately, fold the query through the component equality → the `p.html?mode=A` link proceeds → RED. The three `file:` mutants are device-level rule not exercised while W3 B19 cancels every page-initiated `file:` target first; the same single faults run in W3's pure `guard_navigation_allow_forms` (#1311 `1db53482` §7, "Independent of B19's device-level cancel"), the entrypoint T2 consumes; the `about:` mutant flips on device. |
| AC11 Runtime flag untouched | Any build | `AdditionalBrowserArguments` stays empty; no `--disable-features` naming `msWebView2CancellableAboutNavigations`; cancellable `about:` navigation is never disabled. | Add the flag → AC10's `about:blank` control replaces the document → RED. |
| AC12 Popups | `window.open` with `_blank`/`target`, `opener` forms | `NewWindowRequested` → `SetHandled(TRUE)` (existing `guarded_navigation.rs:142-152`); no popup exists (E5 f1/f2: handled, 0 windows). | Drop the handler → unaccounted popup → RED. |
| AC13 Generated grants | `keld migrate` sees `ipcMain.on('a')`, `ipcMain.handle('b')`, `ipcMain.once('c')`, `ipcMain.handleOnce('d')`, `ipcMain.on(dyn)` and window W's preload exposes only the fixed-channel, fully forwarding method F-B1 (`doA: (...xs) => send('a', ...xs)`) | `windows.W.channels == ["el:a"]`, with `"$keldWindowChannelGrants": 1` and `windows.W.preload.digest`; `b`, `c`, `d` and `dyn` are reported ungranted. For the page-chosen-channel pass-through F-B (`send(c, ...xs)`) the result is `["el:a","el:c"]`: `send` reaches only `on`/`once` registrations, and `b`, `d` are reported ungranted as reachable only through `ipcRenderer.invoke`, which route D does not support (§4.8, r9). A flagged method on the same channel that omits arguments (`doA: () => send('a')`) with a matching attestation yields the same `["el:a"]`, never more. With `ipcMain.on('app:x')` added, an attested F-P (`prefixed: (c, ...xs) => send('app:' + c, ...xs)`) yields exactly `["el:app:x"]`. | Grant every `ipcMain` literal regardless of the preload's reachable sinks → `el:b` appears for W → RED (over-grant). Separately, collect only `on`/`handle` registrations → F-B's result lacks `el:c` and that `once` handler is unreachable → RED (under-grant; draw.io registers ten channels with `ipcMain.once`). Separately, treat F-P's computed channel as page-chosen (`*`) → its window is granted every `on`/`once` literal, `el:a` included → RED (over-grant past the recorded prefix). **Window keys (r7):** a fixture with two sites `new BrowserWindow({ title: 'Main', webPreferences: { preload: A } })` and `… preload: B …` must make `keld migrate` fail with the typed duplicate-key diagnostic naming both sites and write no `keld.config.ts`; a third fixture whose two `Main` sites share preload, navigation and CSP must produce one `t-main` entry listing both sites. Further refusal fixtures: titles `'Main'` and `'main'` with different preloads; a title whose slug is empty (`'!!'`); a computed title (`title: app.name`). Golden vectors for the key function run through `@keld/electron` and through `keld migrate`'s call of it. Mutants: disambiguate by suffix (`t-main`, `t-main-2`) → swapping the two sites in source and regenerating moves A's digest and grants to B's window (the config diff assigns `t-main` B's digest) → RED; keep the last site → `keld migrate` writes a configuration (the test expects refusal) whose one `t-main` entry carries a single site's digest → RED; drop the class prefixes → a site titled `'a js'` with preload `b.js` and a title-less site with preload `a.js` both map to `a-js`, and `keld migrate` refuses a fixture the test expects to yield two entries (`t-a-js`, `p-a-js`) → RED. **Sinks (r8):** with `ipcMain.on('danger')`, a preload whose only exposure is `(...xs) => ipcRenderer.sendToHost('danger', ...xs)` yields no `el:danger` and a typed incompatibility line (its run-time half, zero Bun calls when the page calls it, is AC27's); the same for a `sendSync`-only and an `invoke`-only exposure while #1154 does not implement them. Mutant: count `sendToHost` as a main-IPC sink → `el:danger` is granted → RED. **Dispatch kinds (r9, 0monish 4425538678):** with `ipcMain.on('a')` and only `ipcMain.handle('danger')`, a preload exposing `(c, ...xs) => ipcRenderer.send(c, ...xs)` yields `["el:a"]` with no `el:danger` and a report line naming `danger` as `invoke`-only. AC13 checks the generated manifest and report only; the run-time refusal (`KELD-GUARD003`) and the per-kind handler counters are AC27's. Mutant: treat every `ipcMain` registration kind as reachable by `send` → `el:danger` is granted → RED. |
| AC14 Digest binding | The admitted preload bytes' SHA-256 differs from `windows.W.preload.digest` | The Host treats W as `channels: []` and fails the window with a typed error naming the fix (`re-run keld migrate`); a matching digest loads the grants. | Ignore the digest → grants produced for another preload apply → RED (tracker negative control (b), AC22). |
| AC15 Classifier gate | `keld migrate` over fixtures F-A..F-Q (§4.8). Every single-rule fixture forwards all other arguments by rest/spread, so exactly one rule can flag it | F-B, F-B1, F-B2 and F-O not flagged; every other fixture flagged, and the recorded `verdict.flags` names **only** the rule its row names; F-G1 flagged although representation-only (conservative; branch and delete); F-G (the combined draw.io shape) carries three flags. | Each mutant removes one rule: drop the page-input data-flow edge → F-A passes → RED (F-D stays flagged, because its first argument is not forwarded by identity, and F-G1 by its delete); accept any argument expression as forwarded by identity → F-D passes → RED; treat a page-derived channel as pass-through → F-P passes → RED; treat omitted or truncated arguments as pass-through (the inverted former mutant) → F-C, F-J pass → RED; ignore constant arguments → F-H passes → RED; ignore writes, deletes and mutating calls on page-input values → F-I, F-Q pass → RED; ignore inbound values (rule (b)) → F-F, F-K, F-L pass → RED; flag only non-literal `process.argv` reads → F-M passes → RED; treat dynamic `require`/`eval` as resolvable → F-E passes → RED; let the analyzer "prove" F-G1 representation-only by pattern → F-G1 passes silently → RED (conservatism falsifier); F-G stays flagged under every single mutant. Over-flag falsifier: flag every outbound sink call → F-B and F-B1 flagged → RED (F-B2 makes no outbound sink call). |
| AC19 Verdict and attestation binding | Window W's `preload.digest` is d₁ and the recorded `verdict.flags` is non-empty; the manifest holds (i) no attestation, (ii) an attestation bound to d₂ ≠ d₁, (iii) an attestation bound to d₁; separately, each with (iii)'s matching attestation so that one fault decides, (iv) `preload.verdict` is absent, (v) `channels` lists `el:x` not in `verdict.reachableSinks`, (vi) `verdict.digest` is d₀ ≠ d₁ (the developer edited the preload and hand-updated `preload.digest` and the attestation without re-running `keld migrate`), and (vii) `preload.digest == verdict.digest == d₁` but the recorded flags or sinks are stale (hand-edited, or produced by an older classifier) | (i), (ii), (iv), (vi): the Host loads W with `channels: []` and a typed error whose fix says re-run `keld migrate` (and, for (i)/(ii), attest digest d₁); (iii): W loads with exactly the §4.8 **Grants** intersection; (v): W's grants are rejected (`KELD-GUARD005`-class), not filtered; (vii): a release build re-runs the classifier on the admitted bytes, finds a different verdict and refuses to package W with a typed error. | Accept an attestation by file path or window key instead of digest → case (ii) loads → RED. Separately, skip the ⊆-reachable-sinks check → case (v) grants `el:x` → RED. Separately, treat a missing verdict as "no flags" → case (iv) passes the verdict-presence check, counted there (its typed re-run-`keld migrate` refusal is absent; with an empty default `reachableSinks` the ⊆ check rejects the grants instead) → RED. Separately, skip the `verdict.digest == preload.digest` check → case (vi) loads stale grants → RED. Separately, skip the release recompute → case (vii) is packaged → RED. |
| AC16 Same-document stability | `pushState` to `?route=2#frag`, `replaceState` to `?route=3`, a hash change (`SourceChanged` with `IsNewDocument == FALSE`, source probe v2 3/3), cancelled provisional navigation | Epoch and generation unchanged; a pending granted call completes once; a new granted call after each change is admitted (its `Source` now carries the new query/fragment). | Rotate the epoch on `SourceChanged(IsNewDocument == FALSE)` → the pending call is refused → RED. |
| AC20 Identity is decided at commit, not per Source string | After `pushState` to `?route=2#frag` the page makes a granted call; its `Source` is `file:///…/index.html?route=2#frag` (source probe v2) — and the first call posted immediately after `pushState` may still carry the pre-push `Source` (`SourceChanged` arrived after it, 3/3) | Both calls are admitted: the epoch binds the committed document; the per-call Source rule checks the scheme, then the tuple origin (`https`) or empty host plus in-root path through the one parser (`file:`), never query/fragment equality. | Compare the full `Source` URL to the committed URL → the post-`pushState` call is refused → RED. |
| AC21 Host→renderer generation check (security control) | Document A's granted `invoke` is admitted; the Bun handler's reply is held until B's `ContentLoading` has been processed (deterministic test gate); B's first inline script installs a raw `chrome.webview` `message` listener that counts every post | The Host drops the reply because the target generation moved (§4.4 Host→renderer rule): B's raw-listener counter for posts stamped e₁ reads 0 and the Host's post counter for e₁ after B's `ContentLoading` reads 0. A reply whose target is still current is delivered once. | Remove the Host-side generation check → B's raw listener receives A's reply (counter ≥ 1) → RED (tracker negative control (e), AC22). The natural race between a post and a concurrent commit is not deterministic and is probed under §10 B7, arm H. |
| AC22 Route-D premise mutants (the tracker negative control) | One hostile page, five single-fault Host or `keld migrate` builds, each in its own process: (a) the classifier treats omitted arguments as pass-through; (b) the Host ignores the preload digest; (c) the literal-grant check is removed; (d) admission uses `Source` + generation without the nonce; (e) the Host-side generation check on Host→renderer delivery is removed | The unmodified baseline refuses every attempt with zero effects. Each forbidden effect is counted independently of the Host's refusal log: a Bun-side handler-entry counter keyed by channel and argument list for (a)–(d), and the page's raw `message` listener counter for (e). | (a) baseline (with `ipcMain.on('open-file')` registered): F-C is flagged, no attestation exists, W has `channels: []`; mutant: F-C passes unflagged, W is granted `el:open-file`, and the page posts a raw v2 `send` on `open-file` carrying `'<page path>'` directly → an `on('open-file')` listener entry with an argument the preload never sends → RED; (b) = AC14's mutant; (c) = AC02's mutant (the literal-grant step removed; AC09's prefix/case-fold mutant is a different fault and stays in AC09); (d) = AC06's mutant (A→A2 replay reaches Bun); (e) = AC21's mutant. AC03 (top-level event), AC05 (Source rule) and AC19 (grants ⊆ reachable sinks) carry the remaining premise mutants. The bridge's epoch check (AC08) is not one of them. |
| AC23 Isolation is never reported | (a) A fixture preload records `process.contextIsolated` and calls `contextBridge.exposeInMainWorld('probe', { isolated: process.contextIsolated })`; (b) `keld migrate` runs over F-O (§4.8), which exposes a privileged API only when `process.contextIsolated` is truthy and a limited one otherwise | (a) The first inline page script reads `window.probe.isolated === false`; the exposure succeeded (route-D divergence: in Electron, `exposeInMainWorld` throws when `contextIsolated` is false — §4.2). (b) `verdict.reachableSinks` is `["el:pub"]`: `process.contextIsolated` is constant-folded to `false`, so only the branch the application actually takes is counted; the report lists every preload read of `process.contextIsolated`, and an aliased or computed read is flagged under rule 2(d). | (a) Report `true` (the former Q3 recommendation) → the probe reads `true` → RED. (b) Count sinks from both branches → `verdict.reachableSinks` gains `el:priv` (and, with `ipcMain.on('priv')` registered, `el:priv` is granted) although the application chose the limited API → RED. |
| AC24 In-flight hold (state machine) | Pure Rust test of the Host→renderer delivery state machine with a stub engine (its `Navigate` record counts every Host navigation call, W3's header navigation included; the stub clock reaches `NAV_TERMINAL_BOUND` only in cases (10), (12) and (13)); cases (14) and (15) also drive W2's real barrier (`ProfileReleaseWait`; its `webview2` module is Windows-only, `keld-wv/src/lib.rs:104-105`): (1) `NavigationStarting(n)` → reply due → `ContentLoading(n)`; (2) `NavigationStarting(n)` with `Cancel` → reply due → `ContentLoading(n)` (E5 trigger b shape) → `NavigationCompleted(n, IsSuccess = FALSE)`; (3) `NavigationStarting(n)` → reply due → `NavigationCompleted(n, IsSuccess = FALSE)` with no `ContentLoading(n)`; (4) `NavigationStarting(n₁)` → `NavigationStarting(n₂)` → reply due → `NavigationCompleted(n₁, IsSuccess = FALSE)` → `ContentLoading(n₂)`, then a reply due for the new epoch; (5) `NavigationStarting(n)` → reply due → `SourceChanged(IsNewDocument = TRUE)` → `NavigationCompleted(n, IsSuccess = TRUE)` with no `ContentLoading(n)`; (6) `NavigationStarting(n)` → reply due → `SourceChanged(IsNewDocument = FALSE)` (an unrelated `pushState`) → `NavigationCompleted(n, IsSuccess = TRUE)` with no `ContentLoading(n)`; (7) `NavigationStarting(n)` → reply due → `SourceChanged(IsNewDocument = TRUE)` → `SourceChanged(IsNewDocument = FALSE)` → `NavigationCompleted(n, IsSuccess = TRUE)` with no `ContentLoading(n)`; (8) `NavigationStarting(n)` → reply due → `NavigationCompleted(n, IsSuccess = TRUE)` with no `SourceChanged` and no `ContentLoading(n)` (a same-URL restore shape); (9) `NavigationStarting(n₁)` → `NavigationStarting(n₂)` → reply due → `SourceChanged(IsNewDocument = TRUE)` → `NavigationCompleted(n₁, IsSuccess = FALSE)` → `ContentLoading(n₂)`; (10) `NavigationStarting(n)` → reply due → no terminal event until `NAV_TERMINAL_BOUND` (stub clock) → `NavigationStarting(n₂)` → `ContentLoading(n₂)` → reply due for the new epoch; (11) `NavigationStarting(n)` → reply due → `ProcessFailed(RENDER_PROCESS_EXITED)` → `NavigationStarting(n₂)` → `ContentLoading(n₂)` → reply due; (12) `NavigationStarting(n₁)` → `NavigationStarting(n₂)` → `ContentLoading(n₂)` → reply due for the new epoch → no terminal event for `n₁` until the bound; (13) `NavigationStarting(n)` → bound → `NavigationStarting(n₂)` → `ContentLoading(n₂)` → reply due → late `ContentLoading(n)` → a second reply due for n₂'s epoch; (14) `NavigationStarting(n)` → reply due → `ProcessFailed(BROWSER_PROCESS_EXITED)`; (15) `NavigationStarting(n)` → reply due → `ProcessFailed(BROWSER_PROCESS_EXITED)` for the bound browser → a `CreateWindow` (once the fault is latched W2 answers it `WindowBuildFailed { Webview }` without a build; under the receipt-only mutant below it builds and its controller reports a new browser pid) → late abnormal `BrowserProcessExited` receipt for the old pid | (1), (2), (4): the stub engine receives **no** post for the old epoch; the old epoch is revoked; in (4) the reply for the new epoch is delivered exactly once (liveness). (3), (6): the held reply is released once to the unchanged document, after the pending set is empty. (5), (7), (8): treated as a document change — revoke, FailedClosed, `onLoss`; no held reply released. (9): `n₁`'s completion is **not** a document change (the signal is attributed to `n₂`); no FailedClosed or `onLoss` from `n₁`; `ContentLoading(n₂)` revokes the old epoch; the stub receives no old-epoch post. (10): at the bound the stub records `Stop()`, the old epoch is revoked, the held reply discarded (0 stale posts), the document FailedClosed with `onLoss` count 0; after `ContentLoading(n₂)` the new epoch's reply is delivered exactly once. (11): at `ProcessFailed` no `Stop()` is recorded; the old epoch is revoked, the held reply discarded (0 stale posts) and the document FailedClosed with `onLoss` count 1; after `ContentLoading(n₂)` the new epoch's reply is delivered exactly once. (12): 0 posts to the old epoch; at the bound the new epoch is revoked, its held reply discarded and the document FailedClosed with `onLoss` count 0 — the stated cost of §4.4 rule 2 (iii). (13): the first reply is delivered once; at the late `ContentLoading(n)` the new epoch is revoked, and the second reply, stamped with it, is never posted. (14): no `Stop()`, `Navigate` or `Reload` is recorded; the epoch is revoked, the held reply discarded (0 stale posts), `onLoss` count 0, and the barrier latches `AbnormalExit` at the first `poll` after the `ProcessFailed`, whose handler queues an abnormal receipt (#1280 r16 D8 (`be9e9e56`), its AC21 NC 4 (j)), not at the later `BrowserProcessExited` receipt: session end = 1. (15): session end = 1 from the `ProcessFailed`; the barrier's binding never moves to the new pid (no rebind; the `CreateWindow`'s answer is W2's); the late old-pid receipt changes nothing; 0 posts. | Remove the hold → the stub receives the old-epoch post between `NavigationStarting` and `ContentLoading` in (1) → RED. Separately, start the hold only for navigations the policy allowed → (2) emits the post → RED. Separately, end the hold at the first completion → (4) emits the old-epoch post after `NavigationCompleted(n₁)` → RED. Separately, release held posts on any completion without `ContentLoading` → (5) emits the post → RED. Separately, release whenever a `SourceChanged(IsNewDocument = FALSE)` arrived, ignoring `IsNewDocument = TRUE` (the r4 rule) → (7) releases the held reply to the new document → RED. Separately, release unless a `SourceChanged(IsNewDocument = TRUE)` arrived (the r5 rule) → (8) releases the held reply on a completion that raised no `SourceChanged` → RED. Separately, count every new-document signal since `NavigationStarting(n₁)` regardless of attribution (the r6 rule) → (9) raises FailedClosed and `onLoss` for a window whose successor commits → RED. Separately, remove the bound → (10) keeps the set non-empty after the stub clock passes `NAV_TERMINAL_BOUND` and the new epoch's reply is never delivered → RED. Separately, ignore `ProcessFailed` → (11) the same → RED. Separately, apply `onLoss` at the bound → (10)'s `onLoss` count is 1 → RED. Separately, let `ContentLoading(n₂)` end older pending navigations (supersession) → (12) delivers the new epoch's reply before the bound → RED. Separately, drop the expired `NavigationId` at the bound and ignore a `ContentLoading` for a navigation not pending → (13) posts the second reply, stamped with the new epoch, after the late `ContentLoading(n)` → RED. Separately, treat `BROWSER_PROCESS_EXITED` like `RENDER_PROCESS_EXITED` (the r9 rule) → (14) records a `Navigate` on the closed controller and `onLoss` count 1 → RED. Separately, end the session only on a pid-matched abnormal `BrowserProcessExited` receipt (the #1280 `95594e88` D8 (3) rule; a fault in W2's barrier, whose own control is #1280 r16 AC21 NC 4 (j), run here as an integration check) → in (15) the `CreateWindow` builds and rebinds to the new pid, the old receipt is foreign and session end stays 0 → RED. |
| AC25 Which documents get the app preload and an epoch | Admitted root `D:/Keld Test/Ünï/`, entry `index.html`, allowlist `in-root:**` plus one remote `https://` scope. The application's `loadURL` (a Host attempt, admissible for the entry's components whatever the spelling, §4.6 item 6) loads (a) the entry `file:///D:/Keld%20Test/%C3%9Cn%C3%AF/index.html`, (b) `…/index%2Ehtml`, (c) `…/INDEX.HTML` and (d) the non-ASCII case variant `file:///D:/Keld%20Test/%C3%BCn%C3%AF/index.html`; the page then navigates top-level to (e) another in-root page `…/page2.html`, (f) a missing in-root file (Host `404`), (g) the allowlisted remote document and (h) an in-root junction (Host `403`). (e), (f) and (h) are page-initiated `file:` targets, which W3 cancels at +0 until its Q27 (B19): those cells read blocked, never pass, until then. Each document's first inline script records whether the preload's exposed keys exist (AC01's oracle) | Each cell records the committed `Uri`/`Source` bytes, `location.protocol`, the first-inline result and the Host's `bound`-post counter. T2 runs the epoch, `onLoss` and pure-Rust cells; the preload-API cells need T4's wrapper and run there. (a)–(e): the preload API is present at the first inline script. (a)–(d) receive an epoch; (e) receives one under B3's proposal (§4.5), and under B3's fallback it has the API, no epoch, every call refused not-ready and no `onLoss`. (g): the API is absent and no epoch is minted and no `bound` is posted. (f), (h): whether a Host `404`/`403` refusal commits as a Chromium error page or as an empty `file:` document is UNKNOWN (W3 Q7), so both outcomes are pre-declared and the receipt records which occurred: as an error page, the API is absent; as a `file:` document, the API is present (gate (b′)) and every call is refused not-ready. Under either outcome no epoch is minted and no `bound` is posted and the document is FailedClosed **without** `onLoss` (§4.5). Any epoch or `onLoss` in (f) or (h) is RED. In no cell does an epoch exist without the API. Pure Rust: keld-guard's one component equality holds for `index.html`/`INDEX.HTML`/`index%2Ehtml` (after the parser) and W3's allow-form matching and W1's Host-attempt admission call that same function. | Gate the preload by a `location` string (the r5 key: scheme + authority + path, ASCII-folded, against `ENTRY_KEYS`) → (d) lacks the API (and (b) too while Chromium keeps `%2E`) although it receives an epoch → RED (the defect rpipaliya reported). Separately, run the preload in every top-level document (option (b)) → (g) has the API → RED. Separately, compare components byte-exactly in W1 instead of calling the shared equality → the pure-Rust `INDEX.HTML` vector is unequal to the entry → RED. Separately, attach W3's served record regardless of its status → (f) receives an epoch → RED; while (f) is blocked on W3 Q27 this mutant is decided by `wv2_epoch_mint_predicate`'s status-`404`/`403` input (§7). |
| AC26 One wire grammar and carriage: #540 CompatV2 over WebMessage strings (r9, 0monish 4425538690; r11, §10 Q1(a); T2) | T2's **actual** injected bridge text (§4.3) runs in a qualified document against the **actual** `keld-wv` `decode_wv_link` in CompatV2 mode behind §4.4's admission path (top-level `WebMessageReceived`, `TryGetWebMessageAsString`, strict bounded UTF-16→UTF-8), with the KEL-142 floor (`window.keld.invoke` on Echo channel `1`). A recorder in the bridge logs its listener installation, every post and every inbound message in the renderer; the Host counts decoded envelopes, `bind` receipts, `bound` posts and Echo calls. Fixture vectors (WebMessage STRING literals, ASCII byte counts, no outer JSON string wrapper at the decoder; appended to #1122 §7.1, §7): `bind`, 21 bytes, `{"v":2,"kind":"bind"}`; `bound`, 174 bytes, `{"v":2,"kind":"bound","document":"0123456789abcdef0123456789abcdef","limits":{"payload_bytes":4096,"channel_utf8_bytes":256,"send_records":1,"event_records":1},"ready":false}` | Observed in the renderer and at the Host (the order's own cells are AC29): the listener is installed before the first post; the first post is byte-equal to the 21-byte `bind` vector; the first inbound message is the one `bound` the Host posted after decoding that `bind`, whose `document` is the current epoch (32 lowercase hex characters); in pure Rust the Host's `bound` encoder, given the fixture document and `ready: false`, emits the 174-byte `bound` vector byte for byte, and `decode_wv_link` decodes the `bind` vector to exactly one `Bind`. "`bound` may arrive before `bind`" is not a success oracle (§10 Q1(b)). one floor `invoke` posts one v2 `invoke`, receives one `invoke-result` and resolves; the next floor `invoke` is issued only after that `invoke-result`. Separately, raw page posts carrying the current epoch — `{"v":2,"kind":"hello",…}` (r8's kind name in a v2 envelope), a v1 `invoke` on channel `1`, a v2 `send` whose `payload` is a base64url string, a v2 envelope of kind `epoch`, the `bound` vector carrying the current `document` (wrong direction), a v2 `send` with a duplicated `document` field, and a non-string `chrome.webview.postMessage({"v":2,"kind":"invoke",…})` object (`TryGetWebMessageAsString` fails) — are each refused, by the carriage step or by `decode_wv_link`, with a typed reason; for them the decoded-envelope, `bind`-receipt and Echo counters stay 0, and a floor `invoke` after each still resolves. | Endpoint admits v1 (a LegacyV1 fallback) → the v1 `invoke` reaches the Echo handler → RED. Separately, the decoder also accepts a base64url string payload → the decoded-envelope counter counts that `send` → RED. Separately, the decoder admits an unknown kind → the v2 `epoch` post is counted as decoded → RED. Separately, the decoder aliases `hello` to `bind` → the v2 `hello` post is counted as a `bind` receipt → RED (the LegacyV1 mutant flips neither cell: both are v2 envelopes whose kinds lie outside #540's closed union, and a v1 decoder refuses unknown kinds too, `wv_link.rs:195`). Separately, run r8's v1 bridge text → `decode_wv_link` refuses every post and the floor `invoke` never resolves → RED (fails first). Separately, read `get_WebMessageAsJson` instead of `TryGetWebMessageAsString` → every string post reaches the decoder in its JSON-quoted form and is refused, so the bridge's `bind` is never answered and the floor `invoke` never resolves → RED. Separately, the decoder accepts native-to-renderer kinds from the renderer → the page-posted `bound` vector is counted as decoded → RED. Separately, the decoder lets a duplicated key overwrite the first → the duplicated-`document` `send` is counted as decoded → RED. |
| AC27 Facade send trace, dispatch kind and role capacity (r9; r11, §10 Q1(d); T4) | T2's bridge and admission path, T3's forwarder and T4's facade; `el:a` and `el:x` granted, `el:danger` not. Bun runs #466's `ipcMain` listener registry (`on`/`once`; #1122 §5) and #547's separate invoke-handler registry (`handle`; #466 puts `handle/handleOnce/removeHandler` out of scope as F04-T14, which #547 owns), with `ipcMain.on('a')` (its listener sends one event on `a` back), `ipcMain.on('x')` **and** `ipcMain.handle('x')`, and only `ipcMain.handle('danger')`; each listener and handler has its own counter. The page calls exposures that use `ipcRenderer.send`; the recorder of AC26 logs the renderer side. Fixture vectors (appended to #1122 §7.1, §7): `send`, 123 bytes, `{"v":2,"kind":"send","document":"0123456789abcdef0123456789abcdef","message":1,"channel":"rendererReq","payload":[0,1,255]}`; `consumed`, 120 bytes, `{"v":2,"kind":"consumed","document":"0123456789abcdef0123456789abcdef","delivery":1,"disposition":{"status":"consumed"}}`. Cross-window cell: two windows of one application on one authenticated Bun role, both granted `el:a`; `on('a')`'s completion for window 1's send is held behind a test latch, so its `send-result` is not yet posted | Pure Rust: the bridge's builders emit, and `decode_wv_link` decodes to exactly one variant, the 123-byte `send` and 120-byte `consumed` vectors byte for byte. Observed in the renderer, per direction only (#1122 AC8 promises admission order per document and direction, no cross-direction order). Outbound: `bind`, `send` (message 1, channel `a`), one `consumed` for delivery 1. Inbound: `bound` (answering the `bind`), then `send-result` for message 1 and `event` delivery 1 in either order. The `on('a')` listener fires once; the #540 ledger counter records the one `consumed`. A page `send('a')` made while message 1 is outstanding is a dropped call (divergence 11): it returns `undefined`, emits exactly one typed local `recordBusy` diagnostic, posts no v2 `send`, runs no Bun handler and consumes no credit; the next one, issued after message 1's `send-result` (`bound.limits.send_records` is 1), is posted as message 2. A `bound` with `ready: false` (the fixture holds the Bun route not ready behind a test latch when the document commits) makes calls refuse not-ready with no post until `route-state: ready`; `route-state: retired` makes them refuse not-ready again. `send('x')`: the `on('x')` counter is 1 and the `handle('x')` counter is 0. `send('danger')` from the exposure, and a raw v2 `send` for `danger` with the current epoch: both `KELD-GUARD003`, every `danger` counter 0. AC13's r8 exposures (`sendToHost`, `sendSync`-only, `invoke`-only) produce zero Bun calls. Cross-window: while window 1's message 1 is outstanding, window 2's bridge is locally free (its `outstanding` bit is false) and posts its `send('a')`; the Host/role ledger (#540 §4.8: one DATA record per direction across the role) refuses that reservation: the `on('a')` counter stays 1, the outcome is #540's private terminal/diagnostic for role-global exhaustion (#1122 §4.8, §4.10), and window 2's page sees no throw. | Merge #547's invoke-handler registry into #466's listener registry, so a `send` also reaches `handle` handlers (the case #466's registry-separation negative names) → the `handle('x')` counter is 1 → RED. Separately, the bridge skips its `outstanding` check → the early second `send('a')` is posted → RED. Separately, the bridge never clears its outstanding record on `send-result` → message 2 is never posted → RED. Separately, the bridge ignores `bound.ready` → a post appears before `route-state: ready` → RED. Separately, the bridge drops the `consumed` → the ledger counter stays 0 → RED. Separately, the Host keys the DATA ledger per document instead of per role (a per-window quota) → window 2's `send('a')` reaches `on('a')` while window 1's is outstanding (counter 2) → RED. |
| AC28 Process-failure split (r10, 0monish 4427055014; T2) | A compat window holds a current epoch and one floor `invoke` reply due (T2's floor bridge). (a) The harness terminates the window's main-frame renderer process; (b) in a separate run it terminates the browser process (the pid W2's D16 `REBIND` line names); both mechanisms are T2's `profile-test-hooks` fixtures. The harness records `ProcessFailedKind`, every navigation event, every engine call the Host makes per controller, the Echo handler counter and how the session ends | (a) `RENDER_PROCESS_EXITED`: the old epoch is revoked and 0 stale posts are issued; `onLoss: reload-entry` makes exactly one trusted load on the same surviving control (for `file:`, W3's header navigation, so only in W3's header-proven mode) and `onLoss` count is 1; a new `ContentLoading` mints a new epoch, which the new document receives in the `bound` answering its `bind` (AC29 (h)); a floor `invoke` after it reaches the Echo handler once (a real reload). The runtime replaces the crashed frame's content with an error page (Microsoft, §4.5); whether that raises navigation events, and in which order against the trusted load, is UNKNOWN, so the receipt records every navigation event between the `ProcessFailed` and the new epoch, and a second `onLoss` for one exit is RED. (b) `BROWSER_PROCESS_EXITED`: every epoch is revoked; after the failure there are zero `Navigate`, `Reload` or other engine calls on the closed controller; the session ends exactly once with W2's `AbnormalExit` terminal (`KELD-WV-009`). The order of `ProcessFailed` and `BrowserProcessExited` is not guaranteed, so the receipt records which came first: `ProcessFailed` first latches the terminal at the loop's next `poll` under #1280 r16 D8 (`be9e9e56`), the receipt first latches it as on main, and the later event changes nothing. Zero Echo handler entries follow (defence in depth: a dead browser delivers no post). | Apply `onLoss` after a browser exit (the r9 rule) → a Host navigation call on the closed controller is recorded → RED whenever `ProcessFailed` reaches W1 before the session ends; the runtime picks that order and no hook forces it, so this cell is a positive control, not the decider: AC24 case 14 decides the rule. Separately, treat a renderer exit as a browser exit → (a) records no trusted load and `onLoss` count 0 → RED (W2's latch reads the kind itself, so the session need not end). |
| AC29 Startup order: `bound` answers a verified `bind` (r11, §10 Q1(b); T2) | T2's actual bridge and admission path on the KEL-142 floor; the AC26 recorder plus Host counters for `bind` receipts (with their `Source`), `bound` posts (with their target document and the `bind` they answer), mints, typed startup failures, retries and Echo calls. Cells: (a) positive: the application's `loadURL` of the entry; (b) delayed listener: a fixture bridge build defers its start (listener, then `bind`) to a later task (`setTimeout`; the delay is recorded and the oracle is the Host's order, not the delay); (c) no listener: a fixture bridge build installs no listener and therefore never posts `bind`; (d) early `bind`: on the window's first navigation the replay hook delivers a recorded `bind` with the entry's `Source` after the entry's `NavigationStarting` and before its `ContentLoading`, and a fixture bridge build posts no `bind` of its own; (e) stale `bind`: A (an allowlisted remote `https` document) → B (the entry, by the application's `loadURL`); the replay hook delivers A's recorded `bind`, with A's `Source`, after B's `ContentLoading` and before B's own `bind`; (f) duplicate `bind`: after `bound`, the page posts the raw 21-byte `bind` vector twice; (g) navigation between `bind` and post: A = the entry, B = the entry again by the application's `loadURL`; a Host test hook holds A's `bound` after A's verified `bind` until B's `ContentLoading` has been processed, so only §4.4 rule 1 stands between it and the post (as in AC21); B's first inline script installs a raw `message` counter; (h) recovery: `ProcessFailed(RENDER_PROCESS_EXITED)` with `onLoss: reload-entry` (AC28 (a)) | (a) The Host posts no `bound` before it decodes the document's verified `bind`, then exactly one; the first inbound message is that `bound`; a floor `invoke` then resolves. (b) Zero `bound` posts until the deferred `bind` is decoded, then exactly one, received by the recorder; the floor `invoke` resolves. (c) Zero `bind` receipts and zero `bound` posts; at the startup deadline (§4.4) the window fails with the typed startup error exactly once; zero retries; never reported as success. (d) The early `bind` mints nothing, buffers nothing and is never answered: zero `bound` posts after `ContentLoading`; the window fails at the startup deadline with the typed startup error exactly once, no retry. (e) A's `bind` is refused by the Source rule: zero mints and zero `bound` posts until B's own `bind`, then exactly one `bound` for B. (f) Zero further `bound` posts and zero mints; the epoch and the role's #540 ledger counters are unchanged. (g) A's `bound` is dropped at the final `PostWebMessageAsString` check (B's raw counter for posts carrying A's `document` reads 0); A's startup route is retired; B receives its own `bound` only after its own `bind`. (h) The recovered document's `bind` is answered with one new `bound` whose `document` differs from the pre-crash one; a floor `invoke` after it reaches the Echo handler once. | (a) Treat `bind` as a note that is never answered (the r10 rule) → no `bound` arrives and the floor `invoke` never resolves → RED. (b) Post `bound` at `ContentLoading` instead of in reply to `bind` (the r10 order) → the Host's log shows a `bound` post before any `bind` receipt for that document → RED (the receipt also records whether the recorder received that pushed `bound`; whether `chrome.webview` delivers a post to a listener added in a later task is UNKNOWN, and E5's 24/24 push deliveries do not decide it). (c) Remove the startup deadline → the window stays open with no `bound` and no typed failure → RED. (d) Hold an early `bind` and answer it at the next qualifying `ContentLoading` → a `bound` is posted with no post-qualification `bind` → RED. (e) Skip the Source check on `bind` → a `bound` is posted in reply to A's `bind` before B's own → RED. (f) Answer every verified `bind` with a fresh mint → each duplicate produces a further `bound` with a new `document` → RED. (g) Skip the final-post check for `bound` → B's raw counter for A's `bound` reads 1 → RED. (h) Keep the once-per-document `bind` latch per WebView instead of per document → the recovered document's `bind` is refused as a duplicate, no `bound` arrives and the floor `invoke` never resolves → RED. |
| AC30 Page-readable nonce grants nothing (r11, §10 Q1(c); T4) | A hostile page reads `bound.document` and every raw Host post through its own `chrome.webview` `message` listener (route D, §4.2) and uses the current nonce in: (a) a wrong-document envelope (a nonce not byte-equal to the current one) and (b) a stale-epoch replay (both AC06); (c) a foreign-frame post (AC03); (d) raw v2 envelopes that bypass the preload (AC02; AC26's raw posts; AC27's raw `danger` `send`); (e) a grant bypass (AC09); (f) a forged receipt: on the AC27 harness, while delivery 1 (the `on('a')` listener's event) is outstanding because a fixture bridge build defers its `consumed` behind a test latch, the page posts a raw `consumed` with the current `document` for delivery 2, which was never posted | (a)–(e): exactly the zero-effect observables of the ACs named, unchanged. (f): the forged `consumed` releases no credit: the #540 ledger's release counter is unchanged by it, and delivery 1's record stays charged until the bridge's genuine `consumed` after the latch. No nonce match is counted as preload provenance or an isolated world. | (a)–(e): the mutants of AC06, AC03, AC02, AC26, AC27 and AC09, each in its own process. (f): the ledger releases the outstanding event record on any `consumed` whose `document` matches, without the exact ledger key → the forged receipt releases delivery 1's record before the genuine `consumed` (release counter 1) → RED. |
| AC17 Receipt | A Windows acceptance claim | Runtime `BrowserVersionString`, OS build, head, manifest digest, preload digest, per-AC counters, each RED control's output. | Drop the runtime string → validator RED. Separately, mark `WebMessageReceived` mocked → validator RED. |

## 4. Design

### 4.1 Atomic decomposition (decision input)

| Atom | Owner / boundary | Inputs → outputs | Failure mode | Observable | Evidence |
| --- | --- | --- | --- | --- | --- |
| A1 Script registration | `keld-wv` Windows backend, before the window's first navigation (for a compat window, the application's own `loadURL`, #459; `navigate_initial` only for schema-1 boots) | bridge + wrapped preload text → one document-created script id | Script absent from first document | AC01 | FACT: registration async (Microsoft). UNKNOWN: whether awaiting completion before `Navigate` is needed on 156 (P1 N3b did not falsify) — §10 Q5. |
| A2 Realm identity | WebView2; not controllable | document-created script runs in the main world of every top-level document and same-process frame | Page poisons built-ins; bridge text in frames and in error pages | AC02, AC03 | FACT (E1/E2; E5 `p1-exec-b`: bridge ran in the `about:srcdoc` error page). |
| A3 Event identity (frame) | WebView2 routing | frame post → `Frame2.WebMessageReceived` only | Host registers the frame event | AC03 | FACT: frames guide; E5 trigger q: same-origin srcdoc posts via `parent` arrive with the parent's `Source`. |
| A4 Document eligibility | #1192 §4.4 (KEL-102 verified resource identity); W3's served record `(root, components)` | committed main-frame document + `IsErrorPage` + allowed `NavigationId` + resource identity (§4.5) → qualified / FailedClosed | URL-text spoof; error page at an app URL; an equivalent spelling decided differently from W3 (two component equalities) | AC04, AC25 | FACT: E5 qualified by URL only (probe), so resource-identity qualification is UNTESTED on Windows; FACT: `about:srcdoc` error page committed 5/5 with `Source=about:srcdoc`. |
| A5 Sender authentication (Source, per call) | browser-stamped `Source` URL; `keld-guard`'s one `file:` URL parser (W3 #1185 §4.4) | parse; scheme equal to the committed document's (`https` or `file`); `https`: tuple origin equal; `file:`: empty host and the parser returns in-root components | Opaque-origin schemes (`about:`, `data:`, `blob:`, `chrome-error:`); comparing opaque origins by serialisation (wrong: every opaque origin serialises to `"null"`) or by `Url::origin()` (wrong: `file:` origins are opaque and unique, so nothing matches); a full-URL comparison (wrong: it breaks after history changes); a second path matcher beside the parser; string-loaded documents | AC05, AC18, AC20 | NOT EVIDENCE for this rule: in E5 (`https` only), 0 of 110,575 previous-document messages arrived after the next document's `ContentLoading` in all 96 trials, so its "0 mis-attribution" count had an empty at-risk set and the weaker mutant rules scored 0 too; the statistic discriminates no rule (§10 B7). FACT (source probe, 3/3): `NavigateToString` → `Source = about:blank`, origin `"null"`; `file://` → full file URL, origin `file://`. FACT (source probe v2, 3/3): `Source` **follows the history URL** — after `pushState`/`replaceState`/hash change it reads `…?route=2#frag`, `…?route=3`, `…?route=3#h2`, each preceded by `SourceChanged(IsNewDocument=false)`; an entry URL's query appears in `Source`; a message posted immediately after `pushState` still carried the pre-push `Source`. Hence identity is decided once at commit (A4) and `Source` is a per-call scheme/origin/root check, never a full-URL equality. FACT (`url` 2.5.8, locked in `Cargo.lock`: `origin.rs` L33, doc-test `lib.rs` L859–863): a `file:` URL has an opaque origin and two parses of the same file URL have unequal origins, so origin comparison cannot decide `file:`. Safety of the `file:` path as identity still rests on KEL-102 verification (B6). |
| A6 Document epoch | Host-minted 128-bit nonce; page-readable on route D, a correlation tag, not a secret (§10 Q1(c)) | `ContentLoading` → mint, no post; the document's verified `bind` → one `bound` (§4.3) | A `bound` posted before the bridge's listener exists is lost; a `bound` reaches a replaced document; a duplicate, stale or early `bind` mints or replenishes; a previous-document message arrives after the next commit | AC06, AC08, AC29 | FACT (E5): the epoch posted at `ContentLoading` reached the new document 24/24 per variant (12 trials × 2 documents), 0 wrong-document deliveries; a post made at `NavigationStarting(B)` was received by A 96/96. Cross-document ordering observed, not documented. E5 measured that push: feasibility only, not evidence for the bind→bound order. UNKNOWN: whether a previous-document message can arrive after the next `ContentLoading` (E5's at-risk set was empty) — §10 B7. DESIGN (r11, §10 Q1(b) RESOLVED): `bound` answers the document's verified `bind`; its evidence is AC29's device cells, not yet run. |
| A7 Authorization | `keld-guard` via `dispatch_privileged` in `keld-core` (#1191 T4) | principal + borrowed window grants + `el:c` → Allow / GUARD003 | Second evaluator; over-grant at generation time | AC09, AC13, AC14 | FACT: `windows` ignored today (`lib.rs:625-628`); #1191 is an open draft. |
| A8 Navigation authority | Host `NavigationStarting`; W3 rule A″ (§4.6) | raw `Uri` → scheme-prefix dispatch (`url_scheme_is`); `file:` → parser components (one component equality) and raw query (byte-exact) against the `file:` allow forms; any other scheme → arch 03 §2 matcher on the raw string → `SetCancel` at +0 or proceed (+1 only at commit) | in-root membership taken as allowlist membership; `about:srcdoc` and `document.open` unvetoable | AC10, AC11 | FACT (E5): b committed an error page 5/5 despite Cancel; g (`document.open`) replaced the document 5/5 with **no** `NavigationStarting`. |
| A9 Popups | existing deny-all | handled | — | AC12 | FACT: code. |
| A10 Static classification | `keld migrate` (`keld-cli`) | preload AST + digest → verdict (flag sites, reachable sinks) + grants | False negative (a narrowing or data-holding site passes unflagged) | AC15, AC22(a) | FACT: verb reserved (`verb.rs:24,75`). FACT (pinned draw.io blob `44f2f22f`, read directly): the rules of §4.8 flag seven sites (§4.8 table). UNKNOWN: whether rule 2 has false negatives on real preloads — T5 is not built; falsifiers are AC15's mutants and a corpus preload whose narrowing passes unflagged. |
| A11 Evidence | this spec + keld-research#34 | hashed runs → claims | Second runtime/OS claimed; zero-failure claimed with an empty at-risk set | AC17, B7 | FACT: one runtime, one OS build, one device, `https` only; E5's attribution statistic had no at-risk messages. |
| A12 Host→renderer delivery | `keld-wv` post path + #540 envelope | reply/event + target epoch → post or drop | A post reaches the next document's raw listener | AC21, AC24, AC22(e) | DESIGN (§4.4: generation check plus in-flight hold). FACT (E5): posts made at `NavigationStarting` reached the old document 96/96; a cancelled `about:srcdoc` navigation still raised `ContentLoading` (trigger b: `NS+Cancel > CL > NC(ok=False)`). UNKNOWN: delivery of a post handed to the engine before `NavigationStarting` and still in flight at commit (B7 arm H; non-zero with the hold is a blocker). |
| A13 Isolation reporting | `@keld/electron` `process` subset (#1155) on Windows | → `process.contextIsolated` | Facade reports isolation it lacks | AC23 | FACT (Electron `v45.1.0` `shell/renderer/renderer_client_base.cc:236-237`): `process.contextIsolated` is the frame's `context_isolation` preference; FACT (`shell/renderer/api/electron_api_context_bridge.cc:1145-1169`; `v44.2.0` `lib/renderer/api/context-bridge.ts:3-11`): `exposeInMainWorld` throws when it is false. |
| A14 Attestation trust | developer, recorded in `keld.permissions.jsonc` (§4.8 rule 3) | flag sites → attestation bound to digest | Wrong attestation: narrowing silently lost | AC19 (mechanism only) | DESIGN: no Keld party vouches for third-party attestations; the T6 qualification attestation is independently reviewed (§6 T6). |
| A15 Preload gate | bridge gate (§4.3) | committed top-level document → app preload runs or not | an epoch without the preload (a spelling-sensitive gate); the preload or argv in a remote document or error page | AC25 | DESIGN: the gate tests only top-level + `file:` scheme, and every committed top-level `file:` document is Host-served in-root bytes or a Host `404`/`403` refusal (rule A″ + W3 serving), so every document §4.5 makes eligible under B3's proposal gets the preload. UNKNOWN until the T2 cell (W3 Q7): whether a Host refusal commits as an error page or as an empty `file:` document; either way it gets no epoch and no `onLoss` (AC25 (f)/(h)). |
| A16 Wire grammar and carriage | #540 CompatV2 (`keld.wv-link` v2); `keld-wv`'s one `decode_wv_link`; WebView2 WebMessage strings (§10 Q1(a)) | top-level `TryGetWebMessageAsString` string → strict bounded UTF-8 → one closed v2 variant or a typed refusal | a v1, unknown-kind, wrong-direction or duplicate/extra-field envelope, a non-string message or a non-`JsonBytes` payload is admitted; `get_WebMessageAsJson` becomes a second parser; a second decoder or a fallback mode appears | AC26, AC27 | FACT: `decode_wv_link` is v1-only at `457a9c3b` (`crates/keld-wv/src/wv_link.rs:131-197`; `v != 1` refused at `:156`; kinds `bind`/`invoke` only, `:160-195`). FACT (#1122 `d9211a96` §4.5): the CompatV2 forms and "A mismatched `v` refuses, never switches the selected mode". DESIGN: the carriage is the #540 wire owner's decision (§10 Q1(a)); it changes the carrier, not the v2 schema. |
| A17 Process-failure kind | WebView2 `ProcessFailed`, through the one per-view handler W2 r16 D8 registers (W1 adds its kind dispatch, §4.10); W2's release barrier (#1188) for the terminal | kind → reload on the surviving control, or revoke everything and end the session | a closed controller is navigated; a renderer exit ends the session | AC24 (11, 14, 15), AC28 | FACT (Microsoft Learn, process-related events, `ms.date` 2023-07-13): a browser-process exit closes every associated control, which "need to be recreated"; after a main-frame renderer exit `Reload` may be used, and the impacted frames' content "is replaced with an error page" (whether that raises navigation events is UNKNOWN, §4.5). FACT: `keld-wv` registers no `ProcessFailed` handler at `457a9c3b`. FACT (#1280 `95594e88` D8 (3)): there only a pid-matched abnormal `BrowserProcessExited` receipt latches `AbnormalExit`, and a receipt after a rebind is foreign. DESIGN: the terminal is W2's `AbnormalExit`, latched at the first `poll` after this `ProcessFailed` under #1280 r16 D8 (`be9e9e56`) (§10 Q13 (a)). |

Independence: A16 runs after A3 and before A4–A7: a refused envelope never reaches them, and a well-formed one has no authority until they pass; its `Bind` branch calls A5's `source_ok`. A4 (eligibility) and A8 (navigation) are separate rules with separate
manifest keys; A5 and A6 are separate checks (only A6 separates a same-origin A→A2 pair), and
neither has discriminating natural-timing evidence yet (B7); A7 runs only after A3–A6 pass;
A10 decides the verdict, A14 only covers flagged sites and never widens A7's grants; A12 is
independent of A6 (a renderer-side epoch check cannot stop the page's raw listener). A15
has no authority: a document the gate wrongly selects still has every call refused without
an epoch (A4), and one it wrongly skips loses only its preload API (AC25 detects both).

### 4.2 Trust model: three Host rules instead of three worlds

1. **No IPC authority or Host-delivered data exists only in preload code.** The page can do
   whatever the preload can and read whatever the preload receives; the Host grants exactly
   the literals `keld migrate` generated for this window and this preload digest. A preload
   that may narrow what the page can cause or see is flagged (§4.8) and its window receives
   grants only with a digest-bound attestation. The epoch nonce is page-readable (§2, §10
   Q1(c)): a freshness/correlation tag for the current Host-qualified document, not a secret
   and not authentication; it conveys no authority. Authority comes only from the actual
   top-level event source, the current native epoch, the parsed `Source`, the Webview
   principal, the Host-owned literal `el:` grants, source/digest eligibility and the
   authenticated Bun role. A matching nonce never proves preload provenance or an isolated
   world.
2. **The Host decides by event source, document and epoch, not by the JavaScript world.**
   Admission = top-level event ∧ current nonce of the document qualified at commit ∧
   per-call `Source` scheme/origin/root check (§4.4 step 3; never full-URL equality, never
   a comparison of opaque origins) ∧ literal
   `el:` grant. Exception: `window.keld.invoke` routes only KEL-142's declared Echo channel
   `1` after the same identity steps (§4.4); it is not an `el:` grant.
3. **Navigation and eligibility are Host policy** (§4.5, §4.6), not preload behaviour.

Electron uses two separate boundaries: IPC sender validation by frame/origin (security
checklist item 17) **and** context isolation between page and preload ("helps prevent the
website from accessing Electron internals or the powerful APIs your preload script has
access to", `docs/tutorial/context-isolation.md` at `v45.1.0`). Route D keeps the first and
gives up the second; it is equivalent to Electron only where rule 1 holds for every preload
capability, and that is what the classifier and attestation decide.

**Accepted divergences** (each becomes a T1 scoreboard entry; this list is the one owner):

1. Same-realm `exposeInMainWorld`: no copy, no function proxy; the page receives the
   preload's own function objects (and their source text via `Function.prototype.toString`).
2. The page reaches every granted channel directly, with any codec-valid arguments
   (#1190: ≤ 4096 bytes, Host-opaque), bypassing the preload.
3. Preload text and `process.argv` are treated as page-visible: exposed preload functions
   are page-reachable objects whose text `Function.prototype.toString` returns, and every
   preload use of argv after page code starts runs on page-replaceable built-ins. No secrets
   belong in `additionalArguments`; every argv read is flagged (§4.8 rule 2(c)).
4. Every inbound Host→renderer post to the window is page-visible through the page's own
   `chrome.webview` `message` listener — including replies to other callers, payload
   arguments a preload listener drops, and channels the preload never subscribes to (§10 Q12).
5. Same-origin iframes reach `top.<key>` and `top.keld.invoke` and are indistinguishable
   from the application (§4.7).
6. `process.contextIsolated` is `false` while `contextBridge.exposeInMainWorld` keeps
   working. Electron couples the two: `exposeInMainWorld` throws "contextBridge API can only
   be used when contextIsolation is enabled" unless the frame's `context_isolation`
   preference is set (`v45.1.0` `electron_api_context_bridge.cc:1145-1169`; `v44.2.0`
   `lib/renderer/api/context-bridge.ts:3-11`), and `process.contextIsolated` is that same
   preference (`renderer_client_base.cc:236-237`). The facade MUST NOT report isolation it
   lacks (AC23), and `keld migrate` constant-folds `process.contextIsolated` to `false` when
   it computes reachable sinks, so the branch the application actually takes is the one
   granted. Breakages, reported by `keld migrate` as blocking findings rather than
   discovered at run time: a preload that asserts isolation (`if (!process.contextIsolated)
   throw …`) throws; a non-isolated branch that relies on `nodeIntegration` (`require` of
   Node modules in the page) does not work, because Keld provides none.
7. A page can replace its own exposed key, or forge a `message` event that sets the
   bridge's `epoch` before the real one arrives (§4.3) — self-harm only.
8. AC1's preload trust root and rule 1's digest both rest on the unowned Windows trust root
   (§10 B4).
9. While a top-level navigation is pending (§4.4 in-flight hold) the Host delivers no reply or
   event to the old document; anything due then is discarded at commit — for example an
   `invoke` issued from a `beforeunload` handler never resolves. Electron keeps delivering
   to the old document until the swap. A navigation still pending at `NAV_TERMINAL_BOUND`
   ends with the current document FailedClosed without `onLoss` (§4.4 rule 2 (ii)), and a
   newer document that committed while an older navigation stayed pending is FailedClosed
   with it (rule 2 (iii)); Electron has no such bound. AC24; T1 scoreboard entry.
10. The app preload runs in every Host-served `file:` top-level document of the window that
   is not an error page, entry or not, and never in an allowlisted remote (non-`file:`)
   document or an error page; Electron runs it in every document of the `webContents`
   (§4.3, AC25). T1 scoreboard entry.
11. While one `send` is outstanding in a document (from its post until its `send-result`;
   #540 `bound.limits.send_records: 1`), another `ipcRenderer.send` there is a **dropped
   call**: the adapter detects `recordBusy` synchronously before posting, and the void
   `ipcRenderer.send` returns `undefined` and emits exactly one bounded renderer-local typed
   `recordBusy` diagnostic (#1154 §4.7's policy, whose diagnostic is Host-private there but
   renderer-local here, because the page-world bridge decides and posts nothing). No v2
   `send` is posted, no Bun handler runs and no credit is consumed; it is not a queued call
   or an Electron-equivalent send. The #540 wire owner accepted this as a selected
   first-proof compatibility divergence (§10 Q1(d)); #1122 §4.10 permits void APIs and late
   failures as internal diagnostics ("Synchronous local error before admission";
   "Finite-capacity refusal … explicit Keld behavior, not unqualified Electron parity").
   The renderer's `outstanding` bit is the same-document check only, not the role quota:
   #540 §4.8 keeps one DATA record per direction across the authenticated role (at most one
   per document), so the Host/role ledger refuses a second reservation from another window
   on the same role (AC27), with no per-window quota, backlog or replay. Role-global
   exhaustion is detected only at the Host and follows #540's private terminal/diagnostic and
   exact release; no JS throw is fabricated. Whether a real application loses required
   behaviour this way is T6's send-capacity qualification gate (§6). Electron's
   `ipcRenderer.send` has no such refusal in its JavaScript layer (`v44.2.0`
   `lib/renderer/api/ipc-renderer.ts` L9–11). AC27; T1 scoreboard entry.

### 4.3 The bridge script

One function-wrapped script, no `eval`, registered once per WebView before first navigation.
Shared `@keld/electron` renderer facade; only the transport adapter is Windows-specific.

```ts
// Wire: ONLY #540 CompatV2 (`keld.wv-link` v2, #1122 §4.5). `wire` is that spec's TS projection,
// kept by the bridge-script/decoder owner, not a second parser: `wire.inbound(text)` returns one
// exact NativeToRendererV2 value or null (any `v` but 2, an unknown kind, a missing or extra
// field, a non-JsonBytes payload); the builders emit exact RendererToNativeV2 text. Names are
// illustrative; shapes, bounds and sequences are #540's.
const post = chrome.webview.postMessage.bind(chrome.webview);   // captured before page code
const listen = chrome.webview.addEventListener.bind(chrome.webview);
if (window !== window.top) return;                               // frames get nothing
let epoch: string | null = null;      // `document` of the first `bound`: 32 lowercase hex (#540 §4.3)
let ready = false;                    // `bound.ready`, then each `route-state`
let message = 0;                      // #540 per-document u32, nonzero
let outstanding = false;              // same-document check only; the role quota is the Host's (#540 §4.8)
listen("message", (e) => {
  if (typeof e.data !== "string") return;
  const env = wire.inbound(e.data);                              // no v1 branch, no fallback
  if (env === null) return;
  if (env.kind === "bound") {
    if (epoch === null) { epoch = env.document; ready = env.ready; }
    return;
  }
  if (epoch === null || env.document !== epoch) return;          // AC08: drop stale / pre-epoch
  switch (env.kind) {
    case "route-state": ready = env.state === "ready"; return;   // "retired" → not ready
    case "send-result": if (env.message === message) outstanding = false; return;
    case "invoke-result": settleFloorInvoke(env); return;        // KEL-142 §4.1, one pending
    case "event":                                                // #1154 emitter returns the
      post(wire.consumed(epoch, env.delivery, deliver(env))); return; // disposition: one `consumed`
  }
});
function send(channel: string, payload: Uint8Array): void {     // #1190 codec bytes, ≤ 4096
  if (epoch === null || !ready) throw notReady();                // AC07: #1154 §4.7, no queue
  if (outstanding) throw recordBusy();                           // #1122 §4.10; divergence 11
  if (message === 0xffffffff) throw routeClosed();               // #540: exhaustion, no wrap
  outstanding = true;
  post(wire.send(epoch, ++message, channel, payload));           // payload as JsonBytes
}
post(wire.bind());   // after listen(): {"v":2,"kind":"bind"} once; answered by one `bound` (§10 Q1(b))
installKeldFloor(wire, () => (ready ? epoch : null));            // KEL-142 §4.1: v2 numeric `invoke`, same not-ready rule
installElectronFacade(send);                                      // #1154 shapes
// (b'): every Host-served file: top-level document; no path or spelling is compared in the page.
// If §10 B6 chooses the Keld-owned virtual host instead: location.origin === APP_ORIGIN.
if (location.protocol === "file:") runWrappedPreload();         // #1155 §4.2
```

Rules: the text holds no secret. The gate runs the app preload in every top-level document
whose `location.protocol` is `file:` (or, if §10 B6 chooses the Keld-owned virtual host,
whose `location.origin` is that host's one tuple origin). No path, spelling or entry set is
compared in the page, so there is no second evaluator and no spelling mismatch: every
committed top-level `file:` document is Host-served in-root bytes or a W3 `404`/`403` refusal by construction (rule A″
cancels every `file:` target the parser refuses, and W3 serves only in-root regular files;
anything else commits as an error page or as an empty `file:` document — UNKNOWN, W3 Q7,
AC25 (f)/(h); in the latter case the gate runs the preload in a document §4.5 does not make
eligible: no epoch, no authority, no `onLoss`), so every document §4.5 makes eligible under
B3's proposal gets the preload. Allowlisted remote documents and error
pages (`chrome-error:`, `about:srcdoc`) never get the preload or argv. The gate decides only
whether the app preload runs and has no authority either way (A4 is the rule): a document
without an epoch has every bridge call refused. A page can forge a `chrome.webview`
`message` event and corrupt `epoch`; that only breaks its own calls. The `bind` message is
sent once per document, after the bridge's `message` listener is installed and before any
epoch exists; it carries no field, no channel and no authority, and the Host answers it at
most once with this document's `bound` (§4.4).

**One wire grammar (r9, 0monish 4425538690).** Every message in both directions is one of
#540's closed CompatV2 forms (PR #1122 at `d9211a96`, §4.5): renderer `bind`, `invoke`,
`send`, `consumed`; native `bound`, `invoke-result`, `send-result`, `event`, `route-state`.
W1 adds no kind: the r8 `hello` is #540's `bind` and the r8 `epoch` post is #540's `bound`
(its `document` is the nonce, exactly 32 lowercase hex characters, #1122 §4.3; its `limits`
and `ready` fields are #540's). The bridge dispatches by kind: `send-result` completes its
`send` (one outstanding record, `bound.limits.send_records: 1`); `invoke-result` settles the
floor `invoke`; each `event` is delivered once and answered by exactly one `consumed`;
`bound.ready` and then each `route-state` set readiness, and while not ready (`ready: false`
or `retired`) every call refuses not-ready with no post. The bridge's typed local errors
(`notReady`, `recordBusy`, `routeClosed`) never post anything. #1154's void
`ipcRenderer.send` catches them, returns `undefined` and emits one typed local diagnostic
(#1154 §4.7's policy; renderer-local on route D, divergence 11), so a page never sees a throw from `send` (#1122 §4.10 allows a throw; W1 takes
#1154's void policy). The floor `window.keld.invoke` rejects with them instead (AC07). The bridge has no v1 branch, and the
Host decodes only with `decode_wv_link` in CompatV2 mode (§4.4). #1122 §4.5/§4.8 do not say
when `bound` is sent; the #540 wire owner's decision (§10 Q1(b), RESOLVED) sets the Windows
order to the landed macOS v1 request→response order
(`crates/keld-wv/src/wkwebview/macos_bridge.rs:749-762` at `457a9c3b`: `Bind` →
`state.bind()` → `__keldBridgeNativeBind(nonce)`) with the unchanged v2 `bound` shape,
because `ContentLoading` "triggers before any content is loaded, including scripts added
with `AddScriptToExecuteOnDocumentCreated`" (Microsoft Learn
[`ICoreWebView2::add_ContentLoading`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2#add_contentloading)),
so a Host post at that event can precede the bridge's listener:
(1) at an eligible top-level `ContentLoading` the Host qualifies the document and mints its
epoch **without posting** — no grant, role readiness or ledger reset is inferred from
`ContentLoading` alone; (2) the bridge installs its `message` listener first, then posts
`bind` once; (3) the Host checks the current qualified document, the exact top-level
`Source`, the WebView/controller lifetime, CompatV2 mode and liveness, and answers once with
`bound { document, limits, ready }` for that exact document (`ready` is informational, not
authority; `route-state` later reflects the admitted Bun role); (4) a duplicate, stale or
foreign `bind` mints nothing, replenishes nothing, changes no role and never receives
another document's `bound`; if the document is replaced between the `bind` and the final
post, the old `bound` is dropped (§4.4 rule 1) and that startup route fails; (5) an early
`bind`, before qualification, gets zero buffered work, zero nonce and zero authority and
fails closed through the existing bounded typed startup error/timeout (§4.4; #1192 §4.4's
zero-queue policy). W1 adds no wire kind and no fallback. A timeout with no `bound` is a
typed startup failure, never success and never a retry. "`bound` may arrive before `bind`"
is not a success oracle. E5 measured the earlier push at `ContentLoading` (AC06): it shows
that posting at that point is feasible, not this order, whose evidence is AC29.

Gate options. rpipaliya's review of `043170ca` showed that the r4/r5 gate (an ASCII-folded
`location` key against `ENTRY_KEYS`) and the parser's identity class disagree on
`index%2Ehtml` and on non-ASCII case variants, so a qualified document could run without its
preload. Scores 0–10; a disqualifier overrides the mean.

| Option | Gate ↔ Host agreement | One owner (no second evaluator) | Electron fidelity | First-inline floor (AC01) | No new exposure | Verifiable | Mean | Disqualifier |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| (a) A JavaScript normaliser in the gate (decode once + case fold), kept equal to the parser by shared golden vectors | 4 (ASCII and `%XX` can match; non-ASCII cannot: JavaScript has no ordinal ignore-case — `"ß".toUpperCase()` is `"SS"` — while `CompareStringOrdinal` uses the OS upper-case table) | 3 (a second implementation of the parser's identity class) | 8 | 10 | 9 | 5 (finite vectors; OS-table drift) | 6.5 | — |
| (b) Run the app preload in every top-level document of the window | 10 (no gate) | 10 | 10 | 10 | 2 (preload text and argv, divergence 3, reach allowlisted remote documents and error pages) | 8 | 8.3 | a new exposure to remote content (§4.2 rule 1) |
| **(b′) Run it in every Host-served `file:` top-level document that is not an error page** | 10 (no string comparison; same document set as §4.5 under B3's proposal) | 10 | 9 (every app document, entry or not; remote documents and error pages differ, divergence 10) | 10 | 9 (only Host-served in-root app documents) | 9 (AC25 cells) | **9.5** | — |
| (e) Eligibility requires the entry's one canonical spelling; the gate compares the same string | 10 (same bytes; one device fact) | 9 (one serializer, the parser's inverse) | 4 | 10 | 10 | 8 | 8.5 | page-triggerable denial of service (W3 r3 pre-PR P1-C; W3 §4.6 rule 5 at `1db53482`), once W3 Q27 lets page-initiated `file:` navigations through: a page link to `INDEX.HTML` passes W3's allowlist (one component equality ignores case), fails canonical-spelling eligibility, lands FailedClosed and triggers `onLoss`; three clicks close the window, which Electron never does |

Chosen: (b′), confidence 0.8. Its residual is how a Host `404`/`403` refusal commits (AC25
(f)/(h), W3 Q7); if it commits as a `file:` document, it gets the preload text but no epoch,
so no call is admitted and no `onLoss` fires.

### 4.4 Host admission and forwarding

Carrier (§10 Q1(a), RESOLVED): renderer→Host `chrome.webview.postMessage(<exact v2 JSON text
string>)`, Host→renderer `PostWebMessageAsString(<exact v2 JSON text string>)`, arriving in JS
as `MessageEvent.data: string`. `IsWebMessageEnabled` must be true. The only source is the
top-level `ICoreWebView2::WebMessageReceived` event, never an `ICoreWebView2Frame2` event
(AC03). The Host accepts only `TryGetWebMessageAsString`: a non-string message is refused,
and `get_WebMessageAsJson` is never read (for a JS string it returns the extra JSON-quoted
representation, not the wire text, and would become a second parser). A strict, bounded
UTF-16→UTF-8 conversion and the full encoded-envelope checks precede JSON decoding and every
effect.
Envelope: exactly #540's CompatV2 forms (§4.3), `document` = epoch nonce (exactly 32
lowercase hex characters, #1122 §4.3), payload =
`JsonBytes` (≤ 4096 bytes, `MAX_RENDERER_PAYLOAD_LEN`; envelope ≤ `MAX_WV_LINK_ENVELOPE_LEN`
checked before parsing, `wv_link.rs:14,21,132`), never base64url. The one decoder is
`keld-wv`'s `decode_wv_link` in the endpoint's trusted CompatV2 mode (#540 §4.5 "Version
selection"): a `v:1` envelope, an unknown kind such as `hello` or `epoch`, a missing or extra
field or a non-array payload is refused before any effect. Windows has no LegacyV1 mode and
no second decoder. Today `decode_wv_link` is v1-only (`wv_link.rs:131-197`); its CompatV2
mode is #540 work (#1122 §7.1 W1, owner #465), so T2 blocks on it (§10 Q1). Split of responsibilities per §5: **`keld-wv` authenticates identity
(steps 1–3) and hands a stamped, bounded message to `keld-core`'s forwarder, which is the
only caller of `dispatch_privileged` (#1191 T4).**

```rust
// keld-wv/webview2 (STA callback): identity only; no policy, no Bun write.
fn on_web_message(view: &ViewState, args: &ICoreWebView2WebMessageReceivedEventArgs) {
    // 1. top-level event: registered on ICoreWebView2 only.
    // Only TryGetWebMessageAsString: a non-string message fails here and is refused; never
    // get_WebMessageAsJson (a JS string's JSON-quoted form, a second parser). Names illustrative.
    let Ok(text) = args.TryGetWebMessageAsString() else { return refuse(Refusal::NotString) };
    // Strict, bounded UTF-16 → UTF-8 (unpaired surrogate or over-length → refused) before decoding.
    let Some(text) = strict_utf8(&text, MAX_WV_LINK_ENVELOPE_LEN) else { return refuse(Refusal::Encoding) };
    // The ONE decoder, CompatV2 mode (#540 §4.5); the mode argument's shape is #540's.
    let env = match wv_link::decode_wv_link(text.as_bytes(), Mode::CompatV2) {
        Ok(env) => env,
        Err(reason) => return refuse(Refusal::Wire(reason)),   // v:1, `hello`, base64url, …
    };
    let source = args.Source()?;                    // the RAW string WebView2 stamped; never re-serialised
    let src: &str = &source;
    if let WvLinkEnvelope::Bind = env {
        // 2'. `bind` (§10 Q1(b)): answered at most once, with the CURRENT qualified document's
        //     `bound`. Before qualification: zero buffered work, zero nonce, zero authority
        //     (#1192 §4.4); nothing is held for the later ContentLoading. A duplicate, stale or
        //     foreign `bind` mints, posts and replenishes nothing.
        let Some(doc) = view.qualified_document() else { return refuse(Refusal::EarlyBind) };
        if !source_ok(src, &doc.entry) { return refuse(Refusal::Source); }
        if !doc.take_bind() { return refuse(Refusal::DuplicateBind); }   // once per document
        return view.post_bound(doc);   // the final PostWebMessageAsString re-checks the target (rule 1)
    }
    // `invoke`, `send`, `consumed`: each carries `document`; steps 2–3 apply to all three.
    // 2. epoch byte-equal to the CURRENT committed eligible document's nonce (identity was
    //    decided once, at that document's ContentLoading — A4).
    let Some(doc) = view.qualified_document() else { return refuse(Refusal::NoDocument) };
    if !doc.epoch.matches(env.document()) { return refuse(Refusal::Epoch); }
    // 3. Source per call (one function, also used by step 2'):
    if !source_ok(src, &doc.entry) { return refuse(Refusal::Source); }
    // hand-off: stamped with (WebviewId, NonZeroU32 generation, window key, grants digest-bound, §4.8).
    view.forwarder.submit(Stamped { id: view.id, generation: doc.generation, window: view.window_key, env });
}
// keld-core app_session (#1191 T4 forwarder): the ONE evaluator call, for a v2 `Send`. An
// `Invoke` follows KEL-142 §4.1 (below); a `Consumed` goes to #540's ledger owner.
fn forward(stamped: Stamped) {
    let principal = Principal::Webview { id: stamped.id.0, generation: stamped.generation.get() };
    dispatch_privileged(manifest, principal, RendererChannelQuery {
        channel: &stamped.env.channel,                 // "el:" prefixed exactly once (#1191)
        current_window: grants_for(stamped.window),    // empty unless digest matched (AC14)
        on_allow: |_permit| app_link.forward_bounded(stamped.env),
    })
}
// The per-call Source rule. Never a full-URL equality (Source follows pushState/replaceState/
// hash, source probe v2) and never a comparison of opaque origins: a file: URL's origin is
// opaque and unique (url 2.5.8), and every opaque origin serialises to "null".
fn source_ok(src: &str, entry: &QualifiedEntry) -> bool {
    match entry.scheme {
        // https (a Keld-owned virtual host, if B6 chooses one): the committed tuple origin.
        // keld-guard parses `src` once with `url` 2.5.8 and returns the https tuple origin.
        EntryScheme::Https => keld_guard::https_tuple_origin(src) == Some(entry.tuple_origin),
        // file: exactly `scheme == file && file_url_root_relative(source, admitted_root).is_ok()`.
        // keld-guard's ONE file: URL parser (W3 #1185 §4.4) decides membership: empty host,
        // one percent-decode, ordinal case-insensitive in-root components, package grammar
        // per component; query and fragment cut. No origin equality, no second path helper.
        // `url_scheme_is` is the parser's own R1 test (ASCII case-insensitive `file:` prefix of
        // the raw string), not a URL parse; keld-guard exports that one function (§8) and the
        // navigation dispatch (§4.6) uses it too. The parser gets the raw string.
        EntryScheme::File => keld_guard::url_scheme_is(src, "file")
            && keld_guard::file_url_root_relative(src, &entry.admitted_root).is_ok(),
    }
    // Any other scheme (about:, data:, javascript:, blob:, chrome-error:, …) never matches.
}
```

**URL parsing has one owner.** Every URL decision in this spec receives the raw string
WebView2 reported (`Source`, `NavigationStarting` `Uri`) and parses it at most once:

| Option | Fidelity to Chromium's WHATWG parsing | Owners | Dependency gate | Double-parse risk | Mean |
| --- | --- | --- | --- | --- | --- |
| **(a) keld-guard owns every URL decision: W3's hand-written `file:` parser on the raw string; `url` 2.5.8 (WHATWG) for non-`file:` schemes** | 8 | 9 (one crate, already the security leaf) | 6 (direct dependency; version already locked via `tao`/`wry`) | 9 | **8.0** |
| (b) `keld-wv` parses with `url`; keld-guard keeps only the `file:` parser | 8 | 4 (two owners) | 6 | 6 | 6.0 |
| (c) hand-written `https` parser in keld-guard | 3 (IDNA, IPv4 spellings, percent-encoded hosts) | 9 | 10 | 8 | 7.5 |

Chosen: (a). `keld_guard::https_tuple_origin(raw)` parses once and returns the tuple only
for `https`. A `file:` string is never parsed by `url`: `url` rewrites `file://localhost/`
to `file:///` (`url` 2.5.8 `src/parser.rs` L1049), so feeding a re-serialised URL to `file_url_root_relative` would bypass W3's
R2 (no authority). `NavigationStarting` dispatches on the raw string's scheme prefix: `file:`
goes to the parser; anything else is matched as the raw, Chromium-canonical `Uri` by the
arch 03 §2 scope matcher (spell destinations as the host sends them). No component
re-serialises a URL and hands it to another. This opens the dependency gate (§8).

Refusal order, as the code above runs it: carriage (non-string message, UTF-16→UTF-8
conversion), wire shape (`decode_wv_link`), then no-document, then epoch, then Source; all
precede a grant miss (#1191 AC8). Before any effect the Host revalidates the current
document and epoch, the event's `Source`, the authenticated Bun role and the literal grant
(§10 Q1(a)). Startup deadline: when `ContentLoading` qualifies a document the Host starts
the existing `KELD-WV-005` deadline; if no verified `bind` for that document is answered
with its `bound` within it, the window fails with a typed startup error (code allocated by
the error-registry owner; not `KELD-WV-012/013`) — never success, never a retry. An early
`bind` is never held or replayed (#1192 `cd30f997` §4.4, `docs/specs/gh542-native-document-dual-world-binding.md:133`: "zero queued
Bind/Invoke/Send/Consumed"), so a document whose only `bind` came early fails at this
deadline (AC29 (d)). No new wire kind.

`window.keld.invoke(channel: u16, payload)` is admitted through steps 1–3 and then exactly
as KEL-142 §4.1 admits it on macOS: only the declared Echo channel `1`; it is **not** routed
through `el:` grants and no numeric channel grant grammar is introduced here. Widening it is
KEL-142/#508's decision (§10 Q7).

Host→renderer delivery (`mainResp`, `webContents.send`) uses `PostWebMessageAsString` with
`document` = the **target document's epoch**: `mainResp` keeps its requesting document and
`webContents.send` captures its current target at send admission (#1191 AC9, #543/#466).
Because the page reads every post through its own raw listener, the Host is the only
security control on this path (AC21); the bridge's mismatch drop (AC08) is a correctness
guard. The Host rule:

1. **Generation check.** Immediately before `PostWebMessageAsString`, on the STA thread, the
   Host drops the post unless the window's current qualified document still has the target
   epoch (AC21). This includes `bound`: if the document was replaced between its `bind` and
   the post, the old `bound` is dropped and that startup route fails (AC29 (g)); a queued post
   never reaches a new page (§10 Q1(a)).
2. **In-flight hold.** Every top-level `NavigationStarting` — allowed **or cancelled** —
   adds its `NavigationId` to a pending set; the Host posts nothing while that set is
   non-empty (AC24). Cancelled navigations are included because a cancelled top-level
   `about:srcdoc` still raised `ContentLoading` (E5 trigger b: `NS+Cancel > CL > NC(ok=False)`).
   A navigation leaves the set at its `ContentLoading` (the old epoch is revoked and every
   post held for it is discarded) or at its `NavigationCompleted` without a `ContentLoading`.
   **Bounded exit (proposed r7; split in r10).** (i) `ProcessFailed` of kind
   `RENDER_PROCESS_EXITED` empties the set, revokes the current epoch, discards held posts and
   FailedCloses the current document with `onLoss` (the document is gone; the control
   survives). `BROWSER_PROCESS_EXITED` does the same for every window of that browser but
   never applies `onLoss`, because the controls are closed; the session ends through W2's `AbnormalExit` (§4.5); other kinds
   (`RENDER_PROCESS_UNRESPONSIVE`, frame and utility kinds) change nothing. Window or webview
   retirement follows §4.5's Destroy row (terminal, no `onLoss`). (ii) When a navigation has
   stayed pending for `NAV_TERMINAL_BOUND` (a Host deadline on the STA event loop's
   `WaitUntil`, as `keld-wv` already uses at `webview2/mod.rs:2744-2745`; no thread, no sleep;
   value set at T2), the Host calls `Stop()`, empties the pending set, revokes the current epoch, discards held posts
   and FailedCloses the current document **without** `onLoss`: a slow navigation is not a
   loss, so it cannot use up the recoveries (§4.5). The expired `NavigationId` stays recorded:
   a late `ContentLoading` for it is processed as §4.5 (revoke; mint if eligible), and a late
   `NavigationCompleted` for it changes nothing. A post for a newer epoch handed to the engine after the
   bound while the expired navigation can still commit is outside the hold (rule 2's risk with
   the hold lifted); §10 B7 arm H's bound variant measures it. (iii) The bound gives
   **bounded** liveness, not successor-preserving liveness: when an older navigation stays
   pending while a newer one commits (`NavigationStarting(n₁)` → `NavigationStarting(n₂)` →
   `ContentLoading(n₂)`), the newer document's posts are held until the bound and that
   document is then FailedClosed without `onLoss` (AC24 case 12). Whether `ContentLoading(n₂)`
   may end older pending navigations (supersession) is UNKNOWN: E5's 421 event sessions hold
   **no** `NavigationStarting` raised while another top-level navigation was pending (recount
   2026-10-30 over the same 46 logs; at-risk 0, rule not exercised). A T2 device cell starts a
   slow allowlisted remote navigation, then an in-root one, and records, with an at-risk count
   above 0, whether the older one ever raises `ContentLoading` or `NavigationCompleted(IsSuccess
   = TRUE)` after the newer one's `ContentLoading`; only that evidence may replace (iii).
   Discarded posts release their #540 reservations; renderer-side waiters in a document that is
   still live never settle (divergence 9); Bun-side state is unaffected — INFERRED; AC24 cases
   10–13 assert the stub's #540 reservation counter returns to its initial value. The 1,064/1,064 terminal events below are evidence on one runtime, not a
   proof that a terminal event always arrives.
   **Attribution.** `SourceChanged` carries no `NavigationId`; each `SourceChanged`, of either
   `IsNewDocument` value, is attributed to the most recent still-pending navigation started
   before it. For the ordering AC24 case 9 tests, an older navigation's completion is then not
   judged by a newer navigation's signal; whether WebView2 ever commits an older navigation
   after a newer one starts is (iii)'s open question.
   In the second case the held posts are released — once the pending set is empty, still
   subject to rule 1 — **only** if no `SourceChanged(IsNewDocument == TRUE)` attributed to
   this navigation arrived and either `IsSuccess == FALSE` (no commit: a
   cancelled or failed navigation leaves the live document and its IPC intact, as rule A″
   requires) or at least one `SourceChanged(IsNewDocument == FALSE)` attributed to it arrived
   (a same-document navigation). Every other completion without `ContentLoading` is a document
   change — revoke, FailedClosed, `onLoss`, nothing released: one to which a
   `SourceChanged(IsNewDocument == TRUE)` is attributed (for example a back/forward-cache restore,
   if WebView2 raises no `ContentLoading` for it — UNKNOWN on 156), and one with
   `IsSuccess == TRUE` and no `SourceChanged` at all (for example a same-URL restore, which
   changes no `Source`). `SourceChanged` carries no `NavigationId`, so no single signal
   decides alone: a new-document signal wins over an unrelated `pushState`
   (`IsNewDocument == FALSE`, AC24 case 7), and the absence of any signal fails closed
   (case 8). `SourceChanged(IsNewDocument == TRUE)` preceded `ContentLoading` in every
   source-probe run (`source-v1`/`source-v2`, 3/3 each); within one navigation Microsoft
   documents that order (`ContentLoading` "follows the `NavigationStarting` and `SourceChanged`
   events and precedes the `HistoryChanged` and `NavigationCompleted` events",
   `ICoreWebView2::add_ContentLoading`).
   Liveness: all 1,064 top-level `NavigationStarting` events in E5's 46 JSON event logs
   (`route-d-probe/runs/**`, including the eight gzipped probe-2 logs; `results.json` holds no
   event list),
   270 of them cancelled, ended in a `NavigationCompleted` with the same `NavigationId` in the
   same log (recounted 2026-10-30 by matching each event list's `NavigationStarting` to a
   later `NavigationCompleted` with the same `navId`), so the pending set drains. Whether
   WebView2 raises `NavigationStarting` for a same-document navigation is UNKNOWN
   (`source-v2` did not subscribe). Failing closed has an availability cost if it does: a
   repeated navigation to the current `#fragment` changes no `Source`, raises no
   `SourceChanged` and would revoke a live document (case 8). The T2 receipt records the
   event sequence for a fragment navigation, a repeated same-fragment navigation and a
   back/forward-cache restore; a revoked document in the repeated-fragment cell blocks T2's
   merge until this rule is revised, and is not an accepted divergence.
   **Residual (stated):** the same-document release signal (`SourceChanged(IsNewDocument ==
   FALSE)`) can be raised by the page itself through `pushState`. It releases held posts only
   when no new-document signal is attributed to the navigation; a document change that raised
   neither `ContentLoading` nor `SourceChanged(IsNewDocument == TRUE)` was not observed (E5,
   `source-v1`/`source-v2`). The T2 receipt adds a cell in which the page calls `pushState`
   between `NavigationStarting` and a cross-document completion and records that no held post
   reaches the new document.
   Posts are held within #540's reservations. Reason: the Host learns of a commit only through the asynchronous
   `ContentLoading` event, so a post issued in the gap could reach the next document while
   rule 1 still passes.
3. **Residual the hold cannot close.** A post handed to `PostWebMessageAsString` *before*
   `NavigationStarting` and still undelivered when the next document commits cannot be
   recalled. §10 B7 arm H measures it; a non-zero count with the hold in place is a blocker
   for approval, not an accepted divergence.

### 4.5 Per-document lifecycle (WebView2 events onto #1192 §4.2)

One rule for revocation: **every top-level `ContentLoading` revokes the previous epoch. A
new epoch is minted iff `IsErrorPage == FALSE`, the `NavigationId` equals that of a
`NavigationStarting` (or its `IsRedirected` hop) that rule A″ allowed or a Host attempt
issued, and the document has verified resource identity under #1192 §4.4: for `file:`, W3's
served record for this `NavigationId` (status 200) names the admitted root and in-root
components, with the bytes as currently present in that root (W3 §4.4 document correlation at
#1311 `1db53482`: Host-served iff `served.status == 200 && !IsErrorPage`, credited only by the
Host attempt's nonce, so a page-initiated `file:` document is never credited until W3 B19
resolves; W3 owner Q2); for the Keld-owned virtual host, the verified path it maps.
Proposed (§10 B3, with W3 B7 (v)): every such Host-served in-root regular top-level
document is eligible, entry or not, as in Electron. Query and fragment are never identity
(the allowlist decides queries, §4.6). Never URL text, never a `NavigateToString` load.
Identity is decided **once, here**; the epoch binds to that committed document. Otherwise
the document is FailedClosed with `channels: []`.** Equivalent spellings of one in-root
file (`INDEX.HTML`, `index%2Ehtml`, a case variant of a non-ASCII root component) are one
identity under keld-guard's one component equality (§2), so they get the same navigation
decision, the same eligibility and the same preload (§4.3, AC25). If B3 rejects the
proposal, eligibility is limited to the window's declared entries, compared with that same
equality; W3's rule 5 then emits the declared entries, never `in-root:**`, and a page
navigation to an ineligible in-root document is FailedClosed **without** triggering `onLoss`
(no loss counted), so a page cannot close its own window by navigating to an in-root
document; page navigations to `about:srcdoc`, an allowlisted remote document or an error
page still apply `onLoss` (§4.5, §4.6) and can exhaust the three recoveries — a stated
residual for the owner (§10 B3). Each window's entry
resources come from its generated window entry (§4.8 window mapping; entry derivation is
#1192's). Same-document history/hash changes (`SourceChanged` with `IsNewDocument == FALSE`)
keep the epoch; the per-call Source rule (§4.4 step 3) then checks only the scheme and the
tuple origin (`https`) or the empty host and in-root path (`file:`).
A document loaded with `NavigateToString` can never satisfy this rule (its `Source` is
`about:blank`), so route D **requires** the loading-path change in §4.6a.

| WebView2 event | #1192 transition | Host action |
| --- | --- | --- |
| Scripts registered, controller created | Unbound(0) | Nothing admitted; a `bind` here is an early `bind`: refused with zero buffered work, zero nonce and zero authority, never held for the later `ContentLoading` (§4.4, AC29 (d)) |
| `NavigationStarting` (top-level, each redirect hop) | +0 | Rule A″ (§4.6): off-allowlist → `Cancel`, the live document and its IPC survive; record allowed `NavigationId`; allowed or cancelled, add it to the outbound in-flight hold's pending set (§4.4); never post an epoch here |
| `ContentLoading`, rule satisfied | +1 (checked; `MAX` → Exhausted) | Revoke; mint 128-bit nonce; **post nothing**; start the startup deadline (§4.4). No grant, role readiness or ledger reset is inferred from `ContentLoading` alone. The document's first verified `bind` is answered once with its `bound` (§4.3, AC29) |
| `ContentLoading`, rule not satisfied (error page, remote allowlisted document, unknown `NavigationId`; under B3's fallback, a non-entry in-root document) | +1 then FailedClosed | Revoke; no epoch; apply `onLoss`, except for a page navigation to an in-root document under B3's fallback or to an in-root target W3 served as `404`/`403` (AC25 (f)/(h)) |
| `document.open()`/`write` in the application document (E5 g: 5/5 replaced, no `NavigationStarting`, no `ContentLoading`) | +0 | **Host-unvetoable limit #2.** Same document, same epoch, same authority; the page blanked itself. Electron's `will-navigate` cannot see it either. No Host action; recorded in the report |
| Hash / `pushState` / cancelled provisional navigation | +0 | Keep nonce and work (AC16) |
| `NavigationCompleted(IsSuccess == TRUE)` for a pending navigation with no `ContentLoading`, unless at least one `SourceChanged(IsNewDocument == FALSE)` and no `SourceChanged(IsNewDocument == TRUE)` is attributed to it (§4.4 Attribution) (e.g. a back/forward-cache restore, if WebView2 raises no `ContentLoading` for it — UNKNOWN on 156 — or a same-URL restore that raises no `SourceChanged`) | +1 then FailedClosed | Treat as a document change: revoke; release nothing held; apply `onLoss` (AC24 cases 5, 7, 8). The same-document case releases the held posts once the pending set is empty (AC24 case 6) |
| `ProcessFailed(RENDER_PROCESS_EXITED)` (main frame) | +1 then FailedClosed | Empty the pending set; revoke; discard held posts (reservations released); apply `onLoss` on the surviving control through the trusted Host load row below (AC24 case 11, AC28 (a)). Microsoft ([process-related events](https://learn.microsoft.com/microsoft-edge/webview2/concepts/process-related-events#handle-process-crashes)): "If the main frame is impacted (`RenderProcessExited`), you can use the `Reload` API to reload content in your controls. Alternatively, you can `Close` and recreate the WebView2 controls." Whether a `Navigate` to the entry recovers the control as `Reload` does is AC28 (a)'s to show, not that text's; for `file:` the trusted load is W3's header navigation, never `Reload()`, and if it does not recover the control this row is revised before T2 merges. The same page says "The content in impacted frames (main or subframe) is replaced with an error page"; whether that raises `NavigationStarting`/`ContentLoading`, and in which order against the trusted load, is UNKNOWN. A `ContentLoading` for that error page mints nothing (no allowed `NavigationId`; `IsErrorPage`), but the "rule not satisfied" row would apply `onLoss` a second time: AC28 (a) records the sequence, and a second `onLoss` for one exit blocks T2's merge until this row is revised; it is not an accepted divergence |
| `ProcessFailed(BROWSER_PROCESS_EXITED)` (r10, 0monish 4427055014) | +1 then terminal, for every window of that browser | Empty every pending set; revoke every epoch; discard every held post (reservations released); **no** `onLoss`, and no `Navigate`, `Reload` or other engine call on any closed controller. Microsoft (same page): "All associated WebView2 controls will be closed and your application must handle recovery from this failure. The WebView2 controls need to be recreated." The session ends through W2's `AbnormalExit` terminal, not a second one: under #1280 r16 D8 (`be9e9e56`) this `ProcessFailed`, for the bound browser instance, latches `AbnormalExit` at once (its handler queues an abnormal receipt that the loop's next `poll` latches) and no rebind is allowed after it, so the end does not wait for `BrowserProcessExited`, whose order relative to `ProcessFailed` is not guaranteed. If that receipt comes first it latches `AbnormalExit` as on main and the later `ProcessFailed` changes nothing, so either order ends the session once with nothing delivered (§10 Q13; AC24 cases 14–15, AC28 (b)) |
| Other `ProcessFailed` kinds | +0 | No action |
| A navigation pending for `NAV_TERMINAL_BOUND` (§4.4 rule 2 (ii)) | +1 then FailedClosed | `Stop()`; empty the pending set; revoke; discard held posts (reservations released); **no** `onLoss`; keep the expired `NavigationId` for a late `ContentLoading` (AC24 cases 10, 12, 13) |
| Trusted Host load (`onLoss: reload-entry`) | +1 before the engine call (the window's entry is always admissible for Host attempts once W3's rule 1 attributes them, rule A″; W3's unproven mode refuses every Host `file:` attempt at +0, W3 B18) | Revoke, then navigate to the entry (for `file:`, W3's header navigation `NavigateWithWebResourceRequest`, never `Reload()`); bounded to **3 automatic recoveries per window lifetime**, then close with a typed error. A recovery W3 refuses or cannot attribute fails typed, is never re-issued and does not count toward the three (W3 §4.6 rule 1, B7 (vii)); the document stays FailedClosed |
| App `loadURL`/reload (a #459 Host attempt) | allowlist pre-check, then +1 before the engine call | The window's entry, whatever its query, is always admissible once W3's rule 1 attributes the attempt; any other off-allowlist target is a typed refusal to the caller at +0 and the live document is untouched; a `file:` `reload()` is a typed refusal until W3 Q22 (c) |
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
    "t-main": {
      "channels": ["el:rendererReq"],              // generated (AC13)
      "preload": {                                  // developer-declared, same trust as this file
        "digest": "sha256:…",                      // SHA-256 of the preload bytes (AC14)
        "verdict": {                                // recorded by keld migrate; Host reads it
          "digest": "sha256:…",                    // must equal preload.digest (AC19 case vi)
          "classifier": "<keld migrate version>",   // release builds re-run it and compare
          "reachableSinks": ["el:rendererReq"],    // literal sinks reachable from exposures ("*" = page-chosen)
          "flags": [                                // §4.8 rule 2: (a) outbound, (b) inbound, (c) argv, (d) indeterminate
            { "at": "preload.js:96-100", "kind": "page-input-branch" },
            { "at": "preload.js:77-85", "kind": "inbound-filter" }
          ]
        },
        "attestation": {                            // required iff verdict.flags is non-empty (AC19)
          "digest": "sha256:…",                    // must equal preload.digest
          "sites": [                                // must cover every flag
            { "at": "preload.js:96-100", "why": "removes a function value that cannot cross IPC" },
            { "at": "preload.js:77-85", "why": "drops events only for paths the page never registered" }
          ],
          "acknowledgedBy": "<developer>"           // developer acknowledgement; conveys no authority
        }
      },
      "web": {
        "csp": "default",
        "navigation": {
          "allow": ["in-root:**", "https://app.example/**"], // rule A″ file: forms (#541, W3 B12); others: arch 03 §2 scopes
          "onLoss": "reload-entry"                  // or "close"
        }
      }
    }
  }
}
```

- **Navigation rule: W3 #1185 rule A″.** The verbatim rule is owned by #1185 (W3 spec, PR
  #1311 at `1db53482`, r14, open draft, §4.6). W1 binds to its substance; targets are split by
  **who chose them**:
  1. **Page-initiated** top-level navigation proceeds iff the target matches the window's
     `web.navigation.allow`. The allowlist governs **every** page-initiated target, in-root
     `file:` included; in-root membership is **necessary, not sufficient**, and a parser
     failure is off-allowlist. Until W3's Q27 passes (B19), every page-initiated `file:`
     target is off-allowlist whatever the form (W3 §4.6 rule 2): in-root page-to-page
     navigation is cancelled at +0, with no loss and no `onLoss`.
  2. The query is part of page-initiated matching; the fragment is ignored. The `file:` allow
     forms are `none` (no page-initiated `file:` navigation; what a veto-all or unreadable
     `will-navigate` listener derives), a literal in-root component path carrying the query
     constraint of the predicate it was lifted from (an exact query, or no query; under B3's
     fallback W3's rule 5 emits declared entries with a third value, any query), and
     `in-root:**` (every in-root regular file, any query; what the absence of a listener
     derives — conditional on W1's eligibility proposal, §4.5 and §10 B3).
  3. Path components are compared with keld-guard's **one component equality** (§2), which
     W3's allow forms, attribution and W1's eligibility all use; never by spelling. The query
     is compared **byte-exact** on the raw serialized query (the bytes after `?` and before
     `#`, no decoding and no case folding; an empty query `p.html?` differs from no query
     `p.html`, as W3's `Option<String>` does), as W3 §4.6 requires; it never forms document
     identity. A literal lifted from `p.html?mode=a` does not admit `p.html?mode=A`.
  4. The decision is taken on the original `NavigationStarting`, before any generation
     change: off-allowlist → `Cancel`, generation +0, the live document and its IPC survive;
     on-allowlist → proceed, generation +1 only at commit.
  5. `will-navigate` is a notification only; `preventDefault()` is not consulted. The Host
     never replays a page navigation.
  6. **Host attempts** — the first document of a compat window (the application's own
     `loadURL`, #459; W3 r4 withdrew the boot seal, still absent at `1db53482` (W3 A19), and
     `navigate_initial` remains only for
     schema-1 renderer boots), an `onLoss` reload, and the application's `loadURL`/`reload` —
     to the window's entry components are always admissible, whatever the query, once W3's
     rule 1 attributes the event to that attempt by causal proof (its header-proven mode: every
     Host `file:` attempt is a `NavigateWithWebResourceRequest` carrying a Host nonce header; in
     its unproven mode W3 refuses every Host `file:` attempt at +0, W3 B18, Q22); any other
     Host target is checked against the same allowlist **before** its +1, and an
     off-allowlist Host attempt is a typed refusal at +0, never a revoked document. Redirect
     hops of an admitted Host attempt are checked too; a cancelled hop fails that attempt
     closed (#1192), a stated divergence, not an exemption.

  W1 applies it as follows. The one `NavigationStarting` handler and its `SetCancel` decision
  are W3 T4's (W3 A10; W1 adds its pending-set and generation bookkeeping inside it, §4.10).
  The raw `NavigationStarting` `Uri` is dispatched on its scheme
  prefix by `keld_guard::url_scheme_is` (the parser's own R1 test; §4.4 URL owner): a `file:`
  target goes to `file_url_root_relative`, and the returned components and query are matched
  against the `file:` allow forms by W3's keld-guard navigation entrypoint (the forms and the
  entrypoint are W3 B12, #541 owner; here §10 B6); every other `Uri` is matched as the raw,
  Chromium-canonical string by the arch 03 §2 scope matcher. A `Uri` no allowlist entry
  matches is cancelled, including every `about:`/`data:`/`javascript:`/`blob:` URI in any
  letter case: no allowlist entry can name those authorities, so they never match, and no
  scheme list is hand-maintained. `IsUserInitiated` does not relax it. E5 ran the `about:`
  forms (including `ABOUT:BLANK`); `data:`, `blob:` and `javascript:` are not yet run on
  device (AC10). Each window's entry components are its generated window entry (§4.8 window
  mapping).
- **W1 adds nothing to rule A″ and relaxes nothing in it.** A page-initiated navigation is +0
  until its `ContentLoading` (+1 at commit). W1 adds no "+1 before the engine call" load
  other than the bounded `onLoss: reload-entry` recovery (§4.5), a Host attempt to the
  window's entry, which is always admissible once attributed and otherwise refused by W3, never
  by W1. An app-issued
  `loadURL` is a #459 Host attempt under #1192's trusted-load row, admitted or pre-checked
  by item 6, never a replay. This answers W3's B4 and B7 (iv) for W1; §4.5 and §10 B5 answer
  its B7 (vi)–(viii) (#1311 `1db53482`).
- **Navigation membership is not eligibility.** Rule A″ decides whether a navigation
  proceeds; §4.5 decides whether its document gets an epoch, and §4.3 whether it gets the
  app preload. Equivalent spellings of one in-root file are one identity under the one
  component equality, so all three decisions treat them alike. Navigation permission is not
  channel permission.
- Allowed non-application documents (remote content) get **no epoch and `channels: []`**
  (arch 03 §4.3); navigation permission is not channel permission.
- `FrameNavigationStarting` is not cancelled; frames are governed by CSP (§4.7).
- `AdditionalBrowserArguments` stays empty and the runtime's cancellable-`about:` behaviour is
  never disabled (AC11). `NewWindowRequested` stays deny-all (AC12).
- **Host-unvetoable limits (recorded, not handled by a veto):** top-level `about:srcdoc`
  commits an error page despite `Cancel` (E5 b, 5/5; its `Source` is `about:srcdoc`; rule §4.5
  leaves it FailedClosed with `channels: []`; `onLoss` recovers, bounded to 3); and
  `document.open()` replaces the document's content with no navigation event (E5 g, 5/5; no
  Host action, §4.5).
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
  the probe shows `Source` carries the full path. Identity is decided once at commit by that
  verified path (§4.5); the per-call Source rule then requires scheme `file`, an empty host
  and a path that keld-guard's one `file:` URL parser places inside the verified package root
  (§4.4 step 3). Origins are never compared for `file:`: they are opaque. Whether that path
  is safe as identity depends on the path verification (B6), not on the stamp;
- or a Keld-owned virtual host mapped to the verified package root (`DENY_CORS`): identity is
  the verified entry path at commit, and the per-call rule is the tuple-origin check.

The choice is owned by W3 #1185 (file-origin navigation/containment) together with
#1192/KEL-102 (resource identity). W3's spec (PR #1311 at `1db53482`, r14,
open draft; a virtual host is one of its non-goals, §1) proposes the exact `file://` load and the keld-guard parser named above;
until it is approved both options stay open here. T2 carries the `NavTarget::Html` →
verified-URL change
for compat windows once that decision is recorded. Until then AC18 is the standing proof
that the current path is refused.

### 4.7 Frames rule

Same-origin iframes reach `top.<exposed key>` and `top.keld.invoke`; their calls arrive on the
top-level event with the top document's `Source` and nonce and are indistinguishable from the
application (admitted; Electron behaves the same; stated in the report). A same-origin
iframe's **own** `chrome.webview.postMessage` routes to `Frame2`, which is not registered
(AC03). Cross-origin frames cannot reach the top realm (`SecurityError`, E2); for a `file:`
application an in-root iframe is INFERRED to be one too, because Chromium gives `file:`
documents opaque origins, so it reaches neither `top.<key>` nor `top.keld.invoke` (AC03's
receipt records it). Framing is
governed only by `windows.<w>.web.csp` (`frame-src`, `frame-ancestors`).

### 4.8 `keld migrate`: window mapping, classifier, grants

**Window mapping.** Each statically resolvable `new BrowserWindow(opts)` site in the main
process becomes one `windows.<w>` entry. `keld migrate` aliases `electron` and does not
rewrite application source (arch 04), so `w` must be computable from the options the facade
receives at run time. `w` is computed by **one TypeScript function in `@keld/electron`**;
`keld migrate` executes that module (not a `keld-cli` reimplementation) and AC13 runs shared
golden vectors through both callers. If `opts.title` is present it MUST be a string literal
at every site (`keld migrate` refuses a present non-literal title with a typed `KELD-*`), and
`w = "t-" + slug(title)`. Else, if `webPreferences.preload` is present, it MUST resolve
statically under the application root, and `w = "p-" + slug(root-relative, `/`-separated
path)`; the facade derives the same relative path from the Host-admitted root (how the
facade learns that root is open, §10 Q2). Else `w = "default"`. `slug` = NFC, ASCII
lower-case, every maximal run of characters outside `[a-z0-9]` replaced by `-`, trimmed of
`-`. An empty slug is a migrate-time refusal and a run-time no-entry. The `t-`/`p-`
prefixes make the three key classes disjoint by construction. The generated entry records
every site (`file:line`) that maps to it and the resolved preload path; the facade passes
`w` in its window Create request, where the Host binds it immutably (#1191 AC3; the Create
request's new fields — `w` and the preload path below — are a wire change, §4.10). Sites that are not statically resolvable are
listed in the report as able to select any entry at run time.
**Collisions (proposed r7):** when two sites map to one `w` and differ in any generated
field (preload path or digest, verdict, channel grants, navigation allowlist, CSP),
`keld migrate` fails with a typed `KELD-*` duplicate-key diagnostic naming both sites and
`w`, and writes no configuration. It never disambiguates by suffix or keeps the last site: a
source reorder would then move one site's grants to another silently. Sites equal in every
generated field share the entry. The fix text asks for distinct literal titles, which
changes user-visible window titles; a Keld-only key option (ignored by Electron) would avoid
that and is parked as §10 Q2 rather than added here. At run time a `w` with no entry gets no
channel grants and no preload (default-deny, typed diagnostic; #1191 AC1: a missing window
conveys zero incoming channel authority). The Host loads the bytes at the preload path the
facade passes and compares them with the entry digest (no preload vs. an entry with a
preload → refused); rule 3 bounds a key error's channel grants to those generated for the
same preload bytes; navigation, CSP and `onLoss` follow the selected entry and are not
digest-bound.

**Classifier.** Exposure = function reachable from `contextBridge.exposeInMainWorld` or an
assignment to `window.*`/`globalThis.*`. **Main-IPC outbound sink** (r8, 0monish 4423598922) =
a method of the active renderer facade that is implemented and dispatches to main-process
`ipcMain`: today only `ipcRenderer.send` (#1154 `IpcRendererFacade` implements `send` plus
listener operations; `invoke` and `sendSync` are #1154 non-goals). `invoke`, `sendSync` and
`postMessage` become sinks only from the #1154 revision that implements each with a
main-bound dispatch. `sendToHost` addresses the embedding `<webview>` host page, not
`ipcMain` (Electron `v44.2.0` `lib/renderer/api/ipc-renderer.ts`; W1 excludes `<webview>`),
and `sendTo` does not exist in Electron `v44.2.0`; neither is ever main-bound. A reachable
call of an unsupported or non-main-bound method is a typed migration incompatibility in the
report and contributes **no** `el:` grant; the grant intersection below counts main-IPC
sinks only, by what the method dispatches to, not by its name.
**Dispatch kind (r9, 0monish 4425538678).** A sink reaches only the `ipcMain` registrations
of the kind it dispatches to (Electron `v44.2.0` `lib/browser/ipc-dispatch.ts`, commit `aa650d74`): `send` arrives in
the main process as `-ipc-message`, which emits the channel on the frame, WebContents and
`ipcMain` EventEmitters and so reaches `on`/`once` listeners only (L65–83, L72–77); `sendSync` (`-ipc-message-sync`, L126–154) and
`postMessage` (`-ipc-ports`, L160–180) do the same; `invoke` arrives as `-ipc-invoke`, which
looks the channel up only in `_invokeHandlers` (L85–124, L112–114), the map that
`ipcMain.handle`/`handleOnce` fill and `on`/`once` never touch (`lib/browser/ipc-main-impl.ts`
L6, L15–30). So `send` never reaches a `handle`-only channel and `invoke` never reaches an
`on`-only one. Registrations outside `ipcMain` — `WebFrameMain.ipc` and `WebContents.ipc`, which also
receive `send` (L72–77) and `invoke` (L100–112), and the `webContents.on('ipc-message')`
catch-all (L74) — are reported as unsupported on route D and get no grant; `keld migrate`
never ignores them silently. Inbound value = the payload
arguments (after `event`) of an `ipcRenderer.on/once` listener (and, once implemented, the
settled value of an `invoke` promise). Page input =
exposure parameters (including rest parameters and `arguments`) and values derived from them.

> **Gating = narrowing.** For some page input the preload suppresses, refuses, restricts or
> rewrites what reaches a sink — which channel, which arguments, which values — or it keeps
> from the page some Host-delivered value it received. Under route D the page posts directly
> to every granted channel with any codec-valid arguments and reads every Host→renderer
> post (§4.2 divergences 2 and 4), so every such narrowing is lost. A transform that only
> changes **representation** without narrowing what the page can cause or see — for example
> removing a value that cannot cross the IPC boundary anyway — is **not** gating by definition.

A static analyzer generally **cannot prove** that a transform is representation-only (it
would have to know the value domain and the boundary's rejection rules on every path), and
it cannot see whether the main process reads an argument the preload omitted. That is why
rule 2 exists:

> **Rule 2 — the classifier is conservative.** `keld migrate` flags:
>
> - **(a)** every outbound sink call that is not **wholly page-controlled**. Wholly
>   page-controlled: the channel is a literal (its reach is governed by grants, below) or an
>   unmodified exposure parameter; the argument list after the channel is exactly the
>   exposure's remaining arguments, forwarded by identity (`(c, ...xs) => send(c, ...xs)`
>   or `arguments`); and no page-input-dependent branch lies on the path. Hence a
>   page-input-dependent branch or transform, a channel computed from page input
>   (`send('app:' + c, ...xs)`), any write, delete or mutating call on a page-input value or
>   anything reachable from it (`m.x = …`, `delete m.x`, `Object.assign(m, …)`,
>   `Object.defineProperty(m, …)`), a constant argument, an omitted or truncated
>   argument (`(c, x) => send(c, x)` drops trailing arguments), a reordered or overwritten
>   argument, and a preload-assigned field on a page-input object (`m.reqId = n++`) each
>   flag the call;
> - **(b)** every inbound value that does not reach a page-facing callback (or the
>   exposure's return value) unchanged and whole: consumed without forwarding
>   (`on('auth', (e, t) => { token = t; })`), filtered, field-selected, transformed, or
>   truncated (`(e, a) => cb(a)` drops trailing payload arguments);
> - **(c)** every read of `process.argv` — by literal index, search, iteration, spread, or
>   escape of `process`/`process.argv` as a value;
> - **(d)** indeterminate code: dynamic `require`, `eval`/`Function`, computed member access
>   on `ipcRenderer`, aliasing the analyzer cannot resolve (including an aliased or computed
>   read of `process.contextIsolated`), an unresolvable preload path.
>
> A listener's `event` argument is not payload (under the facade it carries no authority),
> so dropping it is not (b). `process.contextIsolated` is constant-folded to `false` before
> reachable sinks are computed (§4.2 divergence 6, AC23).

Rejected narrower variants (recorded so they are not re-proposed without new evidence):

- *Flag a constant, omitted or truncated argument only when the main-process handler reads
  that position.* Rejected: (1) the verdict is bound to the preload digest only, so a later
  main-process change could make a dropped position live without changing the digest — the
  verdict would go stale silently; (2) finding "the handler" is cross-module data flow
  through dynamic registration, wrappers, routers keyed on payload fields (draw.io's
  `rendererReq` switch), rest/`arguments`, defaults and destructuring, where a miss is a
  silent security false negative while a false positive costs one attestation line.
- *Flag only non-literal `process.argv` reads.* Rejected: argv content comes from the main
  process's `additionalArguments` and is page-visible under route D (§4.2 divergence 3); how
  the preload indexes it has no bearing on whether it is secret.
- *Argument templates enforced by the Bun-side `ipcMain` facade* (arity, constant and
  truncation checks on decoded arguments; they check argument shape, which `keld-guard` does
  not, so they would not duplicate it): parked for T0. They would mechanically restore the
  commonest narrowing classes, but they do not cover transforms, preload-assigned fields,
  inbound data or argv, and they need #1154/#1190/#541 sign-off and their own spec. For the
  pinned draw.io preload they would discharge only L111–114 and part of L102. **Decision
  trigger:** T5's corpus run (#496, #587) reports flags by class; if constant, omitted or
  truncated arguments are the only flag class for at least the share of preloads the #1154
  owner sets at T0, templates become a release blocker with their own spec.

Consequence, stated plainly: most real preloads declare fixed parameters, so most will be
flagged under (a)/(b). For those applications the attestation, not the classifier, is the
load-bearing control (rule 3 residual risk).

Classifier falsifiers: a narrowing or data-holding preload MUST be flagged; a pure
pass-through MUST NOT be; a representation-only transform the analyzer cannot prove MUST be
flagged (conservative), never silently passed.

| Fixture | Content | Definition | Classifier |
| --- | --- | --- | --- |
| F-A | `request: (m, ...xs) => { if (ALLOWED.has(m.action)) send('req', m, ...xs); }` | narrowing (page-input branch) | flagged (a) |
| F-B | `sendMessage: (c, ...xs) => send(c, ...xs)` (page chooses the channel) | pass-through | not flagged |
| F-B1 | `doA: (...xs) => send('a', ...xs)` (fixed channel, all arguments forwarded) | pass-through | not flagged |
| F-B2 | `listen: (c, cb) => on(c, (e, ...v) => cb(...v))` | inbound pass-through | not flagged |
| F-C | `openFile: () => send('open-file')` | narrowing (omitted arguments) | flagged (a) |
| F-D | `save: (p, ...xs) => send('save', path.normalize(p), ...xs)` | narrowing (page-input transform) | flagged (a) |
| F-E | `require(name).send(...)` / `eval` | unknown | flagged (d) |
| F-F | `on('evt', (e, ...v) => { if (v[0].ok) cb(...v); })` | narrowing (inbound filter) | flagged (b) |
| F-G | `request: (m) => { m.reqId = n++; if (m.action == 'watchFile') { L[m.path] = m.listener; delete m.listener; } send('req', m); }` (the draw.io shape) | combined: preload-assigned field, a representation-only branch, truncation | flagged (a), three flags |
| F-G1 | `request: (m, ...xs) => { if (m.action == 'watchFile') { L[m.path] = m.listener; delete m.listener; } send('req', m, ...xs); }` | representation-only by the definition (see below), not provable | flagged (a) (conservative) |
| F-H | `getConfig: (...xs) => send('read-config', 'config.json', ...xs)` | narrowing (constant argument) | flagged (a) |
| F-I | `save: (m, ...xs) => { m.user = USER; send('save', m, ...xs); }` | narrowing (write on page input: preload-assigned field) | flagged (a) |
| F-J | `sendMessage: (c, x) => send(c, x)` | narrowing (truncated trailing arguments) | flagged (a) |
| F-K | `on('auth', (e, ...v) => { token = v; })` | data held (consume without forward) | flagged (b) |
| F-L | `onUser: (cb) => on('user', (e, ...v) => cb(v[0].name))` | data held (inbound payload field-selected) | flagged (b) |
| F-M | `const mode = process.argv[1];` | argv read (literal index) | flagged (c) |
| F-N | `const tok = process.argv.find((a) => a.startsWith('--token='));` | argv read (search) | flagged (c) |
| F-P | `prefixed: (c, ...xs) => send('app:' + c, ...xs)` | narrowing (channel computed from page input) | flagged (a); reachable sinks = `ipcMain` literals starting `app:` |
| F-Q | `save: (m, ...xs) => { delete m.extra; send('save', m, ...xs); }` | narrowing (delete on page input) | flagged (a) |
| F-O | `if (process.contextIsolated) contextBridge.exposeInMainWorld('api', { priv: (...xs) => send('priv', ...xs) }); else window.api = { pub: (...xs) => send('pub', ...xs) };` | branch on isolation; constant-folded to `false` | not flagged; `reachableSinks == ["el:pub"]` (AC23) |

F-C, F-H and F-L use the supported sink (`send`) and listener (`on`): a reachable `invoke` is
today a typed incompatibility, not a sink (r8), so their `invoke` forms become fixtures in the
#1154 revision that implements `invoke` (r10 sweep).

**Rule 3 — recorded verdict and attestation (trust model stated plainly).** The preload
digest, the classifier verdict and any attestation are a **developer-declared integrity
check on `keld migrate` output**, stored in `keld.permissions.jsonc` under the window
(`windows.<w>.preload`, §4.6) with **the same trust as the manifest itself** — they are not
a Keld-reviewed signature and no Keld party vouches for them. The Host enforces them
mechanically:

- The verdict carries its own `digest` and the classifier version that produced it; the Host
  refuses a verdict whose `digest` differs from `preload.digest` (AC19 case vi), and a release
  build re-runs the classifier on the admitted bytes and refuses to package a window whose
  result differs from the recorded verdict. So a developer who edits the preload and
  hand-updates `preload.digest` without re-running `keld migrate` gets no grants, not stale
  ones.
- Route-D grants load for `w` only when `preload.digest` **and** a recorded `preload.verdict`
  (flag sites + the reachable literal sink set) are present and the admitted preload bytes
  hash to `preload.digest`; otherwise `w` gets `channels: []`.
- The Host checks `channels(w)` ⊆ `verdict.reachableSinks` (or `reachableSinks` is `"*"`,
  the page-chosen-channel case); a grant outside that set rejects the window's grants.
- A verdict with non-empty `flags` requires `preload.attestation` whose `digest` equals
  `preload.digest` and whose `sites` cover every flag (AC19); an attestation bound to another
  digest is refused. `acknowledgedBy` is a developer acknowledgement and conveys no
  authority. An attestation **never widens grants**: `channels(w)` stays exactly the §4.8 **Grants**
  intersection below.
- `KELD-GUARD003` stays #1191 AC5's frozen wire error for every `ChannelForbidden` denial,
  route-D windows included: same variant, code, kind, fix text ("Add `{channel}` to this
  principal's channels list in keld.permissions.jsonc.") and `CallError` mapping (r10,
  0monish 4427025547; #1191 `4f246a75` `docs/specs/gh552-exact-channel-grants-guard.md:48` (AC5) and `:140`; `keld-guard` `src/lib.rs:392-394`, asserted at `tests/acl.rs:562-574`, at `457a9c3b`).
  The route-D guidance — re-run `keld migrate`, which regenerates the verdict and grants;
  do not hand-edit `windows.<w>.channels` — lives only in trusted tooling output: `keld
  migrate`'s report for each route-D window (AC09). A hand-added channel outside
  `reachableSinks` is rejected anyway (above). Changing the `KELD-GUARD003` text would need an
  owner-approved #1191/#541 amendment with source and wire tests; this spec asks for none.

**Residual risk, stated plainly.** Without a matching attestation a flagged application's
window gets no grants and is reported unsupported for route D (route B is the documented
escalation; it has no approved spec). With an attestation, the developer asserts that every
flagged site narrows nothing and holds nothing secret. **If that assertion is wrong, the
narrowing is silently lost**: the page can make the calls or read the data the preload
withheld, and nothing in Keld detects it. Keld reviews **this mechanism** under the
permission-model gate (§8), not each application's attestation. The single exception is
Keld's own qualification claim: the T6 attestation for the pinned qualification application
is authored by the W1 implementer and independently reviewed under the permission-model gate
as qualification evidence (§6 T6), because Keld publishes a result that rests on it.

Qualification evidence, stated plainly. Rule 2 applied to the pinned draw.io preload
(`jgraph/drawio-desktop@2edf9fb9`, `src/main/electron-preload.js`, blob `44f2f22f`, read
directly) flags seven sites. Its main-process counterpart is `src/main/electron.js` (blob
`b57b5e20`) at the same commit.

| Lines | Code | Rule | What the T6 attestation must establish |
| --- | --- | --- | --- |
| 14–15 | `process.argv.find((a) => a.startsWith('--initial-adaptive-colors='))` | (c) | the value is one of three UI modes (validated L21; set by `electron.js` L707–711), not a secret |
| 61–75 | `on('mainResp', …)` routes by `resp.reqId`, passes `resp.data` or `resp.msg, resp.e` | (b) field-selected | the page initiated every request and, under route D, reads every `mainResp` post anyway |
| 77–85 | `on('fileChanged', …)` looks up `resp.path`; forwards `curr, prev` only if a listener exists | (b) filter | events dropped are only for paths the page never registered |
| 89–103 | `request`: L91 `msg.reqId = reqId++` (preload-assigned field); L96–100 page-input branch deleting `msg.listener`; L102 `send('rendererReq', msg)` forwards `msg` only | (a) | L91: `reqId` only correlates replies — under route D the page can choose it (in Electron it cannot), affecting only its own reply routing; L96–100: removes a function that cannot cross IPC (below); L102: the `rendererReq` handler declares only `(event, args)` (`electron.js` L4431) |
| 104–110 | `registerMsgListener`: `on(action, (event, args) => callback(args))` | (b) truncated | trailing payload arguments, if the main process sends any, become page-visible under route D |
| 111–114 | `sendMessage`: `send(action, args)` | (a) truncated | the page can add trailing arguments under route D; each reachable handler must ignore them |
| 115–121 | `listenOnce` | (b) truncated | as L104–110 |

Not a flag: L46–55 (a `navigation` listener vetoing `about:`) is reported as replaced by the
Host navigation policy (§4.6, AC10); L12–38 seed `localStorage` (no IPC).

On L96–100: by the definition the branch is representation-only — it moves the
function-valued `msg.listener` out of the message before `ipcRenderer.send`, and a function
cannot cross the IPC boundary in either runtime. Electron documents that `ipcRenderer.send`
arguments are serialized with the Structured Clone Algorithm and that "sending Functions,
Promises, Symbols, WeakMaps, or WeakSets will throw an exception"
([ipcRenderer.send](https://www.electronjs.org/docs/latest/api/ipc-renderer#ipcrenderersendchannel-args),
current docs; the pinned v44 wording is INFERRED identical), and #1190 V3 refuses a nested
Function synchronously before any send. The conservative classifier still flags it (it cannot
prove the value is always a function). **So the pinned draw.io preload qualifies on route D
only with an attestation for its digest covering all seven sites** — through the framework
mechanism of rule 3, not an application exception. Whether draw.io holds any authority
beyond `reqId` assignment depends on its main-process handlers ignoring trailing
arguments; that is what the T6 review reads, and it is not established here.

**Grants.** For window `w` with preload `P` (verdict pass-through):
`channels(w)` = the union, over each supported main-IPC sink `s` reachable from `P`'s
exposures, of (literal channels `s` can carry) **∩** (literal channels of the `ipcMain`
registration kinds `s` dispatches to). Today `s` is only `send`, whose kinds are `on`/`once`:
`channels(w)` = (`send` sink channel literals) ∩ (`ipcMain.on`/`once` literals). When #1154
implements `invoke`, it adds (`invoke` sink channel literals) ∩ (`ipcMain.handle`/`handleOnce`
literals); `sendSync` and `postMessage` would pair with `on`/`once`. An unsupported or
non-main-bound method contributes nothing. If a reachable sink takes its channel from page
input unchanged (F-B), its term is the full literal set of **its** kinds (for `send`, every
`on`/`once` literal, never a `handle`/`handleOnce` one); a channel computed from page input by
literal concatenation (F-P) reaches only that kind's literals of that form; any other
computed channel yields that kind's full set, and the report states that over-grant
explicitly. The report names every `ipcMain` literal left ungranted and why (for example
`invoke`-only, unsupported on route D). The #1154 revision that adds `invoke` must also make
sinks and grants kind-qualified (or add a Host-side kind check) and record
`verdict.reachableSinks` per kind: today an `el:<c>` grant and a `reachableSinks` entry (or
`"*"`) carry no kind, so neither distinguishes a `send` from an `invoke` on that channel. Dynamic
`ipcMain` names are reported ungranted. The report also lists every preload read of
`process.contextIsolated` (AC23). A flagged `P` **without** an attestation for its
digest yields `channels: []` and the report line `✘ preload has N flagged sites that may
narrow authority or hold Host-delivered data (<file>:<line> <rule>, …) — unsupported on Windows route D without an attestation for digest
<d>; escalation: route B (no approved spec)`. A flagged `P` **with** a matching attestation
yields the same intersection as a pass-through `P` — never more. `keld migrate` writes
`windows.<w>.preload.digest` (SHA-256 of `P`'s bytes, #1155 AC08 digest),
`windows.<w>.preload.verdict` (`digest`, `classifier`, `reachableSinks`, `flags`) and, when the developer supplies
one, `windows.<w>.preload.attestation`; the Host applies rule 3 (AC14, AC19). Before release
reliance, run across the compat corpus (#496, #587) and record false positives/negatives.

### 4.9 Rejected alternatives

| Route | Why rejected |
| --- | --- |
| A. CDP `Page.addScriptToEvaluateOnNewDocument` with a named `worldName` world + `Runtime.addBinding` | Universal access: cross-origin XHR upload events fired in every such world, 0 in the main world or a grant-free world (P3) — more than an Electron preload; CDP unmaintained by Microsoft (S2); `worldName` experimental (S3). |
| Supported WebView2 isolated-world API | None exists as of SDK 1.0.4314.35 / runtime 157: `webview2-com-sys` 0.38.2 bindings (SDK 1.0.3650.58) have no `isolat`/`world` symbol; SDK notes 1.0.3065.39–1.0.4314.35 and runtime notes 130–157 document none; WebView2Feedback #554 (Microsoft, 2020-10-21, open, `tracked`, `priority-low`): "we don't currently have a way to execute scripts in a context other than the current page" (note 264 S1, S7). |
| B. `Page.createIsolatedWorld` with `grantUniveralAccess: false` behind a page `debugger;` pause | It does create a grant-free world before page script (36/36, P3-A), the only one of the mechanisms tried in P3 that did (creation on `executionContextCreated` or `frameNavigated` lost to page script 24/24 each; `setInstrumentationBreakpoint` deadlocked; a grant parameter on `addScriptToEvaluateOnNewDocument` was silently ignored). But Debugger stays on, renderer pauses on the Host UI thread per document and per page `debugger;`, two silent failures, undocumented pause semantics. Escalation only; no spec. |
| C. MV3 extension content script | Profile persistence, no host channel without CDP (#3851), extension-origin authority. |
| E. Real preload in the main world as the boundary | Page poisons it; no Host rule compensates. |
| Status quo | No `contextIsolation` preload support on Windows. |

### 4.10 Capabilities, wire, platform, seam, migration

- **Manifest:** `windows.<w>.web.navigation` and `windows.<w>.preload` (`digest`, `verdict`,
  `attestation`; schema owner #541/#1191, §10 Q2); channel grants per #1191.
  Permission-model gate.
- **Wire:** one KIPC change — the `@keld/electron` window Create request carries the window
  key `w` and the preload path the facade received (§4.8); new private WebMessage string carriage of #540's CompatV2 forms, unchanged
  (no added kind, `JsonBytes` payloads), with `bound` answering a verified `bind`. Wire gate;
  #540 (§10 Q1, RESOLVED).
- **Public API:** `window.keld.invoke` unchanged; `@keld/electron` renderer facade reachable
  on Windows; `keld migrate` report/manifest keys; `WebEngine`/`WebviewSpec` extension
  (window key, navigation policy, admitted preload bytes + digest) — trait review, live
  backend in the same PR.
- **Platform:** Windows only; macOS keeps #1155/KEL-142; Linux UNKNOWN.
- **Runtime seam:** `create` additionally registers `WebMessageReceived` identity and the
  document-created script inside the `GuardInstalled` mint, then navigates. One handler per
  WebView2 event and view: W1's `NavigationStarting` hold and generation bookkeeping, its
  `ContentLoading` epoch and its `SourceChanged` attribution run inside W3's registrations (W3
  owns the `SetCancel` decision: rule 1 and B19's cancel in W3 T2, rule 2's allow forms in
  W3 T4; #1311 §6, §8), and its `ProcessFailed` kind dispatch inside the
  per-view handler W2 r16 D8 registers (`be9e9e56`); W1 adds no second registration. STA thread; renderer crash ≠ Host;
  handlers live for the WebView lifetime; manifest and policy snapshotted once (#1191 AC1).
- **Migration unit:** `WebView2Engine::create`, `WebviewSpec`, `app_session` forwarder,
  `@keld/electron` facade/transport, the `WindowRequest::Create` codec (window key, preload path),
  `keld migrate`. No temporary adapter; no persisted
  state beyond the manifest. Compatibility fallback: not required.

## 5. Boundaries

- Implement in: `crates/keld-wv/src/webview2/` (identity, epoch, navigation policy, script
  registration; one owner for the `GuardInstalled` mint); `crates/keld-wv/src/engine.rs`;
  `crates/keld-core/src/app_session.rs` (**the** forwarder, shared with #1191 T4; all
  `dispatch_privileged` calls live here); `packages/@keld/electron` (facade + Windows
  transport + `BrowserWindow` window key); `crates/keld-ipc/src/window.rs` +
  `tests/fixtures/window-v0.tsv` and `packages/@keld/api/src/window-codec.ts` (window-key and
  preload-path fields only); `crates/keld-cli` (`migrate`); `crates/keld-guard` URL helpers only (`url_scheme_is`,
  `https_tuple_origin`; W3 owns the parser, the component equality and the navigation
  entrypoint); `docs/architecture/
  03,04,05` (T0 deviations); `keld-compat` conformance/scoreboard entries (T1).
- Must not touch: `crates/keld-guard` grammar/evaluator (#1191/#541); `crates/keld-ipc`
  framing; the KIPC channel table (#508); macOS `wkwebview/*`; workspace `Cargo.toml`;
  `keld.config.ts` schema beyond the generated window entries.

## 6. Tasks (each ≈ one PR; ordered; owner; blocking dependency)

- [ ] **T0 — this spec + architecture deviations** (owner: @amishabenramani). Blocks on
  every §10 blocker: KEL-142 owner sign-off of the §5.1 Windows amendment (B1); tracker
  amendment (B2); the owners' answers and dependencies of B3–B6; the discriminating admission
  probe (B7); the OCR rerun (B8); and the owner answer to §10 Q2 (§10 Q1 is RESOLVED).
- [ ] **T1 — conformance entries first** (owner: `keld-compat` owner, #464/#567). Expected-fail
  cells and one scoreboard divergence entry per §4.2 accepted divergence (1–11), plus the
  `about:srcdoc`/`document.open` limits. Blocks on T0.
- [ ] **T2 — loading path, navigation policy, eligibility and epoch in `keld-wv`** (owner:
  Windows W1 implementer). Replace `NavTarget::Html` for compat windows with the decided
  Host-identifiable URL (§4.6a); AC01 (`window.keld.invoke` half), AC03–AC06, AC07 (floor half), AC10–AC12, AC16–AC18, AC20, AC21, AC24, AC25 (epoch, `onLoss` and pure cells), AC26, AC28, AC29; one generation source
  for media and channel principals (KEL-102 owner coordination). Blocks on: W3 #1185 +
  #1192/KEL-102 recording the loading-path decision and #541 the `file:` allow forms (B6);
  the non-entry eligibility decision (B3); #541 approving `web.navigation` and
  `preload.digest` keys (Q2); the Windows resource-identity oracle (B3); the Windows trust
  root (B4); #540's CompatV2 mode of `decode_wv_link` landed (§10 Q1 is RESOLVED; T2 implements its bind→bound order, AC26, AC29); W3 T2 and T4 merged (#1311 `1db53482` §6: the parser, served-record credit and `file:` handler; attribution and the allowlist-first cancel) with W3 Q22 (a) and (b) passed on the supported runtime (W3 B18, B19; without them no Windows `file:` application ships, §10 B6); W2 #1280 T1 (the schema-2 Windows boot, W3 B13) and T3 (managed windows with r16 D8's browser `ProcessFailed` latch, `be9e9e56`), which AC24 cases 14–15 and AC28 need (§10 Q13 (a)); and the recorded allocation exception for the per-message parse, or an allocation-free parse (§9).
  Floor-only bridge (`window.keld.invoke`).
- [ ] **T3 — grant and forward** (owner: `keld-core`/#1191 T4 implementer). AC02, AC09, AC14,
  and AC19 (i)–(vi): the Host-side verdict and attestation checks of §4.8 rule 3 land with the
  first grant (fixture manifests here; `keld migrate` output from T5 on). Route-D grants stay
  off in release builds until T5's release-build recompute (AC19 (vii)) lands.
  Blocks on T2, #1191 T1–T4 merged and #540's CompatV2 mode of `decode_wv_link` landed (§10
  Q1 RESOLVED).
- [ ] **T4 — `@keld/electron` facade on the Windows transport** (owner: #1154 implementer).
  Wrapper execution gated on a top-level `file:` document (§4.3, option (b′)); `process.contextIsolated === false` (AC23 (a));
  divergences wired to T1 entries; the window key and preload path in the Create request
  (GH-531 owner review) and the one window-key function (§4.8); AC01's exposed keys, AC07's
  void-API half, AC08, AC25's preload-API cells and gate mutants, AC27 (its cross-window same-role cell included), AC30. Blocks on T3, on #466's `ipcMain` listener registry (#1122 §5) and, for
  AC27's `handle` cells, on #547's invoke-handler registry (#466 leaves `handle`/`handleOnce`
  to F04-T14).
- [ ] **T5 — `keld migrate` mapping, classifier, grants, digest, attestation** (owner:
  `keld-cli`/KEL-17 owner; Host-side refusal in T3's forwarder owner). AC13, AC15, AC19 (vii) (the release-build recompute), AC23 (b), AC09's report half;
  fixtures F-A..F-Q; corpus run reporting flags by class (§4.8 decision trigger). Blocks on KEL-17 admitting the first live `migrate` slice
  (Q9) and on T4's window-key function, which `keld migrate` executes (§4.8).
- [ ] **T6 — Windows qualification receipt** (owner: @amishabenramani). One corpus application
  whose preload is pass-through or carries a same-digest attestation covering every flagged
  site (§4.8 rule 3); full RED matrix including AC22; AC17. Because this is Keld's own
  qualification claim, its attestation (for draw.io: the seven sites of §4.8) is authored by
  the W1 implementer and reviewed by the independent permission-model reviewer, who reads
  each flagged site against the pinned main-process handlers; third-party attestations are
  never reviewed by Keld. **Send-capacity qualification gate (§10 Q1(d)):** trace the real
  pinned draw.io startup and its relevant workflows and count consecutive sends before the
  first `send-result`, across all windows on one Bun role; if a second such send occurs and
  its loss prevents required behaviour, first-proof compatibility is **not** qualified with
  this profile, and #545/#540 must raise a measured, finite, aggregate quota by reviewed
  amendment and retest max/max+1, retirement, DATA/CONTROL isolation and recovery;
  `send_records` is not raised arbitrarily. Blocks on T2–T5 and that review.

## 7. Test plan

| Test | AC | Shape and anti-flake |
| --- | --- | --- |
| `wv2_first_inline_floor` (device, ignored-by-default, dedicated STA harness like `media_acceptance`) | AC01 | First inline writes `typeof window.keld.invoke` + exposed keys to `document.title`; Host reads after `NavigationCompleted`. Mutant installs via `ExecuteScript` at DOMContentLoaded. |
| `wv2_poisoned_page_no_widening` | AC02 | Poison then call granted/ungranted/forged; forwarder + fake Bun counters. |
| `wv2_same_origin_iframe_frame2` | AC03 | An in-root iframe at the entry URL receives the current nonce from the top document by `window.postMessage` and posts a v2 floor `invoke` via its own `chrome.webview`; assert 0 top-level events and 0 Echo entries; mutant registers `Frame2`. |
| `wv2_eligibility_matrix` | AC04 | Device: (a) the `file:` entry the Host loads; (b) a remote document and (c) an error page at an allowlisted path, both on loopback `https` (port 0), (c) from an empty-body HTTP 500 (that Chromium then commits its own error page, `IsErrorPage == TRUE`, is INFERRED; the receipt records it); (d) `about:srcdoc`; `bound`-post counter per document. The three mutants are decided by `wv2_epoch_mint_predicate`; on device they are defence in depth. |
| `wv2_epoch_mint_predicate` (pure Rust) | AC04, AC25 | §4.5's mint predicate over stub inputs, one otherwise-qualifying input per term: `IsErrorPage == TRUE` with a status-200 record; URL text equal to the entry with no verified identity; a `NavigationId` no allowed `NavigationStarting` recorded; a W3 served record with status `404` or `403`. Baseline: no epoch for any; each single-term mutant mints one → RED. |
| `wv2_source_rule` + `guard_file_url_source_vectors` (pure Rust; R7/R8 vectors Windows-only, W3 #1311 `1db53482` §4.4) | AC05 | Device: calls from the app document and from documents the Host loaded at a mixed-case in-root spelling and under the non-ASCII root (admitted), and from the `about:srcdoc` error page and an allowlisted remote document (refused at `NoDocument`, defence in depth); each recorded `Source` string joins the vectors. Pure Rust (the deciding cells, each fed the current epoch): those strings, an out-of-root file URL, a UNC `file://server/share/…` URL, `about:blank`, `file://localhost/…` (Chromium likely strips `localhost`) and the controls, through `file_url_root_relative` and the https tuple check. Five mutants (serialised-origin compare, `Url::origin()` compare, scheme-only, case-sensitive, byte matcher). |
| `wv2_epoch_rotation` | AC06, AC16 | A→B cross-origin, A→A2 by the application's `loadURL` of the entry, hash/pushState; host-replayed A envelopes after B's `ContentLoading`; mutant admits by Source + generation. |
| `wv2_not_ready_refusal` | AC07 | Fixture calls before epoch, each confirmed by the AC26 recorder to precede `bound` (at-risk count above 0); assert `undefined` (T4)/typed rejection, 0 queued, later success; mutant buffers. |
| `wv2_stale_post_dropped` (T4) | AC08 | A `profile-test-hooks` hook posts an e₁-stamped event through `PostWebMessageAsString` after B commits, bypassing §4.4 rule 1; B's facade listener counter 0; mutant removes the bridge check. Correctness guard only. |
| `wv2_delivery_hold_state_machine` (pure Rust; cases 14–15 Windows-only) | AC24 | Feeds the fifteen event sequences into the Host→renderer delivery state machine with a stub engine that records posts, `Stop()`, `Navigate` and `Reload` calls, `onLoss` and a #540 reservation counter, and a stub clock for `NAV_TERMINAL_BOUND`; cases (14) and (15) also drive W2's real barrier (`ProfileReleaseWait`; its `webview2` module is Windows-only, `keld-wv/src/lib.rs:104-105`) and read its latched fault and binding, so they run on Windows and the other thirteen on every OS; fourteen mutants. No device, no wall-clock timing. |
| `wv2_host_post_generation` | AC21 | Bun handler reply gated on a test latch released after B's `ContentLoading` is processed (deterministic; no sleeps); B's first inline installs a raw `chrome.webview` `message` counter; mutant removes the Host generation check. |
| `wv2_route_d_premise_matrix` | AC22 | One hostile page × five single-fault builds (a)–(e), each its own process, baseline first and last; Bun-side handler-entry counter keyed by channel + argument list, page raw-listener counter; zero-effect baseline. |
| `wv2_isolation_not_reported` | AC23 | Fixture preload exposes `process.contextIsolated`; first inline reads it; mutant reports `true`. |
| `guard_channel_literal` (#1191 tests) + `wv2_guard003_wire_parity` (T3) + `migrate_route_d_report_advice` (T5) | AC09 | Pure Rust (#1191). T3: an authentic route-D `el:c` miss through the forwarder; the `CallError` bytes compared with `keld-guard`'s `ChannelForbidden` rendering for `el:c`; app-link write and Bun counters 0. T5: the generated report for a route-D window contains the regeneration line. Mutants: route-D text substituted into the fix → byte mismatch; report line dropped. |
| `wv2_string_load_refused` | AC18 | Device: load the fixture with `NavigateToString`; assert `Source == about:blank`, `bound`-post counter 0, calls refused not-ready, FailedClosed with the fixture's `onLoss: close`; mutant qualifies `about:blank`. |
| `migrate_verdict_binding` | AC19 | Pure Rust over manifests for cases (i)–(vi) (the Host-side checks: fixtures in T3, generated manifests from T5) and (vii) (the release-build recompute, T5); five mutants. |
| `wv2_identity_at_commit` | AC20 | Device: `pushState`/`replaceState`/hash then granted calls, including one posted immediately after `pushState`; mutant compares the full `Source`. |
| `wv2_preload_document_matrix` (device) + `guard_component_equality` (pure Rust) | AC25 | Device: admitted root `D:/Keld Test/Ünï/`, allowlist `in-root:**` plus one remote scope, cells (a)–(d) loaded by the application's `loadURL`, (e)–(h) by page navigation ((e), (f) and (h) blocked until W3 Q27); per cell the committed `Uri`/`Source` bytes, `location.protocol`, AC01's first-inline oracle and the Host's `bound`-post counter; baseline first and last. Pure Rust: the shared component equality over `index.html`/`INDEX.HTML`/`index%2Ehtml` and its single call site in W1. Four mutants (location-string gate; preload in every document; byte-exact W1 comparison; served record attached regardless of status) — the last decided by `wv2_epoch_mint_predicate` while (f) is blocked. |
| `wv2_navigation_matrix` | AC10 | E5 triggers × destinations plus `data:`/`blob:`/`javascript:` in lower and upper case, 5 veto + 3 control runs each, with/without `SendInput` activation; heartbeat oracle; four mutants (exact denylist; in-root membership treated as sufficient; query cut before page-initiated matching; query folded through the component equality); the three `file:` mutants run in W3's pure `guard_navigation_allow_forms` while W3 B19 cancels page-initiated `file:` targets on device. |
| `wv2_environment_args_empty` + source grep | AC11 | Unit. |
| `wv2_popup_handled` | AC12 | Existing KEL-168 test. |
| `migrate_grant_generation` | AC13 | Golden manifest for F-B, F-B1, attested F-C and attested F-P against a fixed `ipcMain` set, plus the window-key fixtures and shared key-function golden vectors; eight mutants (grant all literals; collect only `on`/`handle`; F-P's computed channel treated as page-chosen; suffix disambiguation; last site wins; key-class prefixes dropped; `sendToHost` counted as a main-IPC sink; every `ipcMain` registration kind treated as reachable by `send`). |
| `wv2_bridge_wire_compat_v2` (device; T2) | AC26 | T2's actual bridge text and admission path, real `decode_wv_link` in CompatV2 mode, the KEL-142 floor; the renderer-side recorder and Host decoded/`bind`-receipt/`bound`-post/Echo counters; pure-Rust `bind`/`bound` vector checks (21/174 bytes); seven refused raw posts (v2 `hello`, v1 `invoke`, base64url `send` payload, v2 `epoch` kind, the page-posted `bound` vector, a duplicated-`document` `send`, a non-string object), each followed by a floor `invoke` issued after the previous `invoke-result`; baseline first and last; eight mutants (LegacyV1 fallback in the endpoint; base64url payload accepted; unknown kind admitted; `hello` aliased to `bind`; r8's v1 bridge text; `get_WebMessageAsJson` read; native kinds accepted from the renderer; a duplicated key overwrites), each in its own process. Runs once #540's CompatV2 mode lands and T2 implements the bind→bound order (§10 Q1 RESOLVED). |
| `wv2_facade_send_trace` (device; T4, needs T3) | AC27 | The AC26 harness plus T3's forwarder, T4's facade and a Bun fixture on #466's listener registry and #547's invoke-handler registry with per-listener and per-handler counters and the #540 ledger counter; asserts the 123-byte `send` and 120-byte `consumed` vectors (pure Rust), the per-direction renderer traces, the early second `send` a dropped call (local `recordBusy`, no post, no Bun call, no credit), the cross-window same-role `send` refused by the Host/role ledger, `send-result` before message 2, readiness from `bound.ready`/`route-state`, `on('x')` 1 and `handle('x')` 0, and the `danger` and r8-exposure refusals; six mutants (#547's handler registry merged into #466's listener registry; bridge skips its `outstanding` check; bridge never clears its record on `send-result`; bridge ignores `bound.ready`; bridge drops `consumed`; DATA ledger keyed per document instead of per role). |
| `wv2_bind_bound_startup` (device; T2) | AC29 | Cells (a)–(h) on the dedicated STA harness, baseline first and last; renderer recorder plus Host counters for `bind` receipts (with `Source`), `bound` posts (with target document and the `bind` answered), mints, typed startup failures, retries and Echo calls; the replay and held-`bound` hooks are `profile-test-hooks` fixtures and the deferred, listener-less and bind-less bridges are fixture builds; eight mutants (`bind` never answered; `bound` pushed at `ContentLoading`; no startup deadline; early `bind` held and answered; `bind` Source check skipped; fresh mint per `bind`; final-post check skipped for `bound`; per-WebView `bind` latch), each in its own process. A timeout with no `bound` is a typed startup failure, never a pass. |
| `wv2_nonce_grants_nothing` (device; T4) | AC30 | Cells (a)–(e) are the named ACs' tests, run against a page that read the nonce from its own raw listener; cell (f) adds the forged `consumed` on the AC27 harness with the #540 ledger release counter; one mutant (the ledger releases on a `document` match without the exact key). |
| `wv2_process_failure_split` (device; T2) | AC28 | Two runs per kind on the dedicated STA harness, baseline first and last; renderer and browser terminated by `profile-test-hooks` fixtures; navigation-event log, per-controller engine-call log, Echo counter, which of `ProcessFailed` and `BrowserProcessExited` came first, session-end record; two mutants (`onLoss` after browser exit, a positive control decided by AC24 case 14; renderer exit treated as browser exit), each in its own process. |
| `wv2_digest_binding` | AC14 | Tamper one preload byte after its digest was recorded (a manifest fixture in T3; `keld migrate` output from T6 on); assert `channels: []` + typed error; mutant ignores digest. |
| `migrate_classifier_fixtures` | AC15, AC23(b) | F-A..F-Q + pinned draw.io preload bytes (expected: exactly the seven sites of §4.8); one mutant per rule plus the over-flag and F-G conservatism mutants. |
| Receipt validator | AC17 | `tools/` check of the acceptance comment fields. |

Anti-flake: device tests behind the existing Windows feature flag on the dedicated STA
harness; loopback servers bind port 0; completion observed through callbacks pumped by
`wait_with_pump`, no sleeps; each mutant in its own process.

**Windows WebMessage fixture vectors (§10 Q1).** Byte-exact WebMessage STRING literals (ASCII
byte counts; no outer JSON string wrapper at the decoder), appended to #1122 §7.1's existing W1
row (owner #465) — not another grammar or a parallel test owner:

- `bind`, 21 bytes: `{"v":2,"kind":"bind"}`
- `bound`, 174 bytes: `{"v":2,"kind":"bound","document":"0123456789abcdef0123456789abcdef","limits":{"payload_bytes":4096,"channel_utf8_bytes":256,"send_records":1,"event_records":1},"ready":false}`
- `send`, 123 bytes: `{"v":2,"kind":"send","document":"0123456789abcdef0123456789abcdef","message":1,"channel":"rendererReq","payload":[0,1,255]}`
- `consumed`, 120 bytes: `{"v":2,"kind":"consumed","document":"0123456789abcdef0123456789abcdef","delivery":1,"disposition":{"status":"consumed"}}`

The remaining unchanged forms (`invoke`, `invoke-result`, `send-result`, `event`,
`route-state`), malformed, duplicate and unknown fields and version-fallback mutants run through
the actual v2 decoder there, with one final live-document check on native outgoing posts, one
terminal per sequence, true Host/Bun credit release and a healthy follow-up. Each receipt records
the Windows runtime version, OS build, the W1 and decoder exact heads, independent
message/effect/credit counters, the failing mutants and the native control's actual event order.

## 8. Review gates triggered

- **Permission model — mandatory, independent reviewer per `.agents/coordination.md`:**
  eligibility-before-epoch rule, Source rule, navigation matcher reuse, generated grants
  (intersection + digest binding), no second evaluator, no wildcard; the **verdict/attestation
  mechanism** (§4.8 rule 3: developer-declared, manifest-trust, Host-checked ⊆ reachable
  sinks, same-digest requirement) is reviewed here — third-party attestations are not; the
  T6 qualification attestation is reviewed here as qualification evidence (§6 T6); the
  Host→renderer generation check and in-flight hold (§4.4, AC21, AC24); the per-call Source
  rule's use of the shared `file:` URL parser (§4.4 step 3); rule A″ adopted from W3 (§4.6);
  the one component equality; the proposed eligibility of every Host-served in-root document
  and the preload gate (§4.3, §4.5, AC25).
- **Public API — mandatory:** `WebEngine`/`WebviewSpec`; `@keld/electron` facade on Windows;
  `keld migrate` keys and report; KEL-142 §5.1 Windows amendment; `keld-guard` URL helpers
  `url_scheme_is` (R1's code, exported) and `https_tuple_origin` (keld-guard owner; W3 owns
  the parser, the component equality and the navigation entrypoint).
- **Wire protocol — #540 owner:** WebMessage string carriage of the unchanged CompatV2 forms
  (no added kind, `JsonBytes` payloads, one decoder), `bound` answering a verified `bind`, the
  page-readable `document` nonce, and epoch-stamped Host→renderer envelopes (§10 Q1, RESOLVED;
  the decision approves no W1 text).
- **Wire protocol — window-payload owner (GH-531):** a window key and a preload path in `WindowRequest::Create`
  (`crates/keld-ipc/src/window.rs`, `window-v0.tsv` vectors, `@keld/api` `window-codec.ts`);
  discriminants are append-only and `Create` is `Copy`, so the key travels in a new appended
  variant or a versioned vector set, decided by that owner (§4.8).
- **unsafe:** new COM calls W1 owns in `keld-wv/webview2` (`add_WebMessageReceived`,
  `PostWebMessageAsString`, `AddScriptToExecuteOnDocumentCreated`, `Stop()`); W1's code inside
  handlers that others register adds no registration (`NavigationStarting` with `SetCancel`,
  `add_ContentLoading` and `add_SourceChanged`: W3, #1311 §8; `add_ProcessFailed`: W2 r16 D8);
  local `// SAFETY:` proofs; issue-scoped update of
  `crates/keld-wv/AGENTS.md`'s Windows FFI list; independent unsafe gate on the final diff.
- **Dependency addition: yes.** `url` 2.5.8 as a direct, workspace-pinned dependency of
  `keld-guard` for `https_tuple_origin` (§4.4 URL owner). The version is already in
  `Cargo.lock` through `tao`/`wry`, but the dependency gate (`.agents/dependencies.md`) and
  the keld-guard owner's acceptance still apply.

## 9. Perf impact

- First paint (arch 01 §5): wry's **blocking** `AddScriptToExecuteOnDocumentCreated` cost
  96–109 ms (`docs/agents/learnings.md`, KEL-65); the asynchronous form's critical-path cost
  is UNKNOWN and MUST be measured with the KEL-64 beacon before T2 merges (>5 % → waiver).
- Per message: the bounded envelope parse allocates, and so does the per-call Source parse
  (`https_tuple_origin` through `url`, or `file_url_root_relative`'s component vector) — both
  in `keld-wv`'s identity steps, before and outside keld-guard's evaluator, whose Allow path
  stays zero-allocation (#1191 AC6). That per-message allocation on the engine event loop
  needs its own entry in the recorded allocation exception W3 §10 B10 asks for (B10 at
  #1311 `1db53482` names only W3's payloads) or an allocation-free parse; T2 measures it and
  does not merge without one of the two. Report envelope parse / Source parse / decide /
  forward as separate samples.
- Per navigation: nonce mint + one `bound` post in reply to `bind`; negligible, measured anyway.

## 10. Approval blockers and open questions

**Blockers (must clear before `Status: approved`):**

- **B1 KEL-142 §5.1 amendment (Windows):** the page-world bridge carries the document nonce;
  KEL-142 owner sign-off required. The #540 wire owner's §10 Q1(c) decision keeps the nonce on
  route D as a page-readable correlation tag; it does not approve this separate security
  amendment, which stays mandatory and blocking.
- **B2 Tracker amendment (#1184 owner) — OPEN until the #1184 body carries the text
  below.** The amended body replaces exactly three passages with these lines, byte for
  byte, keeps the previous text in a collapsed block, and links the decision comment in
  place of `https://github.com/gyldlab/keld/issues/1184#issuecomment-6512851695`. The amendment takes effect only with this spec's
  exact-content approval; until then the previous text applies. AC1's external trust root stays required and is
  unowned on Windows (B4).

  *What to build*, replacing its final sentence:

  > Native evidence showed that no supported WebView2 API separates the page and preload worlds. Under the architecture decision proposed at https://github.com/gyldlab/keld/issues/1184#issuecomment-6512851695 (effective only with #1307's exact-content approval; until then the previous text below applies), route D — the "documented emulation" of architecture 04 §3 item 3 — replaces world separation with Host enforcement, and the facade MUST NOT report isolation it lacks (`process.contextIsolated` is `false`, #1307 AC23).

  *Acceptance criterion 2*, replacing it:

  > **(Proposed amendment for Windows route D; effective only with #1307's exact-content approval, until then the previous text below applies — see https://github.com/gyldlab/keld/issues/1184#issuecomment-6512851695.)** Prove the three route-D rules that replace world separation. **(i) No IPC authority or Host-delivered data exists only in preload code.** `keld migrate`'s conservative classifier flags every preload whose sink calls are not wholly page-controlled (page-input-dependent branches or transforms; channels computed from page input; constant, omitted, truncated or overwritten arguments; writes, deletes or mutating calls on page input, including preload-assigned fields), every inbound listener or `invoke` reply whose payload does not reach a page-facing callback unchanged (including consume-without-forward such as `on('auth', (e, t) => { token = t })`), every `process.argv` read, and indeterminate code. A flagged preload's window receives grants only with a developer attestation bound to the recorded preload digest that covers every flagged site and never widens grants; Keld reviews this mechanism, not each attestation, and a wrong attestation silently loses the narrowing. The Host loads grants only when the admitted preload bytes hash to the recorded digest (trust root: #1307 blocker B4), a verdict is recorded, and grants ⊆ `verdict.reachableSinks`. **(ii) The Host authenticates every call:** top-level `WebMessageReceived` only; the current epoch nonce of a document whose identity was decided once at commit; a browser-stamped `Source` whose scheme is that document's (so `about:`, `data:`, `javascript:` and `blob:` are refused) and which, for `https`, has that document's tuple origin and, for `file:`, has an empty host and a percent-decoded, case-folded path inside the verified package root as decided by keld-guard's one `file:` URL parser shared with W3 #1185 (never full-URL equality, never a comparison of opaque origins); and an exact literal `el:` channel grant — except `window.keld.invoke`, which routes only KEL-142's declared Echo channel `1`. The page never receives raw Host objects, and no token other than its own document epoch or an already-revoked one. **(iii) Navigation and eligibility are Host policy:** top-level navigation follows W3 #1185's rule A″, whose verbatim text #1185 owns; in substance: the allowlist governs every page-initiated top-level target, in-root `file:` included, and in-root membership is necessary, not sufficient; the query is part of page-initiated matching; the decision is taken on the original `NavigationStarting`, before any generation change — an off-allowlist target is cancelled at generation +0 and the live document and its IPC survive; `will-navigate` is a notification only, and the Host never replays a page navigation; Host attempts are checked against the same allowlist before their +1, and the window's entry is always admissible for them, whatever its query, once #1185's attribution rule binds the navigation to the attempt. Every `about:`, `data:`, `javascript:` and `blob:` URI in any letter case is off-allowlist (whether `javascript:` raises `NavigationStarting` at all is UNKNOWN, #1307 AC10); `NewWindowRequested` is denied; the runtime's cancellable-`about:` navigation is never disabled; a document receives an epoch only at `ContentLoading` with `IsErrorPage == FALSE`, an allowed `NavigationId` and verified resource identity (#1307 §4.5; never URL text, never a `NavigateToString` load). Top-level `about:srcdoc` (an error page commits despite `Cancel`; FailedClosed with bounded recovery) and `document.open()` (no navigation event; no Host action) are recorded Host-unvetoable limits.

  *Deterministic negative control*, replacing its paragraph:

  > **(Proposed amendment for Windows route D; effective only with #1307's exact-content approval, until then the previous text below applies.)** Run each of the following single-fault mutants in its own process against the same hostile page. Each mutant must produce its forbidden effect, counted independently of the Host's own refusal log, while the unmodified baseline refuses every attempt with zero effects: (a) the classifier treats omitted arguments as pass-through — fixture F-C (`openFile: () => send('open-file')`) passes unflagged, the window is granted `el:open-file` (an `ipcMain.on('open-file')` listener exists), and a direct page post carrying an argument the preload never sends produces a Bun handler entry; (b) the Host ignores the preload digest — grants recorded for another preload apply (a Bun handler entry on a channel the loaded preload cannot reach); (c) the literal-grant check is removed — an ungranted channel reaches a Bun handler; (d) admission uses `Source` + generation without the epoch nonce — a message from previous document A, replayed to the Host after same-origin document A2's `ContentLoading`, reaches a Bun handler; (e) the Host-side generation check on Host→renderer delivery is removed — the next document's raw `chrome.webview` `message` listener receives a reply addressed to the previous document. A stale-document IPC replay (previous document → Host, after the next document's `ContentLoading`) must also be refused with no native side effect. The bridge's own epoch check is a correctness guard, not a security control, and is not one of these mutants.
- **B3 Windows application-document oracle:** #1192 §4.4 eligibility (verified resource
  identity, not URL text) has no Windows implementation; E5 qualified by URL. The #1192
  owner must name the Windows oracle or accept this spec's T2 as it. With W3 B7 (v) and B12,
  the #1192 owner also decides eligibility for **non-entry** in-root documents that rule A″
  allows. Proposed (§4.5): every Host-served in-root regular top-level document is eligible,
  as in Electron. Fallback if rejected: declared entries only, compared with the one
  component equality; W3's rule 5 emits the declared entries, never `in-root:**`; and a page
  navigation to an ineligible in-root document triggers no `onLoss`.
- **B4 Windows preload trust root — unowned gap:** #1155 AC07/AC08 are macOS-only. Without
  it AC14's digest has no trust anchor. Owner to be assigned on #1182/#484.
- **B5 Every reused contract is an open draft:** #1191, #1192, #1154, #1190, #1155, #540
  (#1122), and the W3 #1185 spec (PR #1311 at `1db53482`, r14, open draft) that defines
  keld-guard's `file:` URL parser, its one component equality and rule A″, on which the
  `file:` Source rule, navigation and eligibility depend. W1 inherits that parser's
  fail-closed limits (W3 §4.4): in-root components that fail R8 (`:`, `~`, reserved device
  names, a trailing dot or space, the rest of the package grammar), a relative path over 100
  bytes (R9) and a `.lnk` leaf (R10, `FileUrlRefusal::ShellLink`, W3 B7 (vi)) are refused, so
  a document path with one never qualifies and a `Source` with one is refused; and R7/R8 need
  Windows-only NLS calls (`CompareStringOrdinal`, NFC), so their vectors run on Windows only.
  W3 r2 removed two earlier limits: interior `//` is collapsed (R6), and the root prefix is
  validated with `validate_fs_component`, so roots under `~`-bearing, TEMP or dev-stage
  paths are admitted. This spec cannot be approved as implementable before those it
  consumes are at least approved; list re-checked at approval time.
- **B6 Loading-path decision and `file:` allow forms (W3 #1185 + #1192/KEL-102 + #541).** W1
  binds to the substance of W3's rule A″ (#1311 `1db53482`, §4.6); the `file:` allow forms
  (`none`, a literal in-root component path with its query constraint, `in-root:**`) and
  keld-guard's navigation entrypoint are W3 B12 (#541 owner) and a dependency until
  approved. Under W3 (#1311 §4.6 rule 1) a compat window's first document is the application's
  own `loadURL` (#459), a Host attempt to the entry components, admissible only once its
  Host nonce attributes it (header-proven mode), so T2's loading-path change applies to that
  load; `navigate_initial` remains only for schema-1 renderer boots. If B6 chooses the Keld-owned virtual host, its path identity at
  commit needs a named keld-guard function that this spec does not yet specify. Today's `NavigateToString` path
  yields `Source = about:blank` (source probe 3/3) and can never qualify (§4.6a). The owners
  must record which Host-identifiable URL compat windows load — exact `file://` under the
  verified package root, or a Keld-owned virtual host — and how KEL-102 verifies that exact
  path. Without it AC18 holds but AC04/AC05 have no positive case. Full impact (W3 §1, B18,
  B19 at `1db53482`): W3 delivers no Windows `file:` application unless its Q22 (a) and (b)
  pass on the supported runtime, so no route-D `file:` application ships, and none of T2–T6's
  `file:` cells can pass, until then; and in-root page-to-page navigation stays cancelled at
  +0 until W3 Q27 passes or the #1185 owner accepts that divergence.
- **B7 Discriminating admission probe — OPEN, designed, not run.** E5's "0 mis-attribution
  in ~110k messages" is not evidence for the Source or nonce rule: 0 of 110,575
  previous-document messages arrived after the next document's `ContentLoading` in all 96
  trials (`results.json` `fromA_afterB_ContentLoading` absent in every trial), and the weaker
  mutant rules (`analyze.py` L176–181) also scored 0, so the statistic discriminates no rule.
  Before approval a probe MUST either produce a non-empty at-risk set and show that only the
  full rule refuses it, or cite a documented WebView2 guarantee that a document's messages
  cannot be delivered after the next document's `ContentLoading` (none found: the
  `WebMessageReceived` reference states no cross-document ordering). Design (owner: W1
  implementer, on the T2 device harness; results to keld-research):
  - **Arm F, forcing (renderer→Host).** Document A runs a busy loop of T ∈ {50, 200,
    1000} ms ending in a burst of N posts, each tagged with a page-generated per-document id
    the admission rules never read; the Host navigates top-level during the loop. Variants:
    cross-site B; same-origin A2 forced into a new renderer process (A2 served with
    `Cross-Origin-Opener-Policy: same-origin`, A without — INFERRED to force a
    browsing-context-group swap; the probe logs renderer PIDs to confirm); posts from
    `pagehide`/`unload` handlers; a back/forward-cache restore.
  - **At-risk set** = tagged A messages the Host receives after it processed the next
    document's `ContentLoading`. The arm counts only if that set is non-empty in at least
    one variant; otherwise the forcing is wrong and the outcome is "rule not exercised",
    never a pass.
  - **Discrimination.** On the at-risk set evaluate (1) the full rule, (2) `Source` +
    generation without the nonce, (3) Source origin against committed origin only. Pass:
    (1) admits 0, and in the same-origin variant (2) and (3) each admit ≥ 1.
  - **Negative control.** The same harness without forcing (no busy loop, no COOP swap):
    the at-risk set is expected empty, showing the forcing created it; and a build whose
    rule (1) is replaced by (2) must admit ≥ 1 at-risk message.
  - **Arm H (Host→renderer, AC21/AC24's race).** A test-only document-created script,
    registered before the bridge so it runs first in every document and does not depend on
    page code, installs a raw `chrome.webview` `message` listener that counts every post
    stamped with an epoch other than its document's. Forcing: the Host posts tagged replies
    at its maximum rate while the page navigates A→A2 and A→B, including posts handed to
    `PostWebMessageAsString` immediately before `NavigationStarting`. At-risk set = posts
    stamped with A's epoch that the Host handed to the engine inside a window W around A's
    `NavigationStarting` (W fixed by the probe before it runs and recorded in the receipt),
    as counted by the Host, **split** into a post-`NavigationStarting` subset (rule 2's
    risk) and a pre-`NavigationStarting` subset (rule 3's residual). The pre subset is forced
    separately: A runs a busy loop between the Host's post and a same-process A→A2
    navigation, so the post is still queued when A2 commits. Builds: (1) generation check
    only (hold removed) — must show ≥ 1 post-subset message received by the next document,
    or the arm reads "rule not exercised" for rule 2 and the forcing is revised; (2) full
    rule — MUST show 0 received from the post subset. For the pre subset, any message the
    next document receives is a blocker (§4.4 rule 3), not a divergence; if no variant shows
    a pre-subset message delivered late, rule 3 reads "rule not exercised", never "safe".
    **Bound variant (r7):** a navigation held past `NAV_TERMINAL_BOUND` by a stalled server, a
    newer navigation committed, the Host posting to the newer epoch at its maximum rate, then
    the stalled response released; any post the late document receives is a blocker, and an
    at-risk count of 0 reads "rule not exercised".
- **B8 OCR rerun — OPEN.** The patched high-effort native OCR review at `d7c9bdcb` finished
  `completed_with_warnings` (round 2: host-agent relay timeout). Before exact-content approval,
  OCR is rerun on the then-current head with every round successful and its findings are
  reconciled against the review threads. Check: the job id and a warning-free terminal status
  recorded on #1307.

**Open questions:**

1. **Q1 (wire, #540) — RESOLVED** by the #540 wire owner (0monish), [#1307 comment 6532691875](https://github.com/gyldlab/keld/pull/1307#issuecomment-6532691875).
   (a) Carriage: unchanged CompatV2 over WebView2 WebMessage strings, top-level event and
   `TryGetWebMessageAsString` only (§4.4, AC26). (b) Order: `bound` answers a verified
   `bind`; the one-time push at `ContentLoading` with a liveness-only `bind` is rejected; an
   early `bind` gets zero buffered work, zero nonce and zero authority (§4.3, §4.4, AC29).
   (c) The `document` nonce is page-readable on route D: a freshness/correlation tag, not a
   secret or authentication; #540 §4.2's "stays private to native/isolated world" is
   forfeited for route D only (§2, §4.2, AC30). (d) `send_records: 1`, the 4,096-byte ceiling
   and one DATA record per direction across the role stay; the same-document second send is
   a dropped call (divergence 11); a second window on the same role is refused by the
   Host/role ledger (AC27); T6 carries the send-capacity qualification gate. No new kind,
   v1 fallback, second decoder or second credit ledger. The decision approves no W1 draft text
   and claims no native Windows acceptance. **Still blocking W1 T2–T4** (not merge-approved
   until all pass): the bind→bound order implemented (T2); real-Windows AC26 and AC27 GREEN;
   the #540 wire vectors (§7) and the CompatV2 `decode_wv_link` integration reviewed; B1's
   separate KEL-142 §5.1 security amendment approved; and every other W1 blocker (B2–B8, Q2,
   Q12 and the per-task dependencies in §6). Staging reading (spec author, r11): the list is
   cumulative per task, not a requirement that T2 wait for T4. Each task merges when the items
   it owns pass: T2 needs the bind→bound order with AC26 and AC29 GREEN on real Windows; T3
   needs T2 and the CompatV2 integration review; T4 needs AC27 and AC30 GREEN. No task enables
   a route-D grant before its own cells pass (§6). Recorded for the #540 owner to correct on
   #1307 if a stricter order was meant. Check: each item recorded on #1307.
2. **Q2 (manifest, #541/#1191):** keys `windows.<w>.web.navigation` (`allow`, `onLoss`),
   `windows.<w>.preload.digest`, `windows.<w>.preload.verdict` (`digest`, `classifier`, `reachableSinks`, `flags`)
   and `windows.<w>.preload.attestation` (`digest`, `sites`, `acknowledgedBy`); non-`file:`
   `allow` entries as vetted URL scopes and the `file:` allow forms of rule A″ (W3 B12); all
   under #541's manifest ownership. Also for the #1191 owner: confirm that a window key
   supplied by trusted Bun main in the Create request satisfies AC3's "Host resolves one
   immutable configured window key" (AC3 forbids page-provided selectors only); how the
   facade learns the Host-admitted root for `p-` keys; and whether a Keld-only window-key
   option should replace the literal-title requirement (§4.8). Decide before T2.
3. **Q3 — RESOLVED (2026-10-29): `process.contextIsolated` is `false` on Windows route D.**
   Electron reports the frame's `context_isolation` preference
   (`v45.1.0` `shell/renderer/renderer_client_base.cc:236-237`, also bound for sandboxed
   preloads at `shell/renderer/electron_sandboxed_renderer_client.cc:53`), and its
   `exposeInMainWorld` throws "contextBridge API can only be used when contextIsolation is
   enabled" when that preference is false for a window realm
   (`shell/renderer/api/electron_api_context_bridge.cc:1145-1169`; at `v44.2.0` the same
   guard is `lib/renderer/api/context-bridge.ts:3-11`). Route D has no isolation, so the r2
   recommendation (`true`) would report isolation it lacks and mislead an application that
   exposes a privileged API only when isolated. The facade reports `false` and keeps
   `exposeInMainWorld` working: accepted divergence 6 (§4.2), AC23. Applications that fall
   back to `window.x = api` when not isolated behave the same in one realm, and `keld
   migrate` constant-folds the value so it grants only the branch actually taken (AC23(b)).
   #1155 (`process` subset) and #1154 record it in T1.
4. **Q4 — folded into B2** (the tracker text is B2's quoted text).
5. **Q5 (ordering) UNKNOWN:** whether awaiting registration completion before `Navigate` is
   necessary on 156. Probe: 20 runs each way with the AC01 fixture; recorded in the T2 receipt;
   keep the await regardless.
6. **Q6 (`file://` Source) — resolved to FACT by the source probes v1 and v2 (3/3 each,
   156.0.4314.50):** `Source` is the full file URL including path and **follows the history
   URL** (`pushState`, `replaceState`, hash), each change preceded by
   `SourceChanged(IsNewDocument=false)`; an entry query appears in `Source`; a message posted
   immediately after `pushState` may still carry the pre-push `Source`. Decision applied:
   identity at commit by verified resource (W3's served record); per-call `Source` = scheme
   plus tuple origin (`https`) or the one parser's in-root check (`file:`) (§4.4, §4.5, AC16,
   AC20). Still UNKNOWN for the T2 receipt: `IsErrorPage` for a missing
   file. Safety of the path as identity is B6 (KEL-102), not a Source question.
7. **Q7 (`window.keld.invoke` widening):** numeric channels beyond Echo `1` need KEL-142/#508
   grammar; none proposed here.
8. **Q8 — PROPOSED in r3 (rules widened in r3–r5); becomes the decision with exact-content
   approval of this spec (§10 B2):** gating is
   defined as narrowing of what the page can cause or see; the static classifier is
   conservative and flags what it cannot prove (§4.8 rule 2 (a)–(d)); a flagged preload's
   window receives grants only with a developer attestation bound to its digest that covers every flagged
   site and never widens grants (§4.8 rule 3, AC19). Keld does not review third-party
   attestations; it reviews the mechanism, and separately the T6 qualification attestation
   (§6 T6). Applied to the qualification evidence: the pinned draw.io preload has seven
   flagged sites (§4.8 table), including the representation-only `watchFile` branch, and
   qualifies only with an attestation for its digest covering all seven. This is a framework
   mechanism; no application exception exists.
9. **Q9 (`keld migrate`):** verb reserved (`verb.rs:75`, KEL-17); T5 needs KEL-17's owner to
   admit the classifier as the first live slice.
10. **Q10 — moved to B7.** E5 never observed natural late delivery, but its at-risk set was
    empty: that is an untested condition, not evidence.
11. **Q11 (page CSP vs document-created script) UNKNOWN:** probe with `script-src 'none'`
    on the AC01 fixture.
12. **Q12 (inbound channel narrowing; #540/#541/#1191):** under route D the page sees every
    Host→renderer post, including `webContents.send` channels the preload never subscribes
    to — in Electron no renderer code receives those. Candidate: Host-side inbound literal
    grants (channels the preload subscribes to with a literal; `*` when it subscribes to a
    page-chosen channel, as draw.io's `registerMsgListener` does), enforced before
    `PostWebMessageAsString`. Until decided, accepted divergence 4 covers it. Decide before
    T3.
13. **Q13 (browser-process exit terminal; owner #1188/W2) — (a) answered by #1280 r16 D8
    (`be9e9e56`), a named dependency until #1280 is approved and its T3 lands; (b) a future
    option.** W1 never applies `onLoss` after `ProcessFailed(BROWSER_PROCESS_EXITED)`
    and revokes at once (§4.5). It reuses W2's `AbnormalExit` terminal instead of adding one. On
    main at `457a9c3b` only an abnormal `BrowserProcessExited` receipt latches it
    (`webview2/mod.rs:2675-2676`), any fault requests exit (`:3272-3274`), and the loop returns
    the typed `LifecycleUnproven` profile failure (`:3381-3386`; `KELD-WV-009`,
    `profile/error.rs:122`). Microsoft: "A single `BrowserProcessExited` event will be raised
    from the `CoreWebview2Environment` too, but the order of these events is not guaranteed."
    At #1280 `95594e88` D8 (3) the receipt latches `AbnormalExit` only for the pid bound when
    it fires, and a receipt for any other pid is foreign and dropped; its disclosed limit says
    "an abnormal exit that fires after its browser's rebind (then a foreign pid) … is not
    reported as abnormal". So if `ProcessFailed` comes first and a `CreateWindow` rebinds the
    barrier to a new browser, the old receipt is dropped and the session does not end on that
    failure. (a) Answered, as a dependency: #1280 r16 D8 (`be9e9e56`) makes `ProcessFailed(BROWSER_PROCESS_EXITED)` for the
    bound browser instance latch `AbnormalExit` at once (at the loop's next `poll`), with neutral text, and allows no
    rebind after it (AC24 cases 14–15, AC28 (b)); W1's T2 blocks on W2's T3, which lands it (§6). (b) Re-creating the controller in place (a
    new controller and document, same logical window, HWND and grants only after independent
    verification) remains a possible future option owned by #1188, not a W1 promise. Check:
    #1280 r16 at `be9e9e56` carries that D8 rule; AC24 cases 14–15 and AC28 (b) GREEN.

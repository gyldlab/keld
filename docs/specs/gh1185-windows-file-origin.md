# Spec: Windows WebView2 `file://` navigation, app-code resource containment, honest navigation signals and settled `executeJavaScript` (GH-1185 W3)

Status: draft
Linear: GH-1185 (GitHub tracker under #517; parent program #1182 W3; webContents owner #539) ·
Owner: @amishabenramani (real Windows operator) · Updated: 2026-10-29

Code citations are at `origin/main` `457a9c3b37f71179f167bd1f8c706c461cc4c479`. Device
evidence is one runtime and one OS build: WebView2 Evergreen `156.0.4314.50` on Windows 11
`10.0.26300`, `webview2-com(-sys) 0.38.2` (SDK 1.0.3650.58). The native probe is the
[#1185 probe report](https://github.com/gyldlab/keld/issues/1185#issuecomment-6461689232)
(3 identical baselines, 68 `WebResourceRequested` events each, single-fault controls
N1–N6; `results.json` sha256 `acd3cbe4…34e6`, local to Ramani until published as a
keld-research note, §10 B9). Source-pinned research is the owner's D1–D6 report (Electron
v44.9.3 `c542df1e` / v45.1.0 `45d20ec0`, Chromium 152/156, V8 `392e4442`, WebView2 Win32
reference `38bc5e98` folder `1-0-3650-58/`, draw.io desktop `2edf9fb9`). Owner decisions on
resource authority are [#1185 6491936458](https://github.com/gyldlab/keld/issues/1185#issuecomment-6491936458)
and its correction [6492237806](https://github.com/gyldlab/keld/issues/1185#issuecomment-6492237806);
the independent review that forced them is
[6462197928](https://github.com/gyldlab/keld/issues/1185#issuecomment-6462197928).

This draft authorizes no production code, no permission and no public API. Implementation
starts only after an exact-content `Status: approved` decision, the review gates in §8 and
the blockers in §10 are cleared.

This is a **framework mechanism for every Electron application whose UI is loaded from
`file://`**. No rule below names or special-cases one application. The pinned draw.io
Desktop (`jgraph/drawio-desktop@2edf9fb9`, `src/main/electron.js`) appears only as the
qualification target and as evidence of real demand (bundle sizes, `onHeadersReceived`
CSP, awaited `executeJavaScript` results whose rejection it converts into an irreversible
migration mark, L586–589 and L640–642).

## 1. Goal & non-goals

On Windows the Host loads a migrated application's document with
`Navigate(file:///<admitted root>/<entry>)`, serves **every** `file://` byte of that
window itself from a held handle to the admitted app-code root (no byte of an object
outside that root reaches the renderer, no reparse point is followed, no ADS or alias
spelling is served), emits Electron `webContents` navigation events that are each tied to
one WebView2 navigation and one Host-served document, and returns real settled
`executeJavaScript` values whose delivery is scoped to the document generation and the Bun
role generation current at dispatch. Nothing degrades silently: a Host that cannot provide
settled evaluation refuses to start the application with a typed error.

Observable outcome on a real Windows device: an application loaded from `file://` keeps
its genuine `file:///…/index.html` URL (probe: no virtual-host rewrite; `self.origin ==
"null"`); every out-of-root, aliased or malformed `file://` request is answered `403` with
an empty body and the outside sentinel's nonce never appears in the page; `did-start-
navigation`, `will-navigate`, `did-navigate`, `dom-ready`, `did-finish-load`,
`did-fail-load` and `did-stop-loading` arrive in the facade in an order that matches the
WebView2 event order for that `NavigationId`; an awaited `executeJavaScript` promise
settles with the page's completion value and a result produced for a replaced document is
rejected before any caller observes it.

Non-goals:

- A virtual host, a custom scheme, or any rewrite of the application's `file://` URL
  (#1185 non-goal; W1 AC05/AC18 rely on the genuine URL).
- Navigation **policy** for non-`file:` destinations (`https:`, `data:`, `blob:`,
  `about:`): §4.8 names the owner; W3 passes those requests through untouched.
- A second navigation-generation counter or a second value codec: the generation is
  #1192's checked `u32` document generation plus the existing `RoleGeneration`
  (`crates/keld-runtime/src/role.rs:189`); the value codec is #1190's. §4.7 records a
  recommendation and §10 B3 makes the eval-result domain and bound an owner decision.
- `asar` archives: Keld packages from source; no `asar` reader exists in `crates/`,
  `packages/` or `docs/` (grep 2026-10-29). Not in scope.
- macOS/Linux loading paths (#542/KEL-142 own macOS; Linux UNKNOWN); `<webview>` tags;
  `webContents` methods beyond `executeJavaScript` and the events above (`stop`, `reload`,
  `loadURL` facade vocabulary stay with #539/#459); screen/menu/dialog scope.
- Workers under `file://`: WebView2 156 refuses dedicated/shared/service workers from a
  `file://` document with `SecurityError` (probe N1 note), so no worker request exists to
  contain. If a future runtime admits them, C1's `ALL` source-kinds filter already covers
  them; the probe cannot show it (§3 AC22).
- Conformance score changes (#464/#567 own cells; nothing here flips a row).

## 2. Spec refs

Governing architecture: [03-security](../architecture/03-security.md) §1 (webview
principal = Host WebView id + navigation generation), §2 ("CSP injection by default on
every webview", `web.csp`), §4.3 (webview hardening: CSP injection, navigation allow-lists,
remote-content `channels: []`, devtools off in release) and §6 honesty ledger;
[05-webview-and-native](../architecture/05-webview-and-native.md) §1 (Windows is direct
COM; the destination `WebEngine::eval(id, script, cb: EvalCallback)` at L26; the resource
path is "a destination contract": an adapter may use "a host-backed resource callback …
the observable policy and response semantics stay host-owned", L185–192);
[01-overview](../architecture/01-overview.md) §5 (first-paint budget; >5 % regression needs
a written waiver; §5.1 registry sample policy: 30 valid samples per class);
[04-electron-compat](../architecture/04-electron-compat.md) §3 (documented emulation where
the engine differs).

Deviations this spec records (all land in **T0**, the PR that approves this spec):

- Architecture 05 §1 lists `eval` as fire-and-forget in the live trait
  (`crates/keld-wv/src/engine.rs:348`) while naming the callback form as the destination
  (05 L26). W3 moves the live trait to the destination form (§4.10); T0 marks the Windows
  backend as the first live settled implementation and records that macOS/Linux keep their
  native completion handlers without W3's generation semantics until #542/KEL-142 adopt
  them (§10 Q4).
- Architecture 03 §4.3 says CSP injection "is not in this slice". W3 adds the Windows
  `file://` baseline CSP (§4.5); T0 updates that sentence and the §6 ledger line "Windows
  `file://` documents: the Host serves app-code bytes; a Legacy role can rewrite them
  (#1185 6492237806)".
- KEL-254 AC7/AC8: the owner decision extends the ACL read-back from the root to the
  per-file DACL of every in-root file whose link count is > 1, and adds a write-denial
  control per role token for those files. T0 records that extension in KEL-254 §3 (its
  owner signs, §10 B6); W3 does not implement KEL-254 checks, it consumes them.

| Existing owner (reuse, no duplicate) | Exactly what this spec consumes | Decision / divergence |
| --- | --- | --- |
| [#1192 GH-542 checked navigation binding](https://github.com/gyldlab/keld/pull/1192) (**open draft**, head `cd30f997`) | §4.2 `NativeDocumentPhase` (Unbound/AwaitingCommit/Live/FailedClosed/Exhausted/Destroyed), checked `u32`, +1 on real replacement commit, +1 **before** a trusted Host load, +0 on provisional start/cancel and same-document changes; §4.4 eligibility = verified resource identity, "a `file:` prefix, identical path spelling … is insufficient"; §4.5 "stale evaluation completion releases only its old resource reservation and never delivers to a new document"; N15 Bun recovery keeps the document; N16 one generation in stamp and principal; N21 Windows UNKNOWN. | Reused as the only generation and phase model. W3 is its Windows oracle (§4.6, §4.7) and supplies the served-resource identity that §4.4 demands (§4.4 here). W3 needs #1192 T1 as a platform-neutral pure module (§10 B2). |
| `RoleGeneration` (`crates/keld-runtime/src/role.rs:189`) and the existing app-link retirement owner (KEL-75/KEL-143) | Per-role generation; retirement of role-bound correlations on recovery. | Eval correlations are keyed on it; a G1 result is never delivered to a G2 role (AC11). No second counter. |
| [#1190 GH-544 codec](https://github.com/gyldlab/keld/pull/1190) (**open draft**, `115af9ac`) | One Host-opaque codec (V7); V3 synchronous refusal of functions; the 4096-byte bound (V6) which it claims for `executeJavaScript` results (L9–12). | Reused as the only codec. The 4096-byte bound is **falsified as an eval-result bound** by pinned demand (D4: `.recent` and `.drawio-config` JSON strings have no cap; Electron's domain is V8 `ValueSerializer` with no bound); the domain and bound for eval results are an owner decision (§4.7, §10 B3). W3 introduces no second value format. |
| [#1307 W1 preload facade spec](https://github.com/gyldlab/keld/pull/1307) (`10ef54e7`) | §4.4 Source rule step 3 ("for `file:` the path is inside the verified package root"), §4.5 per-document lifecycle (`ContentLoading` mints/revokes; `NavigationId` tied to an allowed `NavigationStarting`), §4.6 navigation policy keys, §4.6a loading-path requirement, blocker B6. | W3 closes B6 with L1 (§4.2). The W1 Source rule's in-root check MUST call the one `file:` URL parser defined here (§4.4); W3 adds no second path matcher. |
| [#541 GH-541 spec](https://github.com/gyldlab/keld/pull/1123) (`gh541-channel-grants-navigation.md`, approved unmerged) and #460/KEL-79 | "no new web.navigate grammar is authorized"; full navigation policy is #460/KEL-79; first-proof eligibility must come from "boot/resource containment and native navigation evidence". | W3 provides exactly that evidence for `file:` documents and nothing for other schemes (§4.8). The `will-navigate` cancel-and-replay mechanism (§4.6) needs that owner's sign-off (§10 B4). |
| [KEL-254 installed-root boot](kel254-windows-installed-root-boot.md) (approved) and `keld_update::ActivePackageSelection` (`crates/keld-update/src/windows_baseline.rs:207-241`: retains `_version: Dir`, exposes `tree_root() -> &Path`) | Admission of the installed root (mode, owner, DACL read-back, pinned ancestry); AC7/AC8 access profiles; AC13 Legacy refusal of direct installed boot. | W3 adds a narrow accessor that opens `tree` relative to the retained version handle with no-follow (§4.3; public API gate) instead of re-opening `tree_root()` by path. The root's mutation authority stays KEL-254's. |
| `keld-core` dev-stage admission: `validate_windows_dev_stage_acl` (`app_session.rs:1485-1500`) opens the root directory with `FILE_FLAG_OPEN_REPARSE_POINT` + `FILE_FLAG_BACKUP_SEMANTICS` and validates the owner-private DACL through `keld_guard::validate_windows_owner_private_directory`, then drops the handle. | The dev-stage DACL policy (KEL-96) and the owner-private check. | W3 retains that validated handle as the admitted root for the session instead of dropping it (§4.3 A3). Policy unchanged. |
| `keld-core` boot file opener `open_relative_file_windows` (`app_session.rs:1729-1789`): Win32 path walk with `symlink_metadata` per component, then `OpenOptions::open` with `FILE_FLAG_OPEN_REPARSE_POINT`, then a post-open reparse/regular-file check; trusted descriptor input only. | Its post-open checks and its callers (`validate_windows_boot_files`, `:1440-1479`). | **Replaced** (migrated) by the one NT-relative opener in §4.3; a check-then-open over path strings is a TOCTOU and the Win32 name layer is unsafe for page-controlled names. The function is deleted in T2, not kept beside the new opener. |
| `keld-update` `windows_fs.rs`: `NtCreateFile` relative to a retained parent with `OBJ_DONT_REPARSE` (`:406-495` create directory, `:496-583` create file, `:669-834` open for delete with `FILE_OPEN` + `FILE_OPEN_REPARSE_POINT` + `FILE_SYNCHRONOUS_IO_NONALERT`, `entry_kind` via `FileAttributeTagInfo`); `validated_relative_name` (`:907-926`) delegating grammar to `keld_guard::validate_windows_package_paths`. | The relative-open mechanism, its SAFETY discipline and its existing unsafe sanction (`crates/keld-update/AGENTS.md`: "relative NT create/delete/rename"). | **Extended** with one read-open form (§4.3). No existing read-open exists there today; the new function is the only new `unsafe` of this spec (unsafe gate, issue-scoped `keld-update/AGENTS.md` line). |
| `keld-guard` component grammar: `validate_windows_package_component` (`lib.rs:951-958`), `validate_windows_component` (`:1145-1164`: refuses `:`, `<>"\|?*`, control characters, trailing `.`/space, reserved device basenames `CON PRN AUX NUL COM1-9 LPT1-9` incl. superscripts), `validate_portable_component` (`.`/`..`/empty/NUL/separators), `~` refused, `validate_windows_package_paths` (`:975-1090`: NFC via `NormalizeString`, ordinal case-insensitive collision via `CompareStringOrdinal`, 100-byte path bound). | The **one** Windows component grammar. | Reused unchanged by the URL parser (§4.4). Consequence stated plainly: a relative path longer than 100 bytes is refused by the same rule that refuses to package it. Whether `keld dev` stages are validated by the same grammar is UNKNOWN (§10 Q6). |
| `keld-wv` Windows backend: `create` ordering (`webview2/mod.rs:3560-3645`: guard mint → devtools `cfg!(debug_assertions)` at `:3583` → bounds → app navigation handler → `navigate_initial`), `navigate` (`:3647-3659`: `NavigateToString` for `NavTarget::Html`, `Navigate` for `NavTarget::Url`), `eval` (`:3661-3674`: `ExecuteScript` with a no-op completion), `NavigationStarting` diagnostic observer (`:2276-2297`), `GuardInstalled` + deny-all `NewWindowRequested` (`guarded_navigation.rs:50-154`), `AppWindowEvent` channel to the keld-core coordinator (`engine.rs:143`). | Create ordering, the guard mint, the event channel, the pool-callback-posts-via-`EventLoopProxy` pattern sanctioned in `crates/keld-wv/AGENTS.md`. | Extended inside the same mint: the WRR filter and handler, the CDP self-test and the navigation-signal handlers are registered before `navigate_initial` (§4.10). |
| `keld-core` `app_session.rs:3653`: `WebviewSpec { initial: NavTarget::Html(inputs.html) }` — today's app load path. | The place the loading path changes. | **Must change**: L1 (§4.2). The renderer HTML is no longer read into a string for the app document; the entry is navigated by URL and served through WRR. |
| KIPC frame cap `keld_ipc::MAX_FRAME_LEN = 16 MiB` (`crates/keld-ipc/src/lib.rs:113`) | The transport ceiling any eval-result bound must stay under. | Cited in the §4.7 recommendation; not a W3 decision. |
| `@keld/electron` `WebContents` (`packages/@keld/electron/src/browser-window.ts:85-104`): today only the `destroyed` event. | The facade surface W3's events and `executeJavaScript` extend. | Extended by T5 under #1154's facade ownership (§10 Q3). |
| KEL-130 retained filesystem (`kel130-retained-filesystem.md`, approved T0; `keld-native` cap-std broker, `crates/keld-native/src/fs.rs`) | Nothing. | **Not reused** (§4.3 option d): app-grant policy, not webview resource authorization (6462197928); `max-content-8388608` is smaller than the pinned `app.min.js` (9,865,843 B). |

Microsoft documentation relied on (pins in the D1–D6 report; retrieved 2026-10-29 unless
noted): `ICoreWebView2WebResourceResponse` ("Stream must have all the content data
available by the time the WebResourceRequested event deferral of this response is
completed. Stream should be agile or be created from a background thread to prevent
performance impact to the UI thread", `icorewebview2webresourceresponse.md` L61);
`ICoreWebView2::add_WebResourceRequested` ("If a deferral is taken, then the web resource
requested is blocked until the deferral is completed", `icorewebview2.md` L953);
`ICoreWebView2_22::AddWebResourceRequestedFilterWithRequestSourceKinds` ("To receive all
raised events filters have to be added before main page navigation", `icorewebview2_22.md`
L51); `ExecuteScript` after `NavigationStarting` "runs in the new document when loading it"
(`icorewebview2.md` L1408); "CDP method calls may be processed out of order"
(`icorewebview2.md` L1333); "The Chrome DevTools Protocol is maintained by the open source
Chromium project, not by the Microsoft Edge WebView2 team" (`how-to/chromium-devtools-protocol.md`
L177); threading model (`concepts/threading-model.md` L21–23: only the request stream is
named as background-read; L93: an uncompleted deferral blocks rendering); member lists of
`ICoreWebView2NavigationStartingEventArgs` (`Cancel`, `IsRedirected`, `IsUserInitiated`,
`NavigationId`, `RequestHeaders`, `Uri`; **no deferral**), `ICoreWebView2ContentLoadingEventArgs`
(`IsErrorPage`, `NavigationId`), `ICoreWebView2NavigationCompletedEventArgs(2)` (`IsSuccess`,
`WebErrorStatus`, `NavigationId`, `HttpStatusCode`), `ICoreWebView2SourceChangedEventArgs`
(`IsNewDocument`), `ICoreWebView2WebResourceRequestedEventArgs(2)` (`Request`, `Response`,
`ResourceContext`, `GetDeferral`, `RequestedSourceKind`; **no `NavigationId`**), all in
folder `1-0-3650-58/` of `webview2-win32-reference@38bc5e98`. All methods used exist in the
pinned `webview2-com-sys 0.38.2` bindings (bindgen output through `ICoreWebView2_28`).
CDP: `Runtime.evaluate` (`js_protocol.pdl@392e4442` L1519–1574; `awaitPromise`,
`returnByValue`, `userGesture` stable; `uniqueContextId` and `allowUnsafeEvalBlockedByCSP`
experimental, the latter "Defaults to true"), `Runtime.callFunctionOn`,
`Runtime.releaseObject`; W3C CSP3 "multiple policies": every delivered policy is enforced.

## 3. Acceptance criteria (binary, each becomes a test)

Each row's negative control (NC) changes exactly one rule. Every counted effect has an
independent counter: **page nonce** = the outside sentinel's random nonce read back from
the page through CDP; **WRR log** = the Host's per-request record (uri, verdict, status,
bytes, `served`); **opener count** = `NtCreateFile` calls of the one opener (§4.3) per
URL; **signal log** = the facade's ordered event list per `NavigationId`; **eval ledger**
= the Host's per-correlation ledger (dispatched generation, completion generation,
delivered yes/no). "Baseline 0" means the counter reads 0 across the whole at-risk set
(the number of requests or results that could have produced the forbidden effect; a row
whose at-risk set is empty says **rule not exercised** and counts for nothing). Windows
rows are **real-device only** (Windows 11, WebView2 Evergreen); CI mocks, macOS and Linux
are not substitutes.

| ID | Given / when | Pass observable | Negative control (one fault → RED) |
| --- | --- | --- | --- |
| AC01 Loading path (L1) | A compat window boots an application whose entry is `<root>/index.html` | The Host calls `Navigate(file:///<root>/index.html<query>)`; the first top-level `ContentLoading` carries `IsErrorPage == FALSE`; `location.href` read via CDP is that file URL; W1's `Source` is that URL (W1 source probe 3/3). | Load via `NavigateToString` (today's `app_session.rs:3653` → `mod.rs:3654`) → `Source = about:blank`, `location.origin == "null"` (W1 AC18) → RED. |
| AC02 Filter before first navigation (C1) | Same boot | Every `file://` request of the load appears in the WRR log with a Host verdict (probe baseline: 68/68, 3/3 runs); no request of the load reached Chromium's file loader (the entry's raw inline script that only runs without the Host CSP header did not run). | Register the source-kinds filter after the first `Navigate` (probe N2) → `index.html` escaped, the raw inline script ran → RED (at-risk 1 per run, 3/3). |
| AC03 Containment matrix (C2, C4) | The page issues `fetch`/XHR/`<img>`/`<script>`/`<link>`/`<iframe>`/`@font-face` requests for: (served) in-root files, mixed-case in-root spellings, an in-root **hardlink** to an outside file; (refused) plain outside paths, a junction inside the root (mixed case too), `%2e%2e`, `%5C`, `%2F`, `%00`, UNC/URL-host forms (`file://127.0.0.1/C$/…`, `file://localhost/…`), `file:////?/C:/…`, `name:stream`, `name.`/`name␠`, `CON`/`COM1`, a `~` short-name spelling, a file symlink, a **directory symlink** (needs Developer Mode/elevation; UNTESTED in the probe → mandatory real-device cell) | Served cells return the bytes with `200`; every refused cell returns `403`, empty body, and the **page nonce** counter is 0 and the **opener count** for that URL is 0 (the parser refuses before any open). The hardlink cell is **served** and listed as such (owner decision 6491936458/6492237806). At-risk set = number of refused cells × runs (probe: 13 cells × 3). | Delete subresource containment: serve any `file://` whose NT-relative open succeeds, skipping the root prefix rule (#1185's own NC) → the outside sentinel's nonce appears in the page for the plain-outside and `..` cells → RED. A weaker mutant (probe N2) scored 1/1 leak. |
| AC04 One opener, no Win32 name layer (C3) | Any app-root open in `keld-core` boot or `keld-wv` serving | Exactly one `NtCreateFile`-relative opener exists (§4.3); `open_relative_file_windows` is gone; a grep for `OpenOptions::open`/`CreateFileW` with app-root or page-controlled input in `keld-core`/`keld-wv` is empty. Identity: `index.html`, `INDEX.HTML` and `index.html.`/`index.html␠` do **not** all open the same object: the first two are one ordinal-ignore-case spelling class (served), the last two are refused by the grammar (403). | Replace the opener with a Win32 path open (`CreateFileW` on `root\<decoded>`) and bypass the grammar for the trailing-dot cell → `index.html.` opens `index.html` (Win32 strips) → a second spelling class serves the entry → RED (W1's Source in-root check then compares a spelling the Host never admitted). |
| AC05 One URL parser; refusal table (§4.4) | Pure Rust golden vectors in `keld-guard` for every row of the refusal table, plus the probe's URL corpus | Each vector yields the table's refusal or the expected components; the ADS (`:`) and `~` cells are refused **by the parser** (the NT layer would resolve both: NTFS itself parses `name:stream` and 8.3 names), so the parser is the only rule for them. W1's Source rule and W3's WRR handler call this one function (compile-time: one definition, two call sites). | Drop the `:` component refusal → `index.html:hidden` is opened and the ADS bytes (planted with a nonce) are served → RED. Separately drop the `~` refusal → a planted `LONGNA~1.JS` spelling serves the long-named file under a second identity → RED (if the fixture volume has 8.3 generation disabled the row reads **rule not exercised** and the receipt says so). |
| AC06 Served-document correlation (L2) | A top-level navigation `n` to an in-root URL is allowed at `NavigationStarting`; (a) the file exists; (b) the file is absent; (c) an `<iframe>` to the same URL loads concurrently | (a) The WRR log holds one `ResourceContext == DOCUMENT` response for `n`'s URI served from `(root, components)`; `ContentLoading(n)` reports `IsErrorPage == FALSE`; the Host records `ServedDocument { navigation: n, components }` and only then reports the document as Host-served to W1/#1192 eligibility. (b) The Host served `404`/empty for `n`'s DOCUMENT request; `ContentLoading(n)` is **not** reported as Host-served regardless of `IsErrorPage` (UNKNOWN value, §10 Q7); AC09 fires `did-fail-load`. (c) The frame's document is served too; the top-level record is keyed by `n` and its identity is the same `(root, components)`; nothing is confused because both are the same served object. | Qualify at `NavigationStarting` by URL text (no served record) → case (b) is reported Host-served and W1 would mint an epoch for an empty document → RED (counter: Host-served reports for navigations with no served DOCUMENT record). |
| AC07 Signal order (§4.6) | (i) In-root load; (ii) out-of-root top-level navigation; (iii) hash change and `pushState`; (iv) absent in-root file | (i) `did-start-loading`, `did-start-navigation`, `did-navigate`, `dom-ready`, `did-finish-load`, `did-stop-loading`, each stamped with the same `NavigationId` and the generation current at that event (probe order: `NavigationStarting` → `SourceChanged`/`HistoryChanged` → `ContentLoading` → `DOMContentLoaded` → `NavigationCompleted(success)`). (ii) `did-start-loading`, `did-fail-load(-3, ERR_ABORTED)`, `did-stop-loading`; no `did-navigate`, no `dom-ready` (probe: `NavigationCompleted(isSuccess=false, WebErrorStatus 14)`, no `ContentLoading`). (iii) `did-navigate-in-page` only; no `will-navigate`; generation unchanged (probe: `SourceChanged`/`HistoryChanged` + `Page.navigatedWithinDocument` only). (iv) `did-fail-load(-6, ERR_FILE_NOT_FOUND)` exactly once, no `did-finish-load`. No event is synthesized from a Bun-side timer (`app-load-finished` is never fabricated). | Emit `did-finish-load` from `ContentLoading` → the ordering oracle sees `did-finish-load` before `dom-ready` → RED. Separately emit `did-fail-load` for a 403 **subresource** → a navigation-level event without a top-level `NavigationId` → RED (probe: 403 subresources raise no navigation events). |
| AC08 `will-navigate` attribution and veto (§4.6) | The page assigns `location.href` to an allowlisted URL; the app's `will-navigate` listener (a) calls `preventDefault()`, (b) does not | Host cancels the page-initiated `NavigationStarting` (not a pending Host attempt), emits `will-navigate(url)`; (a) no further `NavigationStarting` for that URL, the document survives; (b) the facade requests a trusted Host load of `url`; the replay commits (+1 generation before `Navigate`, #1192). Host-initiated loads (`loadURL`, reload, replay) emit **no** `will-navigate`. Redirect hops (`IsRedirected == TRUE`) emit no second `will-navigate`. `document.open()` is invisible (W1 E5 g; Electron cannot see it either). | Attribute by `IsUserInitiated` instead of "not a pending Host attempt" → a programmatic page navigation (`IsUserInitiated == FALSE`) emits no `will-navigate` and proceeds → the app's veto is skipped and the document is replaced → RED. |
| AC09 `did-fail-load` honesty | (i) Absent in-root entry; (ii) Host policy cancel; (iii) renderer error (`NavigationCompleted` failure with another `WebErrorStatus`) | (i) `-6`; (ii) `-3`; (iii) the §4.6 mapping table's code; `validatedURL` is the navigation's URL; exactly one `did-fail-load` per failed `NavigationId`; `did-stop-loading` follows. The mapping table's correctness against Electron is owned by the GH-551 differential oracle (#1192 §5); this spec's codes are INFERRED from Electron behaviour and marked so in the receipt until that oracle confirms them. | Map every failure to `-3` → case (i) reports `-3` while the Host's own WRR record says `404` → RED (the Host contradicts itself). |
| AC10 Settled eval (E1, E4) | After `did-stop-loading`, `webContents.executeJavaScript` of (a) `new Promise(r => setTimeout(() => r("v"), 500))`; (b) a `try { … } catch (e) { null }` **statement** whose completion value is a string (draw.io L565/L609 shape); (c) `Promise.reject(new Error("m"))`; (d) `throw new Error("t")`; (e) an expression under a page CSP `script-src 'self'` | (a) resolves `"v"` after ~500 ms (probe 3/3); (b) resolves the completion value (CDP returns the script completion value; `await (statement)` is a `SyntaxError`, so a bridge wrapper cannot do this — BRIDGE falsified, D2); (c) rejects with an error carrying message `m`; (d) rejects with the facade's fixed sync-throw message (shape owned by #1154, §10 Q3); (e) evaluates without `eval` and without the Host passing `allowUnsafeEvalBlockedByCSP:true`. | Use `ExecuteScript` (probe N4) → case (a) yields `{}` immediately, never `"v"` → RED. Separately pass `allowUnsafeEvalBlockedByCSP` default (`true`) → a page `eval()` inside the evaluated code succeeds under `script-src 'self'` → RED (divergence from the page's own CSP). |
| AC11 Generation-scoped delivery (E2) | (i) Eval dispatched at document generation G; the document is replaced before completion; (ii) a G1 result is replayed by a test hook after G2 committed (#1185's own NC); (iii) a Bun role G1 awaits an eval, the role recovers to G2 before completion | (i) the caller gets the typed `DocumentReplaced` rejection; CDP reported `-32000 "Execution context was destroyed."` (probe 3/3) or the Host's completion-time generation check refused it; the eval ledger shows `delivered = no`. (ii) rejected before any caller observes (probe baseline refused; at-risk 1 per run). (iii) the G2 role never receives it; the G1 correlation is retired with the role (KEL-75/KEL-143 owner's retirement). | Remove the completion-time generation re-check (probe N6) → the stale G1 value is delivered as G2 → RED. Separately key the ledger on the WebView id only (drop `RoleGeneration`) → case (iii) delivers to G2 → RED. |
| AC12 Dispatch targeting (E4) | `executeJavaScript` issued while a top-level `NavigationStarting` for the window is pending (no `ContentLoading` yet) | The Host refuses with the typed `DocumentReplacing` rejection at dispatch; the script never runs in the new document; the eval ledger records no CDP call. | Dispatch anyway through `ExecuteScript` → the script runs in the **new** document (`icorewebview2.md` L1408) → RED (counter: evaluations observed by the new document's `executionContextCreated` context). |
| AC13 CDP capability self-test (E5) | Webview create, before the first `Navigate` | `Runtime.evaluate("1+1", returnByValue)` returns `2`, `Runtime.evaluate("Promise.resolve(3)", awaitPromise)` returns `3`, and a call carrying `allowUnsafeEvalBlockedByCSP:false` is accepted, each within the existing `WEBVIEW2_STARTUP_BOUND`; on any failure the window is not shown and the application does **not** start, with a typed `KELD-WV-*` error (code allocated by the error-registry owner, `docs/engineering/keld-error-codes.md`; not `KELD-WV-006`) whose fix names the runtime version and the failing method. | Skip the self-test and inject a CDP failure (test hook: the method handler returns `E_FAIL`) → the application starts and every later eval rejects → with the pinned application that is an irreversible migration mark (L586–589) → RED. |
| AC14 Eval value domain and bound (E3) | A result inside the codec owner's admitted domain and at the admitted bound; one byte over | Delivered bit-exact (codec V2 goldens apply) / typed `CodecLimit`-class rejection delivered to the caller, never a hang. **The domain and the bound are owner decisions (§10 B3); until recorded this row is blocked, not green.** The W3 device rows use string and `string[]` values that every candidate domain admits. | Deliver an over-bound value truncated → the caller observes a shorter string without an error → RED. |
| AC15 App-declared response headers (S1) | (a) The app registered `session.webRequest.onHeadersReceived` with a filter matching the entry; (b) no listener; (c) the listener's role is retired or exceeds the round-trip deadline | (a) exactly one Host→Bun deferral round trip per matching request; the served response carries the listener's headers; (b) zero round trips (counter 0) and the baseline CSP only; (c) the request is answered `403`/empty (fail closed), never served without the app's headers. | On deadline, serve the file without the app's headers → the page runs an inline script the app's CSP would have blocked (probe N5 class) → RED. |
| AC16 Baseline CSP cannot be relaxed | The listener returns `Content-Security-Policy: default-src * 'unsafe-inline' 'unsafe-eval'` | The Host emits its baseline as a **separate** `Content-Security-Policy` header; a remote script the baseline forbids stays blocked (CSP3: every policy is enforced); the app's in-root scripts still load (baseline semantics under `file://` are probe P4, §10 Q5). | Merge the policies into one header with the app's winning → the forbidden remote script runs → RED. |
| AC17 Hardlink authority read-back (C4) | The fixture plants ≥ 1 in-root file with link count > 1 (its other name lives outside the root) | The hardlink cell is served (AC03); for **every** served in-root file with `NumberOfLinks > 1` the receipt lists it, a write attempt on the linked object under the Strict role token is denied (KEL-254 AC8 class), and the per-file DACL read-back (KEL-254 AC7 extension) shows no role write grant. Legacy roles: the receipt prints the forfeiture ("a Legacy role can rewrite the app-code root, including the W1 preload source") and runs no denial claim for them. At-risk set = number of multi-link served files; 0 → **rule not exercised** (the fixture MUST plant one). | Grant the Strict role write on the linked object → the DACL read-back detects the grant → RED; the write-denial control then also writes → RED. |
| AC18 Zero bytes on refusal (C5) | AC03's refused cells, with an elevated ETW Kernel-File session | No file-open event for the outside sentinel by `msedgewebview2.exe` or the Host during refused requests; opener count 0 (the parser refused). Without elevation the ETW cell reads NOT RUN and the claim is INFERRED-strength (probe status). | AC03's NC → the ETW session shows the Host open of the outside sentinel → RED. |
| AC19 Non-`file:` pass-through (C7) | The page fetches `https://` and loads `data:`/`blob:` URLs (policy-allowed) | The W3 WRR handler sets no response and takes no deferral for a non-`file:` URI (counter: W3 responses for non-`file:` = 0); the navigation-policy owner's rules decide; the `file:` containment is unaffected. | W3 answers `403` to every non-`file:` request → an allowlisted remote fetch fails → RED (over-reach into #541/#460's scope). |
| AC20 P1 performance gate (§9) | Before T2 merges | The P1 report exists with the arms of §9, the census/work/queue-copy/clock/statistic/artifact atoms separated, 30 valid samples per arm per class, and the chosen arm's first-paint median ≤ 1.05 × arm (e) or a written waiver linked from the PR. | A report with a single arm, or without arm (e), or with first paint measured from `NavigationCompleted` instead of the KEL-64 double-rAF beacon → validator RED. |
| AC21 Receipt | A Windows acceptance claim | Runtime `BrowserVersionString`, OS build, head, admitted-root mode (dev / installed mode), per-AC counters, every at-risk count, each RED control's output, and the "rule not exercised" rows named. | Missing runtime string, a mocked `WebResourceRequested`, or a zero-failure row without its at-risk count → validator RED. |
| AC22 Non-discriminating controls recorded, not counted | Probe N1 (answer only DOCUMENT source kinds) and N3 (deprecated filter) | Both read **rule not exercised** under `file://` on WebView2 156: every `file://` request is `RequestedSourceKind == DOCUMENT` and workers are refused, so no non-DOCUMENT request exists (probe; review 6462197928). They stay in the matrix for a future runtime and contribute nothing to AC03's score. | Count N1/N3 as passed containment controls → receipt validator RED. |

## 4. Design

### 4.1 Atomic decomposition (decision input)

Evidence classes: FACT (source/document cited), PROBE (#1185 probe, run ids `baseline
r1–r3`, `n1–n6`), INFERRED, UNKNOWN (named probe). Security atoms separate identity,
authentication, authorization, OS containment, lifecycle/revocation and evidence
provenance; performance atoms are in §9.

| Atom | Owner / boundary | Inputs → outputs | Failure mode | Observable | Evidence | Independence edges · first falsifier |
| --- | --- | --- | --- | --- | --- | --- |
| A1 Loading path | `keld-core` boot → `keld-wv` `navigate` | admitted root + entry components → `Navigate(file URL)` | string load yields `about:blank` identity | AC01 | FACT: W1 source probe 3/3; code `app_session.rs:3653`, `mod.rs:3654` | Independent of containment (A4–A6) · falsifier: AC01 NC |
| A2 Request census (OS containment, coverage) | WebView2 routing + `keld-wv` filter registration order | filter `*`, `CONTEXT_ALL`, `SOURCE_KINDS_ALL` before first `Navigate` → every `file://` request raises WRR | late registration lets the first document escape | AC02 | PROBE n2 discriminates; FACT `icorewebview2_22.md` L51 | Edge to A1 (create ordering) · falsifier: n2 |
| A3 Admitted root (identity of the root) | `keld-core` boot (dev stage: owner-private DACL; installed: KEL-254 selection) | boot admission → one retained directory handle | handle dropped and reopened by path; root substituted between admission and serving | AC04 (handle retained), KEL-254 AC7 | FACT: dev handle is dropped today (`app_session.rs:1485-1500`); installed exposes `tree_root()` path only (`windows_baseline.rs:238`) | Edge to A5 (the opener needs the handle) · falsifier: a junction swapped in for `tree` after admission is followed |
| A4 URL → components (authentication of the name) | `keld-guard` pure parser (§4.4) | request URI + root prefix → `RootRelativeComponents` or refusal | double decoding; host forms; ADS; `~`; `.`/`..`; Win32-only spellings | AC05 | PROBE: parser cells r1–r3; FACT: grammar `lib.rs:951-1178`; FACT: no `url` crate in the workspace | Independent of A5 for ADS and `~` (NT layer would resolve them); redundant with A5 for `..`, reparse, trailing dot · falsifier: AC05 NC (ADS) |
| A5 Open (OS containment) | `keld-update` `windows_fs.rs` NT-relative read-open (§4.3) | retained root + components → regular-file handle | reparse followed; directory opened; name normalized by Win32; TOCTOU through path strings | AC03, AC04 | FACT: existing relative create/delete use `OBJ_DONT_REPARSE` (`windows_fs.rs:434,533,787`); PROBE: junction refused r1–r3; directory symlink UNTESTED | Edge to A3 · falsifier: Win32 path open mutant (AC04 NC) |
| A6 Hardlink authority (authorization) | KEL-254 (root namespace + per-file DACL); W3 reads back | link count + DACL → served + acceptance record | treating namespace write as the only authority | AC17 | FACT: owner decision + correction; PROBE: hardlink served r1–r3 | Independent of A4/A5 (a hardlink is a regular file) · falsifier: role write grant on the linked object |
| A7 Response and refusal contract | `keld-wv` WRR handler | opened file or refusal → `200`+bytes+headers / `403` empty / `404` empty | Chromium's loader used as fallback; body on refusal | AC03, AC18 | PROBE: 403/empty r1–r3; ETW NOT RUN | Edge to A4/A5 · falsifier: AC03 NC |
| A8 Served-document identity (document eligibility input) | `keld-wv` per-navigation record (§4.4) | allowed `NavigationStarting(n)` + DOCUMENT response for `n`'s URI → `ServedDocument(n, components)` | WRR carries no `NavigationId`; URL text used as identity | AC06 | FACT: WRR args members; INFERRED correlation (§4.4); UNKNOWN: `IsErrorPage` for a Host `404` (§10 Q7) | Edge to A2, A7, A9 · falsifier: AC06 NC |
| A9 Navigation events → facade signals | `keld-wv` handlers → `AppWindowEvent` → `keld-core` → Bun facade | WebView2 events per `NavigationId` → ordered Electron events | fabricated or reordered events; subresource failures promoted to navigation failures | AC07, AC09 | PROBE: order r1–r3; FACT: Electron order INFERRED, GH-551 oracle owns truth | Independent of A4–A7 except through A8 · falsifier: AC07 NC |
| A10 `will-navigate` attribution and veto | `keld-wv` (cancel) + facade (emit/replay) + navigation-policy owner (allowlist) | `NavigationStarting` not matching a pending Host attempt → cancel + `will-navigate`; replay on no-veto | `IsUserInitiated` misread as initiator; synchronous Bun veto impossible (no deferral) | AC08 | FACT: no deferral on `NavigationStarting`; INFERRED: replay semantics (loses POST body, referrer, user gesture) | Edge to #1192 attempts (+1 before replay) · falsifier: AC08 NC; residual probe P3 (§10) |
| A11 Eval mechanism | `keld-wv` CDP `Runtime.evaluate` (+ `callFunctionOn` per codec owner) | script + options → settled completion value / rejection / destroyed | `ExecuteScript` runs in the new document; no promise settle; CSP-eval bypass | AC10, AC12 | PROBE n4 (ExecuteScript `{}`), r1–r3 (settled after ~500 ms, DevTools disabled); FACT `icorewebview2.md` L1408 | Independent of containment · falsifier: n4 |
| A12 Eval lifecycle/revocation | #1192 generation (captured at dispatch, re-checked at completion) + `RoleGeneration` | (G, role G) at dispatch → deliver iff both unchanged | stale delivery; role G1 → G2 | AC11 | PROBE n6; FACT `role.rs:189`; UNKNOWN: #1192 Windows mapping (N21) | Edge to A9 (phase) · falsifier: n6 |
| A13 Eval capability (fail loud) | `keld-wv` create-time self-test | three CDP calls → start / typed refusal | silent per-call rejection | AC13 | FACT: CDP not Microsoft-maintained (L177); draw.io L586–589 | Independent · falsifier: AC13 NC |
| A14 Eval value domain | #1190 codec owner (+ #539/#540) | completion value → opaque bytes ≤ bound | 4096-byte bound rejects real results; second format | AC14 | FACT D4 (falsified bound); FACT `MAX_FRAME_LEN` 16 MiB | Blocker B3 · falsifier: AC14 NC |
| A15 App response headers (S1) | facade (`onHeadersReceived` listener registry) + `keld-wv` deferral round trip | listener filter + request → modified headers or fail-closed | pass-through on timeout; round trip when no listener | AC15 | FACT D3 (Electron fires it for `file://` at source level; executed receipt NEEDS-PROBE P5) | Edge to A7, A16 · falsifier: AC15 NC |
| A16 Baseline CSP | arch 03 §2 `web.csp` owner (#541) for content; W3 for emission | policy → separate header | merged header the app relaxes; `'self'` semantics under opaque `file:` origin | AC16 | FACT CSP3 multiple policies; UNKNOWN `'self'` under `file://` (P4) | Independent of A15 · falsifier: AC16 NC |
| A17 Scheme boundary | #541/#460 navigation policy | non-`file:` URI → pass through | W3 over-reach | AC19 | FACT: filter `*` raises WRR for `http(s)` too | Independent · falsifier: AC19 NC |
| A18 Evidence provenance | this spec + receipts | hashed runs → claims | second runtime claimed; zero-failure without at-risk | AC21, AC22 | FACT: one runtime, one OS; N1/N3 non-discriminating | Independent · falsifier: validator |

Independence check: A4 and A5 overlap on `..`, reparse and trailing dot (both refuse); the
spec keeps both because A4 alone decides ADS and `~` and A5 alone decides reparse objects
that no spelling reveals (junction/symlink) — each has a cell only it decides (AC05 NC,
AC03 directory-symlink cell). A8 depends on A2 (every request raises WRR) — if A2 fails,
A8's "the Host served it" premise is false; AC02 is therefore a precondition of AC06.

### 4.2 Decisions taken as input (kept; evidence re-checked)

| Decision | Statement | Evidence this spec re-checked |
| --- | --- | --- |
| L1 | Load the app with `Navigate(file:///<admitted root>/<entry>)`; `NavigateToString` is retired for application documents. Closes W1 #1307 B6 jointly with #1192/KEL-102 (the identity of the entry is the served `(root, components)`, §4.4). | W1 AC18 (3/3); `NavTarget::Html` kept only for non-application windows (none exist on Windows today; T2 deletes the renderer-string read for app boots). |
| C2 | The Host serves every `file://` byte through WRR registered with `AddWebResourceRequestedFilterWithRequestSourceKinds("*", CONTEXT_ALL, SOURCE_KINDS_ALL)` before the first `Navigate`; no virtual host, no custom scheme, no veto-then-Chromium-loads. | Chromium canonicalises `file:` differently from any Host check (host → UNC, percent-decoding, `.lnk` 301): a check without serving is a parser differential and a TOCTOU (feasibility research). Cost UNMEASURED → P1 (§9). |
| C4 | In-root hardlinks are **served**; link count selects which files get a per-file DACL check and a write-denial control per role token (KEL-254 AC7/AC8 extension); a Legacy role forfeits app-root integrity (printed). Link-count refusal, file-ID allowlists and per-request content hashing are rejected. | 6491936458 + 6492237806; KEL-130's own rejection of link-count isolation (`kel130` L704). |
| E1/E5 | Settled eval via CDP `Runtime.evaluate` with `awaitPromise`, `returnByValue` per the codec owner, `allowUnsafeEvalBlockedByCSP:false`, `uniqueContextId` only as hardening; the authority is the #1192 generation captured at dispatch and re-checked at completion. A failed create-time capability self-test refuses app start with a typed error. | D2 scores CDP 6.0 vs BRIDGE 4.4 (correctness 8/2); BRIDGE falsified on completion values of statements and CSP-blocked `eval`. W1 rejected CDP for a **security boundary**; W3 uses it for a **function whose failure is loud** (E1b). |
| S1 | App-declared response headers: a per-request deferral round trip to Bun only when an `onHeadersReceived` listener exists; URL filters matched Host-side; fail closed on role retirement/deadline; `Complete` on the UI thread; plus a separate Keld baseline CSP the app cannot relax. | D3: Electron proxies `file:` loads and runs `onHeadersReceived` on synthesized `HTTP/1.1 200 OK` headers; static extraction falsified (runtime `isGoogleFontsEnabled`); meta-only weaker. |
| Owners | One generation owner (#1192), one codec owner (#1190). | §2 table; §10 B2, B3. |

No decision above was found contradicted by primary evidence during this write-up. One
input was corrected in scope, not reversed: the probe parser accepted the `localhost`
authority; this spec refuses every non-empty authority (§4.4 R2) because the Host's own
entry URL never carries one, so refusing it removes an alias at zero cost.

### 4.3 C3 — the one Windows "open a regular file beneath the admitted root" owner

Problem: three openers exist today (`keld-core` Win32 path walk; `keld-update` NT-relative
create/delete; `keld-native` cap-std broker). W3's input is **page-controlled**. The Win32
name layer (`CreateFileW`/`RtlDosPathNameToNtPathName`) applies DOS device names, strips
trailing dots and spaces, and treats `name:stream` as one `Normal` component; an NT-relative
open with a retained `RootDirectory` and `OBJ_DONT_REPARSE` applies none of that
normalization and refuses every reparse point in one call. The grammar owner for component
spellings already exists in `keld-guard`.

Options scored 0–10 (confidence in parentheses):

| Option | Correctness vs page-controlled names | One owner / DRY | Security boundary shape | Gate cost | Perf | Mean |
| --- | --- | --- | --- | --- | --- | --- |
| **(a) Mechanism in `keld-update` `windows_fs.rs` (one new `pub` read-open), policy in `keld-core` boot (admitted root handle), a resource-source trait in `keld-wv`** | 9 (0.9) | 8 (0.8): extends the existing NT-relative family; deletes the Win32 walker | 8 (0.8): `keld-wv` holds no root policy; `keld-core` already depends on `keld-update` on Windows (`Cargo.toml` cfg(windows) edge) | 7 (0.9): unsafe (issue-scoped `keld-update/AGENTS.md` line), public API (`keld-update` fn + accessor, `keld-wv` trait/spec), permission model | 8 (0.7) | **8.0** |
| (b) Promote the NT opener into `keld-guard` | 9 (0.9) | 5 (0.8): `keld-guard` becomes an I/O crate beside its lexical evaluator; its AGENTS allows three FFI families (NLS, token, Authenticode) and says the evaluator refuses webview principals | 5 (0.7): mixes enforcement with handle I/O in the leaf security crate | 4 (0.9): same unsafe gate plus a fourth sanctioned FFI family in the security leaf | 8 (0.7) | 6.2 |
| (c) `keld-wv` owns its own NT opener (its AGENTS already sanctions files/volumes FFI) | 9 (0.8) | 3 (0.9): a second NT-relative opener beside `keld-update`'s; `keld-wv` would also hold app-root admission | 4 (0.8) | 5 (0.8) | 8 (0.7) | 5.8 |
| (d) `keld-native` cap-std broker (KEL-130) | 4 (0.7): cap-std's Windows walk is path-based (UNKNOWN DOS-device handling) | 4 (0.9): KEL-130 is app-grant policy, not webview authorization (6462197928) | 4 (0.8) | 6 (0.8) | **0** (0.95): `max-content-8388608` < `app.min.js` 9,865,843 B | 3.6 |
| (e) Keep `open_relative_file_windows` and extend it | 2 (0.95): check-then-open on path strings; Win32 normalization | 6 (0.8) | 2 (0.9) | 8 (0.9) | 7 (0.7) | 5.0 |
| (f) New crate `keld-winfs` | 9 (0.8) | 5 (0.8): one more crate for one function; root YAGNI | 7 (0.7) | 3 (0.9): workspace membership + dependency gate | 8 (0.7) | 6.4 |

**Decision: (a).** Concretely:

1. **`keld-update::windows_fs`** gains one `pub` read-open beside the existing create/delete
   forms, with the same SAFETY discipline (`windows_fs.rs:787-834` is the template):

   ```rust
   // crates/keld-update/src/windows_fs.rs (pub, Windows-only). The only new production
   // `unsafe` of this spec; sanctioned by an issue-scoped line in keld-update/AGENTS.md.
   pub struct OpenedRegularFile { file: File, size: u64, links: u32 } // FileStandardInfo read-back
   /// Opens `components` beneath the retained `root` as a regular file: one NtCreateFile with
   /// RootDirectory = root, ObjectName = components joined by `\`, OBJ_DONT_REPARSE |
   /// OBJ_CASE_INSENSITIVE, FILE_OPEN, FILE_NON_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT,
   /// access FILE_GENERIC_READ, share READ | WRITE | DELETE. Refuses (typed io::Error kinds):
   /// reparse encountered (STATUS_REPARSE_POINT_ENCOUNTERED), directory, absent, any other status.
   /// Post-open: FileAttributeTagInfo must show neither DIRECTORY nor REPARSE_POINT.
   pub fn open_regular_file_beneath(root: &File, components: &keld_guard::RootRelativeComponents)
       -> io::Result<OpenedRegularFile>;
   impl ActivePackageSelection {
       /// `tree` opened relative to the retained version handle with OBJ_DONT_REPARSE and
       /// FILE_DIRECTORY_FILE; the caller retains it for the session. Replaces reopening
       /// `tree_root()` by path for every later open.
       pub fn open_tree(&self) -> io::Result<File>;
   }
   ```

   Sharing is maximal (`READ | WRITE | DELETE`) on purpose: the Host never blocks the root's
   mutation owner (installer, `keld dev` rebuild, or — under Legacy — the app's own Bun). A
   torn read during an owner write is that owner's mutation, not a boundary; bytes are read
   through the handle, which is bound to the object, so there is no path TOCTOU either way.
   `links` is read for AC17's receipt; it never decides service.

2. **`keld-core` boot** owns the admitted root as a **retained directory handle**: the dev
   stage keeps the handle `validate_windows_dev_stage_acl` already opens (today dropped,
   `app_session.rs:1497-1500`); the installed route calls `open_tree()` on the KEL-53
   selection. `validate_windows_boot_files` migrates its four opens (`boot descriptor`,
   `entry`, `renderer`, `permissions file`) onto `open_regular_file_beneath` with the
   descriptor's paths parsed by the same grammar, and `open_relative_file_windows` is
   deleted. The root handle plus the root's URL prefix (§4.4 `AdmittedRootUrl`) form the
   `AppResourceRoot` handed to `keld-wv` at window creation. KEL-254's "single Windows path
   owner" phrase (`kel254` L187, L200, L237; `app_session.rs:1434`) keeps naming `keld-core`
   boot as the **policy** owner; the **mechanism** is `keld-update`'s, exactly as its
   create/delete already are.

3. **`keld-wv`** defines the consumer seam and holds no policy:

   ```rust
   // crates/keld-wv/src/engine.rs (public API gate; live backend: webview2)
   pub struct OpenedResource { pub file: File, pub size: u64 } // keld-wv-owned; keld-core converts into it
   pub enum ResourceRefusal { Absent, Reparse, NotRegular, Other(io::Error) } // → 404 / 403 / 403 / 403
   pub trait AppResourceSource: Send + Sync {
       fn root(&self) -> &keld_guard::AdmittedRootUrl;
       fn open(&self, components: &keld_guard::RootRelativeComponents)
           -> Result<OpenedResource, ResourceRefusal>;
   }
   pub struct WebviewSpec { pub title: String, pub size: LogicalSize, pub initial: NavTarget,
                            pub app_resources: Option<Arc<dyn AppResourceSource>> }
   ```

   `keld-wv` depends only on `keld-guard` today and keeps it that way: the trait's input
   types are `keld-guard`'s (parser output); the output is the `keld-wv`-owned
   `OpenedResource`, which `keld-core`'s implementation builds from `keld-update`'s
   `OpenedRegularFile` (keeping `links` for its own AC17 receipt).

Review gates this decision triggers: **unsafe** (`keld-update/AGENTS.md` gains "relative NT
open-existing for read (W3 #1185)"; local `// SAFETY:` proofs; independent unsafe gate on the
final diff), **public API** (`open_regular_file_beneath`, `OpenedRegularFile`,
`ActivePackageSelection::open_tree`, `WebEngine`/`WebviewSpec` extension, `AppResourceSource`),
**permission model** (the root admission handle is now session-retained; KEL-254 owner
acknowledges the AC7/AC8 extension).

### 4.4 URL → root-relative components, the refusal contract, and document correlation

**One parser, one owner.** A pure function in `keld-guard` (no `unsafe`; no new dependency —
the workspace has no `url` crate and the task is refusal, not canonicalisation):

```rust
// crates/keld-guard/src/lib.rs (pure; adversarial fixtures permanent per keld-guard AGENTS)
pub struct AdmittedRootUrl { drive: u8 /* b'A'..=b'Z' */, components: Vec<String> /* validated, NFC */ }
pub struct RootRelativeComponents(Vec<String>); // 1..=MAX, each validate_windows_package_component + NFC
pub enum FileUrlRefusal { NotFileScheme, Authority, NoDrive, DevicePrefix, ForbiddenEscape,
                          DecodedSeparator, DotSegment, EmptySegment, OutsideRoot, Component(String), TooLong }
pub fn file_url_root_relative(url: &str, root: &AdmittedRootUrl)
    -> Result<RootRelativeComponents, FileUrlRefusal>;
```

Call sites: the W3 WRR handler and the W1 Source rule step 3 (`#1307` §4.4 "for `file:` the
path is inside the verified package root"). `NotFileScheme` is the only result that is not a
refusal: it means "not W3's" (§4.8).

Refusal table (applied in order; the first match decides; query and fragment are cut before
R3 and are never part of identity — W1 AC20):

| Rule | Input | Result | Why / evidence |
| --- | --- | --- | --- |
| R1 | scheme ≠ `file` (ASCII case-insensitive) | `NotFileScheme` → pass through | §4.8 |
| R2 | any non-empty authority (`localhost`, `127.0.0.1`, UNC host) | `Authority` → 403 | Chromium maps host → UNC; the Host's entry URL has none (stricter than the probe, which allowed `localhost`) |
| R3 | path does not start with `/<L>:/` (ASCII letter) ; or starts with `//?/`, `//./`, `\\` | `NoDrive` / `DevicePrefix` → 403 | PROBE cells `file:////?/C:/…` refused |
| R4 | `%2F`, `%5C`, `%00` in any case before decoding; after exactly one percent-decode any byte sequence that is not valid UTF-8 | `ForbiddenEscape` → 403 | Chromium rejects the same three; one decode only (double-decode mutant AC05) |
| R5 | a decoded segment contains `/`, `\`, NUL or `:` | `DecodedSeparator` → 403 | ADS and drive-relative forms; PROBE `index.html:hidden` refused |
| R6 | a segment is `.`, `..` or empty (`//`) | `DotSegment` / `EmptySegment` → 403 | no normalisation, exact only |
| R7 | drive letter or leading components ≠ the admitted root's under `CompareStringOrdinal` ignore-case | `OutsideRoot` → 403 | Windows' own case rule (`keld-guard` already uses it); mixed-case in-root spellings are **one** identity class |
| R8 | a remaining component fails `validate_windows_package_component` (`:`, `<>"\|?*`, control chars, trailing `.`/space, reserved device basename, `~`) or NFC | `Component(name)` → 403 | the only rule for `~` and ADS at the Host (the NT layer resolves both) |
| R9 | relative path > 100 bytes or more than the grammar's component count | `TooLong` → 403 | same bound as `validate_windows_package_paths`; a path that cannot be packaged cannot be served |

Divergence from Electron, stated plainly: Electron serves any readable `file:` (including
outside the app directory, `..`, UNC, short names). W3 serves only the admitted root. This is
the containment #1185 demands; the facade reports it as a documented divergence (T1 entry).

**Refusal contract (A7).** For a `file:` URI the WRR handler always sets a response: `200`
with the opened bytes and headers (`Content-Type` from a fixed extension table owned by
`keld-wv`; `Content-Length`; the baseline CSP on `text/html` documents; S1 headers when a
listener matched), `404` with an empty body when the parser accepted the name but the
opener reported absent, `403` with an empty body for every refusal above and for an opener
refusal other than absent (reparse encountered, directory, sharing/other status). No
refusal body, no `Location`, no `Range` support (UNKNOWN whether media range requests arrive
under `file://`; §10 Q8). Status codes are the Host's own and are recorded in the WRR log;
they are what AC09 maps to `did-fail-load` codes for top-level documents.

**Document correlation (A8, L2).** WRR arguments carry no `NavigationId`; a `ResourceContext
== DOCUMENT` request is raised for top-level and frame documents alike. Eligibility must
rest on the served resource, so the Host keeps one record per allowed top-level navigation:

```rust
struct PendingTopLevel { navigation: u64 /* NavigationId */, uri: FileUri /* parsed, query cut */,
                         served: Option<ServedDocument> }
struct ServedDocument { components: RootRelativeComponents, status: u16, bytes: u64 }
```

- `NavigationStarting` (top-level, allowed, `file:` scheme): push `PendingTopLevel`.
- WRR with `ResourceContext == DOCUMENT` whose parsed URI equals a pending record's `uri`
  (components under R7's ordinal rule) and whose `served` is `None`: after serving, store
  `ServedDocument` into **that** record (first match by `NavigationId` order).
- `ContentLoading(n)`: look up `n`; the document is reported to W1/#1192 as **Host-served
  with identity `(root, components)`** iff `served.status == 200` and `IsErrorPage == FALSE`;
  otherwise it is reported as not Host-served (FailedClosed under #1192). The record is
  dropped at `NavigationCompleted(n)`.
- A concurrent same-URI frame document (AC06 c) can attach to the wrong record only when
  both are the same `(root, components)`, in which case the identity is identical by
  construction; the receipt counts such ambiguities (expected 0 outside the fixture).

INFERRED: that the top-level DOCUMENT request for `n` always arrives between
`NavigationStarting(n)` and `ContentLoading(n)` on the same STA, and that a redirect hop
re-raises `NavigationStarting` with the same `NavigationId` and `IsRedirected == TRUE`
(documented for navigation events). Probe P2 (§10) counts DOCUMENT-context WRRs per
pending window over 30 loads with the same-URI iframe mutant.

### 4.5 Serving contract, threading, and app-declared headers

Byte serving happens inside the WRR callback on the engine STA (`keld-wv` AGENTS: platform
handles stay on the UI thread). The **threading arm is not decided here**; it is decided by
the P1 probe (§9, pre-implementation gate) among the arms recorded in the ledger:

- (a) synchronous read on the UI thread + `SHCreateMemStream` (what the probe did);
- (b) `GetDeferral`, read on a worker relative to the retained root handle, agile stream,
  `Complete` marshalled back to the UI thread through the tao `EventLoopProxy` (the only
  pool-callback shape `keld-wv` AGENTS sanctions);
- (c) as (b) with the stream created on the worker (Microsoft's own wording);
- (d) chunked `IStream` over the open handle versus fully buffered, for the 9.9 MB bundle;
- (e) Chromium's own file loader (perf baseline only; **not shippable**, AC02/AC03);
- (f) (b) plus the S1 Bun round trip.

Contract independent of the arm: one response per request; a deferral, when taken, is
completed exactly once on the UI thread or the request is answered `403`/empty on the
error path (an uncompleted deferral blocks rendering, threading-model L93); the stream
holds all content before completion (L61); no steady-state allocation beyond the response
buffer itself; buffered arms bound the buffer by the opened file's `size` read-back, whose
admissible maximum is the package member bound of the package owner (`keld-pack`) once it
names one (§10 Q9) — the chunked arm needs no such bound.

**S1 app-declared headers.** The facade registers the app's `onHeadersReceived` listener
with its URL filter patterns on the Host (control-plane message owned by #539/#540, wire
gate). When a `file:` response is about to be served and a registered filter matches, the
Host takes the WRR deferral, sends `(correlation, url, statusLine, headers)` to the Bun role
over the existing app link, and completes with the returned headers (or the typed cancel);
no listener → no round trip (AC15 b). The round trip has a deadline; on deadline, role
retirement (`RoleGeneration` moved) or a malformed reply the request is answered
`403`/empty — never served without the app's headers (AC15 c). The Keld baseline CSP is
always emitted as its own `Content-Security-Policy` header on `text/html` responses; CSP3
enforces every delivered policy, so the app's header can only tighten (AC16). The baseline's
**content** belongs to arch 03 §2 `web.csp` and its owner; W3 contributes the probe that
settles `'self'` semantics for an opaque-origin `file:` document (P4, §10 Q5).

### 4.6 Navigation signals

One mapping per top-level `NavigationId`, stamped with the #1192 generation current at
emission; nothing is synthesized from timers. Frame navigations (`FrameNavigationStarting`)
produce no top-level events. Codes are Chromium net error numbers as Electron exposes them.

| Electron event (facade) | WebView2 source | Condition / payload |
| --- | --- | --- |
| `did-start-loading` | `NavigationStarting` (top-level) after the Host's policy decision allowed it, or on a Host attempt | — |
| `did-start-navigation(url, isInPlace=false, isMainFrame=true)` | same event | after `did-start-loading` |
| `will-navigate(url)` | `NavigationStarting` whose `Uri` does not match a pending Host attempt and `IsRedirected == FALSE` | see cancel-and-replay below |
| `did-navigate(url, httpResponseCode, httpStatusText)` | `ContentLoading(n)` with `IsErrorPage == FALSE` and a Host-served record (`200`) for a `file:` document; for allowlisted non-`file:` documents the engine's `HttpStatusCode` at `NavigationCompleted` is the only status the Host has (reported at that later event, documented divergence) | status from the Host's own WRR record |
| `did-navigate-in-page(url, isMainFrame=true)` | `SourceChanged(IsNewDocument == FALSE)` + `HistoryChanged` | generation unchanged; no `will-navigate` |
| `dom-ready` | `DOMContentLoaded` (top-level) | — |
| `did-finish-load` | `NavigationCompleted(n, IsSuccess == TRUE)` | WebView2 documents this after `body.onload` |
| `did-fail-load(errorCode, errorDescription, validatedURL, isMainFrame=true)` | `NavigationCompleted(n, IsSuccess == FALSE)` **or** a Host-served top-level `404`/`403` record for `n` | `OPERATION_CANCELED` (14) → `-3 ERR_ABORTED`; Host `404` → `-6 ERR_FILE_NOT_FOUND`; Host `403` → `-10 ERR_ACCESS_DENIED`; `CONNECTION_ABORTED`/`RESET`/`TIMEOUT`/`HOST_NAME_NOT_RESOLVED` and the remaining `COREWEBVIEW2_WEB_ERROR_STATUS` values → the table T4 lands, verified by the GH-551 oracle |
| `did-stop-loading` | after `did-finish-load` or `did-fail-load` for `n` | the facade's `isLoadingMainFrame()` turns false here; `executeJavaScript`'s load wait (D1 a) keys on it |

Ordering of `did-start-navigation` before `will-navigate` is INFERRED from Electron
(`will-navigate` is emitted from a navigation throttle that runs after the start observer);
the GH-551 differential oracle owns the truth and T4's receipt records any correction.

**`will-navigate` under WebView2 — cancel-and-replay (A10).** `NavigationStartingEventArgs`
has no deferral: `Cancel` must be set before the handler returns, on the UI thread, which
may not block on Bun. Electron's `will-navigate` is a synchronous veto in the main process.
Three designs were scored:

| Design | Fidelity to veto | Honesty | Cost | Mean |
| --- | --- | --- | --- | --- |
| Notification only (Host allowlist decides; `preventDefault()` ineffective) | 2 | 4 (documented but apps relying on veto break silently) | 9 | 5.0 |
| Default-deny all page-initiated top-level navigations | 6 (apps that cancel everything are exact) | 7 | 8 | 7.0 |
| **Cancel-and-replay**: Host cancels every page-initiated top-level navigation, emits `will-navigate`; if no listener vetoed, the facade requests a trusted Host load of that URL (still subject to the allowlist) | 8 | 8 (residuals printed) | 6 | **7.3** (0.6) |

Chosen: cancel-and-replay. Attribution rule: a `NavigationStarting` is **Host-initiated**
iff its `Uri` equals the one pending Host attempt (#1192 `NativeAttempt`) for that window
and `IsRedirected == FALSE`; everything else is page/user-initiated. `IsUserInitiated` is
a gesture flag, not an initiator (AC08 NC). Residuals (printed in the report and in the
T1 divergence entry): the replay is a fresh top-level `GET` — a form `POST` body, the
referrer and the user gesture of the original navigation are lost; the original navigation
produces `did-fail-load(-3)`-free cancellation (the Host suppresses the cancelled attempt's
failure signal because the replay supersedes it); an app whose listener allows some
navigations sees them one event-loop turn later. Probe P3 (§10): 30 trials each of link
click, `location.href`, `location.replace`, form submit, `history.back` across a real
document, recording whether the replay's `NavigationStarting` carries `IsUserInitiated ==
FALSE`, whether redirect hops re-emit, and the latency added. If the navigation-policy owner
rejects replay (B4), the fallback is default-deny with a `keld migrate` classifier flag on
`will-navigate` listeners that conditionally allow — the second-scored design, not the first.

### 4.7 Settled `executeJavaScript`

Flow (Windows): Bun `webContents.executeJavaScript(code, userGesture)` → facade waits for
`did-stop-loading` if loading (D1 a) → control message to the Host (#539/#540 channel)
with `(correlation, role generation)` → `keld-core` checks the role generation and posts
`AppWindowCommand::Eval { view, script, correlation, role }` to the UI loop → `keld-wv`:

1. **Dispatch gate**: phase must be `Live(G)` with no pending top-level navigation record
   (`PendingTopLevel` empty); otherwise typed `DocumentReplacing` (AC12). Capture `G`.
2. `CallDevToolsProtocolMethod("Runtime.evaluate", { expression, awaitPromise: true,
   userGesture, allowUnsafeEvalBlockedByCSP: false, uniqueContextId? })`. `uniqueContextId`
   is attached only when `Runtime.enable` is on and the main frame's current context id is
   known; it is hardening, never the authority (experimental; an unknown id yields
   `InvalidParams`, which the Host treats as `DocumentReplaced`).
3. **Completion gate**: on the completion handler (same STA), re-read the phase: deliver iff
   it is still `Live(G)`; else `DocumentReplaced`. CDP `-32000 "Execution context was
   destroyed."` maps to `DocumentReplaced` too. Post `AppWindowEvent::EvalSettled {
   correlation, outcome, generation: G }`.
4. `keld-core` re-checks `(role generation, G)` before writing the reply to Bun; a retired
   role's correlation is dropped (AC11 iii).

Outcome shape handed to the facade (the facade owns Electron's message texts, §10 Q3):

```rust
pub enum EvalOutcome { Value(EvalBytes), Rejected(EvalBytes), Threw { message: String },
                       DocumentReplaced, DocumentReplacing, Refused(WvError) }
```

**Value domain and bound — recommendation to the GH-544/GH-540 owners (B3).** Electron's
contract is engine-level: evaluate as a script, return the completion value, await
thenables, structured-clone the result with no bound (D1 b/e, v45 `ValueSerializer`).
`returnByValue: true` would make JSON the domain (`Map`/`Set`/`Date`/`-0`/`undefined`
lost) — a second value format by accident. Recommendation: **one codec** — step 2 runs with
`returnByValue: false`, then `Runtime.callFunctionOn(objectId, <generated #1190 renderer
encoder>, returnByValue: true)` returns the codec's base64url bytes, then
`Runtime.releaseObject`; the Host forwards opaque bytes (V7). Both calls sit inside the same
dispatch/completion gates. This needs the codec owner to (i) extend the domain toward
structured clone (at least `Map`, `Set`, `Date`, `RegExp`, `-0`, `NaN`/`Infinity`,
`undefined`, typed arrays, `Error` fields) or record which refusals are acceptable for eval
results, and (ii) set an eval-result bound tied to `MAX_FRAME_LEN` (16 MiB) minus the
envelope rather than 4096 bytes. Until recorded, AC14 is blocked and T3 cannot merge.

**Capability self-test (A13, AC13)** runs in `create` after the controller exists and
before `navigate_initial`, inside the `GuardInstalled` mint; its deadline is the existing
`WEBVIEW2_STARTUP_BOUND`. `Runtime.enable`, when used for `uniqueContextId`, is measured in
P1 for console-event volume (D2 condition).

### 4.8 Scheme boundary

W3 contains `file:` only. For any other scheme the WRR handler sets no response and takes
no deferral (AC19); whether a `file:` page may navigate to or fetch `https:`, `data:`,
`blob:` or `about:` destinations is the navigation-policy owner's rule (#541 manifest keys,
#460/KEL-79 general policy, W1 §4.6 `web.navigation.allow` with the vetted matcher). Any
remote document that commits is not Host-served and receives no eligibility from W3.

### 4.9 Rejected alternatives

| Alternative | Why rejected |
| --- | --- |
| Virtual host (`SetVirtualHostNameToFolderMapping`) or custom scheme | Forbidden by #1185 (rewrites the `file://` senderFrame URL); Chromium still opens the files itself; changes the origin. |
| Veto-only (`NavigationStarting` cancel + WRR 403, Chromium loads the allowed bytes) | Chromium canonicalises `file:` differently (host → UNC, `.lnk` 301, percent-decoding) → parser differential; check-then-load TOCTOU. |
| Link-count refusal / file-ID allowlist / per-request content hashing | Owner decision 6491936458: refuses deduplicated installs; file IDs are not provenance; no manifest owner and unmeasured latency. |
| `ExecuteScript` / `ExecuteScriptWithResult` | Runs in the new document after `NavigationStarting`; no promise settle (probe N4 `{}`). |
| Preload-bridge eval (W1 bridge carries a `Promise` back) | Completion values of statements need `eval` (CSP-blocked); may run before W1's epoch; forgeable; no user gesture; depends on W1 (D2). |
| Static extraction of the app's CSP by `keld migrate` | Falsified: runtime-dependent policy (draw.io L646, L1003). |
| Meta CSP only | `report-uri`/`frame-ancestors`/`sandbox` unsupported in meta; not applied to preceding content; looser than the app's header policy. |
| `url` crate for parsing | Dependency gate for a function whose job is refusal of exact forms, not WHATWG canonicalisation (which would, e.g., resolve `..`). |
| Notification-only `will-navigate` | Breaks apps that rely on the veto silently (§4.6). |

### 4.10 Types & channels, capabilities, wire, platform, seam, migration

- **Rust (sketches above):** `keld_guard::{AdmittedRootUrl, RootRelativeComponents,
  FileUrlRefusal, file_url_root_relative}`; `keld_update::windows_fs::{OpenedRegularFile,
  open_regular_file_beneath}`, `ActivePackageSelection::open_tree`; `keld_wv::{AppResourceSource,
  WebviewSpec.app_resources, EvalOutcome, NavigationSignal}`; `WebEngine::eval` becomes
  `fn eval(&mut self, id: WebviewId, script: &str, on_settled: EvalCallback) -> Result<(), WvError>`
  (the arch 05 destination form); `AppWindowEvent::{Navigation { view, signal, generation },
  EvalSettled { correlation, outcome, generation }}`; `AppWindowCommand::{Eval { view, script,
  correlation, role }, HostLoad { view, url, attempt }}`.
- **TypeScript (`@keld/electron`, #1154 facade owner):** `WebContentsEventMap` gains the
  §4.6 events; `WebContents.executeJavaScript(code: string, userGesture?: boolean):
  Promise<unknown>` (the resolved type is the codec owner's decoded domain, never `any`);
  `session.webRequest.onHeadersReceived(filter, listener)` registration forwarded to the
  Host. Generated contracts regenerate from their generator; none are hand-edited.
- **Capabilities / manifest:** none new. `web.csp` baseline emission reuses the existing
  key; navigation keys are W1 §4.6 / #541's.
- **Wire:** no KIPC framing change. New control-plane kinds on the #539/#540 channel:
  eval request/settle, headers-received round trip, navigation signals, host-load request.
  Wire gate (#540 owner).
- **Platform:** Windows only. macOS/Linux backends implement the new `eval` signature with
  their native completion handlers and no W3 generation semantics (documented; §10 Q4).
- **Runtime seam:** `create` registers, inside the `GuardInstalled` mint and before
  `navigate_initial`: the WRR filter + handler, `NavigationStarting` policy/attribution,
  `ContentLoading`, `DOMContentLoaded`, `SourceChanged`, `HistoryChanged`,
  `NavigationCompleted`, the CDP self-test. Handlers live for the WebView lifetime on the
  STA; renderer crash ≠ Host crash; the admitted root handle lives for the session and is
  closed after the webview is destroyed; config (root, manifest) is captured once at boot.
- **Migration unit:** `app_session.rs` boot (`validate_windows_boot_files`, `NavTarget::Html`
  site), `webview2/mod.rs` (`navigate`, `eval`, create ordering), `engine.rs`, the
  `keld-update` opener, `@keld/electron` facade, conformance entries. No temporary adapter;
  `open_relative_file_windows` is removed in the same PR that lands its replacement.
  Compatibility fallback: not required (no published contract loads app documents by string).

## 5. Boundaries

- Implement in: `crates/keld-guard/src/lib.rs` (parser only; pure), `crates/keld-update/src/windows_fs.rs`
  and `windows_baseline.rs` (`open_regular_file_beneath`, `open_tree`), `crates/keld-core/src/app_session.rs`
  (root handle retention, boot-file migration, eval/role correlation, signal forwarding),
  `crates/keld-wv/src/engine.rs` and `crates/keld-wv/src/webview2/` (WRR, signals, CDP eval,
  self-test), `packages/@keld/electron/src` (facade events, `executeJavaScript`,
  `onHeadersReceived` registration), `docs/architecture/03,05` (T0 deviations),
  `docs/engineering/keld-error-codes.md` (new code via its owner), `keld-compat` entries (T1).
- Must not touch: `crates/keld-guard` evaluator/grammar semantics beyond the new pure
  parser; `crates/keld-ipc` framing; the KIPC channel table (#508); macOS `wkwebview/*`
  beyond the mechanical `eval` signature; `crates/keld-native`; KEL-130; workspace
  `Cargo.toml` (no new crate, no new dependency); the W1 bridge script; `keld.permissions.jsonc`
  schema (#541).

## 6. Tasks (each ≈ one PR; ordered; owner; blocking dependency)

- [ ] **T0 — this spec + architecture deviations** (owner: @amishabenramani). Blocks on §10
  B1–B7 and the owner answers to Q1–Q3.
- [ ] **T1 — conformance entries first** (owner: `keld-compat` owner, #464/#567): expected-fail
  cells and divergence entries (root-only `file:` service, cancel-and-replay residuals,
  separate baseline CSP, `did-navigate` status for remote documents). Blocks on T0.
- [ ] **T2 — one opener, admitted root, loading path, containment** (owner: Windows W3
  implementer): `keld-guard` parser + goldens (AC05), `keld-update` read-open + `open_tree`
  (AC04), `keld-core` root retention and boot migration, `keld-wv` WRR filter/handler and
  `Navigate` loading (AC01–AC04, AC06, AC17–AC19, AC22), threading arm per P1. Blocks on:
  P1 report (AC20), unsafe + public API + permission gates, #1192 T1 module (B2) for the
  phase record.
- [ ] **T3 — settled eval** (owner: same): `WebEngine::eval` destination form, CDP self-test,
  dispatch/completion gates, role-generation correlation (AC10–AC13); AC14 after B3.
  Blocks on B2, B3, the error-registry code allocation, the #539/#540 control kinds (wire gate).
- [ ] **T4 — navigation signals and cancel-and-replay** (owner: same, with #539): §4.6 mapping,
  error-code table, Host attempts (AC07–AC09). Blocks on B4 and P3.
- [ ] **T5 — app headers and baseline CSP** (owner: same, with #1154 for the facade):
  `onHeadersReceived` round trip, baseline emission (AC15, AC16). Blocks on P4, P5 and the
  `web.csp` owner's baseline content.
- [ ] **T6 — Windows qualification receipt** (owner: @amishabenramani): full RED matrix on a
  real device including the elevated ETW cell and the directory-symlink cell, pinned-app
  migration/evaluation demand slices, AC21. Blocks on T2–T5. Full application acceptance
  remains W0 #1183.

## 7. Test plan

| Test | AC | Shape and anti-flake |
| --- | --- | --- |
| `guard_file_url_root_relative` (pure Rust, every OS) | AC05 | Golden vectors for R1–R9 and the probe corpus; mutants: double decode, drop `:`, drop `~`, case-sensitive R7. Adversarial fixtures permanent (keld-guard AGENTS). |
| `update_open_regular_file_beneath` (Windows, native) | AC04 | Fixture tree with junction, file symlink, directory symlink (skipped with a printed reason when Developer Mode is absent — never silently green), hardlink, ADS, trailing-dot name; asserts refusal kinds and `links` read-back; mutant: Win32 `CreateFileW` path open. |
| `core_boot_files_one_opener` (Windows) | AC04 | Boot descriptor paths through the migrated validator; grep-style source test that `open_relative_file_windows` and page-controlled `OpenOptions::open` are absent. |
| `wv2_file_origin_matrix` (device, ignored-by-default, dedicated STA harness as `media_acceptance`) | AC01–AC03, AC06, AC17–AC19, AC22 | Fixture root + outside sentinel with a random nonce; page probes via CDP; WRR log and opener counters; the #1185 NC as a separate process; ETW session when elevated, else NOT RUN printed. |
| `wv2_navigation_signals` (device) | AC07–AC09 | Loads (i)–(iv), hash/pushState, cancel-and-replay with and without veto; ordering oracle per `NavigationId`; mutants per row; GH-551 oracle comparison recorded, not asserted, until it lands. |
| `wv2_settled_eval` (device) | AC10–AC13 | Promise/statement/reject/throw/CSP cases; replacement mid-eval; replayed G1 after G2 via test hook; role G1→G2 via the existing recovery harness (`crates/keld-host/tests/no_flag/windows/recovery.rs` family); self-test failure injected by a test-only CDP shim. |
| `wv2_headers_received` (device) | AC15, AC16 | Listener present/absent/retired; deadline; separate-header assertion via a forbidden remote script counter. |
| `codec_eval_domain` (owner's tests) | AC14 | Lands with the B3 decision in the codec crate's goldens; W3 adds only the over-bound delivery test. |
| P1 harness (`keld-benches`, Windows) | AC20 | §9; interleaved arms; never concurrent with cargo builds; raw JSON committed. |
| Receipt validator (`tools/`) | AC21, AC22 | Field presence, at-risk counts, "rule not exercised" rows. |

Anti-flake: device tests behind the existing Windows feature flag; completion observed via
callbacks pumped by `wait_with_pump`, no sleeps; each mutant in its own process; fixtures
created under a per-run temporary root with its own DACL; the directory-symlink and ETW
cells print their precondition status instead of skipping silently.

## 8. Review gates triggered

- **unsafe — mandatory:** `keld-update/src/windows_fs.rs` read-open (`NtCreateFile`
  `FILE_OPEN`, `FileStandardInfo`/`FileAttributeTagInfo` read-back) and `open_tree`;
  issue-scoped line in `crates/keld-update/AGENTS.md`; new COM registrations in
  `keld-wv/webview2` (`AddWebResourceRequestedFilterWithRequestSourceKinds`,
  `add_WebResourceRequested`, `GetDeferral`/`Complete`, `CreateWebResourceResponse`,
  stream creation, `add_ContentLoading`, `add_DOMContentLoaded`, `add_SourceChanged`,
  `add_HistoryChanged`, `CallDevToolsProtocolMethod`, optional
  `GetDevToolsProtocolEventReceiver`) with local `// SAFETY:` proofs and an issue-scoped
  update of `crates/keld-wv/AGENTS.md`'s Windows FFI list; independent unsafe gate on the
  exact final diff. `keld-guard` gains **no** unsafe.
- **public API — mandatory:** `keld-guard` parser types/function; `keld-update` opener,
  `OpenedRegularFile`, `open_tree`; `WebEngine::eval` signature, `WebviewSpec.app_resources`,
  `AppResourceSource`, `AppWindowEvent`/`AppWindowCommand` variants; `@keld/electron`
  `WebContents` events, `executeJavaScript`, `onHeadersReceived`.
- **permission model — mandatory, independent reviewer per `.agents/coordination.md`:**
  root admission handle retained for the session; refusal table as the only `file:` name
  rule; served-document identity feeding W1/#1192 eligibility; hardlink authority
  read-back; baseline CSP as a separate policy; fail-closed headers round trip; eval
  generation gates; cancel-and-replay (with the navigation-policy owner).
- **dependency addition:** none.
- **wire protocol — #539/#540 owner:** control-plane kinds for eval, headers round trip,
  navigation signals and host-load requests.

## 9. Perf impact — P1 is a pre-implementation gate

Budget at risk: architecture 01 §5 cold start → first paint (Windows measured row: host-only
`--hello` 469 ms median of 7 at `39be9cc`, `budget-scoreboard.md` §"Windows first paint";
the full product-boot arm is unmeasured). A regression > 5 % of the chosen arm against arm
(e) needs a written waiver before T2 merges. No number in this section is a claim; the
probe produces them.

Atoms (each reported separately; no pooling across classes):

| Atom | Definition | Oracle |
| --- | --- | --- |
| Census | requests per load by `ResourceContext`, bytes per request (pinned demand: `app.min.js` 9,865,843 B, `stencils.min.js` 7,688,864 B; probe fixture 68 requests) | WRR log |
| Work | bytes read + bytes streamed per arm; UI-thread occupancy per WRR callback (longest, sum) | handler timers (QPC) |
| Queue/copy | buffer copies per response (read buffer, stream copy, marshal) and deferral hand-offs per arm | code census + counters |
| Clock | one monotonic clock armed before the host process spawn → KEL-64 double-rAF paint beacon; CDP `Network` timings as a diagnostic lane only | KEL-64 harness |
| Statistic | median and p95 with bootstrap CI; **30 valid samples per arm per class** (`fresh-process` and `warm-cache` separate, arch 01 §5.1); arms interleaved; never concurrent with cargo builds | registry policy |
| Artifact | raw JSON per run in `keld-benches` (`windows/bench/`), hash-chained receipt and a keld-research note; the chosen arm named in T2's PR | keld-benches + keld-research |

Arms: (a)–(f) of §4.5 plus (g) = chosen arm with `Runtime.enable` on (console-event volume).
Also reported: per-eval round-trip latency (dispatch → settle) and per-request S1 round-trip
latency, as diagnostics with their own medians.

## 10. Approval blockers and open questions

**Blockers (must clear before `Status: approved`; each names who acts and the first
observable completion check):**

- **B1 Owner approval of C3 and the C4 acceptance binding** — @amishabenramani: an exact-content
  approval comment on #1185 naming this file's head. Check: the comment exists and quotes the head.
- **B2 #1192 T1 as a platform-neutral pure module and the Windows mapping (N21)** — #1192 owner
  (@0monish): amend #1192 §5 so the checked `u32` phase machine is a backend-neutral module
  consumed by `keld-wv/webview2`, and accept §4.6/§4.7 here as the Windows oracle. Check:
  #1192 revision lists the module and names W3 as the Windows oracle.
- **B3 Eval-result value domain and bound** — GH-544 (#1190) and GH-540 owners with #539:
  record the domain (structured-clone extension or explicit refusals) and a bound tied to
  `MAX_FRAME_LEN`, and whether the two-call `callFunctionOn` encoding is the mechanism. Check:
  a recorded decision on #1190 or #539; AC14 unblocked.
- **B4 Cancel-and-replay for `will-navigate`** — navigation-policy owner (#541/#460, W1
  §4.6 owner): accept replay as a trusted Host load subject to the allowlist, or choose
  default-deny with the classifier flag. Check: decision comment on #541 or #1184.
- **B5 Error-registry code** — `docs/engineering/keld-error-codes.md` owner: allocate the
  typed code for the CDP capability refusal and the eval `DocumentReplaced`/`DocumentReplacing`
  outcomes. Check: codes present in the registry.
- **B6 KEL-254 AC7/AC8 extension** — KEL-254 owner: sign the per-file DACL read-back and
  write-denial control for multi-link files and the printed Legacy forfeiture. Check: KEL-254
  §3 amended or an owner comment accepting T0's wording.
- **B7 Every reused contract is an open draft** — #1192, #1190, #1307, #1154 (facade), #540
  (#1122): this spec cannot be approved as implementable before those it consumes are at
  least approved; re-checked at approval time.
- **B8 P1 report** — Windows operator: the §9 report exists before T2 (AC20). Check: PR link
  to the keld-benches JSON and the chosen arm.
- **B9 Probe evidence published** — Windows operator: the #1185 probe bundle (`results.json`,
  `SHA256SUMS`, host source, fixtures) lands as a keld-research note + artifact PR (today it is
  local to Ramani; review 6462197928 noted it was not independently inspected). Check: PR URL
  on #1185.

**Open questions:**

1. **Q1 (owner):** keep maximal sharing (`READ | WRITE | DELETE`) on served opens (§4.3), or
   `READ | DELETE` so a concurrent writer gets a sharing violation? Recommendation: maximal;
   the Host must never block the root's owner.
2. **Q2 (owner):** should the dev-stage route refuse to start when `validate_windows_package_paths`
   would reject a staged path (same rule as the installed route), making R9 symmetrical?
   Recommendation: yes, in T2, with a `keld dev` diagnostic naming the path.
3. **Q3 (#1154 facade owner):** Electron error texts for sync throws, document replacement
   ("WebFrame was removed before script could run…" in v45) and the `executeJavaScript`
   resolved type; which Electron major the facade pins (draw.io ^44 vs stable 45 differ, D1 c).
4. **Q4 (#542/KEL-142 owners):** adopt the `eval` destination signature on macOS with
   BridgeState's own retirement semantics now, or keep a typed "settled eval not owned on this
   backend" refusal until #1192 T1 lands there. Recommendation: the typed refusal (fail
   closed, no stub).
5. **Q5 (P4) UNKNOWN:** CSP `'self'` semantics for an opaque-origin `file:` document on
   WebView2 156 (does `script-src 'self'` admit in-root `file:` scripts?). Probe: AC16 fixture
   with `script-src 'self'` vs `script-src file:`; 30 loads; decides the baseline's grammar
   with the `web.csp` owner.
6. **Q6 UNKNOWN:** whether `keld dev` stages are validated by `validate_windows_package_paths`
   today (grep found the call only under `keld-update`). First action: trace the boot compiler's
   stage writer.
7. **Q7 (P2) UNKNOWN:** `IsErrorPage` for a Host-served `404`/`403` top-level response, and
   the DOCUMENT-WRR-per-pending-window count with the same-URI iframe mutant (§4.4). 30 loads.
8. **Q8 UNKNOWN:** whether media `Range` requests reach WRR under `file://` and whether the
   `Content-Range` contract (L61) must be implemented for `<video>`/`<audio>` sources. Probe:
   fixture with a seekable media element; if yes, T2 adds range serving.
9. **Q9 (keld-pack owner):** the maximum package member size, which bounds buffered serving
   arms; absent a number, only the chunked arm is admissible.
10. **Q10 (P3):** cancel-and-replay residual census (§4.6): 30 trials per initiator kind;
    the facade divergence entry quotes the results.
11. **Q11 (P5):** one executed Electron 44 receipt that `onHeadersReceived` fires for a
    `file://` top-level document and subresources (D3 holds at source level only).
12. **Q12 UNKNOWN:** 8.3 short-name generation state of the qualification volumes
    (`fsutil 8dot3name query` needs elevation); if disabled, AC05's `~` cell reads "rule not
    exercised" and the receipt says so.

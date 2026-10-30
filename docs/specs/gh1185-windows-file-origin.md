# Spec: Windows WebView2 `file://` navigation, app-code resource containment, honest navigation signals and settled `executeJavaScript` (GH-1185 W3)

Status: draft (revision r3 after isolated hostile reviews of `571f6bda` and `5ecd3496`)
Linear: GH-1185 (GitHub tracker under #517; parent program #1182 W3; webContents owner #539) ·
Owner: @amishabenramani (real Windows operator) · Updated: 2026-10-30

Code citations are at `origin/main` `457a9c3b37f71179f167bd1f8c706c461cc4c479`. Device
evidence is one runtime and one OS build: WebView2 Evergreen `156.0.4314.50` on Windows 11
`10.0.26300`, `webview2-com(-sys) 0.38.2` (SDK 1.0.3650.58). The native probe is the
[#1185 probe report](https://github.com/gyldlab/keld/issues/1185#issuecomment-6461689232)
(3 identical baseline **runs**, each loading two documents — `index.html` and
`index.html?x=2` — with 68 `WebResourceRequested` events per run of fixture demand, not per
load and not the pinned application's demand; single-fault controls N1–N6; `results.json`
sha256 `acd3cbe4…34e6`; 31 request cells). Its published form is keld-research note 268
(draft PR [0monish/keld-research#38](https://github.com/0monish/keld-research/pull/38),
head `1524f3e1`, runs `artifacts/runs/gh1185-w3-file-origin-probe-2026-10-28/` and
`artifacts/runs/gh1185-capstd-junction-2026-10-30/`), which also
records the owner's evidence corrections this revision applies: **N6 compared two
constants and never replayed a CDP result (`main.rs` L1170–1174), so #1185's eval-replay
negative control is UNMET and N6 is cited nowhere below as evidence**; only a dedicated
`new Worker` was attempted (`www/app.js` L93), shared/service workers untested; N2's
`indexServedByHost` reflects the second document only; the page-nonce oracle was validated
for `fetch`/XHR only; the exclusive-lock oracle had an empty at-risk set; 8.3, `.lnk`,
drive-case and `sendBeacon` were not exercised; the directory-symlink `403` was
object-not-found (`0xC0000034`), not a reparse refusal; the probe's CSP was a fixture
policy, not a production one. Source-pinned research is the owner's D1–D6
report (Electron v44.9.3 `c542df1e` / v45.1.0 `45d20ec0`, Chromium 152/156, V8
`392e4442`, WebView2 Win32 reference `38bc5e98` folder `1-0-3650-58/`, draw.io desktop
`2edf9fb9`). Electron oracle facts in this revision were re-read at v45.1.0 `45d20ec0`:
`shell/browser/electron_navigation_throttle.cc` L42–62 (`will-navigate` only when
renderer-initiated and main frame, from `WillStartRequest`), L66–86 (`will-redirect`);
`shell/browser/api/electron_api_web_contents.cc` L2745–2752 (`did-start-navigation` from
the `DidStartNavigation` observer, before throttles), L2872 (`DidFinishNavigation` returns
unless committed), L2932–2933 ("Do not emit `did-fail-load` for canceled requests"),
L2599/L2620 (`did-start-loading`/`did-stop-loading`); `docs/api/web-contents.md` L270–276
(`will-navigate` not emitted for `loadURL`/`back`). Owner decisions on resource authority
are [#1185 6491936458](https://github.com/gyldlab/keld/issues/1185#issuecomment-6491936458)
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
CSP, a `will-navigate` listener that cancels every navigation at L2192, awaited
`executeJavaScript` results whose rejection it converts into an irreversible migration mark
at L586–589 and L640–642). Framework consequences that fall out of pre-existing Keld rules
are stated as such: the Windows package grammar already refuses `~` and paths over 100
bytes, so a bundle named like webpack 4's `vendors~main.js` is neither packaged nor served.

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
`did-fail-load` and `did-stop-loading` arrive in the facade with the emission rules Electron
v45.1.0 applies (§4.6); an awaited `executeJavaScript` promise settles with the page's
completion value and a result produced for a replaced document is rejected before any
caller observes it.

Non-goals:

- A virtual host, a custom scheme, or any rewrite of the application's `file://` URL
  (#1185 non-goal; W1 AC05/AC18 rely on the genuine URL).
- Navigation **policy** for non-`file:` destinations (`https:`, `data:`, `blob:`,
  `about:`): §4.8 names the owner; W3's filter never raises `WebResourceRequested` for them.
- A second navigation-generation counter or a second value codec: the generation is
  #1192's checked `u32` document generation plus the existing `RoleGeneration`
  (`crates/keld-runtime/src/role.rs:189`); the value codec is #1190's. §4.7 records a
  recommendation and §10 B3 makes the eval-result domain and bound an owner decision.
- Any navigation replay by the Host. A page-initiated navigation is allowed or cancelled
  on its original `NavigationStarting`; the Host never re-issues it (§4.6).
- `asar` archives: Keld packages from source; no `asar` reader exists in `crates/`,
  `packages/` or `docs/` (grep 2026-10-29). Not in scope.
- macOS/Linux loading paths (#542/KEL-142 own macOS; Linux UNKNOWN); `<webview>` tags;
  `will-frame-navigate`, `will-redirect` and `did-frame-*` events; `webContents` methods
  beyond `executeJavaScript` and the events above (`stop`, `reload`, `loadURL` facade
  vocabulary stay with #539/#459); screen/menu/dialog scope.
- Workers under `file://`: WebView2 156 refused the probe's **dedicated** `new Worker`
  from a `file://` document with `SecurityError` (`www/app.js` L93); shared and service
  workers were **not attempted** (note 268), so their behaviour under `file://` is UNKNOWN
  and the environment-level worker filter below is coverage, not evidence. The environment-level worker filter in §4.5 keeps coverage
  for a future runtime; the probe cannot show it (§3 AC22).
- Conformance score changes (#464/#567 own cells; nothing here flips a row).

## 2. Spec refs

Governing architecture: [03-security](../architecture/03-security.md) §1 (webview
principal = Host WebView id + navigation generation), §2 (`web.csp`; the vetted path/URL
scope matcher and its documented lexical contract, L140–160), §4.3 (webview hardening) and
§6 honesty ledger; [05-webview-and-native](../architecture/05-webview-and-native.md) §1
(Windows is direct COM; the destination `WebEngine::eval(id, script, cb: EvalCallback)` at
L26; "a host-backed resource callback … the observable policy and response semantics stay
host-owned", L185–192); [01-overview](../architecture/01-overview.md) §5 (first-paint
budget; >5 % regression needs a written waiver; §5.1: 30 valid samples per class);
[04-electron-compat](../architecture/04-electron-compat.md) §3 (documented emulation where
the engine differs).

**Two name owners (decision 1, recorded for T0's arch 03 §2 amendment).** `keld_guard::
path_in_scope` (`crates/keld-guard/src/lib.rs:1276-1293`, reached only through `evaluate`,
`:784-819`) stays the owner of **manifest scope patterns**: byte-exact, case-sensitive,
literal `/` boundary, no percent-decoding ("This is not URL normalization", `:779-781`;
arch 03 §2 L148–156), including non-`file:` `web.navigation.allow` entries. The W3 parser
`file_url_root_relative` (§4.4) is the owner of **"`file:` URL → admitted-root membership +
root-relative components"** for three consumers: W3's `WebResourceRequested` serving, W1's
per-call `Source` rule, and in-root `file:` navigation decisions. W1 (#1307, revised in
parallel; published head `043170ca`, local `53ec78e8`; its Source rule at L336–341 already
reads `url_scheme_is(src, "file") && keld_guard::file_url_root_relative(src, &entry.admitted_root).is_ok()`)
uses exactly `scheme == "file" && file_url_root_relative(source, root).is_ok()` and no origin
equality: the `url` crate's
`Origin` for `file:` is a fresh opaque value per call (`url-2.5.8/src/origin.rs:33, 66-68`),
so an origin comparison can never be true. The matcher cannot own `file:` membership
without changing its documented lexical contract for every grant; the parser cannot own
scope patterns without becoming a second matcher. T0 adds both owners to arch 03 §2.

Deviations this spec records (all land in **T0**, the PR that approves this spec):

- Architecture 05 §1 lists `eval` as fire-and-forget in the live trait
  (`crates/keld-wv/src/engine.rs:348`) while naming the callback form as the destination
  (05 L26). W3 moves the live trait to the destination form (§4.10); macOS/Linux keep their
  native completion handlers without W3's generation semantics until #542/KEL-142 adopt
  them (§10 Q4).
- Architecture 03 §4.3 says CSP injection "is not in this slice". W3 adds the Windows
  `file://` baseline CSP (§4.5); T0 updates that sentence and adds the §6 ledger line
  "Windows `file://` documents: the Host serves app-code bytes as currently present in the
  admitted root; a Legacy role can rewrite them (#1185 6492237806)".
- Root `AGENTS.md` "Hot kipc/event-loop/guard paths … no steady-state allocation": the
  `WebResourceRequested` handler runs on the engine event loop and allocates one response
  buffer per request in every arm of §9 except a chunked stream, and the navigation-signal,
  eval-settle and header round-trip payloads allocate on the same loop. T0 records these as
  one named, measured exception (not a kipc path), bounded by P1 and the B10 instruction
  wording.
- `crates/keld-wv/AGENTS.md` L5–6 ("platform handles MUST NOT be touched on I/O/pool
  threads"; the only sanctioned pool callback posts via the tao `EventLoopProxy`): §4.5
  fits the design to that rule (a worker touches only the file handle; every COM object —
  args, deferral, stream — stays on the STA). Arm (c) of §9 (stream created on a worker)
  would need an instruction change and is therefore measured but not adoptable without one
  (§10 B10).
- KEL-254 AC7/AC8: the owner decision extends the ACL read-back from the root to the
  per-file DACL of every in-root file whose link count is > 1, and adds a write-denial
  control per role token for those files. T0 records that extension in KEL-254 §3 (its
  owner signs, §10 B6).
- `keld-guard` KEL-53 T3 FFI sanction (`crates/keld-guard/AGENTS.md`, trigger
  `validate_windows_package_paths`): the two NLS calls (`NormalizeString`,
  `CompareStringOrdinal`) exist today only as locals inside that function
  (`lib.rs:981-1053`). The parser needs them for R7/R8 and therefore relocates them into
  shared private helpers with the same bounds; no new FFI function, but the sanction's
  trigger line gains `file_url_root_relative` (unsafe gate, §8).

| Existing owner (reuse, no duplicate) | Exactly what this spec consumes | Decision / divergence |
| --- | --- | --- |
| [#1192 GH-542 checked navigation binding](https://github.com/gyldlab/keld/pull/1192) (**open draft**, `cd30f997`) | §4.2 `NativeDocumentPhase`, checked `u32`, +1 on real replacement commit, +1 **before** a trusted Host load (L110), **+0 on renderer provisional start/cancel** (L107) and same-document changes; §4.4 eligibility = verified resource identity; §4.5 "stale evaluation completion … never delivers to a new document"; N15 Bun recovery keeps the document; N16 one generation; N21 Windows UNKNOWN; L145 four bounded CONTROL records per role. | Reused as the only generation and phase model; W3 is its Windows oracle. Page-initiated navigations stay page-attributed (+0 on cancel, +1 at commit); W3 issues no Host load for them, so no #1192 amendment is needed for navigation. W3 needs #1192 T1 as a platform-neutral module (§10 B2). |
| `RoleGeneration` (`role.rs:189`) and the app-link retirement owner (KEL-75/KEL-143) | Per-role generation; retirement of role-bound correlations on recovery. | Eval and header round-trip correlations are keyed on it. |
| [#1190 GH-544 codec](https://github.com/gyldlab/keld/pull/1190) (**open draft**, `115af9ac`) | One Host-opaque codec (V7); V3 synchronous refusal of functions; **V4: accessor properties fail typed with getter counter 0** (accessors are never invoked); V6: "Proxy traps can run before returning"; the 4096-byte bound it claims for `executeJavaScript` results (L9–12). | Reused as the only codec for its domain and decoder ownership. The 4096-byte bound is falsified as an eval-result bound (D4). **V4 is not met by the chosen transport (v)**: CDP deep serialization invokes own enumerable getters and drops symbol/non-enumerable keys (§4.7); whether that is accepted for eval results (Electron's structured clone also runs getters) is the codec owner's decision (§10 B3/Q15). |
| [#1307 W1 preload facade spec](https://github.com/gyldlab/keld/pull/1307) (revised in parallel) | §4.4 Source rule step 3, §4.5 lifecycle (`ContentLoading` mints/revokes; allowed `NavigationId`), §4.6 navigation policy keys (`web.navigation.allow`, `onLoss`), §4.6a loading-path requirement, blocker B6; AC10 (the application document survives a vetoed navigation). | W3 closes B6 with L1. W1's `file:` Source rule and W3 share the one parser (above). W1 `043170ca` L390–394 ("the bytes served for it are that verified resource") must adopt W3's identity: name inside the admitted root, bytes as currently present (owner Q2 in 6491936458) — §10 B7. |
| [#541 GH-541 spec](https://github.com/gyldlab/keld/pull/1123) and #460/KEL-79 | "no new web.navigate grammar is authorized"; full navigation policy is #460/KEL-79; first-proof eligibility from "boot/resource containment and native navigation evidence". | W3 provides that evidence for `file:` documents and nothing for other schemes (§4.8). The `will-navigate` rule in §4.6 is allowlist-first and needs no new grammar; the allowlist derivation by `keld migrate` is recorded for that owner (§10 B4). |
| [KEL-254 installed-root boot](kel254-windows-installed-root-boot.md) (approved) and `keld_update::ActivePackageSelection` (`crates/keld-update/src/windows_baseline.rs:207-216`: retains `_roots`, `_version: Dir` **and `tree: Dir`**; exposes `tree_root() -> &Path`, `:238`) | Admission of the installed root; AC7/AC8 access profiles; AC13 Legacy refusal. | W3 **lends the existing `tree: Dir` handle** through one borrowing accessor (`tree(&self) -> &Dir`); no reopen by path, no new handle (§4.3). |
| `keld-update` cap-std leaf opener `open_pinned_leaf(parent: &Dir, leaf: &str)` (`windows_baseline.rs:589-599`: `read`, `FILE_SHARE_READ`, `FollowSymlinks::No`), used directly by `locate.rs:311, 375` and `windows_baseline.rs:606` (`open_machine_file`, whose callers are `helper.rs:207`, `load.rs:481`, `load.rs:1075`, `load/version_contents.rs:58`, `windows_baseline.rs:668`) | The one existing no-follow leaf open beneath a retained directory handle. | **Generalised**, not duplicated: it becomes the single-component case of `open_regular_beneath` (§4.3) and its callers migrate in the same PR, so "exactly one opener" is true (AC04). |
| cap-primitives 4.0.3 (already a workspace dependency through `keld-update` and `keld-native`): Windows `open_impl` = `manually::open` per-component walk (`src/windows/fs/open_impl.rs:29`); each component opened relative to the directory handle through `CreateFileAtW` → `NtCreateFile` with `RootDirectory = dir` (`create_file_at_w.rs:155-158, 242`), `FILE_OPEN_REPARSE_POINT` under no-follow (`:220`); `..` re-roots to an ambient `CreateFileW` (`open_unchecked.rs:60-105`) — unreachable here because the parser refuses `..` | The relative no-follow open mechanism and its already-reviewed `unsafe`. | Reused through the existing dependency; r1's claim that cap-std's Windows walk is "path-based" was wrong and is withdrawn. Named requirement left to a probe cell: a junction on an **intermediate** component is refused by the walk (AC03 cell; cap-std's `is_symlink` classification of mount points UNVERIFIED). |
| cap-fs-ext 4.0.3 `MetadataExt::nlink` (`metadata_ext.rs:30`), `DirExt::open_dir_nofollow` (`dir_ext.rs:87`) | Link-count read-back; no-follow directory step. | Reused for AC17 and the component walk. |
| `keld-native` `unsupported_reparse` (`crates/keld-native/src/fs.rs:1352-1357`: any reparse attribute that is not a symlink is refused) | KEL-130's reparse-tag policy. | W3 applies the same **"any reparse point at any component → refuse"** rule (symlink, mount point, cloud placeholder, dedup, WOF alike); consequence stated in §4.3. KEL-130 itself is **not** reused (app-grant policy; 8 MiB content cap below the pinned 9.9 MB bundle). |
| `keld-core` dev-stage admission: `validate_windows_dev_stage_acl` (`app_session.rs:1485-1500`) opens the root with `FILE_FLAG_OPEN_REPARSE_POINT` + `FILE_FLAG_BACKUP_SEMANTICS`, validates the owner-private DACL, then drops the handle; canonical layout `<project>/.keld/dev/<32-lowerhex>` (`:1668-1678`; `keld-cli/src/boot.rs:428`) | The dev-stage DACL policy (KEL-96). | W3 retains that validated handle as the admitted root (`Dir::from_std_file`) for the session. The project path may carry `~`, spaces and non-ASCII: the root prefix is validated with `validate_fs_component` (`lib.rs:933-938`), never with the package grammar (§4.4). |
| `keld-core` boot file opener `open_relative_file_windows` (`app_session.rs:1729-1789`: Win32 path walk + `OpenOptions::open`) | Its callers in `validate_windows_boot_files` (`:1440-1479`). | **Deleted** in T2; callers migrate to `open_regular_beneath`. |
| `keld-guard` component grammar (`validate_windows_package_component` `:951-958`, `validate_windows_component` `:1145-1164`, `validate_portable_component`, `validate_windows_package_paths` `:975-1090`) and `validate_fs_component` (`:933-938`) | The one Windows component grammar (page-controlled components) and the ordinary fs component rule (Host-admitted root). | Reused unchanged. |
| `keld-wv` Windows backend: `create` ordering (`webview2/mod.rs:3560-3645`), `navigate` (`:3647-3659`), `eval` (`:3661-3674`), `NavigationStarting` observer (`:2276-2297`), `GuardInstalled` + deny-all `NewWindowRequested` (`guarded_navigation.rs:50-154`), `AppWindowEvent` (`engine.rs:142-143`, `#[derive(Copy)]`) | Create ordering, the guard mint, the event channel, the pool-callback-posts-via-`EventLoopProxy` shape. | Extended inside the same mint. New event payloads (URLs, outcomes) cannot be `Copy`: `AppWindowEvent` drops `Copy` (public API gate, §4.10). |
| `keld-core` `app_session.rs:3653`: `NavTarget::Html(inputs.html)` | The loading-path site. | **Must change**: L1. |
| KIPC frame cap `MAX_FRAME_LEN = 16 MiB` (`keld-ipc/src/lib.rs:113`) | Ceiling for any eval-result bound. | Cited in §4.7. |
| `@keld/electron` `WebContents` (`browser-window.ts:85-104`: only `destroyed`) | The facade surface W3 extends. | Extended by T5 under #1154's ownership (§10 Q3). |

Microsoft documentation relied on (pins in the D1–D6 report unless a line is given from
`webview2-win32-reference@38bc5e98` folder `1-0-3650-58/`, re-read 2026-10-30):
`ICoreWebView2WebResourceResponse` stream wording (`icorewebview2webresourceresponse.md`
L61); `add_WebResourceRequested` ("by default raised for file, http, and https URI
schemes", `icorewebview2.md` L957; a deferral blocks the request, L953);
`AddWebResourceRequestedFilter` ("The wildcard filter is compared to the URI after the URI
has been normalized, any URI fragment has been removed, and non-ASCII hostnames have been
converted to punycode", L1307; `*` matches all URIs, table in `icorewebview2_22.md`);
`ICoreWebView2_22::AddWebResourceRequestedFilterWithRequestSourceKinds` ("filters have to
be added before main page navigation", `icorewebview2_22.md` L51; service/shared worker
requests are raised for **all** CoreWebView2s with a matching filter in the environment, so
such a filter should be added on only one, L72); `ExecuteScript` after `NavigationStarting`
runs in the new document (`icorewebview2.md` L1408); "CDP method calls may be processed out
of order" (L1333); CDP not Microsoft-maintained (`how-to/chromium-devtools-protocol.md`
L177); threading model (`concepts/threading-model.md` L21–23, L93); member lists of
`ICoreWebView2NavigationStartingEventArgs` (`Cancel`, `IsRedirected`, `IsUserInitiated`,
`NavigationId`, `RequestHeaders`, `Uri`; **no deferral**), `…EventArgs3::NavigationKind`
(`BACK_OR_FORWARD = 1`, `NEW_DOCUMENT = 2`, `RELOAD = 3`; `webview2-com-sys-0.38.2/src/bindings.rs:483-488, 23955`),
`ICoreWebView2ContentLoadingEventArgs` (`IsErrorPage`, `NavigationId`),
`ICoreWebView2NavigationCompletedEventArgs(2)` (`IsSuccess`, `WebErrorStatus`, `NavigationId`,
`HttpStatusCode`), `ICoreWebView2SourceChangedEventArgs` (`IsNewDocument`),
`ICoreWebView2WebResourceRequestedEventArgs(2)` (`Request`, `Response`, `ResourceContext`,
`GetDeferral`, `RequestedSourceKind`; **no `NavigationId`**), `WebResourceResponseReceived`.
Known runtime defect: [WebView2Feedback #5764](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5764)
(open, 2026-10-16): a service worker using static routing stalls in STARTING when the
WebView has any `WebResourceRequested` filter — irrelevant under `file://` (no service
workers) and recorded for the non-`file:` policy owner.
V8 `392e4442` `src/inspector/v8-runtime-agent-impl.cc`: `innerCallFunctionOn` calls
`scope.allowCodeGenerationFromStrings()` **unconditionally** (L151); `evaluate` calls it
only when `allowUnsafeEvalBlockedByCSP` is true or absent (L400–402); `uniqueContextId`
resolution (L218–232). crdtp `protocol_core.cc` L124–128: an unknown field is not an
error. W3C CSP3 "multiple policies": every delivered policy is enforced.

## 3. Acceptance criteria (binary, each becomes a test)

Each row's negative control (NC) changes exactly one rule. Counters: **page nonce** = the
outside sentinel's random nonce read from the page through CDP (independent of the Host);
**WRR log** = the Host's per-request record; **opener count** / **bytes read** = calls of
the one opener (§4.3) and bytes read through it, per URL; **signal log** = the facade's
ordered event list per `NavigationId`; **page oracle** = `location.href` and the document
body's nonce read through CDP after each navigation (independent of Host records); **eval
ledger** = the Host's per-correlation ledger. "Baseline 0" means the counter reads 0 across
the whole at-risk set; a row whose at-risk set is empty says **rule not exercised** and
counts for nothing. At-risk sets are split by the rule that decides the cell, so a cell the
parser refuses (opener count 0) is never counted as an opener success and vice versa.
Windows rows are **real-device only**; CI mocks, macOS and Linux are not substitutes.

| ID | Given / when | Pass observable | Negative control (one fault → RED) |
| --- | --- | --- | --- |
| AC01 Loading path (L1) | A compat window boots an application whose entry is `<root>/index.html` (the boot descriptor's entry path; no query — `loadURL` with a query is #459's) | The Host calls `Navigate(file:///<root>/index.html)`; the first top-level `ContentLoading` carries `IsErrorPage == FALSE`; `location.href` read via CDP parses to the same root-relative components and query (compared with the exported component equality, never as URL text — Chromium may upper-case the drive letter, and the pinned application itself upper-cases it, `electron.js` L1012); W1's `Source` parses the same way. | Load via `NavigateToString` (`app_session.rs:3653` → `mod.rs:3654`) → `Source = about:blank`, `location.origin == "null"` (W1 AC18) → RED. |
| AC02 `file:*` filter before first navigation (C1) | Same boot, filter `file:*` + `CONTEXT_ALL` + `DOCUMENT` source kind registered per webview before `navigate_initial` | Every `file://` request of the load appears in the WRR log with a Host verdict (**new probe cell**: a `file:*` baseline must reproduce the per-run census of the `*` probe, 68 events per run over its two document loads, 3/3 — the probe used `*`, so its evidence does not transfer until this cell runs); the entry's raw inline script that only runs without the Host CSP header did not run (until P4 settles the production baseline, this oracle uses a **test-only** header policy that blocks the fixture's unnonced inline script), checked for **both** documents of the run (note 268: N2's `indexServedByHost` reflected only the second document). | Register the filter after the first `Navigate` under `file:*` (probe N2 re-run) → the first `index.html` escapes to Chromium's file loader, the raw inline script runs → RED (at-risk 1 per document load). |
| AC03 Containment matrix (C2, C4) | The page issues `fetch`/XHR/`<img>`/`<script>`/`<link>`/`<iframe>`/`@font-face` requests for the cells below | **Served** (`200`, bytes): in-root files, mixed-case in-root spellings, interior `//` (collapsed, §4.4 R6), an in-root **hardlink** to an outside file (owner decision). **Parser-refused** (`403`, empty, opener count 0): **plain outside path** (`file:///C:/…/OUT/sentinel.txt` — missing from the probe, added), `%2e%2e`, `%5C`, `%2F`, `%00`, UNC/URL-host forms, `file:////?/C:/…`, `name:stream`, `name.`/`name␠`, `CON`/`COM1`, a `~` spelling. **Opener-refused** (`403`, empty, opener count 1, bytes read 0): a junction as a **leaf**, a junction as an **intermediate** component (mixed case too), a file symlink, a **directory symlink** (the probe's `sym-fetch` cell was `0xC0000034` object-not-found, i.e. the link could not be created unprivileged — not a reparse refusal; mandatory real-device cell with Developer Mode). The opener cells' negative control is AC04's (Win32 path open / multi-component `open_with`); the mixed-case served cell is **re-run under (g)** (the probe opener's `OBJ_CASE_INSENSITIVE` does not transfer, §4.3). Page nonce 0 for every refused cell, with the nonce oracle validated per request kind (the probe validated it for `fetch`/XHR only; `img`/`script`/`css`/`iframe`/font cells need their own read-back oracle — script self-report, CSS variable, frame body — before they count). Not exercised by the probe and therefore device cells here: 8.3 spellings, `.lnk` files, drive-letter case, `sendBeacon`. At-risk sets reported separately: parser cells × runs, opener cells × runs. Evidence attribution, stated plainly: the probe's `url-dotdot` cell raised no WRR event (Chromium canonicalised `%2e%2e` before the filter), `url-long` threw "Failed to parse URL" in the page, and `url-backslash` was refused by the probe's own decode; those three rows are therefore **Chromium-decided or probe-decided in the evidence**, and R3/R4/R6 are proven only by the pure goldens (AC05) until the cells are re-run against the Host parser with a request that reaches WRR. | **#1185's own control**: set no response for requests whose `ResourceContext` is not `DOCUMENT` (subresource containment deleted; Chromium's file loader serves them) → the plain-outside `fetch` delivers the sentinel nonce → RED; baseline 0 across the parser set. A weaker mutant (N2) scored 1/1. |
| AC04 One opener (C3) | Any app-root open in `keld-core` boot, `keld-update` locate/helper or `keld-wv` serving | Exactly one opener exists (`open_regular_beneath`, §4.3); `open_relative_file_windows` is gone; `open_pinned_leaf` is its single-component call; a source test finds no `OpenOptions::open`/`CreateFileW`/`Dir::open_ambient_dir` on app-root or page-controlled input in `keld-core`/`keld-wv`/`keld-update` serving paths. | Replace the opener with a Win32 path open (`CreateFileW` on `root\<components>`), grammar unchanged → the **intermediate-junction** cell (parser passes `junc/x.txt`; only the opener can refuse it) serves the outside sentinel → RED. Separately replace the per-component walk with one multi-component cap-std `open_with` → an **intermediate directory-symlink** cell (relative, in-root target) or a cloud-placeholder intermediate is followed and served under an alias spelling → RED (needs Developer Mode or a cloud placeholder on the device; absent both, this second mutant is pre-declared **rule not exercised** and the receipt says so). One fault each, one deciding rule. The source test exempts exactly one open by path: root admission in `validate_windows_dev_stage_acl`. |
| AC05 One parser; refusal table (§4.4) | Pure goldens in `keld-guard` for every table row plus the probe's 31-cell corpus | Each vector yields the table's refusal or components. The `:` and `~` cells are decided **only** by R8 (`:` is removed from R5 so one rule owns it); the W1 Source rule and the W3 handler call this one function (one definition, two call sites). | Drop R8's `:` refusal → `index.html:hidden` opens (NTFS parses the stream at the NT layer) and the planted ADS nonce is served → RED. Separately drop R8's `~` refusal → a planted `LONGNA~1.JS` spelling serves the long-named file under a second identity → RED (volume with 8.3 disabled → **rule not exercised**, receipt says so). Separately decode twice → `%252e%252e` yields `..` → R6 refuses anyway: **non-discriminating**, listed, not counted. |
| AC06 Served-document correlation (L2) | A top-level navigation `n` to an in-root URL is allowed at `NavigationStarting`; (a) the file exists; (b) the file is absent; (c) an `<iframe>` to a **different** in-root URL loads concurrently with a top-level navigation to an **absent** file | (a) The WRR log holds one `ResourceContext == DOCUMENT` response for `n`'s URI served from `(root, components)`; `ContentLoading(n)` reports `IsErrorPage == FALSE`; the Host reports the document as Host-served with that identity; **page oracle**: `location.href` equals `n`'s URL and the body carries the fixture nonce. (b) The Host served `404`/empty for `n`; `ContentLoading(n)` is **not** reported Host-served (W1's epoch holder posts no epoch; page oracle: no fixture nonce); AC09 fires `did-fail-load(-6)`. (c) the frame's served record never attaches to `n`; `n` stays not-Host-served. | Attach by `ResourceContext == DOCUMENT` only, ignoring URI equality → case (c) attaches the frame's record to `n` and an absent top-level document is reported Host-served (W1 would mint an epoch; page oracle shows no nonce) → RED. Pre-declared **rule not exercised** conditions: if W1's eligibility (B7) is not implemented on the device, the epoch-holder counter is absent and only the page oracle counts; if Q7 shows `IsErrorPage == TRUE` for a Host `404`, case (b)'s `IsErrorPage` clause is redundant with the served record and its NC must instead flip the served-record rule. |
| AC07 Signal emission and order (§4.6; Electron v45.1.0 oracle) | (i) In-root load; (ii) off-allowlist page-initiated navigation; (iii) hash change and `pushState`; (iv) absent in-root file; (v) Host-initiated load | The signal log is the system under test; every sequence is **compared to the GH-551 Electron v45.1.0 run of the same fixture** (page oracle in both), not asserted from Host records alone. (i) `did-start-loading`, `did-start-navigation`, `did-navigate`, `dom-ready`, `did-finish-load`, `did-stop-loading`, same `NavigationId`, generation stamped. (ii) `did-start-loading`, `did-start-navigation`, `will-navigate` (notification, §4.6), `did-stop-loading`; **no `did-fail-load`** (Electron suppresses `ERR_ABORTED`), no `did-navigate`, no `dom-ready`; the application document survives (W1 AC10). (iii) `did-navigate-in-page` only; no `will-navigate`; generation unchanged. (iv) `did-fail-load(-6)` exactly once, no `did-finish-load`. (v) no `will-navigate`. Nothing is synthesized from a Bun-side timer. | Emit `did-finish-load` from `ContentLoading` → `did-finish-load` before `dom-ready` → RED. Separately emit `did-fail-load(-3)` on a policy cancel → divergence from the oracle (Electron L2932–2933) → RED. |
| AC08 `will-navigate` (§4.6) | A page-initiated `NEW_DOCUMENT` navigation to (a) an off-allowlist URL, (b) an on-allowlist URL; (c) a `BACK_OR_FORWARD`/`RELOAD` navigation; (d) a redirect hop (`IsRedirected == TRUE`) of a Host attempt; (e) a Host attempt itself | (a) cancelled on the original `NavigationStarting` (+0 generation), `will-navigate(url)` emitted when a listener exists (the `shell.openExternal` pattern keeps working); (b) allowed, `will-navigate(url)` emitted when a listener exists, `preventDefault()` has no effect (documented divergence; `keld migrate` derives the allowlist so this case is empty for a veto-all listener); (c) allowlist evaluated, **no** `will-navigate` until P3 settles whether Electron emits it for script history navigations; (d)/(e) never attributed page-initiated (attribution by the attempt's `NavigationId`); no `will-navigate`. (f) **in-root is not enough and the query counts**: a veto-all fixture (`web.navigation.allow` derived to `none`) whose page contains an in-root link (`other.html`), a link to `index.html?attacker=1` and a link to the entry's exact URL — all three cancelled, page nonce unchanged, a granted IPC call still succeeds afterwards. (g) a Host attempt (`loadURL`) to an off-allowlist target → typed refusal at +0, document and IPC untouched; (h) a Host attempt to `index.html?x` (entry components, any query) and an `onLoss` reload-entry under the same `none` allowlist → both admitted. Generation: +0 on cancel, +1 only when an allowed navigation commits. | (a) Attribute "page-initiated iff `IsUserInitiated == TRUE`" → the boot `Navigate` (API navigations are user-initiated per Microsoft) emits `will-navigate` while a scripted `location.href` (`FALSE`) emits none — against the GH-551 oracle (boot emits nothing; scripted emits) → mismatch in both directions → RED; at-risk 1 each per run. (b) Advance the generation at `NavigationStarting` → an off-allowlist click revokes the live document → a page-side IPC round trip fails → RED (W1 AC10). (c) Treat every in-root `file:` target as on-allowlist → (f)'s links replace the document → RED. (d) **Cut the query in the `file:` allow match** → under a lifted literal `index.html` (no-query constraint) the `index.html?attacker=1` link replaces the document → RED. (e) Check the allowlist after the Host attempt's +1 → (g) leaves the window FailedClosed → IPC fails → RED. (f′) Drop the entry exemption for Host attempts → (h)'s `onLoss` reload is refused and the window never recovers → RED. |
| AC09 `did-fail-load` honesty | (i) Absent in-root entry (Host `404`); (ii) parser-accepted, opener-refused entry (Host `403`: directory, reparse); (iii) policy cancel; (iv) renderer error (`NavigationCompleted` failure, other `WebErrorStatus`) | (i) `-6 ERR_FILE_NOT_FOUND`; (ii) `-10 ERR_ACCESS_DENIED` (documented divergence: Electron would list a directory or follow the link); (iii) **nothing**; (iv) the §4.6 table's code; exactly one `did-fail-load` per failed `NavigationId`; `did-stop-loading` follows. Verified against the GH-551 differential oracle (page oracle + Electron run), not against Host records alone. | Map every failure to `-3` → case (i) reports `-3` while the Host served `404` → RED. |
| AC10 Settled eval (E1, E4) | After `did-stop-loading`: (a) `new Promise(r => setTimeout(() => r("v"), 500))`; (b) a `try { … } catch (e) { null }` statement with a string completion value; (c) `Promise.reject(new Error("m"))`; (d) `throw new Error("t")`; (e) under page CSP `script-src 'self'`, an expression whose value is an object with a getter that calls `eval` and sets a marker; (f) same, with a `Proxy` whose `get` trap calls `eval` and sets a marker; (g) the page has patched `Object.getOwnPropertyDescriptor` (and `Reflect.ownKeys`) to call `eval` and set a marker, then returns a plain object; (h) under `script-src 'self'`, the **evaluated code itself** calls `eval("1")` | (a) `"v"` after ~500 ms (probe 3/3); (b) the completion value; (c) rejection carrying `m`; (d) the facade's fixed sync-throw message (#1154 shape, §10 Q3); (e) the getter runs **under the page's own CSP** (deep serialization reads own enumerable properties through `Get` inside `evaluate`, V8 L222/L257): its `eval` throws, marker **not** set, and the throw surfaces as CDP `-32000 "exception during deep serialization"` → typed `EvalFailed` rejection to the caller; (f) `{type:"proxy"}` is reported and the trap never runs (V8 deep serializer L339–341): marker not set — matches Electron's `DataCloneError` class; (g) marker **not** set under (v) — no codegen scope exists; (h) the page's `eval` is refused by its CSP (flag false, V8 L400–402): the rejection names the CSP. | Use `ExecuteScript` (probe N4) → (a) yields `{}` → RED. Separately pass `allowUnsafeEvalBlockedByCSP` default → (h) succeeds → RED. Separately transport through (ii) `callFunctionOn` + encoder → (g)'s marker is set (codegen scope open across the encoder's intrinsic calls, V8 L151/L158/L188) → RED. |
| AC11 Generation-scoped delivery (E2) | (i) Eval dispatched at generation G; replacement before completion; (ii) **real replay control** (#1185's own NC, UNMET by the probe — N6 compared constants): a debug `profile-test-hooks` seam holds the **actual CDP completion payload** of correlation `c` dispatched in document G1, the harness commits G2, then the held payload is delivered to the completion handler as if CDP had just completed; (iii) role G1 awaits an eval, recovers to G2 before completion; (iv) the document enters BFCache or never completes the script | One state machine (`Pending → Retired \| Delivered`, §4.7 step 4), counted in the **Bun facade** (another process): settle frames per correlation ≤ 1, and no payload produced in G₁ observed after G₂ committed — the second assertion is Host-independent: the evaluated script returns a **page-side document nonce** (planted in each fixture document) and the facade compares it with the page oracle's nonce for the committed document, not with any Host stamp. (i) exactly one `DocumentReplaced` frame; CDP `-32000` (probe 3/3) or retirement. (ii) the held G₁ payload produces **no** frame (the entry is `Retired`); the caller observed exactly one frame, the retirement's; at-risk 1 per run. (iii) retired by KEL-75/KEL-143's role retirement (the role axis's existing owner), never delivered to G₂. (iv) one `DocumentReplaced` frame when G changes, in the same STA turn; the entry is freed when its CDP callback fires. | Single faults, each on the one machine: (1) the retire step rejects but leaves the entry `Pending` → the held G₁ payload is delivered → facade sees 2 frames for `c`, the second a G₁ value → RED. (2) the completion handler delivers regardless of state → same observable → RED. (3) the retire step posts no rejection → (iv) hangs the caller (no frame within the test bound) → RED. (4) for (iii): skip the role retirement hook → the G₂ role observes a G₁ frame → RED (pre-declared **rule not exercised** until Q16 confirms that KEL-75/KEL-143's retirement covers eval correlations; if it does not, the hook becomes W3's and the NC counts). |
| AC12 Context binding at dispatch (E4) | `executeJavaScript` issued (a) while the main frame's execution context is known; (b) in the gap between `Runtime.executionContextDestroyed` and the next `executionContextCreated` for the main frame | (a) the call carries the `uniqueContextId` **bound to G at `ContentLoading`** (§4.7 step 1); only `uniqueContextId not found` / `-32000` map to `DocumentReplaced`; (b) typed `NoDocumentContext` returned to the facade, which re-awaits `did-stop-loading` and re-dispatches; the application sees one settled result in the new document, never a rejection, and **no script ran in the gap**. Non-entry and remote documents are evaluable; only generation equality decides delivery. (c) `executionContextDestroyed`/`executionContextsCleared` for the bound context unbinds the pair (§4.7 step 1), so (b)'s gap exists. Deterministic hook for NC1: a debug hook **holds `ContentLoading` and `executionContextDestroyed` in the Host's queue** while the engine already shows the new document, so the Host still holds `(G₁, ctx₁)`; the harness dispatches during the hold. Side-effect oracle: the evaluated script writes a marker into `document.title`; the harness reads the title of **both** documents through CDP. | NC1: drop **only** the `uniqueContextId` parameter → during the hold the eval runs in the **new** document (its title carries the marker) and is then rejected `DocumentReplaced` → RED (side effect without delivery); with the parameter the same dispatch returns `uniqueContextId not found` → `DocumentReplaced`, no marker. NC2: drop **only** the unbind on `Destroyed`/`Cleared` (parameter kept) → the stale id yields `DocumentReplaced` where Electron would wait → the application sees a rejection → RED against the oracle. |
| AC13 CDP capability self-test (E5) | Webview create, before the first `Navigate` | Event receivers registered **before** `Runtime.enable`; `Runtime.enable` succeeds; `Runtime.evaluate("1+1", returnByValue)` → `2`; `Runtime.evaluate("Promise.resolve(3)", awaitPromise)` → `3`; one `evaluate` with `serializationOptions: {serialization:"deep"}` returns a `deepSerializedValue`; each within `WEBVIEW2_STARTUP_BOUND`; the self-test does **not** wait for the initial `executionContextCreated` (its arrival before the first `Navigate` is UNVERIFIED; the probe registered receivers after enabling, `main.rs` L1090–1119). Failure → the application does not start, typed `KELD-WV-*`. **Not claimed**: that `allowUnsafeEvalBlockedByCSP:false` is honored (crdtp L124–128; per-runtime cell AC10 h; §10 Q11). Enterprise CDP policy: UNKNOWN (Q11). | Skip the self-test with an injected CDP failure → the framework counter **"application started" = 1** although every later eval rejects → RED (the counter is Keld's own `NavigationReady`/app-started milestone, not any application's behaviour). Separately drop the deep-serialization probe → a runtime without the option starts and every object result arrives as a `RemoteObject` stub → RED. |
| AC14 Eval value domain and bound (E3) | A result inside the codec owner's admitted domain and at the bound; one byte over | Delivered bit-exact / typed `CodecLimit` rejection, never a hang. **Blocked on §10 B3.** | Deliver truncated → RED. |
| AC15 App-declared response headers (S1) | (a) A compat application registers `onHeadersReceived` inside `app.whenReady()` and then creates its window and calls `loadURL(entry)` (the Electron-correct order; the window's **first** document is that `loadURL`); (b) no listener; (c) the listener's role is confirmed gone; (d) the role is recovering (between the `RoleGeneration` move and the successor's re-registration) and the predecessor had a listener; (d′) same, predecessor had none | (a) one Host→Bun round trip per `file:` response **including the window's first DOCUMENT response** — the registration and the `loadURL` are two commands on the one FIFO, and no Host navigation precedes them (compat boots create no window at boot: `InitialWindow::None`, `app_session.rs:2383, 3431`; W2 "zero windows at Ready"); the served headers are the listener's (Host-owned fields excepted); (b) zero round trips; (c) `403`/empty; (d) requests are **held** behind their deferrals until the successor registers (or declares none), then served; bounded by the recovery deadline and a count, after which `403`/empty; (d′) served with the baseline only (no listener ⇒ no round trip, as before the loss). At-risk for (a) = the first DOCUMENT response per run (non-empty by construction). | (a) Process the `loadURL` command before the earlier registration command (FIFO broken) → the first document is served with the baseline CSP only and the app's inline-script policy is not applied (probe N5 class) → RED. (c) Serve on role loss with baseline-only headers → RED. (d) Answer `403` instead of holding → a recoverable window shows a blank document after every Bun restart → RED. |
| AC16 Baseline CSP cannot be relaxed | The listener returns a permissive CSP, or overrides `Content-Type` to `image/svg+xml`/`application/xhtml+xml` | The baseline is emitted as its own header on **every `DOCUMENT`-context response** regardless of `Content-Type`; a remote script the baseline forbids stays blocked; in-root scripts load (baseline grammar under `file://` is P4, §10 Q5). | Emit the baseline only for `text/html` → an SVG document served with the app's permissive CSP runs the forbidden script → RED. |
| AC17 Hardlink authority read-back (C4) | The fixture plants ≥ 1 in-root file with `nlink > 1` whose other name is outside the root | (a) served (AC03); (b) for every served file with `nlink > 1`: a write attempt on the linked object under **each admitted role token** (Strict; the Legacy forfeiture printed instead of claimed) is denied (KEL-254 AC8 class) and the per-file DACL read-back shows no role write grant (AC7 extension); (c) the owner's paired control: the Strict role attempts to **create an entry inside the admitted root** and is refused (KEL-254 app-role isolation on Windows). At-risk = multi-link served files; 0 → rule not exercised (fixture MUST plant one). | Grant the Strict role write on the linked object → read-back detects, the control writes → RED. Separately grant it create on the root → (c) succeeds → RED. |
| AC18 Zero bytes on refusal (C5) | AC03's refused cells, elevated ETW Kernel-File session | No open of the outside sentinel by `msedgewebview2.exe`; for parser cells the Host opener count is 0; for opener cells bytes read is 0 (the opener returns a handle to the reparse point itself and reads nothing). ETW NOT RUN → INFERRED-strength, printed. | AC03's NC → ETW shows the renderer's own open → RED. |
| AC19 Non-`file:` requests never reach W3 | The page fetches `https://` (policy-allowed) and loads `data:`/`blob:` | **Structural**: the `file:*` filter never raises WRR for them — new probe cell: `https` request with WRR event count 0 and Host response count 0; the policy owner's rules apply unchanged. | Register `*` instead of `file:*` → the `https` request raises WRR and sits behind the UI thread → RED (count ≠ 0). |
| AC20 P1 performance gate (§9) | Before T2 merges | The P1 report exists with the arms of §9 and the six atoms separated, 30 valid samples per arm per class, chosen arm's first-paint median ≤ 1.05 × arm (e) or a written waiver. | Single arm, no arm (e), or paint measured from `NavigationCompleted` → validator RED. |
| AC21 Receipt | A Windows acceptance claim | Runtime string, OS build, head, admitted-root mode, per-AC counters, at-risk counts per deciding rule, each RED output, "rule not exercised" rows, UNKNOWN cells (§3 AC23). | Missing fields or a zero-failure row without its at-risk count → validator RED. |
| AC22 Non-discriminating controls recorded, not counted | Probe N1 (only a dedicated worker was attempted; shared/service workers untested), N3, N6 (compared constants; no CDP result replayed), the exclusive-lock oracle (empty at-risk set); AC05's double-decode mutant | Each reads **rule not exercised** and contributes nothing to scores; N6's claim is replaced by AC11 (ii). | Count any of them → validator RED. |
| AC23 Coverage cells with UNKNOWN status | favicon (probe `r-favicon` timed out), `<link rel=prefetch>`, `navigator.sendBeacon`, web app manifest, media `Range` (probe media cells used filler bytes; `mediaError 4` proves nothing), `blob:`/`data:` dedicated workers (INFERRED to run under `file://`: `fetch` and `importScripts` of the outside sentinel from inside one), Shared/Service Worker construction attempts, `<embed>`/`<object>` PDF, `<a download>`, `<script type=module crossorigin>` and dynamic `import()` (default Vite/electron-vite output), and the environment worker filter's survival when the first webview closes (the Host re-registers it on the next live webview; cell: close the first window, open a second, attempt a shared worker) | Each is run as its own cell on the real device and reported served/refused/UNKNOWN with the request kind observed; none is claimed from the probe. | Claim any of them from the probe → validator RED. |

## 4. Design

### 4.1 Atomic decomposition (decision input)

Evidence classes: FACT, PROBE (run ids `baseline r1–r3`, `n1–n6`), INFERRED, UNKNOWN (named
probe). Security atoms separate identity, authentication, authorization, OS containment,
lifecycle/revocation and evidence provenance; performance atoms are in §9.

| Atom | Owner / boundary | Inputs → outputs | Failure mode | Observable | Evidence | Edges · first falsifier |
| --- | --- | --- | --- | --- | --- | --- |
| A1 Loading path | `keld-core` boot → `keld-wv` `navigate` | admitted root + entry components → `Navigate(file URL)` | string load yields `about:blank` identity | AC01 | FACT W1 source probe; code `:3653`/`:3654` | · AC01 NC |
| A2 Request census and filter scope (coverage, cost) | WebView2 filter normalization + `keld-wv` registration | `file:*` + `CONTEXT_ALL` + `DOCUMENT` per webview, before first `Navigate`; worker kinds once per environment → every `file://` request raises WRR, no other scheme does | late registration; `*` filter puts every remote request on the UI thread | AC02, AC19 | PROBE n2 (under `*`; second document only per note 268); FACT L1307 normalization, `_22` L51/L72 | precondition of A8 · n2 under `file:*`, both documents |
| A3 Admitted root (identity of the root) | `keld-core` boot (dev DACL; KEL-254 selection) | admission → one retained `Dir` | handle dropped and reopened by path | AC04 | FACT dev handle dropped today; `tree: Dir` retained | → A5 · junction swapped in after admission is followed |
| A4 URL → components (authentication of the name) | `keld-guard` parser | URI + `AdmittedRootUrl` → components or refusal | double decode; authority; ADS; `~`; Win32 spellings | AC05 | PROBE parser cells; FACT grammar; FACT Chromium pre-canonicalises `%2e%2e` | independent of A5 for `:`/`~`; redundant for `..`/trailing dot · AC05 NC |
| A5 Open (OS containment) | `keld-update` `open_regular_beneath`, **per-component** cap-std no-follow walk (normative) | `&Dir` + components → regular-file handle + `nlink` | reparse followed (multi-component `open_with` follows intermediate relative symlinks); directory; Win32 name layer | AC03 opener cells, AC04 | FACT cap-primitives relative `NtCreateFile` + `FILE_OPEN_REPARSE_POINT`; PROBE `gh1185-capstd-junction-2026-10-30` (3/3 identical, keld-research note 268): cap-std 4.0.3 refuses an intermediate junction in every form (multi-component path and default `open_dir`: "a path led outside of the filesystem", for out-of-root and in-root targets alike; `open_dir_nofollow`: os error 681), while the ambient `std::fs` negative control reads the out-of-root sentinel through the same junction; directory/file symlinks UNTESTED (creation needs privilege) | ← A3 · AC04 NC |
| A6 Hardlink authority (authorization) | KEL-254; W3 reads back | `nlink` + DACL → served + receipt | namespace write taken as the only authority | AC17 | FACT owner decision + correction | · role write grant |
| A7 Response/refusal contract + MIME table | `keld-wv` handler | file or refusal → `200`/`404`/`403`; `Content-Type` from one `keld-wv` table | Chromium fallback; body on refusal; app override vs baseline | AC03, AC09, AC16, AC18 | PROBE 403/empty | → A16 (baseline keyed on context, not type) · AC03 NC |
| A8 Served-document identity | `keld-wv` per-navigation record for **every** top-level navigation (all schemes) | allowed `NavigationStarting(n)` + DOCUMENT WRR with equal parsed URI → `ServedDocument` | URL text as identity; frame record attached to `n` | AC06 | INFERRED timing; UNKNOWN `IsErrorPage` for Host `404` (Q7) | ← A2; → A12 (gate sees all navigations) · AC06 NC |
| A9 Navigation events → facade | `keld-wv` → `AppWindowEvent` → facade | WebView2 events → Electron emission rules | fabricated/reordered; `did-fail-load` on cancel | AC07, AC09 | FACT Electron v45.1.0 L2745–2752, L2872, L2932–2933 | ← A8 · AC07 NC |
| A10 `will-navigate` policy and attribution | `keld-wv` (allowlist on the original event) + facade (notification) + `keld migrate` (allowlist derivation) | `NavigationStarting` → allow/cancel (+0) and notification | allowlist after +1 (revokes live document, W1 AC10); `IsUserInitiated` as initiator; redirect hop of a Host attempt cancelled | AC08 | FACT: no deferral; FACT #1192 L108 (+0 on cancel); UNKNOWN history-kind emission (P3) | edge to #1192 L108–112 and W1 AC10 made explicit: W3 performs no Host load for page navigations · AC08 NC |
| A11 Eval mechanism | `keld-wv` CDP | script → settled value/rejection/destroyed | `ExecuteScript` targeting; codegen scope | AC10 | PROBE n4; FACT V8 L151, L400–402 | · n4 |
| A12 Eval lifecycle/revocation | one `keld-wv` state machine (`Pending → Retired \| Delivered`) over #1192's generation with the main-frame context bound at `ContentLoading`; role axis = KEL-75/KEL-143's retirement | `(G, ctx)` at dispatch → deliver iff the entry is still `Pending` when CDP completes; retire in the STA turn that advances G | stale delivery; eval in the gap runs in the new document; BFCache never completes | AC11, AC12 | PROBE r1–r3 (`-32000` on replacement); **no probe evidence for the replay rule** (N6 compared constants — note 268); FACT CDP out-of-order (L1333) | ← A8 (all schemes) · AC11 (ii) NC; AC12 NC |
| A13 Eval capability (fail loud) | create-time self-test | CDP calls → start / typed refusal | silent per-call rejection; unknown-field acceptance mistaken for honoring | AC13 | FACT crdtp L124–128 | · AC13 NC |
| A14 Eval value domain | #1190 owner (decoder of the deep-serialized form; domain losses) | `deepSerializedValue` bytes ≤ bound → decoded domain | 4096 bound; getters run and symbol/non-enumerable keys dropped (V4 not met); remote objects leaked without `releaseObjectGroup` | AC14, AC10 (e)–(g) | FACT D4; FACT V8 L222/L257, L339–341; FACT `injected-script.cc` L640–648 | B3/Q15 · AC14 NC |
| A15 App response headers (S1) | facade listener + `keld-wv` round trip | registered listener → modified headers | pass-through on retirement; second URL matcher | AC15 | FACT D3 (source level) | → A16, A19 · AC15 NC |
| A16 Baseline CSP | `web.csp` owner for content; W3 for emission | policy → own header on every DOCUMENT response | merged header; type-keyed emission | AC16 | FACT CSP3; UNKNOWN `'self'` under `file://` (P4) | ← A7 · AC16 NC |
| A17 Scheme boundary | #541/#460 | non-`file:` URI → never raised | `*` filter | AC19 | FACT L1307 | · AC19 NC |
| A18 Evidence provenance | spec + receipts | hashed runs → claims | second runtime; zero-failure without at-risk; probe cells misattributed | AC21–AC23 | FACT one runtime | · validator |
| A19 Registration ordering | `keld-core` control FIFO (compat windows have no Host-navigated first document) + recovery hold | facade registrations sent before `loadURL` apply to that load's first request; during recovery `file:` requests are held until the successor registers (bounded; only if the predecessor had a listener) | FIFO broken; a Host-navigated first document before `ready` | AC15 (a), (d) | FACT: schema-2 boots create no window (`app_session.rs:2383, 3431`); FACT: Electron apps register after `ready` (draw.io `electron.js` L987–994) | → A15 · AC15 NC (FIFO order swap) |

### 4.2 Decisions taken as input (kept; evidence re-checked)

| Decision | Statement | Evidence this spec re-checked |
| --- | --- | --- |
| L1 | Load with `Navigate(file:///<admitted root>/<entry>)`; `NavigateToString` retired for application documents. Closes W1 B6 jointly with #1192/KEL-102. | W1 AC18 (3/3). |
| C2 | The Host serves every `file://` byte through WRR registered before the first `Navigate`; no virtual host, custom scheme or veto-then-Chromium-loads. **Filter scope corrected (r2):** `file:*`, never `*` (§4.5). | Parser differential and TOCTOU (feasibility research); L957/L1307. |
| C4 | In-root hardlinks served; `nlink > 1` selects per-file DACL read-back and write-denial per role token; Legacy forfeits root integrity (printed). | 6491936458 + 6492237806. |
| E1/E5 | CDP `Runtime.evaluate` with `awaitPromise`, `allowUnsafeEvalBlockedByCSP:false`, **mandatory** context binding, generation authority as the one `Pending → Retired \| Delivered` machine (captured at dispatch; retired in the turn G changes; delivered only while `Pending`); create-time self-test fails closed. | D2; probe r1–r3 (settle, `-32000`), n4; V8 L400–402. The replay rule has **no** probe evidence (N6 was not a control, note 268) and is proven only by AC11 (ii). |
| S1 | Per-request round trip to Bun only when an `onHeadersReceived` listener exists; fail closed; `Complete` on the UI thread; separate Keld baseline CSP. **Matcher ownership fixed (r2):** the Electron `filter.urls` match-pattern matcher lives once, in the facade (§4.5). | D3. |
| Owners | One generation owner (#1192), one codec owner (#1190), two name owners (§2). | §2. |

No input decision was contradicted by primary evidence. r1's own derived decisions that
the review falsified are replaced in §4.3 (opener), §4.5 (filter, headers), §4.6
(`will-navigate`, `did-fail-load`) and §4.7 (context binding) with the evidence beside each.

### 4.3 C3 — the one Windows "open a regular file beneath the admitted root" owner

Problem unchanged: page-controlled names make the Win32 name layer unsafe; three openers
exist. r1 chose a new `NtCreateFile` read-open in `keld-update` (new `unsafe`). The review
showed that `keld-update` already opens leaves beneath its retained `tree: Dir` through
cap-std with `FollowSymlinks::No` (`open_pinned_leaf`), and that cap-primitives' Windows
walk is per-component and relative (`NtCreateFile` with `RootDirectory`), not path-based.
Re-scored (0–10, confidence):

| Option | Correctness vs page-controlled names | One owner / DRY | Boundary shape | Gate cost | Perf | Mean |
| --- | --- | --- | --- | --- | --- | --- |
| **(g) cap-std per-component no-follow walk under the existing `tree: Dir` (generalised `open_pinned_leaf`), post-open checks, `nlink` via cap-fs-ext** | 8 (0.85): relative `NtCreateFile` per component with `FILE_OPEN_REPARSE_POINT`; intermediate junctions refused in every walk form, 3/3 with a live-junction negative control (probe `gh1185-capstd-junction-2026-10-30`); symlinks UNTESTED | 9 (0.9): extends the opener `keld-update` already uses; deletes the Win32 walker; `open_pinned_leaf` becomes a call | 8 (0.8): `keld-wv` holds no policy; `keld-core` already depends on `keld-update` | 9 (0.9): **no new production `unsafe`**; public API only | 7 (0.7): one open per component (depth ≤ 3 in the pinned app; measured in P1) | **8.2** |
| (a) r1: new `NtCreateFile` multi-component read-open in `windows_fs.rs` | 9 (0.9) | 7 (0.8): a second relative opener beside cap-std's in the same crate | 8 (0.8) | 6 (0.9): new `unsafe` + AGENTS sanction | 8 (0.7) | 7.6 |
| (b) promote into `keld-guard` | 9 | 5 | 5 | 4 | 8 | 6.2 |
| (c) `keld-wv`-owned opener | 9 | 3 | 4 | 5 | 8 | 5.8 |
| (d) `keld-native` KEL-130 broker | 6 (0.7) | 4 | 4 | 6 | **0** (0.95): 8 MiB cap < 9,865,843 B | 4.0 |
| (e) extend `open_relative_file_windows` | 2 | 6 | 2 | 8 | 7 | 5.0 |
| (f) new crate | 9 | 5 | 7 | 3 | 8 | 6.4 |

**Decision: (g).** Reuse before rewrite holds. The named requirement (g) had to prove, the
intermediate-junction refusal, is now probe evidence (`gh1185-capstd-junction-2026-10-30`:
cells C1–C5 refused, negative control N1 reads the out-of-root sentinel through the same
junction with `std::fs`, 3/3 identical); it stays an AC03 regression cell. **The
per-component walk is normative**: each directory step is `open_dir_nofollow`, the leaf is a
single-component `open_with(.., FollowSymlinks::No)`, and after every step the handle's
attributes are read back. A multi-component `open_with` is **not** used: cap-primitives'
manual resolver would follow a relative in-root symlink at an intermediate component
(`manually/open.rs:369-382` only stops on the last component), which would serve alias
spellings and give one object two identities for W1/#1192. For symlinks the refusal is a
source fact, not a device fact: with following off and no `FILE_FLAG_OPEN_REPARSE_POINT`,
cap-primitives returns `OpenUncheckedError::Symlink` for any name-surrogate reparse point
(`open_unchecked.rs:162-190`, `metadata.file_type().is_symlink()`); the directory- and
file-symlink cells remain device cells (creation needs privilege) and the probe's
`sym-fetch` row was object-not-found, not a refusal. The mixed-case cell is also re-run
under (g): the probe's opener passed `OBJ_CASE_INSENSITIVE` explicitly, cap-primitives sets
it only under `FILE_FLAG_POSIX_SEMANTICS` (`create_file_at_w.rs:148-149`) and otherwise
relies on the kernel default (`ObCaseInsensitive`). Concretely:

```rust
// crates/keld-update/src/windows_baseline.rs (pub, Windows-only; no new unsafe)
pub struct OpenedRegularFile { pub file: cap_std::fs::File, pub size: u64, pub nlink: u64 }
pub enum OpenRefusal { Absent, Reparse, NotRegular, Other(io::Error) } // → 404 / 403 / 403 / 403
/// Walks `components` beneath `root` ONE COMPONENT AT A TIME: every directory step is
/// `open_dir_nofollow(component)`, the leaf is a single-component
/// `open_with(component, read, share, FollowSymlinks::No)`; after each step the handle's
/// metadata must show neither FILE_ATTRIBUTE_REPARSE_POINT (any tag) nor, for the leaf, a
/// directory. Never a multi-component `open_with` (cap-std would follow an intermediate
/// relative symlink). A leaf junction surfaces as AccessDenied (probe C6, raw 5), a symlink as
/// cap-std's Symlink error; both map to `Reparse` → 403.
/// `share` is `Pinned` (FILE_SHARE_READ; the existing open_pinned_leaf contract) or
/// `Served` (READ | WRITE | DELETE: the Host never blocks the root's mutation owner).
pub fn open_regular_beneath(root: &Dir, components: &keld_guard::RootRelativeComponents, share: Share)
    -> Result<OpenedRegularFile, OpenRefusal>;
impl ActivePackageSelection { pub fn tree(&self) -> &Dir { &self.tree } } // lends; no reopen
```

- `open_pinned_leaf(parent, leaf)` becomes `open_regular_beneath(parent,
  &RootRelativeComponents::single(leaf), Share::Pinned)`. Its direct callers (`locate.rs:311`,
  `locate.rs:375`, `windows_baseline.rs:606` inside `open_machine_file`, itself called from
  `helper.rs:207`, `load.rs:481`, `load.rs:1075`, `load/version_contents.rs:58`,
  `windows_baseline.rs:668`) compile against it unchanged in behaviour (one PR, AC04).
  "Exactly one opener" is scoped to **app-code-root opens**; KEL-130's broker remains the
  opener for retained-filesystem grants (`$RESOURCES` and the like) and is not touched.
- **Reparse policy**: any reparse attribute at any component refuses — the rule
  `keld-native` applies to an acquired handle (`ensure_acquired_not_reparse`,
  `fs.rs:1331-1349`; its `unsupported_reparse` at `:1352-1357` is the narrower
  non-symlink form) and the effect `OBJ_DONT_REPARSE` has. Stated consequence: an app root
  under a OneDrive/Files-On-Demand tree, a deduplicated or WOF-compressed path is refused
  at boot (dev stage) or by KEL-254's own checks (installed); a cloud tag that appears
  **mid-session** (Known Folder Move dehydrating a dev stage) turns later requests into
  `403` — `keld dev` admission therefore refuses a stage inside a sync root and prints the
  component (§10 Q6).
- **`keld-core` boot** retains the admitted root as a `Dir` (dev stage: `Dir::from_std_file`
  of the handle `validate_windows_dev_stage_acl` opens — the one remaining open by path,
  which AC04's source test exempts as root admission; installed: `active.tree()` with the
  `Arc<ActivePackageSelection>` itself stored in the `AppResourceSource` so the borrowed
  `&Dir` has an owner for the session — no `try_clone`, no new handle), migrates the four
  boot opens onto `open_regular_beneath` (`Share::Pinned` for installed trees;
  `Share::Served` for the dev stage so an in-place entry rewrite by `keld dev` is not
  blocked for the session), deletes `open_relative_file_windows`, and hands
  `AppResourceRoot { source, url: AdmittedRootUrl }` to `keld-wv`. The root URL is spelled
  from the handle's final long path, and the Bun role is launched with that same spelling
  as its project root, so `__dirname` and the Host agree even under junctioned or
  `subst`-ed ancestors; UNC/network roots are refused at **admission** by the `AdmittedRootUrl`
  constructor (it requires a drive letter; R3 governs request URLs) and therefore cannot be admitted
  app roots. KEL-254's "single Windows path owner" (`kel254` L187, L200, L237) keeps naming
  `keld-core` boot as the policy owner; the mechanism is `keld-update`'s.
- **`keld-wv`** holds no policy:

```rust
// crates/keld-wv/src/engine.rs (public API gate; live backend: webview2)
pub struct OpenedResource { pub file: std::fs::File, pub size: u64 }
pub enum ResourceRefusal { Absent, Reparse, NotRegular, Other(io::Error) }
pub trait AppResourceSource: Send + Sync {
    fn root(&self) -> &keld_guard::AdmittedRootUrl;
    fn open(&self, components: &keld_guard::RootRelativeComponents) -> Result<OpenedResource, ResourceRefusal>;
}
pub struct WebviewSpec { pub title: String, pub size: LogicalSize, pub initial: NavTarget,
                         pub app_resources: Option<Arc<dyn AppResourceSource>> }
```

Gates: **public API** (`open_regular_beneath`, `OpenedRegularFile`, `OpenRefusal`, `Share`,
`tree()`, `WebEngine`/`WebviewSpec`, `AppResourceSource`); **permission model** (session-
retained root handle; KEL-254 AC7/AC8 extension). **unsafe: none** for the opener.

### 4.4 URL → root-relative components, the refusal contract, and document correlation

**One parser, one owner (decision 1).** In `keld-guard`:

```rust
pub struct AdmittedRootUrl { drive: u8, components: Vec<String> } // validate_fs_component per component; spelled from the handle's final long path
pub struct RootRelativeComponents(Vec<String>);                    // 1..=N; each validate_windows_package_component + NFC
pub enum FileUrlRefusal { NotFileScheme, Authority, NoDrive, DevicePrefix, ForbiddenEscape,
                          DecodedSeparator, DotSegment, OutsideRoot, Component(String), TooLong }
pub fn file_url_root_relative(url: &str, root: &AdmittedRootUrl) -> Result<RootRelativeComponents, FileUrlRefusal>;
```

It is pure except for the two already-sanctioned NLS calls relocated from
`validate_windows_package_paths` (R7, R8 NFC); R1–R6 and R9 are tested on every OS, R7/R8
goldens run on Windows. The root's own spelling is **Host-admitted**, never page-controlled:
it is derived once from the retained handle's final long path (`GetFinalPathNameByHandle`
`FILE_NAME_NORMALIZED`, as the existing `normalized_guid_path` in `windows_fs.rs:151` does)
and spelled as Bun's `__dirname` spells it, so a project under `C:\Users\John Doe\` or a
`RUNNER~1` profile boots. Case-sensitive directories (`FILE_CASE_SENSITIVE_INFO`, common for
WSL-created trees) are **unsupported app roots**: R7's ordinal-ignore-case class would admit
a spelling the filesystem treats as another object; the open then fails `404`, so
containment holds but identity does not — boot admission refuses such a root (§10 Q12).

Refusal table (first match decides; query and fragment are cut before R3 and never form
identity — W1 AC20):

| Rule | Input | Result | Why / evidence |
| --- | --- | --- | --- |
| R1 | scheme ≠ `file` (ASCII case-insensitive) | `NotFileScheme` → not W3 | never raised by the `file:*` filter anyway (§4.5) |
| R2 | any non-empty authority | `Authority` → 403 | Chromium strips `localhost` and maps other hosts to UNC, so the `localhost` spelling is unreachable at WRR (NIT accepted) and other hosts arrive as UNC forms |
| R3 | path not of the form `/<L>:/…`; or `//?/`, `//./`, `\\` forms | `NoDrive` / `DevicePrefix` → 403 | code-decided; the probe's `url-long` cell threw in the page before WRR, so it is **not** evidence for R3 |
| R4 | `%2F`, `%5C`, `%00` before decoding; decoded bytes not valid UTF-8 | `ForbiddenEscape` → 403 | Chromium rejects the same three; one decode only; probe `url-backslash` was decided by the probe's own decode, not this rule |
| R5 | a decoded segment contains `/`, `\` or NUL | `DecodedSeparator` → 403 | separators that survived R4; `:` is **R8's alone** (single-fault NC) |
| R6 | a segment is `.` or `..`; **interior empty segments are collapsed** (`a//b` ≡ `a/b`, as Chromium keeps and Electron serves); a trailing empty segment (directory URL) → 403 | `DotSegment` → 403 / collapsed | Chromium canonicalises `%2e%2e` before WRR, so this rule is exercised by goldens, not by the probe |
| R7 | drive or leading components ≠ the admitted root's under `CompareStringOrdinal` ignore-case | `OutsideRoot` → 403 | Windows' own rule; one identity class per spelling on case-insensitive directories |
| R8 | a remaining component fails `validate_windows_package_component` (`:`, `<>"\|?*`, control chars, trailing `.`/space, reserved device basenames, `~`) or NFC | `Component(name)` → 403 | the only Host rule for `:` (ADS) and `~` (8.3): NTFS resolves both at the NT layer |
| R9 | relative path > 100 bytes | `TooLong` → 403 | same bound as the package namespace owner |

Divergence from Electron, stated plainly: Electron serves any readable `file:`; W3 serves
only the admitted root, refuses `~` spellings and paths over 100 bytes (pre-existing package
grammar), and refuses every reparse point. T1 records these.

**Refusal contract (A7).** For a `file:` URI the handler always sets a response: `200`
with bytes and headers (`Content-Type` from one `keld-wv` extension table — the MIME table is
its own atom; apps override it through S1; `Content-Length` is Host-owned; the baseline CSP
on every `DOCUMENT`-context response), `404` empty when the parser accepted and the opener
reported absent, `403` empty for every parser refusal and every other opener refusal. No
body on refusal, no `Location`, no `Range` until AC23 settles whether media range requests
arrive under `file://`.

**Document correlation (A8, L2).** The record is kept for **every** top-level navigation the
policy allowed, whatever its scheme, so the eval gate (A12) sees all of them:

```rust
struct PendingTopLevel { navigation: u64, uri: ParsedTarget /* file: parsed; other schemes as text */,
                         kind: NavigationKind, served: Option<ServedDocument> }
struct ServedDocument { components: RootRelativeComponents, status: u16, bytes: u64 }
```

- `NavigationStarting` (top-level, allowed): push.
- WRR `ResourceContext == DOCUMENT` whose **parsed** URI equals a pending `file:` record's
  components and whose `served` is `None`: serve, then attach.
- `ContentLoading(n)`: Host-served iff `served.status == 200 && !IsErrorPage`; the identity
  reported to W1/#1192 is `(root, components)` with the bytes as currently present in the
  root (owner Q2; W1 L390–394 to be aligned, §10 B7); the attach compares components with
  the exported component equality. Dropped at `NavigationCompleted(n)`.
- A frame document with a **different** URI never attaches (AC06 c); a frame with the
  **same** URI attaching to `n` is the same `(root, components)` by construction.

INFERRED: the top-level DOCUMENT request for `n` arrives between `NavigationStarting(n)`
and `ContentLoading(n)` on the STA. Probe P2 (§10 Q7) counts DOCUMENT WRRs per pending
window over 30 loads with the different-URI iframe racing an absent top-level file, and the
"DOCUMENT-only, ignore URI" mutant must fail. Counters are the page oracle and W1's epoch
holder, not Host records.

### 4.5 Serving contract, filter scope, threading, and app-declared headers

**Filter scope (A2, A17).** Per webview, before `navigate_initial`:
`AddWebResourceRequestedFilterWithRequestSourceKinds(L"file:*", CONTEXT_ALL, DOCUMENT)`.
The filter is matched against the normalized URI with the scheme lowercased (L1307), so
`FILE:`, UNC and host spellings all match; `https:`, `data:` and `blob:` never raise the
event and never touch the UI thread. Worker kinds (`SHARED_WORKER | SERVICE_WORKER`) for
`file:*` are registered **once per environment** on the first live webview (L72) and
re-registered on the next live webview when that one closes (AC23 cell); whether they ever
fire under `file://` is UNKNOWN (only a dedicated worker was attempted; **rule not
exercised**). Never `*`.

**Threading (A7; fits `keld-wv` AGENTS L5–6).** The handler runs on the engine STA. If P1
selects a deferred arm, a worker may read bytes **from the file handle only** (not a
platform handle) into a buffer and must post completion through the tao `EventLoopProxy`;
the STA then creates the stream and calls `Complete`. Every COM object (args, request,
deferral, response, stream) stays on the STA. Arm (c) (stream created on the worker) is
measured for the record but is not adoptable without an instruction change (§10 B10).
Contract independent of the arm: one response per request; a deferral completed exactly
once on the STA or the request answered `403`/empty on the error path; the stream complete
before `Complete`; buffered arms bound the buffer by the opened file's `size` whose
admissible maximum is the package member bound once `keld-pack` names one (§10 Q9); the
chunked arm needs no such bound. The per-request response allocation is the recorded
exception in §2.

**S1 app-declared headers (A15, A19).** The facade forwards `session.webRequest.
onHeadersReceived(filter, listener)` registration to the Host as a control message that
carries only "listener present"; **the Electron match-pattern matcher lives once, in the
facade**. When a listener is present the Host takes the WRR deferral for **every** `file:`
response and asks the role; the facade applies the app's `filter.urls` and replies
"unchanged" or the listener's headers; no listener → no round trip. Host-owned fields
(`Content-Length`, status line) are not overridable; `Content-Type` and the app's CSP are.
No wall-clock deadline while the role is alive: a slow role delays the page, as Electron's
blocking listener would; role retirement (`RoleGeneration` moved) fails every pending
request closed (`403`/empty) **once the role is known to be gone**; between the
`RoleGeneration` move and the successor's registration (or its explicit "no listener"), and
**only when the predecessor role had a listener**, the Host **holds** `file:` requests on the
STA FIFO behind their deferrals instead of refusing them, bounded by the existing recovery
deadline and a count bound; when the bound trips, held requests are answered `403`/empty;
with no predecessor listener nothing is held (no round trip existed before the loss). The round trip is **not** one of #540's four CONTROL records (#1192 L145): it
needs its own bounded record type or a dedicated FIFO on the compat-control channel, with
a stated bound and an overflow rule that prefers holding on the STA FIFO over `403` — owner
decision (§10 B11). **Ordering (A19) — no seal.** For compat roles the window's first
document is the application's own first `loadURL`, exactly as in Electron: a schema-2
compat boot creates **no** window at boot (`InitialWindow::None` → ready trigger
`HostInitialized`, `app_session.rs:2383, 3431`; W2 gh1188 "zero windows at Ready"), Bun's
`app.whenReady()` resolves, the application registers `session.webRequest.onHeadersReceived`
(the Electron-correct place; the pinned application does so inside `whenReady().then`,
`electron.js` L987–L994), then creates its `BrowserWindow` and calls `loadURL` (#459's seam) —
registration and load are two commands on the **one FIFO control channel**, so the listener
covers the window's first DOCUMENT response (AC15 a). r3's "registrations sealed" record is
withdrawn: it would have had to be sent before any Electron-correct registration (so the
entry would have been served with the baseline only), moving it after `ready` would deadlock
(`ready` ← Host Ready ← initial navigation ← seal), and `KELD-WV-005` is the renderer hang
guard summed into `WEBVIEW2_STARTUP_BOUND`, not a startup phase of its own. The
`create_app`/`navigate_initial` path (`app_session.rs:3650-3667`) remains only for schema-1
renderer-descriptor boots, which have no Electron facade and no listener: their entry is
served with the Keld baseline alone, stated. **Recovery:** held as above when the
predecessor had a listener (AC15 d). A19's falsifier is the FIFO order swap (AC15 a). The Keld baseline CSP is always emitted as its own header on every
`DOCUMENT`-context response (CSP3: every policy enforced; AC16); its content belongs to arch
03 §2 `web.csp` and its owner; W3 contributes probe P4 for `'self'` under an opaque-origin
`file:` document (§10 Q5).

### 4.6 Navigation signals and `will-navigate`

**Oracle facts (Electron v45.1.0):** `did-start-loading` (`DidStartLoading`, L2599) and
`did-start-navigation` (`DidStartNavigation` observer, L2752) are emitted before the
navigation throttle runs; `will-navigate` is emitted by `ElectronNavigationThrottle::
WillStartRequest` only when the navigation is renderer-initiated and in the main frame
(throttle L42–62), never for `loadURL`/`back` (docs L270–276); a vetoed navigation is
`CANCEL` → `ERR_ABORTED`, for which `DidFinishNavigation` emits **no** `did-fail-load`
(L2932–2933) and nothing commits (L2872); `did-stop-loading` follows (L2620); `did-navigate`
is emitted at commit with the response headers' status (L2907).

**Mapping (per top-level `NavigationId`; generation stamped; nothing synthesized):**

| Electron event | WebView2 source | Rule |
| --- | --- | --- |
| `did-start-loading` | `NavigationStarting` (top-level), **before** the policy decision | also for navigations the Host then cancels |
| `did-start-navigation(url, isInPlace=false, isMainFrame=true)` | same event, before the policy decision | matches Electron's observer-before-throttle order |
| `will-navigate(url)` | `NavigationStarting` classified page-initiated (below), `NavigationKind == NEW_DOCUMENT`, `IsRedirected == FALSE` | notification only; `preventDefault()` is not consulted by the Host (no deferral exists); emitted whether the Host allows or cancels |
| `did-navigate(url, httpResponseCode, httpStatusText)` | `ContentLoading(n)` with `IsErrorPage == FALSE` **and no Host `404`/`403` DOCUMENT record for `n`**; status from the Host's own served record for `file:`; for allowlisted remote documents from `WebResourceResponseReceived` for `n`'s DOCUMENT request when available, else `HttpStatusCode` at `NavigationCompleted` (documented ordering divergence) | — |
| `did-navigate-in-page(url, isMainFrame=true)` | `SourceChanged(IsNewDocument == FALSE)` + `HistoryChanged` | +0 generation; no `will-navigate` |
| `dom-ready` | `DOMContentLoaded` (top-level) | — |
| `did-finish-load` | `NavigationCompleted(n, IsSuccess == TRUE)` **and no Host `404`/`403` DOCUMENT record for `n`** (a committed empty error body never finishes "loading" in Electron's sense) | — |
| `did-fail-load(errorCode, errorDescription, validatedURL, isMainFrame=true)` | Exactly one per `NavigationId`, decided at `NavigationCompleted(n)`: a Host `404`/`403` DOCUMENT record for `n` **takes precedence** (emitted once, whatever `IsSuccess` says, including `IsSuccess == TRUE` for a committed empty body); otherwise `IsSuccess == FALSE` with `WebErrorStatus ≠ OPERATION_CANCELED` | Host `404` → `-6`; Host `403` → `-10`; **cancelled navigations emit nothing** (`OPERATION_CANCELED` 14 ≙ `ERR_ABORTED`); remaining `COREWEBVIEW2_WEB_ERROR_STATUS` values → the table T4 lands, verified by the GH-551 oracle |
| `did-stop-loading` | after `did-finish-load`, after `did-fail-load`, or after `NavigationCompleted(n, OPERATION_CANCELED)` | the facade's `isLoadingMainFrame()` turns false here; `executeJavaScript`'s load wait keys on it |

**`will-navigate` under WebView2 (A10).** Constraint set: `NavigationStartingEventArgs`
has no deferral, so `Cancel` must be decided on the STA without Bun; the allowlist must be
evaluated on the original event before any generation change (#1192 L107: page-initiated
start/cancel is +0; a trusted Host load is +1 before the engine call and revokes the live
document, L110–112 — which is why any replay would kill IPC on a refused or failed
destination and contradict W1 AC10). Designs scored (0–10; confidence of the chosen 0.75):

| Design | Fidelity: veto-all listener | Fidelity: conditional listener | Fidelity: no listener | Honesty | Generation/IPC safety | Complexity | Mean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| (C) r1 cancel-and-replay (every page navigation) | 7 | 7 | 3 (replays without a listener) | 5 | **1** (+1 before allowlist; history/POST/redirect corruption; loops on canonicalization mismatch) | 3 | 4.3 |
| (B) default-deny all page-initiated top-level | 10 | 3 | 2 | 7 | 10 | 9 | 6.8 |
| (D) reviewer hybrid: no listener → allowlist; veto-all listener (classifier) → cancel + notify; conditional listener → replay `NEW_DOCUMENT` only, page-attributed generation (#1192 amendment) | 9 | 7 | 9 | 7 | 6 (replay still re-issues; POST/referrer lost; needs B2 amendment) | 4 | 7.0 |
| (E) hybrid without replay: listener present → cancel every page-initiated `NEW_DOCUMENT` + notify; no listener → allowlist | 10 | 4 | 9 | 8 | 10 | 8 | 8.17 |
| (A′) r2: allowlist-first, notification-only, derived allowlist — **but every in-root `file:` target counted as on-allowlist** | 4 (with in-root always allowed, "entry only" cannot stop a user-content link from reloading the entry with an attacker-chosen query or reaching any vendored in-root HTML; Electron checklist #13 disappears for in-root targets) | 6 | 9 | 6 | 10 | 8 | 7.17 |
| (A″) r3: allowlist governs every page-initiated target incl. in-root, **query cut** | 7 (an `entry` form still admitted `index.html?attacker=1`, which a veto-all listener cancels in Electron; W1 treats the query as non-identity, so the attacker-query document would have received the epoch) | 8 | 9 | 8 | 10 | 7 | 8.17 |
| **(A″) r4: the derived allowlist governs EVERY page-initiated top-level target including in-root `file:`; `file:` forms match components AND query; targets split by who chose them (Host attempts to the window's entry always admissible)** | 10 (the `none` form cancels every page navigation, entry-with-query included, as the listener did) | 8 (a literal is lifted only when every predicate guarding the allow path is a URL predicate and keeps that predicate's query constraint; anything else `none` + flag/attestation) | 9 (no listener → `in-root:**`, Electron's default) | 9 | 10 (no Host load, no replay, +0 on cancel; allowlist before any +1; entry always reachable for recovery) | 6 | **8.67** |

Means are over the six numeric columns. **Decision: (A″, r4).** The rule, stated so W1 can
quote it verbatim:

> **W3 navigation rule (A″).** Navigation targets are split by **who chose them**.
> **Page-initiated** top-level navigation proceeds iff the target matches the window's
> `web.navigation.allow`. For a `file:` target the match is on the components
> `file_url_root_relative(target, admitted_root)` returns **and on the query**; the fragment
> is ignored; a parser failure is off-allowlist, and in-root membership is necessary, not
> sufficient. The `file:` allow forms are `none` (no page-initiated `file:` navigation at all
> — what a veto-all or unreadable `will-navigate` listener derives), a literal in-root
> component path carrying the query constraint of the application predicate it was lifted
> from (an exact query, or "no query"), and `in-root:**` (every in-root regular file, any
> query — what the absence of a listener derives). Non-`file:` entries use the vetted
> manifest matcher. Components and queries are compared with the parser's **one component
> equality** (per-component NFC + `CompareStringOrdinal` ignore-case, the R7 class), which
> `keld-guard` exports and every consumer — allow forms, attribution, W1 eligibility — uses.
> The decision is taken on the original `NavigationStarting`, before any generation change:
> off-allowlist → `Cancel`, generation +0, the live document and its IPC survive;
> on-allowlist → proceed, generation +1 only at commit. `will-navigate` is a notification
> only; `preventDefault()` is not consulted. The Host never replays a page navigation.
> **Host attempts** (the boot `Navigate`, an `onLoss` reload, the application's
> `loadURL`/`reload`) to the window's **entry components are always admissible, whatever the
> query**; any other Host target is checked against the same allowlist **before** its +1 — an
> off-allowlist Host attempt is a typed refusal at +0, never a revoked document. Redirect hops
> of an admitted Host attempt are checked too; a cancelled hop fails that attempt closed
> (#1192 L112), which is a stated divergence, not an exemption.

Rules, applied on the original `NavigationStarting` in this order:

1. **Attribution.** The event is **Host-initiated** iff (a) it is the first
   `NavigationStarting` after the window's one pending Host attempt was issued and its URI is
   parsed-equal to the attempt's (for `file:`: equal components under the exported
   component equality **and an equal query** — a page `index.html?b` racing the app's
   `loadURL(index.html?a)` is never attributed to the Host; byte-equal normalized text plus
   query otherwise) — the Host then records that event's
   `NavigationId` as the attempt's — or (b) its `NavigationId` equals the recorded attempt id
   (redirect hops keep the id; "`IsRedirected` while an attempt is pending" is **not** used,
   because a superseding page navigation could carry it). Everything else is page-initiated.
   `IsUserInitiated` is never consulted for attribution: Microsoft documents that
   "navigations initiated via WebView2 APIs are considered as user initiated"
   (`icorewebview2navigationstartingeventargs.md`, `get_IsUserInitiated` remarks), so it
   cannot separate a Host `Navigate` from a user click. `javascript:` and `about:` URIs are
   page-initiated and off-allowlist by construction.
2. **Allowlist, before anything else — for every page-initiated target, in-root `file:`
   included.** `file:` targets: `file_url_root_relative(uri, root)` must succeed **and** the
   returned components plus the query must match an allow form (`none` matches nothing; a
   literal matches equal components and its query constraint; `in-root:**` matches any
   in-root components with any query), compared with `keld_guard::components_equal` (the
   exported R7-class equality; the same function W1 eligibility and rule 1 use, so
   `INDEX.HTML` and `index%2Ehtml` decide identically everywhere); other schemes:
   `web.navigation.allow` through `path_in_scope`. Off-allowlist → `SetCancel(TRUE)`;
   generation **+0** (#1192 L107); the live document and its IPC survive
   (W1 AC10). On-allowlist → proceed; generation +1 only at commit. The `file:` allow forms
   are a #541 decision (§10 B12): they are matched on parser components, never through
   `path_in_scope`, and exposing a navigation entrypoint of `keld-guard` outside `evaluate`
   (which refuses webview principals) is a `keld-guard` public API change (§8).
3. **Notification.** For a page-initiated `NEW_DOCUMENT` event with `IsRedirected == FALSE`
   the facade emits `will-navigate(url)` when the app has a listener, after
   `did-start-navigation`, whether the Host allowed or cancelled. `event.preventDefault()`
   changes nothing (documented divergence). `BACK_OR_FORWARD`/`RELOAD`: allowlist applies,
   no notification until P3 (§10 Q10) settles whether Electron treats script history
   navigations as renderer-initiated (UNKNOWN).
4. **No replay, ever; Host attempts are checked first; the entry is always reachable.** The
   Host never re-issues a page navigation. A Host attempt (boot `Navigate`, `onLoss`
   reload-entry, the app's `loadURL`/`reload`, #459) whose target has the window's **entry
   components is always admissible, whatever its query** (so `onLoss` recovery and an
   `index.html?x` reload can never be refused by a derived allowlist that contains no entry
   form); any other Host target is checked against the same allowlist **before** #1192's
   "+1 before the engine call": an off-allowlist target is a typed refusal to the caller at
   +0 and the live document is untouched. Redirect hops of an admitted Host attempt are
   checked by rule 2 too; a cancelled hop fails that attempt closed (#1192 L112) — a stated
   divergence from Electron, where the app's own redirect policy decides. Edge recorded for
   #459: its `loadURL` contract consumes this pre-check and the entry exemption.
5. **`keld migrate` derives and writes `web.navigation.allow`** (framework mechanism, same
   classifier family as W1 §4.8; it never removes developer-added entries and merges by
   union): a `will-navigate` (or main-frame `will-frame-navigate`) listener whose every path
   calls `preventDefault()` → `none`; a listener that lets a navigation through only under
   predicates that are **all** predicates on the target URL (literal, prefix, origin, query
   tests) → those literals become entries (in-root literals in parser-component form **with
   the predicate's query constraint** — exact query or no query; others in vetted-matcher
   grammar); any allow path guarded by a non-URL predicate (`!loggedIn`, a setting, page
   input) → **not lifted**: `none` plus a flag naming the site, the window runs strict with
   the listener as notification, and an attestation (W1 rule 3) is the way to widen; a
   listener the classifier cannot read (minified bundle, listener installed from
   `app.on('web-contents-created')`, computed handlers) → `none` + notification + report
   line, **not** "unsupported"; no listener → `in-root:**` (Electron's default of free
   in-root navigation) **provided W1's eligibility makes every Host-served in-root regular
   top-level document eligible (recommended, B7 v)** — if W1 instead keeps a declared-entry
   eligibility, rule 5 emits exactly the declared entries and never `in-root:**`, because W3
   must never admit a page navigation to a document W1 would send to FailedClosed and
   `onLoss` (three such clicks would close the window: a page-triggerable loss absent in
   Electron). No application exception exists.

Residuals printed in the report and T1 entries: `preventDefault()` on an allowlisted
destination is ineffective; `shell.openExternal`-from-listener keeps working (the URL is
notified); an app that cancels and then re-issues a navigation through `loadURL` gets the
pre-checked Host attempt of rule 4 (allowed targets behave as in Electron, off-allowlist
targets are a typed refusal instead of Electron's load); a cancelled `http(s)` navigation's
request may already have left the process (non-`file:` owner's concern). Probe P3 (§10
Q10): 30 trials each of link click, `location.href`, `location.replace`, form `POST`,
`history.back()`, `location.reload()` on a real document, recording `NavigationKind`,
whether Electron (same fixture) emits `will-navigate` for the history kinds, and
`history.length` before/after; `did-start-loading` order for cancelled navigations is taken
from the GH-551 differential, not from the emit site alone.

### 4.7 Settled `executeJavaScript`

Flow (Windows): Bun `webContents.executeJavaScript(code, userGesture)` → facade waits for
`did-stop-loading` if loading → control message with `(correlation, role generation)` →
`keld-core` checks the role generation and posts `AppWindowCommand::Eval` → `keld-wv`:

1. **Context binding (mandatory), one clock.** `Runtime.enable` is on from create; the
   Host registers the `executionContextCreated` / `executionContextDestroyed` /
   `executionContextsCleared` receivers **before** calling `Runtime.enable` (the probe
   registered them after, `main.rs` L1090–1119, so the initial event's arrival order is
   UNVERIFIED and nothing below depends on it). The main frame's default context id
   (`auxData.isDefault`, main `frameId`) is **bound to the generation at `ContentLoading`**:
   when `ContentLoading(n)` advances to `G`, the Host records `(G, ctx)` once the created
   event for the new document has arrived (whichever of the two arrives second completes the
   binding); CDP events and WebView2 events are two clocks whose relative order is UNKNOWN,
   so neither alone names the current document. **Unbinding:** `executionContextDestroyed`
   or `executionContextsCleared` for the bound `ctx`, every Host +1 (#1192 L110) and every
   uncorrelated replacement (L114) clear the pair; it is complete again only after the next
   `ContentLoading`/created pairing. At dispatch the Host uses the bound pair `(G, ctx)`; if
   the pair is incomplete (the gap), the result is a typed `NoDocumentContext` that the
   **facade** converts into a bounded wait for the Host's per-generation **`context-bound`**
   event (not a tight re-dispatch: Electron's own wait returns immediately when the frame is
   not loading, `web-contents.ts:44-45`, which would livelock here), then re-dispatches; the
   application never sees it unless the bound elapses or the window dies. After a Host
   attempt fails or is cancelled (#1192 L112, FailedClosed), the pair is **not rebound** to
   the surviving engine document: evaluations are refused typed until the next admitted
   load binds a new pair (refuse, not rebind — the revoked document has no authority and W1
   applies `onLoss`). An eval whose `G` is old but whose `ctx` is new cannot be formed, so no
   script runs in a document that is then rejected (`G_old, ctx_new` was the r2 defect: side
   effects in the current document followed by `DocumentReplaced`, which for the pinned
   application means an irreversible migration mark).
2. **Generation capture.** `G` is the bound generation, whatever the phase (`Live`,
   `FailedClosed` for a remote/non-entry document): Electron evaluates in the current
   document regardless of IPC eligibility. Only `Destroyed`/`Exhausted` refuse.
3. `CallDevToolsProtocolMethod("Runtime.evaluate", { expression, awaitPromise: true,
   userGesture, allowUnsafeEvalBlockedByCSP: false, uniqueContextId: ctx, objectGroup:
   <correlation id>, serializationOptions: { serialization: "deep", maxDepth: <codec depth
   bound> } })`, followed after settlement by `Runtime.releaseObjectGroup(<correlation id>)`
   on every path (deep mode binds a remote object for each object result and a rejected
   error is wrapped by id, `injected-script.cc` L640–648, L1181–1200; without the release they
   live for the document). The Host parses **only CDP failure envelopes** (`code` **and exact
   `message`**: `InvalidParams` "uniqueContextId not found", `v8-runtime-agent-impl.cc` L232,
   and `-32000` "Execution context was destroyed." map to `DocumentReplaced`; `-32000` with
   "exception during deep serialization", "Cannot find context with specified id" or
   "Promise was collected" map to typed `EvalFailed` with the text); a **success body**
   (`result` plus `exceptionDetails`) is forwarded to the facade unparsed and opaque.
4. **One eval state machine, one owner.** Each correlation is `Pending → Retired |
   Delivered`, owned by the `keld-wv` STA; the transition is taken exactly once. **Retire:**
   in **every** STA turn that changes `G` — page replacement commit (#1192 L111), the Host
   load's "+1 before the engine call" (L110), an uncorrelated replacement (L114),
   `Exhausted` and `Destroy` — every `Pending` correlation whose `G` **differs from the new
   `G`** becomes `Retired` and **one** rejection (`DocumentReplaced`) is posted for it (a
   G₁ entry can therefore never outlive G₂→G₃); an attempt fail/cancel (L112) changes no
   `G` and retires nothing, but the pair stays unbound (step 1: refuse, not rebind).
   `Retired` entries are owned by their outstanding CDP callback and freed when that
   callback fires or the webview dies (bounded by the in-flight count, no tombstone ledger —
   #1192's "no unbounded tombstone ledger"); until freed they **count against
   `MAX_PENDING_EVALS`** (a back/forward-cached context may never call back). **Deliver:**
   the CDP completion handler delivers iff the entry is still `Pending`; a `Retired` entry
   is freed without a post. There is no second generation check in the completion handler
   and no third in `keld-core`: `keld-core` forwards settle frames; the **role** axis stays
   with KEL-75/KEL-143's existing retirement of role-bound correlations (the role moves →
   its correlations are retired by that owner, not by a duplicate here). Capacity: pending
   evals per window are bounded (`MAX_PENDING_EVALS`, a typed refusal when exceeded; the
   number is set with the #540 owner, §10 B11) because a never-settling script
   (`new Promise(r => window.done = r)`) holds its entry until retirement; Electron has no
   such bound (documented divergence).
5. Independent counter for every eval AC: the **Bun facade** (another process) counts settle
   frames per correlation (≤ 1) and asserts that no payload produced in generation G₁ is
   observed after G₂ committed.

**Value transport — scored, recommendation to the GH-544/GH-540 owners (B3):**

| Mechanism | Domain fidelity | CSP integrity (no codegen scope) | One codec | Host attack surface | Stability | Mean |
| --- | --- | --- | --- | --- | --- | --- |
| (i) `Runtime.evaluate` `returnByValue:true` | 4 (JSON: `Map`/`Set`/`Date`/`-0`/`undefined` lost) | 10 | 5 (JSON is a second format) | 10 (Host forwards text) | 10 | 7.8 |
| (ii) `Runtime.evaluate` (`returnByValue:false`) → `Runtime.callFunctionOn(objectId, <generated #1190 encoder>)` → `releaseObject` | 8 | **3** — correctly scoped: `innerCallFunctionOn` enables string codegen unconditionally (V8 L151) and the scope stays open across the call and its drained microtasks (L158, L188); the encoder calls page-patchable intrinsics (`Reflect.ownKeys`, `Object.getOwnPropertyDescriptor`, `sort`, `Map`/`Set` iterators — #1190 L155–157, L201–204), so **any** patched intrinsic or microtask, not only a Proxy trap, runs with `eval` allowed | 10 | 10 | 8 | 7.8 |
| (iii) `Runtime.evaluate` with `serializationOptions: { serialization: "deep" }`, **transcoded on the Host** into #1190 bytes | 8 (Proxy → `{type:"proxy"}` without running traps, `v8-deep-serializer.cc` L339–341 ≈ Electron's `DataCloneError`; getters run under the page's intact CSP; typed arrays/ArrayBuffer/Error are type-only, L334–357 — a codec-owner domain decision) | 10 (runs under `evaluate`; codegen only when the flag is true, L400–402) | 6 (a second format, but absorbed on the Host) | **4**: the Host parses a page-controlled nested structure (depth/width/aliases) in Rust — exactly the attack surface #1190 V7 keeps off the Host | 5 (experimental) | 6.6 |
| (iv) `returnByValue:false`; primitives taken from the `RemoteObject`; objects via (iii) transcoded on the Host | 8 | 10 | 6 | 4 (same Host parsing for objects) | 5 | 6.6 |
| **(v) (iii)'s `deepSerializedValue` forwarded Host-OPAQUE as bytes to the Bun facade, decoded there by a decoder the codec owner (#1190) owns** | 8 | 10 | 7 (a second value format, but one owner — the codec owner — and no second encoder) | **10** (Host forwards opaque bytes exactly as V7 requires; the only Host-side rule is the existing frame bound) | 5 (experimental; a removed option would surface in the self-test as a missing `deepSerializedValue`, AC13) | **8.0** |

Means are over the five numeric columns. **Decision: (v).** (i) also has no codegen scope
and no Host parsing; the discriminators against it are domain fidelity (8 vs 4) and one
codec (7 vs 5), and against (ii) CSP integrity (10 vs 3). **Stated plainly: (v) does not meet
#1190 V4.** The deep serializer reads own enumerable properties through `value->Get`, so
getters **run** (`v8-deep-serializer.cc` L222, L257; a throwing getter is `-32000 "exception
during deep serialization"`), symbol-keyed and non-enumerable keys are silently dropped
(`ONLY_ENUMERABLE | SKIP_SYMBOLS`), cycles and aliases are represented rather than refused,
and `maxDepth` defaults to `kMaxInt` (`v8-runtime-agent-impl.cc` L295–297) — hence the
explicit `maxDepth` in step 3. Electron's structured clone also runs getters, so this is a
fidelity argument the codec owner weighs (B3/Q15), not a W3 waiver of V4. The codec owner
decides the decoder (deep-serialized → the #1190
decoded domain), the domain gaps to accept (typed arrays/ArrayBuffer/Error type-only) and a
bound tied to `MAX_FRAME_LEN` (16 MiB) rather than 4096 bytes (§10 B3). AC10(g) is the
falsifier that separates (v) from (ii). Until recorded, AC14 is blocked.

**Capability self-test (A13, AC13)** runs in `create` after the controller exists and before
`navigate_initial`, inside the `GuardInstalled` mint, within `WEBVIEW2_STARTUP_BOUND`:
receivers registered, `Runtime.enable`, `1+1` by value, `Promise.resolve(3)` awaited, and
one `evaluate` with `serializationOptions: { serialization: "deep" }` whose result **must
contain `deepSerializedValue`** (a runtime that dropped the option would silently fall back,
crdtp L124–128; this is the only way the self-test can see it), and one **negative probe**:
`evaluate` with a bogus `uniqueContextId` must fail `InvalidParams` "uniqueContextId not
found" (L232) — the binding AC12 depends on is thereby shown to be honored, not ignored. It
does not depend on the initial `executionContextCreated` arriving before the first
`Navigate` (UNVERIFIED). The CSP flag: a self-test is **INFERRED possible** — inject a
`<meta http-equiv="Content-Security-Policy" content="script-src 'none'">` into the initial
`about:blank` through `evaluate`, then `evaluate("eval('1')")` with the flag false must
throw — and is a device cell until shown to work (§10 Q11); it is not claimed by this
revision. `Runtime.enable` also makes the engine serialize console arguments (page-
observable timing; T1 divergence) and its event volume is P1 arm (g).

### 4.8 Scheme boundary

W3 contains `file:` only and its filter never raises `WebResourceRequested` for any other
scheme (AC19). Whether a `file:` page may navigate to or fetch `https:`, `data:`, `blob:` or
`about:` destinations is the navigation-policy owner's rule (#541 manifest keys, #460/
KEL-79, W1 §4.6 `web.navigation.allow` through `path_in_scope`). A remote document that
commits is not Host-served and receives no eligibility from W3; it is still evaluable
(§4.7) and its `did-navigate` status comes from `WebResourceResponseReceived`.

### 4.9 Rejected alternatives

| Alternative | Why rejected |
| --- | --- |
| Virtual host / custom scheme | Forbidden by #1185; Chromium opens the files; changes the origin. |
| Veto-only (Chromium loads allowed bytes) | Parser differential (host → UNC, `.lnk` 301, decoding); TOCTOU. |
| `*` WRR filter (r1) | Every remote request blocks behind the UI thread; environment-wide worker events; perf gate blind to it (review P1-4). |
| New `NtCreateFile` read-open in `keld-update` (r1 option a) | A second relative opener beside cap-std's; new `unsafe` where a reviewed safe one exists (review P1-8). |
| Link-count refusal / file-ID allowlist / content hashing | Owner decision 6491936458. |
| `ExecuteScript` / `ExecuteScriptWithResult` | Runs in the new document; no promise settle (probe N4). |
| Preload-bridge eval | Needs `eval` for statement completion values; CSP-blocked; forgeable; W1-dependent (D2). |
| Cancel-and-replay `will-navigate` (r1) | Revokes the live document before the allowlist; replays history/POST; loops on canonicalization mismatch (review P1-1/P1-2). |
| (A′) with every in-root `file:` target on-allowlist (r2) | An entry-only allowlist could not stop in-root page navigations (attacker-chosen entry query; vendored in-root HTML); Electron checklist #13 lost for in-root targets (review r2 P1-1). |
| Eval transport (ii) `callFunctionOn` + encoder (r2) | Unconditional codegen scope across the encoder's page-patchable intrinsic calls and drained microtasks (V8 L151, L158, L188; review r2 P1-3). |
| Host-side transcoding of deep-serialized values ((iii)/(iv)) | Puts page-controlled nested structure through a Rust parser on the Host — the surface #1190 V7 keeps off the Host. |
| Three redundant eval guards (r2) | Masked every single-fault control; duplicated lifecycle policy; unbounded retired ledger (review r2 P1-4). |
| Hybrid with replay for conditional listeners (reviewer D) | Still re-issues navigations; needs a #1192 page-attributed-attempt amendment; (A′) reaches the same fidelity statically. |
| `DocumentReplacing` dispatch gate on pending navigations (r1) | Refused evaluations Electron performs in the still-current document; `Live`-only gating refused remote/non-entry documents (review P1-6). |
| Static CSP extraction / meta CSP only | Falsified by runtime policy; meta limitations (D3). |
| `url` crate for parsing | Present in `Cargo.lock` (2.5.8, via tao/wry) but not a workspace dependency of `keld-guard`; its job is canonicalisation (resolves `..`), ours is refusal; its `file:` origin is opaque-per-call. Dependency gate otherwise. |
| Host-side Electron match-pattern matcher for `onHeadersReceived` | A second matcher; the facade owns it once (§4.5). |

### 4.10 Types & channels, capabilities, wire, platform, seam, migration

- **Rust (sketches above):** `keld_guard::{AdmittedRootUrl, RootRelativeComponents, FileUrlRefusal,
  file_url_root_relative}`; `keld_update::{open_regular_beneath, OpenedRegularFile, OpenRefusal,
  Share}`, `ActivePackageSelection::tree`; `keld_wv::{AppResourceSource, OpenedResource,
  ResourceRefusal, WebviewSpec.app_resources, EvalOutcome, NavigationSignal}`;
  `WebEngine::eval(&mut self, id, script: &str, on_settled: EvalCallback) -> Result<(), WvError>`;
  `AppWindowEvent::{Navigation { view, signal, generation }, EvalSettled { correlation, outcome, generation },
  HeadersRequested { view, correlation, url, headers } }` (**`AppWindowEvent` drops `Copy`**);
  `AppWindowCommand::{Eval { view, script, correlation, role }, HeadersReply { correlation, headers },
  RegisterListener { view, kind } }`.
- **TypeScript (`@keld/electron`, #1154):** `WebContentsEventMap` gains the §4.6 events;
  `WebContents.executeJavaScript(code: string, userGesture?: boolean): Promise<unknown>`
  (resolved type = the codec owner's decoded domain; never `any`); `session.webRequest.
  onHeadersReceived(filter, listener)` with the match-pattern matcher in the facade.
- **Capabilities / manifest:** none new; `web.csp` and `web.navigation.allow` are existing
  keys (W1 §4.6 / #541). `keld migrate` writes `web.navigation.allow` (§4.6 rule 5).
- **Wire:** no KIPC framing change. New control-plane kinds on the #539/#540 channel:
  eval request/settle, headers round trip (own bounded record or FIFO, B11), navigation
  signals, listener registration. Wire gate.
- **Platform:** Windows only; macOS/Linux implement the `eval` signature with native
  completion handlers and no W3 generation semantics (§10 Q4).
- **Runtime seam:** `create` registers, inside the `GuardInstalled` mint and before
  `navigate_initial`: `file:*` filter + handler, `NavigationStarting` policy/attribution,
  `ContentLoading`, `DOMContentLoaded`, `SourceChanged`, `HistoryChanged`,
  `NavigationCompleted`, `WebResourceResponseReceived`, `Runtime.enable` + context tracking,
  the self-test. Handlers live for the WebView lifetime on the STA; renderer crash ≠ Host
  crash; the admitted root `Dir` lives for the session and is dropped after the webview;
  config captured once at boot; listener registrations applied in control-channel order.
- **Migration unit:** `app_session.rs` boot and coordinator, `webview2/mod.rs`, `engine.rs`,
  `keld-update` opener + accessor, `keld-guard` parser, `@keld/electron` facade, `keld migrate`
  allowlist derivation, conformance entries. No temporary adapter; `open_relative_file_windows`
  removed in the PR that lands its replacement. Compatibility fallback: not required.

## 5. Boundaries

- Implement in: `crates/keld-guard/src/lib.rs` (parser; relocated NLS helpers),
  `crates/keld-update/src/windows_baseline.rs` (+ `locate.rs`, `helper.rs` call sites),
  `crates/keld-core/src/app_session.rs`, `crates/keld-wv/src/engine.rs` and
  `crates/keld-wv/src/webview2/`, `packages/@keld/electron/src`, `crates/keld-cli` (`migrate`
  allowlist derivation), `docs/architecture/03,05` (T0), `docs/engineering/keld-error-codes.md`
  (via owner), `keld-compat` entries (T1).
- Must not touch: `keld-guard` evaluator/`path_in_scope` semantics; `crates/keld-ipc`
  framing; the KIPC channel table (#508); macOS `wkwebview/*` beyond the mechanical `eval`
  signature; `crates/keld-native`; KEL-130; workspace `Cargo.toml` (no new crate or
  dependency); the W1 bridge script; the manifest schema (#541).

## 6. Tasks (each ≈ one PR; ordered; owner; blocking dependency)

- [ ] **T0 — this spec + architecture deviations** (owner: @amishabenramani). Blocks on §10
  B1–B11 and Q1–Q3.
- [ ] **T1 — conformance entries first** (`keld-compat` owner): root-only `file:` service,
  `~`/100-byte/reparse refusals, `will-navigate` notification-only + derived allowlist,
  `did-fail-load(-10)` for opener refusals, separate baseline CSP, remote `did-navigate`
  status ordering. Blocks on T0.
- [ ] **T2 — one opener, admitted root, loading path, containment** (Windows W3 implementer):
  parser + goldens (AC05), `open_regular_beneath` + `tree()` + `open_pinned_leaf` migration
  (AC04), root retention and boot migration, `file:*` filter/handler and `Navigate` loading
  (AC01–AC04, AC06, AC17–AC19, AC22, AC23), threading arm per P1. Blocks on P1 (AC20),
  public API + permission gates, the `keld-guard` unsafe-trigger line, #1192 T1 (B2).
- [ ] **T3 — settled eval** (same): `WebEngine::eval` destination form, self-test, context
  tracking, the one `Pending → Retired | Delivered` machine, per-correlation object groups, role correlation (AC10–AC13); AC14 after B3. Blocks
  on B2, B3, B5, the control kinds (wire gate).
- [ ] **T4 — navigation signals and `will-navigate`** (same, with #539): §4.6 mapping,
  error-code table, attribution, allowlist-first cancel (AC07–AC09). Blocks on B4 and P3.
- [ ] **T5 — app headers, baseline CSP, registration ordering** (same, with #1154):
  round trip, facade matcher, FIFO ordering (AC15, AC16). Blocks on P4, P5, B11.
- [ ] **T6 — `keld migrate` allowlist derivation** (`keld-cli`/KEL-17 owner): §4.6 rule 5,
  fixtures (veto-all, literal-conditional, unprovable), corpus run. Blocks on KEL-17 admitting
  the slice (W1 Q9).
- [ ] **T7 — Windows qualification receipt** (@amishabenramani): full RED matrix on a real
  device incl. elevated ETW, directory-symlink and intermediate-junction cells, AC23 cells,
  pinned-app migration/evaluation demand slices, AC21. Full application acceptance stays W0.

## 7. Test plan

| Test | AC | Shape and anti-flake |
| --- | --- | --- |
| `guard_file_url_root_relative` (pure; R7/R8 goldens Windows-only) | AC05 | Table rows + probe corpus; mutants: drop `:`, drop `~`, decode twice (non-discriminating, recorded), case-sensitive R7. Adversarial fixtures permanent. |
| `update_open_regular_beneath` (Windows, native) | AC04 | Fixture with leaf junction, **intermediate junction**, file symlink, directory symlink (precondition printed, never silently green), hardlink (`nlink` read-back), ADS, trailing dot; `Share::Pinned` and `Share::Served`; mutant: Win32 `CreateFileW` path open. |
| `update_pinned_leaf_migrated` | AC04 | Existing locate/helper tests pass unchanged through the generalised opener; source test that `open_relative_file_windows` and page-controlled `OpenOptions::open` are absent. |
| `wv2_file_origin_matrix` (device, ignored-by-default, STA harness as `media_acceptance`) | AC01–AC03, AC06, AC17–AC19, AC22, AC23 | `file:*` baseline + N2; plain-outside and `https` cells; per-rule at-risk counters; #1185's non-DOCUMENT NC in its own process; ETW when elevated. |
| `wv2_navigation_signals` (device) | AC07–AC09 | Loads (i)–(v); page oracle; GH-551 differential against the same fixture in Electron v45.1.0; mutants per row. |
| `wv2_settled_eval` (device) | AC10–AC13 | Promise/statement/reject/throw/getter/Proxy cells; gap dispatch; replayed G1; role recovery via the existing `no_flag/windows/recovery.rs` harness; injected CDP failure. |
| `wv2_headers_received` (device) | AC15, AC16 | Listener before/after `loadURL` (ordering), absent, retired; SVG/XHTML `Content-Type` override; forbidden-remote-script counter. |
| `migrate_navigation_allowlist` | §4.6 rule 5 | Fixtures: veto-all, literal-conditional, unprovable; golden manifests; mutant lifts a page-input condition. |
| `codec_eval_domain` (owner's) | AC14 | Lands with B3. |
| P1 harness (`keld-benches`) | AC20 | §9. |
| Receipt validator (`tools/`) | AC21–AC23 | Fields, at-risk per rule, "rule not exercised", UNKNOWN cells. |

Anti-flake: device tests behind the Windows feature flag; callbacks pumped by
`wait_with_pump`, no sleeps; each mutant in its own process; per-run temporary root with
its own DACL; precondition status printed for symlink/8.3/ETW cells.

## 8. Review gates triggered

- **unsafe — mandatory:** no new FFI function. (1) `keld-guard`: the two NLS calls move
  from locals of `validate_windows_package_paths` into shared private helpers used by the
  parser; `crates/keld-guard/AGENTS.md` KEL-53 T3 trigger line gains `file_url_root_relative`;
  same bounds and SAFETY proofs. (2) `keld-wv/webview2`: new COM registrations
  (`AddWebResourceRequestedFilterWithRequestSourceKinds`, `add_WebResourceRequested`,
  `GetDeferral`/`Complete`, `CreateWebResourceResponse`, stream creation,
  `add_ContentLoading`, `add_DOMContentLoaded`, `add_SourceChanged`, `add_HistoryChanged`,
  `add_WebResourceResponseReceived`, `CallDevToolsProtocolMethod`,
  `GetDevToolsProtocolEventReceiver`, `NavigationKind`) with local proofs and an issue-scoped
  `crates/keld-wv/AGENTS.md` update; independent unsafe gate on the final diff. The opener
  adds **no** unsafe.
- **public API — mandatory:** `keld-guard` parser **and a navigation entrypoint that
  evaluates `web.navigation.allow` for a `Principal::Webview` outside `evaluate`** (today
  `path_in_scope` is private behind `evaluate`, which refuses webview principals; the
  `file:` allow forms of B12 are matched on parser components by this entrypoint, never by
  `path_in_scope`); `keld-update` opener/accessor;
  `WebEngine::eval`, `WebviewSpec.app_resources`, `AppResourceSource`, `AppWindowEvent`
  (drops `Copy`)/`AppWindowCommand` variants; `@keld/electron` events, `executeJavaScript`,
  `onHeadersReceived`; `keld migrate` output key.
- **permission model — mandatory, independent reviewer:** two name owners; refusal table;
  allowlist-first cancel with +0; served-document identity; hardlink read-back; separate
  baseline CSP; fail-closed headers; eval context + generation gates; allowlist derivation.
- **dependency addition:** none.
- **wire protocol — #539/#540 owner:** control-plane kinds; the headers round-trip record (B11).

## 9. Perf impact — P1 is a pre-implementation gate

Budget at risk: architecture 01 §5 cold start → first paint (Windows measured row 469 ms
host-only at `39be9cc`; product-boot arm unmeasured). > 5 % of the chosen arm against arm
(e) needs a written waiver before T2. No number here is a claim.

| Atom | Definition | Oracle |
| --- | --- | --- |
| Census | `file:` requests **per document load** by `ResourceContext`, bytes per request, measured on the pinned application's real demand (`app.min.js` 9,865,843 B, `stencils.min.js` 7,688,864 B); the probe's 68 events were **per run over two fixture document loads** and are not a census of application demand; **non-`file:` requests raised = 0** under `file:*` | WRR log |
| Work | bytes read + streamed; opens per request (component depth); UI-thread occupancy per callback | handler timers (QPC) |
| Queue/copy | buffer copies per response; `EventLoopProxy` hand-offs; S1 round trips | counters |
| Clock | pre-spawn monotonic → KEL-64 double-rAF beacon; CDP `Network` as diagnostic | KEL-64 harness |
| Statistic | median, p95, bootstrap CI; 30 valid samples per arm per class; interleaved; never concurrent with cargo builds | registry policy |
| Artifact | raw JSON in `keld-benches`, hash-chained receipt, keld-research note; chosen arm named in T2 | keld-benches + keld-research |

Arms: (a) sync read + memory stream on the STA; (b) deferral + worker read of the file
handle + STA stream + `Complete`; (c) stream created on the worker (**measured only**,
B10); (d) chunked vs buffered for the 9.9 MB bundle; (e) Chromium file loader (baseline,
not shippable); (f) (b) + S1 round trip; (g) chosen arm + `Runtime.enable`. Diagnostics:
eval dispatch→settle latency; headers round-trip latency.

## 10. Approval blockers and open questions

**Blockers (who acts; first observable completion check):**

- **B1 Owner approval of C3 (g), the two-owner rule and the C4 binding** — @amishabenramani:
  exact-content approval on #1185 naming this head.
- **B2 #1192 T1 as a platform-neutral module; Windows mapping (N21)** — #1192 owner: §5
  amended; W3 named as the Windows oracle. No page-attributed-attempt amendment is needed
  (no replay).
- **B3 Eval-result domain, bound and transport** — GH-544/GH-540 owners with #539: accept
  transport (v) (deep-serialized value, Host-opaque, decoded in the facade by the codec
  owner's decoder) with its stated V4 departures — own enumerable getters run, symbol and
  non-enumerable keys dropped, cycles/aliases represented, typed arrays/ArrayBuffer/Error
  type-only — or name which are typed refusals; the depth bound passed as `maxDepth`; a
  byte bound tied to `MAX_FRAME_LEN`. Check: recorded decision; AC14 unblocked.
- **B4 Allowlist-first `will-navigate` and the `keld migrate` derivation** — navigation-policy
  owner (#541/#460; W1 §4.6): accept §4.6 rules 1–5 and the `web.navigation.allow` writer.
- **B5 Error-registry codes** — registry owner: capability refusal, `DocumentReplaced`,
  `NoDocumentContext`.
- **B6 KEL-254 AC7/AC8 extension** — KEL-254 owner.
- **B7 W1 alignment** — W1 owner (published head `043170ca`): (i) the Source rule already
  matches (`gh1184` L336–341); (ii) the identity text at L390–394 ("the bytes served for it
  are that verified resource") aligned to W3's identity — name inside the admitted root,
  bytes as currently present (owner Q2); (iii) W1 L121 still describes the r1 parser (root
  validated with the package grammar, no R6 collapse) — updated to §4.4 here; (iv) W1 quotes
  the A″ navigation rule (§4.6, r4 text) and its L493/L503–504/L512–520 text ("entry is
  implicit", "always allowed", "aligned with W3 r2 A′", "the only +1-before-engine load is
  onLoss", "allow entries cover non-`file:`") is replaced by it; W1 adopts the exported
  component equality for eligibility and runs the app preload in every Host-served `file:`
  top-level non-error document (no page-side string gate); (v) eligibility
  for **non-entry in-root documents** (allowed by `in-root:**` or a literal) is decided
  together with B12 — entry-only eligibility would send every in-root page navigation to
  FailedClosed and `onLoss` (W1 caps reload-entry at 3 then closes the window), which makes
  multi-page `file://` applications unusable; every reused contract is still an open draft
  (re-checked at approval).
- **B8 P1 report** — Windows operator (AC20).
- **B9 Probe evidence published** — Windows operator: keld-research note 268 (draft PR
  0monish/keld-research#38, head `1524f3e1`, both runs) merged, so every probe citation here
  resolves to a public path. Check: PR merged; note path stable.
- **B10 `keld-wv` threading instruction** — instruction owner through `.agents/instructions.md`:
  `crates/keld-wv/AGENTS.md` L5–6 must state that a worker may read a **file** handle while
  every COM/platform handle stays on the STA (arms (b), (d), (f) depend on that reading; this
  spec interprets the current text and cannot rely on its own interpretation), and that the
  per-request response, navigation-signal, eval-settle and header round-trip payloads are the
  recorded allocation exception; arm (c) needs the further sentence only if P1 selects it.
- **B11 Headers round trip and eval capacity** — #540 owner: a bounded record type or FIFO
  outside the four CONTROL records (#1192 L145) with its bound and an overflow rule that
  holds on the STA FIFO rather than answering `403`; the `MAX_PENDING_EVALS` bound and its
  typed refusal (§4.7 step 4).
- **B12 `file:` allow forms** — #541 owner (manifest grammar): the three forms of the A″ rule
  (`entry`, literal in-root component path, `in-root:**`), matched on parser components by
  the new `keld-guard` navigation entrypoint (§8), never by `path_in_scope`; the writer is
  `keld migrate` (union merge, developer entries preserved); non-entry in-root eligibility
  decided with B7 (v). Check: #541 revision names the forms and the entrypoint.

**Open questions:**

1. **Q1 (owner):** `Share::Served` = `READ | WRITE | DELETE` (recommended) or `READ | DELETE`.
2. **Q2 (owner):** dev stage validated by the package grammar at `keld dev` time so R8/R9
   refusals surface before serving (recommended, with a diagnostic naming the path).
3. **Q3 (#1154):** Electron error texts for sync throw / replacement; facade's pinned major.
4. **Q4 (#542/KEL-142):** `eval` destination signature on macOS now with a typed "not owned"
   refusal (recommended) or native completion semantics.
5. **Q5 (P4) UNKNOWN:** CSP `'self'` semantics for an opaque-origin `file:` document; 30 loads
   with `script-src 'self'` vs `script-src file:`.
6. **Q6 UNKNOWN:** whether `keld dev` stages pass `validate_windows_package_paths` today and
   how `keld dev` reports a reparse component (OneDrive/dedup/WOF roots).
7. **Q7 (P2) UNKNOWN:** `IsErrorPage` for a Host `404`/`403` top-level response; DOCUMENT WRR
   count per pending window with the different-URI iframe mutant; 30 loads.
8. **Q8 UNKNOWN:** media `Range` requests under `file://` (AC23).
9. **Q9 (keld-pack):** maximum package member size bounding buffered arms.
10. **Q10 (P3):** Electron emission for script `history.back()`/`location.reload()`;
    `NavigationKind` census; `history.length`; form `POST`.
11. **Q11 UNKNOWN:** CSP-flag honoring cannot be self-tested (crdtp ignores unknown fields);
    enterprise DevTools/CDP policy effects on `CallDevToolsProtocolMethod`.
12. **Q12 UNKNOWN:** 8.3 generation state of the qualification volumes; case-sensitive
    directory admission check at boot (`FILE_CASE_SENSITIVE_INFO`).
13. **Q13 (P5):** one executed Electron 44 receipt that `onHeadersReceived` fires for `file://`.
14. **Q14 UNKNOWN:** relative order of CDP `executionContextCreated` and WebView2
    `ContentLoading` for the same document (two clocks, §4.7 step 1); 30 loads, both orders
    recorded; the `(G, ctx)` binding tolerates either, the receipt reports the census.
15. **Q15 (codec owner, with B3):** the decoder from CDP's deep-serialized form to the #1190
    decoded domain lives in the facade; which type-only kinds (typed arrays, `ArrayBuffer`,
    `Error` fields) are accepted as losses and which are typed refusals.
16. **Q16 (KEL-75/KEL-143 owner):** confirm that role retirement already retires eval
    correlations keyed on `RoleGeneration` (AC11 iii) so W3 adds no second role-axis guard;
    until confirmed AC11 NC(4) is pre-declared "rule not exercised".
17. **Q17 (keld-guard, with B12):** the exported component equality is `CompareStringOrdinal`
    ignore-case over NFC components; whether it diverges from NTFS's per-volume `$UpCase`
    table for any admitted spelling is INFERRED-equal and is a golden + device cell
    (`INDEX.HTML`, `index%2Ehtml`, a Turkish dotless-i spelling) rather than a claim.
18. **Q18 (P6) INFERRED:** the create-time CSP-flag probe (`<meta>` `script-src 'none'` injected
    into the initial `about:blank`, then `eval('1')` with the flag false must throw); if it works
    on the device it becomes part of AC13 and Q11's residual shrinks to runtimes that ignore
    the meta policy on `about:blank`.

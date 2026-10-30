# Spec: Windows WebView2 `file://` navigation, app-code resource containment, honest navigation signals and settled `executeJavaScript` (GH-1185 W3)

Status: draft (revision r6 — C2 "Host serves every `file:` byte" replaced by measured hybrid serving; r3 followed isolated hostile reviews of `571f6bda` and `5ecd3496`)
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
head `1524f3e1`, now `ae64046`, runs `artifacts/runs/gh1185-w3-file-origin-probe-2026-10-28/` and
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

**r6 serving evidence (same device and runtime).** Both runs are in keld-research note 268
(draft PR [0monish/keld-research#38](https://github.com/0monish/keld-research/pull/38), head
`ae64046`). **E8** — P1 serving cost, run `artifacts/runs/gh1185-p1-serving-cost-2026-10-30/`
(first published at `3bae3001`): the draw.io desktop web root (`jgraph/drawio@24b76c2c`
`src/main/webapp`, the entry of drawio-desktop `2edf9fb9`), 19 `file:` requests and
24,119,045 B per load, 30 measured rounds per arm per profile class (cold, warm), metric
`navToReady` = `Navigate()` → the page's double-rAF beacon queued after `load` and after
`js/PostConfig.js` (the last chain script); no preload, so draw.io stops at "Runtime
Environment not found." and its later UI loads are outside the census. **E9** — parser
differential, injected races, root swap, drive-letter rebinding, CSP and hybrid cost, run
`artifacts/runs/gh1185-p2-diff-hybrid-2026-10-30/` with its follow-up. Numbers below are
quoted with their 95 % CIs from the runs' `results.json` and are not strengthened.

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
`Navigate(file:///<admitted root>/<entry>)` and **verifies every `file://` request** of that
window against a session-pinned handle to the admitted app-code root (one parser, one
no-follow walk). It then answers by request kind (**hybrid**, §4.5): a **document**
(`ResourceContext == DOCUMENT`: top-level and frame documents) is **served by the Host from
the verified handle** with Host-owned `Content-Type`/`Content-Length`, the Keld baseline CSP
and app-declared headers; every **other** request is **passed through** after verification
(the Host sets no response and Chromium's file loader reads the verified name); every
refusal is `403`/`404` with an empty body. For the verified name no object outside the root,
no reparse point and no ADS or alias spelling is admitted. Documents are delivered from the
verified object itself; a pass-through subresource is re-opened by Chromium by path, and the
windows between the two opens are stated as residuals with the principals that can use them
(§4.5: in-root race by a root mutation owner, volume-letter shadowing, `.lnk` documents),
not hidden. The r1–r5 design (the Host serves every byte) measured +34–36 % `navToReady`
and is rejected (§4.9, §9). The Host also emits Electron `webContents` navigation events that
are each tied to one WebView2 navigation and one Host-served document, and returns real
settled `executeJavaScript` values whose delivery is scoped to the document generation and
the Bun role generation current at dispatch. Nothing degrades silently: a Host that cannot
provide settled evaluation refuses to start the application with a typed error.

Observable outcome on a real Windows device: an application loaded from `file://` keeps
its genuine `file:///…/index.html` URL (probe: no virtual-host rewrite; `self.origin ==
"null"`); every out-of-root, aliased or malformed `file://` request is answered `403` with
an empty body and the outside sentinel's nonce never appears in the page; every allowed
subresource delivers exactly the file the Host verified (E9); `did-start-
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
  `file://` baseline CSP on Host-served documents (§4.5); T0 updates that sentence and adds
  the §6 ledger lines "Windows `file://` documents: the Host serves app-code bytes as
  currently present in the admitted root; a Legacy role can rewrite them (#1185
  6492237806)" and "Windows `file://` subresources: verified by the Host, then read by
  Chromium by path; residuals: an in-root race open to root mutation owners and
  volume-letter shadowing by a principal that can redefine a DOS device (§4.5)".
- Root `AGENTS.md` "Hot kipc/event-loop/guard paths … no steady-state allocation": the
  `WebResourceRequested` handler runs on the engine event loop and allocates the parser's
  components for every `file:` request and one response buffer per **served document**
  (arm (b), §4.5), and the navigation-signal, eval-settle and header round-trip payloads
  allocate on the same loop. T0 records these as one named, measured exception (not a kipc
  path), bounded by E8/E9 and the B10 instruction wording.
- `crates/keld-wv/AGENTS.md` L5–6 ("platform handles MUST NOT be touched on I/O/pool
  threads"; the only sanctioned pool callback posts via the tao `EventLoopProxy`): §4.5
  fits the design to that rule (a worker touches only a served document's file handle;
  subresource verification runs synchronously on the STA; every COM object —
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
- KEL-66 browser arguments: `keld-wv` ships **empty** `AdditionalBrowserArguments`
  (`webview2/mod.rs:1430-1436`) and the test `environment_options_do_not_disable_smartscreen`
  (`:4853-4875`) asserts that no backend code sets browser arguments. W3 sets exactly
  `--allow-file-access-from-files` on the environment that hosts an admitted `file:` app root
  (§4.5, A22). T0 restates the KEL-66 invariant as "exactly the W3 argument, never
  `--disable-features`" and T2 changes the test in the same PR.

| Existing owner (reuse, no duplicate) | Exactly what this spec consumes | Decision / divergence |
| --- | --- | --- |
| [#1192 GH-542 checked navigation binding](https://github.com/gyldlab/keld/pull/1192) (**open draft**, `cd30f997`) | §4.2 `NativeDocumentPhase`, checked `u32`, +1 on real replacement commit, +1 **before** a trusted Host load (L110), **+0 on renderer provisional start/cancel** (L107) and same-document changes; §4.4 eligibility = verified resource identity; §4.5 "stale evaluation completion … never delivers to a new document"; N15 Bun recovery keeps the document; N16 one generation; N21 Windows UNKNOWN; L145 four bounded CONTROL records per role. | Reused as the only generation and phase model; W3 is its Windows oracle. Page-initiated navigations stay page-attributed (+0 on cancel, +1 at commit); W3 issues no Host load for them, so no #1192 amendment is needed for navigation. W3 needs #1192 T1 as a platform-neutral module (§10 B2). |
| `RoleGeneration` (`role.rs:189`) and the app-link retirement owner (KEL-75/KEL-143) | Per-role generation; retirement of role-bound correlations on recovery. | Eval and header round-trip correlations are keyed on it. |
| [#1190 GH-544 codec](https://github.com/gyldlab/keld/pull/1190) (**open draft**, `115af9ac`) | One Host-opaque codec (V7); V3 synchronous refusal of functions; **V4: accessor properties fail typed with getter counter 0** (accessors are never invoked); V6: "Proxy traps can run before returning"; the 4096-byte bound it claims for `executeJavaScript` results (L9–12). | Reused as the only codec for its domain and decoder ownership. The 4096-byte bound is falsified as an eval-result bound (D4). **V4 is not met by the chosen transport (v)**: CDP deep serialization invokes own enumerable getters and drops symbol/non-enumerable keys (§4.7); whether that is accepted for eval results (Electron's structured clone also runs getters) is the codec owner's decision (§10 B3/Q15). |
| [#1307 W1 preload facade spec](https://github.com/gyldlab/keld/pull/1307) (revised in parallel) | §4.4 Source rule step 3, §4.5 lifecycle (`ContentLoading` mints/revokes; allowed `NavigationId`), §4.6 navigation policy keys (`web.navigation.allow`, `onLoss`), §4.6a loading-path requirement, blocker B6; AC10 (the application document survives a vetoed navigation). | W3 closes B6 with L1. W1's `file:` Source rule and W3 share the one parser (above). W1 `043170ca` L390–394 ("the bytes served for it are that verified resource") must adopt W3's identity: name inside the admitted root, bytes as currently present (owner Q2 in 6491936458) — §10 B7. |
| [#1280 W2 window adapter spec](https://github.com/gyldlab/keld/pull/1280) (gh1188 `efe819e0`, **open draft**) | D2/D3: schema-2 admitted on Windows, `create_app` skipped for facade boots, `HostInitialized` Ready, factored controller build; L54–55: registry windows load an empty document and file-origin `loadURL` is W3's. | The compat boot model W3's A19/AC15 (a)/AC08 (g)(h) depend on is this **target**, not today's code (`app_session.rs:3431-3438` refuses; §10 B13). W3 owns the Windows `HostLoad` seam W2 defers to it; the facade vocabulary stays with #539/#459. |
| [#541 GH-541 spec](https://github.com/gyldlab/keld/pull/1123) and #460/KEL-79 | "no new web.navigate grammar is authorized"; full navigation policy is #460/KEL-79; first-proof eligibility from "boot/resource containment and native navigation evidence". | W3 provides that evidence for `file:` documents and nothing for other schemes (§4.8). The `will-navigate` rule in §4.6 is allowlist-first and needs no new grammar; the allowlist derivation by `keld migrate` is recorded for that owner (§10 B4). |
| [KEL-254 installed-root boot](kel254-windows-installed-root-boot.md) (approved) and `keld_update::ActivePackageSelection` (`crates/keld-update/src/windows_baseline.rs:207-216`: retains `_roots`, `_version: Dir` **and `tree: Dir`**; exposes `tree_root() -> &Path`, `:238`) | Admission of the installed root; AC7/AC8 access profiles; AC13 Legacy refusal. | W3 **lends the existing `tree: Dir` handle** through one borrowing accessor (`tree(&self) -> &Dir`); no reopen by path, no new handle (§4.3). `tree` comes from cap-std `open_dir_nofollow`, whose `dir_options()` shares `READ \| WRITE` without `DELETE` (`cap-primitives-4.0.3/src/windows/fs/dir_utils.rs:62-71`), so it already is the session root pin (A20). |
| `keld-update` cap-std leaf opener `open_pinned_leaf(parent: &Dir, leaf: &str)` (`windows_baseline.rs:589-599`: `read`, `FILE_SHARE_READ`, `FollowSymlinks::No`), used directly by `locate.rs:311, 375` and `windows_baseline.rs:606` (`open_machine_file`, whose callers are `helper.rs:207`, `load.rs:481`, `load.rs:1075`, `load/version_contents.rs:58`, `windows_baseline.rs:668`) | The one existing no-follow leaf open beneath a retained directory handle. | **Generalised**, not duplicated: it becomes the single-component case of `open_regular_beneath` (§4.3) and its callers migrate in the same PR, so "exactly one opener" is true (AC04). |
| cap-primitives 4.0.3 (already a workspace dependency through `keld-update` and `keld-native`): Windows `open_impl` = `manually::open` per-component walk (`src/windows/fs/open_impl.rs:29`); each component opened relative to the directory handle through `CreateFileAtW` → `NtCreateFile` with `RootDirectory = dir` (`create_file_at_w.rs:155-158, 242`), `FILE_OPEN_REPARSE_POINT` under no-follow (`:220`); `..` re-roots to an ambient `CreateFileW` (`open_unchecked.rs:60-105`) — unreachable here because the parser refuses `..` | The relative no-follow open mechanism and its already-reviewed `unsafe`. | Reused through the existing dependency; r1's claim that cap-std's Windows walk is "path-based" was wrong and is withdrawn. Named requirement left to a probe cell: a junction on an **intermediate** component is refused by the walk (AC03 cell; cap-std's `is_symlink` classification of mount points UNVERIFIED). |
| cap-fs-ext 4.0.3 `MetadataExt::nlink` (`metadata_ext.rs:30`), `DirExt::open_dir_nofollow` (`dir_ext.rs:87`) | Link-count read-back; no-follow directory step. | Reused for AC17 and the component walk. |
| `keld-native` `unsupported_reparse` (`crates/keld-native/src/fs.rs:1352-1357`: any reparse attribute that is not a symlink is refused) | KEL-130's reparse-tag policy. | W3 applies the same **"any reparse point at any component → refuse"** rule (symlink, mount point, cloud placeholder, dedup, WOF alike); consequence stated in §4.3. KEL-130 itself is **not** reused (app-grant policy; 8 MiB content cap below the pinned 9.9 MB bundle). |
| `keld-core` dev-stage admission: `validate_windows_dev_stage_acl` (`app_session.rs:1485-1500`) opens the root with `FILE_FLAG_OPEN_REPARSE_POINT` + `FILE_FLAG_BACKUP_SEMANTICS` and share `READ \| WRITE` (**no `FILE_SHARE_DELETE`**, `:1495-1498`), validates the owner-private DACL, then drops the handle; canonical layout `<project>/.keld/dev/<32-lowerhex>` (`:1668-1678`; `keld-cli/src/boot.rs:428`) | The dev-stage DACL policy (KEL-96). | W3 retains that validated handle as the admitted root (`Dir::from_std_file`) for the session; its share mode is the session root pin (A20), so no new handle is opened. The project path may carry `~`, spaces and non-ASCII: the root prefix is validated with `validate_fs_component` (`lib.rs:933-938`), never with the package grammar (§4.4). |
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
E9 observed on 156 that `RequestedSourceKind` is `DOCUMENT` for **every** request (it names the
filter source kind: document vs shared/service worker) and that `ResourceContext` carries the
resource type: `DOCUMENT` (1) for top-level and frame documents, `STYLESHEET` 2, `IMAGE` 3,
`SCRIPT` 6, and `XML_HTTP_REQUEST` 7 for XHR **and** `fetch()` (never `FETCH` 8 on this runtime).
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
| AC01 Loading path (L1) | (i) A schema-1 renderer boot whose descriptor entry is `<root>/index.html` (no query); (ii) **[TARGET, B13]** a compat window whose first document is the application's `loadURL(file:///<root>/index.html?x)` through the W3 Host seam | (i) The Host calls `Navigate(file:///<root>/index.html)` from `navigate_initial`; (ii) the Host calls `Navigate` with the application's URL (entry components, query kept) from the `HostLoad` command; in both the first top-level `ContentLoading` carries `IsErrorPage == FALSE`; `location.href` read via CDP parses to the same root-relative components and query (compared with the exported component equality, never as URL text — Chromium may upper-case the drive letter, and the pinned application itself upper-cases it, `electron.js` L1012); W1's `Source` parses the same way. | Load via `NavigateToString` (`app_session.rs:3653` → `mod.rs:3654`) → `Source = about:blank`, `location.origin == "null"` (W1 AC18) → RED. |
| AC02 `file:*` filter before first navigation (C1) | (i) The schema-1 boot of AC01 (i); (ii) **[TARGET, B13]** a compat window whose first navigation is its first `loadURL`; in both, filter `file:*` + `CONTEXT_ALL` + `DOCUMENT` registered in that webview's controller build before its first `Navigate` | Every `file://` request of the load appears in the WRR log with a Host verdict (**new probe cell**: a `file:*` baseline must reproduce the per-run census of the `*` probe, 68 events per run over its two document loads, 3/3 — the probe used `*`, so its evidence does not transfer until this cell runs; E9 used exactly this filter shape before the first `Navigate` and mediated every corpus request, which is not the census cell); the entry's raw inline script that only runs without the Host CSP header did not run (until P4 settles the production baseline, this oracle uses a **test-only** header policy that blocks the fixture's unnonced inline script), checked for **both** documents of the run (note 268: N2's `indexServedByHost` reflected only the second document). | (i) Register the filter after the first `Navigate` under `file:*` (probe N2 re-run) → the first `index.html` escapes to Chromium's file loader, the raw inline script runs → RED (at-risk 1 per document load). (ii) Register the filter only in `navigate_initial` (so a compat window built without it has none) → the compat window's first `loadURL` document escapes to Chromium's file loader → RED. |
| AC03 Containment matrix (C2 hybrid, C4) | The page issues `fetch`/XHR/`<img>`/`<script>`/`<link>`/`<iframe>`/top-level/`@font-face` requests for the cells below | **Document cells** (`ResourceContext == DOCUMENT`: `<iframe>`, top-level) are **Host-served** (`200`, bytes of the verified handle, Host headers); **subresource cells** are **verified then passed through** (no Host response) and the delivered identity equals the Host-verified identity — E9's pre-registered oracle: byte-exact sha256 for `fetch`/XHR, executed nonce for `<script>`, PNG width for `<img>`, posted nonce for frame/top documents. Allowed spellings: in-root files, mixed-case in-root spellings, drive-letter case, interior `//` (collapsed, §4.4 R6), an in-root **hardlink** to an outside file (owner decision; delivered on both paths). **Parser-refused** (`403`, empty, opener count 0): **plain outside path**, `%2e%2e`, `%5C`, `%2F`, `%00`, UNC/URL-host forms, `file:////?/C:/…`, `name:stream`, `name.`/`name␠`, `CON`/`COM1`, a `~` spelling. **Opener-refused** (`403`, empty, opener count 1, bytes read 0): a junction as a **leaf**, a junction as an **intermediate** component (mixed case too), a file symlink, a **directory symlink** (real-device cells; E9's fixture created both symlinks and the check refused them). **`.lnk`** (E9): a subresource `.lnk` is followed by Chromium as a redirect whose target raises a **new** WRR and is verified on its own (out-of-root target `403`, in-root target passed through); a **document** `.lnk` is served raw by the Host and renders nothing — documented divergence from Electron, which follows it (§4.5 residual 3). Evidence: E9 all-context verify-then-pass-through, 0 parser differentials over 341 cells × 6 enforcing runs (with and without `--allow-file-access-from-files`, with and without a hold chain); hybrid, 0 over 349 cells × 2 runs; both counts **exclude** the injected race cells (AC26 ii); without enforcement Chromium delivered bytes for 42 of the 108 cells the check refuses, so those refusals are load-bearing. The mixed-case served cell is **re-run under (g)** (§4.3). Page nonce 0 for every refused cell, with the nonce oracle validated per request kind. Still not exercised (device cells): 8.3 spellings (E9's volume had no 8.3 names), `sendBeacon`. At-risk sets reported separately: parser cells × runs, opener cells × runs, document vs pass-through cells. Evidence attribution, unchanged: the r1 probe's `url-dotdot`, `url-long` and `url-backslash` rows were Chromium-decided or probe-decided; R3/R4/R6 are proven by the pure goldens (AC05) until re-run against the Host parser with a request that reaches WRR. | (1) **Naive verifier** (raw-string prefix match, no percent-decoding, no R5–R9, Win32 path open that follows reparse points) with pass-through → E9: 11 differential cells (25 with `--allow-file-access-from-files`) delivering out-of-root bytes through a junction, a directory symlink, an ADS, a hardlink and sibling-prefix names → RED. (2) **#1185's control, restated for the hybrid**: pass non-`DOCUMENT` requests through **without** verification (refusals deleted) → the plain-outside `fetch` delivers the sentinel nonce → RED; baseline 0 across the parser set. A weaker mutant (N2) scored 1/1. |
| AC04 One opener (C3) | Any app-root open in `keld-core` boot, `keld-update` locate/helper or `keld-wv` document serving and subresource verification | Exactly one opener exists (`open_regular_beneath`, §4.3); `open_relative_file_windows` is gone; `open_pinned_leaf` is its single-component call; a source test finds no `OpenOptions::open`/`CreateFileW`/`Dir::open_ambient_dir` on app-root or page-controlled input in `keld-core`/`keld-wv`/`keld-update` serving paths. | Replace the opener with a Win32 path open (`CreateFileW` on `root\<components>`), grammar unchanged → the **intermediate-junction** cell (parser passes `junc/x.txt`; only the opener can refuse it) serves the outside sentinel → RED. Separately replace the per-component walk with one multi-component cap-std `open_with` → an **intermediate directory-symlink** cell (relative, in-root target) or a cloud-placeholder intermediate is followed and served under an alias spelling → RED (needs Developer Mode or a cloud placeholder on the device; absent both, this second mutant is pre-declared **rule not exercised** and the receipt says so). One fault each, one deciding rule. The source test exempts exactly one open by path: root admission in `validate_windows_dev_stage_acl`. |
| AC05 One parser; refusal table (§4.4) | Pure goldens in `keld-guard` for every table row plus the probe's 31-cell corpus | Each vector yields the table's refusal or components. The `:` and `~` cells are decided **only** by R8 (`:` is removed from R5 so one rule owns it); the W1 Source rule and the W3 handler call this one function (one definition, two call sites). | Drop R8's `:` refusal → `index.html:hidden` opens (NTFS parses the stream at the NT layer) and the planted ADS nonce is served → RED. Separately drop R8's `~` refusal → a planted `LONGNA~1.JS` spelling serves the long-named file under a second identity → RED (volume with 8.3 disabled → **rule not exercised**, receipt says so). Separately decode twice → `%252e%252e` yields `..` → R6 refuses anyway: **non-discriminating**, listed, not counted. |
| AC06 Served-document correlation (L2) | A top-level navigation `n` to an in-root URL is allowed at `NavigationStarting`; (a) the file exists; (b) the file is absent; (c) an `<iframe>` to a **different** in-root URL loads concurrently with a top-level navigation to an **absent** file | (a) The WRR log holds one `ResourceContext == DOCUMENT` response for `n`'s URI served from `(root, components)`; `ContentLoading(n)` reports `IsErrorPage == FALSE`; the Host reports the document as Host-served with that identity; **page oracle**: `location.href` parses to `n`'s components (component equality) and raw query — never URL-text equality, the drive letter's case may differ — and the body carries the fixture nonce. (b) The Host served `404`/empty for `n`; `ContentLoading(n)` is **not** reported Host-served (W1's epoch holder posts no epoch; page oracle: no fixture nonce); AC09 fires `did-fail-load(-6)`. (c) the frame's served record never attaches to `n`; `n` stays not-Host-served. | Attach by `ResourceContext == DOCUMENT` only, ignoring URI equality → case (c) attaches the frame's record to `n` and an absent top-level document is reported Host-served (W1 would mint an epoch; page oracle shows no nonce) → RED. Pre-declared **rule not exercised** conditions: if W1's eligibility (B7) is not implemented on the device, the epoch-holder counter is absent and only the page oracle counts; if Q7 shows `IsErrorPage == TRUE` for a Host `404`, case (b)'s `IsErrorPage` clause is redundant with the served record and its NC must instead flip the served-record rule. |
| AC07 Signal emission and order (§4.6; Electron v45.1.0 oracle) | (i) In-root load; (ii) off-allowlist page-initiated navigation; (iii) hash change and `pushState`; (iv) absent in-root file; (v) Host-initiated load | The signal log is the system under test; every sequence is **compared to the GH-551 Electron v45.1.0 run of the same fixture** (page oracle in both), not asserted from Host records alone. (i) `did-start-loading`, `did-start-navigation`, `did-navigate`, `dom-ready`, `did-finish-load`, `did-stop-loading`, same `NavigationId`, generation stamped. (ii) `did-start-loading`, `did-start-navigation`, `will-navigate` (notification, §4.6), `did-stop-loading`; **no `did-fail-load`** (Electron suppresses `ERR_ABORTED`), no `did-navigate`, no `dom-ready`; the application document survives (W1 AC10). (iii) `did-navigate-in-page` only; no `will-navigate`; generation unchanged. (iv) `did-fail-load(-6)` exactly once, no `did-finish-load`. (v) no `will-navigate`. Nothing is synthesized from a Bun-side timer. | Emit `did-finish-load` from `ContentLoading` → `did-finish-load` before `dom-ready` → RED. Separately emit `did-fail-load(-3)` on a policy cancel → divergence from the oracle (Electron L2932–2933) → RED. |
| AC08 `will-navigate` (§4.6) | A page-initiated `NEW_DOCUMENT` navigation to (a) an off-allowlist URL, (b) an on-allowlist URL; (c) a `BACK_OR_FORWARD`/`RELOAD` navigation; (d) a redirect hop (`IsRedirected == TRUE`) of a Host attempt; (e) a Host attempt itself | (a) cancelled on the original `NavigationStarting` (+0 generation), `will-navigate(url)` emitted when a listener exists (the `shell.openExternal` pattern keeps working); (b) allowed, `will-navigate(url)` emitted when a listener exists, `preventDefault()` has no effect (documented divergence; `keld migrate` derives the allowlist so this case is empty for a veto-all listener); (c) allowlist evaluated, **no** `will-navigate` until P3 settles whether Electron emits it for script history navigations; (d)/(e) never attributed page-initiated (attribution by the attempt's `NavigationId`); no `will-navigate`. (f) **in-root is not enough and the query counts**: a veto-all fixture (`web.navigation.allow` derived to `none`) whose page contains an in-root link (`other.html`), a link to `index.html?attacker=1` and a link to the entry's exact URL — all three cancelled, page nonce unchanged, a granted IPC call still succeeds afterwards. (g) **[TARGET, B13]** a Host attempt (`loadURL`) to an off-allowlist target → typed refusal at +0, document and IPC untouched; (h) **[TARGET, B13]** a Host attempt to `index.html?x` (entry components, any query) and an `onLoss` reload-entry — triggered by W1's rule (a page-initiated allowed navigation that commits an ineligible document) — under the same `none` allowlist → both admitted; (i) **lifted-literal pass cases**: with `index.html` (no-query constraint) a link to `index.html` proceeds and `index.html?attacker=1` is cancelled; with `p.html?mode=a` links to `p.html?mode=b` and `p.html?mode=A` are cancelled, `p.html?mode=a` proceeds; a query needing escaping (`?q=a%20b`, `?q=%C3%A9`, `?q=%41` vs `?q=A`) compares byte-exact on the engine-reported serialization (device cell); (j) **component equality**: under `in-root:**`, links to `INDEX.HTML`, `index%2Ehtml` and a Turkish dotless-i spelling decide identically in W3's allow match and W1's eligibility (Q17); (k) `location.reload()` of the live document under `none`: the Host cancels it (`RELOAD` kind, page-initiated); Electron's behaviour for a veto-all listener on reload is UNKNOWN until P3 — pre-declared, the cell records both. Generation: +0 on cancel, +1 only when an allowed navigation commits. | (a) Attribute "page-initiated iff `IsUserInitiated == TRUE`" → the application's own `loadURL` (API navigations are user-initiated per Microsoft) emits `will-navigate` while a scripted `location.href` (`FALSE`) emits none — against the GH-551 oracle (`loadURL` emits nothing; scripted emits) → mismatch in both directions → RED; at-risk 1 each per run. (b) Advance the generation at `NavigationStarting` → an off-allowlist click revokes the live document → a page-side IPC round trip fails → RED (W1 AC10). (c) Treat every in-root `file:` target as on-allowlist → (f)'s links replace the document → RED. (d) **Cut the query in the `file:` allow match** → in (i) the `index.html?attacker=1` link replaces the document → RED; separately fold the query through the component equality → `p.html?mode=A` proceeds → RED. (e) Check the allowlist after the Host attempt's +1 → (g) leaves the window FailedClosed → IPC fails → RED. (f′) Drop the entry exemption for Host attempts → (h)'s `onLoss` reload is refused and the window never recovers → RED. (j′) Compare components byte-exact in one consumer (W1 or W3) → `INDEX.HTML` is allowed by one and refused by the other → the page lands FailedClosed and `onLoss` fires → RED. |
| AC09 `did-fail-load` honesty | (i) Absent in-root entry (Host `404`); (ii) parser-accepted, opener-refused entry (Host `403`: directory, reparse); (iii) policy cancel; (iv) renderer error (`NavigationCompleted` failure, other `WebErrorStatus`) | (i) `-6 ERR_FILE_NOT_FOUND`; (ii) `-10 ERR_ACCESS_DENIED` (documented divergence: Electron would list a directory or follow the link); (iii) **nothing**; (iv) the §4.6 table's code; exactly one `did-fail-load` per failed `NavigationId`; `did-stop-loading` follows. Verified against the GH-551 differential oracle (page oracle + Electron run), not against Host records alone. | Map every failure to `-3` → case (i) reports `-3` while the Host served `404` → RED. |
| AC10 Settled eval (E1, E4) | After `did-stop-loading`: (a) `new Promise(r => setTimeout(() => r("v"), 500))`; (b) a `try { … } catch (e) { null }` statement with a string completion value; (c) `Promise.reject(new Error("m"))`; (d) `throw new Error("t")`; (e) under page CSP `script-src 'self'`, an expression whose value is an object with a getter that calls `eval` and sets a marker; (f) same, with a `Proxy` whose `get` trap calls `eval` and sets a marker; (g) the page has patched `Object.getOwnPropertyDescriptor` (and `Reflect.ownKeys`) to call `eval` and set a marker, then returns a plain object; (h) under `script-src 'self'`, the **evaluated code itself** calls `eval("1")` | (a) `"v"` after ~500 ms (probe 3/3); (b) the completion value; (c) rejection carrying `m`; (d) the facade's fixed sync-throw message (#1154 shape, §10 Q3); (e) the getter runs **under the page's own CSP** (deep serialization reads own enumerable properties through `Get` inside `evaluate`, V8 L222/L257): its `eval` throws, marker **not** set, and the throw surfaces as CDP `-32000 "exception during deep serialization"` → typed `EvalFailed` rejection to the caller; (f) `{type:"proxy"}` is reported and the trap never runs (V8 deep serializer L339–341): marker not set — matches Electron's `DataCloneError` class; (g) marker **not** set under (v) — no codegen scope exists; (h) the page's `eval` is refused by its CSP (flag false, V8 L400–402): the rejection names the CSP. | Use `ExecuteScript` (probe N4) → (a) yields `{}` → RED. Separately pass `allowUnsafeEvalBlockedByCSP` default → (h) succeeds → RED. Separately transport through (ii) `callFunctionOn` + encoder → (g)'s marker is set (codegen scope open across the encoder's intrinsic calls, V8 L151/L158/L188) → RED. |
| AC11 Generation-scoped delivery (E2) | (i) Eval dispatched at generation G; replacement before completion; (ii) **real replay control** (#1185's own NC, UNMET by the probe — N6 compared constants): a debug `profile-test-hooks` seam holds the **actual CDP completion payload** of correlation `c` dispatched in document G1, the harness commits G2, then the held payload is delivered to the completion handler as if CDP had just completed; (iii) role G1 awaits an eval, recovers to G2 before completion; (iv) the document enters BFCache or never completes the script | One state machine (`Pending → Retired \| Delivered`, §4.7 step 4), counted in the **Bun facade** (another process): settle frames per correlation ≤ 1, and no payload produced in G₁ observed after G₂ committed — the second assertion is Host-independent: the evaluated script returns a **page-side document nonce** (planted in each fixture document) and the facade compares it with the page oracle's nonce for the committed document, not with any Host stamp. (i) exactly one `DocumentReplaced` frame; CDP `-32000` (probe 3/3) or retirement. (ii) the held G₁ payload produces **no** frame (the entry is `Retired`); the caller observed exactly one frame, the retirement's; at-risk 1 per run. (iii) retired by KEL-75/KEL-143's role retirement (the role axis's existing owner), never delivered to G₂. (iv) one `DocumentReplaced` frame when G changes, in the same STA turn; the entry is freed when its CDP callback fires. | Single faults, each on the one machine: (1) the retire step rejects but leaves the entry `Pending` → the held G₁ payload is delivered → facade sees 2 frames for `c`, the second a G₁ value → RED. (2) the completion handler delivers regardless of state → same observable → RED. (3) the retire step posts no rejection → (iv) hangs the caller (no frame within the test bound) → RED. (4) for (iii): skip the role retirement hook → the G₂ role observes a G₁ frame → RED (pre-declared **rule not exercised** until Q16 confirms that KEL-75/KEL-143's retirement covers eval correlations; if it does not, the hook becomes W3's and the NC counts). |
| AC12 Context binding at dispatch (E4) | `executeJavaScript` issued (a) while the main frame's execution context is known; (b) in the gap between `Runtime.executionContextDestroyed` and the next `executionContextCreated` for the main frame | (a) the call carries the `uniqueContextId` **bound to G at `ContentLoading`** (§4.7 step 1); only `InvalidParams` "uniqueContextId not found" and `-32000` "Execution context was destroyed." map to `DocumentReplaced` (§4.7 step 3); (b) typed `NoDocumentContext` returned to the facade, which waits (bounded) for the Host's per-generation `context-bound` event and re-dispatches; the application sees one settled result in the new document, never a rejection, and **no script ran in the gap**. Non-entry and remote documents are evaluable; only generation equality decides delivery. (c) `executionContextDestroyed`/`executionContextsCleared` for the bound context unbinds the pair (§4.7 step 1), so (b)'s gap exists. Two deterministic hooks (at-risk 1 per run each): **hook 1** holds `ContentLoading` and `executionContextDestroyed` in the Host's queue while the engine already shows the new document (Host still holds `(G₁, ctx₁)`); **hook 2** releases `executionContextDestroyed` but holds `executionContextCreated` and `ContentLoading` (Host holds no pair). The harness dispatches during each hold. Side-effect oracle: the evaluated script writes a marker into `document.title`; the harness reads both documents' titles through CDP. Baseline under hook 2: `NoDocumentContext` → facade waits → hook released → `context-bound` → one result in the new document. A back/forward-cache restore that re-creates `ctx₁` under hook 1 is pre-declared **rule not exercised** until Q14's census says whether 156 raises it. | NC1 (hook 1): drop **only** the `uniqueContextId` parameter → the eval runs in the **new** document (its title carries the marker) and is then rejected `DocumentReplaced` → RED (side effect without delivery); with the parameter the same dispatch returns "uniqueContextId not found" → `DocumentReplaced`, no marker. NC2 (hook 2): drop **only** the unbind on `Destroyed`/`Cleared` (parameter kept) → the Host still dispatches with `(G₁, ctx₁)` → "uniqueContextId not found" → `DocumentReplaced` where the baseline delivers a result → RED. |
| AC13 CDP capability self-test (E5) | Webview create, before the first `Navigate` | Event receivers registered **before** `Runtime.enable`; `Runtime.enable` succeeds; `Runtime.evaluate("1+1", returnByValue)` → `2`; `Runtime.evaluate("Promise.resolve(3)", awaitPromise)` → `3`; one `evaluate` with `serializationOptions: {serialization:"deep"}` returns a `deepSerializedValue`; each within `WEBVIEW2_STARTUP_BOUND`; the self-test does **not** wait for the initial `executionContextCreated` (its arrival before the first `Navigate` is UNVERIFIED; the probe registered receivers after enabling, `main.rs` L1090–1119). Failure → the application does not start, typed `KELD-WV-*`. **Not claimed**: that `allowUnsafeEvalBlockedByCSP:false` is honored (crdtp L124–128; per-runtime cell AC10 h; §10 Q11). Enterprise CDP policy: UNKNOWN (Q11). | Skip the self-test with an injected CDP failure → the framework counter **"application started" = 1** although every later eval rejects → RED (the counter is Keld's own `NavigationReady`/app-started milestone, not any application's behaviour). Separately drop the deep-serialization probe → a runtime without the option starts and every object result arrives as a `RemoteObject` stub → RED. |
| AC14 Eval value domain and bound (E3) | A result inside the codec owner's admitted domain and at the bound; one byte over | Delivered bit-exact / typed `CodecLimit` rejection, never a hang. **Blocked on §10 B3.** | Deliver truncated → RED. |
| AC15 App-declared response headers (S1, documents only) | (a) A compat application registers `onHeadersReceived` inside `app.whenReady()` and then creates its window and calls `loadURL(entry)` (the Electron-correct order; the window's **first** document is that `loadURL`); (b) no listener; (c) the listener's role is confirmed gone; (d) the role is recovering (between the `RoleGeneration` move and the successor's re-registration) and the predecessor had a listener; (d′) same, predecessor had none; (e) a listener whose `filter.urls` also matches the document's `<script>`/`fetch` subresources | (a) **[TARGET, B13: blocked until W2 D2/D3 land on Windows]** one Host→Bun round trip per **served document** (`ResourceContext == DOCUMENT`) **including the window's first** — the registration and the `loadURL` are two records on the one FIFO (a requirement T4/T5 test, §4.5), and no Host navigation precedes them (W2's facade boot: no window at boot, `HostInitialized` Ready); the served headers are the listener's (Host-owned fields excepted); (b) zero round trips; (c) document requests `403`/empty; (d) document requests are **held** behind their deferrals until the successor registers, or until its `listeners-settled` record (posted by the facade after its `whenReady` callbacks drained) arrives without a registration, then served; bounded by the recovery deadline and a count, after which `403`/empty; (d′) with no predecessor listener nothing is held and documents carry the baseline only (as before the loss); (e) **zero** round trips for the pass-through subresources and the listener's headers are **not** applied to them — the recorded divergence from Electron, whose listener sees every response (T1 entry; §4.5). At-risk for (a) = the first DOCUMENT response per run (non-empty by construction). | (a) Process the `loadURL` record before the earlier registration record (FIFO broken) → the first document is served with the baseline CSP only and the app's inline-script policy is not applied (probe N5 class) → RED. (c) Serve on role loss with baseline-only headers → RED. (d) Answer `403` instead of holding → a recoverable window shows a blank document after every Bun restart → RED. (d′) Hold although the predecessor had no listener → requests stall until the recovery bound for no reason → the page oracle sees the document blank past the bound → RED. (e) Round-trip every `file:` request the listener's filter matches → round-trip count > served-document count → RED. |
| AC16 Baseline CSP cannot be relaxed | The listener returns a permissive CSP, or overrides `Content-Type` to `image/svg+xml`/`application/xhtml+xml` | The baseline is emitted as its own header on **every `DOCUMENT`-context response** regardless of `Content-Type`; a remote script the baseline forbids stays blocked; in-root scripts load under a baseline written with `'self'`, nonces or hashes (AC25; cross-directory `'self'` is P4, §10 Q5). Pass-through subresources carry no Host header, so the document's policy is the one enforced on them. | Emit the baseline only for `text/html` → an SVG document served with the app's permissive CSP runs the forbidden script → RED. |
| AC17 Hardlink authority read-back (C4) | The fixture plants ≥ 1 in-root file with `nlink > 1` whose other name is outside the root | (a) served (AC03); (b) for every served file with `nlink > 1`: a write attempt on the linked object under **each admitted role token** (Strict; the Legacy forfeiture printed instead of claimed) is denied (KEL-254 AC8 class) and the per-file DACL read-back shows no role write grant (AC7 extension); (c) the owner's paired control: the Strict role attempts to **create an entry inside the admitted root** and is refused (KEL-254 app-role isolation on Windows). At-risk = multi-link served files; 0 → rule not exercised (fixture MUST plant one). | Grant the Strict role write on the linked object → read-back detects, the control writes → RED. Separately grant it create on the root → (c) succeeds → RED. |
| AC18 Zero bytes on refusal (C5) | AC03's refused cells, elevated ETW Kernel-File session | No open of the outside sentinel by `msedgewebview2.exe`; for parser cells the Host opener count is 0; for opener cells bytes read is 0 (the opener returns a handle to the reparse point itself and reads nothing). ETW NOT RUN → INFERRED-strength, printed. | AC03's NC → ETW shows the renderer's own open → RED. |
| AC19 Non-`file:` requests never reach W3 | The page fetches `https://` (policy-allowed) and loads `data:`/`blob:` | **Structural**: the `file:*` filter never raises WRR for them — new probe cell: `https` request with WRR event count 0 and Host response count 0; the policy owner's rules apply unchanged. | Register `*` instead of `file:*` → the `https` request raises WRR and sits behind the UI thread → RED (count ≠ 0). |
| AC20 P1 performance gate (§9) | Before T2 merges | **Measured for the hybrid (E8, E9; B8 resolved by measurement):** E8 reports the serving arms with the six atoms separated (30 valid samples per arm per class, cold and warm). The chosen design (E9 arm `h`: verify every `file:` request, serve documents with headers, pass subresources through, no-`DELETE` root pin, `--allow-file-access-from-files`) is `navToReady` **+2.40 % [+0.26, +4.62]** vs arm (e) (ratio of medians, n = 60 over two blocks; paired +9.7 ms [+2.7, +18.5]) with its pre-registered negative control passing (+10 ms STA sleep per request → +134.2 ms [129.7, 143.7], predicted [110, 170]): **within 5 % on the pooled estimate; per-block upper bounds exceed 5 % (+6.06 %, +5.36 %) — re-measure at T2.** The `h` document used a permissive `default-src` policy, so a restrictive CSP's effect is not included. T2's receipt re-runs `h` vs `e` on the implementation with the production baseline CSP and keeps the pooled CI upper bound ≤ +5 % or carries a written waiver. Unmeasured: arm (f) (S1 round trip, B13) and the product-boot clock (pre-spawn → beacon). | Single arm, no arm (e), no negative control, or paint measured from `NavigationCompleted` → validator RED. |
| AC21 Receipt | A Windows acceptance claim | Runtime string, OS build, head, admitted-root mode, per-AC counters, at-risk counts per deciding rule, each RED output, "rule not exercised" rows, UNKNOWN cells (§3 AC23). | Missing fields or a zero-failure row without its at-risk count → validator RED. |
| AC22 Non-discriminating controls recorded, not counted | Probe N1 (only a dedicated worker was attempted; shared/service workers untested), N3, N6 (compared constants; no CDP result replayed), the exclusive-lock oracle (empty at-risk set); AC05's double-decode mutant | Each reads **rule not exercised** and contributes nothing to scores; N6's claim is replaced by AC11 (ii). | Count any of them → validator RED. |
| AC23 Coverage cells with UNKNOWN status | favicon (probe `r-favicon` timed out), `<link rel=prefetch>`, `navigator.sendBeacon`, web app manifest, media `Range` (probe media cells used filler bytes; `mediaError 4` proves nothing), `blob:`/`data:` dedicated workers (INFERRED to run under `file://`: `fetch` and `importScripts` of the outside sentinel from inside one), Shared/Service Worker construction attempts, `<embed>`/`<object>` PDF, `<a download>`, `<script type=module crossorigin>` and dynamic `import()` (default Vite/electron-vite output) with the `ResourceContext` each reports (a non-`DOCUMENT` module or worker script is passed through and carries no Host CSP header), and the environment worker filter's survival when the first webview closes (the Host re-registers it on the next live webview; cell: close the first window, open a second, attempt a shared worker) | Each is run as its own cell on the real device and reported served/refused/UNKNOWN with the request kind observed; none is claimed from the probe. | Claim any of them from the probe → validator RED. |
| AC24 Document classification (A7) | One load issuing a top-level document, an `<iframe>`, `<script>`, stylesheet, `<img>`, XHR and `fetch()` to in-root `file:` URLs | WRR log per request: `ResourceContext == DOCUMENT` (1) for the top-level and frame documents → **Host-served** (`200`; the Host's `Content-Type` and CSP observed in `WebResourceResponseReceived`); script 6, stylesheet 2, image 3, XHR **and** `fetch` 7 → **verified, no response**; served count = document count (E9 census: every observed `ctx = 1` served, every other context passed through). | (1) Classify by `RequestedSourceKind == DOCUMENT` (E9: `DOCUMENT` for every request) → every subresource is served → served count = request count (19/19 on the E8 workload) and E8's serving cost returns (+34–36 %) → RED. (2) Treat frame `DOCUMENT`s as subresources (pass them through) → the frame carries no Host CSP header and the AC25 `'none'` fixture's scripts run inside it → RED. |
| AC25 The served document's CSP governs pass-through subresources (A16) | Host-served documents carrying, one per cell, `script-src 'none'`; `script-src 'self'`; `script-src 'nonce-…'`; `img-src 'none'; style-src 'none'`; `script-src file:///<root>/csp/ext.js`; no CSP — each loading same-directory `file:` scripts, an inline script, a nonced inline script, a `data:` script, a stylesheet and an image that pass through; **T2 device cell**: `script-src 'self'` with a script in **another** in-root directory (`index.html` → `js/x.js`) | As E9 `h-csp` and both hybrid corpus runs (every cell passes): `'none'` blocks every script; `'self'` runs the **same-directory** `file:` scripts and blocks inline/`data:`; the nonce policy runs only nonced scripts; `img-src`/`style-src 'none'` block the pass-through image and stylesheet; the exact `file:` host-source **blocks** its own script (fails closed); no CSP runs everything. Pass-through `Content-Type` is Chromium's extension mapping (E9: `.js`/`.mjs` `text/javascript`, `.wasm` `application/wasm`, `.svg` `image/svg+xml`, `.json` `application/json`, no extension `text/plain`). The cross-directory `'self'` cell is UNKNOWN (P4) and is reported, not assumed. | Drop the CSP header from the served document → the `'none'` cell runs its `file:` scripts → RED. |
| AC26 Root swap and in-root race (A20, A23) | (i) After admission the root's **parent** mutation owner renames `<root>` → `<root>.swapped` and plants a junction `<root>` → OUT (look-alike names, other nonces), then the page loads documents and subresources; (ii) **T2 device cell**: after verification and before Chromium's open, a pass-through `<script>` and `fetch` have their leaf replaced by rename and, separately, their directory replaced by a junction to OUT; the same injection on a frame document | (i) The rename fails `ERROR_SHARING_VIOLATION` (32) and every cell matches its verified identity (E9 hybrid with the no-`DELETE` root: 0/9 differential). (ii) **Residual census, not a pass condition** (§4.5 residual 1): the pass-through cells' delivered identity is recorded (INFERRED to be the replacement/OUT, as E9's frame cells under all-context pass-through: 4/4 replacement, 2/4 OUT bytes in every pass-through run); the frame document delivers the verified bytes (served from the handle). | (i) Open the session root handle **with** `FILE_SHARE_DELETE` (r5's `Share::Served` root) → the rename succeeds and every later request is verified in the old root through the handle but read from OUT by path: E9 control 4/9, exactly the pass-through script, `fetch`, XHR and image, persistently (not a race) → RED. (ii) Serve the frame document by path instead of from the verified handle → the frame cell delivers the replacement → RED. |
| AC27 Drive-letter binding (A21) | The project is reached through a `subst` letter `K:` → `…\A`; a separate same-user process rebinds `K:` → `…\B` (look-alike files) while the page loads | (i) Admission from the `K:` spelling is **refused** typed (`KELD-*`, naming the final path to launch from), because the requested drive letter differs from the root handle's final path. (ii) A root spelled by its final path is unaffected by rebinding a spare letter: E9 0/12 — that run's URLs were spelled with the final path's `D:`, so it does **not** test `K:` URLs. Not exercised (pre-declared UNKNOWN): shadowing the **real** volume letter of the final path. E9 `ddd`: an AppContainer and an LPAC child get `ERROR_ACCESS_DENIED` (5) from `DefineDosDeviceW`; a medium-IL same-user child succeeds and the Host's session sees the mapping. | Spell the root URL from the requested literal letter `K:` (admission check removed) → E9: 5 of 12 cells, exactly the pass-through ones, deliver `B`'s bytes → RED. |
| AC28 File-access argument (A22) | The environment carries `--allow-file-access-from-files`; the page (i) `fetch`es/XHRs in-root and refused `file:` URLs; (ii) `fetch`es a cross-origin `https:` URL whose response carries no CORS header | (i) In-root `fetch`/XHR deliver the verified bytes (byte-exact), every `file:` request raises WRR and every refusal holds (`403`, empty; E9 `spec-af`: 0 differentials over 341 cells × 2 runs); (ii) recorded either way (Q19 UNKNOWN; not a pass condition until the policy owner decides). | (1) Drop the argument → in-root `fetch`/XHR fail (`TypeError`/status 0, E9) → RED. (2) Under the argument, pass `file:` requests through unverified → refused cells deliver out-of-root bytes (AC03 NC 1 class: 25 cells under the argument) → RED. |

## 4. Design

### 4.1 Atomic decomposition (decision input)

Evidence classes: FACT, PROBE (run ids `baseline r1–r3`, `n1–n6`), INFERRED, UNKNOWN (named
probe). Security atoms separate identity, authentication, authorization, OS containment,
lifecycle/revocation and evidence provenance; performance atoms are in §9.

| Atom | Owner / boundary | Inputs → outputs | Failure mode | Observable | Evidence | Edges · first falsifier |
| --- | --- | --- | --- | --- | --- | --- |
| A1 Loading path | `keld-core` boot → `keld-wv` `navigate` | admitted root + entry components → `Navigate(file URL)` | string load yields `about:blank` identity | AC01 | FACT W1 source probe; code `:3653`/`:3654` | · AC01 NC |
| A2 Request census and filter scope (coverage, cost) | WebView2 filter normalization + `keld-wv` registration | `file:*` + `CONTEXT_ALL` + `DOCUMENT` per webview, before first `Navigate`; worker kinds once per environment → every `file://` request raises WRR, no other scheme does | late registration; `*` filter puts every remote request on the UI thread | AC02, AC19 | PROBE n2 (under `*`; second document only per note 268); FACT L1307 normalization, `_22` L51/L72 | precondition of A8 · n2 under `file:*`, both documents |
| A3 Admitted root (identity of the root) | `keld-core` boot (dev DACL; KEL-254 selection) | admission → one retained `Dir` | handle dropped and reopened by path | AC04 | FACT dev handle dropped today; `tree: Dir` retained | → A5, A20, A21 · junction swapped in after admission is followed |
| A4 URL → components (authentication of the name) | `keld-guard` parser | URI + `AdmittedRootUrl` → components or refusal | double decode; authority; ADS; `~`; Win32 spellings | AC05 | PROBE parser cells; FACT grammar; FACT Chromium pre-canonicalises `%2e%2e` | independent of A5 for `:`/`~`; redundant for `..`/trailing dot · AC05 NC |
| A5 Open (OS containment) | `keld-update` `open_regular_beneath`, **per-component** cap-std no-follow walk (normative) | `&Dir` + components → regular-file handle + `nlink` | reparse followed (multi-component `open_with` follows intermediate relative symlinks); directory; Win32 name layer | AC03 opener cells, AC04 | FACT cap-primitives relative `NtCreateFile` + `FILE_OPEN_REPARSE_POINT`; PROBE `gh1185-capstd-junction-2026-10-30` (3/3 identical, keld-research note 268): cap-std 4.0.3 refuses an intermediate junction in every form (multi-component path and default `open_dir`: "a path led outside of the filesystem", for out-of-root and in-root targets alike; `open_dir_nofollow`: os error 681), while the ambient `std::fs` negative control reads the out-of-root sentinel through the same junction; directory/file symlinks UNTESTED (creation needs privilege) | ← A3 · AC04 NC |
| A6 Hardlink authority (authorization) | KEL-254; W3 reads back | `nlink` + DACL → served + receipt | namespace write taken as the only authority | AC17 | FACT owner decision + correction | · role write grant |
| A7 Request classification and response contract (hybrid) | `keld-wv` handler | WRR `ResourceContext` → `DOCUMENT` (1): `200` from the verified handle with Host `Content-Type` (one `keld-wv` document table), `Content-Length`, baseline CSP, S1 headers; any other context: verified, **no** response (A23); refusal: `403`/`404` empty | classifying by `RequestedSourceKind` (always `DOCUMENT`: every request served, E8 cost); a document passed through (no Host headers); body on refusal | AC03, AC09, AC16, AC18, AC24 | PROBE E9 census (top-level and frame `ctx = 1`; script 6, stylesheet 2, image 3, XHR and `fetch` 7) | → A16, A23 · AC24 NC |
| A8 Served-document identity | `keld-wv` per-navigation record for **every** top-level navigation (all schemes) | allowed `NavigationStarting(n)` + DOCUMENT WRR with equal parsed URI → `ServedDocument` | URL text as identity; frame record attached to `n` | AC06 | INFERRED timing; UNKNOWN `IsErrorPage` for Host `404` (Q7) | ← A2; → A12 (gate sees all navigations) · AC06 NC |
| A9 Navigation events → facade | `keld-wv` → `AppWindowEvent` → facade | WebView2 events → Electron emission rules | fabricated/reordered; `did-fail-load` on cancel | AC07, AC09 | FACT Electron v45.1.0 L2745–2752, L2872, L2932–2933 | ← A8 · AC07 NC |
| A10 `will-navigate` policy and attribution | `keld-wv` (allowlist on the original event) + facade (notification) + `keld migrate` (allowlist derivation) | `NavigationStarting` → allow/cancel (+0) and notification | allowlist after +1 (revokes live document, W1 AC10); `IsUserInitiated` as initiator; redirect hop of a Host attempt cancelled | AC08 | FACT: no deferral; FACT #1192 L108 (+0 on cancel); UNKNOWN history-kind emission (P3) | edge to #1192 L108–112 and W1 AC10 made explicit: W3 performs no Host load for page navigations · AC08 NC |
| A11 Eval mechanism | `keld-wv` CDP | script → settled value/rejection/destroyed | `ExecuteScript` targeting; codegen scope | AC10 | PROBE n4; FACT V8 L151, L400–402 | · n4 |
| A12 Eval lifecycle/revocation | one `keld-wv` state machine (`Pending → Retired \| Delivered`) over #1192's generation with the main-frame context bound at `ContentLoading`; role axis = KEL-75/KEL-143's retirement | `(G, ctx)` at dispatch → deliver iff the entry is still `Pending` when CDP completes; retire in the STA turn that advances G | stale delivery; eval in the gap runs in the new document; BFCache never completes | AC11, AC12 | PROBE r1–r3 (`-32000` on replacement); **no probe evidence for the replay rule** (N6 compared constants — note 268); FACT CDP out-of-order (L1333) | ← A8 (all schemes) · AC11 (ii) NC; AC12 NC |
| A13 Eval capability (fail loud) | create-time self-test | CDP calls → start / typed refusal | silent per-call rejection; unknown-field acceptance mistaken for honoring | AC13 | FACT crdtp L124–128 | · AC13 NC |
| A14 Eval value domain | #1190 owner (decoder of the deep-serialized form; domain losses) | `deepSerializedValue` bytes ≤ bound → decoded domain | 4096 bound; getters run and symbol/non-enumerable keys dropped (V4 not met); remote objects leaked without `releaseObjectGroup` | AC14, AC10 (e)–(g) | FACT D4; FACT V8 L222/L257, L339–341; FACT `injected-script.cc` L640–648 | B3/Q15 · AC14 NC |
| A15 App response headers (S1) | facade listener + `keld-wv` round trip | registered listener → modified headers on **served documents only**; pass-through subresources carry none (recorded divergence) | pass-through on retirement; second URL matcher; round trip for subresources | AC15 | FACT D3 (source level); INFERRED from the API surface, untested: a request answered with no response cannot carry Host headers | → A16, A19 · AC15 NC |
| A16 Baseline CSP | `web.csp` owner for content; W3 for emission | policy → own header on every served `DOCUMENT` response; Chromium enforces it on that document's pass-through subresources | merged header; type-keyed emission; `file:` host-sources (fail closed) | AC16, AC25 | FACT CSP3; PROBE E9 `h-csp` (`'self'` admits same-directory scripts; an exact `file:` host-source fails closed); cross-directory `'self'` UNKNOWN (P4) | ← A7 · AC16 NC, AC25 NC |
| A17 Scheme boundary | #541/#460 | non-`file:` URI → never raised | `*` filter | AC19 | FACT L1307 | · AC19 NC |
| A18 Evidence provenance | spec + receipts | hashed runs → claims | second runtime; zero-failure without at-risk; probe cells misattributed | AC21–AC23 | FACT one runtime | · validator |
| A19 Registration ordering | `keld-core` control FIFO (**TARGET**: compat windows have no Host-navigated first document once W2 D2/D3 land) + recovery hold | facade registrations sent before `loadURL` apply to that load's first request; during recovery `file:` requests are held until the successor registers or posts `listeners-settled` (bounded; only if the predecessor had a listener) | FIFO broken; a Host-navigated first document before `ready` | AC15 (a), (d), (d′) | FACT: today Windows refuses schema-2 (`app_session.rs:3431-3438` `KELD-CORE-034`; `HostInitialized` is macOS-only); TARGET: W2 gh1188 D2/D3; FACT: Electron apps register after `ready` (draw.io `electron.js` L987–994) | → A15; blocked by B13 · AC15 NC (FIFO order swap) |
| A20 Session root pin (lifecycle) | `keld-core` boot: the retained root `Dir` shares `READ \| WRITE`, never `DELETE` | admitted root handle → rename/delete of the root fails (32); ancestors cannot be renamed while a handle beneath is open | root shared with `DELETE`: the parent's mutation owner swaps the root and pass-through reads OUT persistently | AC26 (i) | FACT both existing handles already lack `DELETE` (dev `app_session.rs:1495-1498`; installed cap-std `dir_options()`); PROBE E9 hybrid 0/9 vs control 4/9 | → A23 · AC26 NC (i) |
| A21 Root drive-letter binding (identity of the root's spelling) | `keld-core` admission + `AdmittedRootUrl` | requested root path + handle final path → root URL spelled from the final path; refusal when the requested drive letter differs | a literal `subst` letter spelled into the URL: a rebind redirects pass-through reads | AC27 | PROBE E9: literal `K:` 5/12 (the pass-through cells); final-path spelling 0/12 (spare-letter rebind; `K:` URLs not tested); real-letter shadowing UNKNOWN; AppContainer/LPAC `DefineDosDeviceW` → 5 | → A23; Strict equivalence #1187 (Q20) · AC27 NC |
| A22 File-access environment argument | `keld-wv` environment creation | `--allow-file-access-from-files` → `file:` → `file:` `fetch`/XHR available under pass-through; WRR still mediates every request | absent: `TypeError`/status 0; present: a universal-access widening (UNKNOWN, Q19) | AC28 | PROBE E9 `spec-af` (every request raised WRR; every refusal held) | · AC28 NC |
| A23 Subresource verify-then-pass-through (OS containment of a name Chromium re-opens) | `keld-wv` handler + the C3 opener (verify only; no bytes read) | URI → verified `(root, components)`, handle closed, no response → Chromium's loader reads the same name by path | parser differential (Chromium resolves a name the Host did not verify); a name mutated between verification and Chromium's open | AC03, AC26 (ii), AC27, AC28 | PROBE E9: 0 differentials 341 cells × 6 enforcing runs and 349 × 2 hybrid (race cells excluded); naive NC 11/25; subresource race INFERRED (E9's 4 race cells were frames: 4/4 replacement, 2/4 OUT, under all-context pass-through) | ← A4, A5, A20, A21 · AC03 NC (1) |

### 4.2 Decisions taken as input (kept; evidence re-checked)

| Decision | Statement | Evidence this spec re-checked |
| --- | --- | --- |
| L1 | Load with `Navigate(file:///<admitted root>/<entry>)`; `NavigateToString` retired for application documents. Closes W1 B6 jointly with #1192/KEL-102. | W1 AC18 (3/3). |
| C2 (r6, owner decision after E8/E9) | **Hybrid.** Every `file://` request raises WRR (filter registered before the first `Navigate`; `file:*`, never `*`) and is verified by the one parser and the one no-follow walk beneath the session-pinned root; `ResourceContext == DOCUMENT` responses are Host-served from the verified handle with Host headers; every other request is passed through after verification; refusals are `403`/`404` empty. No virtual host or custom scheme. | E8: the Host serving every byte costs +34–36 % (rejected, §4.9); E9: verify-then-pass-through 0 parser differentials; hybrid +2.40 % [+0.26, +4.62] pooled (§9). |
| C4 | In-root hardlinks served; `nlink > 1` selects per-file DACL read-back and write-denial per role token; Legacy forfeits root integrity (printed). | 6491936458 + 6492237806. |
| E1/E5 | CDP `Runtime.evaluate` with `awaitPromise`, `allowUnsafeEvalBlockedByCSP:false`, **mandatory** context binding, generation authority as the one `Pending → Retired \| Delivered` machine (captured at dispatch; retired in the turn G changes; delivered only while `Pending`); create-time self-test fails closed. | D2; probe r1–r3 (settle, `-32000`), n4; V8 L400–402. The replay rule has **no** probe evidence (N6 was not a control, note 268) and is proven only by AC11 (ii). |
| S1 | Per-**document** round trip to Bun only when an `onHeadersReceived` listener exists (r6: pass-through subresources carry no app headers — divergence, §4.5); fail closed; `Complete` on the UI thread; separate Keld baseline CSP. **Matcher ownership fixed (r2):** the Electron `filter.urls` match-pattern matcher lives once, in the facade (§4.5). | D3. |
| Owners | One generation owner (#1192), one codec owner (#1190), two name owners (§2). | §2. |

One input decision was contradicted by measurement: C2 as written in r1–r5 (the Host serves
every byte) costs +34–36 % `navToReady` (E8) against a 5 % budget; the owner replaced it with
the hybrid above (r6). r1's own derived decisions that
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
  as its project root, so `__dirname` and the Host agree under junctioned or 8.3-spelled
  ancestors; a root whose **requested drive letter differs** from its final path's (a `subst`
  or other session-defined letter) is **refused at admission** with the final path in the
  fix text (A21, §4.5); UNC/network roots are refused at **admission** by the `AdmittedRootUrl`
  constructor (it requires a drive letter; R3 governs request URLs) and therefore cannot be admitted
  app roots. KEL-254's "single Windows path owner" (`kel254` L187, L200, L237) keeps naming
  `keld-core` boot as the policy owner; the mechanism is `keld-update`'s.
- **Session root pin (A20, r6).** The retained root `Dir` is shared `READ | WRITE` and
  **never `FILE_SHARE_DELETE`**, so while the application runs no principal can rename or
  delete the root (E9: the parent's rename fails `ERROR_SHARING_VIOLATION`, 0/9
  differentials; with `DELETE` shared the swap succeeds and 4/9 cells — exactly the
  pass-through script, `fetch`, XHR and image — read OUT bytes) and no ancestor can be renamed
  while a handle beneath it is open (E9 share-mode table: `ERROR_ACCESS_DENIED`). Both
  existing handles already have this share mode, so no handle is added: the dev stage's
  admission open shares `READ | WRITE` (`app_session.rs:1495-1498`) and the installed `tree`
  comes from cap-std `open_dir_nofollow`, whose `dir_options()` shares `READ | WRITE` "so that
  directories can't be renamed or deleted underneath us" (`cap-primitives-4.0.3/src/windows/
  fs/dir_utils.rs:62-71`). W3 makes that share mode normative (AC26) so a refactor cannot widen
  it. `Share` (Q1) governs **leaf** opens only. **Cost:** the root cannot be renamed or deleted
  while the application runs; files and subdirectories inside it can still be rewritten in
  place. **Conflict check (source, 2026-10-30):** dev-stage cleanup deletes the stage only
  after the staged Host process object is signalled and the attempt Job reports zero
  processes (`keld-host/src/main.rs:280-336` → `WindowsDevStageCleanup::
  wait_and_delete_after_family_exit`, `app_session.rs:1581-1617`, which already retries a
  transient final-directory sharing violation), so the pin is released first — no conflict;
  `keld dev --watch` does not exist (`keld-cli/src/flags.rs:26`), and a future watcher MUST
  rewrite in place, never swap the stage directory; the installed tree's owner already states
  that "neither the version nor a path above it can be renamed or retired while the
  application runs from it" (`windows_baseline.rs:198-201`) — no conflict. No blocker is
  recorded; a future conflicting path is a regression AC26 catches.
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

The parser returns `(RootRelativeComponents, raw_query: Option<String>)`: the query is the
**raw serialized query of the WebView2-reported URI** (not decoded, not normalized), kept
only for the §4.6 allow match and attribution and compared **ordinal byte-exact** there; it
is never identity (W1 AC20). Refusal table (first match decides; query and fragment are cut before R3 and never form
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

**Refusal and response contract (A7, A23).** For a `file:` URI the handler always decides,
and the decision depends on `ResourceContext` (AC24): a **document** (`DOCUMENT`) that the
parser and opener accept is answered `200` with the bytes of the verified handle and Host
headers (`Content-Type` from one `keld-wv` **document** table — apps override it through S1;
`Content-Length` is Host-owned; the baseline CSP); any **other** context that the parser and
opener accept gets **no response** (the handle is closed and Chromium's file loader reads the
name; its `Content-Type` is Chromium's own extension mapping, E9); `404` empty when the
parser accepted and the opener reported absent; `403` empty for every parser refusal and
every other opener refusal, whatever the context. No body on refusal, no `Location`, no
`Range` for served documents until AC23 settles whether media range requests arrive under
`file://`. The document table is a second extension → type map beside Chromium's; its drift
guard is one T2 device test that compares, for every extension in the table, the Host's
document `Content-Type` with the pass-through `Content-Type` of the same file (a difference is
either a recorded divergence or a bug).

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

### 4.5 Serving contract (hybrid), filter scope, threading, and app-declared headers

**Filter scope (A2, A17).** Per webview, **in the one controller build body** (W2's factored
create, gh1188 D2) **before that webview's first `Navigate`** — `navigate_initial` for a
schema-1 boot, the first `loadURL` for a compat window; the same anchor holds for the WRR
handler, `NavigationStarting`, the context tracking and the self-test:
`AddWebResourceRequestedFilterWithRequestSourceKinds(L"file:*", CONTEXT_ALL, DOCUMENT)`.
The filter is matched against the normalized URI with the scheme lowercased (L1307), so
`FILE:`, UNC and host spellings all match; `https:`, `data:` and `blob:` never raise the
event and never touch the UI thread. Worker kinds (`SHARED_WORKER | SERVICE_WORKER`) for
`file:*` are registered **once per environment** on the first live webview (L72) and
re-registered on the next live webview when that one closes (AC23 cell); whether they ever
fire under `file://` is UNKNOWN (only a dedicated worker was attempted; **rule not
exercised**). Never `*`.

**Hybrid serving (A7, A23; owner decision r6 after E8/E9).** Every `file:` request is
verified on the STA by the one parser (§4.4) and the one no-follow walk (§4.3) beneath the
pinned root (A20). Then:

- `ResourceContext == DOCUMENT` (top-level **and** frame documents; WRR carries no frame id,
  and the hybrid needs no top-level/frame distinction — §4.4's correlation makes it):
  **served** from the verified handle with Host headers (threading below).
- Every other context: the handle is closed and **no response is set**; Chromium's file
  loader reads the same URL. E9: 0 parser differentials over 341 cells × 6 enforcing runs
  (pass-through for every context; with and without the file-access argument and a hold
  chain) and 0 over 349 cells × 2 hybrid runs, while without enforcement Chromium delivered
  bytes for 42 of the 108 cells the check refuses. Both counts exclude the injected race
  cells, reported in residual 1.
- `RequestedSourceKind` is **not** the classifier: E9 found it `DOCUMENT` for every request
  (it names the filter's source kind). Classifying by it serves every request and brings back
  E8's cost (AC24 NC 1).

**Environment argument (A22).** The environment that hosts an admitted `file:` app root is
created with `--allow-file-access-from-files`: without it a `file:` page's `fetch`/XHR of a
`file:` URL fails under pass-through (`TypeError`/status 0) although WRR fires; with it every
request still raised WRR and every refusal held (E9 `spec-af`). It is the owner's parity
target with Electron's file-protocol privileges (`GrantFileProtocolExtraPrivileges`; the
Electron source line was not re-read in this revision — T1 pins it). It is environment-wide
(every webview of that environment) and changes KEL-66's empty-argument invariant (§2).
Whether, on 156, the switch also grants a `file:` page universal access to cross-origin
`http(s)` responses is **UNKNOWN** (Q19, AC28 cell ii); if it does, the non-`file:` policy
owner decides (§4.8) before T2 ships the argument.

**Residuals of pass-through, stated plainly** (documents have none of these: they are
delivered from the verified object):

1. **In-root race (verification → Chromium's open).** A principal that can mutate the root
   between the Host's verification and Chromium's open can replace a verified name, including
   with a junction to an outside object; for that interval the reparse-free invariant of §1
   does not hold for that subresource. E9's injected race cells were **frames** under
   all-context pass-through (4/4 delivered the replacement, 2/4 OUT bytes, in every
   pass-through run); no subresource race cell ran, so the subresource window is **INFERRED**
   from the same mechanism (T2 device cell, AC26 ii). Who can: only root mutation owners — the
   user and same-user processes for a dev stage (owner-private DACL), the installer/updater
   for an installed tree, and Legacy roles (which already forfeit root integrity, §2); Strict
   roles cannot write the root (AC17 c). INFERRED: such a principal can already place bytes it
   can read by writing them into the root, so the window adds reach only to objects the Host
   can read and the mutation owner cannot. Option **vh** (hold a share-READ handle chain from
   verification until `WebResourceResponseReceived`) closed the frame window in E9 at
   +3.2 % [−0.3, +6.6] — measured on all-context pass-through (`v`), **not** on the hybrid —
   needs a release path for redirected or failed hops (6 `.lnk` first hops never raised the
   release event) and blocks in-place rewrites during each window; recorded, not adopted
   (§4.9).
2. **Volume-letter shadowing.** Chromium resolves the drive letter on every open in the
   browser process's logon-session DOS-device map; the Host verified through its handle. A
   literal `subst` letter is closed by A21 (refused at admission; E9: with literal `K:` URLs
   5 of 12 cells — exactly the pass-through ones — read the rebound target). Shadowing the
   **real** volume letter of the final path was **not tested** (UNKNOWN). Who can: a same-user
   medium-IL process (E9: its `DefineDosDeviceW` succeeded and the Host's session saw the
   mapping) and Legacy roles; AppContainer and LPAC children got `ERROR_ACCESS_DENIED` (E9).
   Whether every Keld Strict role is AppContainer/LPAC-equivalent for the DOS-device namespace
   is #1187's to state (Q20).
3. **`.lnk` documents.** Chromium's loader (and so Electron) follows a `.lnk` as a redirect;
   under the hybrid a `.lnk` **document** is served raw by the Host and renders nothing (E9);
   a subresource `.lnk` still redirects and each hop is verified on its own. Documented
   divergence (T1).

**Drive-letter binding (A21), scored (0–10, confidence).** The root URL is spelled from the
admitted handle's final path and admission refuses a requested drive letter that differs
from it (§4.3). The open choice is whether to add a per-request check:

| Option | Containment vs defended principals | Cost | One owner / simplicity | Availability | Mean |
| --- | --- | --- | --- | --- | --- |
| **(q0) final-path spelling + admission refusal; no per-request check** | 8 (0.7): closes the literal-letter hole (E9 5/12 → refused at admission); real-letter shadowing stays open to principals that can redefine a device (same-user medium-IL, Legacy; Strict per Q20) | 10 (0.9): one comparison at admission | 9 (0.8) | 10 (0.9) | **9.25** |
| (q1) q0 + per-request `QueryDosDeviceW(letter)` equal to the admission-time device | 8 (0.5): E9 0 differentials after a completed rebind, but it cannot close a rebind between the check and Chromium's open, and real-letter shadowing is untested for it too | 6 (0.3): one more syscall per pass-through request, **unmeasured**, inside a pooled margin whose upper bound is +4.62 % | 6 (0.7): a second per-request root-identity input | 6 (0.8): after a rebind every later request is refused (E9: 8 of 12 cells, no bytes) | 6.5 |

**Decision: (q0).** The principals (q1) would stop either already act on a same-user Host
(INFERRED) or have forfeited root integrity (Legacy), and (q1) is racy and unmeasured. (q1)
becomes required, with an E8-style measurement, if Q20 shows that a Strict role can
redefine the drive letter.

**Threading (A7, A23; fits `keld-wv` AGENTS L5–6).** The handler runs on the engine STA.
Subresource verification is **synchronous on the STA** (parser + walk + attribute read-back;
no deferral, no response object): E9 per-request STA median 218.9 µs, verification p95
302.3 µs; STA occupancy 4.1 ms (`v`) and 6.9 ms (`h`) per load. Only **served documents** take
a deferral, and they use arm (b) (E8's recommendation): a worker reads **from the file handle
only** into a buffer and posts completion through the tao `EventLoopProxy`; the STA creates
the stream and calls `Complete`. Every COM object (args, request, deferral, response, stream)
stays on the STA. Arms (c) and (d) are not adopted: (c) needs an instruction change (§10 B10)
and (d)'s chunked `IStream` is free-threaded — WebView2 called `Read` off the STA (0 of 21,090
reads on it, E8). Contract: one response per served document; a deferral completed exactly
once on the STA or the request answered `403`/empty on the error path; the stream complete
before `Complete`; the buffer bounded by the opened file's `size`, whose admissible maximum
is the package member bound once `keld-pack` names one (§10 Q9). Served bytes cost ~6.1–6.5 ms
per MB inside WebView2's response-body path whatever the arm (E8), so a multi-megabyte
**document** (inline bundles) pays it; pass-through subresources do not. The per-request
allocations are the recorded exception in §2.

**S1 app-declared headers (A15, A19).** The facade forwards `session.webRequest.
onHeadersReceived(filter, listener)` registration to the Host as a control message that
carries only "listener present"; **the Electron match-pattern matcher lives once, in the
facade**. When a listener is present the Host takes the WRR deferral for every **served document**
and asks the role (a pass-through subresource has no Host response that could carry headers —
INFERRED from the API surface, untested — so app headers are **not** applied to subresources:
a recorded divergence from Electron, whose `onHeadersReceived` sees every response; T1 entry,
AC15 e); the facade applies the app's `filter.urls` and replies
"unchanged" or the listener's headers; no listener → no round trip. Host-owned fields
(`Content-Length`, status line) are not overridable; `Content-Type` and the app's CSP are.
No wall-clock deadline while the role is alive: a slow role delays the page, as Electron's
blocking listener would; role retirement (`RoleGeneration` moved) fails every pending
request closed (`403`/empty) **once the role is known to be gone**; between the
`RoleGeneration` move and the successor's registration (or its explicit "no listener"), and
**only when the predecessor role had a listener**, the Host **holds** document requests on the
STA FIFO behind their deferrals instead of refusing them, bounded by the existing recovery
deadline and a count bound; when the bound trips, held requests are answered `403`/empty;
with no predecessor listener nothing is held (no round trip existed before the loss). The round trip is **not** one of #540's four CONTROL records (#1192 L145): it
needs its own bounded record type or a dedicated FIFO on the compat-control channel, with
a stated bound and an overflow rule that prefers holding on the STA FIFO over `403` — owner
decision (§10 B11). **Ordering (A19) — no seal; a TARGET that depends on W2.** The compat
boot model this relies on **does not exist on Windows today**: `app_session.rs:3431-3438`
returns `KELD-CORE-034` "schema 2 is supported on macOS only", `:1340-1351` refuses on
Linux/Windows, `AppReadyTrigger::HostInitialized` is `cfg(macos)` (`:2383` is the macOS
boot; `:4604`, `:4636-4637`), and merged #1286 pins that refusal. W2 (gh1188 `efe819e0`,
D2/D3) plans exactly this: `validate_windows_boot_files` admits `InitialWindow::None`,
`create_app` is skipped for facade boots, `HostInitialized` is widened to Windows and
emitted at the first loop turn, windows are built in-loop from a factored create, and
"registry windows here load an empty document" with file-origin `loadURL` deferred to W3
(gh1188 L54–55; AC "Zero windows at Ready" L137; D2/D3 L468–469). Under that target: a
compat boot creates no window at boot, Bun's `app.whenReady()` resolves on
`HostInitialized`, the application registers `session.webRequest.onHeadersReceived` (the
Electron-correct place; the pinned application does so inside `whenReady().then`,
`electron.js` L987–L994), then creates its `BrowserWindow` and calls `loadURL(file:)`.
**Requirement (not a fact about today's code):** the registration and the `loadURL` MUST
travel as two records on **one FIFO** whose order the Host preserves — today window
commands go through `invokeWindow` (`browser-window.ts` L7–16, L539, L656) and no
`webRequest` or `loadURL` facade exists, so T5/T4 make this ordering a tested property, not
an assumption (AC15 a). **Host seam for `loadURL(file:)` on Windows:** W3 owns the Host
side (as W2 assumes): the `HostLoad { view, url, attempt }` command of §4.10 with the A″
pre-check and the entry exemption; the facade vocabulary (`loadURL`/`reload`/`stop`,
promise semantics) stays with #539/#459. r3's "registrations sealed" record stays
withdrawn (it preceded every Electron-correct registration; after `ready` it would deadlock
`ready` ← Host Ready ← navigation ← seal; `KELD-WV-005` is the renderer hang guard summed
into `WEBVIEW2_STARTUP_BOUND`, not a phase). The `create_app`/`navigate_initial` path
(`app_session.rs:3650-3667`) remains only for schema-1 renderer-descriptor boots; there a
`webRequest` registration is **refused typed** (`KELD-COMPAT-*`, registry owner) because
no Host-navigated first document can be covered, so "baseline only" is enforced, not
assumed. **Recovery:** held as above when the predecessor had a listener; the successor's
"no listener" is the Host-observable fact that its `ready` callbacks drained (the facade
posts `listeners-settled` after `whenReady` resolution) without a registration (AC15 d/d′).
Until W2 T1 lands D2/D3 on Windows, AC15 (a) and AC08 (g)/(h) are **blocked** (§10 B13). The Keld baseline CSP is always emitted as its own header on every
`DOCUMENT`-context response (CSP3: every policy enforced; AC16); its content belongs to arch
03 §2 `web.csp` and its owner; W3 contributes probe P4 for `'self'` under an opaque-origin
`file:` document (§10 Q5). Chromium enforces the served document's policy on its
pass-through subresources (E9, AC25), so baseline and app policies MUST admit `file:`
subresources with `'self'`, nonces or hashes and never with a `file:` host-source, which
Chromium does not match (E9: an exact `file:///<root>/…/ext.js` source blocked that script —
fails closed). E9 showed `'self'` admitting **same-directory** scripts only; cross-directory
in-root `'self'` (`index.html` → `js/app.min.js`, the pinned application's shape) is a T2
device cell (AC25) and stays P4.

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
> manifest matcher. **Components** are compared with the parser's one component equality
> (per-component NFC + `CompareStringOrdinal` ignore-case, the R7 class), which
> `keld-guard` exports and every consumer — allow forms, attribution, W1 eligibility — uses;
> **queries** are the raw serialized query of the engine-reported URI and compare ordinal
> byte-exact (`?mode=Edit` does not admit `?MODE=EDIT`, `%41` does not equal `A`), never
> through the component equality and never as identity. "The window's entry" is the
> components W1 records at commit as the window's entry resource (W1 L434–435).
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
   component equality **and a byte-equal raw query**, the Host's own attempt URI having been
   run through the same serializer as the engine reports — a page `index.html?b` racing the app's
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
   returned components plus the raw query must match an allow form (`none` matches nothing;
   a literal matches equal components and its query constraint — byte-exact raw query, or
   "no query"; `in-root:**` matches any in-root components with any query); components are
   compared with `keld_guard::components_equal` (the
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
   eligibility, rule 5 emits exactly the declared entries — each as a literal form with
   an **any-query** constraint (a third constraint value beside "exact" and "none", so a
   declared page keeps Electron's free in-root query behaviour) — and never `in-root:**`, because W3
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
   or `executionContextsCleared` for the bound `ctx`, and **every member of the one
   G-change set** — page replacement commit (#1192 L108), the Host load's +1 before the
   engine call (L110), that attempt's commit (L111), an uncorrelated replacement (L114) —
   clear the pair; it is complete again only after the next `ContentLoading`/created
   pairing. The same set drives step 4's retirement, so a back/forward-cache restore that
   re-creates `ctx₁` cannot pair with a stale G (the pair was cleared at the commit that
   evicted it; Q14 census). At dispatch the Host uses the bound pair `(G, ctx)`; if
   the pair is incomplete (the gap), the result is a typed `NoDocumentContext` that the
   **facade** converts into a bounded wait for the Host's per-generation **`context-bound`**
   event (not a tight re-dispatch: Electron's own wait returns immediately when the frame is
   not loading, `web-contents.ts:44-45`, which would livelock here), then re-dispatches; the
   application never sees it unless the bound elapses or the window dies. After a Host
   attempt fails or is cancelled (#1192 L112, FailedClosed), the pair is **not rebound** to
   the surviving engine document: evaluations are refused with the typed `DocumentReplaced`
   outcome (not `NoDocumentContext`, so the facade does not wait) until the next admitted
   load binds a new pair (refuse, not rebind — the revoked document has no authority and W1
   applies `onLoss`). Stated divergence: Electron evaluates in the still-current document
   after a failed `loadURL`; Keld refuses until recovery (T1 entry). An eval whose `G` is old but whose `ctx` is new cannot be formed, so no
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
   in **every** STA turn of step 1's one G-change set — page replacement commit (#1192
   L108), the Host load's "+1 before the engine call" (L110), that attempt's commit (L111),
   an uncorrelated replacement (L114) — every `Pending` correlation whose `G` **differs from
   the new `G`** becomes `Retired` and **one** rejection (`DocumentReplaced`) is posted for
   it (a G₁ entry can therefore never outlive G₂→G₃); on `Exhausted` and `Destroy`, where
   `G` does not change, **every** `Pending` entry is retired; an attempt fail/cancel (L112)
   changes no `G` and retires nothing, but the pair stays unbound (step 1: refuse, not
   rebind).
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
and `maxDepth` defaults to `kMaxInt` (`v8-runtime-agent-impl.cc` L301) — hence the
explicit `maxDepth` in step 3. Electron's structured clone also runs getters, so this is a
fidelity argument the codec owner weighs (B3/Q15), not a W3 waiver of V4. The codec owner
decides the decoder (deep-serialized → the #1190
decoded domain), the domain gaps to accept (typed arrays/ArrayBuffer/Error type-only) and a
bound tied to `MAX_FRAME_LEN` (16 MiB) rather than 4096 bytes (§10 B3). AC10(g) is the
falsifier that separates (v) from (ii). Until recorded, AC14 is blocked.

**Capability self-test (A13, AC13)** runs in `create` after the controller exists and before
that webview's first `Navigate` (W2's factored controller build; `navigate_initial` for
schema-1, the first `loadURL` for compat windows), inside the `GuardInstalled` mint, within
`WEBVIEW2_STARTUP_BOUND`:
receivers registered, `Runtime.enable`, `1+1` by value, `Promise.resolve(3)` awaited, and
one `evaluate` with `serializationOptions: { serialization: "deep" }` whose result **must
contain `deepSerializedValue`** (a runtime that dropped the option would silently fall back,
crdtp L124–128; this is the only way the self-test can see it), and one **negative probe**:
`evaluate` with a **well-formed but unknown** `uniqueContextId` (a syntactically valid
`V8DebuggerId` that no context owns; a malformed string would instead yield "invalid
uniqueContextId", L227–228) must fail `InvalidParams` "uniqueContextId not found" (L232) — the binding AC12 depends on is thereby shown to be honored, not ignored. It
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
| Veto-only for **documents** (Chromium loads allowed document bytes) | A pass-through document cannot carry the baseline CSP or S1 headers (no response object; INFERRED from the API surface) and would sit in the race window. For subresources veto-then-load is the adopted design: E9 measured 0 parser differentials, and the residuals are stated in §4.5. |
| Host serves every `file:` byte (r1–r5 C2) | E8: every serving arm regresses `navToReady` ~+34–36 % vs Chromium's loader, CIs within [+28, +42] %, cold and warm (cold medians e 452.1 ms, b 612.9 ms); the cost sits in WebView2's response-body path (~6.1–6.5 ms/MB for arms a/b/c/d alike), not in the Host (arm p, WRR + walk-open without a response: +0.34 % [−4.5, +5.2]). Over the 5 % budget without a waiver. |
| Verify-then-pass-through for every context, documents included | Cheapest (+0.49 % [−2.4, +4.3]; follow-up pair +2.67 % [−0.04, +4.75]) but documents would lose the baseline CSP and S1 headers and sit in the race window. |
| `vh` hold chain (share-READ handle chain until `WebResourceResponseReceived`) | +3.2 % [−0.3, +6.6] measured on `v`, not on the hybrid; needs a release path for redirected/failed hops; blocks in-place rewrites during each window; closes only a window open to root mutation owners (§4.5 residual 1). Re-opened if T2's subresource race cell (AC26 ii) shows reach a mutation owner otherwise lacks. |
| Root handle sharing `DELETE` (r5 `Share::Served` root) | Persistent whole-root swap for pass-through subresources (E9 control: 4/9, exactly script, `fetch`, XHR, image). |
| Root URL spelled with a requested `subst` letter | E9: 5 of 12 cells (the pass-through ones) read the rebound target. |
| Per-request `QueryDosDeviceW` check | Scored in §4.5 (6.5 vs 9.25): racy, unmeasured per-request cost, every request refused after a rebind; required only if Q20 says Strict can redefine a device. |
| Classify documents by `RequestedSourceKind` | `DOCUMENT` for every request (E9); serves everything (E8 cost). |
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
  signals, listener registration, the facade's `listeners-settled` record (posted after its
  `whenReady` callbacks drained; recovery hold trigger), the Host's per-generation
  `context-bound` event, and the Windows `HostLoad` (`loadURL`/`reload`) command. Wire gate.
- **Platform:** Windows only; macOS/Linux implement the `eval` signature with native
  completion handlers and no W3 generation semantics (§10 Q4).
- **Runtime seam:** `create` registers, inside the `GuardInstalled` mint and before
  that webview's first `Navigate` (in W2's factored controller build; `navigate_initial` for
  schema-1, the first `loadURL` for compat windows): `file:*` filter + handler, `NavigationStarting` policy/attribution,
  `ContentLoading`, `DOMContentLoaded`, `SourceChanged`, `HistoryChanged`,
  `NavigationCompleted`, `WebResourceResponseReceived`, `Runtime.enable` + context tracking,
  the self-test. Handlers live for the WebView lifetime on the STA; renderer crash ≠ Host
  crash; the admitted root `Dir` (shared without `DELETE`, A20) lives for the session and is
  dropped after the webview; the environment carries `--allow-file-access-from-files` (A22);
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
  `did-fail-load(-10)` for opener refusals, separate baseline CSP, hybrid serving
  (subresources read by Chromium after verification; app headers not applied to them;
  `.lnk` documents served raw; `file:` CSP host-sources unsupported;
  `--allow-file-access-from-files`), remote `did-navigate`
  status ordering. Blocks on T0.
- [ ] **T2 — one opener, admitted root, loading path, containment** (Windows W3 implementer):
  parser + goldens (AC05), `open_regular_beneath` + `tree()` + `open_pinned_leaf` migration
  (AC04), root retention and boot migration, `file:*` filter/handler and `Navigate` loading
  (AC01–AC04, AC06, AC17–AC19, AC22–AC28), hybrid classification, session root pin,
  drive-letter admission and the environment argument, document arm (b). Blocks on the T2
  re-measurement of AC20, Q19,
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
| `wv2_file_origin_matrix` (device, ignored-by-default, STA harness as `media_acceptance`) | AC01–AC03, AC06, AC17–AC19, AC22–AC28 | `file:*` baseline + N2; plain-outside and `https` cells; per-rule at-risk counters; E9's identity oracle per kind (document vs pass-through); `ResourceContext` census; naive-verifier and unverified-pass-through mutants; whole-root swap with and without `DELETE`; `subst` rebind; CSP cells incl. cross-directory `'self'`; subresource race cell; file-access argument on/off and the cross-origin cell; ETW when elevated. |
| `wv2_navigation_signals` (device) | AC07–AC09 | Loads (i)–(v); page oracle; GH-551 differential against the same fixture in Electron v45.1.0; mutants per row. |
| `wv2_settled_eval` (device) | AC10–AC13 | Promise/statement/reject/throw/getter/Proxy cells; gap dispatch; replayed G1; role recovery via the existing `no_flag/windows/recovery.rs` harness; injected CDP failure. |
| `wv2_headers_received` (device) | AC15, AC16 | Listener before/after `loadURL` (ordering), absent, retired; subresource-matching listener (zero round trips); SVG/XHTML `Content-Type` override; forbidden-remote-script counter. |
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
  `GetDevToolsProtocolEventReceiver`, `NavigationKind`, the environment's
  `AdditionalBrowserArguments` setter) with local proofs and an issue-scoped
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
- **permission model — mandatory, independent reviewer:** two name owners; refusal table; hybrid classification and verify-then-pass-through with its
  stated residuals; session root pin; drive-letter admission; `--allow-file-access-from-files`;
  allowlist-first cancel with +0; served-document identity; hardlink read-back; separate
  baseline CSP; fail-closed headers; eval context + generation gates; allowlist derivation.
- **dependency addition:** none.
- **wire protocol — #539/#540 owner:** control-plane kinds; the headers round-trip record (B11).

## 9. Perf impact — P1 measured (B8 resolved by measurement)

Budget: architecture 01 §5 cold start → first paint; > 5 % against Chromium's own file
loader (arm e) needs a written waiver. Metric used by E8/E9: `navToReady` (`Navigate()` → the
page's double-rAF beacon after `load` and the last chain script). E8's first paint is the
"Loading..." spinner before the large scripts (+0–8 ms on 68–80 ms) and is not
decision-grade.

| Atom | E8/E9 result |
| --- | --- |
| Census | E8 workload: 19 `file:` requests and 24,119,045 B per load (18 files; `app.min.js` 9,865,843 B, `stencils.min.js` 7,688,864 B), one document, 0 non-`file:` requests raised; no preload, so draw.io's later UI loads are outside it |
| Work | Host STA work per load in the serving arms: a 20.1, b 9.2, c 4.7, d 7.9 ms (cold medians); verification 200.7 µs median / 302.3 µs p95 per request (E9 `v`) |
| Queue/copy | served bytes cost ~6.1–6.5 ms/MB inside WebView2's response-body path (r² ≥ 0.99; arms a/b/c/d/al alike, so not threading, Host buffering or `Content-Length`); arm a's `SHCreateMemStream` copy on the STA; arm d's `IStream` read off the STA |
| Clock | `Instant` (QPC), `Navigate()` → beacon; the product-boot pre-spawn clock is **unmeasured** |
| Statistic | median, p95, percentile bootstrap CI (10,000 resamples) of the ratio of medians and of paired differences; 2 warm-up + 30 measured rounds per arm per class, arms rotated per round; E9 pools two independent blocks (n = 60); idle 96–97 % and no `rustc`/`cargo` during runs |
| Artifact | keld-research note 268, runs `gh1185-p1-serving-cost-2026-10-30` (E8) and `gh1185-p2-diff-hybrid-2026-10-30` (E9), PR #38 head `ae64046` |

| Arm | `navToReady` vs e (ratio of medians, 95 % CI) | Verdict |
| --- | --- | --- |
| e Chromium file loader | cold median 452.1 ms [436.1, 470.3] | baseline, not shippable |
| a/b/c/d Host serves every byte (sync; deferral + worker read; worker stream; chunked) | cold +34.2 to +36.1 %, warm +34.4 to +35.3 %, CIs within [+28, +42] %; b 612.9 ms, +35.6 % [+29.5, +42.1] | rejected (§4.9) |
| p WRR + walk-open, no response | +0.34 % [−4.5, +5.2] cold; +0.45 % [−2.5, +2.8] warm | attribution: dispatch and the Host open are not the cost |
| v verify-then-pass-through, every context | +0.49 % [−2.4, +4.3] (first pair); +2.67 % [−0.04, +4.75] (follow-up pair) | rejected for documents (§4.9) |
| vh = v + hold chain | +3.2 % [−0.3, +6.6] (measured on v, not on h) | recorded, not adopted |
| **h hybrid** (v + documents served with headers + `--allow-file-access-from-files` + no-`DELETE` root) | **+2.40 % [+0.26, +4.62]** pooled (paired +9.7 ms [+2.7, +18.5]); blocks +3.06 % [+0.23, +6.06] and +0.39 % [−1.86, +5.36] | **chosen: within 5 % on the pooled estimate; per-block upper bounds exceed 5 % — re-measure at T2** (AC20) |
| hn NC = h + 10 ms STA sleep per request | hn − h +134.2 ms [129.7, 143.7], pre-registered [110, 170] | passed: the harness detects a per-request STA cost |
| g `Runtime.enable` from create (E1) | additive and serving-independent: +164 to +178 ms (gb − b, ge − e); ge vs e +39.9 % [+34.9, +43.8] | **over budget: B14** |

Limits the runs state: arm (f) (S1 round trip) was not run (needs a live compat role, B13);
E8's served responses carried `Content-Type` only and `h`'s served document used a permissive
`default-src` policy, so neither number includes a restrictive CSP's effect; one runtime, one
device; one document per load in the workload, so the per-document serving cost (~6.1–6.5
ms/MB) matters for applications whose HTML documents are large. B8 is resolved by measurement
for the hybrid; the T2 re-measurement, arm (f) and the restrictive-CSP effect remain.

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
- **B8 P1 report** — **resolved by measurement (r6)**: E8/E9 (§9); the hybrid is within 5 % on
  the pooled estimate. Remaining under AC20: the T2 re-measurement (per-block upper bounds
  exceed 5 %), arm (f) and the restrictive-CSP effect.
- **B9 Probe evidence published** — Windows operator: keld-research note 268 (draft PR
  0monish/keld-research#38, head `ae64046`; runs `gh1185-w3-file-origin-probe-2026-10-28`,
  `gh1185-capstd-junction-2026-10-30`, `gh1185-p1-serving-cost-2026-10-30`,
  `gh1185-p2-diff-hybrid-2026-10-30`) merged, so every citation here resolves to a public path
  on the default branch. Check: PR merged; note path stable.
- **B10 `keld-wv` threading instruction** — instruction owner through `.agents/instructions.md`:
  `crates/keld-wv/AGENTS.md` L5–6 must state that a worker may read a **file** handle while
  every COM/platform handle stays on the STA (the served-document arm (b) and arm (f) depend on that reading; this
  spec interprets the current text and cannot rely on its own interpretation), and that the
  per-request response, navigation-signal, eval-settle and header round-trip payloads are the
  recorded allocation exception; arm (c) is not adopted (§4.5), so no further sentence is needed.
- **B11 Headers round trip and eval capacity** — #540 owner: a bounded record type or FIFO
  outside the four CONTROL records (#1192 L145) with its bound and an overflow rule that
  holds on the STA FIFO rather than answering `403`; the `MAX_PENDING_EVALS` bound and its
  typed refusal (§4.7 step 4).
- **B12 `file:` allow forms** — #541 owner (manifest grammar): the three forms of the A″ rule
  (`entry`, literal in-root component path, `in-root:**`), matched on parser components by
  the new `keld-guard` navigation entrypoint (§8), never by `path_in_scope`; the writer is
  `keld migrate` (union merge, developer entries preserved); non-entry in-root eligibility
  decided with B7 (v). Check: #541 revision names the forms and the entrypoint.
- **B13 W2 schema-2 Windows boot (TARGET dependency)** — W2 owner (gh1188 `efe819e0` D2/D3,
  #1280): `validate_windows_boot_files` admits `InitialWindow::None`, `create_app` is skipped
  for facade boots, `HostInitialized` Ready on Windows, windows built in the factored
  controller build. Today `app_session.rs:3431-3438` returns `KELD-CORE-034` and #1286 pins
  it. Until W2 T1 lands: AC01 (ii), AC02 (ii), AC08 (g)/(h) and AC15 (a) are **blocked**,
  the Windows `loadURL(file:)` Host seam (`HostLoad`, W3-owned; facade vocabulary #539/#459)
  has no caller, and the schema-1 path is the only live one. Check: W2 T1 merged; a compat
  boot on Windows reaches `ready` with zero windows.
- **B14 `Runtime.enable` cost (E1, A12)** — @amishabenramani with the eval owners (#539,
  GH-544): E8 measured `Runtime.enable` from create as an additive, serving-independent
  +164 to +178 ms (`ge` vs `e` +39.9 % [+34.9, +43.8]), concentrated in the parse-heavy bundle
  segments; §4.7 step 1 keeps it on from create for context binding. Before T3: a written
  waiver (architecture 01 §5) or a design change (for example enabling it lazily, which
  reopens A12/AC12) with an E8-style re-run. Check: the decision recorded on #1185.

**Open questions:**

1. **Q1 (owner):** `Share::Served` = `READ | WRITE | DELETE` (recommended) or `READ | DELETE`,
   for **leaf** opens (served documents, subresource verification) only; the root pin never
   shares `DELETE` (A20).
2. **Q2 (owner):** dev stage validated by the package grammar at `keld dev` time so R8/R9
   refusals surface before serving (recommended, with a diagnostic naming the path).
3. **Q3 (#1154):** Electron error texts for sync throw / replacement; facade's pinned major.
4. **Q4 (#542/KEL-142):** `eval` destination signature on macOS now with a typed "not owned"
   refusal (recommended) or native completion semantics.
5. **Q5 (P4) partly answered:** E9 showed `'self'` admitting same-directory `file:` scripts and
   an exact `file:` host-source failing closed under an opaque-origin `file:` document;
   cross-directory in-root `'self'` (AC25 T2 cell) and the production baseline grammar remain
   open.
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
12. **Q12 UNKNOWN:** 8.3 generation state of the qualification volumes (E9's volume had no 8.3
    names, so 8.3 alias resolution was not exercised); case-sensitive
    directory admission check at boot (`FILE_CASE_SENSITIVE_INFO`).
13. **Q13 (P5):** one executed Electron 44 receipt that `onHeadersReceived` fires for `file://`.
14. **Q14 UNKNOWN:** relative order of CDP `executionContextCreated` and WebView2
    `ContentLoading` for the same document (two clocks, §4.7 step 1); 30 loads, both orders
    recorded; the `(G, ctx)` binding tolerates either, the receipt reports the census.
15. **Q15 (codec owner, with B3):** the decoder from CDP's deep-serialized form to the #1190
    decoded domain lives in the facade; which departures from V4/V6 are accepted for eval
    results — own enumerable getters **run** (V8 L222/L257), symbol-keyed and
    non-enumerable keys silently dropped, cycles/aliases represented, type-only kinds
    (typed arrays, `ArrayBuffer`, `Error` fields) — and which are typed refusals; the
    `maxDepth` value (default `kMaxInt`, `v8-runtime-agent-impl.cc` L301).
16. **Q16 (KEL-75/KEL-143 owner):** confirm that role retirement already retires eval
    correlations keyed on `RoleGeneration` (AC11 iii) so W3 adds no second role-axis guard;
    until confirmed AC11 NC(4) is pre-declared "rule not exercised".
17. **Q17 (keld-guard, with B12):** the exported component equality is `CompareStringOrdinal`
    ignore-case over NFC components; whether it diverges from NTFS's per-volume `$UpCase`
    table for any admitted spelling is INFERRED-equal and is a golden (AC05) + device cell
    (AC08 j: `INDEX.HTML`, `index%2Ehtml`, a Turkish dotless-i spelling, decided identically
    by W3's allow match and W1's eligibility; NC j′ byte-compares in one consumer → RED)
    rather than a claim.
18. **Q18 (P6) INFERRED:** the create-time CSP-flag probe (`<meta>` `script-src 'none'` injected
    into the initial `about:blank`, then `eval('1')` with the flag false must throw); if it works
    on the device it becomes part of AC13 and Q11's residual shrinks to runtimes that ignore
    the meta policy on `about:blank`.
19. **Q19 UNKNOWN (A22):** whether `--allow-file-access-from-files` on WebView2 156 also lets a
    `file:` page read cross-origin `http(s)` responses without CORS (universal access); AC28
    cell (ii) records it; if it does, the non-`file:` policy owner (§4.8) decides before T2
    ships the argument.
20. **Q20 (#1187):** whether every Keld Strict role on Windows is AppContainer/LPAC-equivalent
    for the DOS-device namespace (E9: AppContainer and LPAC `DefineDosDeviceW` →
    `ERROR_ACCESS_DENIED`; a medium-IL same-user child succeeds). If not, the per-request
    `QueryDosDeviceW` check (§4.5, q1) becomes required with a measured cost.

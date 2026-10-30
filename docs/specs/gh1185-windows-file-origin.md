# Spec: Windows WebView2 `file://` navigation, app-code resource containment, honest navigation signals and settled `executeJavaScript` (GH-1185 W3)

Status: draft (revision r10 — **hso** serving; pass-through holds released only at document retirement or webview close, bound 256, `keld dev` Host-serves everything (`hsoz`/`hsa`, E13/E14); loader-policy check (Q21 resolved, B16 widened) adds production `unsafe` registry calls; AC20 probe gates failed and are carried to T2 with an A/A arm; r9 → r10 after E13/E14; r8 → r9 after measured hold cost and release path; r3 followed isolated hostile reviews of `571f6bda` and `5ecd3496`)
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
head `1524f3e1`, now `3cd5a68`, runs `artifacts/runs/gh1185-w3-file-origin-probe-2026-10-28/` and
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

**r6/r7 serving evidence (same device and runtime).** All runs are in keld-research note 268
(draft PR [0monish/keld-research#38](https://github.com/0monish/keld-research/pull/38)).
**E8** — P1 serving cost, run `artifacts/runs/gh1185-p1-serving-cost-2026-10-30/` (first
published at `3bae3001`): the draw.io desktop web root (`jgraph/drawio@24b76c2c`
`src/main/webapp`, the entry of drawio-desktop `2edf9fb9`), 19 `file:` requests and
24,119,045 B per load, 30 measured rounds per arm per profile class (cold, warm), metric
`navToReady` = `Navigate()` → the page's double-rAF beacon queued after `load` and after
`js/PostConfig.js` (the last chain script); no preload, so draw.io stops at "Runtime
Environment not found." and its later UI loads are outside the census. **E9** — parser
differential, injected races, root swap, drive-letter rebinding, CSP and hybrid cost, run
`artifacts/runs/gh1185-p2-diff-hybrid-2026-10-30/` (PR head `ae64046`) with its follow-up;
E9's hybrid arm `h` and its CSP cells ran **with** `--allow-file-access-from-files`. **E10** —
follow-up 2, run `gh1185-p2-hs-2026-10-30`: the browser-switch side-effect matrix (no switch,
`--allow-file-access-from-files`, `--allow-universal-access-from-files`, both; under
all-context pass-through, `h` and `hs`; filter source kinds `DOCUMENT` and `ALL`), the `hs`
corpus (2 × 349 cells, no switch) and the `hs` cost blocks (n = 60) (PR head `e0985d0`).
**E11** — follow-up 3, run `gh1185-p2-cells-2026-10-30`: `blob:`/`data:` workers (B15),
cross-directory `'self'`, the injected pass-through race with and without a hold, CORS-mode
subresources with the module-serving cost (257 ES modules, 2,930,290 B), browser-argument
injection through `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`, and a pass-through parser corpus
(99 forms × {script, stylesheet, image, `@font-face`}). **E12** — follow-ups 4 and 5, run
`gh1185-p2-hold-2026-10-30` (`results.json` sha256 `1d601afe…` and `241c3111…`): the cost and
release path of the verification-handle hold with `WebResourceResponseReceived` always
registered (`hsoh`) and registered only while a hold is outstanding (`hsod`). **E13** — follow-up
6, run `gh1185-p2-vite-q21-2026-10-30` (`results.json` sha256 `d1cf28e0…`): r9's `hsodr` on a
real Vite 8.3.5 application, re-registration correctness, and the Q21 classification of
loader overrides. **E14** — follow-ups 7 and 8, run `gh1185-p2-hsoz-2026-10-30` (`results.json`
sha256 `06afcabc…` and `3957116b…`): `hsoz` cost and correctness, `hsa`, and a pre-registered
four-block replication with an A/A arm. Numbers below are quoted
with their 95 % CIs from the runs' `results.json` and are not strengthened.

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
no-follow walk). It then answers by request kind (**hso**, §4.5). **Host-served** from the
verified handle, with Host-owned `Content-Type`/`Content-Length` and app-declared headers
(documents also with the Keld baseline CSP): every document (`ResourceContext` 1: top-level,
frame, `<object>`, `<embed>`), every `fetch`/XHR (7; 8 should a runtime report it) and every
**CORS-mode load** — a request whose `WebResourceRequested` headers carry `Origin` (module
scripts, `import()`, `modulepreload`, `crossorigin` scripts, images, stylesheets and font
preloads). **Verified and held**: every other request (classic script, stylesheet, image,
font, media, other) — the Host keeps the verification handle open (share `READ` on the leaf,
no-`DELETE` handles on every directory component), sets no response, lets Chromium's loader
open the same pinned object, and releases the hold only at document retirement or webview
close (at most 256 holds per webview; beyond that the Host serves); under `keld dev` the Host
serves every `file:` request. Every refusal is `403`/`404` with an
empty body. Keld is the **sole owner of browser arguments**: it sets none, its `keld dev`
launcher removes `WEBVIEW2_*` variables from the Host's environment, `keld-wv` refuses to
create the WebView2 environment while any such variable is present, and the Host refuses to start when a
WebView2 loader policy that injects arguments or replaces the profile is set, or a user-scoped one selects the runtime (A22), because
`--allow-file-access-from-files` lets workers read outside the root without raising
`WebResourceRequested` and an inherited `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` injects it
(E10, E11). For the verified name no object outside the root, no reparse point, no shell link
(`.lnk`) and no ADS or alias spelling is admitted; CSP is enforced but is **not** the
containment boundary (§4.5). The residuals that remain are stated with the principals that can
use them (§4.5). Rejected: r1–r5 (Host serves every byte, +34–36 % `navToReady`), r6 (needs the
switch), r7 `hs` (module scripts and CORS-mode loads fail, and pass-through raced) (§4.9, §9).
The Host also emits Electron `webContents` navigation events that are each tied to one
WebView2 navigation and one Host-served document, and returns real settled
`executeJavaScript` values whose delivery is scoped to the document generation and the Bun
role generation current at dispatch. Nothing degrades silently: a Host that cannot provide
settled evaluation refuses to start the application with a typed error.

Observable outcome on a real Windows device: an application loaded from `file://` keeps
its genuine `file:///…/index.html` URL (probe: no virtual-host rewrite; `self.origin ==
"null"`, re-measured in E10's no-switch rows under every host mode, with `location.origin ==
"file://"`); every out-of-root, aliased or malformed `file://` request is answered `403` with
an empty body and the outside sentinel's nonce never appears in the page; every allowed
request delivers exactly the file the Host verified (E9–E11); a `file:` worker cannot be
constructed from a `file:` document (`SecurityError`, as in plain WebView2 — a divergence
from Electron, §4.5); `did-start-
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
  the §6 ledger lines "Windows `file://` documents, `fetch`/XHR and CORS-mode loads: the Host
  serves app-code bytes as currently present in the admitted root; a Legacy role can rewrite
  them (#1185 6492237806)", "Windows `file://` classic script, stylesheet, image, font and
  media loads (installed launches; `keld dev` Host-serves them): verified and held by the Host,
  then read by Chromium from the held object; residual: volume-letter shadowing by a principal that can redefine a DOS device (§4.5)" and
  "browser arguments are Keld-owned: none set; `WEBVIEW2_*` removed by the launcher or refused;
  argument and profile policies refused, user-scoped runtime policies refused".
- Root `AGENTS.md` "Hot kipc/event-loop/guard paths … no steady-state allocation": the
  `WebResourceRequested` handler runs on the engine event loop and allocates the parser's
  components for every `file:` request and one response buffer per **Host-served response**
  (documents, `fetch`/XHR, CORS-mode loads, requests beyond the 256-hold bound and, under
  `keld dev`, every request; arm (b), §4.5), and the navigation-signal, eval-settle and
  header round-trip payloads allocate on the same loop. T0 records these as one named,
  measured exception (not a kipc path), bounded by E8–E10 and the B10 instruction wording.
- `crates/keld-wv/AGENTS.md` L5–6 ("platform handles MUST NOT be touched on I/O/pool
  threads"; the only sanctioned pool callback posts via the tao `EventLoopProxy`): §4.5
  fits the design to that rule (a worker touches only a Host-served response's file handle;
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

| Existing owner (reuse, no duplicate) | Exactly what this spec consumes | Decision / divergence |
| --- | --- | --- |
| [#1192 GH-542 checked navigation binding](https://github.com/gyldlab/keld/pull/1192) (**open draft**, `cd30f997`) | §4.2 `NativeDocumentPhase`, checked `u32`, +1 on real replacement commit, +1 **before** a trusted Host load (L110), **+0 on renderer provisional start/cancel** (L107) and same-document changes; §4.4 eligibility = verified resource identity; §4.5 "stale evaluation completion … never delivers to a new document"; N15 Bun recovery keeps the document; N16 one generation; N21 Windows UNKNOWN; L145 four bounded CONTROL records per role. | Reused as the only generation and phase model; W3 is its Windows oracle. Page-initiated navigations stay page-attributed (+0 on cancel, +1 at commit); W3 issues no Host load for them, so no #1192 amendment is needed for navigation. W3 needs #1192 T1 as a platform-neutral module (§10 B2). |
| `RoleGeneration` (`role.rs:189`) and the app-link retirement owner (KEL-75/KEL-143) | Per-role generation; retirement of role-bound correlations on recovery. | Eval and header round-trip correlations are keyed on it. |
| [#1190 GH-544 codec](https://github.com/gyldlab/keld/pull/1190) (**open draft**, `115af9ac`) | One Host-opaque codec (V7); V3 synchronous refusal of functions; **V4: accessor properties fail typed with getter counter 0** (accessors are never invoked); V6: "Proxy traps can run before returning"; the 4096-byte bound it claims for `executeJavaScript` results (L9–12). | Reused as the only codec for its domain and decoder ownership. The 4096-byte bound is falsified as an eval-result bound (D4). **V4 is not met by the chosen transport (v)**: CDP deep serialization invokes own enumerable getters and drops symbol/non-enumerable keys (§4.7); whether that is accepted for eval results (Electron's structured clone also runs getters) is the codec owner's decision (§10 B3/Q15). |
| [#1307 W1 preload facade spec](https://github.com/gyldlab/keld/pull/1307) (revised in parallel) | §4.4 Source rule step 3, §4.5 lifecycle (`ContentLoading` mints/revokes; allowed `NavigationId`), §4.6 navigation policy keys (`web.navigation.allow`, `onLoss`), §4.6a loading-path requirement, blocker B6; AC10 (the application document survives a vetoed navigation). | W3 closes B6 with L1. W1's `file:` Source rule and W3 share the one parser (above); r7 added R10 (`.lnk` → the new public variant `FileUrlRefusal::ShellLink`), so W1's Source rule refuses `.lnk` sources too (B7 vi). W1 `043170ca` L390–394 ("the bytes served for it are that verified resource") must adopt W3's identity: name inside the admitted root, bytes as currently present (owner Q2 in 6491936458) — §10 B7. |
| [#1280 W2 window adapter spec](https://github.com/gyldlab/keld/pull/1280) (gh1188 `efe819e0`, **open draft**) | D2/D3: schema-2 admitted on Windows, `create_app` skipped for facade boots, `HostInitialized` Ready, factored controller build; L54–55: registry windows load an empty document and file-origin `loadURL` is W3's. | The compat boot model W3's A19/AC15 (a)/AC08 (g)(h) depend on is this **target**, not today's code (`app_session.rs:3431-3438` refuses; §10 B13). W3 owns the Windows `HostLoad` seam W2 defers to it; the facade vocabulary stays with #539/#459. |
| [#541 GH-541 spec](https://github.com/gyldlab/keld/pull/1123) and #460/KEL-79 | "no new web.navigate grammar is authorized"; full navigation policy is #460/KEL-79; first-proof eligibility from "boot/resource containment and native navigation evidence". | W3 provides that evidence for `file:` documents and nothing for other schemes (§4.8). The `will-navigate` rule in §4.6 is allowlist-first and needs no new grammar; the allowlist derivation by `keld migrate` is recorded for that owner (§10 B4). |
| [KEL-254 installed-root boot](kel254-windows-installed-root-boot.md) (approved) and `keld_update::ActivePackageSelection` (`crates/keld-update/src/windows_baseline.rs:207-216`: retains `_roots`, `_version: Dir` **and `tree: Dir`**; exposes `tree_root() -> &Path`, `:238`) | Admission of the installed root; AC7/AC8 access profiles; AC13 Legacy refusal. | W3 **lends the existing `tree: Dir` handle** through one borrowing accessor (`tree(&self) -> &Dir`); no reopen by path, no new handle (§4.3). `tree` comes from cap-std `open_dir_nofollow`, whose `dir_options()` shares `READ \| WRITE` without `DELETE` (`cap-primitives-4.0.3/src/windows/fs/dir_utils.rs:61-72`; `load.rs:1088`, `:1103-1105`; cap-primitives `oflags.rs:28-36` strips `FILE_SHARE_DELETE` from every directory open), so it already is the session root pin (A20). |
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
`GetDeferral`, `RequestedSourceKind`; **no `NavigationId`**), `WebResourceResponseReceived` (not registered by r10).
E9 observed on 156 that `RequestedSourceKind` is `DOCUMENT` for **every** request (it names the
filter source kind: document vs shared/service worker) and that `ResourceContext` carries the
resource type: `DOCUMENT` (1) for top-level and frame documents, `STYLESHEET` 2, `IMAGE` 3,
`SCRIPT` 6, and `XML_HTTP_REQUEST` 7 for XHR **and** `fetch()` (never `FETCH` 8 on this runtime).
E10 added: `<object>` and `<embed>` HTML arrive as `DOCUMENT` (1); the draw.io startup census is
1 × `DOCUMENT`, 3 × `STYLESHEET`, 2 × `IMAGE`, 11 × `SCRIPT`, 2 × `OTHER` (16) and no 7/8; and with
`--allow-file-access-from-files` the loads of a `file:` dedicated worker and of the page's own
out-of-root `new Worker(…)` never raised `WebResourceRequested` (source kinds `ALL` tried in
two host modes, E10 `h` and E11 `hs`). E11 added: CORS-mode loads carry `Origin: null` in the
WRR request headers (no `Sec-Fetch-*` header is visible there); a `blob:`/`data:` worker's
`importScripts` raises WRR (context 6) while its `fetch`, `import()` and nested-worker loads do
not; and a `data:` SharedWorker's `importScripts` raises WRR **only** with the worker source
kinds registered (0 events with `DOCUMENT` only).
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
| AC02 `file:*` filter before first navigation (C1) | (i) The schema-1 boot of AC01 (i); (ii) **[TARGET, B13]** a compat window whose first navigation is its first `loadURL`; in both, filter `file:*` + `CONTEXT_ALL` + `DOCUMENT` registered in that webview's controller build before its first `Navigate` | Every `file://` request of the load appears in the WRR log with a Host verdict (**new probe cell**: a `file:*` baseline must reproduce the per-run census of the `*` probe, 68 events per run over its two document loads, 3/3 — the probe used `*`, so its evidence does not transfer until this cell runs; E9 used exactly this filter shape before the first `Navigate` and mediated every corpus request, which is not the census cell; the shared-worker source kinds are registered once per environment, §4.5); the entry's raw inline script that only runs without the Host CSP header did not run (until P4 settles the production baseline, this oracle uses a **test-only** header policy that blocks the fixture's unnonced inline script), checked for **both** documents of the run (note 268: N2's `indexServedByHost` reflected only the second document). | (i) Register the filter after the first `Navigate` under `file:*` (probe N2 re-run) → the first `index.html` escapes to Chromium's file loader, the raw inline script runs → RED (at-risk 1 per document load). (ii) Register the filter only in `navigate_initial` (so a compat window built without it has none) → the compat window's first `loadURL` document escapes to Chromium's file loader → RED. |
| AC03 Containment matrix (C2 hso, C4) | The page issues `fetch`/XHR/`<img>`/`<script>`/`<script type=module>`/`<link>`/`<iframe>`/top-level/`<object>`/`@font-face` requests for the cells below | **Host-served cells** (contexts 1, 7, 8 and every request carrying `Origin`) answer `200` with the bytes of the verified handle and Host headers; **pass-through cells** (classic `<script>`, `<link rel=stylesheet>`, `<img>`, `@font-face`) are verified, **held**, and read by Chromium. In both the delivered identity equals the Host-verified identity (E9's oracle: byte-exact sha256 for `fetch`/XHR, executed nonce for `<script>`, PNG width for `<img>`, posted nonce for documents; for pass-through also E11's same-object oracle — an exclusive Host hold makes Chromium's open fail, proving it targets the verified object). Allowed spellings: in-root files, mixed-case in-root spellings, drive-letter case, interior `//` (collapsed, §4.4 R6), an in-root **hardlink** to an outside file (owner decision; delivered on both paths; **excluded from every NC's at-risk set**). **Parser-refused** (`403`, empty, opener count 0): **plain outside path**, `%2e%2e`, `%5C`, `%2F`, `%00`, UNC/URL-host forms, `file:////?/C:/…`, `name:stream`, `name.`/`name␠`, `CON`/`COM1`, a `~` spelling, and a **`.lnk`** final component in every context (R10). **Opener-refused** (`403`, empty, opener count 1, bytes read 0): a junction as a **leaf**, a junction as an **intermediate** component (mixed case too), a file symlink, a **directory symlink** (real-device cells; E9's fixture created both symlinks and the check refused them). **Evidence, split by path (r7 check P1-2):** Host-served — E10 `hs` 0 differentials over 345 non-race cells × 2 runs, whose ~153 allowed-with-bytes cells per run are Host-served `fetch`/XHR/frame/top cells; pass-through — E11 `pt` corpus, 99 forms × {script, stylesheet, image, `@font-face`} = 396 cells per run, **48 at-risk per context, 0 differentials** in 3 runs (content oracle: 47 per context opened and matched, `lnk-out` never opened; exclusive-hold oracle, 2 runs: Chromium opened 0 of 48 held objects and the held identity equalled the verified one 48/48 per context). E9 (all-context pass-through, 337 non-race cells × 6 runs) is supporting evidence. Without enforcement Chromium delivered bytes for 42 of the 171 refused cells without the switch and 108 of 174 with it, so those refusals are load-bearing. The mixed-case served cell is **re-run under (g)** (§4.3). Page nonce 0 for every refused cell, with the nonce oracle validated per request kind. Still not exercised: 8.3 spellings (E9's volume had no 8.3 names), `sendBeacon`. At-risk sets reported separately: parser cells × runs, opener cells × runs, Host-served vs pass-through cells per context. Evidence attribution, unchanged: the r1 probe's `url-dotdot`, `url-long` and `url-backslash` rows were Chromium-decided or probe-decided; R3/R4/R6 are proven by the pure goldens (AC05) until re-run against the Host parser with a request that reaches WRR. | One fault each, hardlink cells excluded. **(1a)** no percent-decoding in the check, applied to the **pass-through** contexts (decoys planted at the literal undecoded names) → E11 naive control: 19 differential cells per context (76 with the exclusive-hold oracle), including 2 out-of-root deliveries in the script context → RED. **(1b)** raw string-prefix root match with the correct opener → `…/root-sib/f` strips to `["-sib", "f"]`; the fixture plants an in-root decoy at that stripped name so the mutant serves the decoy instead of `404`, else **rule not exercised** (pre-declared) → RED. **(1c)** Win32 path open that follows reparse points → `junction`, `junction-mixedcase`, `symlink-dir` deliver OUT bytes → RED (= AC04's NC). **(2)** pass every request through **without** verification → the plain-outside classic `<script>` delivers the sentinel nonce → RED; baseline 0 across the parser set. **(3)** drop R10 → `lnk-in`/`lnk-out` `fetch`/XHR and `lnk-out-js` `fetch` answer `200` with raw shell-link bytes (INFERRED from E10, where `hs` served them raw) and `lnk-out-js` `<script>` follows a second hop → the `403`/opener-0 count drops → RED. A weaker mutant (N2) scored 1/1. |
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
| AC15 App-declared response headers (S1: documents and `fetch`/XHR) | (a) A compat application registers `onHeadersReceived` inside `app.whenReady()` and then creates its window and calls `loadURL(entry)` (the Electron-correct order; the window's **first** document is that `loadURL`); (b) no listener; (c) the listener's role is confirmed gone; (d) the role is recovering (between the `RoleGeneration` move and the successor's re-registration) and the predecessor had a listener; (d′) same, predecessor had none; (e) a listener whose `filter.urls` matches the document's `fetch`/XHR, a module script **and** its classic `<script>`/stylesheet/image subresources | (a) **[TARGET, B13: blocked until W2 D2/D3 land on Windows]** one Host→Bun round trip per **Host-served response** (`ResourceContext` 1, 7, 8 and CORS-mode loads) **including the window's first document** — the registration and the `loadURL` are two records on the one FIFO (a requirement T4/T5 test, §4.5), and no Host navigation precedes them (W2's facade boot: no window at boot, `HostInitialized` Ready); the served headers are the listener's (Host-owned fields excepted); (b) zero round trips; (c) Host-served requests `403`/empty; (d) Host-served requests are **held** behind their deferrals until the successor registers, or until its `listeners-settled` record (posted by the facade after its `whenReady` callbacks drained) arrives without a registration, then served; bounded by the recovery deadline and a count, after which `403`/empty; (d′) with no predecessor listener nothing is held and responses carry the baseline only (as before the loss); (e) one round trip per matching `fetch`/XHR and module script with the listener's headers applied; **zero** for the pass-through classic script/stylesheet/image, whose headers are **not** applied — the recorded divergence from Electron, whose listener sees every response (T1 entry; §4.5). At-risk for (a) = the first DOCUMENT response per run (non-empty by construction). | (a) Process the `loadURL` record before the earlier registration record (FIFO broken) → the first document is served with the baseline CSP only and the app's inline-script policy is not applied (probe N5 class) → RED. (c) Serve on role loss with baseline-only headers → RED. (d) Answer `403` instead of holding → a recoverable window shows a blank document after every Bun restart → RED. (d′) Hold although the predecessor had no listener → requests stall until the recovery bound for no reason → the page oracle sees the document blank past the bound → RED. (e) Skip the round trip for `fetch`/XHR → the listener's header is absent from the `fetch` response → RED; separately round-trip pass-through contexts → round-trip count > Host-served count → RED. |
| AC16 Baseline CSP cannot be relaxed | The listener returns a permissive CSP, or overrides `Content-Type` to `image/svg+xml`/`application/xhtml+xml` | The baseline is emitted as its own header on **every `DOCUMENT`-context response** regardless of `Content-Type`; a remote script the baseline forbids stays blocked; in-root scripts load under a baseline written with `'self'` or nonces (AC25; hashes UNTESTED; cross-directory `'self'` is P4, §10 Q5). Pass-through subresources carry no Host header, so the document's policy is the one enforced on them; Host-served `fetch`/XHR responses carry no baseline CSP (a CSP header on a non-document response has no effect on the page). | Emit the baseline only for `text/html` → an SVG document served with the app's permissive CSP runs the forbidden script → RED. |
| AC17 Hardlink authority read-back (C4) | The fixture plants ≥ 1 in-root file with `nlink > 1` whose other name is outside the root | (a) served (AC03); (b) for every delivered file (Host-served or passed through) with `nlink > 1`: a write attempt on the linked object under **each admitted role token** (Strict; the Legacy forfeiture printed instead of claimed) is denied (KEL-254 AC8 class) and the per-file DACL read-back shows no role write grant (AC7 extension); (c) the owner's paired control: the Strict role attempts to **create an entry inside the admitted root** and is refused (KEL-254 app-role isolation on Windows). At-risk = multi-link delivered files; 0 → rule not exercised (fixture MUST plant one). | Grant the Strict role write on the linked object → read-back detects, the control writes → RED. Separately grant it create on the root → (c) succeeds → RED. |
| AC18 Zero bytes on refusal (C5) | AC03's refused cells, elevated ETW Kernel-File session | No open of the outside sentinel by `msedgewebview2.exe`; for parser cells the Host opener count is 0; for opener cells bytes read is 0 (the opener returns a handle to the reparse point itself and reads nothing). ETW NOT RUN → INFERRED-strength, printed. | AC03's NC → ETW shows the renderer's own open → RED. |
| AC19 Non-`file:` requests never reach W3 | The page fetches `https://` (policy-allowed) and loads `data:`/`blob:` | **Structural**: the `file:*` filter never raises WRR for them — new probe cell: `https` request with WRR event count 0 and Host response count 0; the policy owner's rules apply unchanged. | Register `*` instead of `file:*` → the `https` request raises WRR and sits behind the UI thread → RED (count ≠ 0). |
| AC20 P1 performance gate (§9) | Before T2 merges | **Probe status (E13/E14): FAILED, carried to T2.** r9's dynamic handler failed (c) on a real Vite application (E13, +6.2–8.5 % per document). The chosen `hsoz` passed (c) once (E14 follow-up 7) and failed (a), (b) and (c) in the pre-registered four-block replication (follow-up 8: (a) +3.71 % [+0.69, +6.48], (b) +9.01 % [+4.70, +11.83], (c) doc0 upper +8.55 %, doc2 +7.92 %), while the hold's own cost (`hsoz` − `hso`) was small and sign-unstable across sessions and the session's baselines ran above E12's (draw.io `e` +9–11 %, modules `hsm` +6–11 %); `hso` vs `hsm` itself reversed from E12's −2 % to +6.78 % (B8 reopened). No waiver is claimed from probes; the T2 gate below decides on the implementation. **Measured (E8–E12):** `hs` +3.54 % [+1.16, +4.68] vs `e` on draw.io (E10); modules: `hsm` 158.5 ms vs `eaf` 206.6 ms, paired −40.9 ms (E11). E12, `hso` + hold: with `WebResourceResponseReceived` always registered (`hsoh`) (a) draw.io +1.65 % [+0.59, +2.82] **PASS**, (b) modules +5.72 % [+1.77, +9.53] vs `hsm` **FAIL** (the handler's registration costs +11.1 ms [+4.7, +18.4]; the hold +0.3 ms); with the handler registered only while holds exist (`hsod`) (a) +0.67 % [−1.65, +2.43] **PASS**, (b) −1.51 % [−5.53, +1.23] **PASS** — but (b) is a **zero-hold** workload, so it never paid for the handler. Per-request NCs passed in every block. **Gates at T2** (implementation, production baseline CSP, preload so real `fetch`/XHR demand is in the census; pooled ratio-of-medians CI upper bound, with a pre-registered per-request NC in each block): (a) draw.io, chosen design vs `e` ≤ +5 %; (b) module workload vs `hsm` ≤ +5 %; **(c) a realistic module application — Vite output: modules and `crossorigin` CSS Host-served, images and fonts passed through and held, across several documents — chosen design vs `hso` (no hold) ≤ +5 %, with holds per document and peak live handles reported**. Every gate block also runs an **A/A arm** (the comparator twice under two labels) and reports each block's baseline median against E12's; n is fixed in the pre-registration; if the A/A CI exceeds ±2.5 %, the gate is re-run once with n doubled, and a second miss is INCONCLUSIVE and goes to the owner. E14's (a)/(b) A/A intervals fail this rule, so E14 is indicative only. Any bound exceeded → a written waiver (arch 01 §5) or a design change before merge. STA occupancy and the longest single STA block are reported. Unmeasured as well: arm (f) (S1 round trip, B13) and the product-boot clock. | Single arm, no arm (e), no negative control, or paint measured from `NavigationCompleted` → validator RED. |
| AC21 Receipt | A Windows acceptance claim | Runtime string, OS build, head, admitted-root mode, per-AC counters, at-risk counts per deciding rule, each RED output, "rule not exercised" rows, UNKNOWN cells (§3 AC23). | Missing fields or a zero-failure row without its at-risk count → validator RED. |
| AC22 Non-discriminating controls recorded, not counted | Probe N1 (only a dedicated worker was attempted; shared/service workers untested), N3, N6 (compared constants; no CDP result replayed), the exclusive-lock oracle (empty at-risk set); AC05's double-decode mutant | Each reads **rule not exercised** and contributes nothing to scores; N6's claim is replaced by AC11 (ii). | Count any of them → validator RED. |
| AC23 Coverage cells with UNKNOWN status | favicon (probe `r-favicon` timed out), `<link rel=prefetch>`, `navigator.sendBeacon`, web app manifest, media `Range` (probe media cells used filler bytes; `mediaError 4` proves nothing), `blob:`/`data:` workers (measured in E11 and moved to AC28), Shared/Service Worker construction attempts, `<embed>`/`<object>` PDF, `<a download>`, `<script type=module crossorigin>` and dynamic `import()` (default Vite/electron-vite output) — measured in E11 and moved to AC24, and the environment worker filter's survival when the first webview closes (the Host re-registers it on the next live webview; cell: close the first window, open a second, attempt a shared worker) | Each is run as its own cell on the real device and reported served/refused/UNKNOWN with the request kind observed; none is claimed from the probe. | Claim any of them from the probe → validator RED. |
| AC24 Request classification (A7) | One load issuing a top-level document, an `<iframe>`, an `<object>` and an `<embed>` HTML document, classic `<script>`, `<script type=module>` (same and other directory, static `import`), `import()`, `<link rel=modulepreload>`, `<script crossorigin>`, `<img crossorigin>`, `<link rel=stylesheet crossorigin>`, `<link rel=preload as=font crossorigin>`, `@font-face` (inline, external stylesheet, `FontFace` API), plain `<img>`, plain stylesheet, XHR and `fetch()` to in-root `file:` URLs; **dev/release differential (r10)**: the same application under `keld dev` (`hsa`) and as an installed launch, with an `onHeadersReceived` listener, recording response headers on a classic script and an image, `Content-Type` for `.wasm`, `.svg` and `.mjs`, and a media seek (also on a request past the 256-hold bound) | WRR log per request: contexts 1 and 7 (8 if reported) and every request whose WRR headers carry `Origin` (E11: value `null`, no `Sec-Fetch-*` visible) → **Host-served**; classic script, plain image, plain stylesheet, `@font-face` (no `Origin`) and other 16 → **verified, held, no response**. Page oracle (E11 `cm-hso`, 2/2 runs): every module, `import()`, `modulepreload`, `crossorigin` script/image/stylesheet and crossorigin font-preload cell loads and executes, without `Access-Control-Allow-Origin`; classic script, plain image and the three `@font-face` forms load on pass-through. In-root `fetch`/XHR `200` with no switch; out-of-root `403` empty (E10). Dev/release: app headers reach exactly the release classes in both modes and `Content-Type` matches per class; any media-seek difference is recorded as a divergence (Range is UNKNOWN, AC23). | (1) Classify by `RequestedSourceKind == DOCUMENT` (E9: `DOCUMENT` for every request) → every subresource is served → served count = request count (19/19 on the E8 workload) and E8's serving cost returns (+34–36 %) → RED. (2) Drop the `Origin` rule (r7 `hs`) → the module same-/other-directory, static-import, `import()`, `modulepreload`, `crossorigin` script/image/stylesheet and crossorigin font-preload cells fail (E11 `cm-hs`, 2/2) → RED. (3) Pass context 7 through → in-root `fetch`/XHR fail (`TypeError`/status 0, E10 `sx-hyb-off`) → RED. (4) Pass frame `DOCUMENT`s through → the frame carries no Host CSP header and the AC25 `'none'` fixture's scripts run inside it → RED. (5) Under `hsa`, apply app headers to every Host-served request regardless of release class → the dev classic script carries the listener's header and the installed one does not → RED. |
| AC25 The served document's CSP governs pass-through subresources; CSP is not containment (A16) | Host-served documents carrying, one per cell, `script-src 'none'`; `script-src 'self'`; `script-src 'nonce-…'`; `img-src 'none'; style-src 'none'`; `script-src file:///<root>/csp/ext.js`; no CSP — each loading same-directory `file:` scripts, an inline script, a nonced inline script, a `data:` script, a stylesheet and an image; **cross-directory** `'self'`, `'none'` and no-CSP pages at the root and at a deep path loading `js/a.js`, `lib/x/b.js`, `./js/../lib/x/c.js` (root) and `../../js/a.js`-style paths (deep page), plus an out-of-root script and a junction-reached one; **T2 device cell**: a `'sha256-…'` hash source | As E9 `h-csp` (with the switch) and both E10 `hs` corpus runs (no switch; origin as measured in E10's no-switch matrix rows under the same environment configuration), every cell identical: `'none'` blocks every script; `'self'` runs the same-directory `file:` scripts and blocks inline/`data:`; the nonce policy runs only nonced scripts; `img-src`/`style-src 'none'` block the pass-through image and stylesheet; no CSP runs everything. Cross-directory (E11 `cx`, 12/12 cells over 2 runs): `'self'` runs every in-root script from the root and from the deep page, inline is blocked, `'none'` blocks all. **CSP is not a containment boundary**: under `'self'` the out-of-root and junction-reached script URLs raised **no** CSP violation; only the Host's `403` stopped them (E11). The exact `file:` host-source blocked its own script on this runtime — recorded as a runtime quirk (CSP3 would admit it), not required. Pass-through `Content-Type` is Chromium's extension mapping (E9). Hash sources are UNKNOWN until their cell runs. | (1) Drop the CSP header from the served document → the `'none'` cell runs its `file:` scripts → RED. (2) Run the `'self'` cross-directory page with the Host's refusal disabled (observe) → the out-of-root script runs with no CSP violation (INFERRED from E11, where only the `403` stopped it) — shows that containment rests on the Host, not on CSP. |
| AC26 Root swap and in-root race (A20, A23) | (i) After admission the root's **parent** mutation owner renames `<root>` → `<root>.swapped` and plants a junction `<root>` → OUT (look-alike names, other nonces), then the page loads Host-served and pass-through requests. The fixture root has **nothing open beneath it** except the Host's own root handle, because an open handle beneath a directory alone makes its rename fail with `ERROR_ACCESS_DENIED` (5, E9 share-mode table; both production roots contain the running `keld-host.exe`, `app_session.rs:1533-1534`, `:3014`); (ii) after verification and before Chromium's open, a pass-through classic `<script>`, `<img>` and stylesheet have their leaf replaced by rename and, separately, their directory replaced by a junction to OUT (deterministic injection); (iii) the leaf is replaced by an **in-root** file right after verification (the instant at which E11's separate hold chain had a gap); (iv) **release cells**: success, cancelled (`src` cleared, `window.stop()`, iframe removed or navigated), removed-from-DOM, CSP-blocked, duplicate and same-URI loads, page and Host navigation, webview close; (v) **T2 device cell**: three documents in sequence, each holding, then a blank document; and one document issuing 300+ distinct pass-through images (the 256-hold bound) | (i) The rename fails with **`ERROR_SHARING_VIOLATION` (32)** — the code is counted, so a 5 from a handle beneath does not pass — and every cell matches its verified identity (E9 hybrid with the no-`DELETE` root: 0/9). (ii) The verified bytes are delivered 24/24 per kind and injection, every swap fails (32), and, under r10's retirement-only release, a held leaf cannot be rewritten (32) or replaced by rename (5) until retirement and both succeed afterwards (E14 rewrite cell, 3 runs; E11 `hsh`, `hsh-post` measured r9's earlier release). (iii) The verified bytes are delivered: E12 gap cell 288/288 (swaps fail 32, rename-over 5); the hold **is** the verification handle (opened share `READ`), so no verify → hold gap exists. (iv) No hold outstanding after retirement or close in any cell (E12, 3 runs each, `hsoh` and `hsod`; E14 `hsoz`, 3 runs); cancelled loads released only by retirement/close; same-URI holds released as a multiset. (v) Each document's holds are all released at its retirement and none earlier; 0 outstanding after the last retirement and every exclusive open succeeds; at the bound the first 256 are held, the rest Host-served, all images decode (E14 `hsoz`: 3/3 each; peak 768 handles). | (i) Open the session root handle **with** `FILE_SHARE_DELETE` (E9's probe control; only implementable on the dev stage's std handle — cap-primitives strips `DELETE` from every directory open, `oflags.rs:28-36`) → the rename succeeds and later pass-through reads come from OUT: E9 control 4/9 under all-context pass-through; under `hso` the predicted at-risk set is 2 of 9 (the classic script and the plain image; INFERRED) → RED. If the fixture cannot keep the root free of handles beneath it, the NC is pre-declared **rule not exercised**. (ii) No hold (r7 `hs`) → the replacement 24/24 and OUT 24/24 per kind (E11 `sr-hs`) → RED. (iii) No hold (`hso`) → the gap attack succeeds 144/144 (E12) → RED. (iv) Remove the retirement/close release → 14 (`hsoh`) / 15 (`hsod`) holds outstanding and the exclusive-open detector fails 32 (E12) → RED. (v) Remove the retirement release (E14 `hsozx`) → 18 holds outstanding and 18 exclusive opens fail 32 → RED; remove the bound → more than 256 holds outstanding in the bound cell → RED. |
| AC27 Drive-letter binding (A21) | The project is reached through a `subst` letter `K:` → `…\A`; a separate same-user process rebinds `K:` → `…\B` (look-alike files) while the page loads | (i) Admission from the `K:` spelling is **refused** typed (`KELD-*`, naming the final path to launch from) because `QueryDosDeviceW("K:")` names a session-defined target (`\??\…`), not a volume device. This is fail-early UX: containment does not depend on it, because the root URL is spelled from the final path and R7 refuses every `K:` URL as `OutsideRoot`. A project reached through a junctioned ancestor, cross-volume included, is **admitted** and re-spelled from its final path. (ii) A root spelled by its final path is unaffected by rebinding a spare letter: E9 0/12 — that run's URLs were spelled with the final path's `D:`, so it does **not** test `K:` URLs. Not exercised (pre-declared UNKNOWN): shadowing the **real** volume letter of the final path. E9 `ddd`: an AppContainer and an LPAC child get `ERROR_ACCESS_DENIED` (5) from `DefineDosDeviceW`; a medium-IL same-user child succeeds and the Host's session sees the mapping. | (i) Remove the admission refusal → no typed `KELD-*` error at admission (the app boots under the `D:` spelling and every `K:` link is `403` `OutsideRoot`) → RED. (ii) With a declared test seam that admits a `K:`-spelled root URL (precondition printed), spell the URL from the requested letter → E9: 5 of 12 cells, exactly the pass-through ones, deliver `B`'s bytes → RED. |
| AC28 Keld-owned browser arguments (A22) | (i) The environment as Keld creates it; the page constructs `new Worker('wesc.js')` (in-root) and `new Worker('../../out/wout.js')` itself, and `blob:`/`data:` classic, module and shared workers whose `importScripts`, `fetch`, `import()` and nested `Worker` target out-of-root, junction-reached and in-root files; `navigator.serviceWorker.register`; (ii) `self.origin`/`location.origin` and an in-root `file:` frame's DOM; (iii) in-root and out-of-root `fetch`/XHR and a cross-origin `https:`/`http:` URL without CORS; (iv) `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--allow-file-access-from-files` (and, separately, a benign `--user-agent=…`) is set (a) in the environment of `keld dev` and (b) in the environment of a directly launched installed `keld-host`; (v) **T2 device cell** (B16): each loader policy of rule (3) under HKCU, value names the executable's file name, `*` and an AUMID-shaped name; `AdditionalBrowserArguments`, `UserDataFolder` and `BrowserExecutableFolder` (administrator-only and user-writable targets) under HKLM; (vi) source test over `keld-wv`/`keld-cli` | The oracle counts **out-of-root nonces delivered** (WRR counts are reported, not scored). (i) Page-constructed `file:` workers throw `SecurityError`; `blob:`/`data:` classic and `data:` module/shared workers construct with origin `null`, `blob:` module and `blob:` shared workers fail to start, `serviceWorker.register` rejects; every `file:` load from those workers fails — out-of-root and junction `importScripts` raise WRR and get `403`, in-root `importScripts` is allowed and passed through yet still fails `NetworkError`, and `fetch`/`import()`/nested workers raise no WRR and are blocked by Chromium — identically in an observe run (E11 `b15-hs` ×2, `b15-hs-allk`, `b15-observe`): **0** out-of-root nonces. (ii) `self.origin == "null"`, `location.origin == "file://"`, the frame's `contentDocument` is `null`. (iii) In-root `200` (Host-served), out-of-root `403` empty, cross-origin fails (E10). (iv) (a) the launcher removes the variable from the Host's environment, the environment is created and 0 nonces arrive (INFERRED from E11 `env-af-clearenv`, where it was absent at creation); (b) `keld-wv` refuses environment creation with a typed `KELD-*` error naming the variable and no page loads (E11 `env-af-refuseenv`). (v) Typed `KELD-*` refusal naming the root, key and value name for every refused policy; an HKLM `BrowserExecutableFolder` at an administrator-only folder launches and the boot diagnostics record that runtime's version and image path; at a user-writable folder it is refused (B16). (vi) Options `AdditionalBrowserArguments` empty; the KEL-66 test (`webview2/mod.rs:4851-4875`) is extended to refuse `--allow-file-access-from-files` and `--allow-universal-access-from-files` by name. | (1) Put `--allow-file-access-from-files` in the options → every worker kind and the page-constructed out-of-root worker deliver OUT nonces (3/3 targets) with zero WRR events for those loads (E10 every host mode; E11 2 runs and with source kinds `ALL`) → RED. (2) Drop both the launcher removal and the refusal with the variable set → OUT nonces 3/3 and `self.origin == "file://"` (E11 `env-af`); separately drop only the refusal → case (b) escapes (installed launches have no Keld launcher); options `""` or no options object does not override it, and on a conflicting argument the environment value wins (E11 `env-af-nullopts`, `env-afua-optua`: the page's user agent is the variable's) → RED. |

## 4. Design

### 4.1 Atomic decomposition (decision input)

Evidence classes: FACT, PROBE (run ids `baseline r1–r3`, `n1–n6`), INFERRED, UNKNOWN (named
probe). Security atoms separate identity, authentication, authorization, OS containment,
lifecycle/revocation and evidence provenance; performance atoms are in §9.

| Atom | Owner / boundary | Inputs → outputs | Failure mode | Observable | Evidence | Edges · first falsifier |
| --- | --- | --- | --- | --- | --- | --- |
| A1 Loading path | `keld-core` boot → `keld-wv` `navigate` | admitted root + entry components → `Navigate(file URL)` | string load yields `about:blank` identity | AC01 | FACT W1 source probe; code `:3653`/`:3654` | · AC01 NC |
| A2 Request census and filter scope (coverage, cost) | WebView2 filter normalization + `keld-wv` registration | `file:*` + `CONTEXT_ALL`; source kind `DOCUMENT` per webview before first `Navigate`, `SHARED_WORKER \| SERVICE_WORKER` once per environment (together: all source kinds) → every `file://` request a page or worker can issue raises WRR, no other scheme does; worker `fetch`/`import()`/nested-worker loads raise none and Chromium blocks them without the switch (E11) | late registration; `*` filter puts every remote request on the UI thread; worker source kinds missing (shared-worker `importScripts` invisible) | AC02, AC19, AC28 | PROBE n2 (under `*`; second document only per note 268); PROBE E11 (`data:` SharedWorker `importScripts`: WRR only with the worker source kinds); FACT L1307 normalization, `_22` L51/L72 | precondition of A8 · n2 under `file:*`, both documents |
| A3 Admitted root (identity of the root) | `keld-core` boot (dev DACL; KEL-254 selection) | admission → one retained `Dir` | handle dropped and reopened by path | AC04 | FACT dev handle dropped today; `tree: Dir` retained | → A5, A20, A21 · junction swapped in after admission is followed |
| A4 URL → components (authentication of the name) | `keld-guard` parser | URI + `AdmittedRootUrl` → components or refusal | double decode; authority; ADS; `~`; Win32 spellings | AC05 | PROBE parser cells; FACT grammar; FACT Chromium pre-canonicalises `%2e%2e` | independent of A5 for `:`/`~`; redundant for `..`/trailing dot · AC05 NC |
| A5 Open (OS containment) | `keld-update` `open_regular_beneath`, **per-component** cap-std no-follow walk (normative) | `&Dir` + components → regular-file handle + `nlink` | reparse followed (multi-component `open_with` follows intermediate relative symlinks); directory; Win32 name layer | AC03 opener cells, AC04 | FACT cap-primitives relative `NtCreateFile` + `FILE_OPEN_REPARSE_POINT`; PROBE `gh1185-capstd-junction-2026-10-30` (3/3 identical, keld-research note 268): cap-std 4.0.3 refuses an intermediate junction in every form (multi-component path and default `open_dir`: "a path led outside of the filesystem", for out-of-root and in-root targets alike; `open_dir_nofollow`: os error 681), while the ambient `std::fs` negative control reads the out-of-root sentinel through the same junction; directory/file symlinks UNTESTED (creation needs privilege) | ← A3 · AC04 NC |
| A6 Hardlink authority (authorization) | KEL-254; W3 reads back | `nlink` + DACL → served + receipt | namespace write taken as the only authority | AC17 | FACT owner decision + correction | · role write grant |
| A7 Request classification and response contract (hso) | `keld-wv` handler | WRR `ResourceContext` 1 (documents incl. `<object>`/`<embed>` HTML), 7, 8 (`fetch`/XHR), or any request whose WRR headers carry `Origin` (CORS mode): `200` from the verified handle with Host `Content-Type` (one `keld-wv` table), `Content-Length` and S1 headers, the baseline CSP on 1 only; any other context: verified and held, **no** response (A23); refusal: `403`/`404` empty | classifying by `RequestedSourceKind` (always `DOCUMENT`: every request served, E8 cost); a document passed through (no Host headers); `fetch`/XHR or CORS-mode loads passed through (they fail from an opaque origin) | AC03, AC09, AC16, AC18, AC24 | PROBE E9/E10 census; PROBE E11 `cm` (every CORS-mode cell fails passed through, loads Host-served; `Origin: null` at WRR; no `Sec-Fetch-*` visible) | → A16, A23 · AC24 NCs |
| A8 Served-document identity | `keld-wv` per-navigation record for **every** top-level navigation (all schemes) | allowed `NavigationStarting(n)` + DOCUMENT WRR with equal parsed URI → `ServedDocument` | URL text as identity; frame record attached to `n` | AC06 | INFERRED timing; UNKNOWN `IsErrorPage` for Host `404` (Q7) | ← A2; → A12 (gate sees all navigations) · AC06 NC |
| A9 Navigation events → facade | `keld-wv` → `AppWindowEvent` → facade | WebView2 events → Electron emission rules | fabricated/reordered; `did-fail-load` on cancel | AC07, AC09 | FACT Electron v45.1.0 L2745–2752, L2872, L2932–2933 | ← A8 · AC07 NC |
| A10 `will-navigate` policy and attribution | `keld-wv` (allowlist on the original event) + facade (notification) + `keld migrate` (allowlist derivation) | `NavigationStarting` → allow/cancel (+0) and notification | allowlist after +1 (revokes live document, W1 AC10); `IsUserInitiated` as initiator; redirect hop of a Host attempt cancelled | AC08 | FACT: no deferral; FACT #1192 L108 (+0 on cancel); UNKNOWN history-kind emission (P3) | edge to #1192 L108–112 and W1 AC10 made explicit: W3 performs no Host load for page navigations · AC08 NC |
| A11 Eval mechanism | `keld-wv` CDP | script → settled value/rejection/destroyed | `ExecuteScript` targeting; codegen scope | AC10 | PROBE n4; FACT V8 L151, L400–402 | · n4 |
| A12 Eval lifecycle/revocation | one `keld-wv` state machine (`Pending → Retired \| Delivered`) over #1192's generation with the main-frame context bound at `ContentLoading`; role axis = KEL-75/KEL-143's retirement | `(G, ctx)` at dispatch → deliver iff the entry is still `Pending` when CDP completes; retire in the STA turn that advances G | stale delivery; eval in the gap runs in the new document; BFCache never completes | AC11, AC12 | PROBE r1–r3 (`-32000` on replacement); **no probe evidence for the replay rule** (N6 compared constants — note 268); FACT CDP out-of-order (L1333) | ← A8 (all schemes) · AC11 (ii) NC; AC12 NC |
| A13 Eval capability (fail loud) | create-time self-test | CDP calls → start / typed refusal | silent per-call rejection; unknown-field acceptance mistaken for honoring | AC13 | FACT crdtp L124–128 | · AC13 NC |
| A14 Eval value domain | #1190 owner (decoder of the deep-serialized form; domain losses) | `deepSerializedValue` bytes ≤ bound → decoded domain | 4096 bound; getters run and symbol/non-enumerable keys dropped (V4 not met); remote objects leaked without `releaseObjectGroup` | AC14, AC10 (e)–(g) | FACT D4; FACT V8 L222/L257, L339–341; FACT `injected-script.cc` L640–648 | B3/Q15 · AC14 NC |
| A15 App response headers (S1) | facade listener + `keld-wv` round trip | registered listener → modified headers on **Host-served responses** (1, 7, 8, CORS-mode); pass-through classic script/stylesheet/image carry none (recorded divergence) | pass-through on retirement; second URL matcher; round trip for pass-through contexts | AC15 | FACT D3 (source level); INFERRED from the API surface, untested: a request answered with no response cannot carry Host headers | → A16, A19 · AC15 NC |
| A16 Baseline CSP | `web.csp` owner for content; W3 for emission | policy → own header on every served `DOCUMENT` response; Chromium enforces it on that document's subresources | merged header; type-keyed emission; `file:` host-sources (blocked on 156); treating CSP as containment | AC16, AC25 | FACT CSP3; PROBE E9 `h-csp` and E10 `hs` corpus: identical outcomes; PROBE E11 `cx`: cross-directory `'self'` works 12/12, and `'self'` raised no violation for out-of-root URLs (only the Host's `403` stopped them); hashes UNKNOWN | ← A7, A22 · AC16 NC, AC25 NC |
| A17 Scheme boundary | #541/#460 | non-`file:` URI → never raised | `*` filter | AC19 | FACT L1307 | · AC19 NC |
| A18 Evidence provenance | spec + receipts | hashed runs → claims | second runtime; zero-failure without at-risk; probe cells misattributed | AC21–AC23 | FACT one runtime | · validator |
| A19 Registration ordering | `keld-core` control FIFO (**TARGET**: compat windows have no Host-navigated first document once W2 D2/D3 land) + recovery hold | facade registrations sent before `loadURL` apply to that load's first request; during recovery `file:` requests are held until the successor registers or posts `listeners-settled` (bounded; only if the predecessor had a listener) | FIFO broken; a Host-navigated first document before `ready` | AC15 (a), (d), (d′) | FACT: today Windows refuses schema-2 (`app_session.rs:3431-3438` `KELD-CORE-034`; `HostInitialized` is macOS-only); TARGET: W2 gh1188 D2/D3; FACT: Electron apps register after `ready` (draw.io `electron.js` L987–994) | → A15; blocked by B13 · AC15 NC (FIFO order swap) |
| A20 Session root pin (lifecycle) | `keld-core` boot: the retained root `Dir` shares `READ \| WRITE`, never `DELETE` | admitted root handle → rename/delete of the root fails (32); ancestors cannot be renamed while a handle beneath is open | root shared with `DELETE`: the parent's mutation owner swaps the root and pass-through reads OUT persistently | AC26 (i) | FACT both existing handles already lack `DELETE` (dev `app_session.rs:1495-1498`; installed cap-std `dir_options()`); PROBE E9 hybrid 0/9 vs control 4/9 | → A23 · AC26 NC (i) |
| A21 Root drive-letter binding (identity of the root's spelling) | `keld-core` admission + `AdmittedRootUrl` | requested root path + handle final path → root URL spelled from the final path; fail-early refusal when the requested letter is session-defined (`QueryDosDeviceW` names a `\??\` target) | a literal `subst` letter spelled into the URL: a rebind redirects pass-through reads | AC27 | PROBE E9: literal `K:` 5/12 (the pass-through cells); final-path spelling 0/12 (spare-letter rebind; `K:` URLs not tested); real-letter shadowing UNKNOWN; AppContainer/LPAC `DefineDosDeviceW` → 5; the `\??\` heuristic misses a `DDD_RAW_TARGET_PATH` definition (UX only; R7 still refuses) | → A23; Strict equivalence #1187 (Q20) · AC27 NCs |
| A22 Keld-owned browser arguments (invariant) | `keld-wv` environment creation (KEL-66 owner) + every Keld launcher of `keld-host` (today only `keld-cli dev`, `dev.rs:363`) | options → **empty** `AdditionalBrowserArguments`; the launcher removes every `WEBVIEW2_*` variable from the child's environment (`Command::env_remove`, safe); `keld-wv` refuses environment creation with a typed `KELD-*` error naming any `WEBVIEW2_*` variable present (`std::env::var_os`, safe); a refused rule-(3) policy → typed refusal | `--allow-file-access-from-files` from any source: workers run and read out-of-root files with **no** WRR event; `self.origin` becomes `"file://"`; in-root frames' DOM readable | AC28, §1 origin observable | PROBE E10 switch matrix; PROBE E11: the variable merges with the options and wins on conflict, options `""` does not override it, removing it before creation closes it (`env-af-clearenv`, in-process), refusing closes it (`env-af-refuseenv`); a launcher-side removal is INFERRED equivalent (the variable is absent at creation either way); E13 (Q21): `WEBVIEW2_USER_DATA_FOLDER` silently replaces the options' folder, `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` runs a copied runtime (and a missing folder fails `0x80070002` with no fallback); policies UNTESTED on device (B16) | → A16 (origin under which CSP was measured), §1 · AC28 NCs |
| A23 Pass-through verify-and-hold (OS containment of the object Chromium opens) | `keld-wv` handler + the C3 opener; the verification handle is the hold | classic script/stylesheet/image/font/media/other URI → verified `(root, components)`; the leaf stays open share `READ` and every directory component stays open without `DELETE`; no response → Chromium opens the pinned object → release **only** at document retirement or webview close (no response handler; at the 256-hold bound the request is Host-served; `keld dev` Host-serves every request) | parser differential; a mutation between verification and Chromium's open; a hold that is never released; more than 256 holds outstanding | AC03, AC20, AC26 (ii)(iii), AC27 | PROBE E11 `pt` and `sr`; PROBE E12: gap cell verified 288/288 (swaps fail 32, rename-over 5) vs no-hold NC 144/144 attacked; no hold leaked in any release cell; cancelled loads (`src` cleared, `window.stop()`, iframe removed or navigated) get no `WebResourceResponseReceived`; a request in flight when the handler is added never gets one; the handler, not the hold, costs on module loads (+11.1 ms vs +0.3 ms); PROBE E13: the dynamic handler costs +6–8 % per Vite document; PROBE E14 (`hsoz`): gap 144/144 ×2 vs NC attacked, retirement releases all, bound cell 256 held / 64 served / 320 decoded, rewrite blocked while held and allowed after | ← A4, A5, A20, A21 · AC03 NCs, AC26 NCs |

### 4.2 Decisions taken as input (kept; evidence re-checked)

| Decision | Statement | Evidence this spec re-checked |
| --- | --- | --- |
| L1 | Load with `Navigate(file:///<admitted root>/<entry>)`; `NavigateToString` retired for application documents. Closes W1 B6 jointly with #1192/KEL-102. | W1 AC18 (3/3). |
| C2 (r8, owner decision relayed 2026-10-30, to be recorded on #1185 with B1) | **hso.** Every `file://` request raises WRR (filter registered before the first `Navigate`; `file:*`, never `*`; all source kinds across the environment) and is verified by the one parser and the one no-follow walk beneath the session-pinned root; contexts 1 (documents) and 7/8 (`fetch`/XHR) and every CORS-mode request (`Origin` present) are Host-served from the verified handle with Host headers (baseline CSP on documents); every other request is passed through while the verification handle is held; refusals are `403`/`404` empty. No virtual host, no custom scheme; browser arguments are Keld-owned (none). | E8: serving every byte +34–36 % (rejected); E9: pass-through 0 parser differentials; E10: r6's switch is a containment escape; E11: r7's `hs` breaks every CORS-mode load and its pass-through raced (24/24), a hold closes the race (24/24), Host-serving modules is faster than Chromium's own loader with the switch (−40.9 ms [−51.3, −35.8]) (§4.9, §9). |
| C4 | In-root hardlinks served; `nlink > 1` selects per-file DACL read-back and write-denial per role token; Legacy forfeits root integrity (printed). | 6491936458 + 6492237806. |
| E1/E5 | CDP `Runtime.evaluate` with `awaitPromise`, `allowUnsafeEvalBlockedByCSP:false`, **mandatory** context binding, generation authority as the one `Pending → Retired \| Delivered` machine (captured at dispatch; retired in the turn G changes; delivered only while `Pending`); create-time self-test fails closed. | D2; probe r1–r3 (settle, `-32000`), n4; V8 L400–402. The replay rule has **no** probe evidence (N6 was not a control, note 268) and is proven only by AC11 (ii). |
| S1 | Per-**Host-served-response** round trip to Bun only when an `onHeadersReceived` listener exists (r8: documents, `fetch`/XHR and CORS-mode loads; pass-through classic script/stylesheet/image carry no app headers — divergence, §4.5); fail closed; `Complete` on the UI thread; separate Keld baseline CSP. **Matcher ownership fixed (r2):** the Electron `filter.urls` match-pattern matcher lives once, in the facade (§4.5). | D3. |
| Owners | One generation owner (#1192), one codec owner (#1190), two name owners (§2). | §2. |

One input decision was contradicted by measurement: C2 as written in r1–r5 (the Host serves
every byte) costs +34–36 % `navToReady` (E8) against a 5 % budget; the owner replaced it with
a hybrid (r6), r7 replaced r6's document-only hybrid with `hs` after E10 showed that the
browser switch r6 needed is a containment escape, and r8 replaced `hs` with `hso` + hold after
E11 showed that `hs` broke CORS-mode loads and that its pass-through raced. r1's own derived decisions that
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
/// cap-std's Symlink error or raw 681 `ERROR_STOPPED_ON_SYMLINK` (E9); both map to `Reparse` → 403.
/// `share` is `Pinned` (FILE_SHARE_READ; the existing open_pinned_leaf contract) or
/// `Served` (READ | WRITE | DELETE: never blocks in-place rewrites or leaf renames; the root
/// itself is pinned, A20) for Host-served responses; pass-through verification opens the
/// leaf `Pinned` (READ only) and keeps that very handle as the hold (A23).
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
  ancestors (cross-volume included); a root requested through a **session-defined drive
  letter** (`QueryDosDeviceW` names a `\??\` target, as `subst` does) is **refused at
  admission** with the final path in the fix text (a `DefineDosDevice` with
  `DDD_RAW_TARGET_PATH` can evade this heuristic; UX only) — fail-early UX, since the final-path
  spelling and R7 already refuse every such URL (A21, §4.5); UNC/network roots are refused at **admission** by the `AdmittedRootUrl`
  constructor (it requires a drive letter; R3 governs request URLs) and therefore cannot be admitted
  app roots. KEL-254's "single Windows path owner" (`kel254` L187, L200, L237) keeps naming
  `keld-core` boot as the policy owner; the mechanism is `keld-update`'s.
- **Session root pin (A20).** The retained root `Dir` is shared `READ | WRITE` and
  **never `FILE_SHARE_DELETE`**, so while the application runs no principal can rename or
  delete the root (E9: the parent's rename fails `ERROR_SHARING_VIOLATION`, 0/9
  differentials; with `DELETE` shared the swap succeeds and 4/9 cells — exactly the
  pass-through script, `fetch`, XHR and image — read OUT bytes) and no ancestor can be renamed
  while a handle beneath it is open (E9 share-mode table: `ERROR_ACCESS_DENIED`). Both
  existing handles already have this share mode, so no handle is added: the dev stage's
  admission open shares `READ | WRITE` (`app_session.rs:1495-1498`) and the installed `tree`
  comes from cap-std `open_dir_nofollow`, whose `dir_options()` shares `READ | WRITE` "so that
  directories can't be renamed or deleted underneath us" (`cap-primitives-4.0.3/src/windows/
  fs/dir_utils.rs:61-72`; `load.rs:1088`, `:1103-1105`; `oflags.rs:28-36` strips `DELETE` from
  every cap-std directory open). The pin is therefore **not new relative to r5**, which
  retained the same handles; W3 makes the share mode normative (AC26) so a refactor cannot
  widen it. `Share` (Q1) governs **leaf** opens only. **Cost:** the root cannot be renamed or deleted
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
                          DecodedSeparator, DotSegment, OutsideRoot, Component(String), TooLong,
                          ShellLink }
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
| R10 | the final component's extension is `.lnk` (ordinal ignore-case, after NFC) | `ShellLink` → 403 | r7, scored in §4.5: Chromium's loader follows a shell link as a redirect (E9), the Host would serve it raw (E9/E10); one refusal in every context removes the browser-side shell-link parse and makes all consumers (W3 serving, W1 Source, in-root navigation) agree |

Divergence from Electron, stated plainly: Electron serves any readable `file:`; W3 serves
only the admitted root, refuses `~` spellings and paths over 100 bytes (pre-existing package
grammar), refuses every reparse point and every `.lnk` (R10), and leaves `file:` workers
unconstructible (A22). T1 records these.

**Refusal and response contract (A7, A23).** For a `file:` URI the handler always decides,
and the decision depends on `ResourceContext` and on the request's `Origin` header (AC24): a
**document** (1), a **`fetch`/XHR** (7, 8) or a **CORS-mode** request (`Origin` present) that the
parser and opener accept is answered `200` with the bytes of the verified handle and Host
headers (`Content-Type` from one `keld-wv` table — apps override it through S1;
`Content-Length` is Host-owned; the baseline CSP on documents only; no
`Access-Control-Allow-Origin` is needed, E11); any **other** request that the parser and opener
accept gets **no response** while its verification handle is held (A23), and Chromium's file
loader reads the held object (its `Content-Type` is Chromium's own extension mapping, E9);
`404` empty when the parser accepted and the opener reported absent; `403` empty for every
parser refusal and every other opener refusal, whatever the request. No body on refusal, no
`Location`, no `Range` for Host-served responses until AC23 settles whether media range
requests arrive under `file://`. The Host table is a second extension → type map beside
Chromium's; its drift guard is one T2 device test that compares, for every extension in the
table, the Host's `Content-Type` with the pass-through `Content-Type` of the same file (a
difference is either a recorded divergence or a bug).

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

### 4.5 Serving contract (hso), filter scope, threading, and app-declared headers

**Filter scope (A2, A17).** Per webview, **in the one controller build body** (W2's factored
create, gh1188 D2) **before that webview's first `Navigate`** — `navigate_initial` for a
schema-1 boot, the first `loadURL` for a compat window; the same anchor holds for the WRR
handler, `NavigationStarting`, the context tracking and the self-test:
`AddWebResourceRequestedFilterWithRequestSourceKinds(L"file:*", CONTEXT_ALL, DOCUMENT)`.
The filter is matched against the normalized URI with the scheme lowercased (L1307), so
`FILE:`, UNC and host spellings all match; `https:`, `data:` and `blob:` never raise the
event and never touch the UI thread. **All source kinds are covered**: the worker kinds
(`SHARED_WORKER | SERVICE_WORKER`) for `file:*` are registered **once per environment** on the
first live webview (L72: those requests are raised for every CoreWebView2 with a matching
filter, so one registration avoids duplicate handling) and re-registered on the next live
webview when that one closes (AC23 cell). E11: a `data:` SharedWorker's `importScripts` raised
WRR only with the worker kinds registered. Never `*`.

**Serving (hso; A7, A23; owner decision relayed 2026-10-30, to be recorded on #1185 with B1).** Every `file:` request is
verified on the STA by the one parser (§4.4) and the one no-follow walk (§4.3) beneath the
pinned root (A20). Then:

- **Host-served** from the verified handle with Host headers (threading below; baseline CSP on
  documents only): `ResourceContext` 1 (top-level, frame, `<object>`, `<embed>` documents;
  WRR carries no frame id, and §4.4's correlation makes the top-level distinction), 7/8 (XHR
  and `fetch`; 156 reports `fetch` as 7), and **every request whose WRR headers carry
  `Origin`** (CORS mode: on 156 the value is `null`; no `Sec-Fetch-*` header is visible at WRR).
  E11 (`cm`, 2 runs each): passed through, every CORS-mode load fails — module scripts in the
  same and another directory, static `import`, `import()`, `modulepreload`, `<script
  crossorigin>`, `<img crossorigin>`, `<link rel=stylesheet crossorigin>`, `preload as=font
  crossorigin`; Host-served, all of them load and execute, without
  `Access-Control-Allow-Origin`. `@font-face` (inline, external, `FontFace` API) and plain
  script/image requests carry no `Origin` and load on pass-through.
- **Verified and held** (pass-through): every other request (classic script 6, stylesheet 2,
  image 3, font, media, other 16). The verification walk's leaf open is `Share::Pinned`
  (`READ` only) and every directory component's handle comes from cap-std without `DELETE`;
  the Host **keeps exactly those handles** as the hold, sets no response, and Chromium's loader
  (`GENERIC_READ`, share `READ | WRITE`, compatible with the hold — E9 share-mode table) opens
  the pinned object. **Release** (r10, `hsoz`; proposed, replacing r9's dynamic handler after
E13/E14), on the STA:
  1. **Document retirement or webview close releases every hold of that document**, and nothing
     else does: no `WebResourceResponseReceived` handler is registered for holds. Reason: E13
     (follow-up 6) measured r9's dynamic handler (`hsodr`) on a real Vite 8.3.5 application at
     +7.27 / +6.17 / +8.49 % per document vs `hso` (pooled upper bounds 11.71 / 9.02 /
     12.49 %, AC20 c FAIL), attributed to the handler's registration (post-hoc block a1: `hsor` − `hso` +7.2 ms on
     doc0, +1.5 ms [−1.05, +4.65] on doc1, +5.8 ms on doc2),
     not the hold; `hsodr` also stayed registered after the first hold ("sticky"), so it never
     paid less than an always-registered handler. Without the handler (`hsoz`, E14) the hold's
     cost is small and sign-unstable across sessions: `hsoz` − `hso` +3.4 ms [−4.3, +13.8] on
     draw.io against an A/A floor (`hso2` − `hso`) of +6.4 ms [−4.3, +12.4]; on Vite doc0 +1.8 ms
     [+0.3, +7.0] (follow-up 8, no A/A arm) and doc2 −4.6 ms [−6.4, −0.5] (follow-up 7); the
     module workload holds nothing, so it says nothing about the hold.
  2. A per-webview bound `MAX_PASS_THROUGH_HOLDS` = 256: a request arriving at the bound is
     **Host-served** from the verified handle instead of passed through. E14: a document issuing
     320 distinct pass-through images held the first 256 and Host-served 64; all 320 decoded;
     peak 768 live handles (leaf + two directories per hold), 0 outstanding after retirement.
     Peak holds per document: Vite fixture 27 (26 images and one font; JS and CSS carry
     `Origin` and are Host-served — real Vite emits `crossorigin` on its CSS `<link>`, E13),
     draw.io 18 (peak 39 handles).
  3. **`keld dev` serves every `file:` request from the Host** (`hsa`): no pass-through and
     no hold, so an editor save never meets a pinned file. Cost, development only: draw.io
     +34.76 % [+31.20, +42.75] vs `e` (E14, confirming E8's ~+35 %); the Vite fixture's point
     estimates within ±1.5 % of `hso` (CIs up to +7.09 %) because its JS and CSS are
     Host-served already. Under `hsa` the Host still classifies every request and applies app
     headers (A15) and `Content-Type` per its **release** class, so dev and release differ only
     in who reads the bytes; AC24 adds dev/release differential cells. Containment in `keld dev`
     is the strongest of all modes (every byte is the verified handle's).
  E14 correctness (`rel` cells, `hsoz`): gap cell verified bytes 144/144 per run in both runs
  (swaps fail 32: 216, 5: 72) while the no-hold NC delivered the replacement 72/72 and OUT
  72/72; multidoc a → b → c → blank: every hold released at its document's retirement, 0
  outstanding, every exclusive open succeeds afterwards, and the no-retirement NC leaves 18
  holds with 18 exclusive opens failing 32 (3 runs); a held image cannot be rewritten in place
  (32) or replaced by rename (5) and both succeed after retirement (3 runs), while the no-hold
  NC succeeds while "held" (1 run; the gap NC also ran once per run pair). Cancelled pass-through loads (E12) are released at
  retirement like every other hold.
  E11 + E12 race evidence: without a hold an injected leaf rename delivered the replacement and a
  directory → junction swap delivered OUT 24/24 per kind; with the verification handle held the
  verified bytes arrived 288/288 in the gap cell (swaps fail 32, rename-over 5) while the no-hold
  NC was attacked 144/144, and swaps after release succeeded with the delivered bytes already
  verified (E11/E12, under r9's release; r10 releases only at retirement, E14). **Cost:** while a hold is open the leaf cannot be written, renamed or deleted (32/5).
- Evidence for the pass-through parser: E11 `pt` corpus, 0 differentials, 48 at-risk per
  context; E9, 0 over 337 non-race cells × 6 runs of all-context pass-through; without
  enforcement Chromium delivered bytes for 42 of 171 refused cells without the switch and 108
  of 174 with it.
- `RequestedSourceKind` is **not** the classifier: E9 found it `DOCUMENT` for every request.

**Keld-owned browser arguments (A22; KEL-66 invariant kept and extended).** The environment
options keep **empty** `AdditionalBrowserArguments` (`webview2/mod.rs:1430-1436`, test
`:4851-4875`), and no source may set `--allow-file-access-from-files` or any switch that lets
`file:` documents start workers. With that switch (E10, E11): a page-constructed `new
Worker('wesc.js')` runs, its `importScripts`/`fetch` of out-of-root files (directly and through
a junction) and the page's own `new Worker('../../out/wout.js')` deliver the OUT nonces (3/3
targets, every host mode), `blob:`/`data:` workers escape the same way, and none of those loads
raises `WebResourceRequested`, even with the worker source kinds registered (two host modes)
— the Host cannot see, let alone refuse, them; the switch also makes `self.origin` `"file://"`
and in-root frames' and `<object>` documents' DOM readable. It did **not** grant `http(s)`
universal access; `--allow-universal-access-from-files` alone changed nothing observable.
Without it (E11): page-constructed `file:` workers throw `SecurityError` (no out-of-root nonce
arrived; whether any load is attempted is not observable), `blob:`/`data:` classic and `data:`
module/shared workers run with origin `null` but every `file:` load from them fails (out-of-root
and junction `importScripts` raise WRR and get `403`; in-root `importScripts` is allowed yet
fails `NetworkError`; `fetch`/`import()`/nested workers raise no WRR and Chromium blocks them,
identically in an observe run), `blob:` module and shared workers fail to start, and
`serviceWorker.register` rejects — B15 resolved: **no escape without the switch**. The switch
also arrives without Keld's options: `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` merges with the
options' arguments, wins on conflict, and is not overridden by options `""` or a missing
options object (E11 `env-*`). Therefore (owner decision relayed 2026-10-30, to be recorded on #1185 with B1, r9; (1) and (2) add no `unsafe`): (1)
**`keld-wv` refuses to create the WebView2 environment** with a typed `KELD-*` error naming every
`WEBVIEW2_*` variable present in the Host's environment (read with `std::env::var_os`, which is
safe; the whole prefix — Q21 found names of unknown effect, so fail closed); E11 `env-af-refuseenv`
closes the escape; (2) **every Keld launcher of `keld-host` removes them** from the child's
environment with `Command::env_remove` (safe). Today the only such launcher is `keld-cli dev`
(`dev.rs:363`; `dev.rs:522` starts the cleanup sentinel, which creates no webview); an
installed application is started by the OS, so (1) alone governs it and a user whose
environment carries such a variable gets the typed refusal with fix text; (3) **loader
policies** (proposed r10, from E13's Q21 classification): before creating the environment,
`keld-wv` reads, under HKLM and then HKCU, `Software\Policies\Microsoft\Edge\WebView2\<key>` for
the keys `AdditionalBrowserArguments`, `UserDataFolder`, `BrowserExecutableFolder`,
`ChannelSearchKind`, `ReleaseChannels` (S1), `ReleaseChannelPreference` and `DowngradeVersion`
(S2; their value-name lookup is INFERRED to follow S1's). The loader matches a value named by
the process's AppUserModelID, else its compiled code name (the executable's file name, S4's
`/v WebView2APISample.exe`), else `*` (S1). Keld does not resolve its AUMID: it enumerates the
value **names** of each key and treats a name as applying when it is `*`, equals the
executable's file name (ordinal ignore-case), or does not end in `.exe` (it may be an AUMID —
fail closed); a value named after another executable is ignored. Existence decides, never
content; any registry status other than success or "not found" refuses. `AdditionalBrowserArguments`
(E13: registry only, read by the runtime client DLL) and `UserDataFolder` are refused under
either root: one injects the switch, the other moves the profile Keld owns. The
runtime-selection keys are refused under HKCU — per-user configuration that Microsoft treats
as user-modifiable and ignores for elevated hosts (S3); on the operator device the default
ACL gives the user read-only access to `HKCU\Software\Policies` (observed 2026-10-30), so
writing it needs an elevated token, but Keld follows Microsoft's split — and **honoured under
HKLM** only when the selected runtime folder is not writable by the launching user (checked
through the existing ACL reader `windows-permissions`; `ChannelSearchKind`/`ReleaseChannels`
can select a per-user Canary under `%LOCALAPPDATA%`, which this refuses); the boot diagnostics
record the selected runtime's version and image path. Mechanism: `RegOpenKeyExW`,
`RegEnumValueW` (names only) and `RegCloseKey` through `keld-wv`'s existing `windows` 0.61
dependency with one added feature, `Win32_System_Registry` (no new crate) — **new production
`unsafe` operations** that need an issue-scoped `keld-wv` `AGENTS.md` sanction and the unsafe
gate (scored below; B16). Microsoft's `concepts/security.md` (E13 S3) states that `WEBVIEW2_*`
overrides and HKCU policy overrides are ignored when the host runs elevated and HKLM overrides
are honoured; Keld checks both roots in every launch (an elevated launch may be refused for an
HKCU value the runtime would ignore — conservative, stated). `COREWEBVIEW2_MAX_INSTANCES`
(outside the prefix) only bounds retained runtime versions and is not refused.

**CSP is not a containment boundary.** Chromium enforces the served document's policy on its
subresources, but under `script-src 'self'` an out-of-root or junction-reached `file:` script
raised **no** CSP violation: only the Host's `403` stopped it (E11 `cx`). Containment rests on
the parser, the walk, the pin and the hold; CSP limits what in-root code the page runs.

**Residuals, stated plainly:**

1. **Document-lifetime pinning (release builds).** Every file a document passed through (in
   E14: 18 on draw.io, up to 27 per Vite document, at most 256 per webview) stays open share
   `READ` until that document retires or the webview closes, so it cannot be written, renamed
   or deleted meanwhile (32/5). Keld never writes or replaces a running installed version
   (KEL-254, `windows_baseline.rs:198-201`); A20 still permits in-place writes, so a writer
   outside Keld gets 32/5 until retirement — an availability residual. `keld dev` never pins
   because the Host serves every request there (`hsa`).
2. **Volume-letter shadowing.** Chromium resolves the drive letter on every open in the
   browser process's logon-session DOS-device map; the hold pins the object the Host verified
   but Chromium's open still resolves the letter. A literal `subst` letter cannot reach a
   pass-through read (final-path spelling + R7; E9: with literal `K:` URLs 5 of 12 cells —
   exactly the pass-through ones — read the rebound target). Shadowing the **real** volume
   letter of the final path was **not tested** (UNKNOWN); a shadowed letter would make
   Chromium open a different object while the Host's hold pins the verified one. Who can: a
   same-user medium-IL process (E9: its `DefineDosDeviceW` succeeded and the Host's session saw
   the mapping) and Legacy roles; AppContainer and LPAC children got `ERROR_ACCESS_DENIED`
   (E9). Whether every Keld Strict role is AppContainer/LPAC-equivalent for the DOS-device
   namespace is #1187's to state (Q20).
3. **`file:` workers are unusable** (`SecurityError` at construction, as in plain WebView2) and
   `blob:`/`data:` workers cannot load any `file:` script. The owner states this is a
   divergence from Electron, where `file:` workers work (relayed 2026-10-30; not re-measured on
   Electron in this revision — GH-551 oracle cell).

**Drive-letter binding (A21), scored (0–10, confidence).** The root URL is spelled from the
admitted handle's final path; admission refuses a root requested through a session-defined
letter (fail-early; §4.3). The open choice is whether to add a per-request check:

| Option | Containment vs defended principals | Cost | One owner / simplicity | Availability | Mean |
| --- | --- | --- | --- | --- | --- |
| **(q0) final-path spelling + admission refusal; no per-request check** | 8 (0.7): a literal-letter URL cannot reach a read (R7; E9 5/12 without it); real-letter shadowing stays open to principals that can redefine a device (same-user medium-IL, Legacy; Strict per Q20) | 10 (0.9): one comparison at admission | 9 (0.8) | 10 (0.9) | **9.25** |
| (q1) q0 + per-request `QueryDosDeviceW(letter)` equal to the admission-time device | 8 (0.5): E9 0 differentials after a completed rebind, but it cannot close a rebind between the check and Chromium's open, and real-letter shadowing is untested for it too | 6 (0.3): one more syscall per pass-through request, **unmeasured**, inside a pooled margin whose upper bound is +4.68 % | 6 (0.7): a second per-request root-identity input | 6 (0.8): after a rebind every later request is refused (E9: 8 of 12 cells, no bytes) | 6.5 |

**Decision: (q0).** The principals (q1) would stop either already act on a same-user Host
(INFERRED) or have forfeited root integrity (Legacy), and (q1) is racy and unmeasured. (q1)
becomes required, with an E8-style measurement, if Q20 shows that a Strict role can
redefine the drive letter.

**Shell links (R10), scored.** Chromium's loader follows a `.lnk` as a redirect whose target
raises a new WRR (E9); the Host serves a `.lnk` raw to documents and `fetch`/XHR (E10: the only
three corpus cells that changed class from `h`; the `lnk-in` `fetch`/XHR cells were also served
raw but stayed MATCH).

| Option | Containment | Electron fidelity | Consistency across contexts and consumers | Simplicity | Cost | Mean |
| --- | --- | --- | --- | --- | --- | --- |
| (L0) serve raw where Host-served; let Chromium follow it elsewhere, each hop verified | 8 (0.7): every hop verified; the browser process parses an app-shipped shell link | 5 (0.6): pass-through contexts follow as Electron does, Host-served contexts do not | 3 (0.9): one name, two behaviours | 9 | 10 | 7.0 |
| **(L1) refuse `.lnk` in every context (R10)** | 9 (0.7): no shell-link parse, no redirect hop class | 4 (0.6): refuses where Electron follows (code-decided; not measured) | 10 (0.9): one rule in the one parser for W3, W1 and navigation | 8 (0.8): one parser row | 10 | **8.2** |

**Decision: (L1).** Recorded divergence (T1); AC03 parser cells.

**Threading (A7, A23; fits `keld-wv` AGENTS L5–6).** The handler runs on the engine STA.
Pass-through verification is **synchronous on the STA** (parser + walk + attribute
read-back; no deferral, no response object): E9 per-request STA median 218.9 µs, verification
p95 302.3 µs; STA occupancy 4.1 ms (`v`) and 6.9 ms (`hs`, E10) per load. The hold's handles are
file handles owned by the STA-side hold table and closed on the STA. Only **Host-served
responses** (documents, `fetch`/XHR, CORS-mode loads, requests beyond the 256-hold bound and,
under `keld dev`, every request) take a deferral, and they use arm (b)
(E8's recommendation): a worker thread reads **from the file handle only** into a buffer and
posts completion through the tao `EventLoopProxy`; the STA creates the stream and calls
`Complete`. Every COM object (args, request, deferral, response, stream) stays on the STA. Arms
(c) and (d) are not adopted: (c) needs an instruction change (§10 B10) and (d)'s chunked
`IStream` is free-threaded — WebView2 called `Read` off the STA (0 of 21,090 reads on it, E8).
Contract: one response per Host-served request; a deferral completed exactly once on the STA or
the request answered `403`/empty on the error path; the stream complete before `Complete`; the
buffer bounded by the opened file's `size`, whose admissible maximum is the package member
bound once `keld-pack` names one (§10 Q9) — because arm (b) buffers whole files, that bound is
also the memory and availability limit for a large `fetch`/XHR of data, wasm or media. Served
bytes cost ~6.1–6.5 ms per MB whatever the arm (E8; that the cost sits in WebView2's
response-body path is INFERRED — arm p rules out dispatch and the Host open), so large
documents, `fetch`/XHR payloads and module graphs pay it; classic scripts, stylesheets and images
do not (E11's module workload paid 69.0 ms of STA work per load for 258 requests). The
per-request allocations are the recorded exception in §2.

**S1 app-declared headers (A15, A19).** The facade forwards `session.webRequest.
onHeadersReceived(filter, listener)` registration to the Host as a control message that
carries only "listener present"; **the Electron match-pattern matcher lives once, in the
facade**. When a listener is present the Host takes the WRR deferral for every **Host-served response**
(documents, `fetch`/XHR and CORS-mode loads) and asks the role (a pass-through classic script, stylesheet or image has
no Host response that could carry headers — INFERRED from the API surface, untested — so app
headers are **not** applied to them: a recorded divergence from Electron, whose
`onHeadersReceived` sees every response; T1 entry, AC15 e); the facade applies the app's `filter.urls` and replies
"unchanged" or the listener's headers; no listener → no round trip. Host-owned fields
(`Content-Length`, status line) are not overridable; `Content-Type` and the app's CSP are.
No wall-clock deadline while the role is alive: a slow role delays the page, as Electron's
blocking listener would; role retirement (`RoleGeneration` moved) fails every pending
request closed (`403`/empty) **once the role is known to be gone**; between the
`RoleGeneration` move and the successor's registration (or its explicit "no listener"), and
**only when the predecessor role had a listener**, the Host **holds** Host-served requests on the
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
subresources (E9 with the switch, E10 `hs` without it: identical outcomes; E11 cross-directory
12/12; AC25), so baseline and app policies MUST admit in-root `file:` subresources with `'self'`
or nonces (hashes UNTESTED) and never rely on a `file:` host-source, which this runtime did not
match (an exact `file:///<root>/…/ext.js` source blocked that script — a runtime quirk; CSP3
would admit it). CSP is not the containment boundary (above).

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
| `did-navigate(url, httpResponseCode, httpStatusText)` | `ContentLoading(n)` with `IsErrorPage == FALSE` **and no Host `404`/`403` DOCUMENT record for `n`**; status from the Host's own served record for `file:`; for allowlisted remote documents from `HttpStatusCode` at `NavigationCompleted` (`ICoreWebView2NavigationCompletedEventArgs2`; no `WebResourceResponseReceived` handler, whose registration E12/E13 measured at +7–11 ms per module-heavy load; `httpStatusText` is empty — a T1 divergence, as is the later ordering) | — |
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
(§4.7) and its `did-navigate` status comes from `HttpStatusCode` at `NavigationCompleted`.

### 4.9 Rejected alternatives

| Alternative | Why rejected |
| --- | --- |
| Virtual host / custom scheme | Forbidden by #1185; Chromium opens the files; changes the origin. |
| Veto-only for **documents** (Chromium loads allowed document bytes) | A pass-through document cannot carry the baseline CSP or S1 headers (no response object; INFERRED from the API surface) and would sit in the race window. For script/stylesheet/image veto-then-load is the adopted design: E9/E10 measured 0 parser differentials, and the residuals are stated in §4.5. |
| Host serves every `file:` byte (r1–r5 C2) | E8: every serving arm regresses `navToReady` ~+34–36 % vs Chromium's loader, CIs within [+28, +42] %, cold and warm (cold medians e 452.1 ms, b 612.9 ms); the per-MB cost is arm-independent (~6.1–6.5 ms/MB for a/b/c/d alike) and arm p (WRR + walk-open without a response: +0.34 % [−4.5, +5.2]) rules out dispatch and the Host open — its location in WebView2's response-body path is INFERRED. Over the 5 % budget without a waiver. |
| Verify-then-pass-through for every context, documents included | Cheapest (+0.49 % [−2.4, +4.3]; later pairs +2.67 % [−0.04, +4.75] and +2.77 % [+0.94, +3.68]) but documents would lose the baseline CSP and S1 headers, and `fetch`/XHR fail without the switch (E10). |
| `vh` hold chain (a **separate** share-READ handle chain until `WebResourceResponseReceived`) | Superseded by the adopted hold (A23), which keeps the verification handle itself: E11's separate chain closed the injected race 24/24 but left a verify → hold gap. `vh` cost +3.2 % [−0.3, +6.6] was measured on `v`, not on `hso`. |
| Root handle sharing `DELETE` (E9's probe control) | Persistent whole-root swap for pass-through subresources (E9 control: 4/9, exactly script, `fetch`, XHR, image). |
| Root URL spelled with a requested `subst` letter | E9: 5 of 12 cells (the pass-through ones) read the rebound target. |
| Per-request `QueryDosDeviceW` check | Scored in §4.5 (6.5 vs 9.25): racy, unmeasured per-request cost, every request refused after a rebind; required only if Q20 says Strict can redefine a device. |
| Classify by `RequestedSourceKind` | `DOCUMENT` for every request (E9); serves everything (E8 cost). |
| Serve `.lnk` raw / let Chromium follow it (L0) | Scored in §4.5 (7.0 vs 8.2): two behaviours for one name and a browser-side shell-link parse. |
| Rely on empty options only (r7) | E11: `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` merges, wins on conflict and is not overridden by options `""` — an inherited variable silently removes containment. Rejected. |
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

**Serving alternatives after E11, scored 0–10 (confidence):**

| Option | Containment | Electron fidelity | Header coverage | Cost | Simplicity / owners | Mean |
| --- | --- | --- | --- | --- | --- | --- |
| r6 `h` + switch | 1 (0.9): workers read out-of-root files with zero WRR events (E10/E11) | 7 (0.6): `file:` workers and modules work as in Electron; `self.origin` becomes `"file://"` | 5 (0.8): documents only | 9 (0.7): +2.52 % [+0.52, +4.37] (E10) | 3 (0.9): breaks KEL-66 | 5.0 |
| r7 `hs` | 6 (0.8): pass-through raced (E11: replacement/OUT 24/24) | 3 (0.8): every module script, `import()`, `modulepreload` and `crossorigin` load fails (E11) — default Vite/electron-vite output does not run; `file:` workers unusable; in-root frame and `<object>` DOM unreadable (E10) | 7 (0.8): documents and `fetch`/XHR | 9 (0.7): +3.54 % [+1.16, +4.68]; modules cost nothing because they never load | 9 (0.9) | 6.8 |
| serve every script (`hsm` = `hs` + context 6) | 8 (0.7): scripts immune; stylesheet/image still raced | 6 (0.7): modules work; `crossorigin` images/stylesheets/font preloads still fail | 7 (0.8) | 3 (0.6): module workload 158.5 ms (−40.9 ms vs `eaf`), but on the classic draw.io bundle chain serving ~24 MB of scripts brings back E8's ~+34–36 % (INFERRED from E8) | 8 (0.8): context-only rule | 6.4 |
| `hso` + verification-handle hold, dynamic handler (r9) | 9 (0.7) | 6 (0.7) | 8 (0.8) | 4 (0.7): E13 Vite +6–8 % per document vs `hso` (AC20 c FAIL), cost in the handler's registration | 6 (0.8): `Origin` classifier, hold table, handler registered per document | 6.6 (superseded) |
| **`hso` + hold released at retirement/close, bound 256, `keld dev` = `hsa` (r10, `hsoz`)** | 9 (0.7): Host-served loads immune; pass-through pinned to the verified object for the document's lifetime (E14 gap 144/144 ×2 vs NC attacked); residual: real-letter shadowing | 7 (0.7): every CORS-mode cell loads (E11); `file:` workers unusable; `.lnk` refused; `keld dev` never pins (Host serves everything) | 8 (0.8): documents, `fetch`/XHR, CORS-mode loads; classic script/stylesheet/image none (dev: all) | 6 (0.5): the hold adds no measurable cost (E14 `hsoz` − `hso` inside the A/A floor), but AC20 (a)–(c) failed in E14's replication with the session's baselines 9–11 % above E12's; the T2 gate decides | 8 (0.8): `Origin` classifier and a hold table released at retirement; no response handler | **7.6** |

`hso` + hold with the handler **always** registered (`hsoh`, E12) is rejected: module workload
+5.72 % [+1.77, +9.53] vs `hsm`, over the gate.

**Decision (r10, proposed; r8/r9's `hso` + hold kept, its dynamic handler replaced after E13/E14): `hso` + hold released only at retirement/close, bound 256, `keld dev` Host-serves everything (`hsoz`/`hsa`).** E8's per-MB
cost for large classic scripts does not apply: classic scripts stay on pass-through under `hso`.

**Browser-argument ownership, scored 0–10 (confidence)** (empty options alone — r7 — is rejected
in the table above):

| Option | Containment | Availability | Gate cost | Simplicity | Mean |
| --- | --- | --- | --- | --- | --- |
| (s) r8: in-process `remove_var` scrub at `keld-host` start + assertion | 9 (0.8): E11 `env-af-clearenv` | 9 (0.7): launches from any environment keep working | 4 (0.9): new production `unsafe` (`remove_var` is `unsafe` in edition 2024) with an issue-scoped sanction | 7 (0.8) | 7.25 |
| **(r) r9: `keld-wv` refuses while any `WEBVIEW2_*` is present + `Command::env_remove` in every Keld launcher** | 9 (0.8): E11 `env-af-refuseenv`; launcher removal INFERRED equivalent to `clearenv` | 7 (0.7): `keld dev` unaffected; an installed launch from an environment carrying such a variable is refused typed | 10 (0.9): no `unsafe` | 8 (0.8) | **8.5** |

Decision (owner decision relayed 2026-10-30, to be recorded on #1185 with B1): **(r)**.

**Loader-policy check (rule (3)), scored 0–10 (confidence)** (proposed r10):

| Option | Containment | Availability | Gate cost | Simplicity | Mean |
| --- | --- | --- | --- | --- | --- |
| **(p1) value-name enumeration in `keld-wv`, HKLM runtime keys honoured when the runtime folder is not user-writable** | 9 (0.7): superset of the loader's lookup without resolving the AUMID (S1, S4); device-unverified (B16) | 8 (0.7): enterprise fixed-runtime deployments keep working | 6 (0.8): three new production `unsafe` registry calls; one feature on the existing `windows` dependency, no new crate (the workspace `windows-sys` already enables the same feature) | 8 (0.8) | **7.75** |
| (p2) the same rule through the `windows-registry` crate | 9 (0.7) | 8 (0.7) | 6 (0.6): dependency-addition gate; its transitive versions against the lockfile unverified | 8 (0.8) | 7.75 |
| (p3) `reg.exe query` child at boot | 6 (0.7): resolved through `PATH`, parses localized text | 8 (0.7) | 9 (0.8): no `unsafe` | 4 (0.8): a process spawn per launch, unmeasured | 6.75 |
| (p4) (p1) but every listed key refused under both roots | 10 (0.7) | 5 (0.7): every HKLM-pinned runtime refused | 6 (0.8) | 9 (0.8) | 7.5 |

(p1) and (p2) tie; (p1) is chosen under the root rule that dependencies are std-first and minimal. Rejected outright: checking after launch (reading the
browser process's command line needs `NtQueryInformationProcess` or WMI, and the browser has
already started with the argument), and ignoring policies (an HKCU value injects the switch
with no Keld code involved, E13 S1/S3).

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
  `NavigationCompleted`, `Runtime.enable` + context tracking, the self-test (no
  `WebResourceResponseReceived` handler: E12/E13 attribute +7–11 ms per module-heavy load to
  its registration). Handlers live for the WebView lifetime on the STA; renderer crash ≠ Host
  crash; the admitted root `Dir` (shared without `DELETE`, A20) lives for the session and is
  dropped after the webview; the environment carries **no** browser argument and is created
  only when no `WEBVIEW2_*` variable is present and no refused loader policy applies (A22);
  pass-through holds are released only at document retirement or webview close, at most 256
  per webview, and none exist under `keld dev` (A23);
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
  `did-fail-load(-10)` for opener refusals, separate baseline CSP, `hso`/`hsoz` serving
  (classic script/stylesheet/image/font read by Chromium from the held object after verification,
  held until retirement; `keld dev` Host-serves every request;
  `.lnk` refused in every context; `file:` workers unconstructible; `file:` CSP host-sources
  unsupported; app headers applied to documents, `fetch`/XHR and CORS-mode loads only), remote `did-navigate`
  status ordering and empty `httpStatusText`. Blocks on T0.
- [ ] **T2 — one opener, admitted root, loading path, containment** (Windows W3 implementer):
  parser + goldens (AC05), `open_regular_beneath` + `tree()` + `open_pinned_leaf` migration
  (AC04), root retention and boot migration, `file:*` filter/handler and `Navigate` loading
  (AC01–AC04, AC06, AC17–AC19, AC22–AC28), `hso` classification, R10, session root pin,
  drive-letter admission, the extended KEL-66 test, the `WEBVIEW2_*` refusal, the `keld dev`
  launcher's `env_remove`, the policy refusal,
  the `Origin` classifier, the verification-handle hold and its release rules, arm (b) for
  Host-served responses. Merge blocks on its AC20 receipt (a, b, c) and B8, B16, the
  issue-scoped `crates/keld-wv/AGENTS.md` sanction for the registry calls with its unsafe gate,
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
| `wv2_file_origin_matrix` (device, ignored-by-default, STA harness as `media_acceptance`) | AC01–AC03, AC06, AC17–AC19, AC22–AC28 | `file:*` baseline + N2; plain-outside and `https` cells; per-rule at-risk counters split Host-served vs pass-through per context; E9's identity oracle per kind and E11's exclusive-hold same-object oracle; the pass-through parser corpus as `<script>`, stylesheet, `<img>` and `@font-face`; naive mutants 1a–1c, unverified pass-through, drop-R10; `ResourceContext`/`Origin` census incl. `<object>`/`<embed>` and every CORS-mode kind; whole-root swap with and without `DELETE` on a handle-free fixture root; injected race with and without the hold and the verify → hold gap cell; hold release on cancel/failure/navigation; `subst` rebind with the test seam; CSP cells incl. cross-directory `'self'` and a hash; worker cells with the switch on (NC) and off; `WEBVIEW2_*` launcher removal and refusal on/off, the policy cells (B16), the retirement-release, multidoc and 256-bound cells; ETW when elevated. |
| `wv2_navigation_signals` (device) | AC07–AC09 | Loads (i)–(v); page oracle; GH-551 differential against the same fixture in Electron v45.1.0; mutants per row. |
| `wv2_settled_eval` (device) | AC10–AC13 | Promise/statement/reject/throw/getter/Proxy cells; gap dispatch; replayed G1; role recovery via the existing `no_flag/windows/recovery.rs` harness; injected CDP failure. |
| `wv2_headers_received` (device) | AC15, AC16 | Listener before/after `loadURL` (ordering), absent, retired; `fetch`/XHR- and module-matching listener (one round trip each) and classic script/stylesheet/image-matching listener (zero); SVG/XHTML `Content-Type` override; forbidden-remote-script counter. |
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
  `CallDevToolsProtocolMethod`,
  `GetDevToolsProtocolEventReceiver`, `NavigationKind`) with local proofs and an issue-scoped
  `crates/keld-wv/AGENTS.md` update; independent unsafe gate on the final diff. The opener
  adds **no** unsafe, and the browser-argument defence adds none (r9: `std::env::var_os` and
  `Command::env_remove` are safe; r8's `remove_var` scrub is withdrawn); the loader-policy check
  adds `RegOpenKeyExW`/`RegEnumValueW`/`RegCloseKey` (A22 rule (3), B16). r10 registers no
  `WebResourceResponseReceived` handler for holds.
- **public API — mandatory:** `keld-guard` parser **and a navigation entrypoint that
  evaluates `web.navigation.allow` for a `Principal::Webview` outside `evaluate`** (today
  `path_in_scope` is private behind `evaluate`, which refuses webview principals; the
  `file:` allow forms of B12 are matched on parser components by this entrypoint, never by
  `path_in_scope`); `keld-update` opener/accessor;
  `WebEngine::eval`, `WebviewSpec.app_resources`, `AppResourceSource`, `AppWindowEvent`
  (drops `Copy`)/`AppWindowCommand` variants; `@keld/electron` events, `executeJavaScript`,
  `onHeadersReceived`; `keld migrate` output key.
- **permission model — mandatory, independent reviewer:** two name owners; refusal table; `hso` classification and verify-then-pass-through with its
  stated residuals; the `Origin` classifier; the verification-handle hold; R10; session root
  pin; drive-letter admission; Keld-owned browser arguments (launcher removal, refusal, policy refusal);
  the hold's retirement-only release, the 256 bound, `keld dev` Host-serving everything and the
  document-lifetime pinning residual;
  allowlist-first cancel with +0; served-document identity; hardlink read-back; separate
  baseline CSP; fail-closed headers; eval context + generation gates; allowlist derivation.
- **dependency addition:** none (one feature, `Win32_System_Registry`, on `keld-wv`'s existing `windows` 0.61; lockfile unchanged).
- **wire protocol — #539/#540 owner:** control-plane kinds; the headers round-trip record (B11).

## 9. Perf impact — P1 measured; B8 reopened by E14

Budget: architecture 01 §5 cold start → first paint; > 5 % against Chromium's own file
loader (arm e) needs a written waiver. Metric used by E8–E10: `navToReady` (`Navigate()` → the
page's double-rAF beacon after `load` and the last chain script). E8's first paint is the
"Loading..." spinner before the large scripts (+0–8 ms on 68–80 ms) and is not
decision-grade.

| Atom | E8–E10 result |
| --- | --- |
| Census | E8 workload: 19 `file:` requests and 24,119,045 B per load (18 files; `app.min.js` 9,865,843 B, `stencils.min.js` 7,688,864 B); by context 1 × document, 3 × stylesheet, 2 × image, 11 × script, 2 × other, **0 × `fetch`/XHR** (E10); 0 non-`file:` requests raised; no preload, so draw.io's later UI loads (and any `fetch`/XHR they issue) are outside it |
| Work | Host STA work per load in the serving arms: a 20.1, b 9.2, c 4.7, d 7.9 ms (cold medians); `hs` 6.9 ms; verification 200.7 µs median / 302.3 µs p95 per request (E9 `v`) |
| Queue/copy | served bytes cost ~6.1–6.5 ms/MB (r² ≥ 0.99; arms a/b/c/d/al alike, so not threading, Host buffering or `Content-Length`; location inside WebView2's response-body path INFERRED); arm a's `SHCreateMemStream` copy on the STA; arm d's `IStream` read off the STA |
| Clock | `Instant` (QPC), `Navigate()` → beacon; the product-boot pre-spawn clock is **unmeasured** |
| Statistic | median, p95, percentile bootstrap CI (10,000 resamples) of the ratio of medians and of paired differences; 2 warm-up + 30 measured rounds per arm per class, arms rotated per round; E9/E10 pool two independent blocks (n = 60); idle 96–97 % and no `rustc`/`cargo` during runs |
| Artifact | keld-research note 268, runs `gh1185-p1-serving-cost-2026-10-30` (E8), `gh1185-p2-diff-hybrid-2026-10-30` (E9), `gh1185-p2-hs-2026-10-30` (E10) |

| Arm | `navToReady` vs e (ratio of medians, 95 % CI) | Verdict |
| --- | --- | --- |
| e Chromium file loader | cold median 452.1 ms [436.1, 470.3] (E8); 449.0 ms (E10) | baseline, not shippable |
| a/b/c/d Host serves every byte (sync; deferral + worker read; worker stream; chunked) | cold +34.2 to +36.1 %, warm +34.4 to +35.3 %, CIs within [+28, +42] %; b 612.9 ms, +35.6 % [+29.5, +42.1] | rejected (§4.9) |
| p WRR + walk-open, no response | +0.34 % [−4.5, +5.2] cold; +0.45 % [−2.5, +2.8] warm | attribution: dispatch and the Host open are not the cost |
| v verify-then-pass-through, every context | +0.49 % [−2.4, +4.3] (E9 first pair); +2.67 % [−0.04, +4.75] (E9 follow-up); +2.77 % [+0.94, +3.68] (E10) | rejected (§4.9) |
| vh = v + hold chain | +3.2 % [−0.3, +6.6] (measured on v, not on h/hs) | recorded, not adopted |
| h r6 hybrid (documents served; ran with the switch) | +2.40 % [+0.26, +4.62] (E9); +2.52 % [+0.52, +4.37] (E10) | rejected: needs the switch (§4.9) |
| hs (r7: documents and `fetch`/XHR served, no switch, no-`DELETE` root, final-path root) | +3.54 % [+1.16, +4.68] pooled; blocks +2.12 % [−0.40, +6.51] and +4.11 % [+1.31, +5.30]; `hs` − `h` paired −1.8 ms [−11.8, +11.0] | rejected after E11 (CORS-mode loads fail; pass-through raced) |
| hsm = hs + Host serves every script, module workload (E11: `index.html` + 257 ES modules, 2,930,290 B; metric `navToReadyHost` to the module graph's `load`) | 158.5 ms [155.6, 167.8] vs `eaf` (Chromium's loader **with** the switch, not shippable) 206.6 ms [195.1, 214.0]: ratio −23.3 %, paired −40.9 ms [−51.3, −35.8]; Host STA 69.0 ms per load, per-request median 238.8 µs; `e` and `hs` cannot load the graph | `hso` serves the same 258 requests on this workload (every module request carries `Origin`; INFERRED) |
| hsmn NC = hsm + 10 ms STA sleep per request | hsmn − hsm +2705.6 ms [2700.8, 2713.5], predicted [2000, 3000]; per-request +10.33 ms in [9.5, 12] | passed |
| hso (no hold), E12 | draw.io +2.93 % [+0.65, +3.95] vs e (E12 f4; e 427.0 ms) and +2.03 % [−0.54, +3.57] (f5; e 420.1 ms); modules −2.00 % [−4.82, +2.04] and −2.77 % [−6.64, +0.05] vs `hsm` | reference |
| hsoh = hso + hold, `WebResourceResponseReceived` always registered | draw.io +1.65 % [+0.59, +2.82] (pooled upper bound 2.82; blocks 5.19, 3.12) **PASS**; modules +5.72 % [+1.77, +9.53] vs `hsm` (blocks 9.09, 15.36) **FAIL**; attribution on modules: `hsor` − `hso` (handler, no hold) +11.1 ms [+4.7, +18.4], `hsoh` − `hsor` (the hold) +0.3 ms [−5.7, +4.2] | rejected |
| hsod = hso + hold, handler registered only while a hold exists (r9 basis) | draw.io +0.67 % [−1.65, +2.43] (blocks 4.47, 1.55) **PASS**; modules −1.51 % [−5.53, +1.23] (blocks 1.91, 3.67) **PASS** — with **0** holds on that workload, so the handler never registered; STA on draw.io 7.7 ms per load, longest block 2.9 ms | superseded by E13 |
| hsodr = hsod + re-registration at `NavigationStarting`, Vite 8.3.5 application (E13) | per document vs `hso`: +7.27 / +6.17 / +8.49 %, pooled upper bounds 11.71 / 9.02 / 12.49 % **FAIL**; `hsor` − `hso` +7.2 ms (the handler), hold ≈ 0; the handler stays registered after the first hold | rejected |
| **hsoz = hso + hold released at retirement/close, bound 256** (chosen, r10), E14 follow-up 7 | (c) Vite vs `hso` −3.47 / −1.21 / −2.26 % (upper 2.86 / 4.18 / 0.73) PASS; (a) draw.io vs `e` +3.72 % [+1.27, +7.67] FAIL; (b) modules vs `hsm` +7.63 % [+1.17, +11.68] FAIL; `hsoz` − `hso` +10.3 ms [−7.3, +23.6] and +10.4 ms [−6.1, +13.1] | (a), (b) FAIL |
| `hsoz`, E14 follow-up 8 (pre-registered replication: 4 blocks, n = 120 per arm, A/A arm `hso2`) | (a) +3.71 % [+0.69, +6.48] FAIL (per-block `e` medians 441–507 ms vs E12's 420–427); (b) +9.01 % [+4.70, +11.83] FAIL, with `hso` itself +6.78 % [+3.13, +10.10] vs `hsm` (E12: −2.0 %, −2.8 %); (c) doc0 +3.81 % [−2.09, +8.55], doc2 +3.59 % [−0.82, +7.92] FAIL; `hsoz` − `hso` +3.4 ms [−4.3, +13.8] (a), +1.2 ms [−5.1, +8.6] (b), +1.8 ms [+0.3, +7.0] (c doc0); A/A +0.87 % [−1.47, +2.73] (a), +1.65 % [−2.57, +4.08] (b); every NC passed (12/12 blocks). The registered analyser dropped the `hso2` label (regex); verdicts come from the registered code with the crashing line guarded, A/A numbers from a fixed copy (amendment written before data was read) | all three gates **FAIL**; the hold's own cost is small and sign-unstable (Vite had no A/A arm); A/A intervals exceed ±2.5 %, so indicative only |
| hsa = Host serves every `file:` request (`keld dev`), E14 | draw.io +34.76 % [+31.20, +42.75] vs `e`; Vite fixture within ±1.5 % of `hso` | development mode only |
| E12 NCs (+10 ms STA per request) | draw.io blocks +140.4, +140.9 (f4), +134.6, +145.5 ms (f5) in [110, 170]; modules +2716.1, +2712.6 (f4), +2720.0, +2721.8 ms (f5) in [2000, 3000] | all passed |
| hsn NC = hs + 10 ms STA sleep per request | hsn − hs +136.6 ms [129.7, 147.4], pre-registered [110, 170] | passed (E9's hn − h: +134.2 ms [129.7, 143.7]) |
| g `Runtime.enable` from create (E1) | additive and serving-independent: +164 to +178 ms (gb − b, ge − e); ge vs e +39.9 % [+34.9, +43.8] | **over budget: B14** |

Limits the runs state: arm (f) (S1 round trip) was not run (needs a live compat role, B13);
E10's `h` and `hs` served documents used a permissive `default-src` policy; the chosen `hso` +
hold has no end-to-end number; E11's module arms used a synthetic module workload;
the workload issues no `fetch`/XHR before it stops, so `hs` served the same bytes as `h` and
the cost of Host-serving fetched bytes is unmeasured; E8's served responses carried
`Content-Type` only and E9's served documents a permissive `default-src` policy, so no number
includes a restrictive CSP's effect; one runtime, one device. B8 was resolved by measurement
for `hs` and is **reopened by E14** for the chosen `hso`/`hsoz`; the T2 re-measurement with a
preload, arm (f) and the restrictive-CSP effect remain.

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
  multi-page `file://` applications unusable; (vi) W1's Source rule inherits R10 (`.lnk` → the
  new public `FileUrlRefusal::ShellLink`), so a `.lnk` source is refused there too; every reused
  contract is still an open draft
  (re-checked at approval).
- **B8 P1 report — reopened (E14)** — @amishabenramani, before T2 merges. E8–E10 resolved it
  for `hs`; E14 failed AC20 (a)–(c) for the chosen design, and `hso` vs `hsm` reversed from
  E12's −2.0 %/−2.8 % to +6.78 % [+3.13, +10.10] with STA occupancy 82.1 vs 76.6 ms per load —
  possibly real Host work (the `Origin` classifier is a candidate), not only session drift.
  (1) Attribute `hso` − `hsm` per request on the STA; (2) re-run AC20 with the A/A arm;
  (3) if any bound still fails, a written waiver (arch 01 §5) or a design change. Arm (f) and
  the restrictive-CSP effect also remain. Check: AC20 receipt.
- **B9 Probe evidence published** — Windows operator: keld-research note 268 (draft PR
  0monish/keld-research#38, head `3cd5a68`; runs `gh1185-w3-file-origin-probe-2026-10-28`,
  `gh1185-capstd-junction-2026-10-30`, `gh1185-p1-serving-cost-2026-10-30`,
  `gh1185-p2-diff-hybrid-2026-10-30`, `gh1185-p2-hs-2026-10-30`, `gh1185-p2-cells-2026-10-30`,
  `gh1185-p2-hold-2026-10-30`, `gh1185-p2-vite-q21-2026-10-30`, `gh1185-p2-hsoz-2026-10-30`) merged, so every citation here resolves to a public path
  on the default branch. Check: PR merged; note path stable.
- **B10 `keld-wv` threading instruction** — instruction owner through `.agents/instructions.md`:
  `crates/keld-wv/AGENTS.md` L5–6 must state that a worker may read a **file** handle while
  every COM/platform handle stays on the STA (arm (b) for Host-served responses and arm (f) depend on that reading; this
  spec interprets the current text and cannot rely on its own interpretation), and that the
  per-Host-served-response buffer, navigation-signal, eval-settle and header round-trip payloads are the
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
  +164 to +178 ms (`ge` vs `e` +39.9 % [+34.9, +43.8]); per-script segments vs `e`
  (`results-ge.json` `segmentsVsE.ge`): `app.min.js` +80.4 ms, `extensions.min.js` +98.9 ms,
  `stencils.min.js` −0.4 ms; §4.7 step 1 keeps it on from create for context binding. Before T3: a written
  waiver (architecture 01 §5) or a design change (for example enabling it lazily, which
  reopens A12/AC12) with an E8-style re-run. Check: the decision recorded on #1185.
- **B15 `blob:`/`data:` workers under `file://`** — **resolved by measurement (E11)**: without the
  switch no worker kind delivered an out-of-root byte (AC28 i); with it every kind escaped with
  zero WRR events. Consequences taken: the worker source kinds are registered (§4.5) and
  browser arguments are Keld-owned (A22).
- **B16 Loader policies (rule (3), A22)** — Windows operator, before T2 merges. (i) HKCU cells
  (written from an elevated token of the same user — a UAC action; the Host under test runs
  non-elevated): each key of rule (3) with value name = the probe's executable file name,
  separately `*`, and separately an AUMID-shaped name (`Vendor.App!App`); a `UserDataFolder`
  value named `*` is pre-declared as not applied by the loader (S1); `AdditionalBrowserArguments` carries `--allow-file-access-from-files`. Check
  without Keld's check that the value reaches the runtime (worker cells escape; the
  `UserDataFolder` value is the folder used) and with it that every cell is refused typed.
  (ii) HKLM cells (elevated operator, a user UAC action): `AdditionalBrowserArguments` and
  `UserDataFolder` refused; `BrowserExecutableFolder` at an administrator-only copied runtime
  launches and the diagnostics name it, and at a user-writable copy is refused; a value named
  after another executable is ignored. NC: drop
  the check → the HKCU switch cell delivers OUT nonces. Check: receipt with (i), (ii) and the NC.
  The production `unsafe` sanction in `crates/keld-wv/AGENTS.md` lands with T2. If the
  enumeration cannot see a value the loader applies, W3 does not ship until a mitigation is
  designed.

**Open questions:**

1. **Q1 (owner):** `Share::Served` = `READ | WRITE | DELETE` (recommended) or `READ | DELETE`,
   for **leaf** opens (Host-served responses, pass-through verification) only; the root pin never
   shares `DELETE` (A20).
2. **Q2 (owner):** dev stage validated by the package grammar at `keld dev` time so R8/R9
   refusals surface before serving (recommended, with a diagnostic naming the path).
3. **Q3 (#1154):** Electron error texts for sync throw / replacement; facade's pinned major.
4. **Q4 (#542/KEL-142):** `eval` destination signature on macOS now with a typed "not owned"
   refusal (recommended) or native completion semantics.
5. **Q5 (P4) partly answered:** E9 (with the switch, `self.origin "file://"`) and E10 `hs` (no
   switch, `self.origin "null"`) showed `'self'` admitting same-directory `file:` scripts and an
   exact `file:` host-source blocking its script;
   cross-directory in-root `'self'` (AC25 T2 cell) and the production baseline grammar remain
   open.
6. **Q6 UNKNOWN:** whether `keld dev` stages pass `validate_windows_package_paths` today and
   how `keld dev` reports a reparse component (OneDrive/dedup/WOF roots).
7. **Q7 (P2) UNKNOWN:** `IsErrorPage` for a Host `404`/`403` top-level response; DOCUMENT WRR
   count per pending window with the different-URI iframe mutant; 30 loads.
8. **Q8 UNKNOWN:** media `Range` requests under `file://` (AC23).
9. **Q9 (keld-pack):** maximum package member size bounding buffered arms — also the memory and
   availability limit for Host-served `fetch`/XHR of large data, wasm or media files (arm (b)
   buffers whole files).
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
19. **Q19 resolved by measurement (E10):** `--allow-file-access-from-files` did **not** grant
    `http(s)` universal access, but it is a containment escape through workers (A22), so Keld
    sets no browser switch.
20. **Q20 (#1187):** whether every Keld Strict role on Windows is AppContainer/LPAC-equivalent
    for the DOS-device namespace (E9: AppContainer and LPAC `DefineDosDeviceW` →
    `ERROR_ACCESS_DENIED`; a medium-IL same-user child succeeds). If not, the per-request
    `QueryDosDeviceW` check (§4.5, q1) becomes required with a measured cost.
21. **Q21 RESOLVED (E13, A22).** Microsoft's loader reference (S1, retrieved 2026-10-30) lists the
    overriding variables `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`, `WEBVIEW2_USER_DATA_FOLDER`,
    `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`, `WEBVIEW2_CHANNEL_SEARCH_KIND`,
    `WEBVIEW2_RELEASE_CHANNELS`, and the registry keys of rule (3); the device confirms
    `USER_DATA_FOLDER` silently wins over the options and `BROWSER_EXECUTABLE_FOLDER` runs a
    copied runtime (a missing folder fails `0x80070002`, no fallback). Not overrides:
    `WEBVIEW2_WAIT_FOR_SCRIPT_DEBUGGER`, `WEBVIEW2_DEFAULT_BACKGROUND_COLOR` and, outside the
    prefix, `COREWEBVIEW2_MAX_INSTANCES`; `WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER` exposes WebViews to
    other processes together with the arguments variable (Microsoft warning). A string census of
    the loader and runtime DLLs finds further names of unknown effect (`WEBVIEW2_USE_EDGE_VIEW`,
    hosting-mode and game-bar names), so the environment refusal and launcher removal keep the
    **whole prefix** (fail closed); the policy rule is rule (3).

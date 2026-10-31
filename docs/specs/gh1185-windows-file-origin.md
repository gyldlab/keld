# Spec: Windows WebView2 `file://` navigation, app-code resource containment, honest navigation signals and settled `executeJavaScript` (GH-1185 W3)

Status: draft (revision r17, local draft: answers 0monish's review of `fa49dcfb` (4429132878,
4429140665, 4429148703, 4429148709, 4429148711) and sweeps the whole spec for the same five
classes — two independent case-identity locks: admission reads the flag on the root and every
ancestor, and pass-through requires an ordinal-exact root spelling (§4.4, AC30); Q9 decided
with a per-file cap, a reserved in-flight budget and a concurrency cap, provisional until T2
(§4.5, AC31); the eval terminal commits in the fence iff the enqueue succeeds, owner
confirmation requested (§4.7 step 4, B21); AC11 (vii) splits its two forced orders; T2 owns
the Host-attempt deadline and execution becomes T2 → T4 → T3 (§6); one spelling-differential
table (§4.4) and one resource-bound table (§4.11); E16 publishes the case-flag probes and their limits (§4.4); the eval ticket is the eval use of W1's one `AdmissionTicket` (§4.7 step 4); revision r16 records the #1192/#540 owner's
five B21 decisions
([#1311 6532409232](https://github.com/gyldlab/keld/pull/1311#issuecomment-6532409232)) — one Host-minted immutable eval ticket, admitted by a new
nonblocking `PrimaryRouterHandle::try_admit_eval_settle` in one synchronized transaction that
the STA's G-change and the role retirement update synchronously, record-kind-aware provenance
checks and terminals limited by transport certainty (§4.7 step 4, B21; T3 stays DO NOT
MERGE); the `WEBVIEW2_*` refusal and launcher removal enumerate with `std::env::vars_os()` and
compare the prefix ASCII-case-insensitively (§4.5, A22, AC28 (iv′)); T0 no longer waits on
B8, which only T2's cells clear (§6); revision r15 — a whole-spec defect-class sweep — every negative
control re-checked against the fail-closed rules of B18, B19 and Q25 and made reachable,
re-targeted to its rule's owner, or pre-declared not exercised with the unit test that
exercises it; §4.6 rule 1 and B19's cancel moved into T2 with the loading path they protect;
cross-owner citations re-read at current heads (W1 `9bc56726`, W2 `95594e88`, #541
`57280204`, keld-research `c1d15a2`; the navigation-signal oracle is #537's, not GH-551's,
and the `webContents` facade owner is #539, not #1154); revision r14 adopts the #1192/#540
owner's Q25 decision — the STA's
eval settlement is local only, and delivery is #540's final authenticated transport admission
revalidating #1192's document and role stamp (B21; r14b scores AC06's association at a Host
audit and a state-machine fixture, r14c scores AC11 at the STA's settle-post audit); revision r13 answers 0monish's re-review of `6f245546` — Host attribution only
by causal proof (header-proven mode, no URI/order exemption; unproven mode refuses, B18), one
header mechanism for every `file:` attempt including reload, top-level DOCUMENT credit only by
navigation-specific proof (B19), cross-clock eval binding behind a transition gate and a
fresh-loader rule with explicit eval phases, one allowed-writer ACL rule for every
loader-selected runtime (B20, r13e); W3 delivers no Windows `file:` application unless Q22 (a) and
(b) pass (§1); revision r12 answers rpipaliya's review of `d60e0a8e` — open intent and
directory-chain ownership in the resource API, eval delivery linearized at the STA; revision
r11 answers 0monish's review of `d60e0a8e` — resolved AUMID, Host
attribution that fails closed on ambiguity, CDP context binding inside one clock; revision r10 — **hso** serving; pass-through holds released only at document retirement or webview close, bound 256, `keld dev` Host-serves everything (`hsoz`/`hsa`, E13/E14); loader-policy check (Q21 resolved, B16 widened) adds production `unsafe` registry calls; AC20 probe gates failed and are carried to T2 with an A/A arm; r9 → r10 after E13/E14; r8 → r9 after measured hold cost and release path; r3 followed isolated hostile reviews of `571f6bda` and `5ecd3496`)
Linear: GH-1185 (GitHub tracker under #517; parent program #1182 W3; webContents owner #539) ·
Owner: @amishabenramani (real Windows operator) · Updated: 2026-10-31

Code citations are at `origin/main` `457a9c3b37f71179f167bd1f8c706c461cc4c479`. Device
evidence is one runtime and one OS build: WebView2 Evergreen `156.0.4314.50` on Windows 11
`10.0.26300`, `webview2-com(-sys) 0.38.2` (SDK 1.0.3650.58). The native probe is the
[#1185 probe report](https://github.com/gyldlab/keld/issues/1185#issuecomment-6461689232)
(3 identical baseline **runs**, each loading two documents — `index.html` and
`index.html?x=2` — with 68 `WebResourceRequested` events per run of fixture demand, not per
load and not the pinned application's demand; single-fault controls N1–N6; `results.json`
sha256 `acd3cbe4…34e6`; 31 request cells). Its published form is keld-research note 268
(PR [0monish/keld-research#38](https://github.com/0monish/keld-research/pull/38), open and
unmerged, head `1524f3e1`, now `816794e1`, runs `artifacts/runs/gh1185-w3-file-origin-probe-2026-10-28/` and
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
(PR [0monish/keld-research#38](https://github.com/0monish/keld-research/pull/38), open and unmerged, B9).
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
four-block replication with an A/A arm. **E15** — the Q24 census, run
`gh1185-q24-wrr-order-2026-10-30` (sub-bundle `q24/`; note 268 §4.zzzzzzz; published at
`0monish/keld-research@c1d15a2`, full `c1d15a25f94455dd137b18eefe58553012473afe`, unmerged like
every run in PR #38, B9): the STA order of subresource WRRs against their document's
`ContentLoading`, and AC26 (vi)'s forced adverse order. **E16** — the r17 case-flag probes, run
`artifacts/runs/gh1185-r17-case-flag-probe-2026-10-31/` (note 268 §4.zzzzzzzz; published at
`0monish/keld-research@816794e1`, full `816794e149c37eaa7df16ad2163bca38952e4b94`, unmerged like
every run in PR #38, B9): Python `ctypes` and WSL scripts, n = 1 run per script, no Keld, WebView2
or Rust binary involved; §4.4 states what it shows and what it does not. Numbers below are quoted
with their 95 % CIs from the runs' `results.json` and are not strengthened.

This draft authorizes no production code, no permission and no public API. Implementation
starts only after an exact-content `Status: approved` decision and T0's blockers; each later
task merges only after the review gates in §8 and the blockers its §6 entry names, because
several blockers (B8, B16–B20) are cleared by T2's own device cells, so none of them gates
T0 (§6).

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
preloads), and every request whose admitted-root prefix is not spelled ordinal-exact (§4.4).
**Verified and held**: every other request (classic script, stylesheet, image,
font, media, other) — the Host keeps the verification handle open (share `READ` on the leaf,
no-`DELETE` handles on every directory component), sets no response, lets Chromium's loader
open the same pinned object, and releases the hold only at document retirement or webview
close (at most 256 holds per webview; beyond that the Host serves); under `keld dev` the Host
serves every `file:` request. Every containment refusal is `403`/`404` with an
empty body; a capacity outcome is `413` or `503`, empty, exactly once per request (§4.5). Keld is the **sole owner of browser arguments**: it sets none, its `keld dev`
launcher removes every variable whose name starts with `WEBVIEW2_` in any letter case from the
Host's environment, `keld-wv` refuses to create each WebView2 environment while any such
variable is present (both enumerate with `std::env::vars_os()`, §4.5), and the Host refuses to start when a
WebView2 loader policy that injects arguments or replaces the profile is set, or a user-scoped one selects the runtime (A22), because
`--allow-file-access-from-files` lets workers read outside the root without raising
`WebResourceRequested` and an inherited `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` injects it
(E10, E11). It also refuses to create the environment when the runtime the loader selects
fails the allowed-writer rule (§4.5 rule (3), B20). For the verified name no object outside the root, no reparse point, no shell link
(`.lnk`), no ADS spelling and no `~` 8.3 spelling is admitted (the short-name residual is in
§4.4's table); CSP is enforced but is **not** the
containment boundary (§4.5). The residuals that remain are stated with the principals that can
use them (§4.5). Rejected: r1–r5 (Host serves every byte, +34–36 % `navToReady`), r6 (needs the
switch), r7 `hs` (module scripts and CORS-mode loads fail, and pass-through raced) (§4.9, §9).
The Host also emits Electron `webContents` navigation events that are each tied to one
WebView2 navigation and one Host-served document, and returns real settled
`executeJavaScript` values whose delivery is scoped to the document generation and the Bun
role generation current at dispatch. Nothing degrades silently: a Host whose CDP capability
self-test fails refuses to start the application with a typed error (AC13); if B17's premises
fail, `executeJavaScript` is a typed capability refusal instead (§4.7 step 1), and it does not
ship before B21 (§4.7 step 4).

Observable outcome on a real Windows device, once the delivery condition below holds: an application loaded from `file://` keeps
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
completion value, and a result for a replaced document is never newly admitted: unless its
final writer admission won before the revocation (then only its original caller may observe
it late, with its own generation's provenance, Q25), the correlation gets exactly one logical
terminal decision and at most one terminal wire result on a live link (§4.7 step 4, B21).

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
  on its original `NavigationStarting`; the Host never re-issues it (§4.6), nor, since r13, its
  own attempt (an attempt that cannot be attributed fails to its caller, §4.6 rule 1).
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

**Delivery condition (r13, stated plainly).** W3 delivers **no** Windows `file:` application
unless Q22 (a) and (b) both pass on the supported runtime (B18, B19): without (a) no Host
`file:` attempt is admissible — the schema-1 boot `Navigate` included — and without (b) no
`file:` document is credited Host-served. In-root page-to-page navigation (`in-root:**` and
literal forms) stays cancelled at +0 until Q27 passes or the owner accepts that divergence
(B19). `executeJavaScript` additionally waits on B3, B14, B17 and B21 (§6 T3).

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
literal `/` boundary, no percent-decoding ("This is not URL normalization", `:778-780`;
arch 03 §2 L148–156), including non-`file:` `web.navigation.allow` entries. The W3 parser
`file_url_root_relative` (§4.4) is the owner of **"`file:` URL → admitted-root membership +
root-relative components"** for three consumers: W3's `WebResourceRequested` serving, W1's
per-call `Source` rule, and in-root `file:` navigation decisions. W1 (#1307, revised in
parallel; published head `9bc56726`; its Source rule at L467–468
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
  argument and profile policies refused, user-scoped runtime policies refused; a selected
  runtime admitted only under the allowed-writer rule (B20)".
- Root `AGENTS.md` "Hot kipc/event-loop/guard paths … no steady-state allocation": the
  `WebResourceRequested` handler runs on the engine event loop and allocates the parser's
  components and the walk's directory chain (`DirChain`, boxed as `ResourceChain`) for every
  `file:` request and one response buffer per **Host-served response**
  (documents, `fetch`/XHR, CORS-mode loads, requests beyond the 256-hold bound and, under
  `keld dev`, every request; arm (b), §4.5), and the navigation-signal, eval-settle and
  header round-trip payloads allocate on the same loop. T0 records these as one named,
  measured exception (not a kipc path), bounded by E8–E10, the B10 instruction wording and
§4.11's resource bounds.
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
- `crates/keld-update/AGENTS.md` production-unsafe list ("handle kind/listing"): r17 adds one
  read in `src/windows_fs.rs`, `GetFileInformationByHandleEx(FileCaseSensitiveInfo)` behind the
  safe `directory_case` (§4.3). The function is already bound there (`windows_fs.rs:849`,
  `:887`) and `windows-sys` 0.61.2 exposes `FileCaseSensitiveInfo` (23) and
  `FILE_CASE_SENSITIVE_INFO` under the enabled `Win32_Storage_FileSystem`, so no dependency or
  feature changes; T2 updates the list under an issue-scoped sanction (unsafe gate, §8).

| Existing owner (reuse, no duplicate) | Exactly what this spec consumes | Decision / divergence |
| --- | --- | --- |
| [#1192 GH-542 checked navigation binding](https://github.com/gyldlab/keld/pull/1192) (**open draft**, `cd30f997`) | §4.2 `NativeDocumentPhase`, checked `u32`, +1 on real replacement commit, +1 **before** a trusted Host load (L110), **+0 on renderer provisional start/cancel** (L107) and same-document changes; §4.4 eligibility = verified resource identity; §4.5 "stale evaluation completion … never delivers to a new document"; N15 Bun recovery keeps the document; N16 one generation; N21 Windows UNKNOWN; L145 four bounded CONTROL records per role. | Reused as the only generation and phase model; W3 is its Windows oracle. Page-initiated navigations stay page-attributed (+0 on cancel, +1 at commit); W3 issues no Host load for them, so no #1192 amendment is needed for navigation (r13: the Host never re-issues a navigation, its own attempt included; an attempt that cannot be attributed fails to its caller, §4.6 rule 1). W3 needs #1192 T1 as a platform-neutral module (§10 B2). |
| `RoleGeneration` (`role.rs:189`) and the app-link retirement owner (KEL-75/KEL-143) | Per-role generation; retirement of role-bound correlations on recovery. | Eval and header round-trip correlations are keyed on it. |
| [#1190 GH-544 codec](https://github.com/gyldlab/keld/pull/1190) (**open draft**, `115af9ac`) | One Host-opaque codec (V7); V3 synchronous refusal of functions; **V4: accessor properties fail typed with getter counter 0** (accessors are never invoked); V6: "Proxy traps can run before returning"; the 4096-byte bound it claims for `executeJavaScript` results (L9–12). | Reused as the only codec for its domain and decoder ownership. The 4096-byte bound is falsified as an eval-result bound (D4). **V4 is not met by the chosen transport (v)**: CDP deep serialization invokes own enumerable getters and drops symbol/non-enumerable keys (§4.7); whether that is accepted for eval results (Electron's structured clone also runs getters) is the codec owner's decision (§10 B3/Q15). |
| [#1307 W1 preload facade spec](https://github.com/gyldlab/keld/pull/1307) (revised in parallel) | §4.4 Source rule step 3, §4.5 lifecycle (`ContentLoading` mints/revokes; allowed `NavigationId`), §4.6 navigation policy keys (`web.navigation.allow`, `onLoss`), §4.6a loading-path requirement, blocker B6; AC10 (the application document survives a vetoed navigation). | W3 closes B6 with L1. W1's `file:` Source rule and W3 share the one parser (above); r7 added R10 (`.lnk` → the new public variant `FileUrlRefusal::ShellLink`), so W1's Source rule refuses `.lnk` sources too (B7 vi). W1 r9 (`9bc56726` L593–600) already states W3's identity: the served record's in-root components, with the bytes as currently present (owner Q2 in 6491936458; §10 B7 ii). |
| [#1280 W2 window adapter spec](https://github.com/gyldlab/keld/pull/1280) (gh1188 `8c5e5d74`, r18, open) | Its T1: Ready without a window, D2 behind a debug test admission, D3, `create_app` skipped for facade boots, `HostInitialized` Ready; its T2: the D7 release proof and D2's product admission of schema 2 on Windows; its T3: D4, the in-loop `BrowserWindow` build from the factored controller build body (L945, L967), and the managed-window flip; L54–55: registry windows load an empty document and file-origin `loadURL` is W3's. | The compat boot model W3's A19/AC15 (a)/AC08 (g)(h) depend on is this **target**, not today's code (`app_session.rs:3431-3438` refuses; §10 B13). W3 owns the Windows `HostLoad` seam W2 defers to it; the facade vocabulary stays with #539/#459. |
| [#541 GH-541 spec](https://github.com/gyldlab/keld/pull/1123) and #460/KEL-79 | "no new web.navigate schema" (`gh541` L252 at `57280204`); full navigation policy is #460/KEL-79 (L258); first-proof eligibility from "boot/resource containment and native navigation evidence" (L126). | W3 provides that evidence for `file:` documents and nothing for other schemes (§4.8). The `will-navigate` rule in §4.6 is allowlist-first and needs no new grammar; the allowlist derivation by `keld migrate` is recorded for that owner (§10 B4). |
| [KEL-254 installed-root boot](kel254-windows-installed-root-boot.md) (approved) and `keld_update::ActivePackageSelection` (`crates/keld-update/src/windows_baseline.rs:207-216`: retains `_roots`, `_version: Dir` **and `tree: Dir`**; exposes `tree_root() -> &Path`, `:238`) | Admission of the installed root; AC7/AC8 access profiles; AC13 Legacy refusal. | W3 **lends the existing `tree: Dir` handle** through one borrowing accessor (`tree(&self) -> &Dir`); no reopen by path, no new handle (§4.3). `tree` comes from cap-std `open_dir_nofollow`, whose `dir_options()` shares `READ \| WRITE` without `DELETE` (`cap-primitives-4.0.3/src/windows/fs/dir_utils.rs:61-72`; `load.rs:1088`, `:1103-1105`; cap-primitives `oflags.rs:28-36` strips `FILE_SHARE_DELETE` from every directory open), so it already is the session root pin (A20). |
| `keld-update` cap-std leaf opener `open_pinned_leaf(parent: &Dir, leaf: &str)` (`windows_baseline.rs:589-599`: `read`, `FILE_SHARE_READ`, `FollowSymlinks::No`), used directly by `locate.rs:311, 375` and `windows_baseline.rs:606` (`open_machine_file`, whose callers are `helper.rs:207`, `load.rs:481`, `load.rs:1075`, `load/version_contents.rs:58`, `windows_baseline.rs:668`) | The one existing no-follow leaf open beneath a retained directory handle. | **Generalised**, not duplicated: it becomes the single-component case of `open_regular_beneath` (§4.3) and its callers migrate in the same PR, so "exactly one opener" is true (AC04). |
| cap-primitives 4.0.3 (already a workspace dependency through `keld-update` and `keld-native`): Windows `open_impl` = `manually::open` per-component walk (`src/windows/fs/open_impl.rs:29`); each component opened relative to the directory handle through `CreateFileAtW` → `NtCreateFile` with `RootDirectory = dir` (`create_file_at_w.rs:155-158, 242`), `FILE_OPEN_REPARSE_POINT` under no-follow (`:220`); `..` re-roots to an ambient `CreateFileW` (`open_unchecked.rs:60-105`) — unreachable here because the parser refuses `..` | The relative no-follow open mechanism and its already-reviewed `unsafe`. | Reused through the existing dependency; r1's claim that cap-std's Windows walk is "path-based" was wrong and is withdrawn. Named requirement left to a probe cell: a junction on an **intermediate** component is refused by the walk (AC03 cell; cap-std's `is_symlink` classification of mount points UNVERIFIED). |
| cap-fs-ext 4.0.3 `MetadataExt::nlink` (`metadata_ext.rs:30`), `DirExt::open_dir_nofollow` (`dir_ext.rs:87`) | Link-count read-back; no-follow directory step. | Reused for AC17 and the component walk. |
| `keld-native` `unsupported_reparse` (`crates/keld-native/src/fs.rs:1352-1357`: any reparse attribute that is not a symlink is refused) | KEL-130's reparse-tag policy. | W3 applies the same **"any reparse point at any component → refuse"** rule (symlink, mount point, cloud placeholder, dedup, WOF alike); consequence stated in §4.3. KEL-130 itself is **not** reused (app-grant policy; 8 MiB content cap below the pinned 9.9 MB bundle). |
| `keld-core` dev-stage admission: `validate_windows_dev_stage_acl` (`app_session.rs:1485-1500`) opens the root with `FILE_FLAG_OPEN_REPARSE_POINT` + `FILE_FLAG_BACKUP_SEMANTICS` and share `READ \| WRITE` (**no `FILE_SHARE_DELETE`**, `:1495-1498`), validates the owner-private DACL, then drops the handle; canonical layout `<project>/.keld/dev/<32-lowerhex>` (`:1668-1678`; `keld-cli/src/boot.rs:428`) | The dev-stage DACL policy (KEL-96). | W3 retains that validated handle as the admitted root (`Dir::from_std_file`) for the session; its share mode is the session root pin (A20), so no new handle is opened. The project path may carry `~`, spaces and non-ASCII: the root prefix is validated with `validate_fs_component` (`lib.rs:933-938`), never with the package grammar (§4.4). |
| `keld-core` boot file opener `open_relative_file_windows` (`app_session.rs:1729-1789`: Win32 path walk + `OpenOptions::open`) | Its callers in `validate_windows_boot_files` (`:1440-1479`). | **Deleted** in T2; callers migrate to `open_regular_beneath`. |
| `keld-guard` component grammar (`validate_windows_package_component` `:951-958`, `validate_windows_component` `:1145-1164`, `validate_portable_component`, `validate_windows_package_paths` `:975-1088`) and `validate_fs_component` (`:933-938`) | The one Windows component grammar (page-controlled components) and the ordinary fs component rule (Host-admitted root). | Reused unchanged. |
| `keld-wv` Windows backend: `create` ordering (`webview2/mod.rs:3560-3645`), `navigate` (`:3647-3659`), `eval` (`:3661-3674`), `NavigationStarting` observer (`:2276-2297`), `GuardInstalled` + deny-all `NewWindowRequested` (`guarded_navigation.rs:50-154`), `AppWindowEvent` (`engine.rs:142-143`, `#[derive(Copy)]`) | Create ordering, the guard mint, the event channel, the pool-callback-posts-via-`EventLoopProxy` shape. | Extended inside the same mint. New event payloads (URLs, outcomes) cannot be `Copy`: `AppWindowEvent` drops `Copy` (public API gate, §4.10). |
| `keld-core` `app_session.rs:3653`: `NavTarget::Html(inputs.html)` | The loading-path site. | **Must change**: L1. |
| KIPC frame cap `MAX_FRAME_LEN = 16 MiB` (`keld-ipc/src/lib.rs:113`) | Ceiling for any eval-result bound. | Cited in §4.7. |
| `@keld/electron` `WebContents` (`browser-window.ts:85-104`: only `destroyed`) | The facade surface W3 extends. | Extended by T3–T5 with the `webContents` facade owners #539 (Windows lanes, `executeJavaScript`) and #459 (the facade surface) (§10 Q3); #1154 is the GH-467 contextBridge/`ipcRenderer` spec, not this surface. |

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
such a filter should be added on only one, L63); `ExecuteScript` after `NavigationStarting`
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
**r13 gating:** a cell that needs the Host entry exemption or a Host-served credit for a Host
attempt runs in header-proven mode only (§4.6 rule 1, B18); a cell that expects a
page-initiated `file:` navigation to commit and receive an epoch (AC06 for a page-initiated
`n`, AC08 (b) for a `file:` target, the "proceeds" cells of (i) and (j)) is **blocked on B19**
— until Q27 passes, rule 2 cancels it at +0 and the cell reads blocked, never pass.
AC12's hooks 1 and 2, (e)/NC6 and NC7 run on a **page-initiated same-URL `https:`** replacement
(an allowlisted fixture origin; remote documents are evaluable, §4.7 step 2), because a Host
attempt clears the pair and the WebView2 slot at its issue-time +1 (#1192 L110) and
page-initiated `file:` navigations are cancelled under B19. AC08's NCs (c), (d) and (j′) are
device-level **rule not exercised** until Q27 (B19's cancel decides those links first); the
same single faults run as unit tests of the `keld-guard` navigation entrypoint (§7).
**r15 reachability.** Device cells issue Host attempts through the keld-wv entry point that
`HostLoad` uses (header-proven mode); only cells marked **[TARGET, B13]** need the
application's own facade call. AC11's replacements and AC12 (d)'s G-changes are Host attempts
or page-initiated navigations to an allowlisted `https:` fixture origin, and AC12's NC3 runs on
the latter; AC26 (iv)'s page-navigation release cell targets that origin, and AC26 (v)–(vii)'s
documents are Host attempts (on an `in-root:**` allowlist for the non-entry ones once T4's
pre-check exists, r17 below) — a
page-initiated `file:` navigation is cancelled under B19 and changes no `G`. Pass-through cells
and their NCs (AC03, AC24, AC26, AC27 (ii)) run in the STA harness in `hso` mode: under `keld
dev` (`hsa`) every byte is read through handles beneath the root, so no pass-through observable
exists there. An observable that needs W1 (an epoch, an IPC round trip, `onLoss`) counts only
where W1 (#1307, B7) is implemented, and each NC that uses one also names an observable at its
own rule's owner.
**r17 staging (execution order T2 → T4 → T3, §6).** T2's cells run before T4's facade exists.
T2 scores attribution and commit at the **STA navigation audit** (a `profile-test-hooks` read of
the raw `NavigationStarting`, `ContentLoading` and `NavigationCompleted` records of T2's
handlers), never at T4's signal log. T2's Host attempts come from the keld-wv attempt entry
point T2 lands; rule 4's pre-check arrives in T4, so a T2 non-entry attempt is a harness call
production cannot make (T2's one production attempt is the schema-1 boot `Navigate`). A T2
page-initiated `https:` navigation runs under today's observer-only `NavigationStarting`
(`webview2/mod.rs:2276-2297`); rule 2's allow check is T4's. T2's CSP cells (AC02, AC24 NC (4),
AC25) use a `profile-test-hooks` per-cell header policy; the production baseline is T5's. A cell
a task's head cannot run reads **not implemented at this head**, never pass. The Bun facade exists
on Windows only once W2's T3 builds a `BrowserWindow` (B13), so until then T4's signal log is read
at the Host's outgoing signal records (`profile-test-hooks`), and the facade half of T3's and T4's
cells reads blocked. Every `profile-test-hooks` hook or seam in this spec is
`cfg(all(feature = "profile-test-hooks", debug_assertions))`, the class W2 D2 and D16 use, which
release packages never enable (`crates/keld-wv/Cargo.toml` L18–20); none is a product path.

| ID | Given / when | Pass observable | Negative control (one fault → RED) |
| --- | --- | --- | --- |
| AC01 Loading path (L1) | (i) A schema-1 renderer boot whose descriptor entry is `<root>/index.html` (no query); (ii) **[TARGET, B13]** a compat window whose first document is the application's `loadURL(file:///<root>/index.html?x)` through the W3 Host seam | (i) The Host calls `Navigate(file:///<root>/index.html)` from `navigate_initial`; (ii) the Host calls `Navigate` with the application's URL (entry components, query kept) from the `HostLoad` command (r13: in header-proven mode both calls are `NavigateWithWebResourceRequest` carrying the attempt nonce, §4.6 rule 1; in unproven mode both are refused typed, B18); in both the first top-level `ContentLoading` carries `IsErrorPage == FALSE`; `location.href` read via CDP parses to the same root-relative components and query (compared with the exported component equality, never as URL text — Chromium may upper-case the drive letter, and the pinned application itself upper-cases it, `electron.js` L1012); W1's `Source` parses the same way. | Load via `NavigateToString` (`app_session.rs:3653` → `mod.rs:3654`) → `Source = about:blank`, `location.origin == "null"` (W1 AC18) → RED. |
| AC02 `file:*` filter before first navigation (C1) | (i) The schema-1 boot of AC01 (i); (ii) **[TARGET, B13]** a compat window whose first navigation is its first `loadURL`; in both, filter `file:*` + `CONTEXT_ALL` + `DOCUMENT` registered in that webview's controller build before its first `Navigate` | Every `file://` request of the load appears in the WRR log with a Host verdict (**new probe cell**: a `file:*` baseline must reproduce the per-run census of the `*` probe, 68 events per run over its two document loads, 3/3 — the probe used `*`, so its evidence does not transfer until this cell runs; E9 used exactly this filter shape before the first `Navigate` and mediated every corpus request, which is not the census cell; the shared-worker source kinds are registered once per environment, §4.5); the entry's raw inline script that only runs without the Host CSP header did not run (until P4 settles the production baseline, this oracle uses a **test-only** header policy that blocks the fixture's unnonced inline script), checked for **both** documents of the run (note 268: N2's `indexServedByHost` reflected only the second document; the second, `index.html?x=2`, is a Host attempt, since a page-initiated `file:` load is cancelled under B19). | (i) Register the filter after the first `Navigate` under `file:*` (probe N2 re-run) → the first `index.html` escapes to Chromium's file loader, the raw inline script runs → RED (at-risk 1 per document load). (ii) Register the filter only in `navigate_initial` (so a compat window built without it has none) → the compat window's first `loadURL` document escapes to Chromium's file loader → RED. |
| AC03 Containment matrix (C2 hso, C4) | The page issues `fetch`/XHR/`<img>`/`<script>`/`<script type=module>`/`<link>`/`<iframe>`/top-level/`<object>`/`@font-face` requests for the cells below | **Host-served cells** (contexts 1, 7, 8 and every request carrying `Origin`) answer `200` with the bytes of the verified handle and Host headers; **pass-through cells** (classic `<script>`, `<link rel=stylesheet>`, `<img>`, `@font-face`) are verified, **held**, and read by Chromium. In both the delivered identity equals the Host-verified identity (E9's oracle: byte-exact sha256 for `fetch`/XHR, executed nonce for `<script>`, PNG width for `<img>`, posted nonce for documents; for pass-through also E11's same-object oracle — an exclusive Host hold makes Chromium's open fail, proving it targets the verified object). Allowed spellings: in-root files, mixed-case spellings of components beneath the root (each keeps its class), drive-letter case (a pass-through class stays pass-through, §4.4), a case variant of the root prefix (**Host-served in every context**, AC30), interior `//` (collapsed, §4.4 R6), an in-root **hardlink** to an outside file (owner decision; delivered on both paths; **excluded from every NC's at-risk set**). **Parser-refused** (`403`, empty, opener count 0): **plain outside path**, `%2e%2e`, `%5C`, `%2F`, `%00`, UNC/URL-host forms, `file:////?/C:/…`, `name:stream`, `name.`/`name␠`, `CON`/`COM1`, a `~` spelling, and a **`.lnk`** final component in every context (R10). **Opener-refused** (`403`, empty, opener count 1, bytes read 0): a junction as a **leaf**, a junction as an **intermediate** component (mixed case too), a file symlink, a **directory symlink** (real-device cells; E9's fixture created both symlinks and the check refused them). **Evidence, split by path (r7 check P1-2):** Host-served — E10 `hs` 0 differentials over 345 non-race cells × 2 runs, whose ~153 allowed-with-bytes cells per run are Host-served `fetch`/XHR/frame/top cells; pass-through — E11 `pt` corpus, 99 forms × {script, stylesheet, image, `@font-face`} = 396 cells per run, **48 at-risk per context, 0 differentials** in 3 runs (content oracle: 47 per context opened and matched, `lnk-out` never opened; exclusive-hold oracle, 2 runs: Chromium opened 0 of 48 held objects and the held identity equalled the verified one 48/48 per context). E9 (all-context pass-through, 337 non-race cells × 6 runs) is supporting evidence. Without enforcement Chromium delivered bytes for 42 of the 171 refused cells without the switch and 108 of 174 with it, so those refusals are load-bearing. The mixed-case served cell is **re-run under (g)** (§4.3). Page nonce 0 for every refused cell, with the nonce oracle validated per request kind. Still not exercised: 8.3 spellings (E9's volume had no 8.3 names), `sendBeacon`. At-risk sets reported separately: parser cells × runs, opener cells × runs, Host-served vs pass-through cells per context. Evidence attribution, unchanged: the r1 probe's `url-dotdot`, `url-long` and `url-backslash` rows were Chromium-decided or probe-decided; R3/R4/R6 are proven by the pure goldens (AC05) until re-run against the Host parser with a request that reaches WRR. | One fault each, hardlink cells excluded. **(1a)** no percent-decoding in the check, applied to the **pass-through** contexts (decoys planted at the literal undecoded names) → E11 naive control: 19 differential cells per context (76 with the exclusive-hold oracle), including 2 out-of-root deliveries in the script context → RED. **(1b)** raw string-prefix root match with the correct opener → `…/root-sib/f` strips to `["-sib", "f"]`; the fixture plants an in-root decoy at that stripped name so the mutant serves the decoy instead of `404`, else **rule not exercised** (pre-declared) → RED. **(1c)** Win32 path open that follows reparse points → `junction`, `junction-mixedcase`, `symlink-dir` deliver OUT bytes → RED (= AC04's NC). **(2)** pass every request through **without** verification → the plain-outside classic `<script>` delivers the sentinel nonce → RED; baseline 0 across the parser set. **(3)** drop R10 → `lnk-in`/`lnk-out` `fetch`/XHR and `lnk-out-js` `fetch` answer `200` with raw shell-link bytes (INFERRED from E10, where `hs` served them raw) and `lnk-out-js` `<script>` follows a second hop → the `403`/opener-0 count drops → RED. A weaker mutant (N2) scored 1/1. |
| AC04 One opener (C3) | Any app-root open in `keld-core` boot, `keld-update` locate/helper or `keld-wv` document serving and subresource verification | Exactly one opener exists (`open_regular_beneath`, §4.3); `open_relative_file_windows` is gone; `open_pinned_leaf` is its single-component call; a source test finds no `OpenOptions::open`/`CreateFileW`/`Dir::open_ambient_dir` on app-root or page-controlled input in `keld-core`/`keld-wv`/`keld-update` serving paths. | Replace the opener with a Win32 path open (`CreateFileW` on `root\<components>`), grammar unchanged → the **intermediate-junction** cell (parser passes `junc/x.txt`; only the opener can refuse it) serves the outside sentinel → RED. Separately replace the per-component walk with one multi-component cap-std `open_with` → an **intermediate directory-symlink** cell (relative, in-root target) or a cloud-placeholder intermediate is followed and served under an alias spelling → RED (needs Developer Mode or a cloud placeholder on the device; absent both, this second mutant is pre-declared **rule not exercised** and the receipt says so). One fault each, one deciding rule. The source test exempts exactly two opens by path, both Host-spelled from the final path and never page-controlled: root admission in `validate_windows_dev_stage_acl` and the one admission helper that reads the ancestors' case flag (§4.4). |
| AC05 One parser; refusal table (§4.4) | Pure goldens in `keld-guard` for every table row plus the probe's 31-cell corpus | Each vector yields the table's refusal or components, and each accepted vector its `RootSpelling`: `Exact` for the admitted code units with either drive-letter case, `CaseVariant` for any other R7-equal root prefix (ASCII and non-ASCII pairs). The `:` and `~` cells are decided **only** by R8 (`:` is removed from R5 so one rule owns it); the W1 Source rule and the W3 handler call this one function (one definition, two call sites). | Drop R8's `:` refusal → `index.html:hidden` opens (NTFS parses the stream at the NT layer) and the planted ADS nonce is served → RED. Separately drop R8's `~` refusal → a planted `LONGNA~1.JS` spelling serves the long-named file under a second identity → RED (volume with 8.3 disabled → **rule not exercised**, receipt says so). Separately make R7 case-sensitive → the mixed-case and drive-letter-case goldens are refused `OutsideRoot` (and AC03's allowed mixed-case cell fails) → RED. Separately decode twice → `%252e%252e` yields `..` → R6 refuses anyway: **non-discriminating**, listed, not counted. Separately report `Exact` for every R7-equal root prefix → the `CaseVariant` goldens fail → RED (the device half is AC30 NC (2)). Separately compare the drive letter ordinal → the other-case golden reports `CaseVariant` → RED (AC30 NC (6)). |
| AC06 Served-document correlation (L2) | A top-level navigation `n` to an in-root URL is allowed at `NavigationStarting`; (a) the file exists; (b) the file is absent; (c) an `<iframe>` to a **different** in-root URL loads concurrently with a top-level navigation to an **absent** file; (d) **(r13)** an `<iframe>` to the **same exact** URI as `n` (components and query) races `n`: the frame's DOCUMENT WRR is served `200` first, then the top-level WRR is refused (`404`, the fixture's owner removes the file between the two opens) or withheld by a declared Host-queue hook; `n` is a Host attempt in header-proven mode (B18) and, separately, page-initiated (blocked on B19 until Q27) | (a) The WRR log holds one `ResourceContext == DOCUMENT` response for `n`'s URI served from `(root, components)`; `ContentLoading(n)` reports `IsErrorPage == FALSE`; the Host reports the document as Host-served with that identity; **page oracle**: `location.href` parses to `n`'s components (component equality) and raw query — never URL-text equality, the drive letter's case may differ — and the body carries the fixture nonce. (b) The Host served `404`/empty for `n`; `ContentLoading(n)` is **not** reported Host-served (W1's epoch holder posts no epoch; page oracle: no fixture nonce); AC09 fires `did-fail-load(-6)` once T4 lands the signal (T2 scores the Host record and the association audit). (c) the frame's served record never attaches to `n`; `n` stays not-Host-served. (d) `n` is credited only by navigation-specific proof (§4.4: the attempt nonce on the DOCUMENT WRR request). **Association oracle (r14b, scored first):** an independent Host audit (a debug `profile-test-hooks` read of `PendingTopLevel(n).served`, recorded after the frame's `200` and before any `ContentLoading(n)`) and the pure state-machine fixture `wv_served_document_association` (§7) both show `served == None`; at-risk 1 per variant. **Epoch oracle:** W1's epoch holder posts no epoch for `n`, the Bun effect counter reads 0 and the page oracle shows no top-level nonce, counted only where a real `ContentLoading(n)` with `IsErrorPage == FALSE` occurs with no valid nonce-bearing top-level `200` (the `404` arm if Q7 reads `IsErrorPage == FALSE`), or in the separately labelled injected-`ContentLoading` unit fixture; the withheld (no-commit) cells and every cell whose `ContentLoading(n)` reads `IsErrorPage == TRUE` are **rule not exercised** for the epoch oracle, because W1 mints no epoch there under baseline or mutant. Positive control: without the frame, the nonce-bearing top-level `200` credits `n` and W1 mints its epoch. | **(c′) Double fault, labelled and never counted as a single-fault kill (r15):** attach by `ResourceContext == DOCUMENT` only (the nonce and parsed-URI conjuncts both dropped) → in case (c) the association audit and `wv_served_document_association` show `PendingTopLevel(n).served` bound to the frame's record where the baseline shows `None` → RED; the epoch oracle counts only as in (d′). No single fault flips case (c) on the device, because each conjunct alone refuses the frame: the nonce conjunct's single fault is (d′), and the URI conjunct's runs only in `wv_served_document_association`, with an injected nonce-bearing DOCUMENT WRR for other components (`served` bound → RED; INFERRED that no device request carries `n`'s nonce for other components, since it rides only on `n`'s own request and `file:`'s two redirects cannot run: R10 refuses `.lnk` and the walk refuses a directory, §4.4's table). **(b′)** Drop `served.status == 200` from the Host-served test → where Q7 reads `IsErrorPage == FALSE` for a Host `404`, case (b) is reported Host-served at `ContentLoading(n)` (audit; W1 posts an epoch where B7 is implemented) where the baseline reports it not Host-served → RED; where Q7 reads TRUE, the `!IsErrorPage` conjunct also refuses, so (b′) is **rule not exercised** on the device and only the injected-`ContentLoading(n, IsErrorPage = FALSE)` arm of `wv_served_document_association` scores it. Pre-declared: if W1's eligibility (B7) is not implemented on the device, the epoch-holder counter is absent and only the association audit and the page oracle count. (d′) Association-only mutant: credit the first parsed-equal DOCUMENT WRR (r12's rule) → the audit and the state-machine fixture show `PendingTopLevel(n).served` bound to the frame's `200` before any `ContentLoading(n)` → RED, in both the `404` and withheld variants (at-risk 1 per variant). The same mutant turns the epoch oracle RED only in a cell with a real non-error `ContentLoading(n)` or in the labelled injected-`ContentLoading` fixture (`n` reported Host-served, W1 epoch posted); a withheld or `IsErrorPage == TRUE` cell is never counted as a mutation kill, and an epoch claimed with zero such commits is a false test assertion, not a bypass. |
| AC07 Signal emission and order (§4.6; Electron v45.1.0 oracle) | (i) In-root Host load (a page-initiated `file:` load is cancelled under B19); (ii) off-allowlist page-initiated navigation; (iii) hash change and `pushState`; (iv) a Host attempt to an absent in-root file; (v) Host-initiated load | The signal log is the system under test; every sequence is **compared to an Electron run of the same fixture** (page oracle in both), not asserted from Host records alone: the recorded run is #537's webContents navigation-order cells through the X01-T2 recorder (#537 is blocked by #494/#495 and pins Electron v44.4.5 on macOS arm64, while §0's source facts are v45.1.0, so a v44/v45 difference is reported, not scored, Q3); until those cells exist the differential reads blocked and only the source-fact comparison is reported. (i) `did-start-loading`, `did-start-navigation`, `did-navigate`, `dom-ready`, `did-finish-load`, `did-stop-loading`, same `NavigationId`, generation stamped. (ii) `did-start-loading`, `did-start-navigation`, `will-navigate` (notification, §4.6), `did-stop-loading`; **no `did-fail-load`** (Electron suppresses `ERR_ABORTED`), no `did-navigate`, no `dom-ready`; the application document survives (W1 AC10). (iii) `did-navigate-in-page` only; no `will-navigate`; generation unchanged. (iv) `did-fail-load(-6)` exactly once, no `did-finish-load`. (v) no `will-navigate`. Nothing is synthesized from a Bun-side timer. | Emit `did-finish-load` from `ContentLoading` → `did-finish-load` before `dom-ready` → RED. Separately emit `did-fail-load(-3)` on a policy cancel → divergence from the oracle (Electron L2932–2933) → RED. |
| AC08 `will-navigate` (§4.6) | A page-initiated `NEW_DOCUMENT` navigation to (a) an off-allowlist URL, (b) an on-allowlist URL; (c) a `BACK_OR_FORWARD`/`RELOAD` navigation; (d) a redirect hop (`IsRedirected == TRUE`) of a Host attempt; (e) a Host attempt itself; (l) under `allow: none`, a page `location.assign(entry?x)` and a Host `loadURL(entry?x)` queued together, each order forced by a declared Host-queue hook, in both modes of rule 1; **missing-trigger variants (r13)**: Host issued → page-first same-URI `NavigationStarting` → (if admitted) page WRR + `ContentLoading` → an app-link call from the page, with **no** second `NavigationStarting`, and separately with the Host's `NavigationStarting` released only after that call; harness-only oracle `IsUserInitiated` per `NavigationId` (FALSE for the scripted assign, TRUE for the API navigation, Microsoft doc), never fed to attribution; at-risk 1 per order and variant (r11, r13); (m) **(r13)** header-proven mode, `allow: none`, a live entry document: the app's `webContents.reload()` (at T2 through T2's attempt entry point; the facade's `reload()` joins at T4); (n) **(r17, T2's exact head)** a Host attempt with the trigger withheld: a `profile-test-hooks` seam strips the header so the attempt's `NavigationStarting` carries no nonce, and separately drops the engine call after the attempt is registered, so no event arrives; and a nonce-bearing event held past the bound and then released; (o) **(r17, T4)** two views created with different per-view policies (rule 2), one target on one allowlist only, and a third view created with no policy | (a) cancelled on the original `NavigationStarting` (+0 generation), `will-navigate(url)` emitted when a listener exists (the `shell.openExternal` pattern keeps working); (b) allowed, `will-navigate(url)` emitted when a listener exists, `preventDefault()` has no effect (documented divergence; `keld migrate` derives the allowlist so this case is empty for a veto-all listener); (c) allowlist evaluated, **no** `will-navigate` until P3 settles whether Electron emits it for script history navigations; (d)/(e) never attributed page-initiated (attribution by the attempt's `NavigationId`); no `will-navigate`. (f) **in-root is not enough and the query counts**: a veto-all fixture (`web.navigation.allow` derived to `none`) whose page contains an in-root link (`other.html`), a link to `index.html?attacker=1` and a link to the entry's exact URL — all three cancelled, page nonce unchanged, a granted IPC call still succeeds afterwards. (g) **[TARGET, B13]** a Host attempt (`loadURL`) to an off-allowlist target → typed refusal at +0, document and IPC untouched; (h) **[TARGET, B13]** a Host attempt to `index.html?x` (entry components, any query) and an `onLoss` reload-entry — triggered by W1's rule: a page-initiated navigation to the fixture's one allowlisted `https:` origin commits a remote document, which W1 leaves FailedClosed and applies `onLoss` to (`gh1184` `9bc56726` L628) — under the same `none` allowlist for `file:` targets (plus that origin; B19 cancels every page-initiated `file:` target) → both admitted in header-proven mode (in unproven mode both are typed refusals at +0, B18); (i) **lifted-literal pass cases**: with `index.html` (no-query constraint) a link to `index.html` proceeds and `index.html?attacker=1` is cancelled; with `p.html?mode=a` links to `p.html?mode=b` and `p.html?mode=A` are cancelled, `p.html?mode=a` proceeds; a query needing escaping (`?q=a%20b`, `?q=%C3%A9`, `?q=%41` vs `?q=A`) compares byte-exact on the engine-reported serialization (device cell); (j) **component equality**: under `in-root:**`, links to `INDEX.HTML`, `index%2Ehtml` and a Turkish dotless-i spelling decide identically in W3's allow match and W1's eligibility (Q17); (k) `location.reload()` of the live document under `none`: the Host cancels it (`RELOAD` kind, page-initiated); Electron's behaviour for a veto-all listener on reload is UNKNOWN until P3 — pre-declared, the cell records both. Generation: +0 on cancel, +1 only when an allowed navigation commits. (l) Header-proven mode: the page event (no nonce) is cancelled at +0 in every order and variant — no page WRR, `ContentLoading`, epoch or app-link effect (page grant and effect counters 0); the Host attempt commits with an epoch under the oracle's TRUE id when its event arrives, and fails typed (`KELD-*`, not counted toward W1's cap) when it never arrives; the old document's IPC was revoked by the Host's own +1 at issue (#1192 L110), not by the page. Unproven mode: the Host attempt is refused typed at +0 **before** issue (no +1), the page event is cancelled at +0, and the old document and its IPC survive. (m) The reload is issued as rule 1 (i)'s header navigation; the cell records `NavigationKind`, the nonce in `RequestHeaders`, G before/after, `history.length` and a later `onLoss` recovery; the reload is attributed, admitted and commits +1 — until this cell passes, `reload()` of a `file:` document is a typed refusal (B18). (n) The un-nonced event is cancelled at +0 (rule 2); the attempt fails to its caller with exactly one typed `KELD-*` within `HOST_ATTEMPT_START_BOUND` (§4.6 rule 1); #1192's attempt-failure row applies (L112); nothing is re-issued; the late nonce-bearing event is page-initiated and cancelled at +0. Positive control in the same run: (l)'s committing attempt. T4 re-runs (l), (m) and (n) through `HostLoad`. (o) Each view decides by its own policy; the view with no policy cancels every page-initiated target at +0. | (a) (header-proven mode: in unproven mode no Host attempt is issued) Attribute "page-initiated iff `IsUserInitiated == TRUE`" → the application's own `loadURL` (API navigations are user-initiated per Microsoft) emits `will-navigate` while a scripted `location.href` (`FALSE`) emits none — against Electron's documented rule (`web-contents.md` L270–276: no `will-navigate` for `loadURL`; renderer-initiated main-frame navigations emit it, throttle L42–62) and #537's recorded cells (AC07) → mismatch in both directions → RED; at-risk 1 each per run. (b) Advance the generation at `NavigationStarting` → the #1192 generation read back after the cancelled off-allowlist click is G+1 where the baseline reads G (the (m) readback) → RED; a page-side IPC round trip failing (W1 AC10) counts where W1 is implemented. (c) Treat every in-root `file:` target as on-allowlist → (f)'s links replace the document → RED. (d) **Cut the query in the `file:` allow match** → in (i) the `index.html?attacker=1` link replaces the document → RED; separately fold the query through the component equality → `p.html?mode=A` proceeds → RED. (e) Check the allowlist after the Host attempt's +1 → (g)'s refusal comes after a +1: the generation readback is G+1 and #1192's phase is FailedClosed where the baseline refuses at +0 with the document Live → RED (header-proven mode only: in unproven mode every Host attempt is refused before issue, so the order is never reached; IPC failing counts where W1 is implemented). (f′) Drop the entry exemption for Host attempts → (h)'s `index.html?x` Host attempt is a typed refusal where the baseline admits it, and, where W1's `onLoss` is implemented, the reload-entry is refused and the window never recovers → RED. (j′) Compare components byte-exact in one consumer (W1 or W3) → `INDEX.HTML` is allowed by one and refused by the other → the page lands FailedClosed and `onLoss` fires → RED. **(r13) (c), (d) and (j′) are pre-declared rule not exercised on the device until Q27** (B19's cancel decides those links first); the same single faults run in `guard_navigation_allow_forms` (§7). (l′) r12's fallback (the first parsed-equal event is provisionally the Host's) → in the page-first missing-trigger variant the page's `NavigationId` is not cancelled and the STA navigation audit records its `ContentLoading` where the baseline records none (the signal log joins at T4) (page oracle: `location.href` carries the assigned `?x`; the live document is the entry without a query, and both documents are the same file, so the body nonce cannot separate them) → RED; the epoch stays absent because §4.4 credits no un-nonced DOCUMENT WRR, which is recorded separately as defence in depth (epoch holder 0, effect counter 0). (l″) In header-proven mode, fall back to URI attribution for an event without the nonce → the same flipping observable → RED. (m′) Issue the Host reload through `Reload()` (no request, so no nonce) → it is classified page-initiated `RELOAD` and cancelled under `none` → the reload never commits → RED. (n′) Remove the deadline → the attempt never settles: no typed failure by the harness's own kill bound (2 × `HOST_ATTEMPT_START_BOUND`), the caller hangs → RED. (n″) Attribute a nonce after its attempt failed → the late event commits where the baseline cancels it → RED. (o′) Read one session-wide policy instead of the view's → the second view allows the target where the baseline cancels it → RED. |
| AC09 `did-fail-load` honesty | (i) Absent in-root entry (Host `404`); (ii) parser-accepted, opener-refused entry (Host `403`: directory, reparse); (iii) policy cancel; (iv) renderer error (`NavigationCompleted` failure, other `WebErrorStatus`); (v) a top-level document over `MAX_HOST_SERVED_FILE_BYTES`, and separately one refused for capacity (§4.5) | (i) `-6 ERR_FILE_NOT_FOUND`; (ii) `-10 ERR_ACCESS_DENIED` (documented divergence: Electron would list a directory or follow the link); (iii) **nothing**; (iv) the §4.6 table's code; (v) `-8 ERR_FILE_TOO_BIG` and `-12 ERR_INSUFFICIENT_RESOURCES` (Host `413`, `503`; a documented divergence with no Electron cell to compare), and a Host attempt refused so fails typed and consumes none of W1's three recoveries, counted where W1 is implemented (§4.5, B7 xi); exactly one `did-fail-load` per failed `NavigationId`; `did-stop-loading` follows. Verified against #537's recorded Electron cells (AC07; page oracle + Electron run), not against Host records alone. | Map every failure to `-3` → case (i) reports `-3` while the Host served `404` → RED. |
| AC10 Settled eval (E1, E4) | After `did-stop-loading`: (a) `new Promise(r => setTimeout(() => r("v"), 500))`; (b) a `try { … } catch (e) { null }` statement with a string completion value; (c) `Promise.reject(new Error("m"))`; (d) `throw new Error("t")`; (e) under page CSP `script-src 'self'`, an expression whose value is an object with a getter that calls `eval` and sets a marker; (f) same, with a `Proxy` whose `get` trap calls `eval` and sets a marker; (g) the page has patched `Object.getOwnPropertyDescriptor` (and `Reflect.ownKeys`) to call `eval` and set a marker, then returns a plain object; (h) under `script-src 'self'`, the **evaluated code itself** calls `eval("1")`; (i) **(r17, T3's exact head, which contains T4's native producer)** `executeJavaScript` issued through the facade while the document is loading (**[TARGET, B13]** for the Bun process; until then the harness drives the same wait from the native signal) | (a) `"v"` after ~500 ms (probe 3/3); (b) the completion value; (c) rejection carrying `m`; (d) the facade's fixed sync-throw message (the `webContents` owner #539's shape, §10 Q3); (e) the getter runs **under the page's own CSP** (deep serialization reads own enumerable properties through `Get` inside `evaluate`, V8 L222/L257): its `eval` throws, marker **not** set, and the throw surfaces as CDP `-32000 "exception during deep serialization"` → typed `EvalFailed` rejection to the caller; (f) `{type:"proxy"}` is reported and the trap never runs (V8 deep serializer L339–341): marker not set — matches Electron's `DataCloneError` class; (g) marker **not** set under (v) — no codegen scope exists; (h) the page's `eval` is refused by its CSP (flag false, V8 L400–402): the rejection names the CSP. (i) The facade waits, the native `did-stop-loading` (§4.6) ends the wait, and the call settles with exactly one value carrying the loaded document's nonce. | Use `ExecuteScript` (probe N4) → (a) yields `{}` → RED. Separately pass `allowUnsafeEvalBlockedByCSP` default → (h) succeeds → RED. Separately transport through (ii) `callFunctionOn` + encoder → (g)'s marker is set (codegen scope open across the encoder's intrinsic calls, V8 L151/L158/L188) → RED. Separately remove the `did-stop-loading` producer → (i)'s wait never ends: no settle by the harness's kill bound → RED (T3 lands after T4, so this reads RED, never "not implemented"). |
| AC11 Generation-scoped delivery (E2) | (i) Eval dispatched at generation G; replacement before completion; (ii) **real replay control** (#1185's own NC, UNMET by the probe — N6 compared constants): a debug `profile-test-hooks` seam holds the **actual CDP completion payload** of correlation `c` dispatched in document G1, the harness commits G2, then the held payload is delivered to the completion handler as if CDP had just completed; (iii) role G1 awaits an eval, recovers to G2 before completion; (iv) the document enters BFCache or never completes the script; (v) **settled, not admitted** (r14 owner test 2; r16 barriers): a `profile-test-hooks` barrier holds `c`'s settle record after the STA's `Settled` and **before** `try_admit_eval_settle`'s transaction; the harness commits G₂, then releases — in both orders (revocation completed before the admission attempt; admission attempted first with the G-change issued at the barrier) and, each, for a JS success value **and** for a script-derived error (a throw with `exceptionDetails`, a rejected Promise, an `EvalFailed` text); (vi) **admitted before revocation** (owner test 3): the barrier sits **after** the transaction's commit and, in two sub-cells, before and after the complete socket write, before the Bun caller reads it; the harness commits G₂, then releases; (vii) **role retirement** (owner test 4; split r17): the Bun role retired and recovered (KEL-75/KEL-143) at the barrier, in two forced orders: (a) **retirement before admission**, at (v)'s barrier with the retirement's fence update completing first; (b) **admission before retirement**, at (vi)'s barrier, with sub-cells before and after the complete socket write; (viii) **failed enqueue** (r17): a `profile-test-hooks` seam between the reservation and the enqueue injects `Full` and, separately, `Disconnected`, each for a value-bearing record and a framework terminal; (ix) **`Destroy` and `Exhausted`** (r17): `Destroy` with one `Pending` and one `Settled` correlation; `Exhausted` in a state-machine fixture with `G` seeded at `u32::MAX`; and for (v)–(viii): forged (absent from the ledger), duplicate (already terminal) and wrong-role (another role's live correlation) framework terminals offered to the writer; DATA credit exhausted with CONTROL reserved; and writer-side transport faults — a zero-byte write failure, a partial frame, and link loss after a complete write. (v)–(viii) are pre-declared blocked on B21 until the admission is implemented | One state machine (`Pending → Retired \| Settled`, §4.7 step 4), counted at the STA by a `profile-test-hooks` audit of the settle records it posts per correlation (r14c: the STA rule's own owner, because the writer's revalidation suppresses a stray G₁ success record before the facade could see it) and, as defence in depth, in the **Bun facade** (another process): settle frames per correlation ≤ 1, and every payload carries the page-side nonce of the document its eval was dispatched in (§4.7 step 4) — the second assertion is Host-independent: the evaluated script returns a **page-side document nonce** (planted in each fixture document), and the facade compares it with the page oracle's nonce for the document in which the cell dispatched `c` (every cell starts its replacement only after the dispatch), not with any Host stamp. (i) (owner test 1) exactly one `DocumentReplaced` frame and zero G₁ success frames; CDP `-32000` (probe 3/3) or retirement. (ii) the held G₁ payload produces **no** settle post (STA audit: no post for `c` after its retirement post) and no frame (the entry is `Retired`); the caller observed exactly one frame, the retirement's; at-risk 1 per run. (iii) retired by KEL-75/KEL-143's role retirement (the role axis's existing owner): its retirement record lists `c`; never delivered to G₂. (iv) one `DocumentReplaced` frame when G changes, in the same STA turn; the entry is freed when its CDP callback fires. Counters (§4.7 step 5): ledger terminal state per correlation, the exact first socket write, the native `G` and role/link incarnation read in each admission transaction, the shared fence sequence, the page-side G₁/G₂ nonces and zero G₂ effects. (v) Revocation first: no G₁ value **and no G₁ script-derived error** reaches any caller; exactly one terminal for `c` — `DocumentReplaced` from the writer for a `Settled` record (from the STA for a `Pending` one); first socket write of a G₁ value-bearing record: none. Admission first: the record is admitted with the transaction's native `G` = G₁ and its fence sequence precedes the STA's G₂ retirement report. (vi) At most one frame for `c`, exactly one when its write completed: the G₁ value with G₁'s nonce, observed by the original role; when the complete write preceded G₂ the original caller observes it after G₂ committed — a late observation of an already dispatched G₁ result, never a G₂ value; when the write was in flight the receipt records that it completed on the original link or ended `OutcomeUnknown`, never asserted delivered; zero G₂ effects. (vii) (a) Retirement before admission: the fence sequence orders the retirement first; no admission; first socket write of `c`: none; the ledger holds one logical outcome, route-retired, `NotDispatched` (proven by that counter); no on-wire terminal is required once the role's link is gone. (b) Admission before retirement: the fence sequence orders the admission first; the original-link first-socket-write counter reads complete, or the write is lost mid-way; the write completes on the original link and only the original caller may observe the G₁ value, **or** the ledger ends `OutcomeUnknown` — both pass, a claimed rollback does not. In both: the successor's frame count and effect counter read 0, nothing is relabelled or redirected, no duplicate frame, no credit refund before quiescence, no automatic replay. Forged, duplicate and wrong-role terminals: refused, zero socket writes, ledger and credit unchanged. DATA full: the value-bearing record is never admitted on CONTROL (CONTROL readback unchanged by it), and a retirement terminal progresses on reserved CONTROL or the link retires fail-closed; a freed DATA credit re-runs the fence as in (viii) `Full`. Zero-byte, partial and after-write loss: `OutcomeUnknown`, never `NotDispatched`; the link is poisoned and retired, nothing replayed, charges kept until quiescence; the caller-visible code is the error-registry owner's (B5), not asserted to be WV-013 on the broken socket. (viii) `Full` on a value-bearing record: not admitted; first socket write none, ledger terminal none, the reservation readback back at its pre-attempt value inside the same fence sequence, the entry still `Settled`, CONTROL readback unchanged; a writer-drain wake re-runs the whole fence for the same ticket, giving one frame and one terminal, or, with a G-change injected instead, whose fence update wakes it, one `DocumentReplaced` and no G₁ value, or, with the role retired, (vii) (a), woken by the retirement with no drain. `Full` on a framework terminal: no wait; the link retires fail-closed and the retirement is not dropped. `Disconnected`: no socket write of this frame, one route-retired `NotDispatched` outcome, the link retired fail-closed, no successor frame, credit readback bounded. (ix) One logical terminal per correlation, and a `Settled` record offered after `Destroy` is refused at the fence. At-risk 1 per run and cell. | Single faults ((1)–(4) on the STA machine or the role owner, (5)–(7) at the writer, the facade or the role owner): (1) the retire step rejects but leaves the entry `Pending` → the held G₁ payload is settled → the STA audit shows a second post for `c` (a G₁ success record after the retirement post) where the baseline shows none → RED; the facade count is defence in depth only, since the writer's current-`G` check suppresses that record. (2) the completion handler settles regardless of state → the same STA-audit observable → RED. (3) the retire step posts no rejection → (iv) hangs the caller (no frame within the test bound) → RED. (4) for (iii): skip the role retirement hook → KEL-75/KEL-143's retirement record omits `c` where the baseline lists it → RED ("the G₂ role observes a G₁ frame" is defence in depth only, since the writer's ticket check also suppresses it; pre-declared **rule not exercised** until Q16 confirms that KEL-75/KEL-143's retirement covers eval correlations; if it does not, the hook becomes W3's and the NC counts). (5) For (v): let the STA's `Settled` authorize the late transport send (r12/r13's rule) → the G₁ value reaches the caller after G₂'s revocation → RED. (6) For (vi), two single faults run separately: (6a) a facade-side generation guard drops a handed-off frame whose `G` is no longer current (the guard §4.7 step 4 rejects) → the caller gets `DocumentReplaced` instead of the handed-off G₁ value → RED; (6b) the writer revalidates the ticket a second time after its admission transaction, before the physical write (where the (vi) barrier sits) → the same observable → RED. (7) For (vii): re-ticket the old role's pending `c` with the successor's role identity, generation and link incarnation after recovery (a redirect) → the ticket check passes it and the successor observes a G₁ frame → RED (a redirect that keeps the old ticket is suppressed by the ticket check, which is defence in depth and not this NC); separately refund the credit on suppression before quiescence → #540's credit readback rises before quiescence → RED. (8) **G checked, gate released before enqueue** (the owner's one-fault mutant): the admission checks the ticket and the current `G` under the fence, releases it, then reserves and `try_send`s; a test-only bounded pause seam between check and enqueue, during which the harness issues the G₂ change → a G₁ value-bearing frame whose fence sequence follows the STA's G₂ retirement report reaches the socket → RED (baseline: the G-change's fence update orders against the whole transaction, so 0 such frames). (9) **skip terminal provenance** (r14c's role-check-alone rule): framework terminals admitted without the ledger binding → the forged, duplicate and wrong-role terminals are written (first socket write ≥ 1; a second ledger terminal for the duplicate) → RED. (10) **role retirement off the fence**: the runtime owner reports retirement before updating the fence (relying on a later control record) → with the retirement at the pre-admission barrier, a frame for `c` is written to the retired link or the successor → RED. (11) **script error as framework terminal**: an `EvalFailed`/`exceptionDetails` record admitted without the current-`G` check → in (v) the G₁ script-derived error reaches the caller after G₂'s revocation → RED, while the success-value cell stays green (single fault). (12) **value as CONTROL**: a value-bearing record charged to CONTROL → with DATA full it is admitted and consumes CONTROL → RED. (13) **zero-byte as not dispatched**: a zero-byte write failure classified `NotDispatched` → the ledger records `NotDispatched` where the first-socket-write counter records an attempt → RED. (14) **terminal before enqueue** (r16's literal order): commit the terminal, then return the enqueue error → in (viii) `Full` the ledger reads terminal with no frame, the drain wake finds nothing to re-run and the caller never settles → RED. (15) **`Full` keeps the reservation** → the credit readback falls by one per injection → RED. (16) **re-enqueue without the fence**: the wake re-sends the saved frame without re-running (i)–(v) → with the G-change injected before the wake the G₁ value is written → RED. (17) **`Disconnected` to a successor**: re-ticket `c` to the successor on `Disconnected` → a successor frame → RED. (18) **admitted frame dropped at retirement**: the writer purges its FIFO at role retirement → in (vii) (b) with the write held before the socket, first socket write none where the baseline writes on the original link → RED. NC (6b) is kept and also runs in (vii) (b), so an extra post-admission check cannot pass as a retirement rule. (19) `Exhausted` leaves entries `Pending` → (ix)'s state-machine fixture shows no terminal → RED. (20) **lost wake**: the waiter registers after leaving the fence, and a seam frees the last slot in that gap → no later drain wakes it, the caller gets no settle by the harness's kill bound where the baseline settles → RED. (21) **retirement wakes nobody**: in (viii) with the role retired and no drain, the entry stays `Settled` with no terminal where the baseline closes it → RED. |
| AC12 Context binding at dispatch (E4) | `executeJavaScript` issued (a) while the main frame's execution context is known; (b) in the gap between `Runtime.executionContextDestroyed` and the next `executionContextCreated` for the main frame; (d) **(r13 phases)** order 1: `ContentLoading(G₂)` → eval → delayed verified `executionContextCreated(ctx₂)`; order 2: a duplicate default context for the same loader, and separately a failed Host attempt, before the eval; (e) **(r13 cross-clock)** Host-queue fixture on a page-initiated same-URL `https:` replacement (§3 r13 gating): live A at G₇ → page-initiated `NavigationStarting(n_B)` to the **same URL** → CDP `frameNavigated(L_B)` + `executionContextCreated(ctx_B)` released → eval dispatched (the harness bypasses the facade's load wait) → `ContentLoading(n_B)` released; (f) **(r15 wake-up order, facade fixture)** the facade receives `context-bound(G₂, s+1)` before the `NoDocumentContext(G₂, s)` reply it follows, and separately, after an unbind by `executionContextDestroyed` with no G-change, a reply `NoDocumentContext(G₂, s)` while the last event it has seen is the earlier `context-bound(G₂, s−1)` | (a) the call carries the `uniqueContextId` **bound to G at `ContentLoading`** (§4.7 step 1); only `InvalidParams` "uniqueContextId not found" and `-32000` "Execution context was destroyed." map to `DocumentReplaced` (§4.7 step 3); (b) typed `NoDocumentContext` returned to the facade, which waits (bounded) for the Host's per-generation `context-bound` event and re-dispatches; the application sees one settled result in the new document, never a rejection, and **no script ran in the gap**. Non-entry and remote documents are evaluable; only generation equality decides delivery. (c) `executionContextDestroyed`/`executionContextsCleared` for the bound context unbinds the pair (§4.7 step 1), so (b)'s gap exists. Two deterministic hooks (at-risk 1 per run each): **hook 1** holds `ContentLoading` and `executionContextDestroyed` in the Host's queue while the engine already shows the new document (Host still holds `(G₁, ctx₁)`); **hook 2** releases `executionContextDestroyed` but holds `executionContextCreated` and `ContentLoading` (Host holds no pair). The harness dispatches during each hold. Side-effect oracle: the evaluated script writes a marker into `document.title`; the harness reads both documents' titles through CDP. Baseline under hook 2: `NoDocumentContext` → facade waits → hook released → `context-bound` → one result in the new document. A back/forward-cache restore that re-creates `ctx₁` under hook 1 is pre-declared **rule not exercised** until Q14's census says whether 156 raises it. (d) Order 1: `AwaitingContext(G₂)` → `NoDocumentContext` → the facade waits → `context-bound(G₂)` → one result carrying G₂'s nonce, no marker in G₁; order 2: `TerminalAmbiguous(G₂)` (or the post-failure refusal) → `DocumentReplaced` at once, no wait, no marker anywhere. (e) Nothing binds while the transition is open; neither A's nor B's title carries the marker before `ContentLoading(n_B)` is released; the dispatch reaches A's still-bound context (refused `DocumentReplaced` when A is gone) or returns `NoDocumentContext`; after release `(G₈, ctx_B)` binds and the re-dispatch returns B's nonce (positive bind recorded). (f) The wait ends at once on the early `context-bound(G₂, s+1)` and the single re-dispatch returns the document's nonce; the stale `context-bound(G₂, s−1)` does not end the wait, which ends at the next `context-bound` with a greater `s`. | NC1 (hook 1): drop **only** the `uniqueContextId` parameter → the eval runs in the **new** document (its title carries the marker) and is then rejected `DocumentReplaced` → RED (side effect without delivery); with the parameter the same dispatch returns "uniqueContextId not found" → `DocumentReplaced`, no marker. NC2 (hook 2): drop **only** the unbind on `Destroyed`/`Cleared` (parameter kept) → the Host still dispatches with `(G₁, ctx₁)` → "uniqueContextId not found" → `DocumentReplaced` where the baseline delivers a result → RED. NC3 (r11, event-queue fixture): hold A's `executionContextCreated` until after `ContentLoading(n_B)` but before `frameNavigated(L_B)`, then eval; each page's title/nonce is read independently → the baseline does not bind A's context (its loader is not fresh for `n_B`): the eval returns `NoDocumentContext` with nothing sent (`AwaitingContext`), and after `frameNavigated(L_B)` and `ctx_B` bind, the re-dispatch returns B's nonce; the r10 "whichever arrives second" mutant binds `(G_B, ctx_A)` → RED. "Runs in A" is pre-declared at-risk 0 unless `ctx_A` is live (back/forward cache only; `file:` documents are INFERRED cache-ineligible); a back/forward-cache variant runs when Q14 shows it reachable. NC4 (d, order 1): map `AwaitingContext` to `DocumentReplaced` → the caller is rejected where the baseline returns G₂'s value → RED. NC5 (d, order 2): map `TerminalAmbiguous` to `NoDocumentContext` → the facade waits and re-dispatches into the next bound document (a later generation's marker) or settles only at the bound → RED. NC6 (e): r12's URL-only pairing → binds `(G₇, ctx_B)` → B's title carries the marker before release and the settle carries B's nonce under G₇ → RED. NC7: re-dispatch only while G is unchanged → (b), whose G changes during hook 2, is rejected instead of delivered → RED. NC8a (f): end the wait on an event whose `G` is at least the reply's (`G` compared instead of `s`) → the stale `context-bound(G₂, s−1)` ends it at once, the re-dispatch meets `AwaitingContext` and settles `NoDocumentContext` where the baseline returns the value → RED. NC8b (f): consider only events that arrive after the reply → the early `context-bound(G₂, s+1)` is missed and the call settles `NoDocumentContext` at the bound where the baseline returns the value at once → RED. |
| AC13 CDP capability self-test (E5) | Webview create, before the first `Navigate` | Event receivers registered **before** `Runtime.enable`; `Runtime.enable` succeeds; `Runtime.evaluate("1+1", returnByValue)` → `2`; `Runtime.evaluate("Promise.resolve(3)", awaitPromise)` → `3`; one `evaluate` with `serializationOptions: {serialization:"deep"}` returns a `deepSerializedValue`; the negative probe — `evaluate` with a well-formed but unknown `uniqueContextId` — fails `InvalidParams` "uniqueContextId not found" (§4.7); each within `WEBVIEW2_STARTUP_BOUND`; the self-test does **not** wait for the initial `executionContextCreated` (its arrival before the first `Navigate` is UNVERIFIED; the probe registered receivers after enabling, `main.rs` L1090–1119). Failure → the application does not start, typed `KELD-WV-*`. **Not claimed**: that `allowUnsafeEvalBlockedByCSP:false` is honored (crdtp L124–128; per-runtime cell AC10 h; §10 Q11). Enterprise CDP policy: UNKNOWN (Q11). | Skip the self-test with an injected CDP failure → the framework counter **"application started" = 1** although every later eval rejects → RED (the counter is Keld's own `NavigationReady`/app-started milestone, not any application's behaviour). Separately drop the deep-serialization probe, with the injected CDP seam answering it without `deepSerializedValue` (runtime 156 supports the option, so without the injection both builds start and the cell is **rule not exercised**) → the application starts and every object result arrives as a `RemoteObject` stub → RED. Separately drop the negative probe, with the seam answering an unknown `uniqueContextId` as if it bound → the application starts although AC12's binding is unproven → RED (without the injection: **rule not exercised**). |
| AC14 Eval value domain and bound (E3) | A result inside the codec owner's admitted domain and at the bound; one byte over | Delivered bit-exact / typed `CodecLimit` rejection, never a hang. **Blocked on §10 B3.** | Deliver truncated → RED. |
| AC15 App-declared response headers (S1: documents and `fetch`/XHR) | **[TARGET, B13] for every listener case ((a), (c)–(e)): a schema-1 boot refuses the registration typed (§4.5).** (a) A compat application registers `onHeadersReceived` inside `app.whenReady()` and then creates its window and calls `loadURL(entry)` (the Electron-correct order; the window's **first** document is that `loadURL`); (b) no listener; (c) the listener's role is confirmed gone; (d) the role is recovering (between the `RoleGeneration` move and the successor's re-registration) and the predecessor had a listener; (d′) same, predecessor had none; (d″) a round trip in flight to the predecessor at the `RoleGeneration` move, whose reply the harness releases after the move; (d‴) case (d) with the recovery deadline elapsing, and separately with `MAX_HOST_SERVED_IN_FLIGHT` requests held; (e) a listener whose `filter.urls` matches the document's `fetch`/XHR, a module script **and** its classic `<script>`/stylesheet/image subresources | (a) **[TARGET, B13: blocked until W2's D2–D4 land on Windows]** one Host→Bun round trip per **Host-served response** (`ResourceContext` 1, 7, 8 and CORS-mode loads) **including the window's first document** — the registration and the `loadURL` are two records on the one FIFO (a requirement T4/T5 test, §4.5), and no Host navigation precedes them (W2's facade boot: no window at boot, `HostInitialized` Ready); the served headers are the listener's (Host-owned fields excepted); (b) zero round trips; (c) Host-served requests `403`/empty; (d) Host-served requests are **held** behind their deferrals until the successor registers, or until its `listeners-settled` record (posted by the facade after its `whenReady` callbacks drained) arrives without a registration, then served; bounded by the recovery deadline (then `403`/empty) and by `MAX_HOST_SERVED_IN_FLIGHT` (a further request `503`/empty, §4.5); (d′) with no predecessor listener nothing is held and responses carry the baseline only (as before the loss); (d″) the late reply is dropped (correlations are keyed on `RoleGeneration`, §2) and the request is held and served as in (d); (d‴) every held request is answered `403`/empty at the deadline, and at the count a further request is answered `503`/empty at once, each exactly once; (e) one round trip per matching `fetch`/XHR and module script with the listener's headers applied; **zero** for the pass-through classic script/stylesheet/image, and for the same classes Host-served at the 256-hold bound or through a `CaseVariant` spelling, whose headers are **not** applied — the recorded divergence from Electron, whose listener sees every response (T1 entry; §4.5). At-risk for (a) = the first DOCUMENT response per run (non-empty by construction). | (a) Process the `loadURL` record before the earlier registration record (FIFO broken) → the first document is served with the baseline CSP only and the app's inline-script policy is not applied (probe N5 class) → RED. (c) Serve on role loss with baseline-only headers → RED. (d) Answer `403` instead of holding → the page's `fetch`/XHR issued during recovery is answered `403` where the baseline holds it and serves it with the successor's headers → RED (the document survives recovery, #1192 N15, so the in-flight request is the observable, not a blank document). (d′) Hold although the predecessor had no listener → the page's `fetch` issued during recovery stalls until the recovery bound (then `403`) where the baseline answers it at once with the baseline headers → RED. (d″) Apply a reply that carries the predecessor's `RoleGeneration` → the response carries the predecessor's header after the move → RED. (d‴) Remove the count bound → the held count exceeds the constant at the readback → RED. (e) Skip the round trip for `fetch`/XHR → the listener's header is absent from the `fetch` response → RED; separately round-trip pass-through contexts → round-trip count > Host-served count → RED. |
| AC16 Baseline CSP cannot be relaxed | The listener returns a permissive CSP, or overrides `Content-Type` to `image/svg+xml`/`application/xhtml+xml` (until B13 lets a compat role register a listener, a keld-wv harness stub answers `HeadersRequested`; a schema-1 boot refuses the registration typed, §4.5) | The baseline is emitted as its own header on **every `DOCUMENT`-context response** regardless of `Content-Type`; a remote script the baseline forbids stays blocked; in-root scripts load under a baseline written with `'self'` or nonces (AC25; hashes UNTESTED; cross-directory `'self'` is P4, §10 Q5). Pass-through subresources carry no Host header, so the document's policy is the one enforced on them; Host-served `fetch`/XHR responses carry no baseline CSP (a CSP header on a non-document response has no effect on the page). | Emit the baseline only for `text/html` → an SVG document served with the app's permissive CSP runs the forbidden script → RED. |
| AC17 Hardlink authority read-back (C4) | The fixture plants ≥ 1 in-root file with `nlink > 1` whose other name is outside the root | (a) served (AC03); (b) for every delivered file (Host-served or passed through) with `nlink > 1`: a write attempt on the linked object under **each admitted role token** (Strict; the Legacy forfeiture printed instead of claimed) is denied (KEL-254 AC8 class) and the per-file DACL read-back shows no role write grant (AC7 extension); (c) the owner's paired control: the Strict role attempts to **create an entry inside the admitted root** and is refused (KEL-254 app-role isolation on Windows). At-risk = multi-link delivered files; 0 → rule not exercised (fixture MUST plant one). | Grant the Strict role write on the linked object → read-back detects, the control writes → RED. Separately grant it create on the root → (c) succeeds → RED. |
| AC18 Zero bytes on refusal (C5) | AC03's refused cells, elevated ETW Kernel-File session | No open of the outside sentinel by `msedgewebview2.exe`; for parser cells the Host opener count is 0; for opener cells bytes read is 0 (the opener returns a handle to the reparse point itself and reads nothing). ETW NOT RUN → INFERRED-strength, printed. | AC03's NC → ETW shows the renderer's own open → RED. |
| AC19 Non-`file:` requests never reach W3 | The page fetches `https://` (policy-allowed) and loads `data:`/`blob:` | **Structural**: the `file:*` filter never raises WRR for them — new probe cell: `https` request with WRR event count 0 and Host response count 0; the policy owner's rules apply unchanged. | Register `*` instead of `file:*` → the `https` request raises WRR and sits behind the UI thread → RED (count ≠ 0). |
| AC20 P1 performance gate (§9) | Before T2 merges | **Probe status (E13/E14): FAILED, carried to T2.** r9's dynamic handler failed (c) on a real Vite application (E13, +6.2–8.5 % per document). The chosen `hsoz` passed (c) once (E14 follow-up 7) and failed (a), (b) and (c) in the pre-registered four-block replication (follow-up 8: (a) +3.71 % [+0.69, +6.48], (b) +9.01 % [+4.70, +11.83], (c) doc0 upper +8.55 %, doc2 +7.92 %), while the hold's own cost (`hsoz` − `hso`) was small and sign-unstable across sessions and the session's baselines ran above E12's (draw.io `e` +9–11 %, modules `hsm` +6–11 %); `hso` vs `hsm` itself reversed from E12's −2 % to +6.78 % (B8 reopened). No waiver is claimed from probes; the T2 gate below decides on the implementation. **Measured (E8–E12):** `hs` +3.54 % [+1.16, +4.68] vs `e` on draw.io (E10); modules: `hsm` 158.5 ms vs `eaf` 206.6 ms, paired −40.9 ms (E11). E12, `hso` + hold: with `WebResourceResponseReceived` always registered (`hsoh`) (a) draw.io +1.65 % [+0.59, +2.82] **PASS**, (b) modules +5.72 % [+1.77, +9.53] vs `hsm` **FAIL** (the handler's registration costs +11.1 ms [+4.7, +18.4]; the hold +0.3 ms); with the handler registered only while holds exist (`hsod`) (a) +0.67 % [−1.65, +2.43] **PASS**, (b) −1.51 % [−5.53, +1.23] **PASS** — but (b) is a **zero-hold** workload, so it never paid for the handler. Per-request NCs passed in every block. **Gates at T2** (implementation; the production baseline CSP once P4 settles it, else AC02's test-only header policy; the application's preload once W1 (#1307) can run it, so real `fetch`/XHR demand is in the census — W1's route D needs T2's loading path (W1 §4.6a), so a T2 receipt without the preload records its absence and T7 re-runs the gate with it; pooled ratio-of-medians CI upper bound, with a pre-registered per-request NC in each block): (a) draw.io, chosen design vs `e` ≤ +5 %; (b) module workload vs `hsm` ≤ +5 %; **(c) a realistic module application — Vite output: modules and `crossorigin` CSS Host-served, images and fonts passed through and held, across several documents — chosen design vs `hso` (no hold) ≤ +5 %, with holds per document and peak live handles reported**. Every gate block also runs an **A/A arm** (the comparator twice under two labels) and reports each block's baseline median against E12's; n is fixed in the pre-registration; if the A/A CI exceeds ±2.5 %, the gate is re-run once with n doubled, and a second miss is INCONCLUSIVE and goes to the owner. E14's (a)/(b) A/A intervals fail this rule, so E14 is indicative only. Any bound exceeded → a written waiver (arch 01 §5) or a design change before merge. STA occupancy and the longest single STA block are reported. Unmeasured as well: arm (f) (S1 round trip, B13) and the product-boot clock. | Single arm, no arm (e), no negative control, or paint measured from `NavigationCompleted` → validator RED. |
| AC21 Receipt | A Windows acceptance claim | Runtime string, OS build, head, admitted-root mode, per-AC counters, at-risk counts per deciding rule, each RED output, "rule not exercised" rows, UNKNOWN cells (§3 AC23). | Missing fields or a zero-failure row without its at-risk count → validator RED. |
| AC22 Non-discriminating controls recorded, not counted | Probe N1 (only a dedicated worker was attempted; shared/service workers untested), N3, N6 (compared constants; no CDP result replayed), the exclusive-lock oracle (empty at-risk set); AC05's double-decode mutant; AC06's (c′) double fault (scored at the association audit, labelled, never counted as a single-fault kill) | Each reads **rule not exercised** and contributes nothing to scores; N6's claim is replaced by AC11 (ii). | Count any of them → validator RED. |
| AC23 Coverage cells with UNKNOWN status | favicon (probe `r-favicon` timed out), `<link rel=prefetch>`, `navigator.sendBeacon`, web app manifest, media `Range` (probe media cells used filler bytes; `mediaError 4` proves nothing), `blob:`/`data:` workers (measured in E11 and moved to AC28), Shared/Service Worker construction attempts, `<embed>`/`<object>` PDF, `<a download>`, `<script type=module crossorigin>` and dynamic `import()` (default Vite/electron-vite output) — measured in E11 and moved to AC24, and the environment worker filter's survival when the first webview closes (the Host re-registers it on the next live webview; cell: close the first window, open a second, attempt a shared worker) | Each is run as its own cell on the real device and reported served/refused/UNKNOWN with the request kind observed; none is claimed from the probe. | Claim any of them from the probe → validator RED. |
| AC24 Request classification (A7) | One load issuing a top-level document, an `<iframe>`, an `<object>` and an `<embed>` HTML document, classic `<script>`, `<script type=module>` (same and other directory, static `import`), `import()`, `<link rel=modulepreload>`, `<script crossorigin>`, `<img crossorigin>`, `<link rel=stylesheet crossorigin>`, `<link rel=preload as=font crossorigin>`, `@font-face` (inline, external stylesheet, `FontFace` API), plain `<img>`, plain stylesheet, XHR and `fetch()` to in-root `file:` URLs; the same classic script, plain image, plain stylesheet and media file with a case variant of the root prefix; **dev/release differential (r10)**: the same application under `keld dev` (`hsa`) and as an installed launch, with an `onHeadersReceived` listener, recording response headers on a classic script and an image, `Content-Type` for `.wasm`, `.svg` and `.mjs`, and a media seek (also on a request past the 256-hold bound); the listener cells and NC (5) run in T5, which lands S1 (B13) | WRR log per request: contexts 1 and 7 (8 if reported) and every request whose WRR headers carry `Origin` (E11: value `null`, no `Sec-Fetch-*` visible) → **Host-served**; classic script, plain image, plain stylesheet, `@font-face` (no `Origin`) and other 16 → **verified, held, no response**; a case variant of the root prefix → **Host-served** whatever its context, no hold (AC30), a media seek on it recorded like the dev/release one (no `Range`, §4.4). Page oracle (E11 `cm-hso`, 2/2 runs): every module, `import()`, `modulepreload`, `crossorigin` script/image/stylesheet and crossorigin font-preload cell loads and executes, without `Access-Control-Allow-Origin`; classic script, plain image and the three `@font-face` forms load on pass-through. In-root `fetch`/XHR `200` with no switch; out-of-root `403` empty (E10). Dev/release: app headers reach exactly the release classes in both modes and `Content-Type` matches per class; any media-seek difference is recorded as a divergence (Range is UNKNOWN, AC23). | (1) Classify by `RequestedSourceKind == DOCUMENT` (E9: `DOCUMENT` for every request) → every subresource is served → served count = request count (19/19 on the E8 workload) and E8's serving cost returns (+34–36 %) → RED. (2) Drop the `Origin` rule (r7 `hs`) → the module same-/other-directory, static-import, `import()`, `modulepreload`, `crossorigin` script/image/stylesheet and crossorigin font-preload cells fail (E11 `cm-hs`, 2/2) → RED. (3) Pass context 7 through → in-root `fetch`/XHR fail (`TypeError`/status 0, E10 `sx-hyb-off`) → RED. (4) Pass frame `DOCUMENT`s through → the frame carries no Host CSP header and the AC25 `'none'` fixture's scripts run inside it → RED. (5) Under `hsa`, apply app headers to every Host-served request regardless of release class → the dev classic script carries the listener's header and the installed one does not → RED. |
| AC25 The served document's CSP governs pass-through subresources; CSP is not containment (A16) | Host-served documents carrying, one per cell, `script-src 'none'`; `script-src 'self'`; `script-src 'nonce-…'`; `img-src 'none'; style-src 'none'`; `script-src file:///<root>/csp/ext.js`; no CSP — each loading same-directory `file:` scripts, an inline script, a nonced inline script, a `data:` script, a stylesheet and an image; **cross-directory** `'self'`, `'none'` and no-CSP pages at the root and at a deep path loading `js/a.js`, `lib/x/b.js`, `./js/../lib/x/c.js` (root) and `../../js/a.js`-style paths (deep page), plus an out-of-root script and a junction-reached one; **T2 device cell**: a `'sha256-…'` hash source | As E9 `h-csp` (with the switch) and both E10 `hs` corpus runs (no switch; origin as measured in E10's no-switch matrix rows under the same environment configuration), every cell identical: `'none'` blocks every script; `'self'` runs the same-directory `file:` scripts and blocks inline/`data:`; the nonce policy runs only nonced scripts; `img-src`/`style-src 'none'` block the pass-through image and stylesheet; no CSP runs everything. Cross-directory (E11 `cx`, 12/12 cells over 2 runs): `'self'` runs every in-root script from the root and from the deep page, inline is blocked, `'none'` blocks all. **CSP is not a containment boundary**: under `'self'` the out-of-root and junction-reached script URLs raised **no** CSP violation; only the Host's `403` stopped them (E11). The exact `file:` host-source blocked its own script on this runtime — recorded as a runtime quirk (CSP3 would admit it), not required. Pass-through `Content-Type` is Chromium's extension mapping (E9). Hash sources are UNKNOWN until their cell runs. | (1) Drop the CSP header from the served document → the `'none'` cell runs its `file:` scripts → RED. (2) Run the `'self'` cross-directory page with the Host's refusal disabled (observe) → the out-of-root script runs with no CSP violation (INFERRED from E11, where only the `403` stopped it) — shows that containment rests on the Host, not on CSP. |
| AC26 Root swap and in-root race (A20, A23) | (i) After admission the root's **parent** mutation owner renames `<root>` → `<root>.swapped` and plants a junction `<root>` → OUT (look-alike names, other nonces), then the page loads Host-served and pass-through requests. The fixture root has **nothing open beneath it** except the Host's own root handle, because an open handle beneath a directory alone makes its rename fail with `ERROR_ACCESS_DENIED` (5, E9 share-mode table; both production roots contain the running `keld-host.exe`, `app_session.rs:1533-1534`, `:3014`); (ii) after verification and before Chromium's open, a pass-through classic `<script>`, `<img>` and stylesheet have their leaf replaced by rename and, separately, their directory replaced by a junction to OUT (deterministic injection); (iii) the leaf is replaced by an **in-root** file right after verification (the instant at which E11's separate hold chain had a gap); (iv) **release cells**: success, cancelled (`src` cleared, `window.stop()`, iframe removed or navigated), removed-from-DOM, CSP-blocked, duplicate and same-URI loads, page and Host navigation, webview close; (v) **T2 device cell**: three documents in sequence, each holding, then a blank document; and one document issuing 300+ distinct pass-through images (the 256-hold bound); and, under `keld dev` (`hsa`), one document loading `a/b/img.png` (r12); (vi) **subresource across the G-change** (r12): a hook holds `ContentLoading` of document G₂ in the Host's queue while G₂'s first pass-through subresource WRR is handled and held, then releases it, and the harness replaces that file by rename-over before Chromium's open; (vii) **hold across a Host attempt's issue** (r13): a hook defers document G₁'s pass-through WRR after its hold is taken, the Host issues a `loadURL` (#1192's +1 at issue, L110), the hook completes the deferral, and the harness replaces the file by rename-over before Chromium's open; run once with the attempt committing and once with it failing | (i) The rename fails with **`ERROR_SHARING_VIOLATION` (32)** — the code is counted, so a 5 from a handle beneath does not pass — and every cell matches its verified identity (E9 hybrid with the no-`DELETE` root: 0/9). (ii) The verified bytes are delivered 24/24 per kind and injection, every swap fails (32), and, under r10's retirement-only release, a held leaf cannot be rewritten (32) or replaced by rename (5) until retirement and both succeed afterwards (E14 rewrite cell, 3 runs; E11 `hsh`, `hsh-post` measured r9's earlier release). (iii) The verified bytes are delivered: E12 gap cell 288/288 (swaps fail 32, rename-over 5); the hold **is** the verification handle (opened share `READ`), so no verify → hold gap exists. (iv) No hold outstanding after retirement or close in any cell (E12, 3 runs each, `hsoh` and `hsod`; E14 `hsoz`, 3 runs); cancelled loads released only by retirement/close; same-URI holds released as a multiset. (v) Each document's holds are all released at its retirement and none earlier; while held, a rename of each document's own directory (the documents use disjoint directories; one that renames is renamed back and the cell fails) fails **32** (the retained chain, AC29) and succeeds after that document's retirement; 0 outstanding after the last retirement and every exclusive open succeeds; at the bound the first 256 are held, the rest Host-served, all images decode (E14 `hsoz`: 3/3 each; peak 768 handles). Under `hsa`, after the load completes and while the document lives, a rename of `a/b` succeeds (no chain is retained for `Serve`). (vi) The verified bytes are delivered and the hold is released only at G₂'s retirement; at-risk 1 per run. (vii) The rename-over fails (32/5) and the verified bytes are delivered; the hold is released at the attempt's commit, or, when the attempt fails, only at a later commit or webview close; at-risk 1 per variant. | (i) Open the session root handle **with** `FILE_SHARE_DELETE` (E9's probe control; built in the STA harness as a std handle wrapped by `Dir::from_std_file` and served in `hso` mode — in production only the dev stage's std handle could carry it, since cap-primitives strips `DELETE` from every directory open, `oflags.rs:28-36`, and the dev stage runs `hsa`, where every byte is read through handles beneath the original root) → the rename succeeds where the baseline fails 32 and later pass-through reads come from OUT: E9 control 4/9 under all-context pass-through; under `hso` the predicted at-risk set is 2 of 9 (the classic script and the plain image; INFERRED) → RED. If the fixture cannot keep the root free of handles beneath it, the NC is pre-declared **rule not exercised**. (ii) No hold (r7 `hs`) → the replacement 24/24 and OUT 24/24 per kind (E11 `sr-hs`) → RED. (iii) No hold (`hso`) → the gap attack succeeds 144/144 (E12) → RED. (iv) Remove the retirement/close release → 14 (`hsoh`) / 15 (`hsod`) holds outstanding and the exclusive-open detector fails 32 (E12) → RED. (v) Remove the retirement release (E14 `hsozx`) → 18 holds outstanding and 18 exclusive opens fail 32 → RED; remove the bound → more than 256 holds outstanding in the bound cell → RED; release the leaf but keep the chain at retirement → the directory renames still fail 32 → RED; retain the `Serve` chain → under `hsa` the rename of `a/b` fails 32 → RED. (vi) Key the hold by the generation current at the WRR (for G₂'s Host attempt, the attempt's `AwaitingCommit` generation from its issue-time +1, #1192 L110, which stops being current at the attempt's commit, L111) → the released `ContentLoading` frees it, the rename-over lands before Chromium's open and the replacement is delivered → RED. (vii) Release holds at the issue-time G-change → the rename-over lands before Chromium's open and the replacement is delivered → RED. |
| AC27 Drive-letter binding (A21) | The project is reached through a `subst` letter `K:` → `…\A`; a separate same-user process rebinds `K:` → `…\B` (look-alike files) while the page loads | (i) Admission from the `K:` spelling is **refused** typed (`KELD-*`, naming the final path to launch from) because `QueryDosDeviceW("K:")` names a session-defined target (`\??\…`), not a volume device. This is fail-early UX: containment does not depend on it, because the root URL is spelled from the final path and R7 refuses every `K:` URL as `OutsideRoot`. A project reached through a junctioned ancestor, cross-volume included, is **admitted** and re-spelled from its final path. (ii) A root spelled by its final path is unaffected by rebinding a spare letter: E9 0/12 — that run's URLs were spelled with the final path's `D:`, so it does **not** test `K:` URLs. Not exercised (pre-declared UNKNOWN): shadowing the **real** volume letter of the final path. E9 `ddd`: an AppContainer and an LPAC child get `ERROR_ACCESS_DENIED` (5) from `DefineDosDeviceW`; a medium-IL same-user child succeeds and the Host's session sees the mapping. | (i) Remove the admission refusal → no typed `KELD-*` error at admission (the app boots under the `D:` spelling and every `K:` link is `403` `OutsideRoot`) → RED. (ii) With a declared test seam that admits a `K:`-spelled root URL (precondition printed; STA harness in `hso` mode — under `keld dev` (`hsa`) every byte is read through the root handle and a rebind cannot reach a read), spell the URL from the requested letter → E9: 5 of 12 cells, exactly the pass-through ones, deliver `B`'s bytes → RED. |
| AC28 Keld-owned browser arguments (A22) | (i) The environment as Keld creates it; the page constructs `new Worker('wesc.js')` (in-root) and `new Worker('../../out/wout.js')` itself, and `blob:`/`data:` classic, module and shared workers whose `importScripts`, `fetch`, `import()` and nested `Worker` target out-of-root, junction-reached and in-root files; `navigator.serviceWorker.register`; (ii) `self.origin`/`location.origin` and an in-root `file:` frame's DOM; (iii) in-root and out-of-root `fetch`/XHR and a cross-origin `https:`/`http:` URL without CORS; (iv) `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--allow-file-access-from-files` (and, separately, a benign `--user-agent=…`) is set (a) in the environment of `keld dev` and (b) in the environment of a directly launched installed `keld-host`; (iv′) **fail-first case and enumeration cells (r16)**: on a real Windows installed-host launch (no Keld launcher), separately, `webview2_additional_browser_arguments=--allow-file-access-from-files` (lowercase), `WeBvIeW2_UnknownFutureSetting=x` (mixed case, unknown suffix) and the canonical `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--allow-file-access-from-files`; under `keld dev`, the same three spellings in the launcher's environment; an isolated guard-off fixture first records, per spelling, whether the loader applies it (worker cells escape); (v) **T2 device cell** (B16): each loader policy of rule (3) under HKCU, value names the executable's file name, `*` and the process's AUMID (one ending in `.exe`); `AdditionalBrowserArguments`, `UserDataFolder` and `BrowserExecutableFolder` (administrator-only and user-writable targets) under HKLM; (vi) source test over `keld-wv`/`keld-cli`, and a unit test of the one name classifier | The oracle counts **out-of-root nonces delivered** (WRR counts are reported, not scored). (i) Page-constructed `file:` workers throw `SecurityError`; `blob:`/`data:` classic and `data:` module/shared workers construct with origin `null`, `blob:` module and `blob:` shared workers fail to start, `serviceWorker.register` rejects; every `file:` load from those workers fails — out-of-root and junction `importScripts` raise WRR and get `403`, in-root `importScripts` is allowed and passed through yet still fails `NetworkError`, and `fetch`/`import()`/nested workers raise no WRR and are blocked by Chromium — identically in an observe run (E11 `b15-hs` ×2, `b15-hs-allk`, `b15-observe`): **0** out-of-root nonces. (ii) `self.origin == "null"`, `location.origin == "file://"`, the frame's `contentDocument` is `null`. (iii) In-root `200` (Host-served), out-of-root `403` empty, cross-origin fails (E10). (iv) (a) the launcher removes the variable from the Host's environment, the environment is created and 0 nonces arrive (INFERRED from E11 `env-af-clearenv`, where it was absent at creation); (b) `keld-wv` refuses environment creation with a typed `KELD-*` error naming the variable and no page loads (E11 `env-af-refuseenv`). (iv′) Each installed-host cell is refused typed, naming the variable as spelled, **before environment creation** (keld-wv's environment-creation count 0) with **0** out-of-root nonces; under `keld dev` the launcher removes all three spellings, the Host's enumeration finds none, the environment is created and the sanitized launch reaches its page with 0 out-of-root nonces. A spelling the guard-off fixture shows the loader ignores is reported; its refusal still scores and its escape counts as **rule not exercised**. (v) Typed `KELD-*` refusal naming the root, key and value name for every refused policy; an HKLM `BrowserExecutableFolder` at an administrator-only folder launches and the boot diagnostics record that runtime's version and image path; at a user-writable folder it is refused (B16). (vi) Options `AdditionalBrowserArguments` empty; the KEL-66 test (`webview2/mod.rs:4851-4875`) is extended to refuse `--allow-file-access-from-files` and `--allow-universal-access-from-files` by name; the classifier unit test matches a prefixed name with an empty value, a non-Unicode value and a non-Unicode suffix, does not match `WEBVIEW2` without the underscore or `XWEBVIEW2_…`, and finds a match placed last in the enumeration. | (1) Put `--allow-file-access-from-files` in the options → every worker kind and the page-constructed out-of-root worker deliver OUT nonces (3/3 targets) with zero WRR events for those loads (E10 every host mode; E11 2 runs and with source kinds `ALL`) → RED. (2) Drop both the launcher removal and the refusal with the variable set → OUT nonces 3/3 and `self.origin == "file://"` (E11 `env-af`); separately drop only the refusal → case (b) escapes (installed launches have no Keld launcher); options `""` or no options object does not override it, and on a conflicting argument the environment value wins (E11 `env-af-nullopts`, `env-afua-optua`: the page's user agent is the variable's) → RED. (3) (r16) Single faults on the (iv′) cells, each in its own run: (3a) a case-sensitive `starts_with("WEBVIEW2_")` → the lowercase and mixed-case installed-host cells create the environment (count 1 where the baseline reads 0) and, where the guard-off fixture showed the loader applies the lowercase spelling, OUT nonces arrive → RED; the uppercase cell stays refused, so the fault is single. (3b) a fixed `var_os` list of Q21's five names → the `WeBvIeW2_UnknownFutureSetting` cell creates the environment → RED (the lowercase cell is not this mutant's at-risk cell: `var_os` on Windows resolves a name through `GetEnvironmentVariableW`, case-insensitively, as observed above). (3c) the launcher removes a fixed uppercase list of Q21's five names → under `keld dev` the `WeBvIeW2_UnknownFutureSetting` spelling reaches the Host, which refuses, so the sanitized launch fails where the baseline launches → RED (the lowercase spelling of a listed name is not this mutant's at-risk cell: std's Windows `EnvKey` removes it case-insensitively). |
| AC29 Resource-source API: open intent and chain ownership (A23, C3) | Windows test of keld-core's `AppResourceSource` through the published trait, over a fixture root `<root>/a/b/f.js` with nothing else open beneath `<root>`: (i) `open([a, b, f.js], Serve)` and, with the result alive, an in-place rewrite of `f.js` and a rename of `f.js`; (ii) on a fresh copy of the fixture ((i) and (ii) each start from one; (iii) continues (ii)), `open([a, b, f.js], Hold)` and, with the result alive, the same rewrite and rename, then a rename of `a/b` and of `a`, and then, with its `chain` kept and its `file` dropped (both fields are public), the same two directory renames; (iii) the `OpenedResource` of (ii) dropped (the hold table's retirement step), then the same four mutations | (i) Both succeed (`Served` shares `READ \| WRITE \| DELETE`). (ii) The rewrite fails 32 and the leaf rename fails (5 or 32, counted); the renames of `a/b` and `a` fail with **32** (`ERROR_SHARING_VIOLATION`: the retained chain handle lacks `FILE_SHARE_DELETE`), not 5, which is what a leaf handle beneath produces on its own (E9 share-mode table), so the code separates a retained chain from a leaf-only hold (INFERRED until run: E9 measured 32 for a directory handle without `DELETE` and 5 for a handle beneath, never both at once); with the leaf dropped and the chain kept, both directory renames fail 32. (iii) All four succeed. | (1) `Serve` mapped to `Share::Pinned` → (i) the rewrite fails 32 → RED. (2) `Hold` mapped to `Share::Served` → (ii) the rewrite succeeds → RED. (3) The chain dropped inside `open` (a leaf-only hold) → with the leaf dropped and the chain kept, the renames of `a/b` and `a` succeed where the baseline fails 32 → RED (discriminating whatever code Windows returns while the leaf is also open; the 5-vs-32 reading of that combined state stays INFERRED). (4) The source keeps a clone of the chain (a hidden map) → (iii) the directory renames still fail → RED. |
| AC30 Case identity: two locks (A24) | An NTFS fixture built by an elevated operator (a UAC action; without it **rule not exercised**, receipt says so; precondition printed): `<T>\caseparent` case-sensitive; its children `MyApp` and `myapp` distinct; `MyApp` cleared while empty (Microsoft says new directories inherit, and elsewhere that Windows-created ones are not case sensitive; unmeasured, E16, so the fixture reads every directory's flag, §7); each holding `js/app.js`, `js/m.js`, `css/a.css`, `img/a.png`, `data.json`, `f.html`, SAFE in `MyApp`, OUT in `myapp` (same names, other nonces); and a regular tree `<T>\plain\MyApp`. (i) Admit `<T>\caseparent\MyApp`; (ii) admit a root that itself carries the flag; (iii) **positive, regular case:** admit `<T>\plain\MyApp`; request each kind with the exact spelling, with a lower-case drive letter, and with the variant `<T>/plain/myapp/…`; (iv) **lock 2 alone:** the STA harness in `hso` mode builds the root over `<T>\caseparent\MyApp` directly through the published `AppResourceSource` trait, so keld-core's boot admission is not on this path (precondition printed; no hook in admission exists and none is added), and the page requests `file:///<T>/caseparent/myapp/…` as classic `<script>`, stylesheet and `<img>` (pass-through classes) and as module script, `fetch` and `<iframe>` (Host-served classes); (v) a case-sensitive directory beneath a regular root holding `A.js` and `a.js`; (vi) with a root admitted and pinned, the elevated operator enables the flag on its parent and on the root; (vii) a seam fails the flag read on a volume that reports `FILE_CASE_SENSITIVE_SEARCH`, and separately a seam clears that bit (a FAT volume has no DACL for the dev stage's check to pass, INFERRED, so it cannot reach this read) | (i), (ii) Admission refused typed (`KELD-*`, naming the directory), environment-creation count 0, opener count 0. (iii) Exact and lower-case-drive spellings keep AC24's classes; the variant is Host-served for every kind from the pinned handle: pass-through count 0, no hold, delivered identity SAFE, each kind loads under the fixture document's `script-src 'self'`, and `Content-Type` equals the exact spelling's (§4.4's drift guard). (iv) Every kind `200` from the pinned `MyApp` with SAFE; pass-through count 0, no hold; OUT's nonce 0 in the page; OUT never read: Host opener count on `myapp` 0 and no `msedgewebview2.exe` open of any OUT file in an elevated ETW session (ETW not run → INFERRED-strength, printed). (v) `403`, empty, `OpenRefusal::CaseSensitive`, bytes read 0. (vi) The OS result of each set is recorded: Microsoft predicts a refusal on a non-empty directory (§4.4); if a set succeeds on a pinned root, (iv) re-runs through real admission with the flag set afterwards, and the receipt says Microsoft's rule did not hold here. (vii) Failed read: admission refused typed. Bit clear: no flag read, `directory_case` returns `Insensitive`. At-risk: 6 kinds per run in (iv), 1 per other cell. | One fault each. (1) **Lock 1 reads only the root's flag** → (i)'s admission succeeds (admission count 1 where the baseline reads 0) → RED, scored at admission and never at OUT, because lock 2 still Host-serves the variant; (iv) stays green. (2) **Pass through on ignore-case R7 equality** (`root_spelling` ignored) → in (iv) the classification audit counts 3 pass-through requests where the baseline counts 0 → RED; where Chromium then reopens the variant, OUT's nonce reaches the page, the stronger RED; where it refuses or opens SAFE the receipt records that the reviewer's exploit path did not materialize on this runtime, and the audit's RED stands. Admission's refusal in (i) would pre-empt this mutant, which is why (iv) runs beneath admission in the harness: with mutant (2) through boot, (i) still refuses, so each lock flips alone. (3) Host-serve by reopening the URL's path instead of the pinned handle (AC04's `CreateFileW` fault) → (iv)'s Host-served classes deliver OUT → RED. (4) Drop the walk's per-directory flag read → (v) serves `200` → RED. (5) Treat a failed flag read as clear → (vii) admits → RED. (6) Compare the drive letter ordinal → AC05's other-case golden → RED; on the device the URL arrives upper-cased (§4.4), so the lower-case-drive cell is **rule not exercised** unless the WRR URI itself shows a letter case other than the admitted one, receipt says so. |
| AC31 Host-served memory budget (A25) | A fixture root with near-cap files (each `MAX_HOST_SERVED_FILE_BYTES` − 1 MiB), one file over the cap, and more than `MAX_HOST_SERVED_IN_FLIGHT` small files. (i) The page issues parallel `fetch`es of three near-cap files and 1,100 small ones while `profile-test-hooks` barriers hold first worker completions, then STA delivery, then release; (ii) the same under `keld dev` (`hsa`), adding classic scripts and images; (iii) release mode at the 256-hold fallback: 256 holds taken, then near-cap classic images; (iv) the over-cap file in release as classic `<script>` and `<img>` with holds free, then at the bound, then as `crossorigin` script, module, `fetch`, `<iframe>` and with a case variant of the root prefix; under `keld dev` as each of those; (v) **positive:** the E8 draw.io workload and the E13 Vite fixture in both modes; (vi) webview close with one read queued and one running at the worker barrier, a second webview refilling the budget meanwhile; a `fetch` aborted in flight; a document navigated away with entries in flight; an injected worker read error; and an injected failure to create the refusal response; (vii) **residency (Q29):** the page keeps 8 near-cap `Response` objects with unread bodies | At the ledger owner (`profile-test-hooks` readback) at every barrier in every mode: peak reserved bytes ≤ `MAX_HOST_SERVED_RESERVED_BYTES`, peak in-flight ≤ `MAX_HOST_SERVED_IN_FLIGHT`, active read workers ≤ `HOST_SERVE_READ_WORKERS`; an independent test accountant around the worker's buffer allocation agrees. Exactly one response per request (per-WRR ledger: `200`, `413` or `503`; deferrals completed = deferrals taken); reserved bytes 0 at quiescence, and a request refused `503` succeeds when re-issued then. (iv) Release: classic script and image delivered through the hold, no reservation; at the bound and for every other kind `413`, empty; under `keld dev` `413` for all. (v) No `413`, no `503`; peak reserved, peak in-flight and the longest worker-queue wait reported per workload and mode (§4.5). (vi) Every entry settled once, reserved bytes 0, a running read's reservation released only at its worker's post, and no deferral completed with no response set; with no refusal response the webview is retired typed and Chromium delivers nothing for that URL. (vii) Host private bytes peak recorded against Q29's gate. | One fault each, in the cell it names; (1) and (3)–(6) run in (i), (ii) and (iii). (1) **Disable only the aggregate reservation** → the accountant's peak exceeds the budget → RED. (2) Drop the per-file cap → (iv)'s file is read into a buffer → RED. (3) Drop the in-flight cap → peak in-flight exceeds the constant → RED. (4) Release the reservation at worker completion → with STA delivery held the accountant's peak exceeds the budget → RED. (5) The overflow path drops the deferral → that request gets no response by the harness's kill bound → RED. (6) The error path completes twice → the ledger reads 2 → RED. (7) An over-cap `CaseVariant` request falls back to pass-through → hold count 1 where the baseline answers `413` → RED. (8) The error path completes with no response set → the file's bytes reach the page in (vi) where the baseline delivers none → RED. (9) Webview close releases a running read's reservation → in (vi) the second webview's reads push the accountant's peak over the budget → RED. |

## 4. Design

### 4.1 Atomic decomposition (decision input)

Evidence classes: FACT, PROBE (run ids `baseline r1–r3`, `n1–n6`), INFERRED, UNKNOWN (named
probe). Security atoms separate identity, authentication, authorization, OS containment,
lifecycle/revocation and evidence provenance; performance atoms are in §9.

| Atom | Owner / boundary | Inputs → outputs | Failure mode | Observable | Evidence | Edges · first falsifier |
| --- | --- | --- | --- | --- | --- | --- |
| A1 Loading path | `keld-core` boot → `keld-wv` `navigate` | admitted root + entry components → `Navigate(file URL)` (a header navigation in header-proven mode, §4.6 rule 1) | string load yields `about:blank` identity | AC01 | FACT W1 source probe; code `:3653`/`:3654` | · AC01 NC |
| A2 Request census and filter scope (coverage, cost) | WebView2 filter normalization + `keld-wv` registration | `file:*` + `CONTEXT_ALL`; source kind `DOCUMENT` per webview before first `Navigate`, `SHARED_WORKER \| SERVICE_WORKER` once per environment (together: all source kinds) → every `file://` request a page or worker can issue raises WRR, no other scheme does; worker `fetch`/`import()`/nested-worker loads raise none and Chromium blocks them without the switch (E11) | late registration; `*` filter puts every remote request on the UI thread; worker source kinds missing (shared-worker `importScripts` invisible) | AC02, AC19, AC28 | PROBE n2 (under `*`; second document only per note 268); PROBE E11 (`data:` SharedWorker `importScripts`: WRR only with the worker source kinds); FACT L1307 normalization, `_22` L51/L63 | precondition of A8 · n2 under `file:*`, both documents |
| A3 Admitted root (identity of the root) | `keld-core` boot (dev DACL; KEL-254 selection) | admission → one retained `Dir` | handle dropped and reopened by path | AC04 | FACT dev handle dropped today; `tree: Dir` retained | → A5, A20, A21 · junction swapped in after admission is followed |
| A4 URL → components (authentication of the name) | `keld-guard` parser | URI + `AdmittedRootUrl` → components or refusal | double decode; authority; ADS; `~`; Win32 spellings | AC05 | PROBE parser cells; FACT grammar; FACT Chromium pre-canonicalises `%2e%2e` | independent of A5 for `:`/`~`; redundant for `..`/trailing dot · AC05 NC |
| A5 Open (OS containment) | `keld-update` `open_regular_beneath`, **per-component** cap-std no-follow walk (normative) | `&Dir` + components → regular-file handle + `nlink` | reparse followed (multi-component `open_with` follows intermediate relative symlinks); directory; Win32 name layer | AC03 opener cells, AC04 | FACT cap-primitives relative `NtCreateFile` + `FILE_OPEN_REPARSE_POINT`; PROBE `gh1185-capstd-junction-2026-10-30` (3/3 identical, keld-research note 268): cap-std 4.0.3 refuses an intermediate junction in every form (multi-component path and default `open_dir`: "a path led outside of the filesystem", for out-of-root and in-root targets alike; `open_dir_nofollow`: os error 681), while the ambient `std::fs` negative control reads the out-of-root sentinel through the same junction; directory/file symlinks UNTESTED (creation needs privilege) | ← A3 · AC04 NC |
| A6 Hardlink authority (authorization) | KEL-254; W3 reads back | `nlink` + DACL → served + receipt | namespace write taken as the only authority | AC17 | FACT owner decision + correction | · role write grant |
| A7 Request classification and response contract (hso) | `keld-wv` handler | WRR `ResourceContext` 1 (documents incl. `<object>`/`<embed>` HTML), 7, 8 (`fetch`/XHR), any request whose WRR headers carry `Origin` (CORS mode), or any request with a `CaseVariant` root spelling (§4.4): `200` from the verified handle with Host `Content-Type` (one `keld-wv` table), `Content-Length` and S1 headers, the baseline CSP on 1 only; any other context: verified and held, **no** response (A23); refusal: `403`/`404` empty | classifying by `RequestedSourceKind` (always `DOCUMENT`: every request served, E8 cost); a document passed through (no Host headers); `fetch`/XHR or CORS-mode loads passed through (they fail from an opaque origin) | AC03, AC09, AC16, AC18, AC24 | PROBE E9/E10 census; PROBE E11 `cm` (every CORS-mode cell fails passed through, loads Host-served; `Origin: null` at WRR; no `Sec-Fetch-*` visible) | → A16, A23 · AC24 NCs |
| A8 Served-document identity | `keld-wv` per-navigation record for **every** top-level navigation (all schemes) | allowed `NavigationStarting(n)` + DOCUMENT WRR carrying `n`'s attempt nonce (r13; URI equality is not proof) → `ServedDocument` | URL text as identity; a frame record (same URI included) attached to `n` | AC06 | INFERRED timing; UNKNOWN `IsErrorPage` for Host `404` (Q7); FACT no frame or navigation id on WRR (SDK 1.0.3650.58); UNKNOWN nonce on the WRR request (Q22 b) and page-initiated proof (Q27, B19) | ← A2, A10; → A12 (gate sees all navigations) · AC06 NC, (d′) |
| A9 Navigation events → facade | `keld-wv` → `AppWindowEvent` → facade | WebView2 events → Electron emission rules | fabricated/reordered; `did-fail-load` on cancel | AC07, AC09 | FACT Electron v45.1.0 L2745–2752, L2872, L2932–2933 | ← A8 · AC07 NC |
| A10 `will-navigate` policy and attribution | `keld-wv` (allowlist on the original event) + facade (notification) + `keld migrate` (allowlist derivation) | `NavigationStarting` → allow/cancel (+0) and notification | allowlist after +1 (revokes live document, W1 AC10); `IsUserInitiated` as initiator; URI equality or event order as proof of Host initiation (r11/r13, AC08 (l)); redirect hop of a Host attempt cancelled | AC08 | FACT: no deferral; FACT #1192 L107 (+0 on cancel); UNKNOWN history-kind emission (P3); UNKNOWN header exposure for `file:` (Q22, B18) | edge to #1192 L108–112 and W1 AC10 made explicit: W3 performs no Host load for page navigations · AC08 NC |
| A11 Eval mechanism | `keld-wv` CDP | script → settled value/rejection/destroyed | `ExecuteScript` targeting; codegen scope | AC10 | PROBE n4; FACT V8 L151, L400–402 | · n4 |
| A12 Eval lifecycle/revocation | one `keld-wv` state machine (`Pending → Retired \| Settled`) over #1192's generation, local settlement only; the handoff is one synchronized admission transaction of the Host primary writer, `PrimaryRouterHandle::try_admit_eval_settle` (new, r16), over one Host-minted eval ticket and one bounded correlation ledger, with the STA G-change and the role retirement updating the same fence (§4.7 step 4, r16, B21 decisions recorded) with the main-frame context bound only across a verified transition (§4.7 step 1, r13: transition gate, fresh loader, uniqueness; Q23 and the lag premise, B17); role axis = KEL-75/KEL-143's retirement | `(G, ctx)` and the eval ticket minted at dispatch → settle locally iff the entry is still `Pending` when CDP completes; hand off iff the admission transaction verifies the ticket's provenance and, for a value-bearing record, finds the current `G` equal to the ticket's; retire in the STA turn that advances G | stale delivery, including a late transport send after revocation; a G check released before enqueue; a foreign, duplicate or wrong-role terminal admitted; eval in the gap runs in the new document; a delayed context of an older document paired with a newer generation; a newer same-URL document's context paired with the older generation; BFCache never completes | AC11, AC12 | PROBE r1–r3 (`-32000` on replacement); **no probe evidence for the replay rule** (N6 compared constants — note 268); FACT CDP out-of-order (L1333) | ← A8 (all schemes) · AC11 (ii) NC, AC11 (v)–(viii) NCs (8)–(18), (20), (21); AC12 NC |
| A13 Eval capability (fail loud) | create-time self-test | CDP calls → start / typed refusal | silent per-call rejection; unknown-field acceptance mistaken for honoring | AC13 | FACT crdtp L124–128 | · AC13 NC |
| A14 Eval value domain | #1190 owner (decoder of the deep-serialized form; domain losses) | `deepSerializedValue` bytes ≤ bound → decoded domain | 4096 bound; getters run and symbol/non-enumerable keys dropped (V4 not met); remote objects leaked without `releaseObjectGroup` | AC14, AC10 (e)–(g) | FACT D4; FACT V8 L222/L257, L339–341; FACT `injected-script.cc` L640–648 | B3/Q15 · AC14 NC |
| A15 App response headers (S1) | facade listener + `keld-wv` round trip | registered listener → modified headers on **Host-served responses** (1, 7, 8, CORS-mode); pass-through classic script/stylesheet/image carry none (recorded divergence) | pass-through on retirement; second URL matcher; round trip for pass-through contexts | AC15 | FACT D3 (source level); INFERRED from the API surface, untested: a request answered with no response cannot carry Host headers | → A16, A19 · AC15 NC |
| A16 Baseline CSP | `web.csp` owner for content; W3 for emission | policy → own header on every served `DOCUMENT` response; Chromium enforces it on that document's subresources | merged header; type-keyed emission; `file:` host-sources (blocked on 156); treating CSP as containment | AC16, AC25 | FACT CSP3; PROBE E9 `h-csp` and E10 `hs` corpus: identical outcomes; PROBE E11 `cx`: cross-directory `'self'` works 12/12, and `'self'` raised no violation for out-of-root URLs (only the Host's `403` stopped them); hashes UNKNOWN | ← A7, A22 · AC16 NC, AC25 NC |
| A17 Scheme boundary | #541/#460 | non-`file:` URI → never raised | `*` filter | AC19 | FACT L1307 | · AC19 NC |
| A18 Evidence provenance | spec + receipts | hashed runs → claims | second runtime; zero-failure without at-risk; probe cells misattributed | AC21–AC23 | FACT one runtime | · validator |
| A19 Registration ordering | `keld-core` control FIFO (**TARGET**: compat windows have no Host-navigated first document once W2 D2/D3 land) + recovery hold | facade registrations sent before `loadURL` apply to that load's first request; during recovery `file:` requests are held until the successor registers or posts `listeners-settled` (bounded; only if the predecessor had a listener) | FIFO broken; a Host-navigated first document before `ready` | AC15 (a), (d), (d′) | FACT: today Windows refuses schema-2 (`app_session.rs:3431-3438` `KELD-CORE-034`; `HostInitialized` is macOS-only); TARGET: W2 gh1188 `8c5e5d74` D2–D4; FACT: Electron apps register after `ready` (draw.io `electron.js` L987–994) | → A15; blocked by B13 · AC15 NC (FIFO order swap) |
| A20 Session root pin (lifecycle) | `keld-core` boot: the retained root `Dir` shares `READ \| WRITE`, never `DELETE` | admitted root handle → rename/delete of the root fails (32); ancestors cannot be renamed while a handle beneath is open | root shared with `DELETE`: the parent's mutation owner swaps the root and pass-through reads OUT persistently | AC26 (i) | FACT both existing handles already lack `DELETE` (dev `app_session.rs:1495-1498`; installed cap-std `dir_options()`); PROBE E9 hybrid 0/9 vs control 4/9 | → A23 · AC26 NC (i) |
| A21 Root drive-letter binding (identity of the root's spelling) | `keld-core` admission + `AdmittedRootUrl` | requested root path + handle final path → root URL spelled from the final path; fail-early refusal when the requested letter is session-defined (`QueryDosDeviceW` names a `\??\` target) | a literal `subst` letter spelled into the URL: a rebind redirects pass-through reads | AC27 | PROBE E9: literal `K:` 5/12 (the pass-through cells); final-path spelling 0/12 (spare-letter rebind; `K:` URLs not tested); real-letter shadowing UNKNOWN; AppContainer/LPAC `DefineDosDeviceW` → 5; the `\??\` heuristic misses a `DDD_RAW_TARGET_PATH` definition (UX only; R7 still refuses) | → A23; Strict equivalence #1187 (Q20) · AC27 NCs |
| A22 Keld-owned browser arguments (invariant) | `keld-wv` environment creation (KEL-66 owner) + every Keld launcher of `keld-host` (today only `keld-cli dev`, `dev.rs:363`) | options → **empty** `AdditionalBrowserArguments`; the launcher enumerates with `std::env::vars_os()` and removes every variable whose name matches the ASCII prefix `WEBVIEW2_` case-insensitively from the child's environment (`Command::env_remove`, safe); before each environment creation `keld-wv` enumerates the same way and refuses with a typed `KELD-*` error naming every match, whatever its suffix or value (safe); a refused rule-(3) policy → typed refusal | `--allow-file-access-from-files` from any source: workers run and read out-of-root files with **no** WRR event; `self.origin` becomes `"file://"`; in-root frames' DOM readable; a case-sensitive or fixed-name check misses `webview2_…` or an unknown suffix | AC28, §1 origin observable | PROBE E10 switch matrix; PROBE E11: the variable merges with the options and wins on conflict, options `""` does not override it, removing it before creation closes it (`env-af-clearenv`, in-process), refusing closes it (`env-af-refuseenv`); a launcher-side removal is INFERRED equivalent (the variable is absent at creation either way); E13 (Q21): `WEBVIEW2_USER_DATA_FOLDER` silently replaces the options' folder, `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` runs a copied runtime (and a missing folder fails `0x80070002` with no fallback); policies UNTESTED on device (B16) | → A16 (origin under which CSP was measured), §1 · AC28 NCs |
| A23 Pass-through verify-and-hold (OS containment of the object Chromium opens) | `keld-wv` handler + the C3 opener; the verification handle is the hold | classic script/stylesheet/image/font/media/other URI with an `Exact` root spelling → verified `(root, components)`; the leaf stays open share `READ` and every directory component stays open without `DELETE`; no response → Chromium opens the pinned object → release **only** at document retirement or webview close (no response handler; at the 256-hold bound the request is Host-served; `keld dev` Host-serves every request) | parser differential; a mutation between verification and Chromium's open; a hold that is never released; more than 256 holds outstanding | AC03, AC20, AC26 (ii)(iii), AC27, AC29 | PROBE E11 `pt` and `sr`; PROBE E12: gap cell verified 288/288 (swaps fail 32, rename-over 5) vs no-hold NC 144/144 attacked; no hold leaked in any release cell; cancelled loads (`src` cleared, `window.stop()`, iframe removed or navigated) get no `WebResourceResponseReceived`; a request in flight when the handler is added never gets one; the handler, not the hold, costs on module loads (+11.1 ms vs +0.3 ms); PROBE E13: the dynamic handler costs +6–8 % per Vite document; PROBE E14 (`hsoz`): gap 144/144 ×2 vs NC attacked, retirement releases all, bound cell 256 held / 64 served / 320 decoded, rewrite blocked while held and allowed after | ← A4, A5, A20, A21, A24 · AC03 NCs, AC26 NCs |
| A24 Case identity of the root (two locks) | lock 1: `keld-core` admission over `keld-update`'s flag read; lock 2: the `keld-guard` parser and the `keld-wv` classifier | root and ancestor handles → admit or refuse; request URL → `RootSpelling` → pass-through only if `Exact` | a case-sensitive ancestor lets R7's ignore-case class name a sibling, and Chromium reopens the raw spelling | AC30, AC05 | FACT per-directory flag and inheritance, as Microsoft's text only (Learn `wsl/case-sensitivity`, git `7b28cc1e`); FACT `FILE_CASE_SENSITIVE_INFORMATION`; PROBE E16 (not elevated, n = 1): the read is callable on NTFS paths and returns 0 on unflagged ones; UNKNOWN until AC30's elevated fixture: a read of 1, inheritance, a case-colliding sibling, a flag change on a pinned root, and Chromium's behaviour on the sibling (AC30 NC 2) | independent: neither lock reads the other's state · AC30 NCs (1), (2) |
| A25 Host-served memory | `keld-wv` STA reservation ledger | verified `size` → reserve or refuse before any buffer exists; release on the STA after `Complete` or the error path | whole-file buffers without an aggregate bound; a leaked deferral or reservation | AC31 | FACT demand E8, E11, E13; constants provisional (§4.5); UNKNOWN stream lifetime after `Complete` (Q29) | ← A7, A23, A24 · AC31 NCs |

### 4.2 Decisions taken as input (kept; evidence re-checked)

| Decision | Statement | Evidence this spec re-checked |
| --- | --- | --- |
| L1 | Load with `Navigate(file:///<admitted root>/<entry>)`; `NavigateToString` retired for application documents. Closes W1 B6 jointly with #1192/KEL-102. | W1 AC18 (3/3). |
| C2 (r8, owner decision relayed 2026-10-30, to be recorded on #1185 with B1) | **hso.** Every `file://` request raises WRR (filter registered before the first `Navigate`; `file:*`, never `*`; all source kinds across the environment) and is verified by the one parser and the one no-follow walk beneath the session-pinned root; contexts 1 (documents) and 7/8 (`fetch`/XHR) and every CORS-mode request (`Origin` present) are Host-served from the verified handle with Host headers (baseline CSP on documents); every other request is passed through while the verification handle is held; refusals are `403`/`404` empty. No virtual host, no custom scheme; browser arguments are Keld-owned (none). | E8: serving every byte +34–36 % (rejected); E9: pass-through 0 parser differentials; E10: r6's switch is a containment escape; E11: r7's `hs` breaks every CORS-mode load and its pass-through raced (24/24), a hold closes the race (24/24), Host-serving modules is faster than Chromium's own loader with the switch (−40.9 ms [−51.3, −35.8]) (§4.9, §9). |
| C4 | In-root hardlinks served; `nlink > 1` selects per-file DACL read-back and write-denial per role token; Legacy forfeits root integrity (printed). | 6491936458 + 6492237806. |
| E1/E5 | CDP `Runtime.evaluate` with `awaitPromise`, `allowUnsafeEvalBlockedByCSP:false`, **mandatory** context binding, generation authority as the one `Pending → Retired \| Settled` machine (captured at dispatch; retired in the turn G changes; settled locally only while `Pending`; handed off only by the Host primary writer's one synchronized admission of the eval ticket, §4.7 step 4, r16); create-time self-test fails closed. | D2; probe r1–r3 (settle, `-32000`), n4; V8 L400–402. The replay rule has **no** probe evidence (N6 was not a control, note 268) and is proven only by AC11 (ii). |
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
pub struct OpenedRegularFile { pub file: cap_std::fs::File, pub size: u64, pub nlink: u64, pub chain: DirChain }
/// Owns the directory handles the walk opened beneath `root` (root excluded), in walk order.
/// Private field and no accessor: its only operation is drop, which closes them (r12).
pub struct DirChain(Vec<cap_std::fs::Dir>);
pub enum OpenRefusal { Absent, Reparse, NotRegular, CaseSensitive, Other(io::Error) } // → 404 / 403 / 403 / 403 / 403
/// r17: one directory's case identity (§4.4): the volume attribute, then the flag read. Safe wrapper
/// over the one new read in windows_fs.rs; admission and the walk share it.
pub enum DirCase { Insensitive, Sensitive }
pub fn directory_case(directory: &std::fs::File) -> io::Result<DirCase>;
/// Walks `components` beneath `root` ONE COMPONENT AT A TIME: every directory step is
/// `open_dir_nofollow(component)`, the leaf is a single-component
/// `open_with(component, read, share, FollowSymlinks::No)`; after each step the handle's
/// metadata must show neither FILE_ATTRIBUTE_REPARSE_POINT (any tag) nor, for the leaf, a
/// directory, and every directory step must read `DirCase::Insensitive` (a failed read refuses,
/// `Other`). Never a multi-component `open_with` (cap-std would follow an intermediate
/// relative symlink). A leaf junction surfaces as AccessDenied (probe C6, raw 5), a symlink as
/// cap-std's Symlink error or raw 681 `ERROR_STOPPED_ON_SYMLINK` (E9); both map to `Reparse` → 403.
/// `share` is `Pinned` (FILE_SHARE_READ; the existing open_pinned_leaf contract) or
/// `Served` (READ | WRITE | DELETE: never blocks in-place rewrites or leaf renames; the root
/// itself is pinned, A20) for Host-served responses; pass-through verification opens the
/// leaf `Pinned` (READ only) and keeps that very handle as the hold (A23). `chain` is the
/// walk's own directory handles; a caller that needs no hold drops it.
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
- **Case-sensitive directories (r17, §4.4).** Admission reads the flag on the root and every
  ancestor (lock 1); the walk reads it at every directory step beneath the root and refuses a
  set flag (`CaseSensitive` → 403), because `components_equal` would otherwise name two objects
  as one. Cost: `directory_case`'s two reads per directory step, inside AC20's gates and unmeasured alone.
  Option (g)'s gate score above predates this read; the read is one `unsafe` block in
  `windows_fs.rs` and changes no ranking, since option (a) would need it too.
- **`keld-core` boot** retains the admitted root as a `Dir` (dev stage: `Dir::from_std_file`
  of the handle `validate_windows_dev_stage_acl` opens — one of the two remaining opens by path,
  which AC04's source test exempts as root admission, the other being the ancestor flag reads; installed: `active.tree()` with the
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
/// Why the leaf is opened (r12). `Serve`: leaf `Share::Served`; the Host reads it and answers.
/// `Hold`: leaf `Share::Pinned`; nothing is read, and the leaf and chain are the hold (A23).
pub enum OpenIntent { Serve, Hold }
/// Opaque owner of the walk's directory handles (root excluded); its only operation is drop.
pub struct ResourceChain { /* private: Box<dyn Send> */ }
impl ResourceChain { pub fn new(chain: impl Send + 'static) -> Self; }
pub struct OpenedResource { pub file: std::fs::File, pub size: u64, pub chain: ResourceChain }
pub enum ResourceRefusal { Absent, Reparse, NotRegular, CaseSensitive, Other(io::Error) }
pub trait AppResourceSource: Send + Sync {
    fn root(&self) -> &keld_guard::AdmittedRootUrl;
    /// Moves the leaf and its chain to the caller; the source retains no per-request handle.
    fn open(&self, components: &keld_guard::RootRelativeComponents, intent: OpenIntent)
        -> Result<OpenedResource, ResourceRefusal>;
}
pub struct WebviewSpec { pub title: String, pub size: LogicalSize, pub initial: NavTarget,
                         pub app_resources: Option<Arc<dyn AppResourceSource>> }
```

**Ownership and lifecycle (r12, rpipaliya 4423642597).** `open` moves the leaf `File` and its
`ResourceChain` to the caller. The source keeps no per-request handle (it holds only the session
root, §4.3), no map and no document identity, so it needs no release operation. The one lifecycle owner is keld-wv's STA hold table. For `Hold` it
stores the whole `OpenedResource` under a generation key and drops it, chain included, when that
generation retires (below) or the webview closes (A23); the hold is thus the exact verification leaf and
directory chain. WRR carries no frame id (§4.5), and the STA order of a new document's first
subresource WRR against its `ContentLoading` is not documented (Q24: measured 0 inversions in 25,440, not guaranteed), so the key fails safe. Both
the key and §4.7 step 1's transition gate read **one owner state**, the STA's **open-navigation
record** (§4.4's `PendingTopLevel` in state `Started`: an admitted top-level cross-document
navigation between its uncancelled `NavigationStarting` and either its `ContentLoading`, which
moves it to `Committed`, or its `NavigationCompleted`, which drops it). A hold
taken while that record is empty is keyed to the current generation. A hold taken while it is
open is keyed to the **next** generation; if the navigation ends without a G-change
(`NavigationCompleted` without `ContentLoading`: fragment-only, failed or cancelled), it is
rekeyed to the current one. **Retirement (r13):** a generation's holds are released at the first
main-frame commit (`ContentLoading`) after that generation stopped being current — the turn in
which the engine actually replaces its document — or at webview close; a G-change that commits
nothing releases nothing. #1192's +1 at a Host attempt's issue (L110) is such a G-change: the G₁
document stays in the engine, and a G₁ hold taken just before the issue may not yet have been
opened by Chromium, so releasing it in the issue turn would reopen the verify → open gap (AC26
vii). A failed or cancelled attempt (L112) commits nothing either; the surviving engine document
keeps its holds until a later commit replaces it or the webview closes. A late G₁ request is then
released one document later (safe), and a G₂ request is never released at the G₁→G₂ turn, before
Chromium opens it (AC26 vi). For `Serve` the STA drops the
`ResourceChain` as soon as `open` returns and hands only `file` to the worker, so a served request
pins no directory beyond its read (the root stays pinned, A20); under `keld dev` (`hsa`) every
request is `Serve`, and so are a request at the 256-hold bound (the count is known before
`open`) and one with a `CaseVariant` root spelling. keld-core's implementation maps `Serve` to `Share::Served` and `Hold` to
`Share::Pinned`, and wraps keld-update's `DirChain` with `ResourceChain::new`. **Rejected:** a
source-owned map keyed by request or document (the source would need document identity and a
release call across the crate boundary, a second lifecycle owner beside the STA hold table);
returning the chain as `Vec<std::fs::File>` (it would expose handles keld-wv must never use).
AC29 exercises both intents and the chain's retirement through this API.

Gates: **public API** (`open_regular_beneath`, `OpenedRegularFile`, `DirChain`, `OpenRefusal`,
`Share`, `DirCase`, `directory_case`, `tree()`, `WebEngine`/`WebviewSpec`, `AppResourceSource`,
`OpenIntent`, `ResourceChain`); **permission model** (session-
retained root handle; KEL-254 AC7/AC8 extension; case identity, §4.4). **unsafe:** one read in
`windows_fs.rs` (`directory_case`, §8); the opener itself adds none.

### 4.4 URL → root-relative components, the refusal contract, and document correlation

**One parser, one owner (decision 1).** In `keld-guard`:

```rust
pub struct AdmittedRootUrl { drive: u8, components: Vec<String> } // validate_fs_component per component; spelled from the handle's final long path
pub struct RootRelativeComponents(Vec<String>);                    // 1..=N; each validate_windows_package_component + NFC
pub enum FileUrlRefusal { NotFileScheme, Authority, NoDrive, DevicePrefix, ForbiddenEscape,
                          DecodedSeparator, DotSegment, OutsideRoot, Component(String), TooLong,
                          ShellLink }
pub enum RootSpelling { Exact, CaseVariant }                       // r17: lock 2, Case identity below
pub struct ParsedFileUrl { pub components: RootRelativeComponents, pub raw_query: Option<String>,
                           pub root_spelling: RootSpelling }
pub fn file_url_root_relative(url: &str, root: &AdmittedRootUrl) -> Result<ParsedFileUrl, FileUrlRefusal>;
```

It is pure except for the two already-sanctioned NLS calls relocated from
`validate_windows_package_paths` (R7, R8 NFC); R1–R6 and R9 are tested on every OS, R7/R8
goldens run on Windows. The root's own spelling is **Host-admitted**, never page-controlled:
it is derived once from the retained handle's final long path (`GetFinalPathNameByHandle`
`FILE_NAME_NORMALIZED`, as the existing `normalized_guid_path` in `windows_fs.rs:151` does)
and spelled as Bun's `__dirname` spells it, so a project under `C:\Users\John Doe\` or a
`RUNNER~1` profile boots.

**Case identity: two independent locks (r17, review 4429132878; A24, AC30).** NTFS case
sensitivity is per directory, and the directory that decides a name is that name's parent:
Microsoft: "you can assign an attribute flag to make a directory case sensitive" (Learn
`wsl/case-sensitivity`, git `7b28cc1e`), and new directories "will inherit the case sensitivity
from its parent directory" (same page, which also warns that directories created by Windows
applications are not case sensitive and keeps a WSL 1 exception; the 2018 blog says "not
inherited"; AC30's fixture reads the flag either way). So, on Microsoft's text and unmeasured on any device here (E16 below), the app root can be case-insensitive while a parent is
case-sensitive and holds `MyApp` beside `myapp`. R7 compares ignoring case, the cap-std walk
stays beneath the pinned `MyApp`, and Chromium reopens the raw spelling, which on that parent
names the sibling. Whether Chromium actually opens it is unmeasured; AC30 records it either way.

- **Lock 1, admission.** Boot admission reads every directory from the root up to and including
  the volume root. Each is opened read-attributes, no-follow, spelled from the final path. An
  ancestor that is a reparse point refuses typed: INFERRED, a final path has no junction or symlink
  left, but a cloud, dedup or WOF tag or a folder mount point can remain (§4.3's consequence).
  `directory_case` then reads the volume attributes (`GetVolumeInformationByHandleW`, already in
  `windows_fs.rs`). If the volume does not report `FILE_CASE_SENSITIVE_SEARCH` ("The file system
  supports case-sensitive file names", `FILE_FS_ATTRIBUTE_INFORMATION`, git `d7a9f896`) it is
  INFERRED to hold no names that differ only by case, so there is no flag to read. If it does, the volume could
  support the flag and `GetFileInformationByHandleEx(FileCaseSensitiveInfo)` must succeed with
  `FILE_CS_FLAG_CASE_SENSITIVE_DIR` clear (0x1, `FILE_CASE_SENSITIVE_INFORMATION`, git
  `5ea3c2eb`). A set flag or a failed read on such a volume refuses typed, naming the directory.
  Microsoft: "The case sensitivity attribute can only be set on directories within an
  NTFS-formatted file system." A non-NTFS volume that reports the bit therefore either answers
  the read or is refused; a ReFS Dev Drive's answer is UNKNOWN (no such volume on the operator
  device). Microsoft's `GetFileInformationByHandleEx` class table stops at `0x14` and omits this
  class, while the enumeration lists it; the read is therefore device evidence, not a documented
  row. E16 (not elevated, n = 1): five NTFS paths, four directories and one file, each opened with
  `FILE_READ_ATTRIBUTES` and with zero access; 10 of 10 reads returned 0, with `0x3e72eff` volume
  flags (bit 0 set). That shows the read is callable and the default on paths nobody flagged. It
  never read a 1, and its WSL-share paths were malformed (one leading backslash, error 3), so it
  does not test a UNC path. A read of a set flag is AC30's cell.
- **Can the flag change while the root is pinned?** Microsoft: "A directory must be empty in order
  to change the case sensitivity flag attribute on that directory", and "You cannot change the
  case sensitivity setting on a directory that contains other files or directories." Every
  ancestor contains the next component, the root contains the application, and the pin (A20)
  keeps them there, so by that rule neither can change during a session. The page also says "You
  cannot disable the case sensitivity flag on a directory containing folders/files whose names
  differ on only by case", which suggests disabling may work on some non-empty directories;
  disabling cannot create a sibling and does not help the attacker. Device status **UNKNOWN**:
  Microsoft requires "elevated permissions". E16's unelevated `SetFileInformationByHandle`
  (class 23, value 1) failed with 5 on an empty directory and on a non-empty one, with the flag
  reading 0 before and after, so that 5 is **not** proof of the must-be-empty rule;
  `fsutil setCaseSensitiveInfo enable` exited 0 while printing a failure, its code truncated; WSL
  `/mnt/d` xattr get and set failed with errno 95. **E16's limits:** the flag never read back as
  1, so inheritance, a case-colliding sibling, a non-empty clear and open-handle behaviour are
  unmeasured, and so are elevated and Developer-Mode behaviour; the Windows build is not recorded;
  its SHA256SUMS were generated at packing time, so they prove the copy, not the original run.
  Every claim here that rests on a flag value of 1 stays UNKNOWN until AC30's elevated fixture
  runs; until then Microsoft's text is its only source. Containment does not depend on the
  answer, because of lock 2.
- **Lock 2, per request (holds even if lock 1 races).** R7 decides membership only and is never
  the reason for a reopen. The parser also reports `root_spelling`: `Exact` when the URL's drive
  letter equals the admitted one ASCII-case-insensitively and every leading root component equals
  the admitted spelling in UTF-16 code units, with no NLS call; otherwise `CaseVariant`. The drive
  letter is exempt because Chromium upper-cases it before the URL exists ("Normalize Windows drive
  letters to uppercase", `url/url_canon_fileurl.cc` L77 at `chromium/chromium` tag `156.0.8078.49`, the Chromium of Electron
  v45.1.0's `DEPS`; the runtime's own build is not public, so INFERRED for 156.0.4314.50; AC01) and
  because it is one DOS-device name (INFERRED; one local `QueryDosDeviceW` read, outside E16 and
  unpublished, returned one target for `C:` and `c:`). Pass-through requires `Exact`. A `CaseVariant` request is **Host-served** from the
  pinned handle in every context, so no reopen happens, and it never falls back to a hold (§4.5).
- **Cost.** A case variant costs ~6.1–6.5 ms per served MB (E8) where a hold would cost none, so a
  classic bundle requested with a case-variant root pays E8's serving cost and meets §4.5's
  limits. A document loaded by the
  Host carries the admitted spelling, its relative URLs inherit it and the Bun role's `__dirname`
  has it (§4.3), so ordinary applications pay nothing; only code that builds absolute URLs with
  another root case pays. Lock 1's cost is availability: a project in a case-sensitive tree
  (common for WSL-created trees) is refused with the directory named; the fix text says to clear
  the flag or move the project.


The parser returns `ParsedFileUrl`: the query is the
**raw serialized query of the WebView2-reported URI** (not decoded, not normalized), kept
only for the §4.6 allow match and attribution and compared **ordinal byte-exact** there; it
is never identity (W1 AC20). Refusal table (first match decides; query and fragment are cut before R3 and never form
identity — W1 AC20):

| Rule | Input | Result | Why / evidence |
| --- | --- | --- | --- |
| R1 | scheme ≠ `file` (ASCII case-insensitive) | `NotFileScheme` → not W3 | never raised by the `file:*` filter anyway (§4.5) |
| R2 | any non-empty authority | `Authority` → 403 | Chromium strips `localhost` and maps other hosts to UNC (Chromium source, `filename_util.cc`, read for the owner's D-report; no probe cell), so the `localhost` spelling is unreachable at WRR (NIT accepted) and other hosts arrive as UNC forms |
| R3 | path not of the form `/<L>:/…`; or `//?/`, `//./`, `\\` forms | `NoDrive` / `DevicePrefix` → 403 | code-decided; the probe's `url-long` cell threw in the page before WRR, so it is **not** evidence for R3 |
| R4 | `%2F`, `%5C`, `%00` before decoding; decoded bytes not valid UTF-8 | `ForbiddenEscape` → 403 | Chromium rejects the same three (Chromium source, `filename_util.cc`, D-report); one decode only; probe `url-backslash` was decided by the probe's own decode, not this rule |
| R5 | a decoded segment contains `/`, `\` or NUL | `DecodedSeparator` → 403 | separators that survived R4; `:` is **R8's alone** (single-fault NC) |
| R6 | a segment is `.` or `..`; **interior empty segments are collapsed** (`a//b` ≡ `a/b`, as Chromium keeps and Electron serves); a trailing empty segment (directory URL) → 403 | `DotSegment` → 403 / collapsed | Chromium canonicalises `%2e%2e` before WRR, so this rule is exercised by goldens, not by the probe |
| R7 | drive or leading components ≠ the admitted root's under `CompareStringOrdinal` ignore-case | `OutsideRoot` → 403 | Windows' own rule on case-insensitive directories; membership only, never the reason for a reopen (lock 2) |
| R8 | a remaining component fails `validate_windows_package_component` (`:`, `<>"\|?*`, control chars, trailing `.`/space, reserved device basenames, `~`) or NFC | `Component(name)` → 403 | the only Host rule for `:` (ADS) and `~` (8.3): NTFS resolves both at the NT layer |
| R9 | relative path > 100 bytes | `TooLong` → 403 | same bound as the package namespace owner |
| R10 | the final component's extension is `.lnk` (ordinal ignore-case, after NFC) | `ShellLink` → 403 | r7, scored in §4.5: Chromium's loader follows a shell link as a redirect (E9), the Host would serve it raw (E9/E10); one refusal in every context removes the browser-side shell-link parse and makes all consumers (W3 serving, W1 Source, in-root navigation) agree |

**Host-normalized vs Chromium-reopen differentials (r17; every spelling the Host accepts).** A
refusal ends the request, so only an accepted spelling can reach a reopen, and only on
pass-through; a Host-served response is read from the verified handle.

| Spelling | What the Host does | Who reopens the raw URL | What closes the gap | Evidence |
| --- | --- | --- | --- | --- |
| Root prefix, case (R7's fold, NTFS's `$UpCase` fold, Q17) | membership only; `CaseVariant` | nobody | lock 2 Host-serves it; lock 1 refuses the fixture independently | AC30 |
| Drive letter, case | R7; stays `Exact` | Chromium, on pass-through | Chromium upper-cases it; one DOS-device name; a real-letter shadow is residual 2 (§4.5) | AC03, AC05, AC27 |
| Components beneath the root, case | no normalization: the walk opens the request's own spelling | Chromium, the same spelling, beneath the held chain | both resolvers receive identical code units; the walk refuses a case-sensitive directory, so `components_equal` names one object | AC03, AC30 (v), Q17 |
| 8.3 with `~` | R8 refuses a component; R7 refuses a short-name root | nobody | refusal | AC05 |
| 8.3 alias without `~` | **accepted** by R8, which refuses only `~`. INFERRED that `SetFileShortName` can set such a name (Microsoft names only a unique, valid short name, never `~`) and that it resolves at the NT layer | Chromium, the same spelling | same object for both resolvers, so no containment differential. Residual: a second identity for one object. Planting needs `DELETE` on the file ("The file must be opened with **DELETE**", and "Prior to Windows 10, Version 1903, the **SE_RESTORE_NAME** privilege was required", git `194b2748`, so no privilege on a current build, INFERRED from that wording): a dev-stage same-user process, not a role on an installed tree (KEL-254) | not exercised, Q12 |
| Trailing dot or space | R8 refuses; R7 refuses a root spelled so | nobody | refusal | AC05 |
| Percent-encoding | one decode; R4 refuses `%2F`, `%5C`, `%00`, invalid UTF-8; R5, R6 | Chromium, one decode | E11 `pt` 0 differentials, 48 at-risk per context; NC (1a) | AC03 |
| Interior `//` | collapsed (R6) | Chromium; the Win32 path layer collapses it | same object; corpus cells | AC03 |
| Unicode | never normalized: non-NFC refused (R8); lock 2 compares code units | Chromium, identical code units | refusal or identical spelling; a fold difference at the root is lock 2's | AC05, Q17 |
| ADS (`:`) | R8 refuses; R7 refuses a root spelled so | nobody | refusal | AC05 |
| Device names, `COM¹` included | R8 refuses (`lib.rs:1174`) | nobody | refusal | AC05 |
| Query, fragment | cut before R3; never identity | Chromium ignores both for the open (INFERRED, E11 corpus) | not part of the file name | AC03 |
| `.lnk` | R10 refuses, any case | nobody | refusal; Chromium's shortcut redirect (`content/browser/loader/file_url_loader_factory.cc` L557–558 at `chromium/chromium` tag `156.0.8078.49`) never runs | AC03 |
| Directory | R6 refuses a trailing `/`; the walk refuses the rest (`NotRegular`, 403) | nobody | refusal; Chromium's directory `301` (same file, L507–522), its other `file:` redirect, never runs | AC09 (ii) |
| Authority, UNC, device prefix | R2, R3 refuse | nobody | refusal | AC03 |

Divergence from Electron, stated plainly: Electron serves any readable `file:`; W3 serves
only the admitted root, refuses `~` spellings and paths over 100 bytes (pre-existing package
grammar), refuses every reparse point, every `.lnk` (R10) and every case-sensitive directory,
applies the Host-served limits of §4.5, and leaves `file:` workers unconstructible (A22). T1
records these.

**Refusal and response contract (A7, A23).** For a `file:` URI the handler always decides,
and the decision depends on `ResourceContext` and on the request's `Origin` header (AC24): a
**document** (1), a **`fetch`/XHR** (7, 8), a **CORS-mode** request (`Origin` present) or any
request with a `CaseVariant` root spelling that the parser and opener accept is answered `200` with the bytes of the verified handle and Host
headers (`Content-Type` from one `keld-wv` table — apps override it through S1;
`Content-Length` is Host-owned; the baseline CSP on documents only; no
`Access-Control-Allow-Origin` is needed, E11); any **other** request that the parser and opener
accept gets **no response** while its verification handle is held (A23), and Chromium's file
loader reads the held object (its `Content-Type` is Chromium's own extension mapping, E9);
`404` empty when the parser accepted and the opener reported absent; `403` empty for every
parser refusal and every other opener refusal, whatever the request; capacity outcomes are
`413` and `503`, empty (§4.5). No body on refusal, no
`Location`, no `Range` for Host-served responses until AC23 settles whether media range
requests arrive under `file://`. The Host table is a second extension → type map beside
Chromium's; its drift guard is one T2 device test that compares, for every extension in the
table, the Host's `Content-Type` with the pass-through `Content-Type` of the same file (a
difference is either a recorded divergence or a bug). An extension outside the table gets **no**
`Content-Type` header, so a Host-served classic load meets what pass-through gives it where it can
(Chromium's loader maps the extension, else sniffs, same file L727–730; whether it sniffs a WRR
response is UNKNOWN), and the guard adds one unknown-extension file per kind.

**Document correlation (A8, L2).** The record is kept for **every** top-level navigation the
policy allowed, whatever its scheme, so the eval gate (A12) sees all of them:

```rust
struct PendingTopLevel { navigation: u64, uri: ParsedTarget /* file: parsed; other schemes as text */,
                         kind: NavigationKind, served: Option<ServedDocument>,
                         state: NavState /* Started | Committed */ }
struct ServedDocument { components: RootRelativeComponents, status: u16, bytes: u64 }
```

- `NavigationStarting` (top-level, allowed): push in state `Started`; at `MAX_PENDING_TOP_LEVEL`
  page-initiated records (§4.11) a page-initiated navigation is cancelled at +0 instead, so the
  record count cannot grow with a page's navigation rate. A Host attempt's event never meets this
  cap: its +1 has already happened, and `MAX_PENDING_HOST_ATTEMPTS` bounds it. A `Started` record is the
  **open-navigation record** that §4.3's hold key and §4.7 step 1's transition gate read — one
  owner state (r13e).
- WRR `ResourceContext == DOCUMENT` whose **parsed** URI equals a pending `file:` record's
  components: serve it (every admitted DOCUMENT request is served, frames included), but
  **attach** it to `n` only on navigation-specific proof (r13, 0monish 4425916757): in
  header-proven mode (§4.6 rule 1 (i)) the request's headers carry `n`'s attempt nonce
  (Q22 (b)) and `served` is `None`. URI equality, response order and file identity are never
  correlation proof.
- `ContentLoading(n)`: the record moves to `Committed`; Host-served iff `served.status == 200 && !IsErrorPage`; the identity
  reported to W1/#1192 is `(root, components)` with the bytes as currently present in the
  root (owner Q2; W1 states it at `9bc56726` L593–600, §10 B7 ii); the attach compares components with
  the exported component equality. Dropped at `NavigationCompleted(n)`.
- A frame document never attaches without that proof, **whatever its URI**: a same-URI
  `<iframe>` is a distinct document (AC06 c, d).

**No other association exists on the supported SDK (FACT).** The WRR args expose `Request`,
`Response`, `GetDeferral` and `ResourceContext` (`ICoreWebView2WebResourceRequestedEventArgs`,
`webview2-com-sys-0.38.2/src/bindings.rs` L37870–37888) and `RequestedSourceKind` (`…Args2`,
L38050) — no frame and no navigation identity; Microsoft's reference lists the same members
(`icorewebview2webresourcerequestedeventargs`, git `38bc5e98`). `RequestedSourceKind` names the
filter source kind (document vs shared/service worker), not the frame (E9: `DOCUMENT` for every
request). No `ICoreWebView2Frame` through `ICoreWebView2Frame7` has a `WebResourceRequested`
event (L18845–20056: name, host objects, destruction, navigation and content events,
`ExecuteScript`, web messages, permissions, shared buffers, `FrameId`, screen capture,
`FrameCreated`), and `AddWebResourceRequestedFilterWithRequestSourceKinds` (`ICoreWebView2_22`,
L41563) filters by source kind only. Whether a newer SDK adds a frame-scoped request event is
UNKNOWN. **Page-initiated navigations have no proof (B19).** A page navigation carries no Host
nonce, so its DOCUMENT request cannot be told from a same-URI frame request. Until a mechanism
passes its device cells (Q27 candidates), §4.6 rule 2 treats every page-initiated `file:`
target as off-allowlist (cancel, +0): an admitted but uncreditable document would reach W1
FailedClosed and `onLoss`, which rule 5 forbids W3 to cause.

INFERRED: the top-level DOCUMENT request for `n` arrives between `NavigationStarting(n)`
and `ContentLoading(n)` on the STA; if it arrives later, `served` is still `None` at
`ContentLoading(n)` and `n` is not credited (fail-closed in either order). Probe P2 (§10 Q7)
counts DOCUMENT WRRs per pending window over 30 loads with the different-URI iframe racing an
absent top-level file and records `IsErrorPage` for the Host `404`; the association-only
mutant (d′) and the DOCUMENT-only double fault (c′) must fail. Counters are the association
audit (a `profile-test-hooks` read of `PendingTopLevel(n).served`), the page oracle and, only
where a real non-error `ContentLoading(n)` occurs, W1's epoch holder (AC06).

### 4.5 Serving contract (hso), filter scope, threading, and app-declared headers

**Filter scope (A2, A17).** Per webview, **in the one controller build body** (W2's factored
controller build, gh1188 `8c5e5d74` D4 and its reuse row, L945 and L967) **before that webview's first `Navigate`** — `navigate_initial` for a
schema-1 boot, the first `loadURL` for a compat window; the same anchor holds for the WRR
handler, `NavigationStarting`, the context tracking and the self-test:
`AddWebResourceRequestedFilterWithRequestSourceKinds(L"file:*", CONTEXT_ALL, DOCUMENT)`.
The filter is matched against the normalized URI with the scheme lowercased (L1307), so
`FILE:`, UNC and host spellings all match; `https:`, `data:` and `blob:` never raise the
event and never touch the UI thread. **All source kinds are covered**: the worker kinds
(`SHARED_WORKER | SERVICE_WORKER`) for `file:*` are registered **once per environment** on the
first live webview (L63: those requests are raised for every CoreWebView2 with a matching
filter, so one registration avoids duplicate handling) and re-registered on the next live
webview when that one closes (AC23 cell). The "registered" mark is set only after the call
succeeds, and a failure is a typed refusal of that webview (its creation, or a typed close of a
live one), never a silent loss of worker coverage. E11: a `data:` SharedWorker's `importScripts` raised
WRR only with the worker kinds registered. Never `*`.

**Serving (hso; A7, A23; owner decision relayed 2026-10-30, to be recorded on #1185 with B1).** Every `file:` request is
verified on the STA by the one parser (§4.4) and the one no-follow walk (§4.3) beneath the
pinned root (A20). Then:

- **Host-served** from the verified handle with Host headers (threading below; baseline CSP on
  documents only): `ResourceContext` 1 (top-level, frame, `<object>`, `<embed>` documents;
  WRR carries no frame id; §4.4 credits a top-level navigation only on navigation-specific proof), 7/8 (XHR
  and `fetch`; 156 reports `fetch` as 7), and **every request whose WRR headers carry
  `Origin`** (CORS mode: on 156 the value is `null`; no `Sec-Fetch-*` header is visible at WRR).
  E11 (`cm`, 2 runs each): passed through, every CORS-mode load fails — module scripts in the
  same and another directory, static `import`, `import()`, `modulepreload`, `<script
  crossorigin>`, `<img crossorigin>`, `<link rel=stylesheet crossorigin>`, `preload as=font
  crossorigin`; Host-served, all of them load and execute, without
  `Access-Control-Allow-Origin`. `@font-face` (inline, external, `FontFace` API) and plain
  script/image requests carry no `Origin` and load on pass-through. **A `CaseVariant` root
  spelling is Host-served in every context** (lock 2, §4.4).
- **Verified and held** (pass-through): every other request with an `Exact` root spelling
  (classic script 6, stylesheet 2, image 3, font, media, other 16). The verification walk's leaf open is `Share::Pinned`
  (`READ` only) and every directory component's handle comes from cap-std without `DELETE`;
  the Host **keeps exactly those handles** as the hold, sets no response, and Chromium's loader
  (`GENERIC_READ`, share `READ | WRITE`, compatible with the hold — E9 share-mode table) opens
  the pinned object. **Release** (r10, `hsoz`; proposed, replacing r9's dynamic handler after
E13/E14), on the STA:
  1. **Document retirement or webview close releases every hold of that document** (retirement is
     the main-frame commit that replaces it, §4.3 — never #1192's issue-time +1, r13), and nothing
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
     **Host-served** from the verified handle, under the memory budget below, instead of passed
     through. E14: a document issuing
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
matching variable. **Enumeration (r16, review 4427992971).** Before **each** environment
creation it enumerates the process environment with `std::env::vars_os()` (Rust std: "Returns
an iterator of (variable, value) pairs of OS strings, for all the environment variables of the
current process"; `std::env::var_os(key)` fetches one named variable and cannot find a name it
is not given) and compares **every** variable name against the ASCII prefix `WEBVIEW2_`
**case-insensitively**: Windows environment variable names are case-insensitive (Microsoft
`windows-commands/cmd`, git `91fa37e1`: "Variable names aren't case-sensitive."; the
environment block's sort "is case-insensitive", `procthread/changing-environment-variables`,
git `3aee48ac`), so a case-sensitive `starts_with` misses an inherited
`webview2_additional_browser_arguments` (on the operator device `GetEnvironmentVariableW`
returned a lowercase-set variable for its uppercase name, PowerShell 5.1 probe, 2026-10-30,
n = 1; whether the loader reads it that way is the guard-off cell of AC28 (iv′)). A match
refuses whatever its suffix (the whole prefix — Q21 found names of unknown effect, so fail
closed), and whether its value is empty or not Unicode: the decision reads the name only, as
its encoded bytes (`OsStr::as_encoded_bytes`), so a non-Unicode name that begins with the
prefix matches too. Any classification failure refuses. **Enumeration failure:** `vars_os()`
returns no error value; Rust's Windows implementation panics when `GetEnvironmentStringsW`
returns null (std 1.99.1 `library/std/src/sys/env/windows.rs` L76–79), so that branch creates
no environment but is not a typed refusal and cannot be fault-injected on device — **rule not
exercised**; a typed refusal there would need a direct `GetEnvironmentStringsW` call (new
`unsafe`), not adopted. Non-ASCII spellings are outside the ASCII rule: on the same probe
`GetEnvironmentVariableW` did not return a variable spelled with `ı` (U+0131) or `İ` (U+0130)
for its ASCII `WEBVIEW2_…` name (n = 1; INFERRED not folded). E11 `env-af-refuseenv` closes
the uppercase escape; AC28 (iv′) adds the lowercase and mixed-case cells; (2) **every Keld
launcher of `keld-host` removes them** from the child's environment: it enumerates with the
same `vars_os()` pass and the same case-insensitive comparison and calls `Command::env_remove`
(safe) for each name found, never for a hard-coded uppercase list (Rust std's Windows
`EnvKey` compares keys with `CompareStringOrdinal` ignore-case,
`library/std/src/sys/process/windows.rs` L74–90, so the removal matches the spelling found).
The existing KEL-27 test `crates/keld-cli/tests/webview2_runtime.rs` (L9–12, L39 at
`457a9c3b`) simulates an absent runtime by setting `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` and
asserts `KELD-WV-008`; whether it still observes that code depends on where T2 places the
refusal relative to `runtime_version()` (`webview2/mod.rs:163`) — UNKNOWN; T2 keeps or changes
that contract with the KEL-27 owner and never exempts a name. Today the only such launcher is `keld-cli dev`
(`dev.rs:363`; `dev.rs:522` starts the cleanup sentinel, which creates no webview); an
installed application is started by the OS, so (1) alone governs it and a user whose
environment carries such a variable gets the typed refusal with fix text; (3) **loader
policies** (proposed r10, from E13's Q21 classification): before creating the environment,
`keld-wv` reads, under HKLM and then HKCU, `Software\Policies\Microsoft\Edge\WebView2\<key>` for
the keys `AdditionalBrowserArguments`, `UserDataFolder`, `BrowserExecutableFolder`,
`ChannelSearchKind`, `ReleaseChannels` (S1), `ReleaseChannelPreference` and `DowngradeVersion`
(S2; their value-name lookup is INFERRED to follow S1's). The loader matches a value named by
the process's AppUserModelID, else its compiled code name (the executable's file name, S4's
`/v WebView2APISample.exe`), else `*` (S1). **Keld resolves its effective AUMID** (r11, 0monish
4423613615: a legal AUMID may itself end in `.exe`, so the suffix proves nothing): the package
AUMID from `GetCurrentApplicationUserModelId` and the explicit one from
`GetCurrentProcessExplicitAppUserModelID` — **both** names are checked when both exist —
read immediately before **each** environment creation on the creating thread (Electron's
`app.setAppUserModelId`, compat snapshot `app.md:1179`, may change it between creations). It enumerates the
value **names** of each key, and a name applies when it equals that AUMID, the executable's
file name or `*` (ordinal ignore-case) — a superset of the loader's priority order, since any
of them present refuses. "None" results: `APPMODEL_ERROR_NO_APPLICATION` (documented) after
the documented `ERROR_INSUFFICIENT_BUFFER` sizing call; for the explicit query, the HRESULT a
B16 cell measures for an unpackaged launch with no explicit ID (undocumented — Microsoft
documents only `S_OK`). Any other result makes **every** value name apply (fail closed) —
an availability cost, since an enterprise `AdditionalBrowserArguments` set for another
application would then refuse Keld; B16 records how often that branch fires. Existence decides, never
content; any registry status other than success or "not found" refuses. `AdditionalBrowserArguments`
(E13: registry only, read by the runtime client DLL) and `UserDataFolder` are refused under
either root: one injects the switch, the other moves the profile Keld owns. The
runtime-selection keys are refused under HKCU — per-user configuration that Microsoft treats
as user-modifiable and ignores for elevated hosts (S3); on the operator device the default
ACL gives the user read-only access to `HKCU\Software\Policies` (observed 2026-10-30), so
writing it needs an elevated token. Under **HKLM** they are honoured only through the
**allowed-writer rule (r13e)**, which applies before environment creation to **every** runtime the
loader selects — the per-machine default, a per-user default and an HKLM-selected one alike. Over
the runtime folder tree (every directory and every file, recursively) and its ancestors up to the
volume root, a write-class right (`FILE_WRITE_DATA`/`FILE_ADD_FILE`,
`FILE_APPEND_DATA`/`FILE_ADD_SUBDIRECTORY`, `FILE_WRITE_ATTRIBUTES`, `FILE_WRITE_EA`, `DELETE`,
`FILE_DELETE_CHILD`, `WRITE_DAC`, `WRITE_OWNER`, and the generic rights that map to them) may be
granted by an allow ACE only to SYSTEM, TrustedInstaller, Administrators or the launching user's
own SID (the same-user residual, kept as in (q0)); every object's owner must be one of them; an
ACE for CREATOR OWNER is accepted only inherit-only (it then applies to a future object's owner,
which is checked when that object exists). Any other SID holding a write-class allow ACE —
another user, `Users`, `Authenticated Users`, `Everyone`, `INTERACTIVE`, an AppContainer or
capability SID (`S-1-15-2-*`, `S-1-15-3-*`, `ALL APPLICATION PACKAGES` among them) — an ACE form
other than plain allow/deny, an absent DACL, or a reparse point anywhere in the tree refuses
environment creation with a typed `KELD-*` naming the object and the SID. **Ancestors (r13g).**
Inside the runtime product layout, every ancestor up to a stated boundary is checked by the same
allowed-writer rule, `FILE_ADD_SUBDIRECTORY` included: for an Evergreen runtime up to and
including the `EdgeWebView` folder, for a fixed-version or policy-selected runtime the selected
folder's parent. Reason: the existing keld-guard ancestor exception (any trustee may hold
`FILE_ADD_SUBDIRECTORY`; `validate_windows_machine_ancestor_directory` →
`validate_anchor_descriptor`, `crates/keld-guard/src/windows_machine.rs` at `457a9c3b`) is safe
for KEL-254 because Keld's own resolver follows only names it admitted, but here names are
resolved by Microsoft's loader and runtime; another user who can add a folder next to the
version folder (for example a higher-version directory) could be picked if the runtime's
selection or update logic enumerates that parent (INFERRED, undocumented; Q28). Above the
boundary (for example the volume root, which commonly grants `Authenticated Users` that right,
INFERRED) the existing exception stays. Planting in an ordinary `PATH` directory is outside
this rule and is not claimed.
**Why this rule — the third option.** r12 checked only the folder's DACL against the launching
user: it proved nothing about the files (NTFS gives each file its own DACL) and checked a
principal the spec does not defend against. r13's first draft refused every policy-selected
runtime: it cost availability and left the default runtime unchecked. r13d honoured every
runtime with a same-user residual: it missed that an HKLM-selected runtime is machine-wide, so a
file writable by another ordinary user or by `Users` lets that user plant code that runs in every
other user's Keld launch — what KEL-254's machine modes exclude ("Both machine modes install
beneath a protected machine root readable/executable, but not writable, by ordinary users",
`docs/specs/kel254-windows-installed-root-boot.md` L134–135; its standard-user probes, L730).
The allowed-writer rule closes other users, broad groups and AppContainer/LPAC tokens for every
runtime alike and keeps only the same-user residual. **TOCTOU (INFERRED):** once only
admin-class principals and the launching user can write an object in the tree or rename an
ancestor, a change between the check and the runtime's loads needs one of those principals —
trusted, or the same-user residual. **Mechanism (reuse):** `keld-guard`'s machine ACL reader
(`windows_machine.rs`: `GetSecurityInfo` through `windows-permissions` 0.2.4, a workspace
dependency `keld-wv` already declares, `Cargo.toml` L226, `crates/keld-wv/Cargo.toml` L75) and
the C3 per-component no-follow walk for enumeration. No new FFI function and no new dependency
are expected; the allowed-writer entrypoint is a `keld-guard` public-API change (§8), and
whether the enumeration needs new `unsafe` is decided by the unsafe gate at T2, not claimed here.
The folder checked must be the folder the loader uses; binding the two (for example passing the
checked folder as `browserExecutableFolder`, whose precedence against a policy value is UNKNOWN)
is Q28. **Cost (unmeasured, T2):** Microsoft: "The Fixed Version binaries are over 250 MB"
(`webview2/concepts/distribution.md`, git `670ae635`); T2 measures a real runtime tree's object
count and the check's cold-launch cost against architecture 01 §5's 5 % rule. Over budget, the
fallback checks only the executable, the DLLs the runtime loads and their directory chain — stated
as **weaker**: a file the runtime opens later stays unchecked. The boot diagnostics record the
selected runtime's version, image path and class (per-machine; per-user, by the HKCU
`EdgeUpdate` client key Microsoft documents; or policy-selected). Mechanism: `RegOpenKeyExW`,
`RegEnumValueW` (names only), `RegCloseKey`, `GetCurrentApplicationUserModelId` and
`GetCurrentProcessExplicitAppUserModelID` + `CoTaskMemFree` through `keld-wv`'s existing
`windows` 0.61 dependency with added features `Win32_System_Registry` and
`Win32_Storage_Packaging_Appx` (`Win32_UI_Shell` and `Win32_System_Com` are already enabled;
no new crate) — **new production `unsafe` operations** that need an issue-scoped `keld-wv` `AGENTS.md` sanction and the unsafe
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
   Electron in this revision — an oracle cell for the X01-T2 recorder, #494/#495, not yet run).
4. **Runtime file integrity (r13e).** The allowed-writer rule (§4.5 rule (3)) admits a selected
   runtime only when its tree and ancestors grant write-class rights to admin-class principals
   and the launching user alone. Residual: the launching user, a same-user medium-IL process and
   a same-user Legacy role can still write a per-user runtime (or any object the user owns in the
   tree) — principals that already act on a same-user Host ((q0)). Other ordinary users, broad
   groups and AppContainer/LPAC tokens cannot, for every runtime alike (B16 (ii) cells 1–5). What
   the ACL rule cannot express — the loader-folder binding, Strict roles that run under the
   launching user's SID, and the weaker cost fallback — is Q28 (B20).
5. **Stream lifetime after `Complete` (r17, Q29).** The reservation ends at `Complete`, but the
   memory stream lives until WebView2 releases it, and Microsoft does not say when ("Stream must
   have all the content data available by the time the `WebResourceRequested` event deferral of
   this response is completed", `icorewebview2webresourceresponse`, git `38bc5e98`). If a page
   can keep response bodies unread, Host memory could exceed the budget until those responses
   die. AC31 (vii) measures it and Q29 gates T2's merge.
6. **One read worker (r17).** The FIFO is bounded and fair but has no per-read deadline. A near-cap
   read delays every Host-served response behind it, and a page can keep queuing such reads up to
   the budget, so the delay is that budget over the disk's rate; AC31 (v) reports the longest wait.
   A read that never returns (a failing disk, or an oplock held by a same-user process, already in
   (q0)'s residual) stalls every Host-served response of the Host, documents included: those
   requests stay unanswered, in-flight reaches its cap and later requests answer `503`. No page
   can cause that stall (INFERRED). A second worker needs an AC20 result; a read deadline needs
   `CancelSynchronousIo` or overlapped reads, new `unsafe`, not adopted.

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
Contract: one response per Host-served request; a deferral completed exactly once on the STA on
every path (a read error, a failed stream creation and a worker
that cannot take the job all answer `403`/empty through the same completion; webview close
answers each entry `403`/empty the same way, or, once no response can be set, drops the deferral
uncompleted, because a completion with no response reaches Chromium's loader (below); a read
already running keeps its reservation until the worker's post arrives, so the ledger never reads
below the live buffers, and close never joins the worker; a failed `Complete` is recorded, never
repeated); every path releases its reservation exactly once; the stream complete before `Complete`; the worker reads
at most the `size` reserved, so a file grown by a concurrent write is cut at that size and
`Content-Length` follows the bytes read (the bytes as currently present). **No error path may
end without a response:** a request with no response reaches Chromium's loader, which is how
pass-through works (E9), so a failed read of the URI, `ResourceContext` or headers answers
`403`/empty, and if the refusal response itself cannot be created or set, the handler keeps the
request's deferral uncompleted and the webview is retired typed (AC31 (vi); a `GetDeferral`
failure on top of that is not injected, UNKNOWN). Served
bytes cost ~6.1–6.5 ms per MB whatever the arm (E8; that the cost sits in WebView2's
response-body path is INFERRED — arm p rules out dispatch and the Host open), so large
documents, `fetch`/XHR payloads and module graphs pay it; classic scripts, stylesheets and images
do not (E11's module workload paid 69.0 ms of STA work per load for 258 requests). The
per-request allocations are the recorded exception in §2.

**Host-served memory budget (A25; Q9 decided r17, review 4429140665).** Arm (b) buffers whole
files, and nothing else bounds them: `keld-pack` admits one canonical ustar member up to 8 GiB − 1
(`validate_v0_members`, `crates/keld-pack/src/lib.rs:88`), the dev stage has no pack bound, and 256
limits holds, not buffers. These constants are W3's and do not wait on a package bound. All four
are **provisional until T2's measurement** (AC31 (v)); the rule below is not.

| Constant (Host-wide, STA-owned) | Value | Why this value |
| --- | --- | --- |
| `MAX_HOST_SERVED_FILE_BYTES` | 64 MiB | 6.8 × the largest measured file (draw.io `app.min.js`, 9,865,843 B, E8); about 0.42 s of serving cost at E8's rate |
| `MAX_HOST_SERVED_RESERVED_BYTES` | 128 MiB | 2 files at the cap; 5.6 × draw.io's whole `keld dev` load (24,119,045 B, E8), 67 × the whole Vite `dist/assets` (1,984,231 B, E13) |
| `MAX_HOST_SERVED_IN_FLIGHT` | 1,024 | about 4 × E11's 258 module requests, 9.8 × the Vite document's 104 Host-served ones; counts every taken deferral, S1 round trips and recovery holds included |
| `HOST_SERVE_READ_WORKERS` | 1 | E8 arm (b)'s measured shape, one FIFO worker (`spawn_worker`); a second needs an AC20 result |

On the STA, after the parser and in this order (`open` runs the walk). A **pass-through
candidate** is a release-mode request of the held class with an `Exact` root spelling.

1. A candidate with a hold free is held: no reservation, any size.
2. Otherwise `open(Serve)` gives `size`. `size > MAX_HOST_SERVED_FILE_BYTES` → `413`, empty.
3. In-flight at its cap, or reserved + `size` above the budget → `503`, empty, no queue.
4. Otherwise reserve `size`, count one in flight, take the deferral and queue the read. The FIFO's
   length is therefore bounded by the in-flight cap.

Bytes are reserved **before** any buffer exists and stay reserved through the read, the queue to
the STA and stream creation, until `Complete` has returned or the error path has finished. The
peak of live response buffers is budget + one file cap, because the stream copy holds one buffer
twice for a moment on the single STA (arm (b)'s shape, INFERRED until AC31 reads it). `413` and
`503` settle the deferral-free request at once, exactly once, and raise one coalesced typed
diagnostic (B5). **No unbounded queue and no streaming** (arm (d) stays rejected).

**At the limit, evaluated.** Step 1 is the ordinary path for classic scripts, stylesheets, images,
fonts and media, so a large file of those classes stays on the verified, held pass-through while
holds remain and costs no Host buffer. Nothing else can use it: a document would lose its Host
headers (§4.9); `fetch`/XHR and CORS-mode loads fail passed through (E10, E11); a `CaseVariant`
request must not be reopened (lock 2); and at 256 holds no hold exists, while an unheld pass-through
raced 24/24 (E11).

**What fails, plainly.** Over the file cap: in every mode a document, a `fetch`/XHR and every
CORS-mode load (module scripts, `import()`, `crossorigin` assets) are refused `413`. In release a
classic script, stylesheet, image, font or media file keeps loading through a hold, and is refused
`413` only when 256 holds are outstanding. **Under `keld dev` (`hsa`) every over-cap file is refused
`413`, including a classic script or image that loads in release**: dev never pins, so it has no
fallback, and this dev/release difference is a T1 divergence. Over the budget or in-flight cap:
`503` for Host-served classes in release and for every request under `keld dev`; it is transient and
a page's retry can succeed, but Chromium does not retry a module import for it (INFERRED). The
measured workloads stay far below these limits; T2 reports their peaks. A workload above half a
constant reopens that value with the owner, not the rule. Electron has no such limits (T1).

**A top-level document refused for capacity.** `413` and `503` are Host refusals: `served.status`
is not `200`, so §4.4 credits nothing, W1 mints no epoch and §4.6 gives one `did-fail-load`, as for
`404`. Unlike `404`, a `503` is transient and a page can influence it, because the budget is
Host-wide. A Host attempt whose nonce-bearing DOCUMENT request is refused for capacity therefore
fails to its caller typed and is never re-issued, and whatever `ContentLoading(n)` reports (Q7) it
MUST NOT consume one of W1's three recoveries; W1 names `413`/`503` in its rows (`gh1184` `e9b95d0c`, B7 xi).

**S1 app-declared headers (A15, A19).** The facade forwards `session.webRequest.
onHeadersReceived(filter, listener)` registration to the Host as a control message that
carries only "listener present"; **the Electron match-pattern matcher lives once, in the
facade**. When a listener is present the Host takes the WRR deferral for every **Host-served response**
(documents, `fetch`/XHR and CORS-mode loads) and asks the role (a pass-through classic script, stylesheet or image has
no Host response that could carry headers — INFERRED from the API surface, untested — so app
headers are **not** applied to them: a recorded divergence from Electron, whose
`onHeadersReceived` sees every response; T1 entry, AC15 e; a request of those classes Host-served
only because of the 256-hold bound or a `CaseVariant` spelling keeps its release class, as under
`hsa`: no round trip, no app headers); the facade applies the app's `filter.urls` and replies
"unchanged" or the listener's headers; no listener → no round trip. Host-owned fields
(`Content-Length`, status line) are not overridable; `Content-Type` and the app's CSP are.
No wall-clock deadline while the role is alive: a slow role delays the page, as Electron's
blocking listener would; role retirement (`RoleGeneration` moved) fails every pending
request closed (`403`/empty) **once the role is known to be gone**; between the
`RoleGeneration` move and the successor's registration (or its explicit "no listener"), and
**only when the predecessor role had a listener**, the Host **holds** Host-served requests on the
STA FIFO behind their deferrals instead of refusing them, bounded by the existing recovery
deadline (when it trips, held requests are answered `403`/empty) and by
`MAX_HOST_SERVED_IN_FLIGHT` (a further request is answered `503`/empty); a held request keeps
its reservation, and a round trip that cannot be sent answers `403`/empty;
with no predecessor listener nothing is held (no round trip existed before the loss). A reply that carries the predecessor's `RoleGeneration` after the move is dropped, because round-trip correlations are keyed on it (§2); its request is held and asked of the successor like any other (AC15 d″). The round trip is **not** one of #540's four CONTROL records (#1192 L145): it
needs its own bounded record type or a dedicated FIFO on the compat-control channel, with
a stated bound and an overflow rule that prefers holding on the STA FIFO over `403`, capped by
`MAX_HOST_SERVED_IN_FLIGHT` — owner decision (§10 B11). **Ordering (A19) — no seal; a TARGET that depends on W2.** The compat
boot model this relies on **does not exist on Windows today**: `app_session.rs:3431-3438`
returns `KELD-CORE-034` "schema 2 is supported on macOS only", `:1340-1351` refuses on
Linux/Windows, `AppReadyTrigger::HostInitialized` is `cfg(macos)` (`:2383` is the macOS
boot; `:4604`, `:4636-4637`), and merged #1286 pins that refusal. W2 (gh1188 `8c5e5d74`, #1280 r18: its T1 Ready without a window behind a debug test admission,
its T2 the D7 release proof and D2's product admission, its T3 the in-loop build D4 with the derived
managed-window flip) plans exactly this: `validate_windows_boot_files` admits `InitialWindow::None`,
`create_app` is skipped for facade boots, `HostInitialized` is widened to Windows and
emitted at the first loop turn, windows are built in-loop from a factored create, and
"registry windows here load an empty document" with file-origin `loadURL` deferred to W3
(gh1188 L54–55; AC "Zero windows at Ready" L200; D2–D4 L965–967). Under that target: a
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
Until W2's T1, T2 and T3 land D2–D4 on Windows, AC01 (ii), AC02 (ii), AC15's listener cases and AC08 (g)/(h) are **blocked** (§10 B13). The Keld baseline CSP is always emitted as its own header on every
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
| `did-navigate(url, httpResponseCode, httpStatusText)` | `ContentLoading(n)` with `IsErrorPage == FALSE` **and no Host refusal (`403`, `404`, `413`, `503`) DOCUMENT record for `n`**; status from the Host's own served record for `file:`; for allowlisted remote documents from `HttpStatusCode` at `NavigationCompleted` (`ICoreWebView2NavigationCompletedEventArgs2`; no `WebResourceResponseReceived` handler, whose registration E12/E13 measured at +7–11 ms per module-heavy load; `httpStatusText` is empty — a T1 divergence, as is the later ordering) | — |
| `did-navigate-in-page(url, isMainFrame=true)` | `SourceChanged(IsNewDocument == FALSE)` + `HistoryChanged` | +0 generation; no `will-navigate` |
| `dom-ready` | `DOMContentLoaded` (top-level) | — |
| `did-finish-load` | `NavigationCompleted(n, IsSuccess == TRUE)` **and no Host refusal (`403`, `404`, `413`, `503`) DOCUMENT record for `n`** (a committed empty error body never finishes "loading" in Electron's sense) | — |
| `did-fail-load(errorCode, errorDescription, validatedURL, isMainFrame=true)` | Exactly one per `NavigationId`, decided at `NavigationCompleted(n)`: a Host refusal DOCUMENT record for `n` **takes precedence** (emitted once, whatever `IsSuccess` says, including `IsSuccess == TRUE` for a committed empty body); otherwise `IsSuccess == FALSE` with `WebErrorStatus ≠ OPERATION_CANCELED` | Host `404` → `-6`; Host `403` → `-10`; Host `413` → `-8` (`ERR_FILE_TOO_BIG`, "The file is too large."); Host `503` → `-12` (`ERR_INSUFFICIENT_RESOURCES`, "There were not enough resources to complete the operation."; both `net/base/net_error_list.h` L52–53, L65–66 at `chromium/chromium` tag `156.0.8078.49`, the Chromium of Electron v45.1.0's `DEPS`); **cancelled navigations emit nothing** (`OPERATION_CANCELED` 14 ≙ `ERR_ABORTED`); remaining `COREWEBVIEW2_WEB_ERROR_STATUS` values → the table T4 lands, verified by #537's recorded cells (AC07) |
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
> window's generated entry resource in W1's window mapping (W1 `9bc56726` L615–617, L728–729;
> entry derivation is #1192's).
> The decision is taken on the original `NavigationStarting`, before any generation change:
> off-allowlist → `Cancel`, generation +0, the live document and its IPC survive;
> on-allowlist → proceed, generation +1 only at commit. `will-navigate` is a notification
> only; `preventDefault()` is not consulted. The Host never replays a page navigation.
> **Host attempts** (the boot `Navigate`, an `onLoss` reload, the application's
> `loadURL`/`reload`) to the window's **entry components are always admissible, whatever the
> query** once rule 1 attributes the event to that attempt by causal proof (never by URI or
> order); any other Host target is checked against the same allowlist **before** its +1 — an
> off-allowlist Host attempt is a typed refusal at +0, never a revoked document. Redirect hops
> of an admitted Host attempt are checked too; a cancelled hop fails that attempt closed
> (#1192 L112), which is a stated divergence, not an exemption.

Rules, applied on the original `NavigationStarting` in this order:

1. **Attribution (r13).** A `NavigationStarting` is Host-initiated only when it is causally
   bound to one Host-issued attempt. URI equality and event order never grant the Host
   exemption, an epoch or, for a `file:` attempt, a settlement (r11 and r13, 0monish 4423613624: a page
   `location.assign(entry?x)` racing a pending Host `loadURL(entry?x)` presents inputs
   identical to the Host's, and r12's provisional first-event attribution let such a page
   document commit and call IPC before any ambiguity was visible). The mode is fixed per
   runtime by a pinned T2 result (Q22), never per event.
   (i) **Header-proven mode.** Every Host `file:` attempt — the boot `Navigate`, the app's
   `loadURL`, an `onLoss` reload-entry (a navigation to the entry URL, never `Reload()`) and the
   app's `reload()` of a `file:` document — is issued with `NavigateWithWebResourceRequest`
   (`ICoreWebView2_2`, `bindings.rs` L41075; `GET`, no body) carrying `X-Keld-Host-Attempt:
   <128-bit nonce>`. Microsoft: "This lets you provide post data or additional request headers
   during navigation. The headers in the WebResourceRequest override headers added by WebView2
   runtime except for Cookie headers. Method can only be either "GET" or "POST"."
   (`icorewebview2_2`, git `38bc5e98`); it says nothing about `file:` (UNKNOWN, Q22). The Host
   attributes only the `NavigationStarting` whose `RequestHeaders` ("The HTTP request headers
   for the navigation", `icorewebview2navigationstartingeventargs`) carry that nonce, single
   use; page script cannot add a header to a top-level navigation (INFERRED: no web API sets
   request headers on a navigation; Q22 (a) measures the page side). A redirect hop keeps the
   attributed `NavigationId` ("`IsRedirected` while an attempt is pending" is not used). An
   event without the nonce is page-initiated even while an attempt is pending — never a
   fallback to URI attribution. An attempt whose nonce has not appeared when another top-level
   navigation commits, or within `HOST_ATTEMPT_START_BOUND`, fails to its caller with a
   typed `KELD-*` and is never re-issued; #1192's attempt-failure row applies (L112). **T2 owns
   (r17)** the nonce, this deadline and this failure terminal; T4 adds callers, not the deadline.
   The bound is `keld_wv::INITIAL_MILESTONE_DEADLINE` (15 s, `crates/keld-wv/src/lib.rs:47`), a
   Host deadline on the STA event loop like the initial-navigation deadline
   (`webview2/mod.rs:3240`); for the schema-1 boot attempt the typed failure is `KELD-WV-005`. A
   nonce-bearing event after its attempt failed is expired: rule 2 decides it as page-initiated.
   **Issue order (r17):** (1) rule 4's pre-check (T4; T2's boot attempt targets the entry);
   (2) create the `ICoreWebView2WebResourceRequest` and its header — a failure is a typed refusal
   at +0; (3) register the nonce and arm the deadline, with `MAX_PENDING_HOST_ATTEMPTS` = 1
   unattributed attempt per webview, a further attempt being a typed refusal at +0 (a T1
   divergence: Electron's later `loadURL` supersedes); (4) #1192's +1; (5) issue the navigation —
   a failed call is an attempt failure (L112), since the +1 already revoked. `http(s)` attempts never carry the header (no Keld
   header reaches a server or a redirect); they never need the exemption, because rule 4 checks
   every non-entry Host target against the same allowlist and remote documents receive no IPC
   epoch (W1), so their settlement may keep r12's URI correlation with no authority depending
   on it. **Reload (r13, 0monish 4425565118).** `ICoreWebView2::Reload` takes no request
   (`bindings.rs` L1881), so it cannot carry the nonce; header-proven mode issues the app's
   `reload()` of a `file:` document as the header navigation to the committed URI — one
   mechanism for every `file:` attempt. Its `NavigationKind` (`NEW_DOCUMENT` or `RELOAD`),
   `history.length` and scroll/form restoration are UNKNOWN until Q22 (c) records them (`file:`
   loads have no HTTP cache to revalidate, INFERRED). Until that cell passes, `reload()` of a
   `file:` document is a typed refusal (a T1 divergence); `loadURL` and `onLoss` reload-entry
   are navigations, not reloads, and are unaffected.
   (ii) **Unproven mode** (Q22 has not passed on this runtime): **no** event is attributed to a
   Host `file:` attempt; every top-level `NavigationStarting` is decided by rule 2 as
   page-initiated. Because rule 2 treats every page-initiated `file:` target as off-allowlist
   until B19 resolves, **every** Host `file:` attempt — the schema-1 boot `Navigate`, `loadURL`
   and `onLoss` reload-entry alike, under any allowlist — is refused to its caller with a typed
   `KELD-*` at +0, **before** #1192's +1, so a live document and its IPC survive; and even after
   Q27 this mode credits no epoch (§4.4: no navigation-specific proof exists for an un-nonced
   request). Unproven mode is a fail-closed state, not a degraded way to run. Candidate causal
   paths for it — a webview that has never committed a document, and quiesce-then-load through a
   Host-authored `NavigateToString` document — rest on unmeasured premises and are not adopted
   (Q26). **Availability (B18):** W3 delivers no Windows `file:` application in this mode (§1).
   **Residual (stated):** a page whose allowlist admits a navigation can supersede a pending
   Host attempt, as in Electron; that attempt fails typed. Attribution failures **never count
   toward W1's three-recovery cap**. `IsUserInitiated` is never consulted for attribution:
   Microsoft documents that "Navigations initiated via WebView2 APIs are considered as user
   initiated" (`icorewebview2navigationstartingeventargs.md`, `get_IsUserInitiated` remarks),
   so it cannot separate a Host `Navigate` from a user gesture. `javascript:` and `about:` URIs
   are page-initiated and off-allowlist by construction.
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
   (W1 AC10). On-allowlist → proceed; generation +1 only at commit. Until B19 resolves (Q27),
   every page-initiated `file:` target is off-allowlist whatever the form (§4.4: its document
   could not be credited Host-served). **Selection (r17, W1 r13 `e9b95d0c`):** "the window's" allowlist is
   `windows.<w>.web.navigation.allow`, `<w>` being the Host's immutable window binding for that view
   (#1191 AC3; W1 §4.10). keld-core resolves it once at view creation and passes keld-wv one
   immutable per-view policy value; keld-wv reads no manifest, and no URL, title or page input
   selects it. A view with no bound policy matches nothing (fail closed). The `file:` allow forms
   are a #541 decision (§10 B12): they are matched on parser components, never through
   `path_in_scope`, and exposing a navigation entrypoint of `keld-guard` outside `evaluate`
   (which refuses webview principals) is a `keld-guard` public API change (§8).
3. **Notification.** For a page-initiated `NEW_DOCUMENT` event with `IsRedirected == FALSE`
   the facade emits `will-navigate(url)` when the app has a listener, after
   `did-start-navigation`, whether the Host allowed or cancelled. `event.preventDefault()`
   changes nothing (documented divergence). `BACK_OR_FORWARD`/`RELOAD`: allowlist applies,
   no notification until P3 (§10 Q10) settles whether Electron treats script history
   navigations as renderer-initiated (UNKNOWN).
4. **No replay, ever; Host attempts are checked first; the entry is reachable whenever
   attribution is proven.** The Host never re-issues a navigation, a page's or its own (r13).
   A Host attempt (boot `Navigate`, `onLoss` reload-entry, the app's `loadURL`/`reload`, #459)
   whose target has the window's **entry components is always admissible, whatever its
   query, once rule 1 attributes it causally** (so in header-proven mode `onLoss` recovery and an
   `index.html?x` load can never be refused by a derived allowlist that contains no entry
   form; in unproven mode they are typed refusals, B18); any other Host target is checked against the same allowlist **before** #1192's
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
from #537's recorded order cell (AC07), not from the emit site alone.

### 4.7 Settled `executeJavaScript`

Flow (Windows): Bun `webContents.executeJavaScript(code, userGesture)` → facade waits for
`did-stop-loading` if loading (T4's native producer, §4.6; T3 lands after T4, §6) → control message with `(correlation, role generation)` →
`keld-core` checks the role generation and posts `AppWindowCommand::Eval` → `keld-wv`:

1. **Context binding (mandatory), one clock.** `Runtime.enable` is on from create; the
   Host registers the `executionContextCreated` / `executionContextDestroyed` /
   `executionContextsCleared` receivers **before** calling `Runtime.enable` (the probe
   registered them after, `main.rs` L1090–1119, so the initial event's arrival order is
   UNVERIFIED and nothing below depends on it). **Binding (r13, 0monish 4423613626 re-review: URL
   equality cannot separate two same-URL documents).** No documented datum joins the two
   clocks: `ContentLoading` carries `IsErrorPage` and `NavigationId` only (`bindings.rs` L7325),
   `NavigationStarting` no CDP identifier (L23632), and CDP's `Page.frameNavigated` (`loaderId`,
   `url`) and `Runtime.executionContextCreated` (`uniqueId`, `auxData.isDefault`,
   `auxData.frameId`) no WebView2 `NavigationId`; whether that id equals any Chromium identifier
   is undocumented (UNKNOWN). The Host also enables `Page` and keeps a **CDP slot**, reset only
   by CDP events (a main-frame `frameNavigated(L)` replaces it; `executionContextsCleared` empties
   its candidates): the latest main-frame loader `L`, the STA turn in which it arrived, and the
   default main-frame contexts created after it; and a **WebView2 slot**: the `G`, `Source` and
   `NavigationId` of the latest `ContentLoading`. A G-change clears the bound pair and the
   WebView2 slot, never the CDP slot. `(G, ctx)` binds only when all of these hold:
   (a) **transition gate** — the STA's open-navigation record is empty (§4.3: the one owner state
   the hold key also reads; open from an admitted top-level cross-document
   `NavigationStarting(n)` until `ContentLoading(n)` or `NavigationCompleted(n)`);
   while one is open nothing new binds, and an existing pair stays usable for its own document
   until a member of the G-change set clears it;
   (b) **fresh loader** — `L`'s `frameNavigated` reached the STA **after** the handler of the
   `NavigationStarting(n)` whose `ContentLoading(n)` minted `G` returned; a loader seen earlier is
   never a candidate for `n` (INFERRED causal order: Microsoft documents that a cancelled
   navigation leaves "the content of the current page … intact", so `n` cannot commit before
   that handler decides);
   (c) **uniqueness** — exactly one fresh main-frame loader for `n` and exactly one default
   main-frame context under it;
   (d) **URL check** — `L.url` (CDP excludes the fragment; `unreachableUrl` for an error page) is
   parsed-equal to `Source` without its fragment. URL equality is a consistency check, never
   the correlation.
   A failure of (c) or (d) puts `G` in `TerminalAmbiguous(G)` (phases below); a failure of (a)
   or (b) only delays binding. `Runtime.evaluate` is never dispatched to an unverified pair. Two
   premises are **not** detected by this rule and gate T3 (B17): Q23 (a default context created
   after `frameNavigated(L)` belongs to `L`, including after a process swap) and the **lag
   premise** (no main-frame `frameNavigated` of an earlier commit reaches the STA after a later
   navigation's `NavigationStarting` handler returned). If either is refuted, or unmeasured when
   T3 is ready, binding is withheld and `executeJavaScript` is a typed capability refusal on
   Windows — blocker B17, not a silent fallback. **Unbinding:** `executionContextDestroyed`
   or `executionContextsCleared` for the bound `ctx`, and **every member of the one
   G-change set** — page replacement commit (#1192 L108), the Host load's +1 before the
   engine call (L110), that attempt's commit (L111), an uncorrelated replacement (L114), and
   any further +1 #1192's owner admits for Windows (W1 r9 proposes three, B7 ix) —
   clear the pair; it is complete again only after the next `ContentLoading`/created
   pairing. The same set drives step 4's retirement, so a back/forward-cache restore that
   re-creates `ctx₁` cannot pair with a stale G (the pair was cleared at the commit that
   evicted it; Q14 census). **Phases (r13, 0monish 4425565080; one owner,
   the `keld-wv` STA).** At dispatch the Host reads one per-webview phase:
   - `Bound(G, ctx)`: dispatch with `uniqueContextId: ctx` (steps 2–4).
   - `AwaitingContext(G)`: entered at every G-change and when the bound `ctx` is destroyed or
     cleared, unless an ambiguity rule fired. The Host sends **nothing** to CDP and returns a
     typed `NoDocumentContext(G, s)`, `s` being the phase sequence (below). The **facade**
     converts it into one bounded wait (not a tight re-dispatch: Electron's own wait returns
     immediately when the frame is not loading, `web-contents.ts:44-45`, which would livelock
     here). Settle frames and control records are not ordered against each other (step 4
     rejects a shared FIFO), so a `context-bound` posted after the reply can arrive first. The
     facade therefore keeps the `(G, s)` with the highest `s` among the `context-bound` and `context-refused` events it
     has seen, and the wait ends at once when that `s` is greater than the reply's, otherwise at
     the first such event with a greater `s`: `context-bound` → one re-dispatch, whose outcome is
     final (a second `NoDocumentContext` settles; it never waits again); `context-refused` →
     settle `DocumentReplaced`; the bound → settle `NoDocumentContext`; window death → every
     waiter settles once. The sequence, not `G`, is compared: an unbind by
     `executionContextDestroyed` enters `AwaitingContext` without a G-change (AC12 c), and an
     earlier `context-bound` of the same `G` must not end the wait. Because nothing was sent,
     the re-dispatch is the script's first and only execution, in the document current when it
     runs — Electron's semantics for an eval issued during a load, which AC12 (b) requires across
     a G-change (NC7); it is never a replay. T1 entry: such an eval may run in a later remote
     document if the page navigates meanwhile, as Electron's would.
   - `TerminalAmbiguous(G)`: (c) or (d) failed for `G`. `DocumentReplaced` at once, no wait; the
     Host emits `context-refused(G, s)` on entry and never `context-bound` for `G`; the next G-change
     moves to `AwaitingContext(G′)`.
   - `AttemptFailed`: a Host attempt failed or was cancelled (#1192 L112); `DocumentReplaced` at
     once until the next admitted load's G-change (below); the Host emits `context-refused(G, s)`
     on entry. (Named apart from #1192's
     `FailedClosed` document phase, in which step 2 still evaluates a remote or non-entry
     document.)
   - `Destroyed`/`Exhausted`: refused (step 2), no wait; every waiter settles once.
   **Phase sequence:** `s` increments on every phase entry and belongs to the same STA state;
   it is the only ordering token the facade compares, and `context-bound` carries `(G, s)` too.
   The application sees `NoDocumentContext` when the bound elapses or when the single
   re-dispatch meets `AwaitingContext` again; window death settles with step 2's refusal. After a Host
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
   document regardless of IPC eligibility. Only `Destroyed`/`Exhausted` and step 1's phases
   `TerminalAmbiguous` and `AttemptFailed` refuse; `AwaitingContext` returns `NoDocumentContext`.
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
4. **One local eval state machine; delivery is the transport's final admission (r14, Q25
   resolved).** Each correlation is `Pending → Retired | Settled`, owned by the `keld-wv` STA;
   the transition is taken exactly once. **Retire:**
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
   `MAX_PENDING_EVALS`** (a back/forward-cached context may never call back). **Settle:**
   the CDP completion handler settles iff the entry is still `Pending`; a `Retired` entry is
   freed without a post. `Settled` is **local settlement only**: it posts one settle record
   carrying the dispatch's eval ticket and authorizes no delivery by itself.
   **Eval ticket (r16; the #1192/#540 owner's B21 decision 2, [#1311 6532409232](https://github.com/gyldlab/keld/pull/1311#issuecomment-6532409232)).**
   At dispatch (step 2) the Host mints **one immutable eval ticket**, held in the one bounded
   correlation ledger below; it replaces r14's four-field stamp subset (WebView lifetime, `G`,
   role generation, correlation), which the owner found insufficient as a stand-alone ticket.
   It carries: the exact `WebViewId` **and** its native lifetime/instance (not a generation
   number alone); the one checked native `G`; the original role identity/owner, its
   `RoleGeneration` and the actual authenticated app-link incarnation (not numeric generation
   equality alone); the correlation and the original Bun caller/window ownership, whose one-time
   pending/terminal disposition the ledger holds, since the ticket is immutable; and a
   **dispatch-time native attestation** that the main-frame
   CDP `uniqueContextId` and loader of step 1's `Bound(G, ctx)` belonged to that WebView and
   `G` — evidence attached to the ticket, not another document mint. The attestation stands
   only on B17's proof of Q23 and the lag premise; without that proof nothing is dispatched
   (step 1). #1192 L141's app-IPC eligible-document bit, app-realm token and live
   app-world/frame/controller proof are **excluded**: this is a Bun-requested Host eval result,
   not a renderer-originated privileged `el:` dispatch, and step 2 makes qualified remote and
   `FailedClosed` documents evaluable. Page-supplied stamps are never accepted, and an equal URL
   is never document identity.
   **One ticket type (r17, W1 r13 `e9b95d0c`; no B21 change).** This is the eval use of W1's one
   `AdmissionTicket` (#1307 §4.4), not a second type: `view`, `lifetime`, `generation`, `role`,
   `role_generation`, `link`, and `work` = `TicketWork::Eval { correlation, caller, attestation }`.
   It is captured on the STA through the opaque fence handle keld-core lends each view at creation
   (`view.admission`; keld-wv cannot name `PrimaryRouterHandle`, `crates/keld-core/Cargo.toml:26`).
   The same owner and fence serve W1's renderer use, `TicketWork::Renderer`, which is W1's and is
   proposed to the owner under W1 Q14: B21 names the eval use only. If Q14 is refused, W1 needs its
   own ticket type over the same fence and nothing here changes. Uses differ in one outcome: W1's
   draft answers a refused `try_send` with a refusal terminal, while the eval use waits (below).
   **Admission owner and linearization (r16; decision 1).** The handoff owner is the Rust Host's
   primary app-link outgoing writer, `keld-core::app_session::PrimaryRouterHandle` for the
   current `ActivePrimaryGeneration` (`app_session.rs:5851`, `:5946` at `457a9c3b`), integrated
   with #540's one bounded credit ledger and one ordered primary-link writer, through a **new**
   nonblocking entry point `PrimaryRouterHandle::try_admit_eval_settle(ticket, outcome)` (or an
   equivalent method on that owner; T3 records the name). **It does not exist at the pin:**
   `window_router::write_payload` (`app_session/window_router.rs:77`) calls
   `keld_ipc::link::write_frame` (`keld-ipc/src/link.rs:835`), which writes the socket and is not
   a nonblocking admission gate, and the Bun-side `@keld/kipc` `WriteQueue` is the opposite
   direction. T3 reuses and refactors the existing Host primary writer: no second socket writer,
   no parallel FIFO. A settle record is **handed off** only when, in **one short synchronized
   admission transaction**, that owner (i) verifies the exact original authenticated role/link
   and the ticket; (ii) checks the authoritative native #1192 WebView lifetime, the current `G`
   and the role-retirement state; (iii) reserves #540's direction-appropriate record, byte and
   CONTROL/DATA capacity; (iv) `try_send`s the owned frame into the **final bounded per-link
   writer FIFO**; and (v) commits the correlation's one terminal disposition in the ledger **iff
   that enqueue succeeded**, an infallible in-memory write inside the same fence (r17, review
   4429148703; the owner's text orders commit then `try_send`, see B21). A failure at or before
   (iv) is **not admitted**, commits no terminal and has no attempted socket write; an earlier
   Core/UI channel enqueue or the STA's `Settled` mark does not count. The STA's **effective** G-change (every
   member of step 1's G-change set, `Exhausted` and `Destroy` included) and the runtime owner's
   effective Bun-role retirement **update the same admission fence synchronously, before they
   report retirement**; neither relies on a later `DocumentRetired` control or a mirror. Writer
   admission and either revocation therefore have one real total order. The fence is a short
   shared guard or serial gate over the existing owners' live state — not a separately minted
   `G` and not a stale cache — and is **never held** across socket I/O, a callback, blocking
   Guard or Bun work, or an awaited queue slot; the `try_send` and the credit reservation are
   bounded and nonblocking; the one socket writer and #540's reserved CONTROL progress are kept;
   the first of W1 T2 or T3 to land defines one lock order with `PrimaryRouterHandle.current`, the
   existing shutdown transition and the native view lifetime. **If this cross-thread synchronized gate cannot be
   implemented, B21 stays blocking; two unsynchronized prechecks are not a substitute.** After
   admission the frame may finish writing later on the original link, and the write can still
   fail: "no app-link write when revocation wins" means **no fresh writer admission after
   revocation**, not that the OS rolls back an admitted frame.
   **Failed enqueue (r17).** Rust's `try_send` returns the frame in `TrySendError::Full` or
   `Disconnected`. **`Full`** (and a reservation refused at (iii)) on a **value-bearing** record: not
   admitted, no terminal, any reservation released inside the fence, no socket write, and the entry
   stays `Settled`. #540's sequential profile keeps 1 DATA record per direction (`gh540` L486), so
   with two settled evals this is the ordinary path, not a fault. The attempt registers its ticket
   as a waiter inside that fence, and this owner wakes waiters after a FIFO slot or credit frees, so
   a slot freed during the transaction wakes it. A G-change, role retirement, `Exhausted` or
   `Destroy` that covers a waiter wakes it from its own fence update, because no later drain may
   come (a retired link never drains). Admission is re-attempted only for that same ticket, only on
   such a wake, and re-runs the whole fence, so a revocation that won meanwhile decides the outcome
   and a terminal already committed ends the attempt. Waiters are bounded by `MAX_PENDING_EVALS`;
   there is no spin, no timer, no second queue, and nothing moves to CONTROL. A **framework
   terminal** uses reserved CONTROL and does not wait: with that lane exhausted, #540's fail-closed
   link retirement applies (decision 4). **`Disconnected`** (the writer is gone): the correlation closes
   route-retired, `NotDispatched` **only if no write of this frame was attempted**, which the
   returned frame proves; the reservation is released inside the fence (nothing was dispatched, so
   this is no optimistic refund); #540's fail-closed link retirement applies; never a successor.
   A writer that dequeues the frame before (v) runs changes nothing: (v) cannot fail, and the
   writer's failure path takes the fence, so it orders after the commit.
   **Other fallible steps (r17, class sweep).** A synchronous `CallDevToolsProtocolMethod` failure
   leaves a minted ticket with no CDP callback to own it: in that turn the STA posts one `EvalFailed`
   record with no page text, admitted under the value-bearing rule, so a G-change in between makes
   it `DocumentReplaced` and decision 3 gains no third framework-terminal class. A settle or retirement post that fails
   (`EventLoopProxy::send_event` returns tao's `EventLoopClosed`) means the loop is gone: the
   correlation closes through window death, route-retired, `NotDispatched`, so `Settled` cannot
   outlive the loop. A failed `Runtime.releaseObjectGroup` is recorded, not retried; those objects
   live until their document dies.
   **Record kinds (r16; decision 3, which limits #540 §4.9 for this outcome class).** #540 §4.9
   (#1122 `d9211a96`): "Revalidate role and document under the same queue owner on **every**
   enqueue, including delayed core results." For eval outcomes that means original-document
   provenance on **every** record and current-document eligibility on **value-bearing** records
   only. Every record is admitted only when its ticket's original role/link identity and its
   exact per-document and correlation provenance (`WebViewId`, lifetime, original `G`, one-time
   ledger ownership) verify; a foreign, duplicate or future correlation is never a success and
   frees no credit. **Value-bearing** records — the resolved value **and** every page- or
   CDP-derived error: a rejected Promise's details, a throw, `exceptionDetails` and every
   `EvalFailed` carrying CDP text (step 3) — also require the **current live `G`** to equal the
   ticket's at admission; an `EvalFailed` label never bypasses that check. An obsolete
   value-bearing record becomes exactly one framework `DocumentReplaced` if the original role is
   still live. **Framework terminals** carrying no page value — the STA's `DocumentReplaced` on
   retirement and a proven role/route-retirement disposition — do not require `original G ==
   current G` (a post-revocation rejection must stay deliverable) but do require the original
   `G`/correlation ledger binding, a valid retirement reason, the original role/link still live
   if a frame is to be written, and first-terminal-wins; they carry no G₁ JS payload, no
   page-derived text and no authority for G₂. Producers: a `Pending → Retired` entry's
   `DocumentReplaced` comes from the STA; a `Settled` value-bearing record suppressed at the
   writer gets its `DocumentReplaced` from the writer. **One bounded shared correlation
   ledger** makes the two mutually exclusive, and a duplicate or late CDP completion never
   reopens a correlation. #540's distinct CONTROL capacity is kept: a JS value is never
   classified as CONTROL to evade the DATA quota, and the wire owner records the exact eval
   record kind before T3 (B21). r14c's rule that terminal records are "admitted under the role
   check alone" is **withdrawn**: as written it would admit foreign or duplicate terminals.
   **Terminal mapping (r16; decision 4, with its transport-certainty limits).** One logical
   terminal decision per correlation:
   - **Admission won** (before G₁'s revocation and before the role retired): the late-observation
     rule below.
   - **G revocation won before admission, original role still writable:** the value-bearing
     `Settled` record is suppressed and the writer generates **one** `DocumentReplaced`; for an
     entry still `Pending`, the STA produces that one rejection, not the writer.
   - **Role retirement won before admission:** no write to the dead role or its successor. The
     correlation is logically closed as route-retired through #540's canonical
     link-retirement/first-terminal-wins path, `NotDispatched` **only if the writer can prove no
     write was attempted**. No on-wire terminal is promised to a retired or disconnected Bun
     process. #540's `TerminalDisposition::RouteRetired { certainty }` (`gh540` L249, L259–269
     at `d9211a96`) is defined for `Deliver` transport operations and is **not** repurposed as an
     eval-settle wire variant without the protocol gate; the W3/#539 wire integration uses its
     reviewed outcome vocabulary (§8 wire gate).
   - **Write attempted, partial, or outcome unprovable:** `OutcomeUnknown`, never
     `NotDispatched`; the original link is poisoned and retired after partial-frame loss, nothing
     is replayed, no quota is refunded optimistically and charges are kept until owner-observed
     quiescence (#540 §4.8: "No replay, optimistic refund or unhandled internal rejection"; its
     error table, `gh540` L605: "WV-013 outcome unknown; retire poisoned stream; no replay").
     WV-013's uncertainty meaning is kept where the approved eval error surface can represent
     it; otherwise the canonical IPC link-loss terminal and the error-registry owner (B5)
     determine the caller-visible typed code. WV-013 itself is **not** promised to be sendable on
     a broken socket.
   - A full CONTROL lane never silently drops an authoritative retirement; #540's fail-closed
     link retirement applies.
   "Exactly one" means one **logical** terminal decision per original correlation and **at
   most one** terminal wire result on a live link — not one guaranteed delivered frame after EOF
   or role death.
   **Late observation (r16; decision 5).** If the authenticated admission won before G₁'s
   revocation and before the role's retirement, the result stays a G₁ result for the original
   role and correlation. If its complete write also finished before either retirement, the
   original Bun caller may observe it after G₂ commits — an already dispatched G₁ result, never
   a delivery to G₂. If admission won but the socket write was still in flight when a
   retirement occurred, it may finish on that original link **or** end `OutcomeUnknown`; no
   certain delivery, rollback or replay is asserted. No successor-role retarget, no G₂ relabel
   and no G₂ world or page effect is allowed.
   **Blocker B21 — owner decisions RECORDED (r16), implementation still blocking.** The five
   decisions above are the owner's ([#1311 6532409232](https://github.com/gyldlab/keld/pull/1311#issuecomment-6532409232)); recording them
   implements and qualifies nothing. r17 refines the order of steps (iv) and (v) and asks the owner
   to confirm it (B21). T3 is **DO NOT MERGE** until `try_admit_eval_settle` (or its
   recorded equivalent) is implemented and reviewed, the #539/#540 wire and error-owner
   integration passes, real-Windows AC11 (v)–(viii) negative controls pass and the owner confirms
   that order (§10 B21).
   **Superseded, stated plainly.** r12 and r13 made the STA's `Pending → Delivered` transition
   the handoff and rejected any downstream re-check. The owner overruled that: the STA mark is
   local settlement, and a delayed Core/transport send that has not crossed the final admission
   when revocation or role retirement wins is suppressed. **Still rejected:** a facade-only
   generation guard (a second generation owner in another process); keeping the entry
   cancellable until the facade acknowledges receipt (a cross-process acknowledgement and a
   second state owner); a second document counter or an unbounded tombstone ledger; a forced
   common FIFO for settle and navigation records (#1192 L145 keeps four bounded independent
   CONTROL records, and L63 lists "control deadlocks behind DATA" as a failure; the one final
   per-link writer FIFO above is not that, since it keeps #540's reserved CONTROL capacity). The
   **role** axis stays with KEL-75/KEL-143's existing retirement of role-bound correlations,
   whose effective retirement updates the admission fence (decision 1). Capacity: pending
   evals per window are bounded (`MAX_PENDING_EVALS`, a typed refusal when exceeded; the
   number is set with the #540 owner, §10 B11; an entry counts from its ticket until its terminal
   and its one `Runtime.releaseObjectGroup` completion, so it also bounds outstanding CDP calls
   and drain waiters) because a never-settling script
   (`new Promise(r => window.done = r)`) holds its entry until retirement; Electron has no
   such bound (documented divergence).
5. Counters for every eval AC: the STA's settle-post audit (a `profile-test-hooks` read of the
   settle records the STA posts per correlation) scores the STA's own rule, because the writer's
   revalidation can suppress a stray G₁ success record before any facade sees it (AC11, r14c);
   r16 adds, at the admission owner (`profile-test-hooks` readbacks), the correlation ledger's
   terminal state per correlation, the exact first socket write per correlation (none, partial,
   complete), the native `G` and role/link incarnation read inside each admission transaction,
   and one monotonic fence sequence recorded by the writer at admission and by the STA and the
   role owner when they report retirement; with the page oracle's G₁/G₂ nonces they score zero
   G₂ effects (no G₁ value, error text or authority in a G₂ document or world, no
   successor-role frame);
   the **Bun facade** (another process) counts settle frames per correlation (≤ 1) as defence in
   depth. In AC11 and AC12 fixtures, whose scripts return a page-side
   nonce, it also asserts that every payload carries the nonce of the document its eval was
   dispatched in (for AC12 (b), the re-dispatch's document). Step 4: a G₁ result may reach
   its original caller after G₂ commits only when its writer admission won before G₁'s
   revocation and the role's retirement, and it is never a G₂ document's value; the facade's
   count is evidence, never a generation guard.

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
uniqueContextId", L227–229) must fail `InvalidParams` "uniqueContextId not found" (L232) — the binding AC12 depends on is thereby shown to be honored, not ignored. It
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
| Case identity by admission alone, or by per-request alone | Admission alone trusts one boot-time read for every later reopen; per-request alone would admit a tree that holds siblings differing by case. Two locks flip independently (AC30). |
| Refuse every case-variant URL (`OutsideRoot`) | Breaks R7's Windows semantics and W1's spelling rule (mixed-case `Source`, AC03); Host-serving keeps those and adds no reopen. |
| Queue or stream at the memory limit | An unbounded queue moves the exhaustion elsewhere; streaming is arm (d), rejected (§4.5). |

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
| (p1) value-name enumeration against the resolved AUMID, file name and `*` in `keld-wv`, HKLM runtime keys honoured when the runtime folder is not user-writable | 5 (0.7): checks the folder only, against the launching user — misses per-file grants to other users, broad groups or AppContainer SIDs (0monish 4425565098) | 8 (0.7): enterprise fixed-runtime deployments keep working | 6 (0.8): new production `unsafe` registry and AUMID calls; two features on the existing `windows` dependency, no new crate (the workspace `windows-sys` enables `Win32_System_Registry` but not `Win32_Storage_Packaging_Appx`) | 6 (0.8): an ACL reader aimed at the wrong principal | 6.25 |
| (p2) the same rule through the `windows-registry` crate | 5 (0.7): as (p1) | 8 (0.7) | 6 (0.6): dependency-addition gate; its transitive versions against the lockfile unverified | 6 (0.8): as (p1) | 6.25 |
| (p3) `reg.exe query` child at boot | 6 (0.7): resolved through `PATH`, parses localized text | 8 (0.7) | 9 (0.8): no `unsafe` | 4 (0.8): a process spawn per launch, unmeasured | 6.75 |
| (p4) (p1)'s enumeration, every listed key refused under both roots (r13's first draft) | 7 (0.6): refuses policy-selected runtimes but leaves a default runtime with a broad write ACE unchecked | 5 (0.7): every HKLM-pinned runtime refused | 6 (0.8) | 9 (0.8) | 6.75 |
| (p5) r13d: (p1)'s enumeration; runtime selection refused under HKCU and honoured under HKLM with no check; every selected runtime a same-user residual | 5 (0.7): misses a machine-wide runtime writable by another ordinary user or `Users` (KEL-254 L135's machine-root promise) | 8 (0.7) | 6 (0.8) | 8 (0.8) | 6.75 |
| **(p6) r13e: (p1)'s enumeration; `AdditionalBrowserArguments` and `UserDataFolder` refused under both roots; runtime selection refused under HKCU; every selected runtime (default or HKLM) admitted only under the allowed-writer rule** | 9 (0.6): closes other users, broad groups and AppContainer/LPAC writers for every runtime; same-user residual kept ((q0)); loader-folder binding UNKNOWN (Q28) | 8 (0.6): stock runtimes INFERRED to pass (B16 (ii) cell 4) | 6 (0.7): reuses `keld-guard`'s ACL reader and `windows-permissions`; a new public `keld-guard` entrypoint; launch cost unmeasured (T2) | 6 (0.7): a tree walk at every launch | **7.25** |

r13e: (p6) is chosen — the third option after r12's folder-only check (wrong principal, and it
did not hold per file), r13's refuse-all draft (availability lost, default runtime unchecked) and
r13d's honour-all (a machine-wide runtime writable by another ordinary user). Between the two
enumeration mechanisms, (p1)'s is kept over (p2)'s under the root rule that dependencies are
std-first and minimal. Rejected outright: checking after launch (reading the
browser process's command line needs `NtQueryInformationProcess` or WMI, and the browser has
already started with the argument), and ignoring policies (an HKCU value injects the switch
with no Keld code involved, E13 S1/S3).

### 4.10 Types & channels, capabilities, wire, platform, seam, migration

- **Rust (sketches above):** `keld_guard::{AdmittedRootUrl, RootRelativeComponents, ParsedFileUrl, RootSpelling, FileUrlRefusal,
  file_url_root_relative}`; `keld_update::{open_regular_beneath, OpenedRegularFile, DirChain,
  OpenRefusal, Share, DirCase, directory_case}`, `ActivePackageSelection::tree`; `keld_wv::{AppResourceSource, OpenIntent,
  OpenedResource, ResourceChain, ResourceRefusal, WebviewSpec.app_resources, EvalOutcome, NavigationSignal}`;
  `WebEngine::eval(&mut self, id, script: &str, on_settled: EvalCallback) -> Result<(), WvError>`;
  `AppWindowEvent::{Navigation { view, signal, generation }, EvalSettled { correlation, outcome, ticket },
  HeadersRequested { view, correlation, url, headers } }` (**`AppWindowEvent` drops `Copy`**; `ticket` is
  §4.7 step 4's Host-minted immutable eval ticket, r16);
  `AppWindowCommand::{Eval { view, script, correlation, role }, HeadersReply { correlation, headers },
  RegisterListener { view, kind }, HostLoad { view, url, attempt } }` (`HostLoad` carries the §4.5 seam
  to the STA).
- **TypeScript (`@keld/electron`; `webContents` owners #539/#459):** `WebContentsEventMap` gains the §4.6 events;
  `WebContents.executeJavaScript(code: string, userGesture?: boolean): Promise<unknown>`
  (resolved type = the codec owner's decoded domain; never `any`); `session.webRequest.
  onHeadersReceived(filter, listener)` with the match-pattern matcher in the facade.
- **Capabilities / manifest:** none new; `web.csp` and `web.navigation.allow` are existing
  keys (W1 §4.6 / #541). `keld migrate` writes `web.navigation.allow` (§4.6 rule 5).
- **Wire:** no KIPC framing change. New control-plane kinds on the #539/#540 channel:
  eval request/settle with the eval record kind the wire owner records before T3 (value-bearing
  or framework terminal, §4.7 step 4, B21), headers round trip (own bounded record or FIFO, B11), navigation
  signals, listener registration, the facade's `listeners-settled` record (posted after its
  `whenReady` callbacks drained; recovery hold trigger), the Host's per-generation
  `context-bound` and `context-refused` events carrying `(G, s)` (§4.7 step 1 phases), and the Windows `HostLoad` (`loadURL`/`reload`) command. Wire gate.
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
  only when `vars_os()` finds no variable whose name starts with `WEBVIEW2_` in any letter case
  and no refused loader policy applies (A22);
  pass-through holds are released only at document retirement or webview close, at most 256
  per webview, and none exist under `keld dev` (A23);
  config captured once at boot; listener registrations applied in control-channel order.
- **Migration unit:** `app_session.rs` boot and coordinator, `webview2/mod.rs`, `engine.rs`,
  `keld-update` opener + accessor, `keld-guard` parser, `@keld/electron` facade, `keld migrate`
  allowlist derivation, conformance entries. No temporary adapter; `open_relative_file_windows`
  removed in the PR that lands its replacement. Compatibility fallback: not required.

### 4.11 Resource bounds (r17; one table, every queue, counter and registry)

| Resource | Owner | Bound | Outcome at the bound |
| --- | --- | --- | --- |
| Host-served reserved bytes | STA ledger (§4.5) | `MAX_HOST_SERVED_RESERVED_BYTES` (128 MiB) | `503`, empty |
| One Host-served file | same | `MAX_HOST_SERVED_FILE_BYTES` (64 MiB) | `413`, empty; step 1's hold applies first |
| WRR deferrals in flight, read FIFO and its open file handles, S1 round trips, recovery holds | same | `MAX_HOST_SERVED_IN_FLIGHT` (1,024) | `503`, empty |
| Read worker threads | same | `HOST_SERVE_READ_WORKERS` (1), started once | jobs wait in the FIFO above; no per-read deadline (residual 6) |
| Completion posts to the STA | STA ledger, over tao `EventLoopProxy` (an unbounded type) | ≤ one per in-flight entry | bounded by the in-flight cap |
| Pass-through holds | STA hold table | 256 per webview; ≤ 50 handles per hold (R9's 100 bytes allow ≤ 50 components: one leaf, ≤ 49 directories), so ≤ 12,800 per webview | Host-served |
| Directory chain per request | `DirChain` | ≤ 49 handles | R9 refuses longer paths |
| `PendingTopLevel` records | STA | `MAX_PENDING_TOP_LEVEL` = 8 page-initiated records per webview (provisional; the fixtures hold ≤ 2, INFERRED, T2 census), plus the one Host attempt below | a page-initiated navigation is cancelled at +0, coalesced diagnostic |
| Host attempts awaiting attribution | STA | `MAX_PENDING_HOST_ATTEMPTS` = 1 per webview, `HOST_ATTEMPT_START_BOUND` | typed refusal at +0; typed failure at the deadline |
| Pending evals, ledger, drain waiters, outstanding CDP eval and release calls | STA + `PrimaryRouterHandle` | `MAX_PENDING_EVALS` per window (value: B11); `Retired` and `Settled` entries count | typed refusal; one G-change's terminals must fit CONTROL (B11) |
| Credits, writer FIFO | #540 | #540's retention table | §4.7 step 4 `Full`/`Disconnected` |
| CDP slot candidates | STA | latest loader; first default context plus "more than one" | `TerminalAmbiguous` |
| `context-bound`, `context-refused` | STA | latest `(G, s)` per window, coalesced (the facade keeps only the highest) | older event dropped |
| Navigation signals to Bun | keld-wv, #540 lane | proposed per-window FIFO of 64 (B11) | `did-navigate-in-page` coalesces to the latest URL and the signals of a navigation the Host cancelled at +0 coalesce to a counted drop, so a page's loop never reaches #540's exhaust rule (close admission, retire); signals of a commit stay lossless, paced by commits (INFERRED) |
| Listener registry | Host | one presence bit per `(view, kind)` | idempotent; the facade's listener list is the app's own heap |
| Facade eval waiters | Bun facade | one per application call; bounded wait after `NoDocumentContext`; the `did-stop-loading` wait has no timer, as in Electron | application-authored, no page path |
| Capacity diagnostics | keld-wv | one record per cause with a counter | coalesced |
| Allowed-writer walk | keld-guard (§4.5) | a runtime tree's objects; T2 records the count and sets the bound | typed refusal; cost fallback per B20 |
| `G`, phase `s`, fence sequence | #1192, STA, writer | checked `u32` (→ `Exhausted`); `u64` counters | `Exhausted`; `u64` does not wrap in practice |
| CDP reply bytes | WebView2 | **none the Host can set** (Q30) | B3's byte bound applies after receipt |

## 5. Boundaries

- Implement in: `crates/keld-guard/src/lib.rs` (parser; relocated NLS helpers),
  `crates/keld-update/src/windows_baseline.rs` (+ `locate.rs`, `helper.rs` call sites), `windows_fs.rs`
  (the case-flag read),
  `crates/keld-core/src/app_session.rs`, `crates/keld-wv/src/engine.rs` and
  `crates/keld-wv/src/webview2/`, `packages/@keld/electron/src`, `crates/keld-cli` (`migrate`
  allowlist derivation), `docs/architecture/03,05` (T0), `docs/engineering/keld-error-codes.md`
  (via owner), `keld-compat` entries (T1).
- Must not touch: `keld-guard` evaluator/`path_in_scope` semantics; `crates/keld-ipc`
  framing; the KIPC channel table (#508); macOS `wkwebview/*` beyond the mechanical `eval`
  signature; `crates/keld-native`; KEL-130; workspace `Cargo.toml` (no new crate or
  dependency); the W1 bridge script; the manifest schema (#541).

## 6. Tasks (each ≈ one PR; ordered; owner; blocking dependency)

**Execution order (r17, review 4429148711): T0 → T1 → T2 → T4 → T3 → T5 → T6 → T7.** The task
numbers are kept because W1 cites them (`gh1184` `eae63da8` L932, L1303–1304, L1344). T3 waits on
T4's `did-stop-loading` producer; T4 has no dependency on B21 and needs nothing from T3 (no
AC07–AC09 cell dispatches an eval, and their page oracles read CDP in the harness).

- [ ] **T0 — this spec + architecture deviations** (owner: @amishabenramani). Blocks only on
  items completable before implementation (r16, review 4428004695), each with its owner: B1
  (@amishabenramani), B2's §5 amendment (#1192 owner; the module itself gates T2's merge), B3
  (GH-544/GH-540 owners with #539), B4 (#541/#460 navigation-policy owner), B5 (error-registry
  owner), B6 (KEL-254 owner), B7 (W1 owner), B9 (Windows operator; keld-research PR #38
  merged), B10 (instruction owner), B11 (#540 owner), Q1 and Q2 (the #1185 owner, recorded with
  B1) and Q3 (#539/#459 with #464/#567/#537). **B8 is not a T0 gate:** only T2's own AC20
  device cells clear it, so a T0 edge on it would point backwards (T0 → T1 → T2 is the order).
  B8 stays a mandatory pre-T2-merge gate: no T2 feature is marked complete, merged or
  release-qualified while B8 is failed or unmeasured, and no pre-T0 proof or waiver route is
  named.
- [ ] **T1 — conformance entries first** (`keld-compat` owner): root-only `file:` service,
  `~`/100-byte/reparse refusals, `will-navigate` notification-only + derived allowlist,
  `did-fail-load(-10)` for opener refusals, separate baseline CSP, `hso`/`hsoz` serving
  (classic script/stylesheet/image/font read by Chromium from the held object after verification,
  held until retirement; `keld dev` Host-serves every request;
  `.lnk` refused in every context; `file:` workers unconstructible; `file:` CSP host-sources
  unsupported; app headers applied to documents, `fetch`/XHR and CORS-mode loads only), remote `did-navigate`
  status ordering and empty `httpStatusText`, and the divergences stated elsewhere: page-initiated
  `file:` navigation cancelled until Q27 (B19), `reload()` of a `file:` document refused until
  Q22 (c), a cancelled redirect hop failing its Host attempt, one pending Host attempt per webview,
  evaluation refused after a failed
  Host attempt and possibly run in a later remote document (§4.7 step 1), the
  `MAX_PENDING_EVALS` bound, `Runtime.enable`'s console-argument serialization, the refusal of a
  case-sensitive root, ancestor or in-root directory, Host-serving of a case-variant root
  spelling, and the `413`/`503` capacity outcomes with their `did-fail-load` codes, including the
  `keld dev` over-cap refusal release does not share, the page-initiated navigation cap and the
  coalesced signals of cancelled navigations under a page's storm (§4.11). Blocks on T0.
- [ ] **T2 — one opener, admitted root, loading path, containment** (Windows W3 implementer):
  parser + goldens (AC05), `open_regular_beneath` + `tree()` + `open_pinned_leaf` migration
  (AC04), root retention and boot migration, `file:*` filter/handler and `Navigate` loading
  with the safety atoms the loading path needs (r15): §4.6 rule 1's header issuance, nonce, causal
  attribution (unproven mode refuses every Host `file:` attempt, B18), **`HOST_ATTEMPT_START_BOUND`
  and its failure terminal (r17)**, all through one keld-wv attempt entry point, the raw
  `ContentLoading` and `NavigationCompleted` handlers that deadline, the holds and §4.4 consume,
  the STA navigation audit, §4.4's nonce credit, and
  rule 2's B19 cancel of every page-initiated `file:` target, without which an uncreditable
  page-initiated `file:` document would reach W1 FailedClosed and `onLoss` (§4.4)
  (AC01–AC04, AC06, AC17–AC19, AC22–AC31, and AC08 (l)/(m)/(n), B18's checks; the [TARGET, B13]
  cells AC01 (ii) and AC02 (ii) read blocked until B13 and T7 re-runs them; AC24's listener
  cells run in T5), `hso` classification, R10, session root pin,
  drive-letter admission, the extended KEL-66 test, the `WEBVIEW2_*` refusal and the `keld dev`
  launcher's `env_remove`, both over one case-insensitive `vars_os()` enumeration (AC28 (iv′)),
  the policy refusal,
  the `Origin` classifier, the verification-handle hold and its release rules, arm (b) for
  Host-served responses with its reservation ledger and constants (§4.5), both case-identity
  locks, the §4.11 bounds T2's handlers own, the receipt validator (AC20–AC23 consume it) and the
  test-only header policy seam. Merge blocks on its AC20 receipt (a, b, c), **the AC30 and AC31
  receipts with every mutant RED and Q29's gate (r17: Q9's limits and overflow negatives are
  mandatory T2 gates)**, and B8, B16, B18, B19, B20, the
  issue-scoped `crates/keld-wv/AGENTS.md` sanction for the registry calls and
  `crates/keld-update/AGENTS.md` update for the case-flag read, each with its unsafe gate,
  public API + permission gates, the `keld-guard` unsafe-trigger line, #1192 T1 (B2).
- [ ] **T3 — settled eval** (same, with #539 for the facade's `executeJavaScript`, its load wait
  and the `(G, s)` wake-up): `WebEngine::eval` destination form, self-test, context
  tracking, the one `Pending → Retired | Settled` machine, per-correlation object groups, role correlation (AC10–AC13); AC14 after B3. The final admission
  fence lands once, with the first of W1 T2 or this task to merge, and the other extends it (W1 §4.4);
  W1 landing it clears no B21 item for the eval use. The Bun facade half of AC10 (i) and AC12 reads
  blocked until B13 and T7 re-runs it. Blocks
  on **T4 (r17: the native `did-stop-loading` producer and `isLoadingMainFrame()` of AC10 (i), and
  rule 2's allowlisted `https:` replacements of AC11 and AC12)**, B2, B3, B5, B14 (`Runtime.enable` from create: a written waiver or a design change before
  T3), B17, B21 (owner decisions recorded r16; still blocking: `try_admit_eval_settle`
  implemented and reviewed, the wire/error-owner integration, real-Windows AC11 (v)–(viii)
  receipts, the owner's confirmation of the (iv)/(v) order — T3 is DO NOT MERGE until all four), the control kinds and the eval record kind
  (wire gate), a #540/#539 transport integration review, an exact-head security review and
  native AC11 (i), (ii) and (v)–(viii) negative-control receipts (r14, Q25, r16).
- [ ] **T4 — navigation signals and `will-navigate`** (same, with #539): §4.6 mapping,
  error-code table, rule 2's allow forms beyond B19's cancel, rule 4's pre-check and entry
  exemption for the application's Host attempts with the Host side of `HostLoad`, the per-view
  policy rule 2 reads (AC08 (o)), and the notification (AC07–AC09, and AC08 (l)/(m)/(n) re-run through `HostLoad`; rule 1, its deadline and B19's cancel
  landed in T2). Blocks on T2, B4, B12, B18, P3 and the navigation-signal and `HostLoad` control
  kinds with the §4.11 signal FIFO (wire gate, B11);
  AC08 (g)/(h) and the Bun facade half of AC07–AC09 read blocked until B13, and AC07–AC09's Electron
  differential reads blocked until #537's recorded cells exist (#494/#495); T7 re-runs both. T4 lands
  the policy value and its fail-closed default; the `WindowRequest::Create` window key that selects
  it in a compat window is W1 T2's, which blocks on T4, so T4's cells pass the policy at create in
  the harness, and until W1 T2 and #541's key land a production view has no bound policy.
- [ ] **T5 — app headers, baseline CSP, registration ordering** (same, with #539):
  round trip, facade matcher, FIFO ordering (AC15, AC16, and AC24's listener cells). Blocks on T4
  (`HostLoad`, the ordered `loadURL` record), P4, P5, B11 and B13 (a listener exists only in a compat boot; schema-1 refuses the registration
  typed, §4.5).
- [ ] **T6 — `keld migrate` allowlist derivation** (`keld-cli`/KEL-17 owner): §4.6 rule 5,
  fixtures (veto-all, literal-conditional, unprovable), corpus run. Blocks on KEL-17 admitting
  the slice (W1 Q9), B4 (the policy owner accepts the writer) and B12 (the `file:` allow forms
  it writes).
- [ ] **T7 — Windows qualification receipt** (@amishabenramani): full RED matrix on a real
  device incl. elevated ETW, directory-symlink and intermediate-junction cells, AC23 cells,
  pinned-app migration/evaluation demand slices, AC21. Blocks on T2–T6, B9 (every cited run
  published), B13 for the [TARGET] cells and the pinned-app slices (a compat boot), W1 (#1307,
  B7) for the epoch, IPC and preload observables (AC20's preload re-run), and #537's recorded
  cells for the Electron differential; cells still blocked on B19/Q27 read blocked, never pass.
  Full application acceptance stays W0.

## 7. Test plan

| Test | AC | Shape and anti-flake |
| --- | --- | --- |
| `guard_file_url_root_relative` (pure; R7/R8 goldens Windows-only) | AC05 | Table rows + probe corpus; mutants: drop `:`, drop `~`, decode twice (non-discriminating, recorded), case-sensitive R7, ordinal drive letter, `Exact` for every root prefix. Adversarial fixtures permanent. |
| `guard_navigation_allow_forms` (pure, r13) | AC08 (c), (d), (j′) | The `keld-guard` navigation entrypoint over the allow forms (`none`, literal with its query constraint, `in-root:**`) and `components_equal`; single faults: every in-root target on-allowlist, query cut, query folded through the component equality, byte-exact components in one consumer → RED. Independent of B19's device-level cancel. |
| `wv_served_document_association` (pure state machine, r14b, r15) | AC06 (b′), (c′), (d), (d′) | Feeds `NavigationStarting(n)`, a same-URI frame DOCUMENT `200` without the nonce, then a top-level `404` or no top-level request, and asserts `PendingTopLevel(n).served == None` before any `ContentLoading`; a separately labelled arm injects `ContentLoading(n, IsErrorPage = FALSE)` and asserts `n` is not Host-served. Mutants: credit the first parsed-equal DOCUMENT WRR (d′) → RED in both; drop the parsed-URI conjunct, with an injected nonce-bearing DOCUMENT WRR for other components → `served` bound → RED; drop `served.status == 200`, with a nonce-bearing `404` then the injected non-error `ContentLoading(n)` (b′) → Host-served → RED; the DOCUMENT-only double fault (c′), labelled and not counted. |
| `update_open_regular_beneath` (Windows, native) | AC04 | Fixture with leaf junction, **intermediate junction**, file symlink, directory symlink (precondition printed, never silently green), hardlink (`nlink` read-back), ADS, trailing dot; `Share::Pinned` and `Share::Served`; mutants: Win32 `CreateFileW` path open; one multi-component `open_with` (needs Developer Mode or a cloud placeholder, else **rule not exercised**). |
| `core_app_resource_source_api` (Windows, native) | AC29 | Cells (i)–(iii) through the published `AppResourceSource` trait; the error code of every refused mutation is counted (32 vs 5); mutants (1)–(4). |
| `wv2_case_identity` (device; elevated fixture, T2) | AC30 | Cells (i)–(vii) with the two independent locks; lock 2's cell builds the root in the harness beneath admission, printed; mutants (1)–(6) each in its own process; `FILE_CS_FLAG_CASE_SENSITIVE_DIR` read on every fixture directory before and after, so a fixture whose flag did not take reads rule not exercised. |
| `wv2_host_served_budget` (device, T2) | AC31 | Barriers on worker completion and STA delivery, per-WRR response ledger, reservation readback plus an independent buffer accountant; `hso`, `hsa`, and the 256-hold fallback; mutants (1)–(9); peak readbacks for the E8 and E13 workloads; Q29's private-bytes cell. |
| `wv2_host_attempt_attribution` (device, T2) | AC08 (l), (m), (n) | Runs at T2's exact head through T2's entry point, scored at the STA navigation audit, with the positive commit in the same run; mutants (l′), (l″), (m′), (n′), (n″); kill bound 2 × `HOST_ATTEMPT_START_BOUND`. T4's `wv2_navigation_signals` re-runs it through `HostLoad`. |
| `update_pinned_leaf_migrated` | AC04 | Existing locate/helper tests pass unchanged through the generalised opener; source test that `open_relative_file_windows` and page-controlled `OpenOptions::open` are absent. |
| `wv2_file_origin_matrix` (device, ignored-by-default, STA harness as `media_acceptance`) | AC01–AC03, AC06, AC17–AC19, AC22–AC28 | `file:*` baseline + N2; AC06 (d)'s same-exact-URI frame race with its declared Host-queue hook (frame `200` first, top-level WRR refused or withheld), scored first by the `PendingTopLevel(n).served` audit before any `ContentLoading`, with the epoch oracle counted only on a real non-error commit; plain-outside and `https` cells; per-rule at-risk counters split Host-served vs pass-through per context; E9's identity oracle per kind and E11's exclusive-hold same-object oracle; the pass-through parser corpus as `<script>`, stylesheet, `<img>` and `@font-face`; naive mutants 1a–1c, unverified pass-through, drop-R10; `ResourceContext`/`Origin` census incl. `<object>`/`<embed>` and every CORS-mode kind; whole-root swap with and without `DELETE` on a handle-free fixture root; injected race with and without the hold and the verify → hold gap cell; hold release on cancel/failure/navigation; `subst` rebind with the test seam; CSP cells incl. cross-directory `'self'` and a hash; worker cells with the switch on (NC) and off; `WEBVIEW2_*` launcher removal and refusal on/off, the lowercase, mixed-case and uppercase spellings with the guard-off loader cell and mutants (3a)–(3c) (AC28 iv′), the policy cells (B16), the retirement-release, multidoc and 256-bound cells; ETW when elevated. |
| `wv2_navigation_signals` (device) | AC07–AC09 | Loads (i)–(v); AC08 (l) in both modes with the missing-trigger variants, (m) the header reload cell (r13) and (o) the per-view policy cell, with the #1192 generation readback for (b) and (e); page oracle; #537's recorded Electron cells for the same fixture (AC07; blocked until they exist); mutants per row. |
| `wv2_settled_eval` (device) | AC10–AC13 | Promise/statement/reject/throw/getter/Proxy cells; gap dispatch; AC12 (d) phase orders and the (e) same-URL cross-clock Host-queue fixture (r13); replayed G1 scored at the STA's settle-post audit (r14c); AC10 (i) with its producer-removal NC; AC11 (v)–(viii), blocked on B21: barriers before and after `try_admit_eval_settle` in both orders for the G-change and the role retirement, the injected `Full` and `Disconnected`, success and script-error suppression, forged/duplicate/wrong-role terminals, full DATA with reserved CONTROL, zero-byte/partial/after-write loss, the original caller observing a completed write after G₂, and mutants (5)–(18), (20), (21); AC11 (ix) and mutant (19) as a state-machine fixture; role recovery via the existing `no_flag/windows/recovery.rs` harness; injected CDP failure and the AC13 probe injections. |
| `facade_eval_wakeup` (TypeScript unit, `@keld/electron`, with #539) | AC12 (f) | Injects `NoDocumentContext(G, s)` replies and `context-bound`/`context-refused` events in both orders against a stub transport; mutants NC8a (`G` compared instead of `s`) and NC8b (events after the reply only) → RED. |
| `wv2_headers_received` (device) | AC15, AC16, AC24 (listener cells) | Listener before/after `loadURL` (ordering), absent, retired, a predecessor reply released after the `RoleGeneration` move (d″); `fetch`/XHR issued during recovery (d, d′); AC24's dev/release listener cells; `fetch`/XHR- and module-matching listener (one round trip each) and classic script/stylesheet/image-matching listener (zero); SVG/XHTML `Content-Type` override; forbidden-remote-script counter. |
| `migrate_navigation_allowlist` | §4.6 rule 5 | Fixtures: veto-all, literal-conditional, unprovable; golden manifests; mutant lifts a page-input condition. |
| `codec_eval_domain` (owner's) | AC14 | Lands with B3. |
| P1 harness (`keld-benches`) | AC20 | §9. |
| Receipt validator (`tools/`, lands in T2) | AC20–AC23 | Fields, at-risk per rule, "rule not exercised", UNKNOWN cells. |

Anti-flake: device tests behind the Windows feature flag; callbacks pumped by
`wait_with_pump`, no sleeps; each mutant in its own process; per-run temporary root with
its own DACL; precondition status printed for symlink/8.3/ETW cells.

## 8. Review gates triggered

- **unsafe — mandatory:** no new FFI function in (1) and (3). (1) `keld-guard`: the two NLS calls move
  from locals of `validate_windows_package_paths` into shared private helpers used by the
  parser; `crates/keld-guard/AGENTS.md` KEL-53 T3 trigger line gains `file_url_root_relative`;
  same bounds and SAFETY proofs. (2) `keld-wv/webview2`: new COM registrations
  (`AddWebResourceRequestedFilterWithRequestSourceKinds`, `add_WebResourceRequested`,
  `GetDeferral`/`Complete`, `CreateWebResourceResponse`, stream creation,
  `add_ContentLoading`, `add_DOMContentLoaded`, `add_SourceChanged`, `add_HistoryChanged`,
  `CallDevToolsProtocolMethod`,
  `GetDevToolsProtocolEventReceiver`, `NavigationKind`, and for rule 1's header proof
  `CreateWebResourceRequest`, `NavigateWithWebResourceRequest` and the `RequestHeaders` reads) with local proofs and an issue-scoped
  `crates/keld-wv/AGENTS.md` update; independent unsafe gate on the final diff. One handler per
  event and view: rule 1 and B19's cancel extend the existing `NavigationStarting` registration
  (`webview2/mod.rs:2297` at `457a9c3b`), and W1 (#1307 §4.5) runs its `NavigationStarting`
  hold, `ContentLoading` epoch and `SourceChanged` attribution inside W3's registrations and
  adds none. The opener
  adds **no** unsafe, and the browser-argument defence adds none (r16: `std::env::vars_os` and
  `Command::env_remove` are safe; r8's `remove_var` scrub is withdrawn); the loader-policy check
  adds `RegOpenKeyExW`/`RegEnumValueW`/`RegCloseKey`, the two AUMID reads and `CoTaskMemFree` (A22 rule (3), B16). r10 registers no
  `WebResourceResponseReceived` handler for holds. (3) `keld-update/src/windows_fs.rs` (r17): one
  new `unsafe` read, `GetFileInformationByHandleEx(FileCaseSensitiveInfo)` into a
  `FILE_CASE_SENSITIVE_INFO`, a new class on a function already bound there, with a local proof
  and an issue-scoped `crates/keld-update/AGENTS.md` update. The runtime allowed-writer walk (r13e) reuses
  `keld-guard`'s `windows-permissions` reader and the C3 walk; any new `unsafe` it needs is decided
  by this gate at T2.
- **public API — mandatory:** `keld-guard` parser, the runtime allowed-writer entrypoint (§4.5
  rule (3), r13e) **and a navigation entrypoint that
  evaluates `web.navigation.allow` for a `Principal::Webview` outside `evaluate`** (today
  `path_in_scope` is private behind `evaluate`, which refuses webview principals; the
  `file:` allow forms of B12 are matched on parser components by this entrypoint, never by
  `path_in_scope`) and `ParsedFileUrl`/`RootSpelling`; `keld-update` opener/accessor and
  `directory_case`/`DirCase`;
  `WebEngine::eval`, `WebviewSpec.app_resources`, `AppResourceSource` (with `OpenIntent`,
  `ResourceChain`), `AppWindowEvent`
  (drops `Copy`)/`AppWindowCommand` variants; `@keld/electron` events, `executeJavaScript`,
  `onHeadersReceived`; `keld migrate` output key.
- **permission model — mandatory, independent reviewer:** two name owners; refusal table; `hso` classification and verify-then-pass-through with its
  stated residuals; the `Origin` classifier; the verification-handle hold; R10; the two
  case-identity locks; the Host-served memory budget and its `413`/`503` outcomes; session root
  pin; drive-letter admission; Keld-owned browser arguments (launcher removal, refusal, policy refusal);
  the hold's retirement-only release, the 256 bound, `keld dev` Host-serving everything and the
  document-lifetime pinning residual;
  allowlist-first cancel with +0; Host attribution only by causal proof (no URI/order exemption,
  B18); served-document identity credited only by navigation-specific proof (B19);
  the runtime allowed-writer rule for every loader-selected runtime (B20, Q28); hardlink read-back; separate
  baseline CSP; fail-closed headers; eval context + generation gates and the eval ticket's
  admission (B21); allowlist derivation.
- **dependency addition:** none (two features, `Win32_System_Registry` and `Win32_Storage_Packaging_Appx`, on `keld-wv`'s existing `windows` 0.61; lockfile unchanged).
- **wire protocol — #539/#540 owner:** control-plane kinds; the headers round-trip record and the
  navigation-signal FIFO (B11);
  the eval record kind and the eval-settle outcome vocabulary, which does not repurpose #540's
  `TerminalDisposition::RouteRetired` (B21).

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
includes a restrictive CSP's effect; one runtime, one device; r17's per-directory flag read has no
isolated number (AC20 covers it). B8 was resolved by measurement
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
  byte bound tied to `MAX_FRAME_LEN`. #540 §4.10 already requires the `executeJavaScript`-result
  consumer to refuse cycles and shared-reference identity explicitly in v0 (`gh540` L636–642 at
  `d9211a96`), so the decoder refuses what CDP's deep serializer represents unless the owners
  change that text. Check: recorded decision; AC14 unblocked.
- **B4 Allowlist-first `will-navigate` and the `keld migrate` derivation** — navigation-policy
  owner (#541/#460; W1 §4.6): accept §4.6 rules 1–5 and the `web.navigation.allow` writer.
- **B5 Error-registry codes** — registry owner: capability refusal, `DocumentReplaced`,
  `NoDocumentContext`, and (r17) the case-identity refusal, the Host-attempt start failure, the
  `PendingTopLevel` cancel and the capacity diagnostics.
- **B6 KEL-254 AC7/AC8 extension** — KEL-254 owner.
- **B7 W1 alignment** — W1 owner (published head `9bc56726`, re-read for r15; at `043170ca`
  items (ii)–(iv) were open): (i) the Source rule matches (`gh1184` L467–468); (ii) **met**:
  the identity text (L593–600) states W3's identity — the served record's in-root components,
  bytes as currently present (owner Q2); (iii) **met**: the parser row (L139) describes §4.4's
  parser (R6 collapse, `validate_fs_component` root); (iv) **met**: W1 restates the A″ rule
  (§4.6, L687–716; its B6 binds to it, L1260–1263), adopts the exported component equality
  for eligibility and runs the app preload in every Host-served `file:` top-level non-error
  document (L281, L392); (v) eligibility
  for **non-entry in-root documents** (allowed by `in-root:**` or a literal) is decided
  together with B12 — entry-only eligibility would send every in-root page navigation to
  FailedClosed and `onLoss` (W1 caps reload-entry at 3 then closes the window), which makes
  multi-page `file://` applications unusable; (vi) W1's Source rule inherits R10 (`.lnk` → the
  new public `FileUrlRefusal::ShellLink`), so a `.lnk` source is refused there too; (vii) r13's
  attribution leaves no ambiguous page document for W1 to handle (no page event ever receives
  the Host exemption); a Host attempt that cannot be attributed fails typed and never counts
  toward W1's recovery cap; (viii) r13's crediting: a page-initiated `file:` document is never
  credited Host-served until B19 resolves, and W3 cancels such navigations rather than handing
  W1 an ineligible commit; (ix) W1 r9's lifecycle table maps three more WebView2 events onto a
  #1192 +1 — `ProcessFailed`, `NavigationCompleted` with no `ContentLoading`, and the
  navigation bound (L631–633) — which #1192's table at `cd30f997` does not list (L104–118);
  W3's one G-change set (§4.7 step 1) and hold retirement take them only once #1192's owner
  admits them (B2), and until then a value-bearing record whose ticket carries a superseded `G`
  is suppressed at the writer (fail-closed); (x) r17: `file_url_root_relative` returns
  `ParsedFileUrl`; W1's `.is_ok()` call (`gh1184` `eae63da8` L627) is unaffected, `root_spelling`
  is W3's alone, and W1's citations of W3's T2, T3 and T4 keep their numbers; (xi) r17, met by W1 `e9b95d0c`: W1's
  recovery and `ContentLoading` rows name `413`/`503`, and a capacity `413`/`503` on a top-level
  document MUST NOT consume one of the three recoveries (§4.5), or a page could spend them by
  filling the Host-wide budget; every reused
  contract is still an open draft
  (re-checked at approval).
- **B8 P1 report — reopened (E14)** — @amishabenramani, before T2 merges (r16: not a T0 gate;
  no T2 feature is complete, merged or release-qualified while B8 is failed or unmeasured).
  E8–E10 resolved it
  for `hs`; E14 failed AC20 (a)–(c) for the chosen design, and `hso` vs `hsm` reversed from
  E12's −2.0 %/−2.8 % to +6.78 % [+3.13, +10.10] with STA occupancy 82.1 vs 76.6 ms per load —
  possibly real Host work (the `Origin` classifier is a candidate), not only session drift.
  (1) Attribute `hso` − `hsm` per request on the STA; (2) re-run AC20 with the A/A arm;
  (3) if any bound still fails, a written waiver (arch 01 §5) or a design change. Arm (f) and
  the restrictive-CSP effect also remain. Check: AC20 receipt.
- **B9 Probe evidence published** — Windows operator: keld-research note 268 (PR
  0monish/keld-research#38, open and unmerged, head `816794e1`; runs `gh1185-w3-file-origin-probe-2026-10-28`,
  `gh1185-capstd-junction-2026-10-30`, `gh1185-p1-serving-cost-2026-10-30`,
  `gh1185-p2-diff-hybrid-2026-10-30`, `gh1185-p2-hs-2026-10-30`, `gh1185-p2-cells-2026-10-30`,
  `gh1185-p2-hold-2026-10-30`, `gh1185-p2-vite-q21-2026-10-30`, `gh1185-p2-hsoz-2026-10-30`, and E15's
  `gh1185-q24-wrr-order-2026-10-30` published at `c1d15a2`, and E16's
  `gh1185-r17-case-flag-probe-2026-10-31` published at `816794e1`) merged, so every citation here resolves to a public path
  on the default branch. Check: PR merged; note path stable.
- **B10 `keld-wv` threading instruction** — instruction owner through `.agents/instructions.md`:
  `crates/keld-wv/AGENTS.md` L5–6 must state that a worker may read a **file** handle while
  every COM/platform handle stays on the STA (arm (b) for Host-served responses and arm (f) depend on that reading; this
  spec interprets the current text and cannot rely on its own interpretation), and that the
  per-request directory chain, per-Host-served-response buffer (bounded by §4.11), navigation-signal, eval-settle and header round-trip payloads are the
  recorded allocation exception; arm (c) is not adopted (§4.5), so no further sentence is needed.
- **B11 Headers round trip and eval capacity** — #540 owner: a bounded record type or FIFO
  outside the four CONTROL records (#1192 L145) with its bound and an overflow rule that
  holds on the STA FIFO rather than answering `403`; the `MAX_PENDING_EVALS` bound and its
  typed refusal (§4.7 step 4). #540's retention table already bounds the bridge's native
  evaluations at "at most 1 active evaluation per document" (`gh540` L493 at `d9211a96`);
  whether application `executeJavaScript` evaluations share that bound is part of this decision.
  r17: the headers record's bound is capped by `MAX_HOST_SERVED_IN_FLIGHT`, and W3 proposes the
  §4.11 navigation-signal FIFO, which this decision accepts or replaces (T0 blocks on B11). One
  G-change retires every `Pending` entry of its window at once, each a CONTROL terminal, and #540
  keeps 4 CONTROL records per role with "exhaust → close admission/retire" (`gh540` L492), so
  `MAX_PENDING_EVALS` must fit that lane or those terminals must coalesce as idempotent state;
  otherwise a page navigation under five pending evals retires the link.
- **B12 `file:` allow forms** — #541 owner (manifest grammar): the three forms of the A″ rule
  (`none`, a literal in-root component path with its query constraint — exact, none, or any
  for rule 5's declared-entry fallback — and `in-root:**`), matched on parser components by
  the new `keld-guard` navigation entrypoint (§8), never by `path_in_scope`; the writer is
  `keld migrate` (union merge, developer entries preserved); non-entry in-root eligibility
  decided with B7 (v). Check: #541 revision names the forms and the entrypoint.
- **B13 W2 schema-2 Windows boot and window build (TARGET dependency)** — W2 owner (gh1188
  `8c5e5d74`, #1280 r18: its T1 Ready without a window behind a debug test admission, its T2 the D7
  release proof and D2's product admission, its T3 the in-loop `BrowserWindow` build D4 and the
  derived managed-window flip):
  `validate_windows_boot_files` admits `InitialWindow::None`, `create_app` is skipped
  for facade boots, `HostInitialized` Ready on Windows, windows built in the factored
  controller build. Today `app_session.rs:3431-3438` returns `KELD-CORE-034` and #1286 pins
  it. Until W2's T1, T2 and T3 land: AC01 (ii), AC02 (ii), AC08 (g)/(h), every listener case of
  AC15, AC24's listener cells, the Bun facade half of AC07–AC13 (§3) and the pinned-app slices of
  T7 are **blocked**, T5 cannot merge,
  the Windows `loadURL(file:)` Host seam (`HostLoad`, W3-owned; facade vocabulary #539/#459)
  has no caller, and the schema-1 path is the only live one. Check: W2 T1–T3 merged; a
  compat boot on Windows reaches `ready` with zero windows and then builds a `BrowserWindow`.
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
  separately `*`, and separately the process's AUMID — including an explicit AUMID that ends in
  `.exe` (`Vendor.App.exe`, distinct from the executable's name, set through a test-only seam
  because production Keld sets none), whose matching `AdditionalBrowserArguments` value carrying
  `--allow-file-access-from-files` must be refused before environment creation, under HKCU and
  separately under HKLM (mutant: classify names by the `.exe` suffix → it is ignored and
  worker cells escape → RED); a `UserDataFolder`
  value named `*` is pre-declared as not applied by the loader (S1); `AdditionalBrowserArguments` carries `--allow-file-access-from-files`. Check
  without Keld's check that the value reaches the runtime (worker cells escape; the
  `UserDataFolder` value is the folder used) and with it that every cell is refused typed.
  (ii) HKLM cells (elevated operator, a user UAC action): `AdditionalBrowserArguments` and
  `UserDataFolder` refused; `BrowserExecutableFolder` under the allowed-writer rule (r13e), cells: (1) an HKLM-selected runtime copy
  whose folder is administrator-only but where one file (separately: a loaded DLL) grants
  `Users` write → refused typed; (2) a file granting another standard user's SID write →
  refused; (3) a per-machine default runtime given an `ALL APPLICATION PACKAGES` write ACE on
  one file → refused; (4) positive control: the stock per-machine runtime → admitted and the
  diagnostics name it; (5) a per-user default runtime writable only by the launching user →
  admitted (the stated same-user residual); (6) the version folder's parent inside the product
  layout grants `Users` `FILE_ADD_SUBDIRECTORY` → refused (mutant: the KEL-254 ancestor exception
  applied inside the boundary → admitted → RED); a value named
  after another executable is ignored. NC: drop
  the check → the HKCU switch cell delivers OUT nonces; NC (r13): honour runtime selection under
  HKCU → the HKCU `BrowserExecutableFolder` cell launches a user-chosen runtime → RED; NC (r13e):
  check only the runtime folder's DACL (r12's rule) → cell (1) launches → RED. Check: receipt with (i), (ii) and the NC.
  The production `unsafe` sanction in `crates/keld-wv/AGENTS.md` lands with T2. If the
  enumeration cannot see a value the loader applies, W3 does not ship until a mitigation is
  designed.
- **B17 Eval context correlation (Q23)** — W3 implementer, before T3 merges: a CDP census (a
  `profile-test-hooks` probe in the STA harness, needing no T3 code, so it may run at T2's or T4's
  head) with an at-risk count above 0 shows that a default main-frame context created after
  `frameNavigated(L)` in the CDP stream belongs to `L`, including after a cross-process
  navigation, and (r13) the lag premise of §4.7 step 1 (no main-frame `frameNavigated` of an
  earlier commit reaches the STA after a later navigation's `NavigationStarting` handler
  returned), with AC12 (e) RED under NC6. If either is refuted or unmeasured,
  `executeJavaScript` is a typed capability refusal on Windows until a correlation the device
  proves replaces §4.7 step 1. Check: census receipt.
- **B18 Host attribution proof (Q22 (a), (c); §4.6 rule 1)** — W3 implementer with
  @amishabenramani (Windows operator), before T2 merges (its loading path issues the first Host
  `file:` attempt) and before any Host `file:` load ships on Windows: Q22's cells pass on the supported runtime with at-risk counts above 0, AC08 (l)
  turns RED under (l′)/(l″), (m) under (m′) and (n) under (n′)/(n″), and the result is pinned per runtime version.
  Impact if (a) is refuted: unproven mode refuses **every** Host `file:` attempt under every
  allowlist — the schema-1 boot `Navigate`, compat `loadURL` and `onLoss` alike (rule 1 (ii)) —
  so W3 delivers no Windows `file:` application (§1) and the design returns to the owner (Q26).
  If only (c) fails, `reload()` of a `file:` document stays a typed refusal. Check: Q22 receipt
  naming the runtime and head.
- **B19 Top-level served-document credit (§4.4; Q22 (b), Q27)** — W3 implementer, before T2
  merges: Host attempts are credited by the nonce on their DOCUMENT WRR request (Q22 (b)) with
  AC06 (d) RED under (d′); page-initiated `file:` navigations stay cancelled at +0 (§4.6 rule 2)
  until a Q27 mechanism passes its cells. Impact: multi-page `file:` applications whose pages
  link to each other (`in-root:**` or literal forms) do not navigate on Windows (the click is
  cancelled, no loss, no `onLoss`). If Q22 (b) is refuted, no `file:` document can be credited
  on the supported runtime and W3 is not approvable. Check: AC06 (d) receipt; a Q27 receipt or
  an owner decision on #1185 accepting the cancellation divergence.
- **B20 Runtime allowed-writer rule (§4.5 rule (3), residual 4; Q28)** — W3 implementer, before T2
  merges: B16 (ii) cells (1)–(5) pass with the folder-only mutant RED; T2 measures a real runtime
  tree's object count and the check's cold-launch cost against architecture 01 §5's 5 % rule, and
  if it is over budget records the weaker executable-and-loaded-DLL fallback as a stated
  divergence; @amishabenramani accepts the same-user residual (already (q0)). Impact: a runtime
  whose tree grants write to anyone beyond admin-class principals and the launching user is
  refused typed, so a machine misconfiguration surfaces as a launch refusal naming the object and
  the SID. Check: B16 (ii) receipt, the T2 cost record and Q28's receipt.
- **B21 Eval delivery handoff (§4.7 step 4; Q25 resolved) — owner decisions RECORDED, still
  blocking for implementation** — #540/#539 owners and the #1192 owner with the W3 implementer,
  before T3 merges. The #1192/#540 owner's five design decisions are recorded at
  [#1311 6532409232](https://github.com/gyldlab/keld/pull/1311#issuecomment-6532409232) and applied in §4.7 step 4 (r16): (1) the admission
  owner is `keld-core::app_session::PrimaryRouterHandle` with a **new** nonblocking
  `try_admit_eval_settle`, one synchronized transaction over steps (i)–(v) with a `try_send`
  into the final bounded per-link FIFO, whose fence the STA's G-change and the role retirement
  update synchronously and which is never held across I/O; (2) one Host-minted immutable eval
  ticket replaces the four-field stamp subset; (3) original provenance on every record, the
  current `G` on value-bearing records including script-derived errors, framework terminals
  bound to the original ledger entry with first-terminal-wins, one bounded correlation ledger,
  and the eval record kind recorded by the wire owner; (4) route-retired `NotDispatched` only
  with proof of no attempted write, `OutcomeUnknown` with link poisoning and no replay or refund
  otherwise, no repurposed `TerminalDisposition::RouteRetired` and no WV-013 promised on a
  broken socket; (5) the admitted-and-written versus admitted-but-in-flight distinction. No
  further owner choice is needed on these points. **Owner confirmation requested (r17, review
  4429148703):** decision 1 orders "(iv) commits the correlation's one terminal disposition" then
  "(v) successfully `try_send`s", and also says "Any failure before (v) is **not admitted**", which
  leaves a failure *at* (v) with a committed terminal and no frame. W3 swaps the two inside the
  same fence: `try_send`, then commit iff it succeeded. Meaning kept: same owner, same
  transaction, same linearization point, one terminal, and a failed enqueue is still not
  admitted; `Full` and `Disconnected` follow §4.7 step 4. The #1192/#540 owner confirms or
  corrects this on #1311. **Still blocking:** the admission
  implemented and reviewed (if the synchronized cross-thread gate cannot be implemented, B21
  stays blocking), the #539/#540 wire and error-owner integration, and real-Windows AC11
  (v)–(viii) receipts with mutants (5)–(18), (20) and (21) RED, and that confirmation. Impact: until then AC11 (v)–(viii) cannot run,
  T3 is DO NOT MERGE and `executeJavaScript` does not ship on Windows. Check: the T3 PR names
  the entry point, passes the integration review, and attaches the AC11 (v)–(viii) receipts.

**Open questions:**

1. **Q1 (owner):** `Share::Served` = `READ | WRITE | DELETE` (recommended) or `READ | DELETE`,
   for **leaf** opens (Host-served responses, pass-through verification) only; the root pin never
   shares `DELETE` (A20).
2. **Q2 (owner):** dev stage validated by the package grammar at `keld dev` time so R8/R9
   refusals surface before serving (recommended, with a diagnostic naming the path).
3. **Q3 (`webContents` facade owners #539/#459, with the conformance owners #464/#567/#537):**
   Electron error texts for a sync throw and for a replacement; and which Electron major and
   platform the facade's oracle pins — #464/#567/#537 pin v44.4.5 (#537 records on macOS
   arm64), while §0's oracle source facts are v45.1.0. #1154 (the GH-467 contextBridge spec)
   does not own this surface.
4. **Q4 (#542/KEL-142):** `eval` destination signature on macOS now with a typed "not owned"
   refusal (recommended) or native completion semantics.
5. **Q5 (P4) partly answered:** E9 (with the switch, `self.origin "file://"`) and E10 `hs` (no
   switch, `self.origin "null"`) showed `'self'` admitting same-directory `file:` scripts and an
   exact `file:` host-source blocking its script;
   cross-directory in-root `'self'` (AC25 T2 cell) and the production baseline grammar remain
   open.
6. **Q6 UNKNOWN:** whether `keld dev` stages pass `validate_windows_package_paths` today and
   how `keld dev` reports a reparse component (OneDrive/dedup/WOF roots).
7. **Q7 (P2) UNKNOWN:** `IsErrorPage` for a Host `403`, `404`, `413` or `503` top-level response; DOCUMENT WRR
   count per pending window with the different-URI iframe cell and AC06's (c′)/(d′) mutants; 30 loads.
8. **Q8 UNKNOWN:** media `Range` requests under `file://` (AC23).
9. **Q9 DECIDED, values provisional (r17, §4.5):** W3's own file cap, reserved budget, in-flight
   cap and worker count, independent of any package member bound (`keld-pack` admits 8 GiB − 1).
   T2's AC31 measures the peaks and fixes the values; whether `keld-pack` should also warn at
   package time for an over-cap Host-served file stays with its owner.
10. **Q10 (P3):** Electron emission for script `history.back()`/`location.reload()`;
    `NavigationKind` census; `history.length`; form `POST`.
11. **Q11 UNKNOWN:** CSP-flag honoring cannot be self-tested (crdtp ignores unknown fields);
    enterprise DevTools/CDP policy effects on `CallDevToolsProtocolMethod`.
12. **Q12 UNKNOWN:** 8.3 generation state of the qualification volumes (E9's volume had no 8.3
    names, so 8.3 alias resolution was not exercised), and a short name set without `~` (§4.4's
    table). The case-sensitive directory check is decided (r17, §4.4, AC30); its read on a set
    flag, on inheritance, on ReFS and after a set on a pinned root stay UNKNOWN until AC30 (E16 never read a 1).
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
    until confirmed AC11 NC(4) is pre-declared "rule not exercised". KEL-143 is titled macOS
    ("macOS: preserve renderer and guarded API across Bun generation recovery", Linear,
    2026-10-30); Windows recovery exists at the pin as the harness
    `crates/keld-host/tests/no_flag/windows/recovery.rs`, so the answer must cover Windows.
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
    **whole prefix** in any letter case, found by enumeration (fail closed, §4.5 r16); the policy
    rule is rule (3).
22. **Q22 UNKNOWN (§4.6 rule 1, r11; widened r13):** (a) whether a `file:` navigation issued
    with `NavigateWithWebResourceRequest` exposes a Host-set custom header in
    `NavigationStarting`'s `RequestHeaders`, and none for a page `location.assign` to the same
    URI; (b) whether that header reaches the `WebResourceRequested` request headers of the same
    navigation's DOCUMENT request and never a frame's (§4.4 credit, B19); (c) the
    `NavigationKind`, `history.length`, generation change and later `onLoss` recovery of a header
    navigation to the committed URI used as `reload()`, compared with `Reload()` (AC08 m); and
    whether CDP `Page.navigate` (which returns `loaderId`) raises WebView2's
    `NavigationStarting`. T2 device cells with at-risk counts; until (a) and (b) pass, rule 1 (ii)
    governs (B18, B19); until (c) passes, `reload()` of a `file:` document is refused.
23. **Q23 UNKNOWN (§4.7 step 1, r11):** whether CDP always emits a main-frame document's
    `Page.frameNavigated` before that document's default `executionContextCreated`, including
    under delayed events and back/forward-cache restores. A census with an at-risk count above
    0, including old-renderer events after a cross-process navigation; if refuted, eval binding
    is withheld until a proven correlation replaces it (B17). r13 adds the **lag premise**: no
    main-frame `frameNavigated` of an earlier commit reaches the STA after a later navigation's
    `NavigationStarting` handler returned (§4.7 step 1 (b)); same census, same consequence.
24. **Q24 MEASURED, not guaranteed (§4.3 ownership, r12; probe E15, note 268 §4.zzzzzzz at
    `0monish/keld-research@c1d15a2`, run `gh1185-q24-wrr-order-2026-10-30`, sub-bundle `q24/`;
    unmerged like every run in PR #38, B9; 2026-10-30):**
    the STA order of a new document's first subresource `WebResourceRequested` against that
    document's `ContentLoading`. Microsoft documents only that `ContentLoading` "triggers before
    any content is loaded" (`ICoreWebView2::add_ContentLoading`), not the WRR order. FACT
    (WebView2 156.0.4314.50, Windows 10.0.26300.9711, one host; Host-served `file:` documents
    with classic script, stylesheet, preload and image subresources in a 1 MiB document):
    0 of 25,440 at-risk subresource WRRs were handled before their own document's
    `ContentLoading`, across link, `location.assign`, Host `Navigate`, reload, history
    (0 BFCache restores), interrupted and iframe navigations, and 0 of 12,840 more under a
    busy STA (8 ms and 50 ms stalls) and a posted-retirement control; the document's own WRR
    preceded `ContentLoading` in every navigation (the detector's positive control). The first
    subresource WRR of a fresh webview's first document followed `ContentLoading` by as little
    as 65 µs (median 116 µs, n = 32), so a retirement deferred to a later STA turn released
    14 of 14 such holds early under the old "generation at the WRR" key (rename-over
    succeeded, replacement delivered 9 times) and 0 of 21 under the next-generation key.
    AC26 (vi) forced the adverse order: old key released 90/90 and delivered the replacement
    90/90; next-generation key held 90/90 (rename refused, error 5) and delivered the verified
    bytes 90/90. The design does not depend on the order; zero natural inversions is a sampled
    observation on one runtime, not an ordering guarantee, and fetch/XHR/module/`@import`
    subresources were not sampled. T2 repeats the census on the shipped runtime.
25. **Q25 RESOLVED (#1192/#540 owner, 2026-10-30, [#1192 6530111661](https://github.com/gyldlab/keld/pull/1192#issuecomment-6530111661)):**
    conditional yes. The original authenticated Bun caller may observe a G₁ result after G₂
    commits only if the result crossed the final authenticated Host→Bun transport handoff
    before G₁'s revocation and before that role retired; the STA's `Pending → Delivered` mark
    is not that handoff. §4.7 step 4 and AC11 (v)–(viii) are revised (r14); the exact admission
    is B21. The decision approves the policy, not this spec or an implementation. r16: the
    owner's follow-up B21 decisions 1–5 are recorded at [#1311 6532409232](https://github.com/gyldlab/keld/pull/1311#issuecomment-6532409232)
    and applied in §4.7 step 4.
26. **Q26 UNKNOWN (§4.6 rule 1 (ii), r13):** causal attribution without the header. Candidates,
    not adopted: (1) a Host-created webview that has never committed a document has no principal
    that can raise a top-level `NavigationStarting` (premise INFERRED; windows adopted from
    `NewWindowRequested` have an opener and are excluded; an external file drop is a user
    navigation the premise must also exclude); (2) quiesce-then-load: the Host first commits a
    script-free `NavigateToString` document carrying a nonce and then issues its attempt from it
    (premises UNKNOWN: that `NavigationStarting.Uri` carries the `NavigateToString` content, that
    page script cannot navigate the top level to it, and that a script-free document raises no
    navigation; cost: one extra navigation and history entry per attempt). Adopting either needs
    a spec revision with device cells and an AC08 (l)-style RED mutant. Owner: W3 implementer.
27. **Q27 UNKNOWN (§4.4, r13; B19):** navigation-specific credit for page-initiated `file:`
    navigations. Candidates: (1) child-frame exclusivity — credit `n` only if exactly one
    parsed-equal DOCUMENT WRR arrived between `NavigationStarting(n)` and `ContentLoading(n)` and
    no child-frame `NavigationStarting` (`add_FrameNavigationStarting`, `bindings.rs` L1824) with a
    parsed-equal URI was raised in that window (premises UNKNOWN: every child-frame document,
    `<object>`/`<embed>` included, raises it, and it reaches the STA before its own DOCUMENT WRR
    and before `ContentLoading(n)`; a top-level request that raises no WRR defeats the count);
    (2) a per-response Host nonce header on DOCUMENT responses observed by CDP
    `Network.responseReceived` for the main frame (UNKNOWN whether CDP sees WRR-substituted
    `file:` responses; the `Network` domain's cost is unmeasured, cf. B14). Check: device cells
    with at-risk counts and AC06 (d) RED under the candidate's mutant. Owner: W3 implementer.
28. **Q28 UNKNOWN (§4.5 rule (3), r13e; B20):** what the allowed-writer rule cannot express. (a)
    That the folder checked is the folder the loader uses: how the Host learns the selected folder,
    and whether passing it as `browserExecutableFolder` binds the loader against a policy value
    (precedence UNKNOWN). (b) Strict roles that are not AppContainer/LPAC (#1187, Q20) and run
    under the launching user's SID: they fall in the same-user residual unless #1187 gives them a
    distinct SID. (c) Code the runtime loads from outside its tree (system DLLs are out of scope;
    anything else UNKNOWN). (d) The files the cost fallback leaves unchecked. (e) Whether the
    runtime's selection or update logic enumerates a parent folder or probes names that do not yet
    exist, which sets the ancestor boundary of rule (3) (r13g). Device cells with
    at-risk counts. Owner: W3 implementer with the KEL-66 and #1187 owners, before T2 merges.
29. **Q29 UNKNOWN (§4.5 residual 5, r17):** how long WebView2 keeps a response stream after
    `Complete`, and whether unread bodies keep it alive. Gate, before T2 merges: in AC31 (vii) the
    Host's private bytes peak stays below its idle value + the budget + one file cap. If it does
    not, T2 does not merge until the reservation lasts to the stream's final release (a
    release-notifying `IStream` wrapper around the memory stream: a new COM implementation under
    the unsafe gate and B10, still no streaming) or the owner records a written bound. Owner: W3
    implementer.
30. **Q30 UNKNOWN (§4.11, r17):** the Host cannot bound a CDP reply before WebView2 hands it the
    string; `maxDepth` bounds depth, not width, and B3's byte bound applies after receipt. T3
    measures a wide page value under AC14 and reports Host private bytes; a page-reachable peak
    over the budget of §4.5 goes to the codec and #540 owners (B3). Owner: W3 implementer.

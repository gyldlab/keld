# Spec: Windows WebView2 preload compatibility through a Host-enforced declared facade (GH-1184 W1, route D)

Status: draft (revision r15 answers 0monish's OCR r14 review of `6ff95f1b`: the Host answers a
number at most once while it is live, so a raw duplicate cannot settle the bridge's `send` or floor
`invoke`, and the bridge numbers past what a raw post took (§4.4 step 3a and its ordering table,
AC38, §10 Q17); `keld migrate` writes only GH-497's closed output set, and the `mainDigest` list
is KEL-19's read-only census (§4.8, AC39, §10 Q15, B10); with a whole-spec sweep of both defect
classes and of C1–C9;
revision r14 answers 0monish's OCR r13 review of `e9b95d0c`: `mainDigest` now
covers every declared preload, so another window's preload edit invalidates an attestation
(§4.8 rule 3, AC32); the Host checks a consumer `Refused` against #1190's registered codes and
forwards the registry's text (§4.4 rule 5, AC36, §10 Q16); the Host checks the floor's numeric
channel (§4.4 step 3', AC35); with a whole-spec sweep of every raw v2 field and every
page-supplied value that reaches Bun or a diagnostic (AC37);
revision r13 answers 0monish's OCR r12 review of `eae63da8`: any condition on a
path to a sink gates, not only a page-input branch (§4.8 rule 2, AC31); attestations and verdicts
also bind the main-process code inputs (§4.8 rule 3, AC32, §10 Q15); a forwarded message carries the
ticket and crosses the final admission fence W3's eval result uses (§4.4, AC33, proposed, §10
Q14); the bridge decodes before it dispatches, so a malformed value is `refused`, never
`consumed` (§4.3, AC34); with a whole-spec sweep of those classes and of C1–C9;
revision r12 answers 0monish's OCR r11 review of `73553fc2`: a stale same-root
`bind` the Host cannot tell from the current document's (new blocker §10 B9), a forged `consumed`
for a real delivery (reusable credit needs the page receipt and the Host-observed post, §4.4
rule 4), a send stranded by a Bun role retirement and a throwing listener that skipped its
`consumed` (§4.3 bridge terminals), with a whole-spec sweep for the same four defect classes;
revision r11 applies the #540 wire owner's Q1 decision, [#1307 comment 6532691875](https://github.com/gyldlab/keld/pull/1307#issuecomment-6532691875):
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
Owner: @amishabenramani (real Windows operator) · Updated: 2026-10-31

Code citations are at `origin/main` `457a9c3b37f71179f167bd1f8c706c461cc4c479`. W2 and W3
citations name the head each was read at; r13 re-read the task, blocker and rule citations at
#1280 `8c5e5d74` (r18) and #1311 `fa49dcfb` (r16): they hold there except for §6 T2's W3 and W2
task scopes, corrected in r13. Four W3 facts come from its unpushed r17 draft and are re-pinned when #1311
pushes it: the `413`/`503` capacity refusals, the per-view policy, the failed-enqueue split and the
shared ticket type. GH-497 is read at #1172 `2044f22e` (approved source contract, draft PR): neither
it nor its architecture 04 errata are on `origin/main` `0f512846`, whose architecture 04 §2 still
lists the pre-GH-497 outputs.
Device
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
receives no grants unless the developer attests, for that exact preload digest and that exact
main-code digest (§4.8 rule 3), that every flagged site narrows nothing. Without an
attestation nothing degrades silently, provided the recorded verdict is current — it is bound
to both digests and recomputed by release builds (§4.8 rule 3); with an attestation, a wrong
attestation silently loses the narrowing.
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
| [#1191 GH-552 exact channel grants](https://github.com/gyldlab/keld/pull/1191) (**open draft**) | AC1 opt-in `"$keldWindowChannelGrants": 1`; AC2 exact decoded `el:<c>` literal; AC3 Host-bound window selection; AC4 `RendererChannelQuery`/`RendererChannelPermit`; AC5 `KELD-GUARD003`; AC6 zero-allocation Allow inside the one `dispatch_privileged`; AC8 final identity recheck, with §4.4 step 2's identity refusal before a grant miss; T4 narrow `keld-core` forwarder. | Reused unchanged. Windows supplies `Principal::Webview` and the borrowed window grants; the forwarder is #1191 T4's, not a second one. |
| [#1192 GH-542 checked navigation binding](https://github.com/gyldlab/keld/pull/1192) (**open draft**) | §4.2 `NativeDocumentPhase` transitions and checked `u32` generation; §4.4 application-document eligibility (verified stage/package resource identity under #499/KEL-102, never URL text) and its zero-queue early `bind` ("zero queued Bind/Invoke/Send/Consumed", #1192 `cd30f997` §4.4, `docs/specs/gh542-native-document-dual-world-binding.md:133`); N11 commit tied to an allowed attempt; N15 Bun restart keeps the document; N16 one generation in stamp and principal; N21 Windows UNKNOWN. | Reused as the state and eligibility model; this spec is the Windows oracle. Divergence: one realm on Windows, so §4.3's two realm credentials do not exist; the epoch nonce is page-readable and is a correlation tag for the current document, not authentication (§4.2 rule 1, §10 Q1(c)). |
| [#1154 GH-467 facade](https://github.com/gyldlab/keld/pull/1154) (**open draft**) | §4.5 `IpcRendererFacade`/`ContextBridgeFacade`; §4.3 graph bounds (they are the only bounds; this spec adds none); §4.7 fail-closed early-call policy: void return, **zero queued work, no replay**, one typed diagnostic (Host-private in #1154; renderer-local on route D, divergence 11); C09 emitter semantics. | Reused as the one TypeScript facade; only the transport adapter is engine-specific. Divergence: same-realm `exposeInMainWorld` (no copy/proxy). |
| [#1190 GH-544 codec](https://github.com/gyldlab/keld/pull/1190) (**open draft**) | V1–V8; 4096-byte bound; synchronous refusal of functions before send; Host-opaque bytes. | Reused unchanged; the bytes travel as #540's `payload: JsonBytes` (a JSON array of byte values) inside the CompatV2 envelope, never base64url (§4.4). Its page-side refusals bind only the bridge: Bun's receiving decoder and argument adapter hold them against a raw post (AC37 (g)). Its registered codes, once allocated, are §4.4 rule 5's set (§10 Q16). |
| [#1155 GH-484 preload runtime](https://github.com/gyldlab/keld/pull/1155) (**open draft, macOS-only**) | §4.2 wrapper and `process` subset; AC05 throw handling; **AC08 SHA-256 source digest** verified outside the writable package tree. | Wrapper reused. The AC08 digest is reused as the key that binds classifier verdict and grants (§4.8). AC07/AC08's trust root is specified for macOS only; Windows has **no owner** (§10 B4). |
| [KEL-142](kel142-macos-renderer-bridge-api.md) §4.1 | `window.keld.invoke(channel: u16, payload)`; frozen; one pending invoke; routes exactly the declared Echo channel `1`. | Reused as the Windows public floor; admission in §4.4. §5.1 needs the amendment above. |
| [GH-540 spec PR #1122](https://github.com/gyldlab/keld/pull/1122) (**open draft**) `keld.wv-link` v2 | CompatV2 `RendererToNativeV2`/`NativeToRendererV2` forms (`d9211a96` §4.5); fixed `bound.limits` ceilings, "not fresh spendable credit" (§4.5); reservations and terminal release (§4.8); `decode_wv_link` stays the one Rust decoder. | Consumed as the **only** grammar, through that one decoder in CompatV2 mode; no v1 path, no added kind. Windows-specific, set by the #540 wire owner's decision (§10 Q1, RESOLVED, [#1307 comment 6532691875](https://github.com/gyldlab/keld/pull/1307#issuecomment-6532691875)): WebMessage string carriage; `bound` answers a verified `bind` (the landed macOS v1 request→response order); the `document` nonce is page-readable on route D; `send_records: 1` kept. The Windows carriage/handshake vectors are appended to #1122 §7.1, not a parallel owner (§7). |
| [GH-497 first-proof `keld migrate` output](https://github.com/gyldlab/keld/pull/1172) (**approved source contract; draft PR, not on main**) | M01 default analysis: zero writes and child processes; M02 `--write` preimage and rollback; M04 and the closed output map; M07 literal grants; M10 and the forbidden outputs: `keld.build.ts`, `keld.compat.ts`, the Bun alias and tsconfig/jsconfig Electron mappings are not generated (#1172 `2044f22e` `docs/specs/gh497-first-proof-migration-output.md:25-34,50-63,73`). | Binding on every migrate write here (§4.8 output contract). Its three-file set is consumed unchanged; W1's own manifest keys are not in its map (§10 B10). |
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
named here (a replayed message, a stamped post, a reply latch's release signal, a held Host
post (`bound` or `event`), a suppressed `route-state`, a held queued admission, a process termination, W3's AC31 budget barrier) is a debug-only `profile-test-hooks` fixture (`keld-wv` `Cargo.toml` features;
`src/lib.rs:3-4` refuses that feature in release builds; `keld-core` forwards it, its
`Cargo.toml:13`), the class W2 D16 uses; no new hook
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
| AC10 Navigation policy (rule A″) | Each E5 trigger (a, c, d, e1, f2, i, j, k, l, m, n) to an `about:` form and to an off-allowlist `https:` URL; with an allowlist whose only `file:` form is the literal `index.html` with no query, a page navigation to another in-root `file:` document and a page link to `index.html?x=1`; with an allowlist whose only `file:` form is the literal `p.html?mode=a`, page links to `p.html?mode=A`, `p.html?` and `p.html` (empty and absent query differ); and, **not yet run on device**, top-level navigations to `data:`, `blob:` and `javascript:` URIs in lower and upper case; and, for policy selection, two windows whose `w` bind different allowlists, one `https` target on the first's only, and a window whose `w` has no entry | Every page-initiated top-level `NavigationStarting` whose target is off the window's allowlist under rule A″ (for `file:`, decided on the parser's components with keld-guard's one component equality, and on the query: a parser failure, or a target that matches no `file:` allow form, is off-allowlist) has `Cancel` set at generation +0, and the application document and its IPC survive (E5: 5/5 veto per arm, 0/3 control, per trigger, including uppercase `ABOUT:BLANK`). On-allowlist navigations proceed (+1 only at commit); a page-initiated `file:` target is off-allowlist under W3 B19 until W3 Q27, so its on-allowlist cells read blocked until then. For `javascript:`, whether WebView2 raises `NavigationStarting` at all is UNKNOWN; if it does not, the script runs in the current document with that document's authority only, and the T2 receipt records which. Policy selection: each window decides by its own `w`'s allowlist, and the window with no entry cancels every page-initiated target at +0 (W3 AC08 (o) owns the value, this cell the selection). | Replace the allowlist with an exact denylist `["about:blank"]` → triggers k (`about:blank?`), m (`about:blank#keld`) and n (`about:blank#blocked`) replace the document → RED. Separately, treat in-root membership as sufficient (proceed whenever `file_url_root_relative` succeeds) → the in-root non-entry navigation proceeds under that literal-only allowlist → RED. Separately, cut the query before page-initiated matching → the entry-with-query link replaces the document → RED. Separately, fold the query through the component equality → the `p.html?mode=A` link proceeds → RED. Separately, select the policy by anything but the Host's `w` binding (the first entry) → the second window's off-allowlist target proceeds → RED. The three `file:` mutants are device-level rule not exercised while W3 B19 cancels every page-initiated `file:` target first; the same single faults run in W3's pure `guard_navigation_allow_forms` (#1311 `1db53482` §7, "Independent of B19's device-level cancel"), the entrypoint T2 consumes; the `about:` mutant flips on device. |
| AC11 Runtime flag untouched | Any build | `AdditionalBrowserArguments` stays empty; no `--disable-features` naming `msWebView2CancellableAboutNavigations`; cancellable `about:` navigation is never disabled. | Add the flag → AC10's `about:blank` control replaces the document → RED. |
| AC12 Popups | `window.open` with `_blank`/`target`, `opener` forms | `NewWindowRequested` → `SetHandled(TRUE)` (existing `guarded_navigation.rs:142-152`); no popup exists (E5 f1/f2: handled, 0 windows). | Drop the handler → unaccounted popup → RED. |
| AC13 Generated grants | `keld migrate` sees `ipcMain.on('a')`, `ipcMain.handle('b')`, `ipcMain.once('c')`, `ipcMain.handleOnce('d')`, `ipcMain.on(dyn)` and window W's preload exposes only the fixed-channel, fully forwarding method F-B1 (`doA: (...xs) => send('a', ...xs)`) | `windows.W.channels == ["el:a"]`, with `"$keldWindowChannelGrants": 1` and `windows.W.preload.digest`; `b`, `c`, `d` and `dyn` are reported ungranted. For the page-chosen-channel pass-through F-B (`send(c, ...xs)`) the result, only under §10 B10 (iii)'s amendment of GH-497 M07 and otherwise no grant and a report line, is `["el:a","el:c"]`: `send` reaches only `on`/`once` registrations, and `b`, `d` are reported ungranted as reachable only through `ipcRenderer.invoke`, which route D does not support (§4.8, r9). A flagged method on the same channel that omits arguments (`doA: () => send('a')`) with a matching attestation yields the same `["el:a"]`, never more. With `ipcMain.on('app:x')` added, an attested F-P (`prefixed: (c, ...xs) => send('app:' + c, ...xs)`) yields exactly `["el:app:x"]`, under the same condition. | Grant every `ipcMain` literal regardless of the preload's reachable sinks → `el:b` appears for W → RED (over-grant). Separately, collect only `on`/`handle` registrations → F-B's result lacks `el:c` and that `once` handler is unreachable → RED (under-grant; draw.io registers ten channels with `ipcMain.once`). Separately, treat F-P's computed channel as page-chosen (`*`) → its window is granted every `on`/`once` literal, `el:a` included → RED (over-grant past the recorded prefix). **Window keys (r7):** a fixture with two sites `new BrowserWindow({ title: 'Main', webPreferences: { preload: A } })` and `… preload: B …` must make `keld migrate` fail with the typed duplicate-key diagnostic naming both sites and write no `keld.config.ts`; a third fixture whose two `Main` sites share preload, navigation and CSP must produce one `t-main` entry listing both sites. Further refusal fixtures: titles `'Main'` and `'main'` with different preloads; a title whose slug is empty (`'!!'`); a computed title (`title: app.name`). Golden vectors for the key function run through `@keld/electron` and through `keld migrate`'s call of it. Mutants: disambiguate by suffix (`t-main`, `t-main-2`) → swapping the two sites in source and regenerating moves A's digest and grants to B's window (the config diff assigns `t-main` B's digest) → RED; keep the last site → `keld migrate` writes a configuration (the test expects refusal) whose one `t-main` entry carries a single site's digest → RED; drop the class prefixes → a site titled `'a js'` with preload `b.js` and a title-less site with preload `a.js` both map to `a-js`, and `keld migrate` refuses a fixture the test expects to yield two entries (`t-a-js`, `p-a-js`) → RED. **Sinks (r8):** with `ipcMain.on('danger')`, a preload whose only exposure is `(...xs) => ipcRenderer.sendToHost('danger', ...xs)` yields no `el:danger` and a typed incompatibility line (its run-time half, zero Bun calls when the page calls it, is AC27's); the same for a `sendSync`-only and an `invoke`-only exposure while #1154 does not implement them. Mutant: count `sendToHost` as a main-IPC sink → `el:danger` is granted → RED. **Dispatch kinds (r9, 0monish 4425538678):** with `ipcMain.on('a')` and only `ipcMain.handle('danger')`, a preload exposing `(c, ...xs) => ipcRenderer.send(c, ...xs)` yields `["el:a"]` with no `el:danger` and a report line naming `danger` as `invoke`-only. AC13 checks the generated manifest and report only; the run-time refusal (`KELD-GUARD003`) and the per-kind handler counters are AC27's. Mutant: treat every `ipcMain` registration kind as reachable by `send` → `el:danger` is granted → RED. |
| AC14 Digest binding | The admitted preload bytes' SHA-256 differs from `windows.W.preload.digest` | The Host treats W as `channels: []` and fails the window with a typed error naming the fix (`re-run keld migrate`); a matching digest loads the grants. The other branches of §4.8's comparison (r13): a window that passes no preload for an entry that declares one, and one that passes a preload for an entry that declares none, are refused typed with `channels: []`; a `w` with no entry gets `channels: []`, no preload and a typed diagnostic. | Ignore the digest → grants produced for another preload apply → RED (tracker negative control (b), AC22). Separately, compare only when a preload was passed → the preload-less window on an entry that declares one keeps its grants → RED. |
| AC15 Classifier gate | `keld migrate` over fixtures F-A..F-Z (§4.8). Every single-rule fixture forwards all other arguments by rest/spread, so exactly one rule can flag it | F-B, F-B1, F-B2 and F-O not flagged; every other fixture flagged, and the recorded `verdict.flags` names **only** the rule its row names; F-G1 flagged although representation-only (conservative; branch and delete); F-G (the combined draw.io shape) carries three flags. | Each mutant removes one rule: ignore every conditional on the path → F-A, F-R, F-S and F-Z pass → RED (F-D stays flagged, because its first argument is not forwarded by identity, and F-G1 by its delete); count only page-input-dependent branches (the r12 rule) → F-R, F-S and F-Z pass → RED, and F-A stays flagged; count only a conditional that encloses the sink → F-S passes → RED, and F-A and F-R stay flagged; count only statement conditionals → F-Z passes → RED; stop at the exposure's own body → F-Z1 passes → RED; judge each sink call alone → F-Z2 passes → RED; leave a sink with no exposure path out of the rule → F-Z3 passes → RED; ignore deferred paths → F-T and F-Y pass → RED; ignore `try`/`catch` → F-U passes → RED; ignore loops → F-V passes → RED; ignore selective exposure → F-W passes → RED; accept any argument expression as forwarded by identity → F-D passes → RED; treat a page-derived channel as pass-through → F-P passes → RED; treat omitted or truncated arguments as pass-through (the inverted former mutant) → F-C, F-J pass → RED; ignore constant arguments → F-H passes → RED; ignore writes, deletes and mutating calls on page-input values → F-I, F-Q pass → RED; ignore inbound values (rule (b)) → F-F, F-K, F-L, F-X pass → RED; count only filters that read the inbound value → F-X passes → RED; flag only non-literal `process.argv` reads → F-M passes → RED; treat dynamic `require`/`eval` as resolvable → F-E passes → RED; let the analyzer "prove" F-G1 representation-only by pattern → F-G1 passes silently → RED (conservatism falsifier); F-G stays flagged under every single mutant. Over-flag falsifier: flag every outbound sink call → F-B and F-B1 flagged → RED (F-B2 makes no outbound sink call). Separately, keep a branch the analyzer folded as a live condition → F-O flagged → RED. |
| AC19 Verdict and attestation binding | Window W's `preload.digest` is d₁ and the recorded `verdict.flags` is non-empty; the manifest holds (i) no attestation, (ii) an attestation bound to d₂ ≠ d₁, (iii) an attestation bound to d₁; separately, each with (iii)'s matching attestation so that one fault decides, (iv) `preload.verdict` is absent, (v) `channels` lists `el:x` not in `verdict.reachableSinks`, (vi) `verdict.digest` is d₀ ≠ d₁ (the developer edited the preload and hand-updated `preload.digest` and the attestation without re-running `keld migrate`), and (vii) `preload.digest == verdict.digest == d₁` but the recorded flags or sinks are stale (hand-edited, or produced by an older classifier). Every verdict and attestation in these cases carries the same `mainDigest`; that branch is AC32's | (i), (ii), (iv), (vi): the Host loads W with `channels: []` and a typed error whose fix says re-run `keld migrate` (and, for (i)/(ii), attest digest d₁); (iii): W loads with exactly the §4.8 **Grants** intersection; (v): W's grants are rejected (`KELD-GUARD005`-class), not filtered; (vii): a release build re-runs the classifier on the admitted bytes, finds a different verdict and refuses to package W with a typed error. | Accept an attestation by file path or window key instead of digest → case (ii) loads → RED. Separately, skip the ⊆-reachable-sinks check → case (v) grants `el:x` → RED. Separately, treat a missing verdict as "no flags" → case (iv) passes the verdict-presence check, counted there (its typed re-run-`keld migrate` refusal is absent; with an empty default `reachableSinks` the ⊆ check rejects the grants instead) → RED. Separately, skip the `verdict.digest == preload.digest` check → case (vi) loads stale grants → RED. Separately, skip the release recompute → case (vii) is packaged → RED. |
| AC16 Same-document stability | `pushState` to `?route=2#frag`, `replaceState` to `?route=3`, a hash change (`SourceChanged` with `IsNewDocument == FALSE`, source probe v2 3/3), cancelled provisional navigation | Epoch and generation unchanged; a pending granted call completes once; a new granted call after each change is admitted (its `Source` now carries the new query/fragment). | Rotate the epoch on `SourceChanged(IsNewDocument == FALSE)` → the pending call is refused → RED. |
| AC20 Identity is decided at commit, not per Source string | After `pushState` to `?route=2#frag` the page makes a granted call; its `Source` is `file:///…/index.html?route=2#frag` (source probe v2) — and the first call posted immediately after `pushState` may still carry the pre-push `Source` (`SourceChanged` arrived after it, 3/3) | Both calls are admitted: the epoch binds the committed document; the per-call Source rule checks the scheme, then the tuple origin (`https`) or empty host plus in-root path through the one parser (`file:`), never query/fragment equality. | Compare the full `Source` URL to the committed URL → the post-`pushState` call is refused → RED. |
| AC21 Host→renderer generation check (security control) | Document A's granted `invoke` is admitted; the Bun handler's reply is held until B's `ContentLoading` has been processed (deterministic test gate); B's first inline script installs a raw `chrome.webview` `message` listener that counts every post | The Host drops the reply because the target generation moved (§4.4 Host→renderer rule): B's raw-listener counter for posts stamped e₁ reads 0 and the Host's post counter for e₁ after B's `ContentLoading` reads 0. A reply whose target is still current is delivered once. | Remove the Host-side generation check → B's raw listener receives A's reply (counter ≥ 1) → RED (tracker negative control (e), AC22). The natural race between a post and a concurrent commit is not deterministic and is probed under §10 B7, arm H. |
| AC22 Route-D premise mutants (the tracker negative control) | One hostile page, five single-fault Host or `keld migrate` builds, each in its own process: (a) the classifier treats omitted arguments as pass-through; (b) the Host ignores the preload digest; (c) the literal-grant check is removed; (d) admission uses `Source` + generation without the nonce; (e) the Host-side generation check on Host→renderer delivery is removed | The unmodified baseline refuses every attempt with zero effects. Each forbidden effect is counted independently of the Host's refusal log: a Bun-side handler-entry counter keyed by channel and argument list for (a)–(d), and the page's raw `message` listener counter for (e). | (a) baseline (with `ipcMain.on('open-file')` registered): F-C is flagged, no attestation exists, W has `channels: []`; mutant: F-C passes unflagged, W is granted `el:open-file`, and the page posts a raw v2 `send` on `open-file` carrying `'<page path>'` directly → an `on('open-file')` listener entry with an argument the preload never sends → RED; (b) = AC14's mutant; (c) = AC02's mutant (the literal-grant step removed; AC09's prefix/case-fold mutant is a different fault and stays in AC09); (d) = AC06's mutant (A→A2 replay reaches Bun); (e) = AC21's mutant. AC03 (top-level event), AC05 (Source rule) and AC19 (grants ⊆ reachable sinks) carry the remaining premise mutants. The bridge's epoch check (AC08) is not one of them. |
| AC23 Isolation is never reported | (a) A fixture preload records `process.contextIsolated` and calls `contextBridge.exposeInMainWorld('probe', { isolated: process.contextIsolated })`; (b) `keld migrate` runs over F-O (§4.8), which exposes a privileged API only when `process.contextIsolated` is truthy and a limited one otherwise | (a) The first inline page script reads `window.probe.isolated === false`; the exposure succeeded (route-D divergence: in Electron, `exposeInMainWorld` throws when `contextIsolated` is false — §4.2). (b) `verdict.reachableSinks` is `["el:pub"]`: `process.contextIsolated` is constant-folded to `false`, so only the branch the application actually takes is counted; the report lists every preload read of `process.contextIsolated`, and an aliased or computed read is flagged under rule 2(d). | (a) Report `true` (the former Q3 recommendation) → the probe reads `true` → RED. (b) Count sinks from both branches → `verdict.reachableSinks` gains `el:priv` (and, with `ipcMain.on('priv')` registered, `el:priv` is granted) although the application chose the limited API → RED. |
| AC24 In-flight hold (state machine) | Pure Rust test of the Host→renderer delivery state machine with a stub engine (its `Navigate` record counts every Host navigation call, W3's header navigation included; the stub clock reaches `NAV_TERMINAL_BOUND` only in cases (10), (12) and (13)); cases (14) and (15) also drive W2's real barrier (`ProfileReleaseWait`; its `webview2` module is Windows-only, `keld-wv/src/lib.rs:104-105`): (1) `NavigationStarting(n)` → reply due → `ContentLoading(n)`; (2) `NavigationStarting(n)` with `Cancel` → reply due → `ContentLoading(n)` (E5 trigger b shape) → `NavigationCompleted(n, IsSuccess = FALSE)`; (3) `NavigationStarting(n)` → reply due → `NavigationCompleted(n, IsSuccess = FALSE)` with no `ContentLoading(n)`; (4) `NavigationStarting(n₁)` → `NavigationStarting(n₂)` → reply due → `NavigationCompleted(n₁, IsSuccess = FALSE)` → `ContentLoading(n₂)`, then a reply due for the new epoch; (5) `NavigationStarting(n)` → reply due → `SourceChanged(IsNewDocument = TRUE)` → `NavigationCompleted(n, IsSuccess = TRUE)` with no `ContentLoading(n)`; (6) `NavigationStarting(n)` → reply due → `SourceChanged(IsNewDocument = FALSE)` (an unrelated `pushState`) → `NavigationCompleted(n, IsSuccess = TRUE)` with no `ContentLoading(n)`; (7) `NavigationStarting(n)` → reply due → `SourceChanged(IsNewDocument = TRUE)` → `SourceChanged(IsNewDocument = FALSE)` → `NavigationCompleted(n, IsSuccess = TRUE)` with no `ContentLoading(n)`; (8) `NavigationStarting(n)` → reply due → `NavigationCompleted(n, IsSuccess = TRUE)` with no `SourceChanged` and no `ContentLoading(n)` (a same-URL restore shape); (9) `NavigationStarting(n₁)` → `NavigationStarting(n₂)` → reply due → `SourceChanged(IsNewDocument = TRUE)` → `NavigationCompleted(n₁, IsSuccess = FALSE)` → `ContentLoading(n₂)`; (10) `NavigationStarting(n)` → reply due → no terminal event until `NAV_TERMINAL_BOUND` (stub clock) → `NavigationStarting(n₂)` → `ContentLoading(n₂)` → reply due for the new epoch; (11) `NavigationStarting(n)` → reply due → `ProcessFailed(RENDER_PROCESS_EXITED)` → `NavigationStarting(n₂)` → `ContentLoading(n₂)` → reply due; (12) `NavigationStarting(n₁)` → `NavigationStarting(n₂)` → `ContentLoading(n₂)` → reply due for the new epoch → no terminal event for `n₁` until the bound; (13) `NavigationStarting(n)` → bound → `NavigationStarting(n₂)` → `ContentLoading(n₂)` → reply due → late `ContentLoading(n)` → a second reply due for n₂'s epoch; (14) `NavigationStarting(n)` → reply due → `ProcessFailed(BROWSER_PROCESS_EXITED)`; (15) `NavigationStarting(n)` → reply due → `ProcessFailed(BROWSER_PROCESS_EXITED)` for the bound browser → a `CreateWindow` (once the fault is latched W2 answers it `WindowBuildFailed { Webview }` without a build; under the receipt-only mutant below it builds and its controller reports a new browser pid) → late abnormal `BrowserProcessExited` receipt for the old pid | (1), (2), (4): the stub engine receives **no** post for the old epoch; the old epoch is revoked; in (4) the reply for the new epoch is delivered exactly once (liveness). (3), (6): the held reply is released once to the unchanged document, after the pending set is empty. (5), (7), (8): treated as a document change — revoke, FailedClosed, `onLoss`; no held reply released. (9): `n₁`'s completion is **not** a document change (the signal is attributed to `n₂`); no FailedClosed or `onLoss` from `n₁`; `ContentLoading(n₂)` revokes the old epoch; the stub receives no old-epoch post. (10): at the bound the stub records `Stop()`, the old epoch is revoked, the held reply discarded (0 stale posts), the document FailedClosed with `onLoss` count 0; after `ContentLoading(n₂)` the new epoch's reply is delivered exactly once. (11): at `ProcessFailed` no `Stop()` is recorded; the old epoch is revoked, the held reply discarded (0 stale posts) and the document FailedClosed with `onLoss` count 1; after `ContentLoading(n₂)` the new epoch's reply is delivered exactly once. (12): 0 posts to the old epoch; at the bound the new epoch is revoked, its held reply discarded and the document FailedClosed with `onLoss` count 0 — the stated cost of §4.4 rule 2 (iii). (13): the first reply is delivered once; at the late `ContentLoading(n)` the new epoch is revoked, and the second reply, stamped with it, is never posted. (14): no `Stop()`, `Navigate` or `Reload` is recorded; the epoch is revoked, the held reply discarded (0 stale posts), `onLoss` count 0, and the barrier latches `AbnormalExit` at the first `poll` after the `ProcessFailed`, whose handler queues an abnormal receipt (#1280 r16 D8 (`be9e9e56`), its AC21 NC 4 (j)), not at the later `BrowserProcessExited` receipt: session end = 1. (15): session end = 1 from the `ProcessFailed`; the barrier's binding never moves to the new pid (no rebind; the `CreateWindow`'s answer is W2's); the late old-pid receipt changes nothing; 0 posts. | Remove the hold → the stub receives the old-epoch post between `NavigationStarting` and `ContentLoading` in (1) → RED. Separately, start the hold only for navigations the policy allowed → (2) emits the post → RED. Separately, end the hold at the first completion → (4) emits the old-epoch post after `NavigationCompleted(n₁)` → RED. Separately, release held posts on any completion without `ContentLoading` → (5) emits the post → RED. Separately, release whenever a `SourceChanged(IsNewDocument = FALSE)` arrived, ignoring `IsNewDocument = TRUE` (the r4 rule) → (7) releases the held reply to the new document → RED. Separately, release unless a `SourceChanged(IsNewDocument = TRUE)` arrived (the r5 rule) → (8) releases the held reply on a completion that raised no `SourceChanged` → RED. Separately, count every new-document signal since `NavigationStarting(n₁)` regardless of attribution (the r6 rule) → (9) raises FailedClosed and `onLoss` for a window whose successor commits → RED. Separately, remove the bound → (10) keeps the set non-empty after the stub clock passes `NAV_TERMINAL_BOUND` and the new epoch's reply is never delivered → RED. Separately, ignore `ProcessFailed` → (11) the same → RED. Separately, apply `onLoss` at the bound → (10)'s `onLoss` count is 1 → RED. Separately, let `ContentLoading(n₂)` end older pending navigations (supersession) → (12) delivers the new epoch's reply before the bound → RED. Separately, drop the expired `NavigationId` at the bound and ignore a `ContentLoading` for a navigation not pending → (13) posts the second reply, stamped with the new epoch, after the late `ContentLoading(n)` → RED. Separately, treat `BROWSER_PROCESS_EXITED` like `RENDER_PROCESS_EXITED` (the r9 rule) → (14) records a `Navigate` on the closed controller and `onLoss` count 1 → RED. Separately, end the session only on a pid-matched abnormal `BrowserProcessExited` receipt (the #1280 `95594e88` D8 (3) rule; a fault in W2's barrier, whose own control is #1280 r16 AC21 NC 4 (j), run here as an integration check) → in (15) the `CreateWindow` builds and rebinds to the new pid, the old receipt is foreign and session end stays 0 → RED. |
| AC25 Which documents get the app preload and an epoch | Admitted root `D:/Keld Test/Ünï/`, entry `index.html`, allowlist `in-root:**` plus one remote `https://` scope. The application's `loadURL` (a Host attempt, admissible for the entry's components whatever the spelling, §4.6 item 6) loads (a) the entry `file:///D:/Keld%20Test/%C3%9Cn%C3%AF/index.html`, (b) `…/index%2Ehtml`, (c) `…/INDEX.HTML` and (d) the non-ASCII case variant `file:///D:/Keld%20Test/%C3%BCn%C3%AF/index.html`; the page then navigates top-level to (e) another in-root page `…/page2.html`, (f) a missing in-root file (Host `404`), (g) the allowlisted remote document and (h) an in-root junction (Host `403`). (e), (f) and (h) are page-initiated `file:` targets, which W3 cancels at +0 until its Q27 (B19): those cells read blocked, never pass, until then. Separately, (i) the application's `loadURL` of the entry while W3's AC31 barriers hold the Host-served budget full (Host `503`), and a second run in which `onLoss: reload-entry` recovers a lost document into that full budget. Each document's first inline script records whether the preload's exposed keys exist (AC01's oracle) | Each cell records the committed `Uri`/`Source` bytes, `location.protocol`, the first-inline result and the Host's `bound`-post counter. T2 runs the epoch, `onLoss` and pure-Rust cells; the preload-API cells need T4's wrapper and run there. (a)–(e): the preload API is present at the first inline script. (a)–(d) receive an epoch; (e) receives one under B3's proposal (§4.5), and under B3's fallback it has the API, no epoch, every call refused not-ready and no `onLoss`. (g): the API is absent and no epoch is minted and no `bound` is posted. (f), (h): whether a Host `404`/`403` refusal commits as a Chromium error page or as an empty `file:` document is UNKNOWN (W3 Q7), so both outcomes are pre-declared and the receipt records which occurred: as an error page, the API is absent; as a `file:` document, the API is present (gate (b′)) and every call is refused not-ready. Under either outcome no epoch is minted and no `bound` is posted and the document is FailedClosed **without** `onLoss` (§4.5). Any epoch or `onLoss` in (f) or (h) is RED. (i): each attempt fails to its caller typed and is not re-issued, no epoch is minted and no `bound` is posted, the document stays FailedClosed, and neither `onLoss` nor the recovery count moves for it, whatever `ContentLoading` reports (W3 Q7, W3 B7 (xi)). In no cell does an epoch exist without the API. Pure Rust: keld-guard's one component equality holds for `index.html`/`INDEX.HTML`/`index%2Ehtml` (after the parser) and W3's allow-form matching and W1's Host-attempt admission call that same function. | Gate the preload by a `location` string (the r5 key: scheme + authority + path, ASCII-folded, against `ENTRY_KEYS`) → (d) lacks the API (and (b) too while Chromium keeps `%2E`) although it receives an epoch → RED (the defect rpipaliya reported). Separately, run the preload in every top-level document (option (b)) → (g) has the API → RED. Separately, compare components byte-exactly in W1 instead of calling the shared equality → the pure-Rust `INDEX.HTML` vector is unequal to the entry → RED. Separately, attach W3's served record regardless of its status → (f) receives an epoch → RED; while (f) is blocked on W3 Q27 this mutant is decided by `wv2_epoch_mint_predicate`'s status inputs (§7). Separately, count a capacity refusal as a loss → (i)'s recovery count rises and a further reload is issued → RED. |
| AC26 One wire grammar and carriage: #540 CompatV2 over WebMessage strings (r9, 0monish 4425538690; r11, §10 Q1(a); T2) | T2's **actual** injected bridge text (§4.3) runs in a qualified document against the **actual** `keld-wv` `decode_wv_link` in CompatV2 mode behind §4.4's admission path (top-level `WebMessageReceived`, `TryGetWebMessageAsString`, strict bounded UTF-16→UTF-8), with the KEL-142 floor (`window.keld.invoke` on Echo channel `1`). A recorder in the bridge logs its listener installation, every post and every inbound message in the renderer; the Host counts decoded envelopes, `bind` receipts, `bound` posts and Echo calls. Fixture vectors (WebMessage STRING literals, ASCII byte counts, no outer JSON string wrapper at the decoder; appended to #1122 §7.1, §7): `bind`, 21 bytes, `{"v":2,"kind":"bind"}`; `bound`, 174 bytes, `{"v":2,"kind":"bound","document":"0123456789abcdef0123456789abcdef","limits":{"payload_bytes":4096,"channel_utf8_bytes":256,"send_records":1,"event_records":1},"ready":false}` | Observed in the renderer and at the Host (the order's own cells are AC29): the listener is installed before the first post; the first post is byte-equal to the 21-byte `bind` vector; the first inbound message is the one `bound` the Host posted after decoding that `bind`, whose `document` is the current epoch (32 lowercase hex characters); in pure Rust the Host's `bound` encoder, given the fixture document and `ready: false`, emits the 174-byte `bound` vector byte for byte, and `decode_wv_link` decodes the `bind` vector to exactly one `Bind`. "`bound` may arrive before `bind`" is not a success oracle (§10 Q1(b)). one floor `invoke` posts one v2 `invoke`, receives one `invoke-result` and resolves; the next floor `invoke` is issued only after that `invoke-result`. Separately, raw page posts carrying the current epoch — `{"v":2,"kind":"hello",…}` (r8's kind name in a v2 envelope), a v1 `invoke` on channel `1`, a v2 `send` whose `payload` is a base64url string, a v2 envelope of kind `epoch`, the `bound` vector carrying the current `document` (wrong direction), a v2 `send` with a duplicated `document` field, and a non-string `chrome.webview.postMessage({"v":2,"kind":"invoke",…})` object (`TryGetWebMessageAsString` fails) — are each refused, by the carriage step or by `decode_wv_link`, with a typed reason; for them the decoded-envelope, `bind`-receipt and Echo counters stay 0, and a floor `invoke` after each still resolves. | Endpoint admits v1 (a LegacyV1 fallback) → the v1 `invoke` reaches the Echo handler → RED. Separately, the decoder also accepts a base64url string payload → the decoded-envelope counter counts that `send` → RED. Separately, the decoder admits an unknown kind → the v2 `epoch` post is counted as decoded → RED. Separately, the decoder aliases `hello` to `bind` → the v2 `hello` post is counted as a `bind` receipt → RED (the LegacyV1 mutant flips neither cell: both are v2 envelopes whose kinds lie outside #540's closed union, and a v1 decoder refuses unknown kinds too, `wv_link.rs:195`). Separately, run r8's v1 bridge text → `decode_wv_link` refuses every post and the floor `invoke` never resolves → RED (fails first). Separately, read `get_WebMessageAsJson` instead of `TryGetWebMessageAsString` → every string post reaches the decoder in its JSON-quoted form and is refused, so the bridge's `bind` is never answered and the floor `invoke` never resolves → RED. Separately, the decoder accepts native-to-renderer kinds from the renderer → the page-posted `bound` vector is counted as decoded → RED. Separately, the decoder lets a duplicated key overwrite the first → the duplicated-`document` `send` is counted as decoded → RED. |
| AC27 Facade send trace, dispatch kind and role capacity (r9; r11, §10 Q1(d); T4) | T2's bridge and admission path, T3's forwarder and T4's facade; `el:a` and `el:x` granted, `el:danger` not. Bun runs #466's `ipcMain` listener registry (`on`/`once`; #1122 §5) and #547's separate invoke-handler registry (`handle`; #466 puts `handle/handleOnce/removeHandler` out of scope as F04-T14, which #547 owns), with `ipcMain.on('a')` (its listener sends one event on `a` back), `ipcMain.on('x')` **and** `ipcMain.handle('x')`, and only `ipcMain.handle('danger')`; each listener and handler has its own counter. The page calls exposures that use `ipcRenderer.send`; the recorder of AC26 logs the renderer side. Fixture vectors (appended to #1122 §7.1, §7): `send`, 123 bytes, `{"v":2,"kind":"send","document":"0123456789abcdef0123456789abcdef","message":1,"channel":"rendererReq","payload":[0,1,255]}`; `consumed`, 120 bytes, `{"v":2,"kind":"consumed","document":"0123456789abcdef0123456789abcdef","delivery":1,"disposition":{"status":"consumed"}}`. Cross-window cell: two windows of one application on one authenticated Bun role, both granted `el:a`; `on('a')`'s completion for window 1's send is held behind a test latch, so its `send-result` is not yet posted. Restart cell (r12, 0monish 4428451737): window 1 posts message 1 (`send('a')`) and a floor `invoke`, and `on('a')`'s completion is held behind the test latch, so neither result is posted; the harness terminates the Bun role process (a `profile-test-hooks` process termination); the Host records the old role's quiescence under #540 §4.9 (old queued, active and continuation counters 0) without posting message 1's `send-result`; the supervisor's successor role becomes ready. Throwing-listener cell (r12, 0monish 4428451744): the preload registers `ipcRenderer.on('a', () => { throw Error('boom') })`, the page counts `window` `error` events, and two `send('a')` calls, each after the previous `send-result`, make `on('a')` send two events | Pure Rust: the bridge's builders emit, and `decode_wv_link` decodes to exactly one variant, the 123-byte `send` and 120-byte `consumed` vectors byte for byte. Observed in the renderer, per direction only (#1122 AC8 promises admission order per document and direction, no cross-direction order). Outbound: `bind`, `send` (message 1, channel `a`), one `consumed` for delivery 1. Inbound: `bound` (answering the `bind`), then `send-result` for message 1 and `event` delivery 1 in either order. The `on('a')` listener fires once; the #540 ledger counter records the one `consumed`. A page `send('a')` made while message 1 is outstanding is a dropped call (divergence 11): it returns `undefined`, emits exactly one typed local `recordBusy` diagnostic, posts no v2 `send`, runs no Bun handler and consumes no credit; the next one, issued after message 1's `send-result` (`bound.limits.send_records` is 1), is posted as message 2. A `bound` with `ready: false` (the fixture holds the Bun route not ready behind a test latch when the document commits) makes calls refuse not-ready with no post until `route-state: ready`; `route-state: retired` makes them refuse not-ready again. `send('x')`: the `on('x')` counter is 1 and the `handle('x')` counter is 0. `send('danger')` from the exposure, and a raw v2 `send` for `danger` with the current epoch: both `KELD-GUARD003`, every `danger` counter 0. AC13's r8 exposures (`sendToHost`, `sendSync`-only, `invoke`-only) produce zero Bun calls. Cross-window: while window 1's message 1 is outstanding, window 2's bridge is locally free (its `outstanding` bit is false) and posts its `send('a')`; the Host/role ledger (#540 §4.8: one DATA record per direction across the role) refuses that reservation: the `on('a')` counter stays 1, window 2's bridge receives that message's `send-result` with disposition `refused`, code `KELD-WV-011` (#540's busy row, #1122 `d9211a96` `docs/specs/gh540-compat-ipc.md:603`, §4.4), emits one typed local diagnostic and clears `outstanding`, and window 2's page sees no throw; after window 1's `send-result` window 2's next `send('a')` is admitted once. Restart: the bridge receives `route-state: retired` and no `send-result` for message 1; it emits exactly one outcome-unknown diagnostic, posts nothing, replays nothing and refunds nothing (message 1's ledger charge is released by the Host at quiescence, never by the bridge); the floor `invoke` rejects with the typed local `routeRetired` error; a `send('a')` while retired is refused not-ready with no post; after the successor's `route-state: ready` a fresh `send('a')` is posted once as message 2 and admitted exactly once under the ledger's record (the `on('a')` counter rises by 1), and a floor `invoke` resolves. Throwing listener: per event the listener runs once, the page's `error` counter rises by 1 with message `boom`, and the bridge posts exactly one `consumed` for that delivery with disposition `consumed` (#1122 `d9211a96` `docs/specs/gh540-compat-ipc.md:317`); after both events the ledger's release counter is 2 and no event record stays charged. Refused send: after `send('danger')`'s `KELD-GUARD003` the bridge receives that message's `send-result` with the `refused` disposition, and the next `send('a')` is posted. | Merge #547's invoke-handler registry into #466's listener registry, so a `send` also reaches `handle` handlers (the case #466's registry-separation negative names) → the `handle('x')` counter is 1 → RED. Separately, the bridge skips its `outstanding` check → the early second `send('a')` is posted → RED. Separately, the bridge never clears its outstanding record on `send-result` → message 2 is never posted → RED. Separately, the bridge ignores `bound.ready` → a post appears before `route-state: ready` → RED. Separately, the bridge drops the `consumed` → the ledger counter stays 0 → RED. Separately, the Host keys the DATA ledger per document instead of per role (a per-window quota) → window 2's `send('a')` reaches `on('a')` while window 1's is outstanding (counter 2) → RED. Separately (restart), the bridge clears `outstanding` only on `send-result` → after the successor's `ready`, `send('a')` is a local `recordBusy` and no message-2 post appears: permanently busy while ready → RED. Separately (throwing listener), post `consumed` only after a normal return from the emitter → the first delivery gets no `consumed`, the release counter stays 0 and the second event is not posted while the record stays charged → RED. Separately, swallow the listener's exception → the page's `error` counter stays 0 → RED. Separately, the Host posts no `send-result` for a `send` it refused at the grant step → the next `send('a')` is a local `recordBusy` → RED. Separately, the Host refuses the role-quota reservation without a `send-result` → window 2's next `send('a')` is a local `recordBusy` → RED. |
| AC28 Process-failure split (r10, 0monish 4427055014; T2) | A compat window holds a current epoch and one floor `invoke` reply due (T2's floor bridge). (a) The harness terminates the window's main-frame renderer process; (b) in a separate run it terminates the browser process (the pid W2's D16 `REBIND` line names); both mechanisms are T2's `profile-test-hooks` fixtures. The harness records `ProcessFailedKind`, every navigation event, every engine call the Host makes per controller, the Echo handler counter and how the session ends | (a) `RENDER_PROCESS_EXITED`: the old epoch is revoked and 0 stale posts are issued; `onLoss: reload-entry` makes exactly one trusted load on the same surviving control (for `file:`, W3's header navigation, so only in W3's header-proven mode) and `onLoss` count is 1; a new `ContentLoading` mints a new epoch, which the new document receives in the `bound` answering its `bind` (AC29 (h)); a floor `invoke` after it reaches the Echo handler once (a real reload). The runtime replaces the crashed frame's content with an error page (Microsoft, §4.5); whether that raises navigation events, and in which order against the trusted load, is UNKNOWN, so the receipt records every navigation event between the `ProcessFailed` and the new epoch, and a second `onLoss` for one exit is RED. (b) `BROWSER_PROCESS_EXITED`: every epoch is revoked; after the failure there are zero `Navigate`, `Reload` or other engine calls on the closed controller; the session ends exactly once with W2's `AbnormalExit` terminal (`KELD-WV-009`). The order of `ProcessFailed` and `BrowserProcessExited` is not guaranteed, so the receipt records which came first: `ProcessFailed` first latches the terminal at the loop's next `poll` under #1280 r16 D8 (`be9e9e56`), the receipt first latches it as on main, and the later event changes nothing. Zero Echo handler entries follow (defence in depth: a dead browser delivers no post). | Apply `onLoss` after a browser exit (the r9 rule) → a Host navigation call on the closed controller is recorded → RED whenever `ProcessFailed` reaches W1 before the session ends; the runtime picks that order and no hook forces it, so this cell is a positive control, not the decider: AC24 case 14 decides the rule. Separately, treat a renderer exit as a browser exit → (a) records no trusted load and `onLoss` count 0 → RED (W2's latch reads the kind itself, so the session need not end). |
| AC29 Startup order: `bound` answers a verified `bind` (r11, §10 Q1(b); T2) | T2's actual bridge and admission path on the KEL-142 floor; the AC26 recorder plus Host counters for `bind` receipts (with their `Source`), `bound` posts (with their target document and the `bind` they answer), mints, typed startup failures, retries and Echo calls. Cells: (a) positive: the application's `loadURL` of the entry; (b) delayed listener: a fixture bridge build defers its start (listener, then `bind`) to a later task (`setTimeout`; the delay is recorded and the oracle is the Host's order, not the delay); (c) no listener: a fixture bridge build installs no listener and therefore never posts `bind`; (d) early `bind`: on the window's first navigation the replay hook delivers a recorded `bind` with the entry's `Source` after the entry's `NavigationStarting` and before its `ContentLoading`, and a fixture bridge build posts no `bind` of its own; (e) stale `bind`: A (an allowlisted remote `https` document) → B (the entry, by the application's `loadURL`); the replay hook delivers A's recorded `bind`, with A's `Source`, after B's `ContentLoading` and before B's own `bind`; (f) duplicate `bind`: after `bound`, the page posts the raw 21-byte `bind` vector twice; (g) navigation between `bind` and post: A = the entry, B = the entry again by the application's `loadURL`; a Host test hook holds A's `bound` after A's verified `bind` until B's `ContentLoading` has been processed, so only §4.4 rule 1 stands between it and the post (as in AC21); B's first inline script installs a raw `message` counter; (h) recovery: `ProcessFailed(RENDER_PROCESS_EXITED)` with `onLoss: reload-entry` (AC28 (a)); (i) stale same-root `bind` (r12, 0monish 4428451720), in two runs: A = the entry and B = another in-root document (eligible under B3's proposal; under its fallback a second declared entry), each loaded by the application's `loadURL`; and a same-URL reload, A = B = the entry. The replay hook holds A's recorded `bind`; B's bridge is (b)'s deferred fixture build, so B's listener does not exist yet; after B's `ContentLoading` the hook delivers A's `bind`, with A's `Source`; then B's bridge installs its listener and posts its own `bind`. Separately, without the hook, A posts `bind` at its latest points under B7 arm F's forcing (`pagehide`/`unload`, the busy-loop burst), and the receipt records the native at-risk count: how many A `bind`s the Host receives after B's `ContentLoading` (UNKNOWN; a count of 0 reads "rule not exercised") | (a) The Host posts no `bound` before it decodes the document's verified `bind`, then exactly one; the first inbound message is that `bound`; a floor `invoke` then resolves. (b) Zero `bound` posts until the deferred `bind` is decoded, then exactly one, received by the recorder; the floor `invoke` resolves. (c) Zero `bind` receipts and zero `bound` posts; at the startup deadline (§4.4) the window fails with the typed startup error exactly once; zero retries; never reported as success. (d) The early `bind` mints nothing, buffers nothing and is never answered: zero `bound` posts after `ContentLoading`; the window fails at the startup deadline with the typed startup error exactly once, no retry. (e) A's `bind` is refused by the Source rule: zero mints and zero `bound` posts until B's own `bind`, then exactly one `bound` for B. (f) Zero further `bound` posts and zero mints; the epoch and the role's #540 ledger counters are unchanged. (g) A's `bound` is dropped at the final `PostWebMessageAsString` check (B's raw counter for posts carrying A's `document` reads 0); A's startup route is retired; B receives its own `bound` only after its own `bind`. (h) The recovered document's `bind` is answered with one new `bound` whose `document` differs from the pre-crash one; a floor `invoke` after it reaches the Echo handler once. (i) A's `bind` consumes no B state: zero B-latch consumption and zero `bound` posts caused by it (under §10 B9 branch (ii) one identical re-post of B's `bound`, same `document` and `limits`, is counted and allowed), zero mints, zero replenishment and zero role change; B's own `bind` is answered, never refused as a duplicate; B's recorder receives exactly one usable `bound`, carrying B's `document`, and a floor `invoke` reaches the Echo handler once. Today's one-use latch cannot meet this, because the Host cannot tell A's `bind` from B's (§4.4), so the cell fails first and T2 stays blocked on §10 B9. | (a) Treat `bind` as a note that is never answered (the r10 rule) → no `bound` arrives and the floor `invoke` never resolves → RED. (b) Post `bound` at `ContentLoading` instead of in reply to `bind` (the r10 order) → the Host's log shows a `bound` post before any `bind` receipt for that document → RED (the receipt also records whether the recorder received that pushed `bound`; whether `chrome.webview` delivers a post to a listener added in a later task is UNKNOWN, and E5's 24/24 push deliveries do not decide it). (c) Remove the startup deadline → the window stays open with no `bound` and no typed failure → RED. (d) Hold an early `bind` and answer it at the next qualifying `ContentLoading` → a `bound` is posted with no post-qualification `bind` → RED. (e) Skip the Source check on `bind` → a `bound` is posted in reply to A's `bind` before B's own → RED. (f) Answer every verified `bind` with a fresh mint → each duplicate produces a further `bound` with a new `document` → RED. (g) Skip the final-post check for `bound` → B's raw counter for A's `bound` reads 1 → RED. (h) Keep the once-per-document `bind` latch per WebView instead of per document → the recovered document's `bind` is refused as a duplicate, no `bound` arrives and the floor `invoke` never resolves → RED. (i) Accept the old event: A's `bind` takes B's one-use latch (today's rule, so the cell fails first) → B's own `bind` is refused `DuplicateBind` (Host refusal counter 1) → RED; whether B also loses its `bound` depends on whether a post made before the listener exists is delivered (UNKNOWN, recorded). |
| AC30 Page-readable nonce grants nothing (r11, §10 Q1(c); T4) | A hostile page reads `bound.document` and every raw Host post through its own `chrome.webview` `message` listener (route D, §4.2) and uses the current nonce in: (a) a wrong-document envelope (a nonce not byte-equal to the current one) and (b) a stale-epoch replay (both AC06); (c) a foreign-frame post (AC03); (d) raw v2 envelopes that bypass the preload (AC02; AC26's raw posts; AC27's raw `danger` `send`); (e) a grant bypass (AC09); (f) forged receipts on the AC27 harness (r12, 0monish 4428451730): (f1) a matching key before native completion: the held-post hook holds delivery 1's `PostWebMessageAsString` (the `on('a')` listener's event) after its record is reserved, the page posts a byte-valid `consumed` with the current `document`, `delivery: 1` (a predicted key, §4.4 rule 4) and disposition `consumed`, and main then sends a second event on the same role; (f2) a matching key after native completion, during dispatch: delivery 1 is posted, a page-supplied `ipcRenderer.on('a')` listener, which runs inside the bridge's dispatch and so before the bridge's own `consumed`, posts the matching `consumed` (the predicted key; a raw `message` listener would run after the bridge's, whose listener is installed first, INFERRED from `EventTarget` order) and then runs a fixed synchronous loop (its duration recorded) before returning, and main sends a second event; (f3) a double `consumed`: in (f2), the bridge's genuine `consumed` for delivery 1 follows the page's; (f4) an unissued key: while delivery 1 is outstanding, the page posts a `consumed` for delivery 2, which was never reserved; (f5) a dropped post, no forgery: the held-post hook holds delivery 1's post after its record is reserved and the application's `loadURL` commits a new document before the release; (f6) a replaced document: a fixture bridge build posts no `consumed` for delivery 1 after its post, and the application's `loadURL` commits a new document | (a)–(e): exactly the zero-effect observables of the ACs named, unchanged. (f1): the forged `consumed` releases nothing (release counter 0; delivery 1's record stays charged); the second event is not admitted while delivery 1 is held (zero posts for delivery 2; Host event-record high-water 1); after the hook releases delivery 1, the bridge's genuine `consumed` releases it once and the second event follows. (f2): the page's `consumed` arrives after the native half, so it releases the record as a legitimate immediate `consumed` would and the second event may be posted; the recorder logs delivery 1's dispatch start and end and delivery 2's dispatch start, and delivery 2's dispatch is expected to start after delivery 1's ends (JavaScript run-to-completion, INFERRED; a WebView2 nested event loop is UNKNOWN and the receipt records it). No Host rule decides (f2): it records the stated limit that no Host-observable signal shows a page listener finished (§4.4 rule 4). (f3): delivery 1's release counter is exactly 1; the bridge's later `consumed` releases nothing. (f4): the release counter is unchanged and delivery 1 stays charged until its first exact-key `consumed` after its post. (f5): rule 1 drops the post, the Host ends the record through #540's own terminal (§4.4 rule 4), the release counter is 1 with no page `consumed`, and the new document's first `event` is posted. (f6): at that `ContentLoading` the Host ends the record as `route-retired`, `outcome-unknown`, the release counter is 1 and the new document's first `event` is posted. Page acknowledgments are never authority. No nonce match is counted as preload provenance or an isolated world. | (a)–(e): the mutants of AC06, AC03, AC02, AC26, AC27 and AC09, each in its own process. (f1): release on an exact-key page `consumed` alone, before the Host-observed native completion → the second event is reserved while delivery 1 is still held, and after the hook's release the Host event-record high-water reads 2 and the page receives both events → RED. (f3): release on every exact-key `consumed` after the native half → the bridge's later `consumed` releases delivery 1 a second time (release counter 2; the role's event capacity reads 2) → RED. (f4): release on any `consumed` whose `document` matches, without the exact ledger key → the delivery-2 `consumed` releases delivery 1's record (release counter 1) → RED. (f5): release a dropped post only on a page `consumed` → the record stays charged and the new document's first `event` is never posted → RED. (f6): keep the record for a page `consumed` after its document is gone → the new document's first `event` is not posted until #540's 5,000 ms missing-receipt bound retires the route → RED. (f2) has no single-fault Host mutant (rule not exercised by the Host). |
| AC31 Private-state gate (r13, 0monish 4429155170; golden half T5, device half T6) | Fixture F-R: `unlocked` is preload-private and only a trusted `click` sets it; `ipcMain.on('delete-all')` is registered; window W has no attestation. No trusted click happens. On device the page calls `api.deleteAll()`, then posts a raw v2 `send` on `delete-all` with the current epoch after that call returned | Golden (T5, `keld migrate` output): F-R is flagged (a), `verdict.flags` names the site, `channels` is `[]` and the report names the site. Device (T6, T5's generated manifest on T4's facade): the exposed call runs 0 `delete-all` handlers; the raw `send` is denied with `KELD-GUARD003` and a `send-result` with disposition `refused`, 0 `delete-all` handlers. With an attestation for both digests W is granted `el:delete-all` and the raw `send` runs 1 handler: that is the disclosed attestation residual (§4.8), recorded in the receipt, not a pass. | Count only page-input-dependent branches (the r12 rule) → F-R passes unflagged, W is granted `el:delete-all`, and the raw `send` produces 1 Bun entry → RED. |
| AC32 Main-code binding (r13, 0monish 4429155174; r14, 0monish 4429800825; golden half T5, Host half T3, device half T6) | One preload `save: (path) => send('save', path)` (flagged (a), truncated) with a site attestation for main code digest m₁. v1 main: `ipcMain.on('save', (_e, path) => save(path))`, set digest m₁. v2 main: `(_e, path, overwrite = false) => save(path, overwrite)`, set digest m₂, preload bytes unchanged. Cells: (a) v2 packaged with the v1 manifest; (b) `keld migrate` re-run on v2 with no re-attestation; (c) the manifest of (b) with `channels` kept at `["el:save"]` (T3 fixture manifest; T6 on device), and a raw v2 `send` on `save` carrying `[path, true]`; (d) the developer re-attests for m₂; (e) pure: one byte changes in a main helper module, in a renderer bundle, in an input with no script extension (`handler.dat`), in a declared preload and in `keld.permissions.jsonc`; two listings of one set in different orders; `keld migrate` run twice over one unchanged tree; (f) two windows: A's preload is the one above, attested for m₁; B's declared preload exposes nothing and exports `honorOverwrite`, which v1 main imports (`ipcMain.on('save', (_e, path, o) => save(path, honorOverwrite(o)))`) and which returns `false` at m₁. Editing only B makes it return `o` (set digest m₂; A's preload and every main source unchanged). (f1) Positive control, B not edited: a raw v2 `send` on `save` carrying `[path, true]`. (f2) The edited tree packaged with the old manifest, only B's `preload.digest` and `verdict.digest` updated to its new bytes, so B's own digest check decides nothing. (f3) `keld migrate` re-run on the edited tree with no re-attestation, then that manifest with A's `channels` kept at `["el:save"]` (T3 fixture manifest; T6 on device, as (c)) and the same raw `send`. (f4) The developer re-attests A for m₂ | (a) A release build recomputes m₂ ≠ `verdict.mainDigest` and refuses to package W with a typed error (T5, at KEL-19's build). (b) `verdict.mainDigest` is m₂, `attestation.mainDigest` stays m₁, `channels` is `[]` and the report lists every site for review. (c) The Host loads W with `channels: []` and a typed error whose fix is to re-attest; the raw `send` is refused, the `save` handler counter and the protected overwrite counter read 0. (d) W is granted exactly `el:save`, the exposed `save` runs one handler; a raw `send` with `overwrite` then reaches it, which is what the renewed attestation asserts is safe (the residual of §4.8), recorded, not a pass. (e) The helper, the renderer bundle, `handler.dat` and the declared preload each change `mainDigest`; `keld.permissions.jsonc` does not; neither the order nor the second run does. (f1) A has `el:save`; one handler entry, protected overwrite counter 0. (f2) The recompute reads m₂ ≠ A's `verdict.mainDigest` and refuses to package A with a typed error. (f3) A's `verdict.mainDigest` is m₂, its `attestation.mainDigest` stays m₁, `keld migrate` writes `channels: []` and lists A's sites; with `channels` kept, the Host loads A with `channels: []` and a typed re-attest error, and the handler and overwrite counters read 0. A's `preload.digest` never moved. (f4) A is granted exactly `el:save` again and the raw `send` reaches the overwrite: the residual of (d), recorded, not a pass. | The one currency predicate checks only the preload digest → in (c) W keeps `el:save` and the raw `send` causes the protected overwrite (counter 1) → RED. Separately, drop the main comparison from the release recompute → (a) is packaged → RED. Separately, `keld migrate` writes `attestation.mainDigest` by itself → (b)'s window is granted with no developer act → RED. Separately, hash only the main entry file → (e)'s helper change leaves `mainDigest` equal → RED. Separately, filter the set by file extension → (e)'s `handler.dat` change leaves `mainDigest` equal → RED. Separately, keep `keld.permissions.jsonc` in the set → (e)'s second run yields a different digest → RED. Separately, exclude declared preloads from the set (the r13 rule) → (e)'s preload change leaves `mainDigest` equal, and in (f) A stays attested: (f2) is packaged and (f3)'s raw `send` causes the protected overwrite (counter 1) → RED. |
| AC33 Final admission fence (r13, 0monish 4429155178; floor cells T2, `send` cells T3, (h) T4) | A granted route (T2: the floor `invoke` on Echo channel `1`; T3: `el:a`, with `el:danger` ungranted). A `profile-test-hooks` hold keeps an accepted message in Core's queue after `on_web_message` returned and before the fence. Independent counters: writes on the original and on the successor app link, each role's Bun handler, fence admissions and Guard evaluations, plus the next document's raw `message` listener. Cells: (a) positive: hold and release with document and role unchanged; (b) commit: hold A's message, the application's `loadURL` of the entry commits B, release; (c) role: hold, terminate the Bun role, start the successor, wait until the successor's main reports its `on('a')` registered (floor: its Echo handler; that report, not `route-state: ready`: #540 withholds readiness until old queued work is gone), release; (d) order, T3: hold `send('danger')` and proceed as (b); (e) late observation: the message passes the fence, its write completes, then B commits before the Bun handler returns; (f) pure Rust: the final FIFO stub answers `Full`, and separately `Disconnected`; (g) raw posts with the current epoch: a `send` while AC27's latch holds the route not ready with the role's handler registered, a `send` reusing an admitted `message` after its `send-result`, and a second `invoke` while the first is held behind the reply latch; (h) outbound: main's `event` from the old role is held, the role retires, the hold is released | (a) One fence admission, one write on the original link, one handler entry, one terminal. (b) 0 writes on any link, 0 handlers, 0 Guard evaluations; the `send` ledger records the `stale` terminal (floor: no terminal form, §4.4) and A's reservation is released once; B's raw counter for anything carrying A's `document` reads 0; B's own message is then admitted once. (c) 0 writes on either link, 0 handlers on either role; the live document gets one `send-result` with disposition `route-retired`, certainty `not-dispatched` (floor: one `invoke-result` error, ignored if the bridge already settled at `route-state: retired`, §4.3); the charge follows #540 §4.9; after `route-state: ready` a fresh message is admitted once on the successor. (d) The `stale` terminal, no `KELD-GUARD003`, 0 Guard evaluations (#1191 §4.4 step 2). (e) One handler entry on the original role, a G₁ effect only; nothing is rolled back, and B sees 0 posts for it. (f) `Full`: no write; `refused`, `KELD-WV-011`; the reservation is released once. `Disconnected`: no write; `route-retired`, `not-dispatched`; the reservation is released once; no successor write. (g) Each is refused before any queue with §4.4's terminal (the reused `message` at step 3a, the others at capture): 0 queue entries, 0 handlers. (h) 0 posts of that `event`. | Omit the final comparison → (b) reaches the old role's handler → RED (in (c) it can only try the dead original link, so the pure interleaving decides that cell: write counter 1). Separately, compare only `G` → in the pure interleaving a role retirement wins and the stub original FIFO takes the frame (write counter 1) → RED. Separately, compare only the role → (b) reaches the handler → RED. Separately, drop the comparison before Guard → (d)'s Guard counter reads 1 → RED. Separately, select a denial's terminal without its comparison → in the pure interleaving a revocation wins during the evaluator and the terminal is `KELD-GUARD003` → RED. Separately, retarget a role mismatch to the live link → (c) writes on the successor link → RED. Separately, capture the role at dequeue instead of at admission → (c)'s ticket names the successor, the comparison passes and the successor is entered → RED. Separately, leave the fence after the comparison and before `try_send` → in the pure interleaving a revocation wins between them and the write counter reads 1 → RED. Separately, trust the bridge's `ready` bit, with no check at capture → (g)'s raw `send` reaches the handler → RED. Separately, skip step 3a → (g)'s reused `message` runs a second handler entry → RED. Separately, reserve only for `send` → (g)'s second `invoke` reaches Echo while the first is held → RED. Separately, check only the document at rule 1 → (h) posts the old role's `event` → RED. Separately, answer `Disconnected` as `Full` → (f) reports `refused` → RED. If #540's `DocumentRetired` watermark hides (b)'s flip on device, that cell is defence in depth and the pure `wv2_admission_fence` interleaving, which has no such layer, decides the rule. |
| AC34 Event decode before dispatch (r13, 0monish 4429155182; T4) | AC27's harness. A Bun fixture issues #540's `Deliver` with payload bytes `[255]`, past #1190's encoder (the Host treats them as opaque, `gh540-compat-ipc.md:195`), so a syntactically valid `event` carries `JsonBytes` `[255]`. Cells: (a) `[255]` with a listener registered; (b) `[255]` with none; (c) a valid value with none; (d) a valid value and a listener that throws; (e) a valid value and a listener that returns. The page counts `window` `error` events | (a), (b): exactly one `consumed` with `{status:"refused", code, message}`, `code` being #1190's registered code for the failure (#1190 requires a typed rejection for golden `ff`; its class is `MalformedValue`, INFERRED from its three classes), 0 listener calls, page `error` counter 0; the record is released once and the `Deliver` REPLY carries `Refused` with that code and the registry's text (§4.4 rule 5). (c) One `consumed` with `{status:"no-listener"}`, no future-listener queue: a listener added later is not called. (d) One `{status:"consumed"}`, the page `error` counter rises by 1 with the listener's message (AC27's cell). (e) One `{status:"consumed"}`, `error` counter 0. | Catch every failure in one `catch` as `consumed` → (a) reports `consumed` → RED. Separately, rethrow the decode error → (a)'s page `error` counter reads 1 → RED. Separately, look up listeners before decoding → (b) reports `no-listener` → RED. Separately, report every no-dispatch outcome as `consumed` → (c) reports `consumed` → RED. |
| AC35 Floor channel (r14, 0monish 4429800832; T2) | T2's actual bridge, admission path and `decode_wv_link`; one eligible document, the role admitted and ready, the view's `admitted_channel` being Echo `1`. The page skips `window.keld.invoke` and posts byte-valid raw v2 `invoke`s with the current epoch and a fresh `request`: (a) `channel: 2`; (b) `channel: 3`, KEL-142's reserved lifecycle channel; (c) `channel: 2` while a floor `invoke` is held behind the reply latch. Independent counters: queue entries, reservations, fence admissions, app-link writes keyed by channel and frames received at Bun keyed by channel | (a), (b): one `invoke-result` error, `KELD-WV-011`, detail `renderer channel is not declared by this build`; every counter 0. (c) The same refusal; the held `invoke` keeps its reservation and resolves once after the latch. After each cell a floor `invoke` on Echo `1` resolves with one Echo entry. | Omit the check → (a)'s post is captured and admitted, and one app-link write happens where the baseline has none: on channel 2 if the forwarder stamps the page's id, on Echo `1` if it reuses `begin_echo_call`'s fixed `ECHO_CHANNEL` write (`app_session.rs:6155-6162` at `457a9c3b`) → RED, whatever `window.keld.invoke` does, because the post is raw. Separately, compare with a literal `1` instead of the view's id → in pure Rust a view created with id 7 admits channel 1 (macOS's control `admitted_channel_is_the_host_supplied_id`, `macos_bridge.rs:1061` at `457a9c3b`) → RED. |
| AC36 Consumer disposition is Host-checked (r14, 0monish 4429800827; T4) | AC27's harness and a Bun fixture's `Deliver` on `a`. Delivery 1's post has returned `S_OK`, and a page-supplied `ipcRenderer.on('a')` listener, which runs inside the bridge's dispatch, posts a raw exact-key `consumed` (the predicted key, as AC30 (f2)) with `{status:"refused", code, message}` before the bridge's own `consumed`. Cells: (a) `code:"KELD-GUARD003"`; (b) `code:"KELD-WV-013"`; (c) `code:"KELD-COMPAT-001"`, an existing evidence code, and `code:"KELD-COMPAT-999"`, an unallocated one, each on its own delivery; (d) as (a), with a fixture bridge build that posts no `consumed` for delivery 1; (e) a registered code with `message:"KELD-GUARD003: denied"`. Counters: `Refusal::ConsumerCode`, ledger release, the `Deliver` REPLY's disposition, and every string the Bun caller receives | (a)–(c): the refusal counter rises by 1 per forged post and the release counter stays 0 until the bridge's `consumed`, which releases the record once; the one `Deliver` REPLY is `Consumed`; no string the Bun caller receives contains the forged code. (d) No release and no REPLY; at #540's 5,000 ms missing-receipt bound (stub clock in the pure test) route admission closes and the route retires; the caller's one rejection is #540's retirement outcome and carries no forged code; the charge follows #540 §4.9. (e) One `Deliver` REPLY `Refused` with that code and a `message` byte-equal to the registry's text; no string contains `KELD-GUARD003`; the record is released once, and the bridge's later `consumed` releases nothing. | Forward any bounded code (the r13 rule) → in (a) the `Deliver` REPLY is `Refused` with `KELD-GUARD003`, and in (b) with `KELD-WV-013` → RED. Separately, test only a `KELD-COMPAT-` prefix → (c)'s codes reach the caller → RED. Separately, release on a refused receipt → (a)'s release counter reads 1 before the bridge's `consumed`, which then releases nothing → RED. Separately, forward the page's `message` → (e)'s caller string contains `KELD-GUARD003` → RED. |
| AC37 Raw v2 envelope fields (r14 sweep; (a)–(d) T2, (e), (f), (i) T3, (g), (h) T4) | T2's admission path and the real `decode_wv_link`; T3's forwarder; T4's facade and #1190's decoder. Each raw post carries the current epoch and a fresh sequence and is otherwise byte-valid, so one field decides. (a) `bind` with an extra `document` key; `invoke` with, one at a time, `request` `0`, `4294967296` or `1.5`, `channel` `0`, `65536` or `"1"`, a 31-character or upper-case `document`, a payload element `256`, a 4,097-element payload, an extra `role` or `url` key. (b) `bind` padded with JSON whitespace to 4,097 bytes. (c) A `request` reused after its `invoke-result`; positive, T3: `request: 1` and `message: 1` in one document. (d) T2's floor-only build: a raw `send` on `a` and a raw `consumed`. (e) `send` with, one at a time, `message` `0` or `4294967296`, a 257-byte `channel`, a numeric `channel`, `"channel":"\ud800"`, a string `payload`, a 4,097-element payload, an extra `role` key. (f) `send` with `channel` `""`, and with 256 U+0000 characters. (g) With `on('a')` granted: `send` with payload `[255]` (#1190's golden `ff`), and with an argument vector of 17 nulls (20 encoded bytes). (h) With delivery 1's post returned and its record outstanding, from a page-supplied `ipcRenderer.on('a')` listener inside dispatch (AC36's path; the bridge's own `consumed` comes after it): `consumed` with `delivery: 0`, `{status:"stale"}`, `{status:"route-retired","certainty":"not-dispatched"}`, `{status:"consumed","code":"x"}`, `{status:"refused"}`, a 65-byte `code`, a 769-byte `message`, and one padded to 4,097 bytes. (i) A `pushState` to a URL with a 5,000-byte query, then a granted `send`, with a Bun fixture recording `RendererMessage.url`; and a `send` from a document whose committed URL is over 4,096 bytes | After every cell a floor `invoke` still succeeds, and from T3 so does a granted `send`, except in (i)'s over-bound document. (a), (b), (e), (h): refused by the carriage step, by `decode_wv_link` or, for the `document` spellings, by step 2, with a typed reason; ticket, reservation, release and Bun counters 0; in (h) delivery 1 stays charged and the bridge's genuine `consumed` then releases it once. (c) Refused at step 3a, `KELD-WV-011`, with no second Echo entry; the positive pair is admitted. (d) Each refused with no post (§6 T2): 0 reservations, 0 writes. (f) `send-result`, `refused`, `KELD-GUARD003`, byte-exact and inside the field and CONTROL bounds (#1122 `d9211a96` `docs/specs/gh540-compat-ipc.md:621-628`); 0 Bun entries. (g) 0 `on('a')` listener calls; `send-result` with disposition `refused` and a code of §4.4 rule 5's set; the reservation is released once. (i) `url` is the committed URL, without the pushed query; the over-bound document's `send` is refused at capture, `KELD-WV-011`. | Ignore unknown keys in the decoder → (a)'s `role`-bearing `invoke` is decoded → RED. Separately, accept a zero sequence → (a)'s `request: 0` is decoded → RED. Separately, cap CONTROL forms only at 65,536 bytes → (b)'s `bind` is decoded → RED. Separately, decode `consumed` with the terminal disposition type → (h)'s `stale` receipt is decoded → RED. Separately, keep one sequence for `message` and `request` → (c)'s positive `message: 1` is refused → RED. Separately, check the sequence for `send` only → (c)'s reused `request` adds a second Echo entry → RED. Separately, let T2's build forward a decoded `send` to `admit` → (d)'s app-link write counter reads 1 → RED. Separately, skip the receiving adapter's 17-argument refusal → (g)'s 17-null `send` reaches `on('a')` → RED. Separately, stamp `url` from the per-call `Source` → (i)'s Bun fixture records the pushed query → RED. |
| AC38 Duplicate and sequence orderings (r15, 0monish 4431715702; (c), (e)'s floor half and (m)'s `invoke` half T2, (m)'s `send` half T3, the rest T4) | AC27's harness (T4) with `on('a')`'s completion held behind the test latch; in T2, the floor `invoke` behind the reply latch. The page takes each raw post's number from the AC26 recorder (the predicted key of AC30 (f1)), so the at-risk order holds; the recorder confirms that a raw post's number equals the bridge's next one where a cell says so (at-risk count above 0). Raw posts carry the current epoch unless a cell says otherwise. Independent counters: `send-result` and `invoke-result` posts keyed by document and sequence, `Refusal::LiveSequence`, Bun writes, each handler, ledger reservations and releases, the bridge's local `recordBusy` count. AC33 (g) is the after-completion replay, not these orders. Cells: (a) positive control: `send('a')`, its `send-result`, `send('a')`; (b) in-flight duplicate: hold message 1; a raw `send` with `message: 1` on `a`; `send('a')`; release; `send('a')`; (c) floor: hold request R; a raw `invoke` with `request: R` on Echo `1`; a raw `invoke` with `request: R` and `channel: 2`; a second `window.keld.invoke`; release; `window.keld.invoke`; (d) next number in flight: with M the last number the bridge used, a raw `send` with `message` M+1 on `a`, held; `send('a')`, which posts M+1; release; `send('a')`; (e) next number finished: a raw `send` with `message` M+1 on `a` and its `send-result` received, then `send('a')`; floor half: a raw `invoke` with the bridge's next `request` on `channel: 2` and its `invoke-result` received, then `window.keld.invoke`; (f) a large gap: a raw `send` with `message` M+100 on `a`, held; `send('a')`, which posts M+1; release; `send('a')`; (g) slot held elsewhere: two windows on one role; window 1's message held; in window 2 a raw `send` on window 2's next number; after its `send-result`, `send('a')` in window 2; release; `send('a')` in window 2; (h) two windows, one number: window 1's message 1 held; window 2's first `send('a')`, also message 1; release; `send('a')` in window 2; (i) navigation: A's message 1 held; the application's `loadURL` of the entry commits B; the replay hook delivers a duplicate carrying A's `document` after B's `ContentLoading`; B's first `send('a')`, message 1; release; `send('a')` in B; (j) wrong nonce: a raw `send` with the bridge's next number and a `document` not byte-equal to the current one; `send('a')`; (k) teardown: message 1 held; the application's `loadURL` commits B; release, so Bun's terminal arrives for a replaced document; (l) lost notification: AC27's restart cell, with a test hook keeping `route-state: retired` from being enqueued (the fifth-control case of #540 `:557`; a held one would stop the successor's readiness, `:559-560`); the successor's `route-state: ready`; `send('a')`; (m) pure Rust, step 3a over a stub ledger and a stub bridge: a duplicate before and after the terminal's post, with the ledger's release before that post; a raw number refused for quota, then the bridge's post of that number with the refusal still in flight, the quota now free | (a) Two handler entries, two `send-result`s, no `recordBusy`. (b) The raw post: 0 Bun writes, 0 handler entries, 0 reservations, `LiveSequence` 1, 0 posts for message 1 before the release. `send('a')`: one local `recordBusy`, no post. After the release: exactly one `send-result` for message 1, release counter 1, and the next `send('a')` is posted as message 2 and reaches `on('a')` (counter 2). (c) Both raw posts: 0 Echo writes, 0 posts for R before the release; the first promise stays pending; the second `invoke` fails busy (KEL-142 AC6). After the release the first promise resolves once and the next `invoke` resolves. (d) The bridge's post: `LiveSequence` 1, 0 Bun writes, 0 posts; the bridge stays busy; after the release one `send-result` for M+1 settles it; `on('a')` rose by 1, for the raw post only, so the facade's call was lost and the receipt records it; reservation high-water 1, release counter 1; the next `send('a')` is posted as M+2 and admitted once. (e) `send('a')` is posted as M+2 and admitted once, with no refusal; the floor `invoke` resolves. (f) The bridge's post is `Spent`: one `send-result`, `refused`, `KELD-WV-011`, one renderer-local diagnostic, not busy, 0 Bun entries; after the release the raw record's result raises `seen`, and `send('a')` is posted as M+101 and admitted once. (g) One `send-result`, `refused`, `KELD-WV-011` in window 2 for the raw post, reservations unchanged; `send('a')` in window 2 is refused the same way, with the next number; window 1's record is untouched and releases once; after the release window 2's `send('a')` is admitted once. (h) Window 2 gets one `send-result`, `refused`, `KELD-WV-011`; `LiveSequence` 0; after the release its next `send('a')` is posted as message 2 and admitted once. (i) The duplicate: `Refusal::Epoch`, 0 posts. B's message 1: one `send-result`, `refused`, `KELD-WV-011`, while A's record stays charged (role-wide). After the release: 0 posts for A's terminal, A's release counter 1, and B's message 2 is admitted once. (j) `Refusal::Epoch`, 0 posts, the watermark unmoved; `send('a')` is posted with that same number and admitted once. (k) 0 posts of that `send-result` (rule 1), release counter 1, no second release. (l) `ready` settles the send with one outcome-unknown diagnostic; the next `send('a')` is posted and admitted once. (m) Before the post: no post, then the terminal. After it: one `refused`, after the terminal; the stub bridge settles on the terminal once. Crossing: the bridge's post is `Spent`, the number never becomes live, and the stub bridge settles on a number no record holds | Reply to a live duplicate with `send-result` or `invoke-result` `refused` (the r14 rule), the bridge's `message`-only matcher unchanged → in (b) a `send-result` for message 1 precedes the release, the `recordBusy` count reads 0, message 2 is posted early and refused, and the genuine terminal matches nothing, and in (c) the first promise rejects → RED. Separately, run step 3' before step 3a → (c)'s `channel: 2` post answers R and the first promise rejects → RED. Separately, stay silent on a `Spent` number too → (f)'s bridge stays busy, because no result will ever carry M+1, so every later `send('a')` is a local `recordBusy`; and AC33 (g)'s refusal counter reads 0 → RED. Separately, key `Live` on the number alone → (h)'s window 2 gets no `send-result`, so its next `send('a')` is a local `recordBusy`, and (i)'s B likewise → RED. Separately, move the watermark only on admission → in (m) the bridge's post is admitted and the refusal, carrying the same number, settles the stub bridge with the record live → RED. Separately, classify before step 2 → (j)'s forged nonce spends the number and `send('a')` is refused → RED. Separately, end `Live` at the ledger's release → in (m) the `refused` is posted before the terminal and settles the stub bridge → RED. Separately, remove `seen` → (e)'s `send('a')` reuses a spent number and is refused → RED. Separately, settle only on `retired` → (l)'s `send('a')` is a local `recordBusy` while ready → RED. |
| AC39 Digest census on GH-497's output set (r15, 0monish 4431715705; T5) | A fixture tree holding exactly GH-497's three outputs (`keld.config.ts`, `keld.permissions.jsonc`, `package.json`) beside the application's own inputs: a main file, a helper, two declared preloads, one of them flagged and attested, and a renderer bundle; no `keld.build.ts`, `keld.compat.ts`, `bunfig.toml` alias or tsconfig/jsconfig Electron mapping. The list is a fixture parameter until §10 Q15 lands. The `--write` cells need GH-497 T2 and §10 B10 (i). Cells: (a) default `keld migrate`; (b) `keld migrate --write`, then the release recompute on a staged copy; (c) `--write` twice over one tree, and the recompute on a second checkout of it; (d) after (b), one byte changes in the other window's preload, and separately in the helper, then the recompute | (a) The report prints the digest labelled planned, and GH-497 M01's census reads zero writes and zero child processes. (b) Analysis and recompute list the same paths and file hashes and agree on `mainDigest`; M10's census shows no file beyond the three. (c) The digest does not move. (d) Each change moves the digest and the recompute refuses to package; after a `--write` re-run `verdict.mainDigest` moves, the attestation bound to the old digest no longer equals it, and the attested window fails closed (AC32 (b), (c)). | The writer generates one forbidden output, one single-fault build each for `keld.build.ts`, `keld.compat.ts` and a `bunfig.toml` electron alias → GH-497 M10's write census turns RED (#1172 `2044f22e` `docs/specs/gh497-first-proof-migration-output.md:115`, that census's owner; it runs here once GH-497 T2 lands). Separately, read the list from a `keld.build.ts` → the fixture has none, no census exists → RED. Separately, leave the declared preloads out of the census → (d)'s preload change leaves the digest equal → RED. Separately, leave the helper out → (d)'s helper change leaves the digest equal → RED. |
| AC17 Receipt | A Windows acceptance claim | Runtime `BrowserVersionString`, OS build, head, manifest digest, preload digest, main-code digest, per-AC counters, each RED control's output. | Drop the runtime string → validator RED. Separately, mark `WebMessageReceived` mocked → validator RED. |

## 4. Design

### 4.1 Atomic decomposition (decision input)

| Atom | Owner / boundary | Inputs → outputs | Failure mode | Observable | Evidence |
| --- | --- | --- | --- | --- | --- |
| A1 Script registration | `keld-wv` Windows backend, before the window's first navigation (for a compat window, the application's own `loadURL`, #459; `navigate_initial` only for schema-1 boots) | bridge + wrapped preload text → one document-created script id | Script absent from first document | AC01 | FACT: registration async (Microsoft). UNKNOWN: whether awaiting completion before `Navigate` is needed on 156 (P1 N3b did not falsify) — §10 Q5. |
| A2 Realm identity | WebView2; not controllable | document-created script runs in the main world of every top-level document and same-process frame | Page poisons built-ins; bridge text in frames and in error pages | AC02, AC03 | FACT (E1/E2; E5 `p1-exec-b`: bridge ran in the `about:srcdoc` error page). |
| A3 Event identity (frame) | WebView2 routing | frame post → `Frame2.WebMessageReceived` only | Host registers the frame event | AC03 | FACT: frames guide; E5 trigger q: same-origin srcdoc posts via `parent` arrive with the parent's `Source`. |
| A4 Document eligibility | #1192 §4.4 (KEL-102 verified resource identity); W3's served record `(root, components)` | committed main-frame document + `IsErrorPage` + allowed `NavigationId` + resource identity (§4.5) → qualified / FailedClosed | URL-text spoof; error page at an app URL; an equivalent spelling decided differently from W3 (two component equalities) | AC04, AC25 | FACT: E5 qualified by URL only (probe), so resource-identity qualification is UNTESTED on Windows; FACT: `about:srcdoc` error page committed 5/5 with `Source=about:srcdoc`. |
| A5 Sender authentication (Source, per call) | browser-stamped `Source` URL; `keld-guard`'s one `file:` URL parser (W3 #1185 §4.4) | parse; scheme equal to the committed document's (`https` or `file`); `https`: tuple origin equal; `file:`: empty host and the parser returns in-root components | Opaque-origin schemes (`about:`, `data:`, `blob:`, `chrome-error:`); comparing opaque origins by serialisation (wrong: every opaque origin serialises to `"null"`) or by `Url::origin()` (wrong: `file:` origins are opaque and unique, so nothing matches); a full-URL comparison (wrong: it breaks after history changes); a second path matcher beside the parser; string-loaded documents | AC05, AC18, AC20 | NOT EVIDENCE for this rule: in E5 (`https` only), 0 of 110,575 previous-document messages arrived after the next document's `ContentLoading` in all 96 trials, so its "0 mis-attribution" count had an empty at-risk set and the weaker mutant rules scored 0 too; the statistic discriminates no rule (§10 B7). FACT (source probe, 3/3): `NavigateToString` → `Source = about:blank`, origin `"null"`; `file://` → full file URL, origin `file://`. FACT (source probe v2, 3/3): `Source` **follows the history URL** — after `pushState`/`replaceState`/hash change it reads `…?route=2#frag`, `…?route=3`, `…?route=3#h2`, each preceded by `SourceChanged(IsNewDocument=false)`; an entry URL's query appears in `Source`; a message posted immediately after `pushState` still carried the pre-push `Source`. Hence identity is decided once at commit (A4) and `Source` is a per-call scheme/origin/root check, never a full-URL equality. FACT (`url` 2.5.8, locked in `Cargo.lock`: `origin.rs` L33, doc-test `lib.rs` L859–863): a `file:` URL has an opaque origin and two parses of the same file URL have unequal origins, so origin comparison cannot decide `file:`. Safety of the `file:` path as identity still rests on KEL-102 verification (B6). |
| A6 Document epoch | Host-minted 128-bit nonce; page-readable on route D, a correlation tag, not a secret (§10 Q1(c)) | `ContentLoading` → mint, no post; the document's verified `bind` → one `bound` (§4.3) | A `bound` posted before the bridge's listener exists is lost; a `bound` reaches a replaced document; a duplicate, stale or early `bind` mints or replenishes; an old same-root document's late `bind` takes the new document's one-use latch (§10 B9); a previous-document message arrives after the next commit | AC06, AC08, AC29 | FACT (E5): the epoch posted at `ContentLoading` reached the new document 24/24 per variant (12 trials × 2 documents), 0 wrong-document deliveries; a post made at `NavigationStarting(B)` was received by A 96/96. Cross-document ordering observed, not documented. E5 measured that push: feasibility only, not evidence for the bind→bound order. UNKNOWN: whether a previous-document message can arrive after the next `ContentLoading` (E5's at-risk set was empty) — §10 B7. DESIGN (r11, §10 Q1(b) RESOLVED): `bound` answers the document's verified `bind`; its evidence is AC29's device cells, not yet run. |
| A7 Authorization | `keld-guard` via `dispatch_privileged` in `keld-core` (#1191 T4) | principal + borrowed window grants + `el:c` → Allow / GUARD003 | Second evaluator; over-grant at generation time | AC09, AC13, AC14 | FACT: `windows` ignored today (`lib.rs:625-628`); #1191 is an open draft. |
| A8 Navigation authority | Host `NavigationStarting`; W3 rule A″ (§4.6) | raw `Uri` → scheme-prefix dispatch (`url_scheme_is`); `file:` → parser components (one component equality) and raw query (byte-exact) against the `file:` allow forms; any other scheme → arch 03 §2 matcher on the raw string → `SetCancel` at +0 or proceed (+1 only at commit) | in-root membership taken as allowlist membership; `about:srcdoc` and `document.open` unvetoable | AC10, AC11 | FACT (E5): b committed an error page 5/5 despite Cancel; g (`document.open`) replaced the document 5/5 with **no** `NavigationStarting`. |
| A9 Popups | existing deny-all | handled | — | AC12 | FACT: code. |
| A10 Static classification | `keld migrate` (`keld-cli`) | preload AST + digest, main-code digest + `ipcMain` literals → verdict (flag sites, reachable sinks) + grants | False negative (a narrowing or data-holding site passes unflagged, including a gate on state that is not page input) | AC15, AC22(a), AC31 | FACT: verb reserved (`verb.rs:24,75`). FACT (pinned draw.io blob `44f2f22f`, read directly): the rules of §4.8 flag seven sites (§4.8 table). UNKNOWN: whether rule 2 has false negatives on real preloads — T5 is not built; falsifiers are AC15's mutants and a corpus preload whose narrowing passes unflagged. |
| A11 Evidence | this spec + keld-research#34 | hashed runs → claims | Second runtime/OS claimed; zero-failure claimed with an empty at-risk set | AC17, B7 | FACT: one runtime, one OS build, one device, `https` only; E5's attribution statistic had no at-risk messages. |
| A12 Host→renderer delivery | `keld-wv` post path + #540 envelope | reply/event + target epoch → post or drop | A post reaches the next document's raw listener; a page `consumed` releases an event record before the Host observed its post (§4.4 rule 4) | AC21, AC24, AC22(e), AC30 | DESIGN (§4.4: generation check plus in-flight hold). FACT (E5): posts made at `NavigationStarting` reached the old document 96/96; a cancelled `about:srcdoc` navigation still raised `ContentLoading` (trigger b: `NS+Cancel > CL > NC(ok=False)`). UNKNOWN: delivery of a post handed to the engine before `NavigationStarting` and still in flight at commit (B7 arm H; non-zero with the hold is a blocker). |
| A13 Isolation reporting | `@keld/electron` `process` subset (#1155) on Windows | → `process.contextIsolated` | Facade reports isolation it lacks | AC23 | FACT (Electron `v45.1.0` `shell/renderer/renderer_client_base.cc:236-237`): `process.contextIsolated` is the frame's `context_isolation` preference; FACT (`shell/renderer/api/electron_api_context_bridge.cc:1145-1169`; `v44.2.0` `lib/renderer/api/context-bridge.ts:3-11`): `exposeInMainWorld` throws when it is false. |
| A14 Attestation trust | developer, recorded in `keld.permissions.jsonc` (§4.8 rule 3) | flag sites → attestation bound to the preload and main-code digests | Wrong attestation: narrowing silently lost; a main-only change, or another window's preload edit, leaves it stale | AC19, AC32 (mechanism only) | DESIGN: no Keld party vouches for third-party attestations; the T6 qualification attestation is independently reviewed (§6 T6). |
| A15 Preload gate | bridge gate (§4.3) | committed top-level document → app preload runs or not | an epoch without the preload (a spelling-sensitive gate); the preload or argv in a remote document or error page | AC25 | DESIGN: the gate tests only top-level + `file:` scheme, and every committed top-level `file:` document is Host-served in-root bytes or a Host refusal (`403`, `404`, `413`, `503`) (rule A″ + W3 serving), so every document §4.5 makes eligible under B3's proposal gets the preload. UNKNOWN until the T2 cell (W3 Q7): whether a Host refusal commits as an error page or as an empty `file:` document; either way it gets no epoch and no `onLoss` (AC25 (f)/(h)). |
| A16 Wire grammar and carriage | #540 CompatV2 (`keld.wv-link` v2); `keld-wv`'s one `decode_wv_link`; WebView2 WebMessage strings (§10 Q1(a)) | top-level `TryGetWebMessageAsString` string → strict bounded UTF-8 → one closed v2 variant or a typed refusal | a v1, unknown-kind, wrong-direction or duplicate/extra-field envelope, a non-string message or a non-`JsonBytes` payload is admitted; `get_WebMessageAsJson` becomes a second parser; a second decoder or a fallback mode appears | AC26, AC27, AC37 | FACT: `decode_wv_link` is v1-only at `457a9c3b` (`crates/keld-wv/src/wv_link.rs:131-197`; `v != 1` refused at `:156`; kinds `bind`/`invoke` only, `:160-195`). FACT (#1122 `d9211a96` §4.5): the CompatV2 forms and "A mismatched `v` refuses, never switches the selected mode". DESIGN: the carriage is the #540 wire owner's decision (§10 Q1(a)); it changes the carrier, not the v2 schema. |
| A17 Process-failure kind | WebView2 `ProcessFailed`, through the one per-view handler W2 r16 D8 registers (W1 adds its kind dispatch, §4.10); W2's release barrier (#1188) for the terminal | kind → reload on the surviving control, or revoke everything and end the session | a closed controller is navigated; a renderer exit ends the session | AC24 (11, 14, 15), AC28 | FACT (Microsoft Learn, process-related events, `ms.date` 2023-07-13): a browser-process exit closes every associated control, which "need to be recreated"; after a main-frame renderer exit `Reload` may be used, and the impacted frames' content "is replaced with an error page" (whether that raises navigation events is UNKNOWN, §4.5). FACT: `keld-wv` registers no `ProcessFailed` handler at `457a9c3b`. FACT (#1280 `95594e88` D8 (3)): there only a pid-matched abnormal `BrowserProcessExited` receipt latches `AbnormalExit`, and a receipt after a rebind is foreign. DESIGN: the terminal is W2's `AbnormalExit`, latched at the first `poll` after this `ProcessFailed` under #1280 r16 D8 (`be9e9e56`) (§10 Q13 (a)). |
| A18 Final admission | `keld-core::app_session::PrimaryRouterHandle`, the one owner W3 §4.7 names; the native STA and the runtime role owner update its fence synchronously | Host ticket captured at admission + bounded message → one fence transaction (compare with live owners, final FIFO handoff), after a first comparison and Guard for a `send`, or one typed terminal | Queued work reaches a replaced document or a successor role; a second ticket or fence appears; the fence is held across I/O, Guard or a callback | AC33 | DESIGN (§4.4): the B21 owner decision covers the eval use, the renderer use is proposed (§10 Q14). FACT: no such fence exists at `457a9c3b`; `window_router::write_payload` (`app_session/window_router.rs:77`) writes the socket. |
| A19 Event terminal | bridge (§4.3); #1190's one decoder; #1154's emitter | authentic `event` → decode → one `consumed` of three dispositions | A codec failure is labelled `consumed`; a decode error reaches the page; a record stays charged | AC27, AC34 | FACT (#1122 `d9211a96` `docs/specs/gh540-compat-ipc.md:317-319`): the three dispositions are distinct. DESIGN (r13): decode precedes listener lookup. |
| A20 Floor channel | `keld-wv` step 3' (§4.4); the id comes from keld-core's channel table | `invoke.channel` + the view's `admitted_channel` → capture, or one typed refusal | A raw post reaches a channel the floor never declared; the check lives only in the page's JavaScript | AC35 | FACT (`macos_bridge.rs:406-408` at `457a9c3b`): macOS refuses `channel != self.admitted_channel` natively. DESIGN (r14): the same rule on Windows. |
| A21 Consumer disposition | `keld-wv`, before #540's ledger (§4.4 rule 5); the error registry for #1190's codes | raw `consumed` disposition → a checked receipt with registered text, or a refused receipt | A forged reserved code or text reaches Bun; a refused receipt releases credit | AC36 | FACT (#1190 `115af9ac` `docs/specs/gh544-endpoint-value-codec.md:233-238`): three classes, no number allocated (§10 Q16). DESIGN (r14). |
| A22 Sequence classification | `keld-wv` STA admission state, beside #540's ledger (§4.4 step 3a) | `message` or `request`, keyed by view, `G` and form → `Live` (no post), `Spent` (one refusal) or `Fresh` | A refusal shares its bytes with a live record's terminal and settles the bridge early; a no-reply case leaves the bridge busy | AC38, AC33 (g), AC37 (c) | FACT (#1122 `d9211a96` `docs/specs/gh540-compat-ipc.md:355-359`): the two result forms carry only `document`, the sequence and the outcome. INFERRED: `:603` names no wire carrier for a refusal (§10 Q17). DESIGN (r15). |
| A23 Input census | KEL-19 for the list (§10 Q15); one digest function called by `keld migrate` and `keld build` | the existing tree, read only → sorted path and hash lines → `mainDigest` | The list needs a forbidden output; an input is left out; two functions disagree | AC32, AC39 | FACT (#1172 `2044f22e` `docs/specs/gh497-first-proof-migration-output.md:34,73`): `keld.build.ts` is not generated. FACT: neither `keld build` nor a list schema exists at `457a9c3b`. DESIGN (r15). |

Independence: A16 runs after A3 and before A4–A7: a refused envelope never reaches them, and a well-formed one has no authority until they pass; its `Bind` branch calls A5's `source_ok`. A4 (eligibility) and A8 (navigation) are separate rules with separate
manifest keys; A5 and A6 are separate checks (only A6 separates a same-origin A→A2 pair), and
neither has discriminating natural-timing evidence yet (B7); A7 runs only after A3–A6 and A18's first comparison pass, outside A18's fence, and its Allow closure enters A18's transaction (§10 Q14);
A10 decides the verdict, A14 only covers flagged sites and never widens A7's grants; A12 is
independent of A6 (a renderer-side epoch check cannot stop the page's raw listener). A20 runs
after A6 and before A18's capture, and has no edge to A7: a numeric id never reaches Guard. A22
runs after A6 and before A20, because A20's refusal is an answer that carries a number. A21
runs after A6 on `consumed` only and feeds A12's ledger. A23 feeds A10 and A14 and has no edge to
the Host's run-time rules, which compare recorded digests only. A15
has no authority: a document the gate wrongly selects still has every call refused without
an epoch (A4), and one it wrongly skips loses only its preload API (AC25 detects both).

### 4.2 Trust model: three Host rules instead of three worlds

1. **No IPC authority or Host-delivered data exists only in preload code.** The page can do
   whatever the preload can and read whatever the preload receives; the Host grants exactly
   the literals `keld migrate` generated for this window and this preload digest. A preload
   that may narrow what the page can cause or see is flagged (§4.8) and its window receives
   grants only with an attestation bound to the preload and main-code digests. The epoch nonce is page-readable (§2, §10
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
   (#1190: ≤ 4096 bytes, Host-opaque), bypassing the preload. A payload that is not codec-valid
   is refused by Bun's receiving decoder, not by the Host (AC37 (g)).
3. Preload text and `process.argv` are treated as page-visible: exposed preload functions
   are page-reachable objects whose text `Function.prototype.toString` returns, and every
   preload use of argv after page code starts runs on page-replaceable built-ins. No secrets
   belong in `additionalArguments`; every argv read is flagged (§4.8 rule 2(c)).
4. Every inbound Host→renderer post to the window is page-visible through the page's own
   `chrome.webview` `message` listener — including replies to other callers, payload
   arguments a preload listener drops, and channels the preload never subscribes to (§10 Q12).
   The page can also post a `consumed` for any delivery it sees or predicts; that releases no
   credit before the Host's own half of the release (§4.4 rule 4): page acknowledgments are
   never authority, and a `refused` report reaches Bun only with a registered code and the
   registry's text (§4.4 rule 5).
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
   bridge's `epoch` before the real one arrives or clears `outstanding` (§4.3), or take the
   bridge's next number with a raw `send`, so that the bridge's own send is a duplicate and
   never reaches Bun (§4.4 step 3a) — self-harm only, inside one document: the Host's ledger is
   unchanged by any of these.
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
11. While one `send` is outstanding in a document (from its post until its `send-result` or
   a `route-state` that ends its role, §4.3;
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
   exhaustion is detected only at the Host, which answers with that message's `send-result`
   (`refused`, `KELD-WV-011`, §4.4) and #540's exact release; no JS throw is fabricated. Whether a real application loses required
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
const later = queueMicrotask.bind(globalThis);                   // r12: async rethrow; captured too
if (window !== window.top) return;                               // frames get nothing
let epoch: string | null = null;      // `document` of the first `bound`: 32 lowercase hex (#540 §4.3)
let ready = false;                    // `bound.ready`, then each `route-state`; never credit (#540 §4.5)
let message = 0;                      // #540 per-document u32, nonzero: this bridge's last `send`
let seen = 0;                         // r15: highest `message` in any result, raw posts' included
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
    case "route-state":                                          // r12: a retired role may post no
      ready = env.state === "ready";                             // r13: first; a later throw strands nothing
      // result (#540 §4.4); settle locally. r15: also a `ready` that finds work still pending,
      // because no such work survives into a new role and its `retired` may never have arrived.
      if (outstanding) { outstanding = false; diagnose(outcomeUnknown()); } // no replay, no refund
      rejectFloorInvoke(routeRetired());                         // KEL-142's one pending floor invoke
      return;                                                    // successor `ready` (#540 §4.9)
    case "send-result":                                          // late or already settled: no match
      if (env.message > seen) seen = env.message;                // r15: the floor does the same for `request`
      // `message` alone is enough: while a number is live the Host posts only its record's terminal (§4.4).
      if (env.message === message && outstanding) {
        outstanding = false; diagnoseSend(env.disposition);      // r13: each disposition, below
      }
      return;
    case "invoke-result": settleFloorInvoke(env); return;        // KEL-142 §4.1; late result ignored
    case "event": {                                              // r13: decode BEFORE any listener
      const value = decodeArgs(env.payload);                     // #1190's one decoder; returns, never throws
      if (!value.ok) {                                           // codec failure: no listener ran
        post(wire.consumed(epoch, env.delivery, refused(value.code, value.message)));
        return;                                                  // no page exception, no rethrow
      }
      if (!hasListener(env.channel)) {                           // #1154 emitter; no future-listener queue
        post(wire.consumed(epoch, env.delivery, NO_LISTENER));
        return;
      }
      let thrown: unknown = NONE;
      try { emit(env.channel, value.args); }                     // #1154 C09 emitter, synchronous
      catch (err) { thrown = err; }                              // a listener threw: dispatch happened
      finally { post(wire.consumed(epoch, env.delivery, CONSUMED)); } // exactly one `consumed`
      if (thrown !== NONE) later(() => { throw thrown; });       // application exception, rethrown
      return;
    }
  }
});
function send(channel: string, payload: Uint8Array): void {     // #1190 codec bytes, ≤ 4096
  if (epoch === null || !ready) throw notReady();                // AC07: #1154 §4.7, no queue
  if (outstanding) throw recordBusy();                           // #1122 §4.10; divergence 11
  const next = (message > seen ? message : seen) + 1;            // r15: past any number a raw post took
  if (next > 0xffffffff) throw routeClosed();                    // #540: exhaustion, no wrap
  post(wire.send(epoch, next, channel, payload));                // payload as JsonBytes
  message = next; outstanding = true;                            // r12: state only after post returns
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
committed top-level `file:` document is Host-served in-root bytes or a W3 refusal (`403`, `404`, `413`, `503`) by construction (rule A″
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
floor `invoke`; each `event` is delivered once and answered by exactly one `consumed`, also
when a listener throws or its value fails to decode (bridge terminals, below);
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

**Bridge terminals (r12, 0monish 4428451737 and 4428451744).** Every bridge state bit has a
terminal path besides its happy-path message. (1) `outstanding` clears on its `send-result`,
whatever the disposition (a `send` the Host refuses after §4.4 steps 1–3 is answered by a
`send-result` whose `refused` disposition carries the typed code; #540 sizes that envelope for
`KELD-GUARD003`, #1122 `d9211a96` `docs/specs/gh540-compat-ipc.md:203`), **or** on `route-state: retired`, **or** on a
`route-state: ready` that finds it still set. While a number is live the Host posts only its
record's terminal (§4.4 step 3a), so a raw duplicate cannot be mistaken for that terminal. #540 lets a
retired role end without an on-wire terminal: "close, EOF or the existing timeout may reject a
pending call without an on-wire terminal reply" (`:322`), and a missing terminal receipt closes
route admission and invokes retirement (`:496`). Its notification can be lost too: retirement "cannot
depend on enqueueing a fifth control" (`:557`), and CONTROL may "coalesce idempotent state" (`:492`).
A send the bridge posted was posted while it read ready, so a `ready` that finds it pending is a
new role's, with the old `retired` unseen (INFERRED from #540's one per-document FIFO, `:551-552`;
AC38 (l)). On either the bridge settles its pending
`send` locally as outcome-unknown — one renderer-local diagnostic (divergence 11's channel), no
replay, no credit refund — clears `outstanding`, and rejects a pending floor `invoke` with the
typed local `routeRetired` error, because KEL-142's one-pending floor would otherwise block every
later `invoke`; a late `send-result` or `invoke-result` for the settled sequence matches nothing
and is ignored. `diagnoseSend` emits nothing for `consumed` or `no-listener`, and one
renderer-local diagnostic carrying the disposition for `refused`, `stale` or `route-retired`
(#540: "host denial after a completed void call becomes a bounded internal diagnostic/result,
never a retroactive throw", `:630-632`). Calls stay refused not-ready until a successor `route-state: ready`, and #540
makes successor readiness "one atomic transition requiring old queued=0, old active=0, old
framework continuations=0, and old producers permanently unable to enqueue" (`:559-560`).
Neither `route-state` nor `bound.limits` is spendable credit ("`bound.limits` advertises fixed
ceilings, not fresh spendable credit"; `bound.ready` and route-state: "Neither alone settles a
request or proves old work never ran", `:371`): the successor's `send` is admitted only if the
Host/role ledger holds a free record, and the old role's charges stay held until it quiesces
(`:535-536`, `:566`). (2) Each authentic `event` gets exactly one `consumed`, and its disposition
says what happened (r13, 0monish 4429155182). #540 keeps three apart: `Consumed` "means the
consumer completed synchronous dispatch" (`:317`), `NoListener` "means the endpoint found no
listener during this dispatch and discarded the message" (`:318`), and `Refused` "from the
consumer means the endpoint could not decode/admit the supported value envelope" (`:319`). So
the bridge decodes and validates the payload with #1190's one decoder **before** it looks up or
invokes a listener. A codec failure, for example the hostile golden `ff` (#1190 `115af9ac` `docs/specs/gh544-endpoint-value-codec.md:272-276`),
posts one `consumed` with `{status:"refused", code, message}`, runs no listener and raises no
page exception; `code` and `message` are #1190's ("Consumer Refused may carry only the endpoint
owner's registered codec codes", `:616-617`); a raw post skips this code, so the Host enforces that
limit (§4.4 rule 5). A valid value with no listener posts `no-listener` and
is discarded, with no future-listener queue (`:610`); decoding first keeps one rule, so a
malformed value with no listener is `refused`, never `no-listener`. A valid value with a listener
enters #1154's emitter in `try`/`finally`: `consumed`, including when a listener threw ("Resource
release happens in finally even when an application listener throws; that exception retains
application exception semantics", `:317`), and the bridge then rethrows that exception
asynchronously through `queueMicrotask`, captured before page code. `{status:"consumed"}` and
the rethrow are reserved for a dispatch that entered a listener (AC34). #1154 states the requirement ("Exception handling must release framework-owned
reservations even if an app listener throws, without claiming to catch or swallow the
application's exception as an API semantic", #1154 `84fe903e`
`docs/specs/gh467-contextbridge-ipcrenderer.md:527-529`) but names no wrapper, so the bridge is
the one owner of that disposition on route D. Electron `v44.2.0` dispatches with
`sender.emit(channel, { sender, ports }, ...args)` (`lib/renderer/ipc-native-setup.ts:12`), a
Node `EventEmitter.emit`, so a throwing listener ends that emit and later listeners for it do not
run (Node `events` semantics; INFERRED here), and the native caller discards the call's result
(`shell/renderer/electron_ipc_native.cc:62`, `std::ignore = callback->Call(...)`), so the
exception surfaces as an uncaught renderer exception (INFERRED; AC27 records the page-visible
report). The bridge keeps which listeners run; its report comes a microtask after the dispatch
(INFERRED to be invisible to listeners). (3) `send` builds and posts its envelope before it
changes `message` or `outstanding`, so a throw leaves no stuck bit. Each result raises `seen` and the
floor's twin for `request`, so after a raw post's result the bridge numbers past it (§4.4 step 3a,
AC38 (e)). (4) `epoch`, `ready`, `seen` and
`outstanding` are page-world state: the page can read or forge them (divergence 7), so they gate
only the bridge's own posts and are never authority or credit; the Host revalidates document,
epoch, `Source`, role and grant on every message and enforces readiness, the sequence, the quota,
the floor's channel and the consumer code itself, because a raw post skips the bridge (§4.4,
AC33 (g), AC35, AC36). A raw `send` also skips #1190's encoder, so its refusals hold at Bun's
receiving decoder and argument adapter (#1190 `115af9ac`
`docs/specs/gh544-endpoint-value-codec.md:192-195`; AC37 (g)).

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

Chosen: (b′), confidence 0.8. Its residual is how a Host refusal (`403`, `404`, `413`, `503`) commits (AC25
(f), (h), (i), W3 Q7); if it commits as a `file:` document, it gets the preload text but no epoch,
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
field or a non-array payload is refused before any effect, as is any field outside #540's value
domains (#1122 `d9211a96` `docs/specs/gh540-compat-ipc.md:186-199`). `bind` and `consumed` are
CONTROL, so that decoder also caps them at 4,096 encoded bytes (`:199`, `:369`), and `consumed`
takes only `JsonConsumerDisposition` (`:332-335`), so a raw `stale` or `route-retired` receipt is
refused there (`:320`; AC37). Windows has no LegacyV1 mode and
no second decoder. Today `decode_wv_link` is v1-only (`wv_link.rs:131-197`); its CompatV2
mode is #540 work (#1122 §7.1 W1, owner #465), so T2 blocks on it (§10 Q1). Split of responsibilities per §5: **`keld-wv` authenticates identity
(steps 1–3), holds the sequence, floor channel and consumer-disposition checks (steps 3a and 3', rule 5), captures the Host ticket and hands it with the bounded message to `keld-core`'s
forwarder, the only caller of `dispatch_privileged` (#1191 T4).**

```rust
// keld-wv/webview2 (STA callback): identity and #540's wire domains; no Guard policy, no Bun write.
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
    // The RAW string WebView2 stamped; never re-serialised. A failed read is a typed refusal (r12).
    let Ok(source) = args.Source() else { return refuse(Refusal::Source) };
    let src: &str = &source;
    if let WvLinkEnvelope::Bind = env {
        // 2'. `bind` (§10 Q1(b)): answered at most once, with the CURRENT qualified document's
        //     `bound`. Before qualification: zero buffered work, zero nonce, zero authority
        //     (#1192 §4.4); nothing is held for the later ContentLoading. A duplicate, stale or
        //     foreign `bind` mints, posts and replenishes nothing.
        // Provenance limit (r12, §10 B9): `bind` carries no `document`, and the event args expose
        // only `Source` and the message (webview2-com-sys 0.38.2 `bindings.rs:37299-37313`; Args2
        // adds only `AdditionalObjects`, `:37420-37426`). A late `bind` from an old document under
        // the same root, or at the same URL, passes these checks as the current document's would.
        let Some(doc) = view.qualified_document() else { return refuse(Refusal::EarlyBind) };
        if !source_ok(src, &doc.entry) { return refuse(Refusal::Source); }
        if !doc.take_bind() { return refuse(Refusal::DuplicateBind); }   // once per document (B9)
        return view.post_bound(doc);   // final check (rule 1); a failed or dropped post is not answered
    }
    // `invoke`, `send`, `consumed`: each carries `document`; steps 2–3 apply to all three.
    // 2. epoch byte-equal to the CURRENT committed eligible document's nonce (identity was
    //    decided once, at that document's ContentLoading — A4).
    let Some(doc) = view.qualified_document() else { return refuse(Refusal::NoDocument) };
    if !doc.epoch.matches(env.document()) { return refuse(Refusal::Epoch); }
    // 3. Source per call (one function, also used by step 2'):
    if !source_ok(src, &doc.entry) { return refuse(Refusal::Source); }
    // `consumed` names a recorded charge: no ticket, no reservation, no live role (rule 4). Its
    // disposition is checked before the ledger sees it: a raw post skips the bridge (rule 5).
    if let WvLinkEnvelope::Consumed(receipt) = env {
        let Some(receipt) = receipt.checked_disposition() else { return refuse(Refusal::ConsumerCode) };
        return view.ledger.report(&doc, receipt);
    }
    // 3a. `invoke`, `send`: the number, before anything can answer it (r15, §4.4 below).
    match view.admission.sequence(&doc, &env) {
        Sequence::Live => return refuse(Refusal::LiveSequence), // its record's terminal is its one answer
        Sequence::Spent => return view.post_terminal(&doc, &env, Refused::SpentSequence),
        Sequence::Fresh => {}                                   // the watermark moved: a refused number stays spent
    }
    // 3'. `invoke` only: the view's one declared numeric channel, before any ticket, reservation
    //     or queue (macOS `admit_invoke`). A raw post skips the floor's JavaScript.
    if let WvLinkEnvelope::Invoke(invoke) = &env {
        if invoke.channel.get() != view.admitted_channel {
            return view.post_terminal(&doc, &env, Refused::UndeclaredChannel);
        }
    }
    // 4. `invoke`, `send`: capture the ONE Host ticket and #540's reservation on this STA turn,
    //    before any queue (#1191 §4.4: "at admission, not at dequeue"). No admitted role or a busy
    //    quota is answered here (terminal table, below).
    let ticket = match view.admission.capture(&doc, &env) {
        Ok(ticket) => ticket,
        Err(refused) => return view.post_terminal(&doc, &env, refused),
    };
    // A closed queue releases the reservation and answers; nothing leaks.
    if let Err(closed) = view.forwarder.submit(Admitted { ticket, env }) {
        return view.post_terminal(&doc, &closed.env, view.admission.release(closed.ticket));
    }
}
// Host-minted, immutable, never page-supplied. W3 §4.7's eval ticket, one type: only `work` differs.
struct AdmissionTicket {
    view: WebviewId, lifetime: ViewLifetime,       // the native WebView instance, not a number alone
    generation: NonZeroU32,                        // the one checked G (#1192 N16)
    role: RoleIdentity, role_generation: RoleGeneration, link: LinkIncarnation, // authenticated, as captured
    work: TicketWork,  // Renderer { window, sequence } here; Eval { correlation, caller, attestation } in W3
}
// keld-core app_session (#1191 T4 forwarder): runs at dequeue, when steps 1–3 may be stale.
// A `Consumed` never comes here: it went to #540's ledger owner above.
fn forward(item: Admitted) {
    let outcome = primary_router.try_admit_renderer(item.ticket, item.env);
    settle(outcome);                                   // one terminal (table below), posted under rule 1
}
impl PrimaryRouterHandle {
    // The ONE evaluator call for a v2 `Send`. Name recorded by the first task to land it (§6). The
    // fence is never held across Guard, a callback, socket I/O or an awaited queue slot (§10 Q14).
    fn try_admit_renderer(&self, t: AdmissionTicket, env: RendererWork) -> Outcome {
        // Exact original lifetime, G, role, RoleGeneration and link against the LIVE owners, before
        // Guard and before any enqueue. A mismatch never retargets to a successor.
        if let Err(stale) = self.fence.enter().compare(&t) { return self.not_admitted(t, stale); }
        let TicketWork::Renderer { window, .. } = t.work else { return self.not_admitted(t, Foreign) };
        match env {
            RendererWork::Send(send) => {
                let principal = Principal::Webview { id: t.view.0, generation: t.generation.get() };
                let verdict = dispatch_privileged(manifest, principal, RendererChannelQuery {
                    channel: &send.channel,                // "el:" prefixed exactly once (#1191)
                    current_window: grants_for(window),    // empty unless rule 3 passed (AC14, AC19, AC32)
                    on_allow: |_permit| self.admit(&t, send), // enters and leaves the fence inside the closure
                });
                match verdict {
                    Ok(outcome) => outcome,
                    Err(denied) => self.refuse_denied(&t, denied), // compares under the fence: stale wins
                }
            }
            RendererWork::Invoke(invoke) => self.admit(&t, invoke), // step 3' proved the channel; no `el:`
        }
    }
    // W3 §4.7's one transaction, renderer use: the steps of `try_admit_eval_settle`. On `Full` a
    // renderer message refuses (table below), where an eval value waits.
    fn admit(&self, t: &AdmissionTicket, work: impl OwnedFrame) -> Outcome {
        let live = self.fence.enter();
        if let Err(stale) = live.compare(t) { return live.not_admitted(t, stale); } // the deciding one
        live.try_send_original(t, work)    // commit + try_send, one step; t's link, its final FIFO; never `current`
    }
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

**Final admission fence (r13, 0monish 4429155178).** Steps 1–3 run on the STA; the effect comes
after a queue, so a document commit or a role retirement can win in between. The rule is #1191's:
the native owner "captures an authenticated currently admitted Bun role/link generation **at
admission**, not at dequeue", then "At final nonblocking handoff linearize native revocation
versus forwarding, compare the captured original document **and** authenticated Bun role against
current authority, then call `dispatch_privileged` exactly once", and "On revocation or role
replacement first, never retarget old work to successor" (#1191 `4f246a75` `docs/specs/gh552-exact-channel-grants-guard.md:144,148-149`). #1192 names
the stamp ("Before **every** enqueue/forward, revalidate that stamp at the final nonblocking
#552/`dispatch_privileged` boundary", #1192 `cd30f997` `docs/specs/gh542-native-document-dual-world-binding.md:141`), and #540 the queue rule ("Revalidate
role and document under the same queue owner on **every** enqueue, including delayed core
results", #1122 `d9211a96` `docs/specs/gh540-compat-ipc.md:553-554`). W1 mints no second stamp, ticket type or fence. It reuses the
ones the #1192/#540 owner set out for W3's eval result (B21 decision, [#1311 comment 6532409232](https://github.com/gyldlab/keld/pull/1311#issuecomment-6532409232); W3 §4.7 step 4
at #1311 `fa49dcfb`). That decision names the eval use only; the renderer use is this spec's
proposal (§10 Q14).

- **Ticket.** One Host-minted immutable `AdmissionTicket`: the exact `WebviewId` and its native
  lifetime (not a generation number alone), the one checked `G`, and the original role identity,
  `RoleGeneration` and authenticated app-link incarnation (not numeric generation equality
  alone). At the pin that link is `ActivePrimaryGeneration { attempt, identity, writer, .. }`
  (`app_session.rs:5946`). `work` carries what differs by use: here the Host-bound window key
  (#1191 AC3) and the #540 sequence, in W3 `TicketWork::Eval { correlation, caller, attestation }`.
  The ticket is immutable, so the one-time disposition lives in the ledger, not in it. Page-supplied stamps are never accepted, and the same URL is
  never document identity. A renderer message needs what W3's eval result is excused from: its
  document is the current qualified one at capture, and `G` must still be live at the fence.
- **Capture.** On the STA turn that passed steps 1–3 and before any queue, under the fence's short
  guard, `capture` reads the live owners and reserves #540's record (the Compat DATA record of
  #1122 `d9211a96` `docs/specs/gh540-compat-ipc.md:486`, or the `invoke` budget that shares it). This is where raw posts meet what the
  bridge's page-world bits only suggest: a role that is not admitted and ready and a busy quota are
  refused here, because a raw post skips `ready` and `outstanding`. Step 3a has already dealt with
  the bridge's third bit, `message`.
- **Floor channel (r14, 0monish 4429800832).** KEL-142's "routes exactly the declared Echo
  channel" is a Host rule here, not a JavaScript one: a raw page can post a byte-valid v2 `invoke`
  on any nonzero `u16`, the reserved lifecycle channel `3` included. So step 3' compares
  `invoke.channel` with the view's one `admitted_channel` before `capture`, in macOS's order
  (document, channel, pending: `channel != self.admitted_channel` in `admit_invoke`,
  `crates/keld-wv/src/wkwebview/macos_bridge.rs:393-409` at `457a9c3b`, which has no sequence check;
  step 3a comes first here because this refusal is an answer that carries a number) and as #540 requires
  ("still checked against the table-derived admitted numeric id", `:194`). keld-core passes the
  id at view creation from its channel table, as it does for macOS
  (`keld_ipc::channel_table::ECHO.wire_id()`, `app_session.rs:2541-2545`), so W1 writes no literal
  `1`. The id travels as a plain `u16`, and the scan that keeps such a hop free of literals names
  only macOS's calls (`BRIDGE_ID_CALLS`, `crates/keld-cli/tests/channel_table_scan.rs:508-512`), so
  T2 adds the Windows hop to it. macOS tests the id a second time in keld-core at dequeue
  (`app_session.rs:4333`, `#[cfg(target_os = "macos")]`). The Windows forwarder ports no such
  copy: step 3' is the one check, and a second would hide AC35's mutant. A mismatch is
  `Refused::UndeclaredChannel`: macOS's `KELD-WV-011` and fixed detail
  (`macos_bridge.rs:406-408`, `:815`; KEL-142 §7 lists "undeclared renderer channel" under that
  code), with no ticket, reservation or queue entry. This is separate from `el:` grants: a numeric
  id never reaches Guard (AC35).
- **Host-stamped fields (r14 sweep).** `RendererMessage` carries `url` and `origin` (`:237-238`),
  which #540 forbids in renderer JSON (`:373`) and calls Host-stamped "snapshots for that admitted
  navigation" (`:584-585`). They are the values recorded when §4.5 qualified the document, never
  the per-call `Source`: `pushState` lets the page choose that string (A5), so it is parsed and
  dropped, and `senderFrame.url` stays the committed URL (#540's snapshot, not a route-D
  divergence). A committed URL or origin outside #540's domain (`:196`: ≤ 4,096 and ≤ 1,024 UTF-8
  bytes, no truncation) has no faithful stamp, so capture refuses that document's `send` (table
  below, AC37 (i)); `invoke` stamps neither.
- **Transaction.** `PrimaryRouterHandle` (`app_session.rs:5851`) owns one short synchronized
  transaction, the one W3 §4.7 runs for an eval result: compare the ticket with the live owners,
  confirm the reservation, commit the one terminal disposition and `try_send` the owned frame into
  the **original** link's final bounded per-link writer FIFO. The commit and `try_send` are one
  step: a refused `try_send` commits the refusal instead, and nothing outside the fence sees the
  difference. A prior Core or UI queue accepting the message does not count as admission.
- **Guard.** A `send` also crosses Guard, and two rules set where (§10 Q14). #1191 orders it:
  compare, then `dispatch_privileged` once, with the write only from its Allow closure. Decision 1
  keeps the fence off Guard and callbacks. So Core compares once under the fence and leaves it, a
  stale ticket stopping there with no Guard evaluation; calls `dispatch_privileged` with the fence
  released; and enters the transaction inside `on_allow`, leaving it there. A denial selects its
  terminal after one more comparison under the fence, so a revocation that won during the evaluator
  reports its own terminal, never `KELD-GUARD003` (#1191 `4f246a75`
  `docs/specs/gh552-exact-channel-grants-guard.md:147`: stale identity returns "**before** selecting
  an absent-grant error"). The deciding comparison is always the one under the fence with the write
  or the refusal; the first one only spares Guard, so this is not the "two unsynchronized prechecks"
  B21 rules out. A revocation in the gap costs one pure evaluation and no effect. The floor `invoke`
  has no Guard and runs the transaction alone.
- **Revocation.** Every effective `G` change on the STA (§4.5) and the runtime owner's effective
  role retirement update the same fence synchronously, before they report retirement; neither
  relies on a later `DocumentRetired` or `route-state`. Admission and each revocation therefore
  have one real total order. After admission the frame may finish on the original link and the
  write can still fail: "no write when revocation wins" means no fresh admission after
  revocation, not a rollback, and an already admitted message stays a G₁ effect of the original
  role (AC33 (e)).
- **Status.** None of this exists at `457a9c3b`: `window_router::write_payload`
  (`app_session/window_router.rs:77`) writes the socket and is not a nonblocking gate. `keld-wv`
  cannot name `PrimaryRouterHandle` (keld-core depends on keld-wv, `crates/keld-core/Cargo.toml:26`),
  so keld-core lends each view an opaque fence handle at creation (`view.admission`, §4.10); W3's
  STA update needs the same handle. The fence lands once, with the first of W1 T2 and W3 T3 to
  merge, and the other extends it (§6); that task records one lock order with
  `PrimaryRouterHandle.current`, the existing shutdown transition and the native view lifetime
  (B21 decision 1). If this cross-thread gate cannot be built, B21 stays
  blocking and W1 T2 and T3 do not merge; two unsynchronized prechecks are not a substitute.

Every terminal below is #540's. `G` here means the ticket's generation against the live one.

| Point | Case | `send` | floor `invoke` | Host accounting |
| --- | --- | --- | --- | --- |
| Step 3a | The number is live: a record of this document and form has not yet had its terminal posted | **no post**; `Refusal::LiveSequence` in #540's bounded diagnostics | **no post**, same reason | Nothing reserved or queued; the live record is untouched |
| Step 3a | The number is spent: not greater than the form's watermark and not live | `send-result`, `refused`, `KELD-WV-011` (`:603`) | `invoke-result` error, same code | Nothing reserved or queued |
| Step 3' | `invoke.channel` is not the view's `admitted_channel` | not applicable | `invoke-result` error, `KELD-WV-011` (`:603`), detail `renderer channel is not declared by this build` | Nothing reserved or queued |
| Capture | The committed URL or origin is outside #540's domain (`:196`) | `send-result`, `refused`, `KELD-WV-011` (`:603`) | not applicable | Nothing reserved or queued |
| Capture | No admitted, ready role | `send-result`, `refused`, `KELD-WV-012` (zero attempted write, `:604`) | `invoke-result` error, same code | Nothing reserved or queued |
| Capture | Quota busy | `send-result`, `refused`, `KELD-WV-011` (`:603`) | `invoke-result` error, same code | Nothing reserved or queued |
| Submit | Core's queue is closed | `send-result`, `refused`, `KELD-WV-012` (zero attempted write) | `invoke-result` error, same code | Reservation released once |
| Fence | Document replaced or view retired: lifetime or `G` differs | `stale` on the ledger (`:320`, `:603`); no post, because rule 1 has no target | No post; `invoke-result` has no `stale` form | Reservation released once: the fence proves the work never ran; 0 writes |
| Fence | Role retired or replaced, document live | `send-result`, `route-retired`, `not-dispatched` (the fence proves no write attempt) | `invoke-result` error, `KELD-WV-012` | 0 writes on either link; the charge follows #540 §4.9 (`:535-536`) |
| Fence | Grant miss, ticket still current at the denial's comparison | `send-result`, `refused`, `KELD-GUARD003` (#1191 AC5) | not applicable | Reservation released once; 0 writes |
| Fence | Final FIFO answers `Full` | `send-result`, `refused`, `KELD-WV-011` (W3's eval value waits instead, §4.7) | `invoke-result` error, same code | Reservation released once; 0 writes |
| Fence | Final FIFO answers `Disconnected`: the returned frame proves no write | `send-result`, `route-retired`, `not-dispatched` | `invoke-result` error, `KELD-WV-012` | Reservation released inside the fence; #540's fail-closed link retirement; never a successor |
| After admission | Attempted, partial or unprovable write | `send-result`, `route-retired`, `outcome-unknown` | `invoke-result` error, `KELD-WV-013` (`:605`) | Stream retired, no replay, charge held until quiescence; never `not-dispatched` |
| After admission | Bun's terminal | #540's `send-result` | #540's `invoke-result` | Released by #540's ReleaseConsumed path (`:455-458`) |

Each `send-result` and `invoke-result` passes §4.4 rule 1 at its post. No row promises a post to a
retired role, a dead link or a replaced document; "one terminal" means one logical terminal per
admitted sequence and at most one post, and no other answer carries that sequence while it is live
(step 3a, below). A page that floods raw posts can only exhaust its own role's
CONTROL share, which #540 fails closed by closing admission and retiring the route (`:492`): a
self-denial for that application's role, not a widening.

**One answer per live number (r15, 0monish 4431715702).** #540's `send-result` and `invoke-result`
carry only `document`, the sequence and the outcome (#1122 `d9211a96` `docs/specs/gh540-compat-ipc.md:355-359`). A
refusal of a raw duplicate and the terminal of the record it copies are therefore the same kind and
key on the wire, and no matcher in the bridge can tell them apart. The page shares the bridge's
realm and can always post that number, so the rule goes where the number's state is: the Host, at
step 3a, keyed by view, `G` and form, never by the number alone (#540's W9 names that as the fault:
"Key pending/delivery only by WebviewId or request number", `:753`). The state is STA-owned and
fixed: per document and form, one watermark and at most one live number, set at capture and cleared
when the terminal is posted or dropped, or when the charge ends with no terminal ("bounded outstanding
records/watermarks, not a completed-ID set", `:533`; at most one record per document, `:486`). Step 3a
runs after steps 1–3, so a post that fails identity spends nothing.

- **Live:** a record of this document and form holds the number, and its one terminal has not yet
  been posted or dropped by rule 1 on the STA. No post. Liveness ends at that STA step, not at the
  ledger's release, which #540 places before the `send-result` (`:455-457`): the STA orders a duplicate
  against the post, so a duplicate can never overtake the terminal.
- **Spent:** the number is not greater than the watermark and not live. One ordinary refusal. No
  live record can be confused with it, and this refusal is what ends a bridge that posted a number
  a raw post had already used.
- **Fresh:** the number is greater than the watermark. The watermark moves to it before anything
  can answer, whatever step 3' or capture then decides, so a number that was refused can never
  become live later and be confused with its own refusal. Sequences stay per document, with no
  reset on bind or role restart (`:165`).

**Who owns this, and whether #540 requires a reply.** The wire is unchanged: no kind, field, code or
disposition is added, and the behaviour is W1's Host. #540's text does not require a post here. Its
row says "Host pre-admission invalid/busy/stale" gets a "Bounded typed refusal" (`:603`) and names
no carrier; a `send-result` closes a ledger entry (`:455-457`), and a duplicate has none; a denial
of a void call may be "a bounded internal diagnostic/result" (`:631`). A post would give a live
sequence two terminals on the wire, against the owner's "one terminal per sequence" (§10 Q1
decision) and #540's own "live pending result remains settleable" (W11, `:755`). That `:603` is
satisfied by a diagnostic is this spec's reading, so it stays the #540 owner's to confirm (§10 Q17);
the review's naming of this option is not that approval. T2 and T3 block on the answer.

**What bounds `outstanding`.** Nothing waits on it: `send` is void, and the bit only makes a later
`send` a local `recordBusy`. It ends at the live record's terminal, at a `Spent` refusal, at a
`route-state` that ends the role, or with the document. A live record's terminal is bounded by #540's
5,000 ms from original admission, never renewed, which closes route admission and retires the route
(`:496`). W1 adds no timer. If no notification reaches the bridge because quiescence cannot be
proved, "the document route remains unavailable" (`:566-567`), and the bit costs only the label of a
call no Host would admit anyway.

Every order the page can force. This is availability inside one document, not privilege: a raw post
gains no channel, and the Host's ledger holds the quota whatever the bridge's bits say. The `send`
record's credit is released on #540's ReleaseConsumed path (`:455-458`) and an `event` record's under
rule 4 (exact-key `consumed` and the Host-observed post); neither path reads the rows' posts.

| Order | State owner | Host | Bridge sees | Credit |
| --- | --- | --- | --- | --- |
| Raw duplicate of an in-flight genuine send or floor `invoke` (AC38 (b), (c)) | Host, `Live` | No post, 0 queue, 0 Bun | Nothing; still busy; the genuine terminal settles it once | One record, released once; the duplicate charges nothing |
| Raw `send` takes the bridge's next number, still in flight; the bridge then posts it (AC38 (d)) | Host, `Live` (the raw record) | The bridge's post: no post, 0 Bun | Busy until the raw record's terminal, the number's one answer; it reports that disposition, and its own call is lost (divergence 7). Bound: the 5,000 ms above | One record, the raw one, released once |
| Raw `send` takes the next number and finishes first (AC38 (e)) | Bridge `seen`; Host watermark | Two ordinary sends | The raw result raises `seen`; the bridge posts a later number | Two records in turn, each released once |
| Raw `send` jumps ahead and is in flight; the bridge posts a skipped number (AC38 (f)) | Host, `Spent` | One `refused`, 0 queue, 0 Bun | The refusal settles it: no record will ever answer that number, so without this post the bit would never clear | The raw record's, released once; the bridge's post charged nothing |
| The raw record's terminal crosses the bridge's post of that number (AC38 (m)) | Host, `Spent` | One `refused` after the terminal | The terminal settles it; the refusal matches nothing | The raw record's, released once; the bridge's post charged nothing |
| Raw duplicate after completion (AC33 (g), AC37 (c)) | Host, `Spent` | One `refused`, 0 queue, 0 Bun | Matches nothing: not busy, or `message` has moved | Already released once; nothing charged |
| Duplicate crossing the genuine terminal (AC38 (m)) | STA order of the duplicate and the post | Before the post: as row 1. After it: as row 5 | The terminal, once | Released once, on its own path; the ledger's earlier release does not end `Live` |
| Duplicate across a navigation or `G` change (AC38 (i)) | Host, step 2 | The old `document` is refused with no post; a new document's number is a different key | Old document: gone. New one: its own send, refused busy while the old record stays charged | The old record's ends per the terminal table, once; the duplicate charges nothing |
| Raw `send` while the role's slot is held by another document or window (AC38 (g)) | Host/role ledger | Step 3a spends the number; capture posts `refused`, `KELD-WV-011` | Raises `seen`. If its own post of that number crossed the refusal, it settles on the refusal and that post is `Spent`, so the number never becomes live (AC38 (m)) | Nothing reserved |
| Two windows, same number (AC27, AC38 (h)) | Host, per document | Distinct keys; window 2's is `Fresh`, then refused busy | Window 2: its own `refused`. Window 1: nothing | Window 1's record, released once; window 2 charged nothing |
| Raw `send` with a stale or wrong nonce (AC06, AC38 (j)) | Host, step 2 | No post; watermark unmoved | Nothing | Nothing charged |
| The terminal arrives during teardown (AC38 (k), AC33 (e)) | Ledger owner; rule 1 for the post | The post is dropped | Nothing: the document is gone | Released once on its own path, or held to quiescence if unprovable (`:535-536`); the dropped post releases nothing |
| The role retires and `retired` is lost (AC27, AC38 (l)) | Bridge, at the successor's `ready` | A duplicate of the old record's number stays `Live` | One outcome-unknown diagnostic at `ready` | Held to quiescence, then released once (`:535-536`) |
| Raw duplicate of a `consumed` (AC30 (f3)) | Host, rule 4 | Releases nothing | Nothing | Exact key and post completion, once |

Rejected: a distinct code on the refusal, with the bridge ignoring that code (a new registry code, a
second bridge branch and a CONTROL post per duplicate, and the Host would still have to tell `Live`
from `Spent`; it is Q17's fallback); a bridge timeout on `outstanding` (a second clock, beside #540's
5,000 ms, that settles a send still running); a start number the page cannot predict (`send-result` is
page-visible, so one result reveals it, and #540 gives sequences no secrecy).

Refusal order, as the code above runs it: carriage (non-string message, UTF-16→UTF-8
conversion), wire shape (`decode_wv_link`), an unreadable `Source`, then no-document, then epoch
(`bind` has no epoch: its order is no-document, Source, duplicate), then Source, then the
disposition (`consumed`) or the sequence (step 3a), then the floor channel (`invoke`), then capture, then the fence's comparison; all precede a grant miss (#1191 §4.4 step 2). Before any effect the
Host revalidates the current document and epoch, the event's `Source`, the authenticated Bun role
and the literal grant (§10 Q1(a)): the role and document at the fence, the grant in
`dispatch_privileged` just before the transaction.
`refuse` posts nothing ("no private result to an unauthenticated document", `:601`; for
`Refusal::LiveSequence`, an authenticated post, the reason is step 3a's) and records
only its closed `Refusal` reason in #540's bounded diagnostics (8 fixed records, excess coalesced
or dropped, `:495`). It copies no page byte: no `Source`, `document`, `channel`, `code`, `message`
or payload enters a Host diagnostic (`:619`). A page-chosen `channel` returns only to the page,
inside `KELD-GUARD003`'s frozen text and #540's bound (`:203`). The event's `AdditionalObjects` is
never read.
Startup deadline: when `ContentLoading` qualifies a document the Host starts
the existing `KELD-WV-005` deadline; if no verified `bind` for that document is answered
with its `bound` within it, the window fails with a typed startup error (code allocated by
the error-registry owner; not `KELD-WV-012/013`) — never success, never a retry. A `bound` post
that fails or that rule 1 drops does not count as answered. The deadline ends at the Host's
successful `bound` post; no WebView2 API reports that the bridge received it (rule 4), so a
`bound` lost before the listener existed would leave a not-ready bridge with no typed failure;
§10 B9 names the known way this can happen. An early
`bind` is never held or replayed (#1192 `cd30f997` §4.4, `docs/specs/gh542-native-document-dual-world-binding.md:133`: "zero queued
Bind/Invoke/Send/Consumed"), so a document whose only `bind` came early fails at this
deadline (AC29 (d)). No new wire kind.

`window.keld.invoke(channel: u16, payload)` is admitted through steps 1–3, steps 3a and 3', the same
ticket and fence, and then exactly
as KEL-142 §4.1 admits it on macOS: only the declared Echo channel `1`, a Host check (AC35); it is
**not** routed
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
   never reaches a new page (§10 Q1(a)). It checks the role too (#540: "Revalidate role and
   document under the same queue owner on **every** enqueue"): an `event` or `route-state: ready`
   from a role that has retired is dropped (AC33 (h)), and a framework terminal for the original
   document (`send-result`, `invoke-result`, `route-state: retired`) is not, the same distinction
   W3 §4.7 records for eval.
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
4. **Event-record release (r12, 0monish 4428451730).** On route D the page world that receives
   an `event` reads its `delivery` and can post a byte-valid `consumed` for it with the current
   `document`; it can also predict the next key (`delivery` is a per-document, nonzero, checked
   `u32`, #1122 `d9211a96` `docs/specs/gh540-compat-ipc.md:165,190`). The provenance of a same-world acknowledgment
   cannot be proven, and page acknowledgments are never authority. Reusable credit for one
   Host→renderer `event` record, whatever its consumer disposition, needs **both** halves #540 names ("a matching consumer report
   alone cannot release an active native evaluation", `:322`; "A forged early consumed cannot
   release active native retention", `:533-534`): (a) a `consumed` for that exact key
   (document, delivery), and (b) the Host-observed native completion of that delivery's post —
   on WebView2 the `S_OK` return, on the STA thread, of its `PostWebMessageAsString` after rule
   1's final check. The native half is Host-owned. Microsoft documents no completion for that
   call: "The message is delivered asynchronously. If a navigation occurs before the message is
   posted to the page, the message is discarded" (`PostWebMessageAsJson`, which
   `PostWebMessageAsString` "behaves in exactly the same manner as"), and the pinned binding
   takes only the string (`webview2-com-sys` 0.38.2 `bindings.rs:1886-1889`). So (b) proves the
   Host's handoff, not the page's dispatch: no Host-observable signal shows that a page listener
   finished. A `consumed` that arrives before (b) is a future receipt and releases nothing
   (`:531-533`); the first exact-key `consumed` after (b) that passes rule 5 releases the record
   once; a second
   `consumed` for the same key, or one for a key never issued, releases nothing; none creates
   history. If the post fails or rule 1 drops it, the Host ends the record through #540's own
   stale/refused terminal (`:463-464`), never through a page receipt. If the document is replaced
   after (b) and before any `consumed`, no receipt can arrive, because step 2 refuses the old
   document's posts: the Host ends the record at that revocation as `route-retired`,
   `outcome-unknown` (the old page may have run it; #543 owns that `Deliver` REPLY). A forged `consumed` after
   (b) can release the record only as early as a legitimate immediate `consumed` would; the next
   event may then be posted while the page's listener for this one still runs, and it waits in
   the renderer's task queue (JavaScript run-to-completion, INFERRED; whether a WebView2 nested
   event loop could dispatch it earlier is UNKNOWN, AC30 (f2)). Host-side resources stay bounded
   by the one record (`event_records: 1`); a page can only change the pace of its own deliveries.
5. **Consumer disposition (r14, 0monish 4429800827).** The disposition is a page claim too: a raw
   `consumed` skips the bridge, so #540's limit on codes (`:616-617`) holds only if the Host
   enforces it. `keld-wv` checks the disposition before the receipt reaches the ledger.
   `consumed` and `no-listener` pass as endpoint claims (`:317-318`), and the Host infers nothing
   from them. A `refused`
   `code` must be byte-equal to a member of the **endpoint codec set**: the registry codes
   allocated for #1190's three classes (`UnsupportedValue`, `MalformedValue`, `CodecLimit`, #1190
   `115af9ac` `docs/specs/gh544-endpoint-value-codec.md:233-238`). The test is membership, not a
   prefix: `KELD-COMPAT-001` to `012` exist at `457a9c3b` and are not codec codes, and #540 says
   existing evidence codes are not repurposed (`:642-644`). #1190 allocates no number at its pin,
   so T4 blocks on that allocation (§10 Q16).
   - **Outside the set.** #540 lets a consumer carry only those codes, "not forged WV/GUARD proof"
     (`:616-617`). `:197`'s field domain ("Existing registered KELD code") does not decide this:
     `KELD-GUARD003` is registered, and the decoder checks only the code's shape and bound. So
     the Host treats the receipt as one that names no valid release (`:531-532` lists the others;
     this reading is for the #540 owner, §8): `Refusal::ConsumerCode`, no ledger lookup, no
     release, no history, no post.
     Nothing carries the code onward, so it reaches no caller in any form. The record stays
     charged and ends as any delivery does: the first valid exact-key `consumed` releases it once
     (rule 4), or #540's 5,000 ms missing-receipt bound closes route admission and retires the
     route (`:496`) and the caller gets #540's retirement outcome (`:322`). Both are existing
     forms: no added kind or disposition, and a consumer still supplies no `Stale`, `RouteRetired`
     or certainty (`:320`). A page that withholds `consumed` already reaches that bound, so a
     forged code gives it no new effect.
   - **Inside the set.** The Host forwards `CallError { code, message }` with the **registry's**
     text for that code, not the page's `message`, which the decoder checks only against its
     bounds: a registered code carrying `KELD-GUARD003: denied` would otherwise read as a Host
     proof ("an endpoint report cannot impersonate a host certainty proof", `:197`; owners "use
     bounded registered text rather than reflect arbitrary payloads", `:203`). The bridge's
     report takes the same path, so a detail such as `MalformedValue(reason)`'s stops at the
     renderer, and the rule has no raw-only branch.
   Either way this is an endpoint diagnostic, not a Host fact (AC34, AC36).

### 4.5 Per-document lifecycle (WebView2 events onto #1192 §4.2)

One rule for revocation: **every top-level `ContentLoading` revokes the previous epoch. A
new epoch is minted iff `IsErrorPage == FALSE`, the `NavigationId` equals that of a
`NavigationStarting` (or its `IsRedirected` hop) that rule A″ allowed or a Host attempt
issued, and the document has verified resource identity under #1192 §4.4: for `file:`, W3's
served record for this `NavigationId` (status 200; a `403`, `404`, `413` or `503` credits nothing) names the admitted root and in-root
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
| `ContentLoading`, rule not satisfied (error page, remote allowlisted document, unknown `NavigationId`; under B3's fallback, a non-entry in-root document) | +1 then FailedClosed | Revoke; no epoch; apply `onLoss`, except for a page navigation to an in-root document under B3's fallback or to an in-root target W3 refused (`403`, `404`, `413`, `503`; AC25 (f)/(h)), and for a Host attempt W3 refused for capacity (`413`, `503`): it fails to its caller typed, is never re-issued and consumes no recovery, because a `503` is transient and a page can influence it (W3 §4.5, B7 (xi); AC25 (i)) |
| `document.open()`/`write` in the application document (E5 g: 5/5 replaced, no `NavigationStarting`, no `ContentLoading`) | +0 | **Host-unvetoable limit #2.** Same document, same epoch, same authority; the page blanked itself. Electron's `will-navigate` cannot see it either. No Host action; recorded in the report |
| Hash / `pushState` / cancelled provisional navigation | +0 | Keep nonce and work (AC16) |
| `NavigationCompleted(IsSuccess == TRUE)` for a pending navigation with no `ContentLoading`, unless at least one `SourceChanged(IsNewDocument == FALSE)` and no `SourceChanged(IsNewDocument == TRUE)` is attributed to it (§4.4 Attribution) (e.g. a back/forward-cache restore, if WebView2 raises no `ContentLoading` for it — UNKNOWN on 156 — or a same-URL restore that raises no `SourceChanged`) | +1 then FailedClosed | Treat as a document change: revoke; release nothing held; apply `onLoss` (AC24 cases 5, 7, 8). The same-document case releases the held posts once the pending set is empty (AC24 case 6) |
| `ProcessFailed(RENDER_PROCESS_EXITED)` (main frame) | +1 then FailedClosed | Empty the pending set; revoke; discard held posts (reservations released); apply `onLoss` on the surviving control through the trusted Host load row below (AC24 case 11, AC28 (a)). Microsoft ([process-related events](https://learn.microsoft.com/microsoft-edge/webview2/concepts/process-related-events#handle-process-crashes)): "If the main frame is impacted (`RenderProcessExited`), you can use the `Reload` API to reload content in your controls. Alternatively, you can `Close` and recreate the WebView2 controls." Whether a `Navigate` to the entry recovers the control as `Reload` does is AC28 (a)'s to show, not that text's; for `file:` the trusted load is W3's header navigation, never `Reload()`, and if it does not recover the control this row is revised before T2 merges. The same page says "The content in impacted frames (main or subframe) is replaced with an error page"; whether that raises `NavigationStarting`/`ContentLoading`, and in which order against the trusted load, is UNKNOWN. A `ContentLoading` for that error page mints nothing (no allowed `NavigationId`; `IsErrorPage`), but the "rule not satisfied" row would apply `onLoss` a second time: AC28 (a) records the sequence, and a second `onLoss` for one exit blocks T2's merge until this row is revised; it is not an accepted divergence |
| `ProcessFailed(BROWSER_PROCESS_EXITED)` (r10, 0monish 4427055014) | +1 then terminal, for every window of that browser | Empty every pending set; revoke every epoch; discard every held post (reservations released); **no** `onLoss`, and no `Navigate`, `Reload` or other engine call on any closed controller. Microsoft (same page): "All associated WebView2 controls will be closed and your application must handle recovery from this failure. The WebView2 controls need to be recreated." The session ends through W2's `AbnormalExit` terminal, not a second one: under #1280 r16 D8 (`be9e9e56`) this `ProcessFailed`, for the bound browser instance, latches `AbnormalExit` at once (its handler queues an abnormal receipt that the loop's next `poll` latches) and no rebind is allowed after it, so the end does not wait for `BrowserProcessExited`, whose order relative to `ProcessFailed` is not guaranteed. If that receipt comes first it latches `AbnormalExit` as on main and the later `ProcessFailed` changes nothing, so either order ends the session once with nothing delivered (§10 Q13; AC24 cases 14–15, AC28 (b)) |
| Other `ProcessFailed` kinds | +0 | No action |
| A navigation pending for `NAV_TERMINAL_BOUND` (§4.4 rule 2 (ii)) | +1 then FailedClosed | `Stop()`; empty the pending set; revoke; discard held posts (reservations released); **no** `onLoss`; keep the expired `NavigationId` for a late `ContentLoading` (AC24 cases 10, 12, 13) |
| Trusted Host load (`onLoss: reload-entry`) | +1 before the engine call (the window's entry is always admissible for Host attempts once W3's rule 1 attributes them, rule A″; W3's unproven mode refuses every Host `file:` attempt at +0, W3 B18) | Revoke, then navigate to the entry (for `file:`, W3's header navigation `NavigateWithWebResourceRequest`, never `Reload()`); bounded to **3 automatic recoveries per window lifetime**, then close with a typed error. A recovery W3 refuses, refuses for capacity (`413`, `503`) or cannot attribute fails typed, is never re-issued and does not count toward the three (W3 §4.6 rule 1, B7 (vii), (xi)); the document stays FailedClosed |
| App `loadURL`/reload (a #459 Host attempt) | allowlist pre-check, then +1 before the engine call | The window's entry, whatever its query, is always admissible once W3's rule 1 attributes the attempt; any other off-allowlist target is a typed refusal to the caller at +0 and the live document is untouched; a `file:` `reload()` is a typed refusal until W3 Q22 (c). The +1 commits before a fallible call on purpose: if the attempt fails or is cancelled the old document stays revoked and FailedClosed, with no rollback, and a new trusted load is required (#1192 §4.2, `cd30f997` `docs/specs/gh542-native-document-dual-world-binding.md:112`) |
| Bun role restart | +0 | Keep document and nonce; revoke old correlations. At `route-state: retired`, or at the successor's `ready` if that was lost (§4.3), the bridge settles the old role's pending `send` and floor `invoke` locally as outcome-unknown, with no replay and no refund; sends resume only at the successor's `route-state: ready`, which #540 §4.9 allows only after the old role quiesces (§4.3, AC27) |
| Destroy | terminal | Revoke; numeric id never reused |

Every row above that changes the generation or ends the view updates the final admission fence
synchronously, before it reports retirement, and so does the Bun role row's retirement (§4.4).

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
          "mainDigest": "sha256:…",                // main-code input set (§4.8 rule 3, AC32)
          "classifier": "<keld migrate version>",   // release builds re-run it and compare
          "reachableSinks": ["el:rendererReq"],    // literal sinks reachable from exposures ("*" = page-chosen)
          "flags": [                                // §4.8 rule 2: (a) outbound, (b) inbound, (c) argv, (d) indeterminate
            { "at": "preload.js:96-100", "kind": "page-input-branch" },
            { "at": "preload.js:77-85", "kind": "inbound-filter" }
          ]
        },
        "attestation": {                            // required iff verdict.flags is non-empty (AC19)
          "digest": "sha256:…",                    // must equal preload.digest
          "mainDigest": "sha256:…",                // must equal verdict.mainDigest (AC32)
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
  are W3's (W3 A10; W1 adds its pending-set and generation bookkeeping inside it, §4.10). The
  window's allowlist is `windows.<w>.web.navigation.allow`, `<w>` being the Host's immutable
  binding: keld-core resolves it once at view creation and passes keld-wv W3 T4's one immutable
  per-view policy value, and a view with no bound policy matches nothing (W3 §4.6 rule 2).
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
  its B7 (vi)–(viii) (#1311 `1db53482`), and §4.5 with AC25 (i) its B7 (xi) (W3 r17).
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

**Output contract (r15, 0monish 4431715705).** Approved GH-497 owns what `keld migrate` may write,
and this spec adds no file to that set. Default `keld migrate` only inspects and reports; a file
changes only under `--write`, with a preimage and rollback (#1172 `2044f22e` `docs/specs/gh497-first-proof-migration-output.md:25-26`), so
"writes" and "re-run" below mean `--write`. The output set is `keld.config.ts`,
`keld.permissions.jsonc` and `package.json` (`:50-62`; architecture 04 §2 at `2044f22e`). `keld.build.ts`,
`keld.compat.ts`, a Bun `bunfig.toml` alias and tsconfig/jsconfig Electron mappings are **not
generated** (M10, `:34`, `:73`), and nothing in this spec reads one. The map's one `windows.<w>` row is
`channels` (`:58`). `$keldWindowChannelGrants`, `windows.<w>.preload` and `windows.<w>.web` are not in
it, so T5 cannot write them until §10 B10 clears: every sentence below that has `keld migrate` write
one states T5's target under that amendment, not a permitted first-proof output.

**Window mapping.** Each statically resolvable `new BrowserWindow(opts)` site in the main
process becomes one `windows.<w>` entry. `keld migrate` declares the `electron` package
dependency, the sole resolver (#1172 `2044f22e` `docs/specs/gh497-first-proof-migration-output.md:33,60`), and does not rewrite application
source, so `w` must be computable from the options the facade receives at run time. `w` is
computed by **one TypeScript function in `@keld/electron`**; `keld migrate` executes that module
(not a `keld-cli` reimplementation, and under `--write` only, because M01 allows default analysis
no child process, §10 B10 (ii)) and AC13 runs shared golden vectors through both callers. If `opts.title` is present it MUST be a string literal
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
channel grants, no preload and no navigation policy, so every page-initiated target is cancelled (default-deny, typed diagnostic; #1191 AC1: a missing window
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

> **Gating = narrowing.** For some page input, or for some other state (preload-private
> state, `isTrusted`, a timer, a prior callback, any external input), the preload suppresses,
> refuses, restricts or rewrites what reaches a sink — which channel, which arguments, which
> values — or it keeps
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
>   or `arguments`); and **nothing stands between the exposure and the sink** (r13, 0monish
>   4429155170): the sink call is the only statement on the path, reached on every call. Any
>   other statement or call flags, because it can return or throw first: every conditional
>   (`if`, ternary, `&&`, `||`, `??`, `?.`, `switch`, a default parameter), `throw`, early
>   return, `try`/`catch`, loop, callback, timer, `await`, a second sink call, and a call the
>   analyzer does not follow into (a helper, a property read on page input, a built-in). So does
>   selective exposure: an exposure inside any of those or inside a function body, or after a
>   top-level `return` or `throw`. The one exception is a branch constant folding removes, as for
>   `process.contextIsolated`, the only proof of unconditional equivalence today. What a
>   condition depends on does not matter: page arguments,
>   preload-private state, `isTrusted`, a timer, a prior callback and external input all count,
>   because a raw same-epoch `send` skips the exposed function and every gate inside it.
>   Uncertain means flagged: the window then needs an attestation or stays at `channels: []`.
>   A sink with no path from any exposure (top-level preload code, a preload-only listener) is
>   not page-controlled either: it is flagged and adds no grant (**Grants**, below). Hence any
>   such statement, a page-input transform, a channel computed from page input
>   (`send('app:' + c, ...xs)`), any write, delete or mutating call on a page-input value or
>   anything reachable from it (`m.x = …`, `delete m.x`, `Object.assign(m, …)`,
>   `Object.defineProperty(m, …)`), a constant argument, an omitted or truncated
>   argument (`(c, x) => send(c, x)` drops trailing arguments), a reordered or overwritten
>   argument, and a preload-assigned field on a page-input object (`m.reqId = n++`) each
>   flag the call;
> - **(b)** every inbound value that does not reach a page-facing callback (or the
>   exposure's return value) unchanged and whole: consumed without forwarding
>   (`on('auth', (e, t) => { token = t; })`), filtered on any condition (the value itself,
>   private state, a timer, a prior callback: the same default as (a), with the listener
>   callback itself the one path element a pass-through needs), field-selected, transformed, or
>   truncated (`(e, a) => cb(a)` drops trailing payload arguments);
> - **(c)** every read of `process.argv` — by literal index, search, iteration, spread, or
>   escape of `process`/`process.argv` as a value;
> - **(d)** indeterminate code: dynamic `require`, `eval`/`Function`, computed member access
>   on `ipcRenderer`, aliasing the analyzer cannot resolve (including an aliased or computed
>   read of `process.contextIsolated`), an unresolvable preload path.
>
> A listener's `event` argument is not payload (under the facade it carries no authority),
> so dropping it is not (b). `process.contextIsolated` is constant-folded to `false` before
> reachable sinks are computed (§4.2 divergence 6, AC23), so F-O's branch is gone before the
> path rule runs.

Rejected narrower variants (recorded so they are not re-proposed without new evidence):

- *Count only page-input-dependent branches (r12).* Rejected (r13, 0monish 4429155170): a gate on
  private state (`if (unlocked) send('delete-all', ...xs)`) reads no parameter, so the site would
  pass as pass-through and be granted, and a raw same-epoch `send` would reach the handler that
  the exposed function withholds (AC31).
- *Flag a constant, omitted or truncated argument only when the main-process handler reads
  that position.* Rejected: (1) a later main-process change could make a dropped position live,
  and rule 3's `mainDigest` shows only that main code changed, not which position went live, so
  the flags would have to be recomputed by the analysis reason (2) rejects; (2) finding "the handler" is cross-module data flow
  through dynamic registration, wrappers, routers keyed on payload fields (draw.io's
  `rendererReq` switch), rest/`arguments`, defaults and destructuring, where a miss is a
  silent security false negative while a false positive costs one attestation line.
- *Bind each attestation to a slice of main-process code: the handlers it depends on.* Rejected
  (r13): proving that slice is the data flow the previous item rejects, and a miss is silent.
  The whole set's digest (rule 3) over-invalidates, and an over-invalidation costs a re-attestation.
- *Leave declared preloads out of `mainDigest` because each has its own digest (r13).* Rejected
  (r14, 0monish 4429800825): that digest binds a window to its own preload only, and main can
  import any of them, so a B-only edit could change a handler while A's digests all stay equal. The
  review's other option, proving that main cannot load a declared preload, is the cross-module
  data flow rejected above, and main has no enforced boundary that would hold it at run time.
- *Enforce the attested argument template at the Host→Bun boundary (r13, the alternative in
  0monish 4429155174).* Rejected: payload bytes are Host-opaque (#1190; #1122 `d9211a96` `docs/specs/gh540-compat-ipc.md:195`: "Contents
  are opaque to Rust"), so a Host check needs a second value decoder in Rust, which #540 and #1190
  forbid. It would also cover arity only: the rule (a) transforms and preload-assigned fields, and
  the main-dependent claims of rules (b) and (c), stay attested. The Bun-side form is the
  parked item below; neither replaces rule 3's binding.
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

Consequence, stated plainly: most real preloads declare fixed parameters or wrap a sink in a
condition, so most will be flagged under (a)/(b). For those applications the attestation, not the classifier, is the
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
| F-R | `deleteAll: (...xs) => { if (unlocked) send('delete-all', ...xs); }`, with `button.addEventListener('click', (e) => { if (e.isTrusted) unlocked = true; })` | narrowing (private-state branch; no page-input data flow) | flagged (a) (AC31) |
| F-S | `ping: (...xs) => { if (locked) return; send('ping', ...xs); }` | narrowing (early return on private state) | flagged (a) |
| F-T | `later: (...xs) => { setTimeout(() => send('later', ...xs), 0); }` | timer on the path; equivalence not proved | flagged (a) (conservative) |
| F-U | `safe: (...xs) => { try { send('safe', ...xs); } catch (err) { log(err); } }` | `try`/`catch` on the path; equivalence not proved | flagged (a) (conservative) |
| F-V | `thrice: (...xs) => { for (let i = 0; i < 3; i++) send('thrice', ...xs); }` | loop on the path | flagged (a) (conservative) |
| F-W | `if (featureOn) contextBridge.exposeInMainWorld('api', { gated: (...xs) => send('gated', ...xs) });` | selective exposure | flagged (a) |
| F-X | `on('evt', (e, ...v) => { if (unlocked) cb(...v); })` | narrowing (inbound filter on private state) | flagged (b) |
| F-Y | `checked: (...xs) => { ready(() => send('checked', ...xs)); }` | callback on the path; equivalence not proved | flagged (a) (conservative) |
| F-Z | `maybe: (...xs) => { unlocked && send('maybe', ...xs); }` | narrowing (expression condition on private state) | flagged (a) |
| F-Z1 | `guarded: (...xs) => { check(); send('guarded', ...xs); }`, with `const check = () => { if (locked) throw Error('locked'); };` | narrowing (a helper on the path throws first) | flagged (a) |
| F-Z2 | `pair: (...xs) => { send('audit', ...xs); send('delete', ...xs); }` | narrowing (a second sink; a raw `send` can skip the first) | flagged (a) |
| F-Z3 | `ipcRenderer.send('boot');` at preload top level, with `ipcMain.on('boot')` registered | sink with no exposure path | flagged (a); adds no grant |
| F-O | `if (process.contextIsolated) contextBridge.exposeInMainWorld('api', { priv: (...xs) => send('priv', ...xs) }); else window.api = { pub: (...xs) => send('pub', ...xs) };` | branch on isolation; constant-folded to `false` | not flagged; `reachableSinks == ["el:pub"]` (AC23) |

F-C, F-H and F-L use the supported sink (`send`) and listener (`on`): a reachable `invoke` is
today a typed incompatibility, not a sink (r8), so their `invoke` forms become fixtures in the
#1154 revision that implements `invoke` (r10 sweep). Under the r13 default every rule (a)–(d)
row keeps its outcome: F-A's branch now flags without its page-input edge, F-B, F-B1 and F-B2
have nothing else on the path, and F-O's branch is folded away before the path rule runs.

**Rule 3 — recorded verdict and attestation (trust model stated plainly).** The preload
digest, the main-code digest, the classifier verdict and any attestation are a **developer-declared integrity
check on `keld migrate` output**, stored in `keld.permissions.jsonc` under the window
(`windows.<w>.preload`, §4.6) with **the same trust as the manifest itself** — they are not
a Keld-reviewed signature and no Keld party vouches for them. The Host enforces them
mechanically:

- The verdict carries its own `digest`, its `mainDigest` (below) and the classifier version that
  produced it; the Host
  refuses a verdict whose `digest` differs from `preload.digest` (AC19 case vi), and a release
  build re-runs the classifier on the admitted bytes, the main-code input set and the
  `@keld/electron` version the build resolves (it defines the sink set) and refuses to package a
  window whose result differs from the recorded verdict. So a developer who edits the preload and
  hand-updates `preload.digest` without re-running `keld migrate` gets no grants, not stale
  ones.
- Route-D grants load for `w` only when `preload.digest` **and** a recorded `preload.verdict`
  (flag sites, the reachable literal sink set and `mainDigest`) are present and the admitted preload bytes
  hash to `preload.digest`; otherwise `w` gets `channels: []`.
- The Host checks `channels(w)` ⊆ `verdict.reachableSinks` (or `reachableSinks` is `"*"`,
  the page-chosen-channel case); a grant outside that set rejects the window's grants.
- A verdict with non-empty `flags` requires `preload.attestation` whose `digest` equals
  `preload.digest`, whose `mainDigest` equals `verdict.mainDigest` and whose `sites` cover every
  flag (AC19, AC32); an attestation bound to another digest of either kind is refused. `acknowledgedBy` is a developer acknowledgement and conveys no
  authority. An attestation **never widens grants**: `channels(w)` stays exactly the §4.8 **Grants**
  intersection below.
- **Main-code binding (r13, 0monish 4429155174).** What a flagged site can safely lose depends
  on main-process code: an omitted argument is harmless only while no reachable handler reads it,
  argv content comes from `additionalArguments`, and an inbound value is whatever main sends. The
  verdict depends on it too: `channels` and F-P's `reachableSinks` come from `ipcMain` literals. A
  site cannot be proven independent of main, and a narrower handler slice cannot be proven (above),
  so `verdict` and `attestation` each record one `mainDigest` over the whole **main-code input
  set**: every regular file `keld build` takes as input for the package, before the app's bundler
  runs, whatever its extension. Main can load or read any file the package carries at run time,
  renderer bundles included, so an extension list would be a second owner of what can run. The set
  is inputs, not package members, because the members do not exist when `keld migrate` runs:
  `keld build` is "app bundle via the app's bundler → `keld-pack`" and "MUST stamp the transport
  file it stages" (arch 06 §2), and `keld-pack` adds members that are not application code
  (`keld-host.exe`, `keld-updater-helper.exe`, `.keld/update-policy.v1`). KEL-19 owns the list
  (§10 Q15) as a read-only census: no file `keld migrate` generates defines it, and the classifier
  reads `ipcMain` literals from the same inputs. The set also holds
  every declared preload file readable when the digest is computed, of every window, even one
  that list omits (r14, 0monish 4429800825):
  main can import or read a preload as application code, and a preload's own digest is checked
  for its own window only, so without this an edit to window B's preload could change a handler
  behind A's attestation. The set excludes exactly one file, `keld.permissions.jsonc`, the only
  file that records a digest. That leaves no self-reference: `preload.digest`, `verdict.mainDigest`
  and `attestation.mainDigest` all live in that file, and `keld migrate` does not rewrite
  application source, so the digests depend on the preload bytes and never the reverse.
  The other files in GH-497's output set
  (`keld.config.ts`, `package.json`; #1172 `2044f22e` `docs/specs/gh497-first-proof-migration-output.md:50-62`) carry no digest and
  stay in if they are inputs, so `--write` computes `mainDigest` after its other writes, default
  analysis computes it read-only over the same planned bytes and labels it planned, and a second
  run over an unchanged tree reproduces it. `mainDigest` is SHA-256 over the sorted (UTF-8
  byte order) lines of root-relative `/`-separated path and file SHA-256; one function computes it.
  **Who computes it (r15).** At migration analysis, `keld migrate`'s analyzer reads the inputs and
  writes and runs nothing (M01). At release, the recompute of point (2) below runs through
  KEL-19's `keld build`. Both call that one function over KEL-19's census, so once §10 Q15 lands a
  tree holding only GH-497's three outputs has a reproducible digest (AC39 runs on a fixture list
  until then). It is not `keld-pack`'s package
  digest, whose input contains the manifest that would record it.
  Enforcement reuses rule 3's two existing points and owners, the Host's digest check and the
  release recompute, and adds no hashing at boot: (1) the one currency predicate, the Host's
  (T3) and `keld migrate`'s (T5), refuses an attestation whose `mainDigest` differs from
  `verdict.mainDigest`: `channels: []`, typed error, fix: re-attest; (2) the release recompute
  (AC19 (vii)'s owner, T5, called from KEL-19's `keld build`) refuses to package a window whose
  recorded `verdict.mainDigest` differs from the input set. It runs on `keld build`'s private
  staged copy of the inputs and of every declared preload, the bytes the bundler then reads, so a
  file edited after the check cannot reach the package, and a declared preload readable in only
  one of the two runs changes the digest, so the recompute refuses it. Nothing after `keld build` re-checks: signing rewrites package bytes,
  not inputs, and post-package tamper stays §10 B4's. Mismatch invalidates the attestation and the
  grant stays off until re-attested.
  **Cost:** any change to that set, even a renderer bundle or an unrelated handler's file, needs a
  new attestation before a flagged window gets grants again, and a tree that hashes differently on
  another machine (a line-ending conversion, a platform-specific dependency) fails closed at the
  recompute. That includes any preload edit, in any window (r14): one edit takes the grants of
  every flagged window off until each is re-attested, and every unflagged window needs a `keld
  migrate` re-run before the recompute passes. This cost is accepted; the alternative is a stale
  attestation nothing detects. `keld migrate` **cannot renew it
  mechanically**, not even for sites a change seems not to touch: deciding that is the
  cross-module data flow rejected above, and a miss is a silent false negative. It re-runs the
  verdict, keeps the site text, lists every site for review and leaves `channels: []`; only the
  developer's explicit re-acknowledgement (its form is T5's) writes the new
  `attestation.mainDigest`. An unflagged window has no attestation, so re-running `keld migrate`
  renews its verdict with no review.
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
withheld, and nothing in Keld detects it. The same holds for a re-attestation after a
main-process change, and for anything `mainDigest` cannot see: data main reads at run time,
code loaded from outside the package, what the app's bundler adds to its output (the digest binds
the bundler's inputs), and any run outside release packaging (point (2) of the main-code binding
does not run under `keld dev`, and the Host cannot tell whether a package went through it). Keld reviews **this mechanism** under the
permission-model gate (§8), not each application's attestation. The single exception is
Keld's own qualification claim: the T6 attestation for the pinned qualification application
is authored by the W1 implementer and independently reviewed under the permission-model gate
as qualification evidence (§6 T6), because Keld publishes a result that rests on it.

Qualification evidence, stated plainly. Rule 2 applied to the pinned draw.io preload
(`jgraph/drawio-desktop@2edf9fb9`, `src/main/electron-preload.js`, blob `44f2f22f`, read
directly) flags seven sites. r13's default adds none: every extra statement on an
exposure-to-sink path is inside `request` (L89–103, L96 among them), already flagged;
`sendMessage` (L111–114) is the sink alone and keeps its truncation flag; both
`exposeInMainWorld` calls (L87, L125) are unconditional, with no top-level `return` or `throw`
before them; and L46's `if` guards no sink. Its main-process counterpart is `src/main/electron.js` (blob
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
only with an attestation for its digest and its main-code digest covering all seven sites** — through the framework
mechanism of rule 3, not an application exception. Whether draw.io holds any authority
beyond `reqId` assignment depends on its main-process handlers ignoring trailing
arguments; that is what the T6 review reads, and it is not established here. Under GH-497 M07 as
written, `sendMessage` (L111–114) adds no grant (§10 B10 (iii)); whether draw.io's required behaviour
then survives on its literal `rendererReq` path is T6's to show, and is not established here either.

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
explicitly. GH-497 M07 fails "computed, renderer-derived or unioned channels"
(#1172 `2044f22e` `docs/specs/gh497-first-proof-migration-output.md:31`), so the page-chosen and computed terms of this paragraph hold only
under §10 B10 (iii)'s amendment; without it those sinks add no grant and the report names them.
The report names every `ipcMain` literal left ungranted and why (for example
`invoke`-only, unsupported on route D). The #1154 revision that adds `invoke` must also make
sinks and grants kind-qualified (or add a Host-side kind check) and record
`verdict.reachableSinks` per kind: today an `el:<c>` grant and a `reachableSinks` entry (or
`"*"`) carry no kind, so neither distinguishes a `send` from an `invoke` on that channel. Dynamic
`ipcMain` names are reported ungranted. The report also lists every preload read of
`process.contextIsolated` (AC23). A sink reachable only from preload-private code adds no grant and the report names it, so the
preload's own call is refused rather than a raw page post gaining it. A flagged `P`
**without** an attestation for its
digest and main-code digest yields `channels: []` and the report line `✘ preload has N flagged sites that may
narrow authority or hold Host-delivered data (<file>:<line> <rule>, …) — unsupported on Windows route D without an attestation for digest
<d> and main digest <m>; escalation: route B (no approved spec)`. A flagged `P` **with** a matching attestation
yields the same intersection as a pass-through `P` — never more. `keld migrate` writes
`windows.<w>.preload.digest` (SHA-256 of `P`'s bytes, #1155 AC08 digest),
`windows.<w>.preload.verdict` (`digest`, `mainDigest`, `classifier`, `reachableSinks`, `flags`) and, when the developer supplies
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
  on Windows; `keld migrate` report/manifest keys (GH-497's closed map, §10 B10); `WebEngine`/`WebviewSpec` extension
  (window key, admitted preload bytes + digest, the lent admission-fence handle and the admitted
  floor channel id of §4.4; the
  per-view navigation policy value is W3 T4's, and W1's window key selects it) — trait review, live
  backend in the same PR.
- **Platform:** Windows only; macOS keeps #1155/KEL-142; Linux UNKNOWN.
- **Runtime seam:** `create` additionally registers `WebMessageReceived` identity and the
  document-created script inside the `GuardInstalled` mint, then navigates. One handler per
  WebView2 event and view: W1's `NavigationStarting` hold and generation bookkeeping, its
  `ContentLoading` epoch and its `SourceChanged` attribution run inside W3's registrations (W3
  owns the `SetCancel` decision: rule 1 and B19's cancel in W3 T2, rule 2's allow forms in
  W3 T4; #1311 §6, §8), and its `ProcessFailed` kind dispatch inside the
  per-view handler W2 r16 D8 registers (`be9e9e56`); W1 adds no second registration. STA thread; renderer crash ≠ Host;
  handlers live for the WebView lifetime; manifest and policy snapshotted once (#1191 AC1), the
  navigation policy as W3 T4's one immutable per-view value (W3 §4.6 rule 2). W1's
  generation bookkeeping updates the fence synchronously inside those handlers (§4.4, §4.5).
- **Migration unit:** `WebView2Engine::create`, `WebviewSpec`, `app_session` forwarder and fence,
  `@keld/electron` facade/transport, the `WindowRequest::Create` codec (window key, preload path),
  `keld migrate`. No temporary adapter; no persisted
  state beyond the manifest. Compatibility fallback: not required.

## 5. Boundaries

- Implement in: `crates/keld-wv/src/webview2/` (identity, epoch, W1's bookkeeping inside W3's navigation handlers, script
  registration; one owner for the `GuardInstalled` mint); `crates/keld-wv/src/engine.rs`;
  `crates/keld-core/src/app_session.rs` (**the** forwarder, shared with #1191 T4, and the
  final admission fence on `PrimaryRouterHandle`, shared with W3 §4.7; all
  `dispatch_privileged` calls live here); `packages/@keld/electron` (facade + Windows
  transport + `BrowserWindow` window key); `crates/keld-ipc/src/window.rs` +
  `tests/fixtures/window-v0.tsv` and `packages/@keld/api/src/window-codec.ts` (window-key and
  preload-path fields only); `crates/keld-cli` (`migrate`, and the Windows hop in `tests/channel_table_scan.rs`); `crates/keld-guard` URL helpers only (`url_scheme_is`,
  `https_tuple_origin`; W3 owns the parser, the component equality and the navigation
  entrypoint); `docs/architecture/
  03,04,05` (T0 deviations); `keld-compat` conformance/scoreboard entries (T1).
- Must not touch: `crates/keld-guard` grammar/evaluator (#1191/#541); `crates/keld-ipc`
  framing; the KIPC channel table (#508); macOS `wkwebview/*`; workspace `Cargo.toml`;
  `keld.config.ts` schema (GH-497's closed map holds its keys; W1 adds none, and `w` lives in
  `keld.permissions.jsonc`).

## 6. Tasks (each ≈ one PR; ordered; owner; blocking dependency)

- [ ] **T0 — this spec + architecture deviations** (owner: @amishabenramani). Blocks only on
  items that can finish before implementation, each with its owner: KEL-142 owner sign-off of the
  §5.1 Windows amendment (B1); the #1184 owner's tracker amendment (B2); the #1192 owner's answers
  (B3); an assigned owner and named design for the trust root (B4); each draft's approval (B5);
  the recorded loading-path decision and allow forms (B6: W3 #1185, #1192/KEL-102, #541); the
  discriminating admission probe, on E5's probe host (B7: W1 implementer); the OCR rerun (B8);
  the #540 wire owner's choice of B9's branch; the GH-497 owner's choice of B10's branch; and the
  owner answers to §10 Q2, Q14 and Q17 (§10 Q1 is RESOLVED). **Not T0 gates:** clearing B9 (branch (i)'s evidence is a T2 cell), W3 Q22 and Q27
  (B6's device gates) and implementing the trust root (B4). T0 → T1 → T2 is the order, so a T0 edge
  on any of them would point backwards; each stays a pre-merge gate of T2, listed there.
- [ ] **T1 — conformance entries first** (owner: `keld-compat` owner, #464/#567). Expected-fail
  cells and one scoreboard divergence entry per §4.2 accepted divergence (1–11), plus the
  `about:srcdoc`/`document.open` limits. Blocks on T0.
- [ ] **T2 — loading path, policy selection, eligibility and epoch in `keld-wv`** (owner:
  Windows W1 implementer). Three PRs, each with its own gates, in this order: T2a, the
  `WindowRequest::Create` fields and their Host binding (GH-531 wire gate); T2b, the final admission
  fence, its lent handle and its pure cells, unless W3 T3 landed it first (public API gate); T2c,
  the rest. Replace `NavTarget::Html` for compat windows with the decided
  Host-identifiable URL (§4.6a); AC01 (`window.keld.invoke` half), AC03–AC06, AC07 (floor half), AC10–AC12, AC16–AC18, AC20, AC21, AC24, AC25 (epoch, `onLoss` and pure cells), AC26, AC27's restart cell (its floor-`invoke` half), AC28, AC29 (cell (i) blocked on §10 B9), AC33's floor cells, AC35, AC37 (a)–(d), AC38's floor and `invoke` cells; one generation source
  for media and channel principals (KEL-102 owner coordination); the `WindowRequest::Create`
  window-key and preload-path fields with the Host's immutable binding (GH-531 owner review),
  because `w` selects W3 T4's per-view navigation policy here (W3 lands the value and its
  fail-closed default, W1 only the selection, AC10) and T3's grants next, with fixtures
  passing a literal `w` until T4's key function exists; the final admission fence for the floor
  `invoke`, unless W3 T3 landed it first (§4.4); step 3a for `invoke` and the floor's twin of `seen` (§4.4); the floor channel check with the view's
  `admitted_channel` (§4.4, AC35), with the Windows hop added to `BRIDGE_ID_CALLS`; and the Windows post path with §4.4 rules 1–3.
  The Q5 and Q11 probes run in its receipt. Blocks on: W3 #1185 +
  #1192/KEL-102 recording the loading-path decision and #541 the `file:` allow forms (B6);
  the non-entry eligibility decision (B3); #541 approving `web.navigation` and
  `preload.digest` keys (Q2); the Windows resource-identity oracle (B3); the Windows trust
  root (B4); #540's CompatV2 mode of `decode_wv_link` landed (§10 Q1 is RESOLVED; T2 implements its bind→bound order, AC26, AC29); #540's reservation ledger (#1122 §6 T3b) and the #1095 settlement with `KELD-WV-012` and `013` (#1122 §6 T2; neither code exists at `457a9c3b`), which capture and §4.4's terminals need; W3 T2 merged (#1311 `fa49dcfb` §6: the parser, served-record credit, `file:` handler, rule 1's attribution and B19's cancel) and W3 T4 merged (rule 2's allow forms, rule 4's pre-check and the per-view policy value), W3's execution order being T2 → T4 → T3 with the numbers unchanged (a W3 author decision that `fa49dcfb` does not yet state), with W3 Q22 (a) and (b) passed on the supported runtime (W3 B18, B19; without them no Windows `file:` application ships, §10 B6); W2 #1280 T1 (Ready without a window, behind its debug test admission), T2 (the release proof and product admission of a schema-2 boot, W3 B13; #1280 `8c5e5d74` §6) and T3 (managed windows with D8's browser `ProcessFailed` latch, `be9e9e56`, unchanged at `8c5e5d74`), which AC24 cases 14–15 and AC28 need (§10 Q13 (a)); GH-531's review of the `Create` fields; §10 Q14's and Q17's answers; §10 B9 resolved by one of its two branches; #540's role retirement and successor-readiness transition (#1122 §4.9), which AC27's restart cell needs; and the recorded allocation exception for the per-message parse, or an allocation-free parse (§9).
  Floor-only bridge (`window.keld.invoke`). The Host decodes every CompatV2 form, and until T3 and
  T4 land their arms it answers a raw `send` or `consumed` with `refuse(Refusal::NotInThisBuild)`:
  no post, because the `send-result` path is T3's, and no effect (AC37 (d)).
- [ ] **T3 — grant and forward** (owner: `keld-core`/#1191 T4 implementer). AC02, AC09, AC14,
  AC19 (i)–(vi), AC32's Host half ((c), (f1), (f3)), AC33's `send` cells, AC37 (c)'s positive pair, (e), (f)
  and (i), AC38 (m)'s `send` half: the Host-side verdict and
  attestation checks of §4.8 rule 3 land with the first grant (fixture manifests here; `keld
  migrate` output from T5 on), as do the Host's read of the preload bytes at the path T2 bound,
  the `send` use of the final admission fence, step 3a for `send`, the `send-result` post and the
  committed `url` and `origin` stamp. Route-D grants stay
  off in release builds until T5's release-build recompute (AC19 (vii), AC32 (a)) lands.
  Blocks on T2, #1191 T1–T4 merged, #540's CompatV2 mode of `decode_wv_link` landed (§10
  Q1 RESOLVED), #540's `send` slice (#1122 §6 T4: the subscribed `RendererMessage` and the
  ReleaseConsumed path a `send-result` follows) and §10 Q12's and Q17's answers.
- [ ] **T4 — Host `event` delivery and the `@keld/electron` facade on the Windows transport**
  (owners: W1 implementer, then #1154 implementer). Two PRs. T4a: `keld-wv`'s `event` post with
  §4.4 rule 1's role check and rule 4's release, and rule 5's disposition check, here because #543
  is the first thing that can drive them (AC33 (h), AC36, AC37 (h)). T4b, the facade: wrapper execution gated on a top-level `file:` document (§4.3, option (b′)); `process.contextIsolated === false` (AC23 (a));
  divergences wired to T1 entries; the facade passing the window key and preload path in the
  Create request T2 landed, and the one window-key function (§4.8); AC01's exposed keys, AC07's
  void-API half, AC08, AC25's preload-API cells and gate mutants, AC27 (its cross-window, restart and throwing-listener cells included), AC30, AC34, AC37 (g), AC38's remaining cells, which need the bridge's `send`. Blocks on T3, on #540's role retirement and successor-readiness transition (#1122 §4.9) for AC27's restart cell, on #543's event delivery (#1122 §6 T5) and #1190's decoder for AC27, AC30, AC34, AC36 and AC37 (g), (h), on §10 Q16's codec codes for rule 5, AC34, AC36 and AC37 (g), on #466's `ipcMain` listener registry (#1122 §5) and, for
  AC27's `handle` cells, on #547's invoke-handler registry (#466 leaves `handle`/`handleOnce`
  to F04-T14).
- [ ] **T5 — `keld migrate` mapping, classifier, grants, digest, attestation** (owner:
  `keld-cli`/KEL-17 owner; Host-side refusal in T3's forwarder owner). AC13, AC15, AC19 (vii) (the release-build recompute), AC23 (b), AC09's report half, AC31's golden half, AC32's golden half, AC39;
  fixtures F-A..F-Z; corpus run reporting flags by class (§4.8 decision trigger). Blocks on KEL-17 admitting the first live `migrate` slice
  (Q9), on GH-497's T1 and T2, the analyzer and the `--write` path with M10's census (#1172 `2044f22e` `docs/specs/gh497-first-proof-migration-output.md:103-104`),
  on §10 B10 (AC39's `--write` cells and every generated route-D key need it; AC39 (a) and its
  digest vectors do not),
  on T4's window-key function, which `keld migrate` executes (§4.8), on §10 Q15's input list
  and, for AC19 (vii), AC32 (a) and AC39's recompute cells, on KEL-19's `keld build`, the recompute's call site (a reserved
  verb at `457a9c3b`, `crates/keld-cli/src/verb.rs`; `keld-pack` alone only streams packages). That
  recompute is its own PR at that call site; route-D grants stay off in release builds until it
  lands (T3).
- [ ] **T6 — Windows qualification receipt** (owner: @amishabenramani). One corpus application
  whose preload is pass-through or carries an attestation for its preload and main-code digests
  covering every flagged
  site (§4.8 rule 3); full RED matrix including AC22 and the device halves of AC31 and AC32; AC17. Because this is Keld's own
  qualification claim, its attestation (for draw.io: the seven sites of §4.8, bound to the
  pinned preload and main-code digests) is authored by
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
| `wv2_epoch_mint_predicate` (pure Rust) | AC04, AC25 | §4.5's mint predicate over stub inputs, one otherwise-qualifying input per term: `IsErrorPage == TRUE` with a status-200 record; URL text equal to the entry with no verified identity; a `NavigationId` no allowed `NavigationStarting` recorded; a W3 served record with status `404`, `403`, `413` or `503`. Baseline: no epoch for any; each single-term mutant mints one → RED. |
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
| `wv2_preload_document_matrix` (device) + `guard_component_equality` (pure Rust) | AC25 | Device: admitted root `D:/Keld Test/Ünï/`, allowlist `in-root:**` plus one remote scope, cells (a)–(d) loaded by the application's `loadURL`, (e)–(h) by page navigation ((e), (f) and (h) blocked until W3 Q27), (i) under the held budget; per cell the committed `Uri`/`Source` bytes, `location.protocol`, AC01's first-inline oracle and the Host's `bound`-post counter; baseline first and last. Pure Rust: the shared component equality over `index.html`/`INDEX.HTML`/`index%2Ehtml` and its single call site in W1. Five mutants (location-string gate; preload in every document; byte-exact W1 comparison; served record attached regardless of status; capacity refusal counted as a loss) — the fourth decided by `wv2_epoch_mint_predicate` while (f) is blocked. |
| `wv2_navigation_matrix` | AC10 | E5 triggers × destinations plus `data:`/`blob:`/`javascript:` in lower and upper case, 5 veto + 3 control runs each, with/without `SendInput` activation; heartbeat oracle; five mutants (exact denylist; in-root membership treated as sufficient; query cut before page-initiated matching; query folded through the component equality; policy not selected by `w`); the three `file:` mutants run in W3's pure `guard_navigation_allow_forms` while W3 B19 cancels page-initiated `file:` targets on device. |
| `wv2_environment_args_empty` + source grep | AC11 | Unit. |
| `wv2_popup_handled` | AC12 | Existing KEL-168 test. |
| `migrate_grant_generation` | AC13 | Golden manifest for F-B, F-B1, attested F-C and attested F-P (F-B and F-P as AC13 conditions them, §10 B10 (iii)) against a fixed `ipcMain` set, plus the window-key fixtures and shared key-function golden vectors; eight mutants (grant all literals; collect only `on`/`handle`; F-P's computed channel treated as page-chosen; suffix disambiguation; last site wins; key-class prefixes dropped; `sendToHost` counted as a main-IPC sink; every `ipcMain` registration kind treated as reachable by `send`). |
| `wv2_bridge_wire_compat_v2` (device; T2) | AC26 | T2's actual bridge text and admission path, real `decode_wv_link` in CompatV2 mode, the KEL-142 floor; the renderer-side recorder and Host decoded/`bind`-receipt/`bound`-post/Echo counters; pure-Rust `bind`/`bound` vector checks (21/174 bytes); seven refused raw posts (v2 `hello`, v1 `invoke`, base64url `send` payload, v2 `epoch` kind, the page-posted `bound` vector, a duplicated-`document` `send`, a non-string object), each followed by a floor `invoke` issued after the previous `invoke-result`; baseline first and last; eight mutants (LegacyV1 fallback in the endpoint; base64url payload accepted; unknown kind admitted; `hello` aliased to `bind`; r8's v1 bridge text; `get_WebMessageAsJson` read; native kinds accepted from the renderer; a duplicated key overwrites), each in its own process. Runs once #540's CompatV2 mode lands and T2 implements the bind→bound order (§10 Q1 RESOLVED). |
| `wv2_facade_send_trace` (device; T4, needs T3) | AC27 | The AC26 harness plus T3's forwarder, T4's facade and a Bun fixture on #466's listener registry and #547's invoke-handler registry with per-listener and per-handler counters and the #540 ledger counter; asserts the 123-byte `send` and 120-byte `consumed` vectors (pure Rust), the per-direction renderer traces, the early second `send` a dropped call (local `recordBusy`, no post, no Bun call, no credit), the cross-window same-role `send` refused by the Host/role ledger with its `send-result`, `send-result` before message 2, readiness from `bound.ready`/`route-state`, `on('x')` 1 and `handle('x')` 0, and the `danger` and r8-exposure refusals, the restart cell (outcome-unknown settlement at `retired`, message 2 admitted once after the successor's `ready`, the floor `invoke` rejected and then resolved), the throwing-listener cell (one `consumed` per event, the `error` counter, no charged record left) and the refused send's `send-result`; eleven mutants (#547's handler registry merged into #466's listener registry; bridge skips its `outstanding` check; bridge never clears its record on `send-result`; bridge ignores `bound.ready`; bridge drops `consumed`; DATA ledger keyed per document instead of per role; `outstanding` cleared only by `send-result`; `consumed` posted only after a normal return; listener exception swallowed; no `send-result` for a grant-refused `send`; no `send-result` for a role-quota refusal). |
| `wv2_bind_bound_startup` (device; T2) | AC29 | Cells (a)–(i) on the dedicated STA harness, baseline first and last; renderer recorder plus Host counters for `bind` receipts (with `Source`), `bound` posts (with target document and the `bind` answered), mints, typed startup failures, retries and Echo calls; the replay and held-`bound` hooks are `profile-test-hooks` fixtures and the deferred, listener-less and bind-less bridges are fixture builds; cell (i) also records the native at-risk count without the hook; nine mutants (`bind` never answered; `bound` pushed at `ContentLoading`; no startup deadline; early `bind` held and answered; `bind` Source check skipped; fresh mint per `bind`; final-post check skipped for `bound`; per-WebView `bind` latch; A's stale same-root `bind` takes B's latch, today's rule, so that cell fails first until §10 B9), each in its own process. A timeout with no `bound` is a typed startup failure, never a pass. |
| `wv2_nonce_grants_nothing` (device; T4) | AC30 | Cells (a)–(e) are the named ACs' tests, run against a page that read the nonce from its own raw listener; cells (f1)–(f6) add the forged `consumed`, dropped-post and replaced-document cases on the AC27 harness with the held-post hook, the #540 ledger release counter, the Host event-record high-water and the renderer dispatch log; five mutants (release on the page `consumed` alone before the native half; release on every exact-key `consumed`; release on a `document` match without the exact key; release a dropped post only on a page `consumed`; keep a replaced document's record for a page `consumed`); (f2) records the limit and has none. |
| `wv2_private_state_gate` (device; T6, needs T4's facade and T5's manifest) + the F-R row of `migrate_classifier_fixtures` (golden; T5) | AC31 | Golden: F-R's flag, verdict, empty grants and report line. Device: no synthesized trusted input; exposed call, then a raw v2 `send` with the current epoch; Bun handler counter keyed by channel; the attested cell is recorded as the residual, not scored. One mutant (only page-input branches count), which needs `ipcMain.on('delete-all')` registered so that its grant exists: at-risk count above 0. |
| `migrate_main_code_binding` (pure; T5; the Host check of (c), (f1) and (f3) T3) + `wv2_stale_attestation_refused` (device; T6) | AC32 | Pure: v1/v2 fixture trees through the main-digest function, the currency predicate and the release recompute; digest vectors for a helper, a renderer bundle, an input with no script extension, a preload, `keld.permissions.jsonc`, listing order and a second `keld migrate` run over one tree, and cell (f)'s two-window tree (the input list is a fixture parameter until §10 Q15 lands). Device: cells (c), (f1) and (f3) with a Bun fixture that counts the protected overwrite, then cells (d) and (f4). Seven mutants (preload digest only; recompute without the main comparison; `keld migrate` self-renews; entry file only; extension filter; manifest kept in the set; declared preloads excluded). |
| `wv2_admission_fence` (pure Rust; floor T2, `send` T3) + `wv2_final_fence_trace` (device; same split, (h) T4) | AC33 | Pure: the fence over stub owners and a stub FIFO, with deterministic barriers before and after the transaction and inside the stub evaluator, each revocation in both orders. Device: the held-admission hook, (a)–(e), (g) and (h), baseline first and last, independent per-link and per-role counters; (c) waits on the successor main's registration report. Thirteen mutants (no final comparison; `G` only; role only; no comparison before Guard; denial selected without a comparison; retarget to the live link; role captured at dequeue; fence left before `try_send`; bridge `ready` trusted; sequence unchecked at step 3a; `invoke` not reserved; rule 1 without the role; `Disconnected` answered as `Full`); the `G`-only, denial, fence-left and `Disconnected` mutants are pure only. |
| `wv2_event_decode_terminal` (device; T4) | AC34 | AC27's harness with a Bun fixture that issues `Deliver` with raw bytes; cells (a)–(e); `consumed` recorder, listener counters, page `error` counter, ledger release counter and the `Deliver` REPLY's disposition. Four mutants (one `catch` for everything; decode error rethrown; listeners looked up before decoding; no-dispatch reported as `consumed`). |
| `wv2_floor_channel` (device; T2) + `wv2_floor_channel_admission` (pure Rust; T2) | AC35 | Device: raw `invoke`s on channels `2` and `3`, alone and while a floor `invoke` is held, on the dedicated STA harness, baseline first and last, with per-channel app-link and Bun frame counters; the deciding observable is the Host's writer counter, because KEL-142's `channels.handle` admits no fixture handler on those ids. Pure: step 3' over a view with id 7. Two mutants (check omitted; literal `1`). |
| `wv2_consumer_disposition` (pure Rust; T4) + `wv2_consumer_forgery` (device; T4) | AC36 | Pure: rule 5's check and the ledger over code vectors, with a stub clock for #540's 5,000 ms bound. Device: AC27's harness, a raw `consumed` posted from a page-supplied listener inside dispatch; (d) with the fixture bridge build; refusal, release and REPLY counters and a scan of every string the Bun caller receives. Four mutants (any bounded code forwarded; `KELD-COMPAT-` prefix test; release on a refused receipt; page `message` forwarded). |
| `wv2_raw_envelope_fields` (pure Rust and device; (a)–(d) T2, (e), (f), (i) T3, (g), (h) T4) | AC37 | One raw post per field value, each followed by a floor `invoke` issued after the previous `invoke-result`; the decoder cells also run in pure Rust against #1122 §7.1's vectors, and (g) on the Bun fixture's listener counter. Baseline first and last. Nine mutants (unknown keys ignored; zero sequence accepted; CONTROL forms capped only at 65,536 bytes; `consumed` decoded with the terminal type; one shared sequence; sequence checked for `send` only; T2 forwards a decoded `send`; Bun's receiving adapter skips its 17-argument refusal; `url` stamped from `Source`). |
| `wv2_sequence_classification` (pure Rust; `invoke` T2, `send` T3) + `wv2_duplicate_orderings` (device; (c) and (e)'s floor half T2, the rest T4) | AC38 | Pure: step 3a over a stub ledger, a stub STA post and a stub bridge, each crossing in both orders with deterministic barriers. Device: AC27's and AC33's hooks, the replay hook for (i), the suppression hook for (l), baseline first and last, counters keyed by document and sequence. (a) is the positive control; the recorder's raw-number log is the at-risk count of (b)–(h). Nine mutants (reply to a live duplicate; step 3' before step 3a; silent on `Spent`; `Live` keyed by the number alone; watermark moved only on admission; classification before step 2; `Live` ended at the ledger's release; `seen` removed; settlement on `retired` only), each in its own process; the watermark and ledger-release mutants are decided in pure Rust, because a device cannot force those crossings. |
| `migrate_digest_census` (T5; `--write` cells need GH-497 T2 and §10 B10 (i); recompute cells need KEL-19's `keld build`) | AC39 | Temporary real directory holding only GH-497's three outputs and the application's inputs; M01's and M10's own censuses, reused, not copied; the list is a fixture parameter until §10 Q15 lands. Four mutants (a forbidden writer, one build per forbidden output; list read from a `keld.build.ts`; preloads left out; helper left out). The preload and helper mutants overlap AC32's and run here on the three-file tree. |
| `wv2_process_failure_split` (device; T2) | AC28 | Two runs per kind on the dedicated STA harness, baseline first and last; renderer and browser terminated by `profile-test-hooks` fixtures; navigation-event log, per-controller engine-call log, Echo counter, which of `ProcessFailed` and `BrowserProcessExited` came first, session-end record; two mutants (`onLoss` after browser exit, a positive control decided by AC24 case 14; renderer exit treated as browser exit), each in its own process. |
| `wv2_digest_binding` | AC14 | Tamper one preload byte after its digest was recorded (a manifest fixture in T3; `keld migrate` output from T6 on); assert `channels: []` + typed error; no-preload and no-entry cells; two mutants (digest ignored; comparison only when a preload was passed). |
| `migrate_classifier_fixtures` | AC15, AC23(b) | F-A..F-Z + pinned draw.io preload bytes (expected: exactly the seven sites of §4.8); one mutant per rule plus the over-flag and F-G conservatism mutants. |
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
  sinks, same-digest requirement for the preload and main-code digests) is reviewed here — third-party attestations are not; the
  T6 qualification attestation is reviewed here as qualification evidence (§6 T6); the
  Host→renderer generation check and in-flight hold (§4.4, AC21, AC24); the per-call Source
  rule's use of the shared `file:` URL parser (§4.4 step 3); rule A″ adopted from W3 (§4.6);
  the one component equality; the proposed eligibility of every Host-served in-root document
  and the preload gate (§4.3, §4.5, AC25); the classifier's any-condition default (§4.8 rule 2,
  AC31); the final admission ticket and fence, including its Guard ordering (§4.4, AC33, §10 Q14);
  the Host's floor-channel and consumer-disposition checks (§4.4 step 3', rule 5, AC35, AC36);
  `mainDigest` over every declared preload (§4.8 rule 3, AC32).
- **Public API — mandatory:** `WebEngine`/`WebviewSpec`, with the lent fence handle and the
  admitted channel id;
  `@keld/electron` facade on Windows;
  `keld migrate` keys and report, with the GH-497 amendment of §10 B10; KEL-142 §5.1 Windows amendment; `keld-guard` URL helpers
  `url_scheme_is` (R1's code, exported) and `https_tuple_origin` (keld-guard owner; W3 owns
  the parser, the component equality and the navigation entrypoint).
- **Wire protocol — #540 owner:** WebMessage string carriage of the unchanged CompatV2 forms
  (no added kind, `JsonBytes` payloads, one decoder), `bound` answering a verified `bind`, the
  page-readable `document` nonce, epoch-stamped Host→renderer envelopes (§10 Q1, RESOLVED;
  the decision approves no W1 text), and, if §10 B9 takes branch (ii), the bounded idempotent
  `bound` re-answer (open). The renderer use of the B21 ticket and fence (§10 Q14, open) and the
  §4.4 terminal table add no wire form; they map onto #540's existing dispositions and need that
  owner's reading. So does §4.4 rule 5, which adds no form either: it narrows what the Host forwards
  to registered codes with the registry's text, and it refuses a receipt outside that set and
  leaves the record charged, a case #540's list of receipts that release nothing does not name
  (#1122 `d9211a96` `docs/specs/gh540-compat-ipc.md:531-532`). So does step 3a, which adds no form:
  no post for a live number, and a spent number once a refusal has used it (§10 Q17, open).
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
- Per message at step 3a (r15): two `u32` comparisons on the STA, with no allocation.
- Per message at the fence (r13): one short shared guard on the STA at capture and, on Core, one
  before Guard and one at admission, so an STA G-change can briefly contend with a writer admission. No allocation is
  added on the warmed path, the guard is held across no I/O, and T2 reports capture and fence
  hold times as separate samples. Hot-path rules (no async, no steady-state allocation) apply.
- `mainDigest` is computed by `keld migrate` and the release recompute only: no boot, first-paint
  or per-message cost, and none is claimed for the release build until T5 measures it.
- Per navigation: nonce mint + one `bound` post in reply to `bind` (at most the fixed re-answer
  count if §10 B9 takes branch (ii)); negligible, measured anyway.

## 10. Approval blockers and open questions

**Blockers (must clear before `Status: approved` as far as §6 T0 lists them; what only a later task can clear stays that task's gate):**

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

  > **(Proposed amendment for Windows route D; effective only with #1307's exact-content approval, until then the previous text below applies — see https://github.com/gyldlab/keld/issues/1184#issuecomment-6512851695.)** Prove the three route-D rules that replace world separation. **(i) No IPC authority or Host-delivered data exists only in preload code.** `keld migrate`'s conservative classifier flags every preload whose sink calls are not wholly page-controlled (any other statement or call on a path from the exposure to the sink, whether a conditional, early return, throw, try/catch, loop, callback, timer or helper and whatever a condition depends on, and selective exposure, unless constant folding removes it; page-input transforms; channels computed from page input; constant, omitted, truncated or overwritten arguments; writes, deletes or mutating calls on page input, including preload-assigned fields), every inbound listener or `invoke` reply whose payload does not reach a page-facing callback unchanged (including consume-without-forward such as `on('auth', (e, t) => { token = t })`), every `process.argv` read, and indeterminate code. A flagged preload's window receives grants only with a developer attestation bound to the recorded preload digest and to the recorded digest of the main-process code inputs, covering every flagged site and never widening grants; a change to either invalidates it until the developer re-attests; Keld reviews this mechanism, not each attestation, and a wrong attestation silently loses the narrowing. The Host loads grants only when the admitted preload bytes hash to the recorded digest (trust root: #1307 blocker B4), a verdict is recorded, and grants ⊆ `verdict.reachableSinks`. **(ii) The Host authenticates every call:** top-level `WebMessageReceived` only; the current epoch nonce of a document whose identity was decided once at commit; a browser-stamped `Source` whose scheme is that document's (so `about:`, `data:`, `javascript:` and `blob:` are refused) and which, for `https`, has that document's tuple origin and, for `file:`, has an empty host and a percent-decoded, case-folded path inside the verified package root as decided by keld-guard's one `file:` URL parser shared with W3 #1185 (never full-URL equality, never a comparison of opaque origins); and an exact literal `el:` channel grant — except `window.keld.invoke`, which routes only KEL-142's declared Echo channel `1`. The page never receives raw Host objects, and no token other than its own document epoch or an already-revoked one. **(iii) Navigation and eligibility are Host policy:** top-level navigation follows W3 #1185's rule A″, whose verbatim text #1185 owns; in substance: the allowlist governs every page-initiated top-level target, in-root `file:` included, and in-root membership is necessary, not sufficient; the query is part of page-initiated matching; the decision is taken on the original `NavigationStarting`, before any generation change — an off-allowlist target is cancelled at generation +0 and the live document and its IPC survive; `will-navigate` is a notification only, and the Host never replays a page navigation; Host attempts are checked against the same allowlist before their +1, and the window's entry is always admissible for them, whatever its query, once #1185's attribution rule binds the navigation to the attempt. Every `about:`, `data:`, `javascript:` and `blob:` URI in any letter case is off-allowlist (whether `javascript:` raises `NavigationStarting` at all is UNKNOWN, #1307 AC10); `NewWindowRequested` is denied; the runtime's cancellable-`about:` navigation is never disabled; a document receives an epoch only at `ContentLoading` with `IsErrorPage == FALSE`, an allowed `NavigationId` and verified resource identity (#1307 §4.5; never URL text, never a `NavigateToString` load). Top-level `about:srcdoc` (an error page commits despite `Cancel`; FailedClosed with bounded recovery) and `document.open()` (no navigation event; no Host action) are recorded Host-unvetoable limits.

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
  implementer; it runs on E5's keld-research probe host, not on T2's harness, because T0 gates on
  it and T2 follows T0; results to keld-research):
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
- **B9 Stale same-root `bind` (r12, 0monish 4428451720) — OPEN; blocks T2, and T0 only on its branch choice (§6).** *What:* #540's
  `bind` is tokenless (`{ v: 2; kind: "bind" }`, #1122 `d9211a96` `docs/specs/gh540-compat-ipc.md:342`), and the
  `WebMessageReceived` event args expose only `Source` and the message: Microsoft Learn lists
  `get_Source` ("The URI of the document that sent this web message."), `get_WebMessageAsJson`
  and `TryGetWebMessageAsString`, and the pinned `webview2-com-sys` 0.38.2 vtable has exactly
  those (`src/bindings.rs:37299-37313`; `ICoreWebView2WebMessageReceivedEventArgs2` adds only
  `AdditionalObjects`, `:37420-37426`). So a late `bind` from an old document A under the same
  root, or at the same URL, passes §4.4's Source check as the new document B's own would and
  takes B's one-use latch. No field identifies the document instance; W1 adds no field and no
  second decoder. *Impact:* availability, not security. A's `bind` is answered with B's own
  `bound`, and rule 1's final-post target revalidation keeps it from reaching any other
  document; nothing is minted, replenished or re-roled and no authority moves. But if that post
  precedes B's listener and is lost (whether a post made before the listener exists is
  delivered is UNKNOWN, AC29 (b)), B's own `bind` is refused as a duplicate and B's bridge stays
  not ready: every call refused, fail closed. Because §4.4's startup deadline ends at the
  successful `bound` post, this is not a typed startup failure: the window stays open with a
  dead bridge. Whether WebView2 delivers an old document's message after the next
  `ContentLoading` at all is UNKNOWN (E5's at-risk set was empty, B7). *What can proceed:* T0's
  text, T1 and every T2 cell except AC29 (i). *Resolution — T2 stays blocked until one holds:*
  (i) device evidence shows WebView2 never delivers an old document's WebMessage after the next
  `ContentLoading`: a documented WebView2 guarantee, or AC29 (i)'s native cell under B7 arm F's
  forcing — and under B7's rule a zero count with an empty at-risk set reads "rule not
  exercised", so a zero count alone does not meet (i). The race is then unreachable, AC29 (i)'s
  native cell stays as the regression cell (any late A `bind` reopens B9), and its hook cell is
  recorded as a demonstration of the known fault, not a pass. Or (ii) the #540 wire owner
  approves a bounded idempotent re-answer. **Recommended: (ii).** For each `bind` received for
  the current qualified document, the Host re-posts the same `bound` — same `document` (the
  nonce) and `limits`; no new mint, no capacity replenishment, no role change — until that
  document's first authenticated `send`, `invoke` or `consumed` proves its listener live, bounded
  to a small fixed count the #540 owner sets; each re-post is a CONTROL record within #540's
  per-role CONTROL share (`docs/specs/gh540-compat-ipc.md:492`) and passes rule 1's final check;
  the bridge keeps the first `bound` it receives and ignores the rest (§4.3). Consequences: (ii)
  lets AC29 (i) pass and keeps Q1(b) step 4 (duplicates "cannot mint a fresh epoch, replenish
  capacity, change the original role, or receive a different document's `bound`"); it changes
  AC29 (f)'s pass observable from zero further `bound` posts to at most that count of identical
  re-posts, and costs at most that count of CONTROL posts per document. (i) keeps the one-use
  latch but needs evidence that may not exist. *Owner:* the #540 wire owner (0monish) for (ii);
  the W1 implementer for (i)'s device cell. *Check:* the owner's decision recorded on #1307, or
  the native cell's receipt in keld-research; then AC29 (i) GREEN under the chosen branch.

- **B10 GH-497's closed output map (r15, 0monish 4431715705) — OPEN; blocks T5, and T0 only on its branch choice (§6).** *What:* approved
  GH-497 closes what `keld migrate` writes (#1172 `2044f22e` `docs/specs/gh497-first-proof-migration-output.md:28,34,50-63`), and
  three things §4.8 needs are outside it. (i) Keys: the map's one `windows.<w>` row is `channels`
  (`:58`). `$keldWindowChannelGrants`, `windows.<w>.preload` (`digest`, `verdict`, `attestation`) and
  `windows.<w>.web.navigation` have no row, and M04 fails an unmapped key. (ii) Child process: M01
  allows default analysis "Zero filesystem mutations, child processes or package installations" (`:25`), and
  the window-key function is TypeScript that the Rust CLI cannot run in-process (INFERRED). (iii)
  Channels: M07 fails "computed, renderer-derived or unioned channels" (`:31`), and §4.8 **Grants**
  takes literals for a page-chosen or computed sink channel (F-B, F-P; draw.io's `sendMessage`, L111–114,
  is that shape). GH-497 is a first-proof macOS contract and says Windows facade acceptance is
  unknown (`:9`, `:93`); whether its map binds a Windows migration is not stated, and W1 treats it as
  binding, the fail-closed reading. *Impact:* T5 cannot generate route-D keys, so `keld migrate` produces
  no route-D grant and the Host's `channels: []` rule holds, so the gap fails closed; T6 needs T5.
  *What can proceed:* T0's text, T1, and T2–T4 on hand-written fixture manifests (§6 T3). *Resolution:* (a) **Recommended:** one distinct owner-approved
  GH-497 amendment that gives each key of (i) its row (file, grammar, owner, consumer test), with M02's
  reversible write and failing negative controls; for (ii), `--write` runs the function and default
  analysis lists each site's inputs without a key, so M01 stands; for (iii), the default stays M07 as
  written, those sinks add no grant and the report names them, and widening M07 for route D is the
  owner's separate choice. (b) A separate Windows output contract under the same owner: no change to
  the macOS proof, but a second map to keep equal to the first. *Owner:* the GH-497 owner (0monish), with
  #541 for the key grammar (§10 Q2). *Check:* the amendment's approval recorded on #497, then AC13 and
  AC39 GREEN under it.

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
   separate KEL-142 §5.1 security amendment approved; and every other W1 blocker (B2–B10, Q2, Q14, Q17,
   Q12 and the per-task dependencies in §6). Staging reading (spec author, r11): the list is
   cumulative per task, not a requirement that T2 wait for T4. Each task merges when the items
   it owns pass: T2 needs the bind→bound order with AC26 and AC29 GREEN on real Windows; T3
   needs T2 and the CompatV2 integration review; T4 needs AC27 and AC30 GREEN. No task enables
   a route-D grant before its own cells pass (§6). Recorded for the #540 owner to correct on
   #1307 if a stricter order was meant. Check: each item recorded on #1307.
2. **Q2 (manifest, #541/#1191):** keys `windows.<w>.web.navigation` (`allow`, `onLoss`),
   `windows.<w>.preload.digest`, `windows.<w>.preload.verdict` (`digest`, `mainDigest`, `classifier`, `reachableSinks`, `flags`)
   and `windows.<w>.preload.attestation` (`digest`, `mainDigest`, `sites`, `acknowledgedBy`); non-`file:`
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
   window receives grants only with a developer attestation bound to its digest and the main-code digest that covers every flagged
   site and never widens grants (§4.8 rule 3, AC19, AC32). Keld does not review third-party
   attestations; it reviews the mechanism, and separately the T6 qualification attestation
   (§6 T6). Applied to the qualification evidence: the pinned draw.io preload has seven
   flagged sites (§4.8 table), including the representation-only `watchFile` branch, and
   qualifies only with an attestation for its digest and main-code digest covering all seven. This is a framework
   mechanism; no application exception exists.
9. **Q9 (`keld migrate`):** verb reserved (`verb.rs:75`, KEL-17); T5 needs KEL-17's owner to
   admit the classifier as the first live slice, on top of GH-497's T1 and T2 (§6 T5).
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
14. **Q14 (renderer use of the B21 ticket and fence; owner #1192/#540, 0monish) — OPEN; blocks
    T2 and T3.** The B21 decision ([#1311 comment 6532409232](https://github.com/gyldlab/keld/pull/1311#issuecomment-6532409232)) names W3's eval result only. Its
    transaction has five steps and no Guard call, because an eval result is "not a
    renderer-originated privileged `el:` dispatch" (decision 2). W1 proposes the same ticket type
    and fence for a renderer `send` and floor `invoke` (§4.4), which needs two answers. (a) May the
    ticket's `work` carry renderer work, not only an eval correlation? (b) Is §4.4's Guard placement
    acceptable? The facts for (b). Decision 1: "Never hold it across socket I/O, a callback,
    blocking Guard/Bun work, or an awaited queue slot." #1191 `4f246a75`
    `docs/specs/gh552-exact-channel-grants-guard.md`: stale identity returns its own category
    "**before** selecting an absent-grant error" (`:147`); the comparison comes before
    `dispatch_privileged` (`:148`); "Only the Allow closure can begin the original app-link write"
    (`:149`); AC8 orders revocation against admission, with zero Bun writes when revocation wins
    first (`:54`). These texts do not conflict. Neither requires one hold from the comparison
    through the write, and that shape is the only one decision 1's sentence rules out, because
    `on_allow` is a callback and Guard would run under the fence. §4.4 meets all of them: a
    comparison before Guard with the fence released after it, then a fresh comparison under the
    fence inside `on_allow`, or after a denial. Its cost is a second short fence entry on every
    `send` and one pure Guard evaluation if a revocation lands in the gap. **Recommended: accept (a)
    and §4.4's placement.** It needs no narrower reading of decision 1, and the `send` transaction
    has the eval result's steps. Alternative to (b): hold the fence across one
    `dispatch_privileged`, for one comparison and no evaluation in that gap. That needs the owner to
    read "a callback" as excluding the fence's own `on_allow` and "blocking Guard" as excluding the
    evaluator (#1191 AC6's warmed Allow does not allocate; that it never blocks is INFERRED, it is
    an open draft), and the STA then waits on the fence for the evaluator's length, which #1191's
    "Never block the UI/navigation owner under a lock while waiting on Bun, Writer queue, callback
    or socket I/O" (`:151`) would also have to allow. AC33's cells hold under either answer; the
    alternative merges two of its mutants. If (a) is refused, W1 needs its own ticket type over the
    same fence. Check: the owner's answer recorded on #1307, and W3's §4.7 names the shared type.
15. **Q15 (input census; owner KEL-19, with KEL-17) — OPEN; blocks T5, not T0.** `mainDigest`
    needs one input census that `keld migrate`'s analysis and `keld build`'s recompute both read
    (§4.8 rule 3), independently owned and approved by KEL-19, with **no new first-proof migrate
    write**. GH-497 M10 forbids generating `keld.build.ts`, printing "not generated: keld build is
    reserved (KELD-CLI-045)" (#1172 `2044f22e` `docs/specs/gh497-first-proof-migration-output.md:34,73`), so r14's recommendation that
    `keld migrate` write the list there is withdrawn (r15, 0monish 4431715705). At `457a9c3b`
    `keld build` is a reserved verb (`crates/keld-cli/src/verb.rs`), and neither a list schema nor a
    reader for such a file exists. *Impact:* T5 can test the digest function with a fixture list,
    but the real list, and with it every flagged window's release grants, waits on this; the Host
    half (T3) and T4 do not. Whichever list KEL-19 approves, it holds every declared preload and
    excludes only `keld.permissions.jsonc` (§4.8 rule 3), and that rule is not part of this
    question. *Options, each read-only and each working on a tree that holds only GH-497's three
    outputs:* (a) **Recommended:** KEL-19 defines the list in `keld build`'s own contract, from what such a tree
    already declares (the `entry` in `keld.config.ts`, `package.json`, the application's
    existing packaging declarations read as data): one owner, and a developer tree and a clean CI
    checkout agree, because a build's own output is not an input. (b) The whole application root
    minus a fixed ignore list: it needs no KEL-19 field, but a previous build's output or a VCS
    directory enters the digest, so a clean checkout disagrees and the recompute refuses it. A later
    migration phase that wants `keld.build.ts` as the list needs a distinct owner-approved GH-497
    output and schema amendment, reversible-write gates and failing negative controls before it can
    be named as an input; this question does not ask for one. Also for this owner: a preload the app's bundler emits
    has no output bytes at migrate time, so `preload.digest` cannot name them until that flow is
    defined; until then such a window fails closed at AC14. Its source files are inputs and stay in
    `mainDigest`; only the emitted file, which does not exist at migrate time, does not. A stale
    emitted file that does exist is read and hashed, and a clean checkout then disagrees: the
    exception opens no way past the digest. *Check:*
    KEL-19's approved census names the list, T5 calls it, and AC39 is GREEN on it.
16. **Q16 (endpoint codec codes; owner #1190 with the error-registry owner) — OPEN; blocks T4, not
    T0.** §4.4 rule 5 needs a closed set, and #1190 has none yet: "Existing canonical
    error-registry owner must allocate noncolliding `KELD-COMPAT-*` codes with fix text **before
    implementation**; this draft does **not** reserve a number or reuse COMPAT-001..009" (#1190
    `115af9ac` `docs/specs/gh544-endpoint-value-codec.md:235-238`). `KELD-COMPAT-010` to `012` are
    taken too at `457a9c3b` (`docs/engineering/keld-error-codes.md:859-871`). *Impact:* AC36, and
    the exact code in AC34 and AC37 (g), cannot run; T0 to T3 can. *Needed:* the numbers, and how
    Rust reads them. The codes and text live in the registry and #1190's TypeScript codec emits
    them, so `keld-wv` needs them in Rust without a second owner. *Options:* (a) **Recommended:** one
    constant beside the v2 decoder, held equal to the registry's codes and text by a drift test in
    the registry's existing test (`crates/keld-cli/tests/error_registry.rs`): smallest, and a
    divergence fails CI. (b) Generate the constant from the registry: no copy, but a new build step.
    *Check:* the registry lists the codes, and how Rust reads them is recorded on #1307. AC36
    proves the rule at T4 and cannot close this question, because T4 blocks on it.
17. **Q17 (no post for a live sequence; owner #540/#1095, 0monish) — OPEN; blocks T2 and T3, and T0
    needs the answer.** *What:* §4.4 step 3a posts nothing for a raw duplicate of a live `message` or
    `request`, records a typed `Refusal` in #540's bounded diagnostics, and refuses by post only a
    number no live record holds. It adds no wire form. It rests on two readings of #1122 `d9211a96`:
    that `docs/specs/gh540-compat-ipc.md:603`'s "Bounded typed refusal" need not be a post, and that
    `:190`'s "non-reusing" lets a refused number stay spent. The review proposes the no-post form as
    an example and asks the #540 wire owner to define the behaviour, so it is not yet decided.
    *Impact:* until answered, T2 and T3 cannot merge step 3a; AC38's pass observables for (b) and (c)
    are what the review itself requires and hold under either option. *Options:* (a) **Recommended:**
    accept step 3a as written: unchanged wire, no new code, no post per duplicate. (b) Require a post:
    the refusal of a live duplicate then needs a distinct registered code the bridge ignores, which
    adds a registry allocation, a bridge branch and a CONTROL post per duplicate, and `Spent` keeps
    its ordinary refusal. *Owner:* the #540 wire owner. *Check:* the answer recorded on #1307, and
    AC38 GREEN under it.

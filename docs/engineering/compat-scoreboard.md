# Keld Electron compatibility scoreboard

> **Placeholder:** Keld does not publish compatibility scores yet. Do not infer API
> support from the planned tiers below. There is **no committed product
> denominator** in this repository, so no compatibility percentage may be
> published — including `0%` or `100%`.

## Denominator honesty (KEL-74)

Machine evidence is the versioned JSON record parsed by
`keld_compat::evidence` (`docs/specs/kel74-compat-evidence-schema.md`).
That schema is framework-generic. VS Code and other named apps are later
**showcase** corpus consumers; they do not redefine product tiers.

Rules that keep a partial corpus from becoming a “100% compatible” claim:

1. A percentage exists only against a committed denominator document
   (`keld.compat.denominator/v1`) that names `panel`, `corpus_id`,
   `corpus_sha256`, kind (`install` / `activation` / `primary_workflow` /
   `full_feature`), and a non-empty unique cell list. `score` rejects an
   empty cell list (`KELD-COMPAT-008`); `0/0` is not complete.
2. `unweighted_percent` is omitted whenever any required cell is missing or
   `unknown`, when contributing records disagree on artifact digest / authority
   profile / engine, or when `panel` is `product` and `corpus_id` is not a
   documented committed product corpus. T1: that committed-id list is empty,
   so a `toy-uncommitted` product 1-cell pass cannot publish `Some(100)`.
   Extra records outside the denominator cannot shrink N.
3. `complete` is true only when `N > 0`, every committed cell is `pass`,
   contributing records share digest, profile, and engine, and — for
   `panel: product` — `corpus_id` is a documented committed product corpus.
   T1: that list is empty, so a `toy-uncommitted` product 1-cell pass is not
   `complete`. Duplicate cells in the denominator are `KELD-COMPAT-008`
   (one Pass cannot become 2/2). Only `score` constructs `Scoreboard`;
   callers cannot mint `complete: true` with `unweighted_percent: Some(100)`.
4. The only allowed claim shape is
   `{passed}/{N} of {panel} corpus {id}@{digest} ({kind})`.
   Never “100% compatible” or “fully compatible.”
5. Waivers need owner, reason, and expiry; expired waivers fail closed.
   A waiver object cannot pair with `pass` (or any non-`waived` verdict);
   `score` rejects that pairing even when the struct was constructed in
   memory rather than parsed.
6. Opaque turn citations, absolute sandbox/`/tmp` paths, userinfo/loopback/
   unspecified/RFC1918/CGNAT/NAT64/6to4/link-local/unique-local https hosts
   (including a trailing FQDN dot and IPv4-mapped / IPv4-compatible /
   IPv4-translated embeddings), and live-mutable
   https URLs (no git object id in the path, e.g. `/blob/main/` or
   `https://example.com/foo`) are
   non-normative leads.    Host checks are literal (no DNS); abbreviated IPv4
   (`127.1`, `010.0.0.1`, `1.1`) and DNS-to-private names (`nip.io`) are T1
   residuals. A trailing FQDN dot is the same host, including GitHub raw CDN
   live-ref checks. An unbracketed `host:port` (or `[v6]:port`) must use a
   decimal `u16` port; `github.com:abc` and `github.com:65536` are leads.
   Allowed pins: `sha256:<64 hex>` or https with a
   parsed public host and a 40- or 64-hex git object id path segment.
   A `/blob/main/<40-hex>` URL is still a live branch, not a pin; the object
   id must be the `blob`/`tree`/`raw` (or GitHub raw CDN) ref itself.
   `score` re-checks the URI; `/tmp/` is not a substring ban on https URLs.
7. A report names a board's authority profile only through
   `Scoreboard::authority_profile()`, rendered with `AuthorityProfile::as_str`,
   which is the one variant-to-string map. The v1 vocabulary is `strict_bun`,
   `sandboxed_addon_worker`, `legacy_sandbox_off`, `user_approved_tool_child` and
   `unverified`, and the record `schema` stays `keld.compat.evidence/v1`.
   `unverified` is a run without verified containment (the KEL-78 `unverified`
   state). It is reported as `unverified` and is never strict or legacy evidence.
   No report may promote it, relabel it, or merge it with another profile. The
   accessor is `None` when contributing records disagree on profile. That board
   already withholds the percentage and `complete` (rules 2 and 3), and a report
   must not name any one record's profile for it. The claim (rule 4) names no
   profile. Labelling rules per run are in
   `docs/specs/gh532-first-proof-evidence-rules.md` rule 6. The KEL-237
   lifecycle report still prints its harness label as a literal until X01-T4
   (#566) migrates `crates/keld-compat/tests/lifecycle_evidence_report.rs`.

Until a product denominator is committed, this page stays a narrative API
board (✔/▲/✘). Installer size/RSS stays on
[`budget-scoreboard.md`](./budget-scoreboard.md).

Installer size, host bytes, and idle RSS are a different board:
[`budget-scoreboard.md`](./budget-scoreboard.md).

The public **API** scoreboard contract is defined by
[`docs/architecture/04-electron-compat.md` §4](../architecture/04-electron-compat.md#4-compat-tiers--the-public-scoreboard):

- per-API status: compatible, compatible with caveats, or unsupported, with notes;
- a measured score for each corpus application; and
- CI-generated updates from the migration corpus.

## Current status

- Tier 1: not measured
- Tier 2: not measured
- Tier 3: not measured
- Migration corpus: not available in this repository
- Public URL: planned at `https://keld.dev/compat`

Until the corpus harness lands, Electron's documented behavior remains the compatibility
oracle and every implemented divergence must be recorded explicitly. Matches are recorded
when a slice lands so the board does not imply a still-open gap. This page will become
generated measurement output when that harness exists; it is not a claim of current
compatibility.

## Recorded matches (KEL-72)

| API | Electron oracle | Keld | Mark | Why |
|---|---|---|---|---|
| `process.type` | [`process.type`](https://github.com/electron/electron/blob/v44.4.5/docs/api/process.md#processtype-readonly): "`browser` - The main process" | `"browser"`, frozen (non-writable, non-configurable, enumerable) | ✔ | Installed by `packages/@keld/electron/src/process-facts.ts`, the one owner of main-role `process.*` values (GH-570). Today the moment is the facade import; a Host-selected preload (GH-570 spec T2) moves it before the app entry evaluates so an electron-log-shaped dependency sees it too. Conformance: `packages/@keld/electron/src/process-facts.test.ts`; #569 cell `process.early.type` (red until T2). |
| `process.defaultApp` | [`process.defaultApp`](https://github.com/electron/electron/blob/v44.4.5/docs/api/process.md#processdefaultapp-readonly): `true` "when the app is started by being passed as parameter to the default Electron executable … even if the app is packaged", "to determine how many arguments will need to be sliced off from `process.argv`" | `true`, frozen; `process.argv` stays Bun's `[bun, entry, ...user]` | ✔ | The Host runs the role as `bun … run <entry> …`, the `electron .` shape (measured on Bun 1.4.4: runtime flags sit in `execArgv`), so `argv.slice(defaultApp ? 2 : 1)` is the user argument list and draw.io's `argv.unshift(null)` branch is skipped. Spec Option A; Option B (argv rewrite) rejected in `docs/specs/gh570-main-role-process-facts.md`. Conformance: `process-facts.test.ts` with `fixtures/process_facts_drawio_shape.ts`; #569 cell `process.argv.default-app.empty-user-args` (red until T2). |
| `window-all-closed` (no listener) | [`window-all-closed`](https://www.electronjs.org/docs/latest/api/app#event-window-all-closed): if you do not subscribe and all windows are closed, the default is to quit; if you subscribe, you control whether the app quits | Host `LastWindowClosed` with no `app.on("window-all-closed")` sends kipc `Quit`; a subscriber is not auto-quit | ✔ | Matches Electron. The public `app.quit()` return type is a separate ▲ below. Conformance: `packages/@keld/electron/fixtures/window_all_closed_default.ts`, `packages/@keld/electron/src/app.test.ts` (`window-all-closed Electron default quit`). |

## Recorded divergences (KEL-72)

Chosen as scoreboard ▲, not a `keld.compat.ts` quirks flag: these are host/kipc
constraints, not per-app toggles. `keld migrate` is not live.

| API | Electron oracle | Keld | Mark | Why |
|---|---|---|---|---|
| `process.versions.electron` | [`process.versions.electron`](https://github.com/electron/electron/blob/v44.4.5/docs/api/process.md#processversionselectron-readonly): "Electron's version string" | `"0.0.1"`, the `@keld/electron` package version, frozen | ▲ | Keld is not Electron, so it never reports an Electron release string: `0.0.1` is outside the npm `electron` release list (which starts at `0.1.0`), and `process-facts.test.ts` binds the constant to `package.json` and refuses `44.4.5`. A dependency that gates on an Electron version sees a version Electron never shipped; that is the honest signal (GH-570; #569 cell `process.versions.electron.honesty`). |
| `process.versions.chrome` | [`process.versions.chrome`](https://github.com/electron/electron/blob/v44.4.5/docs/api/process.md#processversionschrome-readonly): "Chrome's version string" | absent | ✘ | There is no Chromium in the Bun main role; inventing a version would be a lie. Renderer-side facts are #484's. |
| `process.resourcesPath` | [`process.resourcesPath`](https://github.com/electron/electron/blob/v44.4.5/docs/api/process.md#processresourcespath-readonly): "the path to the resources directory" | absent | ✘ | Electron sets it natively to the runtime's resources directory. Keld's honest value is the Host-staged launch root (`keld dev`: `<project>/.keld/dev/<nonce>`), never a path containing `app.asar`, and it must come from the Host session-facts payload (#452) rather than a role-side `cwd`/`__dirname`/environment heuristic. GH-570 spec T3 flips #569 cell `process.resources-path.string` when that payload exists. |
| `process.mas` / `process.windowsStore` / `process.sandboxed` / `process.contextIsolated` / `process.isMainFrame` | [`process`](https://github.com/electron/electron/blob/v44.4.5/docs/api/process.md): each is `true` in its case, "otherwise it is `undefined`"; `contextIsolated` "is `undefined` in the main process" | not own properties; `typeof` is `undefined` | ✔ | Undefined, not `false`: no Mac App Store or MSIX Keld packaging mode exists, and the main role is not a renderer. `process-facts.ts` never defines them; `process-facts.test.ts` asserts their absence. #569 cells `process.*.domain` pass today. |
| `app.quit` | [`app.quit(): void`](https://www.electronjs.org/docs/latest/api/app#appquit) | `Promise<void>`, never parking the caller | ▲ | The Quit Call travels over kipc and can fail (`KELD-IPC-*`). Callers must be able to await or `.then` the result. Do not change the public signature to `void` to paper over that. Since GH-528 T3 it returns at once as Electron's does (PANEL-P2, #419): code after `app.quit()` runs before the host's REPLY. The REPLY must be `LifecycleResponse::Quit` (otherwise `KELD-IPC-003`), and the role's link then closes. Conformance: `crates/keld-compat/tests/electron_lifecycle.rs`, `packages/@keld/electron/src/app.ts`, `packages/@keld/electron/src/link.test.ts` (`app.quit never parks`). |
| `app` `ready` / `window-all-closed` listeners | Electron [`app`](https://www.electronjs.org/docs/latest/api/app) is a Node [EventEmitter](https://nodejs.org/docs/latest/api/events.html#emitteremiteventname-args): a throw in one listener propagates from `emit` and later listeners do not run | Per-listener `try/catch` in `emit` | ▲ | Host `Event` frames arrive on the kipc read loop. An uncaught throw would skip remaining listeners and abort the reader. Isolation is the contract for this slice; do not revert it to match EventEmitter. Conformance: `packages/@keld/electron/fixtures/ready_listener_only.ts` (`packages/@keld/electron/src/app.test.ts`; moved there from `app_ready.ts` in GH-528 T3). |
| `app.whenReady` when the role's link fails | Electron's [`app.whenReady()`](https://www.electronjs.org/docs/latest/api/app#appwhenready) resolves once Electron is initialized and has no link to fail | Rejects with the typed error that failed or ended the link, and keeps rejecting with that same error | ▲ | Since GH-528 T3 a role opens one link per realm (GH-527 §4.2). So a failed connect, or a link that died before `Ready`, is sticky: every later `whenReady` (and `quit`) rejects with the same error, and connect is never retried. A retry could only surface `KELD-IPC-005`. The error carries the real cause: a connect failure keeps its own code (`KELD-IPC-001` no listener, `KELD-IPC-005` a malformed HELLO, `KELD-IPC-006` a silent host, `KELD-IPC-007` a token mismatch). A link that was up ends with the failure that ended it (`KELD-IPC-003` an undecodable lifecycle event, `KELD-IPC-005` a frame the session refused, `KELD-IPC-006` a stalled frame), and a plain close is `KELD-IPC-022`. Conformance: `packages/@keld/electron/fixtures/app_ready.ts` over the real connect, with `crates/keld-compat/tests/electron_lifecycle.rs` (`app_ready_isolates_listeners_retries_connect_without_unhandled_rejection`, a name kept for the frozen KEL-237 receipts); `packages/@keld/electron/src/link.test.ts` (one case per code). |

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
   unspecified/RFC1918/link-local/unique-local https hosts (including a trailing
   FQDN dot and IPv4-mapped / IPv4-compatible / IPv4-translated embeddings),
   and live-mutable
   https URLs (no git object id in the path, e.g. `/blob/main/` or
   `https://example.com/foo`) are
   non-normative leads. Allowed pins: `sha256:<64 hex>` or https with a
   parsed public host and a 40- or 64-hex git object id path segment.
   A `/blob/main/<40-hex>` URL is still a live branch, not a pin; the object
   id must be the `blob`/`tree`/`raw` (or GitHub raw CDN) ref itself.
   `score` re-checks the URI; `/tmp/` is not a substring ban on https URLs.

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
| `window-all-closed` (no listener) | [`window-all-closed`](https://www.electronjs.org/docs/latest/api/app#event-window-all-closed): if you do not subscribe and all windows are closed, the default is to quit; if you subscribe, you control whether the app quits | Host `LastWindowClosed` with no `app.on("window-all-closed")` sends kipc `Quit`; a subscriber is not auto-quit | ✔ | Matches Electron. The public `app.quit()` return type is a separate ▲ below. Conformance: `packages/@keld/electron/fixtures/window_all_closed_default.ts`, `packages/@keld/electron/src/app.test.ts` (`window-all-closed Electron default quit`). |

## Recorded divergences (KEL-72)

Chosen as scoreboard ▲, not a `keld.compat.ts` quirks flag: these are host/kipc
constraints, not per-app toggles. `keld migrate` is not live.

| API | Electron oracle | Keld | Mark | Why |
|---|---|---|---|---|
| `app.quit` | [`app.quit(): void`](https://www.electronjs.org/docs/latest/api/app#appquit) | `Promise<void>` | ▲ | The Quit Call travels over kipc and can fail (`KELD-IPC-*`). Callers must be able to await or `.then` the result. Do not change the public signature to `void` to paper over that. Conformance: `crates/keld-compat/tests/electron_lifecycle.rs`, `packages/@keld/electron/src/app.ts`. |
| `app` `ready` / `window-all-closed` listeners | Electron [`app`](https://www.electronjs.org/docs/latest/api/app) is a Node [EventEmitter](https://nodejs.org/docs/latest/api/events.html#emitteremiteventname-args): a throw in one listener propagates from `emit` and later listeners do not run | Per-listener `try/catch` in `emit` | ▲ | Host `Event` frames arrive on the kipc read loop. An uncaught throw would skip remaining listeners and abort the reader. Isolation is the contract for this slice; do not revert it to match EventEmitter. Conformance: `packages/@keld/electron/fixtures/app_ready.ts`, `crates/keld-compat/tests/electron_lifecycle.rs` (`app_ready_isolates_listeners_retries_connect_without_unhandled_rejection`). |

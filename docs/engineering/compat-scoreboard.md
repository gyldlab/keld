# Keld Electron compatibility scoreboard

> **Placeholder:** Keld does not publish compatibility scores yet. Do not infer API
> support from the planned tiers below.

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

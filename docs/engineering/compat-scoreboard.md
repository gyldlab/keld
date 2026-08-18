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
oracle and every implemented divergence must be recorded explicitly. This page will
become generated measurement output when that harness exists; it is not a claim of
current compatibility.

## Recorded divergences (KEL-72)

Chosen as scoreboard ▲, not a `keld.compat.ts` quirks flag: restoring Electron's
`void` would hide transport errors, so this is not a per-app toggle. `keld migrate`
is not live.

| API | Electron oracle | Keld | Mark | Why |
|---|---|---|---|---|
| `app.quit` | [`app.quit(): void`](https://www.electronjs.org/docs/latest/api/app#appquit) | `Promise<void>` | ▲ | The Quit Call travels over kipc and can fail (`KELD-IPC-*`). Callers must be able to await or `.then` the result. Do not change the public signature to `void` to paper over that. Conformance: `crates/keld-compat/tests/electron_lifecycle.rs`, `packages/@keld/electron/src/app.ts`. |

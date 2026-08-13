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

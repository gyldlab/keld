# Contributing

Engineering rules, the verification gate, and review gates are in
[`AGENTS.md`](AGENTS.md). Read that file before changing code.

## Clone → build → test → PR

1. Clone this repository.
2. Use the pinned toolchain in `rust-toolchain.toml` (1.97.1).
3. Maintainers (optional): after reviewing the current revision, run
   `just hooks-install` once. It copies notification-only hooks into this clone's
   Git common directory and sets the local `core.hooksPath`; checkout and merge
   never execute code from the incoming working tree. The hooks print the exact
   `just research-sync` and `just competitors-sync` commands for you to run
   explicitly after reviewing the new revision. Rerun `just hooks-install` only
   when you intend to trust updated hook bytes. Git cannot enable hooks on clone.
   Push research with `just research-push` (nested `docs/research/` repo only —
   never stage research into Keld).
4. Run the exact full local gate:

   ```bash
   just ci
   ```

5. Branch `agent/kel-<n>-<slug>` from `origin/main` (`.agents/review.md` § Branch and commit contract).
   Open a pull request with `.github/PULL_REQUEST_TEMPLATE.md` (Summary · Spec refs ·
   Review gates · Tests · Platforms · Perf impact). Include `## Linear` only when a
   KELD id exists.

Do not bypass `keld-guard`. Do not add `unwrap` / `expect` / `panic!` in
library code. Do not invent a fifth config filename.

License: MIT OR Apache-2.0 (`LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE`,
workspace `Cargo.toml`).

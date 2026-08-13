# Contributing

Engineering rules, the verification gate, and review gates are in
[`AGENTS.md`](AGENTS.md). Read that file before changing code.

## Clone → build → test → PR

1. Clone this repository.
2. Use the pinned toolchain in `rust-toolchain.toml` (1.93.0).
3. Maintainers (optional): run `just hooks-install` once so this clone uses
   tracked `.githooks/` (`core.hooksPath`, local config only). After that,
   `git pull` / checkout runs `just research-sync` then `just competitors-sync`
   for gitignored private research and competitor clones. Git cannot enable
   hooks automatically on clone. Push research with `just research-push` (nested
   `docs/research/` repo only — never stage research into Keld).
4. Run the three-command gate:

   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo nextest run --workspace --profile ci
   ```

5. Open a pull request with `.github/PULL_REQUEST_TEMPLATE.md`, including a
   Linear issue id (`KEL-n`) and the five review gates (or `none`).

Do not bypass `keld-guard`. Do not add `unwrap` / `expect` / `panic!` in
library code. Do not invent a fifth config filename.

License: MIT OR Apache-2.0 (`LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE`,
workspace `Cargo.toml`).

# AGENTS.md — Keld engineering rules

Desktop framework: Rust host (windows/webviews/native); JS/TS main on supervised Bun child (zero ambient OS authority); kipc IPC; default-deny permissions; Electron compat via `@keld/electron` + `keld migrate`.

## Ground truth
- Specs: `docs/architecture/01..07-*.md`. Research: `docs/research/`.
- Code/spec mismatch = bug in one; fix together in same PR or state why. No silent drift.
- Features: approved spec (`docs/agents/spec-template.md`) + Linear (KELD). Process: `docs/agents/workflow.md`.
- Read crate `AGENTS.md` before editing that crate.

## Repo map
| Crate | Role |
|---|---|
| keld-core | Event loop, windows, lifecycle |
| keld-wv | WebEngine; wkwebview/webview2/webkitgtk/cef |
| keld-ipc | kipc framing/codecs/channels/shm — spec 02 |
| keld-guard | Capabilities, manifest, scopes — spec 03 |
| keld-native | menu/tray/dialog; guard-checked |
| keld-runtime | Bun child supervisor |
| keld-update | bsdiff+zstd, signed manifests |
| keld-pack | Installers, signing, cross-compile |
| keld-compat | Electron emulation — spec 04 |
| keld-host | Shipping host binary |
| keld-cli | create/dev/build/migrate/doctor/gen |
| packages/ | @keld/* TS (upcoming) |

## Commands & verification
Toolchain: `rust-toolchain.toml` (1.93.0). TS: `bun install` / `bun test` in `packages/*`.

```bash
cargo fmt --all                                    # before done
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings \
  && cargo nextest run --workspace --profile ci    # verification gate — all three before "done"
cargo nextest run -p <crate> [-- <filter>]         # single crate/test
# Fallback: cargo test --workspace
```

New behavior needs tests. Report actual command output; never "should work". Failures on other-OS paths: say plainly.

## Rust
- Lints: workspace `Cargo.toml` — `clippy::pedantic`, `missing_docs` warn (CI denies). New `allow` → inline justification.
- `unsafe`: only `keld-wv` backends + `keld-ipc` shm; `#![deny(unsafe_op_in_unsafe_fn)]`, `// SAFETY:` proof. Else = human review.
- No `unwrap`/`expect`/`panic!` in libs. Typed errors (`thiserror`). `expect` ok in tests + `keld-cli` top-level (state invariant).
- Errors state the fix — see `docs/architecture/07-agent-experience.md` §2; model `DenyReason`.
- Hot paths (kipc, event-loop, guard): callbacks/state machines. No async runtime, no steady-state alloc. Async only in cold tooling (cli/pack/update).
- Deps: std-first, minimal, workspace-pinned. Each addition = review gate (name, purpose, alternatives).
- Public items documented.

## TypeScript (`packages/*`)
- Strict; no `any` in public API. Generated (`@keld/schema`) never hand-edited.
- `@keld/electron` never imports Electron at runtime.

## Naming
- Crates `keld-*`, libs `keld_*`, npm `@keld/*`, protocol `KI*`. One canonical name per concept.
- Config: `keld.config.ts`, `keld.permissions.jsonc`, `keld.build.ts`, `keld.compat.ts` only (else spec change).
- Numbered docs are paths; renumber → update all refs.

## Security & performance
- Default-deny sacred: no bypass of `keld-guard`. Dev-permissive only under `keld dev` + recorder; release refuses.
- Perf budgets: `docs/architecture/01-overview.md` §5 (CI once `bench/` lands); >5% regression = waiver + benchmarks.
- Threat models in `keld-guard`/`keld-update` crate docs.

## Review gates — human sign-off; list under `## Review gates` in PR
1. `unsafe` (new/changed)
2. Public API (new/changed)
3. Permission model
4. Dependency addition
5. Wire protocol (kipc frames, manifest schema, update feed)

Write "none" when none apply.

## Working rules
- Tests = contract. Compat: conformance entry *first* (Electron docs = oracle). Bugs: regression test before fix.
- Anti-flake: no sleep-sync; await conditions; port 0; temp dirs; colocated tests; doc *why*.
- No `todo!()`/`unimplemented!()`/stubs on main. Small PRs, one concern.
- Before coding: grep codebase; read spec section; crate `AGENTS.md`; `docs/agents/learnings.md`.

## Self-improvement (mandatory)
Non-obvious gotcha (>10 min saved) → append ONE line to `docs/agents/learnings.md` same PR:
```
- YYYY-MM-DD [area] fact. (evidence: path, issue, or command)
```
Grep first (no dupes/opinions). Stale rule here → fixing it *is* the task.

## Commits & PRs
- Conventional: `feat(ipc): …`, `fix(wv/macos): …`, `docs(research): …`.
- PR: Summary · Spec refs · Review gates · Tests · Platforms · Perf impact (or none).
- No secrets; no `.env*` edits; destructive git ops need human approval. Rebase on main before PR.

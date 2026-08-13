# AGENTS.md — Keld engineering rules

Desktop framework: Rust host (windows/webviews/native); JS/TS main on supervised Bun child (zero ambient OS authority); kipc IPC; default-deny permissions; Electron compat via `@keld/electron` + `keld migrate`.

The key words **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** in agent-facing docs (this file, crate `AGENTS.md`, `.agents/*`, and `docs/agents/*`) are IETF [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119). They bind agents. Architecture specs (`docs/architecture/*`) stay prose.

## Ground truth
- Specs: `docs/architecture/01..07-*.md`. Research: `docs/research/`.
- Agent-readable docs: [`llms.txt`](llms.txt) is the generated compact index;
  [`llms-full.txt`](llms-full.txt) is its ordered authoritative corpus. Run
  `just llms-check` after changing an included source.
- Code/spec mismatch is a bug in one; agents MUST fix both in the same PR or state why. Agents MUST NOT silently drift.
- Features: approved spec (`docs/agents/spec-template.md`) + Linear (KELD). Process: `docs/agents/workflow.md`.
- Agents MUST read crate `AGENTS.md` before editing that crate.

## Agent playbooks
- `.agents/index.md` routes tasks to conditional playbooks; agents MUST load only the relevant entries.
- External research MUST follow `.agents/research.md`: escalate only when local/primary evidence is insufficient and the decision materially depends on current external facts or synthesis.

## Directness and scope
- Lead with evidence and disagree when code, specs, OS contracts, or primary sources contradict an assumption.
- State uncertainty, confidence when useful, and the missing proof; MUST NOT present inference as fact.
- Diagnose the root cause and keep execution scoped; park adjacent cleanup.
- Ask one focused question only when a user-owned choice would materially change the result; otherwise take the smallest reversible path.

## Repo map
| Crate | Role |
|---|---|
| keld-core | Event loop, windows, lifecycle — spec 01 |
| keld-wv | WebEngine; wkwebview/webview2/webkitgtk/cef — spec 05; `AGENTS.md` |
| keld-ipc | kipc framing/codecs/channels/shm — spec 02; `AGENTS.md` |
| keld-guard | Capabilities, manifest, scopes — spec 03; `AGENTS.md` |
| keld-native | menu/tray/dialog; guard-checked — spec 05 |
| keld-runtime | Bun child supervisor — spec 06 |
| keld-update | bsdiff+zstd, signed manifests — spec 06 |
| keld-pack | Installers, signing, cross-compile — spec 06 |
| keld-compat | Electron emulation — spec 04; `AGENTS.md` |
| keld-host | Shipping host binary — spec 01/06 |
| keld-cli | create/dev/build/migrate/doctor/gen — spec 06/07 |
| packages/ | @keld/* TS (upcoming) |

Crate `AGENTS.md` only where invariants exist (`wv`, `ipc`, `guard`, `compat`). Skeletons and `keld-cli`: spec in this table, no hollow file.

## Commands & verification
Toolchain: `rust-toolchain.toml` (1.93.0). TS: `bun install` / `bun test` in `packages/*`.

```bash
cargo fmt --all                                    # before done
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings \
  && cargo nextest run --workspace --profile ci    # verification gate — all three before "done"
cargo nextest run -p <crate> [-- <filter>]         # single crate/test
just llms-check                                    # generated docs are current
# Fallback: cargo test --workspace
```

Agents MUST run all three gates before calling work done. New behavior MUST have tests. Agents MUST report actual command output; MUST NOT write "should work". Failures on other-OS paths: say plainly.

## Rust
- Lints: workspace `Cargo.toml` — `clippy::pedantic`, `missing_docs` warn (CI denies). A new `allow` MUST have an inline justification.
- `unsafe` MUST appear only in `keld-wv` backends + `keld-ipc` shm; `#![deny(unsafe_op_in_unsafe_fn)]`, `// SAFETY:` proof. Else = human review.
- Agents MUST NOT add `unwrap`/`expect`/`panic!` in libs. Typed errors: hand-rolled `Display` + `KELD-*` codes (not `thiserror`). `expect` MAY appear in tests + `keld-cli` top-level (state invariant).
- Errors MUST state the fix — see `docs/architecture/07-agent-experience.md` §2; model `DenyReason`.
- Hot paths (kipc, event-loop, guard): callbacks/state machines. Agents MUST NOT add an async runtime or steady-state alloc there. Async MAY appear only in cold tooling (cli/pack/update).
- Deps: std-first, minimal, workspace-pinned. Each addition is a review gate (name, purpose, alternatives).
- Public items MUST be documented.

## TypeScript (`packages/*`)
- Strict; public API MUST NOT use `any`. Generated (`@keld/schema`) MUST NOT be hand-edited.
- `@keld/electron` MUST NOT import Electron at runtime.

## Naming
- Crates `keld-*`, libs `keld_*`, npm `@keld/*`, protocol `KI*`. One canonical name per concept.
- Config: `keld.config.ts`, `keld.permissions.jsonc`, `keld.build.ts`, `keld.compat.ts` only (else spec change).
- Numbered docs are paths; renumber → update all refs.

## Security & performance
- Default-deny is sacred: agents MUST NOT bypass `keld-guard`. Dev-permissive MAY exist only under `keld dev` + recorder; release MUST refuse.
- Perf budgets: `docs/architecture/01-overview.md` §5 (CI once `bench/` lands); >5% regression MUST have a waiver + benchmarks.
- Threat models in `keld-guard`/`keld-update` crate docs.

## Review gates — human sign-off; list under `## Review gates` in PR
1. `unsafe` (new/changed)
2. Public API (new/changed)
3. Permission model
4. Dependency addition
5. Wire protocol (kipc frames, manifest schema, update feed)

Agents MUST list these five (or write "none") in the PR. Human sign-off is required for any that apply.

## Working rules
- Tests are the contract. Compat work MUST land a conformance entry first (Electron docs = oracle). Bugs MUST have a regression test before the fix.
- Anti-flake: agents MUST NOT sleep-sync; MUST await conditions; MUST bind port 0; MUST use temp dirs; colocated tests; doc *why*.
- Agents MUST NOT land `todo!()`/`unimplemented!()`/stubs on main. PRs SHOULD be small, one concern.
- Before coding, agents MUST: grep the codebase; read the spec section; read crate `AGENTS.md`; read `docs/agents/learnings.md`.

First-principles + YAGNI (MUST; `docs/research/27-first-principles-yagni.md`):
1. Agents MUST decompose every design to OS/process/memory/trust-boundary facts across host / Bun child / webview. If it does not change who owns a handle, who can crash whom, or who can mint a principal, it is not architecture.
2. Agents MUST treat wry layout, Tauri ACL, Electron docs, and platform event loops as evidence of facts — not templates. Copying crate graphs, tokio-in-core, ACL wildcards, or in-process Node is cargo-cult.
3. Agents MUST protect four uniques only: prebuilt host, supervised Bun with zero ambient OS authority, kipc, default-deny (generated, host-enforced). MUST NOT invent a fifth.
4. Two YAGNI tests: (a) can Linear Phase 2 (window + kipc echo + crate map) ship without this? (b) does this file exist only to look complete? Either yes → agents MUST NOT land it.
5. Anti-patterns: crate `AGENTS.md` only when it adds binding rules; agents MUST NOT write an RFC that restates `docs/architecture/` without binary acceptance tests; MUST NOT split toward a 100-crate graph; MUST NOT add a `WebEngine` method until a live backend implements it in the same PR.

No slop (MUST):
Agents MUST follow `.agents/testing.md`. Tests MUST be falsifiable: a real contract defect must fail the test. Test observable contracts (error code, wire bytes, process status, or OS behavior), not implementation-shaped essays.

Nested `crates/<crate>/AGENTS.md` (`docs/research/26-agents-md-cloudflare-rfc.md`):
- A crate MUST have `AGENTS.md` when it has invariants not in this file: `unsafe`/WebEngine, `keld-guard` default-deny, kipc wire protocol. Nested files MUST add constraints; they MUST NOT silently weaken root. Root wins on conflict unless the crate file names a documented exception with justification.
- Agents MUST NOT add hollow stubs; MUST NOT add files for skeletons (`keld-core`, `keld-native`, `keld-runtime`, `keld-update`, `keld-pack`, `keld-host`); MUST NOT add one for `keld-cli` (`expect` already sanctioned in § Rust); MUST NOT add `packages/` until TS exists. Point at the spec in the repo-map table instead.
- Enforcement: `just agents-md` — fails if a crate with `unsafe` / `allow(unsafe_code)` has no `AGENTS.md`. Not a Codex.

## Self-improvement (mandatory)
Non-obvious gotcha (>10 min saved) → agents MUST append ONE line to `docs/agents/learnings.md` in the same PR:
```
- YYYY-MM-DD [area] fact. (evidence: path, issue, or command)
```
Agents MUST grep first (no dupes/opinions). Stale rule here → fixing it *is* the task.

## Private research
`docs/research/` is a nested git checkout of `https://github.com/0monish/keld-research.git` (not the Keld monorepo index). When an agent creates or edits files under `docs/research/`, it MUST commit inside that nested repo and push with `just research-push` (or `git -C docs/research push`) in the same turn, unless push soft-fails for no access (then warn the user). Agents MUST NOT `git add docs/research` from the Keld repo root / MUST NOT commit research into Keld.

## Public benches
Hello-window / installer / RSS fixtures for competitor frameworks and native Swift live in [`https://github.com/gyldlab/keld-benches`](https://github.com/gyldlab/keld-benches) (public) under **OS-first** paths `{macos|windows|linux}/<framework>/...` (e.g. `macos/swift/appkit-wk`, `windows/electron/hello`). When an agent creates or updates such benchmark apps, it MUST commit and push to `gyldlab/keld-benches`, not into the Keld monorepo. Agents MUST pick the OS folder for the machine / pack they actually ran. Agents MUST NOT put OS-agnostic dumps at the `keld-benches` repo root. Agents MUST NOT add Electron / Tauri / Wails / Neutralino / NW.js / Electrobun / Swift hello apps under Keld `docs/`, `competitors/` (shallow reference clones from the lockfile), or `/tmp`-only without pushing to `keld-benches`. Measured numbers MAY be recorded in `docs/engineering/budget-scoreboard.md` (Keld); rows SHOULD link the OS-qualified fixture path in `keld-benches`.

## Commits & PRs
- Conventional: `feat(ipc): …`, `fix(wv/macos): …`, `docs(research): …`.
- PR MUST include: Summary · Spec refs · Review gates · Tests · Platforms · Perf impact (or none).
- Agents MUST NOT commit secrets or edit `.env*`; destructive git ops MUST have human approval. Rebase on main before PR.

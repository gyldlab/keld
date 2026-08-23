# AGENTS.md — Keld engineering rules

Desktop framework: Rust host (windows/webviews/native); JS/TS main and named compat roles on supervised Bun children (zero ambient OS authority per strict-profile principal); kipc IPC; default-deny permissions; Electron compat via `@keld/electron` + `keld migrate`.

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

## Engineering principles — non-negotiable

For every architecture, public-contract, process, IPC, permission, lifecycle or
performance design change, agents MUST apply first-principles systems engineering and
DRY before choosing a design or writing code. Familiar framework shapes, a larger
language rewrite, and a passing happy path are not evidence that a design is correct,
secure or fast. For smaller changes, agents MUST state `No boundary change` when that
distinction would otherwise be ambiguous.

1. **Start from facts, not analogies.** Decompose the change into ownership, process,
   memory, I/O, lifecycle, trust and failure facts. State who owns each handle, who can
   mint each identity, what can crash independently, where copies/queues occur, and
   what observable contract proves the result. An unmeasured performance claim or an
   uncited platform assumption MUST NOT decide architecture.
2. **Reuse before rewrite.** Agents MUST search for and evaluate the existing shared
   abstraction, platform primitive, verified upstream facility and generated contract
   before adding a replacement. A rewrite is permitted only when the existing option
   cannot meet a named correctness, security, ownership or measured-performance
   requirement. An approved spec—or, for a bug fix governed by an existing spec, the
   PR—MUST record the rejected alternative and preserve a compatibility fallback whenever
   the published contract requires one.
3. **One rule, one owner, one source of truth.** Agents MUST NOT duplicate policy,
   schema, permission checks, wire parsing, lifecycle state, platform shims or helpers
   because a shared implementation is inconvenient. Fix or extend the owning
   abstraction with tests. Parallel copies, mirrored constants and diverging fallback
   paths are defects, not expedient implementation choices.
4. **Performance is an outcome, not a language property.** Rewriting an API in Rust,
   adding shared memory, or removing a runtime does not by itself prove improvement.
   Agents MUST establish semantic equivalence and use an attributed, reproducible
   benchmark before claiming or retaining a performance-motivated replacement. The
   baseline remains the simpler correct path until a measured end-to-end gain justifies
   added complexity.
5. **Reject violations at the boundary.** A design or PR that violates these rules MUST
   stop for correction; agents MUST NOT hide it behind a local workaround, flag, special
   case or broad permission. If the shared abstraction itself is wrong, propose its
   smallest root-cause fix in an approved spec. Human review may choose a different
   architecture, but it MUST record the new invariant rather than grant an undocumented
   exception.

## Repo map

Role is what the crate **is** today. `TARGET` marks specified destination scope that is
not implemented; `SKELETON` marks a name-only module surface. Per-crate current status
is `docs/architecture/01-overview.md` §1.

| Crate | Role |
|---|---|
| keld-core | Hello window + lifecycle session; TARGET event loop integration, window registry — spec 01 |
| keld-wv | WebEngine; wkwebview/webview2/webkitgtk; TARGET cef — spec 05; `AGENTS.md` |
| keld-ipc | kipc framing/codecs; TARGET channel registry + shm — spec 02; `AGENTS.md` |
| keld-guard | Capabilities, manifest, scopes — spec 03; `AGENTS.md` |
| keld-native | guard-checked brokers; `fs` live, rest SKELETON — spec 05 |
| keld-runtime | Bun child-role supervisor — spec 06 |
| keld-update | bsdiff+zstd, signed manifests — spec 06 |
| keld-pack | Installers, signing, cross-compile — spec 06 |
| keld-compat | Electron emulation — spec 04; `AGENTS.md` |
| keld-host | Shipping host binary — spec 01/06 |
| keld-cli | create/dev/build/migrate/doctor/gen — spec 06/07 |
| packages/ | `@keld/electron` (KEL-72); other `@keld/*` upcoming |


Crate `AGENTS.md` only where invariants exist (`wv`, `ipc`, `guard`, `compat`). Skeletons and `keld-cli`: spec in this table, no hollow file.

## Commands & verification
Toolchain: `rust-toolchain.toml` (1.97.1). TS: `bun install` / `bun test` in `packages/*`.

```bash
cargo fmt --all                                    # before done
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings \
  && cargo nextest run --workspace --profile ci    # verification gate — all three before "done"
cargo nextest run -p <crate> [-- <filter>]         # single crate/test
just llms-check                                    # generated docs are current
just mermaid-check                                 # Mermaid accessibility/type/palette contract
just mermaid-render-check                          # digest-pinned isolated SVG render
# Fallback: cargo test --workspace
```

Agents MUST run all three Rust gates before calling work done. Mermaid changes MUST also
pass `just mermaid-test` and `just mermaid-check`, then render through an explicitly
versioned stable Mermaid renderer using `just mermaid-render-check`. New behavior MUST
have tests. Agents MUST report actual command output; MUST NOT write "should work".
Failures on other-OS paths: say plainly.

## CI dependency routing

GitHub Actions workflows MUST be created for every pull request and push. Agents MUST
NOT use workflow-level `paths` or `paths-ignore` filters for a required workflow:
GitHub leaves its required check pending when it skips the whole workflow. The
repository-owned CI router instead classifies changed paths and applies conditions at
the **job** boundary; a skipped job reports success while unrelated expensive work is
not scheduled. Job-level `if` MUST use `needs` (and `github` / `vars` / `inputs` only).
Agents MUST NOT reference `matrix` there: GitHub evaluates that condition before matrix
expansion, and an invalid `matrix.os` guard fails the workflow file so rustc never starts.

- Every lane MUST name the observable contract and inputs it owns. A dependency install,
  OS runtime, browser engine, device, cross-target toolchain, or benchmark MAY run only
  when a changed input can affect that contract. `gitleaks` remains unconditional because
  every changed byte is its input.
- The router MUST derive build-dependency closures from the current build metadata where
  available; agents MUST NOT copy a hand-maintained crate list. Unknown, shared,
  workspace-graph, or comparison-base inputs MUST fail safe by enabling every potentially
  affected lane, including Ubuntu WebKitGTK apt when the diff cannot be proven GTK-free.
  Workflow and router edits MUST still create every job (Linux GUI smoke installs WebKitGTK)
  but MUST NOT also `apt-get` on Ubuntu clippy or MSRV: those extra live apt-get update
  calls hang on Azure Ubuntu mirrors while the GUI job finishes the same packages in
  minutes. If the router itself cannot obtain required metadata or parse it, it MUST fail
  the router job before emitting a partial/empty selection; it MUST NOT convert that fault
  into a skipped-green result.
- Conditional routing MUST have a falsifiable contract test for relevant, unrelated,
  unknown, empty, pull-request and push diffs. A workflow/router edit MUST exercise all
  conditional lanes. Agents MUST verify branch-protection behavior from current official
  GitHub Actions documentation before changing this mechanism.
- CI routing changes are a human-reviewed shared-file change. They MUST be their own
  Linear-scoped PR unless the owning issue is CI itself; agents MUST NOT hide a CI
  workaround in an unrelated product PR.

## Rust
- Lints: workspace `Cargo.toml` — `clippy::pedantic`, `missing_docs` warn (CI denies). A new `allow` MUST have an inline justification.
- `unsafe` MUST appear only in `keld-wv` backends (and in `keld-ipc` shm once that module exists; it does not today); `#![deny(unsafe_op_in_unsafe_fn)]`, `// SAFETY:` proof. Else = human review.
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

## Documentation diagrams
- Agents MUST add a Mermaid diagram only when it makes a relationship materially clearer
  than prose or a small table; diagrams MUST NOT be decorative or duplicate nearby text.
- Use `flowchart` for topology, dependencies or decisions; `sequenceDiagram` for ordered
  messages/lifecycle; `stateDiagram-v2` for a state machine; `gantt` only for a real dated
  schedule; and `erDiagram` only for a data model.
- Every Mermaid block MUST include `accTitle` and `accDescr`. Labels MUST carry the
  meaning without color, including current versus target and framework versus showcase;
  styling is redundant emphasis only. The surrounding prose MUST name the source of
  truth and any implementation gap.
- Agents MUST use stable Mermaid syntax supported by the repository renderer/GitHub.
  Before introducing unfamiliar syntax, agents SHOULD use Context7 when available to
  locate current material and MUST confirm it against the current
  [official Mermaid documentation](https://mermaid.js.org/). Every added or changed
  block MUST pass `just mermaid-render-check` and the render/report gate in
  [`.agents/testing.md`](.agents/testing.md); a browser preview or visual inspection
  alone is not verification.
- When a diagram uses semantic color, agents MUST reuse the applicable palette below
  rather than inventing per-file colors:

```text
classDef current fill:#dcfce7,stroke:#15803d,color:#052e16,stroke-width:2px
classDef target fill:#dbeafe,stroke:#1d4ed8,color:#172554,stroke-width:2px
classDef showcase fill:#f3e8ff,stroke:#7e22ce,color:#3b0764,stroke-width:2px,stroke-dasharray:5 3
classDef gate fill:#fef3c7,stroke:#b45309,color:#451a03,stroke-width:2px
classDef external fill:#e2e8f0,stroke:#475569,color:#0f172a,stroke-width:2px
classDef denied fill:#fee2e2,stroke:#b91c1c,color:#450a0a,stroke-width:2px
```

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

Git worktrees:
- Extra `git worktree`s (not the primary clone) that are unused MUST be removed: merged PR branch, no open PR, clean working tree.
- In-use MUST stay: primary checkout, worktrees for **open** PRs, dirty trees with real uncommitted work.
- When an agent will **add** a worktree, or at the start of a session that uses worktrees, it MUST run `git worktree list`, map branches to `gh pr list` (open vs merged), and `git worktree remove` unused + `git worktree prune` **before** creating a new one.
- MUST NOT `--force` remove a dirty tree with real work; MUST NOT delete `main`; MUST NOT force-push; MUST NOT remove sibling repos (`keld-agent-prompts`, `docs/research` nested checkout).
- `git worktree remove` is the owned mechanism; agents MUST NOT `rm -rf` a linked checkout unless `git worktree remove` already succeeded.

## Linear coordination (mandatory)

For every task associated with a KELD Linear issue, Linear is the shared execution
record, not a final reporting form. When the connector is available, agents MUST follow
the canonical lifecycle in [`docs/agents/workflow.md`](docs/agents/workflow.md). When it
is unavailable or permissions prevent an update, agents MUST record that limitation in
the PR or handoff and MUST NOT invent a ticket update.

### Branch + Linear handoff (MUST)

Paste chrome for multi-device agents lives in Prompt Tracker
(`0monish/prompt-tracker` `prompts/SHARED/branch-linear-handoff.md`). Monorepo agents
who never see that paste MUST still follow these rules — Linear is the shared merge-intent
record across devices.

1. If the agent creates or uses a git branch in `gyldlab/keld`, `0monish/keld-research`
   (nested `docs/research`), or `0monish/prompt-tracker`, it MUST comment on the relevant
   Linear issue(s) **before finishing** with a `## Branch handoff` block (template below).
2. Agents MUST NOT silently abandon a branch with no Linear note.
3. Agents MUST NOT auto-merge to `main` unless the paste explicitly authorizes
   `MERGE: yes` / merge-when-green for that ticket, **or** Linear already records
   `Merge: merge-when-CI-green`, required checks are green, and the paste allows merge.
4. Research-note commits still follow Private research / `just research-push`. An incidental
   **product** (`gyldlab/keld`) branch from research defaults to
   `Merge: do-not-merge` or `human-decide` plus a Linear handoff unless the prompt
   explicitly authorizes a PR merge.
5. Not every branch should merge to `main`. Prefer `merge-after:<deps>` or `human-decide`
   over guessing. Multi-device agents MUST read the latest Branch handoff comment before
   creating a duplicate branch or merging.

```text
## Branch handoff
- Repo:
- Branch:
- Tip SHA:
- PR: (url or none)
- Merge: do-not-merge | merge-when-CI-green | merge-after:<deps> | human-decide
- Depends on: (tickets/PRs/notes or none)
- Reason:
```

One block per branch (one per repo when several were touched).

First-principles + YAGNI (MUST; `docs/research/27-first-principles-yagni.md`):
1. Apply the Engineering principles above across host / Bun child / webview; Keld-specific architecture additionally changes who owns a handle, who can crash whom, or who can mint a principal. If it changes none of those facts, it is not architecture.
2. Agents MUST treat wry layout, Tauri ACL, Electron docs, and platform event loops as evidence of facts — not templates. Copying crate graphs, tokio-in-core, ACL wildcards, or in-process Node is cargo-cult.
3. Agents MUST protect four uniques only: prebuilt host, supervised Bun process family with zero ambient OS authority per strict-profile principal, kipc, default-deny (generated, host-enforced). MUST NOT invent a fifth.
4. Two YAGNI tests: (a) can Linear Phase 2 (window + kipc echo + crate map) ship without this? (b) does this file exist only to look complete? Either yes → agents MUST NOT land it.
5. Anti-patterns: crate `AGENTS.md` only when it adds binding rules; agents MUST NOT write an RFC that restates `docs/architecture/` without binary acceptance tests; MUST NOT split toward a 100-crate graph; MUST NOT add a `WebEngine` method until a live backend implements it in the same PR.

No slop (MUST):
Agents MUST follow `.agents/testing.md`. Tests MUST be falsifiable: a real contract defect must fail the test. Test observable contracts (error code, wire bytes, process status, or OS behavior), not implementation-shaped essays.

No workarounds (MUST):
Agents MUST fix causes, not symptoms — in code, tests, builds, docs, and tooling alike. When something resists, the reflex MUST be "why is this happening", never "how do I get past this". Agents MUST NOT make the signal fit the change.

**Failure decomposition protocol (MUST):** before selecting any fix for a failed gate,
CI dependency, platform behavior, build, test, security control, or runtime fault,
agents MUST decompose the problem into atomic reasoning units. For each atom they MUST
(1) state the logical component, (2) validate its independence from the other atoms,
and (3) verify correctness with a falsifiable observation. Only then MAY they synthesize
the atoms into a root-cause design and implementation. A timeout increase, retry,
bypass, flag, skip, mock, hard-coded exception, test weakening, broader permission, or
environment override is forbidden when it merely makes a symptom disappear; it is
permitted only when the decomposed root cause proves that it is the owned correctness
mechanism, its compatibility fallback is explicit, and a test falsifies its removal.

Forbidden in general:
- Special-casing an input, hardcoding a value, or branching on a specific case to make one caller work, when the underlying rule is what is wrong.
- Papering over a failure: swallowing an error, `unwrap_or_default()` on a real fault, retrying a deterministic failure, or widening a type/scope so a mismatch stops being visible.
- Sleeping instead of awaiting a condition; the anti-flake rules above are a special case of this.
- Duplicating a helper because the shared one does not quite fit. Fix the shared one — for a security control this is not slop but a defect, since the copies drift.
- Reaching for a bigger permission, a wider scope, or a looser default because the correct one is inconvenient. Default-deny is not negotiable for convenience.
- Declaring work done on a platform, path, or condition that was never exercised. Say plainly what was not run.

Forbidden when a test, gate, or check fails:
- Weakening, narrowing, `#[ignore]`-ing, or deleting a failing test so a change can land. If a test is genuinely wrong, say so, state why, and get human sign-off — do not edit it silently in the same commit it blocks.
- `allow`-ing a lint, `--no-verify`, `--force` past a gate, or skipping a CI job to go green.
- Regenerating or hand-editing a generated artifact until a check passes without confirming the generator itself is current. A stale generator can emit a self-consistent wrong artifact that satisfies a staleness check and still fails the content test. Rebuild the generator from the working tree first.
- Treating a red gate as noise. Reproduce it locally, find the cause, then fix.

When the real fix is genuinely out of scope, agents MUST stop and say so — name the cause, propose the fix, and let a human choose. A disclosed limitation is acceptable; a silent workaround is not. A required failing check MUST be fixed before merge. If its contract is wrong, the rule/test change requires explicit human approval and lands before or with the dependent implementation; approval alone MUST NOT waive the check.

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
Hello-window / installer / RSS fixtures for competitor frameworks and native Swift live in [`https://github.com/gyldlab/keld-benches`](https://github.com/gyldlab/keld-benches) (public) under **OS-first** paths `{macos|windows|linux}/<framework>/...` (e.g. `macos/swift/appkit-wk`, `windows/electron/hello`). When an agent creates or updates such benchmark apps, it MUST commit and push to `gyldlab/keld-benches`, not into the Keld monorepo. If direct push access is unavailable, agents MUST open a fork PR to `gyldlab/keld-benches`, or warn and skip (do not leave fixtures only under `/tmp` or inside Keld). Agents MUST pick the OS folder for the machine / pack they actually ran. Agents MUST NOT put OS-agnostic dumps at the `keld-benches` repo root. Agents MUST NOT add Electron / Tauri / Wails / Neutralino / NW.js / Electrobun / Swift hello apps under Keld `docs/`, `competitors/` (shallow reference clones from the lockfile), or `/tmp`-only without pushing to `keld-benches`. Measured numbers MAY be recorded in `docs/engineering/budget-scoreboard.md` (Keld); rows SHOULD link the OS-qualified fixture path **and an immutable commit SHA or tag** in `keld-benches` (not only `main`).

## Commits & PRs
- Conventional commit format for the PR title (KeldBot's `title-lint` job enforces this on `opened`/`edited` and auto-applies the matching `type:*` label, since squash-merge uses the PR title as the final commit message): `type(scope): subject` — `type` MUST be one of `feat fix docs test chore ci style build` (the eight in actual use in this repo's history; agents MUST NOT invent a ninth without adding its `type:*` label and updating this list and `title-lint`'s allowed set in the same PR). `scope` is optional, lower-kebab, usually a crate/package/area (`ipc`, `wv/macos`, `research`). `subject` is imperative mood, no trailing period. Examples: `feat(ipc): …`, `fix(wv/macos): …`, `docs(research): …`.
- Individual commits within a branch are not linted and MAY be less formal (e.g. WIP fixups) — only the PR title, which becomes the merged history's commit message, MUST follow the standard.
- Long-lived branch: `main` only. Feature branch: `agent/kel-<n>-<slug>` from `origin/main` (one Linear issue, one worktree `../keld-<issue>`). Agents MUST NOT commit on another agent's checkout. Rebase onto `origin/main` before PR. `--force-with-lease` MAY rewrite the feature branch; agents MUST NOT force-push `main`.
- PR intake: [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md) — an intake form, not a second policy. Body MUST include headings Summary · Spec refs · Review gates · Tests · Platforms · Perf impact. Spec refs is architecture/spec paths or `No boundary change`. Review gates is the five names or `none`. Omit empty optional sections (Linear, rollback, screenshots); do not write N/A. Strip template HTML comments from the submitted body; do not delete them from the template file.
- **CodeRabbit review threads:** after addressing CodeRabbit feedback (same PR or a follow-up), agents MUST resolve the corresponding GitHub review thread(s) on that PR before calling work done. If the fix landed in a follow-up PR, resolve on the original PR too and comment with the merge SHA. Procedure: `.agents/skills/autofix/github.md` § Resolve review threads. Do not resolve threads whose fixes have not landed.
- Agents MUST NOT commit secrets or edit `.env*`; destructive git ops MUST have human approval.

## KeldBot (automated PR checks)
Before opening a PR, agents SHOULD self-check against this so the checks pass on the first
try instead of round-tripping through a CI failure — `.github/workflows/keldbot.yml` is the
source of truth if this drifts:
1. PR title matches `type(scope): subject` (§ Commits & PRs above) — `title-lint` is a
   **required** status check on `main`.
2. PR body has all six non-empty template sections (§ Commits & PRs' PR intake bullet) —
   `gatekeeper` is a **required** status check on `main`.
3. `size-label` auto-labels `size/XS`…`size/XL` from added+deleted lines — informational
   only, never fails, not a required check.

All three re-check on `edited` (title/body) or `synchronize` (new commits) and self-resolve:
fixing the title/body removes the failing label and updates KeldBot's own PR comment to ✅ —
agents MUST NOT wait for a human to clear it, just push the fix and let it re-run. If a check
is red, read KeldBot's own comment first (it names exactly what's missing) before re-deriving
the rule from this file. Comments/labels post from the `keldrobo` account, not
`github-actions[bot]`.

Label taxonomy on `gyldlab/keld` — bot-applied vs. human-applied differ, don't assume either:
- `needs-template-fix`, `needs-conventional-title`, `size/*`, `type:*` (8, one per allowed
  commit type) — bot-applied, per the three checks above.
- `gate:*` (5, matching the § Review gates names exactly) and `crate:*` (12, one per crate in
  the repo-map table) — human-applied only, for reviewer/triage visibility. KeldBot does NOT
  auto-detect `unsafe`/public-API/permission-model/dependency/wire-protocol changes or infer
  which crate a diff touches; agents/reviewers apply these by hand.

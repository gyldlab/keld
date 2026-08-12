# 06 — Documentation Map: What to Read, In What Order, and What Binds You

> Keld has far more documentation than code (see
> [`01-project-summary.md`](01-project-summary.md) for why that matters). Without a map,
> the natural failure mode is reading a research doc, mistaking it for a decision, and
> building the wrong thing. This file tells you which documents are **normative**, which
> are **exploratory**, and which are **point-in-time records**.

## The three tiers, up front

| Tier | What it means | Where it lives |
|---|---|---|
| **Binding rules** | You must follow these. Violating one means the rule change is the PR, not the violation. | [`AGENTS.md`](../../AGENTS.md), per-crate `crates/*/AGENTS.md`, [`docs/agents/workflow.md`](../agents/workflow.md) |
| **Normative specs** | The agreed design for v0.x. Changing one requires a design PR. Code that disagrees with a spec is a bug in one of the two — fix both in the same PR or state why. | [`docs/architecture/01..07-*.md`](../architecture/) |
| **Exploratory / historical** | Informs decisions, does not bind them. Dated. Cite it as evidence, never as a requirement. | [`docs/research/`](../research/), [`docs/engineering/`](../engineering/), [`task.md`](../../task.md) |

Two rules that follow directly from that table, both from
[`docs/agents/learnings.md`](../agents/learnings.md) and [`AGENTS.md`](../../AGENTS.md):

- **Never cite `docs/research/from-outside/` in your work.** Those are raw external
  research exports. The polished numbered docs in `docs/research/` are the citable corpus.
- **Numbered docs are paths.** If you renumber a doc, you must update every reference to
  it across the repo.

### A note on what is actually in the git repo

[`.gitignore`](../../.gitignore) excludes `/docs/`, `/competitors/`, `/ROADMAP.md`,
`/llms.txt`, and `/.github/` — "Proprietary IP (local only — never commit) … Public repo
is implementation code only." Almost everything this map describes exists on maintainer
machines and not in the pushed repository. `.claude/` and `task.md` are not ignored but
are currently untracked. If you clone the public repo, you get the crates, `README.md`,
`AGENTS.md`, and the build/lint configs — nothing else on this page.

## Root files

| File | What's in it | When you read it |
|---|---|---|
| [`AGENTS.md`](../../AGENTS.md) | **The single most important file in the repo.** Ground truth, crate map, the three-command verification gate, Rust rules (`unsafe` policy, no `unwrap`/`expect`/`panic!` in libs, typed errors, hot-path discipline, dependency review), TypeScript rules, naming, security/perf rules, the five human review gates, working rules, the mandatory self-improvement rule, and commit/PR format. | Before your first line of code, and again whenever you're unsure whether something is allowed. |
| [`README.md`](../../README.md) | The 30-second pitch, the intended `migrate → dev → build` flow, the workspace layout, and the two commands that work today. | First five minutes. |
| [`ROADMAP.md`](../../ROADMAP.md) | Phases 0–4 with exit criteria (gated on criteria, not dates) plus standing tracks. Checkbox state is meaningful — unchecked items are genuinely open. | When you want to know whether the thing you're about to build is in scope *now*. |
| [`llms.txt`](../../llms.txt) | A short machine-readable index of the repo's docs for LLM consumers: one-line project description plus links to `AGENTS.md`, architecture 01–07, research 00–09, and the agent workflow docs. Note it stops at research `09` and does not list `10–17` or the drafts — it is a snapshot, not a live index. | When wiring up agent tooling, or as a fast link sheet. |
| [`task.md`](../../task.md) | A **ledger** of a Linear harness run dated 2026-07-10: issue inventory by status, per-ticket blueprints and acceptance criteria, execution order, gate commands, and the results at the time (12/12 tests then). Historical, not a live to-do list. | When you need to know why a Linear issue was closed, or what "COMPLETE" meant for a given ticket. |
| [`justfile`](../../justfile) · [`Cargo.toml`](../../Cargo.toml) · [`clippy.toml`](../../clippy.toml) · [`deny.toml`](../../deny.toml) · [`rustfmt.toml`](../../rustfmt.toml) · [`rust-toolchain.toml`](../../rust-toolchain.toml) · [`.config/nextest.toml`](../../.config/nextest.toml) | Not prose, but they are the enforcement layer behind `AGENTS.md`. `just ci` runs every gate CI would run. Workspace lints (pedantic clippy, `missing_docs`, `unsafe_code = "deny"`) live in `Cargo.toml`, with comments explaining each choice. | When a lint fails and you want to know whether it's negotiable (it usually isn't). |

## `docs/architecture/` — normative for v0.x

Seven documents. Each is binding design; changes go through a design PR. Read 01 in full
on day one; read the others when you touch their area.

| Doc | What's in it | When you'd read it |
|---|---|---|
| [`01-overview.md`](../architecture/01-overview.md) | The one diagram (three principals, three trust levels), the eight ordered design principles, the crate/package topology table with dependency rules, the process and thread model, the CI-gated performance budgets (§5), and the v1 non-goals (§6). | Day one, completely. Everything else assumes it. |
| [`02-ipc.md`](../architecture/02-ipc.md) | kipc: the two-link topology, wire protocol and framing, the zero-copy bulk plane, schema-first contracts, how the Electron shim maps onto it, hot-path implementation rules, and failure/lifecycle semantics. | Any work in `keld-ipc`, or anything that sends a message anywhere. |
| [`03-security.md`](../architecture/03-security.md) | Principals and trust levels, the `keld.permissions.jsonc` manifest, why it's generated rather than hand-written, defense-in-depth enforcement mechanics, update security, and the honesty ledger of what Keld does **not** promise (§6). | Any work in `keld-guard`, any new privileged operation, any capability question. |
| [`04-electron-compat.md`](../architecture/04-electron-compat.md) | The migration developer experience end to end, the exact five-file config surface (§2), how the shim is layered, compat tiers and the public scoreboard, native-module policy, the migration corpus harness, and the updater bridge trap every migrator hits. | Compat work, migration work, or any time you need to answer "how does this behave under `@keld/electron`?" — which principle #1 says is every time. |
| [`05-webview-and-native.md`](../architecture/05-webview-and-native.md) | Why `keld-wv` is Keld's own binding layer (wry-informed, not wry-bound), the `WebEngine` trait, the `window.keld` renderer bridge contract, the `keld-native` API surface, and the `keld-ext` plugin path. | Any `keld-wv` or `keld-native` work. |
| [`06-runtime-and-tooling.md`](../architecture/06-runtime-and-tooling.md) | Bun as a *supervised child*, never embedded (§1, with the reasoning); the CLI verb-by-verb contract (§2); `keld-pack` packaging and cross-compilation; `keld-update` delta updates; and dev-loop targets. | CLI, packaging, updater, or runtime-supervisor work. |
| [`07-agent-experience.md`](../architecture/07-agent-experience.md) | Agents as a first-class user persona: the framework-wide "errors state the fix" standard with the `KELD-<area><nnn>` code shape (§2), docs-for-agents rules, the official MCP server design, the agent-eval harness as a CI metric, guardrails for vibe-coded apps, and the CLI contract for agents. | Before designing any error type, CLI output, or public API. §2 governs error messages everywhere in the codebase. |

## `docs/research/` — exploratory, dated, does not bind

Evidence, not requirements. Every doc carries a research date. Where research and
architecture disagree, architecture wins (and the disagreement is worth raising).

### Competitor teardowns

| Doc | Subject |
|---|---|
| [`00-landscape.md`](../research/00-landscape.md) | **Entry point.** Executive summary: the field one line each, the head-to-head matrix, the five structural problems nobody has solved together, a "steal list" of what each competitor genuinely got right, and Keld's five falsifiable theses. |
| [`01-electron.md`](../research/01-electron.md) | The incumbent Keld must replace *and* stay compatible with — architecture recap and issue catalog. Two postures: attack the architecture, adopt the API surface. |
| [`02-tauri.md`](../research/02-tauri.md) | The strongest technical competitor and closest philosophical cousin. Explains why the differentiation is not "lighter than Tauri." |
| [`03-electrobun.md`](../research/03-electrobun.md) | Closest existing implementation of the Bun-main-process thesis; updated 2026-08-08 for its 2.0 multi-runtime architecture. |
| [`04-deno-desktop.md`](../research/04-deno-desktop.md) | The newest and strategically most dangerous entrant — a subcommand of a runtime people already have. Steal its distribution insights, attack its architectural shortcuts. |
| [`15-wails.md`](../research/15-wails.md) | Wails (Go + system webviews): same single-native-process shape as Tauri with a mandatory Go backend; v3 beta August 2026. |

### Ecosystem and domain surveys

| Doc | Subject |
|---|---|
| [`05-rust-wave.md`](../research/05-rust-wave.md) | What the 2024–2026 Rust rewrites of the JS toolchain teach Keld, each lesson mapped to a Keld decision. The source of the "compatibility-first wins an incumbent's users" (Rspack/Rolldown) and "state machines, no async runtime in hot paths" (Bun) principles. |
| [`06-webview-reality.md`](../research/06-webview-reality.md) | The honest per-platform accounting of system webviews. **The platform truth table** — this is the doc that shapes the per-platform engine policy. |
| [`07-agent-first.md`](../research/07-agent-first.md) | Research basis for the agentic system, with two consumers: how Keld is built (agents write, humans architect and review) and how Keld serves agents as users. Feeds `AGENTS.md`, `docs/agents/`, and architecture 07. |
| [`10-ipc-state-of-the-art.md`](../research/10-ipc-state-of-the-art.md) | Schema-first bridge survey: comparison table, serialization candidates, codegen approaches, transport per leg, hot-path design, prior-art pain to avoid. Feeds architecture 02. |
| [`11-security-model.md`](../research/11-security-model.md) | Competitor security models, OS primitives available, the recommended Keld model, ranked risks and mitigations. Feeds architecture 03. |
| [`12-distribution.md`](../research/12-distribution.md) | Signing and notarization reality in 2026, framework update stacks, packaging formats users and IT actually want, and the pain points to preserve as design constraints. Feeds `keld-pack` / `keld-update` and Phase 3. |
| [`13-electron-api-usage.md`](../research/13-electron-api-usage.md) | Which Electron modules real apps use, expressed as frequency tiers; hardest migration surfaces; native-module risk. Feeds the compat tiers in architecture 04. |
| [`17-native-frameworks.md`](../research/17-native-frameworks.md) | AppKit/SwiftUI, WinUI 3/WPF, Qt/QML, GTK4, Flutter Desktop, Compose Multiplatform — and from them "the native-parity bar": what native gives for free, where it chronically fails, and what Keld must match, can beat, or must refuse to copy. |

### Audits and synthesis

| Doc | Subject |
|---|---|
| [`08-competitor-source-audit.md`](../research/08-competitor-source-audit.md) | Ground-truth survey of the vendored clones in `competitors/`: pinned commit inventory, per-repo layout/CI/core-machinery/steal-or-avoid notes, a consolidated adopt/adapt/avoid list, an **implementer reading order** into competitor source, stated limitations, and a 2026-08-08 refresh log of what changed upstream. Read before you go looking in `competitors/`. |
| [`09-tooling-context7-audit.md`](../research/09-tooling-context7-audit.md) | Per-tool best-practice analysis for Keld's stack, recommended toolchain versions, CI pipeline recommendation, tools to adopt and to skip. |
| [`14-phase0-synthesis.md`](../research/14-phase0-synthesis.md) | **The capstone.** Market gap statement, ranked ~10× opportunities, ranked risks with mitigations, one-paragraph guidance per Phase 1 RFC, the frozen benchmark baseline table, and the Phase 0 exit checklist. If you read only two research docs, read `00` and this. |

### Drafts — `docs/research/drafts/`

Four deep risk audits dated 2026-08-08, explicitly marked **draft** and explicitly
scoped to go deeper and newer than `06-webview-reality.md` without duplicating it. Every
number is either cited or marked as an estimate.

| Draft | Subject |
|---|---|
| [`16a-bun-runtime-risks.md`](../research/drafts/16a-bun-runtime-risks.md) | Bun as the supervised app-process runtime — primary sources only; phase-1 input to revisiting the Bun integration RFC. |
| [`16b-wkwebview-risks.md`](../research/drafts/16b-wkwebview-risks.md) | WKWebView/WebKit on macOS: what it allows today, hard limits against specs 02/03/05, what changed in the last year, and a source-level costing of bundling our own WebKit. |
| [`16c-webview2-risks.md`](../research/drafts/16c-webview2-risks.md) | WebView2 on Windows as the `keld-wv` Windows backend. |
| [`16d-webkitgtk-risks.md`](../research/drafts/16d-webkitgtk-risks.md) | WebKitGTK/WPE on Linux, focused on the question `06` doesn't answer: since this is the one fully open-source system engine Keld targets, is upstream patching or vendoring a real lever, and what does it cost? |

There is no consolidated `16-*.md`; the drafts are the artifact. Numbering jumps from
`15` to `17` in the parent directory.

### Raw external research — `docs/research/from-outside/`

24 files. Fifteen are Perplexity-style deep-research exports whose filenames are the
truncated prompt (Electron criticisms, Tauri v2 complaints, webview differences, IPC/bridge
architectures, security models, distribution and auto-update, Electrobun, Deno Desktop,
Electron API usage, MCP servers, AX, AGENTS.md state of the art, agent-orchestration,
API/error/docs design for agents, vibe-coding failure modes). Nine are `twiter*.md`
digests of practitioner discourse (the Rust tooling wave, Bun's Rust rewrite, the desktop
framework conversation, agentic-engineering practitioners, AX leaders).

**These are inputs, not sources.** The rule is explicit in
[`docs/agents/learnings.md`](../agents/learnings.md): never cite `from-outside/` directly —
cite the polished numbered doc that consumed it.

## `docs/agents/` — how work gets done here

| Doc | What's in it |
|---|---|
| [`workflow.md`](../agents/workflow.md) | The development loop, binding on humans and agents alike: pick up a Linear issue → spec gate (never implement from an unapproved spec) → isolate in a git worktree (`../keld-<issue>`, branch `agent/<issue>-<slug>`, one issue per tree) → implement with tests → run the verification gate → adversarial self-review of the full diff → PR. Plus parallelism rules (3–7 concurrent agents, disjoint crates, single-writer foundational files), the split between hard CI gates and human review gates, and failure etiquette ("a failing test you didn't write is signal, not noise"; "partial + accurate > complete + vague"). |
| [`spec-template.md`](../agents/spec-template.md) | The ten-section template every change bigger than a bug fix needs, copied to `docs/specs/<kebab-name>.md`: goal and non-goals, spec refs, binary acceptance criteria (each becomes a test), design, boundaries (including "must not touch"), ordered tasks, test plan, review gates triggered, perf impact, open questions. Implementation may begin only at `Status: approved`. Note `docs/specs/` does not exist yet — you may be the first. |
| [`learnings.md`](../agents/learnings.md) | The **append-only gotcha log**. Read it before starting a task; it is deliberately kept small because every agent session loads it. |

### The self-improvement rule (mandatory)

[`AGENTS.md`](../../AGENTS.md) § Self-improvement makes this an obligation, not a courtesy:

> Non-obvious gotcha (>10 min saved) → append ONE line to `docs/agents/learnings.md` same PR:
> `- YYYY-MM-DD [area] fact. (evidence: path, issue, or command)`
> Grep first (no dupes/opinions). Stale rule here → fixing it *is* the task.

In practice: one line, newest last, `[area]` is a crate short name or `ts`/`build`/`ci`/`process`,
and it must carry evidence — a path, an issue, or a command. Facts only, no opinions, no
duplicates. Maintainers compact the file past roughly 40 entries by promoting stable
learnings into `AGENTS.md` or the relevant spec. The last clause matters as much as the
first: if a rule in an `AGENTS.md` has gone stale, correcting it *is* the task, not a
distraction from it.

## `docs/engineering/` — point-in-time audits

| Doc | What's in it | When you'd read it |
|---|---|---|
| [`alignment-audit-2026-07-08.md`](../engineering/alignment-audit-2026-07-08.md) | A read-only audit across vision, research, architecture, `AGENTS.md`, roadmap, tooling/CI, Linear, crates, and `competitors/` hygiene. Verdict: "MOSTLY ALIGNED" — the technical story is coherent end to end; drift is concentrated in program tracking. Contains a scorecard, contradictions with suggested fixes, gaps, Linear drift, verification output, and prioritized actions. | When something feels inconsistent between docs, check whether the audit already named it. |
| [`linear-roadmap-mapping.md`](../engineering/linear-roadmap-mapping.md) | The translation table between Linear's project numbering and `ROADMAP.md`'s phase numbering — they do not match. | Every time you link an issue to a roadmap milestone. |
| [`tooling-audit.md`](../engineering/tooling-audit.md) | Senior-engineer review of the toolchain: what was thin at audit start, findings on the workspace and each config file, CI compared against competitors, changes applied, recommendations to adopt later, verification, and open questions. Explains *why* each lint and config choice exists. | Before changing anything in `Cargo.toml` lints, `clippy.toml`, `deny.toml`, or CI. |

## `competitors/` — vendored source as reference and oracle

Sixteen git clones, roughly 2.9 GB, gitignored and local-only. They exist so design
questions get answered by reading real implementations instead of guessing.

Seven have written teardowns in
[`08-competitor-source-audit.md`](../research/08-competitor-source-audit.md), with pinned
commits and per-repo notes:

| Clone | Why it's here |
|---|---|
| `electron/` | The compat **oracle**. `crates/keld-compat/AGENTS.md` makes Electron's documented behavior authoritative — conformance entries cite it *before* implementation. Shell and lib only; no Chromium tree. |
| `tauri/` | ACL/capabilities and IPC reference — the model Keld's guard is measured against. |
| `wry/` | Webview layer reference. `keld-wv`'s per-platform module layout deliberately mirrors `wry/src/{wkwebview,webview2,webkitgtk}/`, and the macOS hello slice currently depends on wry directly. |
| `tao/` | Event loop reference; also a current dependency of the macOS slice. |
| `electrobun/` | Bun-main-process prior art: schema/RPC patterns worth studying, transport worth avoiding. |
| `deno/` | Deno Desktop lives inside this repo (no standalone repo) — ABI pinning and cross-compilation patterns. |
| `wails/` | Go host; `master` holds v2 plus `v3/`. Teardown in `15-wails.md`. |

Nine more were added 2026-08-08 and are **reference material without a written teardown** —
present because they are the primary source for a topic the specs depend on:

| Clone | Topic it backs |
|---|---|
| `bun/` | The runtime Keld supervises (and the Rust-rewrite discipline research/05 draws on) |
| `chromium-embedded/` | The opt-in pinned-engine path (CEF) in the engine policy |
| `webkit/` | WebKit upstream — the "patch or vendor?" question in draft 16d, and WKWebView behavior in 16b |
| `gtk/` | The GTK4 side of the Linux backend and the GTK4 section of `17-native-frameworks.md` |
| `webview2-samples/` | Microsoft's own WebView2 usage patterns for the Windows backend |
| `tauri-plugins-workspace/` | Prior art for the `keld-native` module surface |
| `sparkle/` | The macOS updater bar `keld-update` is measured against |
| `zig-bsdiff/` | Delta-patch implementation reference for `keld-update` |
| `vscode/` | The flagship Electron application — the real-world compat target |

Two working rules. **Reference, never copy** — licensing and architecture both differ,
and `AGENTS.md` requires justified, reviewed dependencies. And **read
`08-competitor-source-audit.md` first**: it has an implementer reading order that points
at the exact paths worth opening, plus an explicit list of what the clones do *not*
contain (no full Chromium/Node trees, no Deno Desktop prebuilts, no built Electrobun
binaries).

## Per-crate `AGENTS.md` — required reading before you edit

[`AGENTS.md`](../../AGENTS.md) says plainly: "Read crate `AGENTS.md` before editing that
crate." Four crates have one today; each adds invariants on top of the root rules rather
than repeating them.

| Crate doc | The invariants it adds |
|---|---|
| [`crates/keld-ipc/AGENTS.md`](../../crates/keld-ipc/AGENTS.md) | The wire is a versioned protocol — any frame-layout/`FrameKind`/flag/handshake change means a version bump plus the wire review gate plus a spec §2 update, in one PR. Test wire constants as facts (`HEADER_LEN == 16`), not struct layout. State-machine readers/writers, no async, no steady-state allocation. Credit-window backpressure, no unbounded queues. `unsafe` only in the future `shm` module. postcard on the hot path; JSON only for `--inspect-ipc`. Fuzz the decode paths — malformed webview input is expected, not a bug. |
| [`crates/keld-wv/AGENTS.md`](../../crates/keld-wv/AGENTS.md) | `unsafe` is allowed here, in platform backends only, with `deny(unsafe_op_in_unsafe_fn)` and a `// SAFETY:` comment citing the platform contract. All engine/window mutations on the UI thread via the command queue; never platform handles from I/O or pool threads. `WebEngine` trait changes are a design review. Platform quirks need OS + version + source link, or they get reverted. Linux: probe the GPU stack and apply safe mode before init — never tell users to export env vars. |
| [`crates/keld-guard/AGENTS.md`](../../crates/keld-guard/AGENTS.md) | This is *the* security boundary. Default-deny: unknown capability, channel, or scope, or a missing manifest → `Deny`, with no interim allow. Principals are host-minted and unforgeable; webview principals rotate on navigation. Deny text is API — every `DenyReason` names the capability/scope and the fix, and that text is tested. Scope matching resolves `$VARS`, symlinks, and `..` before matching, with permanent bypass fixtures (traversal, symlink swap, case folding, wildcard-swallow). No dev-mode special case inside the engine. No allocation on the `Allow` path. |
| [`crates/keld-compat/AGENTS.md`](../../crates/keld-compat/AGENTS.md) | Electron's documented behavior is the oracle; the conformance entry citing a doc or fixture comes *before* implementation. Divergence must be explicit — a `keld.compat.ts` quirks flag or a scoreboard mark, chosen in the PR. Event **ordering** is tested as sequences, not just outcomes. No Electron-isms leak into `keld-core`/`keld-ipc`. A corpus score drop is a P1 regression. |

The other seven crates (`keld-core`, `keld-native`, `keld-runtime`, `keld-update`,
`keld-pack`, `keld-host`, `keld-cli`) have no crate-level `AGENTS.md` yet; their module
docs carry a "Normative spec:" line pointing at the governing architecture section
instead, and that pointer is the current substitute.

## `.claude/` — the agent workflow system (untracked, local)

A portable Claude Code setup dropped into this repo. It is **not** Keld's own agent
system — `AGENTS.md` and `docs/agents/` are — and it is untracked, so it may differ
machine to machine. It is not calibrated to this repo yet: there is no root `CLAUDE.md`,
no `PROJECT_MEMORY.md`, no `project-calibration.json`, and `skills/stack/` is still empty
(all four are outputs the `project-calibrator` skill would generate).

| Path | What it is |
|---|---|
| [`.claude/GUIDE.md`](../../.claude/GUIDE.md) | Step-by-step usage walkthrough — the "start here" file for the workflow, organized by what you're trying to do. |
| [`.claude/README.md`](../../.claude/README.md) | Internal design reference for the system's structure and file layout. |
| [`.claude/DEFINITION_OF_DONE.md`](../../.claude/DEFINITION_OF_DONE.md) | One canonical "done" bar every command's final READY check runs against — feature works and is demonstrated, existing behavior intact, build/type-check/lint pass, tests updated, docs updated, minimal scoped diff. Explicitly excludes committing, pushing, and opening PRs from the definition of done. |
| [`.claude/LESSONS.md`](../../.claude/LESSONS.md) | Accumulated lessons from real work in this system (distinct from `docs/agents/learnings.md`, which is Keld's own binding log). |
| `.claude/agents/` | 12 role agents (backend, frontend, database, devops, QA, security-reviewer, solutions-architect, technical-writer, bug-fixer, business-analyst, domain-expert, git-pr-agent). |
| `.claude/commands/` | 17 workflow entry points (`/feature-builder`, `/bug-fix`, `/refactor`, `/code-review`, `/onboard-project`, `/architecture-review`, `/estimate`, `/gitpush`, …). |
| `.claude/skills/` | `project-calibrator`, `scope-router`, `brainstorm`, an empty generated `stack/`, and `standards/` — 10 situational standards (coding, api-design, database-design, testing, security, performance, caching, authentication, accessibility, documentation). |
| `.claude/templates/` | 5 document templates (architecture doc, PRD, bug report, test plan, UI spec). |
| `.claude/plans/`, `.claude/settings.local.json`, `.claude/local-env.example.json` | Working plans and local machine configuration. |

Where the two systems disagree about Keld, **`AGENTS.md` and `docs/agents/` win** —
`.claude/skills/standards/` states its own precedence rule that the repo's established
conventions override its baseline.

## `docs/onboarding/` — this directory

The guided entry layer over everything above. Start with
[`01-project-summary.md`](01-project-summary.md) for the product, performance budgets,
current-state ledger, and roadmap; use this map to navigate the source-of-truth tiers.
Agents connecting through MCP should then read
[`07-mcp-server.md`](07-mcp-server.md) for client registration and the intended
doctor → docs search → permissions explain workflow.

## Suggested reading order

### First day (~2 hours, in this order)

1. [`README.md`](../../README.md) — the pitch and the intended flow. Five minutes.
2. [`docs/onboarding/01-project-summary.md`](01-project-summary.md) — and specifically its
   current-state section, so you calibrate the spec-to-code gap before reading any spec.
3. [`AGENTS.md`](../../AGENTS.md) — in full. It is the shortest high-value file in the repo.
4. [`docs/architecture/01-overview.md`](../architecture/01-overview.md) — in full. The
   diagram, the eight principles, the crate topology, the budgets, the non-goals.
5. [`docs/research/00-landscape.md`](../research/00-landscape.md) — the five structural
   problems and the head-to-head matrix. This is *why* the architecture looks like it does.
6. [`ROADMAP.md`](../../ROADMAP.md) — where the project is and what gates the next step.
7. [`docs/agents/learnings.md`](../agents/learnings.md) — six lines, all load-bearing.
8. Run it: `cargo nextest run --workspace --profile ci`, then `just hello` on macOS. Seeing
   17 tests pass and one window open tells you more about the state of the project than
   another hour of reading.

### First week

9. [`docs/agents/workflow.md`](../agents/workflow.md) and
   [`docs/agents/spec-template.md`](../agents/spec-template.md) — how a change gets from
   issue to merge here.
10. [`docs/research/14-phase0-synthesis.md`](../research/14-phase0-synthesis.md) — ranked
    opportunities and risks; the fastest way to understand what the team believes matters.
11. The two architecture docs closest to your first task — most likely
    [`02-ipc.md`](../architecture/02-ipc.md) and
    [`05-webview-and-native.md`](../architecture/05-webview-and-native.md), since those are
    where the live code is — plus the matching crate `AGENTS.md`.
12. [`docs/architecture/03-security.md`](../architecture/03-security.md) and
    [`07-agent-experience.md`](../architecture/07-agent-experience.md) §2 — default-deny and
    the error-message standard reach into code you would not expect them to.
13. [`docs/architecture/04-electron-compat.md`](../architecture/04-electron-compat.md) —
    because principle #1 says every design must answer how it behaves under
    `@keld/electron`, and you cannot answer that without reading this.
14. [`docs/research/06-webview-reality.md`](../research/06-webview-reality.md) — the
    platform truth table, before you form any opinion about "the webview."
15. [`docs/engineering/tooling-audit.md`](../engineering/tooling-audit.md) and the
    [`alignment-audit`](../engineering/alignment-audit-2026-07-08.md) — why the toolchain
    is configured the way it is, and which inconsistencies are already known.

### As needed, not up front

`docs/research/01–04`, `15`, `17` (open the one whose competitor you're reasoning about);
`10–13` (open the one matching your domain); `drafts/16a–16d` (before platform-backend
work); `08-competitor-source-audit.md` (before reading competitor source);
`docs/research/from-outside/` (rarely — and never as a citation);
[`task.md`](../../task.md) (when you need the history of a Linear issue).

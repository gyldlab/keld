# 06 — Documentation Map: What to Read, In What Order, and What Binds You

> Keld has far more documentation than code (see
> [`01-project-summary.md`](01-project-summary.md) for why that matters). Without a map,
> the natural failure mode is reading a research doc, mistaking it for a decision, and
> building the wrong thing. This file tells you which documents are **binding**, which
> are **architecture or approved feature specs**, which are **engineering narrative** (why, not a new rule
> layer), which are **exploratory**, and which are **point-in-time records**.

## The tiers, up front

| Tier | What it means | Where it lives |
|---|---|---|
| **Binding rules** | You must follow these. Violating one means the rule change is the PR, not the violation. Topic playbooks bind only when their route matches the task. | [`AGENTS.md`](../../AGENTS.md), per-crate `crates/*/AGENTS.md`, [`.agents/index.md`](../../.agents/index.md), matched `.agents/*.md`, [`docs/agents/workflow.md`](../agents/workflow.md), [`docs/engineering/keld-error-codes.md`](../engineering/keld-error-codes.md) (CI-enforced `KELD-*` registry) |
| **Architecture specs** | The agreed design for v0.x. Changing one requires a design PR. Code that disagrees with a spec is a bug in one of the two — fix both in the same PR or state why. | [`docs/architecture/01..07-*.md`](../architecture/) |
| **Feature specs** | A scoped implementation contract created from the approved template and tied to Linear. `approved`, `implementing`, and `done` specs may govern work at their stated phase; `draft` specs do not authorize implementation. | [`docs/specs/`](../specs/) |
| **Engineering narrative** | What we chose, why, what we rejected, and what is not next. **Not** RFC 2119 — [`AGENTS.md`](../../AGENTS.md) still binds. Onboarding pointer for “why.” | [`docs/engineering/decisions.md`](../engineering/decisions.md) |
| **Exploratory / historical** | Informs decisions, does not bind them. Dated. Cite it as evidence, never as a requirement. | [`docs/research/`](../research/), other [`docs/engineering/`](../engineering/) audits, [`task.md`](../../task.md) |

Two rules that follow directly from that table, both from
[`docs/agents/learnings.md`](../agents/learnings.md) and [`AGENTS.md`](../../AGENTS.md):

- **Never cite `docs/research/from-outside/` in your work.** Those are raw external
  research exports. Numbered `docs/research/` notes are exploratory evidence, not
  required onboarding — the why-pointer is
  [`docs/engineering/decisions.md`](../engineering/decisions.md).
- **Numbered docs are paths.** If you renumber a doc, you must update every reference to
  it across the repo.

### A note on what is actually in the git repo

Documentation under `/docs/` is tracked, including status-bearing feature contracts in
`docs/specs/`, along with generated `llms.txt` and `llms-full.txt`.
[`.gitignore`](../../.gitignore) keeps `/competitors/`, `/ROADMAP.md`,
`/docs/research/`, and `/.claude/` local-only. `.github/` is tracked (KEL-39). The
generated corpus is narrower than the tracked docs tree: its ordered allowlist excludes
research and all unlisted documents. The engineering decision log is on that allowlist;
numbered research is not.

## Root files

| File | What's in it | When you read it |
|---|---|---|
| [`AGENTS.md`](../../AGENTS.md) | **The single most important file in the repo.** Ground truth, crate map, the three-command verification gate, Rust rules (`unsafe` policy, no `unwrap`/`expect`/`panic!` in libs, typed errors, hot-path discipline, dependency review), TypeScript rules, naming, security/perf rules, the five human review gates, working rules, the mandatory self-improvement rule, and commit/PR format. | Before your first line of code, and again whenever you're unsure whether something is allowed. |
| [`README.md`](../../README.md) | The 30-second pitch, the intended `migrate → dev → build` flow, the workspace layout, and the two commands that work today. | First five minutes. |
| [`docs/engineering/linear-roadmap-mapping.md`](../engineering/linear-roadmap-mapping.md) | Tracked phase map between Linear project numbers and the local `ROADMAP.md` (gitignored). Use this, not an untracked file, as required reading for “what is in scope now.” | When you want to know whether the thing you're about to build is in scope *now*. |
| [`llms.txt`](../../llms.txt) · [`llms-full.txt`](../../llms-full.txt) | Generated agent-readable documentation: a compact curated index and the corresponding concatenated corpus. `tools/llms_docs.rs` fixes source order and excludes research/local-only paths; `just llms-check` rejects drift. | When wiring up agent tooling, bulk-ingesting the authoritative docs corpus, or checking what the official MCP docs search embeds. |
| [`task.md`](../../task.md) | A **ledger** of a Linear harness run dated 2026-07-10: issue inventory by status, per-ticket blueprints and acceptance criteria, execution order, gate commands, and the results at the time (12/12 tests then). Historical, not a live to-do list. | When you need to know why a Linear issue was closed, or what "COMPLETE" meant for a given ticket. |
| [`justfile`](../../justfile) · [`Cargo.toml`](../../Cargo.toml) · [`clippy.toml`](../../clippy.toml) · [`deny.toml`](../../deny.toml) · [`rustfmt.toml`](../../rustfmt.toml) · [`rust-toolchain.toml`](../../rust-toolchain.toml) · [`.config/nextest.toml`](../../.config/nextest.toml) | Not prose, but they are the enforcement layer behind `AGENTS.md`. `just ci` runs every gate CI would run. Workspace lints (pedantic clippy, `missing_docs`, `unsafe_code = "deny"`) live in `Cargo.toml`, with comments explaining each choice. | When a lint fails and you want to know whether it's negotiable (it usually isn't). |

## `docs/architecture/` — normative for v0.x

Seven documents. Each is binding design; changes go through a design PR. Read 01 in full
on day one; read the others when you touch their area.

| Doc | What's in it | When you'd read it |
|---|---|---|
| [`01-overview.md`](../architecture/01-overview.md) | The one diagram (three principal classes with host-minted instances), the eight ordered design principles, the crate/package topology table with dependency rules, the process and thread model, the target performance budgets/future gates (§5), and the v1 non-goals (§6). | Day one, completely. Everything else assumes it. |
| [`02-ipc.md`](../architecture/02-ipc.md) | kipc: webview/app-role link classes, wire protocol and framing, measured optional bulk lanes, schema-first contracts, how the Electron shim maps onto it, hot-path implementation rules, and failure/lifecycle semantics. | Any work in `keld-ipc`, or anything that sends a message anywhere. |
| [`03-security.md`](../architecture/03-security.md) | Principals and trust levels, the `keld.permissions.jsonc` manifest, why it's generated rather than hand-written, defense-in-depth enforcement mechanics, update security, and the honesty ledger of what Keld does **not** promise (§6). | Any work in `keld-guard`, any new privileged operation, any capability question. |
| [`04-electron-compat.md`](../architecture/04-electron-compat.md) | The migration developer experience end to end, the exact five-file config surface (§2), how the shim is layered, compat tiers and the public scoreboard, native-module policy, the migration corpus harness, and the updater bridge trap every migrator hits. | Compat work, migration work, or any time you need to answer "how does this behave under `@keld/electron`?" — which principle #1 says is every time. |
| [`05-webview-and-native.md`](../architecture/05-webview-and-native.md) | Why `keld-wv` is Keld's own binding layer (wry-informed, not wry-bound), the `WebEngine` trait, the `window.keld` renderer bridge contract, the `keld-native` API surface, and the `keld-ext` plugin path. | Any `keld-wv` or `keld-native` work. |
| [`06-runtime-and-tooling.md`](../architecture/06-runtime-and-tooling.md) | Bun as a supervised primary child plus named compatibility roles, never embedded (§1); the CLI contract (§2); cross-target assembly with explicit signing flows; delta updates and a minimal relaunch helper; and dev-loop targets. | CLI, packaging, updater, or runtime-supervisor work. |
| [`07-agent-experience.md`](../architecture/07-agent-experience.md) | Agents as a first-class user persona: the framework-wide "errors state the fix" standard with the `KELD-<area><nnn>` code shape (§2), docs-for-agents rules, the official MCP server design, the agent-eval harness as a CI metric, guardrails for vibe-coded apps, and the CLI contract for agents. | Before designing any error type, CLI output, or public API. §2 governs error messages everywhere in the codebase. |

## `docs/specs/` — scoped feature designs

Feature specs use [`docs/agents/spec-template.md`](../agents/spec-template.md), name the
owning Linear issue, expose a visible status, and turn acceptance criteria into tests.
`Status: draft` is design work only. `approved` authorizes the ordered implementation;
`implementing` means approved slices are in progress; `done` means the spec's entire
contract has landed and passed its gates. Read the current status and task checklist—do
not infer completion from the file merely existing.

The directory currently includes approved or implementing contracts such as the shipped
Keld MCP server and the external, optional developer-agent memory pilot. A feature spec
does not silently rewrite `docs/architecture/`: if a feature needs an architecture
change, that design change is part of the review.

## `docs/research/` — exploratory, dated, does not bind

Evidence, not requirements. Every doc carries a research date. Where research and
architecture disagree, architecture wins (and the disagreement is worth raising).
This directory is **not** required onboarding; the why-pointer is
[`docs/engineering/decisions.md`](../engineering/decisions.md).

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
| [`20-vscode-on-keld.md`](../research/20-vscode-on-keld.md) | VS Code north-star feasibility and local Bun/native probes. It is a demanding showcase, not Keld's product denominator. |
| [`46-vscode-north-star-framework-synthesis.md`](../research/46-vscode-north-star-framework-synthesis.md) | Audited P01–P20 synthesis: separates reusable framework contracts from VS Code-only work, records evidence gaps and maps the findings into roadmap/Linear. Read this instead of treating raw P files as decisions. |

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

`45-P1.md` through `45-P20.md` are another raw external-input set retained at the
research root because they arrived under those user-owned paths. Their copied `turn…`
citations are nonportable and P13's ephemeral benchmark artifacts are missing. Each
file carries a raw-input banner; use `46` for decisions and require a direct source
ledger before promoting any unresolved claim.

## `docs/agents/` — how work gets done here

| Doc | What's in it |
|---|---|
| [`workflow.md`](../agents/workflow.md) | The development loop, binding on humans and agents alike: pick up a Linear issue → spec gate (never implement from an unapproved spec) → isolate in a git worktree (`../keld-<issue>`, branch `agent/kel-<n>-<slug>` from `origin/main`, one issue per tree) → implement with tests → run the verification gate → adversarial self-review of the full diff → PR. Plus parallelism rules (3–7 concurrent agents, disjoint crates, single-writer foundational files), the split between hard CI gates and human review gates, and failure etiquette ("a failing test you didn't write is signal, not noise"; "partial + accurate > complete + vague"). Binding branch/PR-body rules live in root [`AGENTS.md`](../../AGENTS.md) § Commits & PRs. |
| [`spec-template.md`](../agents/spec-template.md) | The ten-section template every change bigger than a bug fix needs, copied to `docs/specs/<kebab-name>.md`: goal and non-goals, spec refs, binary acceptance criteria (each becomes a test), design, boundaries (including "must not touch"), ordered tasks, test plan, review gates triggered, perf impact, open questions. Implementation may begin only after human approval; an approved spec may then move to `Status: implementing`. |
| [`learnings.md`](../agents/learnings.md) | The **append-only gotcha log**. Read it before starting a task; it is deliberately kept small because every agent session loads it. |

### `.agents/` — conditional playbooks

[`.agents/index.md`](../../.agents/index.md) is the router. Load only the rows matching
the task; a topic playbook is not a second always-on root rules file. In particular,
[`.agents/memory.md`](../../.agents/memory.md) applies when configuring, using, reviewing,
upgrading, or removing an **approved external** contributor-memory service, and whenever
recalled material or a memory result appears unexpectedly. Ordinary Keld work with no
memory material does not load it. Surprise recall is quarantined and does not expand the
task; the playbook does not authorize installing, starting, or authenticating a service.

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

## `docs/engineering/` — error registry, decision log, and dated audits

[`keld-error-codes.md`](../engineering/keld-error-codes.md) is **binding**: CI
(`crates/keld-cli/tests/error_registry.rs`) requires a 1:1 match with scanned `KELD-*`
sources. [`decisions.md`](../engineering/decisions.md) is **engineering narrative**
(not RFC 2119): what we chose, why, what we rejected, and what is not next.
[`AGENTS.md`](../../AGENTS.md) still binds. Other files in this directory are dated
audits.

| Doc | What's in it | When you'd read it |
|---|---|---|
| [`keld-error-codes.md`](../engineering/keld-error-codes.md) | Canonical `KELD-*` registry. Adding a code without this file + the registry test is a bug. | When you add or change an error code. |
| [`budget-scoreboard.md`](../engineering/budget-scoreboard.md) | Measured hello size/RSS/installer vs architecture 01 §5 budgets, competitors, and Native Swift WKWebView floors. Win-conditions and byte autopsy. No `bench/` CI yet. | When recording or citing installer size, host bytes, or idle RSS. |
| [`decisions.md`](../engineering/decisions.md) | Decision log for humans: four uniques, wry vs spec 05, `KELD-*` errors, verification/CI, cargo-deny, nested `AGENTS.md`, `llms.txt` corpus, MCP/`$VARS`, review gates, the external-only memory boundary, the reuse-first maximum-compatibility program, and what is explicitly not next. | When you need “why we chose this” without treating research as a spec. Day-one why-pointer. |
| [`alignment-audit-2026-07-08.md`](../engineering/alignment-audit-2026-07-08.md) | A read-only audit across vision, research, architecture, `AGENTS.md`, roadmap, tooling/CI, Linear, crates, and `competitors/` hygiene. Verdict: "MOSTLY ALIGNED" — the technical story is coherent end to end; drift is concentrated in program tracking. Contains a scorecard, contradictions with suggested fixes, gaps, Linear drift, verification output, and prioritized actions. | When something feels inconsistent between docs, check whether the audit already named it. |
| [`linear-roadmap-mapping.md`](../engineering/linear-roadmap-mapping.md) | The translation table between Linear's project numbering and `ROADMAP.md`'s phase numbering — they do not match. | Every time you link an issue to a roadmap milestone. |
| [`tooling-audit.md`](../engineering/tooling-audit.md) | Senior-engineer review of the toolchain: what was thin at audit start, findings on the workspace and each config file, CI compared against competitors, changes applied, recommendations to adopt later, verification, and open questions. Explains *why* each lint and config choice exists. | Before changing anything in `Cargo.toml` lints, `clippy.toml`, `deny.toml`, or CI. |

## `competitors/` — vendored source as reference and oracle

Gitignored, local-only competitor checkouts exist so design questions can be answered by
reading pinned real implementations instead of guessing. Their count and disk footprint
depend on the current lock/sync state and are not a documentation contract.

Reviewed entries have written teardowns in
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
| [`crates/keld-guard/AGENTS.md`](../../crates/keld-guard/AGENTS.md) | This is *the* security boundary. Default-deny: unknown capability, channel, or scope, or a missing manifest → `Deny`, with no interim allow. Principals are host-minted and unforgeable; webview principals rotate on navigation. Deny text is API — every `DenyReason` names the capability/scope and the fix, and that text is tested. Destination matching resolves `$VARS`, symlinks, and `..` before matching; **v0** matches `$VARS` as literals and rejects `..` (not an Allow). No dev-mode special case inside the engine. No allocation on the `Allow` path. |
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
doctor → docs search → permissions explain workflow. Contributors explicitly evaluating
the optional external KEL-67 memory pilot should also read
[`08-optional-agent-memory.md`](08-optional-agent-memory.md); everyone else can skip it.

## Suggested reading order

### First day (~2 hours, in this order)

1. [`README.md`](../../README.md) — the pitch and the intended flow. Five minutes.
2. [`docs/onboarding/01-project-summary.md`](01-project-summary.md) — and specifically its
   current-state section, so you calibrate the spec-to-code gap before reading any spec.
3. [`AGENTS.md`](../../AGENTS.md) — in full. It is the shortest high-value file in the repo.
4. [`docs/architecture/01-overview.md`](../architecture/01-overview.md) — in full. The
   diagram, the eight principles, the crate topology, the budgets, the non-goals.
5. [`docs/engineering/decisions.md`](../engineering/decisions.md) — why current
   engineering looks like this (four uniques, wry scaffolding, errors, CI, what is
   not next). Narrative, not a new rule layer; [`AGENTS.md`](../../AGENTS.md) still
   binds.
6. [`docs/engineering/linear-roadmap-mapping.md`](../engineering/linear-roadmap-mapping.md)
   — Linear vs local `ROADMAP.md` numbering; the gitignored file is not required reading.
7. [`docs/agents/learnings.md`](../agents/learnings.md) — the gotcha log; load-bearing
   and short.
8. Run it: `cargo nextest run --workspace --profile ci`, then `just hello` on the target
   OS. A passing build proves only the compiled paths; observing the window on that OS is
   separate runtime evidence.

### First week

9. [`docs/agents/workflow.md`](../agents/workflow.md) and
   [`docs/agents/spec-template.md`](../agents/spec-template.md) — how a change gets from
   issue to merge here.
10. Numbered [`docs/research/`](../research/) (starting with
    [`00-landscape.md`](../research/00-landscape.md) and
    [`14-phase0-synthesis.md`](../research/14-phase0-synthesis.md) if they are in your
    tree) — exploratory evidence behind the architecture, not required reading and not
    a substitute for [`decisions.md`](../engineering/decisions.md).
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
[`07-mcp-server.md`](07-mcp-server.md) (when registering the official Keld MCP);
[`08-optional-agent-memory.md`](08-optional-agent-memory.md) (only for the approved
external pilot); [`task.md`](../../task.md) (when you need the history of a Linear issue).

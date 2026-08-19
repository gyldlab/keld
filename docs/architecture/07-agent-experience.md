# Agent Experience (AX) — Keld as an Agent-First Framework

> Normative for v0.x. Research basis: `docs/research/07-agent-first.md` (AX shipped by
> frameworks, MCP tool-design guidance, error/docs design evidence, vibe-coding failure
> modes). Position: **agents are a primary user persona** alongside humans. Most Keld
> apps will be written at least partly by coding agents; the framework is designed so
> those apps are correct on the first try and safe even when nobody reviews them.

The AX surface has five parts: errors (§2), docs (§3), the MCP server (§4), the eval
harness (§5), and guardrails for agent-written apps (§6). The CLI contract (§7) cuts
across all of them.

## 1. Design rules (inherited by every AX decision)

1. One canonical name per concept, everywhere. Verb+noun operations. Risk encoded in
   names (`fs.readScoped` vs `fs.readRaw`).
2. One obvious way per task; advanced paths explicitly labeled advanced.
3. Schema-first everything: `.k.ts` contracts and generated clients are the paved road;
   deterministic shapes; machine-readable deprecations; no breaking changes within a
   version.
4. Verbosity is cheap; ambiguity is expensive. Explicit over terse in APIs, logs,
   error text.
5. Every repeated agent failure observed in evals (§5) is triaged as a docs, error, or
   API bug — not as "the model was dumb."

## 2. Errors state the fix (framework-wide standard)

Every developer-facing error in Keld — Rust crates, CLI, `@keld/*` packages, compat
shim, guard denials — carries:

```
code      KELD-<area><nnn>   stable, greppable, documented (e.g. KELD-GUARD012)
message   what failed, with the failing value/field named
cause     the specific input/state that triggered it (when known)
fix       imperative next step: exact manifest patch, config change, or API to use
docs      https://keld.dev/e/KELD-<area><nnn> (task-oriented page, works in llms-full.txt)
```

- Rust: typed errors implement `Display` with `code` + `message` + `fix`. The `docs`
  URL is carried on the JSON/MCP `KeldErrorObject` (`https://keld.dev/e/<code>`);
  it is not inlined into every `Display` string. `keld-guard`'s `DenyReason` includes
  a `KELD-GUARD*` code and the `keld.permissions.jsonc` edit that would grant it.
- CLI: same objects rendered human-readable by default, `--json` for agents; exit
  codes are stable API.
- Compat shim: unsupported Electron APIs throw structured errors naming the tier
  status, the tracking issue, and the workaround doc — never a bare "not implemented".
- The error-code registry is `docs/engineering/keld-error-codes.md` (one heading per
  code — that heading **is** the docs stub). A `KELD-*` code emitted in `keld-ipc`,
  `keld-wv`, `keld-cli`, `keld-guard`, `keld-runtime`, `keld-native`, or `keld-compat`
  (plus `keld-cli` templates and workspace `tools/`) without a registry heading fails
  CI (`crates/keld-cli/tests/error_registry.rs`). Per-code website pages are deferred.
  Error messages are tested (exact-match on `fix` text where feasible).

## 3. Docs for agents

- **llms.txt + llms-full.txt** at the docs-site root from day one: `llms.txt` is the
  curated index (quickstart, task guides, API reference, error registry, compat
  scoreboard, changelog — one line + description each); `llms-full.txt` is the
  concatenated Markdown corpus. Both generated from docs sources in CI, never
  hand-maintained.
- **Task-first page skeleton** (every feature): what it does → quick start → how it
  works → when things go wrong → debugging → known limitations. Sections
  self-contained (agents ingest fragments).
- **Examples as tests**: every documented API has a runnable happy-path, a
  validation-error, and an edge-case example; CI extracts and executes doc snippets so
  examples cannot drift.
- **Generated project guidance**: `create-keld` and `keld migrate` emit an `AGENTS.md`
  into the app (build/dev/test commands for *that* app, Keld conventions, permission
  workflow, link to llms.txt) — Bun's `bun init` precedent. Keld's own docs ship an
  `agents.md` alongside llms.txt (Shopify precedent).

## 4. Official Keld MCP server

Ships with the CLI (`keld mcp serve`, stdio transport; part of `@keld/cli`). Small,
task-level, namespaced toolset — not an endpoint wrapper. v1 tools:

| Tool | Does | Notes |
|---|---|---|
| `keld_docs_search` | search docs/API/error registry; returns titled chunks + URLs | Context7/Stripe pattern: docs live in the same surface as actions |
| `keld_scaffold_app` | run create-keld with template/features; returns file map + next steps | idempotent into an empty dir |
| `keld_migrate_analyze` | run the Electron analyzer; returns compat report (score, per-API status, fix list) | read-only |
| `keld_doctor` | env/native-module/permission/web-baseline checks; structured findings, each with a `fix` | wraps `keld doctor --json` |
| `keld_permissions_explain` | why a call was denied + the exact manifest patch that would grant it | reads recorder output (dev) |
| `keld_dev_inspect` | running dev app: windows (id, title, url, state), app-process status, recent restarts | dev-mode only |
| `keld_ipc_trace` | recent kipc frames on a channel, decoded, summarized; `sample_limit`/`channel` filters | dev-mode only; JSON debug codec |
| `keld_logs_search` | query unified log (host/app/renderer, principal-tagged) by pattern/severity/time | `search`, never `read_all` |
| `keld_build_errors` | parse last build/bundle failure into {file, line, code, fix} | reuses §2 error objects |

Design rules (Anthropic tool-writing guidance, applied): responses high-signal and
size-bounded with truncation hints ("narrow `channel` or lower `sample_limit`");
`detail: concise|full` on inspect/trace tools; every error response carries a `hint`;
tool descriptions state cross-tool order ("call `keld_dev_inspect` to get `window_id`
before `keld_ipc_trace`").

Security: runtime-introspection tools (`keld_dev_inspect`, `keld_ipc_trace`,
`keld_logs_search`) bind to the local dev session only — they ride the same dev-mode
channel as `keld dev --inspect-ipc` and do not exist against release builds. The MCP
server holds no elevated authority: it can do exactly what the CLI can do.

## 5. Agent-eval harness (CI metric: one-shot buildability)

`evals/` (lands with the corpus harness, Phase 2): scripted agent sessions against
pinned models/harnesses, run nightly and per docs/API-touching PR:

- **Scenarios**: scaffold + build hello world; add a tray icon + IPC channel from
  docs; migrate a fixture Electron app and fix reported gaps; diagnose a seeded
  permission denial; diagnose a seeded build error.
- **Metrics per scenario**: pass/fail (app builds + smoke-runs + acceptance
  assertions), retries, time-to-first-success, tool/doc lookups used, which errors the
  agent hit and whether the `fix` text was followed.
- **Gate**: one-shot pass rate on the golden path is a release-blocking metric next to
  the perf budgets; a docs/API change that drops it is a regression.
- **Feedback loop**: transcripts are triaged; each repeated failure files a docs/error/
  API issue (rule §1.5). The harness is how AX claims stay honest — same philosophy as
  the compat scoreboard.

## 6. Guardrails for vibe-coded apps (Keld-enforced APIs)

Threat model: the app author's agent wrote code nobody carefully read (research/07 §6:
secrets leak ~2× human rate, ~55% of unguided AI code fails security checks, missing
authz is endemic). Keld's stance — **Keld-enforced APIs** hold even when review
doesn't. That is the host-checked capability manifest, network allowlist, webview
policy, and pack-time secret scan. Application logic the agent writes is not
covered:

1. **Authority is generated, visible, and enforced** (spec 03): default-deny manifest
   generated from recorded/static usage; enforced in the host, not in the app's own
   code; `keld build --frozen-permissions` fails CI on drift.
2. **Secrets**: `secrets` capability + OS keychain API is the paved road; `keld build`
   scans the bundle for high-entropy strings / known key formats and **fails** (not
   warns) on findings; `.env*` is never bundled; docs and templates never show inline
   keys.
3. **Network egress is an allowlist** from the manifest — an injected/hallucinated
   exfil endpoint is a build-time diff a human sees in review and a runtime deny if it
   ships anyway.
4. **Webviews are hardened by default** (spec 03 §4): CSP injected, remote-content
   windows get `channels: []`, navigation allowlists.
5. **Attack-mode doctor**: `keld doctor --attack` probes the built app like an
   attacker — calls every exposed channel from an unprivileged principal, attempts
   scope escapes with the fixture corpus, flags wildcard grants and world-readable
   files. Output is §2 errors with fixes.
6. **Templates are the guardrail delivery vehicle**: `create-keld` output passes
   doctor + attack-mode clean, ships CSP-strict, includes a test harness and the
   generated `AGENTS.md` — agents copy the paved road they are given.
7. **Supply chain**: 24 h `min-release-age` default on template deps; signed host
   binaries and update manifests (spec 03 §4–5).

## 7. CLI contract for agents

Every `keld` verb: non-interactive by default when flags suffice (`--yes` where a
prompt would exist), `--json` on anything with output worth parsing, stable exit codes
(0 ok · 1 failure · 2 misuse · 3 environment), errors in the §2 shape, and no TTY
tricks in `--json` mode. `keld dev` centralizes host/app/renderer logs into one
principal-tagged stream (the thing agents actually read). The CLI is the de-facto MCP
for agents that don't speak MCP; both surfaces wrap the same internals.
Spec-named verbs that are not in this binary (`build`, `migrate`, `gen`, `ext`)
are `KELD-CLI-045` (exit 2): tracking issue plus the Phase 2 workaround — never a
bare "unknown command" or "not implemented". Garbage verbs are `KELD-CLI-046`
(exit 2). Extra tokens on `keld create` / `keld dev` are `KELD-CLI-044` (exit 2);
`--template`, `--watch`, and `--inspect-ipc` are not live.

## 8. Rollout (tracked in ROADMAP + Linear)

- Phase 1: §2 error standard wired into crates as they gain real errors; CLI `--json`
  from the first verb; docs skeleton adopts §3 structure.
- Phase 2: MCP server v1 (docs/doctor/permissions tools + dev inspect/trace on the
  dev-mode channel); llms.txt pipeline with docs-site seed; eval harness v0 (scaffold
  + migrate scenarios) beside the corpus harness.
- Phase 3: attack-mode doctor; `create-keld`-emitted AGENTS.md; eval pass rate becomes
  a release gate.

## 9. Non-goals

- No agent runtime inside Keld apps (Keld hosts apps, not agents; app authors bring
  their own agent stack).
- No bespoke agent protocol: MCP + CLI + llms.txt are the surfaces; we adopt
  standards, we don't invent them.
- No telemetry from developers' agent sessions; evals run on our fixtures, not user
  data.

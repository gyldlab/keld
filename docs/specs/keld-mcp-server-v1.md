# Spec: keld MCP server v1 (doctor · docs search · permissions explain)

Status: **APPROVED** (human approval 2026-08-11)
Linear: KEL-42 · Owner: TBD (human approver) · Updated: 2026-08-13

> Status flipped to **APPROVED** on 2026-08-11; implementation may proceed under
> workflow.md § spec gate. T4 (`keld_permissions_explain`) wraps `keld-guard`
> `load_manifest` / `evaluate` (v0: default-deny path scopes on `app` grants).

## 1. Goal & non-goals

Coding agents building Keld apps need Keld's tooling in their own tool surface: today
they must shell out to a CLI they may not know exists, and there is no way to ask "why
was this denied and what manifest edit fixes it" without a human reading
`docs/architecture/03-security.md`. This spec ships the official Keld MCP server —
`keld mcp serve`, a stdio-transport server inside the existing `keld` binary speaking
MCP spec revision **2026-07-28** (with 2025-11-25 negotiation for lagging clients),
built on **rmcp 3.1.x** — exposing the first three tools of the
`docs/architecture/07-agent-experience.md` §4 toolset: `keld_doctor`,
`keld_docs_search`, `keld_permissions_explain`. Observable outcome: an MCP client
(Claude Code, Cursor, etc.) configured with `command: keld, args: [mcp, serve]` can
list three tools with declared output schemas, run doctor checks, search the Keld docs
corpus, and get a deny explanation with the exact manifest patch — all offline, all
read-only.

Non-goals (explicit, per `docs/research/library/agents-tooling/21-mcp-standard.md`):

- **No HTTP transport** (no Streamable HTTP listener, no OAuth resource-server
  surface, no SEP-2243 routing headers). stdio only.
- **No elicitation / MRTR `input_required`** — every v1 tool is read-only and returns
  `resultType: "complete"`; the elicitation client-fork risk
  (`docs/research/library/agents-tooling/21-mcp-standard.md` § risks) is
  sidestepped entirely.
- **No tasks extension** (`io.modelcontextprotocol/tasks`) — all three tools are
  interactive-latency.
- **No remote access** of any kind; the server binds to nothing.
- **No telemetry** (arch/07 §9: no data leaves the developer's machine).
- **No write operations**: `keld_permissions_explain` reads manifests and *returns* a
  patch; it never edits a file. A future patch-applying tool is out of scope and would
  require MRTR consent gating (`docs/research/library/agents-tooling/21-mcp-standard.md` §4).
- **Not in scope**: the remaining six arch/07 §4 tools (`keld_scaffold_app`,
  `keld_migrate_analyze`, `keld_dev_inspect`, `keld_ipc_trace`, `keld_logs_search`,
  `keld_build_errors`). They land in follow-up specs once their substrates exist
  (migrate analyzer, dev-mode channel, unified log, build pipeline).
- **No deprecated MCP primitives**: no Roots, Sampling, or MCP Logging (all deprecated
  in 2026-07-28); diagnostics go to stderr. No health checks on `ping` (removed).

## 2. Spec refs

- `docs/architecture/07-agent-experience.md` **§4** (governing: toolset, naming,
  stdio, size-bounding, security stance), **§2** (error object shape embedded in tool
  output), **§7** (CLI contract: `--json`, stable exit codes 0/1/2/3, non-interactive).
- `docs/architecture/03-security.md` **§2–3** (manifest schema and generated-patch
  philosophy that `keld_permissions_explain` renders).
- `docs/research/library/agents-tooling/21-mcp-standard.md` (spec revision 2026-07-28, rmcp 3.1.2 readiness,
  v1 transport/primitive decisions — this spec adopts its recommendations verbatim).
- **Deviation: none.** Arch/07 §4 lists nine v1 tools and §8 Phase 2 bundles
  dev-inspect/trace; this spec *sequences within* Phase 2 — the three tools whose
  substrates exist today ship first, the dev-mode-channel tools follow in their own
  spec. The arch doc is unchanged. If the human reviewer reads §8 as requiring all
  Phase-2 tools in one release, T1's PR adds a one-line phasing clarification to
  arch/07 §8 in the same PR (per the code/spec-drift rule in root `AGENTS.md`).

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a machine with no network, when an MCP client spawns `keld mcp serve` and
   sends `server/discover` over stdio, then the response advertises protocol versions
   `2026-07-28` and `2025-11-25` and the server identity
   (`name: "keld"`, the crate version) in result `_meta`.
2. Given a running server, when the client sends `tools/list`, then exactly three
   tools are returned — `keld_doctor`, `keld_docs_search`, `keld_permissions_explain`,
   in that fixed order on every call — each with an `outputSchema` (JSON Schema
   2020-12) byte-identical to the checked-in snapshot, and the result carries
   `ttlMs`/`cacheScope` cache hints.
3. Given a scaffolded project dir with Bun on `PATH`, when `keld_doctor` is called
   with `project_root` pointing at it, then `structuredContent` is a findings array
   equal to `keld doctor --json` output for the same dir, every finding has
   `{label, ok, detail}`, and `resultType` is `"complete"`.
4. Error case: given a `PATH` without `bun`, when `keld_doctor` runs, then the `bun`
   finding has `ok: false` and embeds a §2 error object whose `fix` states the exact
   remedy (`install Bun from https://bun.sh and ensure \`bun\` is on PATH`) —
   exact-match tested.
5. Given the query `"capability manifest"`, when `keld_docs_search` is called, then at
   least one result chunk sourced from `docs/architecture/03-security.md` is returned
   with `{title, source_path, snippet}`, results are identical across repeated calls,
   and the total response is ≤ `max_results` (default 5, cap 20) with
   `truncated: true` plus a `hint` ("raise `max_results` or narrow the query") when
   matches were cut.
6. Error case: given a query matching nothing, when `keld_docs_search` runs, then the
   result is an empty list plus a `hint` naming 2–3 top-level corpus topics — not an
   error, never a hallucinated chunk.
7. Given a fixture `keld.permissions.jsonc` with no `fs.read` grant and the operation
   `{principal: "app", capability: "fs.read", args: {path: "$DOCUMENTS/notes.txt"}}`,
   when `keld_permissions_explain` runs, then `structuredContent.decision` is
   `"deny"`, `deny_reason.kind` is `"not_granted"`, and the embedded §2 error's `fix`
   contains the exact manifest patch (JSON pointer `/app/fs/read` + value to append) —
   exact-match tested; and the manifest file's bytes are unchanged after the call.
8. Error case: given a `manifest_path` that does not exist, when
   `keld_permissions_explain` runs, then the tool returns `isError: true` content
   embedding §2 error `KELD-MCP010` whose `fix` names the path that was tried and the
   expected file name (`keld.permissions.jsonc`) — not a JSON-RPC protocol error.
9. Given the built `keld-cli`, when its dependency tree is inspected in CI, then no
   HTTP-transport crates (`hyper`, `axum`, `reqwest`) are present — rmcp is compiled
   with stdio-transport + server features only.
10. Given `keld mcp` with an unknown sub-verb, when invoked, then usage is printed to
    stderr and the exit code is `2` (misuse, per arch/07 §7).
11. Error case: given a fixture that would **allow** `fs.read` on `$APPDATA/notes.txt`
    and `operation.channel` set to any string (including `""`), when
    `keld_permissions_explain` runs, then the tool returns `isError: true` content
    embedding §2 error `KELD-MCP014` whose `fix` tells the caller to omit `channel`
    — not `decision: "allow"`. `channel?` stays in the input schema (approved v1
    shape); v0 fails closed on it. `channel_forbidden` is not a v0 deny kind.

## 4. Design

### Placement: subcommand, not a separate binary

`keld mcp serve` lives in `keld-cli` (new `src/mcp/` module tree), matching arch/07 §4
verbatim ("Ships with the CLI"). Justification against a separate binary: the server
must hold *no authority beyond the CLI it wraps* (arch/07 §4 security note) — sharing
the binary makes that structural, not aspirational; one artifact to install, sign, and
version; the CLI is already "the de-facto MCP for agents that don't speak MCP"
(arch/07 §7), so both surfaces wrapping the same `keld_cli` library functions keeps
one canonical implementation. Async note: rmcp requires tokio; the MCP server is cold
tooling, so this is sanctioned by root `AGENTS.md` ("Async only in cold tooling
(cli/pack/update)"). The tokio runtime is constructed inside `run_mcp_serve()` only —
no async leaks into `create`/`dev`/`doctor` paths.

### New/changed types (sketch)

```rust
// crates/keld-cli/src/mcp/mod.rs
/// Serves MCP over stdio until the client closes stdin. Blocking; builds its
/// own current-thread tokio runtime (cold tooling — AGENTS.md async rule).
pub fn run_mcp_serve(project_root: Option<&Path>) -> Result<(), McpServeError>;

// crates/keld-cli/src/mcp/error.rs — the §2 error object, serialized into
// structuredContent (NOT into JSON-RPC error codes; MCP reserves -32020..-32099).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct KeldErrorObject {
    pub code: String,          // "KELD-MCP010"
    pub message: String,
    pub cause: Option<String>,
    pub fix: String,           // imperative; exact patch/command
    pub docs: String,          // https://keld.dev/e/KELD-MCP010
}

// crates/keld-cli/src/mcp/doctor_tool.rs
#[derive(Debug, Serialize, JsonSchema)]
pub struct DoctorFinding {
    pub label: String,         // from doctor::Check
    pub ok: bool,
    pub detail: String,
    pub error: Option<KeldErrorObject>, // present iff !ok
}
// output: Vec<DoctorFinding> — top-level array, no wrapper (SEP-2106 any-type
// output; docs/research/library/agents-tooling/21-mcp-standard.md §3). Wraps keld_cli::doctor::run_checks().

// crates/keld-cli/src/mcp/docs_tool.rs
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DocsSearchArgs {
    pub query: String,
    pub max_results: Option<u8>, // default 5, clamped to 20
}
#[derive(Debug, Serialize, JsonSchema)]
pub struct DocsSearchResult {
    pub results: Vec<DocChunk>, // { title, source_path, snippet, score }
    pub truncated: bool,
    pub hint: Option<String>,   // "raise max_results or narrow the query"
}

// crates/keld-cli/src/mcp/permissions_tool.rs
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PermissionsExplainArgs {
    pub manifest_path: PathBuf,      // keld.permissions.jsonc
    pub operation: DeniedOperation,  // { principal, capability, args, channel? }
                                     // channel present → KELD-MCP014 in v0 (fail closed)
}
#[derive(Debug, Serialize, JsonSchema)]
pub struct PermissionsExplainResult {
    pub decision: String,                 // "allow" | "deny"
    pub deny_reason: Option<DenyReasonView>, // v0: not_granted | out_of_scope
    pub error: Option<KeldErrorObject>,   // fix = exact manifest patch
    pub patch: Option<ManifestPatch>,     // { json_pointer, value, snippet }
}
```

- `keld_permissions_explain` calls `keld-guard`'s public evaluation API
  (`evaluate(manifest, principal, operation, path) -> Decision`) — it re-implements
  **nothing**; guard's `DenyReason` display text is the floor and the `fix` is
  the exact JSON-pointer patch (guard `AGENTS.md`: "Every `DenyReason`:
  capability/scope + fix"). v0 evaluate is default-deny path/host scopes on
  `AppProcess` grants; window/plugin principals are `KELD-GUARD006` inside
  the engine and `KELD-MCP012` at this tool (string principal ≠ `"app"`).
  `$VARS` resolution is out of this slice.
- `keld doctor` gains `--json` (emits the `DoctorFinding` array) so CLI and MCP wrap
  identical internals (arch/07 §7); `doctor::Check` gains the optional §2 error.
- Docs corpus: `docs/architecture/*.md` + `docs/engineering/keld-error-codes.md`,
  embedded at compile time (`include_str!` via a small build-time list — no network,
  works from any cwd), chunked by `##` heading, ranked by deterministic keyword score
  (ties broken by path then heading order). No search dependency; std-first.
- Error codes in the §2 registry (`docs/engineering/keld-error-codes.md`):
  `KELD-MCP001`/`002` serve failures · `KELD-MCP020` doctor JSON serialize failure ·
  `KELD-MCP010`/`011`/`012`/`013`/`014` (manifest missing/parse/unknown principal/
  unreadable/channel not evaluated). Deny
  outcomes use `KELD-GUARD001`/`002` in the §2 `error` object with `isError:
  false`. Tool-level failures return `isError` content with these objects;
  JSON-RPC error codes are left to rmcp (reserved range respected).
  v0 `evaluate` covers app path/host scopes only: a present `operation.channel`
  is `KELD-MCP014` (fail closed), not an allow/deny for the path question.

### Dependency review gate — rmcp (and tokio)

Per root `AGENTS.md` (name, purpose, alternatives):

- **Name**: `rmcp` **3.1.2** (official `modelcontextprotocol/rust-sdk`), pinned
  `=3.1.2` in workspace `Cargo.toml`; features: server + stdio transport + schema
  macros only — **no** `transport-streamable-http` (enforced by acceptance
  criterion 9). Transitively brings `tokio` (pinned minor, `rt` + `io-std` features
  only) and `schemars`.
- **Purpose**: MCP 2026-07-28 wire protocol — stateless core, `server/discover`,
  dual-version negotiation (2026-07-28 + 2025-11-25), SEP-2549 cache hints, JSON
  Schema 2020-12 tool schemas. All of this is spec-mandated MUST behavior we would
  otherwise hand-roll and re-verify on every spec revision.
- **Alternatives considered**: (a) hand-rolled JSON-RPC over stdio — std-first but we
  own conformance across five spec revisions and the MRTR/discover matrix; rejected
  as ongoing cost > dependency cost. (b) TypeScript SDK v2 in a Bun sidecar — puts a
  second runtime between agent and CLI, breaks single-binary distribution; rejected.
  (c) community crates (`mcpr` etc.) — not the official SDK, no 2026-07-28 support
  confirmed; rejected.
- **Risk & mitigation**: rmcp trails Tier 1; wire fixes were still landing in Aug 2026
  (`docs/research/library/agents-tooling/21-mcp-standard.md` § risks). Exact-pin; every version bump re-runs the conformance suite
  (§7) before merge. Workspace `Cargo.toml` is single-writer (workflow.md) — the dep
  PR carries human review by construction.

### Capabilities required; manifest changes (spec 03)

none — the MCP server is developer tooling on the developer's machine; it is not a
Keld-app principal and takes no grants. It can do exactly what `keld` (the CLI) can
do, nothing more (arch/07 §4).

### Wire/protocol changes (spec 02)

none — kipc frames, manifest schema, and update feed are untouched. MCP is an
external protocol consumed via rmcp; it is not Keld wire surface.

### Security stance (default-deny alignment)

- **stdio only, zero listeners**: the process opens no sockets; there is no OAuth
  surface, no session state, no `Mcp-Session-Id` (gone in 2026-07-28 anyway). The
  client spawns and owns the process; authority = the invoking user's, exactly like
  running the CLI.
- **Read-only toolset**: doctor inspects, docs_search reads an embedded corpus,
  permissions_explain reads a manifest and returns a patch as *data*. No tool writes,
  spawns (beyond doctor's existing `bun --version` probe), or escalates.
- **No MRTR `requestState`** in v1 (nothing needs it), so no state-signing surface.
- **KELD-\* codes stay in the §2 payload**, never in JSON-RPC error codes (MCP
  reserves `-32020..-32099`).
- Responses size-bounded with truncation hints (arch/07 §4 rule); `tools/list`
  deterministic for prompt-cache stability.

### Platform notes

mac / win / linux: identical — stdio and file reads only. `keld_doctor` inherits
`doctor.rs`'s existing per-OS checks (`check_macos_hello` cfg split); the webview
finding's `detail` differs by OS, which the doctor tool passes through unmodified.
Integration tests assert the platform-invariant findings (`bun`, `project`) and only
shape-check the platform-variant one.

## 5. Boundaries

- Implement in: `crates/keld-cli` (`src/mcp/` module tree, `src/main.rs` verb
  dispatch, `src/doctor.rs` for `--json`/§2 additions, `tests/`,
  `crates/keld-cli/Cargo.toml`); `crates/keld-guard` public `load_manifest` /
  `evaluate` (v0). Workspace `Cargo.toml` **only** to add the pinned
  `rmcp`/`tokio` workspace deps (single-writer file — human-reviewed dep PR).
- Must not touch: `keld-guard` internals beyond the public evaluate API,
  `keld-ipc` (protocol files are single-writer),
  `keld-host`, `keld-core`, `keld-wv`, `docs/architecture/*` (except the optional §8
  phasing line noted in §2), templates, CI workflows.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [x] T1 `keld doctor --json`: add the `DoctorFinding`/§2 error shape to
      `doctor::Check` output, `--json` flag, stable exit codes; exact-match tests on
      `fix` text. (Pure CLI slice — useful standalone, and the substrate T2 wraps.
      Includes the arch/07 §8 phasing clarification if the reviewer wants it.)
- [x] T2 Dependency + server skeleton + first tool: add pinned `rmcp`/`tokio` to
      workspace `Cargo.toml`; `keld mcp serve` verb; `server/discover` +
      dual-version negotiation; `keld_doctor` tool end-to-end; stdio conformance
      harness + schema snapshot tests. (Carries the dependency review gate.)
- [x] T3 `keld_docs_search`: compile-time corpus embedding, heading chunker,
      deterministic ranking, `max_results` clamp + truncation hints; snapshot +
      determinism tests.
- [x] T4 `keld_permissions_explain`: wraps `keld-guard` public
      `load_manifest` / `evaluate` (v0 default-deny path scopes); patch
      synthesis, exact-match fix-text tests, read-only assertion test.
      Missing file → `isError` + `KELD-MCP010`. Present `channel` →
      `isError` + `KELD-MCP014` (v0 does not evaluate channel grants).
- [x] T5 Agent-facing usage doc (client registration snippet for Claude Code/Cursor,
      tool descriptions with cross-tool ordering hints) + `docs/agents/learnings.md`
      entry + error-code registry entries for `KELD-MCP0xx`.

## 7. Test plan

| Criterion | Test |
|---|---|
| 1 (discover, versions) | integration: spawn `keld mcp serve` via `CARGO_BIN_EXE_keld`, drive stdio with framed JSON-RPC, assert discover payload |
| 2 (tools/list snapshot) | conformance + snapshot: `tools/list` result compared byte-wise against `tests/snapshots/tools_list.json` (checked in; regenerating requires a reviewed diff) |
| 3 (doctor parity) | integration: temp dir (`tempfile`) scaffolded via `create_project`, call tool and `keld doctor --json`, assert equal JSON |
| 4 (doctor fix text) | unit on `DoctorFinding` mapping + integration with `PATH` overridden to a temp dir without `bun`; exact-match on `fix` |
| 5 (docs_search hit + determinism) | unit: run query 3×, assert identical results; assert source path |
| 6 (docs_search empty) | unit: nonsense query → empty list + hint |
| 7 (permissions deny + patch) | unit + stdio `tools/call`: fixture with no `fs.read`; exact-match on `fix` and `patch.json_pointer`; hash manifest bytes before/after |
| 8 (manifest missing) | unit + stdio `tools/call`: `KELD-MCP010` object, `isError: true`, `fix` names the tried path |
| 9 (no HTTP deps) | CI check in T2's PR: `cargo tree -p keld-cli` asserted free of `hyper`/`axum`/`reqwest` (script or `cargo-deny` ban list) |
| 10 (exit code 2) | integration: `keld mcp bogus` → stderr usage, exit 2 |
| 11 (channel fail-closed) | unit + stdio `tools/call`: in-scope path + `channel` → `KELD-MCP014`, `isError: true`, not `decision: "allow"` |

Anti-flake (workflow.md + root `AGENTS.md` rules applied): **no sleeps** — the stdio
harness reads until a complete JSON-RPC frame (length/newline-delimited per rmcp
stdio framing), so tests await conditions, never timers; **no ports** — stdio only,
nothing to collide; **temp dirs** via `tempfile` for every project/manifest fixture;
platform-variant doctor findings are shape-checked only (§4 platform notes) so the
suite is green on all three OSes; rmcp version bumps re-run this whole table
(`docs/research/library/agents-tooling/21-mcp-standard.md`: pin + re-verify).

## 8. Review gates triggered

- **Dependency addition**: `rmcp =3.1.2` + `tokio` (workspace-pinned) — justification
  in §4; lands in T2's PR.
- **Public API**: new CLI verb `keld mcp serve`, `keld doctor --json`, and the three
  tool names + input/output schemas (schemas are public API for agents — snapshot-
  guarded).
- **Permission model**: no enforcement change, but `keld_permissions_explain` renders
  authoritative grant advice (its `fix` text can induce over-granting if wrong) —
  flagged for human review of the patch-synthesis rules in T4.
- unsafe: none. Wire protocol (kipc/manifest schema/update feed): none.

## 9. Perf impact

none of the architecture/01 §5 runtime budgets move — this is cold tooling, never on
the host/kipc/event-loop path. `keld-cli` binary size and cold-start will grow
(tokio + rmcp); T2's PR reports the measured size delta (no budget currently governs
CLI size — flag for the reviewer if it exceeds ~2 MiB).

## 10. Open questions

1. **Sequencing of the `keld-guard` manifest-parse/evaluate API** (blocks T4):
   **resolved 2026-08-13** — v0 `load_manifest` / `evaluate(manifest, principal, operation, path)`
   landed; T4 consumes it. KEL-45 (ACL fixture crate + Insta snapshots) remains
   a follow-up, not this slice. KEL-61 added the `Principal` argument
   (`KELD-GUARD006` for non-`AppProcess`).
2. **rmcp pin policy**: exact `=3.1.2` (recommended here, given post-GA wire churn)
   vs `~3.1` with the conformance suite as the bump gate. Human call — dep gate.
3. **Error-code registry**: `KELD-MCP0xx` codes need docs pages, but the registry/CI
   pipeline (arch/07 §2) doesn't exist yet. Acceptable to land codes with doc
   comments now and backfill registry pages when the pipeline ships?
4. **Docs corpus scope**: v1 embeds `docs/architecture/*` only. Include
   `docs/research/*` (large, agent-useful) or hold for the llms-full.txt pipeline?
   Recommendation: architecture-only in v1; corpus grows with the docs site.

# Keld Alignment Audit — 2026-07-08

Auditor: agent (read-only). Scope: vision, research, architecture, AGENTS.md, ROADMAP,
tooling/CI, Linear (team KELD), crate skeleton, competitors/ hygiene.

## Executive verdict

**MOSTLY ALIGNED** — The technical story is coherent end-to-end (host/child, kipc,
default-deny, Electron compat, agent-first AX). Drift is concentrated in **program
tracking**: Linear phase numbering vs `ROADMAP.md`, stale issue checklists after recent
landings, duplicate AX/tooling issues, and the verification gate still documented as
`cargo test` while CI/justfile use nextest.

---

## Scorecard

| Dimension | Score | Notes |
|-----------|-------|-------|
| 1. Vision ↔ Architecture | **Aligned** | README, ROADMAP, arch/01–07 agree on host/child, kipc, permissions, webview policy, compat path, config files (`keld.config.ts`, `keld.permissions.jsonc`, etc.). |
| 2. Architecture ↔ Crate skeleton | **Aligned** | All 11 `keld-*` crates in `AGENTS.md` repo map exist; no orphan crates. `packages/` empty as documented ("upcoming"). |
| 3. Research ↔ Decisions | **Aligned** | research/00–08 conclusions (compat-first, supervised Bun child not in-process, kipc as product, per-platform engine policy) are normative in architecture. |
| 4. Agent-first ↔ Artifacts | **Partial** | AGENTS.md, docs/agents/, arch/07, research/07, llms.txt, per-crate AGENTS.md align. Gaps: llms.txt hand-maintained (arch/07 §3 says CI-generated); `DenyReason` lacks fix text (arch/07 §2); KEL-20 awaiting sign-off; duplicate Linear AX issues. |
| 5. Tooling ↔ AGENTS.md gate | **Partial** | CI + justfile: fmt, clippy `-D warnings`, nextest, deny, 3-OS matrix, MSRV — strong. AGENTS.md verification gate still says `cargo test`; tooling-audit.md still lists nextest as deferred; KEL-34/KEL-41 open despite landings. |
| 6. Linear ↔ ROADMAP | **Partial** | Issue coverage is good; **phase numbering diverges** (see below). Several landings (workspace, CI, docs) not reflected in issue status. Duplicate issues for MCP, eval, guard fixtures. |
| 7. Naming conventions | **Partial** | Repo uses `keld-ipc`, `keld-native` consistently. Linear KEL-12/13/22 still reference `keld-bridge`, `keld-native-apis`. License: `Cargo.toml`/`deny.toml` = MIT OR Apache-2.0; ROADMAP/README = TBD. |
| 8. Non-goals honored | **Aligned** | No `todo!()`/`unimplemented!()` in library code. CLI/host print pre-alpha messages (acceptable). Guard types default-deny; no bypass paths. |

---

## Contradictions (with suggested fixes)

| # | Contradiction | Paths / issues | Suggested fix |
|---|---------------|----------------|---------------|
| C1 | **Phase numbering**: ROADMAP Phase 0 = Foundation (research + workspace + CI); Linear Phase 0 = Research only, Phase 1 = Specs, Phase 2 = Scaffolding. ROADMAP Phase 1 = "Window on screen" has no Linear project. | `ROADMAP.md` vs Linear projects | Reconcile in one doc (either renumber ROADMAP to match Linear, or rename Linear projects and add ROADMAP Phase 1–4 projects). Record mapping table in Meta project. |
| C2 | **Verification gate**: AGENTS.md says `cargo test --workspace`; CI/justfile use `cargo nextest run --workspace --profile ci`. | `AGENTS.md` §Commands/§Verification gate; `.github/workflows/ci.yml`; `justfile` | Update AGENTS.md + `docs/agents/workflow.md` to canonical nextest command; keep `cargo test` as fallback note. |
| C3 | **Stale tooling docs**: tooling-audit and KEL-34/KEL-41 say nextest not adopted; it is. | `docs/engineering/tooling-audit.md`; KEL-34, KEL-41 | Mark KEL-41 Done; update KEL-34 checklist; refresh tooling-audit §Deferred/§Findings. |
| C4 | **llms.txt generation**: arch/07 §3 requires CI-generated llms.txt/llms-full.txt; root `llms.txt` is hand-maintained. | `docs/architecture/07-agent-experience.md` §3; `llms.txt`; KEL-43 | Accept hand-maintained as interim (note in arch/07 or ROADMAP) **or** land KEL-43 pipeline. Until then, llms.txt is a partial implementation. |
| C5 | **License TBD vs declared**: ROADMAP Phase 0 open item + README "TBD"; workspace already `MIT OR Apache-2.0`. | `ROADMAP.md`; `README.md`; `Cargo.toml`; `deny.toml` | Close KEL-32 with explicit decision; update README/ROADMAP or revert Cargo.toml if undecided. |
| C6 | **Linear crate names stale**: KEL-12/22 reference `keld-bridge`, `keld-native-apis`. | KEL-12, KEL-13, KEL-22 | Edit issue bodies to `keld-ipc`, `keld-native`; link to landed `docs/architecture/01-overview.md` §3 instead of pending RFCs. |
| C7 | **KEL-18 vs arch/06**: KEL-18 asks embed vs sidecar; arch/06 §1 already decides supervised spawn (contract, not embedding). | KEL-18; `docs/architecture/06-runtime-and-tooling.md` §1 | Close KEL-18 as superseded by arch/06; point to spec. |
| C8 | **Architecture "normative" vs Linear RFC backlog**: docs/architecture/01–07 exist; KEL-12..21 RFC issues still Backlog/Drafting. | Linear Phase 1 project; `docs/architecture/` | Either mark RFC issues Done with links to architecture docs, or add explicit "spec status: approved" headers in architecture docs + close KEL-20. |

---

## Gaps

### Intentional (deferred per ROADMAP / specs)

- `packages/*` (@keld/api, @keld/electron, etc.) — not scaffolded yet.
- MCP server (arch/07 §4), agent-eval harness (§5), error-code registry (§2) — tracked KEL-35..44.
- `bench/` microbench harness — ROADMAP Phase 0 open item.
- CI hard gates: CODEOWNERS, secret scan, no-placeholder CI check — ROADMAP Phase 0, KEL-39.
- Guard manifest parsing / enforcement — v0 scope note in `keld-guard` crate docs.
- kipc beyond framing (`frame.rs`) — Phase 1/2 per ROADMAP.
- Formal KEL-11 synthesis document — research corpus (`docs/research/00-landscape.md`) partially substitutes.

### Accidental (should fix)

- **Duplicate Linear issues**: KEL-36 ≈ KEL-42 (MCP), KEL-37 ≈ KEL-44 (eval), KEL-40 ≈ KEL-45 (guard ACL fixtures). Merge or mark duplicates.
- **Issue status lag**: KEL-22 (workspace scaffold), KEL-23 (CI), KEL-41 (nextest) largely landed but Backlog/Todo.
- **ROADMAP checkboxes**: Phase 0 CI partially done (3-OS fmt/clippy/nextest/deny) but unchecked; agent-PR hard gates still open.
- **`DenyReason` fix text**: arch/07 §2 + keld-guard AGENTS.md require fix in deny messages; implementation only has capability/scope text (KEL-35).
- **competitors/** clones exist locally and are gitignored ✓; no issue tracking clone refresh cadence.

---

## Linear drift

### Issues missing from ROADMAP

ROADMAP does not mention by name: KEL-31 (Linear conventions), KEL-33 (domain/npm reserve),
KEL-34 (tooling audit tracking), KEL-38–45 (post-audit backlog). These are program/infra
work — add a "Program" standing track or Meta section in ROADMAP.

### ROADMAP items missing Linear issues

| ROADMAP item | Linear coverage |
|--------------|-----------------|
| Phase 0 licensing | KEL-32 (partial) |
| Phase 0 bench/ | KEL-39 (partial — skeleton only) |
| Phase 1 `@keld/api` minimal | No dedicated issue (implicit in KEL-29/30) |
| Phase 2 MCP + eval + llms pipeline | KEL-36/42, 37/44, 43 (duplicated) |
| Phase 3 create-keld templates | No issue yet |
| Phase 4 compat Tier 2/3, CEF, keld-ext | No issues yet (expected) |

### Phase mapping (current mismatch)

| ROADMAP | Linear project | Alignment |
|---------|--------------|-----------|
| Phase 0 — Foundation | Phase 0 Research + parts of Phase 2 Scaffolding + Meta | **Split / renumbered** |
| Phase 1 — Window (v0.1) | Phase 2 Scaffolding (KEL-25..30) | **Content match, number off by 1** |
| Phase 2 — Plane & guard (v0.2) | Scattered / not projectized | **Missing project** |
| Phase 3 — Ship (v0.3) | Not in Linear | Expected later |
| Phase 4 — Compat depth (v0.4+) | Not in Linear | Expected later |

### Spot-check issues

| Issue | vs artifacts | Drift |
|-------|--------------|-------|
| KEL-11 synthesis | Todo; `docs/research/00-landscape.md` is de facto synthesis | Formal deliverable + Phase 1 RFC links missing |
| KEL-20 agentic | In Progress; artifacts landed | Needs human sign-off → Done |
| KEL-25 hello-world | Backlog; no webview backends yet | Correct dependency state |
| KEL-34 tooling | P0 nextest/llms unchecked | nextest + llms.txt partially done |
| KEL-35..45 | Consistent with arch/07 + tooling audit | Duplicates with KEL-36..40 |

---

## Verification (2026-07-08, macOS)

```bash
cargo fmt --check          # exit 0
cargo clippy --workspace --all-targets -- -D warnings  # exit 0
cargo nextest run --workspace --profile ci             # 6/6 passed
```

Not run locally: 3-OS CI matrix, `cargo deny check` (requires cargo-deny install).

---

## Recommended actions (prioritized)

| Priority | Action |
|----------|--------|
| **P0** | Publish ROADMAP ↔ Linear phase mapping (fix C1); update `AGENTS.md` verification gate to nextest (C2). |
| **P0** | Close/mark Done: KEL-22, KEL-23, KEL-41; refresh KEL-34 checklist; dedupe KEL-36/42, 37/44, 40/45. |
| **P1** | Human sign-off KEL-20 → Done; resolve license TBD (C5) via KEL-32. |
| **P1** | Update Linear KEL-12..21 bodies: point to `docs/architecture/*`, fix crate names (C6, C8). |
| **P1** | Land KEL-35 (`DenyReason` fix text) early — AX spec already normative. |
| **P2** | Refresh `docs/engineering/tooling-audit.md`; add ROADMAP Meta/program track; deliver KEL-11 synthesis or explicitly defer to 00-landscape. |

---

## Competitors folder

- `/competitors/` is in `.gitignore` ✓
- Local clones present: electron, tauri, electrobun, deno, wry, tao ✓
- Used by research/08; not part of the shipping repo ✓

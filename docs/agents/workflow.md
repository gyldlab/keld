# Agent development workflow

How Keld is developed by parallel agents with human architectural review. Rationale
and sources: `docs/research/07-agent-first.md`. Rules here bind agents and humans.
Task-specific playbooks are routed from `.agents/index.md`; load only matching entries.

## The loop (one issue, one agent, one concern)

1. **Pick up** a Linear issue (team KELD, status Todo, unassigned, current milestone
   first). Read: the issue, its spec, the governing `docs/architecture/*` sections,
   the target crate's `AGENTS.md`, and `docs/agents/learnings.md`.
2. **Spec gate.** Larger than a bug fix and no spec? Write one from
   `docs/agents/spec-template.md` and stop for human approval. Never implement from an
   unapproved spec. Bug fixes skip the spec but not the regression test.
3. **Isolate.** Work in a git worktree sibling directory (`../keld-<issue>`), branch
   `agent/<issue>-<slug>`, one issue per worktree. Never two agents in one tree.
4. **Implement.** Tests with the change (conformance entries *first* for compat work).
   Small commits, conventional messages. No placeholder code on the branch tip.
5. **Verify** (the gate from root `AGENTS.md`): fmt + clippy `-D warnings` + full test
   suite, plus the spec's test plan. Paste real output in the PR; never "should work".
6. **Self-review.** Re-read the full diff with fresh eyes (or an adversarial review
   subagent): boundary violations? review gates missed? spec drift? slop (dead code,
   duplicated helpers, drive-by refactors)? Fix before pushing.
7. **PR.** Rebase on main first (linear history). Description per root `AGENTS.md`
   (Summary · Spec refs · Review gates · Tests · Platforms · Perf impact). Append any
   learnings to `docs/agents/learnings.md` in the same PR. Update the Linear issue.

## Parallelism rules

- Concurrency budget: 3–7 agents. Decompose so concurrent issues touch disjoint crates;
  cross-crate work is sequenced by a human, not raced.
- Shared/foundational files — workspace `Cargo.toml`, `rust-toolchain.toml`, kipc wire
  protocol, manifest schema, CI workflows, root `AGENTS.md` — are single-writer:
  human-owned or one designated agent with human review. Everything else: first PR to
  green wins; later PRs rebase.
- Subagents for search/read (fan out freely); exactly one builder runs
  `cargo test`/`cargo build` per worktree (no concurrent builds in one tree).
- Long-running autonomy: fresh context per issue; re-read ground-truth files each
  iteration; if blocked >2 attempts on the same failure, stop and report — do not
  thrash the tree.

## Review: CI is the arbiter, humans are the architects

- **Hard gates (block merge, no exceptions):** fmt · clippy `-D warnings` · full test
  suite · secret scan · no `todo!()`/`unimplemented!()` on the diff.
- **Review gates (block until a human signs off):** the five in root `AGENTS.md`
  (unsafe, public API, permissions, dependencies, wire protocol). CODEOWNERS enforces
  human approval on `keld-guard`, `keld-ipc` protocol files, and workspace manifests.
- **Human review is architectural:** intent, boundaries, spec conformance, API shape —
  not line-by-line style (lints own style). A PR too large to review architecturally
  gets split, not skimmed.
- Fork PRs never run with secrets. Perf-budget regressions >5% need a written waiver.

## Failure etiquette

- A failing test you didn't write is signal, not noise: investigate or report; never
  delete, skip, or loosen it to get green.
- If the task requires violating a rule in any `AGENTS.md`, stop and escalate — the
  rule change is the PR, not the violation.
- Honest reporting beats completed-looking work. Partial + accurate > complete + vague.

# Agent development workflow

How Keld is developed by parallel agents with human architectural review. Rationale
and sources: `docs/research/07-agent-first.md`. Rules here bind agents and humans.
Task-specific playbooks are routed from `.agents/index.md`; load only matching entries.

## The loop (one issue, one agent, one concern)

1. **Pick up and refresh.** Fetch the Linear issue (team KELD, current milestone first),
   including its description, comments, status and relations. Reconcile it with the
   checked-out code and specs; read its spec, the governing `docs/architecture/*`
   sections, the target crate's `AGENTS.md`, and `docs/agents/learnings.md`. Set only
   the agent's own issue to `In Progress` and post scope, expected paths, non-goals,
   dependencies and the first falsifiable acceptance check. An agent MAY pick up only a
   `Todo` and unassigned issue, unless a human or the orchestration explicitly assigns
   it to that agent. Otherwise it MUST NOT change the issue or begin overlapping work;
   it MUST record the ownership conflict on its own Linear issue (or the handoff if
   Linear is unavailable) and notify the human/orchestrator.
2. **Spec gate.** Larger than a bug fix and no spec? Write one from
   `docs/agents/spec-template.md` and stop for human approval. Never implement from an
   unapproved spec. Bug fixes skip the spec but not the regression test.
3. **Isolate.** Work in a git worktree sibling directory (`../keld-<issue>`), branch
   `agent/kel-<n>-<slug>` from `origin/main` (root `AGENTS.md` § Commits & PRs). One
   issue per worktree. Never two agents in one tree.
4. **Implement and coordinate.** Tests with the change (conformance entries *first* for
   compat work). Before a material design/scope decision and before integration,
   refresh Linear to pick up other-agent changes. Post a Linear progress comment after
   every contract decision, material pass/fail, blocker, scope change, and substantial
   milestone; name completed work, evidence, remaining work, risks and the next
   acceptance check. Small commits, conventional messages. No placeholder code on the
   branch tip. A *material decision* changes a public API, permission, wire protocol,
   dependency, architecture-spec interpretation, or crate/path boundary. A *substantial
   milestone* is a named task or acceptance checkpoint in the issue/spec. On duplicate,
   blocker, supersession or another active owner, stop the overlap, record the conflict
   on the agent's own issue (or handoff), and notify the human/orchestrator; a worktree
   does not authorize competing architecture decisions.
5. **Verify** (the gate from root `AGENTS.md`): fmt + clippy `-D warnings` + full test
   suite, plus the spec's test plan. Diagram changes additionally run
   `just mermaid-test`, `just mermaid-check`, and `just mermaid-render-check` plus the
   visual/report gate from `.agents/testing.md`; authoritative-doc changes run
   `just llms-check` after regeneration. Paste real output in the PR; never "should work".
6. **Self-review.** Re-read the full diff with fresh eyes (or an adversarial review
   subagent): boundary violations? review gates missed? spec drift? slop (dead code,
   duplicated helpers, drive-by refactors)? Before pushing, explicitly answer: what
   existing abstraction/primitive was reused; if anything was rewritten, what named
   ownership, correctness, security or measured-performance requirement made reuse
   insufficient; and what falsifiable evidence proves the replacement. A duplicate
   policy/helper, copied parser/state machine, silent compatibility regression or
   performance claim based only on language choice blocks the PR until the root cause is
   fixed.
7. **PR and handoff.** Refresh Linear once more, then rebase onto `origin/main` first
   (linear history; `--force-with-lease` the feature branch only — root `AGENTS.md`
   § Commits & PRs). Description per the intake form
   [`.github/PULL_REQUEST_TEMPLATE.md`](../../.github/PULL_REQUEST_TEMPLATE.md):
   Summary · Spec refs · Review gates · Tests · Platforms · Perf impact. Omit empty
   optional sections. Append any learnings to `docs/agents/learnings.md` in the same PR.
   Post actual gate output, unverified conditions, commit/PR links and follow-up issue
   IDs. Move the issue to Done only when every acceptance criterion is met; otherwise
   leave it In Progress or mark it Blocked with the exact dependency.

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

- **Hard gates (block merge, no exceptions):** every gate whose owned contract is
  affected by the diff: fmt · clippy `-D warnings` · full test suite · generated-doc
  freshness · Mermaid structural checks + pinned render when diagrams change · secret
  scan · no `todo!()`/`unimplemented!()` on the diff. The always-created CI router owns
  the job-level applicability decision; unknown/shared/build-graph inputs run every
  potentially affected gate. Workflow/router edits still create those jobs but must not
  duplicate live Ubuntu `apt-get update` onto clippy/MSRV (GUI smoke owns WebKitGTK apt).
  Visual inspection and the render report remain review artifacts in addition to CI.
- **Review gates (block until a human signs off):** the five in root `AGENTS.md`
  (unsafe, public API, permissions, dependencies, wire protocol). `.github/CODEOWNERS`
  requests human review on `keld-guard`, `keld-ipc`, workspace manifests, and CI
  workflows. Secret scan is the checksum-pinned `gitleaks` CLI job in
  `.github/workflows/ci.yml` (not the org-licensed GitHub Action).
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

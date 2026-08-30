# Agent development workflow

How Keld is developed by parallel agents with human architectural review. Rationale
and sources: `docs/research/library/agents-tooling/07-agent-first.md`. Rules here bind agents and humans.
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
   That first comment MUST open with an `## Agent claim` block (template below).
   The claim is posted **before any edit**, not after the work starts: a claim that
   only becomes visible once a branch exists cannot stop a second device from
   starting the same issue, which is how two agents discover each other at merge
   instead of at pickup.
   Posting is not atomic, so posting alone does not win the issue. After posting,
   the agent MUST re-fetch the issue's comments and check for a competing claim it
   could not have seen when it read: two agents can both read an unclaimed issue and
   both post. **The earliest claim by Linear's own `createdAt` wins** — a
   server-assigned order both agents observe identically, rather than either agent's
   local clock. A later claimant MUST record the conflict on its own issue and stop
   **before its first edit**. If two claims carry the same timestamp, neither
   proceeds: that is a human arbitration, not a coin flip.
   Before implementation, classify every acceptance criterion as `CI-only`, `real
   OS/device`, or `not applicable`. For a real OS/device criterion, the initial Linear
   comment MUST include `## OS acceptance` with the required OS/device, exact observable
   check, and current availability. A CI lane on that OS is not user-facing product
   evidence unless the approved issue/spec explicitly says it is.
   For non-trivial work, that same first comment MUST record the decision-bearing atoms
   required by root `AGENTS.md` § Atomic problem-solving protocol. Record each atom's
   owner, boundary and inputs/outputs, failure mode, observable contract, independence
   from the other atoms, and first falsifier. This is the working model before a design
   or fix is selected, not a rationale added after implementation.
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
   acceptance check. A material-decision comment MUST also record every atom changed or
   added by the decision, its independence edges and first falsifier; synthesis waits
   until each decision-bearing atom is passed, explicitly unknown, or a named blocker.
   Small commits, conventional messages. No placeholder code on the branch tip. A
   *material decision* changes a public API, permission, wire protocol, dependency,
   architecture-spec interpretation, or crate/path boundary. A *substantial milestone*
   is a named task or acceptance checkpoint in the issue/spec. On duplicate,
   blocker, supersession or another active owner, stop the overlap, record the conflict
   on the agent's own issue (or handoff), and notify the human/orchestrator; a worktree
   does not authorize competing architecture decisions. For a real OS/device criterion,
   record the executed OS and evidence after each attempt; if its system is unavailable,
   leave an `## OS handoff` naming the exact remaining check, availability blocker, and
   next operator action rather than treating generic CI as completion.
5. **Verify** (the gate from root `AGENTS.md`): fmt + clippy `-D warnings` + full test
   suite, plus the spec's test plan. Diagram changes additionally run
   `just mermaid-test`, `just mermaid-check`, and `just mermaid-render-check` plus the
   visual/report gate from `.agents/testing.md`; authoritative-doc changes run
   `just llms-check` after regeneration. Paste real output in the PR; never "should work".
   If any criterion is `real OS/device`, run it on its named platform. Otherwise run only
   the applicable CI-only checks and record `not applicable` criteria. Cross-compilation,
   emulation, and a different-OS test cannot replace a real OS/device acceptance. CI on
   the named OS proves only its declared automation contract unless the issue/spec defines
   that job as the product fixture.
6. **Self-review.** Re-read the full diff with fresh eyes (or an adversarial review
   subagent): boundary violations? review gates missed? spec drift? slop (dead code,
   duplicated helpers, drive-by refactors)? Before pushing, explicitly answer: what
   existing abstraction/primitive was reused; if anything was rewritten, what named
   ownership, correctness, security or measured-performance requirement made reuse
   insufficient; and what falsifiable evidence proves the replacement. A duplicate
   policy/helper, copied parser/state machine, silent compatibility regression or
   performance claim based only on language choice blocks the PR until the root cause is
   fixed.
   When CodeRabbit has not reviewed the current head commit (root `AGENTS.md` § Commits &
   PRs), an adversarial isolated-context review is **mandatory**, not optional, and it MUST
   have all four of these or it is theatre:
   - **Isolated context.** Reviewers get the diff and the repo, and MUST NOT be given the
     author's rationale. A reviewer handed the reasoning grades the explanation instead of
     the code.
   - **A claim is not evidence.** A comment, commit message or PR body states what the
     author believes and is the thing under test. Reviewers verify behaviour by running it,
     and MAY mutate a file to test a hypothesis provided they restore it and confirm.
   - **One named lens each**, so coverage is deliberate rather than several reviewers
     re-finding the same thing.
   - **An independent refuter per finding**, whose default position is that the claim is
     wrong. What survives refutation is what gets reported.
7. **PR and handoff.** Refresh Linear once more, then rebase onto `origin/main` first
   (linear history; `--force-with-lease` the feature branch only — root `AGENTS.md`
   § Commits & PRs). After CodeRabbit fixes, resolve the addressed GitHub review
   threads on that PR (and on an earlier PR when a follow-up merged the fix) per root
   `AGENTS.md` § Commits & PRs. Description per the intake form
   [`.github/PULL_REQUEST_TEMPLATE.md`](../../.github/PULL_REQUEST_TEMPLATE.md):
   Summary · Spec refs · Review gates · Tests · Platforms · Perf impact. Omit empty
   optional sections. Strip every template HTML comment from the submitted PR
   body. Append any learnings to `docs/agents/learnings.md` in the same PR.
   Post actual gate output, unverified conditions, commit/PR links and follow-up issue
   IDs. Before finishing, leave a Linear `## Branch handoff` block per root
   `AGENTS.md` § Branch + Linear handoff (merge intent is `do-not-merge` /
   `merge-when-CI-green` / `merge-after:<deps>` / `human-decide` — not every branch
   merges). The handoff MUST include its OS acceptance and status. For an unavailable
   real OS/device, also leave `## OS handoff`; a partial merge needs explicit human
   authorization and does not close the parent issue. Move the issue to Done only when
   every acceptance criterion is met; otherwise leave it In Progress or mark it Blocked
   with the exact dependency.

## Agent claim (mandatory, before any edit)

Agents run on more than one machine against one Linear board. Ownership is therefore
declared in Linear before work starts, and it names the device, because `real OS/device`
acceptance cannot move between agents.

```text
## Agent claim
- Agent: claude-code | codex | cursor
- Device: <host the work actually runs on>
- Model/effort: e.g. opus-5 | gpt-5.6-sol@max
- Worktree: ../keld-<issue>
- Branch: agent/kel-<n>-<slug>
- Expected paths: <globs this work will write>
- Single-writer files needed: none | <named shared file>
- Claim expires: <UTC timestamp, at most 24h ahead; refresh while working>
- OS acceptance owned: real:<OS/device + observable> | none
```

The claim names *ownership* of an OS criterion. The availability of that system stays in
the `## OS acceptance` block step 1 already requires, posted in the same comment directly
after the claim — one record of availability, not two.

- An agent MUST NOT begin work on an issue carrying an **unexpired** claim from another
  agent or device. It MUST record the conflict on its own issue and stop.
- **Overlapping `Expected paths` on a single-writer file is a conflict even across
  different issues**, and it carries the same duty: the second agent MUST stop and record
  the conflict on its own issue rather than proceeding because the ticket number differs.
  Overlap on any *other* path is not a stop — § Parallelism rules already governs it, and
  first PR to green wins while later PRs rebase. This bullet narrows nothing there; it
  only says that the single-writer set is claimed in Linear rather than discovered at
  merge.
- Overlap on an ordinary file is still worth declaring, because the failure it warns about
  is not the loud one. Git reports a textual conflict and no work is lost. The silent case
  is two agents editing *different regions* of one file, both merging cleanly, and the
  combined result being wrong — a rule and its exception, a check and the test that pins
  it. Seeing the overlap in a claim is what prompts the second agent to read the first
  agent's diff before assuming a clean rebase is a correct one.
- A claim past `Claim expires` is free. The next agent MAY take the issue and MUST say
  in its own claim that it did.
- `Claim expires` MUST be at most 24h ahead, and an agent MUST NOT extend it except by
  refreshing while actually working. Without a ceiling the expiry rule guarantees
  nothing: a crashed session that wrote a far-future timestamp would hold the board for
  as long as it named.
- A human MAY revoke any claim at any time by saying so on the issue. Revocation takes
  effect immediately, regardless of the expiry, and the next agent records that it took
  a revoked claim. This is the override for a wedged or misbehaving agent that is still
  refreshing.
- `Single-writer files needed` MUST be empty unless that agent is the designated writer
  for the shared file (see § Parallelism rules). Claiming one does not grant it.
- The claim MUST be refreshed at each substantial milestone, alongside the progress
  comment step 4 already requires. An unrefreshed claim is a stale claim.

## Parallelism rules

- Concurrency budget: 3–7 agents. Decompose so concurrent issues touch disjoint crates;
  cross-crate work is sequenced by a human, not raced.
- Shared/foundational files — workspace `Cargo.toml`, `rust-toolchain.toml`, kipc wire
  protocol, manifest schema, CI workflows, root `AGENTS.md` — are single-writer:
  human-owned or one designated agent with human review. Everything else: first PR to
  green wins; later PRs rebase.
- Assign `real OS/device` work only to an agent that has the required system. Agents on
  another OS MAY complete disjoint CI-only work, but MUST hand off the named OS acceptance
  in Linear instead of duplicating or approximating it.
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

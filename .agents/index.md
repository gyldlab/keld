# Agent playbook index

Select task-relevant guidance here. Product behavior: `docs/architecture/`.
Maintainer lifecycle: `docs/agents/workflow.md`.

## Precedence

Root [`AGENTS.md`](../AGENTS.md) binds every task; nearest crate rules add constraints.
Load playbooks only for matching triggers. On conflicting instructions, stop mutation,
cite both rules and ask one focused question.

External contributors and their coding agents read workflow § Public contributions
and follow [public claims](../CONTRIBUTING.md#claim-an-issue-before-editing).
Internal tracker/worktree/branch/handoff/merge steps require maintainer authority;
other engineering playbooks still apply.

## Task routing

| Task or path | Read |
|---|---|
| Any non-trivial design, diagnosis, review, or implementation | Root `AGENTS.md` § Atomic problem-solving protocol, relevant architecture and nearest crate `AGENTS.md`; query learning names/metadata and relevant facts via [`learnings.md`](../docs/agents/learnings.md) |
| Starting, resuming, or closing non-trivial repository work | [`docs/agents/workflow.md`](../docs/agents/workflow.md) (external: § Public contributions only) |
| Maintainer tracker, branch/worktree, OS acceptance or handoff | [`docs/agents/workflow.md`](../docs/agents/workflow.md), [`coordination.md`](coordination.md), and [`review.md`](review.md) for branch operations |
| PR open/update/rebase/push/review or feedback resolution | [`review.md`](review.md); CodeRabbit fixes additionally use [`autofix/SKILL.md`](skills/autofix/SKILL.md) and [`autofix/github.md`](skills/autofix/github.md) |
| Editing GitHub Actions, the CI router/checkers, required checks, KeldBot, or branch protection | [`ci.md`](ci.md) and [`testing.md`](testing.md) |
| Learning record edits/migration | [format](../docs/agents/learnings/AGENTS.md) |
| Observed protocol failure or editing agent instructions, playbooks, workflow/templates, skills, repository harness assembly config, instruction budgets, or their CI enforcement | [`instructions.md`](instructions.md) and [`instruction-review/SKILL.md`](skills/instruction-review/SKILL.md) |
| Editing generated docs, Mermaid, public documentation, or publishing/correcting a public technical audit | [`docs.md`](docs.md) and [`testing.md`](testing.md) |
| Writing or changing a feature/architecture specification | [`docs/agents/spec-template.md`](../docs/agents/spec-template.md) and the governing architecture section |
| Maintainer issue another agent/device may hold | `docs/agents/workflow.md` § Agent claim |
| Test-layout, placement, oversized tests, discovery or acceptance refactors | [`test-layout.md`](test-layout.md) |
| Tests, bug fixes, compatibility, process boundaries, fuzzing, or platform behavior | [`testing.md`](testing.md) |
| Add/change Mermaid under private research or synthesize external evidence | [`docs.md`](docs.md), [`testing.md`](testing.md), and [`research.md`](research.md) |
| Needs a paste prompt or external-research pack; or would otherwise invent a new prompt taxonomy | [`research.md`](research.md) and Prompt Tracker (`0monish/prompt-tracker` / local `keld-agent-prompts`) |
| Material decision depends on current external OS/platform command, SDK/API, runtime, or external-tool semantics | [`research.md`](research.md) § Current-documentation receipt; [`testing.md`](testing.md) when behavior is exercised |
| Material current external facts/sentiment/unpublished changes or cross-source synthesis; local/primary evidence insufficient | [`research.md`](research.md) |
| Local docs, code, tests, and Prompt Tracker still cannot answer a material question | [`memory.md`](memory.md) for MemPalace and untrusted-lead rules |
| Add, bump, remove, migrate, or make a current-version claim about a Cargo or Bun dependency | [`dependencies.md`](dependencies.md) |
| Approved external contributor-memory service configuration/use/review/upgrade/removal, or unexpected recall | [`memory.md`](memory.md) |

Load only matched rows. When touched paths or scope expand, route again before editing
the newly reached domain.

# Agent playbook index

Use this file only to select the minimum task-relevant guidance. Product behavior
remains owned by `docs/architecture/`; development lifecycle remains owned by
`docs/agents/workflow.md`.

## Precedence

Root [`AGENTS.md`](../AGENTS.md) is the binding repository floor. The nearest crate
`AGENTS.md` adds path-local constraints and MUST NOT weaken the root. Topic playbooks
under `.agents/` are loaded only when their trigger matches the task. If applicable
instructions conflict, stop mutation, cite both rules, and ask one focused question;
do not silently choose the less restrictive rule.

## Task routing

| Task or path | Read |
|---|---|
| Any non-trivial design, diagnosis, review, or implementation | Root `AGENTS.md` § Atomic problem-solving protocol, relevant architecture and nearest crate `AGENTS.md`; query only relevant-area entries in `docs/agents/learnings.md` |
| Beginning any repository implementation or edit | [`docs/agents/workflow.md`](../docs/agents/workflow.md) |
| Using Linear, creating/using a branch or worktree, OS-scoped acceptance, or handoff | [`docs/agents/workflow.md`](../docs/agents/workflow.md) and [`coordination.md`](coordination.md) |
| Opening/updating/rebasing/pushing/reviewing a PR or resolving review feedback | [`review.md`](review.md); CodeRabbit fixes additionally use [`autofix/SKILL.md`](skills/autofix/SKILL.md) and [`autofix/github.md`](skills/autofix/github.md) |
| Editing GitHub Actions, the CI router/checkers, required checks, KeldBot, or branch protection | [`ci.md`](ci.md) and [`testing.md`](testing.md) |
| Editing agent instructions, playbooks, workflow/templates, skills, repository `.codex` assembly config, instruction budgets, or their CI enforcement | [`instructions.md`](instructions.md) and [`instruction-review/SKILL.md`](skills/instruction-review/SKILL.md) |
| Editing generated docs, Mermaid, or public documentation | [`docs.md`](docs.md) and [`testing.md`](testing.md) |
| Writing or changing a feature/architecture specification | [`docs/agents/spec-template.md`](../docs/agents/spec-template.md) and the governing architecture section |
| Starting work on an issue when another agent or device may also hold it | `docs/agents/workflow.md` § Agent claim |
| Tests, bug fixes, compatibility, process boundaries, fuzzing, or platform behavior | [`testing.md`](testing.md) |
| Add/change Mermaid under private research or synthesize external evidence | [`docs.md`](docs.md), [`testing.md`](testing.md), and [`research.md`](research.md) |
| Needs a paste prompt or external-research pack; or would otherwise invent a new prompt taxonomy | [`research.md`](research.md) and Prompt Tracker (`0monish/prompt-tracker` / local `keld-agent-prompts`) |
| Material decision needs current external facts, sentiment, unpublished changes, or cross-source synthesis and local/primary evidence is insufficient | [`research.md`](research.md) |
| Local docs, code, tests, and Prompt Tracker still cannot answer a material question | [`memory.md`](memory.md) for MemPalace and untrusted-lead rules |
| Add, bump, remove, migrate, or make a current-version claim about a Cargo or Bun dependency | [`dependencies.md`](dependencies.md) |
| Configure, use, review, upgrade, or remove an approved external contributor-memory service; or encounter recalled material or a memory result unexpectedly | [`memory.md`](memory.md) |

Load only matched rows. When touched paths or scope expand, route again before editing
the newly reached domain.

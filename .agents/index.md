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
| Any implementation or review | Root `AGENTS.md`, [`docs/agents/workflow.md`](../docs/agents/workflow.md), relevant architecture section, nearest crate `AGENTS.md`, and [`docs/agents/learnings.md`](../docs/agents/learnings.md) |
| Tests, bug fixes, compatibility, process boundaries, fuzzing, or platform behavior | [`testing.md`](testing.md) |
| Material decision needs current external facts, sentiment, unpublished changes, or cross-source synthesis and local/primary evidence is insufficient | [`research.md`](research.md) |
| Add, bump, remove, migrate, or make a current-version claim about a Cargo or Bun dependency | [`dependencies.md`](dependencies.md) |

Load only matched rows. When touched paths or scope expand, route again before editing
the newly reached domain.

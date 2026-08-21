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
| Creating/using a git branch, opening a PR, or deciding merge intent across devices | Root `AGENTS.md` § Branch + Linear handoff (Prompt Tracker paste chrome: `0monish/prompt-tracker` `prompts/SHARED/branch-linear-handoff.md`) |
| Tests, bug fixes, compatibility, process boundaries, fuzzing, or platform behavior | [`testing.md`](testing.md) |
| Add or change a Mermaid diagram | [`testing.md`](testing.md); also [`research.md`](research.md) when the diagram is under `docs/research/` or synthesizes external evidence |
| Material decision needs current external facts, sentiment, unpublished changes, or cross-source synthesis and local/primary evidence is insufficient | [`research.md`](research.md) |
| Add, bump, remove, migrate, or make a current-version claim about a Cargo or Bun dependency | [`dependencies.md`](dependencies.md) |
| Configure, use, review, upgrade, or remove an approved external contributor-memory service; or encounter recalled material or a memory result unexpectedly | [`memory.md`](memory.md) |

Load only matched rows. When touched paths or scope expand, route again before editing
the newly reached domain.

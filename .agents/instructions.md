# Agent-instruction authoring protocol

Load for protocol failure or changes to AGENTS, playbooks, workflow/templates, skills,
`.codex` assembly, instruction inventory/checkers or CI enforcement.

## Load classes and ownership

- `always`: automatic AGENTS; universal/path-local invariants only.
- `AGENTS.override.md` is forbidden; fix the canonical root/nested owner.
- `routed`: one exact router trigger or skill description; conditional procedures here.
- `evidence`: query/slice history; never require a full read.
- One normative owner; consumers link owner + section and add only path operations.
  Moves update owner, consumers, route and tests in one PR.

## Required change record

Before editing, record:

1. owner, consumers, load class, exact trigger, inputs/outputs and failure;
2. before/after bytes, pinned tokenizer/encoding tokens and automatic chain size;
3. reused owner and duplicated/rejected alternatives;
4. representative evals; a negative control failing if rule/route disappears;
5. rollback, retirement condition and any requested budget waiver.

MUST NOT raise an `always` budget to make a check green. Budget changes require named
Linear scope, measured semantic benefit, before/after eval and independent instruction
review. Caching does not prevent truncation. `.codex/config.toml` is assembly: justify
each server/tool, eager/routed status and prompt/tool trace; bytes are not schema tokens.

## Feedback and repair

At affected milestones, session closeout and after protocol changes, audit exercised
rules against observed outcomes. On failure or recurring omission, capture a
counterexample; search/reuse the current owner before adding a rule.
Within authorized scope, agents MUST make the smallest Linear-scoped repair without
repeat approval: prove a failing control before editing, independently review/evaluate
with all applicable gates, then adopt or roll back against acceptance.
Retire ineffective/duplicate rules through the same reviewed process; update consumers.
No authority expansion, weakened gates or unbounded rules. New protocols
need the change record above. Report unexercised clients; background audits
require a configured available scheduler; this policy does not schedule them.

## Budgets and enforcement

`.agents/instruction-budget.tsv` owns inventory and per-file caps. `tools/agent_context.rs`
checks bytes, unknown files, routes/classes/owners: root <=16 KiB, nested <=4 KiB,
root/nested chain <=24 KiB, router <=4 KiB. Manifest caps still apply; growth needs review evidence. `just agent-context` tests checker and checkout; hygiene
and `CI required` block merge on failure.

## Representative review

Invoke `.agents/skills/instruction-review` for every instruction diff. Trace root and
every nested AGENTS directory with `codex debug prompt-input`; seed security/path, CI,
docs, research, PR review and ordinary implementation tasks. Compare success/omissions,
loaded files/tokens, unnecessary reads/questions, tool calls, latency and cost.
Lower tokens count only when contract outcomes still pass.

# Agent-instruction authoring protocol

Load only when adding, moving, reviewing, or changing root/nested AGENTS, routed
playbooks, agent workflow/templates, agent skills, repository `.codex` assembly config,
instruction manifests/checkers, or their CI enforcement.

## Load classes and ownership

- `always`: root/nested AGENTS discovered automatically. Only universal or path-local
  invariants; the byte-chain budget is a correctness boundary.
- Repository `AGENTS.override.md` files are forbidden: they can silently replace the
  canonical automatic owner. Fix the root or nearest nested `AGENTS.md` instead.
- `routed`: loaded from one exact `.agents/index.md` task trigger or skill description.
  Conditional examples, procedures, templates, platform rules, and tool mechanics live
  here.
- `evidence`: searched/sliced history or logs. MUST NOT be required as a full read.
- Every normative rule has one canonical owner. Other files link to owner + section and
  add only path-specific operations. Moving a rule updates owner, consumers, route, and
  tests in the same PR.

## Required change record

Before editing, record:

1. canonical owner and affected consumers;
2. load class and exact trigger;
3. before/after bytes, pinned tokenizer/encoding tokens, and automatic chain size;
4. duplicated/rejected alternatives;
5. representative evals and a negative control that fails if the rule/route disappears;
6. rollback and any requested budget waiver.

MUST NOT raise an `always` budget to make a check green. A real budget change requires a
named Linear scope, measured semantic benefit, representative before/after eval, and
independent instruction review. Caching lowers cost, not occupancy or truncation.

`.codex/config.toml` is assembly, not prompt prose. Changes justify each server/tool,
eager/routed status, and a prompt/tool trace; config bytes do not measure schema tokens.

## Budgets and enforcement

`.agents/instruction-budget.tsv` is the inventory. `tools/agent_context.rs` calculates
actual bytes, discovers unknown files, validates routes/classes/owners, and enforces:

- root AGENTS <=16 KiB;
- every nested AGENTS <=4 KiB;
- root + each nested chain <=24 KiB;
- router <=4 KiB;
- per-file manifest budgets for routed/evidence/skill content.

Manifest caps are deliberately close to measured file size; class ceilings are only an
absolute backstop. Growth uses the review evidence above, not unused ceiling space.

`just agent-context` runs checker tests and the real checkout. The required hygiene job
and `CI required` make a failure merge-blocking.

## Representative review

Invoke `.agents/skills/instruction-review` on every agent-instruction diff. At minimum,
trace root plus every nested AGENTS working directory with `codex debug prompt-input`,
and run seeded tasks for security/path rules, CI, docs, research, PR review, and ordinary
implementation. Compare success/omissions, loaded files/tokens, unnecessary reads or
questions, tool calls, latency, and cost. Lower tokens count only when contract outcomes
still pass.

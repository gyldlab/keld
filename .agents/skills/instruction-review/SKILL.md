---
name: instruction-review
description: Review agent instructions, skills, assembly config and checkers for ownership, routing, budgets, truncation and missing eval evidence.
---

# Agent instruction review

Review the exact diff; edit policy only when authorized.

1. Read root `AGENTS.md`, `.agents/instructions.md`, `.agents/index.md` and
   `.agents/instruction-budget.tsv`; then only changed routed owners.
2. Run `just agent-context`, `just atomic-protocol`, `just llms-test` and
   `just llms-check`. Failures block; never inflate budgets or weaken checks.
3. Verify one owner per rule, load class, exact trigger, consumers, before/after bytes
   and pinned tokens, representative eval, negative control and rollback.
4. For `always` changes, run `codex debug prompt-input` at root and every nested AGENTS
   directory; verify complete expected markers, chain budgets and no truncation.
5. Attack missing-route, duplicate-owner, class-drift, hollow/override, renamed-file,
   max+1-byte, hidden/quoted/HTML-decoy, stale-manifest and mandatory full-evidence cases.
6. Report severity/path/evidence. Reject unknown files, missing owners/routes/evals,
   over-budget chains and unexplained growth. Lower tokens count only when contracts pass.

For assembly changes, record enabled server/tool delta and an actual prompt/tool trace;
config size is not schema-token cost. Hooks count as enforced only when trusted and
exercised on the named client; CI or a standalone handler test cannot prove loading.

At protocol changes and closeout after a protocol failure, audit the affected
owner/route/gate chain. Replay lost scope, untracked findings, false remote receipts,
partial-parent completion, unsafe cleanup and untrusted hooks. Separate procedural
controls from exercised automation; route defects through workflow closeout.

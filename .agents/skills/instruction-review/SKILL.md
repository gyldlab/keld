---
name: instruction-review
description: Review changes to AGENTS.md, agent playbooks/workflow, agent skills or assembly config, and instruction budget/checker files for context bloat, routing drift, duplicate ownership, truncation, and missing eval evidence.
---

# Agent instruction review

Review the exact diff; do not rewrite policy unless the user also asks for fixes.

1. Read root `AGENTS.md`, `.agents/instructions.md`, `.agents/index.md`, and
   `.agents/instruction-budget.tsv`. Load only changed routed owners after that.
2. Run `just agent-context`, `just atomic-protocol`, `just llms-test`, and
   `just llms-check`; any failure blocks. Do not raise a budget, weaken a rule/test, or
   add a route merely to make the check green.
3. Verify every changed normative rule has one owner, correct `always|routed|evidence`
   class, exact trigger, updated consumers, before/after bytes and pinned-token count,
   representative eval, negative control, and rollback.
4. For `always` changes, run `codex debug prompt-input` at repository root and every
   nested AGENTS directory. Confirm the complete root and expected nested marker are
   present, no chain exceeds budget, and output is not cut mid-rule.
5. Attack the change with missing-route, duplicate-owner, class-drift, hollow/override,
   renamed-file, max+1-byte, hidden/quoted/HTML-decoy, stale-manifest, and mandatory
   full-evidence cases.
6. Report findings by severity with path/evidence. Refuse approval on any over-budget
   automatic chain, unknown instruction file, absent routed owner, duplicated policy,
   missing semantic eval, or unexplained budget increase.

For `.codex` assembly changes, also report enabled server/tool delta and actual schema or
prompt trace; a short config file is not evidence that the exposed tool surface is cheap.

Token savings count only when representative tasks still satisfy security, process,
OS, review, and verification contracts. Prompt caching is not a context-budget proof.

# Git, PR, and review playbook

Load when creating/rebasing/pushing a branch, opening/updating a PR, or reviewing a
current head. Coordination and handoff are in `.agents/coordination.md`.

## Branch and commit contract

- Long-lived branch is `main`. Feature branches are `agent/kel-<n>-<slug>` in one issue
  worktree. Rebase on `origin/main`; `--force-with-lease` MAY rewrite only the feature
  branch. Use an explicit push refspec and read full push output; never force-push main.
- PR title is the KeldBot conventional format owned by `.agents/ci.md`. Individual
  feature commits need not match it.
- PR body uses the six required intake headings and lists the five root review gates or
  `none`. CI/shared/root policy files require human review.
- MUST NOT commit secrets or edit `.env*`. Destructive git actions need explicit user
  approval.

## Current-head review

- Addressed CodeRabbit threads are resolved only after the fix lands. Follow
  `.agents/skills/autofix/github.md` § Resolve review threads; a follow-up fix also
  resolves/comments on the original PR.
- Zero threads, a green CodeRabbit status, or `mergeStateStatus: CLEAN` is not proof of
  review. Confirm the review/walkthrough commit equals the PR tip. A rate-limit comment
  or older review does not count.
- If CodeRabbit did not review the current tip, run the isolated adversarial substitute
  defined by `docs/agents/workflow.md` step 6 and record findings/refutations in the PR.
- Agent-instruction changes additionally invoke `.agents/skills/instruction-review` and
  must pass `just agent-context` with before/after context evidence.

## Handoff

Before finishing, use `.agents/coordination.md` for Linear branch/OS handoff and merge
intent. MUST NOT auto-merge without explicit recorded authority.

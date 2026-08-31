# Linear, worktree, OS, and merge coordination

Load for claims, worktrees, OS acceptance, or handoff.
`docs/agents/workflow.md` owns lifecycle; review owns git.

## Worktrees and ownership

- Before adding one, map every worktree to its PR/claim. Remove and prune only clean
  merged or unused trees; keep primary, open-PR, unique-commit, dirty, or active trees.
- Claim competition and earliest-`createdAt` ownership remain in the workflow.

## OS acceptance

`## OS acceptance` in the first Linear comment classifies each criterion as `CI-only`,
`real OS/device`, or `not applicable` and records its system, observable, and
availability. Real acceptance passes only on that system; CI, emulation, or another OS
never substitutes. A failed/unavailable real criterion stays In Progress or Blocked and
gets this handoff:

```text
## OS handoff
- Criterion:
- Required OS/device:
- Exact command or observable:
- Current evidence:
- Availability blocker:
- Next operator action:
- Ticket status: In Progress | Blocked
```

## Branch handoff

Every used branch gets one Linear block before its owner finishes:
Research branches obey `.agents/research.md`.

```text
## Branch handoff
- Repo:
- Branch:
- Tip SHA:
- PR: <url or none>
- Merge: do-not-merge | merge-when-complete | merge-after:<deps>
- Depends on:
- OS evidence:
  - Acceptance: CI-only:<contract> | real:<system + observable> | not-applicable
  - Status: passed:<evidence> | failed:<evidence> | awaiting:<operator> | not-applicable
- Reason:
```

## Standing autonomous merge delegation

The repository-owner standing delegation requires the issue agent to merge a Keld PR
without another approval question only after every predicate below passes.
Default eligible merge: `merge-when-complete`.

- Scope, winning claim, required approval artifacts, current base, dependencies and
  single-writer collisions are reconciled.
- Every owned acceptance criterion, including each required real OS/device observable,
  is passed rather than awaiting, failed or unrun.
- `just ci` and every applicable GitHub required check pass on the final tip.
- CodeRabbit reviewed the exact final tip or the isolated substitute passed, and every
  valid finding and review thread is fixed, independently refuted and resolved.
- Every applicable unsafe, public API, permission model, dependency addition and wire
  protocol gate has named independent security or architecture evidence on the exact
  final diff.
- The PR is mergeable and contains only the reviewed issue scope.

A narrower explicit `do-not-merge`, missing approval artifact, or proposal whose
acceptance is the decision itself overrides this delegation.

This delegation authorizes only Keld PR merge; it does not authorize scope expansion,
deployment, release, publication, production mutation, account administration or
another repository.

After merge, fetch main, verify the landed patch or tree and ancestor relation, post the
execution artifact, mark the issue Done, release the claim and remove the clean
worktree.

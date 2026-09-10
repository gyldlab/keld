# Linear, worktree, OS and merge coordination

Workflow owns lifecycle; review owns Git.

Handoffs MUST follow Prompt Tracker `docs/06-graph-engineering.md` for
system/client/exact-model identity.

## Worktrees and ownership

- At start/resume, resource creation and closeout, inventory owned worktrees, clones,
  caches and evidence by exact path and PR/claim. Remove authorized clean merged/unused
  trees; preserve primary, dirty, active, open-PR, unique-commit and user content.
- Before deletion, refresh live state, resolve boundaries/reparse points and preserve
  evidence elsewhere. Use Git for registered worktrees; verify absence/registration
  before pruning stale metadata. No prefix-only janitor or tool-denial bypass.
- Record each resource as removed, retained with reason, or blocked with exact failure,
  owner and next action. Workflow owns claims and session completion.

## OS acceptance

`## OS acceptance` in the first Linear comment records each criterion as `CI-only`,
`real OS/device`, or `not applicable`, with system, observable and availability.
Only that real system proves OS acceptance. Failed/unavailable criteria stay open:

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

## Current-documentation receipt

For each material external semantic selected by
[`.agents/research.md` § Current-documentation receipt](research.md#current-documentation-receipt),
record these exact fields under the required receipt heading in the relevant Linear
decision, OS handoff, or branch handoff. A pure local refactor does not need one.

- Applicability: applied:<external semantic> | not-applicable:<pure-local reason>
- Context7: used:<library ID; query; retrieval date> | not-applicable:<reason> | unavailable:<exact tool failure>
- Official primary: <URL or immutable source; applicable version/tag; retrieval date>
- Supported claim: <exact decision-bearing claim>
- Fallback/blocker: none | <unknown or blocked decision and next action>

## Branch handoff

Post each branch in Linear; research follows `.agents/research.md`.

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

Waive extra human review only if GitHub reports author and merger as `0monish` or
`amishabenramani`; verify both before merge. Native bypass checks merger only; others need
human review.

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
execution artifact, complete only the owned acceptance unit, reconcile remaining parent
criteria before marking its issue Done, release the claim and remove the clean worktree.

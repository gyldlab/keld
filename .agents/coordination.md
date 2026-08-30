# Linear, worktree, and OS-acceptance playbook

Load when an issue/branch/worktree is used, when acceptance names an OS/device, or when
work is handed off. `docs/agents/workflow.md` owns the lifecycle; this file owns the
operational templates and platform evidence boundary.

## Worktrees and ownership

- At the start of any session using worktrees, and before adding one: list worktrees,
  map branches to open PRs, remove every merged or no-PR clean unused worktree with
  `git worktree remove`, then prune.
- Keep primary, open-PR, and genuinely dirty worktrees. Never force-remove real work,
  delete main, or remove sibling repos/nested research. Do not use `rm -rf` for linked
  worktrees.
- Agent-claim ownership, competition, and earliest-`createdAt` mechanics remain owned by
  `docs/agents/workflow.md` § Agent claim.

## OS acceptance

The first Linear comment records:

```text
## OS acceptance
- Criterion:
- Required OS/device:
- Exact observable:
- Availability:
```

- Classify each criterion before implementation: `CI-only`, `real OS/device`, or
  `not applicable`. OS/device/native backend/window/installer/sandbox/user-facing product
  behavior is real unless an approved issue/spec names CI as the acceptance fixture.
- Real acceptance passes only on the named system with the exact observable recorded.
  Cross-compile, emulation, another OS, or generic CI MUST NOT be presented as product
  evidence. A failed run is recorded and remains In Progress.
- If unavailable, leave the OS handoff below and keep In Progress while other work
  remains, or Blocked when the system is the only dependency. Partial merge requires
  explicit human authorization and does not close the parent.

## Branch handoff

```text
## Branch handoff
- Repo:
- Branch:
- Tip SHA:
- PR: <url or none>
- Merge: do-not-merge | merge-when-CI-green | merge-after:<deps> | human-decide
- Depends on: <tickets/PRs/notes or none>
- OS evidence:
  - Acceptance: CI-only:<contract> | real:<OS/device + observable> | not-applicable
  - Status: passed:<evidence> | failed:<evidence> | awaiting:<operator> | not-applicable
- Reason:
```

One block per branch/repository. Every used branch gets a Linear handoff before the
agent finishes. Research branches still obey nested research publication rules.

## OS handoff

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

Repeat per remaining real criterion.

## Merge authority

MUST NOT auto-merge unless the user/paste explicitly authorizes it or Linear already
records merge-when-CI-green and the governing prompt permits merge. Otherwise use
`merge-after:<deps>` or `human-decide`; never silently abandon or guess.

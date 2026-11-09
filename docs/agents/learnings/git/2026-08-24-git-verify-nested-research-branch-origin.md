---
name: "2026-08-24-git-verify-nested-research-branch-origin"
description: "Verify the nested research branch and origin tree; a successful push can publish no commits."
area: "git"
tags: ["git", "verify", "nested", "research", "branch"]
platform: "unspecified"
recorded: "2026-08-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L121"
related: []
---

## Historical source

> - 2026-08-24 [git] The nested `docs/research` checkout can be left on another device's feature branch; `git push origin main` from there pushes the stale local `main` ref and exits 0 while HEAD's commits stay unpublished. Run `git -C docs/research branch --show-current` before committing and verify the landing with `git -C docs/research ls-tree --name-only origin/main | grep <note-id>` — a push that printed no error is not evidence. (evidence: 2026-08-24 audit session; notes 192/196/212/187/188 and two 195 deltas sat on `research/194-bench-stat-repair` until re-published)

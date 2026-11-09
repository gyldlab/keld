---
name: "2026-08-24-process-worktrees-clone-large-competitors-targets"
description: "Worktrees clone large competitors and targets; reuse one and remove regenerable ignored outputs when needed."
area: "process"
tags: ["process", "worktrees", "clone", "large", "competitors"]
platform: "unspecified"
recorded: "2026-08-24"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L127"
related: []
---

## Historical source

> - 2026-08-24 [process] `git worktree add` in this repo triggers `competitors-sync`, which clones 11 reference repos (~1.3 GB) into every new worktree, on top of a multi-GB per-worktree `target/`. Four worktrees filled a 228 GB disk to 100%, at which point no tool can write at all — not even a harness output file. Reuse one worktree per concern and delete `competitors/` + `target/` (both gitignored, both regenerable) when done. (evidence: 2026-08-24 session; `du -sh` showed 5 x 1.3G competitors + 5.5G targets)

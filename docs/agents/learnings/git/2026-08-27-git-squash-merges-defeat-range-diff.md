---
name: "2026-08-27-git-squash-merges-defeat-range-diff"
description: "Squash merges defeat range-diff matching; compare branch content against a named baseline."
area: "git"
tags: ["git", "squash", "merges", "defeat", "range"]
platform: "unspecified"
recorded: "2026-08-27"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L137"
related: []
---

## Historical source

> - 2026-08-27 [git] In a squash-merging repo `git range-diff origin/main...HEAD` cannot match a branch's N commits against the ONE squash commit that landed them, so every commit reports unmatched and a fully-merged branch reads as unmerged work — an audit nearly preserved four worktrees on the strength of "10 unique unmerged commits" that were all already on main. Compare CONTENT, not patch-ids: `B=$(git merge-base origin/main HEAD); git diff origin/main HEAD -- $(git diff --name-only $B..HEAD)` — empty output proves the branch adds nothing main lacks. Also state the baseline when quoting ahead/behind: `origin/main` and the PR's own branch give different numbers for the same worktree (14 vs 16 here) and neither is wrong. (evidence: research note 217 R29/R37, verified across keld-kel105b/112/119/120)

---
name: "2026-08-25-process-review-trigger-inspect-remote-commit"
description: "A review trigger is not a review; inspect the remote review commit after force-pushes."
area: "process"
tags: ["process", "review", "trigger", "inspect", "remote"]
platform: "unspecified"
recorded: "2026-08-25"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L135"
related: []
---

## Historical source

> - 2026-08-25 [process] A rate-limited CodeRabbit posts `Review limit reached`, not a review; read the reviewed commit from `gh api repos/<o>/<r>/pulls/<n>/reviews`. After a force-push that removes commits, `@coderabbitai review` can answer "Review triggered" and post nothing (incremental review skips reviewed commits), leaving the tip unreviewed; `.agents/review.md` owns the review-commit check. (evidence: #85/#86/#87/#89)

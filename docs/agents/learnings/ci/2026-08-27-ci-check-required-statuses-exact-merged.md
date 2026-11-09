---
name: "2026-08-27-ci-check-required-statuses-exact-merged"
description: "Check required statuses on the exact merged commit; PR summaries can hide skipped jobs."
area: "ci"
tags: ["ci", "check", "required", "statuses", "exact"]
platform: "unspecified"
recorded: "2026-08-27"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L138"
related: []
---

## Historical source

> - 2026-08-27 [ci] Verify a merged head's required checks with `gh api repos/<o>/<r>/commits/<merged-head>/check-runs`, never the PR summary or a label: PR #95 merged with both `skipped`. (evidence: research note 217 R22, run 32980053859)

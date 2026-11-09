---
name: "2026-08-19-compat-branch-based-blob-raw-refs"
description: "Branch-based blob and raw refs are mutable; accept immutable refs only when the path itself is a Git object ID."
area: "compat"
tags: ["compat", "branch", "based", "blob", "raw"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L85"
related: ["docs/agents/learnings/compat/2026-08-19-compat-public-scoreboard-fields-forge-completion.md", "docs/agents/learnings/compat/2026-08-19-compat-revalidate-score-cells-result-waiver.md"]
---

## Historical source

> - 2026-08-19 [compat] A 40-hex path segment does not make `/blob/<branch>/` immutable; reject `blob`/`tree`/`raw` (and GitHub raw CDN) refs that are not themselves a git object id. (evidence: KEL-74, PR #34, `https://github.com/gyldlab/keld/blob/main/<40-hex>` parsed at 8f8181d)

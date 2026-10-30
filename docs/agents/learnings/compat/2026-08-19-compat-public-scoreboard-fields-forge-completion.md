---
name: "2026-08-19-compat-public-scoreboard-fields-forge-completion"
description: "Public Scoreboard fields can forge completion; derive the product result from validated score output."
area: "compat"
tags: ["compat", "public", "scoreboard", "fields", "forge"]
platform: "unspecified"
recorded: "2026-08-19"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L86"
related: ["docs/agents/learnings/compat/2026-08-19-compat-branch-based-blob-raw-refs.md", "docs/agents/learnings/compat/2026-08-19-compat-normalize-trailing-fqdn-dots-ipv4.md"]
---

## Historical source

> - 2026-08-19 [compat] Omitting `unweighted_percent` is not enough: product `complete` is a second publish channel, and `Scoreboard` public fields let a caller mint 100% without `score()`. (evidence: KEL-74, PR #34, `product_panel_omits_percent_for_uncommitted_corpus`)
